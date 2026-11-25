use crate::redact::redact;
use crate::state::git::CREATE_NO_WINDOW;
use crate::state::tasks::resolve_agent_cli;
use regex::Regex;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::VecDeque;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::{Arc, Mutex, OnceLock};

const MAX_IN: usize = 2000;
const MAX_OUT: usize = 4000;

pub type RewriteTransport =
    Arc<dyn Fn(&str, &str, usize) -> anyhow::Result<String> + Send + Sync>;

pub fn wrap_prompt(redacted: &str, photos: usize) -> String {
    let photo_note = if photos > 0 {
        let plural = if photos > 1 { "s" } else { "" };
        let pron = if photos > 1 { "them" } else { "it" };
        format!("\n({photos} screenshot{plural} will be attached to it; you cannot see {pron}.)")
    } else {
        String::new()
    };
    format!("Rewrite this message:\n<message>\n{redacted}\n</message>{photo_note}")
}

pub fn parse_claude_json_output(stdout: &str) -> Option<String> {
    let trimmed = stdout.trim();
    if trimmed.is_empty() {
        return None;
    }
    let mut parsed: Option<Value> = serde_json::from_str(trimmed).ok();
    if parsed.is_none() {
        for line in trimmed.lines().rev() {
            if let Ok(v) = serde_json::from_str::<Value>(line.trim()) {
                parsed = Some(v);
                break;
            }
        }
    }
    let v = parsed?;
    if v.get("is_error").and_then(Value::as_bool) == Some(true) {
        return None;
    }
    if v.get("subtype").and_then(Value::as_str) != Some("success") {
        return None;
    }
    let raw = v.get("result").and_then(Value::as_str)?;
    let cleaned = clean_rewrite(raw);
    (!cleaned.is_empty()).then_some(cleaned)
}

pub fn run_claude_cli_transport(
    bin: &Path,
    dir: &Path,
    redacted: &str,
    sys_prompt: &str,
    photos: usize,
) -> anyhow::Result<String> {
    let _ = std::fs::create_dir_all(dir);
    let mut cmd = Command::new(bin);
    cmd.args([
        "-p",
        &wrap_prompt(redacted, photos),
        "--model",
        "haiku",
        "--tools",
        "",
        "--strict-mcp-config",
        "--max-turns",
        "1",
        "--output-format",
        "json",
        "--no-session-persistence",
        "--settings",
        r#"{"disableAllHooks":true}"#,
        "--system-prompt",
        sys_prompt,
    ])
    .env("CLAUDE_CODE_SAFE_MODE", "1")
    .env("CLAUDE_CODE_DISABLE_CLAUDE_MDS", "1")
    .env("CLAUDE_CODE_DISABLE_AUTO_MEMORY", "1");
    if dir.is_dir() {
        cmd.current_dir(dir);
    }
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        cmd.creation_flags(CREATE_NO_WINDOW);
    }
    let out = cmd.output()?;
    let stdout = String::from_utf8_lossy(&out.stdout);
    if let Some(cleaned) = parse_claude_json_output(&stdout) {
        Ok(cleaned)
    } else {
        anyhow::bail!("claude rewrite failed (exit {:?})", out.status.code())
    }
}

pub fn rewrite_system(lang: &str) -> String {
    let (goal, ctx, done) = if lang == "vi" {
        ("Mục tiêu", "Bối cảnh", "Xong khi")
    } else {
        ("Goal", "Context", "Done when")
    };
    [
        "You rewrite a short message that a developer is about to send to an AI coding agent (Claude Code), so the agent gets it right the first time.".to_owned(),
        "Reply with the rewritten message only: no preamble, no comment, no code fence, and never answer or carry out the request yourself.".to_owned(),
        format!("Write three short parts, each starting on its own line with its label: \"{goal}:\" what to do, \"{ctx}:\" what the agent needs to know (where, what it is about), \"{done}:\" how to tell it is finished."),
        "If the message has several jobs, list them under the first part as numbered lines, in the order they should be done.".to_owned(),
        "Write in the language of the message, in the sender's own voice. Vietnamese typed without diacritics is written back with them.".to_owned(),
        "Keep every requirement, name, number, file, quote and word of feedback. Invent nothing: no new requirements, facts, files or steps.".to_owned(),
        "Where something the agent will need is missing, leave a short question in square brackets in its place, e.g. [which page?].".to_owned(),
        "Keep it short: under 90 words unless the message itself is longer.".to_owned(),
    ]
    .join("\n")
}

