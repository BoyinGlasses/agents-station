use crate::i18n::I18nStore;
use crate::icons::{self, IconKind};
use crate::shell::ShellLayout;
use crate::state::{ActiveDrawer, GpuPrefUi, TopBarPopover, UiAction, UiState, ViewMode};
use crate::theme::{
    self, ACCENT_HI, AMBER, BAR, BLUE, CARD, CARD_2, CARD_3, FAINT, GREEN, INK, LINE, LINE_2,
    LOGO, MUTED, RED, SIDE, heading_bold,
};
use bevy_egui::egui::{
    self, Align, Color32, CornerRadius, FontFamily, FontId, Margin, Rect, RichText, Sense, Stroke,
    StrokeKind, vec2,
};
use station_3d::{BackdropKind, ChibiLook, OfficeLayout};
use station_launcher::HarnessStatus;

const ACCENT_SOFT: Color32 = Color32::from_rgba_premultiplied(0x1F, 0x12, 0x0C, 33);
const ACCENT_LINE: Color32 = Color32::from_rgba_premultiplied(0x90, 0x53, 0x39, 153);
const GREEN_SOFT: Color32 = Color32::from_rgba_premultiplied(0x0B, 0x26, 0x17, 28);
const AMBER_SOFT: Color32 = Color32::from_rgba_premultiplied(0x2E, 0x22, 0x0D, 31);
const RED_SOFT: Color32 = Color32::from_rgba_premultiplied(0x31, 0x12, 0x12, 31);
const DEMO_BG: Color32 = Color32::from_rgba_premultiplied(0x0F, 0x18, 0x24, 36);
const DEMO_BORDER: Color32 = Color32::from_rgba_premultiplied(0x35, 0x57, 0x80, 128);
const DEMO_INK: Color32 = Color32::from_rgb(0xBF, 0xDC, 0xFF);
const POPOVER_BG: Color32 = Color32::from_rgb(0x15, 0x1C, 0x27);

const WEATHER_CITIES: &[(&str, &str)] = &[
    ("hcm", ""),
    ("hn", "Hà Nội"),
    ("dn", "Đà Nẵng"),
    ("hue", "Huế"),
    ("hp", "Hải Phòng"),
    ("ct", "Cần Thơ"),
    ("dl", "Đà Lạt"),
    ("nt", "Nha Trang"),
];

