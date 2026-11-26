use crate::i18n::I18nStore;
use crate::state::{ActiveDrawer, UiAction, UiState, WrappedSummaryUi};
use crate::theme::{
    ACCENT, AMBER, BG, BLUE, CARD, CARD_2, GREEN, INK, LINE_2, MUTED, RED, SIDE, heading_bold,
};
use bevy_egui::egui::{self, Color32, CornerRadius, Margin, RichText, Stroke, vec2};

const TIMELAPSE_TOTAL_SEC: u32 = 15;

pub fn format_wrapped_date(day: &str, i18n: &I18nStore) -> String {
    let mut parts = day.split('-');
    let _y = parts.next();
    let Some(m) = parts.next() else {
        return day.to_owned();
    };
    let Some(d) = parts.next() else {
        return day.to_owned();
    };
    i18n.tr_vars("wrapped.date", &[("d", d), ("m", m)])
}

pub fn format_wrapped_date_range(w: &WrappedSummaryUi, i18n: &I18nStore) -> String {
    if w.from.is_empty() && w.to.is_empty() {
        return i18n.tr("wrapped.fileNow");
    }
    if w.from == w.to || w.from.is_empty() {
        return format_wrapped_date(&w.to, i18n);
    }
    format!(
        "{} – {}",
        format_wrapped_date(&w.from, i18n),
        format_wrapped_date(&w.to, i18n)
    )
}

pub fn format_wrapped_hours_key(key: &str, hours: f64, i18n: &I18nStore) -> String {
    let rounded = (hours * 10.0).round() / 10.0;
    let h_str = if (rounded - rounded.round()).abs() < 0.05 {
        format!("{:.0}", rounded)
    } else {
        format!("{:.1}", rounded)
    };
    let n_str = if (rounded - 1.0).abs() < 0.05 { "1" } else { "2" };
    i18n.tr_vars(key, &[("h", &h_str), ("n", n_str)])
}

pub fn format_wrapped_highlights(
    w: &WrappedSummaryUi,
    streamer: bool,
    i18n: &I18nStore,
) -> Vec<String> {
    let mut out = Vec::new();
    if let Some((proj, hrs)) = &w.top_project {
        let name = if streamer {
            i18n.tr("wrapped.aProject")
        } else {
            proj.clone()
        };
        let rounded = (hrs * 10.0).round() / 10.0;
        let h_str = format!("{rounded:.1}");
        let n_str = if (rounded - 1.0).abs() < 0.05 { "1" } else { "2" };
        out.push(i18n.tr_vars(
            "wrapped.topProject",
            &[("name", &name), ("h", &h_str), ("n", n_str)],
        ));
    }
    if let Some((day, weekday, _hrs)) = &w.busiest_day {
        let weekday_lbl = i18n.tr_pick("people.duty.board.days", (*weekday as usize) % 7);
        let date = format_wrapped_date(day, i18n);
        out.push(i18n.tr_vars(
            "wrapped.busiestDay",
            &[("day", &weekday_lbl), ("date", &date)],
        ));
    }
    if w.longest_min > 0 {
        let time_str = if w.longest_min >= 60 {
            i18n.tr_vars(
                "time.hourMin",
                &[
                    ("h", &(w.longest_min / 60).to_string()),
                    ("m", &(w.longest_min % 60).to_string()),
                ],
            )
        } else {
            i18n.tr_count("time.min", w.longest_min as usize)
        };
        out.push(i18n.tr_vars("wrapped.longest", &[("time", &time_str)]));
    }
    if w.commits > 0 || w.pushes > 0 {
        let c = i18n.tr_vars(
            "wrapped.commits",
            &[
                ("n", &w.commits.to_string()),
                ("c", &w.commits.to_string()),
            ],
        );
        let p = i18n.tr_vars(
            "wrapped.pushes",
            &[
                ("n", &w.pushes.to_string()),
                ("c", &w.pushes.to_string()),
            ],
        );
        out.push(format!("{c} · {p}"));
    }
    out.truncate(3);
    out
}

