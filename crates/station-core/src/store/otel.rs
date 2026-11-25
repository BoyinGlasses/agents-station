use crate::atomic::write_json_atomic;
use crate::shared::burn::{BurnSummary, DIMS, DayRollup, SessionMeta, SumTuple, add_to, summarize, zero};
use base64::Engine;
use base64::engine::general_purpose::URL_SAFE_NO_PAD;
use chrono::{Local, TimeZone};
use serde::{Deserialize, Serialize};
use serde_json::{Map, Value};
use sha2::{Digest, Sha256};
use std::collections::{HashMap, HashSet, VecDeque};
use std::path::PathBuf;

pub const COST_METRIC: &str = "claude_code.cost.usage";
pub const TOKENS_METRIC: &str = "claude_code.token.usage";

pub const MAX_DAY_SESSIONS: usize = 500;
pub const MAX_SUMS: usize = 150;
pub const MAX_SESSIONS: usize = 3000;
pub const MAX_SERIES: usize = 20_000;
pub const MAX_SEEN: usize = 5000;
const DAY_MS: u64 = 86_400_000;

fn is_read_attr(k: &str) -> bool {
    matches!(
        k,
        "session.id"
            | "type"
            | "event.name"
            | "event.timestamp"
            | "event.sequence"
            | "cost_usd"
            | "cost_usd_micros"
            | "input_tokens"
            | "output_tokens"
            | "cache_read_tokens"
            | "cache_creation_tokens"
            | "agent_id"
            | "model"
            | "query_source"
            | "agent.name"
            | "skill.name"
            | "plugin.name"
            | "mcp_server.name"
    )
}

fn clean_attr(s: &str) -> String {
    s.chars()
        .map(|c| if matches!(c, '\t' | '\n' | '\r') || (c as u32) < 0x20 { ' ' } else { c })
        .collect::<String>()
        .trim()
        .chars()
        .take(80)
        .collect()
}

pub fn day_key(ms: u64) -> String {
    Local
        .timestamp_millis_opt(ms as i64)
        .single()
        .map(|dt| dt.format("%Y-%m-%d").to_string())
        .unwrap_or_else(|| "1970-01-01".to_owned())
}

fn any_value(v: &Value) -> Option<Value> {
    let obj = v.as_object()?;
    if let Some(s) = obj.get("stringValue") {
        return Some(Value::String(s.as_str().unwrap_or("").to_owned()));
    }
    if let Some(i) = obj.get("intValue") {
        let n = i
            .as_f64()
            .or_else(|| i.as_str().and_then(|s| s.parse::<f64>().ok()))?;
        return serde_json::Number::from_f64(n).map(Value::Number);
    }
    if let Some(d) = obj.get("doubleValue") {
        let n = d.as_f64()?;
        return serde_json::Number::from_f64(n).map(Value::Number);
    }
    if let Some(b) = obj.get("boolValue") {
        return Some(Value::Bool(b.as_bool().unwrap_or(false)));
    }
    None
}

fn pick_attrs(lists: &[Option<&Vec<Value>>]) -> Map<String, Value> {
    let mut out = Map::new();
    for list in lists.iter().flatten() {
        for a in *list {
            let Some(k) = a.get("key").and_then(Value::as_str) else {
                continue;
            };
            if !is_read_attr(k) {
                continue;
            }
            if let Some(v) = a.get("value").and_then(any_value) {
                let norm = match v {
                    Value::String(s) => Value::String(clean_attr(&s)),
                    other => other,
                };
                out.insert(k.to_owned(), norm);
            }
        }
    }
    out
}

