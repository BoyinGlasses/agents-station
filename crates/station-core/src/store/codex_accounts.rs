use crate::atomic::{write_bytes_atomic, write_json_atomic};
use crate::config::home_dir;
use crate::state::sessions::{now_ms, one_line};
use base64::Engine;
use base64::engine::general_purpose::{URL_SAFE, URL_SAFE_NO_PAD};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use std::path::{Path, PathBuf};

/// Strict account slot ID validation (`^[A-Za-z0-9_-]{1,64}$`) preventing path traversal.
pub fn is_safe_id(id: &str) -> bool {
    !id.is_empty()
        && id.len() <= 64
        && id
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b == b'_' || b == b'-')
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct AiAccountSlot {
    pub slug: String,
    pub label: String,
    #[serde(default)]
    pub email: String,
    #[serde(default)]
    pub active: bool,
    #[serde(default)]
    pub later: bool,
    #[serde(default)]
    pub key: String,
    #[serde(default)]
    pub name: String,
    #[serde(default)]
    pub plan: String,
    #[serde(default)]
    pub note: String,
    #[serde(default)]
    pub at: u64,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CodexIdentity {
    pub key: String,
    pub email: String,
    pub name: String,
    pub plan: String,
}

/// Decodes a JWT's payload claims (unverified, for account naming/keying only).
pub fn jwt_claims(token: &str) -> Value {
    let Some(part) = token.split('.').nth(1) else {
        return json!({});
    };
    let decoded = URL_SAFE_NO_PAD
        .decode(part)
        .or_else(|_| URL_SAFE.decode(part))
        .ok();
    decoded
        .and_then(|bytes| serde_json::from_slice::<Value>(&bytes).ok())
        .filter(|v| v.is_object())
        .unwrap_or_else(|| json!({}))
}

/// Extracts ChatGPT account identity from `~/.codex/auth.json`.
pub fn codex_identity(auth: &Value) -> Option<CodexIdentity> {
    let tokens = auth.get("tokens")?;
    let id_tok = tokens.get("id_token").and_then(Value::as_str).unwrap_or("");
    let acc_tok = tokens
        .get("access_token")
        .and_then(Value::as_str)
        .unwrap_or("");
    if id_tok.is_empty() && acc_tok.is_empty() {
        return None;
    }
    let claims = jwt_claims(id_tok);
    let auth_ns = claims
        .get("https://api.openai.com/auth")
        .cloned()
        .or_else(|| jwt_claims(acc_tok).get("https://api.openai.com/auth").cloned())
        .unwrap_or_else(|| json!({}));

    let account = tokens
        .get("account_id")
        .and_then(Value::as_str)
        .or_else(|| auth_ns.get("chatgpt_account_id").and_then(Value::as_str))
        .unwrap_or("");
    let user = auth_ns
        .get("chatgpt_user_id")
        .or_else(|| auth_ns.get("user_id"))
        .and_then(Value::as_str)
        .or_else(|| claims.get("sub").and_then(Value::as_str))
        .unwrap_or("");
    let email = claims.get("email").and_then(Value::as_str).unwrap_or("");
    if account.is_empty() && user.is_empty() && email.is_empty() {
        return None;
    }
    let name = claims.get("name").and_then(Value::as_str).unwrap_or("");
    let plan = auth_ns
        .get("chatgpt_plan_type")
        .and_then(Value::as_str)
        .unwrap_or("");
    Some(CodexIdentity {
        key: format!("{user}|{account}|{email}"),
        email: email.to_owned(),
        name: name.to_owned(),
        plan: plan.to_owned(),
    })
}

fn short_id_hash(key: &str) -> String {
    let mut hasher = Sha256::new();
    hasher.update(key.as_bytes());
    let digest = hasher.finalize();
    let mut hex = String::with_capacity(10);
    for byte in &digest[..5] {
        use std::fmt::Write;
        let _ = write!(&mut hex, "{byte:02x}");
    }
    hex
}

fn default_codex_seat_dir() -> PathBuf {
    std::env::var("CODEX_HOME")
        .ok()
        .filter(|s| !s.trim().is_empty())
        .map(PathBuf::from)
        .unwrap_or_else(|| home_dir().join(".codex"))
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
struct TenureEntry {
    id: Option<String>,
    from: u64,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
struct AccountsFile {
    auto: bool,
    accounts: Vec<AiAccountSlot>,
    #[serde(default)]
    tenure: Vec<TenureEntry>,
}

pub struct CodexAccounts {
    file: PathBuf,
    copies_dir: PathBuf,
    seat_dir: PathBuf,
    data: AccountsFile,
    seat_id: Option<String>,
    seat_text: Option<String>,
    seat_note: String,
}

impl CodexAccounts {
    pub fn load(data_dir: &Path) -> Self {
        Self::load_with_seat(data_dir, default_codex_seat_dir())
    }

    pub fn load_with_seat(data_dir: &Path, seat_dir: PathBuf) -> Self {
        let file = data_dir.join("codex-accounts.json");
        let copies_dir = data_dir.join("codex-accounts");
        let mut data = std::fs::read_to_string(&file)
            .ok()
            .and_then(|s| serde_json::from_str::<AccountsFile>(&s).ok())
            .unwrap_or_default();
        data.accounts.retain(|a| is_safe_id(&a.slug));
        data.tenure
            .retain(|t| t.id.as_deref().is_none_or(is_safe_id));
        let seat_id = data
            .accounts
            .iter()
            .find(|a| a.active)
            .map(|a| a.slug.clone());
        Self {
            file,
            copies_dir,
            seat_dir,
            data,
            seat_id,
            seat_text: None,
            seat_note: String::new(),
        }
    }

    fn seat_auth_file(&self) -> PathBuf {
        self.seat_dir.join("auth.json")
    }

    fn copy_auth_file(&self, id: &str) -> Option<PathBuf> {
        is_safe_id(id).then(|| self.copies_dir.join(id).join("auth.json"))
    }

    fn keep_copy(&self, id: &str, text: &str) {
        let Some(path) = self.copy_auth_file(id) else {
            return;
        };
        let prev = std::fs::read_to_string(&path).ok();
        if prev.as_deref() != Some(text) {
            let _ = write_bytes_atomic(&path, text.as_bytes());
        }
    }

    fn persist(&self) {
        let _ = write_json_atomic(&self.file, &self.data);
    }

    fn record_tenure(&mut self, id: Option<String>) {
        if self.data.tenure.last().map(|t| t.id.as_deref()) != Some(id.as_deref()) {
            let from = if self.data.tenure.is_empty() { 0 } else { now_ms() };
            self.data.tenure.push(TenureEntry { id, from });
            if self.data.tenure.len() > 12 {
                let drain = self.data.tenure.len() - 12;
                self.data.tenure.drain(..drain);
            }
        }
    }

    /// Reads `$CODEX_HOME/auth.json`, discovers any newly signed-in account,
    /// and saves the newest rotated tokens back to that account's own copy.
    pub fn sync_seat(&mut self) {
        let text = std::fs::read_to_string(self.seat_auth_file()).unwrap_or_default();
        if self.seat_text.as_deref() == Some(text.as_str()) {
            return;
        }
        self.seat_text = Some(text.clone());

        let auth: Option<Value> = serde_json::from_str(&text).ok();
        let who = auth.as_ref().and_then(codex_identity);
        self.seat_note = if text.is_empty() {
            "signedOut".to_owned()
        } else if who.is_none() {
            if auth
                .as_ref()
                .and_then(|v| v.get("OPENAI_API_KEY"))
                .and_then(Value::as_str)
                .is_some_and(|s| !s.is_empty())
            {
                "apiKey".to_owned()
            } else {
                "unknown".to_owned()
            }
        } else {
            String::new()
        };

        let mut active_id = None;
        if let Some(id_info) = who {
            let idx = self.data.accounts.iter().position(|a| a.key == id_info.key);
            let slug = if let Some(i) = idx {
                let a = &mut self.data.accounts[i];
                if !id_info.email.is_empty() {
                    a.email = id_info.email;
                }
                if !id_info.name.is_empty() {
                    a.name = id_info.name;
                }
                if !id_info.plan.is_empty() {
                    a.plan = id_info.plan;
                }
                a.slug.clone()
            } else {
                let mut slug = short_id_hash(&id_info.key);
                while self.data.accounts.iter().any(|a| a.slug == slug) {
                    slug.push('x');
                }
                self.data.accounts.push(AiAccountSlot {
                    slug: slug.clone(),
                    label: String::new(),
                    email: id_info.email,
                    active: true,
                    later: false,
                    key: id_info.key,
                    name: id_info.name,
                    plan: id_info.plan,
                    note: String::new(),
                    at: now_ms(),
                });
                slug
            };
            self.keep_copy(&slug, &text);
            active_id = Some(slug);
        }

        for a in &mut self.data.accounts {
            a.active = active_id.as_deref() == Some(a.slug.as_str());
        }
        self.seat_id = active_id.clone();
        self.record_tenure(active_id);
        self.persist();
    }

    pub fn view(&self) -> Value {
        json!({
            "auto": self.data.auto,
            "seat": self.seat_note,
            "now": self.seat_id,
            "accounts": self.data.accounts,
        })
    }

    fn target_id<'a>(body: &'a Value) -> &'a str {
        body.get("id")
            .or_else(|| body.get("slug"))
            .and_then(Value::as_str)
            .unwrap_or("")
    }

    pub fn mutate(&mut self, action: &str, body: &Value) -> Value {
        match action {
            "auto" => {
                self.data.auto = body
                    .get("on")
                    .and_then(Value::as_bool)
                    .unwrap_or(!self.data.auto);
                self.persist();
                json!({ "ok": true, "auto": self.data.auto })
            }
            "add" => {
                self.sync_seat();
                if let Some(auth_val) = body.get("auth")
                    && let Ok(text) = serde_json::to_string(auth_val)
                    && let Some(who) = codex_identity(auth_val)
                {
                    let slug = if let Some(a) = self.data.accounts.iter_mut().find(|a| a.key == who.key) {
                        a.email = who.email;
                        a.name = who.name;
                        a.plan = who.plan;
                        a.slug.clone()
                    } else {
                        let mut s = short_id_hash(&who.key);
                        while self.data.accounts.iter().any(|a| a.slug == s) {
                            s.push('x');
                        }
                        self.data.accounts.push(AiAccountSlot {
                            slug: s.clone(),
                            label: body
                                .get("label")
                                .and_then(Value::as_str)
                                .map(|l| one_line(l).chars().take(24).collect())
                                .unwrap_or_default(),
                            email: who.email,
                            active: self.data.accounts.is_empty(),
                            later: false,
                            key: who.key,
                            name: who.name,
                            plan: who.plan,
                            note: String::new(),
                            at: now_ms(),
                        });
                        s
                    };
                    self.keep_copy(&slug, &text);
                    self.persist();
                    return json!({ "ok": true, "id": slug, "slug": slug });
                }
                let label: String = one_line(
                    body.get("label")
                        .and_then(Value::as_str)
                        .unwrap_or("Codex"),
                )
                .chars()
                .take(24)
                .collect();
                let slug = format!("codex-{}", self.data.accounts.len() + 1);
                self.data.accounts.push(AiAccountSlot {
                    slug: slug.clone(),
                    label,
                    email: body
                        .get("email")
                        .and_then(Value::as_str)
                        .unwrap_or("")
                        .to_owned(),
                    active: self.data.accounts.is_empty(),
                    later: false,
                    key: String::new(),
                    name: String::new(),
                    plan: String::new(),
                    note: String::new(),
                    at: now_ms(),
                });
                self.persist();
                json!({ "ok": true, "id": slug, "slug": slug })
            }
            "use" => {
                let target = Self::target_id(body).to_owned();
                if !is_safe_id(&target) {
                    return json!({ "ok": false, "code": "badId" });
                }
                self.sync_seat();
                if !self.data.accounts.iter().any(|a| a.slug == target) {
                    return json!({ "ok": false, "code": "noAccount" });
                }
                if self.seat_id.as_deref() == Some(target.as_str()) {
                    return json!({ "ok": true, "same": true, "id": target });
                }
                let Some(copy_path) = self.copy_auth_file(&target) else {
                    return json!({ "ok": false, "code": "badId" });
                };
                if !copy_path.is_file() {
                    return json!({ "ok": false, "code": "noCopy" });
                }
                let Ok(text) = std::fs::read_to_string(&copy_path) else {
                    return json!({ "ok": false, "code": "noCopy" });
                };
                if write_bytes_atomic(&self.seat_auth_file(), text.as_bytes()).is_err() {
                    return json!({ "ok": false, "code": "seatFailed" });
                }
                self.seat_text = Some(text);
                self.seat_note.clear();
                let from = self.seat_id.clone();
                self.seat_id = Some(target.clone());
                for a in &mut self.data.accounts {
                    a.active = a.slug == target;
                }
                self.record_tenure(Some(target.clone()));
                self.persist();
                json!({ "ok": true, "id": target, "from": from })
            }
            "rename" => {
                let target = Self::target_id(body);
                if !is_safe_id(target) {
                    return json!({ "ok": false, "code": "badId" });
                }
                let label: String = one_line(body.get("label").and_then(Value::as_str).unwrap_or(""))
                    .chars()
                    .take(24)
                    .collect();
                let mut found = false;
                for a in &mut self.data.accounts {
                    if a.slug == target && !label.is_empty() {
                        a.label = label.clone();
                        found = true;
                    }
                }
                self.persist();
                json!({ "ok": found })
            }
            "remove" => {
                let target = Self::target_id(body);
                if !is_safe_id(target) {
                    return json!({ "ok": false, "code": "badId" });
                }
                if !self.data.accounts.iter().any(|a| a.slug == target) {
                    return json!({ "ok": false, "code": "noAccount" });
                }
                if self.seat_id.as_deref() == Some(target) {
                    return json!({ "ok": false, "code": "inSeat" });
                }
                self.data.accounts.retain(|a| a.slug != target);
                let dir = self.copies_dir.join(target);
                if dir.is_dir() {
                    let _ = std::fs::remove_dir_all(&dir);
                }
                self.persist();
                json!({ "ok": true })
            }
            "order" => {
                if let Some(arr) = body.get("order").and_then(Value::as_array) {
                    let order: Vec<&str> = arr.iter().filter_map(Value::as_str).collect();
                    self.data.accounts.sort_by_key(|a| {
                        order
                            .iter()
                            .position(|&s| s == a.slug)
                            .unwrap_or(usize::MAX)
                    });
                    self.persist();
                }
                json!({ "ok": true })
            }
            "later" => {
                let target = Self::target_id(body);
                for a in &mut self.data.accounts {
                    if a.slug == target {
                        a.later = body.get("on").and_then(Value::as_bool).unwrap_or(!a.later);
                    }
                }
                self.persist();
                json!({ "ok": true })
            }
            "read" => {
                self.sync_seat();
                json!({ "ok": true, "now": self.seat_id, "accounts": self.data.accounts })
            }
            "cancel" => json!({ "ok": true, "accounts": self.data.accounts }),
            _ => json!({ "ok": false, "error": "unknown codex action" }),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use base64::Engine;
    use base64::engine::general_purpose::URL_SAFE_NO_PAD;

    fn fake_jwt(claims: &Value) -> String {
        let h = URL_SAFE_NO_PAD.encode(r#"{"alg":"none","typ":"JWT"}"#);
        let p = URL_SAFE_NO_PAD.encode(claims.to_string());
        format!("{h}.{p}.sig")
    }

    fn auth_json(email: &str, sub: &str, acc: &str, plan: &str, refresh: &str) -> String {
        let id_tok = fake_jwt(&json!({
            "email": email,
            "name": email.split('@').next().unwrap_or(""),
            "sub": sub,
            "https://api.openai.com/auth": {
                "chatgpt_account_id": acc,
                "chatgpt_plan_type": plan,
            }
        }));
        json!({
            "tokens": {
                "id_token": id_tok,
                "access_token": id_tok,
                "refresh_token": refresh,
                "account_id": acc
            }
        })
        .to_string()
    }

    #[test]
    fn codex_accounts_sync_seat_and_swaps_auth_json_preserving_rotated_tokens() {
        let dir = tempfile::tempdir().unwrap();
        let data_dir = dir.path().join("data");
        let seat_dir = dir.path().join("codex-seat");
        std::fs::create_dir_all(&seat_dir).unwrap();

        let alice_auth = auth_json("alice@example.com", "u1", "acc-1", "plus", "ref-alice-1");
        std::fs::write(seat_dir.join("auth.json"), &alice_auth).unwrap();

        let mut mgr = CodexAccounts::load_with_seat(&data_dir, seat_dir.clone());
        mgr.sync_seat();
        let view = mgr.view();
        assert_eq!(view["accounts"].as_array().unwrap().len(), 1);
        let alice_id = view["now"].as_str().unwrap().to_owned();
        assert_eq!(view["accounts"][0]["email"], "alice@example.com");

        // Bob signs into seat
        let bob_v1 = auth_json("bob@example.com", "u2", "acc-2", "pro", "ref-bob-v1");
        std::fs::write(seat_dir.join("auth.json"), &bob_v1).unwrap();
        mgr.sync_seat();
        let bob_id = mgr.view()["now"].as_str().unwrap().to_owned();
        assert_ne!(bob_id, alice_id);

        // Codex rotates Bob's token in the seat before we switch back to Alice
        let bob_v2 = auth_json("bob@example.com", "u2", "acc-2", "pro", "ref-bob-v2");
        std::fs::write(seat_dir.join("auth.json"), &bob_v2).unwrap();

        let sw = mgr.mutate("use", &json!({ "id": alice_id }));
        assert_eq!(sw["ok"], true);
        let seat_after = std::fs::read_to_string(seat_dir.join("auth.json")).unwrap();
        assert!(seat_after.contains("ref-alice-1"), "seat should have Alice");

        // Bob's rotated token must be preserved in Bob's own copy
        let bob_copy =
            std::fs::read_to_string(data_dir.join("codex-accounts").join(&bob_id).join("auth.json"))
                .unwrap();
        assert!(bob_copy.contains("ref-bob-v2"), "bob copy should keep rotated token");

        // Cannot remove active seat, can remove inactive Bob
        assert_eq!(mgr.mutate("remove", &json!({ "id": alice_id }))["code"], "inSeat");
        assert_eq!(mgr.mutate("remove", &json!({ "id": bob_id }))["ok"], true);
        assert!(!data_dir.join("codex-accounts").join(&bob_id).exists());
    }

    #[test]
    fn codex_accounts_rejects_unsnapshotted_switch_and_unsafe_ids() {
        let dir = tempfile::tempdir().unwrap();
        let data_dir = dir.path().join("data");
        let seat_dir = dir.path().join("codex-seat");
        let victim_dir = dir.path().join("victim-outside");
        std::fs::create_dir_all(&seat_dir).unwrap();
        std::fs::create_dir_all(&victim_dir).unwrap();

        let alice_auth = auth_json("alice@example.com", "u1", "acc-1", "plus", "ref-alice-1");
        std::fs::write(seat_dir.join("auth.json"), &alice_auth).unwrap();

        let mut mgr = CodexAccounts::load_with_seat(&data_dir, seat_dir.clone());
        mgr.sync_seat();
        let alice_id = mgr.view()["now"].as_str().unwrap().to_owned();

        // Add an unsnapshotted slot manually and try switching to it -> must fail with noCopy
        let added = mgr.mutate("add", &json!({ "label": "Empty" }));
        let empty_id = added["id"].as_str().unwrap().to_owned();
        let sw = mgr.mutate("use", &json!({ "id": empty_id }));
        assert_eq!(sw["ok"], false);
        assert_eq!(sw["code"], "noCopy");
        assert_eq!(mgr.view()["now"].as_str(), Some(alice_id.as_str()));
        assert!(std::fs::read_to_string(seat_dir.join("auth.json")).unwrap().contains("ref-alice-1"));

        // Crafted state file with traversal slug must be stripped on load and rejected on mutate
        std::fs::write(
            data_dir.join("codex-accounts.json"),
            json!({
                "auto": false,
                "accounts": [
                    { "slug": "../../victim-outside", "label": "bad", "email": "", "active": false, "later": false }
                ]
            })
            .to_string(),
        )
        .unwrap();
        let mut reloaded = CodexAccounts::load_with_seat(&data_dir, seat_dir);
        assert!(reloaded.view()["accounts"].as_array().unwrap().is_empty());
        assert_eq!(
            reloaded.mutate("remove", &json!({ "id": "../../victim-outside" }))["ok"],
            false
        );
        assert!(victim_dir.is_dir(), "outside directory must never be deleted");
    }
}
