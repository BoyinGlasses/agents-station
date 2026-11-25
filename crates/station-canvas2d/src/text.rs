use crate::canvas::Canvas2d;
use ab_glyph::{Font, FontVec, GlyphId, PxScale, ScaleFont};
use std::path::Path;
use unicode_normalization::UnicodeNormalization;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TextAlign {
    Left,
    Center,
    Right,
}

pub struct FontEngine {
    sans_regular: Vec<FontVec>,
    sans_bold: Vec<FontVec>,
    mono_regular: Vec<FontVec>,
    mono_bold: Vec<FontVec>,
}

fn load_font(path: &Path) -> Option<FontVec> {
    let bytes = std::fs::read(path).ok()?;
    FontVec::try_from_vec(bytes).ok()
}

impl FontEngine {
    /// Loads bundled Vietnamese (`BeVietnamPro`), Thai (`PlexThaiLooped`),
    /// and monospace (`JetBrainsMono`) TTF fonts from `assets_dir/fonts`.
    pub fn from_assets_dir(assets_dir: &Path) -> Self {
        let dir = assets_dir.join("fonts");
        let load_first = |names: &[&str]| -> Vec<FontVec> {
            names
                .iter()
                .filter_map(|n| load_font(&dir.join(n)))
                .collect()
        };

        let sans_regular = load_first(&[
            "BeVietnamPro-500-vietnamese.ttf",
            "BeVietnamPro-500-latin.ttf",
            "BeVietnamPro-400-vietnamese.ttf",
            "BeVietnamPro-400-latin.ttf",
            "PlexThaiLooped-400-thai.ttf",
            "JetBrainsMono-400-latin.ttf",
        ]);
        let sans_bold = load_first(&[
            "BeVietnamPro-700-vietnamese.ttf",
            "BeVietnamPro-700-latin.ttf",
            "BeVietnamPro-800-vietnamese.ttf",
            "BeVietnamPro-800-latin.ttf",
            "BeVietnamPro-600-vietnamese.ttf",
            "PlexThaiLooped-600-thai.ttf",
            "JetBrainsMono-600-latin.ttf",
        ]);
        let mono_regular = load_first(&[
            "JetBrainsMono-400-latin.ttf",
            "JetBrainsMono-500-latin.ttf",
            "BeVietnamPro-500-vietnamese.ttf",
            "PlexThaiLooped-400-thai.ttf",
        ]);
        let mono_bold = load_first(&[
            "JetBrainsMono-600-latin.ttf",
            "JetBrainsMono-500-latin.ttf",
            "BeVietnamPro-700-vietnamese.ttf",
            "PlexThaiLooped-600-thai.ttf",
        ]);

        Self {
            sans_regular,
            sans_bold,
            mono_regular,
            mono_bold,
        }
    }

    fn font_stack(&self, bold: bool, mono: bool) -> &[FontVec] {
        match (bold, mono) {
            (false, false) => &self.sans_regular,
            (true, false) => {
                if self.sans_bold.is_empty() {
                    &self.sans_regular
                } else {
                    &self.sans_bold
                }
            }
            (false, true) => {
                if self.mono_regular.is_empty() {
                    &self.sans_regular
                } else {
                    &self.mono_regular
                }
            }
            (true, true) => {
                if self.mono_bold.is_empty() {
                    &self.mono_regular
                } else {
                    &self.mono_bold
                }
            }
        }
    }

