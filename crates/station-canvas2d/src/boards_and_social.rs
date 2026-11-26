use crate::canvas::{Canvas2d, RgbaSurface, hex, hex_rgba};
use crate::text::{FontEngine, TextAlign};

#[derive(Debug, Clone)]
pub struct WhiteboardTask {
    pub name: String,
    pub color: [u8; 4],
    pub state: String, // "doing" | "wait" | "done"
    pub prompt: String,
    pub todos_done: usize,
    pub todos_total: usize,
}

#[derive(Debug, Clone)]
pub struct WhiteboardInput {
    pub title: String,
    pub yours_title: String,
    pub streamer: bool,
    pub tasks: Vec<WhiteboardTask>,
    pub open_todos: usize,
    pub total_todos: usize,
}

fn draw_board_text(
    c: &mut Canvas2d,
    fonts: &mut FontEngine,
    text: &str,
    x: f32,
    y: f32,
    size_px: f32,
    max_w: f32,
    color: [u8; 4],
    bold: bool,
    streamer: bool,
) -> f32 {
    let fitted = fonts.truncate_to_width(text, size_px, max_w, bold, false);
    if !streamer {
        return fonts.draw_text(c, &fitted, x, y, size_px, color, TextAlign::Left, None, bold, false);
    }
    let w = fonts.measure_width(&fitted, size_px, bold, false).min(max_w).max(24.0);
    let h = (size_px * 0.55).max(8.0);
    c.round_rect(x, y + size_px * 0.25, w, h, h * 0.5, [120, 128, 142, 90]);
    w
}

/// Exact Oracle `src/office/ui/whiteboard.js` 1024×512 whiteboard surface.
pub fn draw_whiteboard(input: &WhiteboardInput, fonts: &mut FontEngine) -> RgbaSurface {
    const W: u32 = 1024;
    const H: u32 = 512;
    let mut c = Canvas2d::new(W, H);
    c.fill(hex("#FBFBF7"));
    for i in 0..9 {
        c.fill_rect(0.0, 40.0 + (i as f32) * 52.0, W as f32, 1.0, [43, 58, 85, 14]);
    }

    let doing = input.tasks.iter().filter(|t| t.state == "doing").count();
    let wait = input.tasks.iter().filter(|t| t.state == "wait").count();
    let done = input.tasks.iter().filter(|t| t.state == "done").count();

    fonts.draw_text(&mut c, &input.title.to_uppercase(), 40.0, 26.0, 44.0, hex("#22304A"), TextAlign::Left, Some(540.0), true, false);
    let mut parts = Vec::new();
    if doing > 0 {
        parts.push(format!("{doing} doing"));
    }
    if wait > 0 {
        parts.push(format!("{wait} waiting"));
    }
    if done > 0 {
        parts.push(format!("{done} done"));
    }
    let summary = if parts.is_empty() {
        "No active tasks yet".to_owned()
    } else {
        parts.join(" · ")
    };
    fonts.draw_text(&mut c, &summary, 40.0, 82.0, 22.0, hex("#6B7486"), TextAlign::Left, Some(540.0), false, false);

    if input.tasks.is_empty() {
        fonts.draw_text(&mut c, "Send a prompt to any agent", 40.0, 148.0, 24.0, hex("#8A93A3"), TextAlign::Left, None, false, false);
        fonts.draw_text(&mut c, "to see live tasks appear here.", 40.0, 182.0, 24.0, hex("#8A93A3"), TextAlign::Left, None, false, false);
    } else {
        for (i, t) in input.tasks.iter().take(5).enumerate() {
            let y = 132.0 + (i as f32) * 72.0;
            let box_col = match t.state.as_str() {
                "done" => hex("#2E9E62"),
                "wait" => hex("#E0912A"),
                _ => hex("#22304A"),
            };
            c.stroke_round_rect(40.0, y + 6.0, 32.0, 32.0, 7.0, 3.5, box_col);
            match t.state.as_str() {
                "done" => {
                    c.round_rect(40.0, y + 6.0, 32.0, 32.0, 7.0, hex("#2E9E62"));
                    c.stroke_line(47.0, y + 22.0, 54.0, y + 30.0, 4.0, hex("#FFFFFF"));
                    c.stroke_line(54.0, y + 30.0, 66.0, y + 14.0, 4.0, hex("#FFFFFF"));
                }
                "wait" => {
                    fonts.draw_text(&mut c, "!", 56.0, y + 8.0, 24.0, hex("#E0912A"), TextAlign::Center, None, true, false);
                }
                _ => {
                    c.fill_circle(56.0, y + 22.0, 6.5, hex("#E8730E"));
                }
            }

            draw_board_text(&mut c, fonts, &t.name.to_uppercase(), 88.0, y, 19.0, 330.0, t.color, true, input.streamer);
            if t.todos_total > 0 {
                let lab = format!("{}/{}", t.todos_done, t.todos_total);
                let lw = fonts.measure_width(&lab, 18.0, true, false);
                fonts.draw_text(&mut c, &lab, 576.0, y, 18.0, hex("#6B7486"), TextAlign::Right, None, true, false);
                let bw = 70.0;
                let bx = 576.0 - lw - bw - 10.0;
                c.fill_rect(bx, y + 6.0, bw, 9.0, hex("#E3E6EC"));
                let ratio = (t.todos_done as f32 / t.todos_total as f32).clamp(0.0, 1.0);
                c.fill_rect(bx, y + 6.0, bw * ratio, 9.0, hex("#2E9E62"));
            }

            let pcol = if t.state == "done" { hex("#8A93A3") } else { hex("#22304A") };
            let pw = draw_board_text(&mut c, fonts, &t.prompt, 88.0, y + 26.0, 25.0, 488.0, pcol, false, input.streamer);
            if t.state == "done" && !input.streamer {
                c.fill_rect(88.0, y + 39.0, pw, 2.5, hex("#8A93A3"));
            }
        }
    }

    // Right half: sticky notes section
    c.stroke_line(600.0, 34.0, 600.0, 486.0, 2.0, [43, 58, 85, 32]);
    fonts.draw_text(&mut c, &input.yours_title.to_uppercase(), 626.0, 44.0, 26.0, hex("#22304A"), TextAlign::Left, Some(360.0), true, false);
    let sub = if input.total_todos > 0 {
        format!("{} open", input.open_todos)
    } else {
        "Click to add a note".to_owned()
    };
    fonts.draw_text(&mut c, &sub, 626.0, 80.0, 20.0, hex("#6B7486"), TextAlign::Left, None, false, false);

    // Add-note dashed slot
    c.stroke_round_rect(646.0, 146.0, 118.0, 118.0, 6.0, 3.0, [43, 58, 85, 90]);
    fonts.draw_text(&mut c, "+", 705.0, 172.0, 40.0, [43, 58, 85, 130], TextAlign::Center, None, true, false);
    fonts.draw_text(&mut c, "Add note", 705.0, 224.0, 15.0, [43, 58, 85, 130], TextAlign::Center, None, false, false);

    c.into_surface()
}

