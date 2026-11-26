pub mod group_chat;
pub mod markdown;
pub mod modals;
pub mod session;
pub mod whiteboard;

use crate::i18n::I18nStore;
use crate::state::{ActiveDrawer, GpuPrefUi, UiAction, UiState};
use crate::theme::{
    ACCENT_TERRACOTTA, BG_DEEP, STATUS_ERR, STATUS_INFO, STATUS_WAIT, STATUS_WORK, TEXT_MUTED,
    TEXT_PRIMARY, card_frame, status_dot,
};
use bevy_egui::egui::{self, Color32, RichText, Stroke};
use station_3d::{BackdropKind, ChibiLook, OfficeLayout};

pub fn draw_active_drawer(ctx: &egui::Context, state: &mut UiState, i18n: &I18nStore) {
    if state.timelapse_recording {
        modals::draw_timelapse_overlay(ctx, state, i18n);
    }

    let drawer = state.active_drawer;
    if drawer == ActiveDrawer::None {
        return;
    }

    match drawer {
        ActiveDrawer::Session => {
            session::draw_session_drawer(ctx, state, i18n);
            return;
        }
        ActiveDrawer::GroupChat => {
            group_chat::draw_group_chat_drawer(ctx, state, i18n);
            return;
        }
        ActiveDrawer::Whiteboard => {
            whiteboard::draw_whiteboard_modal(ctx, state, i18n);
            return;
        }
        ActiveDrawer::Wrapped => {
            modals::draw_wrapped_modal(ctx, state, i18n);
            return;
        }
        _ => {}
    }

    let title = match drawer {
        ActiveDrawer::None
        | ActiveDrawer::Session
        | ActiveDrawer::GroupChat
        | ActiveDrawer::Whiteboard
        | ActiveDrawer::Wrapped => return,
        ActiveDrawer::System => "System & Ports Monitor",
        ActiveDrawer::Burn => "OTLP Burn & Tokens",
        ActiveDrawer::Vault => "Prompt Vault & Coach",
        ActiveDrawer::Settings => "Station Settings",
        ActiveDrawer::Timeline => "Activity Timeline",
        ActiveDrawer::PhoneQr => "Phone Companion Pairing",
        ActiveDrawer::AddHarness => "Add Custom GUI App",
    };

    let mut open = true;
    egui::Window::new(RichText::new(title).strong().color(TEXT_PRIMARY))
        .open(&mut open)
        .anchor(egui::Align2::RIGHT_TOP, egui::vec2(-14.0, 62.0))
        .default_width(360.0)
        .max_height(520.0)
        .collapsible(false)
        .resizable(false)
        .show(ctx, |ui| {
            egui::ScrollArea::vertical().show(ui, |ui| match drawer {
                ActiveDrawer::None
                | ActiveDrawer::Session
                | ActiveDrawer::GroupChat
                | ActiveDrawer::Whiteboard
                | ActiveDrawer::Wrapped => {}
                ActiveDrawer::System => draw_system_drawer(ui, state),
                ActiveDrawer::Burn => draw_burn_drawer(ui, state),
                ActiveDrawer::Vault => draw_vault_drawer(ui, state),
                ActiveDrawer::Settings => draw_settings_drawer(ui, state, i18n),
                ActiveDrawer::Timeline => draw_timeline_drawer(ui, state),
                ActiveDrawer::PhoneQr => draw_phone_qr_drawer(ui, state),
                ActiveDrawer::AddHarness => draw_add_harness_drawer(ui, state),
            });
        });

    if !open {
        state.active_drawer = ActiveDrawer::None;
    }
}