pub fn draw_topbar(
    ctx: &egui::Context,
    state: &mut UiState,
    i18n: &I18nStore,
    layout: &ShellLayout,
) {
    let narrow = layout.topbar_height <= 52.0;
    egui::TopBottomPanel::top("station_shell_topbar")
        .exact_height(layout.topbar_height)
        .frame(
            egui::Frame::NONE
                .fill(BAR)
                .inner_margin(Margin::symmetric(if narrow { 10 } else { 14 }, 0)),
        )
        .show(ctx, |ui| {
            let panel_rect = ui.max_rect();
            ui.painter().line_segment(
                [panel_rect.left_bottom(), panel_rect.right_bottom()],
                Stroke::new(1.0_f32, LINE),
            );

            ui.horizontal_centered(|ui| {
                ui.spacing_mut().item_spacing.x = if narrow { 6.0 } else { 8.0 };

                // Brand logo (#F08A3E) + bold "Agents Station"
                let (logo_rect, _) = ui.allocate_exact_size(vec2(26.0, 26.0), Sense::hover());
                icons::paint_icon(ui.painter(), logo_rect, IconKind::Logo, LOGO);
                ui.label(heading_bold(
                    i18n.tr("station.brand"),
                    if narrow { 14.0 } else { 15.5 },
                ));

                // HH:MM clock chip
                if !narrow {
                    draw_compact_chip(
                        ui,
                        CARD,
                        LINE,
                        CornerRadius::same(7),
                        RichText::new(&state.clock_label)
                            .font(FontId::new(11.5, FontFamily::Monospace))
                            .color(MUTED),
                    );
                }

                // Optional DEMO / REPLAY chips
                if state.demo_running {
                    draw_compact_chip(
                        ui,
                        DEMO_BG,
                        DEMO_BORDER,
                        CornerRadius::same(99),
                        heading_bold(i18n.tr("station.demoChip"), 10.5).color(DEMO_INK),
                    );
                }
                if state.replay_playing && !narrow {
                    draw_compact_chip(
                        ui,
                        AMBER_SOFT,
                        AMBER,
                        CornerRadius::same(99),
                        heading_bold(i18n.tr("station.replayChip"), 10.5).color(AMBER),
                    );
                }

                // 3D | 2D segmented switch
                draw_view_switch(ui, state, i18n);

                // Right-aligned control-room action cluster
                ui.with_layout(egui::Layout::right_to_left(Align::Center), |ui| {
                    ui.spacing_mut().item_spacing.x = if narrow { 5.0 } else { 7.0 };

                    // Night moon/sun icon button
                    let night_icon = if state.night {
                        IconKind::Moon
                    } else {
                        IconKind::Sun
                    };
                    if topbar_icon_button(ui, night_icon, state.night, false)
                        .on_hover_text(i18n.tr("ui.topBar.nightTitle"))
                        .clicked()
                    {
                        state.night = !state.night;
                        state.push_action(UiAction::ToggleNight(state.night));
                    }

                    // Phone icon button
                    if !narrow {
                        let phone_on = state.active_drawer == ActiveDrawer::PhoneQr;
                        if topbar_icon_button(ui, IconKind::Phone, phone_on, false)
                            .on_hover_text(i18n.tr("ui.topBar.phoneTitle"))
                            .clicked()
                        {
                            state.open_popover = None;
                            state.toggle_drawer(ActiveDrawer::PhoneQr);
                        }
                    }

                    // Settings gear button (popover)
                    let settings_on = state.open_popover == Some(TopBarPopover::Settings);
                    let settings_lbl = (!layout.compact_topbar_labels)
                        .then(|| i18n.tr("ui.topBar.settings"));
                    if topbar_action_button(
                        ui,
                        IconKind::Gear,
                        settings_lbl.as_deref(),
                        settings_on,
                        false,
                    )
                    .on_hover_text(i18n.tr("ui.topBar.settingsTitle"))
                    .clicked()
                    {
                        toggle_popover(state, TopBarPopover::Settings);
                    }

                    // Replay button
                    let replay_lbl = (!layout.compact_topbar_labels)
                        .then(|| i18n.tr("ui.topBar.replay"));
                    if topbar_action_button(
                        ui,
                        IconKind::Rewind,
                        replay_lbl.as_deref(),
                        state.replay_playing,
                        false,
                    )
                    .on_hover_text(i18n.tr("ui.topBar.replayTitle"))
                    .clicked()
                    {
                        state.push_action(UiAction::ToggleReplay);
                    }

                    // Chat button (with red unread badge)
                    let chat_on = state.active_drawer == ActiveDrawer::GroupChat;
                    let chat_lbl =
                        (!layout.compact_topbar_labels).then(|| i18n.tr("chat.open"));
                    if topbar_action_button(
                        ui,
                        IconKind::Chat,
                        chat_lbl.as_deref(),
                        chat_on,
                        state.unread_chat > 0,
                    )
                    .on_hover_text(i18n.tr("chat.openTitle"))
                    .clicked()
                    {
                        state.open_popover = None;
                        state.unread_chat = 0;
                        state.toggle_drawer(ActiveDrawer::GroupChat);
                    }

                    // PC / RAM pill (84x6 bar)
                    draw_ram_pill(ui, state, i18n, narrow);

                    // Plan quota pill (84x6 bar + Claude|Codex|Antigravity popover)
                    draw_plan_pill(ui, state, i18n, narrow);

                    // Apps (Harnesses) pill/popover
                    draw_apps_pill(ui, state, i18n, layout.compact_topbar_labels);
                });
            });
        });

    draw_active_popover(ctx, state, i18n, layout.topbar_height);
}

fn toggle_popover(state: &mut UiState, target: TopBarPopover) {
    state.open_popover = if state.open_popover == Some(target) {
        None
    } else {
        Some(target)
    };
}

fn draw_compact_chip(
    ui: &mut egui::Ui,
    fill: Color32,
    border: Color32,
    cr: CornerRadius,
    text: RichText,
) {
    let galley = egui::WidgetText::from(text).into_galley(
        ui,
        Some(egui::TextWrapMode::Extend),
        f32::INFINITY,
        egui::FontSelection::Default,
    );
    let size = vec2(galley.size().x + 16.0, 22.0);
    let (rect, _) = ui.allocate_exact_size(size, Sense::hover());
    ui.painter()
        .rect(rect, cr, fill, Stroke::new(1.0_f32, border), StrokeKind::Inside);
    ui.painter().galley(
        egui::pos2(
            rect.center().x - galley.size().x * 0.5,
            rect.center().y - galley.size().y * 0.5,
        ),
        galley,
        INK,
    );
}

