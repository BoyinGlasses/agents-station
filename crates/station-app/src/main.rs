pub mod capture;
pub mod cli;
pub mod gpu;
pub mod instance;

use bevy::prelude::*;
use bevy::render::RenderPlugin;
use bevy::render::settings::{RenderCreation, WgpuSettings};
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
                    power_preference: gpu::wgpu_power_preference(cli.gpu),
                    ..default()
                }),
                ..default()
            }),
    )
    .add_plugins(EguiPlugin {
        enable_multipass_for_primary_context: true,
    })
    .add_plugins(station_3d::SmokeScenePlugin)
    .add_plugins(station_ui::SmokeHudPlugin {
        assets_dir: resolve_assets_dir(),
    });

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

    println!(
        "agents-station cli ({}:{}, gpu={:?})",
        cli.host, cli.port, cli.gpu
    );
    Ok(())
}
