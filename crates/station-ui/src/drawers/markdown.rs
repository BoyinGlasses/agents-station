use crate::theme::{ACCENT, BG, CARD_2, FONT_BOLD, INK, LINE, MUTED, heading_bold};
use bevy_egui::egui::{
    self, Color32, CornerRadius, FontFamily, FontId, Margin, RichText, Stroke, TextFormat,
    text::LayoutJob,
};
use pulldown_cmark::{CodeBlockKind, Event, HeadingLevel, Options, Parser, Tag, TagEnd};

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum MdBlock {
    Heading { level: u8, text: String },
    CodeBlock { lang: String, code: String },
    Quote(String),
    ListItem { ordered: bool, index: u64, text: String },
    Paragraph(Vec<MdSpan>),
    Rule,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum MdSpan {
    Text(String),
    Bold(String),
    Italic(String),
    Code(String),
}

fn heading_level_u8(level: HeadingLevel) -> u8 {
    match level {
        HeadingLevel::H1 => 1,
        HeadingLevel::H2 => 2,
        HeadingLevel::H3 => 3,
        HeadingLevel::H4 => 4,
        HeadingLevel::H5 => 5,
        HeadingLevel::H6 => 6,
    }
}

/// Parses markdown into lightweight display blocks using `pulldown-cmark`.
pub fn parse_markdown_blocks(src: &str) -> Vec<MdBlock> {
    let mut opts = Options::empty();
    opts.insert(Options::ENABLE_STRIKETHROUGH);
    opts.insert(Options::ENABLE_TASKLISTS);
    let parser = Parser::new_ext(src, opts);

    let mut blocks = Vec::new();
    let mut in_heading: Option<u8> = None;
    let mut heading_buf = String::new();
    let mut in_code_lang: Option<String> = None;
    let mut code_buf = String::new();
    let mut in_quote = false;
    let mut quote_buf = String::new();
    let mut list_stack: Vec<(bool, u64)> = Vec::new();
    let mut in_item = false;
    let mut item_buf = String::new();
    let mut para_spans: Vec<MdSpan> = Vec::new();
    let mut bold_depth = 0usize;
    let mut italic_depth = 0usize;

    let flush_para = |spans: &mut Vec<MdSpan>, out: &mut Vec<MdBlock>| {
        if !spans.is_empty() {
            out.push(MdBlock::Paragraph(std::mem::take(spans)));
        }
    };

    for ev in parser {
        match ev {
            Event::Start(Tag::Heading { level, .. }) => {
                flush_para(&mut para_spans, &mut blocks);
                in_heading = Some(heading_level_u8(level));
                heading_buf.clear();
            }
            Event::End(TagEnd::Heading(_)) => {
                if let Some(level) = in_heading.take() {
                    let text = heading_buf.trim().to_owned();
                    if !text.is_empty() {
                        blocks.push(MdBlock::Heading { level, text });
                    }
                }
                heading_buf.clear();
            }
            Event::Start(Tag::CodeBlock(kind)) => {
                flush_para(&mut para_spans, &mut blocks);
                let lang = match kind {
                    CodeBlockKind::Fenced(l) => l.trim().to_owned(),
                    CodeBlockKind::Indented => String::new(),
                };
                in_code_lang = Some(lang);
                code_buf.clear();
            }
            Event::End(TagEnd::CodeBlock) => {
                if let Some(lang) = in_code_lang.take() {
                    let code = code_buf.trim_end_matches('\n').to_owned();
                    blocks.push(MdBlock::CodeBlock { lang, code });
                }
                code_buf.clear();
            }
            Event::Start(Tag::BlockQuote(_)) => {
                flush_para(&mut para_spans, &mut blocks);
                in_quote = true;
                quote_buf.clear();
            }
            Event::End(TagEnd::BlockQuote(_)) => {
                in_quote = false;
                let text = quote_buf.trim().to_owned();
                if !text.is_empty() {
                    blocks.push(MdBlock::Quote(text));
                }
                quote_buf.clear();
            }
            Event::Start(Tag::List(first)) => {
                flush_para(&mut para_spans, &mut blocks);
                match first {
                    Some(start) => list_stack.push((true, start)),
                    None => list_stack.push((false, 0)),
                }
            }
            Event::End(TagEnd::List(_)) => {
                list_stack.pop();
            }
            Event::Start(Tag::Item) => {
                in_item = true;
                item_buf.clear();
            }
            Event::End(TagEnd::Item) => {
                in_item = false;
                let text = item_buf.trim().to_owned();
                if !text.is_empty() {
                    let (ordered, index) = if let Some((ord, idx)) = list_stack.last_mut() {
                        let cur = *idx;
                        if *ord {
                            *idx = idx.saturating_add(1);
                        }
                        (*ord, cur)
                    } else {
                        (false, 0)
                    };
                    blocks.push(MdBlock::ListItem {
                        ordered,
                        index,
                        text,
                    });
                }
                item_buf.clear();
            }
            Event::TaskListMarker(checked) => {
                if in_item {
                    item_buf.push_str(if checked { "[x] " } else { "[ ] " });
                }
            }
            Event::Start(Tag::Paragraph) => {
                if !in_quote && !in_item {
                    para_spans.clear();
                }
            }
            Event::End(TagEnd::Paragraph) => {
                if in_quote {
                    if !quote_buf.is_empty() && !quote_buf.ends_with('\n') {
                        quote_buf.push('\n');
                    }
                } else if !in_item {
                    flush_para(&mut para_spans, &mut blocks);
                }
            }
            Event::Start(Tag::Strong) => bold_depth += 1,
            Event::End(TagEnd::Strong) => bold_depth = bold_depth.saturating_sub(1),
            Event::Start(Tag::Emphasis) => italic_depth += 1,
            Event::End(TagEnd::Emphasis) => italic_depth = italic_depth.saturating_sub(1),
            Event::Code(c) => {
                if in_heading.is_some() {
                    heading_buf.push_str(&c);
                } else if in_quote {
                    quote_buf.push('`');
                    quote_buf.push_str(&c);
                    quote_buf.push('`');
                } else if in_item {
                    item_buf.push('`');
                    item_buf.push_str(&c);
                    item_buf.push('`');
                } else {
                    para_spans.push(MdSpan::Code(c.into_string()));
                }
            }
            Event::Text(t) => {
                if in_code_lang.is_some() {
                    code_buf.push_str(&t);
                } else if in_heading.is_some() {
                    heading_buf.push_str(&t);
                } else if in_quote {
                    quote_buf.push_str(&t);
                } else if in_item {
                    item_buf.push_str(&t);
                } else if bold_depth > 0 {
                    para_spans.push(MdSpan::Bold(t.into_string()));
                } else if italic_depth > 0 {
                    para_spans.push(MdSpan::Italic(t.into_string()));
                } else {
                    para_spans.push(MdSpan::Text(t.into_string()));
                }
            }
            Event::SoftBreak => {
                if in_code_lang.is_some() {
                    code_buf.push('\n');
                } else if in_quote {
                    quote_buf.push(' ');
                } else if in_item {
                    item_buf.push(' ');
                } else if in_heading.is_some() {
                    heading_buf.push(' ');
                } else {
                    para_spans.push(MdSpan::Text(" ".to_owned()));
                }
            }
            Event::HardBreak => {
                if in_code_lang.is_some() {
                    code_buf.push('\n');
                } else if in_quote {
                    quote_buf.push('\n');
                } else if in_item {
                    item_buf.push('\n');
                } else {
                    para_spans.push(MdSpan::Text("\n".to_owned()));
                }
            }
            Event::Rule => {
                flush_para(&mut para_spans, &mut blocks);
                blocks.push(MdBlock::Rule);
            }
            _ => {}
        }
    }

    flush_para(&mut para_spans, &mut blocks);
    blocks
}

/// Renders parsed markdown cleanly into an `egui::Ui`.
pub fn render_markdown(ui: &mut egui::Ui, src: &str) {
    let blocks = parse_markdown_blocks(src);
    if blocks.is_empty() && !src.trim().is_empty() {
        ui.label(RichText::new(src.trim()).size(11.5).color(INK));
        return;
    }

    for (idx, block) in blocks.into_iter().enumerate() {
        if idx > 0 {
            ui.add_space(4.0);
        }
        match block {
            MdBlock::Heading { level, text } => {
                let sz = match level {
                    1 => 14.0,
                    2 => 13.0,
                    _ => 12.0,
                };
                ui.label(heading_bold(text, sz).color(INK));
            }
            MdBlock::CodeBlock { lang, code } => {
                egui::Frame::NONE
                    .fill(BG)
                    .corner_radius(CornerRadius::same(8))
                    .inner_margin(Margin::symmetric(9, 7))
                    .stroke(Stroke::new(1.0_f32, LINE))
                    .show(ui, |ui| {
                        ui.set_min_width(ui.available_width());
                        if !lang.is_empty() {
                            ui.label(
                                RichText::new(lang)
                                    .font(FontId::new(9.5, FontFamily::Monospace))
                                    .color(MUTED),
                            );
                            ui.add_space(2.0);
                        }
                        ui.label(
                            RichText::new(code)
                                .font(FontId::new(11.0, FontFamily::Monospace))
                                .color(INK),
                        );
                    });
            }
            MdBlock::Quote(text) => {
                egui::Frame::NONE
                    .fill(Color32::from_rgba_unmultiplied(0xEF, 0x8A, 0x5F, 14))
                    .corner_radius(CornerRadius::same(6))
                    .inner_margin(Margin::symmetric(9, 6))
                    .stroke(Stroke::new(1.0_f32, ACCENT))
                    .show(ui, |ui| {
                        ui.label(RichText::new(text).size(11.2).italics().color(MUTED));
                    });
            }
            MdBlock::ListItem {
                ordered,
                index,
                text,
            } => {
                ui.horizontal_top(|ui| {
                    ui.spacing_mut().item_spacing.x = 6.0;
                    let bullet = if ordered {
                        format!("{index}.")
                    } else {
                        "•".to_owned()
                    };
                    ui.label(RichText::new(bullet).size(11.5).color(ACCENT));
                    ui.label(RichText::new(text).size(11.5).color(INK));
                });
            }
            MdBlock::Paragraph(spans) => {
                let mut job = LayoutJob::default();
                let normal_fmt = TextFormat {
                    font_id: FontId::new(11.5, FontFamily::Proportional),
                    color: INK,
                    ..Default::default()
                };
                let bold_fmt = TextFormat {
                    font_id: FontId::new(11.5, FontFamily::Name(FONT_BOLD.into())),
                    color: INK,
                    ..Default::default()
                };
                let italic_fmt = TextFormat {
                    font_id: FontId::new(11.5, FontFamily::Proportional),
                    italics: true,
                    color: INK,
                    ..Default::default()
                };
                let code_fmt = TextFormat {
                    font_id: FontId::new(11.0, FontFamily::Monospace),
                    color: INK,
                    background: CARD_2,
                    ..Default::default()
                };
                for sp in spans {
                    match sp {
                        MdSpan::Text(s) => job.append(&s, 0.0, normal_fmt.clone()),
                        MdSpan::Bold(s) => job.append(&s, 0.0, bold_fmt.clone()),
                        MdSpan::Italic(s) => job.append(&s, 0.0, italic_fmt.clone()),
                        MdSpan::Code(s) => job.append(&format!(" {s} "), 0.0, code_fmt.clone()),
                    }
                }
                ui.label(job);
            }
            MdBlock::Rule => {
                let (rect, _) = ui.allocate_exact_size(
                    egui::vec2(ui.available_width(), 1.0),
                    egui::Sense::hover(),
                );
                ui.painter().rect_filled(rect, CornerRadius::ZERO, LINE);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_headings_code_blocks_inline_code_lists_bold_italic_and_quotes() {
        let md = "# Plan Title\n\nUse **bold**, *italic*, and `inline_code`.\n\n- item one\n- item two\n\n> quoted note\n\n```rust\nfn main() {}\n```";
        let blocks = parse_markdown_blocks(md);
        assert!(
            blocks
                .iter()
                .any(|b| matches!(b, MdBlock::Heading { level: 1, text } if text == "Plan Title")),
            "expected heading block, got {blocks:?}"
        );
        assert!(
            blocks
                .iter()
                .any(|b| matches!(b, MdBlock::CodeBlock { lang, code } if lang == "rust" && code.contains("fn main()"))),
            "expected rust fenced code block, got {blocks:?}"
        );
        assert!(
            blocks
                .iter()
                .any(|b| matches!(b, MdBlock::Quote(q) if q.contains("quoted note"))),
            "expected blockquote, got {blocks:?}"
        );
        assert_eq!(
            blocks
                .iter()
                .filter(|b| matches!(b, MdBlock::ListItem { .. }))
                .count(),
            2,
            "expected 2 list items, got {blocks:?}"
        );
        let para_spans: Vec<&MdSpan> = blocks
            .iter()
            .filter_map(|b| match b {
                MdBlock::Paragraph(spans) => Some(spans),
                _ => None,
            })
            .flatten()
            .collect();
        assert!(para_spans.contains(&&MdSpan::Bold("bold".to_owned())));
        assert!(para_spans.contains(&&MdSpan::Italic("italic".to_owned())));
        assert!(para_spans.contains(&&MdSpan::Code("inline_code".to_owned())));
    }
}
