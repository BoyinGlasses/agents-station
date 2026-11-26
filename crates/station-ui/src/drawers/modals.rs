use crate::i18n::I18nStore;
use crate::state::{ActiveDrawer, GpuPrefUi, PhoneTab, UiAction, UiState, WrappedSummaryUi};
use crate::theme::{
    ACCENT, ACCENT_HI, AMBER, BG, BLUE, CARD, CARD_2, FAINT, GREEN, INK, LINE, LINE_2, MUTED, RED,
    SIDE, heading_bold, status_dot,
};
use bevy_egui::egui::{
    self, Align, Color32, CornerRadius, Margin, RichText, Stroke, TextureFilter, TextureOptions, vec2,
};
use station_3d::{BackdropKind, ChibiLook, OfficeLayout};
use std::net::Ipv4Addr;

const TIMELAPSE_TOTAL_SEC: u32 = 15;
const PHONE_QR_CACHE_ID: &str = "station_phone_qr_texture_cache";

#[derive(Clone)]
struct CachedPhoneQr {
    url: String,
    tex: egui::TextureHandle,
}

/// Drops cached PhoneModal QR GPU texture when the modal closes.
pub fn clear_phone_qr_cache(ctx: &egui::Context) {
    ctx.data_mut(|d| {
        d.remove::<CachedPhoneQr>(egui::Id::new(PHONE_QR_CACHE_ID));
    });
}

fn get_or_build_phone_qr_texture(ctx: &egui::Context, url: &str) -> Option<egui::TextureHandle> {
    if let Some(cached) = ctx.data(|d| d.get_temp::<CachedPhoneQr>(egui::Id::new(PHONE_QR_CACHE_ID)))
        && cached.url == url
    {
        return Some(cached.tex);
    }
    let (rgba, w, h) = station_canvas2d::render_qr_rgba(url, 184).ok()?;
    let img = egui::ColorImage::from_rgba_unmultiplied([w as usize, h as usize], &rgba);
    let opts = TextureOptions {
        magnification: TextureFilter::Nearest,
        minification: TextureFilter::Nearest,
        ..Default::default()
    };
    let tex = ctx.load_texture("station_phone_qr", img, opts);
    ctx.data_mut(|d| {
        d.insert_temp(
            egui::Id::new(PHONE_QR_CACHE_ID),
            CachedPhoneQr {
                url: url.to_owned(),
                tex: tex.clone(),
            },
        );
    });
    Some(tex)
}

/// Selects the best Phone PWA pairing URL:
/// 1) Active Cloudflare `https://` tunnel URL
/// 2) Private RFC-1918 LAN IPv4 (`10.x`, `172.16-31.x`, `192.168.x`)
/// 3) Loopback `127.0.0.1` fallback only if neither tunnel nor private LAN IPv4 exists.
pub fn select_phone_url(tunnel_url: Option<&str>, lan_ip: Option<&str>, port: u16) -> String {
    if let Some(raw) = tunnel_url.map(str::trim).filter(|s| !s.is_empty())
        && (raw.starts_with("https://") || raw.starts_with("http://"))
    {
        let trimmed = raw.trim_end_matches('/');
        if trimmed.ends_with("/phone") {
            return trimmed.to_owned();
        }
        return format!("{trimmed}/phone");
    }
    if let Some(ip_raw) = lan_ip.map(str::trim).filter(|s| !s.is_empty())
        && let Ok(v4) = ip_raw.parse::<Ipv4Addr>()
        && v4.is_private()
    {
        return format!("http://{v4}:{port}/phone");
    }
    format!("http://127.0.0.1:{port}/phone")
}

fn format_pin_digits(pin: &str) -> String {
    let digits: Vec<char> = pin.chars().filter(|c| c.is_ascii_digit()).collect();
    if digits.len() == 6 {
        format!(
            "{} {}",
            digits[0..3].iter().collect::<String>(),
            digits[3..6].iter().collect::<String>()
        )
    } else if digits.len() == 8 {
        format!(
            "{} {}",
            digits[0..4].iter().collect::<String>(),
            digits[4..8].iter().collect::<String>()
        )
    } else {
        digits.into_iter().collect()
    }
}

pub fn format_wrapped_date(day: &str, i18n: &I18nStore) -> String {
    let mut parts = day.split('-');
    let _y = parts.next();
    let Some(m) = parts.next() else {
        return day.to_owned();
    };
    let Some(d) = parts.next() else {
        return day.to_owned();
    };
    i18n.tr_vars("wrapped.date", &[("d", d), ("m", m)])
}

pub fn format_wrapped_date_range(w: &WrappedSummaryUi, i18n: &I18nStore) -> String {
    if w.from.is_empty() && w.to.is_empty() {
        return i18n.tr("wrapped.fileNow");
    }
    if w.from == w.to || w.from.is_empty() {
        return format_wrapped_date(&w.to, i18n);
    }
    format!(
        "{} – {}",
        format_wrapped_date(&w.from, i18n),
        format_wrapped_date(&w.to, i18n)
    )
}

pub fn format_wrapped_hours_key(key: &str, hours: f64, i18n: &I18nStore) -> String {
    let rounded = (hours * 10.0).round() / 10.0;
    let h_str = if (rounded - rounded.round()).abs() < 0.05 {
        format!("{:.0}", rounded)
    } else {
        format!("{:.1}", rounded)
    };
    let n_str = if (rounded - 1.0).abs() < 0.05 { "1" } else { "2" };
    i18n.tr_vars(key, &[("h", &h_str), ("n", n_str)])
}

