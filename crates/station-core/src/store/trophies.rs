use crate::atomic::write_json_atomic;
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};

#[derive(Debug, Clone, Copy, Serialize)]
pub struct MilestoneDef {
    pub id: &'static str,
    pub k: &'static str,
    pub n: u64,
    pub kind: &'static str,
    pub tier: u8,
}

pub const MILESTONES: [MilestoneDef; 22] = [
    MilestoneDef { id: "open", k: "open", n: 1, kind: "plaque", tier: 3 },
    MilestoneDef { id: "p100", k: "prompts", n: 100, kind: "cup", tier: 1 },
    MilestoneDef { id: "p1k", k: "prompts", n: 1000, kind: "cup", tier: 2 },
    MilestoneDef { id: "p10k", k: "prompts", n: 10000, kind: "cup", tier: 3 },
    MilestoneDef { id: "e1k", k: "edit", n: 1000, kind: "cup", tier: 1 },
    MilestoneDef { id: "e10k", k: "edit", n: 10000, kind: "cup", tier: 2 },
    MilestoneDef { id: "e100k", k: "edit", n: 100000, kind: "cup", tier: 3 },
    MilestoneDef { id: "c1k", k: "cmd", n: 1000, kind: "star", tier: 1 },
    MilestoneDef { id: "c10k", k: "cmd", n: 10000, kind: "star", tier: 2 },
    MilestoneDef { id: "c100k", k: "cmd", n: 100000, kind: "star", tier: 3 },
    MilestoneDef { id: "r1k", k: "read", n: 1000, kind: "book", tier: 1 },
    MilestoneDef { id: "r10k", k: "read", n: 10000, kind: "book", tier: 2 },
    MilestoneDef { id: "r100k", k: "read", n: 100000, kind: "book", tier: 3 },
    MilestoneDef { id: "d1", k: "deploy", n: 1, kind: "rocket", tier: 1 },
    MilestoneDef { id: "d10", k: "deploy", n: 10, kind: "rocket", tier: 2 },
    MilestoneDef { id: "d100", k: "deploy", n: 100, kind: "rocket", tier: 3 },
    MilestoneDef { id: "s10", k: "sessions", n: 10, kind: "medal", tier: 1 },
    MilestoneDef { id: "s100", k: "sessions", n: 100, kind: "medal", tier: 2 },
    MilestoneDef { id: "s1k", k: "sessions", n: 1000, kind: "medal", tier: 3 },
    MilestoneDef { id: "days7", k: "days", n: 7, kind: "medal", tier: 1 },
    MilestoneDef { id: "days30", k: "days", n: 30, kind: "medal", tier: 2 },
    MilestoneDef { id: "days100", k: "days", n: 100, kind: "medal", tier: 3 },
];

pub const KEYS: [&str; 8] = [
    "prompts", "edit", "read", "cmd", "web", "deploy", "sessions", "days",
];

