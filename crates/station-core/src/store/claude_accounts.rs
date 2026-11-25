use crate::atomic::{write_bytes_atomic, write_json_atomic};
use crate::state::sessions::one_line;
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};

const CREDENTIALS_FILE: &str = ".credentials.json";

/// Normalizes a secondary Claude account name to a safe ASCII slug (`[a-z0-9-]`, max 24 chars).
/// Returns empty string for invalid names or reserved `"main"`.
pub fn slug_of(raw: &str) -> String {
    let mut out = String::new();
    let mut prev_dash = false;
    for ch in raw.trim().to_ascii_lowercase().chars() {
        if ch.is_ascii_alphanumeric() {
            out.push(ch);
            prev_dash = false;
        } else if !prev_dash && !out.is_empty() {
            out.push('-');
            prev_dash = true;
        }
        if out.len() >= 24 {
            break;
        }
    }
    let trimmed = out.trim_matches('-').to_owned();
    if trimmed.is_empty() || trimmed == "main" {
        String::new()
    } else {
        trimmed
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ClaudeAccountSlot {
    pub name: String,
    #[serde(default)]
    pub label: String,
    #[serde(default)]
    pub dir: String,
    #[serde(default)]
    pub has_credentials: bool,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
struct ClaudeAccountsFile {
    #[serde(default)]
    active: String,
    #[serde(default)]
    auto: bool,
    #[serde(default)]
    move_on_limit: bool,
    #[serde(default)]
    resume: bool,
    #[serde(default)]
    order: Vec<String>,
    #[serde(default)]
    labels: HashMap<String, String>,
    #[serde(default)]
    accounts: Vec<ClaudeAccountSlot>,
}

pub struct ClaudeAccounts {
    state_file: PathBuf,
    now_file: PathBuf,
    snapshots_dir: PathBuf,
    main_dir: PathBuf,
    accounts_root: PathBuf,
    data: ClaudeAccountsFile,
}

impl ClaudeAccounts {
    pub fn load(data_dir: &Path, main_dir: &Path) -> Self {
        let state_file = data_dir.join("claude-accounts.json");
        let now_file = data_dir.join("account-now");
        let snapshots_dir = data_dir.join("claude-accounts");
        let accounts_root = main_dir
            .parent()
            .map(Path::to_path_buf)
            .unwrap_or_else(|| PathBuf::from("."));
        let data = std::fs::read_to_string(&state_file)
            .ok()
            .and_then(|s| serde_json::from_str::<ClaudeAccountsFile>(&s).ok())
            .unwrap_or_default();

        let mut mgr = Self {
            state_file,
            now_file,
            snapshots_dir,
            main_dir: main_dir.to_path_buf(),
            accounts_root,
            data,
        };
        mgr.ensure_order_consistent();
        mgr
    }

    fn slot_key(name: &str) -> &str {
        if name.is_empty() { "_main" } else { name }
    }

    fn dir_for(&self, name: &str) -> PathBuf {
        if name.is_empty() {
            self.main_dir.clone()
        } else {
            self.accounts_root.join(format!(".claude-{name}"))
        }
    }

    fn snapshot_cred_path(&self, name: &str) -> PathBuf {
        self.snapshots_dir
            .join(Self::slot_key(name))
            .join(CREDENTIALS_FILE)
    }

    fn has_account(&self, name: &str) -> bool {
        name.is_empty() || self.data.accounts.iter().any(|a| a.name == name)
    }

    fn ensure_order_consistent(&mut self) {
        let mut all = vec![String::new()];
        for a in &self.data.accounts {
            if !all.contains(&a.name) {
                all.push(a.name.clone());
            }
        }
        let mut seen = HashSet::new();
        let mut next = Vec::new();
        for s in &self.data.order {
            if all.contains(s) && seen.insert(s.clone()) {
                next.push(s.clone());
            }
        }
        for s in all {
            if seen.insert(s.clone()) {
                next.push(s);
            }
        }
        self.data.order = next;
    }

    fn persist(&self) {
        let _ = write_json_atomic(&self.state_file, &self.data);
    }

    fn copy_seat_to_snapshot(&self, name: &str) -> bool {
        let src = self.main_dir.join(CREDENTIALS_FILE);
        let Ok(bytes) = std::fs::read(&src) else {
            return false;
        };
        let dst = self.snapshot_cred_path(name);
        write_bytes_atomic(&dst, &bytes).is_ok()
    }

    pub fn view(&self) -> Value {
        let mut ordered = Vec::new();
        for slug in &self.data.order {
            let label = self
                .data
                .labels
                .get(slug)
                .cloned()
                .or_else(|| {
                    self.data
                        .accounts
                        .iter()
                        .find(|a| &a.name == slug)
                        .map(|a| a.label.clone())
                })
                .unwrap_or_default();
            let dir = self.dir_for(slug);
            let has_cred = self.snapshot_cred_path(slug).is_file()
                || (slug == &self.data.active && self.main_dir.join(CREDENTIALS_FILE).is_file());
            ordered.push(json!({
                "name": slug,
                "label": label,
                "dir": dir.to_string_lossy(),
                "active": slug == &self.data.active,
                "hasCredentials": has_cred,
            }));
        }
        json!({
            "ok": true,
            "active": self.data.active,
            "auto": self.data.auto,
            "move": self.data.move_on_limit,
            "resume": self.data.resume,
            "order": self.data.order,
            "accounts": ordered,
        })
    }

    fn prepare(&mut self, raw_name: &str) -> Value {
        let slug = slug_of(raw_name);
        if slug.is_empty() {
            return json!({ "ok": false, "code": "badName" });
        }
        let dir = self.dir_for(&slug);
        if dir == self.main_dir {
            return json!({ "ok": false, "code": "isMain" });
        }
        if std::fs::create_dir_all(&dir).is_err() {
            return json!({ "ok": false, "code": "mkdirFailed" });
        }
        let settings = dir.join("settings.json");
        if !settings.exists() {
            let _ = write_json_atomic(&settings, &json!({}));
        }
        if !self.data.accounts.iter().any(|a| a.name == slug) {
            self.data.accounts.push(ClaudeAccountSlot {
                name: slug.clone(),
                label: String::new(),
                dir: dir.to_string_lossy().into_owned(),
                has_credentials: dir.join(CREDENTIALS_FILE).is_file(),
            });
        }
        self.ensure_order_consistent();
        self.persist();
        json!({
            "ok": true,
            "name": slug,
            "dir": dir.to_string_lossy(),
        })
    }

    fn save_seat(&mut self, raw_name: &str, label: Option<&str>) -> Value {
        let target = if raw_name.trim().is_empty() {
            String::new()
        } else {
            let s = slug_of(raw_name);
            if s.is_empty() {
                return json!({ "ok": false, "code": "badName" });
            }
            s
        };

        if !target.is_empty() && !self.has_account(&target) {
            let dir = self.dir_for(&target);
            let _ = std::fs::create_dir_all(&dir);
            self.data.accounts.push(ClaudeAccountSlot {
                name: target.clone(),
                label: String::new(),
                dir: dir.to_string_lossy().into_owned(),
                has_credentials: false,
            });
        }

        let saved_cred = self.copy_seat_to_snapshot(&target);
        if let Some(slot) = self.data.accounts.iter_mut().find(|a| a.name == target) {
            if saved_cred {
                slot.has_credentials = true;
            }
            if let Some(lbl) = label {
                let clean: String = one_line(lbl).chars().take(24).collect();
                slot.label = clean;
            }
        }
        if let Some(lbl) = label {
            let clean: String = one_line(lbl).chars().take(24).collect();
            if clean.is_empty() {
                self.data.labels.remove(&target);
            } else {
                self.data.labels.insert(target.clone(), clean);
            }
        }
        self.data.active = target.clone();
        self.ensure_order_consistent();
        self.persist();
        json!({
            "ok": true,
            "name": target,
            "savedCredentials": saved_cred,
        })
    }

    fn switch_to(&mut self, raw_name: &str) -> Value {
        let target = if raw_name.trim().is_empty() {
            String::new()
        } else {
            let s = slug_of(raw_name);
            if s.is_empty() {
                return json!({ "ok": false, "code": "badName" });
            }
            s
        };
        if !self.has_account(&target) {
            return json!({ "ok": false, "code": "noAccount" });
        }

        if self.data.active != target {
            // Preserve current seat credentials back to the outgoing slot before swapping.
            let prev = self.data.active.clone();
            let _ = self.copy_seat_to_snapshot(&prev);

            let snap = self.snapshot_cred_path(&target);
            if snap.is_file() {
                let Ok(bytes) = std::fs::read(&snap) else {
                    return json!({ "ok": false, "code": "readFailed" });
                };
                if write_bytes_atomic(&self.main_dir.join(CREDENTIALS_FILE), &bytes).is_err() {
                    return json!({ "ok": false, "code": "seatFailed" });
                }
            }
        }

        let now_line = if target.is_empty() {
            "\n".to_owned()
        } else {
            format!("{}\n", self.dir_for(&target).to_string_lossy())
        };
        let _ = write_bytes_atomic(&self.now_file, now_line.as_bytes());

        self.data.active = target.clone();
        self.persist();
        json!({
            "ok": true,
            "active": target,
            "dir": self.dir_for(&self.data.active).to_string_lossy(),
        })
    }

    fn rename(&mut self, raw_name: &str, raw_label: &str) -> Value {
        let target = if raw_name.trim().is_empty() {
            String::new()
        } else {
            let s = slug_of(raw_name);
            if s.is_empty() || !self.has_account(&s) {
                return json!({ "ok": false, "code": "noAccount" });
            }
            s
        };
        let label: String = one_line(raw_label).chars().take(24).collect();
        if label.is_empty() || label == target {
            self.data.labels.remove(&target);
        } else {
            self.data.labels.insert(target.clone(), label.clone());
        }
        if let Some(slot) = self.data.accounts.iter_mut().find(|a| a.name == target) {
            slot.label = label.clone();
        }
        self.persist();
        json!({ "ok": true, "name": target, "label": label })
    }

    fn remove(&mut self, raw_name: &str) -> Value {
        let slug = slug_of(raw_name);
        if slug.is_empty() {
            return json!({ "ok": false, "code": "isMain" });
        }
        if !self.has_account(&slug) {
            return json!({ "ok": false, "code": "noAccount" });
        }
        if self.data.active == slug {
            return json!({ "ok": false, "code": "inSeat" });
        }
        self.data.accounts.retain(|a| a.name != slug);
        self.data.order.retain(|s| s != &slug);
        self.data.labels.remove(&slug);
        let snap_dir = self.snapshots_dir.join(&slug);
        if snap_dir.is_dir() {
            let _ = std::fs::remove_dir_all(&snap_dir);
        }
        self.persist();
        json!({ "ok": true, "name": slug })
    }

    fn set_order(&mut self, arr: &[Value]) -> Value {
        let mut next = Vec::with_capacity(arr.len());
        for v in arr {
            let Some(s) = v.as_str() else {
                return json!({ "ok": false, "code": "badOrder" });
            };
            let norm = if s.is_empty() { String::new() } else { slug_of(s) };
            next.push(norm);
        }
        self.ensure_order_consistent();
        let expected: HashSet<&String> = self.data.order.iter().collect();
        let got: HashSet<&String> = next.iter().collect();
        if next.len() != self.data.order.len() || expected != got {
            return json!({ "ok": false, "code": "badOrder" });
        }
        self.data.order = next;
        self.persist();
        json!({ "ok": true, "order": self.data.order })
    }

    fn set_auto(&mut self, body: &Value) -> Value {
        if let Some(on) = body.get("on").and_then(Value::as_bool) {
            self.data.auto = on;
        }
        if let Some(mv) = body.get("move").and_then(Value::as_bool) {
            self.data.move_on_limit = mv;
        }
        if let Some(res) = body.get("resume").and_then(Value::as_bool) {
            self.data.resume = res;
        }
        self.persist();
        json!({
            "ok": true,
            "auto": self.data.auto,
            "move": self.data.move_on_limit,
            "resume": self.data.resume,
        })
    }

    pub fn mutate(&mut self, action: &str, body: &Value) -> Value {
        let name = body
            .get("name")
            .or_else(|| body.get("to"))
            .or_else(|| body.get("slug"))
            .and_then(Value::as_str)
            .unwrap_or("");
        match action {
            "" | "prepare" | "add" => self.prepare(name),
            "save" => self.save_seat(name, body.get("label").and_then(Value::as_str)),
            "switch" | "use" => self.switch_to(name),
            "rename" => {
                let label = body.get("label").and_then(Value::as_str).unwrap_or("");
                self.rename(name, label)
            }
            "remove" => self.remove(name),
            "order" => match body.get("order").and_then(Value::as_array) {
                Some(arr) => self.set_order(arr),
                None => json!({ "ok": false, "code": "badOrder" }),
            },
            "auto" => self.set_auto(body),
            "move" => {
                let to = body.get("to").and_then(Value::as_str).unwrap_or("");
                let target = if to.is_empty() { String::new() } else { slug_of(to) };
                if !self.has_account(&target) {
                    json!({ "ok": false, "code": "noAccount" })
                } else {
                    self.switch_to(&target)
                }
            }
            "renew" => {
                let slug = slug_of(name);
                if !name.is_empty() && !self.has_account(&slug) {
                    json!({ "ok": false, "code": "noAccount" })
                } else {
                    json!({ "ok": true, "name": slug })
                }
            }
            "login" | "code" | "cancel" => {
                let slug = slug_of(name);
                json!({ "ok": !slug.is_empty(), "name": slug, "state": if action == "cancel" { "cancelled" } else { "idle" } })
            }
            _ => json!({ "ok": false, "code": "unknownAction" }),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn claude_accounts_prepare_save_swap_rename_order_and_remove() {
        let dir = tempfile::tempdir().unwrap();
        let data_dir = dir.path().join("data");
        let main_dir = dir.path().join(".claude");
        std::fs::create_dir_all(&main_dir).unwrap();
        std::fs::write(
            main_dir.join(".credentials.json"),
            r#"{"claudeAiOauth":{"accessToken":"tok-main-1"}}"#,
        )
        .unwrap();

        let mut mgr = ClaudeAccounts::load(&data_dir, &main_dir);

        // Save current main seat snapshot under main ("")
        let s_main = mgr.mutate("save", &json!({ "name": "", "label": "Primary" }));
        assert_eq!(s_main["ok"], true);

        // Prepare secondary account "work" -> creates sibling .claude-work dir
        let prep = mgr.mutate("prepare", &json!({ "name": "Work Team!" }));
        assert_eq!(prep["ok"], true);
        let slug = prep["name"].as_str().unwrap().to_owned();
        assert_eq!(slug, "work-team");
        assert!(dir.path().join(".claude-work-team").is_dir());

        // Seed work credentials and save snapshot into work slot
        std::fs::write(
            main_dir.join(".credentials.json"),
            r#"{"claudeAiOauth":{"accessToken":"tok-work-2"}}"#,
        )
        .unwrap();
        let s_work = mgr.mutate("save", &json!({ "name": "work-team", "label": "Work" }));
        assert_eq!(s_work["ok"], true);

        // Switch seat back to main ("") -> restores tok-main-1 into main_dir/.credentials.json
        let sw_main = mgr.mutate("switch", &json!({ "name": "" }));
        assert_eq!(sw_main["ok"], true);
        assert_eq!(sw_main["active"], "");
        let cred_after_main = std::fs::read_to_string(main_dir.join(".credentials.json")).unwrap();
        assert!(cred_after_main.contains("tok-main-1"), "got {cred_after_main}");

        // Switch seat to work-team -> restores tok-work-2 and updates data/account-now
        let sw_work = mgr.mutate("use", &json!({ "name": "work-team" }));
        assert_eq!(sw_work["ok"], true);
        assert_eq!(sw_work["active"], "work-team");
        let cred_after_work = std::fs::read_to_string(main_dir.join(".credentials.json")).unwrap();
        assert!(cred_after_work.contains("tok-work-2"), "got {cred_after_work}");
        let account_now = std::fs::read_to_string(data_dir.join("account-now")).unwrap();
        assert!(account_now.trim().ends_with(".claude-work-team"));

        // Rename, reorder, auto-switch, and remove guards
        assert_eq!(
            mgr.mutate("rename", &json!({ "name": "work-team", "label": "Corp" }))["ok"],
            true
        );
        assert_eq!(
            mgr.mutate("order", &json!({ "order": ["work-team", ""] }))["ok"],
            true
        );
        assert_eq!(
            mgr.mutate("auto", &json!({ "on": true, "move": true, "resume": false }))["auto"],
            true
        );

        // Cannot remove active seat
        let rm_active = mgr.mutate("remove", &json!({ "name": "work-team" }));
        assert_eq!(rm_active["ok"], false);
        assert_eq!(rm_active["code"], "inSeat");

        // Switch back to main, then remove work-team cleanly
        assert_eq!(mgr.mutate("switch", &json!({ "name": "" }))["ok"], true);
        assert_eq!(mgr.mutate("remove", &json!({ "name": "work-team" }))["ok"], true);

        // Reload from disk verifies persistence
        let reloaded = ClaudeAccounts::load(&data_dir, &main_dir);
        let view = reloaded.view();
        assert_eq!(view["auto"], true);
        assert_eq!(view["active"], "");
    }
}