fn draw_view_switch(ui: &mut egui::Ui, state: &mut UiState, i18n: &I18nStore) {
    egui::Frame::NONE
        .fill(SIDE)
        .corner_radius(CornerRadius::same(9))
        .inner_margin(Margin::same(2))
        .stroke(Stroke::new(1.0_f32, LINE_2))
        .show(ui, |ui| {
            ui.horizontal(|ui| {
                ui.spacing_mut().item_spacing.x = 2.0;
                let is_3d = state.view_mode == ViewMode::Mode3D;
                for (active, label_key, tip_key, target) in [
                    (is_3d, "station.view3d", "ui.topBar.view3d", ViewMode::Mode3D),
                    (
                        !is_3d,
                        "station.view2d",
                        "ui.topBar.view2d",
                        ViewMode::Lite2D,
                    ),
                ] {
                    let fill = if active { CARD_3 } else { Color32::TRANSPARENT };
                    let txt_col = if active { INK } else { MUTED };
                    let btn = egui::Button::new(
                        heading_bold(i18n.tr(label_key), 11.5).color(txt_col),
                    )
                    .fill(fill)
                    .stroke(Stroke::NONE)
                    .corner_radius(CornerRadius::same(7))
                    .min_size(vec2(30.0, 24.0));
                    if ui
                        .add(btn)
                        .on_hover_text(i18n.tr(tip_key))
                        .clicked()
                        && state.view_mode != target
                    {
                        state.view_mode = target;
                        state.push_action(UiAction::SetViewMode(target));
                    }
                }
            });
        });
}

fn topbar_icon_button(
    ui: &mut egui::Ui,
    kind: IconKind,
    active: bool,
    show_badge: bool,
) -> egui::Response {
    let (rect, resp) = ui.allocate_exact_size(vec2(36.0, 36.0), Sense::click());
    let fill = if active {
        ACCENT_SOFT
    } else if resp.hovered() {
        CARD_2
    } else {
        CARD
    };
    let border = if active { ACCENT_LINE } else { LINE_2 };
    ui.painter()
        .rect(rect, CornerRadius::same(10), fill, Stroke::new(1.0_f32, border), StrokeKind::Inside);
    let icon_rect = Rect::from_center_size(rect.center(), vec2(16.0, 16.0));
    let icon_col = if active { ACCENT_HI } else { INK };
    icons::paint_icon(ui.painter(), icon_rect, kind, icon_col);
    if show_badge {
        let dot_pos = rect.right_top() + vec2(-8.0, 8.0);
        ui.painter().circle_filled(dot_pos, 4.5, CARD);
        ui.painter().circle_filled(dot_pos, 3.3, RED);
    }
    resp
}

fn topbar_action_button(
    ui: &mut egui::Ui,
    kind: IconKind,
    label: Option<&str>,
    active: bool,
    show_badge: bool,
) -> egui::Response {
    let Some(text) = label else {
        return topbar_icon_button(ui, kind, active, show_badge);
    };
    let galley = ui.painter().layout_no_wrap(
        text.to_owned(),
        FontId::new(12.0, FontFamily::Name(theme::FONT_BOLD.into())),
        INK,
    );
    let w = (12.0 + 16.0 + 7.0 + galley.size().x + 12.0).ceil();
    let (rect, resp) = ui.allocate_exact_size(vec2(w, 36.0), Sense::click());
    let fill = if active {
        ACCENT_SOFT
    } else if resp.hovered() {
        CARD_2
    } else {
        CARD
    };
    let border = if active { ACCENT_LINE } else { LINE_2 };
    ui.painter()
        .rect(rect, CornerRadius::same(10), fill, Stroke::new(1.0_f32, border), StrokeKind::Inside);
    let icon_rect = Rect::from_min_size(
        egui::pos2(rect.left() + 12.0, rect.center().y - 8.0),
        vec2(16.0, 16.0),
    );
    icons::paint_icon(
        ui.painter(),
        icon_rect,
        kind,
        if active { ACCENT_HI } else { INK },
    );
    ui.painter().galley(
        egui::pos2(icon_rect.right() + 7.0, rect.center().y - galley.size().y * 0.5),
        galley,
        INK,
    );
    if show_badge {
        let dot_pos = rect.right_top() + vec2(-8.0, 8.0);
        ui.painter().circle_filled(dot_pos, 4.5, CARD);
        ui.painter().circle_filled(dot_pos, 3.3, RED);
    }
    resp
}

fn level_colors(pct: f32) -> (Color32, Color32) {
    if pct >= 0.88 {
        (RED, RED_SOFT)
    } else if pct >= 0.70 {
        (AMBER, AMBER_SOFT)
    } else {
        (GREEN, GREEN_SOFT)
    }
}

