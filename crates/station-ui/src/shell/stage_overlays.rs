use crate::i18n::I18nStore;
use crate::icons::{self, IconKind};
use crate::state::{
    ActiveDrawer, ClipShape, PhotoSize, PhotoTimeMode, ReplayRange, UiAction, UiState,
};
use crate::theme::{
    ACCENT, ACCENT_HI, AMBER, BLUE, CARD, CARD_2, CARD_3, GREEN, INK, LINE, LINE_2, MUTED, RED,
    heading_bold,
};
use bevy_egui::egui::{
    self, Color32, CornerRadius, FontFamily, FontId, Margin, Rect, RichText, Sense, Stroke,
    StrokeKind, pos2, vec2,
};

const ACCENT_SOFT: Color32 = Color32::from_rgba_premultiplied(0x1F, 0x12, 0x0C, 33);
const ACCENT_LINE: Color32 = Color32::from_rgba_premultiplied(0x90, 0x53, 0x39, 153);
const ON_ACCENT: Color32 = Color32::from_rgb(0x2A, 0x13, 0x07);
const SHUTTER_GOLD: Color32 = Color32::from_rgb(0xF2, 0xB3, 0x3D);
const ON_GOLD: Color32 = Color32::from_rgb(0x1A, 0x12, 0x06);
const REC_RED: Color32 = Color32::from_rgb(0xFF, 0x6B, 0x5E);
const BIL_TURN: Color32 = Color32::from_rgb(0xFF, 0xD2, 0x7A);

const REPLAY_SPEEDS: [f32; 4] = [1.0, 2.0, 4.0, 8.0];

fn glass_dark(alpha: u8) -> Color32 {
    Color32::from_rgba_unmultiplied(0x11, 0x17, 0x20, alpha)
}

fn perm_bg() -> Color32 {
    Color32::from_rgba_unmultiplied(0x1E, 0x1A, 0x16, 240)
}

fn deny_bg() -> Color32 {
    Color32::from_rgba_unmultiplied(0xFF, 0x5F, 0x5F, 45)
}

/// Draws all active stage HUD overlays clipped/positioned inside `stage_rect`:
/// `perm-stack`, `FirstPersonHud`, `ReplayBar`, `PhotoBar`, and floating `PlaceCard`.
pub fn draw_stage_overlays(
    ctx: &egui::Context,
    stage_rect: Rect,
    state: &mut UiState,
    i18n: &I18nStore,
) {
    if state.photo_mode {
        draw_photo_bar(ctx, stage_rect, state, i18n);
        return;
    }

    draw_perm_stack(ctx, stage_rect, state, i18n);
    draw_place_card(ctx, stage_rect, state, i18n);

    if state.first_person {
        draw_first_person_hud(ctx, stage_rect, state, i18n);
    }
    if state.replay_active {
        draw_replay_bar(ctx, stage_rect, state, i18n);
    }
}

