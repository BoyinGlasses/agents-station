pub mod drawers;
pub mod fonts;
pub mod hud;
pub mod i18n;
pub mod launcher_dock;
pub mod lite_view;
pub mod state;
pub mod theme;

use bevy::ecs::schedule::ScheduleLabel;
use bevy::prelude::*;
use bevy_egui::{EguiContext, EguiContextPass, EguiContexts, EguiRenderToImage, egui};
use std::path::PathBuf;
use std::sync::Arc;

pub use i18n::I18nStore;
pub use state::{ActiveDrawer, UiAction, UiState, ViewMode};

#[derive(ScheduleLabel, Clone, Debug, PartialEq, Eq, Hash)]
pub struct CaptureUiPass;

#[derive(Resource)]
struct HudAssetsDir(PathBuf);

#[derive(Resource, Default)]
struct CachedFontBundle(Option<fonts::FontBundle>);

#[derive(Component)]
struct FontsApplied;

/// Minimal P0 smoke HUD plugin (kept for `--capture-scene smoke` backward compatibility).
pub struct SmokeHudPlugin {
    pub assets_dir: PathBuf,
}

impl Plugin for SmokeHudPlugin {
    fn build(&self, app: &mut App) {
        let bundle = fonts::load_font_bundle(&self.assets_dir).ok();
        app.insert_resource(HudAssetsDir(self.assets_dir.clone()))
            .insert_resource(CachedFontBundle(bundle))
            .add_systems(EguiContextPass, render_primary_smoke_hud)
            .add_systems(CaptureUiPass, render_capture_smoke_hud);
    }
}

/// Full native Agents Station HUD, 2D Lite board, Drawers, and Harness Launcher Dock plugin.
pub struct StationUiPlugin {
    pub assets_dir: PathBuf,
    pub initial_state: UiState,
    pub initial_lang: String,
}

impl Plugin for StationUiPlugin {
    fn build(&self, app: &mut App) {
        let bundle = fonts::load_font_bundle(&self.assets_dir).ok();
        let i18n = I18nStore::load_from_assets(&self.assets_dir, &self.initial_lang);
        app.insert_resource(HudAssetsDir(self.assets_dir.clone()))
            .insert_resource(CachedFontBundle(bundle))
            .insert_resource(i18n)
            .insert_resource(self.initial_state.clone())
            .init_resource::<station_3d::UiInputCapture>()
            .configure_sets(
                PostUpdate,
                station_3d::CameraControlSet.after(bevy_egui::EguiPostUpdateSet::EndPass),
            )
            .add_systems(Update, tick_ui_timers)
            .add_systems(EguiContextPass, render_primary_station_ui)
            .add_systems(CaptureUiPass, render_capture_station_ui);
    }
}

fn tick_ui_timers(time: Res<Time>, mut state: ResMut<UiState>) {
    state.tick_toasts(time.delta_secs());
}

fn draw_server_banner(ctx: &egui::Context, state: &mut UiState) {
    let Some(msg) = state.server_banner.clone() else {
        return;
    };
    egui::Area::new(egui::Id::new("station_server_banner"))
        .anchor(egui::Align2::CENTER_TOP, egui::vec2(0.0, 56.0))
        .show(ctx, |ui| {
            egui::Frame::NONE
                .fill(egui::Color32::from_rgba_unmultiplied(64, 22, 22, 242))
                .corner_radius(egui::CornerRadius::same(8))
                .inner_margin(egui::Margin::symmetric(12, 7))
                .stroke(egui::Stroke::new(1.4_f32, theme::STATUS_ERR))
                .show(ui, |ui| {
                    ui.horizontal(|ui| {
                        ui.label(
                            egui::RichText::new("SERVER ERROR")
                                .strong()
                                .size(11.0)
                                .color(theme::STATUS_ERR),
                        );
                        ui.label(
                            egui::RichText::new(&msg)
                                .size(11.5)
                                .color(theme::TEXT_PRIMARY),
                        );
                        if ui.small_button("×").clicked() {
                            state.server_banner = None;
                        }
                    });
                });
        });
}

