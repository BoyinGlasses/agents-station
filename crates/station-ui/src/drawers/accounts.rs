use crate::i18n::I18nStore;
use crate::state::{
    ActiveDrawer, AiAccountEntry, AiAccountsState, AiProviderTab, QuotaWindowUi, UiAction, UiState,
};
use crate::theme::{
    ACCENT, ACCENT_HI, AMBER, BG, CARD, CARD_2, FAINT, GREEN, INK, LINE, LINE_2, MUTED, RED, SIDE,
    heading_bold,
};
use bevy_egui::egui::{
    self, Align, Color32, CornerRadius, Margin, Rect, RichText, Sense, Stroke, vec2,
};
use serde_json::json;

/// Switches active AI provider tab (`Claude` | `Codex` | `Antigravity`) and clears transient
/// confirmation/edit state belonging to the previous tab.
pub fn set_active_provider_tab(accts: &mut AiAccountsState, tab: AiProviderTab) {
    if accts.tab != tab {
        accts.tab = tab;
        accts.remove_armed = None;
        accts.renaming = None;
    }
}

fn quota_color(pct: f32) -> Color32 {
    if pct >= 95.0 {
        RED
    } else if pct >= 80.0 {
        AMBER
    } else {
        GREEN
    }
}

fn draw_quota_window(ui: &mut egui::Ui, w: &QuotaWindowUi, i18n: &I18nStore) {
    let lbl = match w.key.as_str() {
        "session" => i18n.tr("mac.plan.fiveHour"),
        "weekly" => i18n.tr("mac.plan.week"),
        _ => w.label.clone(),
    };
    let col = quota_color(w.pct);
    ui.horizontal(|ui| {
        ui.label(RichText::new(lbl).size(10.5).color(MUTED));
        let (bar_r, _) =
            ui.allocate_exact_size(vec2((ui.available_width() - 42.0).max(50.0), 6.0), Sense::hover());
        ui.painter().rect_filled(bar_r, 3.0, BG);
        let frac = (w.pct / 100.0).clamp(0.0, 1.0);
        if frac > 0.0 {
            let fill = Rect::from_min_size(bar_r.min, vec2((bar_r.width() * frac).max(3.0), bar_r.height()));
            ui.painter().rect_filled(fill, 3.0, col);
        }
        ui.with_layout(egui::Layout::right_to_left(Align::Center), |ui| {
            ui.label(heading_bold(format!("{:.0}%", w.pct), 10.5).color(col));
        });
    });
}

fn display_account_name(entry: &AiAccountEntry, provider: AiProviderTab, i18n: &I18nStore) -> String {
    if !entry.label.is_empty() {
        entry.label.clone()
    } else if !entry.email.is_empty() {
        entry.email.split('@').next().unwrap_or(&entry.email).to_owned()
    } else if !entry.slug.is_empty() {
        entry.slug.clone()
    } else if provider == AiProviderTab::Claude {
        i18n.tr("mac.plan.mainAccount")
    } else {
        i18n.tr("ai.title")
    }
}

