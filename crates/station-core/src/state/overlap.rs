use regex::Regex;
use serde_json::Value;
use std::collections::{HashMap, VecDeque};
use std::path::{Path, PathBuf};
use std::sync::OnceLock;

pub const OVERLAP_WINDOW_MS: u64 = 20 * 60 * 1000;

pub fn is_edit_tool(name: &str) -> bool {
    matches!(name, "Edit" | "Write" | "MultiEdit" | "NotebookEdit")
}

pub fn file_of(input: &Value, cwd: &str) -> String {
    let f = input
        .get("file_path")
        .or_else(|| input.get("notebook_path"))
        .and_then(Value::as_str)
        .unwrap_or("");
    if f.is_empty() {
        return String::new();
    }
    let p = Path::new(f);
    if p.is_absolute() {
        normalize_path(p)
    } else if !cwd.is_empty() {
        normalize_path(&Path::new(cwd).join(p))
    } else {
        String::new()
    }
}

fn normalize_path(p: &Path) -> String {
    let mut out = PathBuf::new();
    for comp in p.components() {
        match comp {
            std::path::Component::CurDir => {}
            std::path::Component::ParentDir => {
                out.pop();
            }
            other => out.push(other.as_os_str()),
        }
    }
    out.to_string_lossy().into_owned()
}

pub fn port_in_use(text: &str) -> Option<u16> {
    static PATS: OnceLock<Vec<Regex>> = OnceLock::new();
    let pats = PATS.get_or_init(|| {
        vec![
            Regex::new(r"EADDRINUSE[^\n]*?:(\d{2,5})\b").unwrap(),
            Regex::new(r"(?i)address already in use[^\n]*?:(\d{2,5})\b").unwrap(),
            Regex::new(r"(?i)\bport (\d{2,5}) is (?:already )?in use\b").unwrap(),
            Regex::new(r"(?i)\b(\d{2,5}) is already in use\b").unwrap(),
        ]
    });
    let clipped: &str = if text.len() > 20_000 {
        &text[..text.floor_char_boundary(20_000)]
    } else {
        text
    };
    for re in pats {
        if let Some(caps) = re.captures(clipped)
            && let Ok(n) = caps[1].parse::<u32>()
            && (1..65536).contains(&n)
        {
            return Some(n as u16);
        }
    }
    None
}

#[derive(Debug, Clone)]
pub struct OverlapWarning {
    pub other: String,
    pub at: u64,
}

pub struct OverlapTracker {
    window_ms: u64,
    edits: HashMap<String, (String, u64)>,
    edit_order: VecDeque<String>,
    told: HashMap<String, u64>,
    notes: HashMap<String, Vec<String>>,
}

impl Default for OverlapTracker {
    fn default() -> Self {
        Self::new(OVERLAP_WINDOW_MS)
    }
}

impl OverlapTracker {
    pub fn new(window_ms: u64) -> Self {
        Self {
            window_ms,
            edits: HashMap::new(),
            edit_order: VecDeque::new(),
            told: HashMap::new(),
            notes: HashMap::new(),
        }
    }

    fn prune(&mut self, now_ms: u64) {
        let w = self.window_ms;
        self.edits.retain(|_, (_, at)| now_ms.saturating_sub(*at) <= w);
        self.told.retain(|_, at| now_ms.saturating_sub(*at) <= w);
        while self.edits.len() > 2000 {
            if let Some(old) = self.edit_order.pop_front() {
                self.edits.remove(&old);
            } else {
                break;
            }
        }
    }

    pub fn edited(&mut self, sid: &str, file: &str, now_ms: u64) {
        if sid.is_empty() || file.is_empty() {
            return;
        }
        self.edits.insert(file.to_owned(), (sid.to_owned(), now_ms));
        self.edit_order.retain(|f| f != file);
        self.edit_order.push_back(file.to_owned());
        self.prune(now_ms);
    }

    pub fn before(&mut self, sid: &str, file: &str, now_ms: u64) -> Option<OverlapWarning> {
        if sid.is_empty() || file.is_empty() {
            return None;
        }
        self.prune(now_ms);
        let (other, at) = self.edits.get(file)?.clone();
        if other == sid {
            return None;
        }
        let k = format!("{sid}|{file}|{other}");
        if self.told.contains_key(&k) {
            return None;
        }
        self.told.insert(k, now_ms);
        Some(OverlapWarning { other, at })
    }

    pub fn port_once(&mut self, sid: &str, port: u16, now_ms: u64) -> bool {
        self.prune(now_ms);
        let k = format!("port|{sid}|{port}");
        if self.told.contains_key(&k) {
            return false;
        }
        self.told.insert(k, now_ms);
        true
    }

    pub fn note(&mut self, sid: &str, text: &str) {
        if sid.is_empty() || text.is_empty() {
            return;
        }
        let list = self.notes.entry(sid.to_owned()).or_default();
        if !list.iter().any(|x| x == text) {
            list.push(text.to_owned());
        }
        if list.len() > 3 {
            let drain = list.len() - 3;
            list.drain(..drain);
        }
    }

    pub fn take(&mut self, sid: &str) -> String {
        self.notes
            .remove(sid)
            .map(|l| l.join("\n\n"))
            .unwrap_or_default()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn detects_file_overlap_and_port_in_use_once_per_window() {
        let mut ov = OverlapTracker::default();
        ov.edited("s1", "/app/a.rs", 1000);
        assert!(ov.before("s1", "/app/a.rs", 2000).is_none());
        let w = ov.before("s2", "/app/a.rs", 2000).unwrap();
        assert_eq!(w.other, "s1");
        assert!(ov.before("s2", "/app/a.rs", 3000).is_none());

        assert_eq!(port_in_use("Error: listen EADDRINUSE: address already in use :::3000"), Some(3000));
        assert_eq!(port_in_use("Port 5173 is already in use"), Some(5173));
        assert_eq!(port_in_use("all good"), None);

        assert!(ov.port_once("s2", 3000, 1000));
        assert!(!ov.port_once("s2", 3000, 2000));
    }
}
