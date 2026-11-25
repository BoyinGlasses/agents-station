use crate::atomic::write_json_atomic;
use aes_gcm::aead::{Aead, KeyInit};
use aes_gcm::{Aes128Gcm, Nonce};
use anyhow::{Result, anyhow};
use base64::Engine;
use base64::engine::general_purpose::URL_SAFE_NO_PAD;
use hkdf::Hkdf;
use p256::ecdh::diffie_hellman;
use p256::ecdsa::signature::Signer;
use p256::ecdsa::{Signature, SigningKey};
use p256::elliptic_curve::sec1::ToEncodedPoint;
use p256::{PublicKey, SecretKey};
use rand::RngCore;
use serde::{Deserialize, Serialize};
use sha2::Sha256;
use std::path::{Path, PathBuf};
use std::time::Duration;

const DEFAULT_VAPID_SUBJECT: &str = "mailto:agents-station@users.noreply.github.com";
const VAPID_TTL_SECS: u64 = 12 * 3600;
const RECORD_SIZE: u32 = 4096;
/// RFC 8188 single-record plaintext cap: `rs - 1 (0x02 delimiter) - 16 (AES-GCM tag) = 4079`.
const MAX_SINGLE_RECORD_PLAINTEXT: usize = (RECORD_SIZE as usize) - 17;
const PUSH_TIMEOUT_SECS: u64 = 10;

