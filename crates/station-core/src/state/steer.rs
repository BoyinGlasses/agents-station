use serde::Serialize;
use serde_json::{Value, json};
use std::path::Path;

pub fn steer_line(text: &str) -> String {
    let payload = json!({
        "type": "user",
        "message": { "role": "user", "content": text }
    });
    format!("{payload}\n")
}

pub fn inbound_of<'a>(values: &[Option<&'a str>]) -> Option<&'static str> {
    let mut has_hold = false;
    let mut has_accept = false;
    for v in values.iter().flatten() {
        match *v {
            "refuse" => return Some("refuse"),
            "hold" => has_hold = true,
            "accept" => has_accept = true,
            _ => {}
        }
    }
    if has_hold {
        Some("hold")
    } else if has_accept {
        Some("accept")
    } else {
        None
    }
}

pub fn inbound_from(config_dir: Option<&Path>, cwd: Option<&Path>) -> Option<&'static str> {
    let read_val = |p: std::path::PathBuf| -> Option<String> {
        let s = std::fs::read_to_string(p).ok()?;
        let v: Value = serde_json::from_str(&s).ok()?;
        v.get("crossSessionInbound")
            .and_then(Value::as_str)
            .map(str::to_owned)
    };
    let a = config_dir.and_then(|d| read_val(d.join("settings.json")));
    let b = cwd.and_then(|d| read_val(d.join(".claude").join("settings.json")));
    let c = cwd.and_then(|d| read_val(d.join(".claude").join("settings.local.json")));
    inbound_of(&[a.as_deref(), b.as_deref(), c.as_deref()])
}

pub fn is_socket_path(p: &str) -> bool {
    if cfg!(windows) {
        p.starts_with(r"\\.\pipe\") || p.starts_with(r"\\?\pipe\")
    } else {
        std::fs::metadata(p).is_ok()
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct SteerCheck {
    pub ok: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub why: Option<&'static str>,
}

pub fn steerable(
    inbox: Option<&str>,
    mode: &str,
    inbound: Option<&str>,
    socket_exists: bool,
) -> SteerCheck {
    if inbox.is_none_or(|s| s.is_empty()) || !socket_exists {
        return SteerCheck {
            ok: false,
            why: Some("noInbox"),
        };
    }
    if inbound == Some("refuse") {
        return SteerCheck {
            ok: false,
            why: Some("refuse"),
        };
    }
    if inbound == Some("hold") {
        return SteerCheck {
            ok: false,
            why: Some("hold"),
        };
    }
    if mode == "bypassPermissions" && inbound != Some("accept") {
        return SteerCheck {
            ok: false,
            why: Some("bypass"),
        };
    }
    SteerCheck { ok: true, why: None }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn steer_policy_checks_strictest_inbound_and_bypass_mode() {
        assert_eq!(inbound_of(&[Some("accept"), Some("hold")]), Some("hold"));
        assert_eq!(inbound_of(&[Some("accept"), Some("refuse")]), Some("refuse"));
        assert_eq!(steerable(None, "", None, true).why, Some("noInbox"));
        assert_eq!(steerable(Some(r"\\.\pipe\x"), "bypassPermissions", None, true).why, Some("bypass"));
        assert!(steerable(Some(r"\\.\pipe\x"), "bypassPermissions", Some("accept"), true).ok);
        assert!(steer_line("stop now").ends_with('\n'));
    }
}
