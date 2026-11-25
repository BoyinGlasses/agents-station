use anyhow::{Result, bail};
use serde::{Deserialize, Serialize};
use station_core::atomic::write_json_atomic;
use std::path::{Path, PathBuf};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(tag = "kind", content = "value", rename_all = "snake_case")]
pub enum LaunchTarget {
    Exe(PathBuf),
    Shortcut(PathBuf),
    MsixAumid {
        package_family: String,
        aumid: String,
    },
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct HarnessEntry {
    pub id: String,
    pub label: String,
    pub hook_agent: Option<String>,
    pub target: Option<LaunchTarget>,
    pub custom: bool,
    pub process_names: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
struct StoredCustomCatalog {
    custom: Vec<HarnessEntry>,
}

#[derive(Debug, Clone)]
pub struct HarnessCatalog {
    path: PathBuf,
    custom: Vec<HarnessEntry>,
}

impl HarnessCatalog {
    pub fn load_or_default(data_dir: &Path) -> Self {
        let path = data_dir.join("harnesses.json");
        let custom = std::fs::read_to_string(&path)
            .ok()
            .and_then(|s| serde_json::from_str::<StoredCustomCatalog>(&s).ok())
            .map(|c| c.custom)
            .unwrap_or_default();
        Self { path, custom }
    }

    pub fn custom_entries(&self) -> &[HarnessEntry] {
        &self.custom
    }

    pub fn all_entries(&self, builtins: Vec<HarnessEntry>) -> Vec<HarnessEntry> {
        let mut out = builtins;
        for c in &self.custom {
            if let Some(slot) = out.iter_mut().find(|b| b.id == c.id) {
                *slot = c.clone();
            } else {
                out.push(c.clone());
            }
        }
        out
    }

    pub fn upsert_custom(&mut self, mut entry: HarnessEntry) -> Result<()> {
        let id = entry.id.trim().to_ascii_lowercase();
        if id.is_empty() || !id.chars().all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_') {
            bail!("invalid harness id: {}", entry.id);
        }
        entry.id = id;
        entry.custom = true;
        if let Some(existing) = self.custom.iter_mut().find(|e| e.id == entry.id) {
            *existing = entry;
        } else {
            self.custom.push(entry);
        }
        self.save()
    }

    pub fn remove_custom(&mut self, id: &str) -> Result<bool> {
        let before = self.custom.len();
        self.custom.retain(|e| e.id != id);
        if self.custom.len() != before {
            self.save()?;
            Ok(true)
        } else {
            Ok(false)
        }
    }

    fn save(&self) -> Result<()> {
        let payload = StoredCustomCatalog {
            custom: self.custom.clone(),
        };
        write_json_atomic(&self.path, &payload)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn recovers_from_corrupted_json_and_persists_custom_apps_atomically() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(dir.path().join("harnesses.json"), "{corrupted-json").unwrap();

        let mut cat = HarnessCatalog::load_or_default(dir.path());
        assert!(cat.custom_entries().is_empty());

        cat.upsert_custom(HarnessEntry {
            id: "orca".to_owned(),
            label: "Orca".to_owned(),
            hook_agent: None,
            target: Some(LaunchTarget::Exe(PathBuf::from("C:/Apps/Orca.exe"))),
            custom: true,
            process_names: vec!["Orca.exe".to_owned()],
        })
        .unwrap();
        cat.upsert_custom(HarnessEntry {
            id: "code".to_owned(),
            label: "VS Code".to_owned(),
            hook_agent: None,
            target: Some(LaunchTarget::Exe(PathBuf::from("C:/Apps/Code.exe"))),
            custom: true,
            process_names: vec!["Code.exe".to_owned()],
        })
        .unwrap();

        let reloaded = HarnessCatalog::load_or_default(dir.path());
        assert_eq!(reloaded.custom_entries().len(), 2);
        assert_eq!(reloaded.custom_entries()[0].id, "orca");
        assert_eq!(reloaded.custom_entries()[1].id, "code");

        assert!(cat.remove_custom("orca").unwrap());
        let after_del = HarnessCatalog::load_or_default(dir.path());
        assert_eq!(after_del.custom_entries().len(), 1);
        assert_eq!(after_del.custom_entries()[0].id, "code");
    }
}
