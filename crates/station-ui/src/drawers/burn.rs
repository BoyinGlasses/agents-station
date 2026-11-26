use crate::i18n::I18nStore;
use crate::state::{ActiveDrawer, BurnDayBucketUi, BurnDetailRow, BurnRange, UiAction, UiState};
use crate::theme::{
    ACCENT, ACCENT_HI, AMBER, BG, BLUE, CARD, CARD_2, FAINT, GREEN, INK, LINE, LINE_2, MUTED, RED,
    SIDE, heading_bold,
};
use bevy_egui::egui::{
    self, Align, Color32, CornerRadius, Margin, Rect, RichText, Sense, Stroke, vec2,
};

#[derive(Debug, Clone, PartialEq)]
pub struct ScaledBurnBar {
    pub day: String,
    pub total_usd: f64,
    pub height_px: f32,
    pub segments: Vec<(String, f32)>,
}

/// Scales daily burn buckets proportionally to `max_bar_px` with zero-max safety and minimum 3px visibility.
pub fn scale_burn_buckets(per_day: &[BurnDayBucketUi], max_bar_px: f32) -> Vec<ScaledBurnBar> {
    let max_usd = per_day
        .iter()
        .map(|d| if d.usd.is_finite() && d.usd > 0.0 { d.usd } else { 0.0 })
        .fold(0.0_f64, f64::max);
    let cap_px = max_bar_px.max(0.0);

    per_day
        .iter()
        .map(|bucket| {
            let usd = if bucket.usd.is_finite() && bucket.usd > 0.0 {
                bucket.usd
            } else {
                0.0
            };
            let height_px = if max_usd > 0.0 && usd > 0.0 && cap_px > 0.0 {
                (((usd / max_usd) as f32) * cap_px).max(3.0).min(cap_px)
            } else {
                0.0
            };
            let seg_total: f64 = bucket
                .models
                .iter()
                .map(|(_, v)| if v.is_finite() && *v > 0.0 { *v } else { 0.0 })
                .sum();
            let segments = if height_px > 0.0 && seg_total > 0.0 {
                bucket
                    .models
                    .iter()
                    .filter(|(_, v)| v.is_finite() && *v > 0.0)
                    .map(|(m, v)| (m.clone(), ((*v / seg_total) as f32) * height_px))
                    .collect()
            } else {
                Vec::new()
            };
            ScaledBurnBar {
                day: bucket.day.clone(),
                total_usd: usd,
                height_px,
                segments,
            }
        })
        .collect()
}

fn model_color(name: &str) -> Color32 {
    let low = name.to_ascii_lowercase();
    if low.contains("opus") {
        Color32::from_rgb(0xEF, 0x8A, 0x5F)
    } else if low.contains("sonnet") {
        Color32::from_rgb(0x5B, 0x9C, 0xF5)
    } else if low.contains("haiku") {
        Color32::from_rgb(0x3D, 0xC9, 0x7A)
    } else if low.contains("fable") || low.contains("mythos") {
        Color32::from_rgb(0xB8, 0x7F, 0xF5)
    } else {
        Color32::from_rgb(0xEA, 0xA8, 0x45)
    }
}

fn source_color(name: &str) -> Color32 {
    match name {
        "main" => BLUE,
        "subagent" => ACCENT,
        "auxiliary" => AMBER,
        _ => MUTED,
    }
}

fn fmt_usd(usd: f64, i18n: &I18nStore) -> String {
    if usd > 0.0 && usd < 0.01 {
        i18n.tr_vars("burn.less", &[("usd", "$0.01")])
    } else if usd >= 100.0 {
        format!("${usd:.0}")
    } else {
        format!("${usd:.2}")
    }
}

fn fmt_toks(n: f64, i18n: &I18nStore) -> String {
    if n >= 1_000_000.0 {
        let s = format!("{:.1}", n / 1_000_000.0);
        i18n.tr_vars("burn.tokM", &[("n", &s)])
    } else if n >= 1_000.0 {
        let s = if n >= 10_000.0 {
            format!("{:.0}", n / 1_000.0)
        } else {
            format!("{:.1}", n / 1_000.0)
        };
        i18n.tr_vars("burn.tokK", &[("n", &s)])
    } else {
        format!("{n:.0}")
    }
}