    fn resolve_glyph<'a>(&'a self, ch: char, stack: &'a [FontVec]) -> Option<(&'a FontVec, GlyphId)> {
        for font in stack {
            let gid = font.glyph_id(ch);
            if gid.0 != 0 {
                return Some((font, gid));
            }
        }
        // Fallback across sans_regular if not in primary stack
        for font in &self.sans_regular {
            let gid = font.glyph_id(ch);
            if gid.0 != 0 {
                return Some((font, gid));
            }
        }
        stack.first().map(|f| (f, f.glyph_id(ch)))
    }

    pub fn measure_width(&mut self, text: &str, size_px: f32, bold: bool, mono: bool) -> f32 {
        if text.is_empty() {
            return 0.0;
        }
        let scale = PxScale::from(size_px.max(6.0));
        let stack = self.font_stack(bold, mono);
        let mut cursor_x = 0.0_f32;
        let mut prev: Option<(*const FontVec, GlyphId)> = None;

        for ch in text.nfc() {
            let Some((font, gid)) = self.resolve_glyph(ch, stack) else {
                continue;
            };
            let sf = font.as_scaled(scale);
            let fptr = font as *const FontVec;
            if let Some((pf, pgid)) = prev
                && pf == fptr
            {
                cursor_x += sf.kern(pgid, gid);
            }
            cursor_x += sf.h_advance(gid);
            prev = Some((fptr, gid));
        }
        cursor_x
    }

    /// Draws single-line NFC-normalized text at `(x, top_y)` with optional `max_width` auto-shrink (`fitText`).
    /// Returns the actual rendered width in pixels.
    #[allow(clippy::too_many_arguments)]
    pub fn draw_text(
        &mut self,
        canvas: &mut Canvas2d,
        text: &str,
        x: f32,
        top_y: f32,
        size_px: f32,
        color: [u8; 4],
        align: TextAlign,
        max_width: Option<f32>,
        bold: bool,
        mono: bool,
    ) -> f32 {
        if text.is_empty() {
            return 0.0;
        }
        let mut eff_size = size_px.max(6.0);
        let measured = self.measure_width(text, eff_size, bold, mono);
        if let Some(mw) = max_width
            && mw > 4.0
            && measured > mw
        {
            eff_size = (eff_size * (mw / measured)).max(6.0);
        }

        let line_w = self.measure_width(text, eff_size, bold, mono);
        let ox = match align {
            TextAlign::Left => x,
            TextAlign::Center => x - line_w * 0.5,
            TextAlign::Right => x - line_w,
        };

        let scale = PxScale::from(eff_size);
        let stack = self.font_stack(bold, mono);
        let baseline_y = stack
            .first()
            .map(|f| top_y + f.as_scaled(scale).ascent())
            .unwrap_or(top_y + eff_size * 0.8);

        let mut cursor_x = ox;
        let mut prev: Option<(*const FontVec, GlyphId)> = None;

        for ch in text.nfc() {
            let Some((font, gid)) = self.resolve_glyph(ch, stack) else {
                continue;
            };
            let sf = font.as_scaled(scale);
            let fptr = font as *const FontVec;
            if let Some((pf, pgid)) = prev
                && pf == fptr
            {
                cursor_x += sf.kern(pgid, gid);
            }
            let glyph = gid.with_scale_and_position(scale, ab_glyph::point(cursor_x, baseline_y));
            if let Some(outlined) = font.outline_glyph(glyph) {
                let bounds = outlined.px_bounds();
                let bx = bounds.min.x.floor() as i32;
                let by = bounds.min.y.floor() as i32;
                outlined.draw(|gx, gy, cov| {
                    let a = ((cov * color[3] as f32).round()).clamp(0.0, 255.0) as u8;
                    if a > 0 {
                        canvas.blend_pixel(bx + gx as i32, by + gy as i32, [color[0], color[1], color[2], a]);
                    }
                });
            }
            cursor_x += sf.h_advance(gid);
            prev = Some((fptr, gid));
        }

        line_w
    }

    /// Truncates `text` with `…` to fit `max_width` without shrinking font size.
    pub fn truncate_to_width(
        &mut self,
        text: &str,
        size_px: f32,
        max_width: f32,
        bold: bool,
        mono: bool,
    ) -> String {
        if self.measure_width(text, size_px, bold, mono) <= max_width {
            return text.to_owned();
        }
        let chars: Vec<char> = text.chars().collect();
        let mut lo = 0usize;
        let mut hi = chars.len();
        while lo < hi {
            let mid = (lo + hi).div_ceil(2);
            let mut cand: String = chars[..mid].iter().collect();
            cand.push('…');
            if self.measure_width(&cand, size_px, bold, mono) <= max_width {
                lo = mid;
            } else {
                hi = mid - 1;
            }
        }
        let mut out: String = chars[..lo].iter().collect();
        out = out.trim_end().to_owned();
        out.push('…');
        out
    }
}