pub fn fnv1a_32(s: &str) -> u32 {
    let mut h: u32 = 2166136261;
    for b in s.bytes() {
        h ^= b as u32;
        h = h.wrapping_mul(16777619);
    }
    h
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct TotalsFile {
    since: u64,
    live: HashMap<String, u64>,
    hist: Option<HashMap<String, u64>>,
    #[serde(rename = "lastDay", default)]
    last_day: String,
    #[serde(default)]
    sids: Vec<String>,
    #[serde(default)]
    earned: HashMap<String, u64>,
}

pub struct TrophyStore {
    file: PathBuf,
    data: TotalsFile,
    dedupe: HashSet<u32>,
    dirty: bool,
}

impl TrophyStore {
    pub fn load(data_dir: &Path, now_ms: u64) -> Self {
        let file = data_dir.join("totals.json");
        let existed = file.exists();
        let mut data = std::fs::read_to_string(&file)
            .ok()
            .and_then(|s| serde_json::from_str::<TotalsFile>(&s).ok())
            .unwrap_or_else(|| TotalsFile {
                since: now_ms,
                live: HashMap::new(),
                hist: None,
                last_day: String::new(),
                sids: Vec::new(),
                earned: HashMap::new(),
            });
        for k in KEYS {
            data.live.entry(k.to_owned()).or_insert(0);
        }
        let mut store = Self {
            file,
            data,
            dedupe: HashSet::new(),
            dirty: !existed,
        };
        store.check(now_ms);
        store
    }

    pub fn total(&self, k: &str) -> u64 {
        if k == "open" {
            return 1;
        }
        let live = self.data.live.get(k).copied().unwrap_or(0);
        let hist = self
            .data
            .hist
            .as_ref()
            .and_then(|h| h.get(k).copied())
            .unwrap_or(0);
        live + hist
    }

    fn check(&mut self, now_ms: u64) -> Vec<&'static str> {
        let mut fresh = Vec::new();
        for m in MILESTONES {
            if !self.data.earned.contains_key(m.id) && self.total(m.k) >= m.n {
                let ts = if m.id == "open" { self.data.since } else { now_ms };
                self.data.earned.insert(m.id.to_owned(), ts);
                fresh.push(m.id);
                self.dirty = true;
            }
        }
        fresh
    }

    pub fn bump(&mut self, key: &str, n: u64, today: &str, now_ms: u64) -> Vec<&'static str> {
        *self.data.live.entry(key.to_owned()).or_insert(0) += n;
        self.dirty = true;
        if self.data.last_day != today {
            self.data.last_day = today.to_owned();
            *self.data.live.entry("days".to_owned()).or_insert(0) += 1;
        }
        self.check(now_ms)
    }

    pub fn bump_deduped(&mut self, key: &str, dedupe_id: &str, today: &str, now_ms: u64) -> Vec<&'static str> {
        let h = fnv1a_32(dedupe_id);
        if !self.dedupe.insert(h) {
            return Vec::new();
        }
        if self.dedupe.len() > 20_000 {
            self.dedupe.clear();
        }
        self.bump(key, 1, today, now_ms)
    }

    pub fn session(&mut self, sid: &str, today: &str, now_ms: u64) -> Vec<&'static str> {
        if sid.is_empty() || self.data.sids.iter().any(|s| s == sid) {
            return Vec::new();
        }
        self.data.sids.push(sid.to_owned());
        if self.data.sids.len() > 2000 {
            let drain = self.data.sids.len() - 2000;
            self.data.sids.drain(..drain);
        }
        self.bump("sessions", 1, today, now_ms)
    }

    pub fn pub_view(&self) -> Value {
        let mut totals = serde_json::Map::new();
        for k in KEYS {
            totals.insert(k.to_owned(), json!(self.total(k)));
        }
        json!({
            "since": self.data.since,
            "scanned": self.data.hist.is_some(),
            "totals": totals,
            "earned": self.data.earned,
            "defs": MILESTONES,
        })
    }

    pub fn flush(&mut self) -> anyhow::Result<()> {
        if !self.dirty {
            return Ok(());
        }
        write_json_atomic(&self.file, &self.data)?;
        self.dirty = false;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn trophy_store_earns_milestones_dedupes_and_persists_atomically() {
        let dir = tempfile::tempdir().unwrap();
        let mut st = TrophyStore::load(dir.path(), 1000);
        assert!(st.data.earned.contains_key("open"));

        let earned_d1 = st.bump("deploy", 1, "2026-11-25", 2000);
        assert!(earned_d1.contains(&"d1"));

        let earned_p = st.bump("prompts", 100, "2026-11-25", 3000);
        assert!(earned_p.contains(&"p100"));

        assert!(!st.bump_deduped("edit", "tool_1", "2026-11-25", 3100).contains(&"e1k"));
        let before = st.total("edit");
        st.bump_deduped("edit", "tool_1", "2026-11-25", 3200);
        assert_eq!(st.total("edit"), before);

        st.flush().unwrap();
        let reloaded = TrophyStore::load(dir.path(), 4000);
        assert!(reloaded.data.earned.contains_key("d1"));
        assert!(reloaded.data.earned.contains_key("p100"));
    }
}