pub fn format_wrapped_highlights(
    w: &WrappedSummaryUi,
    streamer: bool,
    i18n: &I18nStore,
) -> Vec<String> {
    let mut out = Vec::new();
    if let Some((proj, hrs)) = &w.top_project {
        let name = if streamer {
            i18n.tr("wrapped.aProject")
        } else {
            proj.clone()
        };
        let rounded = (hrs * 10.0).round() / 10.0;
        let h_str = format!("{rounded:.1}");
        let n_str = if (rounded - 1.0).abs() < 0.05 { "1" } else { "2" };
        out.push(i18n.tr_vars(
            "wrapped.topProject",
            &[("name", &name), ("h", &h_str), ("n", n_str)],
        ));
    }
    if let Some((day, weekday, _hrs)) = &w.busiest_day {
        let weekday_lbl = i18n.tr_pick("people.duty.board.days", (*weekday as usize) % 7);
        let date = format_wrapped_date(day, i18n);
        out.push(i18n.tr_vars(
            "wrapped.busiestDay",
            &[("day", &weekday_lbl), ("date", &date)],
        ));
    }
    if w.longest_min > 0 {
        let time_str = if w.longest_min >= 60 {
            i18n.tr_vars(
                "time.hourMin",
                &[
                    ("h", &(w.longest_min / 60).to_string()),
                    ("m", &(w.longest_min % 60).to_string()),
                ],
            )
        } else {
            i18n.tr_count("time.min", w.longest_min as usize)
        };
        out.push(i18n.tr_vars("wrapped.longest", &[("time", &time_str)]));
    }
    if w.commits > 0 || w.pushes > 0 {
        let c = i18n.tr_vars(
            "wrapped.commits",
            &[
                ("n", &w.commits.to_string()),
                ("c", &w.commits.to_string()),
            ],
        );
        let p = i18n.tr_vars(
            "wrapped.pushes",
            &[
                ("n", &w.pushes.to_string()),
                ("c", &w.pushes.to_string()),
            ],
        );
        out.push(format!("{c} · {p}"));
    }
    out.truncate(3);
    out
}

/// Weekly recap `Wrapped` modal matching `src/office/ui/wrapped.js` with 1080x1350 PNG export.
pub fn draw_wrapped_modal(ctx: &egui::Context, state: &mut UiState, i18n: &I18nStore) {
    if !ctx.wants_keyboard_input() && ctx.input(|i| i.key_pressed(egui::Key::Escape)) {
        state.active_drawer = ActiveDrawer::None;
        return;
    }

    if !state.wrapped.loaded && !state.wrapped.loading {
        state.wrapped.loading = true;
        state.push_action(UiAction::FetchWrapped);
    }

    let mut open = true;
    egui::Window::new(heading_bold(i18n.tr("ui.wrapped.title"), 14.5).color(INK))
        .id(egui::Id::new("station_wrapped_modal"))
        .open(&mut open)
        .anchor(egui::Align2::CENTER_CENTER, vec2(0.0, 0.0))
        .default_width(470.0)
        .max_height(640.0)
        .collapsible(false)
        .resizable(false)
        .frame(
            egui::Frame::window(&ctx.style())
                .fill(SIDE)
                .stroke(Stroke::new(1.2_f32, LINE_2))
                .corner_radius(CornerRadius::same(14))
                .inner_margin(Margin::symmetric(16, 14)),
        )
        .show(ctx, |ui| {
            let w = state.wrapped.clone();

            // Status / note banner
            let note = if w.loading {
                i18n.tr("wrapped.loading")
            } else if w.error.is_some() {
                i18n.tr("wrapped.loadFailed")
            } else if w.days < 7 {
                i18n.tr_count("wrapped.partial", w.days)
            } else {
                i18n.tr("wrapped.full")
            };
            ui.label(
                RichText::new(note)
                    .size(11.0)
                    .color(if w.error.is_some() { RED } else { MUTED }),
            );
            ui.add_space(8.0);

            // Stylized 4:5 Wrapped card preview matching wrapped.js
            egui::Frame::NONE
                .fill(Color32::from_rgb(0x16, 0x16, 0x30))
                .corner_radius(CornerRadius::same(14))
                .inner_margin(Margin::symmetric(16, 14))
                .stroke(Stroke::new(1.2_f32, Color32::from_rgba_unmultiplied(0xEF, 0x8A, 0x5F, 80)))
                .show(ui, |ui| {
                    ui.set_min_width(ui.available_width());
                    ui.horizontal(|ui| {
                        ui.label(heading_bold(i18n.tr("wrapped.title"), 16.0).color(Color32::WHITE));
                        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                            ui.label(
                                RichText::new(format_wrapped_date_range(&w, i18n))
                                    .size(11.0)
                                    .color(MUTED),
                            );
                        });
                    });
                    ui.add_space(8.0);

                    // Headline hours
                    ui.label(
                        heading_bold(format_wrapped_hours_key("wrapped.hours", w.hours_sum, i18n), 32.0)
                            .color(Color32::from_rgb(0xFF, 0xB8, 0x6B)),
                    );
                    ui.label(
                        RichText::new(format_wrapped_hours_key("wrapped.worked", w.hours_wall, i18n))
                            .size(12.0)
                            .color(INK),
                    );
                    let wait_str = format_wrapped_hours_key("wrapped.waited", w.wait_hours, i18n);
                    let sess_str = i18n.tr_vars(
                        "wrapped.sessions",
                        &[
                            ("n", &w.sessions.to_string()),
                            ("c", &w.sessions.to_string()),
                        ],
                    );
                    ui.label(
                        RichText::new(format!("{wait_str} · {sess_str}"))
                            .size(11.0)
                            .color(MUTED),
                    );
                    ui.add_space(10.0);

                    // 4 KPI tiles
                    let tiles = [
                        (w.prompts.to_string(), i18n.tr("wrapped.prompts")),
                        (w.edits.to_string(), i18n.tr("wrapped.edits")),
                        (w.cmds.to_string(), i18n.tr("wrapped.cmds")),
                        (format!("${:.1}", w.cost), i18n.tr("wrapped.cost")),
                    ];
                    ui.columns(4, |cols| {
                        for (idx, (val, lbl)) in tiles.iter().enumerate() {
                            cols[idx].push_id(idx, |ui| {
                                egui::Frame::NONE
                                    .fill(Color32::from_rgba_unmultiplied(255, 255, 255, 14))
                                    .corner_radius(CornerRadius::same(10))
                                    .inner_margin(Margin::symmetric(8, 8))
                                    .stroke(Stroke::new(
                                        1.0_f32,
                                        Color32::from_rgba_unmultiplied(255, 255, 255, 24),
                                    ))
                                    .show(ui, |ui| {
                                        ui.set_min_width(ui.available_width());
                                        ui.label(heading_bold(val, 15.0).color(Color32::WHITE));
                                        ui.label(RichText::new(lbl).size(10.0).color(MUTED));
                                    });
                            });
                        }
                    });

                    // Highlights
                    let highlights = format_wrapped_highlights(&w, state.streamer_mode, i18n);
                    if !highlights.is_empty() {
                        ui.add_space(10.0);
                        for (idx, line) in highlights.iter().enumerate() {
                            ui.push_id(idx, |ui| {
                                ui.label(RichText::new(line).size(11.5).color(INK));
                            });
                        }
                    }

                    ui.add_space(8.0);
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        ui.label(
                            RichText::new(i18n.tr("wrapped.footer"))
                                .size(10.0)
                                .color(MUTED),
                        );
                    });
                });

            if let Some(saved) = &w.last_saved_path {
                ui.add_space(6.0);
                ui.label(RichText::new(saved).size(10.5).color(GREEN));
            }

            ui.add_space(10.0);
            ui.horizontal(|ui| {
                let save_btn = egui::Button::new(
                    heading_bold(i18n.tr("ui.wrapped.save"), 12.0)
                        .color(Color32::from_rgb(0x2A, 0x13, 0x07)),
                )
                .fill(ACCENT)
                .corner_radius(CornerRadius::same(9));
                if ui.add_enabled(!w.loading, save_btn).clicked() {
                    state.push_action(UiAction::ExportWrappedPng {
                        copy_clipboard: false,
                    });
                }

                let copy_btn = egui::Button::new(
                    heading_bold(i18n.tr("ui.wrapped.copy"), 12.0).color(INK),
                )
                .fill(CARD_2)
                .stroke(Stroke::new(1.0_f32, LINE_2))
                .corner_radius(CornerRadius::same(9));
                if ui.add_enabled(!w.loading, copy_btn).clicked() {
                    state.push_action(UiAction::ExportWrappedPng {
                        copy_clipboard: true,
                    });
                }

                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    ui.label(RichText::new("1080 × 1350 PNG").size(10.5).color(MUTED));
                });
            });
        });

    if !open {
        state.active_drawer = ActiveDrawer::None;
    }
}

