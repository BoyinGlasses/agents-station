pub mod catalog;
pub mod detect_win32;
pub mod focus_launch;

use anyhow::{Result, bail};
use catalog::{HarnessCatalog, HarnessEntry, LaunchTarget};
use detect_win32::detect_builtin_harnesses;
use focus_launch::{Win32WindowLauncher, WindowLauncher};
use serde::Serialize;
use serde_json::{Value, json};
use station_core::hooks_cli::{install_agent_hooks, is_agent_hooked};
use station_core::http::HarnessApi;
use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Mutex;

#[derive(Debug, Clone, Copy, Serialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum HarnessStatus {
    RunningHooked,
    InstalledHooked,
    NeedsHook,
    NotInstalled,
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
#[serde(tag = "outcome", rename_all = "snake_case")]
pub enum LaunchOutcome {
    FocusedExisting { pid: u32 },
    LaunchedNew,
    NotInstalled,
}

#[derive(Debug, Clone, Serialize)]
pub struct HarnessView {
    pub id: String,
    pub label: String,
    pub hook_agent: Option<String>,
    pub status: HarnessStatus,
    pub custom: bool,
    pub target: Option<LaunchTarget>,
}

pub struct HarnessService<L: WindowLauncher> {
    catalog: Mutex<HarnessCatalog>,
    builtins: Vec<HarnessEntry>,
    launcher: L,
    bin_path: PathBuf,
    port: u16,
    settings_overrides: HashMap<String, PathBuf>,
}

impl HarnessService<Win32WindowLauncher> {
    pub fn new_win32(data_dir: PathBuf, bin_path: PathBuf, port: u16) -> Self {
        Self {
            catalog: Mutex::new(HarnessCatalog::load_or_default(&data_dir)),
            builtins: detect_builtin_harnesses(),
            launcher: Win32WindowLauncher,
            bin_path,
            port,
            settings_overrides: HashMap::new(),
        }
    }
}

impl<L: WindowLauncher> HarnessService<L> {
    pub fn with_components(
        catalog: HarnessCatalog,
        builtins: Vec<HarnessEntry>,
        launcher: L,
        bin_path: PathBuf,
        port: u16,
        settings_overrides: HashMap<String, PathBuf>,
    ) -> Self {
        Self {
            catalog: Mutex::new(catalog),
            builtins,
            launcher,
            bin_path,
            port,
            settings_overrides,
        }
    }

    fn entries(&self) -> Vec<HarnessEntry> {
        let cat = self.catalog.lock().expect("catalog lock");
        cat.all_entries(self.builtins.clone())
    }

    pub fn list(&self) -> Vec<HarnessView> {
        self.entries()
            .into_iter()
            .map(|e| {
                let installed = e.target.is_some();
                let hooked = match &e.hook_agent {
                    Some(agent) => {
                        let ov = self.settings_overrides.get(agent).map(PathBuf::as_path);
                        is_agent_hooked(agent, ov, self.port)
                    }
                    None => true,
                };
                let running = installed
                    && self
                        .launcher
                        .find_running_window(&e.process_names)
                        .is_some();
                let status = if !installed {
                    HarnessStatus::NotInstalled
                } else if !hooked {
                    HarnessStatus::NeedsHook
                } else if running {
                    HarnessStatus::RunningHooked
                } else {
                    HarnessStatus::InstalledHooked
                };
                HarnessView {
                    id: e.id,
                    label: e.label,
                    hook_agent: e.hook_agent,
                    status,
                    custom: e.custom,
                    target: e.target,
                }
            })
            .collect()
    }

    pub fn open(&self, id: &str) -> Result<LaunchOutcome> {
        let Some(entry) = self.entries().into_iter().find(|e| e.id == id) else {
            return Ok(LaunchOutcome::NotInstalled);
        };
        let Some(target) = &entry.target else {
            return Ok(LaunchOutcome::NotInstalled);
        };

        // Auto-hook preflight before focusing or launching the native GUI desktop app.
        if let Some(agent) = &entry.hook_agent {
            let ov = self.settings_overrides.get(agent).map(PathBuf::as_path);
            if !is_agent_hooked(agent, ov, self.port) {
                let _ = install_agent_hooks(agent, ov, &self.bin_path, self.port)?;
            }
        }

        if let Some((pid, hwnd)) = self.launcher.find_running_window(&entry.process_names) {
            self.launcher.focus_window(hwnd)?;
            return Ok(LaunchOutcome::FocusedExisting { pid });
        }

        self.launcher.launch_target(target)?;
        Ok(LaunchOutcome::LaunchedNew)
    }
}

impl<L: WindowLauncher> HarnessApi for HarnessService<L> {
    fn list_json(&self) -> Value {
        json!({ "ok": true, "harnesses": self.list() })
    }

    fn launch(&self, id: &str) -> Result<Value> {
        let outcome = self.open(id)?;
        Ok(json!({ "ok": true, "result": outcome }))
    }

