use crate::i18n::I18nStore;
use crate::icons::{self, IconKind};
use crate::state::{
    ActiveDrawer, CacheWarmth, PendingPermCard, SessionCard, UiAction, UiState, normalize_status,
};
use crate::theme::{
    ACCENT, ACCENT_HI, AMBER, BLUE, CARD, CARD_2, CARD_3, FAINT, GREEN, INK, LINE, LINE_2, MUTED,
    RED, SIDE, heading_bold,
};
use bevy_egui::egui::{
    self, Align, Color32, CornerRadius, FontFamily, FontId, Key, Margin, Rect, RichText, Sense,
    Shape, Stroke, StrokeKind, pos2, vec2,
};

const AMBER_SOFT: Color32 = Color32::from_rgba_premultiplied(0x2E, 0x22, 0x0D, 34);
const GREEN_SOFT: Color32 = Color32::from_rgba_premultiplied(0x0B, 0x26, 0x17, 28);
const RED_SOFT: Color32 = Color32::from_rgba_premultiplied(0x31, 0x12, 0x12, 31);
const BADGE_BG: Color32 = Color32::from_rgba_premultiplied(0x18, 0x18, 0x18, 22);
const CODE_BG: Color32 = Color32::from_rgb(0x0A, 0x0E, 0x14);
const ON_GREEN: Color32 = Color32::from_rgb(0x07, 0x13, 0x0C);

#[derive(Debug, Default)]
pub struct GroupedSessions<'a> {
    pub wait: Vec<&'a SessionCard>,
    pub work: Vec<&'a SessionCard>,
    pub rest: Vec<&'a SessionCard>,
}

impl<'a> GroupedSessions<'a> {
    pub fn total(&self) -> usize {
        self.wait.len() + self.work.len() + self.rest.len()
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum LiteShortcutOutcome {
    None,
    FocusSearch,
    SelectSession(String),
    OpenSession(String),
    DecidePerm {
        id: String,
        decision: &'static str,
    },
}

/// Resolves a session's effective group (`"wait" | "work" | "rest"`), elevating any session
/// with an active pending permission to `"wait"`.
pub fn effective_status(s: &SessionCard, perms: &[PendingPermCard]) -> &'static str {
    let norm = normalize_status(&s.status);
    if norm == "wait" || perms.iter().any(|p| p.session_id == s.id) {
        "wait"
    } else {
        norm
    }
}

fn matches_query(s: &SessionCard, query: &str) -> bool {
    let q = query.trim().to_ascii_lowercase();
    if q.is_empty() {
        return true;
    }
    let branch = s.git_branch.as_deref().unwrap_or("");
    let hay = format!(
        "{} {} {} {} {} {}",
        s.project, s.agent, s.id, s.dir, branch, s.last_prompt
    )
    .to_ascii_lowercase();
    q.split_whitespace().all(|word| hay.contains(word))
}

/// Groups sessions into `wait`, `work`, and `rest` buckets after applying optional status tab
/// filter and multi-token search query (matching `lite.js`).
pub fn group_sessions<'a>(
    sessions: &'a [SessionCard],
    perms: &[PendingPermCard],
    filter: Option<&str>,
    query: &str,
) -> GroupedSessions<'a> {
    let mut out = GroupedSessions::default();
    for s in sessions {
        let st = effective_status(s, perms);
        if let Some(f) = filter
            && st != f
        {
            continue;
        }
        if !matches_query(s, query) {
            continue;
        }
        match st {
            "wait" => out.wait.push(s),
            "work" => out.work.push(s),
            _ => out.rest.push(s),
        }
    }
    out
}

/// Flattens grouped sessions into top-to-bottom keyboard navigation order (`wait -> work -> rest`).
pub fn ordered_session_ids(grouped: &GroupedSessions<'_>) -> Vec<String> {
    grouped
        .wait
        .iter()
        .chain(grouped.work.iter())
        .chain(grouped.rest.iter())
        .map(|s| s.id.clone())
        .collect()
}

/// Handles `Up` / `Down` / `Enter` / `Y` / `N` / `/` keyboard shortcuts for 2D Lite & Panel.
/// Strictly disabled whenever any text input/search box has focus (`ctx.wants_keyboard_input()`).
pub fn handle_lite_keyboard(
    ctx: &egui::Context,
    state: &mut UiState,
    ordered_ids: &[String],
    search_has_focus: bool,
) -> LiteShortcutOutcome {
    if search_has_focus || ctx.wants_keyboard_input() {
        return LiteShortcutOutcome::None;
    }
    let has_mod = ctx.input(|i| i.modifiers.ctrl || i.modifiers.alt || i.modifiers.command);
    if has_mod {
        return LiteShortcutOutcome::None;
    }

    if ctx.input(|i| i.key_pressed(Key::Slash)) {
        state.lite_focus_search_req = true;
        return LiteShortcutOutcome::FocusSearch;
    }

    if ctx.input(|i| i.key_pressed(Key::ArrowDown)) {
        if let Some(next_id) = step_selection(ordered_ids, state.selected_session.as_deref(), 1) {
            state.selected_session = Some(next_id.clone());
            return LiteShortcutOutcome::SelectSession(next_id);
        }
        return LiteShortcutOutcome::None;
    }

    if ctx.input(|i| i.key_pressed(Key::ArrowUp)) {
        if let Some(prev_id) = step_selection(ordered_ids, state.selected_session.as_deref(), -1) {
            state.selected_session = Some(prev_id.clone());
            return LiteShortcutOutcome::SelectSession(prev_id);
        }
        return LiteShortcutOutcome::None;
    }

    if ctx.input(|i| i.key_pressed(Key::Enter)) {
        let target = state
            .selected_session
            .as_ref()
            .filter(|id| ordered_ids.iter().any(|x| x == *id))
            .cloned()
            .or_else(|| ordered_ids.first().cloned());
        if let Some(id) = target {
            state.selected_session = Some(id.clone());
            state.active_drawer = ActiveDrawer::Session;
            return LiteShortcutOutcome::OpenSession(id);
        }
        return LiteShortcutOutcome::None;
    }

    let decide = if ctx.input(|i| i.key_pressed(Key::Y)) {
        Some("allow")
    } else if ctx.input(|i| i.key_pressed(Key::N)) {
        Some("deny")
    } else {
        None
    };

    if let Some(decision) = decide
        && let Some(perm_id) = pick_target_perm_id(state)
    {
        state.pending_perms.retain(|p| p.id != perm_id);
        state.push_action(UiAction::DecidePermission {
            id: perm_id.clone(),
            decision: decision.to_owned(),
        });
        return LiteShortcutOutcome::DecidePerm {
            id: perm_id,
            decision,
        };
    }

    LiteShortcutOutcome::None
}

