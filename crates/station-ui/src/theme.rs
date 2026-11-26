use bevy_egui::egui::{self, Color32, CornerRadius, FontFamily, FontId, RichText, Stroke, Visuals};

// Exact design tokens from `my-station/src/styles/base.css:6-15`.
pub const BG: Color32 = Color32::from_rgb(0x0D, 0x12, 0x19);
pub const BAR: Color32 = Color32::from_rgb(0x11, 0x17, 0x20);
pub const SIDE: Color32 = Color32::from_rgb(0x13, 0x19, 0x22);
pub const CARD: Color32 = Color32::from_rgb(0x18, 0x20, 0x2B);
pub const CARD_2: Color32 = Color32::from_rgb(0x1E, 0x27, 0x34);
pub const CARD_3: Color32 = Color32::from_rgb(0x26, 0x30, 0x40);
/// `rgba(255, 255, 255, 0.07)` (sRGB premultiplied equivalent of `from_rgba_unmultiplied(255, 255, 255, 18)`).
pub const LINE: Color32 = Color32::from_rgba_premultiplied(0x4B, 0x4B, 0x4B, 18);
/// `rgba(255, 255, 255, 0.13)` (sRGB premultiplied equivalent of `from_rgba_unmultiplied(255, 255, 255, 33)`).
pub const LINE_2: Color32 = Color32::from_rgba_premultiplied(0x65, 0x65, 0x65, 33);
pub const INK: Color32 = Color32::from_rgb(0xE8, 0xEC, 0xF4);
pub const MUTED: Color32 = Color32::from_rgb(0x8D, 0x97, 0xAA);
pub const FAINT: Color32 = Color32::from_rgb(0x5E, 0x68, 0x7B);
pub const GREEN: Color32 = Color32::from_rgb(0x3D, 0xDC, 0x84);
pub const AMBER: Color32 = Color32::from_rgb(0xF2, 0xB5, 0x44);
pub const ACCENT: Color32 = Color32::from_rgb(0xEF, 0x8A, 0x5F);
pub const ACCENT_HI: Color32 = Color32::from_rgb(0xF5, 0xA0, 0x7A);
pub const RED: Color32 = Color32::from_rgb(0xFF, 0x5F, 0x5F);
pub const BLUE: Color32 = Color32::from_rgb(0x6A, 0xAE, 0xFF);
pub const LOGO: Color32 = Color32::from_rgb(0xF0, 0x8A, 0x3E);

// Named bold font family registered in `install_egui_fonts` (`BeVietnamPro-700` + Thai bold).
pub const FONT_BOLD: &str = "bold";

// Compatibility aliases for existing HUD/drawer/lite callers while migrating to exact tokens.
pub const BG_DEEP: Color32 = BG;
pub const BG_PANEL: Color32 = BAR;
pub const BG_CARD: Color32 = CARD;
pub const BG_CARD_HOVER: Color32 = CARD_2;
pub const BORDER_SUBTLE: Color32 = LINE_2;
pub const ACCENT_TERRACOTTA: Color32 = ACCENT;
pub const STATUS_WORK: Color32 = GREEN;
pub const STATUS_WAIT: Color32 = AMBER;
pub const STATUS_ERR: Color32 = RED;
pub const STATUS_INFO: Color32 = BLUE;
pub const TEXT_PRIMARY: Color32 = INK;
pub const TEXT_MUTED: Color32 = MUTED;

/// Bold 700-weight heading/badge text using the registered `"bold"` font family.
pub fn heading_bold(text: impl Into<String>, size: f32) -> RichText {
    RichText::new(text)
        .font(FontId::new(size, FontFamily::Name(FONT_BOLD.into())))
        .color(INK)
}

/// Applies the Agents Station dark control-room design tokens matching `src/styles/base.css`.
pub fn apply_station_theme(ctx: &egui::Context) {
    let mut style = (*ctx.style()).clone();
    let mut v = Visuals::dark();
    v.panel_fill = SIDE;
    v.window_fill = BAR;
    v.extreme_bg_color = BG;
    v.faint_bg_color = CARD;
    v.window_corner_radius = CornerRadius::same(12);
    v.window_stroke = Stroke::new(1.0_f32, LINE_2);
    v.widgets.noninteractive.bg_fill = CARD;
    v.widgets.noninteractive.fg_stroke = Stroke::new(1.0_f32, INK);
    v.widgets.inactive.bg_fill = CARD;
    v.widgets.inactive.weak_bg_fill = CARD;
    v.widgets.inactive.corner_radius = CornerRadius::same(8);
    v.widgets.inactive.bg_stroke = Stroke::new(1.0_f32, LINE);
    v.widgets.hovered.bg_fill = CARD_2;
    v.widgets.hovered.bg_stroke = Stroke::new(1.0_f32, ACCENT);
    v.widgets.hovered.corner_radius = CornerRadius::same(8);
    v.widgets.active.bg_fill = CARD_3;
    v.widgets.active.bg_stroke = Stroke::new(1.0_f32, ACCENT_HI);
    v.widgets.active.corner_radius = CornerRadius::same(8);
    v.selection.bg_fill = Color32::from_rgba_unmultiplied(0xEF, 0x8A, 0x5F, 33);
    v.selection.stroke = Stroke::new(1.0_f32, ACCENT);

    style.visuals = v;
    style.spacing.item_spacing = egui::vec2(8.0, 6.0);
    style.spacing.button_padding = egui::vec2(10.0, 5.0);
    ctx.set_style(style);
}

