use crate::atomic::{write_bytes_atomic, write_json_atomic};
use crate::config::home_dir;
use crate::state::sessions::{now_ms, one_line};
use crate::store::codex_accounts::AiAccountSlot;
use base64::Engine;
use base64::engine::general_purpose::STANDARD;
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

#[cfg(windows)]
use std::os::windows::process::CommandExt;

#[cfg(windows)]
const CREATE_NO_WINDOW: u32 = 0x0800_0000;

pub const KEY_TOKEN: &str = "antigravityUnifiedStateSync.oauthToken";
pub const KEY_STATUS: &str = "antigravityUnifiedStateSync.userStatus";
pub const KEY_PREFS: &str = "antigravityUnifiedStateSync.modelPreferences";

#[derive(Debug, Clone)]
pub enum PbVal<'a> {
    Varint(u64),
    Bytes(&'a [u8]),
}

#[derive(Debug, Clone)]
pub struct PbField<'a> {
    pub f: u32,
    pub v: PbVal<'a>,
}

fn read_varint(buf: &[u8], mut i: usize) -> Option<(u64, usize)> {
    let mut n: u64 = 0;
    let mut shift = 0u32;
    while i < buf.len() {
        let c = buf[i];
        i += 1;
        n |= u64::from(c & 0x7f).checked_shl(shift)?;
        if c < 0x80 {
            return Some((n, i));
        }
        shift += 7;
        if shift >= 64 {
            return None;
        }
    }
    None
}

/// Parses wire-format protobuf fields without a schema (matching Oracle `pbFields`).
pub fn pb_fields(buf: &[u8]) -> Option<Vec<PbField<'_>>> {
    let mut out = Vec::new();
    let mut i = 0usize;
    while i < buf.len() {
        let (k, next) = read_varint(buf, i)?;
        i = next;
        let f = (k >> 3) as u32;
        let w = (k & 7) as u8;
        if f == 0 {
            return None;
        }
        match w {
            0 => {
                let (v, next) = read_varint(buf, i)?;
                i = next;
                out.push(PbField {
                    f,
                    v: PbVal::Varint(v),
                });
            }
            1 => {
                if i + 8 > buf.len() {
                    return None;
                }
                out.push(PbField {
                    f,
                    v: PbVal::Bytes(&buf[i..i + 8]),
                });
                i += 8;
            }
            2 => {
                let (len, next) = read_varint(buf, i)?;
                i = next;
                let end = i.checked_add(len as usize)?;
                if end > buf.len() {
                    return None;
                }
                out.push(PbField {
                    f,
                    v: PbVal::Bytes(&buf[i..end]),
                });
                i = end;
            }
            5 => {
                if i + 4 > buf.len() {
                    return None;
                }
                out.push(PbField {
                    f,
                    v: PbVal::Bytes(&buf[i..i + 4]),
                });
                i += 4;
            }
            _ => return None,
        }
    }
    Some(out)
}