fn draw_metric_pill(
    ui: &mut egui::Ui,
    dot: Color32,
    halo: Color32,
    label: Option<String>,
    val_text: String,
    pct: f32,
    show_bar: bool,
    active: bool,
) -> egui::Response {
    let label_galley = label.map(|s| {
        ui.painter().layout_no_wrap(
            s,
            FontId::new(12.0, FontFamily::Name(theme::FONT_BOLD.into())),
            INK,
        )
    });
    let val_galley = ui.painter().layout_no_wrap(
        val_text,
        FontId::new(11.5, FontFamily::Proportional),
        MUTED,
    );

    let pad_x = 12.0;
    let gap = 8.0;
    let mut w = pad_x + 12.0 + gap; // dot
    if let Some(g) = &label_galley {
        w += g.size().x + gap;
    }
    w += val_galley.size().x;
    if show_bar {
        w += gap + 84.0;
    }
    w += pad_x;

    let (rect, resp) = ui.allocate_exact_size(vec2(w.ceil(), 36.0), Sense::click());
    let fill = if active {
        ACCENT_SOFT
    } else if resp.hovered() {
        CARD_2
    } else {
        CARD
    };
    let border = if active { ACCENT_LINE } else { LINE_2 };
    ui.painter()
        .rect(rect, CornerRadius::same(10), fill, Stroke::new(1.0_f32, border), StrokeKind::Inside);

    let cy = rect.center().y;
    let mut x = rect.left() + pad_x;

    // Status dot + halo
    let dot_center = egui::pos2(x + 6.0, cy);
    ui.painter().circle_filled(dot_center, 6.2, halo);
    ui.painter().circle_filled(dot_center, 3.8, dot);
    x += 12.0 + gap;

    // Label
    if let Some(g) = label_galley {
        let gw = g.size().x;
        let gh = g.size().y;
        ui.painter()
            .galley(egui::pos2(x, cy - gh * 0.5), g, INK);
        x += gw + gap;
    }

    // Value
    let vw = val_galley.size().x;
    let vh = val_galley.size().y;
    ui.painter()
        .galley(egui::pos2(x, cy - vh * 0.5), val_galley, MUTED);
    x += vw + gap;

    // 84x6 progress bar
    if show_bar {
        let bar_rect = Rect::from_min_size(egui::pos2(x, cy - 3.0), vec2(84.0, 6.0));
        ui.painter()
            .rect_filled(bar_rect, CornerRadius::same(3), CARD_3);
        let clamped = pct.clamp(0.0, 1.0);
        if clamped > 0.0 {
            let fill_r = Rect::from_min_size(
                bar_rect.min,
                vec2((bar_rect.width() * clamped).max(4.0), 6.0),
            );
            ui.painter()
                .rect_filled(fill_r, CornerRadius::same(3), dot);
        }
    }

    resp
}

fn draw_ram_pill(ui: &mut egui::Ui, state: &mut UiState, i18n: &I18nStore, narrow: bool) {
    let total = state.sys.mem_total_gb;
    let pct = if total > 0.0 {
        (state.sys.mem_used_gb / total).clamp(0.0, 1.0)
    } else {
        0.0
    };
    let (dot, halo) = level_colors(pct);
    let active = state.active_drawer == ActiveDrawer::System;
    let val_text = if total > 0.0 {
        let free_gb = (total - state.sys.mem_used_gb).max(0.0);
        let pct_int = (pct * 100.0).round() as u32;
        let pct_s = pct_int.to_string();
        let free_s = format!("{free_gb:.1}");
        i18n.tr_vars(
            "station.ramMini",
            &[("pct", pct_s.as_str()), ("free", free_s.as_str())],
        )
    } else {
        i18n.tr("ui.topBar.measuring")
    };

    if draw_metric_pill(
        ui,
        dot,
        halo,
        (!narrow).then(|| i18n.tr("ui.topBar.mac")),
        val_text,
        pct,
        !narrow,
        active,
    )
    .on_hover_text(i18n.tr("ui.topBar.macTitle"))
    .clicked()
    {
        state.open_popover = None;
        state.toggle_drawer(ActiveDrawer::System);
    }
}

fn draw_plan_pill(ui: &mut egui::Ui, state: &mut UiState, i18n: &I18nStore, narrow: bool) {
    let pct = (state.burn.today_usd as f32 / 20.0).clamp(0.0, 1.0);
    let (dot, halo) = level_colors(pct);
    let active = state.open_popover == Some(TopBarPopover::Plan);
    let val_text = if state.burn.today_usd > 0.0 {
        format!("${:.2}", state.burn.today_usd)
    } else {
        i18n.tr("ui.topBar.reading")
    };

    if draw_metric_pill(
        ui,
        dot,
        halo,
        (!narrow).then(|| i18n.tr("ui.topBar.plan")),
        val_text,
        pct,
        !narrow,
        active,
    )
    .on_hover_text(i18n.tr("ui.topBar.planLimits"))
    .clicked()
    {
        toggle_popover(state, TopBarPopover::Plan);
    }
}