fn step_selection(ordered_ids: &[String], current: Option<&str>, delta: isize) -> Option<String> {
    if ordered_ids.is_empty() {
        return None;
    }
    let idx = current.and_then(|cur| ordered_ids.iter().position(|id| id == cur));
    let next = match (idx, delta > 0) {
        (None, _) => 0,
        (Some(i), true) => (i + 1).min(ordered_ids.len() - 1),
        (Some(i), false) => i.saturating_sub(1),
    };
    ordered_ids.get(next).cloned()
}

fn pick_target_perm_id(state: &UiState) -> Option<String> {
    if let Some(sel) = state.selected_session.as_deref()
        && let Some(p) = state.pending_perms.iter().find(|p| p.session_id == sel)
    {
        return Some(p.id.clone());
    }
    state.pending_perms.first().map(|p| p.id.clone())
}

fn agent_accent(agent: &str) -> Color32 {
    match agent.to_ascii_lowercase().as_str() {
        "claude" => ACCENT,
        "codex" => BLUE,
        "antigravity" | "gemini" => GREEN,
        "opencode" => Color32::from_rgb(0xB7, 0x94, 0xF4),
        "zcode" => AMBER,
        _ => Color32::from_rgb(0x7C, 0xB4, 0xFF),
    }
}

fn tool_icon(tool_kind: Option<&str>) -> IconKind {
    match tool_kind.unwrap_or("") {
        "edit" | "write" | "patch" => IconKind::Pencil,
        "read" | "grep" | "glob" => IconKind::File,
        "web" | "fetch" | "browse" => IconKind::Globe,
        "agent" | "subagent" => IconKind::Users,
        "bash" | "cmd" | "shell" => IconKind::Terminal,
        _ => IconKind::Sparkle,
    }
}

