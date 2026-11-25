use serde::{Deserialize, Serialize};
use serde_json::Value;

pub const TTL_5M_MS: u64 = 5 * 60 * 1000;
pub const TTL_1H_MS: u64 = 3600 * 1000;

pub fn ttl_of(raw: Option<&str>) -> &'static str {
    if raw == Some("1h") { "1h" } else { "5m" }
}

pub fn ttl_ms(ttl: &str) -> u64 {
    if ttl == "1h" { TTL_1H_MS } else { TTL_5M_MS }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct CacheState {
    pub until: u64,
    pub ttl: String,
    pub tokens: u64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub usd: Option<f64>,
    pub guessed: bool,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct CacheClockView {
    pub state: &'static str,
    pub left: u64,
    pub warmth: f64,
    pub tokens: u64,
    pub guessed: bool,
}

fn finite_f64(v: Option<&Value>) -> Option<f64> {
    v.and_then(Value::as_f64).filter(|x| x.is_finite())
}

fn finite_u64(v: Option<&Value>) -> Option<u64> {
    finite_f64(v).map(|x| x.max(0.0).round() as u64)
}

pub fn cache_from_status(pc: Option<&Value>, ctx: Option<u64>) -> Option<CacheState> {
    let obj = pc?.as_object()?;
    if obj.get("caching_observed").and_then(Value::as_bool) == Some(false) {
        return None;
    }
    let warm = obj.get("warm").and_then(Value::as_bool).unwrap_or(false);
    let expires_at = finite_f64(obj.get("expires_at"));
    let until = if warm {
        expires_at.map(|s| (s * 1000.0).max(0.0).round() as u64).unwrap_or(0)
    } else {
        0
    };
    let ttl = ttl_of(obj.get("ttl").and_then(Value::as_str)).to_owned();
    let tokens = finite_u64(obj.get("recache_tokens_if_cold"))
        .or(ctx)
        .unwrap_or(0);
    Some(CacheState {
        until,
        ttl,
        tokens,
        usd: None,
        guessed: false,
    })
}

pub fn cache_from_resume(p: Option<&Value>, ttl: &str, now_ms: u64) -> Option<CacheState> {
    let obj = p?.as_object()?;
    let expired = obj
        .get("prompt_cache_likely_expired")
        .and_then(Value::as_bool)?;
    let ago_s = finite_f64(obj.get("seconds_since_last_response")).unwrap_or(0.0);
    let norm_ttl = ttl_of(Some(ttl));
    let life = ttl_ms(norm_ttl) as f64;
    let until = if expired {
        0
    } else {
        ((now_ms as f64) - ago_s * 1000.0 + life).max(0.0).round() as u64
    };
    Some(CacheState {
        until,
        ttl: norm_ttl.to_owned(),
        tokens: finite_u64(obj.get("context_tokens")).unwrap_or(0),
        usd: finite_f64(obj.get("estimated_cache_write_usd")),
        guessed: !expired,
    })
}

pub fn cache_guess(last_at_ms: u64, ctx: Option<u64>, ttl: &str) -> CacheState {
    let norm_ttl = ttl_of(Some(ttl));
    CacheState {
        until: last_at_ms.saturating_add(ttl_ms(norm_ttl)),
        ttl: norm_ttl.to_owned(),
        tokens: ctx.unwrap_or(0),
        usd: None,
        guessed: true,
    }
}

pub fn cache_clock(cache: Option<&CacheState>, now_ms: u64) -> CacheClockView {
    let Some(c) = cache else {
        return CacheClockView {
            state: "none",
            left: 0,
            warmth: 0.0,
            tokens: 0,
            guessed: false,
        };
    };
    let left = c.until.saturating_sub(now_ms);
    let life = ttl_ms(&c.ttl) as f64;
    CacheClockView {
        state: if left > 0 { "warm" } else { "cold" },
        left,
        warmth: if left > 0 {
            ((left as f64) / life).min(1.0)
        } else {
            0.0
        },
        tokens: c.tokens,
        guessed: c.guessed,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    const NOW: u64 = 1_800_000_000_000;

    #[test]
    fn from_status_line_warm_then_cold() {
        let c = cache_from_status(
            Some(&json!({
                "warm": true,
                "caching_observed": true,
                "ttl": "1h",
                "expires_at": NOW / 1000 + 23 * 60,
                "hit_ratio": 0.91,
                "recache_tokens_if_cold": 45000
            })),
            None,
        )
        .unwrap();
        assert_eq!(c.until, NOW + 23 * 60_000);
        assert_eq!(c.ttl, "1h");
        assert_eq!(c.tokens, 45000);
        assert!(!c.guessed);

        let k = cache_clock(Some(&c), NOW);
        assert_eq!(k.state, "warm");
        assert_eq!((k.left as f64 / 60_000.0).round() as u64, 23);
        assert!((k.warmth - 23.0 / 60.0).abs() < 1e-9);
        assert_eq!(cache_clock(Some(&c), NOW + 23 * 60_000).state, "cold");
        assert_eq!(cache_clock(Some(&c), NOW + 23 * 60_000).warmth, 0.0);
        assert_eq!(cache_clock(Some(&c), NOW + 30 * 60_000).tokens, 45000);
    }

    #[test]
    fn from_status_cold_off_and_fallbacks() {
        let cold = cache_from_status(
            Some(&json!({ "warm": false, "caching_observed": true, "ttl": "5m", "expires_at": null, "recache_tokens_if_cold": 9000 })),
            None,
        );
        assert_eq!(cache_clock(cold.as_ref(), NOW).state, "cold");
        assert!(cache_from_status(Some(&json!({ "warm": false, "caching_observed": false })), None).is_none());
        assert!(cache_from_status(None, None).is_none());
        assert_eq!(
            cache_from_status(
                Some(&json!({ "warm": true, "ttl": "5m", "expires_at": NOW / 1000 + 60, "recache_tokens_if_cold": null })),
                Some(52000)
            )
            .unwrap()
            .tokens,
            52000
        );
        assert_eq!(
            cache_from_status(Some(&json!({ "warm": true, "ttl": "weird", "expires_at": NOW / 1000 + 60 })), None)
                .unwrap()
                .ttl,
            "5m"
        );
    }

    #[test]
    fn from_resume_and_guess() {
        let cold = cache_from_resume(
            Some(&json!({
                "source": "resume",
                "seconds_since_last_response": 5400,
                "context_tokens": 182340,
                "prompt_cache_likely_expired": true,
                "estimated_cache_write_usd": 1.1396
            })),
            "1h",
            NOW,
        )
        .unwrap();
        assert_eq!(cache_clock(Some(&cold), NOW).state, "cold");
        assert_eq!(cold.tokens, 182340);
        assert_eq!(cold.usd, Some(1.1396));

        let warm = cache_from_resume(
            Some(&json!({
                "source": "fork",
                "seconds_since_last_response": 120,
                "context_tokens": 30000,
                "prompt_cache_likely_expired": false
            })),
            "5m",
            NOW,
        )
        .unwrap();
        assert_eq!(warm.until, NOW - 120_000 + TTL_5M_MS);
        assert!(warm.guessed);
        assert!(cache_from_resume(Some(&json!({ "source": "startup" })), "5m", NOW).is_none());

        let g = cache_guess(NOW - 4 * 60_000, Some(120_000), "5m");
        assert_eq!(cache_clock(Some(&g), NOW).state, "warm");
        assert!(cache_clock(Some(&g), NOW).guessed);
        assert_eq!(cache_clock(Some(&g), NOW + 60_000).state, "cold");
        assert_eq!(cache_clock(Some(&cache_guess(NOW - 50 * 60_000, Some(1), "1h")), NOW).state, "warm");
        assert_eq!(cache_clock(None, NOW).state, "none");
    }
}
