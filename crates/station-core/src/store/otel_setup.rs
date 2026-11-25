use regex::Regex;
use serde_json::{Map, Value};
use std::collections::{HashMap, HashSet};
use std::sync::OnceLock;

pub const MARK: &str = "MY_OFFICE_OTEL";

pub fn otel_env(port: u16, names: bool) -> HashMap<&'static str, String> {
    let mut m = HashMap::new();
    m.insert("CLAUDE_CODE_ENABLE_TELEMETRY", "1".to_owned());
    m.insert("OTEL_METRICS_EXPORTER", "otlp".to_owned());
    m.insert("OTEL_LOGS_EXPORTER", "otlp".to_owned());
    m.insert("OTEL_EXPORTER_OTLP_PROTOCOL", "http/json".to_owned());
    m.insert("OTEL_EXPORTER_OTLP_ENDPOINT", format!("http://127.0.0.1:{port}"));
    m.insert("OTEL_EXPORTER_OTLP_METRICS_TEMPORALITY_PREFERENCE", "delta".to_owned());
    if names {
        m.insert("OTEL_LOG_TOOL_DETAILS", "1".to_owned());
    }
    m
}

fn telemetry_re() -> &'static Regex {
    static RE: OnceLock<Regex> = OnceLock::new();
    RE.get_or_init(|| {
        Regex::new(r"^(OTEL_|CLAUDE_CODE_ENABLE_TELEMETRY$|CLAUDE_CODE_ENHANCED_TELEMETRY_BETA$|BETA_TRACING_ENDPOINT$)").unwrap()
    })
}

fn still_ours(k: &str, v: &str) -> bool {
    if k == "OTEL_EXPORTER_OTLP_ENDPOINT" {
        static EP: OnceLock<Regex> = OnceLock::new();
        return EP
            .get_or_init(|| Regex::new(r"^http://127\.0\.0\.1:\d+/?$").unwrap())
            .is_match(v);
    }
    otel_env(0, true).get(k).is_some_and(|exp| exp == v)
}

pub fn marked_keys(env: &Map<String, Value>) -> Vec<String> {
    env.get(MARK)
        .and_then(Value::as_str)
        .unwrap_or("")
        .split(',')
        .map(str::trim)
        .filter(|k| telemetry_re().is_match(k))
        .map(str::to_owned)
        .collect()
}

pub fn add_otel(settings: &mut Value, port: u16, names: bool) -> usize {
    let Some(obj) = settings.as_object_mut() else {
        return 0;
    };
    let env_val = obj
        .entry("env")
        .or_insert_with(|| Value::Object(Map::new()));
    let Some(env) = env_val.as_object_mut() else {
        return 0;
    };
    let mut mine: HashSet<String> = marked_keys(env).into_iter().collect();
    let has_foreign = env
        .keys()
        .any(|k| k != MARK && telemetry_re().is_match(k) && !mine.contains(k));
    if has_foreign {
        return 0;
    }
    let mut added = 0usize;
    for (k, v) in otel_env(port, names) {
        if env.get(k).and_then(Value::as_str) == Some(v.as_str()) {
            mine.insert(k.to_owned());
            continue;
        }
        if let Some(existing) = env.get(k).and_then(Value::as_str)
            && !(mine.contains(k) && still_ours(k, existing))
        {
            continue;
        }
        env.insert(k.to_owned(), Value::String(v));
        mine.insert(k.to_owned());
        added += 1;
    }
    if !mine.is_empty() {
        let mut sorted: Vec<String> = mine.into_iter().collect();
        sorted.sort();
        env.insert(MARK.to_owned(), Value::String(sorted.join(",")));
    }
    added
}

pub fn remove_otel(settings: &mut Value) -> usize {
    let Some(obj) = settings.as_object_mut() else {
        return 0;
    };
    let Some(env) = obj.get_mut("env").and_then(Value::as_object_mut) else {
        return 0;
    };
    if !env.contains_key(MARK) {
        return 0;
    }
    let mut removed = 0;
    for k in marked_keys(env) {
        if let Some(v) = env.get(&k).and_then(Value::as_str)
            && still_ours(&k, v)
        {
            env.remove(&k);
            removed += 1;
        }
    }
    env.remove(MARK);
    removed += 1;
    if env.is_empty() {
        obj.remove("env");
    }
    removed
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn add_and_remove_otel_respects_foreign_collector() {
        let mut s = json!({});
        assert!(add_otel(&mut s, 4310, true) >= 6);
        assert_eq!(s["env"]["OTEL_EXPORTER_OTLP_ENDPOINT"], "http://127.0.0.1:4310");
        assert!(remove_otel(&mut s) >= 7);
        assert!(s.get("env").is_none());

        let mut foreign = json!({ "env": { "OTEL_EXPORTER_OTLP_ENDPOINT": "https://corp.example" } });
        assert_eq!(add_otel(&mut foreign, 4310, true), 0);
        assert_eq!(foreign["env"]["OTEL_EXPORTER_OTLP_ENDPOINT"], "https://corp.example");
    }
}