pub fn draw_sticky_note(
    text: &str,
    done: bool,
    color: [u8; 4],
    streamer: bool,
    fonts: &mut FontEngine,
) -> RgbaSurface {
    let mut c = Canvas2d::new(192, 192);
    c.fill(color);
    c.fill_rect(0.0, 0.0, 192.0, 22.0, [0, 0, 0, 16]);
    let fg = if done { [40, 40, 40, 115] } else { hex("#2E2B26") };

    let mut lines: Vec<String> = Vec::new();
    let mut cur = String::new();
    for word in text.split_whitespace() {
        let cand = if cur.is_empty() {
            word.to_owned()
        } else {
            format!("{cur} {word}")
        };
        if fonts.measure_width(&cand, 22.0, false, false) > 164.0 && !cur.is_empty() {
            lines.push(cur);
            cur = word.to_owned();
        } else {
            cur = cand;
        }
    }
    if !cur.is_empty() {
        lines.push(cur);
    }

    for (i, line) in lines.iter().take(5).enumerate() {
        let y = 32.0 + (i as f32) * 28.0;
        let w = draw_board_text(&mut c, fonts, line, 14.0, y, 22.0, 164.0, fg, false, streamer);
        if done && !streamer {
            c.fill_rect(14.0, y + 12.0, w, 2.5, fg);
        }
    }
    if done {
        c.stroke_line(132.0, 160.0, 148.0, 176.0, 6.5, hex("#2E9E62"));
        c.stroke_line(148.0, 176.0, 178.0, 140.0, 6.5, hex("#2E9E62"));
    }
    c.into_surface()
}

#[derive(Debug, Clone)]
pub struct DutyRowInput {
    pub chore_key: String, // "cups" | "trash" | "plants" | "table"
    pub chore_label: String,
    pub person: String,
    pub done_at: Option<String>,
    pub busy: bool,
}

