use crate::atomic::write_json_atomic;
use anyhow::Result;
use serde_json::{Map, Value};
use std::path::{Path, PathBuf};

pub const AUTO_CLOSE_HOURS: [u8; 3] = [2, 4, 8];
pub const SUPPORTED_LANGS: [&str; 3] = ["en", "vi", "th"];

pub fn pick_lang(raw: Option<&str>) -> &'static str {
    let Some(val) = raw else {
        return "en";
    };
    let lower = val.trim().to_ascii_lowercase();
    for lang in SUPPORTED_LANGS {
        if lower == lang || lower.starts_with(&format!("{lang}-")) || lower.starts_with(&format!("{lang}_")) {
            return lang;
        }
    }
    "en"
}

#[derive(Debug, Clone)]
pub struct PrefsStore {
    file: PathBuf,
    data: Map<String, Value>,
    env_lang: Option<String>,
}

impl PrefsStore {
    pub fn load(data_dir: &Path) -> Self {
        Self::load_with_env(data_dir, std::env::var("OFFICE_LANG").ok())
    }

    pub fn load_with_env(data_dir: &Path, env_lang: Option<String>) -> Self {
        let file = data_dir.join("prefs.json");
        let data = std::fs::read_to_string(&file)
            .ok()
            .and_then(|s| serde_json::from_str::<Value>(&s).ok())
            .and_then(|v| v.as_object().cloned())
            .unwrap_or_default();
        Self {
            file,
            data,
            env_lang,
        }
    }

    pub fn lang(&self) -> &'static str {
        let from_file = self.data.get("lang").and_then(Value::as_str);
        pick_lang(from_file.or(self.env_lang.as_deref()))
    }

    pub fn set_lang(&mut self, l: &str) -> Result<bool> {
        if !SUPPORTED_LANGS.contains(&l) {
            return Ok(false);
        }
        self.data.insert("lang".to_owned(), Value::String(l.to_owned()));
        self.save()?;
        Ok(true)
    }

    pub fn auto_close(&self) -> u8 {
        self.data
            .get("autoCloseHours")
            .and_then(Value::as_u64)
            .and_then(|v| u8::try_from(v).ok())
            .filter(|h| AUTO_CLOSE_HOURS.contains(h))
            .unwrap_or(0)
    }

    pub fn set_auto_close(&mut self, h: u8) -> Result<bool> {
        if h != 0 && !AUTO_CLOSE_HOURS.contains(&h) {
            return Ok(false);
        }
        self.data
            .insert("autoCloseHours".to_owned(), Value::Number(h.into()));
        self.save()?;
        Ok(true)
    }

    pub fn get(&self, key: &str) -> Option<&Value> {
        self.data.get(key)
    }

    pub fn set(&mut self, key: &str, value: Option<Value>) -> Result<()> {
        match value {
            Some(v) => {
                self.data.insert(key.to_owned(), v);
            }
            None => {
                self.data.remove(key);
            }
        }
        self.save()
    }

    pub fn snapshot(&self) -> Value {
        Value::Object(self.data.clone())
    }

    pub fn save(&self) -> Result<()> {
        write_json_atomic(&self.file, &self.data)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn recovers_from_corrupted_prefs_and_persists_valid_settings() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(dir.path().join("prefs.json"), "{broken json").unwrap();

        let mut store = PrefsStore::load_with_env(dir.path(), None);
        assert_eq!(store.lang(), "en");
        assert_eq!(store.auto_close(), 0);

        assert!(!store.set_auto_close(3).unwrap());
        assert!(store.set_auto_close(4).unwrap());
        assert!(!store.set_lang("fr").unwrap());
        assert!(store.set_lang("vi").unwrap());
        store
            .set("phoneApprove", Some(Value::Bool(true)))
            .unwrap();

        let reloaded = PrefsStore::load_with_env(dir.path(), None);
        assert_eq!(reloaded.lang(), "vi");
        assert_eq!(reloaded.auto_close(), 4);
        assert_eq!(reloaded.get("phoneApprove"), Some(&Value::Bool(true)));
    }
}
