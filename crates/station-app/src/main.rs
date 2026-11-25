pub mod bridge;
pub mod capture;
pub mod cli;
pub mod commands;
pub mod fps;
pub mod gpu;
pub mod instance;
pub mod tray;

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

/// Bare camera for ultra-low RAM 2D Lite mode (zero 3D meshes, lights, or textures).
fn spawn_lite_2d_camera(mut commands: Commands) {
    commands.spawn((
        Camera3d::default(),
        bevy::core_pipeline::tonemapping::Tonemapping::None,
        Msaa::Off,
        Transform::from_xyz(0.0, 0.0, 5.0).looking_at(Vec3::ZERO, Vec3::Y),
    ));
}

fn build_gui_app(cli: &cli::Cli) -> App {
    let mut app = App::new();
    let capture_mode = cli.capture_scene.is_some();
    let is_lite_2d = cli.capture_scene.as_deref() == Some("lite-2d");
    let plugins = DefaultPlugins
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
        });
    if is_lite_2d {
        app.add_plugins(plugins.disable::<bevy::pbr::PbrPlugin>());
    } else {
        app.add_plugins(plugins)
            .insert_resource(DirectionalLightShadowMap { size: 1024 })
            .insert_resource(PointLightShadowMap { size: 256 });
    }
    app.add_plugins(EguiPlugin {
        enable_multipass_for_primary_context: true,
    })
    .add_plugins(fps::AdaptiveFpsPlugin { capture_mode })
    .add_plugins(tray::TrayPlugin);

    let assets_dir = resolve_assets_dir();
    let scene = cli.capture_scene.as_deref();

    match scene {
        Some("smoke") => {
            app.add_plugins(station_3d::SmokeScenePlugin)
                .add_plugins(station_ui::SmokeHudPlugin { assets_dir });
        }
        Some("lite-2d") => {
            let mut ui_state = station_ui::UiState::with_demo_seed(
                station_3d::OfficeLayout::Pro,
                station_3d::BackdropKind::City,
                false,
            );
            ui_state.view_mode = station_ui::ViewMode::Lite2D;
            ui_state.sys.private_mb = 78.0;
            app.insert_resource(ClearColor(Color::srgb(0.06, 0.075, 0.105)))
                .add_systems(Startup, spawn_lite_2d_camera)
                .add_plugins(station_ui::StationUiPlugin {
                    assets_dir,
                    initial_state: ui_state,
                    initial_lang: "en".to_owned(),
                });
        }
        Some("hud-pro") => {
            let ui_state = station_ui::UiState::with_demo_seed(
                station_3d::OfficeLayout::Pro,
                station_3d::BackdropKind::City,
                false,
            );
            app.add_plugins(station_3d::OfficeWorldPlugin {
                layout: station_3d::OfficeLayout::Pro,
                backdrop: station_3d::BackdropKind::City,
                assets_dir: assets_dir.clone(),
                night: false,
            })
            .add_plugins(station_ui::StationUiPlugin {
                assets_dir,
                initial_state: ui_state,
                initial_lang: "en".to_owned(),
            });
        }
        Some("cozy") => {
            app.add_plugins(station_3d::OfficeWorldPlugin {
                layout: station_3d::OfficeLayout::Cozy,
                backdrop: station_3d::BackdropKind::City,
                assets_dir: assets_dir.clone(),
                night: false,
            })
            .add_plugins(station_ui::SmokeHudPlugin { assets_dir });
        }
        Some("hq") => {
            app.add_plugins(station_3d::OfficeWorldPlugin {
                layout: station_3d::OfficeLayout::Hq,
                backdrop: station_3d::BackdropKind::City,
                assets_dir: assets_dir.clone(),
                night: false,
            })
            .add_plugins(station_ui::SmokeHudPlugin { assets_dir });
        }
        Some("pro-country") => {
            app.add_plugins(station_3d::OfficeWorldPlugin {
                layout: station_3d::OfficeLayout::Pro,
                backdrop: station_3d::BackdropKind::Countryside,
                assets_dir: assets_dir.clone(),
                night: false,
            })
            .add_plugins(station_ui::SmokeHudPlugin { assets_dir });
        }
        Some("pro-beach") => {
            app.add_plugins(station_3d::OfficeWorldPlugin {
                layout: station_3d::OfficeLayout::Pro,
                backdrop: station_3d::BackdropKind::Beach,
                assets_dir: assets_dir.clone(),
                night: false,
            })
            .add_plugins(station_ui::SmokeHudPlugin { assets_dir });
        }
        Some("pro-night") => {
            app.add_plugins(station_3d::OfficeWorldPlugin {
                layout: station_3d::OfficeLayout::Pro,
                backdrop: station_3d::BackdropKind::City,
                assets_dir: assets_dir.clone(),
                night: true,
            })
            .add_plugins(station_ui::SmokeHudPlugin { assets_dir });
        }
        Some(_) => {
            app.add_plugins(station_3d::OfficeWorldPlugin {
                layout: station_3d::OfficeLayout::Pro,
                backdrop: station_3d::BackdropKind::City,
                assets_dir: assets_dir.clone(),
                night: false,
            })
            .add_plugins(station_ui::SmokeHudPlugin { assets_dir });
        }
        None => {
            let ui_state = station_ui::UiState::with_demo_seed(
                station_3d::OfficeLayout::Pro,
                station_3d::BackdropKind::City,
                false,
            );
            let data_dir = commands::resolve_runtime_data_dir();
            app.add_plugins(station_3d::OfficeWorldPlugin {
                layout: station_3d::OfficeLayout::Pro,
                backdrop: station_3d::BackdropKind::City,
                assets_dir: assets_dir.clone(),
                night: false,
            })
            .add_plugins(station_ui::StationUiPlugin {
                assets_dir: assets_dir.clone(),
                initial_state: ui_state,
                initial_lang: "en".to_owned(),
            })
            .add_plugins(bridge::LiveServerBridgePlugin {
                data_dir,
                assets_dir,
                host: cli.host.clone(),
                port: cli.port,
                start_http: true,
            });
        }
    }

    if let Some(scene_name) = &cli.capture_scene {
        let out_path = cli
            .out
            .clone()
            .unwrap_or_else(|| PathBuf::from(format!("artifacts/{scene_name}.png")));
        app.add_plugins(capture::CapturePlugin {
            scene: scene_name.clone(),
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
