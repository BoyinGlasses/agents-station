use crate::atomic::write_json_atomic;
use crate::store::codex_accounts::AiAccountSlot;
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use std::path::{Path, PathBuf};

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
struct AgAccountsFile {
    auto: bool,
    accounts: Vec<AiAccountSlot>,
}

pub struct AntigravityAccounts {
    file: PathBuf,
    data: AgAccountsFile,
}

impl AntigravityAccounts {
    pub fn load(data_dir: &Path) -> Self {
        let file = data_dir.join("antigravity-accounts.json");
        let data = std::fs::read_to_string(&file)
            .ok()
            .and_then(|s| serde_json::from_str::<AgAccountsFile>(&s).ok())
            .unwrap_or_default();
        Self { file, data }
    }

    pub fn view(&self) -> Value {
        json!({ "auto": self.data.auto, "accounts": self.data.accounts })
    }

    pub fn mutate(&mut self, action: &str, body: &Value) -> Value {
        match action {
            "auto" => {
                self.data.auto = body.get("on").and_then(Value::as_bool).unwrap_or(!self.data.auto);
                let _ = write_json_atomic(&self.file, &self.data);
                json!({ "ok": true, "auto": self.data.auto })
            }
            "add" => {
                let label = body.get("label").and_then(Value::as_str).unwrap_or("Antigravity").trim().to_owned();
                let slug = format!("ag-{}", self.data.accounts.len() + 1);
                self.data.accounts.push(AiAccountSlot {
                    slug: slug.clone(),
                    label,
                    email: body.get("email").and_then(Value::as_str).unwrap_or("").to_owned(),
                    active: self.data.accounts.is_empty(),
                    later: false,
                    ..Default::default()
                });
                let _ = write_json_atomic(&self.file, &self.data);
                json!({ "ok": true, "slug": slug })
            }
            "use" => {
                let slug = body.get("slug").and_then(Value::as_str).unwrap_or("");
                let mut found = false;
                for a in &mut self.data.accounts {
                    a.active = a.slug == slug;
                    if a.active {
                        found = true;
                    }
                }
                let _ = write_json_atomic(&self.file, &self.data);
                json!({ "ok": found })
            }
            "rename" => {
                let slug = body.get("slug").and_then(Value::as_str).unwrap_or("");
                let label = body.get("label").and_then(Value::as_str).unwrap_or("").trim();
                let mut found = false;
                for a in &mut self.data.accounts {
                    if a.slug == slug && !label.is_empty() {
                        a.label = label.to_owned();
                        found = true;
                    }
                }
                let _ = write_json_atomic(&self.file, &self.data);
                json!({ "ok": found })
            }
            "remove" => {
                let slug = body.get("slug").and_then(Value::as_str).unwrap_or("");
                let before = self.data.accounts.len();
                self.data.accounts.retain(|a| a.slug != slug);
                let _ = write_json_atomic(&self.file, &self.data);
                json!({ "ok": self.data.accounts.len() != before })
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
                    let _ = write_json_atomic(&self.file, &self.data);
                }
                json!({ "ok": true })
            }
            "later" => {
                let slug = body.get("slug").and_then(Value::as_str).unwrap_or("");
                for a in &mut self.data.accounts {
                    if a.slug == slug {
                        a.later = body.get("on").and_then(Value::as_bool).unwrap_or(!a.later);
                    }
                }
                let _ = write_json_atomic(&self.file, &self.data);
                json!({ "ok": true })
            }
            "cancel" | "read" => json!({ "ok": true, "accounts": self.data.accounts }),
            _ => json!({ "ok": false, "error": "unknown antigravity action" }),
        }
    }
}