pub fn clean_rewrite(raw: &str) -> String {
    static FENCE: OnceLock<Regex> = OnceLock::new();
    let re = FENCE.get_or_init(|| Regex::new(r"(?s)^```[\w-]*\n(.*?)\n```$").unwrap());
    let mut t = raw.trim().to_owned();
    if let Some(c) = re.captures(&t) {
        t = c[1].trim().to_owned();
    }
    if t.starts_with("<message>") && t.ends_with("</message>") {
        t = t[9..t.len() - 10].trim().to_owned();
    }
    let chars: Vec<char> = t.chars().collect();
    if chars.len() >= 2
        && matches!(chars[0], '"' | '“')
        && matches!(chars[chars.len() - 1], '"' | '”')
    {
        t = chars[1..chars.len() - 1].iter().collect::<String>().trim().to_owned();
    }
    t.chars().take(MAX_OUT).collect()
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RewriteResult {
    pub ok: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub text: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub code: Option<&'static str>,
}

pub struct Rewriter {
    per_hour: usize,
    recent: VecDeque<u64>,
    busy: bool,
    transport: Option<RewriteTransport>,
}

impl Default for Rewriter {
    fn default() -> Self {
        Self::new(20)
    }
}

impl Rewriter {
    pub fn new(per_hour: usize) -> Self {
        Self {
            per_hour,
            recent: VecDeque::new(),
            busy: false,
            transport: None,
        }
    }

    pub fn set_transport(&mut self, transport: RewriteTransport) {
        self.transport = Some(transport);
    }

    /// Always runs `redact()` BEFORE invoking the external rewrite transport.
    pub fn rewrite_with<F>(
        &mut self,
        text: &str,
        lang: &str,
        photos: usize,
        now_ms: u64,
        transport: F,
    ) -> RewriteResult
    where
        F: FnOnce(&str, &str, usize) -> anyhow::Result<String>,
    {
        let trimmed: String = text.trim().chars().take(MAX_IN).collect();
        if trimmed.is_empty() {
            return RewriteResult { ok: false, text: None, code: Some("empty") };
        }
        if self.busy {
            return RewriteResult { ok: false, text: None, code: Some("busy") };
        }
        while let Some(&oldest) = self.recent.front() {
            if now_ms.saturating_sub(oldest) > 3_600_000 {
                self.recent.pop_front();
            } else {
                break;
            }
        }
        if self.recent.len() >= self.per_hour {
            return RewriteResult { ok: false, text: None, code: Some("tooMany") };
        }
        // MANDATORY SECURITY INVARIANT: redact before anything leaves the process!
        let redacted = redact(&trimmed);
        let sys_prompt = rewrite_system(lang);
        self.recent.push_back(now_ms);
        match transport(&redacted, &sys_prompt, photos.min(4)) {
            Ok(out) => {
                let cleaned = clean_rewrite(&out);
                if cleaned.is_empty() {
                    RewriteResult { ok: false, text: None, code: Some("failed") }
                } else {
                    RewriteResult { ok: true, text: Some(cleaned), code: None }
                }
            }
            Err(_) => RewriteResult { ok: false, text: None, code: Some("failed") },
        }
    }

    /// Runs the configured transport (or real `claude -p --model haiku` CLI) without
    /// holding the `Mutex` while the subprocess runs.
    pub fn execute_blocking(
        rewriter: Arc<Mutex<Self>>,
        work_dir: PathBuf,
        text: String,
        lang: String,
        photos: usize,
        now_ms: u64,
    ) -> RewriteResult {
        let (redacted, sys_prompt, clamped_photos, custom_transport) = {
            let mut g = rewriter.lock().unwrap_or_else(|e| e.into_inner());
            let trimmed: String = text.trim().chars().take(MAX_IN).collect();
            if trimmed.is_empty() {
                return RewriteResult { ok: false, text: None, code: Some("empty") };
            }
            if g.busy {
                return RewriteResult { ok: false, text: None, code: Some("busy") };
            }
            while let Some(&oldest) = g.recent.front() {
                if now_ms.saturating_sub(oldest) > 3_600_000 {
                    g.recent.pop_front();
                } else {
                    break;
                }
            }
            if g.recent.len() >= g.per_hour {
                return RewriteResult { ok: false, text: None, code: Some("tooMany") };
            }
            let redacted = redact(&trimmed);
            let sys_prompt = rewrite_system(&lang);
            let custom = g.transport.clone();
            if custom.is_none() && resolve_agent_cli("claude").is_none() {
                return RewriteResult { ok: false, text: None, code: Some("noClaude") };
            }
            g.busy = true;
            g.recent.push_back(now_ms);
            (redacted, sys_prompt, photos.min(4), custom)
        };

        let result = if let Some(t) = custom_transport {
            t(&redacted, &sys_prompt, clamped_photos)
        } else if let Some(cli) = resolve_agent_cli("claude") {
            run_claude_cli_transport(&cli.file, &work_dir, &redacted, &sys_prompt, clamped_photos)
        } else {
            Err(anyhow::anyhow!("noClaude"))
        };

        {
            let mut g = rewriter.lock().unwrap_or_else(|e| e.into_inner());
            g.busy = false;
        }

        match result {
            Ok(out) => {
                let cleaned = clean_rewrite(&out);
                if cleaned.is_empty() {
                    RewriteResult { ok: false, text: None, code: Some("failed") }
                } else {
                    RewriteResult { ok: true, text: Some(cleaned), code: None }
                }
            }
            Err(_) => RewriteResult { ok: false, text: None, code: Some("failed") },
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rewriter_always_redacts_before_transport() {
        let mut rw = Rewriter::default();
        let mut seen_by_transport = String::new();
        let res = rw.rewrite_with(
            "fix key sk-secret123456",
            "en",
            0,
            1000,
            |redacted, _sys, _photos| {
                seen_by_transport = redacted.to_owned();
                Ok(format!("```\nGoal: {redacted}\n```"))
            },
        );
        assert!(res.ok);
        assert_eq!(seen_by_transport, "fix key •••");
        assert!(!seen_by_transport.contains("sk-secret123456"));
        assert_eq!(res.text.as_deref(), Some("Goal: fix key •••"));
    }
}
