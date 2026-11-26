use crate::drawers::markdown::render_markdown;
use crate::i18n::I18nStore;
use crate::state::{ActiveDrawer, ChatMessage, ChatReplyRefUi, UiAction, UiState};
use crate::theme::{
    ACCENT, AMBER, BG, BLUE, CARD, CARD_2, CARD_3, FAINT, GREEN, INK, LINE, LINE_2, MUTED, RED,
    SIDE, heading_bold,
};
use bevy_egui::egui::{
    self, Color32, CornerRadius, FontId, Margin, RichText, Stroke, pos2, vec2,
};

const DRAWER_WIDTH: f32 = 420.0;
const TOPBAR_HEIGHT: f32 = 56.0;

pub const REACTIONS: [&str; 6] = ["❤️", "👍", "😆", "😮", "😭", "😡"];
pub const STICKER_EMOJI: [&str; 8] = ["👌", "🙏", "👀", "🔥", "💪", "🤭", "🎉", "🥺"];
const QUICK_EMOJIS: [&str; 24] = [
    "😀", "😂", "🥰", "😎", "🤩", "🥳", "😅", "🤔", "😴", "😮", "😭", "😡",
    "👍", "👏", "🙏", "💪", "👌", "👀", "❤️", "🔥", "✨", "🎉", "☕", "🚀",
];

const INITIAL_BG: [Color32; 8] = [
    Color32::from_rgb(0x4F, 0x8E, 0xF7),
    Color32::from_rgb(0xF2, 0x99, 0x4A),
    Color32::from_rgb(0x27, 0xAE, 0x60),
    Color32::from_rgb(0x9B, 0x51, 0xE0),
    Color32::from_rgb(0xEB, 0x57, 0x57),
    Color32::from_rgb(0x2D, 0x9C, 0xDB),
    Color32::from_rgb(0xF2, 0xC9, 0x4C),
    Color32::from_rgb(0x21, 0x96, 0x53),
];

const AGENT_TAGS: [&str; 5] = ["claude", "codex", "opencode", "antigravity", "zcode"];

fn is_agent_answer(tag: Option<&str>) -> bool {
    tag.is_some_and(|t| AGENT_TAGS.contains(&t))
}

fn avatar_color(from: &str, name: &str) -> Color32 {
    if from == "__claude" {
        return Color32::from_rgb(0xD9, 0x77, 0x57);
    }
    if from == "me" {
        return Color32::from_rgb(0x2B, 0x3A, 0x55);
    }
    let hash = name
        .bytes()
        .fold(0usize, |a, b| a.wrapping_mul(31).wrapping_add(b as usize));
    INITIAL_BG[hash % INITIAL_BG.len()]
}

fn initials(from: &str, name: &str) -> String {
    if from == "__claude" {
        return "✳".to_owned();
    }
    let words: Vec<&str> = name.split_whitespace().collect();
    let tail = if words.len() > 2 {
        &words[words.len() - 2..]
    } else {
        &words
    };
    let out: String = tail
        .iter()
        .filter_map(|w| w.chars().next())
        .collect::<String>()
        .to_uppercase();
    if out.is_empty() { "?".to_owned() } else { out }
}

fn paint_avatar_badge(ui: &mut egui::Ui, from: &str, name: &str, size: f32) {
    let (rect, _) = ui.allocate_exact_size(vec2(size, size), egui::Sense::hover());
    let bg = avatar_color(from, name);
    ui.painter()
        .circle_filled(rect.center(), size * 0.5, bg);
    ui.painter().circle_stroke(
        rect.center(),
        size * 0.5,
        Stroke::new(1.0_f32, Color32::from_rgba_unmultiplied(255, 255, 255, 36)),
    );
    let label = initials(from, name);
    ui.painter().text(
        rect.center(),
        egui::Align2::CENTER_CENTER,
        label,
        FontId::proportional((size * 0.38).max(8.0)),
        Color32::WHITE,
    );
}

fn mask_streamer(text: &str, streamer: bool, i18n: &I18nStore) -> String {
    if !streamer {
        return text.to_owned();
    }
    if text.contains('“') && text.contains('”') {
        let mut out = String::with_capacity(text.len());
        let mut in_quote = false;
        for ch in text.chars() {
            if ch == '“' {
                in_quote = true;
                out.push_str("“…”");
            } else if ch == '”' && in_quote {
                in_quote = false;
            } else if !in_quote {
                out.push(ch);
            }
        }
        out
    } else {
        i18n.tr("station.streamerRedacted")
    }
}

fn extract_image_id(photo: &str) -> Option<String> {
    let q = photo.split_once("?id=")?.1;
    let id = q.split('&').next().unwrap_or("");
    (!id.is_empty() && id.len() <= 64 && id.bytes().all(|b| b.is_ascii_hexdigit()))
        .then(|| id.to_owned())
}

fn short_preview(s: &str, max_chars: usize) -> String {
    let count = s.chars().count();
    if count <= max_chars {
        return s.to_owned();
    }
    let mut out: String = s.chars().take(max_chars.saturating_sub(1)).collect();
    out.push('…');
    out
}

pub fn emit_reaction(state: &mut UiState, msg_id: &str, emoji: &str) -> bool {
    if !REACTIONS.contains(&emoji) {
        return false;
    }
    let Some(msg) = state.chat.iter_mut().find(|m| m.id == msg_id && !m.recalled) else {
        return false;
    };
    let mut toggled_off = false;
    let mut empty_keys = Vec::new();
    for (k, list) in &mut msg.reacts {
        if let Some(pos) = list.iter().position(|x| x == "me") {
            list.remove(pos);
            if list.is_empty() {
                empty_keys.push(k.clone());
            }
            if k == emoji {
                toggled_off = true;
            }
        }
    }
    for k in empty_keys {
        msg.reacts.remove(&k);
    }
    if !toggled_off {
        msg.reacts
            .entry(emoji.to_owned())
            .or_default()
            .push("me".to_owned());
    }
    state.push_action(UiAction::ReactGroupChat {
        id: msg_id.to_owned(),
        emoji: emoji.to_owned(),
    });
    true
}

