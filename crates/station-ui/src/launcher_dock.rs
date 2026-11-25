use crate::state::{ActiveDrawer, UiAction, UiState};
use crate::theme::{
    ACCENT_TERRACOTTA, BG_CARD, BORDER_SUBTLE, STATUS_ERR, STATUS_INFO, STATUS_WAIT, STATUS_WORK,
    TEXT_MUTED, TEXT_PRIMARY, glass_frame,
};
use bevy_egui::egui::{self, Color32, RichText, Stroke};
use station_launcher::HarnessStatus;

fn status_badge(status: HarnessStatus) -> (Color32, &'static str) {
    match status {
        HarnessStatus::RunningHooked => (STATUS_WORK, "Running"),
        HarnessStatus::InstalledHooked => (STATUS_INFO, "Ready"),
        HarnessStatus::NeedsHook => (STATUS_WAIT, "Hook"),
        HarnessStatus::NotInstalled => (STATUS_ERR, "Missing"),
    }
}

/// Renders the floating GUI Harness Launcher Dock (5 built-ins + custom apps, zero terminals).
pub fn draw_launcher_dock(ctx: &egui::Context, state: &mut UiState) {
    egui::Area::new(egui::Id::new("harness_launcher_dock"))
        .anchor(egui::Align2::CENTER_BOTTOM, egui::vec2(0.0, -56.0))
        .show(ctx, |ui| {
            glass_frame().show(ui, |ui| {
                ui.horizontal(|ui| {
                    ui.label(
                        RichText::new("APPS")
                            .size(10.5)
                            .strong()
                            .color(TEXT_MUTED),
                    );
                    ui.separator();

                    let items = state.harnesses.clone();
                    for item in items {
                        let (dot_color, tip) = status_badge(item.status);
                        let is_missing = item.status == HarnessStatus::NotInstalled;
                        let btn = egui::Button::new(
                            RichText::new(format!("● {}", item.label))
                                .size(12.0)
                                .color(if is_missing { TEXT_MUTED } else { TEXT_PRIMARY }),
                        )
                        .fill(if item.status == HarnessStatus::RunningHooked {
                            Color32::from_rgba_unmultiplied(61, 220, 132, 26)
                        } else {
                            BG_CARD
                        })
                        .stroke(Stroke::new(
                            1.0_f32,
                            if item.status == HarnessStatus::NeedsHook {
                                STATUS_WAIT
                            } else if item.status == HarnessStatus::RunningHooked {
                                dot_color
                            } else {
                                BORDER_SUBTLE
                            },
                        ));

                        let resp = ui.add(btn).on_hover_text(format!(
                            "{} ({tip}) — Click to auto-hook & open native GUI app",
                            item.label
                        ));
                        if resp.clicked() {
                            state.push_action(UiAction::LaunchHarness(item.id.clone()));
                            state.push_toast(
                                format!("Opening {}", item.label),
                                "Auto-hook preflight + native GUI focus/launch",
                            );
                        }
                    }

                    ui.separator();
                    let add_btn = egui::Button::new(
                        RichText::new("+ Add")
                            .size(11.5)
                            .color(ACCENT_TERRACOTTA)
                            .strong(),
                    );
                    if ui
                        .add(add_btn)
                        .on_hover_text("Add custom .exe / .lnk desktop app")
                        .clicked()
                    {
                        state.toggle_drawer(ActiveDrawer::AddHarness);
                    }
                });
            });
        });
}

/// Inline harness strip used inside 2D Lite mode.
pub fn draw_launcher_inline(ui: &mut egui::Ui, state: &mut UiState) {
    ui.horizontal_wrapped(|ui| {
        ui.label(RichText::new("Harnesses:").size(12.0).strong().color(TEXT_MUTED));
        let items = state.harnesses.clone();
        for item in items {
            let (dot_color, tip) = status_badge(item.status);
            let btn = egui::Button::new(
                RichText::new(format!("● {} ({tip})", item.label))
                    .size(11.5)
                    .color(TEXT_PRIMARY),
            )
            .stroke(Stroke::new(1.0_f32, dot_color));
            if ui.add(btn).clicked() {
                state.push_action(UiAction::LaunchHarness(item.id.clone()));
            }
        }
        if ui.button("+ Add App").clicked() {
            state.toggle_drawer(ActiveDrawer::AddHarness);
        }
    });
}