#[derive(Debug, Clone)]
pub struct DutyBoardInput {
    pub title: String,
    pub date: String,
    pub off_label: Option<String>,
    pub rows: Vec<DutyRowInput>,
    pub owed_footer: Option<String>,
}

/// Exact Oracle `src/office/world/props/duty-board.js` 512×349 pantry whiteboard.
pub fn draw_duty_board(input: &DutyBoardInput, fonts: &mut FontEngine) -> RgbaSurface {
    const W: u32 = 512;
    const H: u32 = 349;
    let mut c = Canvas2d::new(W, H);
    c.fill(hex("#FBFBF8"));
    for i in 0..9 {
        c.fill_rect(0.0, 60.0 + (i as f32) * 34.0, W as f32, 1.0, [120, 140, 160, 22]);
    }

    fonts.draw_text(&mut c, &input.title, 22.0, 16.0, 32.0, hex("#C8261B"), TextAlign::Left, Some(300.0), true, false);
    fonts.draw_text(&mut c, &input.date, W as f32 - 20.0, 22.0, 22.0, hex("#1F4E9E"), TextAlign::Right, None, true, false);
    c.stroke_line(22.0, 58.0, W as f32 - 20.0, 58.0, 3.0, hex("#C8261B"));

    if let Some(off) = &input.off_label {
        fonts.draw_text(&mut c, off, W as f32 * 0.5, 134.0, 44.0, hex("#2E9E4A"), TextAlign::Center, Some(460.0), true, false);
        fonts.draw_text(&mut c, "Nghỉ trực nhật hôm nay", W as f32 * 0.5, 198.0, 22.0, hex("#5E6670"), TextAlign::Center, None, false, false);
        return c.into_surface();
    }

    for (i, r) in input.rows.iter().take(4).enumerate() {
        let y = 84.0 + (i as f32) * 52.0;
        let icon_col = match r.chore_key.as_str() {
            "cups" => hex("#2E6FD1"),
            "trash" => hex("#3A3F47"),
            "plants" => hex("#2E9E4A"),
            _ => hex("#C2577A"),
        };
        c.round_rect(28.0, y + 3.5, 18.0, 18.0, 4.0, icon_col);
        fonts.draw_text(&mut c, &r.chore_label, 58.0, y, 24.0, hex("#1E232B"), TextAlign::Left, Some(190.0), true, false);
        let pcol = if r.done_at.is_some() { hex("#7A8494") } else { hex("#1F4E9E") };
        let pname = if r.person.is_empty() { "—" } else { &r.person };
        fonts.draw_text(&mut c, pname, 260.0, y, 24.0, pcol, TextAlign::Left, Some(140.0), true, false);
        if let Some(at) = &r.done_at {
            let done_w = fonts.draw_text(&mut c, &format!("✓ {at}"), W as f32 - 20.0, y, 24.0, hex("#2E9E4A"), TextAlign::Right, None, true, false);
            let strike_y = y + 13.0;
            let strike_end = (W as f32 - 20.0 - done_w - 14.0).clamp(260.0, 396.0);
            c.stroke_line(58.0, strike_y, strike_end, strike_y, 2.0, [46, 158, 74, 140]);
        } else if r.busy {
            fonts.draw_text(&mut c, "Đang làm", W as f32 - 20.0, y + 3.0, 19.0, hex("#E8742A"), TextAlign::Right, None, true, false);
        }
    }

    if let Some(owed) = &input.owed_footer {
        fonts.draw_text(&mut c, owed, 22.0, H as f32 - 36.0, 19.0, hex("#C8261B"), TextAlign::Left, Some(W as f32 - 44.0), true, false);
    }

    c.into_surface()
}

#[derive(Debug, Clone)]
pub struct StripPerson {
    pub hair: [u8; 4],
    pub skin: [u8; 4],
    pub shirt: [u8; 4],
}

#[derive(Debug, Clone)]
pub struct PhotostripInput {
    pub tag: String,
    pub day: String,
    pub seed: usize,
    pub people: Vec<StripPerson>,
}

