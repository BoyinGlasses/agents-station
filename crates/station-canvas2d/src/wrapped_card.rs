use crate::canvas::{Canvas2d, RgbaSurface, hex, hex_rgba};
use crate::text::{FontEngine, TextAlign};
use anyhow::{Context, Result};
use image::ImageEncoder;
use image::codecs::png::PngEncoder;

pub const WRAPPED_WIDTH: u32 = 1080;
pub const WRAPPED_HEIGHT: u32 = 1350;

#[derive(Debug, Clone)]
pub struct WrappedCardInput {
    pub brand_title: String,
    pub date_range: String,
    pub heading: String,
    pub hours_headline: String,
    pub worked_line: String,
    pub waited_sessions_line: String,
    pub tiles: [(String, String); 4],
    pub highlights: Vec<String>,
    pub footer: String,
    pub office_shot: Option<RgbaSurface>,
}

#[derive(Debug, Clone)]
pub struct PhotoNameTag {
    pub x_norm: f32,
    pub y_norm: f32,
    pub text: String,
    pub dot_rgba: Option<[u8; 4]>,
}

/// Renders the exact 1080x1350 shareable weekly recap card matching `src/office/ui/wrapped.js`.
pub fn draw_wrapped_card(input: &WrappedCardInput, fonts: &mut FontEngine) -> RgbaSurface {
    let w = WRAPPED_WIDTH as f32;
    let h = WRAPPED_HEIGHT as f32;
    let mut c = Canvas2d::new(WRAPPED_WIDTH, WRAPPED_HEIGHT);

    // 1. Background vertical gradient (#1C1638 -> #121A2E -> #0D1322) + dual radial glows
    c.linear_gradient_rect(
        0.0,
        0.0,
        w,
        h,
        (0.0, 0.0),
        (0.0, h),
        &[
            (0.0, hex("#1C1638")),
            (0.55, hex("#121A2E")),
            (1.0, hex("#0D1322")),
        ],
    );
    c.radial_gradient_circle(
        930.0,
        120.0,
        380.0,
        &[(0.0, [217, 119, 87, 56]), (1.0, [217, 119, 87, 0])],
    );
    c.radial_gradient_circle(
        120.0,
        980.0,
        420.0,
        &[(0.0, [140, 79, 168, 56]), (1.0, [140, 79, 168, 0])],
    );

    // 2. Header: orange office mark badge + brand title + date range + headline title
    draw_office_mark(&mut c, 60.0, 56.0, 52.0);
    fonts.draw_text(
        &mut c,
        &input.brand_title,
        132.0,
        64.0,
        34.0,
        hex("#F6EFE4"),
        TextAlign::Left,
        Some(520.0),
        true,
        false,
    );
    fonts.draw_text(
        &mut c,
        &input.date_range,
        w - 60.0,
        68.0,
        28.0,
        hex("#A9B1C4"),
        TextAlign::Right,
        Some(380.0),
        false,
        false,
    );
    fonts.draw_text(
        &mut c,
        &input.heading,
        60.0,
        142.0,
        60.0,
        hex("#FFFFFF"),
        TextAlign::Left,
        Some(w - 120.0),
        true,
        false,
    );

    // 3. Office hero frame (60, 236, 960x470, r=28)
    let fx = 60.0;
    let fy = 236.0;
    let fw = w - 120.0;
    let fh = 470.0;
    c.round_rect(fx, fy, fw, fh, 28.0, hex("#162033"));
    if let Some(shot) = &input.office_shot {
        blit_cover_rounded(&mut c, shot, fx as i32, fy as i32, fw as u32, fh as u32, 28.0);
    } else {
        draw_stylized_office_backdrop(&mut c, fx, fy, fw, fh);
    }
    c.stroke_round_rect(fx, fy, fw, fh, 28.0, 2.5, [255, 255, 255, 36]);

    // 4. Headline hours + worked/waited subtitles
    fonts.draw_text(
        &mut c,
        &input.hours_headline,
        60.0,
        732.0,
        110.0,
        hex("#FFB86B"),
        TextAlign::Left,
        Some(w - 120.0),
        true,
        false,
    );
    fonts.draw_text(
        &mut c,
        &input.worked_line,
        64.0,
        862.0,
        30.0,
        hex("#DCE3F0"),
        TextAlign::Left,
        Some(w - 128.0),
        true,
        false,
    );
    fonts.draw_text(
        &mut c,
        &input.waited_sessions_line,
        64.0,
        906.0,
        26.0,
        hex("#8D97AA"),
        TextAlign::Left,
        Some(w - 128.0),
        false,
        false,
    );

    // 5. Four KPI tiles (prompts, edits, cmds, cost)
    for (i, (val, label)) in input.tiles.iter().enumerate() {
        let tx = 60.0 + (i as f32) * 245.0;
        let ty = 964.0;
        c.round_rect(tx, ty, 225.0, 150.0, 22.0, [255, 255, 255, 16]);
        c.stroke_round_rect(tx, ty, 225.0, 150.0, 22.0, 1.5, [255, 255, 255, 28]);
        let val_size = if val.chars().count() > 6 { 38.0 } else { 48.0 };
        fonts.draw_text(
            &mut c,
            val,
            tx + 22.0,
            ty + 30.0,
            val_size,
            hex("#FFFFFF"),
            TextAlign::Left,
            Some(182.0),
            true,
            false,
        );
        fonts.draw_text(
            &mut c,
            label,
            tx + 22.0,
            ty + 96.0,
            23.0,
            hex("#A9B1C4"),
            TextAlign::Left,
            Some(182.0),
            false,
            false,
        );
    }

    // 6. Highlights (up to 3 lines)
    for (i, line) in input.highlights.iter().take(3).enumerate() {
        fonts.draw_text(
            &mut c,
            line,
            60.0,
            1146.0 + (i as f32) * 46.0,
            28.0,
            hex("#E6EBF5"),
            TextAlign::Left,
            Some(w - 120.0),
            true,
            false,
        );
    }

    // 7. Footer watermark
    fonts.draw_text(
        &mut c,
        &input.footer,
        w - 60.0,
        h - 54.0,
        22.0,
        hex("#5E6880"),
        TextAlign::Right,
        Some(w - 120.0),
        false,
        false,
    );

    c.into_surface()
}

