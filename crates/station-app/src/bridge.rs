use crate::fps::AdaptiveFpsState;
use crate::tray::{TrayState, show_native_tray_toast};
use bevy::prelude::*;
use serde_json::json;
use station_3d::{BackdropAssets, BackdropEntity, WorldEntity, WorldPalette};
use station_canvas2d::{BrandRegistry, FontEngine};
use station_core::http::{AppState, HarnessApi, start_http_server};
use station_launcher::HarnessService;
use station_launcher::focus_launch::Win32WindowLauncher;
use station_ui::state::{FeedEntry, HarnessDockItem, PendingPermCard, SessionCard, UiAction};
use station_ui::{I18nStore, UiState, ViewMode};
use std::path::PathBuf;
use std::sync::Arc;
use tokio::sync::watch;

#[derive(Resource)]
pub struct LiveServerBridge {
    pub app_state: Arc<AppState>,
    pub harnesses: Arc<HarnessService<Win32WindowLauncher>>,
    pub assets_dir: PathBuf,
    _shutdown: Option<watch::Sender<bool>>,
    sync_timer: Timer,
}

pub struct LiveServerBridgePlugin {
    pub data_dir: PathBuf,
    pub assets_dir: PathBuf,
    pub host: String,
    pub port: u16,
    pub start_http: bool,
}

impl Plugin for LiveServerBridgePlugin {
    fn build(&self, app: &mut App) {
        let bin_path =
            std::env::current_exe().unwrap_or_else(|_| PathBuf::from("agents-station.exe"));
        let harnesses = Arc::new(HarnessService::new_win32(
            self.data_dir.clone(),
            bin_path,
            self.port,
        ));
        let harness_api: Arc<dyn HarnessApi> = harnesses.clone();
        let Ok(app_state) = AppState::new(self.data_dir.clone(), Some(harness_api)) else {
            return;
        };
        let app_state = Arc::new(app_state);

        let shutdown = if self.start_http {
            let state_bg = app_state.clone();
            let host = self.host.clone();
            let port = self.port;
            let (tx, rx) = watch::channel(false);
            std::thread::Builder::new()
                .name("station-http".to_owned())
                .spawn(move || {
                    if let Ok(rt) = tokio::runtime::Builder::new_multi_thread()
                        .worker_threads(2)
                        .enable_all()
                        .build()
                    {
                        rt.block_on(async move {
                            if let Ok(handle) = start_http_server(state_bg.clone(), &host, port).await
                            {
                                let mut rx = rx;
                                while !*rx.borrow() {
                                    if rx.changed().await.is_err() {
                                        break;
                                    }
                                }
                                let _ = handle.shutdown.send(true);
                                let _ = state_bg.flush_all();
                            }
                        });
                    }
                })
                .ok();
            Some(tx)
        } else {
            None
        };

        app.insert_resource(LiveServerBridge {
            app_state,
            harnesses,
            assets_dir: self.assets_dir.clone(),
            _shutdown: shutdown,
            sync_timer: Timer::from_seconds(1.0, TimerMode::Repeating),
        })
        .add_systems(Update, (sync_live_state_to_ui, process_ui_actions));
    }
}

fn sync_live_state_to_ui(
    time: Res<Time>,
    mut bridge: ResMut<LiveServerBridge>,
    mut ui: ResMut<UiState>,
) {
    bridge.sync_timer.tick(time.delta());
    if !bridge.sync_timer.just_finished() {
        return;
    }

    // Refresh real Windows GUI harness statuses
    let list = bridge.harnesses.list();
    if !list.is_empty() {
        ui.harnesses = list
            .into_iter()
            .map(|h| HarnessDockItem {
                id: h.id,
                label: h.label,
                status: h.status,
                custom: h.custom,
                hook_agent: h.hook_agent,
            })
            .collect();
    }

    // Sync live sessions/permissions/feed when server has real hook traffic
    if let Ok(office) = bridge.app_state.office.lock() {
        if !office.sessions.is_empty() {
            ui.sessions = office
                .sessions
                .values()
                .map(|s| SessionCard {
                    id: s.id.clone(),
                    agent: s.agent.clone(),
                    project: s.name.clone(),
                    status: s.status.clone(),
                    model: s.model.clone().unwrap_or_else(|| "default".to_owned()),
                    last_prompt: s.last_prompt.clone().unwrap_or_default(),
                    tool: s.last.as_ref().map(|t| t.label.clone()),
                    cost_usd: 0.0,
                    tokens: s.tool_calls * 420,
                    pinned: office.pins.contains_key(&s.id),
                })
                .collect();
        }
        if !office.pending_perms.is_empty() {
            ui.pending_perms = office
                .pending_perms
                .values()
                .map(|p| PendingPermCard {
                    id: p.id.clone(),
                    session_id: p.sess.clone(),
                    agent: p.name.clone(),
                    tool: p.tool_type.clone(),
                    summary: p.detail.clone(),
                    kind: p.kind.clone(),
                })
                .collect();
        }
        if !office.feed.is_empty() {
            ui.feed = office
                .feed
                .iter()
                .take(12)
                .map(|f| FeedEntry {
                    ts_label: format!("{}", (f.ts / 1000) % 86400),
                    agent: f.name.clone(),
                    message: f.label.clone(),
                    level: if f.kind == "error" {
                        "err".to_owned()
                    } else {
                        "info".to_owned()
                    },
                })
                .collect();
        }
    }
}