fn draw_system_drawer(ui: &mut egui::Ui, state: &UiState) {
    let sys = &state.sys;
    card_frame().show(ui, |ui| {
        ui.label(RichText::new("Host & Process Telemetry").strong().color(TEXT_PRIMARY));
        ui.add_space(4.0);
        metric_bar(ui, "CPU", sys.cpu_pct / 100.0, format!("{:.1}%", sys.cpu_pct), STATUS_INFO);
        let mem_ratio = if sys.mem_total_gb > 0.0 {
            (sys.mem_used_gb / sys.mem_total_gb).clamp(0.0, 1.0)
        } else {
            0.0
        };
        metric_bar(
            ui,
            "Host RAM",
            mem_ratio,
            format!("{:.1} / {:.1} GB", sys.mem_used_gb, sys.mem_total_gb),
            STATUS_WAIT,
        );
        metric_bar(
            ui,
            "Disk",
            (sys.disk_pct / 100.0).clamp(0.0, 1.0),
            format!("{:.0}%", sys.disk_pct),
            ACCENT_TERRACOTTA,
        );
        ui.add_space(4.0);
        ui.label(
            RichText::new(format!(
                "Agents Station Private WS: {:.1} MB · 0 PowerShell",
                sys.private_mb
            ))
            .size(11.5)
            .color(STATUS_WORK),
        );
    });

    ui.add_space(8.0);
    card_frame().show(ui, |ui| {
        ui.label(RichText::new("Active Dev Ports").strong().color(TEXT_PRIMARY));
        for (port, proc_name) in &sys.ports {
            ui.horizontal(|ui| {
                ui.label(RichText::new(format!(":{port}")).strong().color(STATUS_WORK));
                ui.label(RichText::new(proc_name).color(TEXT_MUTED));
            });
        }
    });
}

fn draw_burn_drawer(ui: &mut egui::Ui, state: &UiState) {
    let b = &state.burn;
    card_frame().show(ui, |ui| {
        ui.horizontal(|ui| {
            ui.vertical(|ui| {
                ui.label(RichText::new("TODAY").size(10.0).color(TEXT_MUTED));
                ui.label(
                    RichText::new(format!("${:.2}", b.today_usd))
                        .size(20.0)
                        .strong()
                        .color(ACCENT_TERRACOTTA),
                );
            });
            ui.add_space(20.0);
            ui.vertical(|ui| {
                ui.label(RichText::new("30 DAYS").size(10.0).color(TEXT_MUTED));
                ui.label(
                    RichText::new(format!("${:.2}", b.month_usd))
                        .size(20.0)
                        .strong()
                        .color(TEXT_PRIMARY),
                );
            });
        });
        ui.add_space(6.0);
        ui.label(
            RichText::new(format!(
                "Input: {} · Output: {} · Cache Read: {}",
                b.input_tokens, b.output_tokens, b.cache_read_tokens
            ))
            .size(11.0)
            .color(TEXT_MUTED),
        );
    });
    ui.add_space(8.0);
    card_frame().show(ui, |ui| {
        ui.label(RichText::new("Spend by Model").strong().color(TEXT_PRIMARY));
        for (model, usd) in &b.per_model {
            ui.horizontal(|ui| {
                ui.label(RichText::new(model).size(11.5).color(TEXT_PRIMARY));
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    ui.label(RichText::new(format!("${usd:.2}")).size(11.5).color(STATUS_WORK));
                });
            });
        }
    });
}

fn draw_vault_drawer(ui: &mut egui::Ui, state: &mut UiState) {
    ui.horizontal(|ui| {
        ui.label("Search:");
        ui.text_edit_singleline(&mut state.vault_query);
    });
    ui.add_space(6.0);
    let q = state.vault_query.to_lowercase();
    for (title, body) in &state.vault_results {
        if !q.is_empty()
            && !title.to_lowercase().contains(&q)
            && !body.to_lowercase().contains(&q)
        {
            continue;
        }
        card_frame().show(ui, |ui| {
            ui.label(RichText::new(title).strong().color(ACCENT_TERRACOTTA));
            ui.label(RichText::new(body).size(11.5).color(TEXT_PRIMARY));
        });
        ui.add_space(4.0);
    }
}