/// Floating timelapse recording progress pill with cancel action matching `timelapse.js`.
pub fn draw_timelapse_overlay(ctx: &egui::Context, state: &mut UiState, i18n: &I18nStore) {
    if !state.timelapse_recording {
        return;
    }
    egui::Area::new(egui::Id::new("station_timelapse_overlay"))
        .order(egui::Order::Foreground)
        .anchor(egui::Align2::CENTER_TOP, vec2(0.0, 68.0))
        .show(ctx, |ui| {
            egui::Frame::NONE
                .fill(CARD)
                .corner_radius(CornerRadius::same(12))
                .inner_margin(Margin::symmetric(14, 10))
                .stroke(Stroke::new(1.3_f32, ACCENT))
                .show(ui, |ui| {
                    ui.set_min_width(280.0);
                    ui.horizontal(|ui| {
                        let s = state.timelapse_elapsed_sec.min(TIMELAPSE_TOTAL_SEC);
                        let label = i18n.tr_vars(
                            "ui.clip.making",
                            &[
                                ("s", &s.to_string()),
                                ("total", &TIMELAPSE_TOTAL_SEC.to_string()),
                            ],
                        );
                        ui.label(heading_bold(label, 12.0).color(AMBER));
                        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                            let stop_btn = egui::Button::new(
                                heading_bold(i18n.tr("ui.clip.stop"), 11.0).color(Color32::WHITE),
                            )
                            .fill(RED)
                            .corner_radius(CornerRadius::same(7));
                            if ui.add(stop_btn).clicked() {
                                state.timelapse_recording = false;
                                state.timelapse_progress = 0.0;
                                state.push_action(UiAction::CancelTimelapseClip);
                            }
                        });
                    });
                    ui.add_space(6.0);
                    let (bar_r, _) =
                        ui.allocate_exact_size(vec2(280.0, 6.0), egui::Sense::hover());
                    ui.painter().rect_filled(bar_r, 3.0, BG);
                    let mut fill = bar_r;
                    fill.set_width(bar_r.width() * state.timelapse_progress.clamp(0.0, 1.0));
                    ui.painter().rect_filled(fill, 3.0, BLUE);
                });
        });
}