/// Weekly recap `Wrapped` modal matching `src/office/ui/wrapped.js` with 1080x1350 PNG export.
pub fn draw_wrapped_modal(ctx: &egui::Context, state: &mut UiState, i18n: &I18nStore) {
    if !ctx.wants_keyboard_input() && ctx.input(|i| i.key_pressed(egui::Key::Escape)) {
        state.active_drawer = ActiveDrawer::None;
        return;
    }

    if !state.wrapped.loaded && !state.wrapped.loading {
        state.wrapped.loading = true;
        state.push_action(UiAction::FetchWrapped);
    }

    let mut open = true;
    egui::Window::new(heading_bold(i18n.tr("ui.wrapped.title"), 14.5).color(INK))
        .id(egui::Id::new("station_wrapped_modal"))
        .open(&mut open)
        .anchor(egui::Align2::CENTER_CENTER, vec2(0.0, 0.0))
        .default_width(470.0)
        .max_height(640.0)
        .collapsible(false)
        .resizable(false)
        .frame(
            egui::Frame::window(&ctx.style())
                .fill(SIDE)
                .stroke(Stroke::new(1.2_f32, LINE_2))
                .corner_radius(CornerRadius::same(14))
                .inner_margin(Margin::symmetric(16, 14)),
        )
        .show(ctx, |ui| {
            let w = state.wrapped.clone();

            // Status / note banner
            let note = if w.loading {
                i18n.tr("wrapped.loading")
            } else if w.error.is_some() {
                i18n.tr("wrapped.loadFailed")
            } else if w.days < 7 {
                i18n.tr_count("wrapped.partial", w.days)
            } else {
                i18n.tr("wrapped.full")
            };
            ui.label(
                RichText::new(note)
                    .size(11.0)
                    .color(if w.error.is_some() { RED } else { MUTED }),
            );
            ui.add_space(8.0);

            // Stylized 4:5 Wrapped card preview matching wrapped.js
            egui::Frame::NONE
                .fill(Color32::from_rgb(0x16, 0x16, 0x30))
                .corner_radius(CornerRadius::same(14))
                .inner_margin(Margin::symmetric(16, 14))
                .stroke(Stroke::new(1.2_f32, Color32::from_rgba_unmultiplied(0xEF, 0x8A, 0x5F, 80)))
                .show(ui, |ui| {
                    ui.set_min_width(ui.available_width());
                    ui.horizontal(|ui| {
                        ui.label(heading_bold(i18n.tr("wrapped.title"), 16.0).color(Color32::WHITE));
                        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                            ui.label(
                                RichText::new(format_wrapped_date_range(&w, i18n))
                                    .size(11.0)
                                    .color(MUTED),
                            );
                        });
                    });
                    ui.add_space(8.0);

                    // Headline hours
                    ui.label(
                        heading_bold(format_wrapped_hours_key("wrapped.hours", w.hours_sum, i18n), 32.0)
                            .color(Color32::from_rgb(0xFF, 0xB8, 0x6B)),
                    );
                    ui.label(
                        RichText::new(format_wrapped_hours_key("wrapped.worked", w.hours_wall, i18n))
                            .size(12.0)
                            .color(INK),
                    );
                    let wait_str = format_wrapped_hours_key("wrapped.waited", w.wait_hours, i18n);
                    let sess_str = i18n.tr_vars(
                        "wrapped.sessions",
                        &[
                            ("n", &w.sessions.to_string()),
                            ("c", &w.sessions.to_string()),
                        ],
                    );
                    ui.label(
                        RichText::new(format!("{wait_str} · {sess_str}"))
                            .size(11.0)
                            .color(MUTED),
                    );
                    ui.add_space(10.0);

                    // 4 KPI tiles
                    let tiles = [
                        (w.prompts.to_string(), i18n.tr("wrapped.prompts")),
                        (w.edits.to_string(), i18n.tr("wrapped.edits")),
                        (w.cmds.to_string(), i18n.tr("wrapped.cmds")),
                        (format!("${:.1}", w.cost), i18n.tr("wrapped.cost")),
                    ];
                    ui.columns(4, |cols| {
                        for (idx, (val, lbl)) in tiles.iter().enumerate() {
                            cols[idx].push_id(idx, |ui| {
                                egui::Frame::NONE
                                    .fill(Color32::from_rgba_unmultiplied(255, 255, 255, 14))
                                    .corner_radius(CornerRadius::same(10))
                                    .inner_margin(Margin::symmetric(8, 8))
                                    .stroke(Stroke::new(
                                        1.0_f32,
                                        Color32::from_rgba_unmultiplied(255, 255, 255, 24),
                                    ))
                                    .show(ui, |ui| {
                                        ui.set_min_width(ui.available_width());
                                        ui.label(heading_bold(val, 15.0).color(Color32::WHITE));
                                        ui.label(RichText::new(lbl).size(10.0).color(MUTED));
                                    });
                            });
                        }
                    });

                    // Highlights
                    let highlights = format_wrapped_highlights(&w, state.streamer_mode, i18n);
                    if !highlights.is_empty() {
                        ui.add_space(10.0);
                        for (idx, line) in highlights.iter().enumerate() {
                            ui.push_id(idx, |ui| {
                                ui.label(RichText::new(line).size(11.5).color(INK));
                            });
                        }
                    }

                    ui.add_space(8.0);
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        ui.label(
                            RichText::new(i18n.tr("wrapped.footer"))
                                .size(10.0)
                                .color(MUTED),
                        );
                    });
                });

            if let Some(saved) = &w.last_saved_path {
                ui.add_space(6.0);
                ui.label(RichText::new(saved).size(10.5).color(GREEN));
            }

            ui.add_space(10.0);
            ui.horizontal(|ui| {
                let save_btn = egui::Button::new(
                    heading_bold(i18n.tr("ui.wrapped.save"), 12.0)
                        .color(Color32::from_rgb(0x2A, 0x13, 0x07)),
                )
                .fill(ACCENT)
                .corner_radius(CornerRadius::same(9));
                if ui.add_enabled(!w.loading, save_btn).clicked() {
                    state.push_action(UiAction::ExportWrappedPng {
                        copy_clipboard: false,
                    });
                }

                let copy_btn = egui::Button::new(
                    heading_bold(i18n.tr("ui.wrapped.copy"), 12.0).color(INK),
                )
                .fill(CARD_2)
                .stroke(Stroke::new(1.0_f32, LINE_2))
                .corner_radius(CornerRadius::same(9));
                if ui.add_enabled(!w.loading, copy_btn).clicked() {
                    state.push_action(UiAction::ExportWrappedPng {
                        copy_clipboard: true,
                    });
                }

                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    ui.label(RichText::new("1080 × 1350 PNG").size(10.5).color(MUTED));
                });
            });
        });

    if !open {
        state.active_drawer = ActiveDrawer::None;
    }
}

