use crate::i18n::I18nStore;
use crate::state::{ActiveDrawer, UiAction, UiState};
use crate::theme::{
    ACCENT, ACCENT_HI, AMBER, BG, BLUE, CARD, CARD_2, FAINT, GREEN, INK, LINE, LINE_2, MUTED, RED,
    SIDE, heading_bold,
};
use bevy_egui::egui::{
    self, Align, Color32, CornerRadius, Margin, Rect, RichText, Sense, Stroke, vec2,
};
use serde_json::json;

fn fmt_bytes(bytes: u64, i18n: &I18nStore) -> String {
    if bytes >= 1024 * 1024 * 1024 {
        let s = format!("{:.1}", bytes as f64 / (1024.0 * 1024.0 * 1024.0));
        i18n.tr_vars("time.gb", &[("n", &s)])
    } else {
        let s = format!("{:.0}", (bytes as f64 / (1024.0 * 1024.0)).max(1.0));
        i18n.tr_vars("time.mb", &[("n", &s)])
    }
}

fn level_color(ratio: f32) -> Color32 {
    if ratio >= 0.88 {
        RED
    } else if ratio >= 0.70 {
        AMBER
    } else {
        GREEN
    }
}

fn draw_gauge(ui: &mut egui::Ui, label: &str, big: &str, sub: &str, ratio: f32, col: Color32) {
    ui.horizontal(|ui| {
        ui.label(heading_bold(label, 11.5).color(INK));
        ui.with_layout(egui::Layout::right_to_left(Align::Center), |ui| {
            ui.label(heading_bold(big, 11.5).color(col));
        });
    });
    let (rect, _) = ui.allocate_exact_size(vec2(ui.available_width(), 6.0), Sense::hover());
    ui.painter().rect_filled(rect, 3.0, BG);
    let clamped = ratio.clamp(0.0, 1.0);
    if clamped > 0.0 {
        let fill = Rect::from_min_size(rect.min, vec2((rect.width() * clamped).max(3.0), rect.height()));
        ui.painter().rect_filled(fill, 3.0, col);
    }
    if !sub.is_empty() {
        ui.label(RichText::new(sub).size(10.0).color(MUTED));
    }
    ui.add_space(5.0);
}

fn two_click_button(
    ui: &mut egui::Ui,
    armed: &mut Option<String>,
    key: &str,
    verb: String,
    disabled: bool,
    i18n: &I18nStore,
) -> bool {
    let is_armed = armed.as_deref() == Some(key);
    let label = if is_armed {
        i18n.tr_vars("mac.action.sure", &[("verb", &verb)])
    } else {
        verb
    };
    let btn = egui::Button::new(
        heading_bold(label, 10.5).color(if is_armed { Color32::WHITE } else { INK }),
    )
    .fill(if is_armed { RED } else { CARD_2 })
    .stroke(Stroke::new(1.0_f32, if is_armed { RED } else { LINE_2 }))
    .corner_radius(CornerRadius::same(6));
    if ui.add_enabled(!disabled, btn).clicked() {
        if is_armed {
            *armed = None;
            return true;
        }
        *armed = Some(key.to_owned());
    }
    false
}