pub fn set_reply_target(state: &mut UiState, msg: &ChatMessage, i18n: &I18nStore) {
    if msg.recalled {
        return;
    }
    let preview = match msg.role.as_str() {
        "poll" => msg
            .poll
            .as_ref()
            .map(|p| format!("📊 {}", p.q))
            .unwrap_or_else(|| msg.text.clone()),
        "remind" => msg
            .remind
            .as_ref()
            .map(|r| format!("⏰ {}", r.title))
            .unwrap_or_else(|| msg.text.clone()),
        "photo" => i18n.tr("chat.photo"),
        "sticker" => i18n.tr("chat.sticker"),
        _ => short_preview(&msg.text, 90),
    };
    let name = if msg.from == "me" {
        i18n.tr("chat.you")
    } else {
        msg.sender.clone()
    };
    state.chat_reply_to = Some(ChatReplyRefUi {
        id: msg.id.clone(),
        name,
        text: preview,
    });
}

pub fn emit_send_message(state: &mut UiState, text: &str, sticker: Option<u32>) -> bool {
    let trimmed = text.trim();
    if trimmed.is_empty() && sticker.is_none() {
        return false;
    }
    let reply_to = state.chat_reply_to.take().map(|r| r.id);
    state.push_action(UiAction::SendGroupChat {
        text: trimmed.to_owned(),
        reply_to,
        sticker,
    });
    true
}

pub fn emit_recall_message(state: &mut UiState, msg_id: &str) -> bool {
    let Some(msg) = state
        .chat
        .iter_mut()
        .find(|m| m.id == msg_id && m.from == "me" && !m.recalled)
    else {
        return false;
    };
    msg.recalled = true;
    msg.text.clear();
    msg.sticker = None;
    msg.photo = None;
    msg.reacts.clear();
    if state.chat_pinned_id.as_deref() == Some(msg_id) {
        state.chat_pinned_id = None;
    }
    state.push_action(UiAction::RecallGroupChat {
        id: msg_id.to_owned(),
    });
    true
}

pub fn emit_poll_vote(state: &mut UiState, msg_id: &str, option_index: usize) -> bool {
    let Some(msg) = state
        .chat
        .iter_mut()
        .find(|m| m.id == msg_id && m.role == "poll")
    else {
        return false;
    };
    let Some(poll) = msg.poll.as_mut() else {
        return false;
    };
    if option_index >= poll.opts.len() {
        return false;
    }
    let had = poll.opts[option_index].by.iter().any(|x| x == "me");
    for opt in &mut poll.opts {
        opt.by.retain(|x| x != "me");
    }
    if !had {
        poll.opts[option_index].by.push("me".to_owned());
    }
    state.push_action(UiAction::VoteGroupPoll {
        id: msg_id.to_owned(),
        option_index,
    });
    true
}

pub fn emit_create_poll(state: &mut UiState, question: &str, options: &[String]) -> bool {
    let q = question.trim();
    let opts: Vec<String> = options
        .iter()
        .map(|s| s.trim().to_owned())
        .filter(|s| !s.is_empty())
        .take(6)
        .collect();
    if q.is_empty() || opts.len() < 2 {
        return false;
    }
    state.push_action(UiAction::CreateGroupPoll {
        q: q.to_owned(),
        opts,
    });
    true
}

pub fn emit_remind_rsvp(state: &mut UiState, msg_id: &str, yes: bool) -> bool {
    let Some(msg) = state
        .chat
        .iter_mut()
        .find(|m| m.id == msg_id && m.role == "remind")
    else {
        return false;
    };
    let Some(rem) = msg.remind.as_mut() else {
        return false;
    };
    if rem.done {
        return false;
    }
    rem.yes.retain(|x| x != "me");
    rem.no.retain(|x| x != "me");
    if yes {
        rem.yes.push("me".to_owned());
    } else {
        rem.no.push("me".to_owned());
    }
    state.push_action(UiAction::RsvpGroupRemind {
        id: msg_id.to_owned(),
        yes,
    });
    true
}

fn render_mentions_text(ui: &mut egui::Ui, text: &str) {
    ui.horizontal_wrapped(|ui| {
        ui.spacing_mut().item_spacing.x = 3.0;
        for token in text.split(' ') {
            if token.starts_with('@') && token.len() > 1 {
                ui.label(heading_bold(token, 12.0).color(BLUE));
            } else {
                ui.label(RichText::new(token).size(12.0).color(INK));
            }
        }
    });
}

