use crate::atomic::{write_bytes_atomic, write_json_atomic};
use crate::state::sessions::one_line;
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use std::collections::{HashMap, HashSet};
use std::io::{BufRead, BufReader, Write};
use std::path::{Path, PathBuf};
use std::process::{Child, ChildStdin, Command, Stdio};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

const CREDENTIALS_FILE: &str = ".credentials.json";
#[cfg(windows)]
const CREATE_NO_WINDOW: u32 = 0x0800_0000;

pub(crate) fn find_cli_on_path(env_override: &str, candidates: &[&str]) -> Option<PathBuf> {
    if let Ok(explicit) = std::env::var(env_override) {
        let trimmed = explicit.trim();
        if !trimmed.is_empty() {
            return Some(PathBuf::from(trimmed));
        }
    }
    let path_var = std::env::var_os("PATH")?;
    for dir in std::env::split_paths(&path_var) {
        for name in candidates {
            let p = dir.join(name);
            if p.is_file() {
                return Some(p);
            }
        }
    }
    None
}

fn extract_visit_url(text: &str) -> Option<String> {
    let lower = text.to_ascii_lowercase();
    let idx = lower.find("visit:")?;
    let after = text[idx + 6..].trim_start();
    if !after.starts_with("https://") {
        return None;
    }
    let url: String = after
        .chars()
        .take_while(|c| !c.is_whitespace() && !matches!(c, '"' | '\'' | ')' | '>' | ']'))
        .collect();
    if url.len() > "https://".len() {
        Some(url)
    } else {
        None
    }
}

struct ClaudeLoginState {
    slug: String,
    state: String,
    url: String,
    note: String,
    last_line: String,
    stdin: Option<ChildStdin>,
    child: Option<Child>,
}

impl ClaudeLoginState {
    fn snapshot(&self) -> Value {
        json!({
            "slug": self.slug,
            "state": self.state,
            "url": self.url,
            "note": self.note,
        })
    }

    fn kill_child(&mut self) {
        self.stdin.take();
        if let Some(mut c) = self.child.take() {
            let _ = c.kill();
            let _ = c.wait();
        }
    }
}

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
    data_dir: PathBuf,
    state_file: PathBuf,
    now_file: PathBuf,
    snapshots_dir: PathBuf,
    main_dir: PathBuf,
    accounts_root: PathBuf,
    data: ClaudeAccountsFile,
    active_login: Option<Arc<Mutex<ClaudeLoginState>>>,
}

