use crate::i18n::I18nStore;
use crate::icons::{self, IconKind};
use crate::state::{UiAction, UiState, ViewMode};
use crate::theme::{ACCENT, ACCENT_HI, CARD_3, INK, LINE_2, MUTED};
use bevy_egui::egui::{
    self, Color32, CornerRadius, FontFamily, FontId, Margin, Rect, Sense, Stroke, StrokeKind, pos2,
    vec2,
};
use station_3d::CameraPreset;

const ACCENT_SOFT: Color32 = Color32::from_rgba_premultiplied(0x1F, 0x12, 0x0C, 33);
const ACCENT_LINE: Color32 = Color32::from_rgba_premultiplied(0x90, 0x53, 0x39, 153);
const ON_ACCENT: Color32 = Color32::from_rgb(0x2A, 0x13, 0x07);

const PRESETS: [(CameraPreset, IconKind, &str); 4] = [
    (CameraPreset::Home, IconKind::Home, "ui.cameraToolbar.home"),
    (CameraPreset::Desks, IconKind::Monitor, "ui.cameraToolbar.desks"),
    (CameraPreset::Meeting, IconKind::Users, "ui.cameraToolbar.meeting"),
    (CameraPreset::Server, IconKind::Server, "ui.cameraToolbar.server"),
];

fn toolbar_bg() -> Color32 {
    Color32::from_rgba_unmultiplied(0x0F, 0x14, 0x1C, 214)
}

fn track_bg() -> Color32 {
    Color32::from_rgba_unmultiplied(255, 255, 255, 12)
}

fn hover_bg() -> Color32 {
    Color32::from_rgba_unmultiplied(255, 255, 255, 18)
}

fn dim_color(color: Color32, dimmed: bool) -> Color32 {
    if !dimmed {
        return color;
    }
    Color32::from_rgba_unmultiplied(
        color.r(),
        color.g(),
        color.b(),
        ((color.a() as f32) * 0.42).round() as u8,
    )
}

/// Renders the centered floating `CameraToolbar` 16px above stage bottom matching
/// `my-station/src/styles/stage.css:11-35` and `CameraToolbar.tsx`.
pub fn draw_camera_toolbar(
    ctx: &egui::Context,
    stage_rect: Rect,
    state: &mut UiState,
    i18n: &I18nStore,
) {
    if state.photo_mode || stage_rect.width() < 220.0 || stage_rect.height() < 120.0 {
        return;
    }

    let compact = stage_rect.width() < 600.0;
    let dimmed = state.first_person;
    let center_x = stage_rect.center().x;
    let bottom_y = stage_rect.bottom() - 16.0;

    egui::Area::new(egui::Id::new("station_stage_camera_toolbar"))
        .order(egui::Order::Middle)
        .pivot(egui::Align2::CENTER_BOTTOM)
        .fixed_pos(pos2(center_x, bottom_y))
        .show(ctx, |ui| {
            ui.set_max_width((stage_rect.width() - 24.0).max(180.0));
            egui::Frame::NONE
                .fill(toolbar_bg())
                .corner_radius(CornerRadius::same(15))
                .inner_margin(Margin::same(6))
                .stroke(Stroke::new(1.0_f32, LINE_2))
                .show(ui, |ui| {
                    ui.horizontal(|ui| {
                        ui.spacing_mut().item_spacing.x = 6.0;

                        // 1. Camera presets track (`[Home | Desks | Meeting | Server]`)
                        draw_track_frame(ui, |ui| {
                            ui.spacing_mut().item_spacing.x = 2.0;
                            for (preset, icon, key) in PRESETS {
                                let label = i18n.tr(key);
                                let active = !state.first_person && state.camera_preset == preset;
                                if preset_button(ui, preset, icon, &label, active, compact, dimmed)
                                    .on_hover_text(&label)
                                    .clicked()
                                {
                                    state.camera_preset = preset;
                                    state.first_person = false;
                                    state.push_action(UiAction::SelectCameraPreset(preset));
                                }
                            }
                        });

                        // 2. Zoom controls track (`[- 100% +]`)
                        if stage_rect.width() >= 420.0 {
                            draw_track_frame(ui, |ui| {
                                ui.spacing_mut().item_spacing.x = 2.0;
                                if tool_icon_btn(ui, IconKind::Minus, false, dimmed)
                                    .on_hover_text(i18n.tr("ui.cameraToolbar.zoomOut"))
                                    .clicked()
                                {
                                    state.push_action(UiAction::ZoomCamera(1.0 / 1.2));
                                }

                                let zoom_text = format!("{}%", state.camera_zoom_pct);
                                let (z_rect, z_resp) =
                                    ui.allocate_exact_size(vec2(42.0, 34.0), Sense::click());
                                if z_resp.hovered() {
                                    ui.painter().rect_filled(
                                        z_rect,
                                        CornerRadius::same(8),
                                        hover_bg(),
                                    );
                                }
                                let z_galley = ui.fonts(|f| {
                                    f.layout_no_wrap(
                                        zoom_text,
                                        FontId::new(11.5, FontFamily::Monospace),
                                        dim_color(INK, dimmed),
                                    )
                                });
                                let z_pos = pos2(
                                    z_rect.center().x - z_galley.size().x * 0.5,
                                    z_rect.center().y - z_galley.size().y * 0.5,
                                );
                                ui.painter()
                                    .galley(z_pos, z_galley, dim_color(INK, dimmed));
                                if z_resp
                                    .on_hover_text(i18n.tr("ui.cameraToolbar.home"))
                                    .clicked()
                                {
                                    state.camera_zoom_pct = 100;
                                    state.push_action(UiAction::ResetCameraZoom);
                                }

                                if tool_icon_btn(ui, IconKind::Plus, false, dimmed)
                                    .on_hover_text(i18n.tr("ui.cameraToolbar.zoomIn"))
                                    .clicked()
                                {
                                    state.push_action(UiAction::ZoomCamera(1.2));
                                }
                            });
                        }

                        // 3. Stage tools track (`[Fullscreen | Photo | List]`)
                        draw_track_frame(ui, |ui| {
                            ui.spacing_mut().item_spacing.x = 2.0;
                            if tool_icon_btn(ui, IconKind::Maximize, state.fullscreen_stage, false)
                                .on_hover_text(i18n.tr("ui.cameraToolbar.fullscreen"))
                                .clicked()
                            {
                                state.fullscreen_stage = !state.fullscreen_stage;
                                state.push_action(UiAction::ToggleFullscreenStage);
                            }
                            if tool_icon_btn(ui, IconKind::Camera, state.photo_mode, false)
                                .on_hover_text(i18n.tr("ui.photo.openTitle"))
                                .clicked()
                            {
                                state.photo_mode = true;
                                state.push_action(UiAction::SetPhotoMode(true));
                            }
                            if tool_icon_btn(
                                ui,
                                IconKind::List,
                                state.view_mode != ViewMode::Mode3D,
                                false,
                            )
                            .on_hover_text(i18n.tr("ui.cameraToolbar.listTitle"))
                            .clicked()
                            {
                                state.view_mode = ViewMode::Lite2D;
                                state.push_action(UiAction::SetViewMode(ViewMode::Lite2D));
                            }
                        });

                        // 4. Primary solid coral `#EF8A5F` `[Walk in]` button (`tb-fp`)
                        draw_walk_in_button(ui, state, i18n, compact);
                    });
                });
        });
}