/// Full `PhoneModal` matching `topbar.js`: real ISO-18004 QR code (cached GPU texture),
/// Home Wi-Fi / Cloudflare Away tabs, LAN & Cloudflare quick/custom-domain tunnel toggles,
/// PIN view/edit/2-click regenerate, phone-approve toggle, and Web Push test button.
pub fn draw_phone_modal(ctx: &egui::Context, state: &mut UiState, i18n: &I18nStore) {
    if !ctx.wants_keyboard_input() && ctx.input(|i| i.key_pressed(egui::Key::Escape)) {
        state.active_drawer = ActiveDrawer::None;
        clear_phone_qr_cache(ctx);
        return;
    }

    if !state.phone_modal.loaded {
        state.phone_modal.loaded = true;
        state.push_action(UiAction::FetchPhoneStatus);
    }

    let mut open = true;
    egui::Window::new(heading_bold(i18n.tr("ui.phoneModal.title"), 14.5).color(INK))
        .id(egui::Id::new("station_phone_modal"))
        .open(&mut open)
        .anchor(egui::Align2::CENTER_CENTER, vec2(0.0, 0.0))
        .default_width(470.0)
        .max_height(640.0)
        .collapsible(false)
        .resizable(false)
        .frame(
            egui::Frame::window(&ctx.style())
                .fill(SIDE)
                .stroke(Stroke::new(1.2_f32, LINE_2))
                .corner_radius(CornerRadius::same(14))
                .inner_margin(Margin::symmetric(16, 14)),
        )
        .show(ctx, |ui| {
            egui::ScrollArea::vertical().max_height(575.0).show(ui, |ui| {
                // Home Wi-Fi | Away (Cloudflare) tabs
                egui::Frame::NONE
                    .fill(CARD)
                    .corner_radius(CornerRadius::same(10))
                    .inner_margin(Margin::same(3))
                    .stroke(Stroke::new(1.0_f32, LINE))
                    .show(ui, |ui| {
                        ui.columns(2, |cols| {
                            for (idx, (tab, key, on)) in [
                                (PhoneTab::Home, "topbar.phone.tabHome", state.phone_modal.lan_on),
                                (PhoneTab::Away, "topbar.phone.tabAway", state.phone_modal.tunnel_on),
                            ]
                            .into_iter()
                            .enumerate()
                            {
                                cols[idx].push_id(("pm_tab", idx), |ui| {
                                    let active = state.phone_modal.tab == tab;
                                    let st_lbl = i18n.tr(if on {
                                        "topbar.phone.isOn"
                                    } else {
                                        "topbar.phone.isOff"
                                    });
                                    let lbl = format!("{} · {st_lbl}", i18n.tr(key));
                                    let btn = egui::Button::new(
                                        heading_bold(lbl, 11.5)
                                            .color(if active { INK } else { MUTED }),
                                    )
                                    .fill(if active { CARD_2 } else { Color32::TRANSPARENT })
                                    .stroke(if active {
                                        Stroke::new(1.0_f32, ACCENT)
                                    } else {
                                        Stroke::NONE
                                    })
                                    .corner_radius(CornerRadius::same(7))
                                    .min_size(vec2(ui.available_width(), 28.0));
                                    if ui.add(btn).clicked() {
                                        state.phone_modal.tab = tab;
                                    }
                                });
                            }
                        });
                    });

                ui.add_space(8.0);
                let active_tunnel_url = state
                    .phone_modal
                    .tunnel_url
                    .as_deref()
                    .filter(|s| state.phone_modal.tunnel_on && !s.is_empty())
                    .map(str::to_owned);
                let lan_ip = state.phone_modal.lan_ip.clone();
                let qr_url = match state.phone_modal.tab {
                    PhoneTab::Away if active_tunnel_url.is_some() => {
                        select_phone_url(active_tunnel_url.as_deref(), None, state.server_port)
                    }
                    PhoneTab::Home => select_phone_url(None, lan_ip.as_deref(), state.server_port),
                    _ => select_phone_url(
                        active_tunnel_url.as_deref(),
                        lan_ip.as_deref(),
                        state.server_port,
                    ),
                };

                // Tab-specific controls (LAN vs Cloudflare tunnel)
                egui::Frame::NONE
                    .fill(CARD)
                    .corner_radius(CornerRadius::same(10))
                    .inner_margin(Margin::symmetric(11, 10))
                    .stroke(Stroke::new(1.0_f32, LINE))
                    .show(ui, |ui| {
                        ui.set_width(ui.available_width());
                        match state.phone_modal.tab {
                            PhoneTab::Home => {
                                let mut lan_on = state.phone_modal.lan_on;
                                if ui
                                    .checkbox(&mut lan_on, i18n.tr("topbar.phone.lanSwitch"))
                                    .changed()
                                {
                                    state.phone_modal.lan_on = lan_on;
                                    state.push_action(UiAction::ToggleLan(lan_on));
                                }
                                if !lan_on {
                                    ui.label(
                                        RichText::new(i18n.tr("topbar.phone.lanOffNote"))
                                            .size(10.5)
                                            .color(MUTED),
                                    );
                                }
                            }
                            PhoneTab::Away => {
                                let mut tun_on = state.phone_modal.tunnel_on;
                                if ui
                                    .checkbox(&mut tun_on, i18n.tr("topbar.phone.tunSwitch"))
                                    .changed()
                                {
                                    state.phone_modal.tunnel_on = tun_on;
                                    state.push_action(UiAction::ToggleTunnel(tun_on));
                                }
                                if !tun_on {
                                    ui.label(
                                        RichText::new(i18n.tr("topbar.phone.tunOffNote"))
                                            .size(10.5)
                                            .color(MUTED),
                                    );
                                } else {
                                    if let Some(err) = &state.phone_modal.tunnel_error {
                                        let msg = i18n.tr_vars("topbar.phone.tunError", &[("error", err)]);
                                        ui.label(RichText::new(msg).size(10.5).color(RED));
                                    } else if active_tunnel_url.is_none() {
                                        let key = if state.phone_modal.tunnel_state == "download" {
                                            "topbar.phone.tunDownload"
                                        } else {
                                            "topbar.phone.tunStarting"
                                        };
                                        ui.label(RichText::new(i18n.tr(key)).size(10.5).color(AMBER));
                                    }

                                    // Custom domain box
                                    ui.add_space(6.0);
                                    ui.label(
                                        heading_bold(i18n.tr("topbar.phone.fixedTitle"), 11.0).color(INK),
                                    );
                                    if !state.phone_modal.tunnel_domain.is_empty() {
                                        ui.label(
                                            RichText::new(i18n.tr("topbar.phone.fixedOn"))
                                                .size(10.0)
                                                .color(GREEN),
                                        );
                                        if ui.small_button(i18n.tr("topbar.phone.fixedDrop")).clicked() {
                                            state.phone_modal.tunnel_domain.clear();
                                            state.push_action(UiAction::SetTunnelDomain(String::new()));
                                        }
                                    } else {
                                        ui.label(
                                            RichText::new(i18n.tr("topbar.phone.fixedHint"))
                                                .size(10.0)
                                                .color(MUTED),
                                        );
                                        ui.horizontal(|ui| {
                                            ui.add(
                                                egui::TextEdit::singleline(
                                                    &mut state.phone_modal.tunnel_domain_draft,
                                                )
                                                .hint_text(i18n.tr("topbar.phone.fixedPlaceholder"))
                                                .desired_width(230.0),
                                            );
                                            if ui.button(i18n.tr("topbar.phone.fixedUse")).clicked() {
                                                let host = state
                                                    .phone_modal
                                                    .tunnel_domain_draft
                                                    .trim()
                                                    .to_owned();
                                                if !host.is_empty() {
                                                    state.push_action(UiAction::SetTunnelDomain(host));
                                                }
                                            }
                                        });
                                    }
                                }
                            }
                        }

                        // Real ISO-18004 QR image + URL + Copy button
                        let show_qr = match state.phone_modal.tab {
                            PhoneTab::Home => true,
                            PhoneTab::Away => active_tunnel_url.is_some(),
                        };
                        if show_qr {
                            ui.add_space(8.0);
                            ui.horizontal(|ui| {
                                if let Some(tex) = get_or_build_phone_qr_texture(ctx, &qr_url) {
                                    egui::Frame::NONE
                                        .fill(Color32::WHITE)
                                        .corner_radius(CornerRadius::same(8))
                                        .inner_margin(Margin::same(4))
                                        .show(ui, |ui| {
                                            ui.image((tex.id(), vec2(148.0, 148.0)));
                                        });
                                }
                                ui.add_space(8.0);
                                ui.vertical(|ui| {
                                    let lead_key = if state.phone_modal.tab == PhoneTab::Away {
                                        "topbar.phone.tunOn"
                                    } else {
                                        "topbar.phone.scan"
                                    };
                                    ui.label(RichText::new(i18n.tr(lead_key)).size(11.0).color(MUTED));
                                    ui.add_space(4.0);
                                    let shown_url = if state.streamer_mode {
                                        i18n.tr("station.streamerRedacted")
                                    } else {
                                        qr_url.clone()
                                    };
                                    ui.label(
                                        RichText::new(&shown_url)
                                            .monospace()
                                            .size(11.0)
                                            .color(BLUE),
                                    );
                                    ui.add_space(4.0);
                                    let copy_btn = egui::Button::new(
                                        heading_bold(i18n.tr("topbar.phone.copy"), 11.0).color(INK),
                                    )
                                    .fill(CARD_2)
                                    .stroke(Stroke::new(1.0_f32, LINE_2))
                                    .corner_radius(CornerRadius::same(7));
                                    if ui.add(copy_btn).clicked() {
                                        state.push_action(UiAction::CopyToClipboard {
                                            text: qr_url.clone(),
                                            toast_title: i18n.tr("ui.phoneModal.title"),
                                            toast_body: i18n.tr("topbar.phone.copied"),
                                        });
                                    }
                                });
                            });
                        }
                    });

                // PIN section: view, edit custom 6-10 digits, or 2-click regenerate
                ui.add_space(8.0);
                egui::Frame::NONE
                    .fill(CARD)
                    .corner_radius(CornerRadius::same(10))
                    .inner_margin(Margin::symmetric(11, 9))
                    .stroke(Stroke::new(1.0_f32, LINE))
                    .show(ui, |ui| {
                        ui.set_width(ui.available_width());
                        ui.horizontal(|ui| {
                            ui.label(heading_bold(i18n.tr("topbar.phone.pinTitle"), 11.5).color(MUTED));
                            let pin_disp = if state.streamer_mode {
                                "••• •••".to_owned()
                            } else {
                                format_pin_digits(&state.pairing_pin)
                            };
                            ui.label(heading_bold(pin_disp, 20.0).color(ACCENT_HI));
                            ui.with_layout(egui::Layout::right_to_left(Align::Center), |ui| {
                                if !state.phone_modal.editing_pin {
                                    if ui.small_button(i18n.tr("topbar.phone.setPin")).clicked() {
                                        state.phone_modal.editing_pin = true;
                                        state.phone_modal.pin_draft.clear();
                                    }
                                    let regen_lbl = if state.phone_modal.pin_regen_armed {
                                        i18n.tr("topbar.phone.newPinSure")
                                    } else {
                                        i18n.tr("topbar.phone.newPin")
                                    };
                                    let regen_btn = egui::Button::new(
                                        RichText::new(regen_lbl).size(10.5).color(
                                            if state.phone_modal.pin_regen_armed {
                                                Color32::WHITE
                                            } else {
                                                INK
                                            },
                                        ),
                                    )
                                    .fill(if state.phone_modal.pin_regen_armed { RED } else { CARD_2 })
                                    .corner_radius(CornerRadius::same(6));
                                    if ui.add(regen_btn).clicked() {
                                        if state.phone_modal.pin_regen_armed {
                                            state.phone_modal.pin_regen_armed = false;
                                            state.push_action(UiAction::UpdatePin { pin: None });
                                        } else {
                                            state.phone_modal.pin_regen_armed = true;
                                        }
                                    }
                                }
                            });
                        });

                        if state.phone_modal.editing_pin {
                            ui.add_space(5.0);
                            ui.horizontal(|ui| {
                                ui.add(
                                    egui::TextEdit::singleline(&mut state.phone_modal.pin_draft)
                                        .hint_text(i18n.tr("topbar.phone.setPinHint"))
                                        .desired_width(140.0),
                                );
                                state
                                    .phone_modal
                                    .pin_draft
                                    .retain(|c| c.is_ascii_digit());
                                if state.phone_modal.pin_draft.len() > 10 {
                                    state.phone_modal.pin_draft.truncate(10);
                                }
                                if ui.button(i18n.tr("topbar.phone.save")).clicked() {
                                    let candidate = state.phone_modal.pin_draft.clone();
                                    if station_core::auth::pin_weak(&candidate) {
                                        state.push_toast(
                                            i18n.tr("topbar.phone.pinTitle"),
                                            i18n.tr("station.pinWeak"),
                                        );
                                    } else {
                                        state.phone_modal.editing_pin = false;
                                        state.phone_modal.pin_draft.clear();
                                        state.push_action(UiAction::UpdatePin {
                                            pin: Some(candidate),
                                        });
                                    }
                                }
                                if ui.button(i18n.tr("topbar.phone.cancel")).clicked() {
                                    state.phone_modal.editing_pin = false;
                                    state.phone_modal.pin_draft.clear();
                                }
                            });
                        }
                        ui.label(RichText::new(i18n.tr("topbar.phone.pinNote")).size(10.0).color(FAINT));
                    });

                // Permissions & Web Push test
                ui.add_space(8.0);
                egui::Frame::NONE
                    .fill(CARD)
                    .corner_radius(CornerRadius::same(10))
                    .inner_margin(Margin::symmetric(11, 9))
                    .stroke(Stroke::new(1.0_f32, LINE))
                    .show(ui, |ui| {
                        ui.set_width(ui.available_width());
                        ui.label(heading_bold(i18n.tr("topbar.phone.mayTitle"), 11.5).color(INK));
                        let mut approve = state.phone_modal.phone_approve;
                        if ui
                            .checkbox(&mut approve, i18n.tr("topbar.phone.approve"))
                            .changed()
                        {
                            state.phone_modal.phone_approve = approve;
                            state.push_action(UiAction::TogglePhoneApprove(approve));
                        }
                        ui.label(
                            RichText::new(i18n.tr("topbar.phone.approveShort"))
                                .size(10.0)
                                .color(MUTED),
                        );

                        ui.add_space(6.0);
                        ui.horizontal(|ui| {
                            let push_txt = if state.phone_modal.push_count > 0 {
                                i18n.tr_count(
                                    "topbar.phone.pushCount",
                                    state.phone_modal.push_count,
                                )
                            } else {
                                i18n.tr("topbar.phone.pushNone")
                            };
                            ui.label(RichText::new(push_txt).size(10.5).color(MUTED));
                            ui.with_layout(egui::Layout::right_to_left(Align::Center), |ui| {
                                let test_btn = egui::Button::new(
                                    heading_bold(i18n.tr("topbar.phone.notifyTest"), 10.5).color(INK),
                                )
                                .fill(CARD_2)
                                .stroke(Stroke::new(1.0_f32, LINE_2))
                                .corner_radius(CornerRadius::same(7));
                                if ui.add(test_btn).clicked() {
                                    state.push_action(UiAction::TestWebPush);
                                }
                            });
                        });
                    });
            });
        });

    if !open {
        state.active_drawer = ActiveDrawer::None;
        clear_phone_qr_cache(ctx);
    }
}

