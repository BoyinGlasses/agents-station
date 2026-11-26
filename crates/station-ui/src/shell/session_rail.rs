use crate::i18n::I18nStore;
use crate::icons::{self, IconKind};
use crate::shell::ShellLayout;
use crate::state::{ActiveDrawer, SessionCard, UiState};
use crate::theme::{
    AMBER, BLUE, CARD, CARD_2, CARD_3, FAINT, GREEN, INK, LINE, LINE_2, MUTED, RED, SIDE,
    heading_bold,
};
use bevy_egui::egui::{
    self, Align, Color32, CornerRadius, FontFamily, FontId, Margin, Rect, RichText, Sense, Shape,
    Stroke, StrokeKind, pos2, vec2,
};

const ACCENT_SOFT: Color32 = Color32::from_rgba_premultiplied(0x1F, 0x12, 0x0C, 33);
const ACCENT_LINE: Color32 = Color32::from_rgba_premultiplied(0x90, 0x53, 0x39, 153);
const GREEN_SOFT: Color32 = Color32::from_rgba_premultiplied(0x0B, 0x26, 0x17, 28);
const GREEN_INK: Color32 = Color32::from_rgb(0x8F, 0xED, 0xB8);
const AMBER_SOFT: Color32 = Color32::from_rgba_premultiplied(0x2E, 0x22, 0x0D, 31);
const WORK_DOING: Color32 = Color32::from_rgb(0xC8, 0xD0, 0xDD);
const BADGE_BG: Color32 = Color32::from_rgba_premultiplied(0x18, 0x18, 0x18, 16);

fn agent_accent(agent: &str) -> Color32 {
    match agent.to_ascii_lowercase().as_str() {
        "claude" => Color32::from_rgb(0xEF, 0x8A, 0x5F),
        "codex" => BLUE,
        "antigravity" | "gemini" => GREEN,
        "opencode" => Color32::from_rgb(0xB7, 0x94, 0xF4),
        "zcode" => AMBER,
        _ => Color32::from_rgb(0x7C, 0xB4, 0xFF),
    }
}

fn status_dot_color(status: &str) -> Color32 {
    match status {
        "work" => GREEN,
        "wait" => AMBER,
        _ => FAINT,
    }
}