fn draw_settings_drawer(ui: &mut egui::Ui, state: &mut UiState, i18n: &I18nStore) {
    card_frame().show(ui, |ui| {
        ui.label(RichText::new("Language (i18n)").strong().color(TEXT_PRIMARY));
        ui.horizontal(|ui| {
            for (code, label) in [("en", "English"), ("vi", "Tiếng Việt"), ("th", "ภาษาไทย")] {
                if ui.selectable_label(i18n.lang == code, label).clicked() {
                    state.push_action(UiAction::SetLang(code.to_owned()));
                }
            }
        });
    });
    ui.add_space(6.0);
    card_frame().show(ui, |ui| {
        ui.label(RichText::new("Office & Chibi").strong().color(TEXT_PRIMARY));
        ui.horizontal(|ui| {
            ui.label("Layout:");
            for (lbl, lay) in [
                ("Cozy", OfficeLayout::Cozy),
                ("Pro", OfficeLayout::Pro),
                ("HQ", OfficeLayout::Hq),
            ] {
                if ui.selectable_label(state.layout == lay, lbl).clicked() {
                    state.layout = lay;
                    state.push_action(UiAction::SwitchLayout(lay));
                }
            }
        });
        ui.horizontal(|ui| {
            ui.label("Backdrop:");
            for (lbl, bk) in [
                ("City", BackdropKind::City),
                ("Country", BackdropKind::Countryside),
                ("Beach", BackdropKind::Beach),
            ] {
                if ui.selectable_label(state.backdrop == bk, lbl).clicked() {
                    state.backdrop = bk;
                    state.push_action(UiAction::SwitchBackdrop(bk));
                }
            }
        });
        ui.horizontal(|ui| {
            ui.label("Chibi:");
            for (lbl, lk) in [("Toy", ChibiLook::Toy), ("Pixar", ChibiLook::Pixar)] {
                if ui.selectable_label(state.chibi_look == lk, lbl).clicked() {
                    state.chibi_look = lk;
                    state.push_action(UiAction::SetChibiLook(lk));
                }
            }
        });
    });
    ui.add_space(6.0);
    card_frame().show(ui, |ui| {
        ui.label(RichText::new("Performance & Privacy").strong().color(TEXT_PRIMARY));
        ui.horizontal(|ui| {
            ui.label("GPU:");
            for g in [GpuPrefUi::Auto, GpuPrefUi::Integrated, GpuPrefUi::Discrete] {
                if ui.selectable_label(state.gpu_pref == g, g.label()).clicked() {
                    state.gpu_pref = g;
                    state.push_action(UiAction::SetGpuPref(g));
                }
            }
        });
        ui.horizontal(|ui| {
            ui.label("Weather:");
            let active_code = station_3d::life::weather_city_display(&state.weather_city).0;
            for (code, lbl) in [
                ("hcm", "HCM"),
                ("hn", "HN"),
                ("dn", "DN"),
                ("dl", "DL"),
                ("nt", "NT"),
            ] {
                let active = active_code.eq_ignore_ascii_case(code);
                if ui.selectable_label(active, lbl).clicked() {
                    state.push_action(UiAction::SetWeatherCity(code.to_owned()));
                }
            }
        });
        if ui.checkbox(&mut state.sound_on, "Office SFX Audio").changed() {
            state.push_action(UiAction::SetSound(state.sound_on));
        }
        ui.checkbox(&mut state.streamer_mode, "Streamer Mode (redact prompts)");
        ui.checkbox(&mut state.auto_close_tray, "Close to Tray + True Unload 3D");
    });
}

fn draw_timeline_drawer(ui: &mut egui::Ui, state: &UiState) {
    for ev in &state.feed {
        let col = match ev.level.as_str() {
            "ok" => STATUS_WORK,
            "warn" => STATUS_WAIT,
            "err" => STATUS_ERR,
            _ => STATUS_INFO,
        };
        card_frame().show(ui, |ui| {
            ui.horizontal(|ui| {
                status_dot(ui, col, 3.6);
                ui.label(RichText::new(&ev.agent).strong().color(TEXT_PRIMARY));
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    ui.label(RichText::new(&ev.ts_label).size(10.0).color(TEXT_MUTED));
                });
            });
            ui.label(RichText::new(&ev.message).size(11.5).color(TEXT_PRIMARY));
        });
        ui.add_space(4.0);
    }
}

