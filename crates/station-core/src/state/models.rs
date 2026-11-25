use regex::Regex;
use serde::{Deserialize, Serialize};
use serde_json::{Map, Value};
use std::sync::OnceLock;

pub const MAIN_MODELS: [&str; 3] = ["opus", "sonnet", ""];
pub const SUB_MODELS: [&str; 4] = ["", "sonnet", "haiku", "opus"];
const SUB_KEY: &str = "CLAUDE_CODE_SUBAGENT_MODEL";
const FORCE_KEY: &str = "CLAUDE_CODE_SUBAGENT_MODEL_FORCE";
const OPEN_MARK: &str = "<!-- my-office:subagents -->";
const CLOSE_MARK: &str = "<!-- /my-office:subagents -->";

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ModelConfigView {
    pub main: String,
    pub sub: String,
    pub force: bool,
}

pub fn models_of(settings: &Value) -> ModelConfigView {
    let main = settings
        .get("model")
        .and_then(Value::as_str)
        .unwrap_or("")
        .to_owned();
    let env = settings.get("env").and_then(Value::as_object);
    let sub_raw = env
        .and_then(|e| e.get(SUB_KEY))
        .and_then(Value::as_str)
        .unwrap_or("");
    let sub = if sub_raw == "inherit" { "" } else { sub_raw }.to_owned();
    let force_val = env
        .and_then(|e| e.get(FORCE_KEY))
        .and_then(Value::as_str)
        .unwrap_or("");
    let force = !sub.is_empty()
        && (force_val == "1" || force_val.eq_ignore_ascii_case("true"));
    ModelConfigView { main, sub, force }
}

pub fn with_models(
    settings: &Value,
    main: Option<&str>,
    sub: Option<&str>,
    force: Option<bool>,
) -> Value {
    let mut out = settings.as_object().cloned().unwrap_or_default();
    let mut env = out
        .get("env")
        .and_then(Value::as_object)
        .cloned()
        .unwrap_or_default();

    if let Some(m) = main {
        if m.is_empty() {
            out.remove("model");
        } else {
            out.insert("model".to_owned(), Value::String(m.to_owned()));
        }
    }
    if let Some(s) = sub {
        if s.is_empty() {
            env.remove(SUB_KEY);
        } else {
            env.insert(SUB_KEY.to_owned(), Value::String(s.to_owned()));
        }
    }
    if force.is_some() || sub.is_some() {
        let eff_force = force.unwrap_or_else(|| models_of(settings).force);
        if eff_force && env.contains_key(SUB_KEY) {
            env.insert(FORCE_KEY.to_owned(), Value::String("1".to_owned()));
        } else {
            env.remove(FORCE_KEY);
        }
    }
    if env.is_empty() {
        out.remove("env");
    } else {
        out.insert("env".to_owned(), Value::Object(env));
    }
    Value::Object(out)
}

pub fn has_split(md: &str) -> bool {
    md.contains(OPEN_MARK)
}

pub fn with_split(md: &str, on: bool, note: &str) -> String {
    static BLOCK_RE: OnceLock<Regex> = OnceLock::new();
    let re = BLOCK_RE.get_or_init(|| {
        Regex::new(r"(?s)\n*<!-- my-office:subagents -->.*?<!-- /my-office:subagents -->\n?")
            .unwrap()
    });
    let stripped = re.replace_all(md, "\n");
    let trimmed = stripped.trim_end_matches('\n');
    if !on {
        if trimmed.trim().is_empty() {
            return String::new();
        }
        return format!("{trimmed}\n");
    }
    let body = format!("{OPEN_MARK}\n{}\n{CLOSE_MARK}\n", note.trim());
    if trimmed.trim().is_empty() {
        body
    } else {
        format!("{trimmed}\n\n{body}")
    }
}

pub fn default_claude_models() -> Vec<Value> {
    ["opus", "sonnet", "haiku"]
        .into_iter()
        .map(|id| {
            let mut m = Map::new();
            m.insert("id".to_owned(), Value::String(id.to_owned()));
            let mut chars = id.chars();
            let name = match chars.next() {
                Some(c) => format!("{}{}", c.to_uppercase(), chars.as_str()),
                None => String::new(),
            };
            m.insert("name".to_owned(), Value::String(name));
            Value::Object(m)
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn model_settings_and_claude_md_split_round_trip() {
        let base = json!({ "other": 1 });
        let updated = with_models(&base, Some("opus"), Some("sonnet"), Some(true));
        let view = models_of(&updated);
        assert_eq!(view, ModelConfigView { main: "opus".into(), sub: "sonnet".into(), force: true });
        assert_eq!(updated["other"], 1);

        let cleared = with_models(&updated, Some(""), Some(""), None);
        assert!(cleared.get("model").is_none());
        assert!(cleared.get("env").is_none());

        let md = with_split("# My notes\n", true, "Use subagents.");
        assert!(has_split(&md));
        let removed = with_split(&md, false, "Use subagents.");
        assert!(!has_split(&removed));
        assert_eq!(removed, "# My notes\n");
    }
}
