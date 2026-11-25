use serde::{Deserialize, Serialize};
use std::collections::HashMap;

pub const DIMS: [&str; 6] = ["model", "source", "agent", "skill", "plugin", "mcp"];
const DAY_MS: u64 = 86_400_000;

pub type SumTuple = [f64; 6];

pub fn zero() -> SumTuple {
    [0.0; 6]
}

pub fn add_to(sum: &mut SumTuple, v: &SumTuple) {
    for i in 0..6 {
        sum[i] += v[i];
    }
}

pub fn project_of(cwd: &str) -> String {
    if cwd.is_empty() {
        return String::new();
    }
    let normalized = cwd.replace('\\', "/");
    let target = if let Some(idx) = normalized.find("/.claude/worktrees/") {
        &normalized[..idx]
    } else {
        &normalized
    };
    target
        .split('/')
        .filter(|s| !s.is_empty())
        .next_back()
        .unwrap_or("")
        .to_owned()
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct DaySessionRollup {
    #[serde(default)]
    pub m: HashMap<String, SumTuple>,
    #[serde(default)]
    pub e: HashMap<String, SumTuple>,
    #[serde(default)]
    pub req: u64,
    #[serde(default)]
    pub err: u64,
    #[serde(default)]
    pub pr: u64,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct DayRollup {
    #[serde(default)]
    pub s: HashMap<String, DaySessionRollup>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct SessionMeta {
    #[serde(default)]
    pub m: bool,
    #[serde(default)]
    pub cwd: String,
    #[serde(default)]
    pub l: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TokBreakdown {
    #[serde(rename = "in")]
    pub input: f64,
    pub out: f64,
    #[serde(rename = "cacheRead")]
    pub cache_read: f64,
    #[serde(rename = "cacheWrite")]
    pub cache_write: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BurnRow {
    pub name: String,
    pub usd: f64,
    pub tokens: f64,
    pub tok: TokBreakdown,
    pub share: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BurnTotal {
    #[serde(flatten)]
    pub row: BurnRow,
    pub req: u64,
    pub err: u64,
    pub prompts: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BurnRoleRow {
    pub role: String,
    pub model: String,
    #[serde(flatten)]
    pub row: BurnRow,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BurnRankedRow {
    pub kind: String,
    #[serde(flatten)]
    pub row: BurnRow,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BurnSessionRow {
    #[serde(flatten)]
    pub row: BurnRow,
    pub sid: String,
    pub cwd: String,
    pub project: String,
    pub req: u64,
    pub err: u64,
    pub prompts: u64,
    pub via: String,
    pub last: u64,
    pub models: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BurnProjectRow {
    #[serde(flatten)]
    pub row: BurnRow,
    pub cwd: String,
    pub sessions: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BurnDayRow {
    pub day: String,
    pub usd: f64,
    pub models: HashMap<String, f64>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BurnSummary {
    pub money: bool,
    pub total: BurnTotal,
    pub plain: BurnRow,
    pub roles: Vec<BurnRoleRow>,
    pub by: HashMap<String, Vec<BurnRow>>,
    pub ranked: Vec<BurnRankedRow>,
    pub sessions: Vec<BurnSessionRow>,
    pub projects: Vec<BurnProjectRow>,
    #[serde(rename = "perDay")]
    pub per_day: Vec<BurnDayRow>,
}

struct SessAcc {
    sid: String,
    sum: SumTuple,
    req: u64,
    err: u64,
    prompts: u64,
    via: &'static str,
    models: HashMap<String, f64>,
}

pub fn summarize(
    days: &[(String, DayRollup)],
    sess: &HashMap<String, SessionMeta>,
    anon_at: u64,
    at: u64,
) -> BurnSummary {
    let mut total = zero();
    let mut plain = zero();
    let mut s_map: HashMap<String, SessAcc> = HashMap::new();
    let mut by: [HashMap<String, SumTuple>; 6] = std::array::from_fn(|_| HashMap::new());
    let mut per_day = Vec::with_capacity(days.len());
    let mut roles_map: HashMap<String, SumTuple> = HashMap::new();
    let mut req = 0u64;
    let mut err = 0u64;
    let mut prompts = 0u64;

    let events_count = anon_at == 0 || at.saturating_sub(anon_at) > 8 * DAY_MS;

    for (day, d) in days {
        let mut day_models: HashMap<String, f64> = HashMap::new();
        let mut day_usd = 0.0;
        for (sid, x) in &d.s {
            let via_m = sess.get(sid).is_some_and(|m| m.m);
            let led = if via_m {
                Some(&x.m)
            } else if events_count {
                Some(&x.e)
            } else {
                None
            };
            let entry = s_map.entry(sid.clone()).or_insert_with(|| SessAcc {
                sid: sid.clone(),
                sum: zero(),
                req: 0,
                err: 0,
                prompts: 0,
                via: if via_m { "m" } else { "e" },
                models: HashMap::new(),
            });
            entry.req += x.req;
            entry.err += x.err;
            entry.prompts += x.pr;
            req += x.req;
            err += x.err;
            prompts += x.pr;

            if let Some(ledger) = led {
                for (k, w) in ledger {
                    let names: Vec<&str> = k.split('\t').collect();
                    add_to(&mut total, w);
                    add_to(&mut entry.sum, w);
                    for i in 0..6 {
                        if let Some(&nm) = names.get(i)
                            && !nm.is_empty()
                        {
                            add_to(by[i].entry(nm.to_owned()).or_insert_with(zero), w);
                        }
                    }
                    let has_attr = (2..6).any(|i| names.get(i).is_some_and(|n| !n.is_empty()));
                    if !has_attr {
                        add_to(&mut plain, w);
                    }
                    let model_name = names.first().copied().unwrap_or("");
                    let src_name = names.get(1).copied().unwrap_or("");
                    if src_name == "main" || src_name == "subagent" {
                        let rk = format!("{src_name}\t{model_name}");
                        add_to(roles_map.entry(rk).or_insert_with(zero), w);
                    }
                    *entry.models.entry(model_name.to_owned()).or_insert(0.0) += w[0];
                    let dm_key = if model_name.is_empty() { "?" } else { model_name };
                    *day_models.entry(dm_key.to_owned()).or_insert(0.0) += w[0];
                    day_usd += w[0];
                }
            }
        }
        per_day.push(BurnDayRow {
            day: day.clone(),
            usd: day_usd,
            models: day_models,
        });
    }

    let tokens = |v: &SumTuple| v[1] + v[2] + v[3] + v[4];
    let money = total[0] > 0.0;
    let weight = |v: &SumTuple| if money { v[0] } else { tokens(v) };
    let all = {
        let w = weight(&total);
        if w > 0.0 { w } else { 1.0 }
    };
    let make_row = |name: String, v: &SumTuple| BurnRow {
        name,
        usd: v[0],
        tokens: tokens(v),
        tok: TokBreakdown {
            input: v[1],
            out: v[2],
            cache_read: v[3],
            cache_write: v[4],
        },
        share: weight(v) / all,
    };

    let list_of = |m: &HashMap<String, SumTuple>| -> Vec<BurnRow> {
        let mut v: Vec<BurnRow> = m
            .iter()
            .map(|(name, sum)| make_row(name.clone(), sum))
            .collect();
        v.sort_by(|a, b| b.share.total_cmp(&a.share).then_with(|| a.name.cmp(&b.name)));
        v
    };

    let mut sessions: Vec<BurnSessionRow> = s_map
        .into_values()
        .map(|s| {
            let info = sess.get(&s.sid);
            let cwd = info.map(|i| i.cwd.clone()).unwrap_or_default();
            let last = info.map(|i| i.l).unwrap_or(0);
            let mut mlist: Vec<(String, f64)> = s.models.into_iter().collect();
            mlist.sort_by(|a, b| b.1.total_cmp(&a.1));
            let models: Vec<String> = mlist
                .into_iter()
                .map(|(m, _)| m)
                .filter(|m| !m.is_empty())
                .take(3)
                .collect();
            let row = make_row(s.sid.clone(), &s.sum);
            BurnSessionRow {
                row,
                sid: s.sid,
                project: project_of(&cwd),
                cwd,
                req: s.req,
                err: s.err,
                prompts: s.prompts,
                via: s.via.to_owned(),
                last,
                models,
            }
        })
        .filter(|s| s.row.usd > 0.0 || s.row.tokens > 0.0 || s.req > 0 || s.err > 0 || s.prompts > 0)
        .collect();
    sessions.sort_by(|a, b| b.row.share.total_cmp(&a.row.share).then_with(|| b.last.cmp(&a.last)));

    let mut proj_map: HashMap<String, (String, SumTuple, usize)> = HashMap::new();
    for s in &sessions {
        let e = proj_map
            .entry(s.project.clone())
            .or_insert_with(|| (s.cwd.clone(), zero(), 0));
        add_to(
            &mut e.1,
            &[
                s.row.usd,
                s.row.tok.input,
                s.row.tok.out,
                s.row.tok.cache_read,
                s.row.tok.cache_write,
                0.0,
            ],
        );
        e.2 += 1;
    }
    let mut projects: Vec<BurnProjectRow> = proj_map
        .into_iter()
        .map(|(name, (cwd, sum, count))| BurnProjectRow {
            row: make_row(name, &sum),
            cwd,
            sessions: count,
        })
        .collect();
    projects.sort_by(|a, b| b.row.share.total_cmp(&a.row.share));

    let mut by_map: HashMap<String, Vec<BurnRow>> = HashMap::new();
    for (i, dim) in DIMS.iter().enumerate() {
        by_map.insert((*dim).to_owned(), list_of(&by[i]));
    }

    let mut ranked = Vec::new();
    for kind in ["mcp", "skill", "plugin", "agent"] {
        if let Some(rows) = by_map.get(kind) {
            for r in rows {
                ranked.push(BurnRankedRow {
                    kind: kind.to_owned(),
                    row: r.clone(),
                });
            }
        }
    }
    ranked.sort_by(|a, b| b.row.share.total_cmp(&a.row.share));

    let mut roles: Vec<BurnRoleRow> = roles_map
        .into_iter()
        .filter_map(|(k, v)| {
            let row = make_row(String::new(), &v);
            if row.usd <= 0.0 && row.tokens <= 0.0 {
                return None;
            }
            let mut parts = k.splitn(2, '\t');
            let role = parts.next().unwrap_or("").to_owned();
            let model = parts.next().unwrap_or("").to_owned();
            Some(BurnRoleRow {
                role,
                model: model.clone(),
                row: BurnRow { name: model, ..row },
            })
        })
        .collect();
    roles.sort_by(|a, b| b.row.share.total_cmp(&a.row.share));

    BurnSummary {
        money,
        total: BurnTotal {
            row: make_row(String::new(), &total),
            req,
            err,
            prompts,
        },
        plain: make_row(String::new(), &plain),
        roles,
        by: by_map,
        ranked,
        sessions,
        projects,
        per_day,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn key(model: &str, source: &str, agent: &str) -> String {
        format!("{model}\t{source}\t{agent}\t\t\t")
    }

    #[test]
    fn coordinator_and_subagents_burn_roles_match_oracle() {
        let mut m = HashMap::new();
        m.insert(key("claude-opus-5-5", "main", ""), [6.0, 0.0, 0.0, 0.0, 0.0, 3.0]);
        m.insert(key("claude-sonnet-5-5", "subagent", "Explore"), [3.0, 0.0, 0.0, 0.0, 0.0, 5.0]);
        m.insert(key("claude-haiku-5-5", "subagent", "Explore"), [0.5, 0.0, 0.0, 0.0, 0.0, 2.0]);
        m.insert(key("claude-haiku-5-5", "auxiliary", ""), [0.5, 0.0, 0.0, 0.0, 0.0, 1.0]);

        let mut day = DayRollup::default();
        day.s.insert("s1".to_owned(), DaySessionRollup { m, ..Default::default() });

        let mut sess = HashMap::new();
        sess.insert("s1".to_owned(), SessionMeta { m: true, cwd: "D:/repo/.claude/worktrees/feat".into(), l: 100 });

        let r = summarize(&[("2026-11-02".to_owned(), day)], &sess, 0, 1_800_000_000_000);
        let triples: Vec<(&str, &str, f64)> = r
            .roles
            .iter()
            .map(|x| (x.role.as_str(), x.model.as_str(), x.row.usd))
            .collect();
        assert_eq!(
            triples,
            vec![
                ("main", "claude-opus-5-5", 6.0),
                ("subagent", "claude-sonnet-5-5", 3.0),
                ("subagent", "claude-haiku-5-5", 0.5),
            ]
        );
        assert!((r.roles[0].row.share - 0.6).abs() < 1e-9);
        assert!((r.total.row.usd - 10.0).abs() < 1e-9);
        assert_eq!(project_of("D:/repo/.claude/worktrees/feat"), "repo");
    }
}
