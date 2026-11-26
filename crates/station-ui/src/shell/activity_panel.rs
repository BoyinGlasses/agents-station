use crate::i18n::I18nStore;
use crate::icons::{self, IconKind};
use crate::shell::ShellLayout;
use crate::state::{ActiveDrawer, FeedEntry, UiState};
use crate::theme::{
    ACCENT, AMBER, BLUE, CARD, CARD_2, CARD_3, FAINT, GREEN, INK, LINE, LINE_2, MUTED, RED, SIDE,
    heading_bold,
};
use bevy_egui::egui::{
    self, Align, Color32, CornerRadius, FontFamily, FontId, Margin, Rect, RichText, Sense, Stroke,
    StrokeKind, vec2,
};

const ACCENT_SOFT: Color32 = Color32::from_rgba_premultiplied(0x1F, 0x12, 0x0C, 33);
const ACCENT_LINE: Color32 = Color32::from_rgba_premultiplied(0x90, 0x53, 0x39, 153);
const COST_BG: Color32 = Color32::from_rgb(0x21, 0x22, 0x29);
const COST_BORDER: Color32 = Color32::from_rgba_premultiplied(0x55, 0x31, 0x22, 56);
const CMD_GREEN_BG: Color32 = Color32::from_rgb(0x2F, 0xB3, 0x6C);
const CMD_GREEN_INK: Color32 = Color32::from_rgb(0x06, 0x15, 0x0C);
const BLUE_SOFT: Color32 = Color32::from_rgba_premultiplied(0x13, 0x20, 0x2F, 46);
const AMBER_SOFT: Color32 = Color32::from_rgba_premultiplied(0x2E, 0x22, 0x0D, 31);
const RED_SOFT: Color32 = Color32::from_rgba_premultiplied(0x31, 0x12, 0x12, 31);
const FEED_TEXT: Color32 = Color32::from_rgb(0xC3, 0xCA, 0xD7);

fn agent_dot_color(agent: &str) -> Color32 {
    match agent.to_ascii_lowercase().as_str() {
        "claude" => Color32::from_rgb(0xEF, 0x8A, 0x5F),
        "codex" => BLUE,
        "antigravity" | "gemini" => GREEN,
        "opencode" => Color32::from_rgb(0xB7, 0x94, 0xF4),
        _ => AMBER,
    }
}

pub fn draw_activity_panel(
    ctx: &egui::Context,
    state: &mut UiState,
    i18n: &I18nStore,
    layout: &ShellLayout,
) {
    if layout.show_activity_panel {
        egui::SidePanel::right("station_shell_activity_panel")
            .exact_width(layout.right_width)
            .resizable(false)
            .frame(egui::Frame::NONE.fill(SIDE).inner_margin(Margin::ZERO))
            .show(ctx, |ui| {
                let panel_rect = ui.max_rect();
                ui.painter().line_segment(
                    [panel_rect.left_top(), panel_rect.left_bottom()],
                    Stroke::new(1.0_f32, LINE),
                );
                draw_activity_contents(ui, state, i18n, false, layout.right_width);
            });
    } else if layout.show_activity_bottom_sheet {
        let total_h = ctx.screen_rect().height();
        let sheet_h = if state.activity_sheet_open {
            (total_h * 0.62).clamp(240.0, 520.0)
        } else {
            132.0
        };
        egui::TopBottomPanel::bottom("station_shell_activity_sheet")
            .exact_height(sheet_h)
            .frame(
                egui::Frame::NONE
                    .fill(SIDE)
                    .corner_radius(CornerRadius {
                        nw: 16,
                        ne: 16,
                        sw: 0,
                        se: 0,
                    })
                    .stroke(Stroke::new(1.0_f32, LINE_2))
                    .inner_margin(Margin::ZERO),
            )
            .show(ctx, |ui| {
                let w = ui.available_width();
                draw_activity_contents(ui, state, i18n, true, w);
            });
    }
}

