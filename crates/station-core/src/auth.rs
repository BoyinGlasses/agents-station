use crate::atomic::write_bytes_atomic;
use anyhow::Result;
use axum::http::StatusCode;
use base64::Engine;
use base64::engine::general_purpose::URL_SAFE_NO_PAD;
use hmac::{Hmac, Mac};
use rand::Rng;
use sha2::Sha256;
use std::collections::HashMap;
use std::net::IpAddr;
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

type HmacSha256 = Hmac<Sha256>;

pub const COOKIE_NAME: &str = "office_pin";
pub const KEEP_DAYS: u64 = 365;
pub const VISIT_MS: u64 = 12 * 3600 * 1000;
pub const PIN_LOCK_WINDOW_MS: u64 = 15 * 60 * 1000;
pub const PIN_MAX_IP_FAILURES: usize = 5;
pub const PIN_MAX_GLOBAL_FAILURES: usize = 20;

#[derive(Debug, Default)]
pub struct PinRateLimiter {
    per_ip: HashMap<IpAddr, Vec<u64>>,
    global: Vec<u64>,
}

impl PinRateLimiter {
    pub fn new() -> Self {
        Self::default()
    }

    fn prune(&mut self, now_ms: u64) {
        self.global
            .retain(|&t| now_ms.saturating_sub(t) < PIN_LOCK_WINDOW_MS);
        self.per_ip.retain(|_, list| {
            list.retain(|&t| now_ms.saturating_sub(t) < PIN_LOCK_WINDOW_MS);
            !list.is_empty()
        });
    }

    /// Enforce 5 failures / 15 min per IP and 20 failures / 15 min global lockout.
    /// Returns `Err(StatusCode::TOO_MANY_REQUESTS)` once either budget is exhausted.
    pub fn check_and_record(
        &mut self,
        ip: IpAddr,
        now_ms: u64,
        success: bool,
    ) -> Result<(), StatusCode> {
        self.prune(now_ms);
        let ip_fails = self.per_ip.get(&ip).map_or(0, Vec::len);
        if ip_fails >= PIN_MAX_IP_FAILURES || self.global.len() >= PIN_MAX_GLOBAL_FAILURES {
            return Err(StatusCode::TOO_MANY_REQUESTS);
        }
        if success {
            self.per_ip.remove(&ip);
        } else {
            self.per_ip.entry(ip).or_default().push(now_ms);
            self.global.push(now_ms);
        }
        Ok(())
    }
}

pub fn is_valid_pin_format(pin: &str) -> bool {
    (6..=10).contains(&pin.len()) && pin.bytes().all(|b| b.is_ascii_digit())
}

pub fn pin_weak(pin: &str) -> bool {
    if !is_valid_pin_format(pin) {
        return true;
    }
    let digits: Vec<i16> = pin.bytes().map(|b| (b - b'0') as i16).collect();
    let first_delta = digits[1] - digits[0];
    let all_same_step = digits
        .windows(2)
        .all(|w| (w[1] - w[0]) == first_delta);
    all_same_step && matches!(first_delta, -1 | 0 | 1)
}

pub fn constant_time_eq(a: &str, b: &str) -> bool {
    let ab = a.as_bytes();
    let bb = b.as_bytes();
    if ab.len() != bb.len() {
        return false;
    }
    let mut diff = 0u8;
    for (x, y) in ab.iter().zip(bb.iter()) {
        diff |= x ^ y;
    }
    diff == 0
}

fn now_ms() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_millis() as u64)
        .unwrap_or(0)
}