fn series_hash(name: &str, res: Option<&Vec<Value>>, attrs: Option<&Vec<Value>>) -> String {
    let flat = |list: Option<&Vec<Value>>| -> String {
        let mut items: Vec<String> = list
            .into_iter()
            .flatten()
            .map(|a| {
                let k = a.get("key").and_then(Value::as_str).unwrap_or("");
                let v = a.get("value").cloned().unwrap_or(Value::Null);
                format!("{k}={v}")
            })
            .collect();
        items.sort();
        items.join("\u{0001}")
    };
    let raw = format!("{name}\u{0002}{}\u{0002}{}", flat(res), flat(attrs));
    let digest = Sha256::digest(raw.as_bytes());
    URL_SAFE_NO_PAD.encode(digest).chars().take(18).collect()
}

pub fn nanos_to_ms(v: Option<&Value>) -> u64 {
    let Some(val) = v else {
        return 0;
    };
    if let Some(s) = val.as_str() {
        if s.bytes().all(|b| b.is_ascii_digit()) && s.len() > 6 {
            return s[..s.len() - 6].parse::<u64>().unwrap_or(0);
        }
        return 0;
    }
    if let Some(n) = val.as_f64()
        && n.is_finite()
        && n > 0.0
    {
        return (n / 1e6).floor() as u64;
    }
    if let Some(obj) = val.as_object()
        && let (Some(lo), Some(hi)) = (obj.get("low"), obj.get("high"))
    {
        let l = lo.as_u64().unwrap_or(0) as u128;
        let h = hi.as_u64().unwrap_or(0) as u128;
        return (((h << 32) + l) / 1_000_000) as u64;
    }
    0
}

fn time_id(v: Option<&Value>) -> String {
    match v {
        Some(Value::Object(o)) => format!(
            "{}:{}",
            o.get("high").and_then(Value::as_u64).unwrap_or(0),
            o.get("low").and_then(Value::as_u64).unwrap_or(0)
        ),
        Some(Value::String(s)) => s.clone(),
        Some(other) => other.to_string(),
        None => String::new(),
    }
}

fn temporality_of(v: Option<&Value>) -> &'static str {
    match v {
        Some(Value::Number(n)) if n.as_u64() == Some(2) => "cumulative",
        Some(Value::String(s)) if s == "AGGREGATION_TEMPORALITY_CUMULATIVE" => "cumulative",
        _ => "delta",
    }
}

#[derive(Debug, Clone)]
pub struct ParsedMetricPoint {
    pub name: String,
    pub temporality: &'static str,
    pub start: String,
    pub end: u64,
    pub end_id: String,
    pub value: f64,
    pub attrs: Map<String, Value>,
    pub series: String,
}

#[derive(Debug, Clone)]
pub struct ParsedLogEvent {
    pub name: String,
    pub at: u64,
    pub attrs: Map<String, Value>,
    pub id: String,
}

pub fn parse_metrics(body: &Value) -> Vec<ParsedMetricPoint> {
    let mut out = Vec::new();
    let Some(rms) = body.get("resourceMetrics").and_then(Value::as_array) else {
        return out;
    };
    for rm in rms {
        let res = rm
            .get("resource")
            .and_then(|r| r.get("attributes"))
            .and_then(Value::as_array);
        let mut sms = Vec::new();
        if let Some(a) = rm.get("scopeMetrics").and_then(Value::as_array) {
            sms.extend(a.iter());
        }
        if let Some(a) = rm.get("instrumentationLibraryMetrics").and_then(Value::as_array) {
            sms.extend(a.iter());
        }
        for sm in sms {
            let Some(metrics) = sm.get("metrics").and_then(Value::as_array) else {
                continue;
            };
            for m in metrics {
                let name = m.get("name").and_then(Value::as_str).unwrap_or("");
                if name != COST_METRIC && name != TOKENS_METRIC {
                    continue;
                }
                let Some(sum) = m.get("sum") else {
                    continue;
                };
                let temp = temporality_of(sum.get("aggregationTemporality"));
                let Some(dps) = sum.get("dataPoints").and_then(Value::as_array) else {
                    continue;
                };
                for p in dps {
                    let val = p
                        .get("asDouble")
                        .and_then(Value::as_f64)
                        .or_else(|| {
                            p.get("asInt").and_then(|v| {
                                v.as_f64()
                                    .or_else(|| v.as_str().and_then(|s| s.parse::<f64>().ok()))
                            })
                        });
                    let Some(value) = val.filter(|v| v.is_finite() && *v >= 0.0) else {
                        continue;
                    };
                    let p_attrs = p.get("attributes").and_then(Value::as_array);
                    out.push(ParsedMetricPoint {
                        name: name.to_owned(),
                        temporality: temp,
                        start: time_id(p.get("startTimeUnixNano")),
                        end: nanos_to_ms(p.get("timeUnixNano")),
                        end_id: time_id(p.get("timeUnixNano")),
                        value,
                        attrs: pick_attrs(&[res, p_attrs]),
                        series: series_hash(name, res, p_attrs),
                    });
                }
            }
        }
    }
    out
}

