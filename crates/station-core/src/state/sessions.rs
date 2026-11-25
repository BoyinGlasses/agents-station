use crate::agent_events::{NormalizedEvent, normalize_agent_event};
use crate::atomic::write_json_atomic;
use crate::redact::redact;
use crate::shared::cache_clock::CacheState;
use crate::shared::chat_engine::ChatEngine;
use crate::shared::danger::danger_key;
use crate::state::loops::LoopGuard;
use crate::state::overlap::{OverlapTracker, file_of, is_edit_tool};
use crate::state::undo::UndoManager;
use anyhow::Result;
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use std::collections::{HashMap, VecDeque};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{SystemTime, UNIX_EPOCH};
use tokio::sync::broadcast;

pub const DESKS: usize = 9;
pub const FEED_MAX: usize = 80;
pub const PIN_MAX: usize = 12;
pub const PIN_LEN: usize = 500;

pub fn now_ms() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_millis() as u64)
        .unwrap_or(0)
}

fn clip(s: &str, max_chars: usize) -> String {
    let chars: Vec<char> = s.chars().collect();
    if chars.len() > max_chars {
        let mut out: String = chars[..max_chars.saturating_sub(1)].iter().collect();
        out.push('…');
        out
    } else {
        s.to_owned()
    }
}

fn one_line(s: &str) -> String {
    s.split_whitespace().collect::<Vec<_>>().join(" ")
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolSummary {
    #[serde(rename = "type")]
    pub kind: String,
    pub label: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub file: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SessionRecord {
    pub id: String,
    pub agent: String,
    pub name: String,
    pub cwd: String,
    pub status: String,
    pub desk: usize,
    pub started_at: u64,
    pub updated_at: u64,
    pub last_prompt: Option<String>,
    pub last: Option<ToolSummary>,
    pub model: Option<String>,
    pub cache: Option<CacheState>,
    pub tool_calls: u64,
    pub edits: u64,
    pub fails: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FeedItem {
    pub id: u64,
    pub ts: u64,
    pub sess: String,
    pub name: String,
    #[serde(rename = "type")]
    pub kind: String,
    pub label: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub desc: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PendingPerm {
    pub id: String,
    pub sess: String,
    pub name: String,
    pub kind: String,
    #[serde(rename = "type")]
    pub tool_type: String,
    pub label: String,
    pub detail: String,
    pub danger: Option<String>,
    pub created_at: u64,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct SessionStats {
    pub events: u64,
    pub prompts: u64,
    pub edits: u64,
    pub commands: u64,
    pub reads: u64,
    pub fails: u64,
}

pub struct OfficeState {
    data_dir: PathBuf,
    pub sessions: HashMap<String, SessionRecord>,
    pub feed: VecDeque<FeedItem>,
    pub pending_perms: HashMap<String, PendingPerm>,
    pub pins: HashMap<String, Vec<String>>,
    pub todos: Vec<Value>,
    pub stats: SessionStats,
    pub loops: LoopGuard,
    pub overlap: OverlapTracker,
    pub undo: UndoManager,
    pub chat: ChatEngine,
    pub presence_focused: bool,
    pub bar_last_seen_ms: u64,
    seq: AtomicU64,
    tx: broadcast::Sender<Value>,
}

impl OfficeState {
    pub fn new(data_dir: PathBuf) -> Self {
        let (tx, _) = broadcast::channel(512);
        let pins_file = data_dir.join("pins.json");
        let pins: HashMap<String, Vec<String>> = std::fs::read_to_string(&pins_file)
            .ok()
            .and_then(|s| serde_json::from_str(&s).ok())
            .unwrap_or_default();
        let chat_file = data_dir.join("chat.json");
        let chat_persisted = std::fs::read_to_string(&chat_file)
            .ok()
            .and_then(|s| serde_json::from_str(&s).ok());
        let chat = ChatEngine::from_persisted(chat_persisted, now_ms());
        Self {
            undo: UndoManager::new(data_dir.join("undo")),
            data_dir,
            sessions: HashMap::new(),
            feed: VecDeque::with_capacity(FEED_MAX),
            pending_perms: HashMap::new(),
            pins,
            todos: Vec::new(),
            stats: SessionStats::default(),
            loops: LoopGuard::default(),
            overlap: OverlapTracker::default(),
            chat,
            presence_focused: false,
            bar_last_seen_ms: 0,
            seq: AtomicU64::new(1),
            tx,
        }
    }

    pub fn subscribe(&self) -> broadcast::Receiver<Value> {
        self.tx.subscribe()
    }

    pub fn seq(&self) -> u64 {
        self.seq.load(Ordering::Relaxed)
    }

    pub fn broadcast(&self, msg: Value) {
        self.seq.fetch_add(1, Ordering::Relaxed);
        let _ = self.tx.send(msg);
    }

    fn next_desk(&self) -> usize {
        let used: std::collections::HashSet<usize> =
            self.sessions.values().map(|s| s.desk).collect();
        for d in 0..DESKS {
            if !used.contains(&d) {
                return d;
            }
        }
        self.sessions.len() % DESKS
    }

    pub fn ingest_raw_event(&mut self, payload: Value) -> Option<NormalizedEvent> {
        let ev = normalize_agent_event(payload)?;
        let ts = now_ms();
        self.stats.events += 1;

        let default_name = ev
            .cwd
            .as_deref()
            .and_then(|c| Path::new(c).file_name())
            .and_then(|n| n.to_str())
            .unwrap_or(&ev.agent)
            .to_owned();

        let desk = self
            .sessions
            .get(&ev.session_id)
            .map(|s| s.desk)
            .unwrap_or_else(|| self.next_desk());

        let sess = self
            .sessions
            .entry(ev.session_id.clone())
            .or_insert_with(|| SessionRecord {
                id: ev.session_id.clone(),
                agent: ev.agent.clone(),
                name: default_name.clone(),
                cwd: ev.cwd.clone().unwrap_or_default(),
                status: "rest".to_owned(),
                desk,
                started_at: ts,
                updated_at: ts,
                last_prompt: None,
                last: None,
                model: None,
                cache: None,
                tool_calls: 0,
                edits: 0,
                fails: 0,
            });
        sess.updated_at = ts;
        if let Some(c) = &ev.cwd
            && !c.is_empty()
        {
            sess.cwd = c.clone();
        }

        let sname = sess.name.clone();
        let scwd = sess.cwd.clone();

        let (feed_kind, feed_label) = match ev.hook_event_name.as_str() {
            "SessionStart" => {
                sess.status = "rest".to_owned();
                ("start".to_owned(), sname.clone())
            }
            "SessionEnd" => {
                self.sessions.remove(&ev.session_id);
                self.broadcast(json!({ "k": "gone", "id": ev.session_id }));
                let _ = self.loops.on_event(&ev.raw, ts);
                return Some(ev);
            }
            "UserPromptSubmit" => {
                self.stats.prompts += 1;
                let cleaned = redact(&one_line(ev.prompt.as_deref().unwrap_or("")));
                let clipped = clip(&cleaned, 120);
                sess.status = "work".to_owned();
                sess.last_prompt = Some(clipped.clone());
                self.undo.prompt(&ev.session_id, &clipped, ts);
                ("prompt".to_owned(), clipped)
            }
            "PreToolUse" => {
                sess.status = "work".to_owned();
                sess.tool_calls += 1;
                let tname = ev.tool_name.as_deref().unwrap_or("tool");
                let summary = classify_tool(tname, &ev.tool_input);
                if is_edit_tool(tname) {
                    let f = file_of(&ev.tool_input, &scwd);
                    if !f.is_empty() {
                        let cwd_opt = (!scwd.is_empty()).then(|| Path::new(scwd.as_str()));
                        self.undo.before(&ev.session_id, Path::new(&f), cwd_opt);
                    }
                }
                let k = summary.kind.clone();
                let l = summary.label.clone();
                sess.last = Some(summary);
                (k, l)
            }
            "PostToolUse" => {
                let tname = ev.tool_name.as_deref().unwrap_or("tool");
                if is_edit_tool(tname) {
                    sess.edits += 1;
                    self.stats.edits += 1;
                    let f = file_of(&ev.tool_input, &scwd);
                    if !f.is_empty() {
                        self.overlap.edited(&ev.session_id, &f, ts);
                    }
                } else if tname == "Bash" {
                    self.stats.commands += 1;
                } else if matches!(tname, "Read" | "Grep" | "Glob") {
                    self.stats.reads += 1;
                }
                ("stats".to_owned(), tname.to_owned())
            }
            "PostToolUseFailure" => {
                sess.fails += 1;
                self.stats.fails += 1;
                let tname = ev.tool_name.as_deref().unwrap_or("tool");
                let err = clip(&redact(ev.error.as_deref().unwrap_or("")), 80);
                ("fail".to_owned(), format!("{tname}: {err}"))
            }
            "Stop" | "StopFailure" => {
                sess.status = "rest".to_owned();
                self.undo.stop(&ev.session_id, ts);
                ("stop".to_owned(), sname.clone())
            }
            "PermissionRequest" => {
                sess.status = "wait".to_owned();
                let tname = ev.tool_name.as_deref().unwrap_or("tool");
                let summary = classify_tool(tname, &ev.tool_input);
                let detail = redact(&ev.tool_input.to_string());
                let dk = danger_key(Some("perm"), Some(&summary.kind), Some(&detail)).map(str::to_owned);
                let perm_id = format!("p_{}_{}", ts, self.seq());
                self.pending_perms.insert(
                    perm_id.clone(),
                    PendingPerm {
                        id: perm_id,
                        sess: ev.session_id.clone(),
                        name: sname.clone(),
                        kind: "perm".to_owned(),
                        tool_type: summary.kind.clone(),
                        label: summary.label.clone(),
                        detail: clip(&detail, 240),
                        danger: dk,
                        created_at: ts,
                    },
                );
                ("wait".to_owned(), summary.label)
            }
            "Notification" => ("ask".to_owned(), clip(&redact(ev.prompt.as_deref().unwrap_or("")), 80)),
            "PreCompact" => ("compact".to_owned(), "compacting".to_owned()),
            other => (other.to_ascii_lowercase(), sname.clone()),
        };

        let _ = self.loops.on_event(&ev.raw, ts);

        let id = self.seq.fetch_add(1, Ordering::Relaxed);
        let item = FeedItem {
            id,
            ts,
            sess: ev.session_id.clone(),
            name: sname,
            kind: feed_kind,
            label: feed_label,
            desc: None,
        };
        if self.feed.len() >= FEED_MAX {
            self.feed.pop_front();
        }
        self.feed.push_back(item.clone());

        let sess_json = self.sessions.get(&ev.session_id).cloned();
        self.broadcast(json!({ "k": "ev", "e": item, "s": sess_json }));
        Some(ev)
    }

    pub fn add_pin(&mut self, sid: &str, text: &str) -> Result<bool> {
        let cleaned: String = redact(text.trim()).chars().take(PIN_LEN).collect();
        if sid.is_empty() || cleaned.is_empty() {
            return Ok(false);
        }
        let list = self.pins.entry(sid.to_owned()).or_default();
        if list.len() >= PIN_MAX {
            list.remove(0);
        }
        list.push(cleaned);
        self.save_pins()?;
        Ok(true)
    }

    pub fn remove_pin(&mut self, sid: &str, idx: usize) -> Result<bool> {
        let Some(list) = self.pins.get_mut(sid) else {
            return Ok(false);
        };
        if idx >= list.len() {
            return Ok(false);
        }
        list.remove(idx);
        if list.is_empty() {
            self.pins.remove(sid);
        }
        self.save_pins()?;
        Ok(true)
    }

    pub fn save_pins(&self) -> Result<()> {
        write_json_atomic(&self.data_dir.join("pins.json"), &self.pins)
    }

    pub fn save_chat(&self) -> Result<()> {
        write_json_atomic(&self.data_dir.join("chat.json"), &self.chat.to_persisted())
    }

    pub fn snapshot_json(&self, local: bool, lang: &str) -> Value {
        let sessions: Vec<&SessionRecord> = self.sessions.values().collect();
        let perms: Vec<&PendingPerm> = self.pending_perms.values().collect();
        let feed: Vec<&FeedItem> = self.feed.iter().collect();
        json!({
            "k": "snap",
            "seq": self.seq(),
            "lang": lang,
            "local": local,
            "sessions": sessions,
            "perms": perms,
            "feed": feed,
            "stats": self.stats,
            "todos": self.todos,
        })
    }

    pub fn flush_all(&self) -> Result<()> {
        self.save_pins()?;
        self.save_chat()?;
        Ok(())
    }
}

fn classify_tool(name: &str, input: &Value) -> ToolSummary {
    let base_of = |key: &str| {
        input
            .get(key)
            .and_then(Value::as_str)
            .and_then(|p| Path::new(p).file_name())
            .and_then(|n| n.to_str())
            .unwrap_or("")
            .to_owned()
    };
    match name {
        "Read" => ToolSummary {
            kind: "read".into(),
            label: base_of("file_path"),
            file: input.get("file_path").and_then(Value::as_str).map(str::to_owned),
        },
        "Edit" | "MultiEdit" => ToolSummary {
            kind: "edit".into(),
            label: base_of("file_path"),
            file: input.get("file_path").and_then(Value::as_str).map(str::to_owned),
        },
        "Write" => ToolSummary {
            kind: "write".into(),
            label: base_of("file_path"),
            file: input.get("file_path").and_then(Value::as_str).map(str::to_owned),
        },
        "Grep" => ToolSummary {
            kind: "grep".into(),
            label: clip(&redact(&one_line(input.get("pattern").and_then(Value::as_str).unwrap_or(""))), 40),
            file: None,
        },
        "Glob" => ToolSummary {
            kind: "glob".into(),
            label: clip(&one_line(input.get("pattern").and_then(Value::as_str).unwrap_or("")), 40),
            file: None,
        },
        "Bash" => {
            let desc = input.get("description").and_then(Value::as_str);
            let cmd = input.get("command").and_then(Value::as_str).unwrap_or("");
            let label = match desc {
                Some(d) if !d.is_empty() => clip(&redact(&one_line(d)), 60),
                _ => clip(&redact(&one_line(cmd)), 48),
            };
            ToolSummary { kind: "bash".into(), label, file: None }
        }
        "WebSearch" => ToolSummary {
            kind: "web".into(),
            label: clip(&redact(&one_line(input.get("query").and_then(Value::as_str).unwrap_or(""))), 48),
            file: None,
        },
        "WebFetch" => ToolSummary {
            kind: "fetch".into(),
            label: clip(&redact(input.get("url").and_then(Value::as_str).unwrap_or("web")), 48),
            file: None,
        },
        "Agent" | "Task" => ToolSummary {
            kind: "agent".into(),
            label: input.get("subagent_type").and_then(Value::as_str).unwrap_or("agent").to_owned(),
            file: None,
        },
        _ => ToolSummary {
            kind: "other".into(),
            label: name.to_owned(),
            file: None,
        },
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn office_state_tracks_sessions_bounds_feed_and_persists_pins() {
        let dir = tempfile::tempdir().unwrap();
        let mut st = OfficeState::new(dir.path().to_path_buf());
        let mut rx = st.subscribe();

        st.ingest_raw_event(json!({
            "session_id": "s1",
            "hook_event_name": "SessionStart",
            "cwd": "D:/mystation/agents-station"
        }))
        .unwrap();
        assert_eq!(st.sessions.len(), 1);
        assert!(rx.try_recv().is_ok());

        for i in 0..100 {
            st.ingest_raw_event(json!({
                "session_id": "s1",
                "hook_event_name": "PreToolUse",
                "tool_name": "Read",
                "tool_input": { "file_path": format!("f{i}.rs") }
            }));
        }
        assert_eq!(st.feed.len(), FEED_MAX);

        assert!(st.add_pin("s1", "Remember sk-1234567890").unwrap());
        let saved = &st.pins["s1"][0];
        assert!(!saved.contains("sk-1234567890"));
        st.flush_all().unwrap();

        let reloaded = OfficeState::new(dir.path().to_path_buf());
        assert_eq!(reloaded.pins["s1"][0], *saved);
    }
}