fn draw_apps_pill(ui: &mut egui::Ui, state: &mut UiState, i18n: &I18nStore, compact: bool) {
    let running = state
        .harnesses
        .iter()
        .filter(|h| h.status == HarnessStatus::RunningHooked)
        .count();
    let active = state.open_popover == Some(TopBarPopover::Apps);
    let label_galley = (!compact).then(|| {
        ui.painter().layout_no_wrap(
            i18n.tr("station.apps"),
            FontId::new(12.0, FontFamily::Name(theme::FONT_BOLD.into())),
            INK,
        )
    });
    let badge_galley = (running > 0).then(|| {
        ui.painter().layout_no_wrap(
            running.to_string(),
            FontId::new(10.5, FontFamily::Name(theme::FONT_BOLD.into())),
            Color32::from_rgb(0x8F, 0xED, 0xB8),
        )
    });

    let pad_x = 10.0;
    let gap = 6.0;
    let mut w = pad_x + 15.0;
    if let Some(g) = &label_galley {
        w += gap + g.size().x;
    }
    if let Some(b) = &badge_galley {
        w += gap + (b.size().x + 12.0).max(18.0);
    }
    w += pad_x;

    let (rect, resp) = ui.allocate_exact_size(vec2(w.ceil(), 36.0), Sense::click());
    let fill = if active {
        ACCENT_SOFT
    } else if resp.hovered() {
        CARD_2
    } else {
        CARD
    };
    let border = if active { ACCENT_LINE } else { LINE_2 };
    ui.painter()
        .rect(rect, CornerRadius::same(10), fill, Stroke::new(1.0_f32, border), StrokeKind::Inside);

    let cy = rect.center().y;
    let mut x = rect.left() + pad_x;
    let icon_rect = Rect::from_min_size(egui::pos2(x, cy - 7.5), vec2(15.0, 15.0));
    icons::paint_icon(
        ui.painter(),
        icon_rect,
        IconKind::Apps,
        if active { ACCENT_HI } else { INK },
    );
    x += 15.0;

    if let Some(g) = label_galley {
        x += gap;
        let gw = g.size().x;
        let gh = g.size().y;
        ui.painter().galley(egui::pos2(x, cy - gh * 0.5), g, INK);
        x += gw;
    }

    if let Some(b) = badge_galley {
        x += gap;
        let bw = (b.size().x + 12.0).max(18.0);
        let badge_r = Rect::from_min_size(egui::pos2(x, cy - 9.0), vec2(bw, 18.0));
        ui.painter()
            .rect_filled(badge_r, CornerRadius::same(9), GREEN_SOFT);
        ui.painter().galley(
            egui::pos2(
                badge_r.center().x - b.size().x * 0.5,
                badge_r.center().y - b.size().y * 0.5,
            ),
            b,
            Color32::from_rgb(0x8F, 0xED, 0xB8),
        );
    }

    if resp
        .on_hover_text(i18n.tr("station.appsTitle"))
        .clicked()
    {
        toggle_popover(state, TopBarPopover::Apps);
    }
}

fn draw_active_popover(
    ctx: &egui::Context,
    state: &mut UiState,
    i18n: &I18nStore,
    topbar_height: f32,
) {
    let Some(pop) = state.open_popover else {
        return;
    };
    let anchor_y = topbar_height + 8.0;
    let width = match pop {
        TopBarPopover::Settings => 380.0,
        TopBarPopover::Plan => 340.0,
        TopBarPopover::Apps => 310.0,
    };

    egui::Area::new(egui::Id::new("station_topbar_popover"))
        .order(egui::Order::Foreground)
        .anchor(egui::Align2::RIGHT_TOP, vec2(-12.0, anchor_y))
        .show(ctx, |ui| {
            egui::Frame::NONE
                .fill(POPOVER_BG)
                .corner_radius(CornerRadius::same(16))
                .inner_margin(Margin::same(14))
                .stroke(Stroke::new(1.0_f32, LINE_2))
                .show(ui, |ui| {
                    ui.set_width(width);
                    match pop {
                        TopBarPopover::Apps => draw_apps_popover(ui, state, i18n),
                        TopBarPopover::Plan => draw_plan_popover(ui, state, i18n),
                        TopBarPopover::Settings => draw_settings_popover(ui, state, i18n),
                    }
                });
        });
}

fn harness_status_visual<'a>(status: HarnessStatus, i18n: &I18nStore) -> (Color32, String) {
    match status {
        HarnessStatus::RunningHooked => (GREEN, i18n.tr("station.status.running")),
        HarnessStatus::InstalledHooked => (BLUE, i18n.tr("station.status.ready")),
        HarnessStatus::NeedsHook => (AMBER, i18n.tr("station.status.needsHook")),
        HarnessStatus::NotInstalled => (FAINT, i18n.tr("station.status.missing")),
    }
}

fn popover_header(ui: &mut egui::Ui, title: String, state: &mut UiState, i18n: &I18nStore) {
    ui.horizontal(|ui| {
        ui.label(heading_bold(title, 14.5));
        ui.with_layout(egui::Layout::right_to_left(Align::Center), |ui| {
            let (rect, resp) = ui.allocate_exact_size(vec2(28.0, 28.0), Sense::click());
            if resp.hovered() {
                ui.painter()
                    .rect_filled(rect, CornerRadius::same(8), CARD_2);
            }
            icons::paint_icon(
                ui.painter(),
                Rect::from_center_size(rect.center(), vec2(14.0, 14.0)),
                IconKind::Close,
                MUTED,
            );
            if resp.on_hover_text(i18n.tr("common.close")).clicked() {
                state.open_popover = None;
            }
        });
    });
    ui.add_space(6.0);
}

