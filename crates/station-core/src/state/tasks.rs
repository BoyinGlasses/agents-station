use crate::redact::redact;
use regex::Regex;
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use std::collections::{HashMap, VecDeque};
use std::path::Path;
use std::sync::OnceLock;

const MAX_TEXT: usize = 4000;
const MAX_CONCURRENT: usize = 3;
const MAX_PER_HOUR: usize = 12;

fn valid_model_id(m: &str) -> bool {
    static RE: OnceLock<Regex> = OnceLock::new();
    let re = RE.get_or_init(|| Regex::new(r"^[a-zA-Z0-9][a-zA-Z0-9._:/#-]{0,199}$").unwrap());
    re.is_match(m)
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TaskRecord {
    pub id: String,
    pub agent: String,
    pub dir: String,
    pub text: String,
    pub model: String,
    pub at: u64,
    pub sid: Option<String>,
    pub mode: String,
    pub status: String,
    pub result: String,
    pub error: String,
}

#[derive(Debug, Clone)]
pub struct TaskStartRequest {
    pub dir: String,
    pub text: String,
    pub agent: String,
    pub model: String,
    pub fork: bool,
}

pub struct TaskManager {
    running: HashMap<String, TaskRecord>,
    recent: VecDeque<u64>,
    binds: HashMap<String, (String, String)>,
    seq: u64,
}

impl Default for TaskManager {
    fn default() -> Self {
        Self::new()
    }
}

impl TaskManager {
    pub fn new() -> Self {
        Self {
            running: HashMap::new(),
            recent: VecDeque::new(),
            binds: HashMap::new(),
            seq: 0,
        }
    }

    pub fn validate_and_register(&mut self, req: TaskStartRequest, now_ms: u64) -> Value {
        let text: String = req.text.trim().chars().take(MAX_TEXT).collect();
        if text.is_empty() {
            return json!({ "ok": false, "code": "empty" });
        }
        if self.running.len() >= MAX_CONCURRENT {
            return json!({ "ok": false, "code": "busy" });
        }
        while let Some(&oldest) = self.recent.front() {
            if now_ms.saturating_sub(oldest) > 3_600_000 {
                self.recent.pop_front();
            } else {
                break;
            }
        }
        if self.recent.len() >= MAX_PER_HOUR {
            return json!({ "ok": false, "code": "tooMany" });
        }
        let agent = if req.agent.is_empty() {
            "claude"
        } else {
            req.agent.as_str()
        };
        if !matches!(agent, "claude" | "codex" | "opencode" | "antigravity" | "zcode") {
            return json!({ "ok": false, "code": "noAgent" });
        }
        if matches!(agent, "antigravity" | "zcode") && req.fork {
            return json!({ "ok": false, "code": "noAgent" });
        }
        if !req.model.is_empty() && (!valid_model_id(&req.model) || agent == "zcode") {
            return json!({ "ok": false, "code": "badModel" });
        }
        if req.dir.is_empty() || !Path::new(&req.dir).exists() {
            return json!({ "ok": false, "code": "noDir" });
        }

        self.seq += 1;
        let id = format!("{now_ms:x}_{}", self.seq);
        let rec = TaskRecord {
            id: id.clone(),
            agent: agent.to_owned(),
            dir: req.dir,
            text: redact(&text).chars().take(200).collect(),
            model: req.model,
            at: now_ms,
            sid: None,
            mode: "print".to_owned(),
            status: "running".to_owned(),
            result: String::new(),
            error: String::new(),
        };
        self.running.insert(id, rec.clone());
        self.recent.push_back(now_ms);
        json!({ "ok": true, "task": rec })
    }

    pub fn bind(&mut self, job_id: &str, sid: &str, inbox: &str) {
        if !job_id.is_empty() {
            self.binds
                .insert(job_id.to_owned(), (sid.to_owned(), inbox.to_owned()));
            if let Some(t) = self.running.get_mut(job_id) {
                t.sid = Some(sid.to_owned());
            }
        }
    }

    pub fn finish(&mut self, id: &str) -> Option<TaskRecord> {
        self.binds.remove(id);
        self.running.remove(id)
    }

    pub fn list(&self) -> Vec<&TaskRecord> {
        self.running.values().collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn task_manager_validates_agent_model_dir_and_caps() {
        let dir = tempfile::tempdir().unwrap();
        let mut tm = TaskManager::new();

        let bad_model = tm.validate_and_register(
            TaskStartRequest {
                dir: dir.path().to_string_lossy().into_owned(),
                text: "do work".into(),
                agent: "zcode".into(),
                model: "gpt-5".into(),
                fork: false,
            },
            1000,
        );
        assert_eq!(bad_model["code"], "badModel");

        let ok = tm.validate_and_register(
            TaskStartRequest {
                dir: dir.path().to_string_lossy().into_owned(),
                text: "fix bug with sk-1234567890".into(),
                agent: "claude".into(),
                model: "sonnet".into(),
                fork: false,
            },
            2000,
        );
        assert_eq!(ok["ok"], true);
        let id = ok["task"]["id"].as_str().unwrap().to_owned();
        assert!(!ok["task"]["text"].as_str().unwrap().contains("sk-1234567890"));
        assert!(tm.finish(&id).is_some());
        assert!(tm.list().is_empty());
    }
}
