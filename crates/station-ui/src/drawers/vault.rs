use crate::drawers::markdown::render_markdown;
use crate::i18n::I18nStore;
use crate::state::{ActiveDrawer, UiAction, UiState, VaultListItem, VaultTab};
use crate::theme::{
    ACCENT, ACCENT_HI, AMBER, BG, BLUE, CARD, CARD_2, FAINT, GREEN, INK, LINE, LINE_2, MUTED, RED,
    SIDE, heading_bold,
};
use bevy_egui::egui::{
    self, Align, Color32, CornerRadius, Margin, Rect, RichText, Sense, Stroke, vec2,
};

fn fmt_bytes(bytes: u64, i18n: &I18nStore) -> String {
    if bytes >= 1024 * 1024 * 1024 {
        let s = format!("{:.1}", bytes as f64 / (1024.0 * 1024.0 * 1024.0));
        i18n.tr_vars("time.gb", &[("n", &s)])
    } else if bytes >= 1024 * 1024 {
        let s = format!("{:.1}", bytes as f64 / (1024.0 * 1024.0));
        i18n.tr_vars("time.mb", &[("n", &s)])
    } else {
        let kb = (bytes as f64 / 1024.0).max(1.0);
        format!("{kb:.0} KB")
    }
}

fn project_basename(cwd: &str) -> &str {
    cwd.split(['/', '\\'])
        .filter(|s| !s.is_empty())
        .next_back()
        .unwrap_or("")
}

pub fn matches_vault_query(item: &VaultListItem, query: &str) -> bool {
    let folded_q = station_core::store::vault::fold(query);
    let words: Vec<&str> = folded_q.split_whitespace().collect();
    if words.is_empty() {
        return true;
    }
    let hay = station_core::store::vault::fold(&format!("{} {} {}", item.title, item.cwd, item.first));
    words.iter().all(|w| hay.contains(w))
}

pub fn build_resume_command(cwd: &str, sid: &str) -> String {
    if cwd.trim().is_empty() {
        format!("claude --resume {sid}")
    } else {
        format!("cd \"{}\" && claude --resume {sid}", cwd.replace('"', ""))
    }
}

/// Full `VaultDrawer` matching `vault.js`: accent-insensitive live search, transcript reader modal,
/// 1-click copy `claude --resume <id>`, and `Prompt Coach Weekly Report` tab (`/api/vault/report`).
pub fn draw_vault_drawer(ctx: &egui::Context, state: &mut UiState, i18n: &I18nStore) {
    if !ctx.wants_keyboard_input() && ctx.input(|i| i.key_pressed(egui::Key::Escape)) {
        if state.vault_drawer.open_transcript.is_some() {
            state.vault_drawer.open_transcript = None;
        } else {
            state.active_drawer = ActiveDrawer::None;
        }
        return;
    }

    if !state.vault_drawer.loaded && !state.vault_drawer.loading {
        state.vault_drawer.loading = true;
        let query = state.vault_query.clone();
        state.push_action(UiAction::FetchVaultList { query, fresh: false });
    }
    if state.vault_drawer.tab == VaultTab::Report
        && !state.vault_drawer.report.loaded
        && !state.vault_drawer.report.loading
    {
        state.vault_drawer.report.loading = true;
        state.push_action(UiAction::FetchVaultReport);
    }

    let mut open = true;
    egui::Window::new(heading_bold(i18n.tr("vault.title"), 14.0).color(INK))
        .id(egui::Id::new("station_vault_drawer"))
        .open(&mut open)
        .anchor(egui::Align2::RIGHT_TOP, vec2(-14.0, 62.0))
        .default_width(430.0)
        .max_height(610.0)
        .collapsible(false)
        .resizable(false)
        .frame(
            egui::Frame::window(&ctx.style())
                .fill(SIDE)
                .stroke(Stroke::new(1.1_f32, LINE_2))
                .corner_radius(CornerRadius::same(14))
                .inner_margin(Margin::symmetric(14, 12)),
        )
        .show(ctx, |ui| {
            // Tabs: History | Weekly report
            egui::Frame::NONE
                .fill(CARD)
                .corner_radius(CornerRadius::same(10))
                .inner_margin(Margin::same(3))
                .stroke(Stroke::new(1.0_f32, LINE))
                .show(ui, |ui| {
                    ui.columns(2, |cols| {
                        for (idx, (tab, key)) in [
                            (VaultTab::List, "vault.tabList"),
                            (VaultTab::Report, "vault.tabReport"),
                        ]
                        .into_iter()
                        .enumerate()
                        {
                            cols[idx].push_id(idx, |ui| {
                                let active = state.vault_drawer.tab == tab;
                                let btn = egui::Button::new(
                                    heading_bold(i18n.tr(key), 11.5)
                                        .color(if active { INK } else { MUTED }),
                                )
                                .fill(if active { CARD_2 } else { Color32::TRANSPARENT })
                                .stroke(if active {
                                    Stroke::new(1.0_f32, ACCENT)
                                } else {
                                    Stroke::NONE
                                })
                                .corner_radius(CornerRadius::same(7))
                                .min_size(vec2(ui.available_width(), 26.0));
                                if ui.add(btn).clicked() && !active {
                                    state.vault_drawer.tab = tab;
                                    state.vault_drawer.open_transcript = None;
                                    if tab == VaultTab::Report && !state.vault_drawer.report.loaded {
                                        state.vault_drawer.report.loading = true;
                                        state.push_action(UiAction::FetchVaultReport);
                                    }
                                }
                            });
                        }
                    });
                });

            ui.add_space(6.0);
            match state.vault_drawer.tab {
                VaultTab::List => draw_vault_list_or_transcript(ui, state, i18n),
                VaultTab::Report => draw_vault_report_tab(ui, state, i18n),
            }
        });

    if !open {
        state.active_drawer = ActiveDrawer::None;
    }
}

