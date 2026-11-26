use crate::drawers::markdown::render_markdown;
use crate::i18n::I18nStore;
use crate::state::{
    ActiveDrawer, CacheWarmth, SessionCard, SessionDrawerTab, UiAction, UiState,
};
use crate::theme::{
    ACCENT, AMBER, BG, BLUE, CARD, CARD_2, CARD_3, FAINT, GREEN, INK, LINE, LINE_2, MUTED, RED,
    SIDE, heading_bold, status_dot,
};
use bevy_egui::egui::{
    self, Color32, CornerRadius, FontFamily, FontId, Margin, RichText, Stroke, pos2, vec2,
};

const DRAWER_WIDTH: f32 = 420.0;
const TOPBAR_HEIGHT: f32 = 56.0;
const ON_GREEN: Color32 = Color32::from_rgb(0x06, 0x15, 0x0C);
const ON_ACCENT: Color32 = Color32::from_rgb(0x2A, 0x13, 0x07);

fn accent_soft() -> Color32 {
    Color32::from_rgba_unmultiplied(0xEF, 0x8A, 0x5F, 36)
}

fn agent_color(agent: &str) -> Color32 {
    match agent.to_ascii_lowercase().as_str() {
        "claude" => ACCENT,
        "codex" => GREEN,
        "antigravity" => BLUE,
        "opencode" => AMBER,
        _ => Color32::from_rgb(0xB3, 0x8B, 0xFF),
    }
}

fn status_color(status: &str) -> Color32 {
    match status {
        "work" => GREEN,
        "wait" => AMBER,
        _ => MUTED,
    }
}

fn status_i18n_key(status: &str) -> &'static str {
    match status {
        "work" => "events.status.work",
        "wait" => "events.status.wait",
        _ => "events.status.rest",
    }
}

fn file_basename(path: &str) -> &str {
    path.rsplit(['/', '\\']).next().unwrap_or(path)
}

/// Full 420px right slide-over `SessionDrawer` matching `drawers.css` & `session-drawer.js`.
pub fn draw_session_drawer(ctx: &egui::Context, state: &mut UiState, i18n: &I18nStore) {
    let screen = ctx.screen_rect();
    let drawer_w = DRAWER_WIDTH.min(screen.width()).max(280.0);
    let top_y = if screen.height() > TOPBAR_HEIGHT + 120.0 {
        TOPBAR_HEIGHT
    } else {
        0.0
    };
    let drawer_h = (screen.height() - top_y).max(200.0);

    let selected = state
        .selected_session
        .as_deref()
        .and_then(|id| state.sessions.iter().find(|s| s.id == id))
        .or_else(|| state.sessions.first())
        .cloned();

    // Auto-trigger log fetch once when switching to a session on Chat tab.
    if let Some(sess) = &selected {
        if state.session_log_loaded_for.as_deref() != Some(sess.id.as_str()) {
            state.session_log_loaded_for = Some(sess.id.clone());
            state.push_action(UiAction::FetchSessionLog(sess.id.clone()));
        }
        // Global `O` hotkey opens the session's IDE/terminal window when no text input has focus.
        if !ctx.wants_keyboard_input() {
            let (pressed_o, pressed_esc) = ctx.input(|inp| {
                let no_mods = !inp.modifiers.ctrl && !inp.modifiers.command && !inp.modifiers.alt;
                (
                    no_mods && inp.key_pressed(egui::Key::O),
                    inp.key_pressed(egui::Key::Escape),
                )
            });
            if pressed_o {
                state.push_action(UiAction::OpenSessionWindow(sess.id.clone()));
            } else if pressed_esc {
                state.active_drawer = ActiveDrawer::None;
                return;
            }
        }
    }

    egui::Area::new(egui::Id::new("station_session_drawer"))
        .order(egui::Order::Foreground)
        .fade_in(false)
        .fixed_pos(pos2(screen.right() - drawer_w, top_y))
        .default_size(vec2(drawer_w, drawer_h))
        .show(ctx, |ui| {
            ui.set_min_size(vec2(drawer_w, drawer_h));
            ui.set_max_size(vec2(drawer_w, drawer_h));
            egui::Frame::NONE
                .fill(SIDE)
                .stroke(Stroke::new(1.0_f32, LINE_2))
                .inner_margin(Margin::symmetric(14, 12))
                .show(ui, |ui| {
                    ui.set_width(drawer_w - 28.0);
                    ui.set_height(drawer_h - 24.0);

                    let Some(sess) = selected else {
                        ui.horizontal(|ui| {
                            ui.label(heading_bold(i18n.tr("ui.sessionDrawer.label"), 13.5));
                            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                                if ui
                                    .small_button("×")
                                    .on_hover_text(i18n.tr("common.close"))
                                    .clicked()
                                {
                                    state.active_drawer = ActiveDrawer::None;
                                }
                            });
                        });
                        ui.add_space(12.0);
                        ui.label(RichText::new(i18n.tr("activity.noSessions")).color(MUTED));
                        return;
                    };

                    draw_session_header(ui, &sess, state, i18n);
                    ui.add_space(8.0);
                    draw_tab_bar(ui, &sess, state, i18n);
                    ui.add_space(8.0);

                    // Reserve space at the bottom for Compose box + PromptCoach banner.
                    let compose_reserve = 148.0_f32;
                    let body_h = (ui.available_height() - compose_reserve).max(96.0);
                    egui::ScrollArea::vertical()
                        .id_salt(("session_drawer_body", &sess.id, state.session_tab as u8))
                        .max_height(body_h)
                        .auto_shrink([false, false])
                        .show(ui, |ui| {
                            ui.set_width(ui.available_width());
                            match state.session_tab {
                                SessionDrawerTab::Chat => draw_chat_tab(ui, &sess, state, i18n),
                                SessionDrawerTab::Steps => draw_steps_tab(ui, &sess, state, i18n),
                                SessionDrawerTab::Subagents => {
                                    draw_subagents_tab(ui, &sess, i18n)
                                }
                                SessionDrawerTab::Pins => draw_pins_tab(ui, &sess, state, i18n),
                                SessionDrawerTab::Undo => draw_undo_tab(ui, &sess, state, i18n),
                            }
                        });

                    ui.add_space(6.0);
                    draw_compose_box(ui, &sess, state, i18n);
                });
        });
}

