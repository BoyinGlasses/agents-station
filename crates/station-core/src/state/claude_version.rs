use crate::atomic::write_json_atomic;
use regex::Regex;
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};
use std::sync::OnceLock;

const EDITORS: [&str; 9] = [
    ".vscode",
    ".vscode-insiders",
    ".cursor",
    ".windsurf",
    ".antigravity",
    ".antigravity-ide",
    ".kiro",
    ".trae",
    ".positron",
];

fn semver_parts(v: &str) -> [u32; 3] {
    let mut it = v.split('.').map(|s| s.parse::<u32>().unwrap_or(0));
    [
        it.next().unwrap_or(0),
        it.next().unwrap_or(0),
        it.next().unwrap_or(0),
    ]
}

pub fn newer_semver(a: &str, b: &str) -> bool {
    semver_parts(a) > semver_parts(b)
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct ClaudeVersionState {
    pub version: Option<String>,
    pub prev: Option<String>,
    pub at: u64,
}

pub struct ClaudeVersionWatcher {
    file: PathBuf,
    home: PathBuf,
    state: ClaudeVersionState,
}

impl ClaudeVersionWatcher {
    pub fn load(data_dir: &Path, home: PathBuf) -> Self {
        let file = data_dir.join("claude-version.json");
        let state = std::fs::read_to_string(&file)
            .ok()
            .and_then(|s| serde_json::from_str::<ClaudeVersionState>(&s).ok())
            .unwrap_or_default();
        Self { file, home, state }
    }

    pub fn scan(&self) -> Option<String> {
        static RE: OnceLock<Regex> = OnceLock::new();
        let re = RE.get_or_init(|| Regex::new(r"^anthropic\.claude-code-(\d+\.\d+\.\d+)").unwrap());
        let mut best: Option<String> = None;
        for ed in EDITORS {
            let ext_dir = self.home.join(ed).join("extensions");
            let Ok(entries) = std::fs::read_dir(ext_dir) else {
                continue;
            };
            for entry in entries.flatten() {
                let name = entry.file_name();
                let name_str = name.to_string_lossy();
                if let Some(caps) = re.captures(&name_str) {
                    let v = caps[1].to_owned();
                    if best.as_deref().is_none_or(|b| newer_semver(&v, b)) {
                        best = Some(v);
                    }
                }
            }
        }
        best
    }

    pub fn check(&mut self, now_ms: u64) -> &ClaudeVersionState {
        if let Some(v) = self.scan() {
            match &self.state.version {
                None => {
                    self.state = ClaudeVersionState {
                        version: Some(v),
                        prev: None,
                        at: 0,
                    };
                    let _ = write_json_atomic(&self.file, &self.state);
                }
                Some(cur) if newer_semver(&v, cur) => {
                    self.state = ClaudeVersionState {
                        prev: Some(cur.clone()),
                        version: Some(v),
                        at: now_ms,
                    };
                    let _ = write_json_atomic(&self.file, &self.state);
                }
                _ => {}
            }
        }
        &self.state
    }

    pub fn current(&self) -> &ClaudeVersionState {
        &self.state
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn detects_claude_extension_upgrade_and_persists() {
        let dir = tempfile::tempdir().unwrap();
        let home = dir.path().join("home");
        std::fs::create_dir_all(home.join(".vscode/extensions/anthropic.claude-code-2.1.10-win32"))
            .unwrap();
        let mut w = ClaudeVersionWatcher::load(dir.path(), home.clone());
        let s1 = w.check(1000).clone();
        assert_eq!(s1.version.as_deref(), Some("2.1.10"));
        assert_eq!(s1.at, 0);

        std::fs::create_dir_all(home.join(".antigravity/extensions/anthropic.claude-code-2.2.0-win32"))
            .unwrap();
        let s2 = w.check(5000).clone();
        assert_eq!(s2.version.as_deref(), Some("2.2.0"));
        assert_eq!(s2.prev.as_deref(), Some("2.1.10"));
        assert_eq!(s2.at, 5000);
    }
}
