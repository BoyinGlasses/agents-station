use crate::i18n::I18nStore;
use crate::icons::{self, IconKind};
use crate::lite_view::{
    LiteShortcutOutcome, group_sessions, handle_lite_keyboard, ordered_session_ids,
};
use crate::state::{
    ActiveDrawer, CacheWarmth, PendingPermCard, SessionCard, UiAction, UiState, ViewMode,
};
use crate::theme::{
    ACCENT, ACCENT_HI, AMBER, BAR, BG, BLUE, CARD, CARD_2, CARD_3, FAINT, GREEN, INK, LINE, LINE_2,
    LOGO, MUTED, RED, SIDE, heading_bold,
};
use bevy_egui::egui::{
    self, Align, Color32, CornerRadius, FontFamily, FontId, Key, Margin, Rect, RichText, Sense,
    Stroke, StrokeKind, vec2,
};

const AMBER_SOFT: Color32 = Color32::from_rgba_premultiplied(0x2E, 0x22, 0x0D, 34);
const GREEN_SOFT: Color32 = Color32::from_rgba_premultiplied(0x0B, 0x26, 0x17, 28);
const RED_SOFT: Color32 = Color32::from_rgba_premultiplied(0x31, 0x12, 0x12, 31);
const CODE_BG: Color32 = Color32::from_rgb(0x0A, 0x0E, 0x14);
const ON_GREEN: Color32 = Color32::from_rgb(0x07, 0x13, 0x0C);

