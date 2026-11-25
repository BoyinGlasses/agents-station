use crate::i18n::I18nStore;
use crate::launcher_dock::draw_launcher_inline;
use crate::state::{ActiveDrawer, SessionCard, UiAction, UiState, ViewMode};
use crate::theme::{
    ACCENT_TERRACOTTA, BG_DEEP, STATUS_ERR, STATUS_INFO, STATUS_WAIT, STATUS_WORK, TEXT_MUTED,
    TEXT_PRIMARY, card_frame, status_dot,
};
use bevy_egui::egui::{self, Color32, RichText, Stroke};

fn draw_agent_card(ui: &mut egui::Ui, s: &SessionCard) {
    let dot = match s.status.as_str() {
        "working" => STATUS_WORK,
        "waiting" => STATUS_WAIT,
        "error" => STATUS_ERR,
        _ => STATUS_INFO,
    };
    card_frame().show(ui, |ui| {
        ui.set_width(ui.available_width());
        ui.horizontal(|ui| {
            status_dot(ui, dot, 4.2);
            ui.label(
                RichText::new(s.agent.to_uppercase())
                    .strong()
                    .size(13.0)
                    .color(TEXT_PRIMARY),
            );
            ui.label(RichText::new(&s.project).size(12.0).color(ACCENT_TERRACOTTA));
            ui.label(RichText::new(&s.model).size(11.0).color(TEXT_MUTED));
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                ui.label(
                    RichText::new(format!("${:.2} · {} tok", s.cost_usd, s.tokens))
                        .size(11.0)
                        .color(TEXT_MUTED),
                );
            });
        });
        ui.add_space(4.0);
        ui.label(RichText::new(&s.last_prompt).size(12.5).color(TEXT_PRIMARY));
        if let Some(tool) = &s.tool {
            ui.label(RichText::new(format!("> {tool}")).size(11.0).color(STATUS_INFO));
        } else {
            ui.label(RichText::new("Idle at desk").size(11.0).color(TEXT_MUTED));
        }
    });
}