fn draw_full_station_ui(ctx: &egui::Context, state: &mut UiState, i18n: &I18nStore) {
    match state.view_mode {
        ViewMode::Mode3D => hud::draw_hud(ctx, state, i18n),
        ViewMode::Lite2D | ViewMode::PanelCompact => lite_view::draw_lite_2d_view(ctx, state, i18n),
    }
    draw_server_banner(ctx, state);
    drawers::draw_active_drawer(ctx, state, i18n);
}

#[allow(clippy::too_many_arguments)]
fn render_primary_station_ui(
    mut contexts: EguiContexts,
    mut initialized: Local<bool>,
    mut cached: ResMut<CachedFontBundle>,
    assets_dir: Res<HudAssetsDir>,
    mut state: ResMut<UiState>,
    i18n: Res<I18nStore>,
    mut ui_capture: ResMut<station_3d::UiInputCapture>,
    capture_q: Query<(), With<EguiRenderToImage>>,
) {
    let Some(ctx) = contexts.try_ctx_mut() else {
        return;
    };
    if !*initialized {
        if let Some(bundle) = cached.0.as_ref() {
            install_egui_fonts(ctx, bundle);
        } else if let Ok(bundle) = fonts::load_font_bundle(&assets_dir.0) {
            install_egui_fonts(ctx, &bundle);
        }
        if capture_q.is_empty() {
            cached.0 = None;
        }
        theme::apply_station_theme(ctx);
        *initialized = true;
    }
    draw_full_station_ui(ctx, &mut state, &i18n);
    ui_capture.wants_keyboard = ctx.wants_keyboard_input();
    ui_capture.wants_pointer = ctx.wants_pointer_input();
}

fn render_capture_station_ui(
    mut commands: Commands,
    mut query: Query<(Entity, &mut EguiContext, Has<FontsApplied>), With<EguiRenderToImage>>,
    cached: Res<CachedFontBundle>,
    mut state: ResMut<UiState>,
    i18n: Res<I18nStore>,
) {
    for (entity, mut ctx, applied) in &mut query {
        let egui_ctx = ctx.get_mut();
        if !applied {
            if let Some(bundle) = cached.0.as_ref() {
                install_egui_fonts(egui_ctx, bundle);
            }
            theme::apply_station_theme(egui_ctx);
            commands.entity(entity).insert(FontsApplied);
        }
        draw_full_station_ui(egui_ctx, &mut state, &i18n);
    }
}

pub fn install_egui_fonts(ctx: &egui::Context, bundle: &fonts::FontBundle) {
    let mut defs = egui::FontDefinitions::default();

    let mut prop_names = Vec::new();
    for (idx, bytes) in bundle.ui_regular.iter().enumerate() {
        let name = format!("be_vietnam_pro_400_{idx}");
        defs.font_data
            .insert(name.clone(), Arc::new(egui::FontData::from_owned(bytes.clone())));
        prop_names.push(name);
    }

    let mut mono_names = Vec::new();
    for (idx, bytes) in bundle.mono_regular.iter().enumerate() {
        let name = format!("jetbrains_mono_400_{idx}");
        defs.font_data
            .insert(name.clone(), Arc::new(egui::FontData::from_owned(bytes.clone())));
        mono_names.push(name);
    }

    let thai_name = "plex_thai_looped_400".to_owned();
    defs.font_data.insert(
        thai_name.clone(),
        Arc::new(egui::FontData::from_owned(bundle.thai_regular.clone())),
    );
    prop_names.push(thai_name.clone());
    mono_names.push(thai_name);

    let prop_family = defs.families.entry(egui::FontFamily::Proportional).or_default();
    for name in prop_names.into_iter().rev() {
        prop_family.insert(0, name);
    }

    let mono_family = defs.families.entry(egui::FontFamily::Monospace).or_default();
    for name in mono_names.into_iter().rev() {
        mono_family.insert(0, name);
    }

    ctx.set_fonts(defs);
}

fn draw_smoke_badge(ctx: &egui::Context) {
    egui::Area::new(egui::Id::new("smoke_hud_badge"))
        .fixed_pos(egui::pos2(16.0, 16.0))
        .show(ctx, |ui| {
            egui::Frame::NONE
                .fill(egui::Color32::from_rgba_unmultiplied(18, 20, 28, 235))
                .corner_radius(egui::CornerRadius::same(8))
                .inner_margin(egui::Margin::symmetric(14, 10))
                .stroke(egui::Stroke::new(1.5_f32, egui::Color32::from_rgb(214, 100, 64)))
                .show(ui, |ui| {
                    ui.label(
                        egui::RichText::new("agents-station · Tiếng Việt · ภาษาไทย")
                            .color(egui::Color32::from_rgb(245, 240, 232))
                            .size(16.0),
                    );
                });
        });
}