fn level_color(ratio: f32) -> Color32 {
    if ratio >= 0.88 {
        RED
    } else if ratio >= 0.70 {
        AMBER
    } else {
        GREEN
    }
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

fn strip_coffee(raw: &str) -> &str {
    raw.trim_start_matches('☕').trim_start()
}

/// Renders the standalone `420x640` compact `/panel` window matching
/// `my-station/src/panel/{main,session-card,quota,mac,keys}.js`.
pub fn render_compact_panel_window(ctx: &egui::Context, state: &mut UiState, i18n: &I18nStore) {
    let sessions_snapshot = state.sessions.clone();
    let perms_snapshot = state.pending_perms.clone();
    let search_ref = state.lite_search.clone();

    let grouped = group_sessions(&sessions_snapshot, &perms_snapshot, None, &search_ref);
    let ordered_ids = ordered_session_ids(&grouped);

    // 1. Slim header with brand, live status summary, Plan quota bar, PC/RAM bar, Refresh & Expand buttons
    egui::TopBottomPanel::top("station_compact_panel_head")
        .frame(
            egui::Frame::NONE
                .fill(BAR)
                .inner_margin(Margin::symmetric(12, 9)),
        )
        .show(ctx, |ui| {
            let panel_rect = ui.max_rect();
            ui.painter().line_segment(
                [panel_rect.left_bottom(), panel_rect.right_bottom()],
                Stroke::new(1.0_f32, LINE),
            );

            ui.horizontal(|ui| {
                ui.spacing_mut().item_spacing.x = 6.0;
                let (logo_r, _) = ui.allocate_exact_size(vec2(18.0, 18.0), Sense::hover());
                icons::paint_icon(ui.painter(), logo_r, IconKind::Logo, LOGO);
                ui.label(heading_bold(i18n.tr("station.brand"), 13.5));

                ui.with_layout(egui::Layout::right_to_left(Align::Center), |ui| {
                    ui.spacing_mut().item_spacing.x = 5.0;

                    // Expand to Full (3D)
                    let expand_btn = egui::Button::new(
                        heading_bold(i18n.tr("panel.open3d"), 10.5).color(INK),
                    )
                    .fill(CARD_2)
                    .stroke(Stroke::new(1.0_f32, LINE_2))
                    .corner_radius(CornerRadius::same(7))
                    .min_size(vec2(0.0, 24.0));
                    if ui.add(expand_btn).clicked() {
                        state.view_mode = ViewMode::Mode3D;
                        state.push_action(UiAction::SetViewMode(ViewMode::Mode3D));
                    }

                    // Refresh telemetry
                    let refresh_btn = egui::Button::new(
                        RichText::new(i18n.tr("ui.macDrawer.refresh"))
                            .size(10.5)
                            .color(MUTED),
                    )
                    .fill(CARD)
                    .stroke(Stroke::new(1.0_f32, LINE))
                    .corner_radius(CornerRadius::same(7))
                    .min_size(vec2(0.0, 24.0));
                    if ui.add(refresh_btn).clicked() {
                        state.push_action(UiAction::RefreshTelemetry);
                    }
                });
            });

            ui.add_space(4.0);
            let (sum_text, sum_col) = build_panel_summary(state, &grouped, i18n);
            ui.label(heading_bold(sum_text, 11.0).color(sum_col));

            ui.add_space(6.0);
            // Slim Plan quota + PC/RAM bars side-by-side
            ui.columns(2, |cols| {
                draw_quota_bar_pill(&mut cols[0], state, i18n);
                draw_ram_bar_pill(&mut cols[1], state, i18n);
            });
        });

    // 2. Slim footer with Expand-to-2D/3D and keyboard hints
    egui::TopBottomPanel::bottom("station_compact_panel_foot")
        .frame(
            egui::Frame::NONE
                .fill(BAR)
                .inner_margin(Margin::symmetric(12, 8)),
        )
        .show(ctx, |ui| {
            let r = ui.max_rect();
            ui.painter().line_segment(
                [r.left_top(), r.right_top()],
                Stroke::new(1.0_f32, LINE),
            );
            ui.columns(2, |cols| {
                let btn_3d = egui::Button::new(heading_bold(i18n.tr("panel.open3d"), 11.0))
                    .fill(CARD_2)
                    .stroke(Stroke::new(1.0_f32, LINE_2))
                    .corner_radius(CornerRadius::same(8))
                    .min_size(vec2(cols[0].available_width(), 26.0));
                if cols[0].add(btn_3d).clicked() {
                    state.view_mode = ViewMode::Mode3D;
                    state.push_action(UiAction::SetViewMode(ViewMode::Mode3D));
                }

                let btn_2d = egui::Button::new(heading_bold(i18n.tr("panel.open2d"), 11.0))
                    .fill(CARD_2)
                    .stroke(Stroke::new(1.0_f32, LINE_2))
                    .corner_radius(CornerRadius::same(8))
                    .min_size(vec2(cols[1].available_width(), 26.0));
                if cols[1].add(btn_2d).clicked() {
                    state.view_mode = ViewMode::Lite2D;
                    state.push_action(UiAction::SetViewMode(ViewMode::Lite2D));
                }
            });
            ui.add_space(4.0);
            ui.label(RichText::new(i18n.tr("panel.keys")).size(9.5).color(FAINT));
        });

    // 3. Scrollable compact body
    let mut input_focused = false;
    egui::CentralPanel::default()
        .frame(egui::Frame::NONE.fill(BG).inner_margin(Margin::symmetric(12, 8)))
        .show(ctx, |ui| {
            // Compact search bar (`/` focuses, `Esc` clears & blurs)
            egui::Frame::NONE
                .fill(SIDE)
                .corner_radius(CornerRadius::same(8))
                .inner_margin(Margin::symmetric(8, 5))
                .stroke(Stroke::new(1.0_f32, LINE_2))
                .show(ui, |ui| {
                    ui.horizontal(|ui| {
                        ui.spacing_mut().item_spacing.x = 6.0;
                        let (ic_r, _) = ui.allocate_exact_size(vec2(13.0, 13.0), Sense::hover());
                        icons::paint_icon(ui.painter(), ic_r, IconKind::Search, FAINT);
                        let resp = ui.add(
                            egui::TextEdit::singleline(&mut state.lite_search)
                                .id(egui::Id::new("panel_compact_search_input"))
                                .frame(false)
                                .desired_width(f32::INFINITY)
                                .font(FontId::new(11.5, FontFamily::Proportional))
                                .hint_text(RichText::new(i18n.tr("lite.search")).color(FAINT)),
                        );
                        if state.lite_focus_search_req {
                            state.lite_focus_search_req = false;
                            resp.request_focus();
                        }
                        if resp.has_focus() && ui.input(|i| i.key_pressed(Key::Escape)) {
                            state.lite_search.clear();
                            resp.surrender_focus();
                        }
                        if resp.has_focus() || resp.lost_focus() {
                            input_focused = true;
                        }
                    });
                });

            ui.add_space(8.0);

            egui::ScrollArea::vertical()
                .id_salt("station_compact_panel_scroll")
                .auto_shrink([false, false])
                .show(ui, |ui| {
                    // Section A: Needs you (pending permissions)
                    if !perms_snapshot.is_empty() {
                        let n_s = perms_snapshot.len().to_string();
                        let h = i18n
                            .tr_vars("panel.needsYou", &[("n", n_s.as_str())])
                            .to_uppercase();
                        ui.label(heading_bold(h, 10.0).color(AMBER));
                        ui.add_space(5.0);
                        for perm in &perms_snapshot {
                            draw_compact_perm_card(ui, perm, state, i18n);
                            ui.add_space(6.0);
                        }
                        ui.add_space(4.0);
                    }

                    // Section B: Compact session cards (ranked wait -> work -> rest)
                    ui.label(heading_bold(i18n.tr("panel.sessions").to_uppercase(), 10.0).color(FAINT));
                    ui.add_space(5.0);

                    if grouped.total() == 0 {
                        let msg = if sessions_snapshot.is_empty() {
                            if state.connected {
                                i18n.tr("panel.noSessions")
                            } else {
                                i18n.tr("panel.connecting")
                            }
                        } else {
                            i18n.tr_vars("lite.none", &[("q", search_ref.as_str())])
                        };
                        ui.label(RichText::new(msg).size(11.5).color(MUTED));
                    } else {
                        for (st, list) in [
                            ("wait", &grouped.wait),
                            ("work", &grouped.work),
                            ("rest", &grouped.rest),
                        ] {
                            for sess in list {
                                let steer_focused =
                                    draw_compact_session_card(ui, sess, st, state, i18n);
                                if steer_focused {
                                    input_focused = true;
                                }
                                ui.add_space(5.0);
                            }
                        }
                    }

                    ui.add_space(8.0);
                    // Section C: Collapsible PC / RAM meters (matching `mac.js`)
                    draw_compact_mac_section(ui, state, i18n);
                });
        });

    // Keyboard shortcuts (`Up`/`Down`/`Enter`/`Y`/`N`/`/`) strictly blocked while typing!
    let outcome = handle_lite_keyboard(ctx, state, &ordered_ids, input_focused);
    if let LiteShortcutOutcome::OpenSession(sid) = outcome {
        // In standalone compact panel, Enter opens quick prompt steer on the selected card
        // instead of opening a wide side drawer over the 420px window.
        state.active_drawer = ActiveDrawer::None;
        state.panel_steer_session = Some(sid);
    }
}

fn build_panel_summary(
    state: &UiState,
    grouped: &crate::lite_view::GroupedSessions<'_>,
    i18n: &I18nStore,
) -> (String, Color32) {
    if !state.connected {
        return (i18n.tr("panel.connecting"), MUTED);
    }
    let work_n = grouped.work.len();
    let wait_n = grouped.wait.len();
    let mut parts = Vec::new();
    if work_n > 0 {
        let s = work_n.to_string();
        parts.push(i18n.tr_vars("panel.working", &[("n", s.as_str())]));
    }
    if wait_n > 0 {
        parts.push(i18n.tr_count("panel.waiting", wait_n));
    }
    if parts.is_empty() {
        (i18n.tr("panel.quiet"), MUTED)
    } else {
        let col = if wait_n > 0 { AMBER } else { GREEN };
        (parts.join(" · "), col)
    }
}

fn draw_quota_bar_pill(ui: &mut egui::Ui, state: &mut UiState, i18n: &I18nStore) {
    let ratio = (state.burn.today_usd as f32 / 20.0).clamp(0.0, 1.0);
    let col = level_color(ratio);
    let val = if state.burn.today_usd > 0.0 {
        format!("${:.2}", state.burn.today_usd)
    } else {
        i18n.tr("ui.topBar.reading")
    };
    let resp = egui::Frame::NONE
        .fill(CARD)
        .corner_radius(CornerRadius::same(8))
        .inner_margin(Margin::symmetric(8, 5))
        .stroke(Stroke::new(1.0_f32, LINE))
        .show(ui, |ui| {
            ui.set_width(ui.available_width());
            ui.horizontal(|ui| {
                ui.label(heading_bold(i18n.tr("ui.topBar.plan"), 10.0).color(MUTED));
                ui.with_layout(egui::Layout::right_to_left(Align::Center), |ui| {
                    ui.label(
                        RichText::new(val)
                            .font(FontId::new(10.0, FontFamily::Monospace))
                            .color(col),
                    );
                });
            });
            ui.add_space(3.0);
            draw_thin_bar(ui, ratio, col);
        })
        .response
        .interact(Sense::click());
    if resp.clicked() {
        state.toggle_drawer(ActiveDrawer::Burn);
    }
}

fn draw_ram_bar_pill(ui: &mut egui::Ui, state: &mut UiState, i18n: &I18nStore) {
    let total = state.sys.mem_total_gb;
    let ratio = if total > 0.0 {
        (state.sys.mem_used_gb / total).clamp(0.0, 1.0)
    } else {
        0.0
    };
    let col = level_color(ratio);
    let val = if total > 0.0 {
        format!("{}%", (ratio * 100.0).round() as u32)
    } else {
        i18n.tr("ui.topBar.measuring")
    };
    let resp = egui::Frame::NONE
        .fill(CARD)
        .corner_radius(CornerRadius::same(8))
        .inner_margin(Margin::symmetric(8, 5))
        .stroke(Stroke::new(1.0_f32, LINE))
        .show(ui, |ui| {
            ui.set_width(ui.available_width());
            ui.horizontal(|ui| {
                ui.label(heading_bold(i18n.tr("panel.mac.title"), 10.0).color(MUTED));
                ui.with_layout(egui::Layout::right_to_left(Align::Center), |ui| {
                    ui.label(
                        RichText::new(val)
                            .font(FontId::new(10.0, FontFamily::Monospace))
                            .color(col),
                    );
                });
            });
            ui.add_space(3.0);
            draw_thin_bar(ui, ratio, col);
        })
        .response
        .interact(Sense::click());
    if resp.clicked() {
        state.panel_mac_open = !state.panel_mac_open;
    }
}

fn draw_thin_bar(ui: &mut egui::Ui, ratio: f32, col: Color32) {
    let w = ui.available_width().max(20.0);
    let (r, _) = ui.allocate_exact_size(vec2(w, 4.0), Sense::hover());
    ui.painter().rect_filled(r, CornerRadius::same(2), CARD_3);
    let clamped = ratio.clamp(0.0, 1.0);
    if clamped > 0.0 {
        ui.painter().rect_filled(
            Rect::from_min_size(r.min, vec2((r.width() * clamped).max(3.0), 4.0)),
            CornerRadius::same(2),
            col,
        );
    }
}

fn draw_compact_perm_card(
    ui: &mut egui::Ui,
    perm: &PendingPermCard,
    state: &mut UiState,
    i18n: &I18nStore,
) {
    egui::Frame::NONE
        .fill(AMBER_SOFT)
        .corner_radius(CornerRadius::same(9))
        .inner_margin(Margin::symmetric(10, 8))
        .stroke(Stroke::new(
            1.0_f32,
            Color32::from_rgba_unmultiplied(0xF2, 0xB5, 0x44, 115),
        ))
        .show(ui, |ui| {
            ui.set_width(ui.available_width());
            ui.horizontal(|ui| {
                ui.label(heading_bold(&perm.agent, 11.5).color(INK));
                ui.label(RichText::new(&perm.tool).size(11.0).color(AMBER));
            });
            if !perm.summary.is_empty() && !state.streamer_mode {
                ui.add_space(4.0);
                egui::Frame::NONE
                    .fill(CODE_BG)
                    .corner_radius(CornerRadius::same(6))
                    .inner_margin(Margin::symmetric(7, 4))
                    .show(ui, |ui| {
                        ui.set_width(ui.available_width());
                        ui.label(
                            RichText::new(clip_chars(&perm.summary, 110))
                                .font(FontId::new(10.0, FontFamily::Monospace))
                                .color(INK),
                        );
                    });
            }
            ui.add_space(5.0);
            ui.horizontal(|ui| {
                ui.spacing_mut().item_spacing.x = 6.0;
                let ok_btn = egui::Button::new(
                    heading_bold(format!("{} (Y)", i18n.tr("perm.allow")), 11.0).color(ON_GREEN),
                )
                .fill(GREEN)
                .stroke(Stroke::NONE)
                .corner_radius(CornerRadius::same(7))
                .min_size(vec2(78.0, 24.0));
                if ui.add(ok_btn).clicked() {
                    let id = perm.id.clone();
                    state.pending_perms.retain(|p| p.id != id);
                    state.push_action(UiAction::DecidePermission {
                        id,
                        decision: "allow".to_owned(),
                    });
                }

                let no_btn = egui::Button::new(
                    heading_bold(format!("{} (N)", i18n.tr("perm.deny")), 11.0).color(RED),
                )
                .fill(RED_SOFT)
                .stroke(Stroke::new(
                    1.0_f32,
                    Color32::from_rgba_unmultiplied(0xFF, 0x5F, 0x5F, 105),
                ))
                .corner_radius(CornerRadius::same(7))
                .min_size(vec2(70.0, 24.0));
                if ui.add(no_btn).clicked() {
                    let id = perm.id.clone();
                    state.pending_perms.retain(|p| p.id != id);
                    state.push_action(UiAction::DecidePermission {
                        id,
                        decision: "deny".to_owned(),
                    });
                }
            });
        });
}

fn draw_compact_session_card(
    ui: &mut egui::Ui,
    sess: &SessionCard,
    status: &str,
    state: &mut UiState,
    i18n: &I18nStore,
) -> bool {
    let selected = state.selected_session.as_deref() == Some(sess.id.as_str());
    let steer_open = state.panel_steer_session.as_deref() == Some(sess.id.as_str());
    let is_wait = status == "wait";
    let is_work = status == "work";
    let dot_col = if is_wait {
        AMBER
    } else if is_work {
        GREEN
    } else {
        FAINT
    };
    let border = if selected {
        ACCENT
    } else if is_wait {
        Color32::from_rgba_unmultiplied(0xF2, 0xB5, 0x44, 95)
    } else {
        LINE
    };

    let mut steer_input_focused = false;
    let card = egui::Frame::NONE
        .fill(CARD)
        .corner_radius(CornerRadius::same(9))
        .inner_margin(Margin::symmetric(9, 7))
        .stroke(Stroke::new(if selected { 1.3_f32 } else { 1.0_f32 }, border))
        .show(ui, |ui| {
            ui.set_width(ui.available_width());

            // Top line: status dot + project + age
            ui.horizontal(|ui| {
                ui.spacing_mut().item_spacing.x = 6.0;
                let (d, _) = ui.allocate_exact_size(vec2(9.0, 9.0), Sense::hover());
                if is_work {
                    ui.painter().circle_filled(d.center(), 4.8, GREEN_SOFT);
                } else if is_wait {
                    ui.painter().circle_filled(d.center(), 4.8, AMBER_SOFT);
                }
                ui.painter().circle_filled(d.center(), 3.2, dot_col);
                ui.label(heading_bold(clip_chars(&sess.project, 24), 12.0));
                ui.with_layout(egui::Layout::right_to_left(Align::Center), |ui| {
                    if !sess.age_label.is_empty() {
                        ui.label(
                            RichText::new(&sess.age_label)
                                .font(FontId::new(10.0, FontFamily::Monospace))
                                .color(FAINT),
                        );
                    }
                    if sess.ctx_pct > 0.005 {
                        let pct = (sess.ctx_pct.clamp(0.0, 1.0) * 100.0).round() as u32;
                        let col = if pct >= 85 {
                            RED
                        } else if pct >= 60 {
                            AMBER
                        } else {
                            BLUE
                        };
                        ui.label(
                            RichText::new(format!("{pct}%"))
                                .font(FontId::new(10.0, FontFamily::Monospace))
                                .color(col),
                        );
                    }
                });
            });

            // Where line: agent · branch · cache
            let mut where_bits: Vec<String> = Vec::new();
            where_bits.push(sess.agent.clone());
            if let Some(branch) = &sess.git_branch
                && !branch.is_empty()
            {
                if sess.git_dirty > 0 {
                    where_bits.push(format!("{} •{}", clip_chars(branch, 16), sess.git_dirty));
                } else {
                    where_bits.push(clip_chars(branch, 16));
                }
            }
            match sess.cache_warmth {
                CacheWarmth::Warm(m) => {
                    let ms = m.max(1).to_string();
                    let raw = i18n.tr_vars("lite.cacheWarm", &[("n", ms.as_str())]);
                    where_bits.push(strip_coffee(&raw).to_owned());
                }
                CacheWarmth::Cold => {
                    where_bits.push(strip_coffee(&i18n.tr("lite.cacheCold")).to_owned());
                }
                CacheWarmth::Unknown => {}
            }
            ui.label(
                RichText::new(where_bits.join(" · "))
                    .font(FontId::new(10.0, FontFamily::Monospace))
                    .color(FAINT),
            );

            // What line
            let what = if state.streamer_mode {
                i18n.tr("station.streamerRedacted")
            } else if is_wait {
                i18n.tr("panel.status.wait")
            } else if is_work {
                sess.tool
                    .as_deref()
                    .map(|t| clip_chars(t, 44))
                    .unwrap_or_else(|| i18n.tr("panel.status.work"))
            } else if !sess.last_reply.is_empty() {
                clip_chars(&sess.last_reply, 48)
            } else {
                i18n.tr("panel.status.rest")
            };
            ui.label(RichText::new(what).size(11.0).color(if is_wait { AMBER } else { MUTED }));

            if is_work && !sess.subagents.is_empty() {
                ui.label(
                    RichText::new(i18n.tr_count("panel.subs", sess.subagents.len()))
                        .size(10.0)
                        .color(BLUE),
                );
            }

            // Quick prompt steer composer when this card is expanded
            if steer_open {
                ui.add_space(5.0);
                ui.horizontal(|ui| {
                    ui.spacing_mut().item_spacing.x = 5.0;
                    let edit = ui.add(
                        egui::TextEdit::singleline(&mut state.panel_steer_draft)
                            .id(egui::Id::new(("panel_steer_input", &sess.id)))
                            .desired_width((ui.available_width() - 62.0).max(80.0))
                            .font(FontId::new(11.0, FontFamily::Proportional))
                            .hint_text(
                                RichText::new(i18n.tr("chat.steer.placeholder")).color(FAINT),
                            ),
                    );
                    steer_input_focused = edit.has_focus() || edit.lost_focus();
                    let submit_enter =
                        edit.lost_focus() && ui.input(|i| i.key_pressed(Key::Enter));
                    let send_btn = egui::Button::new(
                        heading_bold(i18n.tr("drawer.compose.send"), 10.5).color(ON_GREEN),
                    )
                    .fill(GREEN)
                    .stroke(Stroke::NONE)
                    .corner_radius(CornerRadius::same(6))
                    .min_size(vec2(52.0, 22.0));
                    if (ui.add(send_btn).clicked() || submit_enter)
                        && !state.panel_steer_draft.trim().is_empty()
                    {
                        let text = state.panel_steer_draft.trim().to_owned();
                        state.panel_steer_draft.clear();
                        state.push_action(UiAction::SendPrompt {
                            session_id: sess.id.clone(),
                            text,
                        });
                    }
                });
            }
        });

    let click = card.response.interact(Sense::click());
    if click.hovered() && !selected {
        ui.painter().rect_stroke(
            card.response.rect,
            CornerRadius::same(9),
            Stroke::new(1.0_f32, ACCENT_HI),
            StrokeKind::Inside,
        );
    }
    if click.clicked() {
        state.selected_session = Some(sess.id.clone());
        state.panel_steer_session = if steer_open {
            None
        } else {
            Some(sess.id.clone())
        };
    }

    steer_input_focused
}

fn draw_compact_mac_section(ui: &mut egui::Ui, state: &mut UiState, i18n: &I18nStore) {
    let total = state.sys.mem_total_gb;
    let ram_ratio = if total > 0.0 {
        (state.sys.mem_used_gb / total).clamp(0.0, 1.0)
    } else {
        0.0
    };
    let ram_pct = (ram_ratio * 100.0).round() as u32;
    let ram_col = level_color(ram_ratio);

    let head = egui::Frame::NONE
        .fill(SIDE)
        .corner_radius(CornerRadius::same(8))
        .inner_margin(Margin::symmetric(9, 6))
        .stroke(Stroke::new(1.0_f32, LINE))
        .show(ui, |ui| {
            ui.set_width(ui.available_width());
            ui.horizontal(|ui| {
                ui.label(
                    heading_bold(i18n.tr("panel.mac.title").to_uppercase(), 10.0).color(FAINT),
                );
                ui.with_layout(egui::Layout::right_to_left(Align::Center), |ui| {
                    let chev = if state.panel_mac_open { "▴" } else { "▾" };
                    ui.label(RichText::new(chev).size(10.0).color(FAINT));
                    ui.label(
                        RichText::new(format!("{} {ram_pct}%", i18n.tr("common.ram")))
                            .font(FontId::new(10.5, FontFamily::Monospace))
                            .color(ram_col),
                    );
                });
            });
        })
        .response
        .interact(Sense::click());
    if head.clicked() {
        state.panel_mac_open = !state.panel_mac_open;
    }

    if state.panel_mac_open {
        ui.add_space(5.0);
        egui::Frame::NONE
            .fill(CARD)
            .corner_radius(CornerRadius::same(8))
            .inner_margin(Margin::symmetric(9, 7))
            .stroke(Stroke::new(1.0_f32, LINE))
            .show(ui, |ui| {
                ui.set_width(ui.available_width());
                draw_meter_row(
                    ui,
                    &i18n.tr("common.ram"),
                    &format!("{:.1}/{:.1} GB", state.sys.mem_used_gb, total),
                    ram_ratio,
                    ram_col,
                );
                ui.add_space(4.0);
                let cpu_r = (state.sys.cpu_pct / 100.0).clamp(0.0, 1.0);
                draw_meter_row(
                    ui,
                    "CPU",
                    &format!("{:.0}%", state.sys.cpu_pct),
                    cpu_r,
                    level_color(cpu_r),
                );
                ui.add_space(4.0);
                let disk_r = (state.sys.disk_pct / 100.0).clamp(0.0, 1.0);
                draw_meter_row(
                    ui,
                    &i18n.tr("common.disk"),
                    &format!("{:.0}%", state.sys.disk_pct),
                    disk_r,
                    level_color(disk_r),
                );
            });
    }
}

fn draw_meter_row(ui: &mut egui::Ui, label: &str, value: &str, ratio: f32, col: Color32) {
    ui.horizontal(|ui| {
        ui.label(RichText::new(label).size(10.5).color(MUTED));
        ui.with_layout(egui::Layout::right_to_left(Align::Center), |ui| {
            ui.label(
                RichText::new(value)
                    .font(FontId::new(10.0, FontFamily::Monospace))
                    .color(INK),
            );
        });
    });
    draw_thin_bar(ui, ratio, col);
}

#[cfg(test)]
mod tests {
    use super::*;
    use bevy_egui::egui::{Modifiers, pos2};
    use std::path::PathBuf;

    #[test]
    fn compact_panel_renders_in_420x640_across_en_vi_th_without_raw_i18n_keys() {
        let assets = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../assets");
        let bundle = crate::fonts::load_font_bundle(&assets).expect("load_font_bundle");
        let keys = [
            "station.brand",
            "panel.open3d",
            "panel.open2d",
            "panel.sessions",
            "panel.quiet",
            "panel.keys",
            "panel.mac.title",
            "panel.status.wait",
            "panel.status.work",
            "panel.status.rest",
            "ui.macDrawer.refresh",
            "chat.steer.placeholder",
            "drawer.compose.send",
        ];

        for lang in ["en", "vi", "th"] {
            let i18n = I18nStore::load_from_assets(&assets, lang);
            for k in keys {
                let v = i18n.tr(k);
                assert!(!v.is_empty() && v != k, "lang={lang} raw key {k}");
            }

            let mut state = UiState::with_demo_seed(
                station_3d::OfficeLayout::Pro,
                station_3d::BackdropKind::City,
                false,
            );
            state.view_mode = ViewMode::PanelCompact;
            state.panel_mac_open = true;

            let ctx = egui::Context::default();
            crate::install_egui_fonts(&ctx, &bundle);
            let raw = egui::RawInput {
                screen_rect: Some(Rect::from_min_size(pos2(0.0, 0.0), vec2(420.0, 640.0))),
                ..Default::default()
            };
            let _ = ctx.run(raw.clone(), |_| {});
            let out = ctx.run(raw, |ctx| {
                render_compact_panel_window(ctx, &mut state, &i18n);
            });
            assert!(
                !out.shapes.is_empty(),
                "compact panel must emit shapes for lang={lang}"
            );
        }
    }

    #[test]
    fn compact_panel_keyboard_enter_opens_steer_and_y_approves_when_unfocused() {
        let assets = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../assets");
        let bundle = crate::fonts::load_font_bundle(&assets).expect("load_font_bundle");
        let i18n = I18nStore::load_from_assets(&assets, "en");
        let mut state = UiState::with_demo_seed(
            station_3d::OfficeLayout::Pro,
            station_3d::BackdropKind::City,
            false,
        );
        state.view_mode = ViewMode::PanelCompact;
        state.selected_session = Some("sess-codex-2".to_owned());

        let ctx = egui::Context::default();
        crate::install_egui_fonts(&ctx, &bundle);
        let base_rect = Rect::from_min_size(pos2(0.0, 0.0), vec2(420.0, 640.0));
        let _ = ctx.run(
            egui::RawInput {
                screen_rect: Some(base_rect),
                ..Default::default()
            },
            |_| {},
        );

        let press = |key: Key| egui::Event::Key {
            key,
            physical_key: None,
            pressed: true,
            repeat: false,
            modifiers: Modifiers::NONE,
        };

        let _ = ctx.run(
            egui::RawInput {
                screen_rect: Some(base_rect),
                events: vec![press(Key::Enter)],
                ..Default::default()
            },
            |ctx| {
                render_compact_panel_window(ctx, &mut state, &i18n);
            },
        );
        assert_eq!(
            state.panel_steer_session.as_deref(),
            Some("sess-codex-2"),
            "Enter in compact panel must expand quick steer for selected session"
        );
        assert_eq!(state.active_drawer, ActiveDrawer::None);

        let _ = ctx.run(
            egui::RawInput {
                screen_rect: Some(base_rect),
                events: vec![press(Key::Y)],
                ..Default::default()
            },
            |ctx| {
                render_compact_panel_window(ctx, &mut state, &i18n);
            },
        );
        assert!(state.pending_perms.is_empty(), "Y must approve pending perm");
    }
}