#[allow(clippy::too_many_arguments)]
fn process_ui_actions(
    mut commands: Commands,
    mut ui: ResMut<UiState>,
    mut i18n: ResMut<I18nStore>,
    bridge: Res<LiveServerBridge>,
    world_q: Query<Entity, With<WorldEntity>>,
    backdrop_q: Query<Entity, With<BackdropEntity>>,
    mut pal: Option<ResMut<WorldPalette>>,
    mut backdrop_assets: Option<ResMut<BackdropAssets>>,
    mut materials: Option<ResMut<Assets<StandardMaterial>>>,
    mut images: Option<ResMut<Assets<Image>>>,
    mut clear_color: ResMut<ClearColor>,
    mut lighting: Option<ResMut<station_3d::lights::LightingState>>,
    mut windows: Query<&mut Window>,
    mut fps: Option<ResMut<AdaptiveFpsState>>,
    mut tray: Option<ResMut<TrayState>>,
) {
    let actions = ui.drain_actions();
    for action in actions {
        match action {
            UiAction::LaunchHarness(id) => {
                let svc = bridge.harnesses.clone();
                std::thread::spawn(move || {
                    let _ = svc.open(&id);
                });
            }
            UiAction::AddCustomHarness {
                id,
                label,
                path,
                hook_agent,
            } => {
                let _ = bridge.harnesses.mutate_custom(json!({
                    "op": "upsert",
                    "id": id,
                    "label": label,
                    "path": path,
                    "hook_agent": hook_agent,
                }));
            }
            UiAction::RemoveCustomHarness(id) => {
                let _ = bridge.harnesses.mutate_custom(json!({
                    "op": "remove",
                    "id": id,
                }));
            }
            UiAction::SetViewMode(ViewMode::Lite2D | ViewMode::PanelCompact) => {
                // True Unload 3D world entities + transient GPU images when entering 2D Lite!
                for ent in &world_q {
                    commands.entity(ent).despawn();
                }
                if let Some(imgs) = images.as_mut() {
                    if let Some(p) = pal.as_mut() {
                        p.purge_transient_images(imgs);
                    }
                    if let Some(ba) = backdrop_assets.as_mut() {
                        ba.purge(imgs);
                    }
                }
            }
            UiAction::SwitchBackdrop(bk) => {
                if let (Some(p), Some(ba), Some(mats), Some(imgs)) = (
                    pal.as_mut(),
                    backdrop_assets.as_mut(),
                    materials.as_mut(),
                    images.as_mut(),
                ) {
                    for ent in &backdrop_q {
                        commands.entity(ent).despawn();
                    }
                    ba.purge(imgs);
                    let spec = station_3d::LayoutSpec::for_layout(ui.layout);
                    let mut brands = BrandRegistry::new(bridge.assets_dir.clone());
                    let mut fonts = FontEngine::from_assets_dir(&bridge.assets_dir);
                    station_3d::backdrops::spawn_backdrop(
                        &mut commands,
                        p,
                        mats,
                        imgs,
                        ba,
                        &mut brands,
                        &mut fonts,
                        &spec,
                        bk,
                    );
                    brands.clear();
                }
            }
            UiAction::ToggleNight(night) => {
                if let Some(l) = lighting.as_mut() {
                    l.night = night;
                }
                clear_color.0 = if night {
                    Color::srgb(0.06, 0.08, 0.13)
                } else {
                    Color::srgb(0.75, 0.85, 0.94)
                };
            }
            UiAction::SetLang(code) => {
                i18n.set_lang(&code);
            }
            UiAction::DecidePermission { id, decision } => {
                if let Ok(mut office) = bridge.app_state.office.lock() {
                    office.pending_perms.remove(&id);
                    ui.push_toast("Permission", format!("{id} -> {decision}"));
                }
            }
            UiAction::MinimizeToTray => {
                for ent in &world_q {
                    commands.entity(ent).despawn();
                }
                if let Some(imgs) = images.as_mut() {
                    if let Some(p) = pal.as_mut() {
                        p.purge_transient_images(imgs);
                    }
                    if let Some(ba) = backdrop_assets.as_mut() {
                        ba.purge(imgs);
                    }
                }
                for mut win in &mut windows {
                    win.visible = false;
                }
                if let Some(f) = fps.as_mut() {
                    f.hidden_to_tray = true;
                }
                if let Some(t) = tray.as_mut() {
                    t.in_tray = true;
                    if show_native_tray_toast(
                        "Agents Station running in Tray",
                        "3D world unloaded · HTTP server active on 127.0.0.1:4310",
                    ) {
                        t.notifications_sent = t.notifications_sent.saturating_add(1);
                    }
                }
            }
            UiAction::SwitchLayout(_)
            | UiAction::SetChibiLook(_)
            | UiAction::SetWeatherCity(_)
            | UiAction::SetSound(_)
            | UiAction::ToggleCameraFp
            | UiAction::TriggerPhoto
            | UiAction::ToggleDemo
            | UiAction::ToggleReplay
            | UiAction::SendPrompt { .. }
            | UiAction::PostGroupChat(_)
            | UiAction::SaveWhiteboard(_)
            | UiAction::SetViewMode(ViewMode::Mode3D) => {}
        }
    }
}
