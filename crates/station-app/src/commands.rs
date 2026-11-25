use crate::cli::{Cli, Command};
use anyhow::Result;
use station_core::config::resolve_data_dir;
use station_core::hook_client::{HookArgs, run_hook_client};
use station_core::hooks_cli::{
    install_agent_hooks, install_windows_service, is_agent_hooked, uninstall_agent_hooks,
    uninstall_windows_service_in, windows_startup_dir,
};
use station_core::http::{AppState, HarnessApi, start_http_server};
use std::collections::HashMap;
use std::io::Read;
use std::path::{Path, PathBuf};
use std::sync::Arc;

fn is_repo_root(dir: &Path) -> bool {
    dir.join("Cargo.toml").is_file() && dir.join("crates").join("station-app").is_dir()
}

fn is_cargo_target_output(dir: &Path) -> bool {
    let name_matches = dir
        .file_name()
        .and_then(|s| s.to_str())
        .is_some_and(|n| matches!(n.to_ascii_lowercase().as_str(), "debug" | "release" | "deps"));
    if name_matches {
        return true;
    }
    dir.ancestors().any(|a| {
        a.file_name()
            .and_then(|s| s.to_str())
            .is_some_and(|n| matches!(n.to_ascii_lowercase().as_str(), "target" | "cargo-target"))
    })
}

fn resolve_install_root_from(exe_path: Option<&Path>, manifest_dir: Option<&Path>) -> PathBuf {
    let repo_from_manifest = manifest_dir
        .and_then(|m| m.parent())
        .and_then(|p| p.parent())
        .filter(|root| is_repo_root(root))
        .map(Path::to_path_buf);

    if let Some(exe_dir) = exe_path.and_then(Path::parent) {
        for ancestor in exe_dir.ancestors() {
            if is_repo_root(ancestor) {
                return ancestor.to_path_buf();
            }
        }
        if exe_dir.join("assets").join("fonts").is_dir() {
            return exe_dir.to_path_buf();
        }
        if is_cargo_target_output(exe_dir)
            && let Some(repo) = repo_from_manifest
        {
            return repo;
        }
        return exe_dir.to_path_buf();
    }

    repo_from_manifest.unwrap_or_else(|| PathBuf::from("."))
}

/// Anchors default runtime `data/` to the workspace repo root (during dev/cargo runs)
/// or the executable's parent directory (when installed/portable) — never arbitrary CWD.
pub fn resolve_install_root() -> PathBuf {
    let exe = std::env::current_exe().ok();
    let manifest = Path::new(env!("CARGO_MANIFEST_DIR"));
    resolve_install_root_from(exe.as_deref(), Some(manifest))
}

pub fn resolve_runtime_data_dir_with(env_dir: Option<&str>, install_root: &Path) -> PathBuf {
    resolve_data_dir(install_root, env_dir)
}

pub fn resolve_runtime_data_dir() -> PathBuf {
    let root = resolve_install_root();
    let env_dir = std::env::var("OFFICE_DATA_DIR").ok();
    resolve_runtime_data_dir_with(env_dir.as_deref(), &root)
}

pub fn run_cli_command(cli: &Cli, harnesses: Option<Arc<dyn HarnessApi>>) -> Result<()> {
    let rt = tokio::runtime::Builder::new_multi_thread()
        .worker_threads(2)
        .enable_all()
        .build()?;
    rt.block_on(run_cli_async(cli, harnesses))
}