/// Fully localized `AddHarness` modal (zero hardcoded English).
pub fn draw_add_harness_modal(ctx: &egui::Context, state: &mut UiState, i18n: &I18nStore) {
    if !ctx.wants_keyboard_input() && ctx.input(|i| i.key_pressed(egui::Key::Escape)) {
        state.active_drawer = ActiveDrawer::None;
        return;
    }

    let mut open = true;
    egui::Window::new(heading_bold(i18n.tr("station.addAppTitle"), 14.0).color(INK))
        .id(egui::Id::new("station_add_harness_modal"))
        .open(&mut open)
        .anchor(egui::Align2::RIGHT_TOP, vec2(-14.0, 62.0))
        .default_width(370.0)
        .max_height(500.0)
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
            ui.label(RichText::new(i18n.tr("station.addAppDesc")).size(11.0).color(MUTED));
            ui.add_space(6.0);
            ui.label(RichText::new(i18n.tr("station.addAppId")).size(11.0).color(INK));
            ui.text_edit_singleline(&mut state.new_harness_id);
            ui.add_space(4.0);
            ui.label(RichText::new(i18n.tr("station.addAppLabel")).size(11.0).color(INK));
            ui.text_edit_singleline(&mut state.new_harness_label);
            ui.add_space(4.0);
            ui.label(RichText::new(i18n.tr("station.addAppPath")).size(11.0).color(INK));
            ui.text_edit_singleline(&mut state.new_harness_path);
            ui.add_space(8.0);

            let save_btn = egui::Button::new(
                heading_bold(i18n.tr("station.addAppSave"), 11.5)
                    .color(Color32::from_rgb(0x2A, 0x13, 0x07)),
            )
            .fill(ACCENT)
            .corner_radius(CornerRadius::same(8));
            if ui.add(save_btn).clicked()
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
                state.push_toast(i18n.tr("station.addAppAdded"), label);
                state.active_drawer = ActiveDrawer::None;
            }

            let custom_items: Vec<_> = state
                .harnesses
                .iter()
                .filter(|h| h.custom)
                .cloned()
                .collect();
            if !custom_items.is_empty() {
                ui.add_space(10.0);
                ui.label(heading_bold(i18n.tr("station.customApps"), 12.0).color(INK));
                ui.add_space(4.0);
                for (idx, item) in custom_items.iter().enumerate() {
                    ui.push_id(("custom_app", idx, &item.id), |ui| {
                        ui.horizontal(|ui| {
                            ui.label(RichText::new(&item.label).size(11.5).color(INK));
                            ui.with_layout(egui::Layout::right_to_left(Align::Center), |ui| {
                                if ui.small_button(i18n.tr("station.remove")).clicked() {
                                    state.push_action(UiAction::RemoveCustomHarness(item.id.clone()));
                                }
                            });
                        });
                    });
                }
            }
        });

    if !open {
        state.active_drawer = ActiveDrawer::None;
    }
}