/// Encodes the 1080x1350 Wrapped card directly to PNG bytes and drops the pixel buffer immediately.
pub fn encode_wrapped_png(input: &WrappedCardInput, fonts: &mut FontEngine) -> Result<Vec<u8>> {
    let surf = draw_wrapped_card(input, fonts);
    let mut out = Vec::with_capacity(256 * 1024);
    let enc = PngEncoder::new(&mut out);
    enc.write_image(
        &surf.rgba,
        surf.width,
        surf.height,
        image::ExtendedColorType::Rgba8,
    )
    .context("encode wrapped PNG")?;
    drop(surf);
    Ok(out)
}

/// Paints optional floating name-tag pills and `"Agents Station"` corner watermark onto an RGBA8 photo buffer.
pub fn paint_photo_overlays(
    rgba: &mut [u8],
    width: u32,
    height: u32,
    tags: &[PhotoNameTag],
    watermark_text: Option<&str>,
    fonts: &mut FontEngine,
) {
    if width == 0 || height == 0 || rgba.len() < (width as usize * height as usize * 4) {
        return;
    }
    if tags.is_empty() && watermark_text.is_none() {
        return;
    }
    let mut overlay = Canvas2d::new(width, height);
    let scale = (width as f32 / 1280.0).clamp(0.75, 3.0);

    for tag in tags {
        let text = tag.text.trim();
        if text.is_empty() {
            continue;
        }
        let cx = (tag.x_norm.clamp(0.05, 0.95)) * width as f32;
        let cy = (tag.y_norm.clamp(0.05, 0.95)) * height as f32;
        let fs = 13.0 * scale;
        let tw = fonts.measure_width(text, fs, true, false);
        let has_dot = tag.dot_rgba.is_some();
        let pad_x = 10.0 * scale;
        let dot_space = if has_dot { 14.0 * scale } else { 0.0 };
        let pw = tw + pad_x * 2.0 + dot_space;
        let ph = 26.0 * scale;
        let x0 = (cx - pw * 0.5).clamp(4.0, (width as f32 - pw - 4.0).max(4.0));
        let y0 = (cy - ph * 0.5).clamp(4.0, (height as f32 - ph - 4.0).max(4.0));

        overlay.round_rect(x0, y0, pw, ph, ph * 0.5, [14, 19, 27, 220]);
        overlay.stroke_round_rect(x0, y0, pw, ph, ph * 0.5, scale.max(1.0), [255, 255, 255, 36]);
        let mut tx = x0 + pad_x;
        if let Some(dot) = tag.dot_rgba {
            overlay.fill_circle(tx + 4.0 * scale, y0 + ph * 0.5, 4.0 * scale, dot);
            tx += dot_space;
        }
        fonts.draw_text(
            &mut overlay,
            text,
            tx,
            y0 + (ph - fs) * 0.42,
            fs,
            hex("#EEF1F7"),
            TextAlign::Left,
            Some(pw),
            true,
            false,
        );
    }

    if let Some(mark) = watermark_text.filter(|s| !s.trim().is_empty()) {
        let fs = 14.0 * scale;
        let pad = 16.0 * scale;
        let tw = fonts.measure_width(mark, fs, true, false);
        let pw = tw + 44.0 * scale;
        let ph = 32.0 * scale;
        let x0 = (width as f32 - pw - pad).max(4.0);
        let y0 = (height as f32 - ph - pad).max(4.0);
        overlay.round_rect(x0, y0, pw, ph, ph * 0.5, [14, 19, 27, 196]);
        overlay.stroke_round_rect(x0, y0, pw, ph, ph * 0.5, scale.max(1.0), [255, 255, 255, 34]);
        draw_office_mark(&mut overlay, x0 + 8.0 * scale, y0 + 6.0 * scale, 20.0 * scale);
        fonts.draw_text(
            &mut overlay,
            mark,
            x0 + 34.0 * scale,
            y0 + (ph - fs) * 0.42,
            fs,
            hex("#F6EFE4"),
            TextAlign::Left,
            Some(tw + 8.0),
            true,
            false,
        );
    }

    let surf = overlay.into_surface();
    for (dst, src) in rgba.chunks_exact_mut(4).zip(surf.rgba.chunks_exact(4)) {
        let sa = src[3] as u32;
        if sa == 0 {
            continue;
        }
        if sa == 255 {
            dst.copy_from_slice(src);
        } else {
            let inv = 255 - sa;
            dst[0] = ((src[0] as u32 * sa + dst[0] as u32 * inv + 127) / 255) as u8;
            dst[1] = ((src[1] as u32 * sa + dst[1] as u32 * inv + 127) / 255) as u8;
            dst[2] = ((src[2] as u32 * sa + dst[2] as u32 * inv + 127) / 255) as u8;
            dst[3] = 255;
        }
    }
}

