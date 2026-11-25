use base64::Engine;
use base64::engine::general_purpose::URL_SAFE_NO_PAD;
use regex::Regex;
use serde::{Deserialize, Serialize};
use serde_json::{Map, Value};
use sha2::{Digest, Sha256};
use std::collections::{HashMap, VecDeque};
use std::sync::OnceLock;

pub const LOOP_N: [u32; 4] = [2, 3, 4, 5];
pub const LOOP_MIN: [u32; 4] = [5, 10, 20, 30];
const SESS_MAX: usize = 64;
const AGENTS_MAX: usize = 8;
const HINT_MAX: usize = 1000;
const COMPACT_N: usize = 3;
const COMPACT_MS: u64 = 30 * 60 * 1000;
const COST_MIN_USD: f64 = 0.5;
const QUIET_MS: u64 = 5 * 60 * 1000;
const STOP_MS: u64 = 15 * 60 * 1000;

#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub struct LoopConfig {
    pub on: bool,
    pub n: u32,
    pub min: u32,
}

impl Default for LoopConfig {
    fn default() -> Self {
        Self {
            on: true,
            n: 3,
            min: 10,
        }
    }
}

impl LoopConfig {
    pub fn normalized(self) -> Self {
        Self {
            on: self.on,
            n: if LOOP_N.contains(&self.n) { self.n } else { 3 },
            min: if LOOP_MIN.contains(&self.min) { self.min } else { 10 },
        }
    }
}

fn is_edit_tool(name: &str) -> bool {
    matches!(name, "Edit" | "MultiEdit" | "Write" | "NotebookEdit")
}

fn is_look_tool(name: &str) -> bool {
    matches!(
        name,
        "Read"
            | "NotebookRead"
            | "Grep"
            | "Glob"
            | "LS"
            | "WebFetch"
            | "WebSearch"
            | "TodoWrite"
            | "TaskCreate"
            | "TaskUpdate"
            | "TaskList"
            | "TaskGet"
            | "ToolSearch"
            | "BashOutput"
            | "TaskOutput"
    )
}

fn canon(v: &Value, top: bool) -> Value {
    match v {
        Value::String(s) => Value::String(s.split_whitespace().collect::<Vec<_>>().join(" ")),
        Value::Array(arr) => Value::Array(arr.iter().map(|x| canon(x, false)).collect()),
        Value::Object(obj) => {
            let mut keys: Vec<&String> = obj
                .keys()
                .filter(|k| !(top && matches!(k.as_str(), "description" | "timeout" | "run_in_background")))
                .collect();
            keys.sort();
            let mut out = Map::new();
            for k in keys {
                out.insert(k.clone(), canon(&obj[k], false));
            }
            Value::Object(out)
        }
        other => other.clone(),
    }
}

fn digest16(s: &str) -> String {
    let hash = Sha256::digest(s.as_bytes());
    let b64 = URL_SAFE_NO_PAD.encode(hash);
    b64.chars().take(16).collect()
}

pub fn call_key(name: &str, input: &Value) -> String {
    let serialized = serde_json::to_string(&canon(input, true)).unwrap_or_default();
    digest16(&format!("{name}\0{serialized}"))
}

pub fn err_key(err: &str) -> String {
    struct ErrRegexes {
        trunc: Regex,
        iso: Regex,
        hms: Regex,
        dur: Regex,
        hex: Regex,
        big_num: Regex,
        tmp: Regex,
    }
    static RE: OnceLock<ErrRegexes> = OnceLock::new();
    let r = RE.get_or_init(|| ErrRegexes {
        trunc: Regex::new(r"\.\.\. \[\d+ characters truncated\] \.\.\.").unwrap(),
        iso: Regex::new(r"\d{4}-\d\d-\d\dT[\d:.]+Z?").unwrap(),
        hms: Regex::new(r"\b\d{1,2}:\d\d(:\d\d(\.\d+)?)?\b").unwrap(),
        dur: Regex::new(r"\b\d+(\.\d+)?\s?(ms|s|sec|m|min)\b").unwrap(),
        hex: Regex::new(r"(?i)\b0x[0-9a-f]+\b").unwrap(),
        big_num: Regex::new(r"\b\d{4,}\b").unwrap(),
        tmp: Regex::new(r"/(private/)?(tmp|var/folders)/\S+").unwrap(),
    });
    let s = r.trunc.replace_all(err, "…");
    let s = r.iso.replace_all(&s, "#time");
    let s = r.hms.replace_all(&s, "#time");
    let s = r.dur.replace_all(&s, "#dur");
    let s = r.hex.replace_all(&s, "#hex");
    let s = r.big_num.replace_all(&s, "#n");
    let s = r.tmp.replace_all(&s, "/tmp/…");
    let collapsed = s.split_whitespace().collect::<Vec<_>>().join(" ");
    let clipped: String = collapsed.chars().take(8000).collect();
    digest16(&clipped)
}

