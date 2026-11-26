use crate::i18n::I18nStore;
use crate::state::{ActiveDrawer, TodoItem, UiAction, UiState};
use crate::theme::{
    ACCENT, AMBER, BG, CARD, CARD_2, GREEN, INK, LINE, LINE_2, MUTED, RED, SIDE, heading_bold,
    status_dot,
};
use bevy_egui::egui::{self, Color32, CornerRadius, Margin, RichText, Stroke, vec2};
use std::sync::atomic::{AtomicU64, Ordering};

const STICKY_PALETTE: [Color32; 6] = [
    Color32::from_rgb(0xFF, 0xE7, 0x87),
    Color32::from_rgb(0xFF, 0xD0, 0xDC),
    Color32::from_rgb(0xC8, 0xF0, 0xD4),
    Color32::from_rgb(0xC7, 0xE5, 0xFF),
    Color32::from_rgb(0xFF, 0xE0, 0xB2),
    Color32::from_rgb(0xE4, 0xD4, 0xFF),
];
const STICKY_DONE_BG: Color32 = Color32::from_rgb(0xE6, 0xE2, 0xD6);
const STICKY_INK: Color32 = Color32::from_rgb(0x2E, 0x2B, 0x26);
const STICKY_DONE_INK: Color32 = Color32::from_rgb(0x6E, 0x6A, 0x62);

static NEXT_TODO_SEQ: AtomicU64 = AtomicU64::new(1);

fn make_todo_id() -> String {
    let seq = NEXT_TODO_SEQ.fetch_add(1, Ordering::Relaxed);
    format!("td-{seq:x}")
}

fn sticky_color(item: &TodoItem) -> Color32 {
    if item.done {
        return STICKY_DONE_BG;
    }
    let bytes = item.id.as_bytes();
    let hash = if bytes.is_empty() {
        item.text.len()
    } else {
        bytes[0] as usize + bytes[bytes.len() - 1] as usize
    };
    STICKY_PALETTE[hash % STICKY_PALETTE.len()]
}

pub fn format_todos_markdown(todos: &[TodoItem]) -> String {
    todos
        .iter()
        .map(|t| {
            if t.done {
                format!("- [x] {}", t.text)
            } else {
                format!("- [ ] {}", t.text)
            }
        })
        .collect::<Vec<_>>()
        .join("\n")
}

fn sync_todos(state: &mut UiState) {
    state.whiteboard_text = format_todos_markdown(&state.session_todos);
    let snapshot = state.session_todos.clone();
    state.push_action(UiAction::SyncWhiteboardTodos(snapshot));
}

pub fn add_sticky_note(state: &mut UiState, text: &str) -> bool {
    let clean: String = text.trim().chars().take(140).collect();
    if clean.is_empty() {
        return false;
    }
    state.session_todos.push(TodoItem {
        id: make_todo_id(),
        text: clean,
        done: false,
        done_at: 0,
    });
    sync_todos(state);
    true
}

pub fn toggle_sticky_note(state: &mut UiState, id: &str) -> bool {
    let Some(item) = state.session_todos.iter_mut().find(|t| t.id == id) else {
        return false;
    };
    item.done = !item.done;
    item.done_at = if item.done {
        NEXT_TODO_SEQ.fetch_add(1, Ordering::Relaxed)
    } else {
        0
    };
    sync_todos(state);
    true
}

pub fn delete_sticky_note(state: &mut UiState, id: &str) -> bool {
    let before = state.session_todos.len();
    state.session_todos.retain(|t| t.id != id);
    if state.session_todos.len() == before {
        return false;
    }
    sync_todos(state);
    true
}

pub fn clear_done_sticky_notes(state: &mut UiState) -> usize {
    let before = state.session_todos.len();
    state.session_todos.retain(|t| !t.done);
    let removed = before.saturating_sub(state.session_todos.len());
    if removed > 0 {
        sync_todos(state);
    }
    removed
}

fn sorted_todos(todos: &[TodoItem]) -> Vec<TodoItem> {
    let mut open: Vec<TodoItem> = todos.iter().filter(|t| !t.done).cloned().collect();
    let mut done: Vec<TodoItem> = todos.iter().filter(|t| t.done).cloned().collect();
    done.sort_by(|a, b| b.done_at.cmp(&a.done_at));
    open.extend(done);
    open
}

