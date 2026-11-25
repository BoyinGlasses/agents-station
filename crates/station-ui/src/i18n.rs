use bevy::prelude::*;
use serde_json::Value;
use std::collections::HashMap;
use std::path::Path;

#[derive(Resource, Debug, Clone)]
pub struct I18nStore {
    pub lang: String,
    bundles: HashMap<String, Value>,
}

impl Default for I18nStore {
    fn default() -> Self {
        Self {
            lang: "en".to_owned(),
            bundles: HashMap::new(),
        }
    }
}

impl I18nStore {
    pub fn load_from_assets(assets_dir: &Path, initial_lang: &str) -> Self {
        let mut bundles = HashMap::new();
        for code in ["en", "vi", "th"] {
            let p = assets_dir.join("i18n").join(format!("{code}.json"));
            if let Ok(text) = std::fs::read_to_string(&p)
                && let Ok(val) = serde_json::from_str::<Value>(&text)
            {
                bundles.insert(code.to_owned(), val);
            }
        }
        let lang = if matches!(initial_lang, "en" | "vi" | "th") {
            initial_lang.to_owned()
        } else {
            "en".to_owned()
        };
        Self { lang, bundles }
    }

    pub fn set_lang(&mut self, code: &str) {
        if matches!(code, "en" | "vi" | "th") {
            self.lang = code.to_owned();
        }
    }

    pub fn lookup(&self, key: &str) -> Option<&str> {
        lookup_in(self.bundles.get(&self.lang), key)
            .or_else(|| lookup_in(self.bundles.get("en"), key))
    }

    pub fn tr(&self, key: &str) -> String {
        self.lookup(key).unwrap_or(key).to_owned()
    }

    pub fn tr_vars(&self, key: &str, vars: &[(&str, &str)]) -> String {
        let mut out = self.tr(key);
        for (k, v) in vars {
            out = out.replace(&format!("{{{k}}}"), v);
        }
        out
    }

    pub fn loaded_langs(&self) -> usize {
        self.bundles.len()
    }
}

fn lookup_in<'a>(root: Option<&'a Value>, key: &str) -> Option<&'a str> {
    let mut cur = root?;
    for part in key.split('.') {
        cur = cur.get(part)?;
    }
    cur.as_str()
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    #[test]
    fn loads_all_three_compiled_locales_and_resolves_nested_keys_and_interpolation() {
        let assets = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../assets");
        let mut store = I18nStore::load_from_assets(&assets, "vi");
        assert_eq!(store.loaded_langs(), 3);

        let vi_title = store.tr("board.title");
        assert!(!vi_title.is_empty() && vi_title != "board.title");

        store.set_lang("th");
        let th_title = store.tr("board.title");
        assert!(!th_title.is_empty() && th_title != "board.title");

        store.set_lang("en");
        let en_title = store.tr("board.title");
        assert!(!en_title.is_empty() && en_title != "board.title");

        let interp = store.tr_vars("time.min", &[("n", "5")]);
        assert!(interp.contains('5'));
        assert_eq!(store.tr("nonexistent.deep.key"), "nonexistent.deep.key");
    }
}