/// Fully localized `Settings` drawer (100% `I18nStore`).
pub fn draw_settings_modal(ctx: &egui::Context, state: &mut UiState, i18n: &I18nStore) {
    if !ctx.wants_keyboard_input() && ctx.input(|i| i.key_pressed(egui::Key::Escape)) {
        state.active_drawer = ActiveDrawer::None;
        return;
    }

    let mut open = true;
    egui::Window::new(heading_bold(i18n.tr("ui.topBar.settings"), 14.0).color(INK))
        .id(egui::Id::new("station_settings_drawer"))
        .open(&mut open)
        .anchor(egui::Align2::RIGHT_TOP, vec2(-14.0, 62.0))
        .default_width(370.0)
        .max_height(540.0)
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
            egui::ScrollArea::vertical().max_height(480.0).show(ui, |ui| {
                // Language
                ui.label(heading_bold(i18n.tr("settings.language"), 11.5).color(INK));
                ui.horizontal(|ui| {
                    for (idx, (code, key)) in [
                        ("en", "station.langNames.en"),
                        ("vi", "station.langNames.vi"),
                        ("th", "station.langNames.th"),
                    ]
                    .into_iter()
                    .enumerate()
                    {
                        ui.push_id(("lang", idx), |ui| {
                            if ui.selectable_label(i18n.lang == code, i18n.tr(key)).clicked() {
                                state.push_action(UiAction::SetLang(code.to_owned()));
                            }
                        });
                    }
                });

                ui.add_space(8.0);
                ui.label(heading_bold(i18n.tr("ui.topBar.office"), 11.5).color(INK));
                ui.horizontal(|ui| {
                    for (idx, (key, lay)) in [
                        ("settings.layouts.cozy.name", OfficeLayout::Cozy),
                        ("settings.layouts.pro.name", OfficeLayout::Pro),
                        ("settings.layouts.hq.name", OfficeLayout::Hq),
                    ]
                    .into_iter()
                    .enumerate()
                    {
                        ui.push_id(("lay", idx), |ui| {
                            if ui.selectable_label(state.layout == lay, i18n.tr(key)).clicked() {
                                state.layout = lay;
                                state.push_action(UiAction::SwitchLayout(lay));
                            }
                        });
                    }
                });

                ui.add_space(6.0);
                ui.label(heading_bold(i18n.tr("ui.topBar.scenery"), 11.5).color(INK));
                ui.horizontal(|ui| {
                    for (idx, (key, bk)) in [
                        ("ui.topBar.scenes.city", BackdropKind::City),
                        ("ui.topBar.scenes.country", BackdropKind::Countryside),
                        ("ui.topBar.scenes.beach", BackdropKind::Beach),
                    ]
                    .into_iter()
                    .enumerate()
                    {
                        ui.push_id(("bk", idx), |ui| {
                            if ui.selectable_label(state.backdrop == bk, i18n.tr(key)).clicked() {
                                state.backdrop = bk;
                                state.push_action(UiAction::SwitchBackdrop(bk));
                            }
                        });
                    }
                });

                ui.add_space(6.0);
                ui.label(heading_bold(i18n.tr("ui.topBar.characters"), 11.5).color(INK));
                ui.horizontal(|ui| {
                    for (idx, (key, lk)) in [
                        ("ui.topBar.toy", ChibiLook::Toy),
                        ("ui.topBar.pixar", ChibiLook::Pixar),
                    ]
                    .into_iter()
                    .enumerate()
                    {
                        ui.push_id(("look", idx), |ui| {
                            if ui.selectable_label(state.chibi_look == lk, i18n.tr(key)).clicked() {
                                state.chibi_look = lk;
                                state.push_action(UiAction::SetChibiLook(lk));
                            }
                        });
                    }
                });

                ui.add_space(8.0);
                ui.label(heading_bold(i18n.tr("station.gpu"), 11.5).color(INK));
                ui.horizontal(|ui| {
                    for (idx, (pref, key)) in [
                        (GpuPrefUi::Auto, "station.gpuOpt.auto"),
                        (GpuPrefUi::Integrated, "station.gpuOpt.integrated"),
                        (GpuPrefUi::Discrete, "station.gpuOpt.discrete"),
                    ]
                    .into_iter()
                    .enumerate()
                    {
                        ui.push_id(("gpu", idx), |ui| {
                            if ui.selectable_label(state.gpu_pref == pref, i18n.tr(key)).clicked() {
                                state.gpu_pref = pref;
                                state.push_action(UiAction::SetGpuPref(pref));
                            }
                        });
                    }
                });

                ui.add_space(8.0);
                if ui.checkbox(&mut state.sound_on, i18n.tr("ui.topBar.toggles.sound")).changed() {
                    state.push_action(UiAction::SetSound(state.sound_on));
                }
                ui.checkbox(&mut state.streamer_mode, i18n.tr("ui.topBar.toggles.stream"));
                ui.checkbox(&mut state.auto_close_tray, i18n.tr("station.closeToTray"));
            });
        });

    if !open {
        state.active_drawer = ActiveDrawer::None;
    }
}