/// Permission & urgent hold banner stack mounted cleanly inside `stage_rect` (expanded in Task 5).
pub fn draw_perm_stack(
    ctx: &egui::Context,
    stage_rect: Rect,
    state: &mut UiState,
    i18n: &I18nStore,
) {
    if state.pending_perms.is_empty() || stage_rect.width() < 260.0 {
        return;
    }
    let card_w = 290.0_f32.min(stage_rect.width() - 24.0);
    let top_y = stage_rect.top() + if state.first_person { 58.0 } else { 14.0 };
    let left_x = stage_rect.right() - card_w - 12.0;

    egui::Area::new(egui::Id::new("station_stage_perm_banners"))
        .order(egui::Order::Middle)
        .fade_in(false)
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

/// First-person HUD matching `stage.css:37-56` & `FirstPersonHud.tsx`:
/// center crosshair, help bar (`WASD`, `Mouse`, `E`, `Esc`), `E` near-action pill, `Exit`, and `bil-hud`.
pub fn draw_first_person_hud(
    ctx: &egui::Context,
    stage_rect: Rect,
    state: &mut UiState,
    i18n: &I18nStore,
) {
    if stage_rect.width() < 200.0 || stage_rect.height() < 160.0 {
        return;
    }

    // 1. Center crosshair dot (non-interactive)
    let painter = ctx.layer_painter(egui::LayerId::new(
        egui::Order::Middle,
        egui::Id::new("station_fp_crosshair"),
    ));
    let clipped = painter.with_clip_rect(stage_rect);
    let center = stage_rect.center();
    clipped.circle_filled(center, 5.0, Color32::from_rgba_unmultiplied(0, 0, 0, 96));
    clipped.circle_filled(center, 3.0, Color32::from_rgba_unmultiplied(255, 255, 255, 220));

    // 2. Top-center help bar (`WASD`, `Mouse`, `E`, `Esc` via `ui.firstPersonHud.*`)
    let help_max_w = (stage_rect.width() - 160.0).max(200.0);
    egui::Area::new(egui::Id::new("station_fp_help"))
        .order(egui::Order::Middle)
        .fade_in(false)
        .pivot(egui::Align2::CENTER_TOP)
        .fixed_pos(pos2(stage_rect.center().x, stage_rect.top() + 14.0))
        .show(ctx, |ui| {
            ui.set_max_width(help_max_w);
            egui::Frame::NONE
                .fill(glass_dark(204))
                .corner_radius(CornerRadius::same(12))
                .inner_margin(Margin::symmetric(14, 8))
                .stroke(Stroke::new(1.0_f32, LINE_2))
                .show(ui, |ui| {
                    ui.horizontal_wrapped(|ui| {
                        ui.spacing_mut().item_spacing.x = 10.0;
                        ui.label(
                            heading_bold(i18n.tr("ui.firstPersonHud.title"), 11.5).color(AMBER),
                        );
                        ui.label(
                            RichText::new(i18n.tr("ui.firstPersonHud.keys"))
                                .size(11.0)
                                .color(MUTED),
                        );
                    });
                });
        });

    // 3. Top-right Exit button (`fp-exit`)
    egui::Area::new(egui::Id::new("station_fp_exit"))
        .order(egui::Order::Middle)
        .pivot(egui::Align2::RIGHT_TOP)
        .fixed_pos(pos2(stage_rect.right() - 14.0, stage_rect.top() + 12.0))
        .show(ctx, |ui| {
            let resp = egui::Frame::NONE
                .fill(glass_dark(220))
                .corner_radius(CornerRadius::same(10))
                .inner_margin(Margin::symmetric(12, 8))
                .stroke(Stroke::new(1.0_f32, LINE_2))
                .show(ui, |ui| {
                    ui.horizontal(|ui| {
                        ui.spacing_mut().item_spacing.x = 6.0;
                        let (ic, _) = ui.allocate_exact_size(vec2(13.0, 13.0), Sense::hover());
                        icons::paint_icon(ui.painter(), ic, IconKind::Close, INK);
                        ui.label(heading_bold(i18n.tr("ui.firstPersonHud.exit"), 11.5).color(INK));
                    });
                })
                .response
                .interact(Sense::click());
            if resp.clicked() {
                state.push_action(UiAction::ToggleCameraFp);
            }
        });

    // 4. Center-below `E` near-action pill (`fp-near`)
    if let Some(near_text) = state.fp_near_label.clone() {
        egui::Area::new(egui::Id::new("station_fp_near"))
            .order(egui::Order::Middle)
            .pivot(egui::Align2::CENTER_TOP)
            .fixed_pos(pos2(stage_rect.center().x, stage_rect.center().y + 34.0))
            .show(ctx, |ui| {
                let resp = egui::Frame::NONE
                    .fill(glass_dark(230))
                    .corner_radius(CornerRadius::same(99))
                    .inner_margin(Margin::symmetric(14, 7))
                    .stroke(Stroke::new(1.2_f32, GREEN))
                    .show(ui, |ui| {
                        ui.horizontal(|ui| {
                            ui.spacing_mut().item_spacing.x = 8.0;
                            let (kbd_rect, _) =
                                ui.allocate_exact_size(vec2(20.0, 20.0), Sense::hover());
                            ui.painter()
                                .rect_filled(kbd_rect, CornerRadius::same(6), GREEN);
                            let e_g = ui.fonts(|f| {
                                f.layout_no_wrap(
                                    "E".to_owned(),
                                    FontId::new(11.0, FontFamily::Name(crate::theme::FONT_BOLD.into())),
                                    Color32::from_rgb(0x06, 0x21, 0x0F),
                                )
                            });
                            ui.painter().galley(
                                pos2(
                                    kbd_rect.center().x - e_g.size().x * 0.5,
                                    kbd_rect.center().y - e_g.size().y * 0.5,
                                ),
                                e_g,
                                Color32::from_rgb(0x06, 0x21, 0x0F),
                            );
                            ui.label(heading_bold(near_text, 12.0).color(INK));
                        });
                    })
                    .response
                    .interact(Sense::click());
                if resp.clicked() {
                    state.push_action(UiAction::TriggerFirstPersonInteract);
                }
            });
    }

    // 5. Billiards power bar (`bil-hud`, matching `stage.css:47-53`)
    if let Some(bil) = state.fp_billiards.clone() {
        egui::Area::new(egui::Id::new("station_fp_bil_hud"))
            .order(egui::Order::Middle)
            .pivot(egui::Align2::CENTER_BOTTOM)
            .fixed_pos(pos2(stage_rect.center().x, stage_rect.bottom() - 74.0))
            .show(ctx, |ui| {
                egui::Frame::NONE
                    .fill(Color32::from_rgba_unmultiplied(0x0A, 0x0E, 0x16, 210))
                    .corner_radius(CornerRadius::same(14))
                    .inner_margin(Margin::symmetric(16, 10))
                    .stroke(Stroke::new(1.0_f32, LINE_2))
                    .show(ui, |ui| {
                        ui.vertical_centered(|ui| {
                            ui.spacing_mut().item_spacing.y = 5.0;
                            if !bil.score_label.is_empty() {
                                ui.label(
                                    RichText::new(&bil.score_label)
                                        .font(FontId::new(13.5, FontFamily::Monospace))
                                        .color(Color32::WHITE),
                                );
                            }
                            let turn_text = if bil.turn_label.is_empty() {
                                i18n.tr("fp.bil.yourTurn")
                            } else {
                                bil.turn_label
                            };
                            ui.label(heading_bold(turn_text, 12.0).color(BIL_TURN));

                            // 220x8 power track filled green -> yellow -> red
                            let (bar_rect, _) =
                                ui.allocate_exact_size(vec2(220.0, 8.0), Sense::hover());
                            ui.painter().rect_filled(
                                bar_rect,
                                CornerRadius::same(4),
                                Color32::from_rgba_unmultiplied(255, 255, 255, 46),
                            );
                            let p = bil.power.clamp(0.0, 1.0);
                            if p > 0.0 {
                                let fill_col = if p < 0.5 {
                                    Color32::from_rgb(0x4A, 0xDE, 0x80)
                                } else if p < 0.8 {
                                    Color32::from_rgb(0xFA, 0xCC, 0x15)
                                } else {
                                    Color32::from_rgb(0xEF, 0x44, 0x44)
                                };
                                let fill_rect = Rect::from_min_size(
                                    bar_rect.min,
                                    vec2(bar_rect.width() * p, bar_rect.height()),
                                );
                                ui.painter()
                                    .rect_filled(fill_rect, CornerRadius::same(4), fill_col);
                            }

                            ui.label(
                                RichText::new(i18n.tr("fp.bil.help"))
                                    .size(10.5)
                                    .color(Color32::from_rgb(0xC9, 0xD4, 0xE3)),
                            );
                        });
                    });
            });
    }
}

/// Replay bar matching `replay.css` & `ReplayBar.tsx`:
/// play/pause, activity heatmap ticks + scrubber slider, range selector, speed `1x|2x|4x|8x`, close.
pub fn draw_replay_bar(
    ctx: &egui::Context,
    stage_rect: Rect,
    state: &mut UiState,
    i18n: &I18nStore,
) {
    if stage_rect.width() < 280.0 || stage_rect.height() < 180.0 {
        return;
    }

    let bar_w = 820.0_f32.min(stage_rect.width() - 24.0);
    egui::Area::new(egui::Id::new("station_stage_replay_bar"))
        .order(egui::Order::Middle)
        .fade_in(false)
        .pivot(egui::Align2::CENTER_BOTTOM)
        .fixed_pos(pos2(stage_rect.center().x, stage_rect.bottom() - 84.0))
        .show(ctx, |ui| {
            ui.set_width(bar_w);
            egui::Frame::NONE
                .fill(glass_dark(236))
                .corner_radius(CornerRadius::same(16))
                .inner_margin(Margin::symmetric(14, 12))
                .stroke(Stroke::new(1.0_f32, LINE_2))
                .show(ui, |ui| {
                    ui.vertical(|ui| {
                        ui.spacing_mut().item_spacing.y = 10.0;

                        // Top row: Play/Pause circle + heatmap/scrubber track + time readout
                        ui.horizontal(|ui| {
                            ui.spacing_mut().item_spacing.x = 12.0;
                            let (play_rect, play_resp) =
                                ui.allocate_exact_size(vec2(40.0, 40.0), Sense::click());
                            let play_bg = if play_resp.hovered() { ACCENT_HI } else { ACCENT };
                            ui.painter().circle_filled(play_rect.center(), 20.0, play_bg);
                            let play_icon = if state.replay_playing {
                                IconKind::Pause
                            } else {
                                IconKind::Play
                            };
                            icons::paint_icon(
                                ui.painter(),
                                Rect::from_center_size(play_rect.center(), vec2(17.0, 17.0)),
                                play_icon,
                                ON_ACCENT,
                            );
                            let play_tip = if state.replay_playing {
                                i18n.tr("replay.pause")
                            } else {
                                i18n.tr("replay.play")
                            };
                            if play_resp.on_hover_text(play_tip).clicked() {
                                state.replay_playing = !state.replay_playing;
                                state.push_action(UiAction::ReplayTogglePlay);
                            }

                            let track_w = (bar_w - 190.0).max(120.0);
                            ui.vertical(|ui| {
                                ui.spacing_mut().item_spacing.y = 2.0;
                                // Activity heatmap tick strip
                                let (heat_rect, _) =
                                    ui.allocate_exact_size(vec2(track_w, 24.0), Sense::hover());
                                paint_replay_heatmap(
                                    ui.painter(),
                                    heat_rect,
                                    &state.replay_heatmap,
                                    state.replay_progress,
                                );

                                // Interactive scrubber slider
                                let mut progress = state.replay_progress.clamp(0.0, 1.0);
                                let slider = egui::Slider::new(&mut progress, 0.0..=1.0)
                                    .show_value(false);
                                ui.spacing_mut().slider_width = track_w;
                                if ui
                                    .add(slider)
                                    .on_hover_text(i18n.tr("ui.replayBar.seek"))
                                    .changed()
                                {
                                    state.replay_progress = progress;
                                    state.push_action(UiAction::ReplayScrub(progress));
                                }
                            });

                            ui.with_layout(egui::Layout::top_down(egui::Align::Max), |ui| {
                                ui.set_min_width(86.0);
                                ui.label(
                                    RichText::new(&state.replay_time_label)
                                        .font(FontId::new(15.0, FontFamily::Monospace))
                                        .color(INK),
                                );
                                let span = if state.replay_span_label.is_empty() {
                                    i18n.tr("ui.replayBar.loading")
                                } else {
                                    state.replay_span_label.clone()
                                };
                                ui.label(RichText::new(span).size(10.5).color(MUTED));
                            });
                        });

                        // Options row: Range chips + Speed chips (1x|2x|4x|8x) + Close
                        ui.horizontal_wrapped(|ui| {
                            ui.spacing_mut().item_spacing = vec2(6.0, 6.0);

                            // Range selector
                            for (range, key) in [
                                (ReplayRange::H1, "ui.ranges.h1"),
                                (ReplayRange::H3, "ui.ranges.h3"),
                                (ReplayRange::Today, "ui.ranges.today"),
                                (ReplayRange::Yesterday, "ui.ranges.yesterday"),
                            ] {
                                let active = state.replay_range == range;
                                if seg_pill_btn(ui, &i18n.tr(key), active).clicked() {
                                    state.replay_range = range;
                                    state.push_action(UiAction::ReplaySetRange(range));
                                }
                            }

                            ui.add_space(6.0);

                            // Speed selector (`1x|2x|4x|8x`)
                            for spd in REPLAY_SPEEDS {
                                let active = (state.replay_speed - spd).abs() < 0.01;
                                let label = format!("{}x", spd as u32);
                                if seg_pill_btn(ui, &label, active)
                                    .on_hover_text(i18n.tr("ui.replayBar.speed"))
                                    .clicked()
                                {
                                    state.replay_speed = spd;
                                    state.push_action(UiAction::ReplaySetSpeed(spd));
                                }
                            }

                            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                                let close_resp = egui::Frame::NONE
                                    .fill(CARD_2)
                                    .corner_radius(CornerRadius::same(8))
                                    .inner_margin(Margin::symmetric(10, 6))
                                    .stroke(Stroke::new(1.0_f32, LINE_2))
                                    .show(ui, |ui| {
                                        ui.horizontal(|ui| {
                                            ui.spacing_mut().item_spacing.x = 5.0;
                                            let (ic, _) = ui.allocate_exact_size(
                                                vec2(12.0, 12.0),
                                                Sense::hover(),
                                            );
                                            icons::paint_icon(ui.painter(), ic, IconKind::Close, INK);
                                            ui.label(
                                                heading_bold(
                                                    i18n.tr("ui.replayBar.backToLive"),
                                                    11.5,
                                                )
                                                .color(INK),
                                            );
                                        });
                                    })
                                    .response
                                    .interact(Sense::click());
                                if close_resp.clicked() {
                                    state.replay_active = false;
                                    state.replay_playing = false;
                                    state.push_action(UiAction::ReplayClose);
                                }
                            });
                        });
                    });
                });
        });
}