fn default_label(name: &str, input: &Value) -> String {
    let s = input
        .get("command")
        .or_else(|| input.get("file_path"))
        .or_else(|| input.get("url"))
        .or_else(|| input.get("pattern"))
        .or_else(|| input.get("query"))
        .and_then(Value::as_str)
        .unwrap_or(name);
    s.split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
        .chars()
        .take(60)
        .collect()
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StuckInfo {
    pub kind: String,
    pub at: u64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub agent: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub label: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tool: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub n: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub blocked: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub usd: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub mins: Option<u32>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LoopSignal {
    #[serde(rename = "type")]
    pub sig_type: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub why: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub stuck: Option<StuckInfo>,
    #[serde(default)]
    pub hint_lost: bool,
    #[serde(default)]
    pub hint: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DenyInfo {
    pub n: u32,
    pub label: String,
    pub blocked: u32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LoopCheckResult {
    pub stop: bool,
    pub deny: Option<DenyInfo>,
    pub hint: Option<String>,
    pub sig: Option<LoopSignal>,
}

struct Streak {
    key: String,
    err: String,
    fails: u32,
    blocked: u32,
    tool: String,
    label: String,
}

struct CostTrack {
    usd0: f64,
    usd: f64,
    at0: u64,
    rise_at: u64,
    lines: String,
    fired: bool,
}

#[derive(Default)]
struct SessLoop {
    streaks: HashMap<String, Streak>,
    streak_order: VecDeque<String>,
    compacts: Vec<u64>,
    cost: Option<CostTrack>,
    stuck: Option<StuckInfo>,
    stop: u64,
    hint: Option<String>,
    mode: String,
}

pub struct LoopGuard {
    config: LoopConfig,
    known: HashMap<String, SessLoop>,
    order: VecDeque<String>,
}

impl Default for LoopGuard {
    fn default() -> Self {
        Self::new(LoopConfig::default())
    }
}

impl LoopGuard {
    pub fn new(config: LoopConfig) -> Self {
        Self {
            config: config.normalized(),
            known: HashMap::new(),
            order: VecDeque::new(),
        }
    }

    pub fn config(&self) -> LoopConfig {
        self.config
    }

    pub fn set_config(&mut self, cfg: LoopConfig) {
        self.config = cfg.normalized();
    }

    fn sess_mut(&mut self, sid: &str) -> &mut SessLoop {
        if !self.known.contains_key(sid) {
            self.known.insert(sid.to_owned(), SessLoop::default());
            self.order.push_back(sid.to_owned());
            while self.known.len() > SESS_MAX {
                if let Some(old) = self.order.pop_front() {
                    self.known.remove(&old);
                }
            }
        }
        self.known.get_mut(sid).unwrap()
    }

    pub fn stop(&mut self, sid: &str, now_ms: u64) -> bool {
        if sid.is_empty() {
            return false;
        }
        self.sess_mut(sid).stop = now_ms;
        true
    }

    pub fn hint(&mut self, sid: &str, text: &str) -> bool {
        let h: String = text.trim().chars().take(HINT_MAX).collect();
        if sid.is_empty() || h.is_empty() {
            return false;
        }
        self.sess_mut(sid).hint = Some(h);
        true
    }

    pub fn stuck_of(&self, sid: &str) -> Option<&StuckInfo> {
        self.known.get(sid)?.stuck.as_ref()
    }

    pub fn on_event(&mut self, p: &Value, now_ms: u64) -> Option<LoopSignal> {
        let sid = p.get("session_id").and_then(Value::as_str)?;
        let ev = p.get("hook_event_name").and_then(Value::as_str)?;
        if sid.is_empty() || ev.is_empty() {
            return None;
        }
        let cfg = self.config;
        let who = p
            .get("agent_id")
            .and_then(Value::as_str)
            .filter(|s| !s.is_empty())
            .unwrap_or("main")
            .to_owned();
        let x = self.sess_mut(sid);
        if let Some(mode) = p.get("permission_mode").and_then(Value::as_str) {
            x.mode = mode.to_owned();
        }

        match ev {
            "UserPromptSubmit" | "Stop" | "StopFailure" | "SessionEnd" => {
                let lost = x.hint.take().is_some();
                x.streaks.clear();
                x.streak_order.clear();
                x.compacts.clear();
                x.stop = 0;
                if let Some(c) = &mut x.cost {
                    c.usd0 = c.usd;
                    c.at0 = now_ms;
                    c.rise_at = now_ms;
                    c.fired = false;
                }
                if let Some(was) = x.stuck.take() {
                    return Some(LoopSignal {
                        sig_type: "clear".to_owned(),
                        why: Some(if ev == "UserPromptSubmit" { "you" } else { "end" }.to_owned()),
                        stuck: Some(was),
                        hint_lost: lost,
                        hint: false,
                    });
                }
                if lost {
                    Some(LoopSignal {
                        sig_type: "hintLost".to_owned(),
                        why: None,
                        stuck: None,
                        hint_lost: true,
                        hint: false,
                    })
                } else {
                    None
                }
            }
            "PostToolUseFailure" => {
                if p.get("is_interrupt").and_then(Value::as_bool) == Some(true) {
                    return None;
                }
                let tool = p.get("tool_name").and_then(Value::as_str).unwrap_or("");
                let input = p.get("tool_input").unwrap_or(&Value::Null);
                let err = p.get("error").and_then(Value::as_str).unwrap_or("");
                let key = call_key(tool, input);
                let ek = err_key(err);
                if let Some(cur) = x.streaks.get_mut(&who)
                    && cur.key == key
                    && cur.err == ek
                {
                    cur.fails += 1;
                    return None;
                }
                x.streaks.insert(
                    who.clone(),
                    Streak {
                        key,
                        err: ek,
                        fails: 1,
                        blocked: 0,
                        tool: tool.to_owned(),
                        label: default_label(tool, input),
                    },
                );
                x.streak_order.retain(|k| k != &who);
                x.streak_order.push_back(who);
                while x.streaks.len() > AGENTS_MAX {
                    if let Some(old) = x.streak_order.pop_front() {
                        x.streaks.remove(&old);
                    }
                }
                None
            }
            "PostToolUse" => {
                let tool = p.get("tool_name").and_then(Value::as_str).unwrap_or("");
                let input = p.get("tool_input").unwrap_or(&Value::Null);
                if is_edit_tool(tool) {
                    x.streaks.clear();
                    x.streak_order.clear();
                    if let Some(c) = &mut x.cost {
                        c.usd0 = c.usd;
                        c.at0 = now_ms;
                        c.rise_at = now_ms;
                        c.fired = false;
                    }
                    if x.stuck
                        .as_ref()
                        .is_some_and(|s| s.kind == "repeat" || s.kind == "cost")
                    {
                        let was = x.stuck.take();
                        return Some(LoopSignal {
                            sig_type: "clear".to_owned(),
                            why: Some("edit".to_owned()),
                            stuck: was,
                            hint_lost: false,
                            hint: false,
                        });
                    }
                    return None;
                }
                let matches_cur = x
                    .streaks
                    .get(&who)
                    .is_some_and(|cur| cur.key == call_key(tool, input) || !is_look_tool(tool));
                if matches_cur {
                    x.streaks.remove(&who);
                    if x.stuck
                        .as_ref()
                        .is_some_and(|s| s.kind == "repeat" && s.agent.as_deref() == Some(who.as_str()))
                    {
                        let was = x.stuck.take();
                        return Some(LoopSignal {
                            sig_type: "clear".to_owned(),
                            why: Some("ok".to_owned()),
                            stuck: was,
                            hint_lost: false,
                            hint: false,
                        });
                    }
                }
                None
            }
            "PreCompact" => {
                if p.get("trigger").and_then(Value::as_str) == Some("manual") {
                    return None;
                }
                x.compacts.retain(|&a| now_ms.saturating_sub(a) < COMPACT_MS);
                x.compacts.push(now_ms);
                if x.compacts.len() < COMPACT_N || x.stuck.is_some() || !cfg.on {
                    return None;
                }
                let mins = ((now_ms.saturating_sub(x.compacts[0]) as f64) / 60_000.0)
                    .round()
                    .max(1.0) as u32;
                let stuck = StuckInfo {
                    kind: "compact".to_owned(),
                    at: now_ms,
                    agent: None,
                    label: None,
                    tool: None,
                    n: Some(x.compacts.len() as u32),
                    blocked: None,
                    usd: None,
                    mins: Some(mins),
                };
                x.stuck = Some(stuck.clone());
                Some(LoopSignal {
                    sig_type: "fire".to_owned(),
                    why: None,
                    stuck: Some(stuck),
                    hint_lost: false,
                    hint: false,
                })
            }
            _ => None,
        }
    }

    pub fn on_status(
        &mut self,
        sid: &str,
        total_cost_usd: f64,
        lines_added: Option<i64>,
        lines_removed: Option<i64>,
        working: bool,
        now_ms: u64,
    ) -> Option<LoopSignal> {
        if sid.is_empty() || !total_cost_usd.is_finite() {
            return None;
        }
        let cfg = self.config;
        let x = self.sess_mut(sid);
        let lines = format!(
            "{}/{}",
            lines_added.map(|v| v.to_string()).unwrap_or_default(),
            lines_removed.map(|v| v.to_string()).unwrap_or_default()
        );
        let c = match &mut x.cost {
            Some(c) if total_cost_usd >= c.usd => c,
            _ => {
                x.cost = Some(CostTrack {
                    usd0: total_cost_usd,
                    usd: total_cost_usd,
                    at0: now_ms,
                    rise_at: now_ms,
                    lines,
                    fired: false,
                });
                return None;
            }
        };
        if lines != c.lines {
            c.lines = lines;
            c.usd = total_cost_usd;
            c.usd0 = total_cost_usd;
            c.at0 = now_ms;
            c.rise_at = now_ms;
            c.fired = false;
            if x.stuck.as_ref().is_some_and(|s| s.kind == "cost") {
                return Some(LoopSignal {
                    sig_type: "clear".to_owned(),
                    why: Some("edit".to_owned()),
                    stuck: x.stuck.take(),
                    hint_lost: false,
                    hint: false,
                });
            }
            return None;
        }
        if total_cost_usd > c.usd {
            if now_ms.saturating_sub(c.rise_at) > QUIET_MS {
                c.usd0 = c.usd;
                c.at0 = now_ms;
                c.fired = false;
            }
            c.rise_at = now_ms;
            c.usd = total_cost_usd;
        }
        if !cfg.on || !working || c.fired || x.stuck.is_some() || x.mode == "plan" {
            return None;
        }
        let min_ms = (cfg.min as u64) * 60_000;
        if now_ms.saturating_sub(c.at0) < min_ms
            || (total_cost_usd - c.usd0) < COST_MIN_USD
            || now_ms.saturating_sub(c.rise_at) > QUIET_MS
        {
            return None;
        }
        c.fired = true;
        let spent = ((total_cost_usd - c.usd0) * 100.0).round() / 100.0;
        let mins = ((now_ms.saturating_sub(c.at0) as f64) / 60_000.0).round() as u32;
        let stuck = StuckInfo {
            kind: "cost".to_owned(),
            at: now_ms,
            agent: None,
            label: None,
            tool: None,
            n: None,
            blocked: None,
            usd: Some(spent),
            mins: Some(mins),
        };
        x.stuck = Some(stuck.clone());
        Some(LoopSignal {
            sig_type: "fire".to_owned(),
            why: None,
            stuck: Some(stuck),
            hint_lost: false,
            hint: false,
        })
    }

    pub fn check(&mut self, p: &Value, now_ms: u64) -> LoopCheckResult {
        let Some(sid) = p.get("session_id").and_then(Value::as_str) else {
            return LoopCheckResult {
                stop: false,
                deny: None,
                hint: None,
                sig: None,
            };
        };
        let cfg = self.config;
        let Some(x) = self.known.get_mut(sid) else {
            return LoopCheckResult {
                stop: false,
                deny: None,
                hint: None,
                sig: None,
            };
        };
        let who = p
            .get("agent_id")
            .and_then(Value::as_str)
            .filter(|s| !s.is_empty())
            .unwrap_or("main");
        if x.stop > 0 && now_ms.saturating_sub(x.stop) < STOP_MS {
            return LoopCheckResult {
                stop: true,
                deny: None,
                hint: None,
                sig: Some(LoopSignal {
                    sig_type: "stopped".to_owned(),
                    why: None,
                    stuck: None,
                    hint_lost: false,
                    hint: false,
                }),
            };
        }
        x.stop = 0;

        let tool = p.get("tool_name").and_then(Value::as_str).unwrap_or("");
        let input = p.get("tool_input").unwrap_or(&Value::Null);
        let mut deny = None;
        let mut sig = None;

        if cfg.on
            && let Some(cur) = x.streaks.get_mut(who)
            && cur.fails >= cfg.n
            && cur.key == call_key(tool, input)
        {
            cur.blocked += 1;
            deny = Some(DenyInfo {
                n: cur.fails,
                label: cur.label.clone(),
                blocked: cur.blocked,
            });
            if let Some(st) = &mut x.stuck
                && st.kind == "repeat"
                && st.agent.as_deref() == Some(who)
            {
                st.blocked = Some(cur.blocked);
                sig = Some(LoopSignal {
                    sig_type: "blocked".to_owned(),
                    why: None,
                    stuck: Some(st.clone()),
                    hint_lost: false,
                    hint: false,
                });
            } else {
                let stuck = StuckInfo {
                    kind: "repeat".to_owned(),
                    at: now_ms,
                    agent: Some(who.to_owned()),
                    label: Some(cur.label.clone()),
                    tool: Some(cur.tool.clone()),
                    n: Some(cur.fails),
                    blocked: Some(cur.blocked),
                    usd: None,
                    mins: None,
                };
                x.stuck = Some(stuck.clone());
                sig = Some(LoopSignal {
                    sig_type: "fire".to_owned(),
                    why: None,
                    stuck: Some(stuck),
                    hint_lost: false,
                    hint: false,
                });
            }
        }

        let target_agent = x
            .stuck
            .as_ref()
            .and_then(|s| s.agent.as_deref())
            .unwrap_or("main");
        let mut hint = None;
        if x.hint.is_some() && who == target_agent {
            hint = x.hint.take();
            match &mut sig {
                Some(s) => s.hint = true,
                None => {
                    sig = Some(LoopSignal {
                        sig_type: "hint".to_owned(),
                        why: None,
                        stuck: None,
                        hint_lost: false,
                        hint: true,
                    });
                }
            }
        }

        LoopCheckResult {
            stop: false,
            deny,
            hint,
            sig,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn loop_guard_denies_repeated_identical_failure_and_forgive_on_prompt() {
        let mut lg = LoopGuard::default();
        let fail = json!({
            "session_id": "s1",
            "hook_event_name": "PostToolUseFailure",
            "tool_name": "Bash",
            "tool_input": { "command": "npm test", "description": "run 1" },
            "error": "FAIL at 12:34:56 (142ms) pid 98765"
        });
        for _ in 0..3 {
            lg.on_event(&fail, 1000);
        }
        let check = lg.check(
            &json!({
                "session_id": "s1",
                "tool_name": "Bash",
                "tool_input": { "command": "npm   test", "description": "run 4" }
            }),
            2000,
        );
        assert!(check.deny.is_some());
        assert_eq!(check.deny.unwrap().blocked, 1);

        let cleared = lg.on_event(
            &json!({ "session_id": "s1", "hook_event_name": "UserPromptSubmit" }),
            3000,
        );
        assert_eq!(cleared.unwrap().sig_type, "clear");
        let after = lg.check(
            &json!({ "session_id": "s1", "tool_name": "Bash", "tool_input": { "command": "npm test" } }),
            4000,
        );
        assert!(after.deny.is_none());
    }
}
