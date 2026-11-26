pub mod activity_panel;
pub mod camera_toolbar;
pub mod session_rail;
pub mod stage_overlays;
pub mod topbar;

use crate::i18n::I18nStore;
use crate::labels3d::{self, LabelClick};
use crate::state::{ActiveDrawer, UiAction, UiState};
use crate::theme::{ACCENT, CARD, MUTED, heading_bold};
use bevy_egui::egui::{self, CornerRadius, Margin, Rect, RichText, Stroke, pos2};

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

    // 3D projected labels render strictly clipped inside `stage_rect` underneath floating HUD bars.
    let (label_clicks, hovered_id) = labels3d::draw_projected_labels(
        ctx,
        stage_rect,
        &state.projected_labels,
        state.photo_mode,
        state.photo_show_names,
        state.show_zone_chips && !state.first_person,
    );
    state.hovered_label_id = hovered_id;
    for click in label_clicks {
        match click {
            LabelClick::Session(sid) => {
                state.selected_session = Some(sid);
                state.active_drawer = ActiveDrawer::Session;
            }
            LabelClick::Place(pid) => {
                state.push_action(UiAction::OpenPlaceCard(pid));
            }
        }
    }

    if layout.show_stage_chips {
        session_rail::draw_stage_session_chips(ctx, stage_rect, state, i18n);
    }

    stage_overlays::draw_stage_overlays(ctx, stage_rect, state, i18n);
    camera_toolbar::draw_camera_toolbar(ctx, stage_rect, state, i18n);
    draw_stage_toasts(ctx, stage_rect, state);

    stage_rect
}

fn draw_stage_toasts(ctx: &egui::Context, stage_rect: Rect, state: &UiState) {
    if state.toasts.is_empty() || stage_rect.width() < 200.0 {
        return;
    }
    let toast_w = 270.0_f32.min(stage_rect.width() - 24.0);
    let bottom_offset = if state.photo_mode {
        118.0
    } else if state.replay_active {
        208.0
    } else {
        68.0
    };
    egui::Area::new(egui::Id::new("station_stage_toasts"))
        .order(egui::Order::Foreground)
        .pivot(egui::Align2::RIGHT_BOTTOM)
        .fixed_pos(pos2(
            stage_rect.right() - 12.0,
            stage_rect.bottom() - bottom_offset,
        ))
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
    use bevy_egui::egui::vec2;

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
        let _ = ctx.run(raw.clone(), |_| {});
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