fn draw_activity_contents(
    ui: &mut egui::Ui,
    state: &mut UiState,
    i18n: &I18nStore,
    is_sheet: bool,
    panel_width: f32,
) {
    // Header (.act-head)
    let head_resp = egui::Frame::NONE
        .inner_margin(Margin {
            left: 14,
            right: 14,
            top: if is_sheet { 10 } else { 14 },
            bottom: 8,
        })
        .show(ui, |ui| {
            if is_sheet {
                // Drag handle pill on narrow mobile bottom sheet
                ui.vertical_centered(|ui| {
                    let (handle_r, _) = ui.allocate_exact_size(vec2(36.0, 4.0), Sense::hover());
                    ui.painter()
                        .rect_filled(handle_r, CornerRadius::same(2), LINE_2);
                });
                ui.add_space(6.0);
            }
            ui.horizontal(|ui| {
                ui.label(heading_bold(i18n.tr("ui.activityPanel.title"), 16.0));
                ui.with_layout(egui::Layout::right_to_left(Align::Center), |ui| {
                    ui.label(
                        RichText::new(i18n.tr("ui.activityPanel.live"))
                            .size(11.0)
                            .color(MUTED),
                    );
                    let (dot_r, _) = ui.allocate_exact_size(vec2(9.0, 9.0), Sense::hover());
                    ui.painter().circle_filled(dot_r.center(), 3.5, GREEN);
                });
            });
        });

    if is_sheet && head_resp.response.interact(Sense::click()).clicked() {
        state.activity_sheet_open = !state.activity_sheet_open;
    }

    if !is_sheet || state.activity_sheet_open {
        egui::Frame::NONE
            .inner_margin(Margin::symmetric(14, 0))
            .show(ui, |ui| {
                // 5 KPI tiles: Tasks | Read | Edit | Cmd | $ API
                if !is_sheet {
                    draw_kpi_strip(ui, state, i18n);
                    ui.add_space(8.0);
                }

                // Timeline card button
                if draw_feature_card(
                    ui,
                    IconKind::Timeline,
                    i18n.tr("ui.activityPanel.timeline"),
                    i18n.tr("ui.activityPanel.timelineSub"),
                    state.active_drawer == ActiveDrawer::Timeline,
                )
                .clicked()
                {
                    state.toggle_drawer(ActiveDrawer::Timeline);
                }
                ui.add_space(6.0);

                // Weekly Wrapped card button
                if draw_feature_card(
                    ui,
                    IconKind::Trophy,
                    i18n.tr("ui.activityPanel.wrapped"),
                    i18n.tr("ui.activityPanel.wrappedSub"),
                    state.active_drawer == ActiveDrawer::Wrapped,
                )
                .clicked()
                {
                    state.toggle_drawer(ActiveDrawer::Wrapped);
                }
                ui.add_space(8.0);

                // [All | Attention] segmented filter
                draw_attention_segment(ui, state, i18n);

                // [Cmd | Edit | Read | Web | Agent] type chips
                if !is_sheet {
                    ui.add_space(8.0);
                    draw_type_chips(ui, state, i18n);
                }
            });
        ui.add_space(6.0);
    }

    // Live scrollable event stream (.feed-list)
    egui::ScrollArea::vertical()
        .id_salt("activity_feed_scroll")
        .auto_shrink([false, false])
        .show(ui, |ui| {
            let content_w = (ui.available_width() - 24.0).max(160.0);
            egui::Frame::NONE
                .inner_margin(Margin {
                    left: 12,
                    right: 12,
                    top: 2,
                    bottom: 12,
                })
                .show(ui, |ui| {
                    ui.set_max_width(content_w);
                    let entries: Vec<FeedEntry> = state
                        .feed
                        .iter()
                        .filter(|ev| {
                            if state.activity_attention_only && !ev.attention && ev.level != "err" && ev.level != "warn" {
                                return false;
                            }
                            if let Some(kind_filter) = &state.activity_type_filter
                                && ev.kind != *kind_filter
                            {
                                return false;
                            }
                            true
                        })
                        .cloned()
                        .collect();

                    if entries.is_empty() {
                        let empty_key = if state.activity_attention_only {
                            "activity.empty.att"
                        } else if state.activity_type_filter.is_some() {
                            "activity.empty.type"
                        } else {
                            "activity.empty.all"
                        };
                        ui.add_space(8.0);
                        ui.horizontal(|ui| {
                            ui.add_space(4.0);
                            ui.label(RichText::new(i18n.tr(empty_key)).size(12.0).color(MUTED));
                        });
                        return;
                    }

                    for ev in &entries {
                        draw_feed_card(ui, ev, state.streamer_mode, i18n, panel_width);
                        ui.add_space(6.0);
                    }
                });
        });
}