pub fn parse_logs(body: &Value) -> Vec<ParsedLogEvent> {
    let mut out = Vec::new();
    let Some(rls) = body.get("resourceLogs").and_then(Value::as_array) else {
        return out;
    };
    for rl in rls {
        let res = rl
            .get("resource")
            .and_then(|r| r.get("attributes"))
            .and_then(Value::as_array);
        let mut sls = Vec::new();
        if let Some(a) = rl.get("scopeLogs").and_then(Value::as_array) {
            sls.extend(a.iter());
        }
        if let Some(a) = rl.get("instrumentationLibraryLogs").and_then(Value::as_array) {
            sls.extend(a.iter());
        }
        for sl in sls {
            let Some(recs) = sl.get("logRecords").and_then(Value::as_array) else {
                continue;
            };
            for r in recs {
                let r_attrs = r.get("attributes").and_then(Value::as_array);
                let attrs = pick_attrs(&[res, r_attrs]);
                let raw_name = attrs
                    .get("event.name")
                    .and_then(Value::as_str)
                    .or_else(|| {
                        r.get("body")
                            .and_then(|b| b.get("stringValue"))
                            .and_then(Value::as_str)
                    })
                    .unwrap_or("");
                let name = raw_name.strip_prefix("claude_code.").unwrap_or(raw_name);
                if !matches!(name, "api_request" | "api_error" | "user_prompt") {
                    continue;
                }
                let at = attrs
                    .get("event.timestamp")
                    .and_then(Value::as_str)
                    .and_then(|s| chrono::DateTime::parse_from_rfc3339(s).ok())
                    .map(|dt| dt.timestamp_millis().max(0) as u64)
                    .or_else(|| {
                        let m = nanos_to_ms(r.get("timeUnixNano"));
                        if m > 0 { Some(m) } else { None }
                    })
                    .unwrap_or_else(|| nanos_to_ms(r.get("observedTimeUnixNano")));
                let sid = attrs.get("session.id").and_then(Value::as_str).unwrap_or("");
                let seq = attrs
                    .get("event.sequence")
                    .map(|v| match v {
                        Value::String(s) => s.clone(),
                        other => other.to_string(),
                    })
                    .unwrap_or_default();
                let ts_key = attrs
                    .get("event.timestamp")
                    .and_then(Value::as_str)
                    .map(str::to_owned)
                    .unwrap_or_else(|| time_id(r.get("timeUnixNano")));
                let id = format!("{sid}|{seq}|{ts_key}|{name}");
                let ev_name = name.to_owned();
                out.push(ParsedLogEvent {
                    name: ev_name,
                    at,
                    attrs,
                    id,
                });
            }
        }
    }
    out
}

