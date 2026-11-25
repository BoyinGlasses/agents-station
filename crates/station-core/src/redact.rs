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
}