fn fmt_pct(share: f64) -> String {
    let pct = (share.clamp(0.0, 1.0)) * 100.0;
    if pct > 0.0 && pct < 9.5 {
        format!("{pct:.1}%")
    } else {
        format!("{pct:.0}%")
    }
}

fn short_model_name(raw: &str) -> String {
    let low = raw.to_ascii_lowercase();
    for fam in ["opus", "sonnet", "haiku", "fable", "mythos"] {
        if let Some(rest) = low.strip_prefix(&format!("claude-{fam}-")) {
            let mut cap = fam.chars();
            let title = match cap.next() {
                Some(c) => format!("{}{}", c.to_ascii_uppercase(), cap.as_str()),
                None => fam.to_owned(),
            };
            let ver: Vec<&str> = rest
                .split('-')
                .take(2)
                .filter(|p| p.chars().all(|c| c.is_ascii_digit()))
                .collect();
            return if ver.is_empty() {
                title
            } else {
                format!("{title} {}", ver.join("."))
            };
        }
    }
    if raw.is_empty() { "?".to_owned() } else { raw.to_owned() }
}

fn draw_share_row(
    ui: &mut egui::Ui,
    badge: Option<(&str, Color32)>,
    name: &str,
    share: f64,
    value_str: &str,
    sub: Option<&str>,
) {
    ui.horizontal(|ui| {
        if let Some((tag, col)) = badge {
            egui::Frame::NONE
                .fill(Color32::from_rgba_unmultiplied(col.r(), col.g(), col.b(), 36))
                .corner_radius(CornerRadius::same(5))
                .inner_margin(Margin::symmetric(5, 1))
                .show(ui, |ui| {
                    ui.label(RichText::new(tag).size(9.5).color(col));
                });
        }
        ui.label(RichText::new(name).size(11.5).color(INK));
        ui.with_layout(egui::Layout::right_to_left(Align::Center), |ui| {
            ui.label(RichText::new(value_str).size(11.0).color(MUTED));
            ui.label(heading_bold(fmt_pct(share), 11.0).color(INK));
        });
    });
    let (rect, _) = ui.allocate_exact_size(vec2(ui.available_width(), 5.0), Sense::hover());
    ui.painter().rect_filled(rect, 2.5, BG);
    let frac = (share as f32).clamp(0.0, 1.0);
    if frac > 0.0 {
        let fill_w = (rect.width() * frac).max(3.0);
        let fill = Rect::from_min_size(rect.min, vec2(fill_w, rect.height()));
        ui.painter().rect_filled(fill, 2.5, ACCENT);
    }
    if let Some(meta) = sub
        && !meta.is_empty()
    {
        ui.label(RichText::new(meta).size(10.0).color(FAINT));
    }
    ui.add_space(4.0);
}

fn draw_stacked_bar(
    ui: &mut egui::Ui,
    items: &[BurnDetailRow],
    color_fn: fn(&str) -> Color32,
    name_fn: impl Fn(&str) -> String,
    money: bool,
    i18n: &I18nStore,
) {
    if items.is_empty() {
        return;
    }
    let (rect, _) = ui.allocate_exact_size(vec2(ui.available_width(), 10.0), Sense::hover());
    ui.painter().rect_filled(rect, 5.0, BG);
    let mut x = rect.left();
    for item in items.iter().take(5) {
        let w = (rect.width() * (item.share as f32).clamp(0.0, 1.0)).max(2.0);
        let seg = Rect::from_min_size(egui::pos2(x, rect.top()), vec2(w.min(rect.right() - x), rect.height()));
        if seg.width() > 0.5 {
            ui.painter().rect_filled(seg, 3.0, color_fn(&item.name));
        }
        x += w;
    }
    ui.add_space(5.0);
    for (idx, item) in items.iter().take(5).enumerate() {
        ui.push_id(("stack_leg", idx), |ui| {
            let col = color_fn(&item.name);
            ui.horizontal(|ui| {
                let (dot_r, _) = ui.allocate_exact_size(vec2(10.0, 10.0), Sense::hover());
                ui.painter().circle_filled(dot_r.center(), 4.0, col);
                ui.label(RichText::new(name_fn(&item.name)).size(11.0).color(INK));
                ui.with_layout(egui::Layout::right_to_left(Align::Center), |ui| {
                    let amt = if money {
                        fmt_usd(item.usd, i18n)
                    } else {
                        fmt_toks(item.tokens, i18n)
                    };
                    ui.label(RichText::new(amt).size(10.5).color(MUTED));
                    ui.label(heading_bold(fmt_pct(item.share), 10.5).color(INK));
                });
            });
        });
    }
}