fn paint_replay_heatmap(painter: &egui::Painter, rect: Rect, ticks: &[f32], progress: f32) {
    painter.rect_filled(
        rect,
        CornerRadius::same(4),
        Color32::from_rgba_unmultiplied(255, 255, 255, 10),
    );
    let count = ticks.len().max(16);
    let gap = 2.0;
    let bar_w = ((rect.width() - gap * (count as f32 + 1.0)) / count as f32).max(2.0);
    for i in 0..count {
        let intensity = ticks.get(i).copied().unwrap_or(0.12).clamp(0.08, 1.0);
        let frac = (i as f32 + 0.5) / count as f32;
        let col = if frac <= progress {
            ACCENT
        } else {
            Color32::from_rgba_unmultiplied(0x3D, 0xDC, 0x84, (80.0 + intensity * 140.0) as u8)
        };
        let h = (rect.height() - 4.0) * intensity;
        let x = rect.left() + gap + i as f32 * (bar_w + gap);
        let r = Rect::from_min_max(pos2(x, rect.bottom() - 2.0 - h), pos2(x + bar_w, rect.bottom() - 2.0));
        painter.rect_filled(r, CornerRadius::same(2), col);
    }
    let playhead_x = rect.left() + rect.width() * progress.clamp(0.0, 1.0);
    painter.line_segment(
        [pos2(playhead_x, rect.top()), pos2(playhead_x, rect.bottom())],
        Stroke::new(1.6_f32, Color32::WHITE),
    );
}

