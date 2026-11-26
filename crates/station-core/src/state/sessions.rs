use crate::agent_events::{NormalizedEvent, normalize_agent_event};
use crate::atomic::write_json_atomic;
use crate::redact::redact;
use crate::shared::cache_clock::CacheState;
use crate::shared::chat_engine::ChatEngine;
use crate::shared::model_name::model_long;
use crate::shared::quick_choices::{QuickChoice, quick_choices};
use crate::state::git::RepoInfo;
use crate::state::loops::LoopGuard;
use crate::state::overlap::{OverlapTracker, file_of, is_edit_tool};
use crate::state::testrun::{test_counts, test_runner};
use crate::state::undo::UndoManager;
use crate::store::usage::{SessionUsage, ctx_max};
use anyhow::Result;
use serde::{Deserialize, Serialize};
use serde_json::{Map, Value, json};
use std::collections::{HashMap, VecDeque};
use std::io::Write;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{SystemTime, UNIX_EPOCH};
use tokio::sync::{broadcast, oneshot};

pub const DESKS: usize = 9;
pub const FEED_MAX: usize = 80;
pub const PIN_MAX: usize = 12;
pub const PIN_LEN: usize = 500;
pub const PERM_TTL_MS: u64 = 10 * 60 * 1000;

#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
pub struct ReapReport {
    pub reaped_sessions: usize,
    pub reaped_perms: usize,
    pub reaped_undo_dirs: usize,
}

pub fn now_ms() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_millis() as u64)
        .unwrap_or(0)
}

pub fn append_replay_event(data_dir: &Path, now_ms: u64, compact_event: &Value) -> Result<()> {
    let replay_dir = data_dir.join("replay");
    std::fs::create_dir_all(&replay_dir)?;
    let day = crate::store::otel::day_key(now_ms);
    let path = replay_dir.join(format!("{day}.jsonl"));
    let mut line = serde_json::to_string(&json!({ "t": now_ms, "m": compact_event }))?;
    line.push('\n');
    let mut file = std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(&path)?;
    file.write_all(line.as_bytes())?;
    Ok(())
}

pub const REPLAY_RANGE_MAX_DAYS: i64 = 31;

fn local_day_keys_in_range(from_ms: u64, to_ms: u64) -> Vec<String> {
    use chrono::{Duration, Local, TimeZone};

    let end_ms = to_ms.max(from_ms);
    let Some(end_date) = Local
        .timestamp_millis_opt(end_ms as i64)
        .single()
        .map(|dt| dt.date_naive())
    else {
        return vec![crate::store::otel::day_key(end_ms)];
    };
    let start_date = Local
        .timestamp_millis_opt(from_ms as i64)
        .single()
        .map(|dt| dt.date_naive())
        .unwrap_or(end_date);
    let min_start = end_date - Duration::days(REPLAY_RANGE_MAX_DAYS);
    let clamped_start = start_date.max(min_start).min(end_date);

    let mut keys = Vec::new();
    let mut cur = clamped_start;
    while cur <= end_date {
        keys.push(cur.format("%Y-%m-%d").to_string());
        let Some(next) = cur.succ_opt() else {
            break;
        };
        cur = next;
    }
    keys
}

pub fn read_replay_range(
    data_dir: &Path,
    from_ms: u64,
    to_ms: u64,
) -> Vec<crate::shared::timeline::ReplayItem> {
    let replay_dir = data_dir.join("replay");
    let end_ms = to_ms.max(from_ms);
    let day_keys = local_day_keys_in_range(from_ms, end_ms);
    let mut items = Vec::new();
    for dk in day_keys {
        let Ok(text) = std::fs::read_to_string(replay_dir.join(format!("{dk}.jsonl"))) else {
            continue;
        };
        for line in text.lines() {
            if let Ok(item) = serde_json::from_str::<crate::shared::timeline::ReplayItem>(line)
                && item.t <= end_ms
            {
                items.push(item);
            }
        }
    }
    items
}

pub(crate) fn clip(s: &str, max_chars: usize) -> String {
    let chars: Vec<char> = s.chars().collect();
    if chars.len() > max_chars {
        let mut out: String = chars[..max_chars.saturating_sub(1)].iter().collect();
        out.push('…');
        out
    } else {
        s.to_owned()
    }
}