fn draw_session_header(
    ui: &mut egui::Ui,
    sess: &SessionCard,
    state: &mut UiState,
    i18n: &I18nStore,
) {
    // Top title row: dot + agent pill + project + status pill + Close button
    ui.horizontal(|ui| {
        status_dot(ui, agent_color(&sess.agent), 4.5);
        egui::Frame::NONE
            .fill(CARD_2)
            .corner_radius(CornerRadius::same(6))
            .inner_margin(Margin::symmetric(6, 2))
            .show(ui, |ui| {
                ui.label(
                    heading_bold(sess.agent.to_uppercase(), 9.5).color(agent_color(&sess.agent)),
                );
            });
        ui.label(heading_bold(&sess.project, 13.5).color(INK));

        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            if ui
                .add(
                    egui::Button::new(RichText::new("×").size(14.0).color(MUTED))
                        .fill(CARD)
                        .corner_radius(CornerRadius::same(8)),
                )
                .on_hover_text(i18n.tr("common.close"))
                .clicked()
            {
                state.active_drawer = ActiveDrawer::None;
            }
            let st_col = status_color(&sess.status);
            egui::Frame::NONE
                .fill(CARD_2)
                .corner_radius(CornerRadius::same(99))
                .inner_margin(Margin::symmetric(8, 2))
                .stroke(Stroke::new(1.0_f32, st_col))
                .show(ui, |ui| {
                    ui.label(
                        heading_bold(i18n.tr(status_i18n_key(&sess.status)), 10.0).color(st_col),
                    );
                });
        });
    });

    // Directory + git branch/dirty + cache warmth
    ui.add_space(3.0);
    ui.horizontal_wrapped(|ui| {
        ui.spacing_mut().item_spacing.x = 6.0;
        let dir_text = if state.streamer_mode {
            i18n.tr("station.streamerRedacted")
        } else {
            sess.dir.clone()
        };
        if !dir_text.is_empty() {
            ui.label(
                RichText::new(dir_text)
                    .font(FontId::new(10.5, FontFamily::Monospace))
                    .color(MUTED),
            );
        }
        if let Some(branch) = &sess.git_branch {
            let b_lbl = if sess.git_dirty > 0 {
                format!("{branch} *{}", sess.git_dirty)
            } else {
                branch.clone()
            };
            egui::Frame::NONE
                .fill(CARD)
                .corner_radius(CornerRadius::same(6))
                .inner_margin(Margin::symmetric(5, 1))
                .stroke(Stroke::new(1.0_f32, LINE))
                .show(ui, |ui| {
                    ui.label(
                        RichText::new(b_lbl)
                            .font(FontId::new(10.0, FontFamily::Monospace))
                            .color(if sess.git_dirty > 0 { AMBER } else { BLUE }),
                    );
                });
        }
        match sess.cache_warmth {
            CacheWarmth::Warm(mins) => {
                ui.label(
                    RichText::new(i18n.tr_vars("lite.cacheWarm", &[("n", &mins.to_string())]))
                        .size(10.0)
                        .color(AMBER),
                );
            }
            CacheWarmth::Cold => {
                ui.label(RichText::new(i18n.tr("lite.cacheCold")).size(10.0).color(FAINT));
            }
            CacheWarmth::Unknown => {}
        }
    });

    // Compact stat strip: Context % | Cost | Model
    ui.add_space(6.0);
    ui.horizontal(|ui| {
        let ctx_pct = (sess.ctx_pct * 100.0).round() as u32;
        stat_pill(ui, &i18n.tr("ui.sessionDrawer.ctx"), &format!("{ctx_pct}%"));
        stat_pill(
            ui,
            &i18n.tr("ui.sessionDrawer.cost"),
            &format!("${:.2}", sess.cost_usd),
        );
        if !sess.model.is_empty() {
            egui::Frame::NONE
                .fill(CARD)
                .corner_radius(CornerRadius::same(8))
                .inner_margin(Margin::symmetric(8, 4))
                .stroke(Stroke::new(1.0_f32, LINE))
                .show(ui, |ui| {
                    ui.label(
                        RichText::new(&sess.model)
                            .font(FontId::new(10.0, FontFamily::Monospace))
                            .color(MUTED),
                    );
                });
        }
    });

    // Header actions: Open IDE Window (O) + Account Switcher pill
    ui.add_space(6.0);
    ui.horizontal_wrapped(|ui| {
        ui.spacing_mut().item_spacing.x = 6.0;
        let open_btn = egui::Button::new(
            heading_bold(format!("{} (O)", i18n.tr("drawer.openWindowIde")), 11.0).color(INK),
        )
        .fill(CARD_2)
        .stroke(Stroke::new(1.0_f32, LINE_2))
        .corner_radius(CornerRadius::same(8));
        if ui
            .add(open_btn)
            .on_hover_text(i18n.tr("drawer.openWindowTitle"))
            .clicked()
        {
            state.push_action(UiAction::OpenSessionWindow(sess.id.clone()));
        }

        if !state.accounts.is_empty() {
            let active_label = state
                .accounts
                .iter()
                .find(|a| a.active)
                .map(|a| a.label.as_str())
                .unwrap_or("");
            let acct_text = if active_label.is_empty() {
                i18n.tr("drawer.acct.label")
            } else {
                format!("{} · {}", i18n.tr("drawer.acct.label"), active_label)
            };
            let acct_btn = egui::Button::new(heading_bold(acct_text, 11.0).color(INK))
                .fill(if state.account_menu_open {
                    accent_soft()
                } else {
                    CARD_2
                })
                .stroke(Stroke::new(
                    1.0_f32,
                    if state.account_menu_open { ACCENT } else { LINE_2 },
                ))
                .corner_radius(CornerRadius::same(8));
            if ui
                .add(acct_btn)
                .on_hover_text(i18n.tr("drawer.acct.title"))
                .clicked()
            {
                state.account_menu_open = !state.account_menu_open;
            }
        }
    });

    if state.account_menu_open && !state.accounts.is_empty() {
        ui.add_space(4.0);
        let accounts = state.accounts.clone();
        egui::Frame::NONE
            .fill(CARD)
            .corner_radius(CornerRadius::same(10))
            .inner_margin(Margin::symmetric(8, 6))
            .stroke(Stroke::new(1.0_f32, LINE_2))
            .show(ui, |ui| {
                for acct in accounts {
                    ui.horizontal(|ui| {
                        let sub = if acct.active {
                            i18n.tr("drawer.acct.current")
                        } else if let Some(pct) = acct.pct {
                            i18n.tr_vars("drawer.acct.used", &[("pct", &pct.to_string())])
                        } else {
                            String::new()
                        };
                        let btn = egui::Button::new(
                            RichText::new(if sub.is_empty() {
                                acct.label.clone()
                            } else {
                                format!("{} — {sub}", acct.label)
                            })
                            .size(11.0)
                            .color(if acct.active { MUTED } else { INK }),
                        )
                        .fill(if acct.active { BG } else { CARD_2 })
                        .corner_radius(CornerRadius::same(7));
                        if ui.add_enabled(!acct.active, btn).clicked() {
                            state.account_menu_open = false;
                            state.push_action(UiAction::SwitchAccount {
                                session_id: sess.id.clone(),
                                target: acct.slug.clone(),
                            });
                        }
                    });
                }
            });
    }
}

