use crate::state::git::CREATE_NO_WINDOW;
use regex::Regex;
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use std::io::Read;
use std::path::Path;
use std::process::{Command, Stdio};
use std::sync::OnceLock;
use std::time::{Duration, Instant};

const PROBE_TIMEOUT: Duration = Duration::from_secs(5);
const MAX_PROBE_BYTES: u64 = 256 * 1024;

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct BgProbeResult {
    pub supported: bool,
    pub version: Option<String>,
}

fn run_claude_probe(bin: &Path, arg: &str) -> String {
    let mut cmd = Command::new(bin);
    cmd.arg(arg)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        cmd.creation_flags(CREATE_NO_WINDOW);
    }
    let Ok(mut child) = cmd.spawn() else {
        return String::new();
    };
    let out_h = child.stdout.take().map(|o| {
        std::thread::spawn(move || {
            let mut b = Vec::new();
            let _ = o.take(MAX_PROBE_BYTES).read_to_end(&mut b);
            b
        })
    });
    let err_h = child.stderr.take().map(|e| {
        std::thread::spawn(move || {
            let mut b = Vec::new();
            let _ = e.take(MAX_PROBE_BYTES).read_to_end(&mut b);
            b
        })
    });
    let deadline = Instant::now() + PROBE_TIMEOUT;
    loop {
        match child.try_wait() {
            Ok(Some(_)) => break,
            Ok(None) if Instant::now() >= deadline => {
                let _ = child.kill();
                let _ = child.wait();
                if let Some(h) = out_h {
                    let _ = h.join();
                }
                if let Some(h) = err_h {
                    let _ = h.join();
                }
                return String::new();
            }
            Ok(None) => std::thread::sleep(Duration::from_millis(20)),
            Err(_) => {
                let _ = child.kill();
                let _ = child.wait();
                return String::new();
            }
        }
    }
    let out_bytes = out_h.and_then(|h| h.join().ok()).unwrap_or_default();
    let s = String::from_utf8_lossy(&out_bytes).into_owned();
    if !s.trim().is_empty() {
        return s;
    }
    let err_bytes = err_h.and_then(|h| h.join().ok()).unwrap_or_default();
    String::from_utf8_lossy(&err_bytes).into_owned()
}

pub fn probe_claude_bg(bin: &Path) -> BgProbeResult {
    let help = run_claude_probe(bin, "--help");
    let ver_out = run_claude_probe(bin, "--version");
    BgProbeResult {
        supported: has_bg(&help),
        version: version_of(&ver_out),
    }
}

pub const HOLD_S: u64 = 24 * 3600;

pub fn has_bg(help: &str) -> bool {
    static BG_RE: OnceLock<Regex> = OnceLock::new();
    static AGENTS_RE: OnceLock<Regex> = OnceLock::new();
    let bg = BG_RE.get_or_init(|| Regex::new(r"(^|\s)--bg\b").unwrap());
    let ag = AGENTS_RE.get_or_init(|| Regex::new(r"(?m)^\s+agents\b").unwrap());
    bg.is_match(help) && ag.is_match(help)
}