/// Computes `(col_w, gap)` for daily burn bar charts accounting for `8.0`px horizontal padding
/// so 30-day charts fit every bar inside narrow drawers without clipping the rightmost bars.
pub fn compute_daily_chart_layout(avail_w: f32, count: usize) -> (f32, f32) {
    let n = count.max(1);
    let inner_w = (avail_w - 8.0).max(0.0);
    let gap = if n > 14 { 2.0 } else { 4.0 };
    let col_w = ((inner_w - gap * (n.saturating_sub(1) as f32)) / n as f32).clamp(4.0, 42.0);
    (col_w, gap)
}

fn draw_daily_chart(ui: &mut egui::Ui, per_day: &[BurnDayBucketUi], i18n: &I18nStore) {
    let bars = scale_burn_buckets(per_day, 84.0);
    if bars.is_empty() {
        return;
    }
    let count = bars.len().max(1);
    let avail = ui.available_width();
    let (col_w, gap) = compute_daily_chart_layout(avail, count);
    let chart_h = 98.0;
    let (area, _) = ui.allocate_exact_size(vec2(avail, chart_h), Sense::hover());
    ui.painter().rect_filled(area, 6.0, BG);

    let max_usd = bars.iter().map(|b| b.total_usd).fold(0.0_f64, f64::max);
    for (idx, bar) in bars.iter().enumerate() {
        let x0 = area.left() + 4.0 + idx as f32 * (col_w + gap);
        if x0 + col_w > area.right() - 3.5 {
            break;
        }
        let bottom = area.bottom() - 16.0;
        let mut cur_y = bottom;
        if bar.segments.is_empty() && bar.height_px > 0.0 {
            let r = Rect::from_min_max(egui::pos2(x0, bottom - bar.height_px), egui::pos2(x0 + col_w, bottom));
            ui.painter().rect_filled(r, 3.0, ACCENT);
        } else {
            for (model, seg_h) in &bar.segments {
                let h = seg_h.max(1.5);
                let top = (cur_y - h).max(area.top() + 12.0);
                let r = Rect::from_min_max(egui::pos2(x0, top), egui::pos2(x0 + col_w, cur_y));
                ui.painter().rect_filled(r, 2.0, model_color(model));
                cur_y = top;
            }
        }
        if bar.total_usd > 0.0 && (bar.total_usd - max_usd).abs() < 1e-6 {
            let cap = fmt_usd(bar.total_usd, i18n);
            ui.painter().text(
                egui::pos2(x0 + col_w * 0.5, (cur_y - 9.0).max(area.top() + 2.0)),
                egui::Align2::CENTER_TOP,
                cap,
                egui::FontId::proportional(9.0),
                ACCENT_HI,
            );
        }
        let short_day = bar.day.get(5..).unwrap_or(&bar.day);
        if count <= 10 || idx % 5 == 0 || idx == count - 1 {
            ui.painter().text(
                egui::pos2(x0 + col_w * 0.5, area.bottom() - 12.0),
                egui::Align2::CENTER_TOP,
                short_day,
                egui::FontId::proportional(9.0),
                MUTED,
            );
        }
    }
}

