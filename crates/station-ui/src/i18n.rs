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
        self.lookup_value(key).and_then(Value::as_str)
    }

    fn lookup_value(&self, key: &str) -> Option<&Value> {
        lookup_value_in(self.bundles.get(&self.lang), key)
            .or_else(|| lookup_value_in(self.bundles.get("en"), key))
    }

    fn device_label(&self, kind: &str) -> &str {
        let os = device_os_key();
        let path = format!("common.device.{kind}.{os}");
        if let Some(s) = self.lookup(&path) {
            return s;
        }
        match (os, kind) {
            ("win32", _) => "PC",
            ("darwin", _) => "Mac",
            (_, "Mac") => "Computer",
            _ => "computer",
        }
    }

    fn fill_template(&self, template: &str, vars: &[(&str, &str)]) -> String {
        if !template.contains('{') {
            return template.to_owned();
        }
        let mut out = String::with_capacity(template.len());
        let mut rest = template;
        while let Some(open) = rest.find('{') {
            out.push_str(&rest[..open]);
            let after_open = &rest[open + 1..];
            let Some(close) = after_open.find('}') else {
                out.push_str(&rest[open..]);
                return out;
            };
            let candidate = &after_open[..close];
            // If another `{` appears before `}`, emit up to that inner `{` and rescan.
            if let Some(inner_open) = candidate.find('{') {
                out.push('{');
                out.push_str(&candidate[..inner_open]);
                rest = &after_open[inner_open..];
                continue;
            }
            let is_word = !candidate.is_empty()
                && candidate
                    .bytes()
                    .all(|b| b.is_ascii_alphanumeric() || b == b'_');
            if is_word {
                if let Some((_, val)) = vars.iter().find(|(k, _)| *k == candidate) {
                    out.push_str(val);
                } else if candidate == "mac" {
                    out.push_str(self.device_label("mac"));
                } else if candidate == "Mac" {
                    out.push_str(self.device_label("Mac"));
                } else {
                    out.push_str(&rest[open..open + 1 + close + 1]);
                }
            } else {
                out.push_str(&rest[open..open + 1 + close + 1]);
            }
            rest = &after_open[close + 1..];
        }
        out.push_str(rest);
        out
    }

    pub fn tr(&self, key: &str) -> String {
        let raw = self
            .lookup_value(key)
            .and_then(|v| resolve_template(v, None))
            .unwrap_or(key);
        self.fill_template(raw, &[])
    }

    pub fn tr_vars(&self, key: &str, vars: &[(&str, &str)]) -> String {
        let count = vars
            .iter()
            .find(|(k, _)| *k == "n")
            .or_else(|| vars.iter().find(|(k, _)| *k == "c" || *k == "h"))
            .and_then(|(_, v)| v.trim().parse::<usize>().ok());
        let raw = self
            .lookup_value(key)
            .and_then(|v| resolve_template(v, count))
            .unwrap_or(key);
        self.fill_template(raw, vars)
    }

    pub fn tr_count(&self, key: &str, count: usize) -> String {
        let n_str = count.to_string();
        let raw = self
            .lookup_value(key)
            .and_then(|v| resolve_template(v, Some(count)))
            .unwrap_or(key);
        self.fill_template(raw, &[("n", n_str.as_str())])
    }

    pub fn tr_pick(&self, key: &str, seed: usize) -> String {
        let raw = match self.lookup_value(key) {
            Some(Value::Array(arr)) if !arr.is_empty() => {
                arr[seed % arr.len()].as_str().unwrap_or(key)
            }
            Some(v) => resolve_template(v, None).unwrap_or(key),
            None => key,
        };
        self.fill_template(raw, &[])
    }

    pub fn loaded_langs(&self) -> usize {
        self.bundles.len()
    }
}

fn device_os_key() -> &'static str {
    if cfg!(target_os = "windows") {
        "win32"
    } else if cfg!(target_os = "macos") {
        "darwin"
    } else {
        "linux"
    }
}

fn lookup_value_in<'a>(root: Option<&'a Value>, key: &str) -> Option<&'a Value> {
    let mut cur = root?;
    for part in key.split('.') {
        cur = cur.get(part)?;
    }
    Some(cur)
}