fn draw_track_frame(ui: &mut egui::Ui, add_contents: impl FnOnce(&mut egui::Ui)) {
    egui::Frame::NONE
        .fill(track_bg())
        .corner_radius(CornerRadius::same(11))
        .inner_margin(Margin::same(3))
        .show(ui, |ui| {
            ui.horizontal(|ui| {
                add_contents(ui);
            });
        });
}

fn preset_button(
    ui: &mut egui::Ui,
    preset: CameraPreset,
    icon: IconKind,
    label: &str,
    active: bool,
    compact: bool,
    dimmed: bool,
) -> egui::Response {
    // Track hover across frames so inactive presets expand smoothly to reveal their name
    // matching `.seg-btn:hover > span:last-child` in `stage.css:17-19`.
    let hover_mem_id = ui.make_persistent_id(("cam_tb_preset_hover", preset));
    let was_hovered = ui.ctx().data(|d| d.get_temp::<bool>(hover_mem_id).unwrap_or(false));
    let show_label = !compact && (active || was_hovered);

    let font_id = FontId::new(12.0, FontFamily::Name(crate::theme::FONT_BOLD.into()));
    let galley = if show_label {
        Some(ui.fonts(|f| f.layout_no_wrap(label.to_owned(), font_id, dim_color(INK, dimmed))))
    } else {
        None
    };

    let width = if let Some(g) = &galley {
        10.0 + 15.0 + 7.0 + g.size().x + 12.0
    } else {
        34.0
    };
    let (rect, resp) = ui.allocate_exact_size(vec2(width, 34.0), Sense::click());
    let hovered = resp.hovered();
    ui.ctx().data_mut(|d| d.insert_temp(hover_mem_id, hovered));

    let fill = if active {
        CARD_3
    } else if hovered {
        hover_bg()
    } else {
        Color32::TRANSPARENT
    };
    let cr = CornerRadius::same(8);
    if fill != Color32::TRANSPARENT {
        ui.painter().rect_filled(rect, cr, fill);
    }
    if active {
        ui.painter().rect_stroke(
            rect,
            cr,
            Stroke::new(1.0_f32, Color32::from_rgba_unmultiplied(255, 255, 255, 22)),
            StrokeKind::Inside,
        );
    }

    let icon_color = dim_color(
        if active {
            ACCENT
        } else if hovered {
            INK
        } else {
            MUTED
        },
        dimmed,
    );
    let icon_center = if galley.is_some() {
        pos2(rect.left() + 17.5, rect.center().y)
    } else {
        rect.center()
    };
    icons::paint_icon(
        ui.painter(),
        Rect::from_center_size(icon_center, vec2(15.0, 15.0)),
        icon,
        icon_color,
    );

    if let Some(g) = galley {
        let tx = rect.left() + 32.0;
        let ty = rect.center().y - g.size().y * 0.5;
        ui.painter().galley(pos2(tx, ty), g, dim_color(INK, dimmed));
    }

    resp
}