async fn run_cli_async(cli: &Cli, harnesses: Option<Arc<dyn HarnessApi>>) -> Result<()> {
    let bin_path = std::env::current_exe().unwrap_or_else(|_| PathBuf::from("agents-station.exe"));
    let data_dir = resolve_runtime_data_dir();

    match &cli.command {
        Some(Command::Serve { lan }) => {
            let host = if *lan { "0.0.0.0" } else { cli.host.as_str() };
            let state = Arc::new(AppState::new(data_dir, harnesses)?);
            let handle = start_http_server(state.clone(), host, cli.port).await?;
            println!("Agents Station server listening on http://{}", handle.addr);

            let _ = tokio::signal::ctrl_c().await;
            let _ = handle.shutdown.send(true);
            state.flush_all()?;
            Ok(())
        }
        Some(Command::Hook(args)) => {
            let mut stdin_bytes = Vec::new();
            let _ = std::io::stdin().read_to_end(&mut stdin_bytes);
            let env: HashMap<String, String> = std::env::vars().collect();
            let out = run_hook_client(
                &HookArgs {
                    url: args.url.clone(),
                    r#async: args.r#async,
                    fast: args.fast,
                    wait: args.wait,
                    job: args.job,
                    agent: args.agent.clone(),
                    event: args.event.clone(),
                    response_json: args.response_json,
                },
                &stdin_bytes,
                &env,
            )
            .await;
            if !out.is_empty() {
                print!("{out}");
            }
            Ok(())
        }
        Some(Command::Install { agent, settings }) => {
            let rep = install_agent_hooks(agent, settings.as_deref(), &bin_path, cli.port)?;
            println!(
                "{}: installed {} hook(s) in {}",
                rep.agent,
                rep.added,
                rep.settings_path.display()
            );
            Ok(())
        }
        Some(Command::Uninstall { agent, settings }) => {
            let removed = uninstall_agent_hooks(agent, settings.as_deref(), cli.port)?;
            println!(
                "{}: {}",
                agent,
                if removed {
                    "hooks removed"
                } else {
                    "no Agents Station hooks found"
                }
            );
            Ok(())
        }
        Some(Command::Service { action, lan }) => {
            let host = if *lan { "0.0.0.0" } else { cli.host.as_str() };
            let log_path = data_dir.join("server.log");
            match action.as_str() {
                "install" => {
                    let file = install_windows_service(&bin_path, host, cli.port, &log_path)?;
                    println!("Service installed: {}", file.display());
                }
                "uninstall" => {
                    let ok = uninstall_windows_service_in(&windows_startup_dir())?;
                    println!(
                        "Service {}",
                        if ok { "removed" } else { "not installed" }
                    );
                }
                _ => {
                    let exists = windows_startup_dir().join("Agents Station.vbs").exists();
                    println!("Service: {}", if exists { "installed" } else { "missing" });
                }
            }
            Ok(())
        }
        Some(Command::Doctor) => {
            println!("Agents Station Doctor (port {})", cli.port);
            for agent in ["claude", "codex", "opencode", "zcode", "antigravity"] {
                let hooked = is_agent_hooked(agent, None, cli.port);
                println!("  {agent:<12} hooks: {}", if hooked { "ok" } else { "missing" });
            }
            Ok(())
        }
        None | Some(Command::Gui) => Ok(()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Mutex;

    static ENV_CWD_TEST_LOCK: Mutex<()> = Mutex::new(());

    struct RestoreCwd(PathBuf);
    impl Drop for RestoreCwd {
        fn drop(&mut self) {
            let _ = std::env::set_current_dir(&self.0);
        }
    }

    #[test]
    fn resolve_runtime_data_dir_ignores_unrelated_temp_cwd_when_env_unset() {
        let _serial = ENV_CWD_TEST_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        let orig = std::env::current_dir().unwrap();
        let _guard = RestoreCwd(orig);
        let tmp = tempfile::tempdir().unwrap();
        std::env::set_current_dir(tmp.path()).unwrap();

        let install_root = resolve_install_root();
        let resolved = resolve_runtime_data_dir_with(None, &install_root);

        assert_ne!(
            resolved,
            tmp.path().join("data"),
            "data_dir must not anchor to arbitrary CWD"
        );
        assert_eq!(resolved, install_root.join("data"));

        let custom = tmp.path().join("override-data");
        assert_eq!(
            resolve_runtime_data_dir_with(custom.to_str(), &install_root),
            custom
        );
    }

    #[test]
    fn resolve_install_root_anchors_installed_exe_to_exe_parent() {
        let tmp = tempfile::tempdir().unwrap();
        let installed_exe = tmp.path().join("agents-station.exe");
        let root = resolve_install_root_from(Some(&installed_exe), None);
        assert_eq!(root, tmp.path());
    }
}