fn draw_vault_list_or_transcript(ui: &mut egui::Ui, state: &mut UiState, i18n: &I18nStore) {
    let streamer = state.streamer_mode;
    let redacted = i18n.tr("station.streamerRedacted");

    if let Some(rec) = state.vault_drawer.open_transcript.clone() {
        ui.horizontal(|ui| {
            let back_btn = egui::Button::new(heading_bold(i18n.tr("vault.back"), 11.0).color(INK))
                .fill(CARD_2)
                .stroke(Stroke::new(1.0_f32, LINE_2))
                .corner_radius(CornerRadius::same(7));
            if ui.add(back_btn).clicked() {
                state.vault_drawer.open_transcript = None;
            }
            let title = if streamer {
                redacted.clone()
            } else if rec.title.is_empty() {
                i18n.tr("vault.untitled")
            } else {
                rec.title.clone()
            };
            ui.label(heading_bold(title, 12.5).color(INK));
        });
        if !rec.cwd.is_empty() {
            let cwd_disp = if streamer { redacted.as_str() } else { rec.cwd.as_str() };
            ui.label(RichText::new(cwd_disp).size(10.0).color(MUTED));
        }
        ui.add_space(4.0);

        if rec.gone_ms > 0 {
            ui.label(RichText::new(i18n.tr("vault.gone")).size(11.0).color(AMBER));
        } else {
            let cmd = build_resume_command(&rec.cwd, &rec.sid);
            let shown_cmd = if streamer {
                format!("claude --resume {}", rec.sid)
            } else {
                cmd.clone()
            };
            egui::Frame::NONE
                .fill(CARD)
                .corner_radius(CornerRadius::same(8))
                .inner_margin(Margin::symmetric(8, 6))
                .stroke(Stroke::new(1.0_f32, LINE))
                .show(ui, |ui| {
                    ui.label(RichText::new(i18n.tr("vault.resume")).size(10.0).color(MUTED));
                    ui.horizontal(|ui| {
                        ui.label(RichText::new(&shown_cmd).monospace().size(10.5).color(BLUE));
                        ui.with_layout(egui::Layout::right_to_left(Align::Center), |ui| {
                            let copy_btn = egui::Button::new(
                                heading_bold(i18n.tr("vault.copy"), 10.5).color(INK),
                            )
                            .fill(CARD_2)
                            .stroke(Stroke::new(1.0_f32, LINE_2))
                            .corner_radius(CornerRadius::same(6));
                            if ui.add(copy_btn).clicked() {
                                state.push_action(UiAction::CopyToClipboard {
                                    text: cmd,
                                    toast_title: i18n.tr("vault.title"),
                                    toast_body: i18n.tr("vault.copied"),
                                });
                            }
                        });
                    });
                });
        }

        ui.add_space(6.0);
        egui::ScrollArea::vertical().max_height(430.0).show(ui, |ui| {
            for (idx, m) in rec.msgs.iter().enumerate() {
                ui.push_id(("vmsg", idx), |ui| {
                    let is_user = m.role == "u";
                    let who = if is_user {
                        i18n.tr("vault.you")
                    } else {
                        i18n.tr("vault.claude")
                    };
                    let fill = if is_user { CARD_2 } else { CARD };
                    egui::Frame::NONE
                        .fill(fill)
                        .corner_radius(CornerRadius::same(8))
                        .inner_margin(Margin::symmetric(9, 7))
                        .stroke(Stroke::new(1.0_f32, LINE))
                        .show(ui, |ui| {
                            ui.set_width(ui.available_width());
                            ui.label(
                                heading_bold(who, 10.5)
                                    .color(if is_user { ACCENT_HI } else { BLUE }),
                            );
                            if streamer {
                                ui.label(RichText::new(&redacted).size(11.0).color(MUTED));
                            } else if is_user {
                                ui.label(RichText::new(&m.text).size(11.5).color(INK));
                            } else {
                                render_markdown(ui, &m.text);
                            }
                        });
                    ui.add_space(4.0);
                });
            }
        });
        return;
    }

    // Search bar + Refresh
    let mut query_changed = false;
    ui.horizontal(|ui| {
        let resp = ui.add(
            egui::TextEdit::singleline(&mut state.vault_query)
                .desired_width(ui.available_width() - 68.0)
                .hint_text(i18n.tr("vault.search")),
        );
        if resp.changed() {
            query_changed = true;
        }
        let refresh = egui::Button::new(RichText::new(i18n.tr("ui.macDrawer.refresh")).size(10.5))
            .fill(CARD_2)
            .stroke(Stroke::new(1.0_f32, LINE_2))
            .corner_radius(CornerRadius::same(7));
        if ui.add(refresh).clicked() {
            state.vault_drawer.loading = true;
            let query = state.vault_query.clone();
            state.push_action(UiAction::FetchVaultList { query, fresh: true });
        }
    });
    if query_changed {
        let query = state.vault_query.clone();
        state.push_action(UiAction::FetchVaultList { query, fresh: false });
    }

    let stats_line = i18n.tr_vars(
        "vault.at",
        &[
            ("n", &state.vault_drawer.total_count.to_string()),
            ("size", &fmt_bytes(state.vault_drawer.total_bytes, i18n)),
            ("time", &state.clock_label),
        ],
    );
    ui.label(RichText::new(stats_line).size(10.0).color(MUTED));
    ui.add_space(6.0);

    if state.vault_drawer.transcript_loading.is_some() {
        ui.label(RichText::new(i18n.tr("vault.loading")).size(11.5).color(MUTED));
    }

    let items: Vec<VaultListItem> = state
        .vault_drawer
        .items
        .iter()
        .filter(|it| matches_vault_query(it, &state.vault_query))
        .cloned()
        .collect();

    egui::ScrollArea::vertical().max_height(460.0).show(ui, |ui| {
        if items.is_empty() {
            let empty_msg = if state.vault_query.trim().is_empty() {
                i18n.tr("vault.empty")
            } else {
                i18n.tr_vars("vault.none", &[("q", state.vault_query.trim())])
            };
            ui.label(RichText::new(empty_msg).size(11.5).color(MUTED));
            return;
        }
        for (idx, it) in items.iter().enumerate() {
            ui.push_id(("vitem", idx, &it.sid), |ui| {
                let frame = egui::Frame::NONE
                    .fill(CARD)
                    .corner_radius(CornerRadius::same(9))
                    .inner_margin(Margin::symmetric(10, 8))
                    .stroke(Stroke::new(1.0_f32, LINE))
                    .show(ui, |ui| {
                        ui.set_width(ui.available_width());
                        ui.horizontal(|ui| {
                            let title = if streamer {
                                redacted.clone()
                            } else if !it.title.is_empty() {
                                it.title.clone()
                            } else if !it.first.is_empty() {
                                it.first.chars().take(60).collect()
                            } else {
                                i18n.tr("vault.untitled")
                            };
                            ui.label(heading_bold(title, 12.0).color(INK));
                            if it.gone_ms > 0 {
                                ui.with_layout(egui::Layout::right_to_left(Align::Center), |ui| {
                                    ui.label(RichText::new(i18n.tr("vault.gone")).size(9.5).color(AMBER));
                                });
                            }
                        });
                        let proj = if streamer {
                            redacted.as_str()
                        } else {
                            project_basename(&it.cwd)
                        };
                        let msgs_lbl = i18n.tr_count("vault.msgs", it.msg_count);
                        let meta = if proj.is_empty() {
                            msgs_lbl
                        } else {
                            format!("{proj} · {msgs_lbl}")
                        };
                        ui.label(RichText::new(meta).size(10.0).color(MUTED));
                        if !it.first.is_empty() && !streamer {
                            let snip: String = it.first.chars().take(90).collect();
                            ui.label(RichText::new(snip).size(10.5).color(FAINT));
                        }
                    });
                if frame.response.interact(Sense::click()).clicked() {
                    state.vault_drawer.transcript_loading = Some(it.sid.clone());
                    state.push_action(UiAction::FetchVaultTranscript(it.sid.clone()));
                }
                ui.add_space(4.0);
            });
        }
        ui.add_space(6.0);
        ui.label(RichText::new(i18n.tr("vault.note")).size(10.0).color(FAINT));
    });
}

