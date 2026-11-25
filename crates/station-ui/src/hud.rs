use crate::i18n::I18nStore;
use crate::launcher_dock::draw_launcher_dock;
use crate::state::{ActiveDrawer, UiAction, UiState, ViewMode};
use crate::theme::{
    ACCENT_TERRACOTTA, BG_CARD, STATUS_ERR, STATUS_INFO, STATUS_WAIT, STATUS_WORK,
    TEXT_MUTED, TEXT_PRIMARY, card_frame, glass_frame,
};
use bevy_egui::egui::{self, Color32, RichText, Stroke};
use station_3d::{BackdropKind, OfficeLayout};

pub fn draw_hud(ctx: &egui::Context, state: &mut UiState, i18n: &I18nStore) {
    draw_topbar(ctx, state, i18n);
    draw_session_rail(ctx, state);
    draw_permission_banners(ctx, state);
    draw_launcher_dock(ctx, state);
    draw_camera_toolbar(ctx, state);
    draw_toasts(ctx, state);
}

fn status_color(status: &str) -> Color32 {
    match status {
        "working" => STATUS_WORK,
        "waiting" => STATUS_WAIT,
        "error" | "denied" => STATUS_ERR,
        _ => STATUS_INFO,
    }
}

pub fn draw_topbar(ctx: &egui::Context, state: &mut UiState, i18n: &I18nStore) {
    egui::Area::new(egui::Id::new("station_topbar"))
        .anchor(egui::Align2::CENTER_TOP, egui::vec2(0.0, 10.0))
        .show(ctx, |ui| {
            glass_frame().show(ui, |ui| {
                ui.horizontal(|ui| {
                    ui.label(
                        RichText::new("● AGENTS STATION")
                            .size(13.0)
                            .strong()
                            .color(ACCENT_TERRACOTTA),
                    );
                    ui.separator();

                    // Live status counts
                    let working = state.working_count();
                    let waiting = state.waiting_count();
                    ui.label(
                        RichText::new(format!("{working} working"))
                            .size(11.5)
                            .color(STATUS_WORK),
                    );
                    if waiting > 0 {
                        ui.label(
                            RichText::new(format!("{waiting} waiting"))
                                .size(11.5)
                                .color(STATUS_WAIT),
                        );
                    }
                    ui.separator();

                    // Clock + Weather + RAM pill
                    ui.label(
                        RichText::new(format!(
                            "{} · {} {}°C · {:.0} MB",
                            state.clock_label,
                            state.weather_city,
                            state.weather_temp_c,
                            state.sys.private_mb
                        ))
                        .size(11.5)
                        .color(TEXT_MUTED),
                    );
                    ui.separator();

                    // Layout quick switcher
                    for (lbl, lay) in [
                        ("Cozy", OfficeLayout::Cozy),
                        ("Pro", OfficeLayout::Pro),
                        ("HQ", OfficeLayout::Hq),
                    ] {
                        let active = state.layout == lay && state.view_mode == ViewMode::Mode3D;
                        if ui
                            .selectable_label(active, RichText::new(lbl).size(11.5))
                            .clicked()
                        {
                            state.layout = lay;
                            state.view_mode = ViewMode::Mode3D;
                            state.push_action(UiAction::SwitchLayout(lay));
                        }
                    }

                    // 2D Lite toggle
                    let is_2d = state.view_mode == ViewMode::Lite2D;
                    if ui
                        .selectable_label(is_2d, RichText::new("2D Lite").size(11.5))
                        .on_hover_text("True unload 3D world to run ultra-low RAM 2D board")
                        .clicked()
                    {
                        let next = if is_2d {
                            ViewMode::Mode3D
                        } else {
                            ViewMode::Lite2D
                        };
                        state.view_mode = next;
                        state.push_action(UiAction::SetViewMode(next));
                    }

                    ui.separator();

                    // Night & Language toggles
                    if ui
                        .selectable_label(state.night, RichText::new(if state.night { "Night" } else { "Day" }).size(11.5))
                        .clicked()
                    {
                        state.night = !state.night;
                        state.push_action(UiAction::ToggleNight(state.night));
                    }

                    let next_lang = match i18n.lang.as_str() {
                        "en" => "vi",
                        "vi" => "th",
                        _ => "en",
                    };
                    if ui
                        .button(RichText::new(i18n.lang.to_uppercase()).size(11.0))
                        .on_hover_text("Switch language (EN / VI / TH)")
                        .clicked()
                    {
                        state.push_action(UiAction::SetLang(next_lang.to_owned()));
                    }

                    ui.separator();

                    // Drawer triggers
                    for (lbl, drawer) in [
                        ("Sys", ActiveDrawer::System),
                        ("Burn", ActiveDrawer::Burn),
                        ("Vault", ActiveDrawer::Vault),
                        ("Chat", ActiveDrawer::GroupChat),
                        ("Board", ActiveDrawer::Whiteboard),
                        ("QR", ActiveDrawer::PhoneQr),
                        ("⚙", ActiveDrawer::Settings),
                    ] {
                        let open = state.active_drawer == drawer;
                        if ui
                            .selectable_label(open, RichText::new(lbl).size(11.5))
                            .clicked()
                        {
                            state.toggle_drawer(drawer);
                        }
                    }
                });
            });
        });
}

