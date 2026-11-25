use crate::atomic::write_json_atomic;
use chrono::{Local, TimeZone};
use serde::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};

pub const FAREWELL_DAYS: [u32; 4] = [14, 30, 60, 90];
pub const FAREWELL_DEFAULT: u32 = 30;
pub const PROMOTE_TURNS: u32 = 20;
pub const PROMOTE_DAYS: u32 = 2;
const DAY_MS: u64 = 86_400_000;
const MAX_PROJECTS: usize = 300;

pub fn day_of(ms: u64) -> String {
    Local
        .timestamp_millis_opt(ms as i64)
        .single()
        .map(|dt| dt.format("%Y-%m-%d").to_string())
        .unwrap_or_else(|| "1970-01-01".to_owned())
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct ProjectTenure {
    pub first: u64,
    pub last: u64,
    pub days: u32,
    #[serde(rename = "lastDay", default)]
    pub last_day: String,
    #[serde(default)]
    pub turns: u32,
    #[serde(default)]
    pub pre: bool,
    #[serde(default)]
    pub checked: bool,
    #[serde(default)]
    pub official: u64,
    #[serde(default)]
    pub farewell: u64,
    #[serde(default)]
    pub back: u64,
}

pub fn was_active(p: &ProjectTenure) -> bool {
    p.days >= 2
}

pub fn farewell_due(p: &ProjectTenure, days: u32, now_ms: u64, live: bool) -> bool {
    days > 0
        && was_active(p)
        && p.farewell == 0
        && !live
        && now_ms.saturating_sub(p.last) >= (days as u64) * DAY_MS
}

pub fn is_intern(p: &ProjectTenure) -> bool {
    !p.pre && p.official == 0 && p.checked
}

pub fn promote_due(p: &ProjectTenure) -> bool {
    is_intern(p) && p.turns >= PROMOTE_TURNS && p.days >= PROMOTE_DAYS
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct TenureFile {
    v: u32,
    since: u64,
    seeded: bool,
    interns: u64,
    projects: HashMap<String, ProjectTenure>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TenureView {
    pub name: String,
    pub last: u64,
    pub days: u32,
    pub turns: u32,
    pub due: bool,
    pub farewell: u64,
    pub back: u64,
    pub intern: bool,
    pub ready: bool,
    pub official: u64,
}

pub struct TenureStore {
    file: PathBuf,
    data: TenureFile,
}

impl TenureStore {
    pub fn load(data_dir: &Path, now_ms: u64) -> Self {
        let file = data_dir.join("tenure.json");
        let mut data = std::fs::read_to_string(&file)
            .ok()
            .and_then(|s| serde_json::from_str::<TenureFile>(&s).ok())
            .unwrap_or_else(|| TenureFile {
                v: 1,
                since: now_ms,
                seeded: false,
                interns: now_ms,
                projects: HashMap::new(),
            });
        if data.interns == 0 {
            data.interns = now_ms;
            for p in data.projects.values_mut() {
                p.pre = true;
            }
        }
        Self { file, data }
    }

    pub fn touch(&mut self, name: &str, now_ms: u64, turn: bool) -> Option<&'static str> {
        if name.is_empty() {
            return None;
        }
        let mut change = None;
        if !self.data.projects.contains_key(name) {
            self.data.projects.insert(
                name.to_owned(),
                ProjectTenure {
                    first: now_ms,
                    last: now_ms,
                    checked: true,
                    ..Default::default()
                },
            );
            change = Some("new");
            self.cap();
        }
        let p = self.data.projects.get_mut(name).unwrap();
        let was_ready = promote_due(p);
        if p.farewell > 0 {
            p.back = now_ms;
            p.farewell = 0;
            change = Some("back");
        }
        let d = day_of(now_ms);
        if p.last_day != d {
            p.last_day = d;
            p.days += 1;
        }
        p.last = p.last.max(now_ms);
        if turn {
            p.turns += 1;
        }
        if change.is_none() && !was_ready && promote_due(p) {
            change = Some("ready");
        }
        change
    }

    pub fn mark_official(&mut self, name: &str, now_ms: u64) -> bool {
        let Some(p) = self.data.projects.get_mut(name) else {
            return false;
        };
        if !promote_due(p) {
            return false;
        }
        p.official = now_ms;
        let _ = self.save();
        true
    }

    pub fn mark_farewell(&mut self, name: &str, days: u32, now_ms: u64, live: bool) -> bool {
        let Some(p) = self.data.projects.get_mut(name) else {
            return false;
        };
        if !farewell_due(p, days, now_ms, live) {
            return false;
        }
        p.farewell = now_ms;
        p.back = 0;
        let _ = self.save();
        true
    }

    pub fn list(&self, days: u32, now_ms: u64, live: &HashSet<String>) -> Vec<TenureView> {
        let mut out = Vec::new();
        for (name, p) in &self.data.projects {
            let due = farewell_due(p, days, now_ms, live.contains(name));
            let back = if p.back > 0 && now_ms.saturating_sub(p.back) < DAY_MS {
                p.back
            } else {
                0
            };
            let intern = is_intern(p);
            if due || p.farewell > 0 || back > 0 || intern || p.official > 0 {
                out.push(TenureView {
                    name: name.clone(),
                    last: p.last,
                    days: p.days,
                    turns: p.turns,
                    due,
                    farewell: p.farewell,
                    back,
                    intern,
                    ready: promote_due(p),
                    official: p.official,
                });
            }
        }
        out.sort_by(|a, b| b.last.cmp(&a.last));
        out.truncate(60);
        out
    }

    fn cap(&mut self) {
        if self.data.projects.len() <= MAX_PROJECTS {
            return;
        }
        let mut entries: Vec<(String, u64)> = self
            .data
            .projects
            .iter()
            .map(|(k, v)| (k.clone(), v.last))
            .collect();
        entries.sort_by(|a, b| b.1.cmp(&a.1));
        for (k, _) in entries.into_iter().skip(MAX_PROJECTS) {
            self.data.projects.remove(&k);
        }
    }

    pub fn save(&self) -> anyhow::Result<()> {
        write_json_atomic(&self.file, &self.data)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tenure_tracks_intern_promotion_and_farewell_cycle() {
        let dir = tempfile::tempdir().unwrap();
        let day1 = 1_793_091_600_000u64;
        let day2 = day1 + DAY_MS;
        let mut st = TenureStore::load(dir.path(), day1);
        assert_eq!(st.touch("proj-a", day1, true), Some("new"));
        for _ in 1..19 {
            st.touch("proj-a", day1, true);
        }
        assert_eq!(st.touch("proj-a", day2, true), Some("ready"));
        assert!(st.mark_official("proj-a", day2));
        assert!(!st.mark_official("proj-a", day2));

        let later = day2 + 31 * DAY_MS;
        assert!(st.mark_farewell("proj-a", 30, later, false));
        assert_eq!(st.touch("proj-a", later + 1000, false), Some("back"));
    }
}