fn tool_icon_btn(ui: &mut egui::Ui, kind: IconKind, active: bool, dimmed: bool) -> egui::Response {
    let (rect, resp) = ui.allocate_exact_size(vec2(34.0, 34.0), Sense::click());
    let hovered = resp.hovered();
    let fill = if active {
        CARD_3
    } else if hovered {
        hover_bg()
    } else {
        Color32::TRANSPARENT
    };
    if fill != Color32::TRANSPARENT {
        ui.painter().rect_filled(rect, CornerRadius::same(8), fill);
    }
    let col = dim_color(
        if active {
            ACCENT
        } else if hovered {
            INK
        } else {
            MUTED
        },
        dimmed,
    );
    icons::paint_icon(
        ui.painter(),
        Rect::from_center_size(rect.center(), vec2(15.0, 15.0)),
        kind,
        col,
    );
    resp
}

fn draw_walk_in_button(
    ui: &mut egui::Ui,
    state: &mut UiState,
    i18n: &I18nStore,
    compact: bool,
) {
    let label = i18n.tr("ui.cameraToolbar.walkIn");
    let (fill, fg, stroke) = if state.first_person {
        (ACCENT_SOFT, ACCENT_HI, Stroke::new(1.0_f32, ACCENT_LINE))
    } else {
        (ACCENT, ON_ACCENT, Stroke::NONE)
    };
    let font_id = FontId::new(12.2, FontFamily::Name(crate::theme::FONT_BOLD.into()));
    let galley = if compact {
        None
    } else {
        Some(ui.fonts(|f| f.layout_no_wrap(label, font_id, fg)))
    };

    let width = if let Some(g) = &galley {
        13.0 + 15.0 + 8.0 + g.size().x + 15.0
    } else {
        40.0
    };
    let (rect, resp) = ui.allocate_exact_size(vec2(width, 40.0), Sense::click());
    let hovered = resp.hovered();
    let final_fill = if !state.first_person && hovered {
        ACCENT_HI
    } else {
        fill
    };
    let cr = CornerRadius::same(11);
    ui.painter().rect_filled(rect, cr, final_fill);
    if stroke != Stroke::NONE {
        ui.painter().rect_stroke(rect, cr, stroke, StrokeKind::Inside);
    }

    let icon_center = if galley.is_some() {
        pos2(rect.left() + 20.5, rect.center().y)
    } else {
        rect.center()
    };
    icons::paint_icon(
        ui.painter(),
        Rect::from_center_size(icon_center, vec2(15.0, 15.0)),
        IconKind::Walk,
        fg,
    );

    if let Some(g) = galley {
        let tx = rect.left() + 36.0;
        let ty = rect.center().y - g.size().y * 0.5;
        ui.painter().galley(pos2(tx, ty), g, fg);
    }

    if resp
        .on_hover_text(i18n.tr("ui.cameraToolbar.walkInTitle"))
        .clicked()
    {
        state.push_action(UiAction::ToggleCameraFp);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    #[test]
    fn camera_toolbar_renders_all_controls_in_en_vi_th_without_raw_i18n_keys() {
        let assets = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../assets");
        let bundle = crate::fonts::load_font_bundle(&assets).expect("load_font_bundle");
        let stage = Rect::from_min_size(pos2(300.0, 56.0), vec2(650.0, 664.0));

        for lang in ["en", "vi", "th"] {
            let i18n = I18nStore::load_from_assets(&assets, lang);
            for (_, _, key) in PRESETS {
                let val = i18n.tr(key);
                assert!(!val.is_empty() && val != key, "lang={lang} raw key {key}");
            }
            for key in [
                "ui.cameraToolbar.zoomOut",
                "ui.cameraToolbar.zoomIn",
                "ui.cameraToolbar.fullscreen",
                "ui.cameraToolbar.listTitle",
                "ui.cameraToolbar.walkIn",
                "ui.photo.openTitle",
            ] {
                let val = i18n.tr(key);
                assert!(!val.is_empty() && val != key, "lang={lang} raw key {key}");
            }

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
            // Frame 0 activates installed fonts; frame 1 paints the toolbar.
            let _ = ctx.run(raw.clone(), |_| {});
            let out = ctx.run(raw, |ctx| {
                draw_camera_toolbar(ctx, stage, &mut state, &i18n);
            });
            assert!(
                !out.shapes.is_empty(),
                "CameraToolbar must emit paint shapes for lang={lang}"
            );
        }
    }
}