#[derive(Debug, Clone)]
pub struct AuthStore {
    token: String,
    pin_file: PathBuf,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct VerifiedCookie {
    pub exp_ms: u64,
    pub keep: bool,
}

impl AuthStore {
    pub fn load_or_init(data_dir: &Path) -> Result<Self> {
        std::fs::create_dir_all(data_dir)?;
        let token_file = data_dir.join("token");
        let token = match std::fs::read_to_string(&token_file) {
            Ok(t) if !t.trim().is_empty() => t.trim().to_owned(),
            _ => {
                let mut bytes = [0u8; 9];
                rand::rng().fill(&mut bytes);
                let t = URL_SAFE_NO_PAD.encode(bytes);
                write_bytes_atomic(&token_file, t.as_bytes())?;
                t
            }
        };

        let store = Self {
            token,
            pin_file: data_dir.join("pin"),
        };
        let _ = store.current_pin();
        Ok(store)
    }

    pub fn token(&self) -> &str {
        &self.token
    }

    pub fn current_pin(&self) -> String {
        if let Ok(raw) = std::fs::read_to_string(&self.pin_file) {
            let trimmed = raw.trim();
            if is_valid_pin_format(trimmed) {
                return trimmed.to_owned();
            }
        }
        let n: u32 = rand::rng().random_range(0..1_000_000);
        let pin = format!("{n:06}");
        let _ = write_bytes_atomic(&self.pin_file, pin.as_bytes());
        pin
    }

    pub fn set_pin(&self, pin: &str) -> Result<bool> {
        if pin_weak(pin) {
            return Ok(false);
        }
        write_bytes_atomic(&self.pin_file, pin.as_bytes())?;
        Ok(true)
    }

    pub fn regenerate_pin(&self) -> Result<String> {
        let prev = self.current_pin();
        let mut rng = rand::rng();
        for _ in 0..4096 {
            let n: u32 = rng.random_range(0..1_000_000);
            let pin = format!("{n:06}");
            if !pin_weak(&pin) && pin != prev {
                write_bytes_atomic(&self.pin_file, pin.as_bytes())?;
                return Ok(pin);
            }
        }
        anyhow::bail!("failed to generate non-weak random PIN")
    }

    fn sign(&self, exp_ms: u64, kind: &str, pin: &str) -> String {
        let mut mac =
            HmacSha256::new_from_slice(self.token.as_bytes()).expect("HMAC accepts any key size");
        let payload = if kind.is_empty() {
            format!("pin:{exp_ms}:{pin}")
        } else {
            format!("pin:{exp_ms}:{kind}:{pin}")
        };
        mac.update(payload.as_bytes());
        URL_SAFE_NO_PAD.encode(mac.finalize().into_bytes())
    }

    pub fn sign_cookie(&self, remember: bool) -> String {
        let kind = if remember { "k" } else { "s" };
        let ttl = if remember {
            KEEP_DAYS * 86_400 * 1000
        } else {
            VISIT_MS
        };
        let exp = now_ms().saturating_add(ttl);
        let pin = self.current_pin();
        let sig = self.sign(exp, kind, &pin);
        format!("{exp}.{kind}.{sig}")
    }

    pub fn set_cookie_header(&self, remember: bool, via_tunnel: bool) -> String {
        let val = self.sign_cookie(remember);
        let max_age = if remember {
            format!(" Max-Age={};", KEEP_DAYS * 86_400)
        } else {
            String::new()
        };
        let secure = if via_tunnel { "; Secure" } else { "" };
        format!("{COOKIE_NAME}={val};{max_age} Path=/; HttpOnly; SameSite=Lax{secure}")
    }

    pub fn verify_cookie_value(&self, cookie_val: &str) -> Option<VerifiedCookie> {
        let parts: Vec<&str> = cookie_val.split('.').collect();
        let (exp_str, kind, sig) = match parts.as_slice() {
            [exp, sig] => (*exp, "", *sig),
            [exp, kind @ ("k" | "s"), sig] => (*exp, *kind, *sig),
            _ => return None,
        };
        let exp_ms: u64 = exp_str.parse().ok()?;
        if exp_ms <= now_ms() {
            return None;
        }
        let expected = self.sign(exp_ms, kind, &self.current_pin());
        if !constant_time_eq(sig, &expected) {
            return None;
        }
        Some(VerifiedCookie {
            exp_ms,
            keep: kind != "s",
        })
    }