fn draw_kpi_strip(ui: &mut egui::Ui, state: &UiState, i18n: &I18nStore) {
    let gap = 5.0;
    let avail = ui.available_width();
    let unit = ((avail - gap * 4.0) / 5.75).max(32.0);
    let cost_w = (avail - gap * 4.0 - unit * 4.0).max(58.0);

    ui.horizontal(|ui| {
        ui.spacing_mut().item_spacing.x = gap;
        draw_kpi_tile(
            ui,
            unit,
            IconKind::List,
            compact_count(state.kpi_tasks),
            i18n.tr("activity.kpi.prompts"),
            false,
        );
        draw_kpi_tile(
            ui,
            unit,
            IconKind::File,
            compact_count(state.kpi_reads),
            i18n.tr("activity.kpi.read"),
            false,
        );
        draw_kpi_tile(
            ui,
            unit,
            IconKind::Pencil,
            compact_count(state.kpi_edits),
            i18n.tr("activity.kpi.edit"),
            false,
        );
        draw_kpi_tile(
            ui,
            unit,
            IconKind::Terminal,
            compact_count(state.kpi_cmds),
            i18n.tr("activity.kpi.cmd"),
            false,
        );
        draw_kpi_tile(
            ui,
            cost_w,
            IconKind::Alert,
            format!("≈${:.2}", state.burn.today_usd),
            i18n.tr("activity.kpi.cost"),
            true,
        );
    });
}

fn draw_kpi_tile(
    ui: &mut egui::Ui,
    width: f32,
    icon: IconKind,
    value: String,
    label: String,
    is_cost: bool,
) {
    let fill = if is_cost { COST_BG } else { CARD };
    let stroke = if is_cost { COST_BORDER } else { LINE };
    let (rect, _) = ui.allocate_exact_size(vec2(width, 62.0), Sense::hover());
    ui.painter()
        .rect(rect, CornerRadius::same(11), fill, Stroke::new(1.0_f32, stroke), StrokeKind::Inside);

    let pad_x = rect.left() + 7.0;
    let icon_r = Rect::from_min_size(egui::pos2(pad_x, rect.top() + 7.0), vec2(12.0, 12.0));
    if is_cost {
        ui.painter()
            .circle_stroke(icon_r.center(), 5.5, Stroke::new(1.3_f32, ACCENT));
        ui.painter().text(
            icon_r.center(),
            egui::Align2::CENTER_CENTER,
            "$",
            FontId::new(8.5, FontFamily::Name(crate::theme::FONT_BOLD.into())),
            ACCENT,
        );
    } else {
        icons::paint_icon(ui.painter(), icon_r, icon, FAINT);
    }

    let val_clip = clip_chars(&value, if is_cost { 8 } else { 4 });
    ui.painter().text(
        egui::pos2(pad_x, rect.top() + 24.0),
        egui::Align2::LEFT_TOP,
        val_clip,
        FontId::new(13.5, FontFamily::Name(crate::theme::FONT_BOLD.into())),
        INK,
    );

    let lbl_clip = clip_chars(&label, if is_cost { 10 } else { 6 });
    ui.painter().text(
        egui::pos2(pad_x, rect.bottom() - 15.0),
        egui::Align2::LEFT_TOP,
        lbl_clip,
        FontId::new(9.5, FontFamily::Proportional),
        MUTED,
    );
}

fn draw_feature_card(
    ui: &mut egui::Ui,
    icon: IconKind,
    title: String,
    subtitle: String,
    active: bool,
) -> egui::Response {
    let frame = egui::Frame::NONE
        .fill(if active { ACCENT_SOFT } else { CARD })
        .corner_radius(CornerRadius::same(11))
        .inner_margin(Margin::symmetric(11, 9))
        .stroke(Stroke::new(1.0_f32, if active { ACCENT_LINE } else { LINE }));
    let resp = frame.show(ui, |ui| {
        ui.set_width(ui.available_width());
        ui.horizontal(|ui| {
            ui.spacing_mut().item_spacing.x = 8.0;
            let (ic_r, _) = ui.allocate_exact_size(vec2(15.0, 15.0), Sense::hover());
            icons::paint_icon(ui.painter(), ic_r, icon, ACCENT);
            ui.label(heading_bold(clip_chars(&title, 18), 12.0));
            ui.with_layout(egui::Layout::right_to_left(Align::Center), |ui| {
                ui.label(
                    RichText::new(clip_chars(&subtitle, 22))
                        .size(10.5)
                        .color(MUTED),
                );
            });
        });
    });
    resp.response.interact(Sense::click())
}