/// Photo mode bar matching `photo.css` & `PhotoBar.tsx`:
/// `Names` / `Logo` toggles, `Auto|Day|Night`, `Screen|FHD (1920x1080)|4K (3840x2160)` PNG capture,
/// `15s Timelapse` trigger + shape/music toggles, shutter CTA, and close.
pub fn draw_photo_bar(
    ctx: &egui::Context,
    stage_rect: Rect,
    state: &mut UiState,
    i18n: &I18nStore,
) {
    if state.photo_capture_hide_frames > 0
        || stage_rect.width() < 260.0
        || stage_rect.height() < 120.0
    {
        return;
    }

    egui::Area::new(egui::Id::new("station_stage_photo_bar"))
        .order(egui::Order::Foreground)
        .fade_in(false)
        .pivot(egui::Align2::CENTER_BOTTOM)
        .fixed_pos(pos2(stage_rect.center().x, stage_rect.bottom() - 18.0))
        .show(ctx, |ui| {
            ui.set_max_width((stage_rect.width() - 24.0).max(240.0));
            ui.vertical_centered(|ui| {
                ui.spacing_mut().item_spacing.y = 8.0;

                // Hint pill
                if stage_rect.width() >= 620.0 {
                    egui::Frame::NONE
                        .fill(Color32::from_rgba_unmultiplied(0x0E, 0x13, 0x1B, 165))
                        .corner_radius(CornerRadius::same(99))
                        .inner_margin(Margin::symmetric(12, 4))
                        .show(ui, |ui| {
                            ui.label(
                                RichText::new(i18n.tr("ui.photo.hint"))
                                    .size(11.0)
                                    .color(INK),
                            );
                        });
                }

                // Controls glass bar
                egui::Frame::NONE
                    .fill(Color32::from_rgba_unmultiplied(0x0E, 0x13, 0x1B, 228))
                    .corner_radius(CornerRadius::same(16))
                    .inner_margin(Margin::same(8))
                    .stroke(Stroke::new(1.0_f32, LINE_2))
                    .show(ui, |ui| {
                        ui.horizontal_wrapped(|ui| {
                            ui.spacing_mut().item_spacing = vec2(6.0, 6.0);

                            // Nametags & mark toggles
                            if photo_toggle_btn(ui, &i18n.tr("ui.photo.names"), state.photo_show_names)
                                .clicked()
                            {
                                state.photo_show_names = !state.photo_show_names;
                            }
                            if photo_toggle_btn(ui, &i18n.tr("ui.photo.mark"), state.photo_show_mark)
                                .clicked()
                            {
                                state.photo_show_mark = !state.photo_show_mark;
                            }

                            photo_sep(ui);

                            // Time mode (`Auto | Day | Night`)
                            for (tm, key) in [
                                (PhotoTimeMode::Auto, "ui.topBar.auto"),
                                (PhotoTimeMode::Day, "ui.topBar.day"),
                                (PhotoTimeMode::Night, "ui.topBar.night"),
                            ] {
                                if photo_toggle_btn(ui, &i18n.tr(key), state.photo_time_mode == tm)
                                    .clicked()
                                {
                                    state.photo_time_mode = tm;
                                    match tm {
                                        PhotoTimeMode::Day => {
                                            state.night = false;
                                            state.push_action(UiAction::ToggleNight(false));
                                        }
                                        PhotoTimeMode::Night => {
                                            state.night = true;
                                            state.push_action(UiAction::ToggleNight(true));
                                        }
                                        PhotoTimeMode::Auto => {
                                            state.push_action(UiAction::SetPhotoTimeAuto);
                                        }
                                    }
                                }
                            }

                            photo_sep(ui);

                            // Resolution selector (`Screen | FHD (1920x1080) | 4K (3840x2160)`)
                            let size_tip = i18n.tr("ui.photo.sizeTitle");
                            for (sz, label) in [
                                (PhotoSize::Screen, i18n.tr("ui.photo.screen")),
                                (PhotoSize::Fhd, "FHD (1920x1080)".to_owned()),
                                (PhotoSize::Uhd4k, "4K (3840x2160)".to_owned()),
                            ] {
                                if photo_toggle_btn(ui, &label, state.photo_size == sz)
                                    .on_hover_text(&size_tip)
                                    .clicked()
                                {
                                    state.photo_size = sz;
                                }
                            }

                            photo_sep(ui);

                            // Clip shape & music + 15s Timelapse trigger
                            let shape_tip = i18n.tr("ui.clip.shapeTitle");
                            for (shape, key) in [
                                (ClipShape::Square, "ui.clip.square"),
                                (ClipShape::Tall, "ui.clip.tall"),
                            ] {
                                if photo_toggle_btn(ui, &i18n.tr(key), state.clip_shape == shape)
                                    .on_hover_text(&shape_tip)
                                    .clicked()
                                {
                                    state.clip_shape = shape;
                                }
                            }

                            if photo_icon_toggle_btn(
                                ui,
                                IconKind::Music,
                                &i18n.tr("ui.clip.music"),
                                state.clip_music,
                            )
                            .on_hover_text(i18n.tr("ui.clip.musicTitle"))
                            .clicked()
                            {
                                state.clip_music = !state.clip_music;
                            }

                            // 15s Timelapse trigger button
                            let clip_resp = egui::Frame::NONE
                                .fill(Color32::from_rgba_unmultiplied(255, 255, 255, 20))
                                .corner_radius(CornerRadius::same(11))
                                .inner_margin(Margin::symmetric(12, 8))
                                .stroke(Stroke::new(1.0_f32, ACCENT_LINE))
                                .show(ui, |ui| {
                                    ui.horizontal(|ui| {
                                        ui.spacing_mut().item_spacing.x = 6.0;
                                        let (ic, _) = ui.allocate_exact_size(
                                            vec2(13.0, 13.0),
                                            Sense::hover(),
                                        );
                                        icons::paint_icon(ui.painter(), ic, IconKind::Record, REC_RED);
                                        ui.label(
                                            heading_bold(i18n.tr("ui.clip.button"), 12.0)
                                                .color(INK),
                                        );
                                    });
                                })
                                .response
                                .interact(Sense::click());
                            if clip_resp
                                .on_hover_text(i18n.tr("ui.clip.buttonTitle"))
                                .clicked()
                            {
                                state.push_action(UiAction::TriggerTimelapseClip {
                                    shape: state.clip_shape,
                                    music: state.clip_music,
                                });
                            }

                            // Golden shutter button (`Take photo`)
                            let shoot_resp = egui::Frame::NONE
                                .fill(SHUTTER_GOLD)
                                .corner_radius(CornerRadius::same(11))
                                .inner_margin(Margin::symmetric(14, 8))
                                .show(ui, |ui| {
                                    ui.horizontal(|ui| {
                                        ui.spacing_mut().item_spacing.x = 7.0;
                                        let (ic, _) = ui.allocate_exact_size(
                                            vec2(14.0, 14.0),
                                            Sense::hover(),
                                        );
                                        icons::paint_icon(
                                            ui.painter(),
                                            ic,
                                            IconKind::Camera,
                                            ON_GOLD,
                                        );
                                        ui.label(
                                            heading_bold(i18n.tr("ui.photo.shoot"), 12.2)
                                                .color(ON_GOLD),
                                        );
                                    });
                                })
                                .response
                                .interact(Sense::click());
                            if shoot_resp.clicked() {
                                state.push_action(UiAction::CapturePhoto {
                                    size: state.photo_size,
                                    show_names: state.photo_show_names,
                                    show_mark: state.photo_show_mark,
                                });
                            }

                            // Close Photo mode button
                            let (exit_rect, exit_resp) =
                                ui.allocate_exact_size(vec2(32.0, 32.0), Sense::click());
                            let exit_bg = if exit_resp.hovered() { CARD_2 } else { Color32::TRANSPARENT };
                            ui.painter()
                                .rect_filled(exit_rect, CornerRadius::same(9), exit_bg);
                            ui.painter().rect_stroke(
                                exit_rect,
                                CornerRadius::same(9),
                                Stroke::new(1.0_f32, LINE_2),
                                StrokeKind::Inside,
                            );
                            icons::paint_icon(
                                ui.painter(),
                                Rect::from_center_size(exit_rect.center(), vec2(14.0, 14.0)),
                                IconKind::Close,
                                INK,
                            );
                            if exit_resp
                                .on_hover_text(i18n.tr("ui.photo.exit"))
                                .clicked()
                            {
                                state.photo_mode = false;
                                state.push_action(UiAction::SetPhotoMode(false));
                            }
                        });
                    });
            });
        });
}

