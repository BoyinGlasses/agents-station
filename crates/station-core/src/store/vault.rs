use crate::atomic::write_json_atomic;
use crate::redact::redact;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::{Path, PathBuf};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VaultPrompt {
    pub t: String,
    pub ph: usize,
    pub at: u64,
    pub sid: String,
    pub title: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VaultEntry {
    pub sid: String,
    pub agent: String,
    pub title: String,
    pub cwd: String,
    pub updated_at: u64,
    pub prompts: Vec<VaultPrompt>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
struct VaultFile {
    entries: HashMap<String, VaultEntry>,
}

pub struct VaultStore {
    file: PathBuf,
    data: VaultFile,
}

impl VaultStore {
    pub fn load(data_dir: &Path) -> Self {
        let file = data_dir.join("vault.json");
        let data = std::fs::read_to_string(&file)
            .ok()
            .and_then(|s| serde_json::from_str::<VaultFile>(&s).ok())
            .unwrap_or_default();
        Self { file, data }
    }

    pub fn record_prompt(
        &mut self,
        sid: &str,
        agent: &str,
        title: &str,
        cwd: &str,
        text: &str,
        photos: usize,
        now_ms: u64,
    ) {
        let safe_text = redact(text);
        let safe_title = redact(title);
        let entry = self
            .data
            .entries
            .entry(sid.to_owned())
            .or_insert_with(|| VaultEntry {
                sid: sid.to_owned(),
                agent: agent.to_owned(),
                title: safe_title.clone(),
                cwd: cwd.to_owned(),
                updated_at: now_ms,
                prompts: Vec::new(),
            });
        entry.updated_at = now_ms;
        if !safe_title.is_empty() {
            entry.title = safe_title.clone();
        }
        entry.prompts.push(VaultPrompt {
            t: safe_text,
            ph: photos,
            at: now_ms,
            sid: sid.to_owned(),
            title: entry.title.clone(),
        });
        if entry.prompts.len() > 200 {
            let drain = entry.prompts.len() - 200;
            entry.prompts.drain(..drain);
        }
        if self.data.entries.len() > 500 {
            if let Some(oldest) = self
                .data
                .entries
                .iter()
                .min_by_key(|(_, e)| e.updated_at)
                .map(|(k, _)| k.clone())
            {
                self.data.entries.remove(&oldest);
            }
        }
    }

    pub fn search(&self, query: &str, limit: usize) -> Vec<&VaultEntry> {
        let q = query.trim().to_ascii_lowercase();
        let mut list: Vec<&VaultEntry> = self
            .data
            .entries
            .values()
            .filter(|e| {
                q.is_empty()
                    || e.title.to_ascii_lowercase().contains(&q)
                    || e.cwd.to_ascii_lowercase().contains(&q)
                    || e.prompts
                        .iter()
                        .any(|p| p.t.to_ascii_lowercase().contains(&q))
            })
            .collect();
        list.sort_by(|a, b| b.updated_at.cmp(&a.updated_at));
        list.truncate(limit);
        list
    }

    pub fn recent_prompts(&self, since_ms: u64) -> Vec<VaultPrompt> {
        let mut out = Vec::new();
        for e in self.data.entries.values() {
            for p in &e.prompts {
                if p.at >= since_ms {
                    out.push(p.clone());
                }
            }
        }
        out.sort_by_key(|p| p.at);
        out
    }

    pub fn flush(&self) -> anyhow::Result<()> {
        write_json_atomic(&self.file, &self.data)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn vault_redacts_secrets_searches_and_persists() {
        let dir = tempfile::tempdir().unwrap();
        let mut v = VaultStore::load(dir.path());
        v.record_prompt("s1", "claude", "API work", "/repo", "fix token sk-secret123456", 0, 1000);
        v.flush().unwrap();
        let loaded = VaultStore::load(dir.path());
        let found = loaded.search("api", 10);
        assert_eq!(found.len(), 1);
        assert!(!found[0].prompts[0].t.contains("sk-secret123456"));
    }
}