/// Exact Oracle `src/office/life/photostrips.js` 128×384 Korean 4-frame photostrip.
pub fn draw_photostrip(input: &PhotostripInput, fonts: &mut FontEngine) -> RgbaSurface {
    const W: u32 = 128;
    const H: u32 = 384;
    let mut c = Canvas2d::new(W, H);
    c.fill(hex("#FFFFFF"));

    let bgs = [hex("#FFD6E4"), hex("#D4F2E4"), hex("#E3DAFF"), hex("#FFF0B8")];
    let pad = W as f32 * 0.06;
    let fh = (H as f32 - pad * 5.0 - W as f32 * 0.42) / 4.0;
    let fw = W as f32 - pad * 2.0;

    for f in 0..4 {
        let fy = pad + (f as f32) * (fh + pad);
        c.fill_rect(pad, fy, fw, fh, bgs[(f + input.seed) % 4]);
        for i in 0..6 {
            let bx = pad + ((i * 37 + f * 19 + input.seed * 7) % fw.max(1.0) as usize) as f32;
            let by = fy + ((i * 23 + f * 11) % fh.max(1.0) as usize) as f32;
            c.fill_circle(bx, by, W as f32 * 0.025, [255, 255, 255, 140]);
        }
        let n = input.people.len().clamp(1, 4);
        let r = (fw / (n as f32 * 2.6)).min(fh * 0.22);
        for (k, p) in input.people.iter().take(4).enumerate() {
            let hx = pad + (fw * (k as f32 + 0.5)) / n as f32;
            let hy = fy + fh * 0.56;
            c.fill_ellipse(hx, hy + r * 1.45, r * 1.0, r * 0.65, p.shirt);
            c.fill_circle(hx, hy, r, p.skin);
            c.fill_ellipse(hx, hy - r * 0.55, r * 0.95, r * 0.45, p.hair);
            c.fill_circle(hx - r * 0.35, hy, (r * 0.12).max(1.2), hex("#2B2420"));
            c.fill_circle(hx + r * 0.35, hy, (r * 0.12).max(1.2), hex("#2B2420"));
            if f == 0 {
                let ear_col = [hex("#FF8FB8"), hex("#1E2125"), hex("#F4F4F2"), hex("#FFD84A")][k % 4];
                c.fill_polygon(&[(hx - r * 0.85, hy - r * 0.8), (hx - r * 0.55, hy - r * 1.5), (hx - r * 0.25, hy - r * 0.85)], ear_col);
                c.fill_polygon(&[(hx + r * 0.85, hy - r * 0.8), (hx + r * 0.55, hy - r * 1.5), (hx + r * 0.25, hy - r * 0.85)], ear_col);
            } else if f == 1 {
                let sg = [hex("#E8327A"), hex("#FF7A1A"), hex("#7A4FD8"), hex("#18A0C8")][k % 4];
                c.fill_circle(hx - r * 0.4, hy, r * 0.36, sg);
                c.fill_circle(hx + r * 0.4, hy, r * 0.36, sg);
            } else if f == 2 {
                let hat = [hex("#3FB6E8"), hex("#FF6B9A"), hex("#FFC93C"), hex("#5DD39E")][k % 4];
                c.fill_polygon(&[(hx - r * 0.55, hy - r * 0.8), (hx + r * 0.1, hy - r * 1.9), (hx + r * 0.6, hy - r * 0.75)], hat);
            } else if k == input.seed % 4 {
                c.fill_polygon(&[(hx - r * 0.5, hy - r * 0.9), (hx, hy - r * 1.55), (hx + r * 0.5, hy - r * 0.9)], hex("#FFC93C"));
            }
        }
    }

    let foot_y = H as f32 - W as f32 * 0.42;
    fonts.draw_text(&mut c, &input.tag, W as f32 * 0.5, foot_y + 4.0, 20.0, hex("#E8327A"), TextAlign::Center, Some(W as f32 - 14.0), true, false);
    fonts.draw_text(&mut c, &format!("{} · 4 khung", input.day), W as f32 * 0.5, foot_y + 26.0, 10.0, hex("#7A6A72"), TextAlign::Center, Some(W as f32 - 12.0), false, false);
    fonts.draw_text(&mut c, "CHỤP ẢNH HÀN QUỐC", W as f32 * 0.5, foot_y + 39.0, 9.0, hex("#B03A6A"), TextAlign::Center, Some(W as f32 - 12.0), true, false);
    c.into_surface()
}