pub fn glass_frame() -> egui::Frame {
    egui::Frame::NONE
        .fill(Color32::from_rgba_unmultiplied(0x11, 0x17, 0x20, 238))
        .corner_radius(CornerRadius::same(11))
        .inner_margin(egui::Margin::symmetric(12, 8))
        .stroke(Stroke::new(1.0_f32, LINE_2))
}

pub fn card_frame() -> egui::Frame {
    egui::Frame::NONE
        .fill(CARD)
        .corner_radius(CornerRadius::same(9))
        .inner_margin(egui::Margin::symmetric(10, 8))
        .stroke(Stroke::new(1.0_f32, LINE))
}

/// Crisp vector status indicator circle (avoids missing Unicode glyph tofu in subset fonts).
pub fn status_dot(ui: &mut egui::Ui, color: Color32, radius: f32) {
    let size = egui::vec2(radius * 2.0 + 2.0, radius * 2.0 + 2.0);
    let (rect, _) = ui.allocate_exact_size(size, egui::Sense::hover());
    ui.painter().circle_filled(rect.center(), radius, color);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn base_css_palette_tokens_match_exact_hex_and_alpha_values() {
        assert_eq!(BG, Color32::from_rgb(0x0D, 0x12, 0x19));
        assert_eq!(BAR, Color32::from_rgb(0x11, 0x17, 0x20));
        assert_eq!(SIDE, Color32::from_rgb(0x13, 0x19, 0x22));
        assert_eq!(CARD, Color32::from_rgb(0x18, 0x20, 0x2B));
        assert_eq!(CARD_2, Color32::from_rgb(0x1E, 0x27, 0x34));
        assert_eq!(CARD_3, Color32::from_rgb(0x26, 0x30, 0x40));
        assert_eq!(LINE, Color32::from_rgba_unmultiplied(255, 255, 255, 18));
        assert_eq!(LINE_2, Color32::from_rgba_unmultiplied(255, 255, 255, 33));
        assert_eq!(INK, Color32::from_rgb(0xE8, 0xEC, 0xF4));
        assert_eq!(MUTED, Color32::from_rgb(0x8D, 0x97, 0xAA));
        assert_eq!(FAINT, Color32::from_rgb(0x5E, 0x68, 0x7B));
        assert_eq!(GREEN, Color32::from_rgb(0x3D, 0xDC, 0x84));
        assert_eq!(AMBER, Color32::from_rgb(0xF2, 0xB5, 0x44));
        assert_eq!(ACCENT, Color32::from_rgb(0xEF, 0x8A, 0x5F));
        assert_eq!(ACCENT_HI, Color32::from_rgb(0xF5, 0xA0, 0x7A));
        assert_eq!(RED, Color32::from_rgb(0xFF, 0x5F, 0x5F));
        assert_eq!(BLUE, Color32::from_rgb(0x6A, 0xAE, 0xFF));
        assert_eq!(LOGO, Color32::from_rgb(0xF0, 0x8A, 0x3E));
    }

    #[test]
    fn heading_bold_uses_named_bold_font_family() {
        let ctx = egui::Context::default();
        let mut defs = egui::FontDefinitions::default();
        let fallback = defs
            .families
            .get(&FontFamily::Proportional)
            .cloned()
            .unwrap_or_default();
        defs.families
            .insert(FontFamily::Name(FONT_BOLD.into()), fallback);
        ctx.set_fonts(defs);

        let mut captured_family = None;
        let _ = ctx.run(egui::RawInput::default(), |ctx| {
            egui::CentralPanel::default().show(ctx, |ui| {
                let rich = heading_bold("Agents Station", 15.0);
                let galley = egui::WidgetText::from(rich).into_galley(
                    ui,
                    None,
                    f32::INFINITY,
                    egui::FontSelection::Default,
                );
                captured_family = galley
                    .job
                    .sections
                    .first()
                    .map(|s| (s.format.font_id.family.clone(), s.format.font_id.size));
            });
        });
        assert_eq!(
            captured_family,
            Some((egui::FontFamily::Name("bold".into()), 15.0))
        );
    }
}