/// Full `BurnDrawer` matching `burn.js`: `Today | 7d | 30d` tabs, stacked daily bar chart,
/// ranked MCP/skill/plugin/subagent breakdown, by-model, by-source, roles, projects, and sessions.
pub fn draw_burn_drawer(ctx: &egui::Context, state: &mut UiState, i18n: &I18nStore) {
    if !ctx.wants_keyboard_input() && ctx.input(|i| i.key_pressed(egui::Key::Escape)) {
        state.active_drawer = ActiveDrawer::None;
        return;
    }

    if state.burn_drawer.loaded_range != Some(state.burn_drawer.range) && !state.burn_drawer.loading {
        state.burn_drawer.loading = true;
        let range = state.burn_drawer.range;
        state.push_action(UiAction::FetchBurn(range));
    }

    let mut open = true;
    egui::Window::new(heading_bold(i18n.tr("burn.title"), 14.0).color(INK))
        .id(egui::Id::new("station_burn_drawer"))
        .open(&mut open)
        .anchor(egui::Align2::RIGHT_TOP, vec2(-14.0, 62.0))
        .default_width(410.0)
        .max_height(600.0)
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
            // Range tabs: Today | 7 days | 30 days
            egui::Frame::NONE
                .fill(CARD)
                .corner_radius(CornerRadius::same(10))
                .inner_margin(Margin::same(3))
                .stroke(Stroke::new(1.0_f32, LINE))
                .show(ui, |ui| {
                    ui.columns(3, |cols| {
                        for (idx, (range, key)) in [
                            (BurnRange::Today, "burn.today"),
                            (BurnRange::Days7, "burn.week"),
                            (BurnRange::Days30, "station.burn30d"),
                        ]
                        .into_iter()
                        .enumerate()
                        {
                            cols[idx].push_id(idx, |ui| {
                                let active = state.burn_drawer.range == range;
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
                                    state.burn_drawer.range = range;
                                    state.burn_drawer.loading = true;
                                    state.push_action(UiAction::FetchBurn(range));
                                }
                            });
                        }
                    });
                });

            ui.add_space(8.0);
            let bd = state.burn_drawer.clone();
            let streamer = state.streamer_mode;
            let redacted = i18n.tr("station.streamerRedacted");

            egui::ScrollArea::vertical().max_height(510.0).show(ui, |ui| {
                if bd.loading && bd.loaded_range != Some(bd.range) {
                    ui.label(RichText::new(i18n.tr("burn.loading")).size(12.0).color(MUTED));
                    return;
                }
                if let Some(err) = &bd.error {
                    ui.label(RichText::new(format!("{}: {err}", i18n.tr("burn.unreachable"))).size(11.5).color(RED));
                    return;
                }
                if !(bd.total_usd > 0.0 || bd.total_tokens > 0.0) {
                    let msg_key = match bd.setup.as_str() {
                        "other" => "burn.empty.theirs",
                        "ours" => "burn.empty.waiting",
                        _ => match bd.range {
                            BurnRange::Today => "burn.empty.range.today",
                            _ => "burn.empty.range.7d",
                        },
                    };
                    ui.label(RichText::new(i18n.tr(msg_key)).size(12.0).color(MUTED));
                    return;
                }

                // Hero summary card
                egui::Frame::NONE
                    .fill(CARD)
                    .corner_radius(CornerRadius::same(10))
                    .inner_margin(Margin::symmetric(12, 10))
                    .stroke(Stroke::new(1.0_f32, LINE))
                    .show(ui, |ui| {
                        ui.set_width(ui.available_width());
                        ui.label(RichText::new(i18n.tr("burn.hero.label")).size(10.5).color(MUTED));
                        let big = if bd.money {
                            fmt_usd(bd.total_usd, i18n)
                        } else {
                            fmt_toks(bd.total_tokens, i18n)
                        };
                        ui.label(heading_bold(big, 24.0).color(ACCENT_HI));
                        let mut parts = vec![
                            i18n.tr_count("burn.sub.req", bd.req as usize),
                            i18n.tr_vars("burn.sub.tok", &[("n", &fmt_toks(bd.total_tokens, i18n))]),
                            i18n.tr_count("burn.sub.sessions", bd.sessions.len()),
                        ];
                        if bd.prompts > 0 && bd.money {
                            let per = fmt_usd(bd.total_usd / bd.prompts as f64, i18n);
                            parts.push(i18n.tr_vars("burn.perPrompt", &[("usd", &per)]));
                        }
                        if bd.err > 0 {
                            parts.push(i18n.tr_count("burn.sub.err", bd.err as usize));
                        }
                        ui.label(RichText::new(parts.join(" · ")).size(10.5).color(MUTED));
                    });

                // Stacked daily chart for 7d / 30d
                if bd.range != BurnRange::Today && !bd.per_day.is_empty() {
                    ui.add_space(8.0);
                    ui.label(heading_bold(i18n.tr("burn.perDay"), 12.0).color(INK));
                    ui.add_space(4.0);
                    draw_daily_chart(ui, &bd.per_day, i18n);
                }

                // Who eats the most (MCP / Skill / Plugin / Subagent)
                ui.add_space(10.0);
                ui.label(heading_bold(i18n.tr("burn.who.title"), 12.0).color(INK));
                ui.add_space(4.0);
                if bd.ranked.is_empty() {
                    ui.label(RichText::new(i18n.tr("burn.who.none")).size(11.0).color(MUTED));
                } else {
                    for (idx, r) in bd.ranked.iter().take(8).enumerate() {
                        ui.push_id(("ranked", idx), |ui| {
                            let kind_key = format!("burn.kind.{}", r.kind);
                            let kind_lbl = i18n.tr(&kind_key);
                            let tag_col = match r.kind.as_str() {
                                "mcp" => BLUE,
                                "skill" => GREEN,
                                "plugin" => AMBER,
                                _ => ACCENT,
                            };
                            let display_name = if streamer {
                                redacted.clone()
                            } else if matches!(
                                r.name.as_str(),
                                "custom" | "third-party" | "custom_skill"
                            ) {
                                i18n.tr("burn.hidden")
                            } else {
                                r.name.clone()
                            };
                            let val = if bd.money {
                                fmt_usd(r.usd, i18n)
                            } else {
                                fmt_toks(r.tokens, i18n)
                            };
                            draw_share_row(
                                ui,
                                Some((kind_lbl.as_str(), tag_col)),
                                &display_name,
                                r.share,
                                &val,
                                None,
                            );
                        });
                    }
                    if bd.plain_share > 0.0 {
                        let plain_val = fmt_usd(bd.plain_usd, i18n);
                        draw_share_row(
                            ui,
                            None,
                            &i18n.tr("burn.who.plain"),
                            bd.plain_share,
                            &plain_val,
                            None,
                        );
                    }
                    ui.label(RichText::new(i18n.tr("burn.who.note")).size(10.0).color(FAINT));
                }

                // By model
                if !bd.by_model.is_empty() {
                    ui.add_space(10.0);
                    ui.label(heading_bold(i18n.tr("burn.byModel"), 12.0).color(INK));
                    ui.add_space(4.0);
                    draw_stacked_bar(ui, &bd.by_model, model_color, short_model_name, bd.money, i18n);
                }

                // By source
                if !bd.by_source.is_empty() {
                    ui.add_space(10.0);
                    ui.label(heading_bold(i18n.tr("burn.bySource"), 12.0).color(INK));
                    ui.add_space(4.0);
                    draw_stacked_bar(
                        ui,
                        &bd.by_source,
                        source_color,
                        |s| match s {
                            "main" | "subagent" | "auxiliary" => i18n.tr(&format!("burn.source.{s}")),
                            other => other.to_owned(),
                        },
                        bd.money,
                        i18n,
                    );
                }

                // Coordinator vs Subagent roles
                if !bd.roles.is_empty() {
                    ui.add_space(10.0);
                    ui.label(heading_bold(i18n.tr("burn.roles.title"), 12.0).color(INK));
                    ui.add_space(4.0);
                    for (idx, r) in bd.roles.iter().take(6).enumerate() {
                        ui.push_id(("role", idx), |ui| {
                            let role_lbl = i18n.tr(&format!("burn.source.{}", r.role));
                            let label = format!("{role_lbl} · {}", short_model_name(&r.model));
                            let val = if bd.money {
                                fmt_usd(r.usd, i18n)
                            } else {
                                fmt_toks(r.tokens, i18n)
                            };
                            draw_share_row(ui, None, &label, r.share, &val, None);
                        });
                    }
                    ui.label(RichText::new(i18n.tr("burn.roles.note")).size(10.0).color(FAINT));
                }

                // By project
                if !bd.projects.is_empty() {
                    ui.add_space(10.0);
                    ui.label(heading_bold(i18n.tr("burn.projects"), 12.0).color(INK));
                    ui.add_space(4.0);
                    for (idx, p) in bd.projects.iter().take(10).enumerate() {
                        ui.push_id(("proj", idx), |ui| {
                            let name = if streamer {
                                redacted.clone()
                            } else if p.name.is_empty() {
                                i18n.tr("burn.noProject")
                            } else {
                                p.name.clone()
                            };
                            let val = fmt_usd(p.usd, i18n);
                            let sub = i18n.tr_count("burn.sub.sessions", p.sessions);
                            draw_share_row(ui, None, &name, p.share, &val, Some(&sub));
                        });
                    }
                }

                // By session
                if !bd.sessions.is_empty() {
                    ui.add_space(10.0);
                    ui.label(heading_bold(i18n.tr("burn.sessions"), 12.0).color(INK));
                    ui.add_space(4.0);
                    for (idx, s) in bd.sessions.iter().take(12).enumerate() {
                        ui.push_id(("sess", idx), |ui| {
                            let name = if streamer {
                                redacted.clone()
                            } else if !s.name.is_empty() {
                                s.name.clone()
                            } else {
                                let short_id: String = s.sid.chars().take(6).collect();
                                let proj = if s.project.is_empty() {
                                    i18n.tr("burn.noProject")
                                } else {
                                    s.project.clone()
                                };
                                format!("{proj} · {short_id}")
                            };
                            let val = fmt_usd(s.usd, i18n);
                            let models_str = s
                                .models
                                .iter()
                                .map(|m| short_model_name(m))
                                .collect::<Vec<_>>()
                                .join(", ");
                            let req_s = i18n.tr_count("burn.sub.req", s.req as usize);
                            let sub = if models_str.is_empty() {
                                req_s
                            } else {
                                format!("{models_str} · {req_s}")
                            };
                            draw_share_row(ui, None, &name, s.share, &val, Some(&sub));
                        });
                    }
                }

                ui.add_space(8.0);
                ui.label(RichText::new(i18n.tr("burn.foot")).size(10.0).color(FAINT));
            });
        });

    if !open {
        state.active_drawer = ActiveDrawer::None;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::state::BurnRange;

    #[test]
    fn burn_chart_bucket_scaling_handles_ranges_and_zero_max_safely() {
        assert_eq!(BurnRange::Today.api_str(), "today");
        assert_eq!(BurnRange::Days7.api_str(), "7d");
        assert_eq!(BurnRange::Days30.api_str(), "30d");

        let empty_days = vec![
            BurnDayBucketUi {
                day: "2026-11-24".to_owned(),
                usd: 0.0,
                models: vec![],
            },
            BurnDayBucketUi {
                day: "2026-11-25".to_owned(),
                usd: 0.0,
                models: vec![],
            },
        ];
        let zero_scaled = scale_burn_buckets(&empty_days, 96.0);
        assert_eq!(zero_scaled.len(), 2);
        assert!(zero_scaled.iter().all(|b| b.height_px == 0.0 && b.height_px.is_finite()));

        let days = vec![
            BurnDayBucketUi {
                day: "2026-11-23".to_owned(),
                usd: 0.01,
                models: vec![("claude-haiku-4-5".to_owned(), 0.01)],
            },
            BurnDayBucketUi {
                day: "2026-11-24".to_owned(),
                usd: 10.0,
                models: vec![
                    ("claude-opus-4-6".to_owned(), 6.0),
                    ("claude-sonnet-4-6".to_owned(), 4.0),
                ],
            },
            BurnDayBucketUi {
                day: "2026-11-25".to_owned(),
                usd: 20.0,
                models: vec![("claude-opus-4-6".to_owned(), 20.0)],
            },
        ];
        let scaled = scale_burn_buckets(&days, 100.0);
        assert_eq!(scaled.len(), 3);
        assert!(scaled[0].height_px >= 3.0, "tiny positive spend must remain visible");
        assert!((scaled[1].height_px - 50.0).abs() < 0.5);
        assert!((scaled[2].height_px - 100.0).abs() < 0.5);
        assert_eq!(scaled[1].segments.len(), 2);
        let seg_sum: f32 = scaled[1].segments.iter().map(|(_, h)| *h).sum();
        assert!((seg_sum - scaled[1].height_px).abs() < 0.5);
    }

    #[test]
    fn burn_30_day_chart_fits_all_30_bars_inside_366px_plot_width() {
        let avail_w = 366.0_f32;
        let (col_w, gap) = compute_daily_chart_layout(avail_w, 30);
        let mut rendered = 0usize;
        for idx in 0..30 {
            let x0 = 4.0 + idx as f32 * (col_w + gap);
            if x0 + col_w <= avail_w - 3.5 {
                rendered += 1;
            }
        }
        assert_eq!(rendered, 30, "all 30 daily bars must fit inside 366px plot without clipping");
        assert!(col_w >= 4.0);
        assert_eq!(gap, 2.0);
    }
}