    pub fn verify_cookie(&self, cookie_header: &str) -> bool {
        self.parse_cookie_header(cookie_header).is_some()
    }

    pub fn parse_cookie_header(&self, cookie_header: &str) -> Option<VerifiedCookie> {
        for part in cookie_header.split(';') {
            let trimmed = part.trim();
            if let Some(val) = trimmed.strip_prefix(&format!("{COOKIE_NAME}="))
                && let Some(verified) = self.verify_cookie_value(val.trim())
            {
                return Some(verified);
            }
        }
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn pin_weak_rejects_runs_repeats_and_bad_lengths() {
        assert!(pin_weak("111111"));
        assert!(pin_weak("123456"));
        assert!(pin_weak("987654"));
        assert!(pin_weak("12345"));
        assert!(pin_weak("12345678901"));
        assert!(pin_weak("12a456"));
        assert!(!pin_weak("492817"));
        assert!(!pin_weak("49281703"));
    }

    #[test]
    fn auth_store_token_pin_and_cookie_invalidation_on_pin_change() {
        let dir = tempfile::tempdir().unwrap();
        let store = AuthStore::load_or_init(dir.path()).unwrap();
        assert_eq!(store.token().len(), 12);
        assert!(is_valid_pin_format(&store.current_pin()));

        let cookie_val = store.sign_cookie(true);
        let header = format!("foo=bar; {COOKIE_NAME}={cookie_val}");
        assert!(store.verify_cookie(&header));
        assert!(!store.verify_cookie("office_pin=9999999999999.k.tampered"));

        assert!(!store.set_pin("123456").unwrap());
        assert!(store.set_pin("492817").unwrap());
        assert_eq!(store.current_pin(), "492817");
        // Changing PIN invalidates all prior signed cookies.
        assert!(!store.verify_cookie(&header));
        let fresh = format!("{COOKIE_NAME}={}", store.sign_cookie(false));
        let parsed = store.parse_cookie_header(&fresh).unwrap();
        assert!(!parsed.keep);
    }

    #[test]
    fn pin_rate_limiter_enforces_per_ip_global_and_window_expiry() {
        use std::net::Ipv4Addr;
        let mut lim = PinRateLimiter::new();
        let ip1 = IpAddr::V4(Ipv4Addr::new(192, 168, 1, 10));
        let ip2 = IpAddr::V4(Ipv4Addr::new(192, 168, 1, 11));
        let t0 = 1_000_000u64;

        for i in 0..5 {
            assert_eq!(lim.check_and_record(ip1, t0 + i * 1000, false), Ok(()));
        }
        assert_eq!(
            lim.check_and_record(ip1, t0 + 6000, false),
            Err(StatusCode::TOO_MANY_REQUESTS)
        );
        // Even a valid PIN is rejected while locked out
        assert_eq!(
            lim.check_and_record(ip1, t0 + 7000, true),
            Err(StatusCode::TOO_MANY_REQUESTS)
        );
        // Other IP still allowed until global cap
        assert_eq!(lim.check_and_record(ip2, t0 + 8000, true), Ok(()));

        // After 15 min window passes, ip1 is unlocked
        assert_eq!(
            lim.check_and_record(ip1, t0 + PIN_LOCK_WINDOW_MS + 10_000, true),
            Ok(())
        );

        // Global lockout: 20 failures across 5 IPs -> 21st from 6th IP rejected
        let mut g_lim = PinRateLimiter::new();
        for host in 1u8..=5 {
            let ip = IpAddr::V4(Ipv4Addr::new(10, 0, 0, host));
            for attempt in 0..4 {
                assert_eq!(
                    g_lim.check_and_record(ip, t0 + u64::from(host) * 100 + attempt, false),
                    Ok(())
                );
            }
        }
        let ip6 = IpAddr::V4(Ipv4Addr::new(10, 0, 0, 6));
        assert_eq!(
            g_lim.check_and_record(ip6, t0 + 1000, false),
            Err(StatusCode::TOO_MANY_REQUESTS)
        );
    }
}