fn draw_office_mark(c: &mut Canvas2d, x: f32, y: f32, size: f32) {
    let r = (size * 0.24).max(3.0);
    c.round_rect(x, y, size, size, r, hex("#F08A3E"));
    let inset = size * 0.22;
    let w = size - inset * 2.0;
    let h = size * 0.36;
    c.round_rect(x + inset, y + size * 0.20, w, h, r * 0.4, hex("#111720"));
    c.round_rect(
        x + size * 0.34,
        y + size * 0.64,
        size * 0.32,
        size * 0.14,
        r * 0.25,
        hex("#111720"),
    );
}

fn draw_stylized_office_backdrop(c: &mut Canvas2d, fx: f32, fy: f32, fw: f32, fh: f32) {
    c.linear_gradient_rect(
        fx + 2.0,
        fy + 2.0,
        fw - 4.0,
        fh - 4.0,
        (fx, fy),
        (fx + fw, fy + fh),
        &[(0.0, hex("#1D2940")), (1.0, hex("#111927"))],
    );
    // Warm desk glow accents inside the stylized office floor card
    c.radial_gradient_circle(
        fx + fw * 0.28,
        fy + fh * 0.56,
        150.0,
        &[(0.0, hex_rgba("#EF8A5F", 70)), (1.0, hex_rgba("#EF8A5F", 0))],
    );
    c.radial_gradient_circle(
        fx + fw * 0.72,
        fy + fh * 0.45,
        160.0,
        &[(0.0, hex_rgba("#6AAEFF", 60)), (1.0, hex_rgba("#6AAEFF", 0))],
    );
    for col in 0..4 {
        let dx = fx + 95.0 + col as f32 * 205.0;
        let dy = fy + 200.0 + (col % 2) as f32 * 52.0;
        c.round_rect(dx, dy, 146.0, 82.0, 12.0, hex("#263449"));
        c.round_rect(dx + 22.0, dy + 14.0, 102.0, 44.0, 6.0, hex("#121A28"));
        let accent = match col {
            0 => hex("#EF8A5F"),
            1 => hex("#3DDC84"),
            2 => hex("#6AAEFF"),
            _ => hex("#F2B544"),
        };
        c.fill_rect(dx + 32.0, dy + 24.0, 56.0, 6.0, accent);
        c.fill_rect(dx + 32.0, dy + 36.0, 74.0, 5.0, [220, 228, 240, 120]);
    }
}

fn blit_cover_rounded(
    c: &mut Canvas2d,
    src: &RgbaSurface,
    dx: i32,
    dy: i32,
    dw: u32,
    dh: u32,
    radius: f32,
) {
    if src.width == 0 || src.height == 0 || dw == 0 || dh == 0 {
        return;
    }
    let scale = (dw as f32 / src.width as f32).max(dh as f32 / src.height as f32);
    let crop_w = dw as f32 / scale;
    let crop_h = dh as f32 / scale;
    let sx0 = ((src.width as f32 - crop_w) * 0.5).max(0.0);
    let sy0 = ((src.height as f32 - crop_h) * 0.5).max(0.0);
    let r2 = radius * radius;
    let fw = dw as f32;
    let fh = dh as f32;

    for oy in 0..dh {
        let fy = oy as f32 + 0.5;
        let sy = ((sy0 + (oy as f32 / dh as f32) * crop_h) as u32).min(src.height - 1);
        for ox in 0..dw {
            let fx = ox as f32 + 0.5;
            if outside_rounded_rect(fx, fy, fw, fh, radius, r2) {
                continue;
            }
            let sx = ((sx0 + (ox as f32 / dw as f32) * crop_w) as u32).min(src.width - 1);
            let si = ((sy * src.width + sx) * 4) as usize;
            let px = [src.rgba[si], src.rgba[si + 1], src.rgba[si + 2], src.rgba[si + 3]];
            c.blend_pixel(dx + ox as i32, dy + oy as i32, px);
        }
    }
}