/// Zalo-style circular chat avatar with initials or Claude's `✳` mark.
pub fn draw_chat_avatar_initials(
    name: &str,
    key: &str,
    size: u32,
    fonts: &mut FontEngine,
) -> RgbaSurface {
    let s = size.max(16);
    let mut c = Canvas2d::new(s, s);
    let (bg_a, bg_b, label) = if key == "__claude" {
        (hex("#D97757"), hex("#E8A07A"), "✳".to_owned())
    } else if key == "me" {
        (hex("#2B3A55"), hex("#4A6491"), initials(name))
    } else {
        let pal = [
            hex("#4F8EF7"),
            hex("#F2994A"),
            hex("#27AE60"),
            hex("#9B51E0"),
            hex("#EB5757"),
            hex("#2D9CDB"),
            hex("#F2C94C"),
            hex("#219653"),
        ];
        let hash = name.bytes().fold(0usize, |a, b| a.wrapping_mul(31).wrapping_add(b as usize));
        let col = pal[hash % pal.len()];
        (col, hex_rgba("#1E222E", 255), initials(name))
    };

    c.linear_gradient_rect(0.0, 0.0, s as f32, s as f32, (0.0, 0.0), (s as f32, s as f32), &[(0.0, bg_a), (1.0, bg_b)]);
    let fs = if key == "__claude" { s as f32 * 0.5 } else { s as f32 * 0.38 };
    fonts.draw_text(&mut c, &label, s as f32 * 0.5, (s as f32 - fs) * 0.45, fs, hex("#FFFFFF"), TextAlign::Center, None, true, false);
    c.into_surface()
}