fn render_poll_card(ui: &mut egui::Ui, msg: &ChatMessage, state: &mut UiState, i18n: &I18nStore) {
    let Some(poll) = msg.poll.as_ref() else {
        return;
    };
    let mut voters = std::collections::HashSet::new();
    let mut max_votes = 1usize;
    for opt in &poll.opts {
        max_votes = max_votes.max(opt.by.len());
        for v in &opt.by {
            voters.insert(v.as_str());
        }
    }
    let show_voters = state.chat_poll_show_voters.contains(&msg.id);
    let voted_me = voters.contains("me");

    egui::Frame::NONE
        .fill(CARD_2)
        .corner_radius(CornerRadius::same(12))
        .inner_margin(Margin::symmetric(12, 10))
        .stroke(Stroke::new(1.0_f32, LINE_2))
        .show(ui, |ui| {
            ui.set_min_width(ui.available_width());
            ui.label(
                heading_bold(format!("📊 {}", i18n.tr("chat.pollTag")), 10.5).color(BLUE),
            );
            ui.label(heading_bold(&poll.q, 13.0).color(INK));
            ui.add_space(6.0);

            for (idx, opt) in poll.opts.iter().enumerate() {
                ui.push_id(idx, |ui| {
                    let is_mine = opt.by.iter().any(|x| x == "me");
                    let ratio = (opt.by.len() as f32 / max_votes as f32).clamp(0.0, 1.0);
                    let row_stroke = if is_mine {
                        Stroke::new(1.2_f32, BLUE)
                    } else {
                        Stroke::new(1.0_f32, LINE)
                    };
                    let btn = egui::Frame::NONE
                        .fill(CARD)
                        .corner_radius(CornerRadius::same(8))
                        .inner_margin(Margin::symmetric(8, 6))
                        .stroke(row_stroke)
                        .show(ui, |ui| {
                            let full_w = ui.available_width();
                            ui.horizontal(|ui| {
                                ui.label(RichText::new(&opt.t).size(11.5).color(INK));
                                ui.with_layout(
                                    egui::Layout::right_to_left(egui::Align::Center),
                                    |ui| {
                                        ui.label(
                                            heading_bold(opt.by.len().to_string(), 11.0)
                                                .color(if is_mine { BLUE } else { MUTED }),
                                        );
                                    },
                                );
                            });
                            let (bar_rect, _) =
                                ui.allocate_exact_size(vec2(full_w, 4.0), egui::Sense::hover());
                            ui.painter().rect_filled(bar_rect, 2.0, BG);
                            let mut fill = bar_rect;
                            fill.set_width(bar_rect.width() * ratio);
                            ui.painter()
                                .rect_filled(fill, 2.0, if is_mine { BLUE } else { ACCENT });
                        });
                    if ui
                        .interact(btn.response.rect, ui.id().with("vote"), egui::Sense::click())
                        .clicked()
                    {
                        emit_poll_vote(state, &msg.id, idx);
                    }
                    if show_voters && !opt.by.is_empty() {
                        let names: Vec<String> = opt
                            .by
                            .iter()
                            .map(|k| {
                                if k == "me" {
                                    i18n.tr("chat.you")
                                } else {
                                    state
                                        .sessions
                                        .iter()
                                        .find(|s| s.id == *k)
                                        .map(|s| s.project.clone())
                                        .unwrap_or_else(|| k.clone())
                                }
                            })
                            .collect();
                        ui.label(RichText::new(names.join(", ")).size(10.0).color(MUTED));
                    }
                    ui.add_space(4.0);
                });
            }

            ui.horizontal(|ui| {
                ui.label(
                    RichText::new(i18n.tr_vars(
                        "chat.pollVoters",
                        &[("n", &voters.len().to_string())],
                    ))
                    .size(10.5)
                    .color(MUTED),
                );
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    let lbl_key = if show_voters {
                        "chat.pollSee"
                    } else if voted_me {
                        "chat.pollChange"
                    } else {
                        "chat.pollVote"
                    };
                    if ui.small_button(i18n.tr(lbl_key)).clicked() {
                        if show_voters {
                            state.chat_poll_show_voters.remove(&msg.id);
                        } else {
                            state.chat_poll_show_voters.insert(msg.id.clone());
                        }
                    }
                });
            });
        });
}

fn render_remind_card(
    ui: &mut egui::Ui,
    msg: &ChatMessage,
    state: &mut UiState,
    i18n: &I18nStore,
) {
    let Some(rem) = msg.remind.as_ref() else {
        return;
    };
    egui::Frame::NONE
        .fill(CARD_2)
        .corner_radius(CornerRadius::same(12))
        .inner_margin(Margin::symmetric(12, 10))
        .stroke(Stroke::new(1.0_f32, AMBER))
        .show(ui, |ui| {
            ui.set_min_width(ui.available_width());
            ui.label(
                heading_bold(format!("⏰ {}", i18n.tr("chat.remindTag")), 10.5).color(AMBER),
            );
            ui.label(heading_bold(&rem.title, 13.0).color(INK));
            if !rem.repeat.is_empty() {
                ui.label(
                    RichText::new(format!("🔁 {}", i18n.tr("chat.remindWeek")))
                        .size(10.5)
                        .color(MUTED),
                );
            }
            ui.add_space(4.0);
            let status_txt = if rem.done {
                i18n.tr("chat.remindDone")
            } else {
                i18n.tr_vars("chat.remindGoing", &[("n", &rem.yes.len().to_string())])
            };
            ui.label(RichText::new(status_txt).size(10.5).color(MUTED));
            ui.add_space(6.0);
            ui.horizontal(|ui| {
                let yes_on = rem.yes.iter().any(|x| x == "me");
                let no_on = rem.no.iter().any(|x| x == "me");
                let yes_btn = egui::Button::new(
                    RichText::new(i18n.tr("chat.remindYes"))
                        .size(11.0)
                        .color(if yes_on { Color32::from_rgb(0x06, 0x15, 0x0C) } else { INK }),
                )
                .fill(if yes_on { GREEN } else { CARD_3 })
                .corner_radius(CornerRadius::same(7));
                if ui.add_enabled(!rem.done, yes_btn).clicked() {
                    emit_remind_rsvp(state, &msg.id, true);
                }

                let no_btn = egui::Button::new(
                    RichText::new(i18n.tr("chat.remindNo"))
                        .size(11.0)
                        .color(if no_on { Color32::WHITE } else { MUTED }),
                )
                .fill(if no_on { RED } else { CARD_3 })
                .corner_radius(CornerRadius::same(7));
                if ui.add_enabled(!rem.done, no_btn).clicked() {
                    emit_remind_rsvp(state, &msg.id, false);
                }
            });
        });
}

