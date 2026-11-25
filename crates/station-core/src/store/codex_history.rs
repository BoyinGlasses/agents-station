use crate::redact::redact;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::path::Path;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CodexMessage {
    pub role: String,
    pub text: String,
}

pub fn read_codex_history(transcript_path: &Path, max_items: usize) -> Vec<CodexMessage> {
    let Ok(text) = std::fs::read_to_string(transcript_path) else {
        return Vec::new();
    };
    let mut out = Vec::new();
    for line in text.lines() {
        let Ok(v) = serde_json::from_str::<Value>(line) else {
            continue;
        };
        let role = v
            .get("role")
            .or_else(|| v.get("payload").and_then(|p| p.get("role")))
            .and_then(Value::as_str);
        let msg = v
            .get("content")
            .or_else(|| v.get("text"))
            .or_else(|| v.get("payload").and_then(|p| p.get("text")))
            .and_then(Value::as_str);
        if let (Some(r), Some(m)) = (role, msg)
            && !m.trim().is_empty()
        {
            out.push(CodexMessage {
                role: r.to_owned(),
                text: redact(m),
            });
        }
    }
    if out.len() > max_items {
        let drain = out.len() - max_items;
        out.drain(..drain);
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reads_and_redacts_codex_transcript() {
        let dir = tempfile::tempdir().unwrap();
        let f = dir.path().join("t.jsonl");
        std::fs::write(&f, "{\"role\":\"user\",\"text\":\"run with sk-secret123456\"}\n").unwrap();
        let hist = read_codex_history(&f, 50);
        assert_eq!(hist.len(), 1);
        assert!(!hist[0].text.contains("sk-secret123456"));
    }
}