    fn mutate_custom(&self, body: Value) -> Result<Value> {
        let op = body.get("op").and_then(Value::as_str).unwrap_or("upsert");
        let mut cat = self.catalog.lock().expect("catalog lock");
        match op {
            "remove" => {
                let id = body.get("id").and_then(Value::as_str).unwrap_or("");
                let removed = cat.remove_custom(id)?;
                Ok(json!({ "ok": removed }))
            }
            "upsert" => {
                let id = body
                    .get("id")
                    .and_then(Value::as_str)
                    .unwrap_or("")
                    .to_owned();
                let label = body
                    .get("label")
                    .and_then(Value::as_str)
                    .unwrap_or(&id)
                    .to_owned();
                let path = body
                    .get("path")
                    .and_then(Value::as_str)
                    .map(PathBuf::from);
                let target = path.map(|p| {
                    if p.extension()
                        .and_then(|e| e.to_str())
                        .is_some_and(|e| e.eq_ignore_ascii_case("lnk"))
                    {
                        LaunchTarget::Shortcut(p)
                    } else {
                        LaunchTarget::Exe(p)
                    }
                });
                let process_names = body
                    .get("process_names")
                    .and_then(Value::as_array)
                    .map(|a| {
                        a.iter()
                            .filter_map(Value::as_str)
                            .map(str::to_owned)
                            .collect()
                    })
                    .unwrap_or_default();
                cat.upsert_custom(HarnessEntry {
                    id,
                    label,
                    hook_agent: body
                        .get("hook_agent")
                        .and_then(Value::as_str)
                        .map(str::to_owned),
                    target,
                    custom: true,
                    process_names,
                })?;
                Ok(json!({ "ok": true }))
            }
            other => bail!("unknown op: {other}"),
        }
    }
}

pub fn launcher_ready() -> bool {
    true
}

#[cfg(test)]
mod tests {
    use super::*;
    use axum::body::Body;
    use axum::http::{Request, StatusCode};
    use station_core::http::{AppState, build_router};
    use std::sync::{Arc, Mutex};
    use tower::ServiceExt;

    #[derive(Default)]
    struct MockLauncher {
        running: Mutex<Option<(u32, isize)>>,
        focused: Mutex<Vec<isize>>,
        launched: Mutex<Vec<LaunchTarget>>,
    }

    impl WindowLauncher for MockLauncher {
        fn find_running_window(&self, _process_names: &[String]) -> Option<(u32, isize)> {
            *self.running.lock().unwrap()
        }

        fn focus_window(&self, hwnd: isize) -> Result<()> {
            self.focused.lock().unwrap().push(hwnd);
            Ok(())
        }

        fn launch_target(&self, target: &LaunchTarget) -> Result<()> {
            self.launched.lock().unwrap().push(target.clone());
            Ok(())
        }
    }

    #[tokio::test]
    async fn auto_hooks_focuses_existing_launches_new_and_blocks_spoofed_origin() {
        let dir = tempfile::tempdir().unwrap();
        let agy_hooks = dir.path().join("agy-hooks.json");
        let mut overrides = HashMap::new();
        overrides.insert("antigravity".to_owned(), agy_hooks.clone());

        let builtins = vec![
            HarnessEntry {
                id: "antigravity".to_owned(),
                label: "Antigravity".to_owned(),
                hook_agent: Some("antigravity".to_owned()),
                target: Some(LaunchTarget::Exe(PathBuf::from("C:/Apps/Antigravity.exe"))),
                custom: false,
                process_names: vec!["Antigravity.exe".to_owned()],
            },
            HarnessEntry {
                id: "missing".to_owned(),
                label: "Missing".to_owned(),
                hook_agent: None,
                target: None,
                custom: false,
                process_names: vec![],
            },
        ];

        let mock = Arc::new(MockLauncher::default());
        struct ArcLauncher(Arc<MockLauncher>);
        impl WindowLauncher for ArcLauncher {
            fn find_running_window(&self, p: &[String]) -> Option<(u32, isize)> {
                self.0.find_running_window(p)
            }
            fn focus_window(&self, hwnd: isize) -> Result<()> {
                self.0.focus_window(hwnd)
            }
            fn launch_target(&self, t: &LaunchTarget) -> Result<()> {
                self.0.launch_target(t)
            }
        }

        let svc = Arc::new(HarnessService::with_components(
            HarnessCatalog::load_or_default(dir.path()),
            builtins,
            ArcLauncher(mock.clone()),
            PathBuf::from("D:/app/agents-station.exe"),
            4310,
            overrides,
        ));

        // 1. Not hooked & not running -> auto-installs hook + launches new
        assert_eq!(svc.list()[0].status, HarnessStatus::NeedsHook);
        assert_eq!(svc.open("antigravity").unwrap(), LaunchOutcome::LaunchedNew);
        assert!(is_agent_hooked("antigravity", Some(&agy_hooks), 4310));
        assert_eq!(mock.launched.lock().unwrap().len(), 1);
        assert_eq!(mock.focused.lock().unwrap().len(), 0);

        // 2. Already running -> focuses existing window, does NOT launch new
        *mock.running.lock().unwrap() = Some((4242, 999));
        assert_eq!(svc.list()[0].status, HarnessStatus::RunningHooked);
        assert_eq!(
            svc.open("antigravity").unwrap(),
            LaunchOutcome::FocusedExisting { pid: 4242 }
        );
        assert_eq!(mock.focused.lock().unwrap().as_slice(), &[999]);
        assert_eq!(mock.launched.lock().unwrap().len(), 1);

        // 3. Uninstalled -> NotInstalled without panic
        assert_eq!(svc.open("missing").unwrap(), LaunchOutcome::NotInstalled);

        // 4. HTTP POST /api/harnesses/launch with spoofed Origin -> 403 and never invokes launcher
        let app_state = Arc::new(AppState::new(dir.path().to_path_buf(), Some(svc)).unwrap());
        let router = build_router(app_state);
        let req = Request::builder()
            .method("POST")
            .uri("/api/harnesses/launch")
            .header("host", "127.0.0.1:4310")
            .header("origin", "https://evil.com")
            .header("content-type", "application/json")
            .body(Body::from(json!({"id": "antigravity"}).to_string()))
            .unwrap();
        let resp = router.oneshot(req).await.unwrap();
        assert_eq!(resp.status(), StatusCode::FORBIDDEN);
        assert_eq!(mock.focused.lock().unwrap().len(), 1);
        assert_eq!(mock.launched.lock().unwrap().len(), 1);
    }
}