/// Renders the full-window 2D Lite board (0 3D entities -> ultra-low RAM <= 110 MB)
/// or the compact `/panel` view.
pub fn draw_lite_2d_view(ctx: &egui::Context, state: &mut UiState, i18n: &I18nStore) {
    let compact = state.view_mode == ViewMode::PanelCompact;

    egui::TopBottomPanel::top("lite_header")
        .frame(
            egui::Frame::NONE
                .fill(Color32::from_rgb(20, 24, 34))
                .inner_margin(egui::Margin::symmetric(16, 10))
                .stroke(Stroke::new(1.0_f32, Color32::from_rgb(48, 55, 74))),
        )
        .show(ctx, |ui| {
            ui.horizontal(|ui| {
                status_dot(ui, ACCENT_TERRACOTTA, 4.5);
                ui.label(
                    RichText::new("AGENTS STATION · 2D LITE")
                        .size(14.5)
                        .strong()
                        .color(ACCENT_TERRACOTTA),
                );
                ui.separator();
                ui.label(
                    RichText::new(format!(
                        "{} working · {} waiting · ${:.2} today · {:.0} MB",
                        state.working_count(),
                        state.waiting_count(),
                        state.burn.today_usd,
                        state.sys.private_mb
                    ))
                    .size(12.0)
                    .color(TEXT_MUTED),
                );

                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    if ui
                        .button(RichText::new("Switch to 3D Office").size(12.0).strong())
                        .clicked()
                    {
                        state.view_mode = ViewMode::Mode3D;
                        state.push_action(UiAction::SetViewMode(ViewMode::Mode3D));
                    }
                    if ui
                        .selectable_label(compact, RichText::new("Compact /panel").size(11.5))
                        .clicked()
                    {
                        state.view_mode = if compact {
                            ViewMode::Lite2D
                        } else {
                            ViewMode::PanelCompact
                        };
                    }
                    if ui
                        .button(RichText::new(i18n.lang.to_uppercase()).size(11.5))
                        .clicked()
                    {
                        let next = match i18n.lang.as_str() {
                            "en" => "vi",
                            "vi" => "th",
                            _ => "en",
                        };
                        state.push_action(UiAction::SetLang(next.to_owned()));
                    }
                    if ui.button("Settings").clicked() {
                        state.toggle_drawer(ActiveDrawer::Settings);
                    }
                });
            });
        });

    egui::TopBottomPanel::bottom("lite_dock")
        .frame(
            egui::Frame::NONE
                .fill(Color32::from_rgb(20, 24, 34))
                .inner_margin(egui::Margin::symmetric(16, 10))
                .stroke(Stroke::new(1.0_f32, Color32::from_rgb(48, 55, 74))),
        )
        .show(ctx, |ui| {
            draw_launcher_inline(ui, state);
        });

    egui::CentralPanel::default()
        .frame(
            egui::Frame::NONE
                .fill(BG_DEEP)
                .inner_margin(egui::Margin::symmetric(18, 14)),
        )
        .show(ctx, |ui| {
            egui::ScrollArea::vertical().show(ui, |ui| {
                // Pending permission approval banner row
                if !state.pending_perms.is_empty() {
                    let perms = state.pending_perms.clone();
                    for p in perms {
                        egui::Frame::NONE
                            .fill(Color32::from_rgb(38, 31, 24))
                            .corner_radius(egui::CornerRadius::same(10))
                            .inner_margin(egui::Margin::symmetric(14, 10))
                            .stroke(Stroke::new(1.3_f32, STATUS_WAIT))
                            .show(ui, |ui| {
                                ui.horizontal(|ui| {
                                    ui.label(
                                        RichText::new(format!(
                                            "APPROVAL NEEDED · {} ({})",
                                            p.agent.to_uppercase(),
                                            p.tool
                                        ))
                                        .strong()
                                        .color(STATUS_WAIT),
                                    );
                                    ui.label(RichText::new(&p.summary).color(TEXT_PRIMARY));
                                    ui.with_layout(
                                        egui::Layout::right_to_left(egui::Align::Center),
                                        |ui| {
                                            if ui.button("Deny").clicked() {
                                                state.pending_perms.retain(|x| x.id != p.id);
                                                state.push_action(UiAction::DecidePermission {
                                                    id: p.id.clone(),
                                                    decision: "deny".to_owned(),
                                                });
                                            }
                                            let approve = egui::Button::new(
                                                RichText::new("Approve")
                                                    .color(Color32::BLACK)
                                                    .strong(),
                                            )
                                            .fill(STATUS_WORK);
                                            if ui.add(approve).clicked() {
                                                state.pending_perms.retain(|x| x.id != p.id);
                                                state.push_action(UiAction::DecidePermission {
                                                    id: p.id.clone(),
                                                    decision: "allow".to_owned(),
                                                });
                                            }
                                        },
                                    );
                                });
                            });
                        ui.add_space(10.0);
                    }
                }

                // Active agent cards (clean 2-column split with zero overflow!)
                ui.label(
                    RichText::new(format!("ACTIVE AGENTS ({})", state.sessions.len()))
                        .size(11.5)
                        .strong()
                        .color(TEXT_MUTED),
                );
                ui.add_space(6.0);

                let sessions = state.sessions.clone();
                if compact {
                    for s in &sessions {
                        draw_agent_card(ui, s);
                        ui.add_space(8.0);
                    }
                } else {
                    for pair in sessions.chunks(2) {
                        ui.columns(2, |cols| {
                            draw_agent_card(&mut cols[0], &pair[0]);
                            if let Some(second) = pair.get(1) {
                                draw_agent_card(&mut cols[1], second);
                            }
                        });
                        ui.add_space(10.0);
                    }
                }

                ui.add_space(8.0);

                // Bottom 2-column summary: Live Feed + Host / Burn Gauges
                ui.columns(2, |cols| {
                    card_frame().show(&mut cols[0], |ui| {
                        ui.set_width(ui.available_width());
                        ui.label(
                            RichText::new("LIVE ACTIVITY FEED")
                                .size(11.5)
                                .strong()
                                .color(TEXT_MUTED),
                        );
                        ui.add_space(6.0);
                        for ev in &state.feed {
                            ui.horizontal(|ui| {
                                ui.label(RichText::new(&ev.ts_label).size(10.5).color(TEXT_MUTED));
                                ui.label(
                                    RichText::new(&ev.agent)
                                        .strong()
                                        .size(11.5)
                                        .color(ACCENT_TERRACOTTA),
                                );
                                ui.label(RichText::new(&ev.message).size(11.5).color(TEXT_PRIMARY));
                            });
                            ui.add_space(2.0);
                        }
                    });

                    card_frame().show(&mut cols[1], |ui| {
                        ui.set_width(ui.available_width());
                        ui.label(
                            RichText::new("SYSTEM & OTLP BURN")
                                .size(11.5)
                                .strong()
                                .color(TEXT_MUTED),
                        );
                        ui.add_space(6.0);
                        ui.label(format!(
                            "CPU: {:.1}% · Host RAM: {:.1}/{:.1} GB · Disk: {:.0}%",
                            state.sys.cpu_pct,
                            state.sys.mem_used_gb,
                            state.sys.mem_total_gb,
                            state.sys.disk_pct
                        ));
                        ui.add_space(2.0);
                        ui.label(format!(
                            "Today Burn: ${:.2} · 30d: ${:.2} · Cache Read: {}",
                            state.burn.today_usd, state.burn.month_usd, state.burn.cache_read_tokens
                        ));
                        ui.add_space(4.0);
                        ui.label(
                            RichText::new("3D World Unloaded · Zero PowerShell · Low-RAM 2D Lite")
                                .size(11.5)
                                .color(STATUS_WORK),
                        );
                    });
                });
            });
        });
}