impl Drop for ClaudeAccounts {
    fn drop(&mut self) {
        if let Some(login) = self.active_login.take() {
            login
                .lock()
                .unwrap_or_else(|e| e.into_inner())
                .kill_child();
        }
    }
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
            data_dir: data_dir.to_path_buf(),
            state_file,
            now_file,
            snapshots_dir,
            main_dir: main_dir.to_path_buf(),
            accounts_root,
            data,
            active_login: None,
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
                || dir.join(CREDENTIALS_FILE).is_file()
                || (slug == &self.data.active && self.main_dir.join(CREDENTIALS_FILE).is_file());
            ordered.push(json!({
                "name": slug,
                "label": label,
                "dir": dir.to_string_lossy(),
                "active": slug == &self.data.active,
                "hasCredentials": has_cred,
            }));
        }
        let login_view = self
            .active_login
            .as_ref()
            .map(|l| l.lock().unwrap_or_else(|e| e.into_inner()).snapshot())
            .unwrap_or(Value::Null);
        json!({
            "ok": true,
            "active": self.data.active,
            "auto": self.data.auto,
            "move": self.data.move_on_limit,
            "resume": self.data.resume,
            "order": self.data.order,
            "accounts": ordered,
            "login": login_view,
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
            let snap = self.snapshot_cred_path(&target);
            if !snap.is_file() {
                return json!({ "ok": false, "code": "noCopy" });
            }
            let Ok(bytes) = std::fs::read(&snap) else {
                return json!({ "ok": false, "code": "readFailed" });
            };
            // Preserve current seat credentials back to the outgoing slot before swapping.
            let prev = self.data.active.clone();
            let _ = self.copy_seat_to_snapshot(&prev);

            if write_bytes_atomic(&self.main_dir.join(CREDENTIALS_FILE), &bytes).is_err() {
                return json!({ "ok": false, "code": "seatFailed" });
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
            "renew" => self.start_renew(name),
            "login" => self.start_login(name),
            "code" => {
                let code = body
                    .get("code")
                    .and_then(Value::as_str)
                    .unwrap_or("");
                self.submit_login_code(code)
            }
            "cancel" => self.cancel_login(name),
            _ => json!({ "ok": false, "code": "unknownAction" }),
        }
    }

    fn sync_completed_login_slot(&mut self) {
        let done_slug = self.active_login.as_ref().and_then(|l| {
            let st = l.lock().unwrap_or_else(|e| e.into_inner());
            (st.state == "done").then(|| st.slug.clone())
        });
        let Some(slug) = done_slug else {
            return;
        };
        let dir = self.dir_for(&slug);
        let cred = dir.join(CREDENTIALS_FILE);
        if cred.is_file()
            && let Ok(bytes) = std::fs::read(&cred)
        {
            let _ = write_bytes_atomic(&self.snapshot_cred_path(&slug), &bytes);
        }
        if !self.has_account(&slug) && dir.is_dir() {
            self.data.accounts.push(ClaudeAccountSlot {
                name: slug.clone(),
                label: String::new(),
                dir: dir.to_string_lossy().into_owned(),
                has_credentials: cred.is_file(),
            });
            self.ensure_order_consistent();
            self.persist();
        }
    }

    fn start_login(&mut self, raw_name: &str) -> Value {
        self.sync_completed_login_slot();
        let slug = slug_of(raw_name);
        if slug.is_empty() {
            return json!({ "ok": false, "state": "idle", "code": "badName" });
        }
        if let Some(existing) = &self.active_login {
            let st = existing.lock().unwrap_or_else(|e| e.into_inner());
            if st.slug == slug && st.state == "waiting" {
                return json!({
                    "ok": true,
                    "name": st.slug,
                    "state": st.state,
                    "url": st.url,
                    "note": st.note,
                });
            }
        }
        if let Some(prev) = self.active_login.take() {
            prev.lock().unwrap_or_else(|e| e.into_inner()).kill_child();
        }

        let Some(bin) = find_cli_on_path("OFFICE_CLAUDE_BIN", &["claude.exe", "claude.cmd", "claude"])
        else {
            return json!({
                "ok": false,
                "name": slug,
                "state": "failed",
                "code": "noCli",
                "url": "",
                "note": "Claude CLI not found",
            });
        };

        let dir = self.dir_for(&slug);
        if std::fs::create_dir_all(&dir).is_err() {
            return json!({
                "ok": false,
                "name": slug,
                "state": "failed",
                "code": "mkdirFailed",
                "url": "",
                "note": "Failed to create account directory",
            });
        }
        if !self.has_account(&slug) {
            self.data.accounts.push(ClaudeAccountSlot {
                name: slug.clone(),
                label: String::new(),
                dir: dir.to_string_lossy().into_owned(),
                has_credentials: dir.join(CREDENTIALS_FILE).is_file(),
            });
            self.ensure_order_consistent();
            self.persist();
        }

        let mut cmd = Command::new(&bin);
        cmd.args(["auth", "login", "--claudeai"])
            .env("CLAUDE_CONFIG_DIR", &dir)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped());
        #[cfg(windows)]
        {
            use std::os::windows::process::CommandExt;
            cmd.creation_flags(CREATE_NO_WINDOW);
        }

        let Ok(mut child) = cmd.spawn() else {
            return json!({
                "ok": false,
                "name": slug,
                "state": "failed",
                "code": "noCli",
                "url": "",
                "note": "Failed to spawn Claude CLI",
            });
        };

        let stdin = child.stdin.take();
        let stdout = child.stdout.take();
        let stderr = child.stderr.take();
        let login = Arc::new(Mutex::new(ClaudeLoginState {
            slug: slug.clone(),
            state: "waiting".to_owned(),
            url: String::new(),
            note: String::new(),
            last_line: String::new(),
            stdin,
            child: Some(child),
        }));
        self.active_login = Some(Arc::clone(&login));

        let spawn_reader = |stream: Box<dyn std::io::Read + Send>, shared: Arc<Mutex<ClaudeLoginState>>| {
            let _ = std::thread::Builder::new()
                .name("claude-login-io".to_owned())
                .spawn(move || {
                    let reader = BufReader::new(stream);
                    for line in reader.lines().map_while(Result::ok) {
                        let mut st = shared.lock().unwrap_or_else(|e| e.into_inner());
                        let clean: String = one_line(&line).chars().take(140).collect();
                        if !clean.is_empty() {
                            st.last_line = clean;
                        }
                        if st.url.is_empty()
                            && let Some(u) = extract_visit_url(&line)
                        {
                            st.url = u;
                        }
                    }
                });
        };
        if let Some(out) = stdout {
            spawn_reader(Box::new(out), Arc::clone(&login));
        }
        if let Some(err) = stderr {
            spawn_reader(Box::new(err), Arc::clone(&login));
        }

        let waiter = Arc::clone(&login);
        let snap_dst = self.snapshot_cred_path(&slug);
        let cred_src = dir.join(CREDENTIALS_FILE);
        let _ = std::thread::Builder::new()
            .name("claude-login-wait".to_owned())
            .spawn(move || {
                let deadline = Instant::now() + Duration::from_secs(15 * 60);
                loop {
                    std::thread::sleep(Duration::from_millis(40));
                    let mut st = waiter.lock().unwrap_or_else(|e| e.into_inner());
                    if st.state != "waiting" {
                        break;
                    }
                    if Instant::now() >= deadline {
                        st.state = "failed".to_owned();
                        st.note = "timeout".to_owned();
                        st.kill_child();
                        break;
                    }
                    let exit = match st.child.as_mut() {
                        Some(c) => c.try_wait().ok().flatten(),
                        None => break,
                    };
                    if let Some(status) = exit {
                        st.stdin.take();
                        st.child.take();
                        if status.success() {
                            st.state = "done".to_owned();
                            if cred_src.is_file()
                                && let Ok(bytes) = std::fs::read(&cred_src)
                            {
                                let _ = write_bytes_atomic(&snap_dst, &bytes);
                            }
                        } else {
                            st.state = "failed".to_owned();
                            if st.note.is_empty() {
                                st.note = st.last_line.clone();
                            }
                        }
                        break;
                    }
                }
            });

        let st = login.lock().unwrap_or_else(|e| e.into_inner());
        json!({
            "ok": true,
            "name": st.slug,
            "state": st.state,
            "url": st.url,
            "note": st.note,
        })
    }

    fn submit_login_code(&mut self, raw_code: &str) -> Value {
        self.sync_completed_login_slot();
        let code: String = one_line(raw_code).chars().take(400).collect();
        if code.is_empty() {
            return json!({ "ok": false, "state": "idle" });
        }
        let Some(login) = &self.active_login else {
            return json!({ "ok": false, "state": "idle" });
        };
        let mut st = login.lock().unwrap_or_else(|e| e.into_inner());
        if st.state != "waiting" {
            return json!({ "ok": false, "name": st.slug, "state": st.state });
        }
        let Some(stdin) = st.stdin.as_mut() else {
            return json!({ "ok": false, "name": st.slug, "state": st.state });
        };
        if writeln!(stdin, "{code}").and_then(|()| stdin.flush()).is_err() {
            return json!({ "ok": false, "name": st.slug, "state": st.state });
        }
        json!({ "ok": true, "name": st.slug, "state": st.state })
    }

    fn cancel_login(&mut self, raw_name: &str) -> Value {
        let Some(login) = self.active_login.take() else {
            return json!({ "ok": false, "name": slug_of(raw_name), "state": "idle" });
        };
        let mut st = login.lock().unwrap_or_else(|e| e.into_inner());
        let slug = st.slug.clone();
        st.state = "cancelled".to_owned();
        st.kill_child();
        json!({ "ok": true, "name": slug, "state": "cancelled" })
    }

    fn start_renew(&mut self, raw_name: &str) -> Value {
        self.sync_completed_login_slot();
        let slug = if raw_name.trim().is_empty() {
            String::new()
        } else {
            let s = slug_of(raw_name);
            if s.is_empty() || !self.has_account(&s) {
                return json!({ "ok": false, "code": "noAccount" });
            }
            s
        };
        let Some(bin) = find_cli_on_path("OFFICE_CLAUDE_BIN", &["claude.exe", "claude.cmd", "claude"])
        else {
            return json!({ "ok": false, "name": slug, "code": "noCli" });
        };
        let dir = self.dir_for(&slug);
        let cwd = self.data_dir.join("renew");
        let _ = std::fs::create_dir_all(&cwd);
        let mut cmd = Command::new(&bin);
        cmd.args([
            "-p",
            "--model",
            "haiku",
            "--no-session-persistence",
            "--strict-mcp-config",
            "--tools",
            "",
            "--settings",
            r#"{"disableAllHooks":true}"#,
            "Reply with the single word: ok",
        ])
        .current_dir(&cwd)
        .env("CLAUDE_CONFIG_DIR", &dir)
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null());
        #[cfg(windows)]
        {
            use std::os::windows::process::CommandExt;
            cmd.creation_flags(CREATE_NO_WINDOW);
        }
        let Ok(mut child) = cmd.spawn() else {
            return json!({ "ok": false, "name": slug, "code": "noCli" });
        };
        let _ = std::thread::Builder::new()
            .name("claude-renew-reap".to_owned())
            .spawn(move || {
                let deadline = Instant::now() + Duration::from_secs(90);
                while Instant::now() < deadline {
                    if let Ok(Some(_)) = child.try_wait() {
                        return;
                    }
                    std::thread::sleep(Duration::from_millis(100));
                }
                let _ = child.kill();
                let _ = child.wait();
            });
        json!({ "ok": true, "name": slug, "state": "renewing" })
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

    #[test]
    fn claude_accounts_rejects_switch_to_unsnapshotted_slot_without_contamination() {
        let dir = tempfile::tempdir().unwrap();
        let data_dir = dir.path().join("data");
        let main_dir = dir.path().join(".claude");
        std::fs::create_dir_all(&main_dir).unwrap();
        std::fs::write(
            main_dir.join(".credentials.json"),
            r#"{"claudeAiOauth":{"accessToken":"tok-main-clean"}}"#,
        )
        .unwrap();

        let mut mgr = ClaudeAccounts::load(&data_dir, &main_dir);
        assert_eq!(mgr.mutate("save", &json!({ "name": "" }))["ok"], true);
        assert_eq!(mgr.mutate("prepare", &json!({ "name": "unsaved" }))["ok"], true);

        let res = mgr.mutate("switch", &json!({ "name": "unsaved" }));
        assert_eq!(res["ok"], false);
        assert_eq!(res["code"], "noCopy");
        assert_eq!(mgr.view()["active"], "");
        assert!(
            std::fs::read_to_string(main_dir.join(".credentials.json"))
                .unwrap()
                .contains("tok-main-clean")
        );
        assert!(!data_dir.join("claude-accounts").join("unsaved").exists());
    }

    #[test]
    fn claude_login_spawns_cli_captures_url_pipes_code_cancels_and_renews() {
        let dir = tempfile::tempdir().unwrap();
        let data_dir = dir.path().join("data");
        let main_dir = dir.path().join(".claude");
        std::fs::create_dir_all(&main_dir).unwrap();

        // Create a real fake CLI script that speaks the exact Claude auth/renew protocol.
        let code_log = dir.path().join("received-code.txt");
        let renew_log = dir.path().join("renew-ran.txt");
        #[cfg(windows)]
        let fake_cli = {
            let p = dir.path().join("fake-claude.cmd");
            std::fs::write(
                &p,
                format!(
                    "@echo off\r\n\
if \"%1\"==\"auth\" goto do_auth\r\n\
if \"%1\"==\"-p\" goto do_renew\r\n\
exit /b 1\r\n\
:do_auth\r\n\
echo Please visit: https://claude.ai/oauth/test-authorize?state=xyz\r\n\
set /p OAUTH_CODE=\r\n\
>\"{}\" echo %OAUTH_CODE%\r\n\
>\"%CLAUDE_CONFIG_DIR%\\.credentials.json\" echo {{\"claudeAiOauth\":{{\"accessToken\":\"tok-oauth-1\"}}}}\r\n\
exit /b 0\r\n\
:do_renew\r\n\
>\"{}\" echo renewed\r\n\
exit /b 0\r\n",
                    code_log.display(),
                    renew_log.display()
                ),
            )
            .unwrap();
            p
        };
        #[cfg(not(windows))]
        let fake_cli = {
            use std::os::unix::fs::PermissionsExt;
            let p = dir.path().join("fake-claude.sh");
            std::fs::write(
                &p,
                format!(
                    "#!/bin/sh\n\
if [ \"$1\" = \"auth\" ]; then\n\
  echo \"Please visit: https://claude.ai/oauth/test-authorize?state=xyz\"\n\
  read OAUTH_CODE\n\
  echo \"$OAUTH_CODE\" > \"{}\"\n\
  printf '{{\"claudeAiOauth\":{{\"accessToken\":\"tok-oauth-1\"}}}}' > \"$CLAUDE_CONFIG_DIR/.credentials.json\"\n\
  exit 0\n\
fi\n\
if [ \"$1\" = \"-p\" ]; then\n\
  echo renewed > \"{}\"\n\
  exit 0\n\
fi\n\
exit 1\n",
                    code_log.display(),
                    renew_log.display()
                ),
            )
            .unwrap();
            let mut perms = std::fs::metadata(&p).unwrap().permissions();
            perms.set_mode(0o755);
            std::fs::set_permissions(&p, perms).unwrap();
            p
        };

        // SAFETY: tests run with `--test-threads=1` and restore env before exit.
        unsafe { std::env::set_var("OFFICE_CLAUDE_BIN", &fake_cli) };
        let mut mgr = ClaudeAccounts::load(&data_dir, &main_dir);

        // 1. Start login -> real child spawns and emits URL parsed into `view()["login"]["url"]`.
        let start = mgr.mutate("login", &json!({ "name": "oauth-acct" }));
        assert_eq!(start["ok"], true);
        assert_eq!(start["state"], "waiting");

        let mut saw_url = false;
        for _ in 0..60 {
            if mgr.view()["login"]["url"]
                .as_str()
                .unwrap_or("")
                .starts_with("https://claude.ai/oauth/test-authorize")
            {
                saw_url = true;
                break;
            }
            std::thread::sleep(Duration::from_millis(50));
        }
        assert!(saw_url, "must parse live OAuth URL from child stdout: {:?}", mgr.view()["login"]);

        // 2. Submit code -> piped to real child stdin, child writes credentials & exits 0 -> state == "done".
        let submitted = mgr.mutate("code", &json!({ "code": "real-code-9876" }));
        assert_eq!(submitted["ok"], true);

        let mut done = false;
        for _ in 0..60 {
            if mgr.view()["login"]["state"] == "done" {
                done = true;
                break;
            }
            std::thread::sleep(Duration::from_millis(50));
        }
        assert!(done, "login child must complete to state=done: {:?}", mgr.view()["login"]);
        let piped = std::fs::read_to_string(&code_log).unwrap_or_default();
        assert!(piped.contains("real-code-9876"), "code must be piped to stdin, got: {piped:?}");
        assert!(
            data_dir
                .join("claude-accounts")
                .join("oauth-acct")
                .join(".credentials.json")
                .is_file(),
            "completed OAuth login must snapshot .credentials.json"
        );

        // 3. Renew -> spawns real `-p` subprocess with CLAUDE_CONFIG_DIR.
        let renew = mgr.mutate("renew", &json!({ "name": "oauth-acct" }));
        assert_eq!(renew["ok"], true);
        assert_eq!(renew["state"], "renewing");
        let mut renewed = false;
        for _ in 0..60 {
            if renew_log.is_file() {
                renewed = true;
                break;
            }
            std::thread::sleep(Duration::from_millis(50));
        }
        assert!(renewed, "renew must execute real Claude CLI subprocess");

        // 4. Cancel -> kills waiting login child cleanly.
        let start2 = mgr.mutate("login", &json!({ "name": "cancel-me" }));
        assert_eq!(start2["ok"], true);
        let cancelled = mgr.mutate("cancel", &json!({ "name": "cancel-me" }));
        assert_eq!(cancelled["ok"], true);
        assert_eq!(cancelled["state"], "cancelled");

        unsafe { std::env::remove_var("OFFICE_CLAUDE_BIN") };
    }
}
