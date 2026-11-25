use regex::Regex;
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use std::sync::OnceLock;

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
