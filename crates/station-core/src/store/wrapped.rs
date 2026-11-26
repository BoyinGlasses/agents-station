use crate::shared::timeline::{ReplayItem, build_timeline};
use chrono::{Datelike, Local, NaiveDate, TimeZone};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use std::collections::{HashMap, HashSet};
use std::path::Path;

pub fn local_day_bounds_ms(nd: NaiveDate) -> Option<(u64, u64)> {
    let start_naive = nd.and_hms_opt(0, 0, 0)?;
    let start_dt = Local
        .from_local_datetime(&start_naive)
        .earliest()
        .or_else(|| Local.from_local_datetime(&start_naive).latest())?;
    let from_ms = start_dt.timestamp_millis().max(0) as u64;
    let to_ms = nd
        .succ_opt()
        .and_then(|next| next.and_hms_opt(0, 0, 0))
        .and_then(|end_naive| {
            Local
                .from_local_datetime(&end_naive)
                .earliest()
                .or_else(|| Local.from_local_datetime(&end_naive).latest())
        })
        .map(|dt| dt.timestamp_millis().max(0) as u64)
        .unwrap_or(from_ms + 86_400_000);
    Some((from_ms, to_ms))
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WrappedDay {
    pub day: String,
    pub weekday: u32,
    pub hours: f64,
    pub prompts: u64,
}

pub fn union_ms(mut list: Vec<(u64, u64)>) -> u64 {
    list.sort_by_key(|&(a, _)| a);
    let mut total = 0u64;
    let mut end = 0u64;
    for (a, b) in list {
        if b <= end {
            continue;
        }
        total += b - a.max(end);
        end = b;
    }
    total
}

pub fn week_summary(replay_dir: &Path, days: usize, now_ms: u64) -> Value {
    let mut files: Vec<String> = std::fs::read_dir(replay_dir)
        .ok()
        .into_iter()
        .flatten()
        .flatten()
        .filter_map(|e| {
            let n = e.file_name().to_string_lossy().into_owned();
            if n.len() == 16 && n.ends_with(".jsonl") {
                Some(n)
            } else {
                None
            }
        })
        .collect();
    files.sort();
    if files.len() > days {
        files = files.split_off(files.len() - days);
    }

    let mut from_day: Option<String> = None;
    let mut to_day: Option<String> = None;
    let mut hours_sum = 0.0f64;
    let mut hours_wall = 0.0f64;
    let mut wait_hours = 0.0f64;
    let mut commits = 0u64;
    let mut pushes = 0u64;
    let mut longest_min = 0u64;
    let stat_keys = ["prompts", "edit", "read", "cmd", "web", "deploy"];
    let mut stats_totals: HashMap<&str, u64> = stat_keys.iter().map(|&k| (k, 0)).collect();
    let mut costs: HashMap<String, f64> = HashMap::new();
    let mut projects: HashMap<String, u64> = HashMap::new();
    let mut names: HashSet<String> = HashSet::new();
    let mut per_day: Vec<WrappedDay> = Vec::new();

    for f in &files {
        let day = &f[..10];
        let Ok(nd) = NaiveDate::parse_from_str(day, "%Y-%m-%d") else {
            continue;
        };
        let Some((from_ms, to_ms)) = local_day_bounds_ms(nd) else {
            continue;
        };
        let Ok(text) = std::fs::read_to_string(replay_dir.join(f)) else {
            continue;
        };
        let items: Vec<ReplayItem> = text
            .lines()
            .filter_map(|l| serde_json::from_str::<ReplayItem>(l).ok())
            .collect();
        if items.is_empty() {
            continue;
        }
        if from_day.is_none() {
            from_day = Some(day.to_owned());
        }
        to_day = Some(day.to_owned());
        let mut day_stats_max: HashMap<&str, u64> = stat_keys.iter().map(|&k| (k, 0)).collect();
        for it in &items {
            if let Some(st) = it.m.get("stats").and_then(Value::as_object) {
                for &k in &stat_keys {
                    if let Some(v) = st.get(k).and_then(Value::as_u64) {
                        let e = day_stats_max.entry(k).or_insert(0);
                        if v > *e {
                            *e = v;
                        }
                    }
                }
            }
            let sess_vals: Vec<&Value> = if it.m.get("k").and_then(Value::as_str) == Some("snap") {
                it.m.get("sessions")
                    .and_then(Value::as_array)
                    .map(|a| a.iter().collect())
                    .unwrap_or_default()
            } else {
                it.m.get("s").into_iter().collect()
            };
            for s in sess_vals {
                if let Some(cost) = s
                    .get("usage")
                    .and_then(|u| u.get("cost"))
                    .and_then(Value::as_f64)
                {
                    let key = s
                        .get("name")
                        .and_then(Value::as_str)
                        .filter(|x| !x.is_empty())
                        .or_else(|| s.get("id").and_then(Value::as_str))
                        .unwrap_or("")
                        .to_owned();
                    if !key.is_empty() {
                        let entry = costs.entry(key).or_insert(0.0);
                        if cost > *entry {
                            *entry = cost;
                        }
                    }
                }
            }
            if it.m.get("k").and_then(Value::as_str) == Some("ev") {
                let et = it.m.get("e").and_then(|e| e.get("type")).and_then(Value::as_str);
                if et == Some("commit") {
                    commits += 1;
                } else if et == Some("push") {
                    pushes += 1;
                }
            }
        }
        for &k in &stat_keys {
            *stats_totals.entry(k).or_insert(0) += day_stats_max[k];
        }
        let tl = build_timeline(&items, from_ms, to_ms.min(now_ms), now_ms);
        let mut day_work = 0u64;
        let mut day_wait = 0u64;
        let mut intervals = Vec::new();
        for r in &tl.rows {
            if r.agent.is_some() {
                continue;
            }
            names.insert(r.name.clone());
            day_work += r.totals.work;
            day_wait += r.totals.wait + r.totals.turn;
            *projects.entry(r.name.clone()).or_insert(0) += r.totals.work;
            let mut run = 0u64;
            let mut run_end = 0u64;
            for (a, b, k, _) in &r.spans {
                if matches!(k.as_str(), "think" | "read" | "edit" | "shell" | "web" | "agent") {
                    intervals.push((*a, *b));
                    let dur = b.saturating_sub(*a);
                    run = if run_end > 0 && a.saturating_sub(run_end) < 60_000 {
                        run.saturating_add(dur)
                    } else {
                        dur
                    };
                    run_end = *b;
                    let mins = ((run as f64) / 60_000.0).round() as u64;
                    if mins > longest_min {
                        longest_min = mins;
                    }
                }
            }
        }
        hours_sum += (day_work as f64) / 3_600_000.0;
        hours_wall += (union_ms(intervals) as f64) / 3_600_000.0;
        wait_hours += (day_wait as f64) / 3_600_000.0;
        per_day.push(WrappedDay {
            day: day.to_owned(),
            weekday: nd.weekday().num_days_from_sunday(),
            hours: ((day_work as f64 / 3_600_000.0) * 10.0).round() / 10.0,
            prompts: day_stats_max["prompts"].max(tl.totals.prompts as u64),
        });
    }

    let mut top: Vec<(String, u64)> = projects.into_iter().collect();
    top.sort_by(|a, b| b.1.cmp(&a.1));
    let top_projects: Vec<Value> = top
        .into_iter()
        .take(5)
        .map(|(name, ms)| json!({ "name": name, "hours": ((ms as f64 / 3_600_000.0) * 10.0).round() / 10.0 }))
        .collect();
    let busiest = per_day
        .iter()
        .max_by(|a, b| a.hours.total_cmp(&b.hours))
        .map(|d| json!(d))
        .unwrap_or(Value::Null);
    let total_cost = (costs.values().sum::<f64>() * 100.0).round() / 100.0;

    json!({
        "from": from_day,
        "to": to_day,
        "days": files.len(),
        "hoursSum": (hours_sum * 10.0).round() / 10.0,
        "hoursWall": (hours_wall * 10.0).round() / 10.0,
        "waitHours": (wait_hours * 10.0).round() / 10.0,
        "stats": {
            "prompts": stats_totals["prompts"],
            "edit": stats_totals["edit"],
            "read": stats_totals["read"],
            "cmd": stats_totals["cmd"],
            "web": stats_totals["web"],
            "deploy": stats_totals["deploy"],
        },
        "commits": commits,
        "pushes": pushes,
        "longestMin": longest_min,
        "cost": total_cost,
        "sessions": names.len(),
        "projects": top_projects,
        "busiest": busiest,
        "perDay": per_day,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn union_ms_merges_overlapping_intervals() {
        assert_eq!(union_ms(vec![(10, 50), (30, 80), (100, 120)]), 90);
        let dir = tempfile::tempdir().unwrap();
        let w = week_summary(dir.path(), 7, 1_800_000_000_000);
        assert_eq!(w["days"], 0);
    }

    #[test]
    fn week_summary_uses_local_timezone_day_boundaries() {
        use chrono::{Local, TimeZone};

        let dir = tempfile::tempdir().unwrap();
        let nd = NaiveDate::from_ymd_opt(2026, 11, 25).unwrap();
        let local_start = Local
            .from_local_datetime(&nd.and_hms_opt(0, 0, 0).unwrap())
            .earliest()
            .unwrap()
            .timestamp_millis() as u64;
        let local_next = Local
            .from_local_datetime(&nd.succ_opt().unwrap().and_hms_opt(0, 0, 0).unwrap())
            .earliest()
            .unwrap()
            .timestamp_millis() as u64;

        // Verify helper matches Local midnight bounds directly (timezone-invariant check).
        assert_eq!(local_day_bounds_ms(nd), Some((local_start, local_next)));

        // Event between local 00:30 and 01:30 (1.0 hour of work) — dropped if UTC midnight is used in non-UTC zones.
        let t_start = local_start + 30 * 60 * 1000;
        let t_stop = t_start + 3_600_000;
        let lines = format!(
            "{}\n{}\n",
            json!({
                "t": t_start,
                "m": {
                    "k": "ev",
                    "e": { "sess": "s1", "type": "prompt", "label": "night work", "ts": t_start },
                    "s": { "id": "s1", "name": "repo-a", "status": "work" }
                }
            }),
            json!({
                "t": t_stop,
                "m": {
                    "k": "ev",
                    "e": { "sess": "s1", "type": "stop", "label": "repo-a", "ts": t_stop },
                    "s": { "id": "s1", "name": "repo-a", "status": "rest" }
                }
            }),
        );
        std::fs::write(dir.path().join("2026-11-25.jsonl"), lines).unwrap();

        let w = week_summary(dir.path(), 7, t_stop + 60_000);
        assert_eq!(w["days"], 1);
        assert_eq!(w["perDay"][0]["day"], "2026-11-25");
        assert_eq!(w["perDay"][0]["prompts"], 1);
        assert_eq!(w["perDay"][0]["hours"], 1.0);
        assert_eq!(w["hoursSum"], 1.0);
    }

    #[test]
    fn week_summary_includes_oracle_busiest_cost_longest_min_and_stats() {
        let dir = tempfile::tempdir().unwrap();
        let w = week_summary(dir.path(), 7, 1_800_000_000_000);
        let obj = w.as_object().unwrap();
        for key in [
            "busiest",
            "commits",
            "cost",
            "days",
            "from",
            "hoursSum",
            "hoursWall",
            "longestMin",
            "perDay",
            "projects",
            "pushes",
            "sessions",
            "stats",
            "to",
            "waitHours",
        ] {
            assert!(obj.contains_key(key), "wrapped missing Oracle key `{key}`");
        }
        assert!(obj["stats"].get("prompts").is_some());
    }
}