pub fn version_of(out: &str) -> Option<String> {
    static RE: OnceLock<Regex> = OnceLock::new();
    let re = RE.get_or_init(|| Regex::new(r"(\d+\.\d+\.\d+)").unwrap());
    re.captures(out).map(|c| c[1].to_owned())
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct BackgroundedInfo {
    pub id: String,
    pub copy: bool,
}

pub fn parse_backgrounded(out: &str) -> Option<BackgroundedInfo> {
    static BG_RE: OnceLock<Regex> = OnceLock::new();
    static COPY_RE: OnceLock<Regex> = OnceLock::new();
    let bg = BG_RE.get_or_init(|| Regex::new(r"(?i)backgrounded\s+·\s+([0-9a-f]{6,16})\b").unwrap());
    let cp = COPY_RE.get_or_init(|| Regex::new(r"(?i)started a copy as\s+[0-9a-f]{6,16}").unwrap());
    let caps = bg.captures(out)?;
    Some(BackgroundedInfo {
        id: caps[1].to_owned(),
        copy: cp.is_match(out),
    })
}

pub fn bg_refusal(out: &str) -> &'static str {
    let low = out.to_ascii_lowercase();
    if low.contains("workspace not trusted") || low.contains("not trusted") {
        "untrusted"
    } else {
        "failed"
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct BgPhaseView {
    pub phase: &'static str,
    pub wait: Option<String>,
}

pub fn bg_phase(row: Option<&Value>) -> BgPhaseView {
    let Some(obj) = row.and_then(Value::as_object) else {
        return BgPhaseView { phase: "gone", wait: None };
    };
    let st = obj.get("state").and_then(Value::as_str).unwrap_or("");
    let wait = obj.get("waitingFor").and_then(Value::as_str).map(str::to_owned);
    match st {
        "failed" => BgPhaseView { phase: "fail", wait: None },
        "stopped" => BgPhaseView { phase: "stopped", wait: None },
        "done" => BgPhaseView { phase: "done", wait: None },
        "blocked" => BgPhaseView { phase: "wait", wait },
        _ if obj.get("status").and_then(Value::as_str) == Some("waiting") => {
            BgPhaseView { phase: "wait", wait }
        }
        _ => BgPhaseView { phase: "work", wait: None },
    }
}

pub fn parse_agents(out: &str) -> Vec<Value> {
    let Some(i) = out.find('[') else {
        return Vec::new();
    };
    let Some(j) = out.rfind(']') else {
        return Vec::new();
    };
    if j < i {
        return Vec::new();
    }
    serde_json::from_str::<Vec<Value>>(&out[i..=j])
        .unwrap_or_default()
        .into_iter()
        .filter(Value::is_object)
        .collect()
}

pub fn bg_settings(port: u16, send_cmd: &str) -> String {
    let bind = format!("{send_cmd} http://127.0.0.1:{port}/hook/bind --async --job");
    let perm = format!("{send_cmd} http://127.0.0.1:{port}/hook/permission?long=1 --wait {HOLD_S}");
    let ask = format!("{send_cmd} http://127.0.0.1:{port}/hook/question?long=1 --wait {HOLD_S}");
    json!({
        "worktree": { "bgIsolation": "none" },
        "hooks": {
            "SessionStart": [{ "hooks": [{ "type": "command", "command": bind, "timeout": 5 }] }],
            "PermissionRequest": [{ "hooks": [{ "type": "command", "command": perm, "timeout": HOLD_S + 10 }] }],
            "PreToolUse": [{ "matcher": "AskUserQuestion|ExitPlanMode", "hooks": [{ "type": "command", "command": ask, "timeout": HOLD_S + 10 }] }]
        }
    })
    .to_string()
}

pub fn print_settings(port: u16, task_id: &str, send_cmd: &str) -> String {
    let cmd = format!("{send_cmd} http://127.0.0.1:{port}/hook/bind?task={task_id} --async --job");
    json!({
        "hooks": {
            "SessionStart": [{ "hooks": [{ "type": "command", "command": cmd, "timeout": 5 }] }]
        }
    })
    .to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_bg_help_output_phases_and_settings() {
        assert!(has_bg("Usage:\n  --bg  run in background\nCommands:\n  agents  list"));
        assert_eq!(version_of("2.1.316 (Claude Code)").as_deref(), Some("2.1.316"));
        assert_eq!(
            parse_backgrounded("Session backgrounded · ab12cd34 (started a copy as ef567890)"),
            Some(BackgroundedInfo { id: "ab12cd34".into(), copy: true })
        );
        assert_eq!(bg_refusal("Workspace not trusted"), "untrusted");
        assert_eq!(bg_phase(Some(&json!({ "state": "blocked", "waitingFor": "permission" }))).phase, "wait");
        assert_eq!(parse_agents("noise [{\"id\":\"a1\"}] tail").len(), 1);
        assert!(bg_settings(4310, "agents-station hook").contains("/hook/bind"));
    }
}