fn draw_apps_popover(ui: &mut egui::Ui, state: &mut UiState, i18n: &I18nStore) {
    popover_header(ui, i18n.tr("station.appsTitle"), state, i18n);

    if state.harnesses.is_empty() {
        ui.label(RichText::new(i18n.tr("station.noApps")).size(12.0).color(MUTED));
    } else {
        let items = state.harnesses.clone();
        for item in items {
            let (dot_col, status_lbl) = harness_status_visual(item.status, i18n);
            let frame_resp = egui::Frame::NONE
                .fill(CARD)
                .corner_radius(CornerRadius::same(10))
                .inner_margin(Margin::symmetric(10, 8))
                .stroke(Stroke::new(1.0_f32, LINE))
                .show(ui, |ui| {
                    ui.set_width(ui.available_width());
                    ui.horizontal(|ui| {
                        theme::status_dot(ui, dot_col, 4.0);
                        ui.label(heading_bold(&item.label, 12.5));
                        ui.with_layout(egui::Layout::right_to_left(Align::Center), |ui| {
                            ui.label(RichText::new(status_lbl).size(11.0).color(dot_col));
                        });
                    });
                });
            if frame_resp
                .response
                .interact(Sense::click())
                .on_hover_text(i18n.tr("station.clickToOpen"))
                .clicked()
            {
                state.push_action(UiAction::LaunchHarness(item.id.clone()));
                state.open_popover = None;
            }
            ui.add_space(4.0);
        }
    }

    ui.add_space(6.0);
    let add_btn = egui::Button::new(heading_bold(i18n.tr("station.addApp"), 12.0).color(ACCENT_HI))
        .fill(ACCENT_SOFT)
        .stroke(Stroke::new(1.0_f32, ACCENT_LINE))
        .corner_radius(CornerRadius::same(10))
        .min_size(vec2(ui.available_width(), 32.0));
    if ui.add(add_btn).clicked() {
        state.open_popover = None;
        state.active_drawer = ActiveDrawer::AddHarness;
    }
}

fn draw_plan_popover(ui: &mut egui::Ui, state: &mut UiState, i18n: &I18nStore) {
    popover_header(ui, i18n.tr("ai.title"), state, i18n);
    egui::ScrollArea::vertical().max_height(460.0).show(ui, |ui| {
        crate::drawers::accounts::draw_accounts_panel(ui, state, i18n);
    });
}

fn draw_settings_popover(ui: &mut egui::Ui, state: &mut UiState, i18n: &I18nStore) {
    popover_header(ui, i18n.tr("ui.topBar.settings"), state, i18n);

    // 4 tabs matching SettingsPanel.tsx: Office | Characters | Display | More
    egui::Frame::NONE
        .fill(CARD)
        .corner_radius(CornerRadius::same(12))
        .inner_margin(Margin::same(4))
        .stroke(Stroke::new(1.0_f32, LINE))
        .show(ui, |ui| {
            ui.columns(4, |cols| {
                for (idx, key) in [
                    (0u8, "settings.tabs.office"),
                    (1u8, "settings.tabs.people"),
                    (2u8, "settings.tabs.display"),
                    (3u8, "settings.tabs.more"),
                ] {
                    let active = state.settings_tab == idx;
                    let btn = egui::Button::new(
                        heading_bold(i18n.tr(key), 11.5).color(if active { INK } else { MUTED }),
                    )
                    .fill(if active { ACCENT_SOFT } else { Color32::TRANSPARENT })
                    .stroke(if active {
                        Stroke::new(1.0_f32, ACCENT_LINE)
                    } else {
                        Stroke::NONE
                    })
                    .corner_radius(CornerRadius::same(9))
                    .min_size(vec2(cols[idx as usize].available_width(), 30.0));
                    if cols[idx as usize].add(btn).clicked() {
                        state.settings_tab = idx;
                    }
                }
            });
        });

    ui.add_space(10.0);
    match state.settings_tab {
        0 => draw_settings_office_tab(ui, state, i18n),
        1 => draw_settings_people_tab(ui, state, i18n),
        2 => draw_settings_display_tab(ui, state, i18n),
        _ => draw_settings_more_tab(ui, state, i18n),
    }
}

fn section_heading(ui: &mut egui::Ui, text: String) {
    ui.label(heading_bold(text.to_uppercase(), 10.5).color(FAINT));
    ui.add_space(4.0);
}