pub fn draw_session_rail(
    ctx: &egui::Context,
    state: &mut UiState,
    i18n: &I18nStore,
    layout: &ShellLayout,
) {
    egui::SidePanel::left("station_shell_session_rail")
        .exact_width(layout.left_width)
        .resizable(false)
        .frame(egui::Frame::NONE.fill(SIDE).inner_margin(Margin::ZERO))
        .show(ctx, |ui| {
            let panel_rect = ui.max_rect();
            ui.painter().line_segment(
                [panel_rect.right_top(), panel_rect.right_bottom()],
                Stroke::new(1.0_f32, LINE),
            );

            // Header section (.lp-head)
            egui::Frame::NONE
                .inner_margin(Margin {
                    left: 14,
                    right: 14,
                    top: 14,
                    bottom: 8,
                })
                .show(ui, |ui| {
                    // Title + live connection indicator
                    ui.horizontal(|ui| {
                        ui.label(heading_bold(i18n.tr("ui.sessionRail.title"), 16.0));
                        ui.with_layout(egui::Layout::right_to_left(Align::Center), |ui| {
                            let (conn_col, conn_key) = if state.connected {
                                (GREEN, "activity.live.on")
                            } else {
                                (RED, "activity.live.off")
                            };
                            ui.label(RichText::new(i18n.tr(conn_key)).size(11.0).color(if state.connected {
                                MUTED
                            } else {
                                RED
                            }));
                            let (dot_r, _) = ui.allocate_exact_size(vec2(8.0, 8.0), Sense::hover());
                            ui.painter().circle_filled(dot_r.center(), 3.2, conn_col);
                        });
                    });

                    // Active session summary (.lp-sub)
                    ui.add_space(2.0);
                    let count_text = if state.sessions.is_empty() {
                        i18n.tr("activity.noSessions")
                    } else {
                        i18n.tr_count("activity.sessions", state.sessions.len())
                    };
                    ui.label(RichText::new(count_text).size(11.5).color(MUTED));

                    ui.add_space(8.0);

                    // 3 status filter tiles (work / wait / rest)
                    let work_n = state.working_count();
                    let wait_n = state.waiting_count();
                    let rest_n = state.resting_count();
                    ui.columns(3, |cols| {
                        draw_status_tile(
                            &mut cols[0],
                            "work",
                            work_n,
                            i18n.tr("ui.sessionRail.work"),
                            GREEN,
                            GREEN_SOFT,
                            state,
                        );
                        draw_status_tile(
                            &mut cols[1],
                            "wait",
                            wait_n,
                            i18n.tr("ui.sessionRail.wait"),
                            AMBER,
                            AMBER_SOFT,
                            state,
                        );
                        draw_status_tile(
                            &mut cols[2],
                            "rest",
                            rest_n,
                            i18n.tr("ui.sessionRail.rest"),
                            FAINT,
                            Color32::TRANSPARENT,
                            state,
                        );
                    });

                    ui.add_space(8.0);

                    // Search input (.search)
                    egui::Frame::NONE
                        .fill(SIDE)
                        .corner_radius(CornerRadius::same(10))
                        .inner_margin(Margin::symmetric(10, 6))
                        .stroke(Stroke::new(1.0_f32, LINE_2))
                        .show(ui, |ui| {
                            ui.horizontal(|ui| {
                                ui.spacing_mut().item_spacing.x = 7.0;
                                let (ic_r, _) =
                                    ui.allocate_exact_size(vec2(14.0, 14.0), Sense::hover());
                                icons::paint_icon(ui.painter(), ic_r, IconKind::Search, FAINT);
                                ui.add(
                                    egui::TextEdit::singleline(&mut state.session_search)
                                        .frame(false)
                                        .desired_width(f32::INFINITY)
                                        .font(FontId::new(12.0, FontFamily::Proportional))
                                        .hint_text(
                                            RichText::new(i18n.tr("ui.sessionRail.search"))
                                                .color(FAINT),
                                        ),
                                );
                            });
                        });
                });

            // Scrollable session list (.sess-list)
            egui::ScrollArea::vertical()
                .auto_shrink([false, false])
                .show(ui, |ui| {
                    egui::Frame::NONE
                        .inner_margin(Margin {
                            left: 8,
                            right: 8,
                            top: 2,
                            bottom: 12,
                        })
                        .show(ui, |ui| {
                            let query = state.session_search.trim().to_ascii_lowercase();
                            let filter = state.session_filter_status.clone();
                            let sessions = state.sessions.clone();

                            let matching: Vec<SessionCard> = sessions
                                .into_iter()
                                .filter(|s| {
                                    if let Some(st) = &filter
                                        && s.status != *st
                                    {
                                        return false;
                                    }
                                    if !query.is_empty() {
                                        let hay = format!(
                                            "{} {} {} {}",
                                            s.project, s.agent, s.id, s.last_prompt
                                        )
                                        .to_ascii_lowercase();
                                        if !hay.contains(&query) {
                                            return false;
                                        }
                                    }
                                    true
                                })
                                .collect();

                            if matching.is_empty() {
                                ui.add_space(10.0);
                                ui.horizontal(|ui| {
                                    ui.add_space(8.0);
                                    ui.label(
                                        RichText::new(i18n.tr("activity.noSessions"))
                                            .size(12.0)
                                            .color(MUTED),
                                    );
                                });
                                return;
                            }

                            for (group_status, title_key, is_wait) in [
                                ("wait", "activity.group.wait", true),
                                ("work", "activity.group.work", false),
                                ("rest", "activity.group.rest", false),
                            ] {
                                let group_items: Vec<&SessionCard> = matching
                                    .iter()
                                    .filter(|s| s.status == group_status)
                                    .collect();
                                if group_items.is_empty() {
                                    continue;
                                }

                                // Group header (.grp-btn)
                                ui.add_space(6.0);
                                ui.horizontal(|ui| {
                                    ui.add_space(6.0);
                                    ui.label(
                                        heading_bold(i18n.tr(title_key).to_uppercase(), 10.0)
                                            .color(if is_wait { AMBER } else { FAINT }),
                                    );
                                    egui::Frame::NONE
                                        .fill(CARD)
                                        .corner_radius(CornerRadius::same(99))
                                        .inner_margin(Margin::symmetric(6, 1))
                                        .show(ui, |ui| {
                                            ui.label(
                                                heading_bold(group_items.len().to_string(), 10.0)
                                                    .color(MUTED),
                                            );
                                        });
                                });
                                ui.add_space(3.0);

                                for sess in group_items {
                                    draw_session_row(ui, sess, state, i18n);
                                    if !sess.subagents.is_empty() {
                                        draw_subagent_rows(ui, sess, state);
                                    }
                                    ui.add_space(2.0);
                                }
                            }
                        });
                });
        });
}