fn pb_bytes<'a>(fields: &'a [PbField<'a>], num: u32) -> Option<&'a [u8]> {
    fields.iter().find_map(|x| match (&x.v, x.f == num) {
        (PbVal::Bytes(b), true) => Some(*b),
        _ => None,
    })
}

fn pb_varint(fields: &[PbField<'_>], num: u32) -> Option<u64> {
    fields.iter().find_map(|x| match (&x.v, x.f == num) {
        (PbVal::Varint(n), true) => Some(*n),
        _ => None,
    })
}

fn pb_text(fields: &[PbField<'_>], num: u32) -> String {
    pb_bytes(fields, num)
        .map(|b| String::from_utf8_lossy(b).into_owned())
        .unwrap_or_default()
}

pub fn unwrap_sync_value(value: &str) -> HashMap<String, Vec<u8>> {
    let mut map = HashMap::new();
    let Ok(raw) = STANDARD.decode(value.trim()) else {
        return map;
    };
    let Some(root) = pb_fields(&raw) else {
        return map;
    };
    for entry in root {
        if entry.f != 1 {
            continue;
        }
        let PbVal::Bytes(eb) = entry.v else {
            continue;
        };
        let Some(ef) = pb_fields(eb) else {
            continue;
        };
        let name = pb_text(&ef, 1);
        let Some(val_wrap) = pb_bytes(&ef, 2).and_then(pb_fields) else {
            continue;
        };
        let inner_b64 = pb_text(&val_wrap, 1);
        if let Ok(decoded) = STANDARD.decode(inner_b64.trim()) {
            map.insert(name, decoded);
        }
    }
    map
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AgToken {
    pub access: String,
    pub refresh: String,
    pub expires: u64,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AgStatus {
    pub name: String,
    pub email: String,
    pub tier: String,
    pub tier_name: String,
}

pub fn token_of(value: &str) -> Option<AgToken> {
    let map = unwrap_sync_value(value);
    let raw = map.get("oauthTokenInfoSentinelKey")?;
    if raw.is_empty() {
        return None;
    }
    let fields = pb_fields(raw)?;
    let expires = pb_bytes(&fields, 4)
        .and_then(pb_fields)
        .and_then(|ef| pb_varint(&ef, 1))
        .unwrap_or(0)
        .saturating_mul(1000);
    Some(AgToken {
        access: pb_text(&fields, 1),
        refresh: pb_text(&fields, 3),
        expires,
    })
}

pub fn status_of(value: &str) -> Option<AgStatus> {
    let map = unwrap_sync_value(value);
    let raw = map.get("userStatusSentinelKey")?;
    if raw.is_empty() {
        return None;
    }
    let fields = pb_fields(raw)?;
    let plan_fields = pb_bytes(&fields, 36).and_then(pb_fields).unwrap_or_default();
    Some(AgStatus {
        name: pb_text(&fields, 3),
        email: pb_text(&fields, 7),
        tier: pb_text(&plan_fields, 1),
        tier_name: pb_text(&plan_fields, 2),
    })
}

fn sha256_prefix(bytes: &[u8], hex_chars: usize) -> String {
    let mut hasher = Sha256::new();
    hasher.update(bytes);
    let digest = hasher.finalize();
    let take = (hex_chars / 2).min(digest.len());
    let mut out = String::with_capacity(take * 2);
    for byte in &digest[..take] {
        use std::fmt::Write;
        let _ = write!(&mut out, "{byte:02x}");
    }
    out
}

fn sql_quote(s: &str) -> String {
    format!("'{}'", s.replace('\'', "''"))
}

fn is_sqlite_header(bytes: &[u8]) -> bool {
    bytes.starts_with(b"SQLite format 3\0")
}

fn sqlite_read_keys(vscdb: &Path, keys: &[&str]) -> Option<HashMap<String, String>> {
    let key_list = keys.iter().map(|k| sql_quote(k)).collect::<Vec<_>>().join(", ");
    let query = format!(
        "SELECT key, CAST(value AS TEXT) AS value FROM ItemTable WHERE key IN ({key_list})"
    );
    let mut cmd = Command::new("sqlite3");
    cmd.args(["-readonly", "-json"])
        .arg(vscdb)
        .arg(&query)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::null());
    #[cfg(windows)]
    cmd.creation_flags(CREATE_NO_WINDOW);
    let out = cmd.output().ok()?;
    if !out.status.success() {
        return None;
    }
    let text = String::from_utf8_lossy(&out.stdout);
    let rows: Vec<Value> = if text.trim().is_empty() {
        Vec::new()
    } else {
        serde_json::from_str(text.trim()).ok()?
    };
    let mut map = HashMap::new();
    for row in rows {
        if let (Some(k), Some(v)) = (
            row.get("key").and_then(Value::as_str),
            row.get("value").and_then(Value::as_str),
        ) {
            map.insert(k.to_owned(), v.to_owned());
        }
    }
    Some(map)
}

fn sqlite_write_keys(vscdb: &Path, set: &[(&str, &str)]) -> bool {
    use std::io::Write;
    let mut sql = String::from("BEGIN;\n");
    for (k, v) in set {
        sql.push_str(&format!(
            "INSERT OR REPLACE INTO ItemTable (key, value) VALUES ({}, {});\n",
            sql_quote(k),
            sql_quote(v)
        ));
    }
    sql.push_str("COMMIT;\n");

    let mut cmd = Command::new("sqlite3");
    cmd.arg(vscdb)
        .stdin(Stdio::piped())
        .stdout(Stdio::null())
        .stderr(Stdio::null());
    #[cfg(windows)]
    cmd.creation_flags(CREATE_NO_WINDOW);
    let Ok(mut child) = cmd.spawn() else {
        return false;
    };
    if let Some(mut stdin) = child.stdin.take() {
        let _ = stdin.write_all(sql.as_bytes());
    }
    child.wait().is_ok_and(|s| s.success())
}

fn default_vscdb_path() -> PathBuf {
    if let Ok(ov) = std::env::var("ANTIGRAVITY_VSCDB")
        && !ov.trim().is_empty()
    {
        return PathBuf::from(ov);
    }
    let appdata = std::env::var("APPDATA")
        .map(PathBuf::from)
        .unwrap_or_else(|_| home_dir().join("AppData").join("Roaming"));
    appdata
        .join("Antigravity")
        .join("User")
        .join("globalStorage")
        .join("state.vscdb")
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
struct AgAccountsFile {
    auto: bool,
    #[serde(default)]
    away: bool,
    #[serde(default)]
    pending: Option<String>,
    accounts: Vec<AiAccountSlot>,
    #[serde(default)]
    tokens: HashMap<String, String>,
    #[serde(default)]
    statuses: HashMap<String, String>,
}

pub struct AntigravityAccounts {
    file: PathBuf,
    snapshots_dir: PathBuf,
    vscdb_path: PathBuf,
    data: AgAccountsFile,
    seat_id: Option<String>,
    note: String,
}

impl AntigravityAccounts {
    pub fn load(data_dir: &Path) -> Self {
        Self::load_with_vscdb(data_dir, default_vscdb_path())
    }

    pub fn load_with_vscdb(data_dir: &Path, vscdb_path: PathBuf) -> Self {
        let file = data_dir.join("antigravity-accounts.json");
        let snapshots_dir = data_dir.join("antigravity-accounts");
        let data = std::fs::read_to_string(&file)
            .ok()
            .and_then(|s| serde_json::from_str::<AgAccountsFile>(&s).ok())
            .unwrap_or_default();
        let seat_id = data
            .accounts
            .iter()
            .find(|a| a.active)
            .map(|a| a.slug.clone());
        Self {
            file,
            snapshots_dir,
            vscdb_path,
            data,
            seat_id,
            note: String::new(),
        }
    }

    fn snapshot_file(&self, id: &str) -> PathBuf {
        self.snapshots_dir.join(id).join("state.vscdb")
    }

    fn backup_file(&self) -> PathBuf {
        self.snapshots_dir.join("state-before-switch.vscdb")
    }

    fn persist(&self) {
        let _ = write_json_atomic(&self.file, &self.data);
    }

    /// Reads `state.vscdb` (via `sqlite3` + protobuf if SQLite + CLI available,
    /// otherwise raw snapshot digest), records/updates the signed-in seat account,
    /// and saves a clean copy under `data_dir/antigravity-accounts/<id>/state.vscdb`.
    pub fn sync_vscdb(&mut self) {
        let Ok(bytes) = std::fs::read(&self.vscdb_path) else {
            self.note = "noDb".to_owned();
            return;
        };
        if bytes.is_empty() {
            self.note = "noDb".to_owned();
            return;
        }
        self.note.clear();

        let mut token_raw = String::new();
        let mut status_raw = String::new();
        let mut parsed_email = String::new();
        let mut parsed_name = String::new();
        let mut parsed_plan = String::new();
        let key: String;

        if is_sqlite_header(&bytes)
            && let Some(vals) = sqlite_read_keys(&self.vscdb_path, &[KEY_TOKEN, KEY_STATUS])
        {
            token_raw = vals.get(KEY_TOKEN).cloned().unwrap_or_default();
            status_raw = vals.get(KEY_STATUS).cloned().unwrap_or_default();
            if let Some(st) = status_of(&status_raw) {
                parsed_email = st.email;
                parsed_name = st.name;
                parsed_plan = if st.tier_name.is_empty() {
                    st.tier
                } else {
                    st.tier_name
                };
            }
            if let Some(tok) = token_of(&token_raw)
                && !tok.refresh.is_empty()
            {
                key = sha256_prefix(tok.refresh.as_bytes(), 16);
            } else {
                key = sha256_prefix(&bytes, 16);
            }
        } else {
            key = sha256_prefix(&bytes, 16);
        }

        let slug = if let Some(idx) = self.data.accounts.iter().position(|a| {
            (!a.key.is_empty() && a.key == key)
                || (!parsed_email.is_empty() && a.email == parsed_email)
        }) {
            let a = &mut self.data.accounts[idx];
            a.key = key.clone();
            if !parsed_email.is_empty() {
                a.email = parsed_email;
            }
            if !parsed_name.is_empty() {
                a.name = parsed_name;
            }
            if !parsed_plan.is_empty() {
                a.plan = parsed_plan;
            }
            a.at = now_ms();
            a.slug.clone()
        } else {
            let mut s = key.chars().take(10).collect::<String>();
            while self.data.accounts.iter().any(|a| a.slug == s) {
                s.push('x');
            }
            self.data.accounts.push(AiAccountSlot {
                slug: s.clone(),
                label: String::new(),
                email: parsed_email,
                active: true,
                later: false,
                key,
                name: parsed_name,
                plan: parsed_plan,
                note: String::new(),
                at: now_ms(),
            });
            s
        };

        if !token_raw.is_empty() {
            self.data.tokens.insert(slug.clone(), token_raw);
        }
        if !status_raw.is_empty() {
            self.data.statuses.insert(slug.clone(), status_raw);
        }
        let _ = write_bytes_atomic(&self.snapshot_file(&slug), &bytes);

        for a in &mut self.data.accounts {
            a.active = a.slug == slug;
        }
        self.seat_id = Some(slug);
        self.persist();
    }

    pub fn view(&self) -> Value {
        json!({
            "note": self.note,
            "auto": self.data.auto,
            "away": self.data.away,
            "now": self.seat_id,
            "pending": self.data.pending,
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
                if let Some(on) = body.get("on").and_then(Value::as_bool) {
                    self.data.auto = on;
                } else {
                    self.data.auto = !self.data.auto;
                }
                if let Some(away) = body.get("away").and_then(Value::as_bool) {
                    self.data.away = away;
                }
                self.persist();
                json!({ "ok": true, "auto": self.data.auto, "away": self.data.away })
            }
            "add" => {
                self.sync_vscdb();
                let label: String = one_line(
                    body.get("label")
                        .and_then(Value::as_str)
                        .unwrap_or("Antigravity"),
                )
                .chars()
                .take(24)
                .collect();
                let slug = format!("ag-{}", self.data.accounts.len() + 1);
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
                    at: now_ms(),
                    ..Default::default()
                });
                self.persist();
                json!({ "ok": true, "id": slug, "slug": slug })
            }
            "use" => {
                let target = Self::target_id(body).to_owned();
                if !self.data.accounts.iter().any(|a| a.slug == target) {
                    return json!({ "ok": false, "code": "noAccount" });
                }
                if self.seat_id.as_deref() == Some(target.as_str()) {
                    self.data.pending = None;
                    self.persist();
                    return json!({ "ok": true, "same": true, "id": target });
                }

                // Back up current state.vscdb (and save back into outgoing seat snapshot).
                if let Ok(cur_bytes) = std::fs::read(&self.vscdb_path) {
                    let _ = write_bytes_atomic(&self.backup_file(), &cur_bytes);
                    if let Some(cur_id) = &self.seat_id {
                        let _ = write_bytes_atomic(&self.snapshot_file(cur_id), &cur_bytes);
                    }
                }

                // Prefer SQLite key update when token/status + sqlite3 are available; else swap snapshot bytes atomically.
                let mut wrote = false;
                if let Some(tok) = self.data.tokens.get(&target)
                    && self
                        .vscdb_path
                        .is_file()
                    && std::fs::read(&self.vscdb_path)
                        .ok()
                        .is_some_and(|b| is_sqlite_header(&b))
                {
                    let mut pairs = vec![(KEY_TOKEN, tok.as_str())];
                    if let Some(st) = self.data.statuses.get(&target) {
                        pairs.push((KEY_STATUS, st.as_str()));
                    }
                    wrote = sqlite_write_keys(&self.vscdb_path, &pairs);
                }
                if !wrote {
                    let snap = self.snapshot_file(&target);
                    if snap.is_file() {
                        let Ok(bytes) = std::fs::read(&snap) else {
                            return json!({ "ok": false, "code": "writeFailed" });
                        };
                        if write_bytes_atomic(&self.vscdb_path, &bytes).is_err() {
                            return json!({ "ok": false, "code": "writeFailed" });
                        }
                    }
                }

                let from = self.seat_id.clone();
                self.seat_id = Some(target.clone());
                self.data.pending = None;
                for a in &mut self.data.accounts {
                    a.active = a.slug == target;
                }
                self.persist();
                json!({ "ok": true, "id": target, "from": from })
            }
            "rename" => {
                let target = Self::target_id(body);
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
                if !self.data.accounts.iter().any(|a| a.slug == target) {
                    return json!({ "ok": false, "code": "noAccount" });
                }
                if self.seat_id.as_deref() == Some(target) {
                    return json!({ "ok": false, "code": "inSeat" });
                }
                self.data.accounts.retain(|a| a.slug != target);
                self.data.tokens.remove(target);
                self.data.statuses.remove(target);
                if self.data.pending.as_deref() == Some(target) {
                    self.data.pending = None;
                }
                let dir = self.snapshots_dir.join(target);
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
                if target.is_empty() {
                    self.data.pending = None;
                } else {
                    for a in &mut self.data.accounts {
                        if a.slug == target {
                            a.later = body.get("on").and_then(Value::as_bool).unwrap_or(!a.later);
                        }
                    }
                }
                self.persist();
                json!({ "ok": true })
            }
            "read" => {
                self.sync_vscdb();
                json!({ "ok": true, "now": self.seat_id, "accounts": self.data.accounts })
            }
            "cancel" => {
                self.data.pending = None;
                self.persist();
                json!({ "ok": true, "accounts": self.data.accounts })
            }
            _ => json!({ "ok": false, "error": "unknown antigravity action" }),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use base64::Engine;
    use base64::engine::general_purpose::STANDARD;

    fn pb_len(field: u32, payload: &[u8]) -> Vec<u8> {
        let mut out = Vec::new();
        let mut tag = (field << 3) | 2;
        loop {
            let mut b = (tag & 0x7f) as u8;
            tag >>= 7;
            if tag != 0 {
                b |= 0x80;
            }
            out.push(b);
            if tag == 0 {
                break;
            }
        }
        let mut len = payload.len();
        loop {
            let mut b = (len & 0x7f) as u8;
            len >>= 7;
            if len != 0 {
                b |= 0x80;
            }
            out.push(b);
            if len == 0 {
                break;
            }
        }
        out.extend_from_slice(payload);
        out
    }

    fn wrap_sync(sentinel: &str, inner_pb: &[u8]) -> String {
        let inner_b64 = STANDARD.encode(inner_pb);
        let val_msg = pb_len(1, inner_b64.as_bytes());
        let mut entry = pb_len(1, sentinel.as_bytes());
        entry.extend_from_slice(&pb_len(2, &val_msg));
        let root = pb_len(1, &entry);
        STANDARD.encode(root)
    }

    #[test]
    fn antigravity_protobuf_decodes_token_and_status() {
        let mut tok_pb = pb_len(1, b"access-xyz");
        tok_pb.extend_from_slice(&pb_len(3, b"refresh-abc"));
        let wrapped_tok = wrap_sync("oauthTokenInfoSentinelKey", &tok_pb);
        let parsed_tok = token_of(&wrapped_tok).expect("should parse oauthToken");
        assert_eq!(parsed_tok.access, "access-xyz");
        assert_eq!(parsed_tok.refresh, "refresh-abc");

        let mut st_pb = pb_len(3, b"Alice Dev");
        st_pb.extend_from_slice(&pb_len(7, b"alice@gmail.com"));
        let wrapped_st = wrap_sync("userStatusSentinelKey", &st_pb);
        let parsed_st = status_of(&wrapped_st).expect("should parse userStatus");
        assert_eq!(parsed_st.name, "Alice Dev");
        assert_eq!(parsed_st.email, "alice@gmail.com");
    }

    #[test]
    fn antigravity_accounts_syncs_state_vscdb_backs_up_and_swaps_seats() {
        let dir = tempfile::tempdir().unwrap();
        let data_dir = dir.path().join("data");
        let vscdb = dir.path().join("User").join("globalStorage").join("state.vscdb");
        std::fs::create_dir_all(vscdb.parent().unwrap()).unwrap();

        let seat1_bytes = b"VSCDB_STATE_ACCOUNT_ONE_TOKEN_A";
        std::fs::write(&vscdb, seat1_bytes).unwrap();

        let mut mgr = AntigravityAccounts::load_with_vscdb(&data_dir, vscdb.clone());
        mgr.sync_vscdb();
        let id1 = mgr.view()["now"].as_str().unwrap().to_owned();
        assert!(!id1.is_empty());

        // Account 2 signs in (state.vscdb changes)
        let seat2_bytes = b"VSCDB_STATE_ACCOUNT_TWO_TOKEN_B";
        std::fs::write(&vscdb, seat2_bytes).unwrap();
        mgr.sync_vscdb();
        let id2 = mgr.view()["now"].as_str().unwrap().to_owned();
        assert_ne!(id1, id2);

        // Switch back to account 1 -> backs up current vscdb & writes seat1 atomically
        let sw = mgr.mutate("use", &json!({ "id": id1 }));
        assert_eq!(sw["ok"], true);
        assert_eq!(std::fs::read(&vscdb).unwrap(), seat1_bytes);
        assert_eq!(
            std::fs::read(
                data_dir
                    .join("antigravity-accounts")
                    .join("state-before-switch.vscdb")
            )
            .unwrap(),
            seat2_bytes
        );

        // Cannot remove active seat, can remove inactive account 2
        assert_eq!(mgr.mutate("remove", &json!({ "id": id1 }))["code"], "inSeat");
        assert_eq!(mgr.mutate("remove", &json!({ "id": id2 }))["ok"], true);
        assert!(!data_dir.join("antigravity-accounts").join(&id2).exists());
    }
}