/// Floating quick-info `PlaceCard` anchored inside `stage_rect` matching `places.css:26-66`.
pub fn draw_place_card(
    ctx: &egui::Context,
    stage_rect: Rect,
    state: &mut UiState,
    i18n: &I18nStore,
) {
    let Some(card) = state.active_place_card.clone() else {
        return;
    };
    if stage_rect.width() < 260.0 || stage_rect.height() < 180.0 {
        return;
    }

    let card_w = 330.0_f32.min(stage_rect.width() - 28.0);
    let max_h = (stage_rect.height() - 104.0).clamp(160.0, 480.0);
    let mut close_clicked = false;
    let mut select_session: Option<String> = None;
    let mut triggered_action: Option<String> = None;

    egui::Area::new(egui::Id::new("station_stage_place_card"))
        .order(egui::Order::Middle)
        .fade_in(false)
        .fixed_pos(pos2(stage_rect.left() + 14.0, stage_rect.top() + 14.0))
        .show(ctx, |ui| {
            ui.set_width(card_w);
            egui::Frame::NONE
                .fill(Color32::from_rgba_unmultiplied(0x13, 0x19, 0x22, 246))
                .corner_radius(CornerRadius::same(16))
                .inner_margin(Margin::symmetric(14, 12))
                .stroke(Stroke::new(1.0_f32, LINE_2))
                .show(ui, |ui| {
                    // Header
                    ui.horizontal(|ui| {
                        ui.spacing_mut().item_spacing.x = 10.0;
                        let (ico_rect, _) =
                            ui.allocate_exact_size(vec2(34.0, 34.0), Sense::hover());
                        ui.painter()
                            .rect_filled(ico_rect, CornerRadius::same(10), ACCENT_SOFT);
                        icons::paint_icon(
                            ui.painter(),
                            Rect::from_center_size(ico_rect.center(), vec2(17.0, 17.0)),
                            card.icon,
                            ACCENT,
                        );

                        ui.vertical(|ui| {
                            ui.set_max_width((card_w - 104.0).max(120.0));
                            ui.label(heading_bold(&card.title, 14.0).color(INK));
                            if !card.subtitle.is_empty() {
                                ui.label(RichText::new(&card.subtitle).size(11.0).color(MUTED));
                            }
                        });

                        ui.with_layout(egui::Layout::right_to_left(egui::Align::Min), |ui| {
                            let (c_rect, c_resp) =
                                ui.allocate_exact_size(vec2(28.0, 28.0), Sense::click());
                            if c_resp.hovered() {
                                ui.painter()
                                    .rect_filled(c_rect, CornerRadius::same(8), CARD_2);
                            }
                            icons::paint_icon(
                                ui.painter(),
                                Rect::from_center_size(c_rect.center(), vec2(13.0, 13.0)),
                                IconKind::Close,
                                MUTED,
                            );
                            if c_resp.on_hover_text(i18n.tr("common.close")).clicked() {
                                close_clicked = true;
                            }
                        });
                    });

                    ui.add_space(8.0);
                    ui.painter().line_segment(
                        [
                            pos2(ui.min_rect().left(), ui.cursor().top()),
                            pos2(ui.min_rect().left() + card_w - 28.0, ui.cursor().top()),
                        ],
                        Stroke::new(1.0_f32, LINE),
                    );
                    ui.add_space(6.0);

                    // Body rows
                    egui::ScrollArea::vertical()
                        .max_height((max_h - 120.0).max(80.0))
                        .show(ui, |ui| {
                            if card.rows.is_empty() {
                                ui.label(
                                    RichText::new(&card.empty_text).size(11.5).color(MUTED),
                                );
                            } else {
                                for row in &card.rows {
                                    let dot_col = match row.accent_tone.as_deref() {
                                        Some("green") => GREEN,
                                        Some("amber") => AMBER,
                                        Some("red") => RED,
                                        Some("blue") => BLUE,
                                        _ => MUTED,
                                    };
                                    let row_resp = egui::Frame::NONE
                                        .fill(CARD)
                                        .corner_radius(CornerRadius::same(10))
                                        .inner_margin(Margin::symmetric(9, 7))
                                        .show(ui, |ui| {
                                            ui.horizontal(|ui| {
                                                ui.spacing_mut().item_spacing.x = 8.0;
                                                crate::theme::status_dot(ui, dot_col, 4.0);
                                                ui.vertical(|ui| {
                                                    ui.label(
                                                        heading_bold(&row.title, 12.0).color(INK),
                                                    );
                                                    if !row.subtitle.is_empty() {
                                                        ui.label(
                                                            RichText::new(&row.subtitle)
                                                                .size(10.5)
                                                                .color(MUTED),
                                                        );
                                                    }
                                                });
                                            });
                                        })
                                        .response
                                        .interact(if row.session_id.is_some() {
                                            Sense::click()
                                        } else {
                                            Sense::hover()
                                        });
                                    if row_resp.clicked()
                                        && let Some(sid) = &row.session_id
                                    {
                                        select_session = Some(sid.clone());
                                    }
                                    ui.add_space(4.0);
                                }
                            }
                        });

                    // Footer
                    if card.footer_note.is_some() || !card.actions.is_empty() {
                        ui.add_space(6.0);
                        ui.horizontal_wrapped(|ui| {
                            ui.spacing_mut().item_spacing = vec2(8.0, 6.0);
                            if let Some(note) = &card.footer_note {
                                ui.label(RichText::new(note).size(11.0).color(MUTED));
                            }
                            for act in &card.actions {
                                let btn = if act.primary {
                                    egui::Button::new(
                                        heading_bold(&act.label, 11.5).color(ON_ACCENT),
                                    )
                                    .fill(ACCENT)
                                    .corner_radius(CornerRadius::same(9))
                                } else {
                                    egui::Button::new(heading_bold(&act.label, 11.5).color(INK))
                                        .fill(CARD_2)
                                        .stroke(Stroke::new(1.0_f32, LINE_2))
                                        .corner_radius(CornerRadius::same(9))
                                };
                                if ui.add(btn).clicked() {
                                    triggered_action = Some(act.id.clone());
                                }
                            }
                        });
                    }
                });
        });

    if close_clicked {
        state.active_place_card = None;
        state.push_action(UiAction::ClosePlaceCard);
    }
    if let Some(sid) = select_session {
        state.selected_session = Some(sid);
        state.active_drawer = ActiveDrawer::Session;
        state.active_place_card = None;
    }
    if let Some(action_id) = triggered_action {
        state.push_action(UiAction::TriggerPlaceAction {
            place_id: card.place_id,
            action_id,
        });
    }
}