/// Interactive sticky-note `Whiteboard` modal matching `src/office/ui/whiteboard.js`.
pub fn draw_whiteboard_modal(ctx: &egui::Context, state: &mut UiState, i18n: &I18nStore) {
    if !ctx.wants_keyboard_input() && ctx.input(|i| i.key_pressed(egui::Key::Escape)) {
        state.active_drawer = ActiveDrawer::None;
        return;
    }

    let mut open = true;
    egui::Window::new(heading_bold(i18n.tr("board.cardTitle"), 14.5).color(INK))
        .id(egui::Id::new("station_whiteboard_modal"))
        .open(&mut open)
        .anchor(egui::Align2::CENTER_CENTER, vec2(0.0, 0.0))
        .default_width(540.0)
        .max_height(600.0)
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
            ui.label(RichText::new(i18n.tr("board.cardSub")).size(11.5).color(MUTED));
            ui.add_space(10.0);

            // 1. Your sticky notes header + open counter
            let open_cnt = state.session_todos.iter().filter(|t| !t.done).count();
            let done_cnt = state.session_todos.len().saturating_sub(open_cnt);
            ui.horizontal(|ui| {
                ui.label(heading_bold(i18n.tr("board.yours"), 12.5).color(INK));
                if !state.session_todos.is_empty() {
                    ui.label(
                        RichText::new(i18n.tr_vars("board.open", &[("n", &open_cnt.to_string())]))
                            .size(11.0)
                            .color(MUTED),
                    );
                }
            });
            ui.add_space(6.0);

            // 2. Add sticky note form
            ui.horizontal(|ui| {
                let edit = ui.add(
                    egui::TextEdit::singleline(&mut state.whiteboard_note_draft)
                        .desired_width((ui.available_width() - 82.0).max(160.0))
                        .char_limit(140)
                        .hint_text(i18n.tr("board.addPlaceholder")),
                );
                let enter = edit.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter));
                let add_btn = egui::Button::new(
                    heading_bold(i18n.tr("board.add"), 11.5)
                        .color(Color32::from_rgb(0x2A, 0x13, 0x07)),
                )
                .fill(ACCENT)
                .corner_radius(CornerRadius::same(8));
                if (ui.add(add_btn).clicked() || enter)
                    && !state.whiteboard_note_draft.trim().is_empty()
                {
                    let draft = std::mem::take(&mut state.whiteboard_note_draft);
                    add_sticky_note(state, &draft);
                }
            });
            ui.add_space(8.0);

            egui::ScrollArea::vertical()
                .id_salt("whiteboard_modal_scroll")
                .max_height(430.0)
                .show(ui, |ui| {
                    // 3. Sticky notes list
                    let items = sorted_todos(&state.session_todos);
                    if items.is_empty() {
                        egui::Frame::NONE
                            .fill(CARD)
                            .corner_radius(CornerRadius::same(10))
                            .inner_margin(Margin::symmetric(12, 10))
                            .stroke(Stroke::new(1.0_f32, LINE))
                            .show(ui, |ui| {
                                ui.set_min_width(ui.available_width());
                                ui.label(
                                    RichText::new(i18n.tr("board.mineEmpty"))
                                        .size(11.5)
                                        .color(MUTED),
                                );
                            });
                    } else {
                        for (idx, item) in items.iter().enumerate() {
                            let row_id = if item.id.is_empty() {
                                format!("todo-{idx}")
                            } else {
                                item.id.clone()
                            };
                            ui.push_id(row_id, |ui| {
                                let bg = sticky_color(item);
                                let fg = if item.done { STICKY_DONE_INK } else { STICKY_INK };
                                egui::Frame::NONE
                                    .fill(bg)
                                    .corner_radius(CornerRadius::same(9))
                                    .inner_margin(Margin::symmetric(10, 7))
                                    .stroke(Stroke::new(
                                        1.0_f32,
                                        Color32::from_rgba_unmultiplied(0, 0, 0, 28),
                                    ))
                                    .show(ui, |ui| {
                                        ui.set_min_width(ui.available_width());
                                        ui.horizontal(|ui| {
                                            let check_lbl = if item.done { "☑" } else { "☐" };
                                            let check_hint = if item.done {
                                                i18n.tr("board.markUndone")
                                            } else {
                                                i18n.tr("board.markDone")
                                            };
                                            if ui
                                                .add(
                                                    egui::Button::new(
                                                        RichText::new(check_lbl)
                                                            .size(15.0)
                                                            .color(if item.done {
                                                                Color32::from_rgb(0x2E, 0x9E, 0x62)
                                                            } else {
                                                                fg
                                                            }),
                                                    )
                                                    .fill(Color32::TRANSPARENT)
                                                    .frame(false),
                                                )
                                                .on_hover_text(check_hint)
                                                .clicked()
                                            {
                                                toggle_sticky_note(state, &item.id);
                                            }

                                            let txt = if state.streamer_mode {
                                                i18n.tr("station.streamerRedacted")
                                            } else {
                                                item.text.clone()
                                            };
                                            let mut rt = RichText::new(&txt).size(12.0).color(fg);
                                            if item.done {
                                                rt = rt.strikethrough();
                                            }
                                            ui.label(rt);

                                            ui.with_layout(
                                                egui::Layout::right_to_left(egui::Align::Center),
                                                |ui| {
                                                    let del_hint = i18n.tr_vars(
                                                        "board.deleteTodo",
                                                        &[("text", &txt)],
                                                    );
                                                    if ui
                                                        .add(
                                                            egui::Button::new(
                                                                RichText::new("×")
                                                                    .size(13.0)
                                                                    .color(fg),
                                                            )
                                                            .fill(Color32::TRANSPARENT)
                                                            .frame(false),
                                                        )
                                                        .on_hover_text(del_hint)
                                                        .clicked()
                                                    {
                                                        delete_sticky_note(state, &item.id);
                                                    }
                                                },
                                            );
                                        });
                                    });
                                ui.add_space(5.0);
                            });
                        }

                        if done_cnt > 0
                            && ui
                                .button(i18n.tr_count("board.clearDone", done_cnt))
                                .clicked()
                        {
                            clear_done_sticky_notes(state);
                        }
                    }

                    ui.add_space(12.0);
                    ui.separator();
                    ui.add_space(8.0);

                    // 4. Agent tasks checklist synced from live sessions
                    let mut tasks: Vec<_> = state
                        .sessions
                        .iter()
                        .filter(|s| !s.last_prompt.trim().is_empty())
                        .cloned()
                        .collect();
                    tasks.sort_by_key(|s| match s.status.as_str() {
                        "wait" => 0u8,
                        "work" => 1u8,
                        _ => 2u8,
                    });

                    let doing_n = tasks.iter().filter(|s| s.status == "work").count();
                    let wait_n = tasks.iter().filter(|s| s.status == "wait").count();
                    let done_n = tasks.iter().filter(|s| s.status != "work" && s.status != "wait").count();
                    let mut sum_parts = Vec::new();
                    if doing_n > 0 {
                        sum_parts.push(i18n.tr_vars("board.sumDoing", &[("n", &doing_n.to_string())]));
                    }
                    if wait_n > 0 {
                        sum_parts.push(i18n.tr_vars("board.sumWait", &[("n", &wait_n.to_string())]));
                    }
                    if done_n > 0 {
                        sum_parts.push(i18n.tr_vars("board.sumDone", &[("n", &done_n.to_string())]));
                    }
                    let sum_str = if sum_parts.is_empty() {
                        i18n.tr("board.sumNone")
                    } else {
                        sum_parts.join(" · ")
                    };

                    ui.horizontal(|ui| {
                        ui.label(heading_bold(i18n.tr("board.agentTasks"), 12.5).color(INK));
                        ui.label(RichText::new(sum_str).size(11.0).color(MUTED));
                    });
                    ui.add_space(6.0);

                    if tasks.is_empty() {
                        egui::Frame::NONE
                            .fill(CARD)
                            .corner_radius(CornerRadius::same(10))
                            .inner_margin(Margin::symmetric(12, 10))
                            .stroke(Stroke::new(1.0_f32, LINE))
                            .show(ui, |ui| {
                                ui.set_min_width(ui.available_width());
                                ui.label(
                                    RichText::new(i18n.tr("board.agentEmpty"))
                                        .size(11.5)
                                        .color(MUTED),
                                );
                            });
                    } else {
                        for sess in tasks {
                            ui.push_id(&sess.id, |ui| {
                                let dot_col = match sess.status.as_str() {
                                    "wait" => RED,
                                    "work" => GREEN,
                                    _ => AMBER,
                                };
                                let frame = egui::Frame::NONE
                                    .fill(CARD_2)
                                    .corner_radius(CornerRadius::same(9))
                                    .inner_margin(Margin::symmetric(10, 7))
                                    .stroke(Stroke::new(1.0_f32, LINE))
                                    .show(ui, |ui| {
                                        ui.set_min_width(ui.available_width());
                                        ui.horizontal(|ui| {
                                            status_dot(ui, dot_col, 4.0);
                                            ui.label(
                                                heading_bold(&sess.project, 11.5).color(INK),
                                            );
                                            if sess.todos_total > 0 {
                                                ui.with_layout(
                                                    egui::Layout::right_to_left(egui::Align::Center),
                                                    |ui| {
                                                        ui.label(
                                                            RichText::new(format!(
                                                                "{}/{}",
                                                                sess.todos_done, sess.todos_total
                                                            ))
                                                            .size(10.5)
                                                            .color(GREEN),
                                                        );
                                                    },
                                                );
                                            }
                                        });
                                        let prompt = if state.streamer_mode {
                                            i18n.tr("station.streamerRedacted")
                                        } else {
                                            sess.last_prompt.clone()
                                        };
                                        ui.label(RichText::new(prompt).size(11.5).color(MUTED));
                                        if sess.todos_total > 0 {
                                            let ratio = (sess.todos_done as f32
                                                / sess.todos_total as f32)
                                                .clamp(0.0, 1.0);
                                            let (bar_r, _) = ui.allocate_exact_size(
                                                vec2(ui.available_width(), 4.0),
                                                egui::Sense::hover(),
                                            );
                                            ui.painter().rect_filled(bar_r, 2.0, BG);
                                            let mut f = bar_r;
                                            f.set_width(bar_r.width() * ratio);
                                            ui.painter().rect_filled(f, 2.0, GREEN);
                                        }
                                    });
                                if ui
                                    .interact(
                                        frame.response.rect,
                                        ui.id().with("open_sess"),
                                        egui::Sense::click(),
                                    )
                                    .clicked()
                                {
                                    state.selected_session = Some(sess.id.clone());
                                    state.active_drawer = ActiveDrawer::Session;
                                }
                                ui.add_space(5.0);
                            });
                        }
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

    #[test]
    fn whiteboard_sticky_note_add_toggle_delete_emits_persistence_payloads() {
        let mut st = UiState::default();
        assert!(!add_sticky_note(&mut st, "   "));
        assert!(add_sticky_note(&mut st, "Review PR #42"));
        assert_eq!(st.session_todos.len(), 1);
        let id = st.session_todos[0].id.clone();
        assert!(!id.is_empty());
        assert!(!st.session_todos[0].done);

        assert!(toggle_sticky_note(&mut st, &id));
        assert!(st.session_todos[0].done);
        assert!(st.whiteboard_text.contains("[x] Review PR #42"));

        assert!(add_sticky_note(&mut st, "Ship Task 6"));
        assert_eq!(st.session_todos.len(), 2);
        let id2 = st.session_todos[1].id.clone();

        assert!(delete_sticky_note(&mut st, &id2));
        assert_eq!(st.session_todos.len(), 1);

        assert_eq!(clear_done_sticky_notes(&mut st), 1);
        assert!(st.session_todos.is_empty());

        let actions = st.drain_actions();
        assert_eq!(actions.len(), 5);
        assert!(matches!(
            &actions[0],
            UiAction::SyncWhiteboardTodos(list) if list.len() == 1 && !list[0].done && list[0].text == "Review PR #42"
        ));
        assert!(matches!(
            &actions[1],
            UiAction::SyncWhiteboardTodos(list) if list.len() == 1 && list[0].done
        ));
        assert!(matches!(
            &actions[3],
            UiAction::SyncWhiteboardTodos(list) if list.len() == 1 && list[0].id == id
        ));
        assert!(matches!(
            &actions[4],
            UiAction::SyncWhiteboardTodos(list) if list.is_empty()
        ));
    }

    #[test]
    fn whiteboard_modal_renders_across_en_vi_th() {
        let mut st = UiState::with_demo_seed(
            station_3d::OfficeLayout::Pro,
            station_3d::BackdropKind::City,
            false,
        );
        st.active_drawer = ActiveDrawer::Whiteboard;

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
                draw_whiteboard_modal(ctx, &mut st, &tr);
            });
        }
    }
}