fn choice_card(ui: &mut egui::Ui, title: String, sub: String, active: bool) -> egui::Response {
    let frame = egui::Frame::NONE
        .fill(if active { ACCENT_SOFT } else { CARD })
        .corner_radius(CornerRadius::same(11))
        .inner_margin(Margin::symmetric(8, 8))
        .stroke(Stroke::new(1.0_f32, if active { ACCENT_LINE } else { LINE }));
    let resp = frame.show(ui, |ui| {
        ui.set_width(ui.available_width());
        ui.label(heading_bold(title, 12.0));
        if !sub.is_empty() {
            ui.label(RichText::new(sub).size(10.5).color(MUTED));
        }
    });
    resp.response.interact(Sense::click())
}

fn draw_settings_office_tab(ui: &mut egui::Ui, state: &mut UiState, i18n: &I18nStore) {
    section_heading(ui, i18n.tr("ui.topBar.office"));
    ui.columns(3, |cols| {
        for (i, (lay, name_k, sub_k)) in [
            (OfficeLayout::Cozy, "settings.layouts.cozy.name", "settings.layouts.cozy.sub"),
            (OfficeLayout::Pro, "settings.layouts.pro.name", "settings.layouts.pro.sub"),
            (OfficeLayout::Hq, "settings.layouts.hq.name", "settings.layouts.hq.sub"),
        ]
        .into_iter()
        .enumerate()
        {
            if choice_card(&mut cols[i], i18n.tr(name_k), i18n.tr(sub_k), state.layout == lay)
                .clicked()
            {
                state.layout = lay;
                state.view_mode = ViewMode::Mode3D;
                state.push_action(UiAction::SwitchLayout(lay));
            }
        }
    });

    ui.add_space(10.0);
    section_heading(ui, i18n.tr("ui.topBar.scenery"));
    ui.columns(3, |cols| {
        for (i, (bk, name_k, sub_k)) in [
            (BackdropKind::City, "ui.topBar.scenes.city", "ui.topBar.scenes.citySub"),
            (
                BackdropKind::Countryside,
                "ui.topBar.scenes.country",
                "ui.topBar.scenes.countrySub",
            ),
            (BackdropKind::Beach, "ui.topBar.scenes.beach", "ui.topBar.scenes.beachSub"),
        ]
        .into_iter()
        .enumerate()
        {
            if choice_card(&mut cols[i], i18n.tr(name_k), i18n.tr(sub_k), state.backdrop == bk)
                .clicked()
            {
                state.backdrop = bk;
                state.push_action(UiAction::SwitchBackdrop(bk));
            }
        }
    });

    ui.add_space(10.0);
    section_heading(ui, i18n.tr("ui.topBar.weather"));
    ui.horizontal_wrapped(|ui| {
        ui.spacing_mut().item_spacing = vec2(6.0, 6.0);
        for &(code, fixed_name) in WEATHER_CITIES {
            let label = if fixed_name.is_empty() {
                i18n.tr("life.weather.hcm")
            } else {
                fixed_name.to_owned()
            };
            let selected = state.weather_city.eq_ignore_ascii_case(code)
                || state.weather_city.eq_ignore_ascii_case(&label);
            let btn = egui::Button::new(
                heading_bold(&label, 11.0).color(if selected { INK } else { MUTED }),
            )
            .fill(if selected { ACCENT_SOFT } else { CARD })
            .stroke(Stroke::new(1.0_f32, if selected { ACCENT_LINE } else { LINE }))
            .corner_radius(CornerRadius::same(99));
            if ui.add(btn).clicked() {
                state.weather_city = label.clone();
                state.push_action(UiAction::SetWeatherCity(code.to_owned()));
            }
        }
    });
}

fn draw_settings_people_tab(ui: &mut egui::Ui, state: &mut UiState, i18n: &I18nStore) {
    section_heading(ui, i18n.tr("ui.topBar.characters"));
    ui.columns(2, |cols| {
        for (i, (look, title_k, sub_k)) in [
            (ChibiLook::Pixar, "ui.topBar.pixar", "settings.looks.pixar.sub"),
            (ChibiLook::Toy, "ui.topBar.toy", "settings.looks.toy.sub"),
        ]
        .into_iter()
        .enumerate()
        {
            if choice_card(
                &mut cols[i],
                i18n.tr(title_k),
                i18n.tr(sub_k),
                state.chibi_look == look,
            )
            .clicked()
            {
                state.chibi_look = look;
                state.push_action(UiAction::SetChibiLook(look));
            }
        }
    });
    ui.add_space(6.0);
    ui.label(RichText::new(i18n.tr("settings.lookNote")).size(11.0).color(MUTED));
}