#[allow(clippy::too_many_arguments)]
fn draw_status_tile(
    ui: &mut egui::Ui,
    status_key: &str,
    count: usize,
    label: String,
    dot_col: Color32,
    halo_col: Color32,
    state: &mut UiState,
) {
    let active = state.session_filter_status.as_deref() == Some(status_key);
    let fill = if active { ACCENT_SOFT } else { CARD };
    let border = if active { ACCENT_LINE } else { LINE };

    let resp = egui::Frame::NONE
        .fill(fill)
        .corner_radius(CornerRadius::same(11))
        .inner_margin(Margin::symmetric(9, 7))
        .stroke(Stroke::new(1.0_f32, border))
        .show(ui, |ui| {
            ui.set_width(ui.available_width());
            ui.horizontal(|ui| {
                ui.spacing_mut().item_spacing.x = 6.0;
                let (dot_r, _) = ui.allocate_exact_size(vec2(10.0, 10.0), Sense::hover());
                if halo_col != Color32::TRANSPARENT {
                    ui.painter().circle_filled(dot_r.center(), 5.5, halo_col);
                }
                ui.painter().circle_filled(dot_r.center(), 3.6, dot_col);
                let num_col = if status_key == "wait" && count > 0 {
                    AMBER
                } else {
                    INK
                };
                ui.label(heading_bold(count.to_string(), 14.5).color(num_col));
            });
            ui.label(RichText::new(label).size(10.5).color(MUTED));
        });

    if resp.response.interact(Sense::click()).clicked() {
        if active {
            state.session_filter_status = None;
        } else {
            state.session_filter_status = Some(status_key.to_owned());
        }
    }
}