fn seg_pill_btn(ui: &mut egui::Ui, label: &str, active: bool) -> egui::Response {
    let fill = if active { CARD_3 } else { CARD };
    let stroke = if active {
        Stroke::new(1.0_f32, ACCENT_LINE)
    } else {
        Stroke::new(1.0_f32, LINE)
    };
    let fg = if active { INK } else { MUTED };
    ui.add(
        egui::Button::new(heading_bold(label, 11.0).color(fg))
            .fill(fill)
            .stroke(stroke)
            .corner_radius(CornerRadius::same(8)),
    )
}

fn photo_toggle_btn(ui: &mut egui::Ui, label: &str, active: bool) -> egui::Response {
    let fill = if active { ACCENT_SOFT } else { Color32::TRANSPARENT };
    let stroke = if active {
        Stroke::new(1.0_f32, ACCENT_LINE)
    } else {
        Stroke::new(1.0_f32, LINE_2)
    };
    let fg = if active { Color32::WHITE } else { MUTED };
    ui.add(
        egui::Button::new(heading_bold(label, 11.5).color(fg))
            .fill(fill)
            .stroke(stroke)
            .corner_radius(CornerRadius::same(10)),
    )
}

fn photo_icon_toggle_btn(
    ui: &mut egui::Ui,
    icon: IconKind,
    label: &str,
    active: bool,
) -> egui::Response {
    let fill = if active { ACCENT_SOFT } else { Color32::TRANSPARENT };
    let stroke = if active {
        Stroke::new(1.0_f32, ACCENT_LINE)
    } else {
        Stroke::new(1.0_f32, LINE_2)
    };
    let fg = if active { Color32::WHITE } else { MUTED };
    egui::Frame::NONE
        .fill(fill)
        .corner_radius(CornerRadius::same(10))
        .inner_margin(Margin::symmetric(10, 7))
        .stroke(stroke)
        .show(ui, |ui| {
            ui.horizontal(|ui| {
                ui.spacing_mut().item_spacing.x = 5.0;
                let (ic, _) = ui.allocate_exact_size(vec2(13.0, 13.0), Sense::hover());
                icons::paint_icon(ui.painter(), ic, icon, fg);
                ui.label(heading_bold(label, 11.5).color(fg));
            });
        })
        .response
        .interact(Sense::click())
}