fn draw_session_rail(ctx: &egui::Context, state: &mut UiState) {
    egui::Area::new(egui::Id::new("station_session_rail"))
        .fixed_pos(egui::pos2(12.0, 62.0))
        .show(ctx, |ui| {
            glass_frame().show(ui, |ui| {
                ui.set_width(248.0);
                ui.horizontal(|ui| {
                    ui.label(
                        RichText::new(format!("AGENTS ({})", state.sessions.len()))
                            .size(11.0)
                            .strong()
                            .color(TEXT_MUTED),
                    );
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        if ui
                            .small_button("Timeline")
                            .clicked()
                        {
                            state.toggle_drawer(ActiveDrawer::Timeline);
                        }
                    });
                });
                ui.add_space(4.0);

                let sessions = state.sessions.clone();
                for s in sessions {
                    let selected = state.selected_session.as_deref() == Some(&s.id);
                    let col = status_color(&s.status);
                    let mut frame = card_frame();
                    if selected {
                        frame = frame.stroke(Stroke::new(1.2_f32, ACCENT_TERRACOTTA));
                    }
                    let resp = frame.show(ui, |ui| {
                        ui.set_width(226.0);
                        ui.horizontal(|ui| {
                            ui.label(RichText::new("●").color(col).size(11.0));
                            ui.label(
                                RichText::new(s.agent.to_uppercase())
                                    .strong()
                                    .size(11.5)
                                    .color(TEXT_PRIMARY),
                            );
                            ui.label(RichText::new(&s.project).size(11.0).color(TEXT_MUTED));
                            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                                ui.label(
                                    RichText::new(format!("${:.2}", s.cost_usd))
                                        .size(10.5)
                                        .color(TEXT_MUTED),
                                );
                            });
                        });
                        let prompt = if state.streamer_mode {
                            "••• streamer redaction •••".to_owned()
                        } else {
                            s.last_prompt.clone()
                        };
                        ui.label(RichText::new(prompt).size(11.0).color(TEXT_PRIMARY));
                        if let Some(tool) = &s.tool {
                            ui.label(RichText::new(format!("↳ {tool}")).size(10.0).color(TEXT_MUTED));
                        }
                    });
                    if resp.response.interact(egui::Sense::click()).clicked() {
                        state.selected_session = Some(s.id.clone());
                        state.active_drawer = ActiveDrawer::Session;
                    }
                    ui.add_space(4.0);
                }
            });
        });
}

