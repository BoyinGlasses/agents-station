use crate::redact::redact;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::HashMap;
use std::path::Path;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AgentHistoryItem {
    pub agent: String,
    pub sid: String,
    pub title: String,
    pub cwd: String,
    pub updated_at: u64,
}

#[derive(Debug, Default)]
pub struct AgentHistory {
    entries: HashMap<String, AgentHistoryItem>,
}

impl AgentHistory {
    pub fn record(&mut self, item: AgentHistoryItem) {
        let key = format!("{}:{}", item.agent, item.sid);
        self.entries.insert(
            key,
            AgentHistoryItem {
                title: redact(&item.title),
                ..item
            },
        );
    }

    pub fn list(&self, agent: Option<&str>, limit: usize) -> Vec<&AgentHistoryItem> {
        let mut out: Vec<&AgentHistoryItem> = self
            .entries
            .values()
            .filter(|e| agent.is_none_or(|a| e.agent == a))
            .collect();
        out.sort_by(|a, b| b.updated_at.cmp(&a.updated_at));
        out.truncate(limit);
        out
    }

    pub fn scan_jsonl_dir(&mut self, agent: &str, dir: &Path) -> usize {
        let Ok(rd) = std::fs::read_dir(dir) else {
            return 0;
        };
        let mut count = 0;
        for e in rd.flatten().take(100) {
            let p = e.path();
            if p.extension().and_then(|x| x.to_str()) != Some("jsonl") {
                continue;
            }
            let sid = p
                .file_stem()
                .and_then(|s| s.to_str())
                .unwrap_or("")
                .to_owned();
            if sid.is_empty() {
                continue;
            }
            let Ok(text) = std::fs::read_to_string(&p) else {
                continue;
            };
            let first = text
                .lines()
                .find_map(|l| serde_json::from_str::<Value>(l).ok());
            let title = first
                .as_ref()
                .and_then(|v| v.get("title").or_else(|| v.get("prompt")).and_then(Value::as_str))
                .unwrap_or(&sid)
                .to_owned();
            let cwd = first
                .as_ref()
                .and_then(|v| v.get("cwd").and_then(Value::as_str))
                .unwrap_or("")
                .to_owned();
            let updated_at = e
                .metadata()
                .ok()
                .and_then(|m| m.modified().ok())
                .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
                .map(|d| d.as_millis() as u64)
                .unwrap_or(0);
            self.record(AgentHistoryItem {
                agent: agent.to_owned(),
                sid,
                title,
                cwd,
                updated_at,
            });
            count += 1;
        }
        count
    }
}
