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
use std::path::PathBuf;
use std::sync::Arc;

pub fn resolve_runtime_data_dir() -> PathBuf {
    let cwd = std::env::current_dir().unwrap_or_else(|_| PathBuf::from("."));
    let env_dir = std::env::var("OFFICE_DATA_DIR").ok();
    resolve_data_dir(&cwd, env_dir.as_deref())
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
