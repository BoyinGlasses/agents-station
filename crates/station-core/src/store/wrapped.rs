use crate::shared::timeline::{ReplayItem, build_timeline};
use chrono::NaiveDate;
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use std::collections::{HashMap, HashSet};
use std::path::Path;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WrappedDay {
    pub day: String,
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
    let mut projects: HashMap<String, u64> = HashMap::new();
    let mut names: HashSet<String> = HashSet::new();
    let mut per_day: Vec<WrappedDay> = Vec::new();

    for f in &files {
        let day = &f[..10];
        let Ok(nd) = NaiveDate::parse_from_str(day, "%Y-%m-%d") else {
            continue;
        };
        let from_ms = nd
            .and_hms_opt(0, 0, 0)
            .unwrap()
            .and_utc()
            .timestamp_millis() as u64;
        let to_ms = from_ms + 86_400_000;
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
        for it in &items {
            if it.m.get("k").and_then(Value::as_str) == Some("ev") {
                let et = it.m.get("e").and_then(|e| e.get("type")).and_then(Value::as_str);
                if et == Some("commit") {
                    commits += 1;
                } else if et == Some("push") {
                    pushes += 1;
                }
            }
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
            for (a, b, k, _) in &r.spans {
                if matches!(k.as_str(), "think" | "read" | "edit" | "shell" | "web" | "agent") {
                    intervals.push((*a, *b));
                }
            }
        }
        hours_sum += (day_work as f64) / 3_600_000.0;
        hours_wall += (union_ms(intervals) as f64) / 3_600_000.0;
        wait_hours += (day_wait as f64) / 3_600_000.0;
        per_day.push(WrappedDay {
            day: day.to_owned(),
            hours: ((day_work as f64 / 3_600_000.0) * 10.0).round() / 10.0,
            prompts: tl.totals.prompts as u64,
        });
    }

    let mut top: Vec<(String, u64)> = projects.into_iter().collect();
    top.sort_by(|a, b| b.1.cmp(&a.1));
    let top_projects: Vec<Value> = top
        .into_iter()
        .take(5)
        .map(|(name, ms)| json!({ "name": name, "hours": ((ms as f64 / 3_600_000.0) * 10.0).round() / 10.0 }))
        .collect();

    json!({
        "from": from_day,
        "to": to_day,
        "days": files.len(),
        "hoursSum": (hours_sum * 10.0).round() / 10.0,
        "hoursWall": (hours_wall * 10.0).round() / 10.0,
        "waitHours": (wait_hours * 10.0).round() / 10.0,
        "commits": commits,
        "pushes": pushes,
        "sessions": names.len(),
        "projects": top_projects,
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
}