fn draw_session_row(
    ui: &mut egui::Ui,
    sess: &SessionCard,
    state: &mut UiState,
    i18n: &I18nStore,
) {
    let selected = state.selected_session.as_deref() == Some(sess.id.as_str());
    let is_wait = sess.status == "wait";
    let is_work = sess.status == "work";

    let (fill, stroke) = if selected {
        (GREEN_SOFT, Stroke::new(1.0_f32, GREEN))
    } else if is_wait {
        (
            AMBER_SOFT,
            Stroke::new(1.0_f32, Color32::from_rgba_unmultiplied(0xF2, 0xB5, 0x44, 115)),
        )
    } else {
        (Color32::TRANSPARENT, Stroke::new(1.0_f32, Color32::TRANSPARENT))
    };

    let row_resp = egui::Frame::NONE
        .fill(fill)
        .corner_radius(CornerRadius::same(12))
        .inner_margin(Margin::symmetric(8, 7))
        .stroke(stroke)
        .show(ui, |ui| {
            ui.set_width(ui.available_width());
            ui.horizontal(|ui| {
                ui.spacing_mut().item_spacing.x = 9.0;

                // 34x34 circular avatar with agent ring + status dot
                draw_agent_avatar(ui, &sess.agent, &sess.status);

                // Middle info stack
                ui.vertical(|ui| {
                    ui.spacing_mut().item_spacing.y = 2.0;
                    ui.horizontal(|ui| {
                        ui.spacing_mut().item_spacing.x = 5.0;
                        ui.label(
                            heading_bold(clip_chars(&sess.project, 15), 12.5)
                                .color(INK),
                        );
                        egui::Frame::NONE
                            .fill(BADGE_BG)
                            .corner_radius(CornerRadius::same(6))
                            .inner_margin(Margin::symmetric(5, 1))
                            .stroke(Stroke::new(1.0_f32, LINE))
                            .show(ui, |ui| {
                                ui.label(
                                    RichText::new(clip_chars(&sess.agent, 11))
                                        .size(9.5)
                                        .color(MUTED),
                                );
                            });

                        // Right-aligned age/cost meta
                        ui.with_layout(egui::Layout::right_to_left(Align::Center), |ui| {
                            let meta = if !sess.age_label.is_empty() {
                                sess.age_label.clone()
                            } else if sess.cost_usd > 0.0 {
                                format!("${:.2}", sess.cost_usd)
                            } else {
                                String::new()
                            };
                            if !meta.is_empty() {
                                ui.label(
                                    RichText::new(meta)
                                        .font(FontId::new(10.5, FontFamily::Monospace))
                                        .color(if is_work { GREEN_INK } else { MUTED }),
                                );
                            }
                        });
                    });

                    // Doing line
                    let doing = if state.streamer_mode {
                        i18n.tr("station.streamerRedacted")
                    } else if is_wait {
                        i18n.tr("activity.doing.wait")
                    } else if let Some(tool) = &sess.tool {
                        clip_chars(tool, 28)
                    } else if is_work {
                        i18n.tr("activity.doing.working")
                    } else if !sess.age_label.is_empty() {
                        i18n.tr_vars("activity.doing.resting", &[("time", sess.age_label.as_str())])
                    } else {
                        i18n.tr("activity.doing.justDone")
                    };
                    let doing_col = if is_wait {
                        AMBER
                    } else if is_work {
                        WORK_DOING
                    } else {
                        MUTED
                    };
                    ui.label(RichText::new(doing).size(11.0).color(doing_col));
                });
            });

            // Optional 3px context meter bar (.ri-ctx)
            if sess.ctx_pct > 0.01 {
                ui.add_space(4.0);
                ui.horizontal(|ui| {
                    ui.add_space(43.0);
                    let w = (ui.available_width() - 4.0).max(20.0);
                    let (bar_r, _) = ui.allocate_exact_size(vec2(w, 3.0), Sense::hover());
                    ui.painter()
                        .rect_filled(bar_r, CornerRadius::same(2), CARD_3);
                    let col = if sess.ctx_pct >= 0.85 {
                        RED
                    } else if sess.ctx_pct >= 0.65 {
                        AMBER
                    } else {
                        BLUE
                    };
                    let fill_r = Rect::from_min_size(
                        bar_r.min,
                        vec2(bar_r.width() * sess.ctx_pct.clamp(0.0, 1.0), 3.0),
                    );
                    ui.painter()
                        .rect_filled(fill_r, CornerRadius::same(2), col);
                });
            }
        });

    let interact = row_resp.response.interact(Sense::click());
    if interact.hovered() && !selected && !is_wait {
        ui.painter().rect_stroke(
            row_resp.response.rect,
            CornerRadius::same(12),
            Stroke::new(1.0_f32, LINE),
            StrokeKind::Inside,
        );
    }
    if interact.clicked() {
        state.selected_session = Some(sess.id.clone());
        state.active_drawer = ActiveDrawer::Session;
    }
}

fn draw_agent_avatar(ui: &mut egui::Ui, agent: &str, status: &str) {
    let (rect, _) = ui.allocate_exact_size(vec2(34.0, 34.0), Sense::hover());
    let ring = agent_accent(agent);
    let bg = Color32::from_rgba_unmultiplied(ring.r(), ring.g(), ring.b(), 52);
    let dim = if status == "rest" { 0.65 } else { 1.0 };
    let ring_col = ring.linear_multiply(dim);

    ui.painter().circle_filled(rect.center(), 16.5, bg);
    ui.painter()
        .circle_stroke(rect.center(), 16.0, Stroke::new(1.6_f32, ring_col));

    let initial = agent
        .chars()
        .next()
        .unwrap_or('A')
        .to_ascii_uppercase()
        .to_string();
    ui.painter().text(
        rect.center(),
        egui::Align2::CENTER_CENTER,
        initial,
        FontId::new(12.5, FontFamily::Name(crate::theme::FONT_BOLD.into())),
        INK,
    );

    // Bottom-right status dot
    let dot_center = pos2(rect.right() - 4.0, rect.bottom() - 4.0);
    ui.painter().circle_filled(dot_center, 6.0, SIDE);
    ui.painter()
        .circle_filled(dot_center, 4.2, status_dot_color(status));
}

