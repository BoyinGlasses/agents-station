use bevy_egui::egui::{self, Color32, CornerRadius, Stroke, Visuals};

pub const BG_DEEP: Color32 = Color32::from_rgb(16, 19, 27);
pub const BG_PANEL: Color32 = Color32::from_rgba_premultiplied(24, 28, 39, 240);
pub const BG_CARD: Color32 = Color32::from_rgb(31, 36, 49);
pub const BG_CARD_HOVER: Color32 = Color32::from_rgb(40, 46, 62);
pub const BORDER_SUBTLE: Color32 = Color32::from_rgb(54, 62, 82);
pub const ACCENT_TERRACOTTA: Color32 = Color32::from_rgb(214, 100, 64);
pub const STATUS_WORK: Color32 = Color32::from_rgb(61, 220, 132);
pub const STATUS_WAIT: Color32 = Color32::from_rgb(255, 178, 63);
pub const STATUS_ERR: Color32 = Color32::from_rgb(255, 95, 95);
pub const STATUS_INFO: Color32 = Color32::from_rgb(106, 174, 255);
pub const TEXT_PRIMARY: Color32 = Color32::from_rgb(245, 240, 232);
pub const TEXT_MUTED: Color32 = Color32::from_rgb(154, 163, 178);

/// Applies the Agents Station warm-slate + terracotta design tokens matching `src/styles/*.css`.
pub fn apply_station_theme(ctx: &egui::Context) {
    let mut style = (*ctx.style()).clone();
    let mut v = Visuals::dark();
    v.panel_fill = BG_PANEL;
    v.window_fill = BG_PANEL;
    v.extreme_bg_color = BG_DEEP;
    v.faint_bg_color = BG_CARD;
    v.window_corner_radius = CornerRadius::same(12);
    v.window_stroke = Stroke::new(1.0_f32, BORDER_SUBTLE);
    v.widgets.noninteractive.bg_fill = BG_CARD;
    v.widgets.noninteractive.fg_stroke = Stroke::new(1.0_f32, TEXT_PRIMARY);
    v.widgets.inactive.bg_fill = BG_CARD;
    v.widgets.inactive.weak_bg_fill = BG_CARD;
    v.widgets.inactive.corner_radius = CornerRadius::same(8);
    v.widgets.inactive.bg_stroke = Stroke::new(1.0_f32, BORDER_SUBTLE);
    v.widgets.hovered.bg_fill = BG_CARD_HOVER;
    v.widgets.hovered.bg_stroke = Stroke::new(1.0_f32, ACCENT_TERRACOTTA);
    v.widgets.hovered.corner_radius = CornerRadius::same(8);
    v.widgets.active.bg_fill = ACCENT_TERRACOTTA;
    v.widgets.active.corner_radius = CornerRadius::same(8);
    v.selection.bg_fill = Color32::from_rgba_unmultiplied(214, 100, 64, 90);
    v.selection.stroke = Stroke::new(1.0_f32, ACCENT_TERRACOTTA);

    style.visuals = v;
    style.spacing.item_spacing = egui::vec2(8.0, 6.0);
    style.spacing.button_padding = egui::vec2(10.0, 5.0);
    ctx.set_style(style);
}

pub fn glass_frame() -> egui::Frame {
    egui::Frame::NONE
        .fill(Color32::from_rgba_unmultiplied(20, 24, 34, 232))
        .corner_radius(CornerRadius::same(11))
        .inner_margin(egui::Margin::symmetric(12, 8))
        .stroke(Stroke::new(1.0_f32, BORDER_SUBTLE))
}

pub fn card_frame() -> egui::Frame {
    egui::Frame::NONE
        .fill(BG_CARD)
        .corner_radius(CornerRadius::same(9))
        .inner_margin(egui::Margin::symmetric(10, 8))
        .stroke(Stroke::new(1.0_f32, BORDER_SUBTLE))
}
