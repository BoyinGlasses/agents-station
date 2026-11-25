use crate::atomic::write_bytes_atomic;
use anyhow::Result;
use base64::Engine;
use base64::engine::general_purpose::URL_SAFE_NO_PAD;
use hmac::{Hmac, Mac};
use rand::Rng;
use sha2::Sha256;
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

type HmacSha256 = Hmac<Sha256>;

pub const COOKIE_NAME: &str = "office_pin";
pub const KEEP_DAYS: u64 = 365;
pub const VISIT_MS: u64 = 12 * 3600 * 1000;

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
}