fn draw_subagent_rows(ui: &mut egui::Ui, sess: &SessionCard, state: &mut UiState) {
    let start_y = ui.cursor().top();
    let x_rail = ui.min_rect().left() + 26.0;

    egui::Frame::NONE
        .inner_margin(Margin {
            left: 34,
            right: 4,
            top: 2,
            bottom: 4,
        })
        .show(ui, |ui| {
            for sub in &sess.subagents {
                let sub_resp = egui::Frame::NONE
                    .fill(Color32::TRANSPARENT)
                    .corner_radius(CornerRadius::same(9))
                    .inner_margin(Margin::symmetric(7, 5))
                    .show(ui, |ui| {
                        ui.set_width(ui.available_width());
                        ui.horizontal(|ui| {
                            ui.spacing_mut().item_spacing.x = 7.0;
                            let (ic_r, _) =
                                ui.allocate_exact_size(vec2(13.0, 13.0), Sense::hover());
                            icons::paint_icon(ui.painter(), ic_r, IconKind::Users, BLUE);
                            ui.vertical(|ui| {
                                ui.spacing_mut().item_spacing.y = 1.0;
                                ui.label(heading_bold(clip_chars(&sub.label, 22), 11.5));
                                ui.label(
                                    RichText::new(clip_chars(&sub.doing, 26))
                                        .size(10.5)
                                        .color(MUTED),
                                );
                            });
                        });
                    });
                let click = sub_resp.response.interact(Sense::click());
                if click.hovered() {
                    ui.painter()
                        .rect_filled(sub_resp.response.rect, CornerRadius::same(9), CARD_2);
                }
                if click.clicked() {
                    state.selected_session = Some(sess.id.clone());
                    state.active_drawer = ActiveDrawer::Session;
                }
            }
        });

    let end_y = (ui.cursor().top() - 4.0).max(start_y + 6.0);
    let dashed = Shape::dashed_line(
        &[pos2(x_rail, start_y + 2.0), pos2(x_rail, end_y)],
        Stroke::new(1.0_f32, LINE_2),
        3.0,
        3.0,
    );
    ui.painter().extend(dashed);
}

/// Top-of-stage horizontal session chips shown when viewport width < 1100px collapses `SessionRail`.
pub fn draw_stage_session_chips(
    ctx: &egui::Context,
    stage_rect: Rect,
    state: &mut UiState,
    _i18n: &I18nStore,
) {
    if state.sessions.is_empty() || stage_rect.width() < 80.0 {
        return;
    }
    egui::Area::new(egui::Id::new("station_stage_session_chips"))
        .order(egui::Order::Middle)
        .fixed_pos(pos2(stage_rect.left() + 10.0, stage_rect.top() + 10.0))
        .show(ctx, |ui| {
            ui.set_max_width((stage_rect.width() - 20.0).max(40.0));
            egui::ScrollArea::horizontal()
                .id_salt("stage_chips_scroll")
                .show(ui, |ui| {
                    ui.horizontal(|ui| {
                        ui.spacing_mut().item_spacing.x = 7.0;
                        let sessions = state.sessions.clone();
                        for sess in sessions {
                            let selected = state.selected_session.as_deref() == Some(&sess.id);
                            let border = if selected {
                                GREEN
                            } else if sess.status == "wait" {
                                AMBER
                            } else {
                                LINE_2
                            };
                            let fill = if selected {
                                GREEN_SOFT
                            } else {
                                Color32::from_rgba_unmultiplied(0x11, 0x17, 0x20, 224)
                            };
                            let resp = egui::Frame::NONE
                                .fill(fill)
                                .corner_radius(CornerRadius::same(99))
                                .inner_margin(Margin::symmetric(10, 6))
                                .stroke(Stroke::new(1.0_f32, border))
                                .show(ui, |ui| {
                                    ui.horizontal(|ui| {
                                        ui.spacing_mut().item_spacing.x = 6.0;
                                        let (d, _) = ui
                                            .allocate_exact_size(vec2(8.0, 8.0), Sense::hover());
                                        ui.painter().circle_filled(
                                            d.center(),
                                            3.8,
                                            status_dot_color(&sess.status),
                                        );
                                        ui.label(heading_bold(clip_chars(&sess.project, 16), 11.5));
                                    });
                                });
                            if resp.response.interact(Sense::click()).clicked() {
                                state.selected_session = Some(sess.id.clone());
                                state.active_drawer = ActiveDrawer::Session;
                            }
                        }
                    });
                });
        });
}

fn clip_chars(s: &str, max_chars: usize) -> String {
    let mut chars = s.chars();
    let collected: String = chars.by_ref().take(max_chars).collect();
    if chars.next().is_some() {
        format!("{collected}…")
    } else {
        collected
    }
}