fn strip_leading_coffee_emoji(raw: &str) -> &str {
    raw.trim_start_matches('☕').trim_start()
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

/// Renders the 2D Lite board inside the central stage column (between `SessionRail` and
/// `ActivityPanel`), matching `my-station/src/office/ui/lite.js` and `my-station-latest-check.png`.
pub fn render_lite_stage_center(ui: &mut egui::Ui, state: &mut UiState, i18n: &I18nStore) {
    let sessions_snapshot = state.sessions.clone();
    let perms_snapshot = state.pending_perms.clone();

    // Unfiltered counts for the status tabs.
    let all_grouped = group_sessions(&sessions_snapshot, &perms_snapshot, None, "");
    let counts = (
        sessions_snapshot.len(),
        all_grouped.wait.len(),
        all_grouped.work.len(),
        all_grouped.rest.len(),
    );

    // Filtered & grouped lists for display + keyboard navigation.
    let filter_ref = state.lite_filter.clone();
    let search_ref = state.lite_search.clone();
    let grouped = group_sessions(
        &sessions_snapshot,
        &perms_snapshot,
        filter_ref.as_deref(),
        &search_ref,
    );
    let ordered_ids = ordered_session_ids(&grouped);

    // Sticky top header: title + filter tabs + search input (`/` focuses, `Esc` clears/blurs).
    let mut search_focused = false;
    ui.horizontal(|ui| {
        ui.label(heading_bold(i18n.tr("lite.title"), 15.0));
        ui.add_space(6.0);
        draw_filter_tabs(ui, state, i18n, counts);
    });
    ui.add_space(6.0);

    // Search input row
    egui::Frame::NONE
        .fill(SIDE)
        .corner_radius(CornerRadius::same(9))
        .inner_margin(Margin::symmetric(10, 6))
        .stroke(Stroke::new(1.0_f32, LINE_2))
        .show(ui, |ui| {
            ui.horizontal(|ui| {
                ui.spacing_mut().item_spacing.x = 7.0;
                let (ic_r, _) = ui.allocate_exact_size(vec2(14.0, 14.0), Sense::hover());
                icons::paint_icon(ui.painter(), ic_r, IconKind::Search, FAINT);
                let resp = ui.add(
                    egui::TextEdit::singleline(&mut state.lite_search)
                        .id(egui::Id::new("lite_stage_search_input"))
                        .frame(false)
                        .desired_width(f32::INFINITY)
                        .font(FontId::new(12.0, FontFamily::Proportional))
                        .hint_text(RichText::new(i18n.tr("lite.search")).color(FAINT)),
                );
                if state.lite_focus_search_req {
                    state.lite_focus_search_req = false;
                    resp.request_focus();
                }
                if resp.has_focus() && ui.input(|i| i.key_pressed(Key::Escape)) {
                    state.lite_search.clear();
                    resp.surrender_focus();
                }
                search_focused = resp.has_focus() || resp.lost_focus();
            });
        });

    // Keyboard shortcuts strictly gated against active text input focus.
    handle_lite_keyboard(ui.ctx(), state, &ordered_ids, search_focused);

    ui.add_space(8.0);

    // Scrollable grouped cards
    egui::ScrollArea::vertical()
        .id_salt("station_lite_stage_scroll")
        .auto_shrink([false, false])
        .show(ui, |ui| {
            // Render any orphaned pending permissions at the top so no approval is ever hidden.
            let orphan_perms: Vec<&PendingPermCard> = perms_snapshot
                .iter()
                .filter(|p| !sessions_snapshot.iter().any(|s| s.id == p.session_id))
                .collect();
            for perm in orphan_perms {
                draw_orphan_perm_card(ui, perm, state, i18n);
                ui.add_space(8.0);
            }

            if grouped.total() == 0 {
                ui.add_space(20.0);
                let empty_msg = if sessions_snapshot.is_empty() {
                    i18n.tr("lite.empty")
                } else {
                    i18n.tr_vars("lite.none", &[("q", search_ref.as_str())])
                };
                ui.label(RichText::new(empty_msg).size(12.5).color(MUTED));
                return;
            }

            let sections: [(&str, &str, &[&SessionCard]); 3] = [
                ("wait", "lite.wait", &grouped.wait),
                ("work", "lite.work", &grouped.work),
                ("rest", "lite.rest", &grouped.rest),
            ];

            for (sec_key, title_key, rows) in sections {
                if rows.is_empty() {
                    continue;
                }
                let heading = format!("{} · {}", i18n.tr(title_key).to_uppercase(), rows.len());
                let heading_col = if sec_key == "wait" { AMBER } else { FAINT };
                ui.label(heading_bold(heading, 10.5).color(heading_col));
                ui.add_space(5.0);

                for sess in rows {
                    let matching_perm = perms_snapshot.iter().find(|p| p.session_id == sess.id);
                    draw_lite_session_card(ui, sess, sec_key, matching_perm, state, i18n);
                    ui.add_space(7.0);
                }
                ui.add_space(6.0);
            }
        });
}

fn draw_filter_tabs(
    ui: &mut egui::Ui,
    state: &mut UiState,
    i18n: &I18nStore,
    counts: (usize, usize, usize, usize),
) {
    let (all_n, wait_n, work_n, rest_n) = counts;
    egui::Frame::NONE
        .fill(SIDE)
        .corner_radius(CornerRadius::same(9))
        .inner_margin(Margin::same(2))
        .stroke(Stroke::new(1.0_f32, LINE_2))
        .show(ui, |ui| {
            ui.horizontal(|ui| {
                ui.spacing_mut().item_spacing.x = 2.0;
                for (opt, key, n, highlight_wait) in [
                    (None, "lite.all", all_n, false),
                    (Some("wait"), "lite.wait", wait_n, wait_n > 0),
                    (Some("work"), "lite.work", work_n, false),
                    (Some("rest"), "lite.rest", rest_n, false),
                ] {
                    let active = state.lite_filter.as_deref() == opt;
                    let fill = if active { CARD_3 } else { Color32::TRANSPARENT };
                    let text_col = if active { INK } else { MUTED };
                    let count_col = if highlight_wait { AMBER } else { FAINT };

                    let resp = egui::Frame::NONE
                        .fill(fill)
                        .corner_radius(CornerRadius::same(7))
                        .inner_margin(Margin::symmetric(8, 4))
                        .show(ui, |ui| {
                            ui.horizontal(|ui| {
                                ui.spacing_mut().item_spacing.x = 5.0;
                                ui.label(heading_bold(i18n.tr(key), 11.0).color(text_col));
                                ui.label(
                                    RichText::new(n.to_string())
                                        .font(FontId::new(10.5, FontFamily::Monospace))
                                        .color(count_col),
                                );
                            });
                        })
                        .response
                        .interact(Sense::click());
                    if resp.clicked() {
                        state.lite_filter = opt.map(str::to_owned);
                    }
                }
            });
        });
}

fn draw_orphan_perm_card(
    ui: &mut egui::Ui,
    perm: &PendingPermCard,
    state: &mut UiState,
    i18n: &I18nStore,
) {
    egui::Frame::NONE
        .fill(AMBER_SOFT)
        .corner_radius(CornerRadius::same(10))
        .inner_margin(Margin::symmetric(12, 9))
        .stroke(Stroke::new(
            1.0_f32,
            Color32::from_rgba_unmultiplied(0xF2, 0xB5, 0x44, 120),
        ))
        .show(ui, |ui| {
            ui.set_width(ui.available_width());
            ui.horizontal(|ui| {
                let (ic_r, _) = ui.allocate_exact_size(vec2(14.0, 14.0), Sense::hover());
                icons::paint_icon(ui.painter(), ic_r, IconKind::Alert, AMBER);
                ui.label(heading_bold(i18n.tr("station.approval"), 12.0).color(AMBER));
                ui.label(heading_bold(&perm.agent, 12.0).color(INK));
                ui.label(RichText::new(&perm.tool).size(11.0).color(MUTED));
            });
            if !perm.summary.is_empty() && !state.streamer_mode {
                ui.add_space(4.0);
                draw_code_snippet(ui, &perm.summary);
            }
            ui.add_space(6.0);
            draw_inline_perm_buttons(ui, perm, state, i18n);
        });
}

fn draw_lite_session_card(
    ui: &mut egui::Ui,
    sess: &SessionCard,
    group_status: &str,
    perm: Option<&PendingPermCard>,
    state: &mut UiState,
    i18n: &I18nStore,
) {
    let selected = state.selected_session.as_deref() == Some(sess.id.as_str());
    let is_wait = group_status == "wait";
    let is_work = group_status == "work";

    let stripe_col = if is_wait {
        AMBER
    } else if is_work {
        GREEN
    } else {
        agent_accent(&sess.agent)
    };
    let dot_col = if is_wait {
        AMBER
    } else if is_work {
        GREEN
    } else {
        FAINT
    };
    let border_col = if selected {
        ACCENT
    } else if is_wait {
        Color32::from_rgba_unmultiplied(0xF2, 0xB5, 0x44, 110)
    } else {
        LINE_2
    };

    let card = egui::Frame::NONE
        .fill(CARD)
        .corner_radius(CornerRadius::same(10))
        .inner_margin(Margin {
            left: 14,
            right: 12,
            top: 9,
            bottom: 9,
        })
        .stroke(Stroke::new(if selected { 1.4_f32 } else { 1.0_f32 }, border_col))
        .show(ui, |ui| {
            ui.set_width(ui.available_width());

            // Row 1: status dot + title + right-aligned metrics (zero overlap at 1280px width)
            ui.horizontal(|ui| {
                ui.spacing_mut().item_spacing.x = 6.0;
                let (dot_r, _) = ui.allocate_exact_size(vec2(10.0, 10.0), Sense::hover());
                if is_work {
                    ui.painter().circle_filled(dot_r.center(), 5.5, GREEN_SOFT);
                } else if is_wait {
                    ui.painter().circle_filled(dot_r.center(), 5.5, AMBER_SOFT);
                }
                ui.painter().circle_filled(dot_r.center(), 3.6, dot_col);

                ui.label(heading_bold(clip_chars(&sess.project, 24), 13.0));

                // Right-aligned metrics: cost, age, context bar, todo counter
                ui.with_layout(egui::Layout::right_to_left(Align::Center), |ui| {
                    ui.spacing_mut().item_spacing.x = 8.0;
                    if sess.cost_usd > 0.0 {
                        ui.label(
                            RichText::new(format!("${:.2}", sess.cost_usd))
                                .font(FontId::new(10.5, FontFamily::Monospace))
                                .color(MUTED),
                        );
                    }
                    if !sess.age_label.is_empty() {
                        ui.label(
                            RichText::new(&sess.age_label)
                                .font(FontId::new(10.5, FontFamily::Monospace))
                                .color(FAINT),
                        );
                    }
                    if sess.ctx_pct > 0.005 {
                        draw_ctx_bar(ui, sess.ctx_pct, i18n);
                    }
                    if sess.todos_total > 0 {
                        let done_s = sess.todos_done.to_string();
                        let total_s = sess.todos_total.to_string();
                        let tip = i18n.tr_vars(
                            "lite.todoTitle",
                            &[("done", done_s.as_str()), ("n", total_s.as_str())],
                        );
                        ui.label(
                            RichText::new(format!("{}/{}", sess.todos_done, sess.todos_total))
                                .font(FontId::new(10.5, FontFamily::Monospace))
                                .color(if sess.todos_done == sess.todos_total {
                                    GREEN
                                } else {
                                    MUTED
                                }),
                        )
                        .on_hover_text(tip);
                    }
                });
            });

            // Row 1b: folder pill + git branch/dirty badge + cache-warmth indicator (clean `.v2-sub` line)
            ui.add_space(4.0);
            ui.horizontal_wrapped(|ui| {
                ui.spacing_mut().item_spacing = vec2(6.0, 4.0);
                let chip_text = if state.streamer_mode {
                    sess.agent.clone()
                } else if !sess.dir.is_empty() {
                    clip_chars(&sess.dir, 30)
                } else {
                    sess.agent.clone()
                };
                draw_meta_pill(ui, &chip_text, MUTED, BADGE_BG, LINE);

                if let Some(branch) = &sess.git_branch
                    && !branch.is_empty()
                {
                    let branch_label = if sess.git_dirty > 0 {
                        format!("{} •{}", clip_chars(branch, 20), sess.git_dirty)
                    } else {
                        clip_chars(branch, 20)
                    };
                    let b_col = if sess.git_dirty > 0 { AMBER } else { MUTED };
                    draw_meta_pill(ui, &branch_label, b_col, BADGE_BG, LINE);
                }

                draw_cache_warmth_badge(ui, sess.cache_warmth, i18n);
            });

            // Row 2: current doing / status line
            ui.add_space(4.0);
            ui.horizontal(|ui| {
                ui.spacing_mut().item_spacing.x = 6.0;
                let (ic, text, col) = resolve_now_line(sess, group_status, state.streamer_mode, i18n);
                let (ic_r, _) = ui.allocate_exact_size(vec2(13.0, 13.0), Sense::hover());
                icons::paint_icon(ui.painter(), ic_r, ic, col);
                ui.label(RichText::new(text).size(12.0).color(col));
            });

            // Optional prompt preview line (matches `my-station-latest-check.png`)
            if !state.streamer_mode && !sess.last_prompt.is_empty() {
                ui.add_space(2.0);
                ui.label(
                    RichText::new(format!("“{}”", clip_chars(&sess.last_prompt, 92)))
                        .size(11.0)
                        .color(FAINT),
                );
            }

            // Inline Approve / Deny directly on Waiting cards
            if is_wait && let Some(p) = perm {
                ui.add_space(6.0);
                if !p.summary.is_empty() && !state.streamer_mode {
                    draw_code_snippet(ui, &p.summary);
                    ui.add_space(5.0);
                }
                draw_inline_perm_buttons(ui, p, state, i18n);
            }

            // Nested subagent rows under active cards (up to 4, matching `lite.js:75-82`)
            if !sess.subagents.is_empty() && group_status != "rest" {
                ui.add_space(5.0);
                draw_nested_subagents(ui, sess, state.streamer_mode);
            }
        });

    // Crisp left accent stripe inside card left edge (matching `my-station-latest-check.png`).
    let r = card.response.rect;
    let stripe = Rect::from_min_max(pos2(r.left() + 1.0, r.top() + 3.0), pos2(r.left() + 4.0, r.bottom() - 3.0));
    ui.painter()
        .rect_filled(stripe, CornerRadius::same(2), stripe_col);

    // Click on card selects session & opens SessionDrawer (only when no inline button consumed click).
    let click = card.response.interact(Sense::click());
    if click.hovered() && !selected {
        ui.painter().rect_stroke(
            r,
            CornerRadius::same(10),
            Stroke::new(1.0_f32, ACCENT_HI),
            StrokeKind::Inside,
        );
    }
    if click.clicked() {
        state.selected_session = Some(sess.id.clone());
        state.active_drawer = ActiveDrawer::Session;
    }
}

fn draw_meta_pill(ui: &mut egui::Ui, text: &str, color: Color32, fill: Color32, border: Color32) {
    egui::Frame::NONE
        .fill(fill)
        .corner_radius(CornerRadius::same(6))
        .inner_margin(Margin::symmetric(6, 1))
        .stroke(Stroke::new(1.0_f32, border))
        .show(ui, |ui| {
            ui.label(
                RichText::new(text)
                    .font(FontId::new(10.0, FontFamily::Monospace))
                    .color(color),
            );
        });
}

fn draw_cache_warmth_badge(ui: &mut egui::Ui, warmth: CacheWarmth, i18n: &I18nStore) {
    let (dot_col, label) = match warmth {
        CacheWarmth::Unknown => return,
        CacheWarmth::Warm(mins) => {
            let m = mins.max(1).to_string();
            let raw = i18n.tr_vars("lite.cacheWarm", &[("n", m.as_str())]);
            let dot = if mins >= 6 { GREEN } else { AMBER };
            (dot, strip_leading_coffee_emoji(&raw).to_owned())
        }
        CacheWarmth::Cold => {
            let raw = i18n.tr("lite.cacheCold");
            (FAINT, strip_leading_coffee_emoji(&raw).to_owned())
        }
    };

    egui::Frame::NONE
        .fill(BADGE_BG)
        .corner_radius(CornerRadius::same(6))
        .inner_margin(Margin::symmetric(6, 1))
        .stroke(Stroke::new(1.0_f32, LINE))
        .show(ui, |ui| {
            ui.horizontal(|ui| {
                ui.spacing_mut().item_spacing.x = 4.0;
                let (d, _) = ui.allocate_exact_size(vec2(7.0, 7.0), Sense::hover());
                ui.painter().circle_filled(d.center(), 3.0, dot_col);
                ui.label(RichText::new(label).size(10.0).color(MUTED));
            });
        });
}

fn draw_ctx_bar(ui: &mut egui::Ui, ctx_pct: f32, i18n: &I18nStore) {
    let pct = (ctx_pct.clamp(0.0, 1.0) * 100.0).round() as u32;
    let pct_s = pct.to_string();
    let tip = i18n.tr_vars("lite.ctx", &[("pct", pct_s.as_str())]);
    let bar_col = if pct >= 85 {
        RED
    } else if pct >= 60 {
        AMBER
    } else {
        BLUE
    };

    ui.horizontal(|ui| {
        ui.spacing_mut().item_spacing.x = 5.0;
        ui.label(
            RichText::new(format!("{pct}%"))
                .font(FontId::new(10.0, FontFamily::Monospace))
                .color(FAINT),
        )
        .on_hover_text(&tip);
        let (bar_r, bar_resp) = ui.allocate_exact_size(vec2(44.0, 4.0), Sense::hover());
        ui.painter()
            .rect_filled(bar_r, CornerRadius::same(2), CARD_3);
        let fill_w = (bar_r.width() * ctx_pct.clamp(0.0, 1.0)).max(2.0);
        ui.painter().rect_filled(
            Rect::from_min_size(bar_r.min, vec2(fill_w, 4.0)),
            CornerRadius::same(2),
            bar_col,
        );
        bar_resp.on_hover_text(tip);
    });
}

fn resolve_now_line(
    sess: &SessionCard,
    group_status: &str,
    streamer_mode: bool,
    i18n: &I18nStore,
) -> (IconKind, String, Color32) {
    if streamer_mode {
        return (IconKind::Sparkle, i18n.tr("station.streamerRedacted"), MUTED);
    }
    if group_status == "wait" {
        let text = match &sess.tool {
            Some(what) if !what.is_empty() => {
                let clipped = clip_chars(what, 64);
                i18n.tr_vars("lite.now.waitOn", &[("what", clipped.as_str())])
            }
            _ => i18n.tr("lite.now.wait"),
        };
        return (IconKind::Alert, text, AMBER);
    }
    if group_status == "rest" {
        let text = if !sess.last_reply.is_empty() {
            clip_chars(&sess.last_reply, 110)
        } else if !sess.age_label.is_empty() {
            i18n.tr_vars("activity.doing.resting", &[("time", sess.age_label.as_str())])
        } else {
            i18n.tr("lite.now.done")
        };
        return (IconKind::Check, text, MUTED);
    }
    match &sess.tool {
        Some(tool) if !tool.is_empty() => (
            tool_icon(sess.tool_kind.as_deref()),
            clip_chars(tool, 88),
            INK,
        ),
        _ => (IconKind::Sparkle, i18n.tr("activity.doing.working"), INK),
    }
}

fn draw_code_snippet(ui: &mut egui::Ui, text: &str) {
    egui::Frame::NONE
        .fill(CODE_BG)
        .corner_radius(CornerRadius::same(7))
        .inner_margin(Margin::symmetric(8, 5))
        .stroke(Stroke::new(1.0_f32, LINE))
        .show(ui, |ui| {
            ui.set_width(ui.available_width());
            ui.label(
                RichText::new(clip_chars(text, 140))
                    .font(FontId::new(10.5, FontFamily::Monospace))
                    .color(INK),
            );
        });
}

fn draw_inline_perm_buttons(
    ui: &mut egui::Ui,
    perm: &PendingPermCard,
    state: &mut UiState,
    i18n: &I18nStore,
) {
    ui.horizontal(|ui| {
        ui.spacing_mut().item_spacing.x = 7.0;
        let allow_btn = egui::Button::new(
            heading_bold(format!("{} (Y)", i18n.tr("perm.allow")), 11.5).color(ON_GREEN),
        )
        .fill(GREEN)
        .stroke(Stroke::NONE)
        .corner_radius(CornerRadius::same(7))
        .min_size(vec2(84.0, 26.0));
        if ui.add(allow_btn).clicked() {
            let id = perm.id.clone();
            state.pending_perms.retain(|x| x.id != id);
            state.push_action(UiAction::DecidePermission {
                id,
                decision: "allow".to_owned(),
            });
        }

        let deny_btn = egui::Button::new(
            heading_bold(format!("{} (N)", i18n.tr("perm.deny")), 11.5).color(RED),
        )
        .fill(RED_SOFT)
        .stroke(Stroke::new(
            1.0_f32,
            Color32::from_rgba_unmultiplied(0xFF, 0x5F, 0x5F, 110),
        ))
        .corner_radius(CornerRadius::same(7))
        .min_size(vec2(76.0, 26.0));
        if ui.add(deny_btn).clicked() {
            let id = perm.id.clone();
            state.pending_perms.retain(|x| x.id != id);
            state.push_action(UiAction::DecidePermission {
                id,
                decision: "deny".to_owned(),
            });
        }
    });
}

fn draw_nested_subagents(ui: &mut egui::Ui, sess: &SessionCard, streamer_mode: bool) {
    let start_y = ui.cursor().top();
    let rail_x = ui.min_rect().left() + 8.0;
    egui::Frame::NONE
        .inner_margin(Margin {
            left: 16,
            right: 4,
            top: 2,
            bottom: 2,
        })
        .show(ui, |ui| {
            for sub in sess.subagents.iter().take(4) {
                egui::Frame::NONE
                    .fill(CARD_2)
                    .corner_radius(CornerRadius::same(7))
                    .inner_margin(Margin::symmetric(8, 4))
                    .show(ui, |ui| {
                        ui.set_width(ui.available_width());
                        ui.horizontal(|ui| {
                            ui.spacing_mut().item_spacing.x = 6.0;
                            let (ic_r, _) =
                                ui.allocate_exact_size(vec2(12.0, 12.0), Sense::hover());
                            icons::paint_icon(ui.painter(), ic_r, IconKind::Users, BLUE);
                            ui.label(heading_bold(clip_chars(&sub.label, 24), 11.0));
                            if !streamer_mode && !sub.doing.is_empty() {
                                ui.label(
                                    RichText::new(clip_chars(&sub.doing, 56))
                                        .size(10.5)
                                        .color(MUTED),
                                );
                            }
                        });
                    });
                ui.add_space(3.0);
            }
        });
    let end_y = (ui.cursor().top() - 4.0).max(start_y + 4.0);
    let dashed = Shape::dashed_line(
        &[pos2(rail_x, start_y + 2.0), pos2(rail_x, end_y)],
        Stroke::new(1.0_f32, LINE_2),
        3.0,
        3.0,
    );
    ui.painter().extend(dashed);
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::state::{CacheWarmth, PendingPermCard, SubagentItem, ViewMode};
    use bevy_egui::egui::{Key, Modifiers, Pos2, Rect, pos2, vec2};
    use std::path::PathBuf;

    fn sample_sessions() -> Vec<SessionCard> {
        vec![
            SessionCard {
                id: "s-work".to_owned(),
                agent: "claude".to_owned(),
                project: "agents-station".to_owned(),
                dir: "D:\\mystation\\agents-station".to_owned(),
                git_branch: Some("feat/lite-board".to_owned()),
                git_dirty: 3,
                cache_warmth: CacheWarmth::Warm(18),
                todos_done: 3,
                todos_total: 5,
                status: "work".to_owned(),
                model: "claude-opus-4-6".to_owned(),
                last_prompt: "Implement 2D Lite board".to_owned(),
                tool: Some("Edit lite_view.rs".to_owned()),
                tool_kind: Some("edit".to_owned()),
                ctx_pct: 0.42,
                cost_usd: 1.42,
                age_label: "2m".to_owned(),
                subagents: vec![SubagentItem {
                    id: "sub-1".to_owned(),
                    label: "shell-inspector".to_owned(),
                    doing: "cargo test -p station-ui".to_owned(),
                }],
                ..Default::default()
            },
            SessionCard {
                id: "s-wait".to_owned(),
                agent: "codex".to_owned(),
                project: "station-core".to_owned(),
                dir: "D:\\mystation\\station-core".to_owned(),
                git_branch: Some("main".to_owned()),
                git_dirty: 0,
                cache_warmth: CacheWarmth::Warm(4),
                todos_done: 1,
                todos_total: 2,
                status: "wait".to_owned(),
                model: "gpt-5.3-codex".to_owned(),
                last_prompt: "Run OTLP tests".to_owned(),
                tool: Some("cargo test -p station-core".to_owned()),
                tool_kind: Some("bash".to_owned()),
                ctx_pct: 0.66,
                cost_usd: 0.68,
                age_label: "1m".to_owned(),
                ..Default::default()
            },
            SessionCard {
                id: "s-rest".to_owned(),
                agent: "opencode".to_owned(),
                project: "station-launcher".to_owned(),
                dir: "D:\\mystation\\launcher".to_owned(),
                git_branch: Some("win32-tray".to_owned()),
                git_dirty: 1,
                cache_warmth: CacheWarmth::Cold,
                status: "rest".to_owned(),
                last_prompt: "Win32 GUI detection".to_owned(),
                last_reply: "Finished Win32 focus check".to_owned(),
                ctx_pct: 0.14,
                cost_usd: 0.19,
                age_label: "9m".to_owned(),
                ..Default::default()
            },
        ]
    }

    fn key_event(key: Key) -> egui::Event {
        egui::Event::Key {
            key,
            physical_key: None,
            pressed: true,
            repeat: false,
            modifiers: Modifiers::NONE,
        }
    }

    #[test]
    fn groups_sessions_by_wait_work_rest_and_elevates_pending_perm_sessions() {
        let sessions = sample_sessions();
        let perms = vec![PendingPermCard {
            id: "p-elevate".to_owned(),
            session_id: "s-work".to_owned(),
            agent: "claude".to_owned(),
            tool: "Bash".to_owned(),
            summary: "cargo test".to_owned(),
            kind: "permission".to_owned(),
        }];

        let grouped = group_sessions(&sessions, &perms, None, "");
        assert_eq!(
            grouped.wait.len(),
            2,
            "s-wait + s-work (with perm) must be in wait"
        );
        assert_eq!(grouped.work.len(), 0);
        assert_eq!(grouped.rest.len(), 1);

        let no_perms = group_sessions(&sessions, &[], None, "");
        assert_eq!(no_perms.wait.len(), 1);
        assert_eq!(no_perms.work.len(), 1);
        assert_eq!(no_perms.rest.len(), 1);
        assert_eq!(
            ordered_session_ids(&no_perms),
            vec!["s-wait".to_owned(), "s-work".to_owned(), "s-rest".to_owned()]
        );
    }

    #[test]
    fn filters_sessions_by_status_tab_and_multi_word_search_across_branch_and_prompt() {
        let sessions = sample_sessions();
        let only_work = group_sessions(&sessions, &[], Some("work"), "");
        assert!(only_work.wait.is_empty());
        assert_eq!(only_work.work.len(), 1);
        assert!(only_work.rest.is_empty());

        let by_branch = group_sessions(&sessions, &[], None, "feat/lite board");
        assert_eq!(by_branch.work.len(), 1);
        assert_eq!(by_branch.work[0].id, "s-work");
        assert!(by_branch.wait.is_empty());
        assert!(by_branch.rest.is_empty());
    }

    #[test]
    fn keyboard_shortcuts_focus_search_navigate_open_and_decide_perms_when_unfocused() {
        let ctx = egui::Context::default();
        let mut state = UiState::with_demo_seed(
            station_3d::OfficeLayout::Pro,
            station_3d::BackdropKind::City,
            false,
        );
        state.selected_session = None;
        state.active_drawer = ActiveDrawer::None;
        let ids = vec![
            "sess-codex-2".to_owned(),
            "sess-claude-1".to_owned(),
            "sess-oc-4".to_owned(),
        ];

        // 1. `/` focuses search
        let mut out = LiteShortcutOutcome::None;
        let raw_slash = egui::RawInput {
            events: vec![key_event(Key::Slash)],
            ..Default::default()
        };
        let _ = ctx.run(raw_slash, |ctx| {
            out = handle_lite_keyboard(ctx, &mut state, &ids, false);
        });
        assert_eq!(out, LiteShortcutOutcome::FocusSearch);
        assert!(state.lite_focus_search_req);

        // 2. ArrowDown selects first, then second; ArrowUp steps back
        let raw_down = egui::RawInput {
            events: vec![key_event(Key::ArrowDown)],
            ..Default::default()
        };
        let _ = ctx.run(raw_down.clone(), |ctx| {
            out = handle_lite_keyboard(ctx, &mut state, &ids, false);
        });
        assert_eq!(
            out,
            LiteShortcutOutcome::SelectSession("sess-codex-2".to_owned())
        );
        assert_eq!(state.selected_session.as_deref(), Some("sess-codex-2"));

        let _ = ctx.run(raw_down, |ctx| {
            out = handle_lite_keyboard(ctx, &mut state, &ids, false);
        });
        assert_eq!(state.selected_session.as_deref(), Some("sess-claude-1"));

        let raw_up = egui::RawInput {
            events: vec![key_event(Key::ArrowUp)],
            ..Default::default()
        };
        let _ = ctx.run(raw_up, |ctx| {
            out = handle_lite_keyboard(ctx, &mut state, &ids, false);
        });
        assert_eq!(state.selected_session.as_deref(), Some("sess-codex-2"));

        // 3. Enter opens ActiveDrawer::Session
        let raw_enter = egui::RawInput {
            events: vec![key_event(Key::Enter)],
            ..Default::default()
        };
        let _ = ctx.run(raw_enter, |ctx| {
            out = handle_lite_keyboard(ctx, &mut state, &ids, false);
        });
        assert_eq!(
            out,
            LiteShortcutOutcome::OpenSession("sess-codex-2".to_owned())
        );
        assert_eq!(state.active_drawer, ActiveDrawer::Session);

        // 4. Y approves pending perm and emits DecidePermission("allow")
        let raw_y = egui::RawInput {
            events: vec![key_event(Key::Y)],
            ..Default::default()
        };
        let _ = ctx.run(raw_y, |ctx| {
            out = handle_lite_keyboard(ctx, &mut state, &ids, false);
        });
        assert_eq!(
            out,
            LiteShortcutOutcome::DecidePerm {
                id: "perm-1".to_owned(),
                decision: "allow",
            }
        );
        assert!(state.pending_perms.is_empty());
        let actions = state.drain_actions();
        assert!(matches!(
            actions.as_slice(),
            [UiAction::DecidePermission { id, decision }] if id == "perm-1" && decision == "allow"
        ));

        // 5. N denies pending perm
        state.pending_perms.push(PendingPermCard {
            id: "perm-2".to_owned(),
            session_id: "sess-codex-2".to_owned(),
            agent: "codex".to_owned(),
            tool: "Bash".to_owned(),
            summary: "rm -rf tmp".to_owned(),
            kind: "permission".to_owned(),
        });
        let raw_n = egui::RawInput {
            events: vec![key_event(Key::N)],
            ..Default::default()
        };
        let _ = ctx.run(raw_n, |ctx| {
            out = handle_lite_keyboard(ctx, &mut state, &ids, false);
        });
        assert_eq!(
            out,
            LiteShortcutOutcome::DecidePerm {
                id: "perm-2".to_owned(),
                decision: "deny",
            }
        );
    }

    #[test]
    fn keyboard_shortcuts_strictly_blocked_when_search_or_text_input_has_focus() {
        let ctx = egui::Context::default();
        let mut state = UiState::with_demo_seed(
            station_3d::OfficeLayout::Pro,
            station_3d::BackdropKind::City,
            false,
        );
        state.selected_session = Some("sess-claude-1".to_owned());
        state.active_drawer = ActiveDrawer::None;
        let ids = vec!["sess-codex-2".to_owned(), "sess-claude-1".to_owned()];

        // Focus a real TextEdit inside egui so `ctx.wants_keyboard_input()` becomes true.
        let mut text_rect = Rect::NOTHING;
        let raw0 = egui::RawInput {
            screen_rect: Some(Rect::from_min_size(pos2(0.0, 0.0), vec2(400.0, 200.0))),
            ..Default::default()
        };
        let _ = ctx.run(raw0, |ctx| {
            egui::CentralPanel::default().show(ctx, |ui| {
                let r = ui.text_edit_singleline(&mut state.lite_search);
                r.request_focus();
                text_rect = r.rect;
            });
        });

        for key in [
            Key::Y,
            Key::N,
            Key::ArrowDown,
            Key::ArrowUp,
            Key::Slash,
            Key::Enter,
        ] {
            // Re-establish focus before each key (Enter surrenders singleline focus after handling).
            let _ = ctx.run(
                egui::RawInput {
                    screen_rect: Some(Rect::from_min_size(pos2(0.0, 0.0), vec2(400.0, 200.0))),
                    ..Default::default()
                },
                |ctx| {
                    egui::CentralPanel::default().show(ctx, |ui| {
                        let r = ui.text_edit_singleline(&mut state.lite_search);
                        r.request_focus();
                    });
                },
            );

            let mut out = LiteShortcutOutcome::FocusSearch;
            let mut input_active = false;
            let raw = egui::RawInput {
                screen_rect: Some(Rect::from_min_size(pos2(0.0, 0.0), vec2(400.0, 200.0))),
                events: vec![key_event(key)],
                ..Default::default()
            };
            let _ = ctx.run(raw, |ctx| {
                egui::CentralPanel::default().show(ctx, |ui| {
                    let r = ui.text_edit_singleline(&mut state.lite_search);
                    input_active = r.has_focus() || r.lost_focus() || ctx.wants_keyboard_input();
                    out = handle_lite_keyboard(ctx, &mut state, &ids, r.has_focus() || r.lost_focus());
                });
            });
            assert!(
                input_active,
                "TextEdit must be active (has_focus/lost_focus/wants_keyboard_input) for {key:?}"
            );
            assert_eq!(
                out,
                LiteShortcutOutcome::None,
                "key {key:?} MUST be blocked when search input is focused"
            );
        }
        assert_eq!(
            state.pending_perms.len(),
            1,
            "Y/N must NOT resolve perm while typing"
        );
        assert_eq!(state.selected_session.as_deref(), Some("sess-claude-1"));
        assert_eq!(state.active_drawer, ActiveDrawer::None);
        assert!(state.drain_actions().is_empty());
        assert_ne!(text_rect, Rect::NOTHING);
        let _ = Pos2::ZERO;
    }

    #[test]
    fn lite_2d_renders_inside_center_stage_while_keeping_sidebars_visible() {
        let assets = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../assets");
        let bundle = crate::fonts::load_font_bundle(&assets).expect("load_font_bundle");
        let i18n = I18nStore::load_from_assets(&assets, "vi");
        let mut state = UiState::with_demo_seed(
            station_3d::OfficeLayout::Pro,
            station_3d::BackdropKind::City,
            false,
        );
        state.view_mode = ViewMode::Lite2D;

        let ctx = egui::Context::default();
        crate::install_egui_fonts(&ctx, &bundle);
        let raw = egui::RawInput {
            screen_rect: Some(Rect::from_min_size(pos2(0.0, 0.0), vec2(1280.0, 720.0))),
            ..Default::default()
        };
        let _ = ctx.run(raw.clone(), |_| {});

        let mut center_rect = Rect::NOTHING;
        let _ = ctx.run(raw, |ctx| {
            center_rect = crate::shell::draw_shell(ctx, &mut state, &i18n);
        });
        assert!(
            (center_rect.left() - 300.0).abs() <= 1.0,
            "2D Lite MUST keep left SessionRail (300px at 1280w), got left={}",
            center_rect.left()
        );
        assert!(
            (center_rect.right() - 950.0).abs() <= 1.0,
            "2D Lite MUST keep right ActivityPanel (330px at 1280w), got right={}",
            center_rect.right()
        );
        assert!(
            (center_rect.top() - 56.0).abs() <= 1.0,
            "2D Lite MUST keep TopBar (56px), got top={}",
            center_rect.top()
        );
    }

    #[test]
    fn lite_view_i18n_keys_resolve_cleanly_in_en_vi_th() {
        let assets = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../assets");
        let keys = [
            "lite.title",
            "lite.all",
            "lite.wait",
            "lite.work",
            "lite.rest",
            "lite.search",
            "lite.empty",
            "lite.cacheCold",
            "lite.now.wait",
            "lite.now.done",
            "perm.allow",
            "perm.deny",
            "station.approval",
        ];
        for lang in ["en", "vi", "th"] {
            let i18n = I18nStore::load_from_assets(&assets, lang);
            for key in keys {
                let val = i18n.tr(key);
                assert!(!val.is_empty() && val != key, "lang={lang} raw key {key}");
            }
        }
    }
}