fn stat_pill(ui: &mut egui::Ui, label: &str, value: &str) {
    egui::Frame::NONE
        .fill(CARD)
        .corner_radius(CornerRadius::same(8))
        .inner_margin(Margin::symmetric(8, 4))
        .stroke(Stroke::new(1.0_f32, LINE))
        .show(ui, |ui| {
            ui.horizontal(|ui| {
                ui.spacing_mut().item_spacing.x = 4.0;
                ui.label(RichText::new(label).size(10.0).color(MUTED));
                ui.label(heading_bold(value, 11.0).color(INK));
            });
        });
}

fn draw_tab_bar(
    ui: &mut egui::Ui,
    sess: &SessionCard,
    state: &mut UiState,
    i18n: &I18nStore,
) {
    let sub_n = sess.subagents.len();
    let sub_lbl = if sub_n > 0 {
        i18n.tr_vars("drawer.tabSubsN", &[("n", &sub_n.to_string())])
    } else {
        i18n.tr("drawer.tabSubs")
    };
    let pin_n = state
        .session_pins
        .get(&sess.id)
        .map(Vec::len)
        .unwrap_or(0);
    let pin_lbl = if pin_n > 0 {
        i18n.tr_vars("drawer.pins.tabN", &[("n", &pin_n.to_string())])
    } else {
        i18n.tr("drawer.pins.tab")
    };

    let tabs = [
        (SessionDrawerTab::Chat, i18n.tr("ui.sessionDrawer.chat")),
        (SessionDrawerTab::Steps, i18n.tr("ui.sessionDrawer.activity")),
        (SessionDrawerTab::Subagents, sub_lbl),
        (SessionDrawerTab::Pins, pin_lbl),
        (SessionDrawerTab::Undo, i18n.tr("chat.coach.undo")),
    ];

    egui::Frame::NONE
        .fill(CARD)
        .corner_radius(CornerRadius::same(10))
        .inner_margin(Margin::same(3))
        .stroke(Stroke::new(1.0_f32, LINE))
        .show(ui, |ui| {
            ui.horizontal_wrapped(|ui| {
                ui.spacing_mut().item_spacing = vec2(3.0, 3.0);
                for (tab, label) in tabs {
                    let active = state.session_tab == tab;
                    let btn = egui::Button::new(
                        heading_bold(label, 10.5).color(if active { INK } else { MUTED }),
                    )
                    .fill(if active {
                        accent_soft()
                    } else {
                        Color32::TRANSPARENT
                    })
                    .stroke(Stroke::new(
                        1.0_f32,
                        if active { ACCENT } else { Color32::TRANSPARENT },
                    ))
                    .corner_radius(CornerRadius::same(7));
                    if ui.add(btn).clicked() {
                        state.session_tab = tab;
                        if tab == SessionDrawerTab::Chat {
                            state.push_action(UiAction::FetchSessionLog(sess.id.clone()));
                        } else if tab == SessionDrawerTab::Undo {
                            let need_fetch = state
                                .session_undo
                                .as_ref()
                                .is_none_or(|u| u.session_id != sess.id);
                            if need_fetch {
                                state.push_action(UiAction::FetchSessionUndo(sess.id.clone()));
                            }
                        }
                    }
                }
            });
        });
}

fn draw_chat_tab(
    ui: &mut egui::Ui,
    sess: &SessionCard,
    state: &UiState,
    i18n: &I18nStore,
) {
    let items = state.session_logs.get(&sess.id);
    let Some(items) = items.filter(|v| !v.is_empty()) else {
        if !sess.last_prompt.is_empty() || !sess.last_reply.is_empty() {
            if !sess.last_prompt.is_empty() {
                draw_log_bubble(ui, "user", &sess.last_prompt, "", state.streamer_mode, i18n, &sess.agent);
                ui.add_space(6.0);
            }
            if !sess.last_reply.is_empty() {
                draw_log_bubble(ui, "assistant", &sess.last_reply, "", state.streamer_mode, i18n, &sess.agent);
            }
            return;
        }
        ui.label(RichText::new(i18n.tr("drawer.chatEmpty")).size(11.5).color(MUTED));
        return;
    };

    for item in items {
        draw_log_bubble(
            ui,
            &item.role,
            &item.text,
            &item.ts,
            state.streamer_mode,
            i18n,
            &sess.agent,
        );
        ui.add_space(6.0);
    }
}