fn draw_settings_display_tab(ui: &mut egui::Ui, state: &mut UiState, i18n: &I18nStore) {
    section_heading(ui, i18n.tr("settings.language"));
    ui.columns(3, |cols| {
        for (i, (code, name_k)) in [
            ("en", "station.langNames.en"),
            ("vi", "station.langNames.vi"),
            ("th", "station.langNames.th"),
        ]
        .into_iter()
        .enumerate()
        {
            let active = i18n.lang == code;
            if choice_card(&mut cols[i], code.to_uppercase(), i18n.tr(name_k), active).clicked() {
                state.push_action(UiAction::SetLang(code.to_owned()));
            }
        }
    });

    ui.add_space(10.0);
    section_heading(ui, i18n.tr("station.gpu"));
    ui.columns(3, |cols| {
        for (i, (pref, key)) in [
            (GpuPrefUi::Auto, "station.gpuOpt.auto"),
            (GpuPrefUi::Integrated, "station.gpuOpt.integrated"),
            (GpuPrefUi::Discrete, "station.gpuOpt.discrete"),
        ]
        .into_iter()
        .enumerate()
        {
            let active = state.gpu_pref == pref;
            if choice_card(&mut cols[i], i18n.tr(key), String::new(), active).clicked() {
                state.gpu_pref = pref;
                state.push_action(UiAction::SetGpuPref(pref));
            }
        }
    });

    ui.add_space(10.0);
    toggle_switch_row(
        ui,
        i18n.tr("ui.topBar.toggles.stream"),
        &mut state.streamer_mode,
    );
}

fn draw_settings_more_tab(ui: &mut egui::Ui, state: &mut UiState, i18n: &I18nStore) {
    section_heading(ui, i18n.tr("settings.behaviour"));
    let mut sound = state.sound_on;
    if toggle_switch_row(ui, i18n.tr("ui.topBar.toggles.sound"), &mut sound) {
        state.sound_on = sound;
        state.push_action(UiAction::SetSound(sound));
    }
    toggle_switch_row(ui, i18n.tr("station.closeToTray"), &mut state.auto_close_tray);

    ui.add_space(10.0);
    let tray_btn = egui::Button::new(heading_bold(i18n.tr("station.tray"), 12.0))
        .fill(CARD_2)
        .stroke(Stroke::new(1.0_f32, LINE_2))
        .corner_radius(CornerRadius::same(10))
        .min_size(vec2(ui.available_width(), 34.0));
    if ui.add(tray_btn).clicked() {
        state.open_popover = None;
        state.push_action(UiAction::MinimizeToTray);
    }
}

fn toggle_switch_row(ui: &mut egui::Ui, label: String, value: &mut bool) -> bool {
    let mut changed = false;
    ui.horizontal(|ui| {
        ui.label(RichText::new(label).size(12.0).color(INK));
        ui.with_layout(egui::Layout::right_to_left(Align::Center), |ui| {
            let (rect, resp) = ui.allocate_exact_size(vec2(34.0, 20.0), Sense::click());
            if resp.clicked() {
                *value = !*value;
                changed = true;
            }
            let track = if *value { GREEN } else { CARD_3 };
            ui.painter()
                .rect_filled(rect, CornerRadius::same(10), track);
            let knob_x = if *value {
                rect.right() - 10.0
            } else {
                rect.left() + 10.0
            };
            let knob_col = if *value {
                Color32::WHITE
            } else {
                Color32::from_rgb(0xC8, 0xD0, 0xDD)
            };
            ui.painter()
                .circle_filled(egui::pos2(knob_x, rect.center().y), 7.0, knob_col);
        });
    });
    ui.add_space(4.0);
    changed
}

/// Compact inline harness launcher strip for 2D Lite mode.
pub fn draw_launcher_inline(ui: &mut egui::Ui, state: &mut UiState, i18n: &I18nStore) {
    ui.horizontal_wrapped(|ui| {
        ui.label(heading_bold(i18n.tr("station.apps"), 11.5).color(MUTED));
        ui.separator();
        let items = state.harnesses.clone();
        for item in items {
            let (dot_col, status_lbl) = harness_status_visual(item.status, i18n);
            let frame_resp = egui::Frame::NONE
                .fill(CARD)
                .corner_radius(CornerRadius::same(8))
                .inner_margin(Margin::symmetric(9, 4))
                .stroke(Stroke::new(1.0_f32, LINE_2))
                .show(ui, |ui| {
                    ui.horizontal(|ui| {
                        ui.spacing_mut().item_spacing.x = 5.0;
                        theme::status_dot(ui, dot_col, 3.6);
                        ui.label(RichText::new(&item.label).size(11.5).color(INK));
                    });
                });
            if frame_resp
                .response
                .interact(Sense::click())
                .on_hover_text(format!("{} ({status_lbl})", item.label))
                .clicked()
            {
                state.push_action(UiAction::LaunchHarness(item.id.clone()));
            }
        }
        let add_btn = egui::Button::new(heading_bold(i18n.tr("station.addApp"), 11.5).color(ACCENT_HI))
            .fill(ACCENT_SOFT)
            .stroke(Stroke::new(1.0_f32, ACCENT_LINE))
            .corner_radius(CornerRadius::same(8));
        if ui.add(add_btn).clicked() {
            state.toggle_drawer(ActiveDrawer::AddHarness);
        }
    });
}
