pub mod capture;
pub mod cli;
pub mod commands;
pub mod gpu;
pub mod instance;

use bevy::pbr::{DirectionalLightShadowMap, PointLightShadowMap};
use bevy::prelude::*;
use bevy::render::RenderPlugin;
use bevy::render::settings::{Backends, InstanceFlags, MemoryHints, RenderCreation, WgpuSettings};
use bevy_egui::EguiPlugin;
use clap::Parser;
use std::path::PathBuf;

const GUI_MUTEX_NAME: &str = "Local\\agents-station-gui";

fn resolve_assets_dir() -> PathBuf {
    let candidates = [
        PathBuf::from("assets"),
        std::env::current_exe()
            .ok()
            .and_then(|p| p.parent().map(|d| d.join("assets")))
            .unwrap_or_default(),
        PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../assets"),
    ];
    for c in candidates {
        if c.join("fonts").exists() {
            return c;
        }
    }
    PathBuf::from("assets")
}

fn build_gui_app(cli: &cli::Cli) -> App {
    let mut app = App::new();
    app.add_plugins(
        DefaultPlugins
            .set(WindowPlugin {
                primary_window: Some(Window {
                    title: "Agents Station".to_owned(),
                    resolution: (1280.0_f32, 720.0_f32).into(),
                    ..default()
                }),
                ..default()
            })
            .set(RenderPlugin {
                render_creation: RenderCreation::Automatic(WgpuSettings {
                    backends: Some(Backends::from_env().unwrap_or(Backends::VULKAN)),
                    power_preference: gpu::wgpu_power_preference(cli.gpu),
                    memory_hints: MemoryHints::MemoryUsage,
                    instance_flags: InstanceFlags::empty(),
                    ..default()
                }),
                ..default()
            }),
    )
    .insert_resource(DirectionalLightShadowMap { size: 1024 })
    .insert_resource(PointLightShadowMap { size: 256 })
    .add_plugins(EguiPlugin {
        enable_multipass_for_primary_context: true,
    });

    let assets_dir = resolve_assets_dir();
    match cli.capture_scene.as_deref() {
        Some("smoke") => {
            app.add_plugins(station_3d::SmokeScenePlugin);
        }
        Some("cozy") => {
            app.add_plugins(station_3d::OfficeWorldPlugin {
                layout: station_3d::OfficeLayout::Cozy,
                backdrop: station_3d::BackdropKind::City,
                assets_dir: assets_dir.clone(),
                night: false,
            });
        }
        Some("hq") => {
            app.add_plugins(station_3d::OfficeWorldPlugin {
                layout: station_3d::OfficeLayout::Hq,
                backdrop: station_3d::BackdropKind::City,
                assets_dir: assets_dir.clone(),
                night: false,
            });
        }
        Some("pro-country") => {
            app.add_plugins(station_3d::OfficeWorldPlugin {
                layout: station_3d::OfficeLayout::Pro,
                backdrop: station_3d::BackdropKind::Countryside,
                assets_dir: assets_dir.clone(),
                night: false,
            });
        }
        Some("pro-beach") => {
            app.add_plugins(station_3d::OfficeWorldPlugin {
                layout: station_3d::OfficeLayout::Pro,
                backdrop: station_3d::BackdropKind::Beach,
                assets_dir: assets_dir.clone(),
                night: false,
            });
        }
        Some("pro-night") => {
            app.add_plugins(station_3d::OfficeWorldPlugin {
                layout: station_3d::OfficeLayout::Pro,
                backdrop: station_3d::BackdropKind::City,
                assets_dir: assets_dir.clone(),
                night: true,
            });
        }
        _ => {
            app.add_plugins(station_3d::OfficeWorldPlugin {
                layout: station_3d::OfficeLayout::Pro,
                backdrop: station_3d::BackdropKind::City,
                assets_dir: assets_dir.clone(),
                night: false,
            });
        }
    }

    app.add_plugins(station_ui::SmokeHudPlugin { assets_dir });

    if let Some(scene) = &cli.capture_scene {
        let out_path = cli
            .out
            .clone()
            .unwrap_or_else(|| PathBuf::from(format!("artifacts/{scene}.png")));
        app.add_plugins(capture::CapturePlugin {
            scene: scene.clone(),
            out_path,
            settle_frames: 6,
        });
    }

    app
}

fn main() -> anyhow::Result<()> {
    let cli = cli::Cli::parse();
    let is_gui = matches!(cli.command, None | Some(cli::Command::Gui))
        && cli.capture_scene.is_none();

    let _lock = if is_gui {
        match instance::InstanceLock::try_acquire(GUI_MUTEX_NAME)? {
            Some(lock) => Some(lock),
            None => return Ok(()),
        }
    } else {
        None
    };

    if cli.capture_scene.is_some() || matches!(cli.command, None | Some(cli::Command::Gui)) {
        let mut app = build_gui_app(&cli);
        app.run();
        return Ok(());
    }

    let data_dir = commands::resolve_runtime_data_dir();
    let bin_path = std::env::current_exe().unwrap_or_else(|_| PathBuf::from("agents-station.exe"));
    let harnesses: std::sync::Arc<dyn station_core::http::HarnessApi> =
        std::sync::Arc::new(station_launcher::HarnessService::new_win32(
            data_dir,
            bin_path,
            cli.port,
        ));
    commands::run_cli_command(&cli, Some(harnesses))
}