fn draw_log_bubble(
    ui: &mut egui::Ui,
    role: &str,
    text: &str,
    ts: &str,
    streamer_mode: bool,
    i18n: &I18nStore,
    agent_name: &str,
) {
    let is_user = role == "user";
    let fill = if is_user {
        Color32::from_rgba_unmultiplied(0xEF, 0x8A, 0x5F, 20)
    } else if role == "tool" {
        BG
    } else {
        CARD
    };
    let stroke = if is_user {
        Stroke::new(1.0_f32, Color32::from_rgba_unmultiplied(0xEF, 0x8A, 0x5F, 90))
    } else {
        Stroke::new(1.0_f32, LINE)
    };

    egui::Frame::NONE
        .fill(fill)
        .corner_radius(CornerRadius::same(10))
        .inner_margin(Margin::symmetric(10, 8))
        .stroke(stroke)
        .show(ui, |ui| {
            ui.set_min_width(ui.available_width());
            ui.horizontal(|ui| {
                let who = if is_user {
                    i18n.tr("drawer.you")
                } else {
                    agent_name.to_owned()
                };
                ui.label(
                    heading_bold(who, 10.5).color(if is_user { ACCENT } else { INK }),
                );
                if !ts.is_empty() {
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        ui.label(RichText::new(ts).size(9.5).color(FAINT));
                    });
                }
            });
            ui.add_space(3.0);
            if streamer_mode {
                ui.label(
                    RichText::new(i18n.tr("station.streamerRedacted"))
                        .size(11.0)
                        .color(MUTED),
                );
            } else if role == "tool" {
                ui.label(
                    RichText::new(text)
                        .font(FontId::new(10.5, FontFamily::Monospace))
                        .color(MUTED),
                );
            } else {
                render_markdown(ui, text);
            }
        });
}

fn draw_steps_tab(
    ui: &mut egui::Ui,
    sess: &SessionCard,
    state: &UiState,
    i18n: &I18nStore,
) {
    // 1. Todo checklist (done/total)
    let done = if !state.session_todos.is_empty() {
        state.session_todos.iter().filter(|t| t.done).count()
    } else {
        sess.todos_done
    };
    let total = if !state.session_todos.is_empty() {
        state.session_todos.len()
    } else {
        sess.todos_total
    };

    if total > 0 {
        egui::Frame::NONE
            .fill(CARD)
            .corner_radius(CornerRadius::same(10))
            .inner_margin(Margin::symmetric(10, 8))
            .stroke(Stroke::new(1.0_f32, LINE))
            .show(ui, |ui| {
                ui.set_min_width(ui.available_width());
                ui.label(
                    heading_bold(
                        i18n.tr_vars(
                            "lite.todoTitle",
                            &[("done", &done.to_string()), ("n", &total.to_string())],
                        ),
                        11.5,
                    )
                    .color(INK),
                );
                ui.add_space(4.0);
                for todo in &state.session_todos {
                    ui.horizontal(|ui| {
                        let mark = if todo.done { "✓" } else { "○" };
                        ui.label(
                            RichText::new(mark)
                                .size(11.5)
                                .color(if todo.done { GREEN } else { MUTED }),
                        );
                        ui.label(
                            RichText::new(&todo.text)
                                .size(11.5)
                                .color(if todo.done { MUTED } else { INK }),
                        );
                    });
                }
            });
        ui.add_space(8.0);
    }

    // 2. Recent tool steps for this session
    let steps: Vec<_> = state
        .feed
        .iter()
        .filter(|e| {
            e.agent.eq_ignore_ascii_case(&sess.agent)
                || e.agent.eq_ignore_ascii_case(&sess.project)
        })
        .collect();

    if steps.is_empty() {
        ui.label(RichText::new(i18n.tr("drawer.noActivity")).size(11.5).color(MUTED));
        return;
    }

    ui.label(
        heading_bold(i18n.tr_count("drawer.steps", steps.len()), 11.0).color(MUTED),
    );
    ui.add_space(4.0);
    for ev in steps {
        egui::Frame::NONE
            .fill(CARD)
            .corner_radius(CornerRadius::same(8))
            .inner_margin(Margin::symmetric(9, 6))
            .stroke(Stroke::new(1.0_f32, LINE))
            .show(ui, |ui| {
                ui.set_min_width(ui.available_width());
                ui.horizontal(|ui| {
                    let col = match ev.level.as_str() {
                        "err" => RED,
                        "warn" => AMBER,
                        "ok" => GREEN,
                        _ => BLUE,
                    };
                    status_dot(ui, col, 3.0);
                    let msg = if state.streamer_mode {
                        i18n.tr("station.streamerRedacted")
                    } else {
                        ev.message.clone()
                    };
                    ui.label(
                        RichText::new(msg)
                            .font(FontId::new(11.0, FontFamily::Monospace))
                            .color(INK),
                    );
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        ui.label(RichText::new(&ev.ts_label).size(9.5).color(FAINT));
                    });
                });
            });
        ui.add_space(4.0);
    }
}

fn draw_subagents_tab(ui: &mut egui::Ui, sess: &SessionCard, i18n: &I18nStore) {
    if sess.subagents.is_empty() {
        ui.label(RichText::new(i18n.tr("drawer.noSubs")).size(11.5).color(MUTED));
        return;
    }
    ui.label(
        heading_bold(
            i18n.tr_vars(
                "drawer.groupWorking",
                &[("n", &sess.subagents.len().to_string())],
            ),
            11.0,
        )
        .color(MUTED),
    );
    ui.add_space(4.0);
    for sub in &sess.subagents {
        egui::Frame::NONE
            .fill(CARD)
            .corner_radius(CornerRadius::same(10))
            .inner_margin(Margin::symmetric(10, 8))
            .stroke(Stroke::new(1.0_f32, LINE))
            .show(ui, |ui| {
                ui.set_min_width(ui.available_width());
                ui.horizontal(|ui| {
                    status_dot(ui, BLUE, 3.5);
                    ui.label(heading_bold(&sub.label, 11.5).color(INK));
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        ui.label(RichText::new(i18n.tr("drawer.working")).size(10.0).color(GREEN));
                    });
                });
                if !sub.doing.is_empty() {
                    ui.add_space(2.0);
                    ui.label(
                        RichText::new(&sub.doing)
                            .font(FontId::new(10.5, FontFamily::Monospace))
                            .color(MUTED),
                    );
                }
            });
        ui.add_space(5.0);
    }
}