/// Renders the `AI Accounts` (`Claude | Codex | Antigravity`) panel matching `ai-accounts.js`
/// inside either the TopBar `Plan` popover or the standalone `ActiveDrawer::Accounts` window.
pub fn draw_accounts_panel(ui: &mut egui::Ui, state: &mut UiState, i18n: &I18nStore) {
    if !state.ai_accounts.loaded {
        state.ai_accounts.loaded = true;
        state.push_action(UiAction::FetchAiAccounts);
    }

    // Provider segmented tabs: Claude | Codex | Antigravity
    egui::Frame::NONE
        .fill(CARD)
        .corner_radius(CornerRadius::same(11))
        .inner_margin(Margin::same(3))
        .stroke(Stroke::new(1.0_f32, LINE))
        .show(ui, |ui| {
            ui.columns(3, |cols| {
                for (idx, (tab, key)) in [
                    (AiProviderTab::Claude, "station.planProviders.claude"),
                    (AiProviderTab::Codex, "station.planProviders.codex"),
                    (AiProviderTab::Antigravity, "station.planProviders.antigravity"),
                ]
                .into_iter()
                .enumerate()
                {
                    cols[idx].push_id(("provider_tab", idx), |ui| {
                        let active = state.ai_accounts.tab == tab;
                        let btn = egui::Button::new(
                            heading_bold(i18n.tr(key), 11.5).color(if active { INK } else { MUTED }),
                        )
                        .fill(if active { CARD_2 } else { Color32::TRANSPARENT })
                        .stroke(if active {
                            Stroke::new(1.0_f32, ACCENT)
                        } else {
                            Stroke::NONE
                        })
                        .corner_radius(CornerRadius::same(8))
                        .min_size(vec2(ui.available_width(), 28.0));
                        if ui.add(btn).clicked() {
                            set_active_provider_tab(&mut state.ai_accounts, tab);
                            state.plan_tab = tab.as_u8();
                        }
                    });
                }
            });
        });

    ui.add_space(8.0);
    let provider = state.ai_accounts.tab;
    let streamer = state.streamer_mode;
    let redacted = i18n.tr("station.streamerRedacted");

    let entries: Vec<AiAccountEntry> = match provider {
        AiProviderTab::Claude => state.ai_accounts.claude.clone(),
        AiProviderTab::Codex => state.ai_accounts.codex.clone(),
        AiProviderTab::Antigravity => state.ai_accounts.ag.clone(),
    };

    if provider == AiProviderTab::Codex && state.ai_accounts.codex_seat_note == "apiKey" {
        ui.label(RichText::new(i18n.tr("ai.codex.apiKey")).size(10.5).color(AMBER));
        ui.add_space(4.0);
    }
    if provider == AiProviderTab::Antigravity && state.ai_accounts.ag_note == "noDb" {
        let msg = i18n.tr_vars("ai.ag.noDb", &[("app", "Antigravity")]);
        ui.label(RichText::new(msg).size(10.5).color(AMBER));
        ui.add_space(4.0);
    }

    if entries.is_empty() {
        ui.label(RichText::new(i18n.tr("ui.topBar.planEmpty")).size(11.0).color(MUTED));
    } else {
        let total = entries.len();
        for (idx, entry) in entries.iter().enumerate() {
            ui.push_id(("acct_row", provider.as_u8(), idx, &entry.slug), |ui| {
                egui::Frame::NONE
                    .fill(CARD)
                    .corner_radius(CornerRadius::same(9))
                    .inner_margin(Margin::symmetric(9, 7))
                    .stroke(Stroke::new(
                        1.0_f32,
                        if entry.active { GREEN } else { LINE },
                    ))
                    .show(ui, |ui| {
                        ui.set_width(ui.available_width());
                        let is_renaming = state
                            .ai_accounts
                            .renaming
                            .as_ref()
                            .is_some_and(|(p, s)| *p == provider && *s == entry.slug);

                        if is_renaming {
                            ui.horizontal(|ui| {
                                ui.add(
                                    egui::TextEdit::singleline(&mut state.ai_accounts.rename_draft)
                                        .desired_width(140.0),
                                );
                                if ui.small_button(i18n.tr("common.save")).clicked() {
                                    let lbl = state.ai_accounts.rename_draft.trim().to_owned();
                                    let body = if provider == AiProviderTab::Claude {
                                        json!({ "name": entry.slug, "label": lbl })
                                    } else {
                                        json!({ "id": entry.slug, "label": lbl })
                                    };
                                    state.ai_accounts.renaming = None;
                                    state.push_action(UiAction::AiAccountMutate {
                                        provider,
                                        action: "rename".to_owned(),
                                        body,
                                    });
                                }
                                if ui.small_button(i18n.tr("common.cancel")).clicked() {
                                    state.ai_accounts.renaming = None;
                                }
                            });
                        } else {
                            ui.horizontal(|ui| {
                                // Claude order arrows when multiple accounts exist
                                if provider == AiProviderTab::Claude && total > 1 {
                                    if idx > 0
                                        && ui
                                            .small_button("↑")
                                            .on_hover_text(i18n.tr("station.moveUp"))
                                            .clicked()
                                    {
                                        let mut order: Vec<String> =
                                            entries.iter().map(|e| e.slug.clone()).collect();
                                        order.swap(idx, idx - 1);
                                        state.push_action(UiAction::AiAccountMutate {
                                            provider,
                                            action: "order".to_owned(),
                                            body: json!({ "order": order }),
                                        });
                                    }
                                    if idx + 1 < total
                                        && ui
                                            .small_button("↓")
                                            .on_hover_text(i18n.tr("station.moveDown"))
                                            .clicked()
                                    {
                                        let mut order: Vec<String> =
                                            entries.iter().map(|e| e.slug.clone()).collect();
                                        order.swap(idx, idx + 1);
                                        state.push_action(UiAction::AiAccountMutate {
                                            provider,
                                            action: "order".to_owned(),
                                            body: json!({ "order": order }),
                                        });
                                    }
                                }

                                let raw_name = display_account_name(entry, provider, i18n);
                                let disp_name = if streamer && !entry.slug.is_empty() {
                                    redacted.clone()
                                } else {
                                    raw_name.clone()
                                };
                                if ui
                                    .add(
                                        egui::Button::new(heading_bold(&disp_name, 12.0).color(INK))
                                            .frame(false),
                                    )
                                    .on_hover_text(i18n.tr("mac.plan.rename"))
                                    .clicked()
                                {
                                    state.ai_accounts.renaming = Some((provider, entry.slug.clone()));
                                    state.ai_accounts.rename_draft = raw_name;
                                }
                                if !entry.plan.is_empty() {
                                    ui.label(RichText::new(&entry.plan).size(10.0).color(MUTED));
                                }

                                ui.with_layout(egui::Layout::right_to_left(Align::Center), |ui| {
                                    if entry.active {
                                        ui.label(
                                            heading_bold(i18n.tr("mac.plan.inUse"), 10.5).color(GREEN),
                                        );
                                        if provider == AiProviderTab::Claude
                                            && ui
                                                .small_button(i18n.tr("station.saveSeat"))
                                                .clicked()
                                        {
                                            state.push_action(UiAction::AiAccountMutate {
                                                provider,
                                                action: "save".to_owned(),
                                                body: json!({ "name": entry.slug }),
                                            });
                                        }
                                    } else {
                                        // 2-click Remove for non-main non-active accounts
                                        let can_remove =
                                            provider != AiProviderTab::Claude || !entry.slug.is_empty();
                                        if can_remove {
                                            let armed = state
                                                .ai_accounts
                                                .remove_armed
                                                .as_ref()
                                                .is_some_and(|(p, s)| *p == provider && *s == entry.slug);
                                            let rm_lbl = if armed {
                                                i18n.tr("ai.removeSure")
                                            } else {
                                                "×".to_owned()
                                            };
                                            let rm_btn = egui::Button::new(
                                                RichText::new(rm_lbl)
                                                    .size(10.5)
                                                    .color(if armed { Color32::WHITE } else { RED }),
                                            )
                                            .fill(if armed { RED } else { CARD_2 })
                                            .corner_radius(CornerRadius::same(5));
                                            if ui.add(rm_btn).clicked() {
                                                if armed {
                                                    state.ai_accounts.remove_armed = None;
                                                    let body = if provider == AiProviderTab::Claude {
                                                        json!({ "name": entry.slug })
                                                    } else {
                                                        json!({ "id": entry.slug })
                                                    };
                                                    state.push_action(UiAction::AiAccountMutate {
                                                        provider,
                                                        action: "remove".to_owned(),
                                                        body,
                                                    });
                                                } else {
                                                    state.ai_accounts.remove_armed =
                                                        Some((provider, entry.slug.clone()));
                                                }
                                            }
                                        }

                                        let use_btn = egui::Button::new(
                                            heading_bold(i18n.tr("mac.plan.use"), 10.5).color(INK),
                                        )
                                        .fill(CARD_2)
                                        .stroke(Stroke::new(1.0_f32, LINE_2))
                                        .corner_radius(CornerRadius::same(6));
                                        if ui.add(use_btn).clicked() {
                                            let body = if provider == AiProviderTab::Claude {
                                                json!({ "name": entry.slug })
                                            } else {
                                                json!({ "id": entry.slug })
                                            };
                                            state.push_action(UiAction::AiAccountMutate {
                                                provider,
                                                action: "use".to_owned(),
                                                body,
                                            });
                                        }
                                    }
                                });
                            });
                        }

                        // Quota 5h / 7d windows
                        for (widx, w) in entry.windows.iter().enumerate() {
                            ui.push_id(("win", widx), |ui| {
                                ui.add_space(3.0);
                                draw_quota_window(ui, w, i18n);
                            });
                        }
                    });
                ui.add_space(4.0);
            });
        }
    }

    // Add account form / button
    let is_adding = state.ai_accounts.adding_provider == Some(provider);
    if is_adding {
        egui::Frame::NONE
            .fill(CARD)
            .corner_radius(CornerRadius::same(9))
            .inner_margin(Margin::symmetric(9, 8))
            .stroke(Stroke::new(1.0_f32, ACCENT))
            .show(ui, |ui| {
                ui.set_width(ui.available_width());
                ui.label(RichText::new(i18n.tr("mac.add.nameLabel")).size(10.5).color(MUTED));
                ui.add(
                    egui::TextEdit::singleline(&mut state.ai_accounts.add_name_draft)
                        .hint_text(i18n.tr("mac.add.namePh"))
                        .desired_width(ui.available_width()),
                );
                if provider != AiProviderTab::Claude {
                    ui.add_space(3.0);
                    ui.label(RichText::new(i18n.tr("station.emailLabel")).size(10.5).color(MUTED));
                    ui.add(
                        egui::TextEdit::singleline(&mut state.ai_accounts.add_email_draft)
                            .desired_width(ui.available_width()),
                    );
                }
                ui.add_space(6.0);
                ui.horizontal(|ui| {
                    if ui.button(i18n.tr("mac.add.cancel")).clicked() {
                        state.ai_accounts.adding_provider = None;
                        state.ai_accounts.add_name_draft.clear();
                        state.ai_accounts.add_email_draft.clear();
                    }
                    let next_btn = egui::Button::new(
                        heading_bold(i18n.tr("mac.add.next"), 11.0)
                            .color(Color32::from_rgb(0x2A, 0x13, 0x07)),
                    )
                    .fill(ACCENT)
                    .corner_radius(CornerRadius::same(7));
                    if ui.add(next_btn).clicked() {
                        let name = state.ai_accounts.add_name_draft.trim().to_owned();
                        let email = state.ai_accounts.add_email_draft.trim().to_owned();
                        if !name.is_empty() || provider != AiProviderTab::Claude {
                            let (action, body) = match provider {
                                AiProviderTab::Claude => ("prepare", json!({ "name": name })),
                                AiProviderTab::Codex | AiProviderTab::Antigravity => {
                                    ("add", json!({ "label": name, "email": email }))
                                }
                            };
                            state.ai_accounts.adding_provider = None;
                            state.ai_accounts.add_name_draft.clear();
                            state.ai_accounts.add_email_draft.clear();
                            state.push_action(UiAction::AiAccountMutate {
                                provider,
                                action: action.to_owned(),
                                body,
                            });
                        }
                    }
                });
            });
    } else {
        let add_key = match provider {
            AiProviderTab::Claude => "mac.add.button",
            AiProviderTab::Codex => "ai.codex.add",
            AiProviderTab::Antigravity => "ai.ag.add",
        };
        let add_btn = egui::Button::new(heading_bold(i18n.tr(add_key), 11.5).color(ACCENT_HI))
            .fill(CARD)
            .stroke(Stroke::new(1.0_f32, LINE_2))
            .corner_radius(CornerRadius::same(8))
            .min_size(vec2(ui.available_width(), 28.0));
        if ui.add(add_btn).clicked() {
            state.ai_accounts.adding_provider = Some(provider);
        }
    }

    // Auto-switch controls
    ui.add_space(6.0);
    match provider {
        AiProviderTab::Claude => {
            let mut auto = state.ai_accounts.claude_auto;
            if ui.checkbox(&mut auto, i18n.tr("mac.plan.autoTitle")).changed() {
                state.ai_accounts.claude_auto = auto;
                state.push_action(UiAction::AiAccountMutate {
                    provider,
                    action: "auto".to_owned(),
                    body: json!({ "on": auto }),
                });
            }
            if auto {
                let mut mv = state.ai_accounts.claude_move;
                if ui.checkbox(&mut mv, i18n.tr("mac.plan.autoMove")).changed() {
                    state.ai_accounts.claude_move = mv;
                    state.push_action(UiAction::AiAccountMutate {
                        provider,
                        action: "auto".to_owned(),
                        body: json!({ "move": mv }),
                    });
                }
                if mv {
                    let mut res = state.ai_accounts.claude_resume;
                    if ui.checkbox(&mut res, i18n.tr("mac.plan.autoResume")).changed() {
                        state.ai_accounts.claude_resume = res;
                        state.push_action(UiAction::AiAccountMutate {
                            provider,
                            action: "auto".to_owned(),
                            body: json!({ "resume": res }),
                        });
                    }
                }
            }
        }
        AiProviderTab::Codex => {
            let mut auto = state.ai_accounts.codex_auto;
            if ui.checkbox(&mut auto, i18n.tr("ai.auto")).changed() {
                state.ai_accounts.codex_auto = auto;
                state.push_action(UiAction::AiAccountMutate {
                    provider,
                    action: "auto".to_owned(),
                    body: json!({ "on": auto }),
                });
            }
        }
        AiProviderTab::Antigravity => {
            let mut auto = state.ai_accounts.ag_auto;
            if ui.checkbox(&mut auto, i18n.tr("ai.auto")).changed() {
                state.ai_accounts.ag_auto = auto;
                state.push_action(UiAction::AiAccountMutate {
                    provider,
                    action: "auto".to_owned(),
                    body: json!({ "on": auto }),
                });
            }
        }
    }

    // Today / 30d burn summary + Open detailed burn drawer button
    ui.add_space(6.0);
    let usd_str = format!("${:.2}", state.burn.today_usd);
    let summary = i18n.tr_vars("station.planSummary", &[("usd", usd_str.as_str())]);
    let month_str = format!("${:.2}", state.burn.month_usd);
    let month_summary = i18n.tr_vars("station.planMonth", &[("usd", month_str.as_str())]);
    ui.horizontal(|ui| {
        ui.label(heading_bold(summary, 11.5).color(INK));
        ui.with_layout(egui::Layout::right_to_left(Align::Center), |ui| {
            ui.label(RichText::new(month_summary).monospace().size(10.5).color(MUTED));
        });
    });

    ui.add_space(6.0);
    let burn_btn = egui::Button::new(heading_bold(i18n.tr("station.openBurn"), 11.5).color(INK))
        .fill(CARD_2)
        .stroke(Stroke::new(1.0_f32, LINE_2))
        .corner_radius(CornerRadius::same(9))
        .min_size(vec2(ui.available_width(), 30.0));
    if ui.add(burn_btn).clicked() {
        state.open_popover = None;
        state.active_drawer = ActiveDrawer::Burn;
    }

    ui.add_space(4.0);
    ui.label(RichText::new(i18n.tr("ui.topBar.planFoot")).size(9.5).color(FAINT));
}

