use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::{HashMap, HashSet};

pub const KINDS: [&str; 9] = [
    "think", "read", "edit", "shell", "web", "agent", "wait", "turn", "idle",
];
pub const TURN_CAP_MS: u64 = 30 * 60 * 1000;

fn kind_of_type(t: &str) -> &'static str {
    match t {
        "prompt" | "plan" | "other" | "compact" => "think",
        "skill" | "read" | "grep" | "glob" => "read",
        "edit" | "write" => "edit",
        "bash" => "shell",
        "web" | "fetch" | "mcp" => "web",
        "agent" => "agent",
        "wait" | "ask" => "wait",
        "stop" | "idle" | "interrupt" => "turn",
        _ => "think",
    }
}

fn is_tool_kind(k: &str) -> bool {
    matches!(k, "read" | "edit" | "shell" | "web" | "agent")
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ReplayItem {
    pub t: u64,
    pub m: Value,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct TimelineTotals {
    pub work: u64,
    pub wait: u64,
    pub turn: u64,
    pub prompts: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TimelineRow {
    pub key: String,
    pub sess: String,
    pub agent: Option<String>,
    pub name: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub title: Option<String>,
    pub desk: i32,
    pub spans: Vec<(u64, u64, String, String)>,
    pub marks: Vec<(u64, String, String)>,
    pub totals: TimelineTotals,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TimelineResult {
    pub from: u64,
    pub to: u64,
    pub rows: Vec<TimelineRow>,
    pub totals: TimelineTotals,
}

struct RowAcc {
    key: String,
    sess: String,
    agent: Option<String>,
    name: String,
    title: Option<String>,
    desk: i32,
    spans: Vec<(u64, u64, String, String)>,
    marks: Vec<(u64, String, String)>,
    cur: Option<(String, u64, String)>,
}

impl RowAcc {
    fn close(&mut self, t: u64, from: u64, to: u64) {
        let Some((kind, start, label)) = self.cur.take() else {
            return;
        };
        let s = start.max(from);
        let e = t.min(to);
        if e > s {
            if let Some(last) = self.spans.last_mut()
                && last.2 == kind
                && last.1 == s
                && last.3 == label
            {
                last.1 = e;
                return;
            }
            self.spans.push((s, e, kind, label));
        }
    }

    fn open(&mut self, t: u64, kind: &str, label: &str, from: u64, to: u64) {
        if let Some((ck, _, cl)) = &self.cur
            && ck == kind
            && cl == label
        {
            return;
        }
        self.close(t, from, to);
        self.cur = Some((kind.to_owned(), t, label.to_owned()));
    }

    fn mark(&mut self, t: u64, kind: &str, label: &str, from: u64, to: u64) {
        if t >= from && t <= to {
            self.marks.push((t, kind.to_owned(), label.to_owned()));
        }
    }
}

pub fn build_timeline(items: &[ReplayItem], from: u64, to: u64, now_ms: u64) -> TimelineResult {
    let mut rows: HashMap<String, RowAcc> = HashMap::new();
    let mut name_of: HashMap<String, String> = HashMap::new();

    let key_of = |sess: &str, names: &HashMap<String, String>| -> String {
        names.get(sess).cloned().unwrap_or_else(|| sess.to_owned())
    };

    let get_row = |rows: &mut HashMap<String, RowAcc>,
                   names: &mut HashMap<String, String>,
                   sess: &str,
                   agent: Option<&str>,
                   name: Option<&str>|
     -> String {
        if let Some(nm) = name
            && !nm.is_empty()
            && agent.is_none()
        {
            names.insert(sess.to_owned(), nm.to_owned());
        }
        let base_k = key_of(sess, names);
        let key = match agent {
            Some(a) => format!("{base_k}|{a}"),
            None => base_k,
        };
        if let Some(r) = rows.get_mut(&key) {
            r.sess = sess.to_owned();
            if let Some(nm) = name
                && !nm.is_empty()
                && agent.is_none()
            {
                r.name = nm.to_owned();
            }
        } else {
            let resolved_name = match (name, agent) {
                (Some(nm), _) if !nm.is_empty() => nm.to_owned(),
                (_, Some(_)) => String::new(),
                _ => "claude".to_owned(),
            };
            rows.insert(
                key.clone(),
                RowAcc {
                    key: key.clone(),
                    sess: sess.to_owned(),
                    agent: agent.map(str::to_owned),
                    name: resolved_name,
                    title: None,
                    desk: -1,
                    spans: Vec::new(),
                    marks: Vec::new(),
                    cur: None,
                },
            );
        }
        key
    };

    for item in items {
        let t = item.t;
        if t > to {
            continue;
        }
        let Some(obj) = item.m.as_object() else {
            continue;
        };
        let k = obj.get("k").and_then(Value::as_str).unwrap_or("");
        match k {
            "snap" => {
                let mut ids = HashSet::new();
                if let Some(sess_arr) = obj.get("sessions").and_then(Value::as_array) {
                    for s in sess_arr {
                        let sid = s.get("id").and_then(Value::as_str).unwrap_or("");
                        let sname = s.get("name").and_then(Value::as_str);
                        ids.insert(sid.to_owned());
                        let rk = get_row(&mut rows, &mut name_of, sid, None, sname);
                        ids.insert(rk.clone());
                        if let Some(r) = rows.get_mut(&rk) {
                            if let Some(desk) = s.get("desk").and_then(Value::as_i64) {
                                r.desk = desk as i32;
                            }
                            if let Some(title) = s.get("title").and_then(Value::as_str)
                                && !title.is_empty()
                            {
                                r.title = Some(title.to_owned());
                            }
                            if r.cur.is_none() {
                                let status = s.get("status").and_then(Value::as_str).unwrap_or("");
                                let init_k = if status == "work" {
                                    "think"
                                } else if status == "wait" {
                                    "wait"
                                } else if s.get("lastPrompt").is_some_and(|v| !v.is_null()) {
                                    "turn"
                                } else {
                                    "idle"
                                };
                                r.open(t, init_k, "", from, to);
                            }
                        }
                        if let Some(subs) = s.get("subs").and_then(Value::as_array) {
                            for sub in subs {
                                let sub_id = sub.get("id").and_then(Value::as_str).unwrap_or("");
                                let sub_type = sub.get("type").and_then(Value::as_str).unwrap_or("");
                                let sub_desc = sub.get("desc").and_then(Value::as_str).unwrap_or("");
                                let sub_name = if sub_desc.is_empty() {
                                    sub_type.to_owned()
                                } else {
                                    format!("{sub_type} · {sub_desc}")
                                };
                                let srk = get_row(&mut rows, &mut name_of, sid, Some(sub_id), Some(&sub_name));
                                if let Some(sr) = rows.get_mut(&srk)
                                    && sr.cur.is_none()
                                {
                                    let lt = sub
                                        .get("last")
                                        .and_then(|l| l.get("type"))
                                        .and_then(Value::as_str)
                                        .map(kind_of_type)
                                        .unwrap_or("think");
                                    sr.open(t, lt, "", from, to);
                                }
                            }
                        }
                    }
                }
                for r in rows.values_mut() {
                    if r.cur.is_some() {
                        let parent_k = if r.agent.is_some() {
                            r.key.rsplit_once('|').map(|(p, _)| p).unwrap_or(&r.key)
                        } else {
                            &r.key
                        };
                        if !ids.contains(parent_k) {
                            r.close(t, from, to);
                        }
                    }
                }
            }
            "ev" => {
                let Some(e) = obj.get("e").and_then(Value::as_object) else {
                    continue;
                };
                let sess = e.get("sess").and_then(Value::as_str).unwrap_or("");
                let ev_type = e.get("type").and_then(Value::as_str).unwrap_or("");
                let label = e.get("label").and_then(Value::as_str).unwrap_or("");
                let agent = e.get("agent").and_then(Value::as_str).filter(|s| !s.is_empty());
                let s_obj = obj.get("s").and_then(Value::as_object);
                let name = s_obj
                    .and_then(|s| s.get("name"))
                    .and_then(Value::as_str)
                    .or_else(|| e.get("name").and_then(Value::as_str));

                if let Some(s) = s_obj
                    && agent.is_none()
                {
                    let rk = get_row(&mut rows, &mut name_of, sess, None, name);
                    if let Some(r) = rows.get_mut(&rk) {
                        if let Some(desk) = s.get("desk").and_then(Value::as_i64) {
                            r.desk = desk as i32;
                        }
                        if let Some(title) = s.get("title").and_then(Value::as_str)
                            && !title.is_empty()
                        {
                            r.title = Some(title.to_owned());
                        }
                    }
                }

                if ev_type == "substart" {
                    let srk = get_row(&mut rows, &mut name_of, sess, agent, Some(label));
                    if let Some(sr) = rows.get_mut(&srk) {
                        sr.open(t, "think", "", from, to);
                    }
                    continue;
                }
                if ev_type == "substop" {
                    if let Some(a) = agent {
                        let srk = format!("{}|{a}", key_of(sess, &name_of));
                        if let Some(sr) = rows.get_mut(&srk) {
                            if let Some(desc) = e.get("desc").and_then(Value::as_str)
                                && !desc.is_empty()
                            {
                                let left = if !label.is_empty() { label } else { &sr.name };
                                sr.name = if left.is_empty() {
                                    desc.to_owned()
                                } else {
                                    format!("{left} · {desc}")
                                };
                            }
                            sr.close(t, from, to);
                        }
                    }
                    continue;
                }
                if let Some(a) = agent {
                    let atype = e.get("agentType").and_then(Value::as_str).unwrap_or("");
                    let srk = get_row(&mut rows, &mut name_of, sess, Some(a), Some(atype));
                    if let Some(sr) = rows.get_mut(&srk) {
                        if ev_type == "fail" {
                            sr.mark(t, "fail", label, from, to);
                        } else {
                            sr.open(t, kind_of_type(ev_type), label, from, to);
                        }
                    }
                    continue;
                }

                let rk = get_row(&mut rows, &mut name_of, sess, None, name);
                if let Some(r) = rows.get_mut(&rk) {
                    match ev_type {
                        "start" => {
                            if r.cur.is_none() {
                                r.open(t, "idle", "", from, to);
                            }
                            r.mark(t, "start", "", from, to);
                        }
                        "end" => {
                            r.close(t, from, to);
                            r.mark(t, "end", "", from, to);
                        }
                        "fail" | "deploy" => {
                            r.mark(t, ev_type, label, from, to);
                        }
                        _ => {
                            if ev_type == "prompt" {
                                r.mark(t, "prompt", label, from, to);
                            }
                            r.open(t, kind_of_type(ev_type), label, from, to);
                        }
                    }
                }
            }
            "stats" => {
                let Some(s) = obj.get("s").and_then(Value::as_object) else {
                    continue;
                };
                let sid = s.get("id").and_then(Value::as_str).unwrap_or("");
                if let Some(nm) = s.get("name").and_then(Value::as_str)
                    && !nm.is_empty()
                {
                    name_of.insert(sid.to_owned(), nm.to_owned());
                }
                let post = obj.get("post").and_then(Value::as_str);
                let target_k = match post {
                    Some(p) if p != "main" && !p.is_empty() => format!("{}|{p}", key_of(sid, &name_of)),
                    _ => key_of(sid, &name_of),
                };
                if let Some(r) = rows.get_mut(&target_k)
                    && r.cur.as_ref().is_some_and(|(ck, _, _)| is_tool_kind(ck))
                {
                    r.open(t, "think", "", from, to);
                }
            }
            "sess" => {
                let Some(s) = obj.get("s").and_then(Value::as_object) else {
                    continue;
                };
                let sid = s.get("id").and_then(Value::as_str).unwrap_or("");
                let sname = s.get("name").and_then(Value::as_str);
                let rk = get_row(&mut rows, &mut name_of, sid, None, sname);
                if let Some(r) = rows.get_mut(&rk) {
                    if let Some(title) = s.get("title").and_then(Value::as_str)
                        && !title.is_empty()
                    {
                        r.title = Some(title.to_owned());
                    }
                    let status = s.get("status").and_then(Value::as_str).unwrap_or("");
                    let cur_k = r.cur.as_ref().map(|(ck, _, _)| ck.as_str());
                    if status == "wait" && cur_k != Some("wait") {
                        r.open(t, "wait", "", from, to);
                    } else if status == "work" && matches!(cur_k, None | Some("wait" | "idle" | "turn")) {
                        r.open(t, "think", "", from, to);
                    } else if status == "rest" && cur_k.is_some_and(|ck| ck != "turn" && ck != "idle") {
                        r.open(t, "turn", "", from, to);
                    }
                }
            }
            "gone" => {
                if let Some(id) = obj.get("id").and_then(Value::as_str) {
                    let k = key_of(id, &name_of);
                    let prefix = format!("{k}|");
                    for r in rows.values_mut() {
                        if r.key == k || r.key.starts_with(&prefix) {
                            r.close(t, from, to);
                        }
                    }
                }
            }
            _ => {}
        }
    }

    let end = to.min(now_ms);
    for r in rows.values_mut() {
        r.close(end, from, to);
    }

    let sum_kinds = |spans: &[(u64, u64, String, String)], kinds: &[&str], cap: u64| -> u64 {
        spans
            .iter()
            .filter(|(_, _, k, _)| kinds.contains(&k.as_str()))
            .map(|(s, e, _, _)| (e.saturating_sub(*s)).min(cap))
            .sum()
    };

    let work_kinds = ["think", "read", "edit", "shell", "web", "agent"];
    let active_rows: Vec<TimelineRow> = rows
        .into_values()
        .filter(|r| {
            r.spans.iter().any(|x| x.2 != "idle" && x.2 != "turn")
                || r.marks.iter().any(|x| x.1 == "prompt")
        })
        .map(|r| {
            let totals = TimelineTotals {
                work: sum_kinds(&r.spans, &work_kinds, u64::MAX),
                wait: sum_kinds(&r.spans, &["wait"], u64::MAX),
                turn: sum_kinds(&r.spans, &["turn"], TURN_CAP_MS),
                prompts: r.marks.iter().filter(|x| x.1 == "prompt").count(),
            };
            TimelineRow {
                key: r.key,
                sess: r.sess,
                agent: r.agent,
                name: r.name,
                title: r.title,
                desk: r.desk,
                spans: r.spans,
                marks: r.marks,
                totals,
            }
        })
        .collect();

    let first_t = |r: &TimelineRow| -> u64 {
        r.spans
            .iter()
            .find(|x| x.2 != "idle" && x.2 != "turn")
            .or_else(|| r.spans.first())
            .map(|x| x.0)
            .unwrap_or(to)
    };

    let mut mains: Vec<TimelineRow> = active_rows
        .iter()
        .filter(|r| r.agent.is_none())
        .cloned()
        .collect();
    mains.sort_by_key(first_t);

    fn parent_of(r: &TimelineRow) -> &str {
        r.key.rsplit_once('|').map(|(p, _)| p).unwrap_or(&r.key)
    }

    let mut ordered = Vec::with_capacity(active_rows.len());
    let main_keys: HashSet<String> = mains.iter().map(|m| m.key.clone()).collect();
    for m in mains {
        let mk = m.key.clone();
        ordered.push(m);
        let mut subs: Vec<TimelineRow> = active_rows
            .iter()
            .filter(|x| x.agent.is_some() && parent_of(x) == mk)
            .cloned()
            .collect();
        subs.sort_by_key(first_t);
        ordered.extend(subs);
    }
    for r in active_rows {
        if r.agent.is_some() && !main_keys.contains(parent_of(&r)) {
            ordered.push(r);
        }
    }

    let mut totals = TimelineTotals::default();
    for r in &ordered {
        if r.agent.is_none() {
            totals.work += r.totals.work;
            totals.wait += r.totals.wait;
            totals.turn += r.totals.turn;
            totals.prompts += r.totals.prompts;
        }
    }

    TimelineResult {
        from,
        to,
        rows: ordered,
        totals,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    const T0: u64 = 1_793_091_600_000;

    fn ev(sec: u64, sess: &str, typ: &str, label: Option<&str>) -> ReplayItem {
        let t = T0 + sec * 1000;
        let mut e = json!({ "sess": sess, "type": typ, "ts": t });
        if let Some(l) = label {
            e["label"] = json!(l);
        }
        ReplayItem {
            t,
            m: json!({ "k": "ev", "e": e, "s": { "id": sess, "name": "shop-api", "status": "work" } }),
        }
    }

    #[test]
    fn timeline_spans_and_empty_match_oracle() {
        let items = vec![
            ev(0, "a", "prompt", Some("add cart")),
            ev(10, "a", "read", Some("cart.ts")),
            ev(40, "a", "edit", Some("cart.ts")),
            ev(100, "a", "stop", None),
        ];
        let tl = build_timeline(&items, T0, T0 + 200_000, T0 + 200_000);
        assert_eq!(tl.rows.len(), 1);
        let spans = &tl.rows[0].spans;
        assert!(spans.len() >= 3);
        for w in spans.windows(2) {
            assert!(w[1].0 >= w[0].0);
        }
        let kinds: Vec<&str> = spans[..3].iter().map(|s| s.2.as_str()).collect();
        assert_eq!(kinds, vec!["think", "read", "edit"]);

        let empty = build_timeline(&[], T0, T0 + 60_000, T0 + 60_000);
        assert!(empty.rows.is_empty());
        assert_eq!(empty.totals.prompts, 0);
    }
}