fn draw_phone_qr_drawer(ui: &mut egui::Ui, state: &UiState) {
    card_frame().show(ui, |ui| {
        ui.label(RichText::new("Phone Companion PWA").strong().color(TEXT_PRIMARY));
        ui.label(
            RichText::new(format!("http://127.0.0.1:{}/phone", state.server_port))
                .color(STATUS_INFO),
        );
        ui.add_space(6.0);
        ui.label(
            RichText::new(format!("PIN: {}", state.pairing_pin))
                .size(22.0)
                .strong()
                .color(ACCENT_TERRACOTTA),
        );
        ui.label(
            RichText::new("Approve permissions and inspect sessions from your phone over LAN.")
                .size(11.0)
                .color(TEXT_MUTED),
        );
    });
}

fn draw_add_harness_drawer(ui: &mut egui::Ui, state: &mut UiState) {
    ui.label(RichText::new("Register any Windows desktop .exe or .lnk app:").size(11.5).color(TEXT_MUTED));
    ui.add_space(4.0);
    ui.label("ID (slug):");
    ui.text_edit_singleline(&mut state.new_harness_id);
    ui.label("Display Label:");
    ui.text_edit_singleline(&mut state.new_harness_label);
    ui.label("Executable / Shortcut Path:");
    ui.text_edit_singleline(&mut state.new_harness_path);
    ui.add_space(6.0);
    if ui.button("Save Custom App").clicked()
        && !state.new_harness_id.trim().is_empty()
        && !state.new_harness_path.trim().is_empty()
    {
        let id = std::mem::take(&mut state.new_harness_id).trim().to_owned();
        let label = {
            let l = std::mem::take(&mut state.new_harness_label).trim().to_owned();
            if l.is_empty() { id.clone() } else { l }
        };
        let path = std::mem::take(&mut state.new_harness_path).trim().to_owned();
        state.push_action(UiAction::AddCustomHarness {
            id,
            label: label.clone(),
            path,
            hook_agent: None,
        });
        state.push_toast("Custom App Added", label);
        state.active_drawer = ActiveDrawer::None;
    }

    let custom_items: Vec<_> = state.harnesses.iter().filter(|h| h.custom).cloned().collect();
    if !custom_items.is_empty() {
        ui.add_space(8.0);
        ui.label(RichText::new("Custom Apps").strong().color(TEXT_PRIMARY));
        for item in custom_items {
            ui.horizontal(|ui| {
                ui.label(&item.label);
                if ui.small_button("Remove").clicked() {
                    state.push_action(UiAction::RemoveCustomHarness(item.id.clone()));
                }
            });
        }
    }
}

fn metric_bar(ui: &mut egui::Ui, label: &str, frac: f32, val_text: String, color: Color32) {
    ui.horizontal(|ui| {
        ui.label(RichText::new(label).size(11.0).color(TEXT_MUTED));
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            ui.label(RichText::new(val_text).size(11.0).color(TEXT_PRIMARY));
        });
    });
    let (rect, _) = ui.allocate_exact_size(egui::vec2(ui.available_width(), 6.0), egui::Sense::hover());
    ui.painter().rect_filled(rect, 3.0, BG_DEEP);
    let mut fill = rect;
    fill.set_width(rect.width() * frac.clamp(0.0, 1.0));
    ui.painter().rect_filled(fill, 3.0, color);
    ui.painter().rect_stroke(rect, 3.0, Stroke::new(0.5_f32, Color32::from_gray(70)), egui::StrokeKind::Inside);
    ui.add_space(4.0);
}