fn draw_permission_banners(ctx: &egui::Context, state: &mut UiState) {
    if state.pending_perms.is_empty() {
        return;
    }
    egui::Area::new(egui::Id::new("station_perm_banners"))
        .anchor(egui::Align2::RIGHT_TOP, egui::vec2(-14.0, 62.0))
        .show(ctx, |ui| {
            ui.set_width(296.0);
            let perms = state.pending_perms.clone();
            for p in perms {
                let border = match p.kind.as_str() {
                    "denied" => STATUS_ERR,
                    "ci" => STATUS_INFO,
                    _ => STATUS_WAIT,
                };
                egui::Frame::NONE
                    .fill(Color32::from_rgba_unmultiplied(30, 26, 22, 240))
                    .corner_radius(egui::CornerRadius::same(10))
                    .inner_margin(egui::Margin::symmetric(12, 9))
                    .stroke(Stroke::new(1.4_f32, border))
                    .show(ui, |ui| {
                        ui.horizontal(|ui| {
                            ui.label(
                                RichText::new(format!("APPROVAL · {}", p.agent.to_uppercase()))
                                    .strong()
                                    .size(11.0)
                                    .color(border),
                            );
                            ui.label(RichText::new(&p.tool).size(11.0).color(TEXT_PRIMARY));
                        });
                        ui.label(RichText::new(&p.summary).size(11.5).color(TEXT_PRIMARY));
                        ui.add_space(4.0);
                        ui.horizontal(|ui| {
                            let allow_btn = egui::Button::new(
                                RichText::new("Approve").size(11.5).color(Color32::BLACK).strong(),
                            )
                            .fill(STATUS_WORK);
                            if ui.add(allow_btn).clicked() {
                                state.pending_perms.retain(|x| x.id != p.id);
                                state.push_action(UiAction::DecidePermission {
                                    id: p.id.clone(),
                                    decision: "allow".to_owned(),
                                });
                            }
                            let deny_btn = egui::Button::new(
                                RichText::new("Deny").size(11.5).color(TEXT_PRIMARY),
                            )
                            .fill(Color32::from_rgba_unmultiplied(255, 95, 95, 45));
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

fn draw_camera_toolbar(ctx: &egui::Context, state: &mut UiState) {
    egui::Area::new(egui::Id::new("station_camera_toolbar"))
        .anchor(egui::Align2::CENTER_BOTTOM, egui::vec2(0.0, -10.0))
        .show(ctx, |ui| {
            glass_frame().show(ui, |ui| {
                ui.horizontal(|ui| {
                    // Backdrop switcher
                    for (lbl, bk) in [
                        ("City", BackdropKind::City),
                        ("Country", BackdropKind::Countryside),
                        ("Beach", BackdropKind::Beach),
                    ] {
                        if ui
                            .selectable_label(state.backdrop == bk, RichText::new(lbl).size(11.0))
                            .clicked()
                        {
                            state.backdrop = bk;
                            state.push_action(UiAction::SwitchBackdrop(bk));
                        }
                    }
                    ui.separator();

                    if ui
                        .selectable_label(
                            state.first_person,
                            RichText::new(if state.first_person { "Walk (F)" } else { "Iso (F)" }).size(11.0),
                        )
                        .clicked()
                    {
                        state.first_person = !state.first_person;
                        state.push_action(UiAction::ToggleCameraFp);
                    }
                    if ui
                        .selectable_label(state.demo_running, RichText::new("Demo").size(11.0))
                        .clicked()
                    {
                        state.demo_running = !state.demo_running;
                        state.push_action(UiAction::ToggleDemo);
                    }
                    if ui
                        .selectable_label(state.replay_playing, RichText::new("Replay").size(11.0))
                        .clicked()
                    {
                        state.replay_playing = !state.replay_playing;
                        state.push_action(UiAction::ToggleReplay);
                    }
                    if ui.button(RichText::new("Photo").size(11.0)).clicked() {
                        state.push_action(UiAction::TriggerPhoto);
                        state.push_toast("Photo Booth", "Captured office photostrip");
                    }
                    if ui.button(RichText::new("Wrapped").size(11.0)).clicked() {
                        state.toggle_drawer(ActiveDrawer::Wrapped);
                    }
                    if ui
                        .button(RichText::new("Tray").size(11.0).color(TEXT_MUTED))
                        .on_hover_text("Unload 3D world and minimize to Windows System Tray")
                        .clicked()
                    {
                        state.push_action(UiAction::MinimizeToTray);
                    }
                });
            });
        });
}

fn draw_toasts(ctx: &egui::Context, state: &UiState) {
    if state.toasts.is_empty() {
        return;
    }
    egui::Area::new(egui::Id::new("station_toasts"))
        .anchor(egui::Align2::RIGHT_BOTTOM, egui::vec2(-14.0, -56.0))
        .show(ctx, |ui| {
            ui.set_width(270.0);
            for t in &state.toasts {
                egui::Frame::NONE
                    .fill(BG_CARD)
                    .corner_radius(egui::CornerRadius::same(9))
                    .inner_margin(egui::Margin::symmetric(10, 7))
                    .stroke(Stroke::new(1.0_f32, ACCENT_TERRACOTTA))
                    .show(ui, |ui| {
                        ui.label(RichText::new(&t.title).strong().size(11.5).color(TEXT_PRIMARY));
                        ui.label(RichText::new(&t.body).size(10.5).color(TEXT_MUTED));
                    });
                ui.add_space(4.0);
            }
        });
}