fn render_message_row(
    ui: &mut egui::Ui,
    msg: &ChatMessage,
    state: &mut UiState,
    i18n: &I18nStore,
) {
    if msg.role == "sys" {
        ui.vertical_centered(|ui| {
            egui::Frame::NONE
                .fill(CARD)
                .corner_radius(CornerRadius::same(10))
                .inner_margin(Margin::symmetric(10, 4))
                .show(ui, |ui| {
                    ui.label(RichText::new(&msg.text).size(10.5).color(MUTED));
                });
        });
        return;
    }

    if msg.role == "poll" && msg.poll.is_some() {
        render_poll_card(ui, msg, state, i18n);
        return;
    }

    if msg.role == "remind" && msg.remind.is_some() {
        render_remind_card(ui, msg, state, i18n);
        return;
    }

    let mine = msg.from == "me";
    let bubble_fill = if mine {
        Color32::from_rgb(0x1B, 0x2F, 0x4A)
    } else {
        CARD
    };
    let bubble_stroke = if mine {
        Stroke::new(1.0_f32, Color32::from_rgba_unmultiplied(0x6A, 0xAE, 0xFF, 70))
    } else {
        Stroke::new(1.0_f32, LINE)
    };

    ui.horizontal_top(|ui| {
        if !mine {
            paint_avatar_badge(ui, &msg.from, &msg.sender, 28.0);
        }
        egui::Frame::NONE
            .fill(bubble_fill)
            .corner_radius(CornerRadius::same(10))
            .inner_margin(Margin::symmetric(10, 7))
            .stroke(bubble_stroke)
            .show(ui, |ui| {
                ui.set_min_width((ui.available_width() - 6.0).max(160.0));
                ui.horizontal(|ui| {
                    let display_sender = if mine {
                        i18n.tr("chat.you")
                    } else {
                        msg.sender.clone()
                    };
                    ui.label(
                        heading_bold(display_sender, 11.0)
                            .color(if mine { BLUE } else { ACCENT }),
                    );
                    if let Some(tag) = msg.tag.as_deref()
                        && is_agent_answer(Some(tag))
                    {
                        let badge_text = if tag == "claude" {
                            i18n.tr("chat.claudeTag")
                        } else {
                            tag.to_owned()
                        };
                        egui::Frame::NONE
                            .fill(Color32::from_rgba_unmultiplied(0xEF, 0x8A, 0x5F, 36))
                            .corner_radius(CornerRadius::same(4))
                            .inner_margin(Margin::symmetric(5, 1))
                            .show(ui, |ui| {
                                ui.label(RichText::new(badge_text).size(9.5).color(ACCENT));
                            });
                    }
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        ui.label(RichText::new(&msg.ts).size(9.5).color(FAINT));
                    });
                });

                if !msg.recalled
                    && let Some(rep) = msg.reply.as_ref()
                {
                    egui::Frame::NONE
                        .fill(BG)
                        .corner_radius(CornerRadius::same(6))
                        .inner_margin(Margin::symmetric(7, 4))
                        .stroke(Stroke::new(1.0_f32, LINE_2))
                        .show(ui, |ui| {
                            ui.label(heading_bold(&rep.name, 10.0).color(BLUE));
                            let qtxt = mask_streamer(&rep.text, state.streamer_mode, i18n);
                            ui.label(RichText::new(qtxt).size(10.5).color(MUTED));
                        });
                    ui.add_space(3.0);
                }

                if msg.recalled {
                    ui.label(
                        RichText::new(i18n.tr("chat.recalled"))
                            .italics()
                            .size(11.5)
                            .color(MUTED),
                    );
                } else if let Some(st_idx) = msg.sticker {
                    let emo = STICKER_EMOJI[(st_idx as usize) % STICKER_EMOJI.len()];
                    let cap = i18n.tr_pick("chat.stickers", st_idx as usize);
                    ui.horizontal(|ui| {
                        ui.label(RichText::new(emo).size(28.0));
                        ui.label(heading_bold(cap, 13.0).color(AMBER));
                    });
                } else if let Some(photo) = msg.photo.as_deref().filter(|p| !p.is_empty()) {
                    ui.horizontal_wrapped(|ui| {
                        ui.label(RichText::new(i18n.tr("chat.photo")).size(11.5).color(BLUE));
                        if let Some(img_id) = extract_image_id(photo) {
                            if ui.small_button(i18n.tr("chat.readMore")).clicked() {
                                state.push_action(UiAction::FetchChatImage { id: img_id });
                            }
                        }
                    });
                    if !msg.text.is_empty() {
                        let body = mask_streamer(&msg.text, state.streamer_mode, i18n);
                        render_mentions_text(ui, &body);
                    }
                } else {
                    let body = mask_streamer(&msg.text, state.streamer_mode, i18n);
                    if is_agent_answer(msg.tag.as_deref()) {
                        render_markdown(ui, &body);
                    } else {
                        render_mentions_text(ui, &body);
                    }
                }

                if !msg.recalled {
                    if !msg.reacts.is_empty() {
                        ui.add_space(3.0);
                        ui.horizontal_wrapped(|ui| {
                            for (emoji, who) in &msg.reacts {
                                if who.is_empty() {
                                    continue;
                                }
                                let is_mine = who.iter().any(|x| x == "me");
                                let pill_txt = if who.len() > 1 {
                                    format!("{emoji} {}", who.len())
                                } else {
                                    emoji.clone()
                                };
                                let pill = egui::Button::new(
                                    RichText::new(pill_txt).size(10.5).color(INK),
                                )
                                .fill(if is_mine {
                                    Color32::from_rgba_unmultiplied(0x6A, 0xAE, 0xFF, 52)
                                } else {
                                    CARD_3
                                })
                                .corner_radius(CornerRadius::same(10));
                                if ui.add(pill).clicked() {
                                    emit_reaction(state, &msg.id, emoji);
                                }
                            }
                        });
                    }

                    ui.add_space(4.0);
                    ui.horizontal_wrapped(|ui| {
                        ui.spacing_mut().item_spacing.x = 3.0;
                        for (r_idx, emoji) in REACTIONS.iter().enumerate() {
                            ui.push_id(r_idx, |ui| {
                                if ui
                                    .add(
                                        egui::Button::new(RichText::new(*emoji).size(11.0))
                                            .fill(Color32::TRANSPARENT)
                                            .frame(false),
                                    )
                                    .on_hover_text(i18n.tr("chat.react"))
                                    .clicked()
                                {
                                    emit_reaction(state, &msg.id, emoji);
                                }
                            });
                        }

                        let reply_hint = i18n.tr_vars("chat.replyTo", &[("name", &msg.sender)]);
                        if ui
                            .small_button("↩")
                            .on_hover_text(reply_hint)
                            .clicked()
                        {
                            set_reply_target(state, msg, i18n);
                        }
                        if ui
                            .small_button("📌")
                            .on_hover_text(i18n.tr("chat.pinned"))
                            .clicked()
                        {
                            state.chat_pinned_id = Some(msg.id.clone());
                            state.push_action(UiAction::PinGroupChat { id: msg.id.clone() });
                        }
                        if mine
                            && ui
                                .small_button(i18n.tr("chat.recall"))
                                .clicked()
                        {
                            emit_recall_message(state, &msg.id);
                        }
                    });
                }
            });
    });
}

