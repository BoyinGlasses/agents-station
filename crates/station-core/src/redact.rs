use regex::Regex;
use serde_json::Value;
use std::sync::OnceLock;

static REDACT_RULES: OnceLock<Vec<(Regex, &'static str)>> = OnceLock::new();

fn rules() -> &'static [(Regex, &'static str)] {
    REDACT_RULES.get_or_init(|| {
        vec![
            (Regex::new(r#"(?i)(\bbearer\s+)[^\s"']+"#).unwrap(), "${1}•••"),
            (Regex::new(r"\b(?:sk|pk|rk)-[A-Za-z0-9_-]{8,}").unwrap(), "•••"),
            (Regex::new(r"\bgh[pousr]_[A-Za-z0-9]{10,}").unwrap(), "•••"),
            (Regex::new(r"\b(?:AKIA|ASIA)[0-9A-Z]{16}\b").unwrap(), "•••"),
            (Regex::new(r"\bxox[abpr]-[A-Za-z0-9-]{10,}").unwrap(), "•••"),
            (Regex::new(r"\beyJ[\w-]{8,}\.[\w-]{8,}\.[\w-]{4,}").unwrap(), "•••"),
            (
                Regex::new(
                    r#"(?i)(\b(?:api[_-]?key|access[_-]?key|token|secret|password|passwd|pwd|authorization|cookie)\b["']?\s*[:=]\s*["']?)[^\s"'&]+"#,
                )
                .unwrap(),
                "${1}•••",
            ),
            (Regex::new(r"(//)[^\s/:@]+:[^\s/@]+@").unwrap(), "${1}•••@"),
            (Regex::new(r"\b(?:\d{1,3}\.){3}\d{1,3}\b").unwrap(), "•••"),
            (Regex::new(r"[A-Za-z0-9+_-]{32,}={0,2}").unwrap(), "•••"),
        ]
    })
}

pub fn redact(input: &str) -> String {
    let mut out = input.to_owned();
    for (re, rep) in rules() {
        out = re.replace_all(&out, *rep).into_owned();
    }
    out
}

pub const TOOL_INPUT_WIRE_MAX_STR_CHARS: usize = 16_000;
const MAX_JSON_ARRAY_ITEMS: usize = 64;
const MAX_JSON_OBJECT_KEYS: usize = 64;
const MAX_JSON_DEPTH: usize = 8;

pub fn redact_json_value(v: &Value) -> Value {
    match v {
        Value::String(s) => Value::String(redact(s)),
        Value::Array(arr) => Value::Array(arr.iter().map(redact_json_value).collect()),
        Value::Object(map) => Value::Object(
            map.iter()
                .map(|(k, val)| (k.clone(), redact_json_value(val)))
                .collect(),
        ),
        other => other.clone(),
    }
}

fn clip_chars(s: &str, max_chars: usize) -> String {
    if max_chars == 0 {
        return String::new();
    }
    let mut iter = s.chars();
    let head: String = iter.by_ref().take(max_chars).collect();
    if iter.next().is_some() {
        let mut trimmed: String = head.chars().take(max_chars.saturating_sub(1)).collect();
        trimmed.push('…');
        trimmed
    } else {
        head
    }
}

fn redact_and_clip_inner(v: &Value, max_str_chars: usize, depth: usize) -> Value {
    if depth >= MAX_JSON_DEPTH {
        return Value::String("…".to_owned());
    }
    match v {
        Value::String(s) => Value::String(clip_chars(&redact(s), max_str_chars)),
        Value::Array(arr) => Value::Array(
            arr.iter()
                .take(MAX_JSON_ARRAY_ITEMS)
                .map(|item| redact_and_clip_inner(item, max_str_chars, depth + 1))
                .collect(),
        ),
        Value::Object(map) => Value::Object(
            map.iter()
                .take(MAX_JSON_OBJECT_KEYS)
                .map(|(k, val)| {
                    (
                        k.clone(),
                        redact_and_clip_inner(val, max_str_chars, depth + 1),
                    )
                })
                .collect(),
        ),
        other => other.clone(),
    }
}

/// Recursively redacts secrets and bounds string lengths / container fan-out so wire snapshots
/// and UI state cannot leak credentials or exceed SSE line budgets.
pub fn redact_and_clip_json_value(v: &Value, max_str_chars: usize) -> Value {
    redact_and_clip_inner(v, max_str_chars, 0)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn redacts_all_nine_secret_classes_verbatim() {
        let raw = "Bearer abc123 sk-12345678 ghp_1234567890 xoxb-1234567890 \
                   eyJhbGciOiJI.eyJzdWIiOiIx.abcd https://u:p@host 192.168.1.5 \
                   password=secret ABCDEFGHIJKLMNOPQRSTUVWXYZ012345";
        let masked = redact(raw);
        assert!(masked.contains("Bearer •••"), "got: {masked}");
        assert!(!masked.contains("abc123"));
        assert!(!masked.contains("sk-12345678"));
        assert!(!masked.contains("ghp_1234567890"));
        assert!(!masked.contains("xoxb-1234567890"));
        assert!(!masked.contains("eyJhbGciOiJI"));
        assert!(masked.contains("https://•••@host"), "got: {masked}");
        assert!(!masked.contains("192.168.1.5"));
        assert!(masked.contains("password=•••"), "got: {masked}");
        assert!(!masked.contains("ABCDEFGHIJKLMNOPQRSTUVWXYZ012345"));
    }

    #[test]
    fn preserves_normal_text_without_secrets() {
        let plain = "git status && cargo test -p station-core";
        assert_eq!(redact(plain), plain);
    }

    #[test]
    fn redact_json_value_recursively_masks_nested_strings_and_aws_keys() {
        let raw = serde_json::json!({
            "num": 42,
            "flag": true,
            "null": null,
            "text": "sk-ant-api03-abcdef123456",
            "nested": {
                "arr": ["ghp_1234567890abcdef", "AKIAIOSFODNN7EXAMPLE", "plain"]
            }
        });
        let masked = redact_json_value(&raw);
        assert_eq!(masked["num"], 42);
        assert_eq!(masked["flag"], true);
        assert!(masked["null"].is_null());
        let s = masked.to_string();
        assert!(!s.contains("sk-ant-api03-abcdef123456"));
        assert!(!s.contains("ghp_1234567890abcdef"));
        assert!(!s.contains("AKIAIOSFODNN7EXAMPLE"));
        assert!(s.contains("plain"));
    }

    #[test]
    fn redact_and_clip_json_value_masks_secrets_and_caps_huge_strings() {
        let huge = format!(
            "plan header {} {}",
            "plan step line\n".repeat(200),
            "sk-ant-api03-secret9999"
        );
        let raw = serde_json::json!({
            "plan": huge,
            "nested": { "token": "ghp_1234567890abcdef" }
        });
        let safe = redact_and_clip_json_value(&raw, 1024);
        let plan_str = safe["plan"].as_str().expect("plan string");
        assert!(plan_str.chars().count() <= 1024);
        assert!(plan_str.ends_with('…'));
        assert!(!safe.to_string().contains("sk-ant-api03-secret9999"));
        assert!(!safe.to_string().contains("ghp_1234567890abcdef"));
    }
}
