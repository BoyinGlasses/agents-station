pub mod bridge;
pub mod capture;
pub mod cli;
pub mod clip_music;
pub mod commands;
pub mod fps;
pub mod gpu;
pub mod instance;
pub mod tray;

use bevy::app::AppExit;
use bevy::pbr::{DirectionalLightShadowMap, PointLightShadowMap};
use bevy::prelude::*;
use bevy::render::RenderPlugin;
use bevy::render::settings::{Backends, InstanceFlags, MemoryHints, RenderCreation, WgpuSettings};
use bevy::window::WindowCloseRequested;
use bevy_egui::EguiPlugin;
use clap::Parser;
use std::path::PathBuf;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Duration;

fn resolve_assets_dir() -> PathBuf {
    let candidates = [
        commands::resolve_install_root().join("assets"),
        std::env::current_exe()
            .ok()
            .and_then(|p| p.parent().map(|d| d.join("assets")))
            .unwrap_or_default(),
        PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../assets"),
        PathBuf::from("assets"),
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
    station_3d::camera::spawn_lite_2d_camera(&mut commands);
}

/// Frames the `--capture-scene street-shops` camera directly onto the nearest walk-in Vietnamese
/// shop row, sidewalk props, and street traffic from an unobstructed 3/4 street angle.
fn frame_street_shops_camera(
    mut rig: Option<ResMut<station_3d::camera::CameraRig>>,
    shops: Query<&station_3d::street::shops_walkin::WalkInShopRoom>,
) {
    let Some(rig) = rig.as_deref_mut() else {
        return;
    };
    let mut rooms: Vec<(Vec3, Vec3)> = shops.iter().map(|s| (s.doorway, s.dir_out)).collect();
    if rooms.is_empty() {
        return;
    }
    rooms.sort_by(|a, b| {
        a.0.distance_squared(Vec3::new(3.0, 0.0, 0.7))
            .partial_cmp(&b.0.distance_squared(Vec3::new(3.0, 0.0, 0.7)))
            .unwrap_or(std::cmp::Ordering::Equal)
    });
    let anchor = rooms[0].0;
    let dir_out = rooms[0].1;
    // Average nearby walk-in shops sharing this street corridor so 2–3 adjacent shops fill the frame
    let mut sum = anchor;
    let mut count = 1.0_f32;
    for &(pos, dout) in rooms.iter().skip(1) {
        if dout.dot(dir_out) > 0.5 && pos.distance(anchor) < 16.0 {
            sum += pos;
            count += 1.0;
        }
    }
    let focus = sum / count;
    rig.target = Vec3::new(focus.x, 1.35, focus.z);
    // Look from outward +X/+Z side so shop fascias, interiors, and vehicles face the camera cleanly
    rig.yaw = if dir_out.x > 0.5 { 1.05 } else { 0.52 };
    rig.pitch = 0.42;
    rig.distance = 26.0;
    rig.viewport_height = 14.5;
}

fn handle_window_close_requests(
    mut close_events: EventReader<WindowCloseRequested>,
    worker_mode: Res<bridge::GuiWorkerMode>,
    ui: Option<Res<station_ui::UiState>>,
    bridge: Option<Res<bridge::LiveServerBridge>>,
    mut app_exit: EventWriter<AppExit>,
) {
    if close_events.read().next().is_none() {
        return;
    }
    if let Some(b) = bridge.as_deref() {
        b.notify_presence_unfocused();
    }
    let close_to_tray = ui.as_ref().map(|u| u.auto_close_tray).unwrap_or(true);
    if worker_mode.0 {
        if close_to_tray {
            let _ = tray::notify_parent_in_tray(true);
        } else {
            let _ = tray::request_parent_quit();
        }
    }
    app_exit.write(AppExit::Success);
}

fn resolve_effective_gpu_pref(cli: &cli::Cli) -> cli::GpuPreference {
    if cli.gpu != cli::GpuPreference::Auto {
        return cli.gpu;
    }
    let data_dir = commands::resolve_runtime_data_dir();
    let prefs = station_core::prefs::PrefsStore::load(&data_dir);
    match prefs.get("gpuPref").and_then(|v| v.as_str()) {
        Some("integrated") => cli::GpuPreference::Integrated,
        Some("discrete") => cli::GpuPreference::Discrete,
        _ => cli::GpuPreference::Auto,
    }
}

pub fn initial_window_size(cli: &cli::Cli) -> (f32, f32) {
    if cli.panel || cli.capture_scene.as_deref() == Some("panel") {
        (420.0, 640.0)
    } else {
        (1280.0, 720.0)
    }
}

fn is_lite_or_panel(cli: &cli::Cli) -> bool {
    cli.lite
        || cli.panel
        || matches!(cli.capture_scene.as_deref(), Some("lite-2d" | "panel"))
}

fn build_default_plugins(cli: &cli::Cli) -> bevy::app::PluginGroupBuilder {
    let default_backend = if is_lite_or_panel(cli) {
        Backends::DX12
    } else {
        Backends::VULKAN
    };
    let effective_gpu = resolve_effective_gpu_pref(cli);
    let (win_w, win_h) = initial_window_size(cli);
    // Keep PbrPlugin active even in --lite/--panel so switching 2D -> 3D works seamlessly at runtime.
    DefaultPlugins
        .set(WindowPlugin {
            primary_window: Some(Window {
                title: "Agents Station".to_owned(),
                resolution: (win_w, win_h).into(),
                ..default()
            }),
            close_when_requested: false,
            ..default()
        })
        .set(RenderPlugin {
            render_creation: RenderCreation::Automatic(WgpuSettings {
                backends: Some(Backends::from_env().unwrap_or(default_backend)),
                power_preference: gpu::wgpu_power_preference(effective_gpu),
                memory_hints: MemoryHints::MemoryUsage,
                instance_flags: InstanceFlags::empty(),
                ..default()
            }),
            ..default()
        })
}

fn build_gui_app(cli: &cli::Cli) -> App {
    let mut app = App::new();
    let capture_mode = cli.capture_scene.is_some();
    let is_lite_2d = is_lite_or_panel(cli);
    app.add_plugins(build_default_plugins(cli))
        .add_plugins(MeshPickingPlugin)
        .insert_resource(DirectionalLightShadowMap { size: 2048 })
        .insert_resource(PointLightShadowMap { size: 256 });
    app.add_plugins(EguiPlugin {
        enable_multipass_for_primary_context: true,
    })
    .add_plugins(fps::AdaptiveFpsPlugin { capture_mode })
    .add_plugins(tray::TrayPlugin)
    .insert_resource(bridge::GuiWorkerMode(cli.gui_worker))
    .add_systems(Update, handle_window_close_requests)
    .add_systems(
        PostUpdate,
        (
            bridge::sync_camera_viewport_to_stage
                .after(bevy_egui::EguiPostUpdateSet::EndPass)
                .before(bevy::render::camera::CameraUpdateSystem)
                .run_if(resource_exists::<station_ui::StageViewportRect>),
            bridge::project_stage_labels_to_ui
                .after(bevy::render::camera::CameraUpdateSystem)
                .after(bevy::transform::TransformSystem::TransformPropagate)
                .run_if(
                    resource_exists::<station_ui::StageViewportRect>
                        .and(resource_exists::<station_ui::UiState>)
                        .and(resource_exists::<station_ui::I18nStore>),
                ),
        ),
    );

    let assets_dir = resolve_assets_dir();
    let scene = cli.capture_scene.as_deref();

    match scene {
        Some("smoke") => {
            app.add_plugins(station_3d::SmokeScenePlugin)
                .add_plugins(station_ui::SmokeHudPlugin { assets_dir });
        }
        Some("chibi-closeup") => {
            app.add_plugins(station_3d::ChibiCloseupPlugin)
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
            let initial_lang = cli.lang.clone().unwrap_or_else(|| "en".to_owned());
            app.insert_resource(ClearColor(Color::srgb(0.06, 0.075, 0.105)))
                .add_systems(Startup, spawn_lite_2d_camera)
                .add_plugins(station_ui::StationUiPlugin {
                    assets_dir,
                    initial_state: ui_state,
                    initial_lang,
                });
        }
        Some("panel") => {
            let mut ui_state = station_ui::UiState::with_demo_seed(
                station_3d::OfficeLayout::Pro,
                station_3d::BackdropKind::City,
                false,
            );
            ui_state.view_mode = station_ui::ViewMode::PanelCompact;
            ui_state.sys.private_mb = 74.0;
            let initial_lang = cli.lang.clone().unwrap_or_else(|| "en".to_owned());
            app.insert_resource(ClearColor(Color::srgb(0.06, 0.075, 0.105)))
                .add_systems(Startup, spawn_lite_2d_camera)
                .add_plugins(station_ui::StationUiPlugin {
                    assets_dir,
                    initial_state: ui_state,
                    initial_lang,
                });
        }
        Some("hud-pro") => {
            let ui_state = station_ui::UiState::with_demo_seed(
                station_3d::OfficeLayout::Pro,
                station_3d::BackdropKind::City,
                false,
            );
            let initial_lang = cli.lang.clone().unwrap_or_else(|| "en".to_owned());
            app.add_plugins(station_3d::OfficeWorldPlugin {
                layout: station_3d::OfficeLayout::Pro,
                backdrop: station_3d::BackdropKind::City,
                assets_dir: assets_dir.clone(),
                night: false,
            })
            .add_plugins(station_ui::StationUiPlugin {
                assets_dir,
                initial_state: ui_state,
                initial_lang,
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
        Some("street-shops") => {
            app.add_plugins(station_3d::OfficeWorldPlugin {
                layout: station_3d::OfficeLayout::Pro,
                backdrop: station_3d::BackdropKind::City,
                assets_dir: assets_dir.clone(),
                night: false,
            })
            .add_systems(PostStartup, frame_street_shops_camera)
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
            let data_dir = commands::resolve_runtime_data_dir();
            let prefs = station_core::prefs::PrefsStore::load(&data_dir);
            let initial_lang = cli
                .lang
                .clone()
                .unwrap_or_else(|| prefs.lang().to_owned());
            let mut ui_state = station_ui::UiState::new_live(
                station_3d::OfficeLayout::Pro,
                station_3d::BackdropKind::City,
                false,
            );
            ui_state.server_port = cli.port;
            if let Some(g) = prefs.get("gpuPref").and_then(|v| v.as_str()) {
                ui_state.gpu_pref = station_ui::state::GpuPrefUi::from_pref_str(g);
            }
            if let Some(s) = prefs.get("soundOn").and_then(|v| v.as_bool()) {
                ui_state.sound_on = s;
            }
            let (initial_city_code, initial_city_display) = station_3d::life::weather_city_display(
                prefs
                    .get("weatherCity")
                    .and_then(|v| v.as_str())
                    .unwrap_or("hcm"),
            );
            ui_state.weather_city = initial_city_display;
            if let Ok(raw) = std::fs::read_to_string(data_dir.join("whiteboard.json"))
                && let Ok(val) = serde_json::from_str::<serde_json::Value>(&raw)
            {
                if let Some(text) = val.get("text").and_then(|v| v.as_str()) {
                    ui_state.whiteboard_text = text.to_owned();
                }
                if let Some(arr) = val.get("todos").and_then(|v| v.as_array()) {
                    ui_state.session_todos = arr
                        .iter()
                        .enumerate()
                        .filter_map(|(i, t)| {
                            let text = t.get("text").and_then(|v| v.as_str())?.trim();
                            if text.is_empty() {
                                return None;
                            }
                            let id = t
                                .get("id")
                                .and_then(|v| v.as_str())
                                .filter(|s| !s.is_empty())
                                .map(str::to_owned)
                                .unwrap_or_else(|| format!("td-init-{i}"));
                            let done = t.get("done").and_then(|v| v.as_bool()).unwrap_or(false);
                            let done_at = t.get("doneAt").and_then(|v| v.as_u64()).unwrap_or(0);
                            Some(station_ui::state::TodoItem {
                                id,
                                text: text.to_owned(),
                                done,
                                done_at,
                            })
                        })
                        .collect();
                }
            }
            let initial_sound = ui_state.sound_on;
            let initial_city = initial_city_code;
            if is_lite_2d {
                ui_state.view_mode = if cli.panel {
                    station_ui::ViewMode::PanelCompact
                } else {
                    station_ui::ViewMode::Lite2D
                };
                app.add_plugins(station_3d::OfficeWorldRuntimePlugin {
                    assets_dir: assets_dir.clone(),
                })
                .insert_resource(ClearColor(Color::srgb(0.06, 0.075, 0.105)))
                .add_systems(Startup, spawn_lite_2d_camera);
            } else {
                app.add_plugins(station_3d::OfficeWorldPlugin {
                    layout: station_3d::OfficeLayout::Pro,
                    backdrop: station_3d::BackdropKind::City,
                    assets_dir: assets_dir.clone(),
                    night: false,
                });
            }
            app.world_mut()
                .resource_mut::<station_3d::audio::SfxEngine>()
                .enabled = initial_sound;
            app.world_mut()
                .resource_mut::<station_3d::life::OfficeLifeState>()
                .weather_city = initial_city;
            app.add_plugins(station_ui::StationUiPlugin {
                assets_dir: assets_dir.clone(),
                initial_state: ui_state,
                initial_lang,
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

struct ParentHttpGuard {
    state: Arc<station_core::http::AppState>,
    shutdown: tokio::sync::watch::Sender<bool>,
    thread: Option<std::thread::JoinHandle<()>>,
}

impl Drop for ParentHttpGuard {
    fn drop(&mut self) {
        let _ = self.shutdown.send(true);
        if let Some(handle) = self.thread.take() {
            let _ = handle.join();
        }
    }
}

fn start_parent_http_if_needed(cli: &cli::Cli) -> Option<ParentHttpGuard> {
    if bridge::probe_local_health(cli.port) {
        return None;
    }

    let data_dir = commands::resolve_runtime_data_dir();
    let bin_path = std::env::current_exe().unwrap_or_else(|_| PathBuf::from("agents-station.exe"));
    let harnesses: Arc<dyn station_core::http::HarnessApi> =
        Arc::new(station_launcher::HarnessService::new_win32(
            data_dir.clone(),
            bin_path,
            cli.port,
        ));
    let Ok(state) = station_core::http::AppState::new(data_dir, Some(harnesses)) else {
        return None;
    };
    let state = Arc::new(state);
    let state_guard = state.clone();
    let host = cli.host.clone();
    let port = cli.port;
    let (shutdown_tx, mut shutdown_rx) = tokio::sync::watch::channel(false);
    let (ready_tx, ready_rx) = std::sync::mpsc::sync_channel::<bool>(1);

    let thread = std::thread::Builder::new()
        .name("station-parent-http".to_owned())
        .spawn(move || {
            let Ok(rt) = tokio::runtime::Builder::new_multi_thread()
                .worker_threads(2)
                .enable_all()
                .build()
            else {
                let _ = ready_tx.send(false);
                return;
            };
            rt.block_on(async move {
                match station_core::http::start_http_server(state.clone(), &host, port).await {
                    Ok(handle) => {
                        let _ = ready_tx.send(true);
                        while !*shutdown_rx.borrow() {
                            if shutdown_rx.changed().await.is_err() {
                                break;
                            }
                        }
                        let _ = handle.shutdown.send(true);
                        let _ = state.flush_all();
                    }
                    Err(_) => {
                        let _ = ready_tx.send(false);
                    }
                }
            });
        })
        .ok()?;

    let bound = ready_rx
        .recv_timeout(Duration::from_secs(3))
        .unwrap_or(false);
    if bound {
        Some(ParentHttpGuard {
            state: state_guard,
            shutdown: shutdown_tx,
            thread: Some(thread),
        })
    } else {
        let _ = shutdown_tx.send(true);
        let _ = thread.join();
        None
    }
}

/// Runs the lightweight parent process hosting `:4310` HTTP daemon (or attaching to an
/// existing external service) + persistent Win32 `TrayDaemon` (<= 45 MB Private WS),
/// spawning `--gui-worker` on demand for the Bevy window.
fn run_tray_parent(cli: &cli::Cli) -> anyhow::Result<()> {
    let Some(_lock) = instance::InstanceLock::try_acquire(instance::GUI_MUTEX_NAME)? else {
        let _ = instance::signal_show_event(instance::SHOW_EVENT_NAME);
        return Ok(());
    };

    let _http_guard = start_parent_http_if_needed(cli);

    let worker_cfg = tray::TrayWorkerConfig {
        host: cli.host.clone(),
        port: cli.port,
        gpu: cli.gpu,
        default_lite: cli.lite,
    };
    let mut daemon = tray::TrayDaemon::new(cli.port, worker_cfg)?;
    if let Some(guard) = &_http_guard {
        let st = guard.state.clone();
        daemon.set_on_worker_hidden(Arc::new(move || {
            st.office
                .lock()
                .unwrap_or_else(|e| e.into_inner())
                .presence_focused = false;
        }));
    }
    let hwnd_raw = daemon.hwnd_raw();

    let stop_listener = Arc::new(AtomicBool::new(false));
    let show_thread = match instance::ShowEventListener::create(instance::SHOW_EVENT_NAME) {
        Ok(listener) => {
            let stop_bg = stop_listener.clone();
            std::thread::Builder::new()
                .name("station-show-event".to_owned())
                .spawn(move || {
                    while !stop_bg.load(Ordering::Relaxed) {
                        if listener.wait_timeout(200) && !stop_bg.load(Ordering::Relaxed) {
                            let _ = tray::post_show_gui(hwnd_raw);
                        }
                    }
                })
                .ok()
        }
        Err(_) => None,
    };

    daemon.spawn_initial_worker();
    daemon.run_message_loop();

    stop_listener.store(true, Ordering::Relaxed);
    let _ = instance::signal_show_event(instance::SHOW_EVENT_NAME);
    if let Some(handle) = show_thread {
        let _ = handle.join();
    }
    daemon.cleanup();
    // `_http_guard` drops here: shuts down HTTP listener and runs `state.flush_all()`.
    Ok(())
}

fn main() -> anyhow::Result<()> {
    let cli = cli::Cli::parse();
    let is_gui = matches!(cli.command, None | Some(cli::Command::Gui))
        && cli.capture_scene.is_none();

    if is_gui && !cli.gui_worker && !cli.panel {
        return run_tray_parent(&cli);
    }

    if cli.capture_scene.is_some() || is_gui {
        let mut app = build_gui_app(&cli);
        app.run();
        return Ok(());
    }

    let data_dir = commands::resolve_runtime_data_dir();
    let bin_path = std::env::current_exe().unwrap_or_else(|_| PathBuf::from("agents-station.exe"));
    let harnesses: Arc<dyn station_core::http::HarnessApi> =
        Arc::new(station_launcher::HarnessService::new_win32(
            data_dir,
            bin_path,
            cli.port,
        ));
    commands::run_cli_command(&cli, Some(harnesses))
}

#[cfg(test)]
mod tests {
    use super::*;
    use bevy::app::AppExit;
    use bevy::window::WindowCloseRequested;

    #[test]
    fn worker_window_close_with_close_to_tray_exits_only_worker_cleanly() {
        let mut app = App::new();
        let mut ui = station_ui::UiState::default();
        ui.auto_close_tray = true;
        app.add_event::<WindowCloseRequested>()
            .add_event::<AppExit>()
            .insert_resource(bridge::GuiWorkerMode(true))
            .insert_resource(ui)
            .add_systems(Update, handle_window_close_requests);

        app.world_mut()
            .send_event(WindowCloseRequested {
                window: Entity::PLACEHOLDER,
            });
        app.update();

        let exits = app.world().resource::<Events<AppExit>>();
        let mut reader = exits.get_cursor();
        let emitted: Vec<&AppExit> = reader.read(exits).collect();
        assert_eq!(emitted.len(), 1);
        assert!(matches!(emitted[0], AppExit::Success));
    }

    #[test]
    fn lite_mode_keeps_pbr_plugin_enabled_for_runtime_3d_switch() {
        let lite_cli = cli::Cli::parse_from(["agents-station", "--lite"]);
        let plugins = build_default_plugins(&lite_cli);
        assert!(
            plugins.enabled::<bevy::pbr::PbrPlugin>(),
            "PbrPlugin must remain enabled in --lite so switching 2D Lite -> 3D works seamlessly"
        );
    }

    #[test]
    fn panel_flag_selects_420x640_compact_window_and_lite_backend() {
        let panel_cli = cli::Cli::parse_from(["agents-station", "--panel"]);
        assert_eq!(initial_window_size(&panel_cli), (420.0, 640.0));
        assert!(is_lite_or_panel(&panel_cli));

        let full_cli = cli::Cli::parse_from(["agents-station"]);
        assert_eq!(initial_window_size(&full_cli), (1280.0, 720.0));
        assert!(!is_lite_or_panel(&full_cli));
    }
}