#[inline]
fn outside_rounded_rect(x: f32, y: f32, w: f32, h: f32, r: f32, r2: f32) -> bool {
    let cx = if x < r {
        r - x
    } else if x > w - r {
        x - (w - r)
    } else {
        return false;
    };
    let cy = if y < r {
        r - y
    } else if y > h - r {
        y - (h - r)
    } else {
        return false;
    };
    cx * cx + cy * cy > r2
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    fn fonts() -> FontEngine {
        FontEngine::from_assets_dir(&PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../assets"))
    }

    fn sample_input(with_shot: bool) -> WrappedCardInput {
        let office_shot = with_shot.then(|| {
            let mut c = Canvas2d::new(320, 180);
            c.fill(hex("#24344D"));
            c.round_rect(40.0, 30.0, 240.0, 120.0, 12.0, hex("#E8865F"));
            c.into_surface()
        });
        WrappedCardInput {
            brand_title: "Agents Station".to_owned(),
            date_range: "11/19 – 11/25".to_owned(),
            heading: "My week with Claude".to_owned(),
            hours_headline: "63.4 hours".to_owned(),
            worked_line: "of agent work · in 38.2 hours on the clock".to_owned(),
            waited_sessions_line: "9.6 hours waiting · 27 sessions".to_owned(),
            tiles: [
                ("312".to_owned(), "prompts".to_owned()),
                ("1,840".to_owned(), "file edits".to_owned()),
                ("2,985".to_owned(), "commands run".to_owned()),
                ("$412.5".to_owned(), "API cost ≈".to_owned()),
            ],
            highlights: vec![
                "🏆 Busiest project: agents-station · 21.3 hours".to_owned(),
                "📅 Busiest day: Thursday 11/23".to_owned(),
                "⏱️ Longest unbroken run: 3h 34m".to_owned(),
            ],
            footer: "Agents Station · a 3D office for Claude Code".to_owned(),
            office_shot,
        }
    }

    #[test]
    fn wrapped_png_export_has_exact_1080x1350_dimensions_and_valid_signature() {
        let mut f = fonts();
        let png_bytes = encode_wrapped_png(&sample_input(true), &mut f).expect("encode wrapped");
        assert!(
            png_bytes.starts_with(b"\x89PNG\r\n\x1a\n"),
            "encoded bytes must start with PNG magic signature"
        );
        let decoded = image::load_from_memory(&png_bytes)
            .expect("valid decodable PNG")
            .to_rgba8();
        assert_eq!(decoded.width(), WRAPPED_WIDTH);
        assert_eq!(decoded.height(), WRAPPED_HEIGHT);

        let surf = RgbaSurface {
            width: decoded.width(),
            height: decoded.height(),
            rgba: decoded.into_raw(),
        };
        assert!(
            surf.non_background_ratio(hex_rgba("#0D1322", 255)) > 0.20,
            "wrapped card must render visible gradient, office frame, tiles, and text"
        );
    }

    #[test]
    fn wrapped_png_export_works_without_office_shot_fallback() {
        let mut f = fonts();
        let png_bytes = encode_wrapped_png(&sample_input(false), &mut f).expect("encode wrapped");
        assert!(png_bytes.starts_with(b"\x89PNG\r\n\x1a\n"));
        let decoded = image::load_from_memory(&png_bytes).expect("valid PNG");
        assert_eq!((decoded.width(), decoded.height()), (1080, 1350));
    }

    #[test]
    fn photo_overlays_paint_watermark_and_name_tags() {
        let mut f = fonts();
        let mut rgba = vec![20u8; 640 * 360 * 4];
        for px in rgba.chunks_exact_mut(4) {
            px[3] = 255;
        }
        let before = rgba.clone();
        paint_photo_overlays(
            &mut rgba,
            640,
            360,
            &[PhotoNameTag {
                x_norm: 0.5,
                y_norm: 0.4,
                text: "Claude".to_owned(),
                dot_rgba: Some(hex("#3DDC84")),
            }],
            Some("Agents Station"),
            &mut f,
        );
        assert_ne!(rgba, before, "photo overlay must paint tag pill and watermark");
    }
}