/// Full 420px right slide-over `GroupChat` drawer matching `group-chat-view.js` & `chat-avatars.js`.
pub fn draw_group_chat_drawer(ctx: &egui::Context, state: &mut UiState, i18n: &I18nStore) {
    state.unread_chat = 0;

    if !ctx.wants_keyboard_input() && ctx.input(|i| i.key_pressed(egui::Key::Escape)) {
        state.active_drawer = ActiveDrawer::None;
        return;
    }

    let screen = ctx.screen_rect();
    let drawer_w = DRAWER_WIDTH.min(screen.width()).max(300.0);
    let top_y = if screen.height() > TOPBAR_HEIGHT + 120.0 {
        TOPBAR_HEIGHT
    } else {
        0.0
    };
    let drawer_h = (screen.height() - top_y).max(240.0);

    egui::Area::new(egui::Id::new("station_group_chat_drawer"))
        .order(egui::Order::Foreground)
        .fixed_pos(pos2(screen.right() - drawer_w, top_y))
        .show(ctx, |ui| {
            egui::Frame::NONE
                .fill(SIDE)
                .stroke(Stroke::new(1.0_f32, LINE_2))
                .inner_margin(Margin::symmetric(12, 10))
                .show(ui, |ui| {
                    ui.set_width(drawer_w - 24.0);
                    ui.set_height(drawer_h - 20.0);

                    // 1. Header
                    ui.horizontal(|ui| {
                        paint_avatar_badge(ui, "__claude", &i18n.tr("chat.group"), 34.0);
                        ui.vertical(|ui| {
                            ui.label(heading_bold(i18n.tr("chat.group"), 14.0).color(INK));
                            let members_n = state.sessions.len() + 1;
                            ui.label(
                                RichText::new(i18n.tr_vars(
                                    "chat.members",
                                    &[("n", &members_n.to_string())],
                                ))
                                .size(10.5)
                                .color(MUTED),
                            );
                        });
                        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                            if ui
                                .button(RichText::new("×").size(15.0).color(INK))
                                .on_hover_text(i18n.tr("chat.close"))
                                .clicked()
                            {
                                state.active_drawer = ActiveDrawer::None;
                            }
                        });
                    });
                    ui.add_space(6.0);

                    // 2. Pinned message banner
                    if let Some(pin_id) = state.chat_pinned_id.clone()
                        && let Some(pinned) = state
                            .chat
                            .iter()
                            .find(|m| m.id == pin_id && !m.recalled)
                            .cloned()
                    {
                        egui::Frame::NONE
                            .fill(CARD_2)
                            .corner_radius(CornerRadius::same(8))
                            .inner_margin(Margin::symmetric(8, 5))
                            .stroke(Stroke::new(1.0_f32, LINE))
                            .show(ui, |ui| {
                                ui.set_min_width(ui.available_width());
                                ui.horizontal(|ui| {
                                    ui.label(
                                        heading_bold(format!("📌 {}", i18n.tr("chat.pinned")), 10.5)
                                            .color(AMBER),
                                    );
                                    let preview = if let Some(p) = &pinned.poll {
                                        format!("📊 {}", p.q)
                                    } else if let Some(r) = &pinned.remind {
                                        format!("⏰ {}", r.title)
                                    } else {
                                        short_preview(
                                            &mask_streamer(&pinned.text, state.streamer_mode, i18n),
                                            60,
                                        )
                                    };
                                    ui.label(RichText::new(preview).size(11.0).color(INK));
                                });
                            });
                        ui.add_space(6.0);
                    }

                    // 3. Active /api/image preview strip (dropped immediately on close)
                    if let Some(prev) = state.chat_preview_image.clone() {
                        egui::Frame::NONE
                            .fill(CARD_2)
                            .corner_radius(CornerRadius::same(8))
                            .inner_margin(Margin::symmetric(8, 6))
                            .stroke(Stroke::new(1.0_f32, BLUE))
                            .show(ui, |ui| {
                                ui.horizontal(|ui| {
                                    ui.label(
                                        heading_bold(
                                            format!("{} · {}", i18n.tr("chat.photo"), prev.id),
                                            10.5,
                                        )
                                        .color(BLUE),
                                    );
                                    ui.with_layout(
                                        egui::Layout::right_to_left(egui::Align::Center),
                                        |ui| {
                                            if ui.small_button("×").clicked() {
                                                state.chat_preview_image = None;
                                            }
                                        },
                                    );
                                });
                                if prev.width > 0
                                    && prev.height > 0
                                    && prev.rgba.len() == prev.width * prev.height * 4
                                {
                                    let img = egui::ColorImage::from_rgba_unmultiplied(
                                        [prev.width, prev.height],
                                        &prev.rgba,
                                    );
                                    let tex = ctx.load_texture(
                                        format!("chat_img_preview_{}", prev.id),
                                        img,
                                        egui::TextureOptions::LINEAR,
                                    );
                                    let max_w = (drawer_w - 48.0).min(prev.width as f32);
                                    let aspect = prev.height as f32 / prev.width as f32;
                                    ui.image((tex.id(), vec2(max_w, (max_w * aspect).min(180.0))));
                                }
                            });
                        ui.add_space(6.0);
                    }

                    // 4. Scrollable thread
                    let reserve_bottom = 128.0
                        + if state.chat_poll_open { 145.0 } else { 0.0 }
                        + if state.chat_sticker_open { 115.0 } else { 0.0 }
                        + if state.chat_mention_open { 72.0 } else { 0.0 }
                        + if state.chat_reply_to.is_some() { 38.0 } else { 0.0 };
                    let scroll_h = (ui.available_height() - reserve_bottom).max(120.0);

                    egui::ScrollArea::vertical()
                        .id_salt("group_chat_messages_scroll")
                        .max_height(scroll_h)
                        .stick_to_bottom(true)
                        .show(ui, |ui| {
                            let msgs = state.chat.clone();
                            for (idx, msg) in msgs.iter().enumerate() {
                                let row_id = if msg.id.is_empty() {
                                    format!("row-{idx}")
                                } else {
                                    msg.id.clone()
                                };
                                ui.push_id(row_id, |ui| {
                                    render_message_row(ui, msg, state, i18n);
                                    ui.add_space(6.0);
                                });
                            }
                        });

                    // 5. Typing indicator
                    if !state.chat_typing.is_empty() {
                        let who = if state.chat_typing.len() > 1 {
                            format!("{} +{}", state.chat_typing[0], state.chat_typing.len() - 1)
                        } else {
                            state.chat_typing[0].clone()
                        };
                        ui.label(
                            RichText::new(i18n.tr_vars("chat.typing", &[("name", &who)]))
                                .size(10.5)
                                .italics()
                                .color(MUTED),
                        );
                    }

                    // 6. Reply-to banner
                    if let Some(rep) = state.chat_reply_to.clone() {
                        egui::Frame::NONE
                            .fill(CARD_2)
                            .corner_radius(CornerRadius::same(7))
                            .inner_margin(Margin::symmetric(8, 4))
                            .stroke(Stroke::new(1.0_f32, BLUE))
                            .show(ui, |ui| {
                                ui.horizontal(|ui| {
                                    ui.label(
                                        heading_bold(
                                            i18n.tr_vars("chat.replyTo", &[("name", &rep.name)]),
                                            10.5,
                                        )
                                        .color(BLUE),
                                    );
                                    ui.label(
                                        RichText::new(short_preview(&rep.text, 40))
                                            .size(10.5)
                                            .color(MUTED),
                                    );
                                    ui.with_layout(
                                        egui::Layout::right_to_left(egui::Align::Center),
                                        |ui| {
                                            if ui.small_button("×").clicked() {
                                                state.chat_reply_to = None;
                                            }
                                        },
                                    );
                                });
                            });
                        ui.add_space(4.0);
                    }

                    // 7. Collapsible Sticker/Emoji panel
                    if state.chat_sticker_open {
                        egui::Frame::NONE
                            .fill(CARD)
                            .corner_radius(CornerRadius::same(8))
                            .inner_margin(Margin::symmetric(8, 6))
                            .stroke(Stroke::new(1.0_f32, LINE))
                            .show(ui, |ui| {
                                ui.horizontal(|ui| {
                                    if ui
                                        .selectable_label(
                                            state.chat_sticker_tab == 0,
                                            i18n.tr("chat.stickerTab"),
                                        )
                                        .clicked()
                                    {
                                        state.chat_sticker_tab = 0;
                                    }
                                    if ui
                                        .selectable_label(
                                            state.chat_sticker_tab == 1,
                                            i18n.tr("chat.emojiTab"),
                                        )
                                        .clicked()
                                    {
                                        state.chat_sticker_tab = 1;
                                    }
                                });
                                ui.add_space(4.0);
                                if state.chat_sticker_tab == 0 {
                                    ui.horizontal_wrapped(|ui| {
                                        for (i, emo) in STICKER_EMOJI.iter().enumerate() {
                                            ui.push_id(i, |ui| {
                                                let cap = i18n.tr_pick("chat.stickers", i);
                                                if ui
                                                    .button(format!("{emo} {cap}"))
                                                    .clicked()
                                                {
                                                    emit_send_message(state, "", Some(i as u32));
                                                    state.chat_sticker_open = false;
                                                }
                                            });
                                        }
                                    });
                                } else {
                                    ui.horizontal_wrapped(|ui| {
                                        for (i, emo) in QUICK_EMOJIS.iter().enumerate() {
                                            ui.push_id(i, |ui| {
                                                if ui.small_button(*emo).clicked() {
                                                    state.chat_draft.push_str(emo);
                                                }
                                            });
                                        }
                                    });
                                }
                            });
                        ui.add_space(4.0);
                    }

                    // 8. Collapsible @Mention picker
                    if state.chat_mention_open {
                        egui::Frame::NONE
                            .fill(CARD)
                            .corner_radius(CornerRadius::same(8))
                            .inner_margin(Margin::symmetric(8, 6))
                            .stroke(Stroke::new(1.0_f32, LINE))
                            .show(ui, |ui| {
                                ui.horizontal_wrapped(|ui| {
                                    let all_lbl = format!("@{}", i18n.tr("chat.everyone"));
                                    if ui.small_button(&all_lbl).clicked() {
                                        state.chat_draft.push_str(&all_lbl);
                                        state.chat_draft.push(' ');
                                        state.chat_mention_open = false;
                                    }
                                    let names: Vec<String> = state
                                        .sessions
                                        .iter()
                                        .map(|s| s.project.clone())
                                        .collect();
                                    for (idx, name) in names.iter().enumerate() {
                                        ui.push_id(idx, |ui| {
                                            let tag = format!("@{name}");
                                            if ui.small_button(&tag).clicked() {
                                                state.chat_draft.push_str(&tag);
                                                state.chat_draft.push(' ');
                                                state.chat_mention_open = false;
                                            }
                                        });
                                    }
                                });
                            });
                        ui.add_space(4.0);
                    }

                    // 9. Collapsible Poll creation form
                    if state.chat_poll_open {
                        egui::Frame::NONE
                            .fill(CARD)
                            .corner_radius(CornerRadius::same(8))
                            .inner_margin(Margin::symmetric(8, 6))
                            .stroke(Stroke::new(1.0_f32, LINE_2))
                            .show(ui, |ui| {
                                ui.label(heading_bold(i18n.tr("chat.newPoll"), 11.5).color(BLUE));
                                ui.add(
                                    egui::TextEdit::singleline(&mut state.chat_poll_q)
                                        .desired_width(f32::INFINITY)
                                        .hint_text(i18n.tr("chat.pollAsk")),
                                );
                                if state.chat_poll_opts.len() < 2 {
                                    state.chat_poll_opts.resize(2, String::new());
                                }
                                for idx in 0..state.chat_poll_opts.len() {
                                    ui.push_id(idx, |ui| {
                                        ui.add(
                                            egui::TextEdit::singleline(
                                                &mut state.chat_poll_opts[idx],
                                            )
                                            .desired_width(f32::INFINITY)
                                            .hint_text(format!("#{}", idx + 1)),
                                        );
                                    });
                                }
                                ui.horizontal(|ui| {
                                    if state.chat_poll_opts.len() < 6
                                        && ui.small_button(i18n.tr("chat.quick.add")).clicked()
                                    {
                                        state.chat_poll_opts.push(String::new());
                                    }
                                    ui.with_layout(
                                        egui::Layout::right_to_left(egui::Align::Center),
                                        |ui| {
                                            if ui.small_button(i18n.tr("chat.send")).clicked() {
                                                let q = state.chat_poll_q.clone();
                                                let opts = state.chat_poll_opts.clone();
                                                if emit_create_poll(state, &q, &opts) {
                                                    state.chat_poll_q.clear();
                                                    state.chat_poll_opts =
                                                        vec![String::new(), String::new()];
                                                    state.chat_poll_open = false;
                                                }
                                            }
                                            if ui.small_button(i18n.tr("chat.cancel")).clicked() {
                                                state.chat_poll_open = false;
                                            }
                                        },
                                    );
                                });
                            });
                        ui.add_space(4.0);
                    }

                    // 10. Action toolbar & composer input
                    ui.horizontal(|ui| {
                        if ui
                            .selectable_label(state.chat_sticker_open, "😊")
                            .on_hover_text(i18n.tr("chat.stickerTab"))
                            .clicked()
                        {
                            state.chat_sticker_open = !state.chat_sticker_open;
                            state.chat_poll_open = false;
                            state.chat_mention_open = false;
                        }
                        if ui
                            .selectable_label(state.chat_poll_open, "📊")
                            .on_hover_text(i18n.tr("chat.newPoll"))
                            .clicked()
                        {
                            state.chat_poll_open = !state.chat_poll_open;
                            state.chat_sticker_open = false;
                            state.chat_mention_open = false;
                        }
                        if ui
                            .selectable_label(state.chat_mention_open, "@")
                            .on_hover_text(i18n.tr("chat.mention"))
                            .clicked()
                        {
                            state.chat_mention_open = !state.chat_mention_open;
                            state.chat_sticker_open = false;
                            state.chat_poll_open = false;
                        }
                        if ui
                            .small_button("📷")
                            .on_hover_text(i18n.tr("chat.sendPhoto"))
                            .clicked()
                        {
                            state.push_action(UiAction::SendGroupPhoto);
                        }
                    });
                    ui.add_space(4.0);

                    let placeholder = i18n.tr_vars(
                        "chat.placeholderPc",
                        &[("group", &i18n.tr("chat.group"))],
                    );
                    ui.horizontal(|ui| {
                        let edit = ui.add(
                            egui::TextEdit::singleline(&mut state.chat_draft)
                                .desired_width((ui.available_width() - 74.0).max(120.0))
                                .hint_text(placeholder),
                        );
                        let has_text = !state.chat_draft.trim().is_empty();
                        let send_lbl = if has_text {
                            i18n.tr("chat.sendPc")
                        } else {
                            "👍".to_owned()
                        };
                        let enter =
                            edit.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter));
                        let btn = egui::Button::new(
                            heading_bold(send_lbl, 11.0).color(if has_text {
                                Color32::from_rgb(0x2A, 0x13, 0x07)
                            } else {
                                INK
                            }),
                        )
                        .fill(if has_text { ACCENT } else { CARD_2 })
                        .corner_radius(CornerRadius::same(8));
                        let clicked = ui
                            .add(btn)
                            .on_hover_text(if has_text {
                                i18n.tr("chat.send")
                            } else {
                                i18n.tr("chat.like")
                            })
                            .clicked();
                        if clicked || enter {
                            if has_text {
                                let text = std::mem::take(&mut state.chat_draft);
                                emit_send_message(state, &text, None);
                            } else if clicked {
                                emit_send_message(state, "👍", None);
                            }
                        }
                    });
                });
        });
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::state::{ChatPollOptUi, ChatPollUi, ChatRemindUi};
    use std::path::PathBuf;

    fn i18n(lang: &str) -> I18nStore {
        I18nStore::load_from_assets(
            &PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../assets"),
            lang,
        )
    }

    #[test]
    fn group_chat_reaction_emission_validates_and_toggles_six_reactions() {
        let mut st = UiState::default();
        st.chat.push(ChatMessage {
            id: "m1".to_owned(),
            from: "sess-1".to_owned(),
            sender: "Claude".to_owned(),
            role: "text".to_owned(),
            text: "Hello!".to_owned(),
            ..Default::default()
        });

        for emoji in REACTIONS {
            assert!(
                emit_reaction(&mut st, "m1", emoji),
                "reaction `{emoji}` must be accepted"
            );
        }
        assert!(
            !emit_reaction(&mut st, "m1", "not-an-emoji"),
            "unknown reaction emoji must be rejected"
        );

        let actions = st.drain_actions();
        assert_eq!(actions.len(), 6);
        assert!(matches!(
            &actions[0],
            UiAction::ReactGroupChat { id, emoji } if id == "m1" && emoji == "❤️"
        ));
    }

    #[test]
    fn group_chat_reply_to_and_recall_emit_expected_actions() {
        let tr = i18n("en");
        let mut st = UiState::default();
        let target = ChatMessage {
            id: "m-other".to_owned(),
            from: "sess-1".to_owned(),
            sender: "Claude".to_owned(),
            role: "text".to_owned(),
            text: "Shipped PR #12".to_owned(),
            ..Default::default()
        };
        let mine = ChatMessage {
            id: "m-me".to_owned(),
            from: "me".to_owned(),
            sender: "Boss".to_owned(),
            role: "text".to_owned(),
            text: "Typo message".to_owned(),
            ..Default::default()
        };
        st.chat.push(target.clone());
        st.chat.push(mine);

        set_reply_target(&mut st, &target, &tr);
        assert_eq!(
            st.chat_reply_to.as_ref().map(|r| r.id.as_str()),
            Some("m-other")
        );
        assert!(emit_send_message(&mut st, "Looks great @Claude", None));
        assert!(
            st.chat_reply_to.is_none(),
            "sending reply must clear reply banner"
        );

        assert!(
            !emit_recall_message(&mut st, "m-other"),
            "cannot recall another sender's message"
        );
        assert!(
            emit_recall_message(&mut st, "m-me"),
            "must allow recalling own message"
        );

        let actions = st.drain_actions();
        assert!(actions.iter().any(|a| matches!(
            a,
            UiAction::SendGroupChat {
                text,
                reply_to: Some(rid),
                sticker: None
            } if text == "Looks great @Claude" && rid == "m-other"
        )));
        assert!(actions.iter().any(|a| matches!(
            a,
            UiAction::RecallGroupChat { id } if id == "m-me"
        )));
    }

    #[test]
    fn group_chat_poll_create_vote_and_remind_rsvp_emit_actions() {
        let mut st = UiState::default();
        st.chat.push(ChatMessage {
            id: "p1".to_owned(),
            from: "me".to_owned(),
            sender: "Boss".to_owned(),
            role: "poll".to_owned(),
            poll: Some(ChatPollUi {
                q: "Lunch?".to_owned(),
                opts: vec![
                    ChatPollOptUi {
                        t: "Pho".to_owned(),
                        by: vec![],
                    },
                    ChatPollOptUi {
                        t: "Banh mi".to_owned(),
                        by: vec![],
                    },
                ],
            }),
            ..Default::default()
        });
        st.chat.push(ChatMessage {
            id: "r1".to_owned(),
            from: "__hr".to_owned(),
            sender: "HR".to_owned(),
            role: "remind".to_owned(),
            remind: Some(ChatRemindUi {
                title: "Standup".to_owned(),
                at_ms: 1_700_000_000_000,
                repeat: "week".to_owned(),
                yes: vec![],
                no: vec![],
                done: false,
            }),
            ..Default::default()
        });

        assert!(
            !emit_create_poll(&mut st, "Lunch?", &["Only one".to_owned()]),
            "poll requires at least 2 non-empty options"
        );
        assert!(emit_create_poll(
            &mut st,
            "Coffee?",
            &["Bac xiu".to_owned(), "Matcha".to_owned()]
        ));
        assert!(emit_poll_vote(&mut st, "p1", 1));
        assert!(!emit_poll_vote(&mut st, "p1", 9));
        assert!(emit_remind_rsvp(&mut st, "r1", true));

        let actions = st.drain_actions();
        assert!(actions.iter().any(|a| matches!(
            a,
            UiAction::CreateGroupPoll { q, opts } if q == "Coffee?" && opts.len() == 2
        )));
        assert!(actions.iter().any(|a| matches!(
            a,
            UiAction::VoteGroupPoll { id, option_index: 1 } if id == "p1"
        )));
        assert!(actions.iter().any(|a| matches!(
            a,
            UiAction::RsvpGroupRemind { id, yes: true } if id == "r1"
        )));
    }

    #[test]
    fn group_chat_drawer_renders_cleanly_in_en_vi_th_with_zero_raw_keys() {
        let mut st = UiState::with_demo_seed(
            station_3d::OfficeLayout::Pro,
            station_3d::BackdropKind::City,
            false,
        );
        st.active_drawer = ActiveDrawer::GroupChat;
        st.chat_poll_open = true;
        st.chat_sticker_open = true;
        st.chat_mention_open = true;

        for lang in ["en", "vi", "th"] {
            let tr = i18n(lang);
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
            let _ = ctx.run(egui::RawInput::default(), |ctx| {
                draw_group_chat_drawer(ctx, &mut st, &tr);
            });
            assert_eq!(st.unread_chat, 0);
        }
    }
}
