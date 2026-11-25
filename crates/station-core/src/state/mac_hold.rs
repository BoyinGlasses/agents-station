use serde::{Deserialize, Serialize};

pub const BAR_ALIVE_MS: u64 = 8_000;
pub const BAR_IDLE_MAX_SEC: f64 = 120.0;
pub const HOLD_MS: u64 = 45_000;
pub const DENY_HOLD_MS: u64 = 40_000;
pub const BAR_HOLD_MS: u64 = 20_000;
pub const PHONE_HOLD_MS: u64 = 600_000;
pub const BACK_SEC: f64 = 20.0;

pub fn effective_hold_ms() -> u64 {
    std::env::var("OFFICE_HOLD_MS")
        .ok()
        .and_then(|s| s.parse::<u64>().ok())
        .filter(|&v| v > 0)
        .map(|v| v.min(HOLD_MS))
        .unwrap_or(HOLD_MS)
}

pub fn effective_bar_hold_ms() -> u64 {
    std::env::var("OFFICE_BAR_HOLD_MS")
        .ok()
        .and_then(|s| s.parse::<u64>().ok())
        .filter(|&v| v > 0)
        .map(|v| v.min(BAR_HOLD_MS))
        .unwrap_or(BAR_HOLD_MS)
}

/// Parses `X-Office-Wait` header (in seconds) into a capped millisecond hold ceiling.
/// For hook waits > 10s, subtracts a 5s safety margin (matching `hookLimit` in `server/index.mjs`)
/// and clamps at `PHONE_HOLD_MS`; for shorter waits (<= 10s), uses the exact millisecond duration.
pub fn wait_header_ms(wait_header: Option<&str>) -> Option<u64> {
    let raw = wait_header?.trim();
    let secs = raw.parse::<f64>().ok()?;
    if !secs.is_finite() || secs < 0.0 {
        return None;
    }
    let ms = if secs > 10.0 {
        ((secs - 5.0) * 1000.0).round() as u64
    } else {
        (secs * 1000.0).round() as u64
    };
    Some(ms.min(PHONE_HOLD_MS))
}

#[derive(Debug, Clone, Copy)]
pub struct HoldInput {
    pub focused: bool,
    pub bar_alive: bool,
    pub idle_sec: f64,
    pub pref: bool,
    pub normal_ms: u64,
    pub bar_ms: u64,
    pub phone: bool,
    pub phone_ms: u64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct HoldDecision {
    pub ms: u64,
    pub by: Option<&'static str>,
}

pub fn hold_plan(input: HoldInput) -> HoldDecision {
    let known = input.idle_sec.is_finite();
    if input.phone && input.phone_ms > 0 && known && input.idle_sec >= BAR_IDLE_MAX_SEC {
        return HoldDecision {
            ms: input.phone_ms,
            by: Some("phone"),
        };
    }
    if input.focused {
        return HoldDecision {
            ms: input.normal_ms,
            by: Some("page"),
        };
    }
    if input.pref && input.bar_alive && known && input.idle_sec < BAR_IDLE_MAX_SEC {
        return HoldDecision {
            ms: input.bar_ms,
            by: Some("bar"),
        };
    }
    HoldDecision { ms: 0, by: None }
}

pub fn back_at_mac(idle_sec: f64) -> bool {
    idle_sec.is_finite() && idle_sec < BACK_SEC
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hold_plan_prioritizes_away_phone_over_focused_page_over_bar() {
        let away_phone = hold_plan(HoldInput {
            focused: true,
            bar_alive: true,
            idle_sec: 180.0,
            pref: true,
            normal_ms: 55_000,
            bar_ms: BAR_HOLD_MS,
            phone: true,
            phone_ms: PHONE_HOLD_MS,
        });
        assert_eq!(away_phone, HoldDecision { ms: PHONE_HOLD_MS, by: Some("phone") });

        let focused = hold_plan(HoldInput {
            focused: true,
            bar_alive: true,
            idle_sec: 5.0,
            pref: true,
            normal_ms: 55_000,
            bar_ms: BAR_HOLD_MS,
            phone: true,
            phone_ms: PHONE_HOLD_MS,
        });
        assert_eq!(focused, HoldDecision { ms: 55_000, by: Some("page") });

        let bar = hold_plan(HoldInput {
            focused: false,
            bar_alive: true,
            idle_sec: 15.0,
            pref: true,
            normal_ms: 55_000,
            bar_ms: BAR_HOLD_MS,
            phone: false,
            phone_ms: 0,
        });
        assert_eq!(bar, HoldDecision { ms: BAR_HOLD_MS, by: Some("bar") });

        let none = hold_plan(HoldInput {
            focused: false,
            bar_alive: false,
            idle_sec: f64::INFINITY,
            pref: true,
            normal_ms: 55_000,
            bar_ms: BAR_HOLD_MS,
            phone: true,
            phone_ms: PHONE_HOLD_MS,
        });
        assert_eq!(none, HoldDecision { ms: 0, by: None });

        assert!(back_at_mac(19.9));
        assert!(!back_at_mac(20.0));
        assert!(!back_at_mac(f64::INFINITY));
    }

    #[test]
    fn wait_header_ms_parses_seconds_with_margin_and_caps() {
        assert_eq!(wait_header_ms(None), None);
        assert_eq!(wait_header_ms(Some("")), None);
        assert_eq!(wait_header_ms(Some("-5")), None);
        assert_eq!(wait_header_ms(Some("NaN")), None);
        assert_eq!(wait_header_ms(Some("0")), Some(0));
        assert_eq!(wait_header_ms(Some("0.15")), Some(150));
        assert_eq!(wait_header_ms(Some("5")), Some(5_000));
        assert_eq!(wait_header_ms(Some("20")), Some(15_000));
        assert_eq!(wait_header_ms(Some("600")), Some(595_000));
        assert_eq!(wait_header_ms(Some("9999")), Some(PHONE_HOLD_MS));
    }
}