/// Floating timelapse recording progress pill with cancel action matching `timelapse.js`.
pub fn draw_timelapse_overlay(ctx: &egui::Context, state: &mut UiState, i18n: &I18nStore) {
    if !state.timelapse_recording {
        return;
    }
    egui::Area::new(egui::Id::new("station_timelapse_overlay"))
        .order(egui::Order::Foreground)
        .anchor(egui::Align2::CENTER_TOP, vec2(0.0, 68.0))
        .show(ctx, |ui| {
            egui::Frame::NONE
                .fill(CARD)
                .corner_radius(CornerRadius::same(12))
                .inner_margin(Margin::symmetric(14, 10))
                .stroke(Stroke::new(1.3_f32, ACCENT))
                .show(ui, |ui| {
                    ui.set_min_width(280.0);
                    ui.horizontal(|ui| {
                        let s = state.timelapse_elapsed_sec.min(TIMELAPSE_TOTAL_SEC);
                        let label = i18n.tr_vars(
                            "ui.clip.making",
                            &[
                                ("s", &s.to_string()),
                                ("total", &TIMELAPSE_TOTAL_SEC.to_string()),
                            ],
                        );
                        ui.label(heading_bold(label, 12.0).color(AMBER));
                        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                            let stop_btn = egui::Button::new(
                                heading_bold(i18n.tr("ui.clip.stop"), 11.0).color(Color32::WHITE),
                            )
                            .fill(RED)
                            .corner_radius(CornerRadius::same(7));
                            if ui.add(stop_btn).clicked() {
                                state.timelapse_recording = false;
                                state.timelapse_progress = 0.0;
                                state.push_action(UiAction::CancelTimelapseClip);
                            }
                        });
                    });
                    ui.add_space(6.0);
                    let (bar_r, _) =
                        ui.allocate_exact_size(vec2(280.0, 6.0), egui::Sense::hover());
                    ui.painter().rect_filled(bar_r, 3.0, BG);
                    let mut fill = bar_r;
                    fill.set_width(bar_r.width() * state.timelapse_progress.clamp(0.0, 1.0));
                    ui.painter().rect_filled(fill, 3.0, BLUE);
                });
        });
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    fn i18n(lang: &str) -> I18nStore {
        I18nStore::load_from_assets(
            &PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../assets"),
            lang,
        )
    }

    fn setup_ctx() -> egui::Context {
        let ctx = egui::Context::default();
        let mut defs = egui::FontDefinitions::default();
        let fallback = defs
            .families
            .get(&egui::FontFamily::Proportional)
            .cloned()
            .unwrap_or_default();
        defs.families
            .insert(egui::FontFamily::Name(crate::theme::FONT_BOLD.into()), fallback);
        ctx.set_fonts(defs);
        let _ = ctx.run(egui::RawInput::default(), |_| {});
        ctx
    }

    #[test]
    fn wrapped_modal_auto_fetches_summary_when_opened_unloaded() {
        let tr = i18n("en");
        let mut st = UiState::default();
        st.active_drawer = ActiveDrawer::Wrapped;
        assert!(!st.wrapped.loaded && !st.wrapped.loading);

        let ctx = setup_ctx();
        let _ = ctx.run(egui::RawInput::default(), |ctx| {
            draw_wrapped_modal(ctx, &mut st, &tr);
        });

        assert!(st.wrapped.loading);
        let actions = st.drain_actions();
        assert!(actions.iter().any(|a| matches!(a, UiAction::FetchWrapped)));
    }

    #[test]
    fn wrapped_modal_and_timelapse_overlay_render_cleanly_in_en_vi_th() {
        let mut st = UiState::with_demo_seed(
            station_3d::OfficeLayout::Pro,
            station_3d::BackdropKind::City,
            false,
        );
        st.active_drawer = ActiveDrawer::Wrapped;
        st.timelapse_recording = true;
        st.timelapse_progress = 0.45;
        st.timelapse_elapsed_sec = 7;

        for lang in ["en", "vi", "th"] {
            let tr = i18n(lang);
            let ctx = setup_ctx();
            let _ = ctx.run(egui::RawInput::default(), |ctx| {
                draw_wrapped_modal(ctx, &mut st, &tr);
                draw_timelapse_overlay(ctx, &mut st, &tr);
            });
        }
    }

    #[test]
    fn wrapped_highlights_resolves_localized_weekday_without_duplicating_iso_date() {
        let summary = WrappedSummaryUi {
            busiest_day: Some(("2026-11-26".to_owned(), 4, 12.5)), // 4 = Thu
            ..Default::default()
        };
        for (lang, expected_day) in [("en", "Thu"), ("vi", "Thứ Năm"), ("th", "พฤ.")] {
            let tr = i18n(lang);
            let lines = format_wrapped_highlights(&summary, false, &tr);
            assert_eq!(lines.len(), 1);
            assert!(
                lines[0].contains(expected_day),
                "expected localized weekday `{expected_day}` in `{}`",
                lines[0]
            );
            assert!(
                !lines[0].contains("2026-11-26"),
                "must not duplicate raw ISO date in `{}`",
                lines[0]
            );
        }
    }
}
