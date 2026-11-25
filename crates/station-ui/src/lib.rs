pub mod fonts;

use bevy::prelude::*;
use bevy_egui::{EguiContextPass, EguiContexts, egui};
use std::path::PathBuf;
use std::sync::Arc;

#[derive(Resource)]
struct HudAssetsDir(PathBuf);

#[derive(Resource, Default)]
struct FontsInstalled(bool);

pub struct SmokeHudPlugin {
    pub assets_dir: PathBuf,
}

impl Plugin for SmokeHudPlugin {
    fn build(&self, app: &mut App) {
        app.insert_resource(HudAssetsDir(self.assets_dir.clone()))
            .init_resource::<FontsInstalled>()
            .add_systems(EguiContextPass, render_smoke_hud);
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

fn render_smoke_hud(
    mut contexts: EguiContexts,
    assets_dir: Res<HudAssetsDir>,
    mut installed: ResMut<FontsInstalled>,
) {
    let Some(ctx) = contexts.try_ctx_mut() else {
        return;
    };

    if !installed.0 {
        if let Ok(bundle) = fonts::load_font_bundle(&assets_dir.0) {
            install_egui_fonts(ctx, &bundle);
        }
        installed.0 = true;
    }

    egui::Area::new(egui::Id::new("smoke_hud_badge"))
        .fixed_pos(egui::pos2(16.0, 16.0))
        .show(ctx, |ui| {
            egui::Frame::NONE
                .fill(egui::Color32::from_rgba_unmultiplied(18, 20, 28, 225))
                .corner_radius(egui::CornerRadius::same(8))
                .inner_margin(egui::Margin::symmetric(14, 10))
                .stroke(egui::Stroke::new(1.0_f32, egui::Color32::from_rgb(214, 100, 64)))
                .show(ui, |ui| {
                    ui.label(
                        egui::RichText::new("agents-station · Tiếng Việt · ภาษาไทย")
                            .color(egui::Color32::from_rgb(245, 240, 232))
                            .size(16.0),
                    );
                });
        });
}

pub fn ui_ready() -> bool {
    true
}