pub fn draw_accounts_drawer(ctx: &egui::Context, state: &mut UiState, i18n: &I18nStore) {
    if !ctx.wants_keyboard_input() && ctx.input(|i| i.key_pressed(egui::Key::Escape)) {
        state.active_drawer = ActiveDrawer::None;
        return;
    }
    let mut open = true;
    egui::Window::new(heading_bold(i18n.tr("ai.title"), 14.0).color(INK))
        .id(egui::Id::new("station_accounts_drawer"))
        .open(&mut open)
        .anchor(egui::Align2::RIGHT_TOP, vec2(-14.0, 62.0))
        .default_width(380.0)
        .max_height(580.0)
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
            egui::ScrollArea::vertical().max_height(520.0).show(ui, |ui| {
                draw_accounts_panel(ui, state, i18n);
            });
        });
    if !open {
        state.active_drawer = ActiveDrawer::None;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ai_accounts_tab_state_switches_between_claude_codex_and_antigravity() {
        let mut st = AiAccountsState::default();
        assert_eq!(st.tab, AiProviderTab::Claude);

        st.adding_provider = Some(AiProviderTab::Claude);
        st.remove_armed = Some((AiProviderTab::Claude, "work".to_owned()));

        set_active_provider_tab(&mut st, AiProviderTab::Codex);
        assert_eq!(st.tab, AiProviderTab::Codex);
        assert_eq!(st.tab.api_prefix(), "/api/ai/codex");
        assert!(st.remove_armed.is_none(), "switching provider clears armed removal");

        set_active_provider_tab(&mut st, AiProviderTab::Antigravity);
        assert_eq!(st.tab, AiProviderTab::Antigravity);
        assert_eq!(st.tab.api_prefix(), "/api/ai/ag");

        set_active_provider_tab(&mut st, AiProviderTab::Claude);
        assert_eq!(st.tab, AiProviderTab::Claude);
        assert_eq!(st.tab.api_prefix(), "/api/accounts");
    }
}
