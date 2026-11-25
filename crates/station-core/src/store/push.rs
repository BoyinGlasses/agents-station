use crate::atomic::write_json_atomic;
use base64::Engine;
use base64::engine::general_purpose::URL_SAFE_NO_PAD;
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

pub fn service_ok(endpoint: &str) -> bool {
    let Some(rest) = endpoint.strip_prefix("https://") else {
        return false;
    };
    let host_port = rest.split('/').next().unwrap_or("");
    let host = host_port.split(':').next().unwrap_or("").to_ascii_lowercase();
    host == "fcm.googleapis.com"
        || host == "android.googleapis.com"
        || host == "push.apple.com"
        || host.ends_with(".push.apple.com")
        || host == "push.services.mozilla.com"
        || host.ends_with(".push.services.mozilla.com")
        || host == "notify.windows.com"
        || host.ends_with(".notify.windows.com")
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PushKeys {
    pub p256dh: String,
    pub auth: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PushSubscription {
    pub endpoint: String,
    pub keys: PushKeys,
    #[serde(default)]
    pub ua: String,
    #[serde(default)]
    pub at: u64,
}

pub struct PushStore {
    subs_file: PathBuf,
    pub public_key: String,
    subs: Vec<PushSubscription>,
}

impl PushStore {
    pub fn load(data_dir: &Path) -> Self {
        let subs_file = data_dir.join("push.json");
        let vapid_file = data_dir.join("vapid.json");
        let public_key = std::fs::read_to_string(&vapid_file)
            .ok()
            .and_then(|s| serde_json::from_str::<serde_json::Value>(&s).ok())
            .and_then(|v| v.get("publicKey").and_then(|k| k.as_str()).map(str::to_owned))
            .unwrap_or_else(|| {
                let mut raw = [0u8; 65];
                raw[0] = 4;
                rand::Rng::fill(&mut rand::rng(), &mut raw[1..]);
                let pk = URL_SAFE_NO_PAD.encode(raw);
                let _ = write_json_atomic(&vapid_file, &serde_json::json!({ "publicKey": pk }));
                pk
            });
        let subs: Vec<PushSubscription> = std::fs::read_to_string(&subs_file)
            .ok()
            .and_then(|s| serde_json::from_str::<Vec<PushSubscription>>(&s).ok())
            .unwrap_or_default()
            .into_iter()
            .filter(|s| service_ok(&s.endpoint))
            .collect();
        Self {
            subs_file,
            public_key,
            subs,
        }
    }

    pub fn count(&self) -> usize {
        self.subs.len()
    }

    pub fn has(&self, endpoint: &str) -> bool {
        self.subs.iter().any(|s| s.endpoint == endpoint)
    }

    pub fn add(&mut self, sub: PushSubscription, now_ms: u64) -> bool {
        if !service_ok(&sub.endpoint) {
            return false;
        }
        let Ok(p256) = URL_SAFE_NO_PAD.decode(&sub.keys.p256dh) else {
            return false;
        };
        let Ok(auth) = URL_SAFE_NO_PAD.decode(&sub.keys.auth) else {
            return false;
        };
        if p256.len() != 65 || auth.len() != 16 {
            return false;
        }
        self.subs.retain(|s| s.endpoint != sub.endpoint);
        self.subs.push(PushSubscription {
            endpoint: sub.endpoint,
            keys: sub.keys,
            ua: sub.ua.chars().take(160).collect(),
            at: now_ms,
        });
        if self.subs.len() > 20 {
            let drain = self.subs.len() - 20;
            self.subs.drain(..drain);
        }
        let _ = write_json_atomic(&self.subs_file, &self.subs);
        true
    }

    pub fn remove(&mut self, endpoint: &str) -> bool {
        let before = self.subs.len();
        self.subs.retain(|s| s.endpoint != endpoint);
        if self.subs.len() != before {
            let _ = write_json_atomic(&self.subs_file, &self.subs);
            true
        } else {
            false
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn push_store_validates_endpoints_and_key_lengths() {
        assert!(service_ok("https://fcm.googleapis.com/fcm/send/abc"));
        assert!(service_ok("https://web.push.apple.com/xyz"));
        assert!(!service_ok("https://evil.example.com/push"));
        assert!(!service_ok("http://fcm.googleapis.com/insecure"));

        let dir = tempfile::tempdir().unwrap();
        let mut st = PushStore::load(dir.path());
        let ok = st.add(
            PushSubscription {
                endpoint: "https://fcm.googleapis.com/fcm/send/1".into(),
                keys: PushKeys {
                    p256dh: URL_SAFE_NO_PAD.encode([4u8; 65]),
                    auth: URL_SAFE_NO_PAD.encode([1u8; 16]),
                },
                ua: "iPhone".into(),
                at: 0,
            },
            1000,
        );
        assert!(ok);
        assert_eq!(st.count(), 1);
        assert!(st.remove("https://fcm.googleapis.com/fcm/send/1"));
        assert_eq!(st.count(), 0);
    }
}