/// Fully localized `Timeline` drawer (100% `I18nStore`).
pub fn draw_timeline_modal(ctx: &egui::Context, state: &mut UiState, i18n: &I18nStore) {
    if !ctx.wants_keyboard_input() && ctx.input(|i| i.key_pressed(egui::Key::Escape)) {
        state.active_drawer = ActiveDrawer::None;
        return;
    }

    let mut open = true;
    egui::Window::new(heading_bold(i18n.tr("ui.timeline.title"), 14.0).color(INK))
        .id(egui::Id::new("station_timeline_drawer"))
        .open(&mut open)
        .anchor(egui::Align2::RIGHT_TOP, vec2(-14.0, 62.0))
        .default_width(390.0)
        .max_height(540.0)
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
            ui.label(RichText::new(i18n.tr("ui.timeline.intro")).size(10.5).color(MUTED));
            ui.add_space(6.0);
            let streamer = state.streamer_mode;
            let redacted = i18n.tr("station.streamerRedacted");
            egui::ScrollArea::vertical().max_height(460.0).show(ui, |ui| {
                if state.feed.is_empty() {
                    ui.label(RichText::new(i18n.tr("ui.activityPanel.empty")).size(11.5).color(MUTED));
                    return;
                }
                for (idx, ev) in state.feed.iter().enumerate() {
                    ui.push_id(("tl", idx), |ui| {
                        let col = match ev.level.as_str() {
                            "ok" => GREEN,
                            "warn" => AMBER,
                            "err" => RED,
                            _ => BLUE,
                        };
                        egui::Frame::NONE
                            .fill(CARD)
                            .corner_radius(CornerRadius::same(8))
                            .inner_margin(Margin::symmetric(9, 7))
                            .stroke(Stroke::new(1.0_f32, LINE))
                            .show(ui, |ui| {
                                ui.set_width(ui.available_width());
                                ui.horizontal(|ui| {
                                    status_dot(ui, col, 3.6);
                                    let ag = if streamer { redacted.as_str() } else { ev.agent.as_str() };
                                    ui.label(heading_bold(ag, 11.5).color(INK));
                                    ui.with_layout(egui::Layout::right_to_left(Align::Center), |ui| {
                                        ui.label(RichText::new(&ev.ts_label).size(10.0).color(MUTED));
                                    });
                                });
                                let msg = if streamer { redacted.as_str() } else { ev.message.as_str() };
                                ui.label(RichText::new(msg).size(11.0).color(INK));
                            });
                        ui.add_space(4.0);
                    });
                }
            });
        });

    if !open {
        state.active_drawer = ActiveDrawer::None;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    fn i18n(lang: &str) -> I18nStore {
        I18nStore::load_from_assets(
            &PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../assets"),
            lang,
        )
    }

    fn setup_ctx() -> egui::Context {
        let ctx = egui::Context::default();
        let mut defs = egui::FontDefinitions::default();
        let fallback = defs
            .families
            .get(&egui::FontFamily::Proportional)
            .cloned()
            .unwrap_or_default();
        defs.families
            .insert(egui::FontFamily::Name(crate::theme::FONT_BOLD.into()), fallback);
        ctx.set_fonts(defs);
        let _ = ctx.run(egui::RawInput::default(), |_| {});
        ctx
    }

    #[test]
    fn wrapped_modal_auto_fetches_summary_when_opened_unloaded() {
        let tr = i18n("en");
        let mut st = UiState::default();
        st.active_drawer = ActiveDrawer::Wrapped;
        assert!(!st.wrapped.loaded && !st.wrapped.loading);

        let ctx = setup_ctx();
        let _ = ctx.run(egui::RawInput::default(), |ctx| {
            draw_wrapped_modal(ctx, &mut st, &tr);
        });

        assert!(st.wrapped.loading);
        let actions = st.drain_actions();
        assert!(actions.iter().any(|a| matches!(a, UiAction::FetchWrapped)));
    }

    #[test]
    fn wrapped_modal_and_timelapse_overlay_render_cleanly_in_en_vi_th() {
        let mut st = UiState::with_demo_seed(
            station_3d::OfficeLayout::Pro,
            station_3d::BackdropKind::City,
            false,
        );
        st.active_drawer = ActiveDrawer::Wrapped;
        st.timelapse_recording = true;
        st.timelapse_progress = 0.45;
        st.timelapse_elapsed_sec = 7;

        for lang in ["en", "vi", "th"] {
            let tr = i18n(lang);
            let ctx = setup_ctx();
            let _ = ctx.run(egui::RawInput::default(), |ctx| {
                draw_wrapped_modal(ctx, &mut st, &tr);
                draw_timelapse_overlay(ctx, &mut st, &tr);
            });
        }
    }

    #[test]
    fn wrapped_highlights_resolves_localized_weekday_without_duplicating_iso_date() {
        let summary = WrappedSummaryUi {
            busiest_day: Some(("2026-11-26".to_owned(), 4, 12.5)), // 4 = Thu
            ..Default::default()
        };
        for (lang, expected_day) in [("en", "Thu"), ("vi", "Thứ Năm"), ("th", "พฤ.")] {
            let tr = i18n(lang);
            let lines = format_wrapped_highlights(&summary, false, &tr);
            assert_eq!(lines.len(), 1);
            assert!(
                lines[0].contains(expected_day),
                "expected localized weekday `{expected_day}` in `{}`",
                lines[0]
            );
            assert!(
                !lines[0].contains("2026-11-26"),
                "must not duplicate raw ISO date in `{}`",
                lines[0]
            );
        }
    }

    #[test]
    fn phone_url_selection_prefers_tunnel_then_private_lan_ipv4_over_localhost() {
        // 1. Active Cloudflare tunnel URL wins over LAN IP and localhost
        assert_eq!(
            select_phone_url(
                Some("https://office-demo.trycloudflare.com"),
                Some("192.168.1.42"),
                4310
            ),
            "https://office-demo.trycloudflare.com/phone"
        );
        // Trailing slash / already ending in /phone preserved cleanly
        assert_eq!(
            select_phone_url(Some("https://office.example.com/phone"), Some("10.0.0.5"), 4310),
            "https://office.example.com/phone"
        );

        // 2. Private RFC1918 LAN IPv4 (192.168.x.x, 10.x.x.x, 172.16-31.x.x) wins over 127.0.0.1
        assert_eq!(
            select_phone_url(None, Some("192.168.1.42"), 4310),
            "http://192.168.1.42:4310/phone"
        );
        assert_eq!(
            select_phone_url(Some(""), Some("10.12.0.9"), 4310),
            "http://10.12.0.9:4310/phone"
        );
        assert_eq!(
            select_phone_url(None, Some("172.20.10.4"), 4310),
            "http://172.20.10.4:4310/phone"
        );

        // 3. Reject non-private / loopback / invalid LAN strings and fall back to 127.0.0.1
        assert_eq!(
            select_phone_url(None, Some("8.8.8.8"), 4310),
            "http://127.0.0.1:4310/phone"
        );
        assert_eq!(
            select_phone_url(None, Some("127.0.0.1"), 4310),
            "http://127.0.0.1:4310/phone"
        );
        assert_eq!(
            select_phone_url(None, None, 4310),
            "http://127.0.0.1:4310/phone"
        );
    }

    #[test]
    fn all_task7_drawers_and_phone_qr_modal_render_cleanly_in_en_vi_th() {
        let mut st = UiState::with_demo_seed(
            station_3d::OfficeLayout::Pro,
            station_3d::BackdropKind::City,
            false,
        );
        for lang in ["en", "vi", "th"] {
            let tr = i18n(lang);
            let ctx = setup_ctx();
            for drawer in [
                ActiveDrawer::Burn,
                ActiveDrawer::Vault,
                ActiveDrawer::System,
                ActiveDrawer::Accounts,
                ActiveDrawer::PhoneQr,
                ActiveDrawer::AddHarness,
                ActiveDrawer::Settings,
                ActiveDrawer::Timeline,
            ] {
                st.active_drawer = drawer;
                let _ = ctx.run(egui::RawInput::default(), |ctx| {
                    crate::drawers::draw_active_drawer(ctx, &mut st, &tr);
                });
            }
            // Closing PhoneQr drops cached QR texture cleanly
            st.active_drawer = ActiveDrawer::None;
            let _ = ctx.run(egui::RawInput::default(), |ctx| {
                crate::drawers::draw_active_drawer(ctx, &mut st, &tr);
            });
            assert!(
                ctx.data(|d| d.get_temp::<CachedPhoneQr>(egui::Id::new(PHONE_QR_CACHE_ID)))
                    .is_none(),
                "QR texture cache must be dropped when PhoneModal closes"
            );
        }
    }
}