pub(crate) fn one_line(s: &str) -> String {
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

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
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
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub model_label: Option<String>,
    pub cache: Option<CacheState>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub git: Option<RepoInfo>,
    #[serde(default)]
    pub cost_usd: f64,
    #[serde(default)]
    pub input_tokens: u64,
    #[serde(default)]
    pub output_tokens: u64,
    #[serde(default)]
    pub ctx_tokens: u64,
    #[serde(default)]
    pub ctx_max: u64,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub last_test: Option<Value>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub choices: Vec<QuickChoice>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub inbox: Option<String>,
    #[serde(skip)]
    pub transcript_path: Option<PathBuf>,
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
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub image: Option<String>,
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

#[derive(Debug, Clone, PartialEq)]
pub enum PermDecision {
    Allow,
    Deny { reason: String },
    Answers(Map<String, Value>),
    Retry,
    Keep,
}

struct HoldEntry {
    tx: oneshot::Sender<Value>,
    kind: String,
    sess: String,
    tool_input: Option<Value>,
}

fn build_decision_payload(
    kind: &str,
    tool_input: Option<&Value>,
    decision: &PermDecision,
) -> Value {
    match decision {
        PermDecision::Allow => {
            if kind == "plan" {
                json!({
                    "hookSpecificOutput": {
                        "hookEventName": "PreToolUse",
                        "permissionDecision": "allow",
                        "updatedInput": tool_input.cloned().unwrap_or_else(|| json!({}))
                    }
                })
            } else {
                json!({
                    "hookSpecificOutput": {
                        "hookEventName": "PermissionRequest",
                        "permissionDecision": "allow",
                        "decision": {
                            "behavior": "allow"
                        }
                    }
                })
            }
        }
        PermDecision::Deny { reason } => {
            let msg = if reason.trim().is_empty() {
                "Denied from Agents Station".to_owned()
            } else {
                reason.clone()
            };
            if kind == "plan" || kind == "ask" {
                json!({
                    "hookSpecificOutput": {
                        "hookEventName": "PreToolUse",
                        "permissionDecision": "deny",
                        "permissionDecisionReason": msg
                    }
                })
            } else {
                json!({
                    "hookSpecificOutput": {
                        "hookEventName": "PermissionRequest",
                        "permissionDecision": "deny",
                        "permissionDecisionReason": msg,
                        "decision": {
                            "behavior": "deny",
                            "message": msg
                        }
                    }
                })
            }
        }
        PermDecision::Answers(answers) => {
            let mut updated = tool_input
                .and_then(Value::as_object)
                .cloned()
                .unwrap_or_default();
            updated.insert("answers".to_owned(), Value::Object(answers.clone()));
            json!({
                "hookSpecificOutput": {
                    "hookEventName": "PreToolUse",
                    "permissionDecision": "allow",
                    "updatedInput": Value::Object(updated)
                }
            })
        }
        PermDecision::Retry => json!({
            "hookSpecificOutput": {
                "hookEventName": "PermissionDenied",
                "retry": true
            }
        }),
        PermDecision::Keep => json!({}),
    }
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
    holds: HashMap<String, HoldEntry>,
    pub pins: HashMap<String, Vec<String>>,
    pub known_dirs: Vec<String>,
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

fn perm_wire_json(perm: &PendingPerm, tool_input: Option<&Value>, meta: Option<&Value>) -> Value {
    let mut val = json!(perm);
    if let Some(obj) = val.as_object_mut() {
        if let Some(ti) = tool_input {
            obj.insert(
                "tool_input".to_owned(),
                crate::redact::redact_and_clip_json_value(
                    ti,
                    crate::redact::TOOL_INPUT_WIRE_MAX_STR_CHARS,
                ),
            );
        }
        if let Some(m) = meta {
            obj.insert(
                "meta".to_owned(),
                crate::redact::redact_and_clip_json_value(
                    m,
                    crate::redact::TOOL_INPUT_WIRE_MAX_STR_CHARS,
                ),
            );
        }
    }
    val
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
        let dirs_file = data_dir.join("projects.json");
        let known_dirs: Vec<String> = std::fs::read_to_string(&dirs_file)
            .ok()
            .and_then(|s| serde_json::from_str(&s).ok())
            .unwrap_or_default();
        Self {
            undo: UndoManager::new(data_dir.join("undo")),
            data_dir,
            sessions: HashMap::new(),
            feed: VecDeque::with_capacity(FEED_MAX),
            pending_perms: HashMap::new(),
            holds: HashMap::new(),
            pins,
            known_dirs,
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

    pub fn next_seq(&self) -> u64 {
        self.seq.fetch_add(1, Ordering::Relaxed)
    }

    pub fn broadcast(&self, msg: Value) {
        self.seq.fetch_add(1, Ordering::Relaxed);
        let _ = self.tx.send(msg);
    }

    pub fn register_hold(&mut self, perm: PendingPerm, tx: oneshot::Sender<Value>) {
        self.register_hold_with_input(perm, tx, None);
    }

    pub fn register_hold_with_input(
        &mut self,
        perm: PendingPerm,
        tx: oneshot::Sender<Value>,
        tool_input: Option<Value>,
    ) {
        let id = perm.id.clone();
        let sess_id = perm.sess.clone();
        let deny_meta = if perm.kind == "denied" {
            self.chat.by_id(&id).and_then(|m| m.deny.as_ref())
        } else {
            None
        };
        let wire_perm = perm_wire_json(&perm, tool_input.as_ref(), deny_meta);
        let entry = HoldEntry {
            tx,
            kind: perm.kind.clone(),
            sess: sess_id.clone(),
            tool_input,
        };
        self.holds.insert(id.clone(), entry);
        self.pending_perms.insert(id, perm);

        if let Some(s) = self.sessions.get_mut(&sess_id)
            && s.status != "wait"
        {
            s.status = "wait".to_owned();
            let s_clone = s.clone();
            self.broadcast(json!({ "k": "sess", "s": s_clone }));
        }
        self.broadcast(json!({ "k": "perm", "p": wire_perm }));
    }

    pub fn hold_tool_input(&self, id: &str) -> Option<&Value> {
        self.holds.get(id).and_then(|h| h.tool_input.as_ref())
    }

    pub fn resolve_hold(&mut self, id: &str, decision: PermDecision) -> Option<Value> {
        let hold = self.holds.remove(id);
        let perm = self.pending_perms.remove(id);
        if hold.is_none() && perm.is_none() {
            return None;
        }
        let kind = hold
            .as_ref()
            .map(|h| h.kind.as_str())
            .or_else(|| perm.as_ref().map(|p| p.kind.as_str()))
            .unwrap_or("perm");
        let sess_id = hold
            .as_ref()
            .map(|h| h.sess.clone())
            .or_else(|| perm.as_ref().map(|p| p.sess.clone()))
            .unwrap_or_default();
        let response = build_decision_payload(
            kind,
            hold.as_ref().and_then(|h| h.tool_input.as_ref()),
            &decision,
        );

        if kind == "denied" {
            let next_state = if matches!(decision, PermDecision::Retry) {
                "retry"
            } else {
                "kept"
            };
            self.chat.set_deny(id, next_state);
            let _ = self.save_chat();
        }
        if let Some(h) = hold {
            let _ = h.tx.send(response.clone());
        }
        self.broadcast(json!({ "k": "permDone", "id": id }));
        self.restore_session_work_if_idle(&sess_id);
        Some(response)
    }

    pub fn cancel_hold(&mut self, id: &str) -> bool {
        let hold = self.holds.remove(id);
        let perm = self.pending_perms.remove(id);
        if hold.is_none() && perm.is_none() {
            return false;
        }
        let sess_id = hold
            .as_ref()
            .map(|h| h.sess.clone())
            .or_else(|| perm.as_ref().map(|p| p.sess.clone()))
            .unwrap_or_default();
        let is_denied = hold.as_ref().is_some_and(|h| h.kind == "denied")
            || perm.as_ref().is_some_and(|p| p.kind == "denied");
        if is_denied {
            self.chat.set_deny(id, "kept");
            let _ = self.save_chat();
        }
        self.broadcast(json!({ "k": "permDone", "id": id }));
        self.restore_session_work_if_idle(&sess_id);
        true
    }

    fn restore_session_work_if_idle(&mut self, sess_id: &str) {
        if !sess_id.is_empty() && !self.pending_perms.values().any(|p| p.sess == sess_id) {
            if let Some(s) = self.sessions.get_mut(sess_id)
                && s.status == "wait"
            {
                s.status = "work".to_owned();
                let s_clone = s.clone();
                self.broadcast(json!({ "k": "sess", "s": s_clone }));
            }
        }
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
                model_label: None,
                cache: None,
                git: None,
                cost_usd: 0.0,
                input_tokens: 0,
                output_tokens: 0,
                ctx_tokens: 0,
                ctx_max: 0,
                last_test: None,
                choices: Vec::new(),
                inbox: None,
                transcript_path: None,
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
        if let Some(tp) = ev
            .raw
            .get("transcript_path")
            .and_then(Value::as_str)
            .filter(|s| !s.is_empty())
        {
            sess.transcript_path = Some(PathBuf::from(tp));
        }
        if let Some(ib) = ev
            .raw
            .get("messagingSocketPath")
            .or_else(|| ev.raw.get("inbox"))
            .and_then(Value::as_str)
            .filter(|s| !s.is_empty())
        {
            sess.inbox = Some(ib.to_owned());
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
                self.pending_perms.retain(|_, p| p.sess != ev.session_id);
                self.holds.retain(|id, _| self.pending_perms.contains_key(id));
                let msg = json!({ "k": "gone", "id": ev.session_id });
                let _ = append_replay_event(&self.data_dir, ts, &msg);
                self.broadcast(msg);
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
                    if let Some(cmd) = ev.tool_input.get("command").and_then(Value::as_str)
                        && let Some(runner) = test_runner(cmd)
                    {
                        let out_text = ev
                            .raw
                            .get("tool_response")
                            .map(|tr| {
                                if let Some(s) = tr.as_str() {
                                    s.to_owned()
                                } else {
                                    let stdout = tr.get("stdout").and_then(Value::as_str).unwrap_or("");
                                    let stderr = tr.get("stderr").and_then(Value::as_str).unwrap_or("");
                                    format!("{stdout}\n{stderr}")
                                }
                            })
                            .unwrap_or_default();
                        if let Some(counts) = test_counts(&out_text) {
                            sess.last_test = Some(json!({
                                "runner": runner,
                                "pass": counts.pass,
                                "fail": counts.fail,
                                "at": ts,
                            }));
                        } else {
                            sess.last_test = Some(json!({
                                "runner": runner,
                                "at": ts,
                            }));
                        }
                    }
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
                if let Some(msg_txt) = ev
                    .raw
                    .get("last_assistant_message")
                    .or_else(|| ev.raw.get("message"))
                    .and_then(Value::as_str)
                {
                    sess.choices = quick_choices(msg_txt);
                }
                self.undo.stop(&ev.session_id, ts);
                ("stop".to_owned(), sname.clone())
            }
            "PermissionRequest" => {
                sess.status = "wait".to_owned();
                let tname = ev.tool_name.as_deref().unwrap_or("tool");
                let summary = classify_tool(tname, &ev.tool_input);
                ("wait".to_owned(), summary.label)
            }
            "Notification" => ("ask".to_owned(), clip(&redact(ev.prompt.as_deref().unwrap_or("")), 80)),
            "PreCompact" => ("compact".to_owned(), "compacting".to_owned()),
            other => (other.to_ascii_lowercase(), sname.clone()),
        };

        let _ = self.loops.on_event(&ev.raw, ts);
        if !scwd.is_empty() {
            self.note_dir(&scwd);
        }

        let id = self.seq.fetch_add(1, Ordering::Relaxed);
        let item = FeedItem {
            id,
            ts,
            sess: ev.session_id.clone(),
            name: sname,
            kind: feed_kind,
            label: feed_label,
            desc: None,
            image: None,
        };
        if self.feed.len() >= FEED_MAX {
            self.feed.pop_front();
        }
        self.feed.push_back(item.clone());

        let sess_json = self.sessions.get(&ev.session_id).cloned();
        let msg = json!({ "k": "ev", "e": item, "s": sess_json });
        let _ = append_replay_event(&self.data_dir, ts, &msg);
        self.broadcast(msg);
        Some(ev)
    }

    pub fn note_dir(&mut self, dir: &str) {
        if dir.is_empty() || !Path::new(dir).is_absolute() {
            return;
        }
        if self.known_dirs.first().map(String::as_str) == Some(dir) {
            return;
        }
        self.known_dirs.retain(|d| d != dir);
        self.known_dirs.insert(0, dir.to_owned());
        self.known_dirs.truncate(30);
        let _ = write_json_atomic(&self.data_dir.join("projects.json"), &self.known_dirs);
    }

    pub fn apply_git(&mut self, sid: &str, info: RepoInfo) {
        if let Some(s) = self.sessions.get_mut(sid) {
            s.git = Some(info);
        }
    }

    pub fn apply_usage(&mut self, sid: &str, u: &SessionUsage) {
        if let Some(s) = self.sessions.get_mut(sid) {
            s.cost_usd = u.cost;
            s.input_tokens = u.input;
            s.output_tokens = u.out;
            s.ctx_tokens = u.ctx;
            s.ctx_max = ctx_max(u);
            if !u.model.is_empty() {
                s.model = Some(u.model.clone());
                let lbl = model_long(&u.model);
                if !lbl.is_empty() {
                    s.model_label = Some(lbl);
                }
            }
        }
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
        let perms: Vec<Value> = self
            .pending_perms
            .values()
            .map(|p| {
                let meta = if p.kind == "denied" {
                    self.chat.by_id(&p.id).and_then(|m| m.deny.as_ref())
                } else {
                    None
                };
                perm_wire_json(p, self.hold_tool_input(&p.id), meta)
            })
            .collect();
        let stuck: Vec<Value> = self
            .sessions
            .values()
            .filter_map(|s| {
                let st = self.loops.stuck_of(&s.id)?;
                Some(json!({
                    "id": format!("stuck:{}", s.id),
                    "sess": s.id,
                    "name": s.name,
                    "kind": st.kind,
                    "label": st.label,
                    "n": st.n,
                    "mins": st.mins,
                    "usd": st.usd,
                }))
            })
            .collect();
        let feed: Vec<&FeedItem> = self.feed.iter().collect();
        json!({
            "k": "snap",
            "seq": self.seq(),
            "lang": lang,
            "local": local,
            "sessions": sessions,
            "perms": perms,
            "stuck": stuck,
            "feed": feed,
            "stats": self.stats,
            "todos": self.todos,
            "pins": self.pins,
        })
    }

    pub fn flush_all(&self) -> Result<()> {
        self.save_pins()?;
        self.save_chat()?;
        Ok(())
    }

    pub fn reap_stale(&mut self, now: u64, auto_close_hours: u8) -> ReapReport {
        let mut reaped_sids = std::collections::HashSet::new();
        if auto_close_hours > 0 {
            let idle_limit_ms = (auto_close_hours as u64).saturating_mul(3_600_000);
            let stale: Vec<String> = self
                .sessions
                .iter()
                .filter(|(_, s)| now.saturating_sub(s.updated_at) >= idle_limit_ms)
                .map(|(id, _)| id.clone())
                .collect();
            for sid in stale {
                if self.sessions.remove(&sid).is_some() {
                    let msg = json!({ "k": "gone", "id": sid });
                    let _ = append_replay_event(&self.data_dir, now, &msg);
                    self.broadcast(msg);
                    reaped_sids.insert(sid);
                }
            }
        }

        let perms_before = self.pending_perms.len();
        self.pending_perms.retain(|_, p| {
            now.saturating_sub(p.created_at) < PERM_TTL_MS && !reaped_sids.contains(&p.sess)
        });
        self.holds.retain(|id, _| self.pending_perms.contains_key(id));
        let reaped_perms = perms_before - self.pending_perms.len();
        let reaped_undo_dirs = self.undo.prune_stale_disk();

        ReapReport {
            reaped_sessions: reaped_sids.len(),
            reaped_perms,
            reaped_undo_dirs,
        }
    }
}

pub(crate) fn classify_tool(name: &str, input: &Value) -> ToolSummary {
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

    #[test]
    fn append_replay_event_writes_local_date_jsonl_on_every_live_event() {
        let dir = tempfile::tempdir().unwrap();
        let mut st = OfficeState::new(dir.path().to_path_buf());

        let before_ms = now_ms();
        let ev1 = st
            .ingest_raw_event(json!({
                "session_id": "s-replay",
                "hook_event_name": "UserPromptSubmit",
                "cwd": "D:/repo/app",
                "prompt": "build replay log"
            }))
            .unwrap();
        assert_eq!(ev1.session_id, "s-replay");
        st.ingest_raw_event(json!({
            "session_id": "s-replay",
            "hook_event_name": "Stop",
            "cwd": "D:/repo/app"
        }))
        .unwrap();
        st.ingest_raw_event(json!({
            "session_id": "s-replay",
            "hook_event_name": "SessionEnd"
        }))
        .unwrap();

        let day = crate::store::otel::day_key(before_ms);
        let replay_file = dir.path().join("replay").join(format!("{day}.jsonl"));
        assert!(
            replay_file.exists(),
            "expected replay JSONL at {} immediately without flush_all",
            replay_file.display()
        );
        let text = std::fs::read_to_string(&replay_file).unwrap();
        let items: Vec<crate::shared::timeline::ReplayItem> = text
            .lines()
            .map(|l| serde_json::from_str(l).unwrap())
            .collect();
        assert_eq!(items.len(), 3);
        assert_eq!(items[0].m["k"], "ev");
        assert_eq!(items[0].m["e"]["type"], "prompt");
        assert_eq!(items[1].m["k"], "ev");
        assert_eq!(items[1].m["e"]["type"], "stop");
        assert_eq!(items[2].m["k"], "gone");

        // Direct call to append_replay_event writes with Local day key.
        let fixed_ts = 1_793_091_600_000u64;
        append_replay_event(dir.path(), fixed_ts, &json!({ "k": "ev", "e": { "type": "commit" } }))
            .unwrap();
        let fixed_day = crate::store::otel::day_key(fixed_ts);
        let fixed_file = dir.path().join("replay").join(format!("{fixed_day}.jsonl"));
        assert!(fixed_file.exists());
    }

    #[test]
    fn reap_stale_removes_idle_sessions_expired_perms_and_stale_undo_dirs() {
        let dir = tempfile::tempdir().unwrap();
        let mut st = OfficeState::new(dir.path().to_path_buf());
        let now = 1_793_091_600_000u64;
        let hour = 3_600_000u64;

        st.ingest_raw_event(json!({
            "session_id": "s-idle",
            "hook_event_name": "SessionStart",
            "cwd": "D:/repo/idle"
        }))
        .unwrap();
        st.ingest_raw_event(json!({
            "session_id": "s-active",
            "hook_event_name": "SessionStart",
            "cwd": "D:/repo/active"
        }))
        .unwrap();
        st.sessions.get_mut("s-idle").unwrap().updated_at = now - 3 * hour;
        st.sessions.get_mut("s-active").unwrap().updated_at = now - 10 * 60_000;

        let make_perm = |id: &str, sess: &str, created_at: u64| PendingPerm {
            id: id.to_owned(),
            sess: sess.to_owned(),
            name: sess.to_owned(),
            kind: "perm".to_owned(),
            tool_type: "bash".to_owned(),
            label: "cmd".to_owned(),
            detail: "echo hi".to_owned(),
            danger: None,
            created_at,
        };
        st.pending_perms
            .insert("p-expired".into(), make_perm("p-expired", "s-active", now - 15 * 60_000));
        st.pending_perms
            .insert("p-orphaned".into(), make_perm("p-orphaned", "s-idle", now - 60_000));
        st.pending_perms
            .insert("p-fresh".into(), make_perm("p-fresh", "s-active", now - 60_000));

        let undo_dir = dir.path().join("undo");
        for i in 0..53usize {
            std::fs::create_dir_all(undo_dir.join(format!("old_{i:03}"))).unwrap();
        }

        // autoCloseHours = 0: sessions kept, expired perms & stale undo dirs still cleaned.
        let r0 = st.reap_stale(now, 0);
        assert_eq!(r0.reaped_sessions, 0);
        assert_eq!(r0.reaped_perms, 1);
        assert!(r0.reaped_undo_dirs >= 3);
        assert!(st.sessions.contains_key("s-idle"));
        assert!(!st.pending_perms.contains_key("p-expired"));
        assert!(st.pending_perms.contains_key("p-fresh"));

        // autoCloseHours = 2: s-idle (3h idle) and its perm are reaped; s-active survives.
        let r2 = st.reap_stale(now, 2);
        assert_eq!(r2.reaped_sessions, 1);
        assert_eq!(r2.reaped_perms, 1);
        assert!(!st.sessions.contains_key("s-idle"));
        assert!(st.sessions.contains_key("s-active"));
        assert!(!st.pending_perms.contains_key("p-orphaned"));
        assert!(st.pending_perms.contains_key("p-fresh"));
    }

    #[test]
    fn read_replay_range_iterates_contiguous_calendar_days_and_clamps_without_gaps() {
        use chrono::{Duration, Local, NaiveDate, TimeZone};

        let dir = tempfile::tempdir().unwrap();
        let base_date = NaiveDate::from_ymd_opt(2026, 10, 1).unwrap();
        let noon_ms = |d: NaiveDate| -> u64 {
            Local
                .from_local_datetime(&d.and_hms_opt(12, 0, 0).unwrap())
                .earliest()
                .unwrap()
                .timestamp_millis() as u64
        };

        // Write 40 consecutive days of replay events (2026-10-01 ..= 2026-11-09).
        for offset in 0..40i64 {
            let d = base_date + Duration::days(offset);
            let ts = noon_ms(d);
            let ds = d.format("%Y-%m-%d").to_string();
            append_replay_event(dir.path(), ts, &json!({ "k": "ev", "day": ds })).unwrap();
        }

        // 1. 20-day range: every interior day must be present (old 14-day loop dropped days 15..19).
        let d0 = base_date;
        let d19 = base_date + Duration::days(19);
        let items_20 = read_replay_range(dir.path(), noon_ms(d0), noon_ms(d19));
        let days_20: Vec<&str> = items_20
            .iter()
            .map(|it| it.m["day"].as_str().unwrap())
            .collect();
        assert_eq!(days_20.len(), 20, "expected all 20 contiguous days, got {days_20:?}");
        assert_eq!(days_20.first().copied(), Some("2026-10-01"));
        assert_eq!(days_20.last().copied(), Some("2026-10-20"));

        // 2. 40-day range (> 31 days): clamped at end - 31 days with zero interior gaps.
        let d39 = base_date + Duration::days(39);
        let items_40 = read_replay_range(dir.path(), noon_ms(d0), noon_ms(d39));
        let parsed_dates: Vec<NaiveDate> = items_40
            .iter()
            .map(|it| NaiveDate::parse_from_str(it.m["day"].as_str().unwrap(), "%Y-%m-%d").unwrap())
            .collect();
        let expected_start = d39 - Duration::days(31);
        assert_eq!(parsed_dates.first().copied(), Some(expected_start));
        assert_eq!(parsed_dates.last().copied(), Some(d39));
        assert_eq!(parsed_dates.len(), 32);
        for pair in parsed_dates.windows(2) {
            assert_eq!(
                pair[0].succ_opt(),
                Some(pair[1]),
                "interior gap between {} and {}",
                pair[0],
                pair[1]
            );
        }
    }

    #[test]
    fn cancel_hold_restores_session_status_from_wait_to_work() {
        let dir = tempfile::tempdir().unwrap();
        let mut st = OfficeState::new(dir.path().to_path_buf());
        st.ingest_raw_event(json!({
            "session_id": "s-cancel",
            "hook_event_name": "UserPromptSubmit",
            "cwd": "D:/repo/app",
            "prompt": "do work"
        }))
        .unwrap();
        assert_eq!(st.sessions["s-cancel"].status, "work");

        let mut rx_sse = st.subscribe();
        let (tx, _rx) = oneshot::channel();
        st.register_hold(
            PendingPerm {
                id: "p-cancel-1".into(),
                sess: "s-cancel".into(),
                name: "app".into(),
                kind: "perm".into(),
                tool_type: "bash".into(),
                label: "echo".into(),
                detail: "echo hi".into(),
                danger: None,
                created_at: 1,
            },
            tx,
        );
        assert_eq!(st.sessions["s-cancel"].status, "wait");

        while rx_sse.try_recv().is_ok() {}

        assert!(st.cancel_hold("p-cancel-1"));
        assert_eq!(
            st.sessions["s-cancel"].status, "work",
            "cancel_hold must restore session status from wait to work when no holds remain"
        );
        let mut saw_work_sess = false;
        while let Ok(msg) = rx_sse.try_recv() {
            if msg["k"] == "sess" && msg["s"]["id"] == "s-cancel" && msg["s"]["status"] == "work" {
                saw_work_sess = true;
            }
        }
        assert!(saw_work_sess, "cancel_hold must broadcast updated sess status=work");
    }

    #[test]
    fn snapshot_json_redacts_clips_tool_input_attaches_denied_meta_and_stuck_entries() {
        let dir = tempfile::tempdir().unwrap();
        let mut st = OfficeState::new(dir.path().to_path_buf());
        let ts = now_ms();
        st.ingest_raw_event(json!({
            "session_id": "s-wire",
            "hook_event_name": "UserPromptSubmit",
            "cwd": "D:/repo/app",
            "prompt": "work"
        }))
        .unwrap();

        // Trigger a stuck repeat loop on s-wire (3 identical failures + next PreToolUse retry)
        for i in 0..3 {
            st.loops.on_event(
                &json!({
                    "session_id": "s-wire",
                    "hook_event_name": "PostToolUseFailure",
                    "tool_name": "Bash",
                    "tool_input": { "command": "cargo test" },
                    "error": "exit 101"
                }),
                ts + i * 1000,
            );
        }
        let _ = st.loops.check(
            &json!({
                "session_id": "s-wire",
                "hook_event_name": "PreToolUse",
                "tool_name": "Bash",
                "tool_input": { "command": "cargo test" }
            }),
            ts + 4000,
        );

        // Register a plan hold with secret + huge plan (> 16k chars)
        let (tx_plan, _rx_plan) = oneshot::channel();
        let huge_plan = format!(
            "secret sk-ant-api03-leak123456 {}",
            "step item details\n".repeat(1200)
        );
        st.register_hold_with_input(
            PendingPerm {
                id: "plan-wire".into(),
                sess: "s-wire".into(),
                name: "app".into(),
                kind: "plan".into(),
                tool_type: "plan".into(),
                label: "Plan".into(),
                detail: "Plan".into(),
                danger: None,
                created_at: ts,
            },
            tx_plan,
            Some(json!({ "plan": huge_plan })),
        );

        // Register a denied hold with matching deny chat card containing reason
        let card = st.chat.deny_card(
            "s-wire",
            "app",
            json!({ "reason": "Protected branch", "state": "held" }),
            ts,
        );
        let (tx_den, _rx_den) = oneshot::channel();
        st.register_hold(
            PendingPerm {
                id: card.id.clone(),
                sess: "s-wire".into(),
                name: "app".into(),
                kind: "denied".into(),
                tool_type: "bash".into(),
                label: "git push".into(),
                detail: "git push".into(),
                danger: None,
                created_at: ts,
            },
            tx_den,
        );

        let snap = st.snapshot_json(true, "en");
        let perms = snap["perms"].as_array().unwrap();

        let plan_p = perms.iter().find(|p| p["id"] == "plan-wire").unwrap();
        let plan_str = plan_p["tool_input"]["plan"].as_str().unwrap();
        assert!(!plan_str.contains("sk-ant-api03-leak123456"));
        assert!(plan_str.chars().count() <= crate::redact::TOOL_INPUT_WIRE_MAX_STR_CHARS);
        // Raw HoldEntry.tool_input must remain unclipped for hook decision round-trip
        assert!(
            st.hold_tool_input("plan-wire").unwrap()["plan"]
                .as_str()
                .unwrap()
                .len()
                > 20_000
        );

        let den_p = perms.iter().find(|p| p["id"] == card.id).unwrap();
        assert_eq!(den_p["meta"]["reason"], "Protected branch");

        let stuck = snap["stuck"].as_array().unwrap();
        assert_eq!(stuck.len(), 1);
        assert_eq!(stuck[0]["sess"], "s-wire");
        assert_eq!(stuck[0]["kind"], "repeat");
    }
}