pub fn source_of(attrs: &Map<String, Value>, from_event: bool) -> String {
    let q = attrs.get("query_source").and_then(Value::as_str).unwrap_or("");
    if !from_event || matches!(q, "main" | "subagent" | "auxiliary") {
        return if q.is_empty() { "main".to_owned() } else { q.to_owned() };
    }
    if attrs.contains_key("agent.name") || attrs.contains_key("agent_id") {
        return "subagent".to_owned();
    }
    if q.contains("main_thread") || q.starts_with("sdk") || q.starts_with("main") {
        "main".to_owned()
    } else {
        "auxiliary".to_owned()
    }
}

fn key_of(attrs: &Map<String, Value>, from_event: bool) -> String {
    let attr_for = |d: &str| match d {
        "model" => "model",
        "source" => "query_source",
        "agent" => "agent.name",
        "skill" => "skill.name",
        "plugin" => "plugin.name",
        "mcp" => "mcp_server.name",
        _ => "",
    };
    DIMS.iter()
        .map(|&d| {
            if d == "source" {
                source_of(attrs, from_event)
            } else {
                attrs
                    .get(attr_for(d))
                    .and_then(Value::as_str)
                    .map(clean_attr)
                    .unwrap_or_default()
            }
        })
        .collect::<Vec<_>>()
        .join("\t")
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct CumPoint {
    s: String,
    v: f64,
    t: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct OtelStateFile {
    v: u32,
    sess: HashMap<String, SessionMeta>,
    cum: HashMap<String, CumPoint>,
    #[serde(rename = "anonAt")]
    anon_at: u64,
    #[serde(rename = "lastM")]
    last_m: u64,
    #[serde(rename = "lastL")]
    last_l: u64,
}

impl Default for OtelStateFile {
    fn default() -> Self {
        Self {
            v: 1,
            sess: HashMap::new(),
            cum: HashMap::new(),
            anon_at: 0,
            last_m: 0,
            last_l: 0,
        }
    }
}

pub struct OtelStore {
    dir: PathBuf,
    keep_days: u64,
    days: HashMap<String, DayRollup>,
    dirty_days: HashSet<String>,
    st: OtelStateFile,
    dirty: bool,
    seen_delta: HashMap<String, VecDeque<String>>,
    seen_delta_order: VecDeque<String>,
    seen_events: HashMap<String, u64>,
    seen_events_order: VecDeque<String>,
}

impl OtelStore {
    pub fn load(dir: PathBuf, keep_days: u64, now_ms: u64) -> Self {
        let state_file = dir.join("state.json");
        let st = std::fs::read_to_string(&state_file)
            .ok()
            .and_then(|s| serde_json::from_str::<OtelStateFile>(&s).ok())
            .filter(|j| j.v == 1)
            .unwrap_or_default();
        let oldest = day_key(now_ms.saturating_sub(keep_days * DAY_MS));
        let mut days = HashMap::new();
        if let Ok(entries) = std::fs::read_dir(&dir) {
            for e in entries.flatten() {
                let name = e.file_name().to_string_lossy().into_owned();
                if let Some(stem) = name.strip_suffix(".json")
                    && stem.len() == 10
                    && stem.as_bytes()[4] == b'-'
                    && stem.as_bytes()[7] == b'-'
                    && stem >= oldest.as_str()
                    && let Ok(text) = std::fs::read_to_string(e.path())
                    && let Ok(dr) = serde_json::from_str::<DayRollup>(&text)
                {
                    days.insert(stem.to_owned(), dr);
                }
            }
        }
        Self {
            dir,
            keep_days,
            days,
            dirty_days: HashSet::new(),
            st,
            dirty: false,
            seen_delta: HashMap::new(),
            seen_delta_order: VecDeque::new(),
            seen_events: HashMap::new(),
            seen_events_order: VecDeque::new(),
        }
    }

    fn touch_session(&mut self, sid: &str, at: u64) -> &mut SessionMeta {
        if !self.st.sess.contains_key(sid) {
            if self.st.sess.len() >= MAX_SESSIONS
                && let Some(oldest_k) = self
                    .st
                    .sess
                    .iter()
                    .min_by_key(|(_, v)| v.l)
                    .map(|(k, _)| k.clone())
            {
                self.st.sess.remove(&oldest_k);
            }
            self.st.sess.insert(
                sid.to_owned(),
                SessionMeta {
                    m: false,
                    cwd: String::new(),
                    l: at,
                },
            );
        }
        let s = self.st.sess.get_mut(sid).unwrap();
        if at > s.l {
            s.l = at;
        }
        self.dirty = true;
        s
    }

    fn add_sum(led: &mut HashMap<String, SumTuple>, key: String, v: &SumTuple) {
        let final_k = if !led.contains_key(&key) && led.len() >= MAX_SUMS {
            let parts: Vec<&str> = key.split('\t').collect();
            format!(
                "{}\t{}\t\t\t\t",
                parts.first().copied().unwrap_or(""),
                parts.get(1).copied().unwrap_or("")
            )
        } else {
            key
        };
        let entry = led.entry(final_k).or_insert_with(zero);
        add_to(entry, v);
    }

    fn ingest_metric_point(&mut self, p: ParsedMetricPoint, now_ms: u64) -> usize {
        let mut inc = p.value;
        if p.temporality == "cumulative" {
            if let Some(c) = self.st.cum.get(&p.series)
                && c.s == p.start
                && p.value >= c.v
            {
                inc = p.value - c.v;
            }
            self.st.cum.insert(
                p.series.clone(),
                CumPoint {
                    s: p.start.clone(),
                    v: p.value,
                    t: now_ms,
                },
            );
        } else {
            let w = format!("{}-{}", p.start, p.end_id);
            let q = self.seen_delta.entry(p.series.clone()).or_default();
            if q.iter().any(|x| x == &w) {
                return 0;
            }
            q.push_back(w);
            if q.len() > 16 {
                q.pop_front();
            }
            self.seen_delta_order.retain(|s| s != &p.series);
            self.seen_delta_order.push_back(p.series.clone());
            while self.seen_delta.len() > MAX_SEEN {
                if let Some(old) = self.seen_delta_order.pop_front() {
                    self.seen_delta.remove(&old);
                }
            }
        }
        if !(inc > 0.0) {
            return 0;
        }
        let sid = p
            .attrs
            .get("session.id")
            .and_then(Value::as_str)
            .filter(|s| !s.is_empty())
            .unwrap_or("-")
            .to_owned();
        let at = if p.end > 0 { p.end } else { now_ms };
        let mut v = zero();
        if p.name == COST_METRIC {
            v[0] = inc;
        } else {
            let slot = match p.attrs.get("type").and_then(Value::as_str).unwrap_or("") {
                "input" => 1,
                "output" => 2,
                "cacheRead" => 3,
                "cacheCreation" => 4,
                _ => return 0,
            };
            v[slot] = inc;
        }
        if sid == "-" {
            self.st.anon_at = now_ms;
        }
        self.touch_session(&sid, at).m = true;
        let dk = day_key(at);
        self.dirty_days.insert(dk.clone());
        let overflow = self
            .days
            .get(&dk)
            .is_some_and(|d| !d.s.contains_key(&sid) && d.s.len() >= MAX_DAY_SESSIONS);
        let eff_sid = if overflow { "~".to_owned() } else { sid };
        if eff_sid == "~" {
            self.touch_session("~", at).m = true;
        }
        let day = self.days.entry(dk).or_default();
        let rec = day.s.entry(eff_sid).or_default();
        Self::add_sum(&mut rec.m, key_of(&p.attrs, false), &v);
        1
    }

    fn ingest_log_event(&mut self, e: ParsedLogEvent, now_ms: u64) -> usize {
        if self.seen_events.contains_key(&e.id) {
            return 0;
        }
        self.seen_events.insert(e.id.clone(), now_ms);
        self.seen_events_order.push_back(e.id.clone());
        while self.seen_events.len() > MAX_SEEN {
            if let Some(old) = self.seen_events_order.pop_front() {
                self.seen_events.remove(&old);
            }
        }
        let sid = e
            .attrs
            .get("session.id")
            .and_then(Value::as_str)
            .filter(|s| !s.is_empty())
            .unwrap_or("-")
            .to_owned();
        let at = if e.at > 0 { e.at } else { now_ms };
        self.touch_session(&sid, at);
        let dk = day_key(at);
        self.dirty_days.insert(dk.clone());
        let overflow = self
            .days
            .get(&dk)
            .is_some_and(|d| !d.s.contains_key(&sid) && d.s.len() >= MAX_DAY_SESSIONS);
        let eff_sid = if overflow { "~".to_owned() } else { sid };
        if eff_sid == "~" {
            self.touch_session("~", at);
        }
        let day = self.days.entry(dk).or_default();
        let rec = day.s.entry(eff_sid).or_default();
        if e.name == "api_error" {
            rec.err += 1;
            return 1;
        }
        if e.name == "user_prompt" {
            rec.pr += 1;
            return 1;
        }
        let num = |k: &str| -> f64 {
            e.attrs
                .get(k)
                .and_then(|v| v.as_f64().or_else(|| v.as_str().and_then(|s| s.parse().ok())))
                .filter(|x| x.is_finite() && *x > 0.0)
                .unwrap_or(0.0)
        };
        let usd = if e.attrs.contains_key("cost_usd") {
            num("cost_usd")
        } else {
            num("cost_usd_micros") / 1e6
        };
        rec.req += 1;
        let v = [
            usd,
            num("input_tokens"),
            num("output_tokens"),
            num("cache_read_tokens"),
            num("cache_creation_tokens"),
            1.0,
        ];
        Self::add_sum(&mut rec.e, key_of(&e.attrs, true), &v);
        1
    }

    pub fn ingest_metrics(&mut self, body: &Value, now_ms: u64) -> usize {
        let mut accepted = 0;
        for p in parse_metrics(body) {
            accepted += self.ingest_metric_point(p, now_ms);
        }
        if body.get("resourceMetrics").is_some() {
            self.st.last_m = now_ms;
            self.dirty = true;
        }
        accepted
    }

    pub fn ingest_logs(&mut self, body: &Value, now_ms: u64) -> usize {
        let mut accepted = 0;
        for e in parse_logs(body) {
            accepted += self.ingest_log_event(e, now_ms);
        }
        if body.get("resourceLogs").is_some() {
            self.st.last_l = now_ms;
            self.dirty = true;
        }
        accepted
    }

    pub fn is_dirty(&self) -> bool {
        self.dirty || !self.dirty_days.is_empty()
    }

    pub fn burn_summary(&self, range: &str, now_ms: u64) -> BurnSummary {
        let n = match range {
            "30d" => 30usize,
            "7d" => 7usize,
            _ => 1usize,
        };
        let keys: Vec<String> = if let Some(today_dt) = Local.timestamp_millis_opt(now_ms as i64).single() {
            let today = today_dt.date_naive();
            (0..n)
                .map(|i| {
                    let delta = (n - 1 - i) as i64;
                    (today - chrono::Duration::days(delta))
                        .format("%Y-%m-%d")
                        .to_string()
                })
                .collect()
        } else {
            let mut ks = Vec::with_capacity(n);
            for i in 0..n {
                let dk = day_key(now_ms.saturating_sub(((n - 1 - i) as u64) * DAY_MS));
                if !ks.contains(&dk) {
                    ks.push(dk);
                }
            }
            ks
        };
        let pairs: Vec<(String, DayRollup)> = keys
            .into_iter()
            .map(|k| {
                let d = self.days.get(&k).cloned().unwrap_or_default();
                (k, d)
            })
            .collect();
        summarize(&pairs, &self.st.sess, self.st.anon_at, now_ms)
    }

    pub fn flush(&mut self, now_ms: u64) -> anyhow::Result<()> {
        let oldest = day_key(now_ms.saturating_sub(self.keep_days * DAY_MS));
        let drop_days: Vec<String> = self
            .days
            .keys()
            .filter(|k| k.as_str() < oldest.as_str())
            .cloned()
            .collect();
        for k in drop_days {
            self.days.remove(&k);
            self.dirty_days.remove(&k);
            let _ = std::fs::remove_file(self.dir.join(format!("{k}.json")));
        }
        self.st
            .cum
            .retain(|_, c| now_ms.saturating_sub(c.t) <= 10 * DAY_MS);
        if self.st.cum.len() > MAX_SERIES {
            let mut entries: Vec<(String, u64)> =
                self.st.cum.iter().map(|(k, v)| (k.clone(), v.t)).collect();
            entries.sort_by_key(|(_, t)| *t);
            for (k, _) in entries.into_iter().take(self.st.cum.len() - MAX_SERIES) {
                self.st.cum.remove(&k);
            }
        }
        let dirty_keys: Vec<String> = self.dirty_days.iter().cloned().collect();
        for k in dirty_keys {
            if let Some(d) = self.days.get(&k) {
                write_json_atomic(&self.dir.join(format!("{k}.json")), d)?;
            }
            self.dirty_days.remove(&k);
        }
        if self.dirty {
            write_json_atomic(&self.dir.join("state.json"), &self.st)?;
            self.dirty = false;
        }
        Ok(())
    }

    pub fn info(&self) -> Value {
        serde_json::json!({
            "lastMetricsAt": self.st.last_m,
            "lastLogsAt": self.st.last_l,
            "sessions": self.st.sess.len(),
            "days": self.days.len(),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::Path;

    #[test]
    fn ingests_oracle_otel_fixtures_and_never_persists_secrets_or_prompts() {
        let fixtures = Path::new("D:/mystation/my-station/tests/fixtures/otel");
        if !fixtures.exists() {
            return;
        }
        let dir = tempfile::tempdir().unwrap();
        let now = 1_793_091_600_000u64;
        let mut store = OtelStore::load(dir.path().join("otel"), 30, now);

        for name in [
            "metrics-cumulative-1.json",
            "metrics-cumulative-2.json",
            "metrics-cumulative-3.json",
            "metrics-delta-1.json",
            "metrics-delta-2.json",
        ] {
            let text = std::fs::read_to_string(fixtures.join(name)).unwrap();
            let v: Value = serde_json::from_str(&text).unwrap();
            store.ingest_metrics(&v, now);
        }
        let logs: Value =
            serde_json::from_str(&std::fs::read_to_string(fixtures.join("logs.json")).unwrap())
                .unwrap();
        store.ingest_logs(&logs, now);
        store.flush(now).unwrap();

        assert!(store.days.len() >= 1);
        assert!(store.st.sess.len() >= 1);
        // Verify zero raw prompts or email addresses written to otel dir
        for entry in std::fs::read_dir(dir.path().join("otel")).unwrap().flatten() {
            let body = std::fs::read_to_string(entry.path()).unwrap();
            assert!(!body.contains("sk-"), "secret leaked to otel disk");
            assert!(!body.contains("user.email"), "email attr leaked to otel disk");
        }
    }

    #[test]
    fn burn_summary_30d_returns_30_days_and_includes_older_usage() {
        let dir = tempfile::tempdir().unwrap();
        let now = 1_793_091_600_000u64;
        let mut store = OtelStore::load(dir.path().join("otel"), 30, now);

        let twenty_days_ago_ns = (now - 20 * DAY_MS) * 1_000_000;
        let metrics = serde_json::json!({
            "resourceMetrics": [{
                "scopeMetrics": [{
                    "metrics": [{
                        "name": COST_METRIC,
                        "sum": {
                            "aggregationTemporality": 1,
                            "dataPoints": [{
                                "asDouble": 1.25,
                                "startTimeUnixNano": (twenty_days_ago_ns - 1_000_000).to_string(),
                                "timeUnixNano": twenty_days_ago_ns.to_string(),
                                "attributes": [
                                    { "key": "session.id", "value": { "stringValue": "s-old" } },
                                    { "key": "model", "value": { "stringValue": "claude-sonnet-4" } }
                                ]
                            }]
                        }
                    }]
                }]
            }]
        });
        assert_eq!(store.ingest_metrics(&metrics, now), 1);

        let sum_today = store.burn_summary("today", now);
        assert_eq!(sum_today.per_day.len(), 1);
        assert_eq!(sum_today.total.row.usd, 0.0);

        let sum_7d = store.burn_summary("7d", now);
        assert_eq!(sum_7d.per_day.len(), 7);
        assert_eq!(sum_7d.total.row.usd, 0.0);

        let sum_30d = store.burn_summary("30d", now);
        assert_eq!(sum_30d.per_day.len(), 30);
        assert!((sum_30d.total.row.usd - 1.25).abs() < 1e-6);
    }

    #[test]
    fn flush_retains_dirty_flags_on_write_failure_and_recovers_on_retry() {
        let dir = tempfile::tempdir().unwrap();
        let otel_dir = dir.path().join("otel");
        let now = 1_793_091_600_000u64;
        let today = day_key(now);
        let mut store = OtelStore::load(otel_dir.clone(), 30, now);

        let make_metric = |val: f64, seq: u64| {
            let t_ns = (now + seq * 1000) * 1_000_000;
            serde_json::json!({
                "resourceMetrics": [{
                    "scopeMetrics": [{
                        "metrics": [{
                            "name": COST_METRIC,
                            "sum": {
                                "aggregationTemporality": 1,
                                "dataPoints": [{
                                    "asDouble": val,
                                    "startTimeUnixNano": (t_ns - 1_000_000).to_string(),
                                    "timeUnixNano": t_ns.to_string(),
                                    "attributes": [
                                        { "key": "session.id", "value": { "stringValue": "s-retry" } },
                                        { "key": "model", "value": { "stringValue": "claude-sonnet-4" } }
                                    ]
                                }]
                            }
                        }]
                    }]
                }]
            })
        };

        // 1. Failing day-file write must keep dirty_days + is_dirty() true.
        assert_eq!(store.ingest_metrics(&make_metric(1.0, 1), now), 1);
        let day_blocker = otel_dir.join(format!("{today}.json"));
        std::fs::create_dir_all(&day_blocker).unwrap();
        assert!(store.flush(now).is_err());
        assert!(store.is_dirty(), "is_dirty must remain true after failed day flush");
        assert!(
            store.dirty_days.contains(&today),
            "dirty_days must retain failed day key for retry"
        );
        std::fs::remove_dir(&day_blocker).unwrap();
        store.flush(now).unwrap();
        assert!(!store.is_dirty());

        // 2. Failing state.json write must keep self.dirty + is_dirty() true.
        assert_eq!(store.ingest_metrics(&make_metric(2.0, 2), now), 1);
        let state_path = otel_dir.join("state.json");
        std::fs::remove_file(&state_path).unwrap();
        std::fs::create_dir_all(&state_path).unwrap();
        assert!(store.flush(now).is_err());
        assert!(store.is_dirty(), "is_dirty must remain true after failed state.json flush");
        assert!(store.dirty, "dirty flag must remain true after failed state.json flush");
        std::fs::remove_dir(&state_path).unwrap();
        store.flush(now).unwrap();
        assert!(!store.is_dirty());

        let reloaded = OtelStore::load(otel_dir, 30, now);
        assert!((reloaded.burn_summary("today", now).total.row.usd - 3.0).abs() < 1e-6);
    }
}
