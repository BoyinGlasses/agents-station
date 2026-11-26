pub mod activity_panel;
pub mod session_rail;
pub mod topbar;

use crate::i18n::I18nStore;
use crate::icons::{self, IconKind};
use crate::state::{UiAction, UiState};
use crate::theme::{
    ACCENT, ACCENT_HI, AMBER, BLUE, CARD, CARD_2, GREEN, INK, LINE_2, MUTED, RED, heading_bold,
};
use bevy_egui::egui::{self, Color32, CornerRadius, Margin, Rect, RichText, Sense, Stroke, pos2, vec2};

const ACCENT_SOFT: Color32 = Color32::from_rgba_premultiplied(0x1F, 0x12, 0x0C, 33);
const ACCENT_LINE: Color32 = Color32::from_rgba_premultiplied(0x90, 0x53, 0x39, 153);
const ON_ACCENT: Color32 = Color32::from_rgb(0x2A, 0x13, 0x07);

fn toolbar_bg() -> Color32 {
    Color32::from_rgba_unmultiplied(0x0F, 0x14, 0x1C, 214)
}

fn perm_bg() -> Color32 {
    Color32::from_rgba_unmultiplied(0x1E, 0x1A, 0x16, 240)
}

fn deny_bg() -> Color32 {
    Color32::from_rgba_unmultiplied(0xFF, 0x5F, 0x5F, 45)
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ShellLayout {
    pub show_topbar: bool,
    pub show_session_rail: bool,
    pub show_stage_chips: bool,
    pub show_activity_panel: bool,
    pub show_activity_bottom_sheet: bool,
    pub topbar_height: f32,
    pub left_width: f32,
    pub right_width: f32,
    pub compact_topbar_labels: bool,
}

pub fn compute_shell_layout(
    viewport_width: f32,
    fullscreen: bool,
    photo_mode: bool,
) -> ShellLayout {
    if fullscreen || photo_mode {
        return ShellLayout {
            show_topbar: false,
            show_session_rail: false,
            show_stage_chips: false,
            show_activity_panel: false,
            show_activity_bottom_sheet: false,
            topbar_height: 0.0,
            left_width: 0.0,
            right_width: 0.0,
            compact_topbar_labels: true,
        };
    }

    let compact_topbar_labels = viewport_width <= 1560.0;

    if viewport_width < 720.0 {
        ShellLayout {
            show_topbar: true,
            show_session_rail: false,
            show_stage_chips: true,
            show_activity_panel: false,
            show_activity_bottom_sheet: true,
            topbar_height: 52.0,
            left_width: 0.0,
            right_width: 0.0,
            compact_topbar_labels: true,
        }
    } else if viewport_width < 1100.0 {
        ShellLayout {
            show_topbar: true,
            show_session_rail: false,
            show_stage_chips: true,
            show_activity_panel: true,
            show_activity_bottom_sheet: false,
            topbar_height: 56.0,
            left_width: 0.0,
            right_width: 320.0,
            compact_topbar_labels,
        }
    } else if viewport_width <= 1440.0 {
        ShellLayout {
            show_topbar: true,
            show_session_rail: true,
            show_stage_chips: false,
            show_activity_panel: true,
            show_activity_bottom_sheet: false,
            topbar_height: 56.0,
            left_width: 300.0,
            right_width: 330.0,
            compact_topbar_labels,
        }
    } else {
        ShellLayout {
            show_topbar: true,
            show_session_rail: true,
            show_stage_chips: false,
            show_activity_panel: true,
            show_activity_bottom_sheet: false,
            topbar_height: 56.0,
            left_width: 330.0,
            right_width: 350.0,
            compact_topbar_labels,
        }
    }
}

/// Draws the 3-column control-room shell (`TopBar` + `SessionRail` + transparent `Stage` + `ActivityPanel`)
/// and returns the exact logical `egui::Rect` of the central stage for 3D camera viewport cropping.
pub fn draw_shell(ctx: &egui::Context, state: &mut UiState, i18n: &I18nStore) -> Rect {
    let layout = compute_shell_layout(
        ctx.screen_rect().width(),
        state.fullscreen_stage,
        state.photo_mode,
    );

    if layout.show_topbar {
        topbar::draw_topbar(ctx, state, i18n, &layout);
    }
    if layout.show_session_rail {
        session_rail::draw_session_rail(ctx, state, i18n, &layout);
    }
    if layout.show_activity_panel || layout.show_activity_bottom_sheet {
        activity_panel::draw_activity_panel(ctx, state, i18n, &layout);
    }

    // Central stage stays transparent so Bevy's cropped 3D camera paints directly inside `stage_rect`.
    let stage_rect = ctx.available_rect();

    if layout.show_stage_chips {
        session_rail::draw_stage_session_chips(ctx, stage_rect, state, i18n);
    }

    if !state.photo_mode {
        draw_stage_permission_banners(ctx, stage_rect, state, i18n);
        draw_stage_camera_dock(ctx, stage_rect, state, i18n);
    }
    draw_stage_toasts(ctx, stage_rect, state);

    stage_rect
}

fn draw_stage_permission_banners(
    ctx: &egui::Context,
    stage_rect: Rect,
    state: &mut UiState,
    i18n: &I18nStore,
) {
    if state.pending_perms.is_empty() || stage_rect.width() < 260.0 {
        return;
    }
    let card_w = 290.0_f32.min(stage_rect.width() - 24.0);
    let top_y = stage_rect.top() + if state.sessions.is_empty() { 12.0 } else { 14.0 };
    let left_x = stage_rect.right() - card_w - 12.0;

    egui::Area::new(egui::Id::new("station_stage_perm_banners"))
        .order(egui::Order::Middle)
        .fixed_pos(pos2(left_x, top_y))
        .show(ctx, |ui| {
            ui.set_width(card_w);
            let perms = state.pending_perms.clone();
            for p in perms {
                let border = match p.kind.as_str() {
                    "denied" => RED,
                    "ci" => BLUE,
                    _ => AMBER,
                };
                egui::Frame::NONE
                    .fill(perm_bg())
                    .corner_radius(CornerRadius::same(12))
                    .inner_margin(Margin::symmetric(12, 9))
                    .stroke(Stroke::new(1.3_f32, border))
                    .show(ui, |ui| {
                        ui.horizontal(|ui| {
                            ui.label(
                                heading_bold(
                                    format!("{} · {}", i18n.tr("station.approval"), p.agent),
                                    11.0,
                                )
                                .color(border),
                            );
                            ui.label(RichText::new(&p.tool).size(11.0).color(INK));
                        });
                        let summary = if state.streamer_mode {
                            i18n.tr("station.streamerRedacted")
                        } else {
                            p.summary.clone()
                        };
                        ui.label(RichText::new(summary).size(11.5).color(INK));
                        ui.add_space(5.0);
                        ui.horizontal(|ui| {
                            let allow_btn = egui::Button::new(
                                heading_bold(i18n.tr("perm.allow"), 11.5)
                                    .color(Color32::from_rgb(0x06, 0x15, 0x0C)),
                            )
                            .fill(GREEN)
                            .corner_radius(CornerRadius::same(8));
                            if ui.add(allow_btn).clicked() {
                                state.pending_perms.retain(|x| x.id != p.id);
                                state.push_action(UiAction::DecidePermission {
                                    id: p.id.clone(),
                                    decision: "allow".to_owned(),
                                });
                            }
                            let deny_btn = egui::Button::new(
                                heading_bold(i18n.tr("perm.deny"), 11.5).color(INK),
                            )
                            .fill(deny_bg())
                            .corner_radius(CornerRadius::same(8));
                            if ui.add(deny_btn).clicked() {
                                state.pending_perms.retain(|x| x.id != p.id);
                                state.push_action(UiAction::DecidePermission {
                                    id: p.id.clone(),
                                    decision: "deny".to_owned(),
                                });
                            }
                        });
                    });
                ui.add_space(6.0);
            }
        });
}

/// Compact floating bottom-center stage dock (full camera presets & photo bar arrive in Task 3).
fn draw_stage_camera_dock(
    ctx: &egui::Context,
    stage_rect: Rect,
    state: &mut UiState,
    i18n: &I18nStore,
) {
    if stage_rect.width() < 180.0 || stage_rect.height() < 120.0 {
        return;
    }
    let center_x = stage_rect.center().x;
    let bottom_y = stage_rect.bottom() - 16.0;

    egui::Area::new(egui::Id::new("station_stage_camera_dock"))
        .order(egui::Order::Middle)
        .pivot(egui::Align2::CENTER_BOTTOM)
        .fixed_pos(pos2(center_x, bottom_y))
        .show(ctx, |ui| {
            egui::Frame::NONE
                .fill(toolbar_bg())
                .corner_radius(CornerRadius::same(14))
                .inner_margin(Margin::same(6))
                .stroke(Stroke::new(1.0_f32, LINE_2))
                .show(ui, |ui| {
                    ui.horizontal(|ui| {
                        ui.spacing_mut().item_spacing.x = 5.0;

                        // Camera photo trigger
                        if dock_icon_btn(ui, IconKind::Camera, false)
                            .on_hover_text(i18n.tr("ui.photo.openTitle"))
                            .clicked()
                        {
                            state.push_action(UiAction::TriggerPhoto);
                        }

                        // Fullscreen stage toggle
                        if dock_icon_btn(ui, IconKind::Maximize, state.fullscreen_stage)
                            .on_hover_text(i18n.tr("ui.cameraToolbar.fullscreen"))
                            .clicked()
                        {
                            state.fullscreen_stage = !state.fullscreen_stage;
                        }

                        // Walk-in (First Person) primary coral CTA
                        let fp_fill = if state.first_person {
                            ACCENT_SOFT
                        } else {
                            ACCENT
                        };
                        let fp_fg = if state.first_person {
                            ACCENT_HI
                        } else {
                            ON_ACCENT
                        };
                        let fp_stroke = if state.first_person {
                            Stroke::new(1.0_f32, ACCENT_LINE)
                        } else {
                            Stroke::NONE
                        };
                        let fp_resp = egui::Frame::NONE
                            .fill(fp_fill)
                            .corner_radius(CornerRadius::same(10))
                            .inner_margin(Margin::symmetric(12, 7))
                            .stroke(fp_stroke)
                            .show(ui, |ui| {
                                ui.horizontal(|ui| {
                                    ui.spacing_mut().item_spacing.x = 6.0;
                                    let (ic, _) =
                                        ui.allocate_exact_size(vec2(14.0, 14.0), Sense::hover());
                                    icons::paint_icon(ui.painter(), ic, IconKind::Walk, fp_fg);
                                    ui.label(
                                        heading_bold(i18n.tr("ui.cameraToolbar.walkIn"), 12.0)
                                            .color(fp_fg),
                                    );
                                });
                            });
                        if fp_resp
                            .response
                            .interact(Sense::click())
                            .on_hover_text(i18n.tr("ui.cameraToolbar.walkInTitle"))
                            .clicked()
                        {
                            state.push_action(UiAction::ToggleCameraFp);
                        }
                    });
                });
        });
}

fn dock_icon_btn(ui: &mut egui::Ui, kind: IconKind, active: bool) -> egui::Response {
    let (rect, resp) = ui.allocate_exact_size(vec2(34.0, 34.0), Sense::click());
    let fill = if active {
        ACCENT_SOFT
    } else if resp.hovered() {
        CARD_2
    } else {
        Color32::TRANSPARENT
    };
    ui.painter()
        .rect_filled(rect, CornerRadius::same(8), fill);
    let col = if active {
        ACCENT
    } else if resp.hovered() {
        INK
    } else {
        MUTED
    };
    icons::paint_icon(
        ui.painter(),
        Rect::from_center_size(rect.center(), vec2(15.0, 15.0)),
        kind,
        col,
    );
    resp
}

fn draw_stage_toasts(ctx: &egui::Context, stage_rect: Rect, state: &UiState) {
    if state.toasts.is_empty() || stage_rect.width() < 200.0 {
        return;
    }
    let toast_w = 270.0_f32.min(stage_rect.width() - 24.0);
    egui::Area::new(egui::Id::new("station_stage_toasts"))
        .order(egui::Order::Foreground)
        .pivot(egui::Align2::RIGHT_BOTTOM)
        .fixed_pos(pos2(stage_rect.right() - 12.0, stage_rect.bottom() - 68.0))
        .show(ctx, |ui| {
            ui.set_width(toast_w);
            for t in &state.toasts {
                egui::Frame::NONE
                    .fill(CARD)
                    .corner_radius(CornerRadius::same(10))
                    .inner_margin(Margin::symmetric(11, 8))
                    .stroke(Stroke::new(1.0_f32, ACCENT))
                    .show(ui, |ui| {
                        ui.label(heading_bold(&t.title, 12.0));
                        ui.label(RichText::new(&t.body).size(11.0).color(MUTED));
                    });
                ui.add_space(5.0);
            }
        });
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn wide_desktop_shows_three_column_shell_with_exact_widths() {
        let l = compute_shell_layout(1600.0, false, false);
        assert!(l.show_topbar);
        assert!(l.show_session_rail);
        assert!(!l.show_stage_chips);
        assert!(l.show_activity_panel);
        assert!(!l.show_activity_bottom_sheet);
        assert_eq!(l.topbar_height, 56.0);
        assert_eq!(l.left_width, 330.0);
        assert_eq!(l.right_width, 350.0);
        assert!(!l.compact_topbar_labels);
    }

    #[test]
    fn medium_desktop_1280_tightens_sidebars_and_hides_topbar_button_labels() {
        let l = compute_shell_layout(1280.0, false, false);
        assert!(l.show_topbar);
        assert!(l.show_session_rail);
        assert!(!l.show_stage_chips);
        assert!(l.show_activity_panel);
        assert_eq!(l.topbar_height, 56.0);
        assert_eq!(l.left_width, 300.0);
        assert_eq!(l.right_width, 330.0);
        assert!(l.compact_topbar_labels);
    }

    #[test]
    fn below_1100px_collapses_session_rail_to_stage_chips() {
        let l = compute_shell_layout(1050.0, false, false);
        assert!(l.show_topbar);
        assert!(!l.show_session_rail);
        assert!(l.show_stage_chips);
        assert!(l.show_activity_panel);
        assert!(!l.show_activity_bottom_sheet);
        assert_eq!(l.left_width, 0.0);
        assert_eq!(l.right_width, 320.0);
    }

    #[test]
    fn below_720px_collapses_activity_panel_to_bottom_sheet() {
        let l = compute_shell_layout(640.0, false, false);
        assert!(l.show_topbar);
        assert_eq!(l.topbar_height, 52.0);
        assert!(!l.show_session_rail);
        assert!(l.show_stage_chips);
        assert!(!l.show_activity_panel);
        assert!(l.show_activity_bottom_sheet);
        assert_eq!(l.left_width, 0.0);
        assert_eq!(l.right_width, 0.0);
    }

    #[test]
    fn fullscreen_and_photo_mode_hide_all_chrome() {
        for (fs, photo) in [(true, false), (false, true), (true, true)] {
            let l = compute_shell_layout(1600.0, fs, photo);
            assert!(!l.show_topbar);
            assert!(!l.show_session_rail);
            assert!(!l.show_stage_chips);
            assert!(!l.show_activity_panel);
            assert!(!l.show_activity_bottom_sheet);
            assert_eq!(l.topbar_height, 0.0);
            assert_eq!(l.left_width, 0.0);
            assert_eq!(l.right_width, 0.0);
        }
    }

    #[test]
    fn draw_shell_returns_cropped_central_stage_rect_between_sidebars_and_below_topbar() {
        let assets = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../assets");
        let bundle = crate::fonts::load_font_bundle(&assets).expect("load_font_bundle");
        let i18n = I18nStore::load_from_assets(&assets, "vi");
        let mut state = UiState::with_demo_seed(
            station_3d::OfficeLayout::Pro,
            station_3d::BackdropKind::City,
            false,
        );
        let ctx = egui::Context::default();
        crate::install_egui_fonts(&ctx, &bundle);
        let raw = egui::RawInput {
            screen_rect: Some(Rect::from_min_size(pos2(0.0, 0.0), vec2(1280.0, 720.0))),
            ..Default::default()
        };
        let mut stage = Rect::NOTHING;
        let _ = ctx.run(raw, |ctx| {
            stage = draw_shell(ctx, &mut state, &i18n);
        });
        assert!(
            (stage.top() - 56.0).abs() <= 1.0,
            "stage top should be 56px, got {}",
            stage.top()
        );
        assert!(
            (stage.left() - 300.0).abs() <= 1.0,
            "stage left should be 300px, got {}",
            stage.left()
        );
        assert!(
            (stage.right() - 950.0).abs() <= 1.0,
            "stage right should be 950px, got {}",
            stage.right()
        );
        assert!(
            (stage.bottom() - 720.0).abs() <= 1.0,
            "stage bottom should be 720px, got {}",
            stage.bottom()
        );
    }
}
