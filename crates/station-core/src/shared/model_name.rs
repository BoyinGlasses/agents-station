use regex::Regex;
use std::sync::OnceLock;

fn model_re() -> &'static Regex {
    static RE: OnceLock<Regex> = OnceLock::new();
    RE.get_or_init(|| {
        Regex::new(r"^claude-(opus|sonnet|haiku|fable|mythos)-(\d+)(?:-(\d{1,2}))?(?:$|[^\d])")
            .unwrap()
    })
}

fn cap(w: &str) -> String {
    let mut chars = w.chars();
    match chars.next() {
        Some(first) => format!("{}{}", first.to_uppercase(), chars.as_str()),
        None => String::new(),
    }
}

pub fn model_family(m: &str) -> String {
    let Some(caps) = model_re().captures(m) else {
        return String::new();
    };
    cap(&caps[1])
}

pub fn model_long(m: &str) -> String {
    let Some(caps) = model_re().captures(m) else {
        return String::new();
    };
    let family = cap(&caps[1]);
    let major = &caps[2];
    match caps.get(3) {
        Some(minor) => format!("{family} {major}.{}", minor.as_str()),
        None => format!("{family} {major}"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn model_id_family_and_long_match_oracle() {
        assert_eq!(model_family("claude-sonnet-5-5"), "Sonnet");
        assert_eq!(model_long("claude-opus-5-5[1m]"), "Opus 5.5");
        assert_eq!(model_long("claude-haiku-4-5-20251001"), "Haiku 4.5");
        assert_eq!(model_family("<synthetic>"), "");
        assert_eq!(model_long("<synthetic>"), "");
    }
}