fn draw_vault_report_tab(ui: &mut egui::Ui, state: &mut UiState, i18n: &I18nStore) {
    let rep = state.vault_drawer.report.clone();
    let streamer = state.streamer_mode;
    let redacted = i18n.tr("station.streamerRedacted");

    egui::ScrollArea::vertical().max_height(500.0).show(ui, |ui| {
        if rep.loading {
            ui.label(RichText::new(i18n.tr("vault.loading")).size(12.0).color(MUTED));
            return;
        }
        if let Some(err) = &rep.error {
            ui.label(RichText::new(format!("{}: {err}", i18n.tr("drawer.unreachable"))).size(11.5).color(RED));
            return;
        }
        if rep.n == 0 {
            ui.label(RichText::new(i18n.tr("vault.report.empty")).size(12.0).color(MUTED));
            return;
        }

        let count_line = i18n.tr_vars(
            "vault.report.count",
            &[
                ("n", &rep.n.to_string()),
                ("convs", &rep.convs.to_string()),
            ],
        );
        ui.label(RichText::new(count_line).size(10.5).color(MUTED));
        ui.add_space(6.0);

        // Hero clear %
        egui::Frame::NONE
            .fill(CARD)
            .corner_radius(CornerRadius::same(10))
            .inner_margin(Margin::symmetric(12, 10))
            .stroke(Stroke::new(1.0_f32, LINE))
            .show(ui, |ui| {
                ui.set_width(ui.available_width());
                let pct = rep.pct.unwrap_or(0);
                ui.label(heading_bold(format!("{pct}%"), 28.0).color(GREEN));
                ui.label(RichText::new(i18n.tr("vault.report.clear")).size(11.5).color(INK));
            });

        ui.add_space(10.0);
        ui.label(heading_bold(i18n.tr("vault.report.habits"), 12.0).color(INK));
        ui.add_space(4.0);
        let active_habits: Vec<_> = rep.by.iter().filter(|(_, c)| *c > 0).collect();
        if active_habits.is_empty() {
            ui.label(RichText::new(i18n.tr("vault.report.none")).size(11.5).color(GREEN));
        } else {
            for (idx, (kind, count)) in active_habits.iter().enumerate() {
                ui.push_id(("habit", idx, kind), |ui| {
                    ui.horizontal(|ui| {
                        ui.label(heading_bold(i18n.tr(&format!("vault.report.hint.{kind}")), 11.5).color(INK));
                        ui.with_layout(egui::Layout::right_to_left(Align::Center), |ui| {
                            ui.label(heading_bold(count.to_string(), 11.5).color(AMBER));
                        });
                    });
                    let frac = (*count as f32 / rep.n.max(1) as f32).clamp(0.03, 1.0);
                    let (bar_r, _) = ui.allocate_exact_size(vec2(ui.available_width(), 5.0), Sense::hover());
                    ui.painter().rect_filled(bar_r, 2.5, BG);
                    let fill = Rect::from_min_size(bar_r.min, vec2(bar_r.width() * frac, bar_r.height()));
                    ui.painter().rect_filled(fill, 2.5, AMBER);
                    ui.label(RichText::new(i18n.tr(&format!("vault.report.tip.{kind}"))).size(10.0).color(MUTED));
                    ui.add_space(5.0);
                });
            }
        }

        if !rep.examples.is_empty() {
            ui.add_space(8.0);
            ui.label(heading_bold(i18n.tr("vault.report.examples"), 12.0).color(INK));
            ui.add_space(4.0);
            for (idx, ex) in rep.examples.iter().enumerate() {
                ui.push_id(("ex", idx), |ui| {
                    egui::Frame::NONE
                        .fill(CARD)
                        .corner_radius(CornerRadius::same(8))
                        .inner_margin(Margin::symmetric(9, 7))
                        .stroke(Stroke::new(1.0_f32, LINE))
                        .show(ui, |ui| {
                            ui.set_width(ui.available_width());
                            let quote = if streamer {
                                redacted.clone()
                            } else {
                                format!("“{}”", ex.text)
                            };
                            ui.label(RichText::new(quote).size(11.0).color(INK));
                            let hint_str = i18n.tr_vars(
                                &format!("chat.coach.{}", ex.hint),
                                &[("phrase", &ex.phrase), ("n", &ex.n.to_string())],
                            );
                            ui.label(RichText::new(hint_str).size(10.0).color(AMBER));
                        });
                    ui.add_space(4.0);
                });
            }
        }

        ui.add_space(6.0);
        ui.label(RichText::new(i18n.tr("vault.report.note")).size(10.0).color(FAINT));
    });
}