fn draw_pins_tab(
    ui: &mut egui::Ui,
    sess: &SessionCard,
    state: &mut UiState,
    i18n: &I18nStore,
) {
    ui.label(RichText::new(i18n.tr("drawer.pins.intro")).size(11.0).color(MUTED));
    ui.add_space(6.0);

    let pins = state
        .session_pins
        .get(&sess.id)
        .cloned()
        .unwrap_or_default();

    if pins.is_empty() {
        egui::Frame::NONE
            .fill(Color32::TRANSPARENT)
            .corner_radius(CornerRadius::same(10))
            .inner_margin(Margin::symmetric(10, 8))
            .stroke(Stroke::new(1.0_f32, LINE))
            .show(ui, |ui| {
                ui.set_min_width(ui.available_width());
                ui.label(RichText::new(i18n.tr("drawer.pins.empty")).size(11.5).color(FAINT));
            });
    } else {
        let mut remove_idx: Option<usize> = None;
        for (idx, text) in pins.iter().enumerate() {
            egui::Frame::NONE
                .fill(CARD)
                .corner_radius(CornerRadius::same(10))
                .inner_margin(Margin::symmetric(10, 7))
                .stroke(Stroke::new(1.0_f32, LINE))
                .show(ui, |ui| {
                    ui.set_min_width(ui.available_width());
                    ui.horizontal(|ui| {
                        ui.label(RichText::new(text).size(11.5).color(INK));
                        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                            if ui
                                .add(
                                    egui::Button::new(RichText::new("×").size(12.0).color(MUTED))
                                        .fill(CARD_2)
                                        .corner_radius(CornerRadius::same(6)),
                                )
                                .on_hover_text(i18n.tr("drawer.pins.remove"))
                                .clicked()
                            {
                                remove_idx = Some(idx);
                            }
                        });
                    });
                });
            ui.add_space(4.0);
        }
        if let Some(idx) = remove_idx {
            if let Some(list) = state.session_pins.get_mut(&sess.id)
                && idx < list.len()
            {
                list.remove(idx);
            }
            state.push_action(UiAction::RemoveSessionPin {
                session_id: sess.id.clone(),
                index: idx,
            });
        }
    }

    ui.add_space(8.0);
    ui.add(
        egui::TextEdit::multiline(&mut state.pin_draft)
            .desired_rows(2)
            .desired_width(f32::INFINITY)
            .hint_text(i18n.tr("drawer.pins.placeholder")),
    );
    ui.add_space(4.0);
    let can_add = !state.pin_draft.trim().is_empty();
    let add_btn = egui::Button::new(heading_bold(i18n.tr("drawer.pins.add"), 11.5).color(ON_GREEN))
        .fill(GREEN)
        .corner_radius(CornerRadius::same(8));
    if ui.add_enabled(can_add, add_btn).clicked() {
        let text = std::mem::take(&mut state.pin_draft).trim().to_owned();
        if !text.is_empty() {
            state
                .session_pins
                .entry(sess.id.clone())
                .or_default()
                .push(text.clone());
            state.push_action(UiAction::AddSessionPin {
                session_id: sess.id.clone(),
                text,
            });
        }
    }
}

fn draw_undo_tab(
    ui: &mut egui::Ui,
    sess: &SessionCard,
    state: &mut UiState,
    i18n: &I18nStore,
) {
    ui.horizontal(|ui| {
        ui.label(heading_bold(i18n.tr("drawer.undo.btn"), 12.5).color(INK));
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            let refresh_btn = egui::Button::new(
                RichText::new(i18n.tr("ui.macDrawer.refresh"))
                    .size(10.5)
                    .color(MUTED),
            )
            .fill(CARD_2)
            .corner_radius(CornerRadius::same(7));
            if ui.add(refresh_btn).clicked() {
                state.push_action(UiAction::FetchSessionUndo(sess.id.clone()));
            }
        });
    });
    ui.add_space(6.0);

    let Some(undo) = state.session_undo.as_mut().filter(|u| u.session_id == sess.id) else {
        ui.label(RichText::new(i18n.tr("common.loading")).size(11.5).color(MUTED));
        return;
    };

    if undo.loading {
        ui.label(RichText::new(i18n.tr("common.loading")).size(11.5).color(MUTED));
        return;
    }

    if let Some(err) = &undo.error {
        ui.label(RichText::new(err).size(11.5).color(RED));
        return;
    }

    if !undo.done_files.is_empty() {
        let dir = undo.backup_dir.as_deref().unwrap_or("");
        ui.label(
            RichText::new(i18n.tr_vars(
                "drawer.undo.done",
                &[("n", &undo.done_files.len().to_string()), ("dir", dir)],
            ))
            .size(11.5)
            .color(INK),
        );
        ui.add_space(4.0);
        for (file, act) in &undo.done_files {
            ui.horizontal(|ui| {
                ui.label(
                    RichText::new(file_basename(file))
                        .font(FontId::new(11.0, FontFamily::Monospace))
                        .color(INK),
                );
                ui.label(
                    RichText::new(i18n.tr(&format!("drawer.undo.act.{act}")))
                        .size(10.5)
                        .color(GREEN),
                );
            });
        }
        ui.add_space(6.0);
        ui.label(RichText::new(i18n.tr("drawer.undo.tellNote")).size(10.5).color(MUTED));
        ui.add_space(4.0);
        let tell_files: Vec<String> = undo.done_files.iter().map(|(f, _)| f.clone()).collect();
        let tell_btn =
            egui::Button::new(heading_bold(i18n.tr("drawer.undo.tell"), 11.5).color(ON_GREEN))
                .fill(GREEN)
                .corner_radius(CornerRadius::same(8));
        if ui.add(tell_btn).clicked() {
            state.push_action(UiAction::TellClaudeUndo {
                session_id: sess.id.clone(),
                files: tell_files,
            });
        }
        return;
    }

    if undo.files.is_empty() {
        ui.label(RichText::new(i18n.tr("drawer.undo.none")).size(11.5).color(MUTED));
        return;
    }

    if !undo.prompt.is_empty() {
        ui.label(
            RichText::new(i18n.tr_vars(
                "drawer.undo.intro",
                &[("prompt", &undo.prompt), ("time", &undo.time_label)],
            ))
            .size(11.0)
            .color(MUTED),
        );
        ui.add_space(6.0);
    }

    let mut selected_files = Vec::new();
    for entry in &mut undo.files {
        let can_undo = entry.is_undoable();
        egui::Frame::NONE
            .fill(CARD)
            .corner_radius(CornerRadius::same(9))
            .inner_margin(Margin::symmetric(9, 6))
            .stroke(Stroke::new(1.0_f32, LINE))
            .show(ui, |ui| {
                ui.set_min_width(ui.available_width());
                ui.horizontal(|ui| {
                    if can_undo {
                        ui.checkbox(&mut entry.selected, "");
                    }
                    ui.label(
                        RichText::new(file_basename(&entry.file))
                            .font(FontId::new(11.0, FontFamily::Monospace))
                            .color(if can_undo { INK } else { MUTED }),
                    )
                    .on_hover_text(&entry.file);
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        let act_col = match entry.action.as_str() {
                            "restore" => GREEN,
                            "delete" => AMBER,
                            _ => MUTED,
                        };
                        ui.label(
                            RichText::new(i18n.tr(&format!("drawer.undo.act.{}", entry.action)))
                                .size(10.0)
                                .color(act_col),
                        );
                    });
                });
            });
        ui.add_space(4.0);
        if can_undo && entry.selected {
            selected_files.push(entry.file.clone());
        }
    }

    ui.add_space(6.0);
    let n = selected_files.len();
    let go_label = i18n.tr_count("drawer.undo.go", n);
    let go_btn = egui::Button::new(heading_bold(go_label, 11.5).color(Color32::WHITE))
        .fill(if n > 0 { RED } else { CARD_3 })
        .corner_radius(CornerRadius::same(8));
    if ui.add_enabled(n > 0, go_btn).clicked() {
        state.push_action(UiAction::ApplySessionUndo {
            session_id: sess.id.clone(),
            files: selected_files,
        });
    }
}