fn draw_attention_segment(ui: &mut egui::Ui, state: &mut UiState, i18n: &I18nStore) {
    let att_count = state.attention_feed_count();

    egui::Frame::NONE
        .fill(CARD)
        .corner_radius(CornerRadius::same(12))
        .inner_margin(Margin::same(4))
        .stroke(Stroke::new(1.0_f32, LINE))
        .show(ui, |ui| {
            ui.columns(2, |cols| {
                // All
                let all_active = !state.activity_attention_only;
                if seg_toggle_button(
                    &mut cols[0],
                    Some(IconKind::List),
                    None,
                    i18n.tr("ui.activityPanel.all"),
                    0,
                    all_active,
                )
                .clicked()
                {
                    state.activity_attention_only = false;
                }
                // Attention
                let att_active = state.activity_attention_only;
                if seg_toggle_button(
                    &mut cols[1],
                    None,
                    Some(RED),
                    i18n.tr("ui.activityPanel.attention"),
                    att_count,
                    att_active,
                )
                .clicked()
                {
                    state.activity_attention_only = true;
                }
            });
        });
}

fn seg_toggle_button(
    ui: &mut egui::Ui,
    icon: Option<IconKind>,
    dot: Option<Color32>,
    label: String,
    badge: usize,
    active: bool,
) -> egui::Response {
    let fill = if active {
        ACCENT_SOFT
    } else {
        Color32::TRANSPARENT
    };
    let stroke = if active {
        Stroke::new(1.0_f32, ACCENT_LINE)
    } else {
        Stroke::NONE
    };
    let txt_col = if active { INK } else { MUTED };
    let resp = egui::Frame::NONE
        .fill(fill)
        .corner_radius(CornerRadius::same(9))
        .inner_margin(Margin::symmetric(8, 6))
        .stroke(stroke)
        .show(ui, |ui| {
            ui.set_width(ui.available_width());
            ui.horizontal(|ui| {
                ui.spacing_mut().item_spacing.x = 6.0;
                if let Some(kind) = icon {
                    let (r, _) = ui.allocate_exact_size(vec2(13.0, 13.0), Sense::hover());
                    icons::paint_icon(ui.painter(), r, kind, txt_col);
                }
                if let Some(col) = dot {
                    let (r, _) = ui.allocate_exact_size(vec2(9.0, 9.0), Sense::hover());
                    ui.painter().circle_filled(r.center(), 3.8, col);
                }
                ui.label(heading_bold(clip_chars(&label, 12), 11.0).color(txt_col));
                if badge > 0 {
                    egui::Frame::NONE
                        .fill(RED)
                        .corner_radius(CornerRadius::same(99))
                        .inner_margin(Margin::symmetric(5, 1))
                        .show(ui, |ui| {
                            ui.label(heading_bold(badge.to_string(), 9.5).color(Color32::WHITE));
                        });
                }
            });
        });
    resp.response.interact(Sense::click())
}

fn draw_type_chips(ui: &mut egui::Ui, state: &mut UiState, i18n: &I18nStore) {
    ui.horizontal_wrapped(|ui| {
        ui.spacing_mut().item_spacing = vec2(5.0, 5.0);
        for (kind_key, icon, i18n_key) in [
            ("cmd", IconKind::Terminal, "ui.activityPanel.cmd"),
            ("edit", IconKind::Pencil, "ui.activityPanel.edit"),
            ("read", IconKind::File, "ui.activityPanel.read"),
            ("web", IconKind::Globe, "ui.activityPanel.web"),
            ("agent", IconKind::Users, "ui.activityPanel.agent"),
        ] {
            let active = state.activity_type_filter.as_deref() == Some(kind_key);
            let txt_col = if active { INK } else { MUTED };
            let galley = ui.painter().layout_no_wrap(
                i18n.tr(i18n_key),
                FontId::new(10.5, FontFamily::Name(crate::theme::FONT_BOLD.into())),
                txt_col,
            );
            let w = (8.0 + 11.0 + 4.0 + galley.size().x + 8.0).ceil();
            let (rect, resp) = ui.allocate_exact_size(vec2(w, 25.0), Sense::click());
            let fill = if active {
                ACCENT_SOFT
            } else if resp.hovered() {
                CARD_2
            } else {
                Color32::TRANSPARENT
            };
            let border = if active { ACCENT_LINE } else { LINE_2 };
            ui.painter().rect(
                rect,
                CornerRadius::same(12),
                fill,
                Stroke::new(1.0_f32, border),
                StrokeKind::Inside,
            );
            let ic_r = Rect::from_min_size(
                egui::pos2(rect.left() + 8.0, rect.center().y - 5.5),
                vec2(11.0, 11.0),
            );
            icons::paint_icon(ui.painter(), ic_r, icon, txt_col);
            ui.painter().galley(
                egui::pos2(ic_r.right() + 4.0, rect.center().y - galley.size().y * 0.5),
                galley,
                txt_col,
            );
            if resp.clicked() {
                if active {
                    state.activity_type_filter = None;
                } else {
                    state.activity_type_filter = Some(kind_key.to_owned());
                }
            }
        }
    });
}