/// Strict SSRF guard for Web Push endpoints:
/// - must parse cleanly as `https://`
/// - must contain no userinfo (`user:pass@`)
/// - must use default HTTPS port (443)
/// - host must match an official browser push service domain
pub fn service_ok(endpoint: &str) -> bool {
    let Ok(url) = reqwest::Url::parse(endpoint) else {
        return false;
    };
    if url.scheme() != "https" {
        return false;
    }
    if !url.username().is_empty() || url.password().is_some() {
        return false;
    }
    if let Some(port) = url.port()
        && port != 443
    {
        return false;
    }
    let Some(host) = url.host_str() else {
        return false;
    };
    let host = host.to_ascii_lowercase();
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

#[derive(Debug, Clone, Serialize)]
pub struct PushMessage {
    pub title: String,
    pub body: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tag: Option<String>,
    pub url: String,
    pub urgent: bool,
}

#[derive(Debug, Clone)]
pub struct PreparedPush {
    pub endpoint: String,
    pub authorization: String,
    pub urgency: &'static str,
    pub body: Vec<u8>,
}

#[derive(Debug, Serialize, Deserialize)]
struct VapidDisk {
    #[serde(rename = "publicKey")]
    public_key: String,
    #[serde(rename = "privateKey")]
    private_key: String,
}

fn generate_p256_secret_key() -> SecretKey {
    let mut rng = rand::rng();
    let mut scalar = [0u8; 32];
    loop {
        rng.fill_bytes(&mut scalar);
        if let Ok(sk) = SecretKey::from_slice(&scalar) {
            return sk;
        }
    }
}

fn uncompressed_sec1_bytes(sk: &SecretKey) -> [u8; 65] {
    let pt = sk.public_key().to_encoded_point(false);
    let mut out = [0u8; 65];
    out.copy_from_slice(pt.as_bytes());
    out
}

fn load_valid_vapid(vapid_file: &Path) -> Option<(SecretKey, String)> {
    let raw = std::fs::read_to_string(vapid_file).ok()?;
    let disk: VapidDisk = serde_json::from_str(&raw).ok()?;
    let priv_bytes = URL_SAFE_NO_PAD.decode(&disk.private_key).ok()?;
    if priv_bytes.len() != 32 {
        return None;
    }
    let sk = SecretKey::from_slice(&priv_bytes).ok()?;
    let pub_bytes = URL_SAFE_NO_PAD.decode(&disk.public_key).ok()?;
    if pub_bytes.len() != 65 {
        return None;
    }
    let pk = PublicKey::from_sec1_bytes(&pub_bytes).ok()?;
    if sk.public_key() != pk {
        return None;
    }
    Some((sk, disk.public_key))
}

fn valid_subscription_keys(keys: &PushKeys) -> bool {
    let Ok(p256) = URL_SAFE_NO_PAD.decode(&keys.p256dh) else {
        return false;
    };
    let Ok(auth) = URL_SAFE_NO_PAD.decode(&keys.auth) else {
        return false;
    };
    p256.len() == 65 && auth.len() == 16 && PublicKey::from_sec1_bytes(&p256).is_ok()
}

/// Encrypts a Web Push payload per RFC 8291 (`aes128gcm`) / RFC 8188 single-record framing:
/// `salt(16) || rs(4, BE 4096) || idlen(1, 65) || as_pub(65) || AES-128-GCM(plaintext || 0x02)`
pub fn encrypt_aes128gcm(keys: &PushKeys, plaintext: &[u8]) -> Result<Vec<u8>> {
    let mut salt = [0u8; 16];
    rand::rng().fill_bytes(&mut salt);
    let as_secret = generate_p256_secret_key();
    encrypt_aes128gcm_with_ephemeral(keys, plaintext, salt, &as_secret)
}

pub fn encrypt_aes128gcm_with_ephemeral(
    keys: &PushKeys,
    plaintext: &[u8],
    salt: [u8; 16],
    as_secret: &SecretKey,
) -> Result<Vec<u8>> {
    if plaintext.len() > MAX_SINGLE_RECORD_PLAINTEXT {
        return Err(anyhow!(
            "plaintext length {} exceeds RFC 8188 single-record maximum of {} bytes",
            plaintext.len(),
            MAX_SINGLE_RECORD_PLAINTEXT
        ));
    }
    let ua_bytes = URL_SAFE_NO_PAD
        .decode(&keys.p256dh)
        .map_err(|_| anyhow!("invalid p256dh base64url"))?;
    if ua_bytes.len() != 65 {
        return Err(anyhow!("p256dh must be 65-byte uncompressed SEC1 point"));
    }
    let ua_pub =
        PublicKey::from_sec1_bytes(&ua_bytes).map_err(|_| anyhow!("p256dh is not on P-256"))?;

    let auth_secret = URL_SAFE_NO_PAD
        .decode(&keys.auth)
        .map_err(|_| anyhow!("invalid auth base64url"))?;
    if auth_secret.len() != 16 {
        return Err(anyhow!("auth secret must be 16 bytes"));
    }

    let as_pub = uncompressed_sec1_bytes(as_secret);
    let shared = diffie_hellman(as_secret.to_nonzero_scalar(), ua_pub.as_affine());

    // RFC 8291 §3.3: IKM = HKDF(salt=auth_secret, ikm=ecdh_secret, info="WebPush: info\0" || ua_pub || as_pub, L=32)
    let mut key_info = Vec::with_capacity(14 + 65 + 65);
    key_info.extend_from_slice(b"WebPush: info\0");
    key_info.extend_from_slice(&ua_bytes);
    key_info.extend_from_slice(&as_pub);

    let hkdf_auth = Hkdf::<Sha256>::new(Some(&auth_secret), shared.raw_secret_bytes());
    let mut ikm = [0u8; 32];
    hkdf_auth
        .expand(&key_info, &mut ikm)
        .map_err(|_| anyhow!("hkdf ikm expand failed"))?;

    // RFC 8291 §3.4: CEK (16 bytes) and Nonce (12 bytes)
    let hkdf_content = Hkdf::<Sha256>::new(Some(&salt), &ikm);
    let mut cek = [0u8; 16];
    hkdf_content
        .expand(b"Content-Encoding: aes128gcm\0", &mut cek)
        .map_err(|_| anyhow!("hkdf cek expand failed"))?;
    let mut nonce = [0u8; 12];
    hkdf_content
        .expand(b"Content-Encoding: nonce\0", &mut nonce)
        .map_err(|_| anyhow!("hkdf nonce expand failed"))?;

    // RFC 8188 §2: plaintext || 0x02 (final record delimiter)
    let mut padded = Vec::with_capacity(plaintext.len() + 1);
    padded.extend_from_slice(plaintext);
    padded.push(0x02);

    let cipher = Aes128Gcm::new_from_slice(&cek).map_err(|_| anyhow!("aes128gcm key init failed"))?;
    let ciphertext = cipher
        .encrypt(Nonce::from_slice(&nonce), padded.as_slice())
        .map_err(|_| anyhow!("aes128gcm encrypt failed"))?;

    // RFC 8188 §2.1 header (86 bytes): salt(16) || rs(4) || idlen(1) || as_pub(65)
    let mut out = Vec::with_capacity(86 + ciphertext.len());
    out.extend_from_slice(&salt);
    out.extend_from_slice(&RECORD_SIZE.to_be_bytes());
    out.push(as_pub.len() as u8);
    out.extend_from_slice(&as_pub);
    out.extend_from_slice(&ciphertext);
    Ok(out)
}

pub struct PushStore {
    subs_file: PathBuf,
    pub public_key: String,
    secret_key: SecretKey,
    subject: String,
    subs: Vec<PushSubscription>,
}

impl PushStore {
    pub fn load(data_dir: &Path) -> Self {
        let subs_file = data_dir.join("push.json");
        let vapid_file = data_dir.join("vapid.json");
        let (secret_key, public_key) = load_valid_vapid(&vapid_file).unwrap_or_else(|| {
            let sk = generate_p256_secret_key();
            let pub_raw = uncompressed_sec1_bytes(&sk);
            let pk_b64 = URL_SAFE_NO_PAD.encode(pub_raw);
            let sk_b64 = URL_SAFE_NO_PAD.encode(sk.to_bytes());
            let disk = VapidDisk {
                public_key: pk_b64.clone(),
                private_key: sk_b64,
            };
            let _ = write_json_atomic(&vapid_file, &disk);
            (sk, pk_b64)
        });
        let subs: Vec<PushSubscription> = std::fs::read_to_string(&subs_file)
            .ok()
            .and_then(|s| serde_json::from_str::<Vec<PushSubscription>>(&s).ok())
            .unwrap_or_default()
            .into_iter()
            .filter(|s| service_ok(&s.endpoint) && valid_subscription_keys(&s.keys))
            .collect();
        Self {
            subs_file,
            public_key,
            secret_key,
            subject: DEFAULT_VAPID_SUBJECT.to_owned(),
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
        if !service_ok(&sub.endpoint) || !valid_subscription_keys(&sub.keys) {
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

    /// Signs an RFC 8292 ES256 VAPID JWT (`header.payload.raw_r_s_sig`).
    pub fn vapid_jwt(&self, audience: &str, now_secs: u64) -> Result<String> {
        let header_b64 = URL_SAFE_NO_PAD.encode(r#"{"typ":"JWT","alg":"ES256"}"#);
        let payload = serde_json::json!({
            "aud": audience,
            "exp": now_secs.saturating_add(VAPID_TTL_SECS),
            "sub": self.subject,
        });
        let payload_b64 = URL_SAFE_NO_PAD.encode(serde_json::to_vec(&payload)?);
        let signing_input = format!("{header_b64}.{payload_b64}");
        let signing_key = SigningKey::from(&self.secret_key);
        let sig: Signature = signing_key.sign(signing_input.as_bytes());
        let sig_b64 = URL_SAFE_NO_PAD.encode(sig.to_bytes());
        Ok(format!("{signing_input}.{sig_b64}"))
    }

    /// Builds encrypted push payloads + VAPID Authorization headers without doing network I/O under lock.
    pub fn prepare_deliveries(&self, msg: &PushMessage, now_secs: u64) -> Vec<PreparedPush> {
        let Ok(plain) = serde_json::to_vec(msg) else {
            return Vec::new();
        };
        let urgency = if msg.urgent { "high" } else { "normal" };
        let mut out = Vec::with_capacity(self.subs.len());
        for sub in &self.subs {
            if !service_ok(&sub.endpoint) {
                continue;
            }
            let Ok(url) = reqwest::Url::parse(&sub.endpoint) else {
                continue;
            };
            let Some(host) = url.host_str() else {
                continue;
            };
            let aud = format!("https://{host}");
            let Ok(jwt) = self.vapid_jwt(&aud, now_secs) else {
                continue;
            };
            let Ok(body) = encrypt_aes128gcm(&sub.keys, &plain) else {
                continue;
            };
            out.push(PreparedPush {
                endpoint: sub.endpoint.clone(),
                authorization: format!("vapid t={jwt}, k={}", self.public_key),
                urgency,
                body,
            });
        }
        out
    }
}

/// Sends prepared Web Push notifications with strict SSRF & no-redirect guards.
/// Returns endpoints that responded with HTTP 404 or 410 so callers can prune dead subscriptions.
pub async fn deliver_prepared(deliveries: Vec<PreparedPush>) -> Vec<String> {
    if deliveries.is_empty() {
        return Vec::new();
    }
    let Ok(client) = reqwest::Client::builder()
        .timeout(Duration::from_secs(PUSH_TIMEOUT_SECS))
        .redirect(reqwest::redirect::Policy::none())
        .build()
    else {
        return Vec::new();
    };
    let mut dead = Vec::new();
    for item in deliveries {
        if !service_ok(&item.endpoint) {
            continue;
        }
        let res = client
            .post(&item.endpoint)
            .header("TTL", "86400")
            .header("Urgency", item.urgency)
            .header("Content-Encoding", "aes128gcm")
            .header("Content-Type", "application/octet-stream")
            .header("Authorization", item.authorization)
            .body(item.body)
            .send()
            .await;
        if let Ok(resp) = res
            && matches!(resp.status().as_u16(), 404 | 410)
        {
            dead.push(item.endpoint);
        }
    }
    dead
}

#[cfg(test)]
mod tests {
    use super::*;
    use p256::ecdsa::VerifyingKey;
    use p256::ecdsa::signature::Verifier;

    fn test_subscriber_keys() -> (SecretKey, [u8; 16], PushKeys) {
        let ua_secret = generate_p256_secret_key();
        let ua_pub = uncompressed_sec1_bytes(&ua_secret);
        let auth = [0x42u8; 16];
        let keys = PushKeys {
            p256dh: URL_SAFE_NO_PAD.encode(ua_pub),
            auth: URL_SAFE_NO_PAD.encode(auth),
        };
        (ua_secret, auth, keys)
    }

    #[test]
    fn push_store_validates_endpoints_and_key_lengths() {
        assert!(service_ok("https://fcm.googleapis.com/fcm/send/abc"));
        assert!(service_ok("https://web.push.apple.com/xyz"));
        assert!(!service_ok("https://evil.example.com/push"));
        assert!(!service_ok("http://fcm.googleapis.com/insecure"));

        let dir = tempfile::tempdir().unwrap();
        let mut st = PushStore::load(dir.path());
        let (_, _, keys) = test_subscriber_keys();
        let ok = st.add(
            PushSubscription {
                endpoint: "https://fcm.googleapis.com/fcm/send/1".into(),
                keys,
                ua: "iPhone".into(),
                at: 0,
            },
            1000,
        );
        assert!(ok);
        assert_eq!(st.count(), 1);
        assert!(st.remove("https://fcm.googleapis.com/fcm/send/1"));
        assert_eq!(st.count(), 0);

        // Off-curve 65-byte p256dh must be rejected
        let bogus_ok = st.add(
            PushSubscription {
                endpoint: "https://fcm.googleapis.com/fcm/send/2".into(),
                keys: PushKeys {
                    p256dh: URL_SAFE_NO_PAD.encode([4u8; 65]),
                    auth: URL_SAFE_NO_PAD.encode([1u8; 16]),
                },
                ua: "Bad".into(),
                at: 0,
            },
            1000,
        );
        assert!(!bogus_ok);
    }

    #[test]
    fn push_store_generates_valid_p256_vapid_keypair_and_replaces_legacy_bogus_keys() {
        let dir = tempfile::tempdir().unwrap();
        let st = PushStore::load(dir.path());
        let pub_bytes = URL_SAFE_NO_PAD
            .decode(&st.public_key)
            .expect("public_key must be base64url");
        assert_eq!(pub_bytes.len(), 65, "uncompressed SEC1 P-256 key is 65 bytes");
        assert!(
            p256::PublicKey::from_sec1_bytes(&pub_bytes).is_ok(),
            "generated VAPID publicKey must be a valid NIST P-256 curve point"
        );
        let vapid_json: serde_json::Value =
            serde_json::from_str(&std::fs::read_to_string(dir.path().join("vapid.json")).unwrap())
                .unwrap();
        assert!(
            vapid_json.get("privateKey").and_then(|v| v.as_str()).is_some(),
            "vapid.json must persist privateKey alongside publicKey"
        );

        // Reloading must preserve the valid keypair
        let st2 = PushStore::load(dir.path());
        assert_eq!(st2.public_key, st.public_key);

        // Legacy bogus non-curve random key on disk must be automatically regenerated
        let dir_legacy = tempfile::tempdir().unwrap();
        let mut bogus = [0u8; 65];
        bogus[0] = 4;
        for (i, b) in bogus[1..].iter_mut().enumerate() {
            *b = (i as u8).wrapping_mul(7).wrapping_add(3);
        }
        assert!(
            p256::PublicKey::from_sec1_bytes(&bogus).is_err(),
            "fixture must be off-curve"
        );
        let bogus_b64 = URL_SAFE_NO_PAD.encode(bogus);
        std::fs::write(
            dir_legacy.path().join("vapid.json"),
            serde_json::json!({ "publicKey": bogus_b64 }).to_string(),
        )
        .unwrap();

        let repaired = PushStore::load(dir_legacy.path());
        assert_ne!(repaired.public_key, bogus_b64, "legacy bogus key must be replaced");
        let repaired_bytes = URL_SAFE_NO_PAD.decode(&repaired.public_key).unwrap();
        assert!(
            p256::PublicKey::from_sec1_bytes(&repaired_bytes).is_ok(),
            "repaired VAPID publicKey must be a valid NIST P-256 point"
        );
    }

    #[test]
    fn push_service_ok_blocks_ssrf_userinfo_and_non_standard_ports() {
        assert!(!service_ok("https://fcm.googleapis.com@127.0.0.1/push"));
        assert!(!service_ok("https://fcm.googleapis.com:8443/fcm/send/1"));
        assert!(!service_ok("https://127.0.0.1/push"));
        assert!(!service_ok("http://127.0.0.1:4310/event"));
        assert!(service_ok("https://fcm.googleapis.com:443/fcm/send/1"));
    }

    #[test]
    fn rfc8291_aes128gcm_roundtrip_decrypts_with_subscriber_private_key() {
        let (ua_secret, auth_secret, keys) = test_subscriber_keys();
        let ua_pub_bytes = uncompressed_sec1_bytes(&ua_secret);
        let plaintext = br#"{"title":"Approve Bash","body":"cargo test","urgent":true}"#;

        let wire = encrypt_aes128gcm(&keys, plaintext).expect("encryption must succeed");
        assert!(wire.len() > 86, "wire format must include 86-byte header + ciphertext");

        // Parse RFC 8188 / RFC 8291 header
        let salt = &wire[0..16];
        let rs = u32::from_be_bytes(wire[16..20].try_into().unwrap());
        let idlen = wire[20] as usize;
        assert_eq!(rs, 4096);
        assert_eq!(idlen, 65);
        let as_pub_bytes = &wire[21..21 + idlen];
        let ciphertext = &wire[21 + idlen..];
        assert!(!ciphertext.is_empty());

        let as_pub = PublicKey::from_sec1_bytes(as_pub_bytes)
            .expect("header keyid must be a valid ephemeral P-256 public key");

        // Subscriber decrypts using its own private key + auth_secret
        let shared = diffie_hellman(ua_secret.to_nonzero_scalar(), as_pub.as_affine());
        let mut key_info = Vec::with_capacity(14 + 65 + 65);
        key_info.extend_from_slice(b"WebPush: info\0");
        key_info.extend_from_slice(&ua_pub_bytes);
        key_info.extend_from_slice(as_pub_bytes);

        let hkdf_auth = Hkdf::<Sha256>::new(Some(&auth_secret), shared.raw_secret_bytes());
        let mut ikm = [0u8; 32];
        hkdf_auth.expand(&key_info, &mut ikm).unwrap();

        let hkdf_content = Hkdf::<Sha256>::new(Some(salt), &ikm);
        let mut cek = [0u8; 16];
        hkdf_content
            .expand(b"Content-Encoding: aes128gcm\0", &mut cek)
            .unwrap();
        let mut nonce = [0u8; 12];
        hkdf_content
            .expand(b"Content-Encoding: nonce\0", &mut nonce)
            .unwrap();

        let cipher = Aes128Gcm::new_from_slice(&cek).unwrap();
        let mut decrypted = cipher
            .decrypt(Nonce::from_slice(&nonce), ciphertext)
            .expect("subscriber private key must decrypt RFC 8291 ciphertext");
        while decrypted.last() == Some(&0x00) {
            decrypted.pop();
        }
        assert_eq!(decrypted.pop(), Some(0x02), "final record delimiter must be 0x02");
        assert_eq!(decrypted.as_slice(), plaintext);
    }

    #[test]
    fn vapid_es256_jwt_is_well_formed_and_verifies_with_p256_verifying_key() {
        let dir = tempfile::tempdir().unwrap();
        let st = PushStore::load(dir.path());
        let now_secs = 1_750_000_000u64;
        let aud = "https://fcm.googleapis.com";
        let jwt = st.vapid_jwt(aud, now_secs).expect("jwt signing must succeed");

        let parts: Vec<&str> = jwt.split('.').collect();
        assert_eq!(parts.len(), 3, "JWT must have header.payload.signature");

        let header: serde_json::Value =
            serde_json::from_slice(&URL_SAFE_NO_PAD.decode(parts[0]).unwrap()).unwrap();
        assert_eq!(header["typ"], "JWT");
        assert_eq!(header["alg"], "ES256");

        let payload: serde_json::Value =
            serde_json::from_slice(&URL_SAFE_NO_PAD.decode(parts[1]).unwrap()).unwrap();
        assert_eq!(payload["aud"], aud);
        assert_eq!(payload["exp"], now_secs + VAPID_TTL_SECS);
        assert!(payload["sub"].as_str().unwrap().starts_with("mailto:"));

        let sig_bytes = URL_SAFE_NO_PAD.decode(parts[2]).unwrap();
        assert_eq!(sig_bytes.len(), 64, "ES256 JWS signature must be raw 64-byte r||s");
        let sig = Signature::from_slice(&sig_bytes).expect("valid P-256 signature");

        let pub_bytes = URL_SAFE_NO_PAD.decode(&st.public_key).unwrap();
        let pk = PublicKey::from_sec1_bytes(&pub_bytes).unwrap();
        let vk = VerifyingKey::from(&pk);
        let signing_input = format!("{}.{}", parts[0], parts[1]);
        assert!(
            vk.verify(signing_input.as_bytes(), &sig).is_ok(),
            "ES256 VAPID JWT must verify with VAPID public key"
        );
    }

    #[test]
    fn rfc8291_enforces_single_record_payload_limit() {
        let (_, _, keys) = test_subscriber_keys();

        // Exact RFC 8188 single-record boundary: 4079 bytes (+ 1 delimiter + 16 tag == 4096) succeeds
        let max_plain = vec![b'a'; MAX_SINGLE_RECORD_PLAINTEXT];
        assert_eq!(max_plain.len(), 4079);
        let wire = encrypt_aes128gcm(&keys, &max_plain).expect("4079-byte payload must succeed");
        assert_eq!(wire.len(), 86 + RECORD_SIZE as usize);

        // 4080 bytes exceeds single-record rs=4096 and must return Err
        let over_plain = vec![b'a'; MAX_SINGLE_RECORD_PLAINTEXT + 1];
        assert_eq!(over_plain.len(), 4080);
        assert!(
            encrypt_aes128gcm(&keys, &over_plain).is_err(),
            "4080-byte payload must be rejected"
        );
    }
}