fn render_primary_smoke_hud(
    mut contexts: EguiContexts,
    mut font_installed: Local<bool>,
    cached: Res<CachedFontBundle>,
    assets_dir: Res<HudAssetsDir>,
) {
    let Some(ctx) = contexts.try_ctx_mut() else {
        return;
    };
    if !*font_installed {
        if let Some(bundle) = cached.0.as_ref() {
            install_egui_fonts(ctx, bundle);
        } else if let Ok(bundle) = fonts::load_font_bundle(&assets_dir.0) {
            install_egui_fonts(ctx, &bundle);
        }
        *font_installed = true;
    }
    draw_smoke_badge(ctx);
}

fn render_capture_smoke_hud(
    mut commands: Commands,
    mut query: Query<(Entity, &mut EguiContext, Has<FontsApplied>), With<EguiRenderToImage>>,
    cached: Res<CachedFontBundle>,
) {
    for (entity, mut ctx, applied) in &mut query {
        let egui_ctx = ctx.get_mut();
        if !applied {
            if let Some(bundle) = cached.0.as_ref() {
                install_egui_fonts(egui_ctx, bundle);
            }
            commands.entity(entity).insert(FontsApplied);
        }
        draw_smoke_badge(egui_ctx);
    }
}

pub fn ui_ready() -> bool {
    true
}

#[cfg(test)]
mod tests {
    use super::*;
    use bevy::input::mouse::{MouseMotion, MouseScrollUnit, MouseWheel};
    use bevy_egui::EguiPostUpdateSet;
    use std::time::Duration;

    #[test]
    fn same_frame_egui_input_capture_blocks_camera_control_without_one_frame_lag() {
        let assets_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../assets");
        let mut app = App::new();
        app.insert_resource(Time::<()>::default())
            .insert_resource(ButtonInput::<KeyCode>::default())
            .insert_resource(ButtonInput::<MouseButton>::default())
            .add_event::<MouseMotion>()
            .add_event::<MouseWheel>()
            .add_plugins(station_3d::OfficeWorldRuntimePlugin {
                assets_dir: assets_dir.clone(),
            })
            .add_plugins(StationUiPlugin {
                assets_dir,
                initial_state: UiState::default(),
                initial_lang: "en".to_owned(),
            });

        // Simulate egui setting UiInputCapture during PostUpdate::EguiPostUpdateSet::EndPass
        // on the EXACT same frame that KeyF + MouseWheel arrive.
        app.add_systems(
            PostUpdate,
            (|mut cap: ResMut<station_3d::UiInputCapture>| {
                cap.wants_keyboard = true;
                cap.wants_pointer = true;
            })
            .in_set(EguiPostUpdateSet::EndPass),
        );

        let spec = station_3d::LayoutSpec::for_layout(station_3d::OfficeLayout::Pro);
        let mut cmds = app.world_mut().commands();
        station_3d::camera::spawn_office_camera(&mut cmds, &spec);
        app.world_mut().flush();

        let before = app.world().resource::<station_3d::camera::CameraRig>().clone();
        app.world_mut()
            .resource_mut::<ButtonInput<KeyCode>>()
            .press(KeyCode::KeyF);
        app.world_mut().send_event(MouseWheel {
            unit: MouseScrollUnit::Line,
            x: 0.0,
            y: 5.0,
            window: Entity::PLACEHOLDER,
        });
        app.world_mut()
            .resource_mut::<Time>()
            .advance_by(Duration::from_millis(16));
        app.update();

        let after = app.world().resource::<station_3d::camera::CameraRig>().clone();
        assert_eq!(
            after.first_person, before.first_person,
            "KeyF must be blocked on the exact same frame EguiPostUpdateSet::EndPass sets wants_keyboard"
        );
        assert_eq!(
            after.viewport_height, before.viewport_height,
            "MouseWheel must be blocked on the exact same frame EguiPostUpdateSet::EndPass sets wants_pointer"
        );
    }
}