fn draw_feed_card(
    ui: &mut egui::Ui,
    ev: &FeedEntry,
    streamer_mode: bool,
    i18n: &I18nStore,
    panel_width: f32,
) {
    let (card_fill, card_stroke) = if ev.level == "err" {
        (
            RED_SOFT,
            Stroke::new(1.0_f32, Color32::from_rgba_unmultiplied(0xFF, 0x5F, 0x5F, 128)),
        )
    } else if ev.level == "warn" || ev.attention {
        (
            AMBER_SOFT,
            Stroke::new(1.0_f32, Color32::from_rgba_unmultiplied(0xF2, 0xB5, 0x44, 140)),
        )
    } else {
        (CARD, Stroke::new(1.0_f32, LINE))
    };

    let card_inner_w = (panel_width - 44.0).max(160.0);
    let resp = egui::Frame::NONE
        .fill(card_fill)
        .corner_radius(CornerRadius::same(12))
        .inner_margin(Margin::symmetric(10, 9))
        .stroke(card_stroke)
        .show(ui, |ui| {
            ui.set_width(card_inner_w);
            ui.set_max_width(card_inner_w);
            // Header row: agent dot + bold name + monospace time
            ui.horizontal(|ui| {
                ui.spacing_mut().item_spacing.x = 6.0;
                let (d, _) = ui.allocate_exact_size(vec2(8.0, 8.0), Sense::hover());
                ui.painter()
                    .circle_filled(d.center(), 3.6, agent_dot_color(&ev.agent));
                ui.label(heading_bold(clip_chars(&ev.agent, 20), 12.0));
                ui.with_layout(egui::Layout::right_to_left(Align::Center), |ui| {
                    ui.label(
                        RichText::new(&ev.ts_label)
                            .font(FontId::new(10.5, FontFamily::Monospace))
                            .color(FAINT),
                    );
                });
            });
            ui.add_space(5.0);

            // Event row: 22x22 tone icon + bounded wrapped message
            ui.horizontal(|ui| {
                ui.spacing_mut().item_spacing.x = 8.0;
                let (ic_bg, ic_fg, ic_kind) = feed_icon_style(ev);
                let (r, _) = ui.allocate_exact_size(vec2(22.0, 22.0), Sense::hover());
                ui.painter()
                    .rect_filled(r, CornerRadius::same(6), ic_bg);
                icons::paint_icon(
                    ui.painter(),
                    Rect::from_center_size(r.center(), vec2(12.0, 12.0)),
                    ic_kind,
                    ic_fg,
                );

                let msg = if streamer_mode {
                    i18n.tr("station.streamerRedacted")
                } else {
                    ev.message.clone()
                };
                let text_w = (card_inner_w - 32.0).max(80.0);
                ui.allocate_ui_with_layout(
                    vec2(text_w, 0.0),
                    egui::Layout::top_down(Align::LEFT),
                    |ui| {
                        ui.add(
                            egui::Label::new(RichText::new(msg).size(11.5).color(FEED_TEXT)).wrap(),
                        );
                    },
                );
            });
        });

    if resp.response.hovered() && ev.level != "err" && ev.level != "warn" {
        ui.painter().rect_stroke(
            resp.response.rect,
            CornerRadius::same(12),
            Stroke::new(1.0_f32, CARD_2),
            StrokeKind::Inside,
        );
    }
}

fn feed_icon_style(ev: &FeedEntry) -> (Color32, Color32, IconKind) {
    if ev.level == "err" {
        return (RED, Color32::WHITE, IconKind::Alert);
    }
    if ev.level == "warn" {
        return (AMBER, Color32::from_rgb(0x1E, 0x14, 0x05), IconKind::Alert);
    }
    match ev.kind.as_str() {
        "cmd" => (CMD_GREEN_BG, CMD_GREEN_INK, IconKind::Terminal),
        "edit" => (CMD_GREEN_BG, CMD_GREEN_INK, IconKind::Pencil),
        "read" => (CARD_3, MUTED, IconKind::File),
        "web" => (BLUE_SOFT, BLUE, IconKind::Globe),
        "agent" => (BLUE_SOFT, BLUE, IconKind::Users),
        _ => (CARD_3, MUTED, IconKind::Check),
    }
}

fn compact_count(n: u64) -> String {
    if n >= 10_000 {
        format!("{}k", n / 1000)
    } else if n >= 1_000 {
        format!("{:.1}k", n as f64 / 1000.0)
    } else {
        n.to_string()
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