/// Full `MacDrawer` matching `mac-drawer.js` + `stranded.js`: CPU/RAM/swap/disk gauges,
/// top process memory table, 2-click safe cleanup/kill buttons, idle auto-close `[Off, 2h, 4h, 8h]`,
/// and Stranded uncommitted/unpushed git worktrees with `Open` / `Remove`.
pub fn draw_mac_drawer(ctx: &egui::Context, state: &mut UiState, i18n: &I18nStore) {
    if !ctx.wants_keyboard_input() && ctx.input(|i| i.key_pressed(egui::Key::Escape)) {
        state.active_drawer = ActiveDrawer::None;
        return;
    }

    if !state.mac_drawer.stranded_loaded && !state.mac_drawer.stranded_loading {
        state.mac_drawer.stranded_loading = true;
        state.push_action(UiAction::FetchStranded { fresh: false });
    }

    let mut open = true;
    egui::Window::new(heading_bold(i18n.tr("ui.macDrawer.title"), 14.0).color(INK))
        .id(egui::Id::new("station_mac_drawer"))
        .open(&mut open)
        .anchor(egui::Align2::RIGHT_TOP, vec2(-14.0, 62.0))
        .default_width(420.0)
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
            ui.horizontal(|ui| {
                let updated = i18n.tr_vars("mac.updated", &[("time", &state.clock_label)]);
                ui.label(RichText::new(updated).size(10.0).color(MUTED));
                ui.with_layout(egui::Layout::right_to_left(Align::Center), |ui| {
                    let ref_btn = egui::Button::new(
                        RichText::new(i18n.tr("ui.macDrawer.refresh")).size(10.5).color(INK),
                    )
                    .fill(CARD_2)
                    .stroke(Stroke::new(1.0_f32, LINE_2))
                    .corner_radius(CornerRadius::same(6));
                    if ui.add(ref_btn).clicked() {
                        state.mac_drawer.stranded_loading = true;
                        state.push_action(UiAction::RefreshTelemetry);
                        state.push_action(UiAction::FetchStranded { fresh: true });
                    }
                });
            });
            ui.add_space(6.0);

            let sys = state.sys.clone();
            let mac = state.mac_drawer.clone();
            let streamer = state.streamer_mode;
            let redacted = i18n.tr("station.streamerRedacted");
            let mut armed = state.mac_drawer.armed_action.clone();
            let mut stranded_confirm = state.mac_drawer.stranded_confirm_id.clone();

            egui::ScrollArea::vertical().max_height(530.0).show(ui, |ui| {
                // 1. CPU / RAM / Swap / Disk gauges
                egui::Frame::NONE
                    .fill(CARD)
                    .corner_radius(CornerRadius::same(10))
                    .inner_margin(Margin::symmetric(10, 9))
                    .stroke(Stroke::new(1.0_f32, LINE))
                    .show(ui, |ui| {
                        ui.set_width(ui.available_width());
                        let ram_ratio = if sys.mem_total_gb > 0.0 {
                            (sys.mem_used_gb / sys.mem_total_gb).clamp(0.0, 1.0)
                        } else {
                            0.0
                        };
                        let pressure_key = match mac.mem_pressure.as_str() {
                            "critical" => "mac.meter.pressure.critical",
                            "warn" => "mac.meter.pressure.warn",
                            _ => "mac.meter.pressure.normal",
                        };
                        let used_s = i18n.tr_vars("time.gb", &[("n", &format!("{:.1}", sys.mem_used_gb))]);
                        let total_s = i18n.tr_vars("time.gb", &[("n", &format!("{:.1}", sys.mem_total_gb))]);
                        let ram_sub = i18n.tr_vars(
                            "mac.meter.ramSub",
                            &[
                                ("used", &used_s),
                                ("total", &total_s),
                                ("pressure", &i18n.tr(pressure_key)),
                            ],
                        );
                        draw_gauge(
                            ui,
                            &i18n.tr("common.ram"),
                            &format!("{:.0}%", ram_ratio * 100.0),
                            &ram_sub,
                            ram_ratio,
                            level_color(ram_ratio),
                        );

                        let swap_ratio = if mac.swap_total_gb > 0.0 {
                            (mac.swap_used_gb / mac.swap_total_gb).clamp(0.0, 1.0)
                        } else {
                            0.0
                        };
                        let swap_big = if mac.swap_total_gb > 0.0 {
                            i18n.tr_vars("time.gb", &[("n", &format!("{:.1}", mac.swap_used_gb))])
                        } else {
                            "0".to_owned()
                        };
                        let swap_sub = if mac.swap_total_gb > 0.0 {
                            let st = i18n.tr_vars("time.gb", &[("n", &format!("{:.1}", mac.swap_total_gb))]);
                            i18n.tr_vars("mac.meter.swapSub", &[("size", &st)])
                        } else {
                            i18n.tr("mac.meter.swapNone")
                        };
                        draw_gauge(
                            ui,
                            &i18n.tr("mac.trend.swap"),
                            &swap_big,
                            &swap_sub,
                            swap_ratio,
                            if mac.swap_used_gb > 8.0 {
                                RED
                            } else if mac.swap_used_gb > 2.0 {
                                AMBER
                            } else {
                                GREEN
                            },
                        );

                        let cpu_ratio = (sys.cpu_pct / 100.0).clamp(0.0, 1.0);
                        let cpu_sub = i18n.tr_vars(
                            "mac.meter.cpuSub",
                            &[
                                ("n", &mac.cpu_cores.max(1).to_string()),
                                ("load", &format!("{:.1}", mac.cpu_load)),
                            ],
                        );
                        draw_gauge(
                            ui,
                            "CPU",
                            &format!("{:.0}%", sys.cpu_pct),
                            &cpu_sub,
                            cpu_ratio,
                            if sys.cpu_pct > 85.0 {
                                RED
                            } else if sys.cpu_pct > 60.0 {
                                AMBER
                            } else {
                                BLUE
                            },
                        );

                        let disk_ratio = (sys.disk_pct / 100.0).clamp(0.0, 1.0);
                        let disk_free = i18n.tr_vars("time.gb", &[("n", &format!("{:.0}", mac.disk_free_gb))]);
                        let disk_tot = i18n.tr_vars("time.gb", &[("n", &format!("{:.0}", mac.disk_total_gb))]);
                        let disk_sub = i18n.tr_vars(
                            "mac.meter.diskSub",
                            &[("free", &disk_free), ("total", &disk_tot)],
                        );
                        draw_gauge(
                            ui,
                            &i18n.tr("mac.meter.disk"),
                            &format!("{:.0}%", sys.disk_pct),
                            &disk_sub,
                            disk_ratio,
                            if disk_ratio > 0.90 { AMBER } else { ACCENT },
                        );

                        let ws_lbl = i18n.tr_vars(
                            "station.privateWs",
                            &[("mb", &format!("{:.1}", sys.private_mb))],
                        );
                        ui.label(RichText::new(ws_lbl).size(10.5).color(GREEN));
                    });

                // 2. Idle auto-close selector [Off, 2h, 4h, 8h]
                ui.add_space(10.0);
                egui::Frame::NONE
                    .fill(CARD)
                    .corner_radius(CornerRadius::same(10))
                    .inner_margin(Margin::symmetric(10, 8))
                    .stroke(Stroke::new(1.0_f32, LINE))
                    .show(ui, |ui| {
                        ui.set_width(ui.available_width());
                        ui.label(heading_bold(i18n.tr("mac.auto.label"), 11.5).color(INK));
                        ui.add_space(4.0);
                        ui.columns(4, |cols| {
                            for (idx, h) in [0u8, 2, 4, 8].into_iter().enumerate() {
                                cols[idx].push_id(("auto_h", idx), |ui| {
                                    let active = mac.auto_close_hours == h;
                                    let lbl = if h == 0 {
                                        i18n.tr("mac.auto.off")
                                    } else {
                                        i18n.tr_vars("mac.auto.hours", &[("n", &h.to_string())])
                                    };
                                    let btn = egui::Button::new(
                                        heading_bold(lbl, 11.0).color(if active { INK } else { MUTED }),
                                    )
                                    .fill(if active { CARD_2 } else { BG })
                                    .stroke(Stroke::new(1.0_f32, if active { ACCENT } else { LINE }))
                                    .corner_radius(CornerRadius::same(7))
                                    .min_size(vec2(ui.available_width(), 24.0));
                                    if ui.add(btn).clicked() && !active {
                                        state.mac_drawer.auto_close_hours = h;
                                        state.push_action(UiAction::SetSysAutoClose(h));
                                    }
                                });
                            }
                        });
                        ui.add_space(3.0);
                        ui.label(RichText::new(i18n.tr("mac.auto.note")).size(10.0).color(FAINT));
                    });

                // 3. Claude sessions processes (2-click safe close)
                if !mac.claude_procs.is_empty() {
                    ui.add_space(10.0);
                    ui.label(heading_bold(i18n.tr("mac.group.sessions"), 12.0).color(INK));
                    ui.add_space(4.0);
                    for (idx, p) in mac.claude_procs.iter().enumerate() {
                        ui.push_id(("cproc", idx, p.pid), |ui| {
                            egui::Frame::NONE
                                .fill(CARD)
                                .corner_radius(CornerRadius::same(8))
                                .inner_margin(Margin::symmetric(9, 6))
                                .stroke(Stroke::new(1.0_f32, LINE))
                                .show(ui, |ui| {
                                    ui.set_width(ui.available_width());
                                    ui.horizontal(|ui| {
                                        let nm = if streamer { redacted.as_str() } else { p.name.as_str() };
                                        ui.label(heading_bold(nm, 11.5).color(INK));
                                        ui.with_layout(egui::Layout::right_to_left(Align::Center), |ui| {
                                            let key = format!("kill:{}", p.pid);
                                            if two_click_button(
                                                ui,
                                                &mut armed,
                                                &key,
                                                i18n.tr("common.close"),
                                                p.busy,
                                                i18n,
                                            ) {
                                                state.push_action(UiAction::SysAction {
                                                    action: "kill".to_owned(),
                                                    target: json!(p.pid),
                                                    force: false,
                                                    start: (p.start > 0).then_some(p.start),
                                                });
                                            }
                                            ui.label(
                                                RichText::new(fmt_bytes(p.mem_bytes, i18n))
                                                    .size(10.5)
                                                    .color(MUTED),
                                            );
                                        });
                                    });
                                });
                            ui.add_space(3.0);
                        });
                    }
                }

                // 4. Top process / app memory table
                if !mac.apps.is_empty() {
                    ui.add_space(10.0);
                    ui.label(heading_bold(i18n.tr("mac.apps.title"), 12.0).color(INK));
                    ui.add_space(4.0);
                    for (idx, a) in mac.apps.iter().take(6).enumerate() {
                        ui.push_id(("app", idx, &a.name), |ui| {
                            egui::Frame::NONE
                                .fill(CARD)
                                .corner_radius(CornerRadius::same(8))
                                .inner_margin(Margin::symmetric(9, 6))
                                .stroke(Stroke::new(1.0_f32, LINE))
                                .show(ui, |ui| {
                                    ui.set_width(ui.available_width());
                                    ui.horizontal(|ui| {
                                        ui.vertical(|ui| {
                                            ui.label(heading_bold(&a.name, 11.5).color(INK));
                                            let sub = i18n.tr_vars(
                                                "mac.apps.procs",
                                                &[
                                                    ("n", &a.count.max(1).to_string()),
                                                    ("cpu", &format!("{:.0}", a.cpu)),
                                                ],
                                            );
                                            ui.label(RichText::new(sub).size(10.0).color(MUTED));
                                        });
                                        ui.with_layout(egui::Layout::right_to_left(Align::Center), |ui| {
                                            if a.can_quit {
                                                let key = format!("quit:{}", a.name);
                                                if two_click_button(
                                                    ui,
                                                    &mut armed,
                                                    &key,
                                                    i18n.tr("mac.verb.quit"),
                                                    false,
                                                    i18n,
                                                ) {
                                                    state.push_action(UiAction::SysAction {
                                                        action: "quit".to_owned(),
                                                        target: json!(a.name),
                                                        force: false,
                                                        start: None,
                                                    });
                                                }
                                            } else {
                                                ui.label(
                                                    RichText::new(i18n.tr("mac.apps.kept"))
                                                        .size(10.0)
                                                        .color(FAINT),
                                                );
                                            }
                                            ui.label(
                                                RichText::new(fmt_bytes(a.mem_bytes, i18n))
                                                    .size(10.5)
                                                    .color(INK),
                                            );
                                        });
                                    });
                                });
                            ui.add_space(3.0);
                        });
                    }
                }

                // 5. Running dev servers (2-click Stop)
                ui.add_space(10.0);
                ui.label(heading_bold(i18n.tr("mac.dev.title"), 12.0).color(INK));
                ui.add_space(4.0);
                if mac.dev_servers.is_empty() && sys.ports.is_empty() {
                    ui.label(RichText::new(i18n.tr("mac.dev.none")).size(11.0).color(MUTED));
                } else if !mac.dev_servers.is_empty() {
                    for (idx, d) in mac.dev_servers.iter().enumerate() {
                        ui.push_id(("dev", idx, d.pid), |ui| {
                            egui::Frame::NONE
                                .fill(CARD)
                                .corner_radius(CornerRadius::same(8))
                                .inner_margin(Margin::symmetric(9, 6))
                                .stroke(Stroke::new(1.0_f32, LINE))
                                .show(ui, |ui| {
                                    ui.set_width(ui.available_width());
                                    ui.horizontal(|ui| {
                                        let ports_s = d
                                            .ports
                                            .iter()
                                            .map(u16::to_string)
                                            .collect::<Vec<_>>()
                                            .join(", ");
                                        let title = i18n.tr_vars(
                                            "mac.dev.name",
                                            &[
                                                ("label", &d.label),
                                                ("ports", &ports_s),
                                                ("n", &d.ports.len().to_string()),
                                            ],
                                        );
                                        ui.label(heading_bold(title, 11.5).color(INK));
                                        ui.with_layout(egui::Layout::right_to_left(Align::Center), |ui| {
                                            let key = format!("dev:{}", d.pid);
                                            if two_click_button(
                                                ui,
                                                &mut armed,
                                                &key,
                                                i18n.tr("mac.verb.stop"),
                                                false,
                                                i18n,
                                            ) {
                                                state.push_action(UiAction::SysAction {
                                                    action: "kill".to_owned(),
                                                    target: json!(d.pid),
                                                    force: false,
                                                    start: (d.start > 0).then_some(d.start),
                                                });
                                            }
                                            ui.label(
                                                RichText::new(fmt_bytes(d.mem_bytes, i18n))
                                                    .size(10.5)
                                                    .color(MUTED),
                                            );
                                        });
                                    });
                                    if !d.dir.is_empty() {
                                        let dir_s = if streamer { redacted.as_str() } else { d.dir.as_str() };
                                        ui.label(RichText::new(dir_s).size(10.0).color(FAINT));
                                    }
                                });
                            ui.add_space(3.0);
                        });
                    }
                } else {
                    for (idx, (port, label)) in sys.ports.iter().enumerate() {
                        ui.push_id(("port", idx, port), |ui| {
                            ui.horizontal(|ui| {
                                ui.label(heading_bold(format!(":{port}"), 11.5).color(GREEN));
                                ui.label(RichText::new(label).size(11.0).color(MUTED));
                            });
                        });
                    }
                }

                // 6. Stranded uncommitted/unpushed git worktrees (stranded.js)
                ui.add_space(10.0);
                ui.label(heading_bold(i18n.tr("mac.stranded.title"), 12.0).color(INK));
                ui.label(RichText::new(i18n.tr("mac.stranded.hint")).size(10.0).color(MUTED));
                ui.add_space(4.0);
                if mac.stranded_loading && !mac.stranded_loaded {
                    ui.label(RichText::new(i18n.tr("mac.stranded.looking")).size(11.0).color(MUTED));
                } else if mac.stranded.is_empty() {
                    let key = if mac.stranded_repos > 0 {
                        "mac.stranded.none"
                    } else {
                        "mac.stranded.noRepos"
                    };
                    ui.label(RichText::new(i18n.tr(key)).size(11.0).color(MUTED));
                } else {
                    for (idx, it) in mac.stranded.iter().enumerate() {
                        ui.push_id(("stranded", idx, &it.id), |ui| {
                            egui::Frame::NONE
                                .fill(CARD)
                                .corner_radius(CornerRadius::same(8))
                                .inner_margin(Margin::symmetric(9, 7))
                                .stroke(Stroke::new(
                                    1.0_f32,
                                    if it.work { AMBER } else { LINE },
                                ))
                                .show(ui, |ui| {
                                    ui.set_width(ui.available_width());
                                    let repo_disp = if streamer { redacted.as_str() } else { it.repo.as_str() };
                                    let br = if it.branch.is_empty() {
                                        i18n.tr_vars("mac.stranded.detached", &[("sha", "HEAD")])
                                    } else {
                                        it.branch.clone()
                                    };
                                    ui.horizontal(|ui| {
                                        ui.label(heading_bold(format!("{repo_disp} · {br}"), 11.5).color(INK));
                                        if it.main {
                                            ui.label(RichText::new(i18n.tr("mac.stranded.main")).size(9.5).color(BLUE));
                                        }
                                        if it.in_use {
                                            ui.label(RichText::new(i18n.tr("mac.stranded.inUse")).size(9.5).color(GREEN));
                                        }
                                    });
                                    let path_disp = if streamer { redacted.as_str() } else { it.path.as_str() };
                                    ui.label(RichText::new(path_disp).size(10.0).color(FAINT));

                                    let mut facts = Vec::new();
                                    if it.dirty > 0 {
                                        facts.push(i18n.tr_count("mac.stranded.dirty", it.dirty));
                                    }
                                    if it.ahead > 0 {
                                        facts.push(i18n.tr_count("mac.stranded.ahead", it.ahead as usize));
                                    }
                                    if it.main && it.stashes > 0 {
                                        facts.push(i18n.tr_count("mac.stranded.stashes", it.stashes));
                                    }
                                    if facts.is_empty() {
                                        facts.push(i18n.tr("mac.stranded.clean"));
                                    }
                                    ui.label(
                                        RichText::new(facts.join(" · "))
                                            .size(10.5)
                                            .color(if it.work { AMBER } else { MUTED }),
                                    );

                                    ui.add_space(4.0);
                                    ui.horizontal(|ui| {
                                        let open_btn = egui::Button::new(
                                            RichText::new(i18n.tr("mac.stranded.open")).size(10.5).color(INK),
                                        )
                                        .fill(CARD_2)
                                        .stroke(Stroke::new(1.0_f32, LINE_2))
                                        .corner_radius(CornerRadius::same(6));
                                        if ui.add(open_btn).clicked() {
                                            state.push_action(UiAction::OpenStranded(it.id.clone()));
                                        }
                                        if it.removable && !it.in_use {
                                            let confirming = stranded_confirm.as_deref() == Some(it.id.as_str());
                                            let rm_lbl = if confirming {
                                                i18n.tr_vars(
                                                    "mac.action.sure",
                                                    &[("verb", &i18n.tr("mac.stranded.remove"))],
                                                )
                                            } else {
                                                i18n.tr("mac.stranded.remove")
                                            };
                                            let rm_btn = egui::Button::new(
                                                heading_bold(rm_lbl, 10.5)
                                                    .color(if confirming { Color32::WHITE } else { RED }),
                                            )
                                            .fill(if confirming { RED } else { CARD_2 })
                                            .stroke(Stroke::new(1.0_f32, RED))
                                            .corner_radius(CornerRadius::same(6));
                                            if ui.add(rm_btn).clicked() {
                                                if confirming {
                                                    stranded_confirm = None;
                                                    state.push_action(UiAction::RemoveStranded(it.id.clone()));
                                                } else {
                                                    stranded_confirm = Some(it.id.clone());
                                                }
                                            }
                                        }
                                    });
                                });
                            ui.add_space(4.0);
                        });
                    }
                }

                // 7. Disk cleanup (2-click Clean)
                ui.add_space(10.0);
                ui.label(heading_bold(i18n.tr("mac.disk.title"), 12.0).color(INK));
                ui.add_space(4.0);
                if mac.cleanups.is_empty() {
                    ui.label(RichText::new(i18n.tr("mac.disk.none")).size(11.0).color(MUTED));
                } else {
                    for (idx, c) in mac.cleanups.iter().enumerate() {
                        ui.push_id(("clean", idx, &c.id), |ui| {
                            egui::Frame::NONE
                                .fill(CARD)
                                .corner_radius(CornerRadius::same(8))
                                .inner_margin(Margin::symmetric(9, 6))
                                .stroke(Stroke::new(1.0_f32, LINE))
                                .show(ui, |ui| {
                                    ui.set_width(ui.available_width());
                                    ui.horizontal(|ui| {
                                        ui.vertical(|ui| {
                                            ui.label(heading_bold(&c.title, 11.5).color(INK));
                                            ui.label(RichText::new(&c.note).size(10.0).color(MUTED));
                                        });
                                        ui.with_layout(egui::Layout::right_to_left(Align::Center), |ui| {
                                            let key = format!("clean:{}", c.id);
                                            if two_click_button(
                                                ui,
                                                &mut armed,
                                                &key,
                                                i18n.tr("mac.verb.clean"),
                                                false,
                                                i18n,
                                            ) {
                                                state.push_action(UiAction::SysAction {
                                                    action: "clean".to_owned(),
                                                    target: json!(c.id),
                                                    force: false,
                                                    start: None,
                                                });
                                            }
                                            ui.label(
                                                RichText::new(fmt_bytes(c.size_bytes, i18n))
                                                    .size(10.5)
                                                    .color(ACCENT_HI),
                                            );
                                        });
                                    });
                                });
                            ui.add_space(3.0);
                        });
                    }
                }
            });

            state.mac_drawer.armed_action = armed;
            state.mac_drawer.stranded_confirm_id = stranded_confirm;
        });

    if !open {
        state.active_drawer = ActiveDrawer::None;
    }
}
