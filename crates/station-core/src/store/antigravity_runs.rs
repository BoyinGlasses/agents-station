use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::{Path, PathBuf};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AntigravityRun {
    pub id: String,
    pub path: PathBuf,
    pub modified_ms: u64,
}

pub struct AntigravityWatcher {
    brain_dir: PathBuf,
    seen: HashMap<String, u64>,
}

impl AntigravityWatcher {
    pub fn new(brain_dir: PathBuf) -> Self {
        Self {
            brain_dir,
            seen: HashMap::new(),
        }
    }

    pub fn scan_changed(&mut self) -> Vec<AntigravityRun> {
        let Ok(entries) = std::fs::read_dir(&self.brain_dir) else {
            return Vec::new();
        };
        let mut changed = Vec::new();
        for e in entries.flatten().take(200) {
            let p = e.path();
            if !p.is_dir() {
                continue;
            }
            let id = e.file_name().to_string_lossy().into_owned();
            let modified_ms = e
                .metadata()
                .ok()
                .and_then(|m| m.modified().ok())
                .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
                .map(|d| d.as_millis() as u64)
                .unwrap_or(0);
            if self.seen.get(&id).copied() != Some(modified_ms) {
                self.seen.insert(id.clone(), modified_ms);
                changed.push(AntigravityRun {
                    id,
                    path: p,
                    modified_ms,
                });
            }
        }
        changed
    }

    pub fn brain_dir(&self) -> &Path {
        &self.brain_dir
    }
}