fn resolve_template(val: &Value, count: Option<usize>) -> Option<&str> {
    match val {
        Value::String(s) => Some(s.as_str()),
        Value::Object(map) => {
            let preferred = if count == Some(1) { "one" } else { "other" };
            map.get(preferred)
                .or_else(|| map.get("other"))
                .or_else(|| map.get("one"))
                .and_then(Value::as_str)
        }
        Value::Array(arr) => arr.first().and_then(Value::as_str),
        _ => None,
    }
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

    #[test]
    fn tr_count_and_tr_vars_resolve_plural_objects_and_plain_templates() {
        let assets = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../assets");
        let store = I18nStore::load_from_assets(&assets, "en");

        assert_eq!(store.tr_count("time.min", 1), "1 min");
        assert_eq!(store.tr_count("time.min", 5), "5 min");
        assert_eq!(store.tr_count("time.day", 1), "1 day");
        assert_eq!(store.tr_count("time.day", 5), "5 days");
        assert_eq!(store.tr_count("activity.sessions", 1), "1 session");
        assert_eq!(store.tr_count("activity.sessions", 3), "3 sessions");

        assert_eq!(store.tr_vars("time.day", &[("n", "1")]), "1 day");
        assert_eq!(store.tr_vars("time.day", &[("n", "5")]), "5 days");
        assert_eq!(store.tr_vars("time.min", &[("n", "5")]), "5 min");
    }

    #[test]
    fn tr_pick_resolves_array_keys_deterministically_by_seed() {
        let assets = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../assets");
        let store = I18nStore::load_from_assets(&assets, "en");

        let a0 = store.tr_pick("events.bubble.testok", 0);
        let a1 = store.tr_pick("events.bubble.testok", 1);
        let a2 = store.tr_pick("events.bubble.testok", 2);
        let a3 = store.tr_pick("events.bubble.testok", 3);
        assert_ne!(a0, "events.bubble.testok");
        assert_ne!(a0, a1);
        assert_ne!(a1, a2);
        assert_eq!(a0, a3, "seed wrap-around must be deterministic");

        let goal = store.tr_pick("people.foos.goal", 1);
        assert!(!goal.is_empty() && goal != "people.foos.goal");
        assert_eq!(store.tr_pick("nonexistent.array", 0), "nonexistent.array");
    }

    #[test]
    fn device_tokens_substitute_mac_and_cap_mac_across_helpers() {
        let assets = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../assets");
        let mut store = I18nStore::load_from_assets(&assets, "en");

        let top_mac = store.tr("ui.topBar.mac");
        let risky = store.tr("perm.riskyOnMac");
        let scan = store.tr_vars("topbar.phone.scan", &[]);

        assert!(!top_mac.contains("{Mac}") && !top_mac.contains("{mac}"));
        assert!(!risky.contains("{mac}") && !scan.contains("{mac}"));
        if cfg!(target_os = "windows") {
            assert_eq!(top_mac, "PC");
            assert!(risky.contains("PC"), "expected PC in '{risky}'");
            assert!(scan.contains("PC"), "expected PC in '{scan}'");
        }

        // Caller-supplied vars override automatic device substitution.
        assert_eq!(
            store.tr_vars("ui.topBar.mac", &[("Mac", "Workstation")]),
            "Workstation"
        );

        // Thai and Vietnamese also resolve without leaking placeholders.
        store.set_lang("th");
        let th_mac = store.tr("ui.topBar.mac");
        assert!(!th_mac.contains("{Mac}"));
        if cfg!(target_os = "windows") {
            assert_eq!(th_mac, "PC");
        }

        store.set_lang("vi");
        let vi_mac = store.tr("ui.topBar.mac");
        let vi_risky = store.tr("perm.riskyOnMac");
        assert!(!vi_mac.contains("{Mac}") && !vi_risky.contains("{mac}"));
    }

    #[test]
    fn switching_en_vi_th_resolves_ui_keys_without_raw_key_leaks() {
        let assets = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../assets");
        let mut store = I18nStore::load_from_assets(&assets, "en");
        let keys = [
            "ui.sessionRail.title",
            "ui.activityPanel.title",
            "ui.topBar.settings",
            "ui.topBar.replay",
            "ui.topBar.mac",
            "ui.cameraToolbar.home",
            "board.title",
            "station.brand",
            "station.apps",
            "station.addApp",
            "station.status.running",
            "station.gpu",
            "station.tray",
        ];

        for lang in ["en", "vi", "th"] {
            store.set_lang(lang);
            for key in keys {
                let val = store.tr(key);
                assert!(
                    !val.is_empty() && val != key,
                    "lang={lang} leaked raw key '{key}' -> '{val}'"
                );
                assert!(
                    !val.contains("{mac}") && !val.contains("{Mac}"),
                    "lang={lang} leaked device token in '{key}' -> '{val}'"
                );
            }
            let sessions = store.tr_count("activity.sessions", 2);
            assert!(
                sessions.contains('2') && !sessions.contains("{n}") && sessions != "activity.sessions",
                "lang={lang} bad tr_count(activity.sessions, 2) -> '{sessions}'"
            );
            let picked = store.tr_pick("events.bubble.testok", 0);
            assert!(
                !picked.is_empty() && picked != "events.bubble.testok",
                "lang={lang} bad tr_pick(events.bubble.testok) -> '{picked}'"
            );
        }
    }

    #[test]
    fn tr_vars_uses_single_pass_without_double_substitution() {
        let assets = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../assets");
        let store = I18nStore::load_from_assets(&assets, "en");

        // Caller value containing {mac}/{Mac} must be preserved literally, not re-substituted.
        let prompt = store.tr_vars("events.verb.prompt", &[("label", "echo {mac} {Mac}")]);
        assert!(
            prompt.contains("echo {mac} {Mac}"),
            "expected literal {{mac}}/{{Mac}} in '{prompt}'"
        );

        // Caller value containing a later variable placeholder must not be re-expanded.
        let tokens = store.tr_vars(
            "drawer.tokens",
            &[("used", "{max}"), ("max", "200k"), ("model", "opus")],
        );
        assert_eq!(tokens, "{max} / 200k tokens · opus");

        // Unknown placeholders remain verbatim.
        let partial = store.tr_vars("drawer.tokens", &[("used", "10k")]);
        assert_eq!(partial, "10k / {max} tokens · {model}");
    }
}