fn photo_sep(ui: &mut egui::Ui) {
    let (rect, _) = ui.allocate_exact_size(vec2(1.0, 24.0), Sense::hover());
    ui.painter().rect_filled(rect, CornerRadius::ZERO, LINE_2);
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::state::{BilliardsHudState, PlaceCardAction, PlaceCardInfo, PlaceCardRow};
    use std::path::PathBuf;

    #[test]
    fn stage_overlays_render_fp_replay_photo_and_place_card_without_raw_keys() {
        let assets = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../assets");
        let bundle = crate::fonts::load_font_bundle(&assets).expect("load_font_bundle");
        let stage = Rect::from_min_size(pos2(300.0, 56.0), vec2(650.0, 664.0));

        let required_keys = [
            "ui.firstPersonHud.title",
            "ui.firstPersonHud.keys",
            "ui.firstPersonHud.exit",
            "fp.bil.yourTurn",
            "fp.bil.help",
            "ui.replayBar.seek",
            "ui.replayBar.loading",
            "ui.replayBar.speed",
            "ui.replayBar.backToLive",
            "replay.play",
            "replay.pause",
            "ui.ranges.h1",
            "ui.ranges.h3",
            "ui.ranges.today",
            "ui.ranges.yesterday",
            "ui.photo.hint",
            "ui.photo.names",
            "ui.photo.mark",
            "ui.photo.screen",
            "ui.photo.shoot",
            "ui.photo.exit",
            "ui.clip.button",
            "ui.clip.square",
            "ui.clip.tall",
            "ui.clip.music",
            "common.close",
        ];

        for lang in ["en", "vi", "th"] {
            let i18n = I18nStore::load_from_assets(&assets, lang);
            for key in required_keys {
                let val = i18n.tr(key);
                assert!(!val.is_empty() && val != key, "lang={lang} raw key {key}");
            }

            let ctx = egui::Context::default();
            crate::install_egui_fonts(&ctx, &bundle);
            let raw = egui::RawInput {
                screen_rect: Some(Rect::from_min_size(pos2(0.0, 0.0), vec2(1280.0, 720.0))),
                ..Default::default()
            };
            let _ = ctx.run(raw.clone(), |_| {});

            // 1. FirstPerson + Replay + PlaceCard
            let mut state = UiState::with_demo_seed(
                station_3d::OfficeLayout::Pro,
                station_3d::BackdropKind::City,
                false,
            );
            state.first_person = true;
            state.fp_near_label = Some(i18n.tr("fp.petCat"));
            state.fp_billiards = Some(BilliardsHudState {
                score_label: "2 - 1".to_owned(),
                turn_label: i18n.tr("fp.bil.yourTurn"),
                power: 0.65,
            });
            state.replay_active = true;
            state.active_place_card = Some(PlaceCardInfo {
                place_id: "meeting".to_owned(),
                icon: IconKind::Users,
                title: i18n.tr("places.meeting.title"),
                subtitle: i18n.tr("places.meeting.sub"),
                rows: vec![PlaceCardRow {
                    title: "shell-inspector".to_owned(),
                    subtitle: "cargo test -p station-ui".to_owned(),
                    session_id: Some("sess-claude-1".to_owned()),
                    accent_tone: Some("blue".to_owned()),
                }],
                empty_text: i18n.tr("places.meeting.empty"),
                footer_note: None,
                actions: vec![PlaceCardAction {
                    id: "focus".to_owned(),
                    label: i18n.tr("ui.cameraToolbar.meeting"),
                    primary: true,
                }],
            });
            let out_live = ctx.run(raw.clone(), |ctx| {
                draw_stage_overlays(ctx, stage, &mut state, &i18n);
            });
            assert!(!out_live.shapes.is_empty(), "live overlays emitted no shapes");

            // 2. Photo mode bar
            state.photo_mode = true;
            let full = Rect::from_min_size(pos2(0.0, 0.0), vec2(1280.0, 720.0));
            let out_photo = ctx.run(raw.clone(), |ctx| {
                draw_stage_overlays(ctx, full, &mut state, &i18n);
            });
            assert!(!out_photo.shapes.is_empty(), "photo bar emitted no shapes");

            // 3. While shutter capture hides PhotoBar, zero HUD shapes are emitted over the photo.
            state.photo_capture_hide_frames = 1;
            let out_hidden = ctx.run(raw, |ctx| {
                draw_stage_overlays(ctx, full, &mut state, &i18n);
            });
            assert!(
                out_hidden.shapes.is_empty(),
                "PhotoBar must hide cleanly on capture frames"
            );
        }
    }
}