fn draw_compose_box(
    ui: &mut egui::Ui,
    sess: &SessionCard,
    state: &mut UiState,
    i18n: &I18nStore,
) {
    // Separator line
    let (sep, _) = ui.allocate_exact_size(vec2(ui.available_width(), 1.0), egui::Sense::hover());
    ui.painter().rect_filled(sep, CornerRadius::ZERO, LINE);
    ui.add_space(5.0);

    // Live real-time PromptCoach hint bar powered by `station_core::shared::prompt_coach::coach`
    let hints = station_core::shared::prompt_coach::coach(&state.compose_draft, 0, false);
    if let Some(hint) = hints.first() {
        let msg = match hint.id {
            "vague" => i18n.tr_vars(
                "chat.coach.vague",
                &[("phrase", hint.phrase.as_deref().unwrap_or(""))],
            ),
            "dangling" => i18n.tr_vars(
                "chat.coach.dangling",
                &[("phrase", hint.phrase.as_deref().unwrap_or(""))],
            ),
            "multi" => i18n.tr_vars(
                "chat.coach.multi",
                &[("n", &hint.n.unwrap_or(2).to_string())],
            ),
            "photoBare" => i18n.tr("chat.coach.photoBare"),
            _ => i18n.tr("chat.coach.tooShort"),
        };
        egui::Frame::NONE
            .fill(Color32::from_rgba_unmultiplied(0xF2, 0xB5, 0x44, 24))
            .corner_radius(CornerRadius::same(8))
            .inner_margin(Margin::symmetric(8, 5))
            .stroke(Stroke::new(1.0_f32, AMBER))
            .show(ui, |ui| {
                ui.set_min_width(ui.available_width());
                ui.horizontal_wrapped(|ui| {
                    ui.label(heading_bold(i18n.tr("chat.coach.label"), 10.0).color(AMBER));
                    ui.label(RichText::new(msg).size(10.5).color(INK));
                    if hint.id == "multi"
                        && ui
                            .small_button(i18n.tr("chat.coach.chip.number"))
                            .clicked()
                    {
                        let head = i18n.tr("chat.coach.chip.inOrder");
                        state.compose_draft = station_core::shared::prompt_coach::number_tasks(
                            &state.compose_draft,
                            &head,
                        );
                    }
                });
            });
        ui.add_space(4.0);
    }

    let placeholder = i18n.tr_vars("chat.to.placeholder", &[("name", &sess.project)]);
    ui.add(
        egui::TextEdit::multiline(&mut state.compose_draft)
            .desired_rows(2)
            .desired_width(f32::INFINITY)
            .hint_text(placeholder),
    );
    ui.add_space(5.0);

    let has_text = !state.compose_draft.trim().is_empty();
    ui.horizontal(|ui| {
        // Rewrite button (`POST /api/prompt/rewrite`)
        let rw_lbl = if state.compose_rewriting {
            i18n.tr("chat.coach.rewriting")
        } else {
            i18n.tr("chat.coach.rewrite")
        };
        let short_rw: String = rw_lbl.split('·').next().unwrap_or(&rw_lbl).trim().to_owned();
        let rw_btn = egui::Button::new(heading_bold(short_rw, 10.5).color(INK))
            .fill(CARD_2)
            .stroke(Stroke::new(1.0_f32, LINE_2))
            .corner_radius(CornerRadius::same(8));
        if ui
            .add_enabled(has_text && !state.compose_rewriting, rw_btn)
            .on_hover_text(i18n.tr("chat.coach.rewriteTitle"))
            .clicked()
        {
            state.compose_prev_draft = Some(state.compose_draft.clone());
            state.compose_rewriting = true;
            state.push_action(UiAction::RewritePrompt {
                text: state.compose_draft.clone(),
                lang: i18n.lang.clone(),
            });
        }

        if let Some(prev) = state.compose_prev_draft.clone() {
            let undo_btn = egui::Button::new(
                RichText::new(i18n.tr("chat.coach.undo"))
                    .size(10.5)
                    .color(MUTED),
            )
            .fill(CARD)
            .corner_radius(CornerRadius::same(7));
            if ui.add(undo_btn).clicked() {
                state.compose_draft = prev;
                state.compose_prev_draft = None;
            }
        }

        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            let is_working = sess.status == "work";
            let send_lbl = if is_working {
                i18n.tr("chat.steer.btn")
            } else {
                i18n.tr("drawer.compose.send")
            };
            let send_btn = egui::Button::new(heading_bold(send_lbl, 11.0).color(ON_ACCENT))
                .fill(ACCENT)
                .corner_radius(CornerRadius::same(8));
            if ui.add_enabled(has_text, send_btn).clicked() {
                let text = std::mem::take(&mut state.compose_draft);
                state.compose_prev_draft = None;
                if is_working {
                    state.push_action(UiAction::SteerSession {
                        session_id: sess.id.clone(),
                        text,
                    });
                } else {
                    state.push_action(UiAction::SendPrompt {
                        session_id: sess.id.clone(),
                        text,
                    });
                }
            }

            if is_working {
                let stop_btn = egui::Button::new(
                    heading_bold(i18n.tr("chat.loop.stop"), 10.5).color(Color32::WHITE),
                )
                .fill(RED)
                .corner_radius(CornerRadius::same(8));
                if ui.add(stop_btn).clicked() {
                    state.push_action(UiAction::StopSessionTask {
                        session_id: sess.id.clone(),
                    });
                }
            }
        });
    });
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::state::{
        ActiveDrawer, SessionDrawerTab, SessionUndoView, UiAction, UndoFileEntry,
    };
    use bevy_egui::egui::{self, Event, Key, Modifiers, PointerButton, Pos2, Rect, pos2, vec2};
    use std::path::PathBuf;

    fn load_i18n(lang: &str) -> I18nStore {
        let assets = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../assets");
        I18nStore::load_from_assets(&assets, lang)
    }

    fn setup_ctx() -> egui::Context {
        let assets = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../assets");
        let bundle = crate::fonts::load_font_bundle(&assets).expect("load_font_bundle");
        let ctx = egui::Context::default();
        crate::install_egui_fonts(&ctx, &bundle);
        let raw = egui::RawInput {
            screen_rect: Some(Rect::from_min_size(pos2(0.0, 0.0), vec2(1280.0, 720.0))),
            ..Default::default()
        };
        let _ = ctx.run(raw, |_| {});
        ctx
    }

    fn find_text_in_shape(shape: &egui::epaint::Shape, needle: &str) -> Option<Pos2> {
        match shape {
            egui::epaint::Shape::Text(ts) if ts.galley.text().contains(needle) => {
                Some(ts.galley.rect.translate(ts.pos.to_vec2()).center())
            }
            egui::epaint::Shape::Vec(children) => {
                children.iter().find_map(|s| find_text_in_shape(s, needle))
            }
            _ => None,
        }
    }

    fn find_text_center(shapes: &[egui::epaint::ClippedShape], needle: &str) -> Option<Pos2> {
        shapes
            .iter()
            .find_map(|cs| find_text_in_shape(&cs.shape, needle))
    }

    fn collect_text_centers(
        shape: &egui::epaint::Shape,
        exact: &str,
        out: &mut Vec<Pos2>,
    ) {
        match shape {
            egui::epaint::Shape::Text(ts) if ts.galley.text() == exact => {
                out.push(ts.galley.rect.translate(ts.pos.to_vec2()).center());
            }
            egui::epaint::Shape::Vec(children) => {
                for c in children {
                    collect_text_centers(c, exact, out);
                }
            }
            _ => {}
        }
    }

    fn click_at(ctx: &egui::Context, pos: Pos2, f: impl FnMut(&egui::Context)) {
        let mut f = f;
        let base = egui::RawInput {
            screen_rect: Some(Rect::from_min_size(pos2(0.0, 0.0), vec2(1280.0, 720.0))),
            ..Default::default()
        };
        let mut down = base.clone();
        down.events = vec![
            Event::PointerMoved(pos),
            Event::PointerButton {
                pos,
                button: PointerButton::Primary,
                pressed: true,
                modifiers: Modifiers::NONE,
            },
        ];
        let _ = ctx.run(down, |c| f(c));
        let mut up = base;
        up.events = vec![
            Event::PointerMoved(pos),
            Event::PointerButton {
                pos,
                button: PointerButton::Primary,
                pressed: false,
                modifiers: Modifiers::NONE,
            },
        ];
        let _ = ctx.run(up, |c| f(c));
    }

    #[test]
    fn session_drawer_switches_tabs_emits_pin_add_delete_and_selective_undo() {
        let ctx = setup_ctx();
        let i18n = load_i18n("en");
        let mut state = UiState::with_demo_seed(
            station_3d::OfficeLayout::Pro,
            station_3d::BackdropKind::City,
            false,
        );
        state.active_drawer = ActiveDrawer::Session;
        state.selected_session = Some("sess-claude-1".to_owned());

        let base = egui::RawInput {
            screen_rect: Some(Rect::from_min_size(pos2(0.0, 0.0), vec2(1280.0, 720.0))),
            ..Default::default()
        };

        // 1. Renders markdown conversation on Chat tab (warm up Area sizing pass first)
        let _ = ctx.run(base.clone(), |c| draw_session_drawer(c, &mut state, &i18n));
        let out = ctx.run(base.clone(), |c| draw_session_drawer(c, &mut state, &i18n));
        assert!(
            find_text_center(&out.shapes, "Shell Progress").is_some(),
            "expected markdown heading rendered in Chat tab"
        );

        // 2. Click Pinned tab -> switches to SessionDrawerTab::Pins
        let pins_tab_pos =
            find_text_center(&out.shapes, &i18n.tr_vars("drawer.pins.tabN", &[("n", "1")]))
                .or_else(|| find_text_center(&out.shapes, &i18n.tr("drawer.pins.tab")))
                .expect("expected Pins tab button");
        click_at(&ctx, pins_tab_pos, |c| draw_session_drawer(c, &mut state, &i18n));
        assert_eq!(state.session_tab, SessionDrawerTab::Pins);

        // 3. Add a pin -> emits AddSessionPin
        state.pin_draft = "Always run cargo test".to_owned();
        let out_pins = ctx.run(base.clone(), |c| draw_session_drawer(c, &mut state, &i18n));
        let mut pin_btns = Vec::new();
        let pin_lbl = i18n.tr("drawer.pins.add");
        for cs in &out_pins.shapes {
            collect_text_centers(&cs.shape, &pin_lbl, &mut pin_btns);
        }
        let add_pos = pin_btns
            .into_iter()
            .last()
            .expect("expected Pin add button");
        click_at(&ctx, add_pos, |c| draw_session_drawer(c, &mut state, &i18n));
        let actions = state.drain_actions();
        assert!(
            actions.iter().any(|a| matches!(
                a,
                UiAction::AddSessionPin { session_id, text }
                if session_id == "sess-claude-1" && text == "Always run cargo test"
            )),
            "expected AddSessionPin, got {actions:?}"
        );

        // 4. Delete existing pin -> emits RemoveSessionPin
        let out_pins2 = ctx.run(base.clone(), |c| draw_session_drawer(c, &mut state, &i18n));
        let mut xs = Vec::new();
        for cs in &out_pins2.shapes {
            collect_text_centers(&cs.shape, "×", &mut xs);
        }
        let del_pos = xs
            .into_iter()
            .find(|p| p.y > 160.0)
            .expect("expected pin remove × button");
        click_at(&ctx, del_pos, |c| draw_session_drawer(c, &mut state, &i18n));
        let actions = state.drain_actions();
        assert!(
            actions.iter().any(|a| matches!(
                a,
                UiAction::RemoveSessionPin { session_id, index: 0 }
                if session_id == "sess-claude-1"
            )),
            "expected RemoveSessionPin, got {actions:?}"
        );

        // 5. Switch to Undo tab with 2 files, only 1 selected -> emits ApplySessionUndo with ONLY selected file
        state.session_tab = SessionDrawerTab::Undo;
        state.session_undo = Some(SessionUndoView {
            session_id: "sess-claude-1".to_owned(),
            loading: false,
            prompt: "Refactor topbar".to_owned(),
            time_label: "16:41".to_owned(),
            files: vec![
                UndoFileEntry {
                    file: "src/a.rs".to_owned(),
                    action: "restore".to_owned(),
                    selected: true,
                },
                UndoFileEntry {
                    file: "src/b.rs".to_owned(),
                    action: "delete".to_owned(),
                    selected: false,
                },
            ],
            ..Default::default()
        });
        let out_undo = ctx.run(base.clone(), |c| draw_session_drawer(c, &mut state, &i18n));
        let undo_btn_label = i18n.tr_count("drawer.undo.go", 1);
        let undo_pos = find_text_center(&out_undo.shapes, &undo_btn_label)
            .expect("expected Undo 1 file button");
        click_at(&ctx, undo_pos, |c| draw_session_drawer(c, &mut state, &i18n));
        let actions = state.drain_actions();
        assert!(
            actions.iter().any(|a| matches!(
                a,
                UiAction::ApplySessionUndo { session_id, files }
                if session_id == "sess-claude-1" && files == &vec!["src/a.rs".to_owned()]
            )),
            "expected selective ApplySessionUndo for src/a.rs only, got {actions:?}"
        );

        // 6. Press 'O' hotkey -> emits OpenSessionWindow
        let mut key_o = base;
        key_o.events = vec![Event::Key {
            key: Key::O,
            physical_key: Some(Key::O),
            pressed: true,
            repeat: false,
            modifiers: Modifiers::NONE,
        }];
        let _ = ctx.run(key_o, |c| draw_session_drawer(c, &mut state, &i18n));
        let actions = state.drain_actions();
        assert!(
            actions
                .iter()
                .any(|a| matches!(a, UiAction::OpenSessionWindow(sid) if sid == "sess-claude-1")),
            "expected OpenSessionWindow on O hotkey, got {actions:?}"
        );
    }

    #[test]
    fn session_drawer_compose_shows_prompt_coach_and_emits_rewrite_steer_stop() {
        let ctx = setup_ctx();
        let i18n = load_i18n("en");
        let mut state = UiState::with_demo_seed(
            station_3d::OfficeLayout::Pro,
            station_3d::BackdropKind::City,
            false,
        );
        state.active_drawer = ActiveDrawer::Session;
        state.selected_session = Some("sess-claude-1".to_owned());
        state.compose_draft = "make it better".to_owned();

        let base = egui::RawInput {
            screen_rect: Some(Rect::from_min_size(pos2(0.0, 0.0), vec2(1280.0, 720.0))),
            ..Default::default()
        };

        let _ = ctx.run(base.clone(), |c| draw_session_drawer(c, &mut state, &i18n));
        let out = ctx.run(base.clone(), |c| draw_session_drawer(c, &mut state, &i18n));
        assert!(
            find_text_center(&out.shapes, &i18n.tr("chat.coach.label")).is_some(),
            "expected live PromptCoach hint for vague prompt"
        );

        // Click Rewrite -> emits RewritePrompt
        let rw_lbl = i18n.tr("chat.coach.rewrite");
        let short_rw = rw_lbl.split('·').next().unwrap_or(&rw_lbl).trim();
        let rw_pos = find_text_center(&out.shapes, short_rw).expect("expected Rewrite button");
        click_at(&ctx, rw_pos, |c| draw_session_drawer(c, &mut state, &i18n));
        let actions = state.drain_actions();
        assert!(
            actions.iter().any(|a| matches!(
                a,
                UiAction::RewritePrompt { text, lang } if text == "make it better" && lang == "en"
            )),
            "expected RewritePrompt, got {actions:?}"
        );

        // Working session shows Steer + Stop -> click Steer -> emits SteerSession
        let steer_pos = find_text_center(&out.shapes, &i18n.tr("chat.steer.btn"))
            .expect("expected Steer button for working session");
        click_at(&ctx, steer_pos, |c| draw_session_drawer(c, &mut state, &i18n));
        let actions = state.drain_actions();
        assert!(
            actions.iter().any(|a| matches!(
                a,
                UiAction::SteerSession { session_id, text }
                if session_id == "sess-claude-1" && text == "make it better"
            )),
            "expected SteerSession, got {actions:?}"
        );

        // Click Stop -> emits StopSessionTask
        let out2 = ctx.run(base, |c| draw_session_drawer(c, &mut state, &i18n));
        let stop_pos = find_text_center(&out2.shapes, &i18n.tr("chat.loop.stop"))
            .expect("expected Stop button for working session");
        click_at(&ctx, stop_pos, |c| draw_session_drawer(c, &mut state, &i18n));
        let actions = state.drain_actions();
        assert!(
            actions
                .iter()
                .any(|a| matches!(a, UiAction::StopSessionTask { session_id } if session_id == "sess-claude-1")),
            "expected StopSessionTask, got {actions:?}"
        );
    }
}