fn initials(name: &str) -> String {
    let words: Vec<&str> = name.split_whitespace().collect();
    let tail = if words.len() > 2 { &words[words.len() - 2..] } else { &words };
    let out: String = tail
        .iter()
        .filter_map(|w| w.chars().next())
        .collect::<String>()
        .to_uppercase();
    if out.is_empty() { "?".to_owned() } else { out }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    fn fonts() -> FontEngine {
        FontEngine::from_assets_dir(&PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../assets"))
    }

    #[test]
    fn whiteboard_stickies_duty_board_photostrip_and_avatars_render_cleanly() {
        let mut f = fonts();
        let wb_normal = draw_whiteboard(
            &WhiteboardInput {
                title: "Whiteboard".to_owned(),
                yours_title: "Yours".to_owned(),
                streamer: false,
                tasks: vec![
                    WhiteboardTask {
                        name: "Claude".to_owned(),
                        color: hex("#D97757"),
                        state: "doing".to_owned(),
                        prompt: "Tối ưu RAM và xây dựng station-canvas2d với tiếng Việt".to_owned(),
                        todos_done: 3,
                        todos_total: 5,
                    },
                    WhiteboardTask {
                        name: "Antigravity".to_owned(),
                        color: hex("#5E9BF0"),
                        state: "done".to_owned(),
                        prompt: "Certify G1 backend soak".to_owned(),
                        todos_done: 2,
                        todos_total: 2,
                    },
                ],
                open_todos: 2,
                total_todos: 3,
            },
            &mut f,
        );
        let wb_stream = draw_whiteboard(
            &WhiteboardInput {
                title: "Whiteboard".to_owned(),
                yours_title: "Yours".to_owned(),
                streamer: true,
                tasks: vec![WhiteboardTask {
                    name: "Secret".to_owned(),
                    color: hex("#D97757"),
                    state: "wait".to_owned(),
                    prompt: "Top secret prompt".to_owned(),
                    todos_done: 0,
                    todos_total: 0,
                }],
                open_todos: 0,
                total_todos: 0,
            },
            &mut f,
        );
        assert_eq!((wb_normal.width, wb_normal.height), (1024, 512));
        assert_ne!(wb_normal.rgba, wb_stream.rgba);

        let sticky = draw_sticky_note("Mua cà phê sữa đá cho team", true, hex("#FFE787"), false, &mut f);
        assert_eq!((sticky.width, sticky.height), (192, 192));

        let duty = draw_duty_board(
            &DutyBoardInput {
                title: "TRỰC NHẬT".to_owned(),
                date: "25/11".to_owned(),
                off_label: None,
                rows: vec![
                    DutyRowInput { chore_key: "cups".into(), chore_label: "Rửa ly".into(), person: "Mai".into(), done_at: Some("11:30".into()), busy: false },
                    DutyRowInput { chore_key: "trash".into(), chore_label: "Đổ rác".into(), person: "Long".into(), done_at: None, busy: true },
                ],
                owed_footer: Some("Nợ hôm qua: Tưới cây (An)".into()),
            },
            &mut f,
        );
        assert_eq!((duty.width, duty.height), (512, 349));

        let strip = draw_photostrip(
            &PhotostripInput {
                tag: "v1.0.0".into(),
                day: "25/11".into(),
                seed: 2,
                people: vec![
                    StripPerson { hair: hex("#3B2618"), skin: hex("#F5D0B5"), shirt: hex("#E8865F") },
                    StripPerson { hair: hex("#1E1E24"), skin: hex("#EBC3A0"), shirt: hex("#5F8FD8") },
                ],
            },
            &mut f,
        );
        assert_eq!((strip.width, strip.height), (128, 384));

        assert_eq!(draw_chat_avatar_initials("Minh Anh", "p1", 96, &mut f).width, 96);
        assert_eq!(draw_chat_avatar_initials("Claude", "__claude", 96, &mut f).width, 96);
    }

    #[test]
    fn duty_board_strikethrough_centers_vertically_and_clears_checkmark() {
        let mut f = fonts();
        let mut make_board = |done_at: Option<&str>| {
            draw_duty_board(
                &DutyBoardInput {
                    title: "TRỰC NHẬT".to_owned(),
                    date: "25/11".to_owned(),
                    off_label: None,
                    rows: vec![DutyRowInput {
                        chore_key: "cups".into(),
                        chore_label: "Rửa ly".into(),
                        person: "Mai".into(),
                        done_at: done_at.map(str::to_owned),
                        busy: false,
                    }],
                    owed_footer: None,
                },
                &mut f,
            )
        };

        let open = make_board(None);
        let done = make_board(Some("11:30"));

        let pixel = |surf: &RgbaSurface, x: u32, y: u32| -> [u8; 4] {
            let i = ((y * surf.width + x) * 4) as usize;
            [surf.rgba[i], surf.rgba[i + 1], surf.rgba[i + 2], surf.rgba[i + 3]]
        };

        let is_green_ink = |p: [u8; 4], min_delta: i32| -> bool {
            let (r, g, b) = (p[0] as i32, p[1] as i32, p[2] as i32);
            g - r > min_delta && g - b > min_delta.min(10)
        };

        // Measure actual vertical ink bounds of "Mai" (blue text on white board) in row 0.
        let mut text_min_y = u32::MAX;
        let mut text_max_y = 0u32;
        for y in 80..120 {
            for x in 260..340 {
                let p = pixel(&open, x, y);
                // Blue text `#1F4E9E` has low red channel compared to `#FBFBF8` background.
                if (p[0] as i32) < 180 && (p[2] as i32) - (p[0] as i32) > 20 {
                    text_min_y = text_min_y.min(y);
                    text_max_y = text_max_y.max(y);
                }
            }
        }
        assert!(text_min_y < text_max_y, "expected visible person name ink in row 0");
        let text_mid_y = (text_min_y as f32 + text_max_y as f32) * 0.5;

        // Measure actual vertical center of the green strikethrough line in the gap x=180..240.
        let mut strike_ys = Vec::new();
        for y in 80..120 {
            let green_hits = (180..240)
                .filter(|&x| is_green_ink(pixel(&done, x, y), 15))
                .count();
            if green_hits >= 40 {
                strike_ys.push(y as f32);
            }
        }
        assert!(!strike_ys.is_empty(), "expected visible green strikethrough in row 0");
        let strike_mid_y = strike_ys.iter().sum::<f32>() / strike_ys.len() as f32;

        // Measure leftmost x of the `✓ 11:30` checkmark (outside the strikethrough y-band).
        let mut check_min_x = u32::MAX;
        for y in 80..120 {
            if (y as f32 - strike_mid_y).abs() <= 2.5 {
                continue;
            }
            for x in 360..495 {
                if is_green_ink(pixel(&done, x, y), 20) {
                    check_min_x = check_min_x.min(x);
                }
            }
        }
        assert!(check_min_x < 495, "expected visible green ✓ 11:30 badge");

        // Measure rightmost x of the strikethrough line before the checkmark.
        let sy = strike_mid_y.round() as u32;
        let mut strike_max_x = 0u32;
        for x in 260..check_min_x {
            if is_green_ink(pixel(&done, x, sy), 15) {
                strike_max_x = x;
            }
        }

        assert!(
            (strike_mid_y - text_mid_y).abs() <= 1.25,
            "strikethrough y ({strike_mid_y:.2}) must center on text ink [{text_min_y}..{text_max_y}] (mid={text_mid_y:.2})"
        );
        assert!(
            strike_max_x + 6 <= check_min_x,
            "strikethrough end x ({strike_max_x}) must leave >= 6px gap before checkmark x ({check_min_x})"
        );
    }
}
