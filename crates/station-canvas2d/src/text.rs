use crate::canvas::Canvas2d;
use cosmic_text::{
    Attrs, Buffer, Family, FontSystem, Metrics, Shaping, SwashCache, SwashContent, Weight, Wrap,
    fontdb,
};
use std::path::Path;
use std::sync::{Arc, Mutex};
use unicode_normalization::UnicodeNormalization;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TextAlign {
    Left,
    Center,
    Right,
}

#[derive(Debug, Clone, PartialEq)]
pub struct ShapedGlyphInfo {
    pub glyph_id: u16,
    pub x: f32,
    pub y: f32,
    pub w: f32,
    pub x_offset: f32,
    pub y_offset: f32,
}

struct FontEngineInner {
    font_system: FontSystem,
    swash_cache: SwashCache,
    scratch: Buffer,
}

#[derive(Clone)]
pub struct FontEngine {
    inner: Arc<Mutex<FontEngineInner>>,
}

/// Ordered bundled TTFs so Latin precedes Vietnamese/Latin-ext for the same family+weight,
/// keeping ASCII in `BeVietnamPro-*-latin` while Vietnamese diacritics resolve in
/// `BeVietnamPro-*-vietnamese` at identical weight before falling back to other families.
const BUNDLED_FONTS: &[&str] = &[
    // Be Vietnam Pro (sans-serif primary: Latin -> Vietnamese -> Latin-ext per weight)
    "BeVietnamPro-400-latin.ttf",
    "BeVietnamPro-400-vietnamese.ttf",
    "BeVietnamPro-400-latin-ext.ttf",
    "BeVietnamPro-500-latin.ttf",
    "BeVietnamPro-500-vietnamese.ttf",
    "BeVietnamPro-500-latin-ext.ttf",
    "BeVietnamPro-600-latin.ttf",
    "BeVietnamPro-600-vietnamese.ttf",
    "BeVietnamPro-600-latin-ext.ttf",
    "BeVietnamPro-700-latin.ttf",
    "BeVietnamPro-700-vietnamese.ttf",
    "BeVietnamPro-700-latin-ext.ttf",
    "BeVietnamPro-800-latin.ttf",
    "BeVietnamPro-800-vietnamese.ttf",
    "BeVietnamPro-800-latin-ext.ttf",
    // IBM Plex Sans Thai Looped (Thai OpenType GSUB/GPOS shaping)
    "PlexThaiLooped-400-thai.ttf",
    "PlexThaiLooped-500-thai.ttf",
    "PlexThaiLooped-600-thai.ttf",
    "PlexThaiLooped-700-thai.ttf",
    // JetBrains Mono (monospace primary)
    "JetBrainsMono-400-latin.ttf",
    "JetBrainsMono-400-vietnamese.ttf",
    "JetBrainsMono-400-latin-ext.ttf",
    "JetBrainsMono-600-latin.ttf",
    "JetBrainsMono-600-vietnamese.ttf",
    "JetBrainsMono-600-latin-ext.ttf",
];

/// Lightweight symbol/math fallback font paths (loaded selectively — NEVER `load_system_fonts()`).
const SYMBOL_FALLBACK_CANDIDATES: &[&str] = &[
    r"C:\Windows\Fonts\seguisym.ttf",
    r"C:\Windows\Fonts\segoeui.ttf",
    "/usr/share/fonts/truetype/dejavu/DejaVuSans.ttf",
    "/System/Library/Fonts/Apple Symbols.ttf",
    "/System/Library/Fonts/Supplemental/Arial Unicode.ttf",
];

fn load_symbol_fallback(db: &mut fontdb::Database) {
    for candidate in SYMBOL_FALLBACK_CANDIDATES {
        let path = Path::new(candidate);
        if path.is_file() && db.load_font_file(path).is_ok() {
            return;
        }
    }
}

fn build_attrs(bold: bool, mono: bool) -> Attrs<'static> {
    let family = if mono {
        Family::Monospace
    } else {
        Family::SansSerif
    };
    let weight = match (bold, mono) {
        (false, false) => Weight::MEDIUM,
        (true, false) => Weight::BOLD,
        (false, true) => Weight::NORMAL,
        (true, true) => Weight::SEMIBOLD,
    };
    Attrs::new().family(family).weight(weight)
}

fn shape_scratch(inner: &mut FontEngineInner, text: &str, size_px: f32, bold: bool, mono: bool) {
    let eff = size_px.max(6.0);
    let metrics = Metrics::new(eff, (eff * 1.15).max(eff + 1.0));
    let attrs = build_attrs(bold, mono);
    let normalized: String = text.nfc().collect();
    inner.scratch.set_metrics(&mut inner.font_system, metrics);
    inner
        .scratch
        .set_text(&mut inner.font_system, &normalized, &attrs, Shaping::Advanced);
    inner.scratch.shape_until_scroll(&mut inner.font_system, false);
}

impl FontEngine {
    /// Loads bundled Vietnamese (`BeVietnamPro`), Thai (`PlexThaiLooped`),
    /// and monospace (`JetBrainsMono`) TTF fonts from `assets_dir/fonts` ONCE into a single
    /// shared `cosmic-text` `FontSystem` + `SwashCache`, plus one lightweight symbol fallback
    /// font file (never scanning the full OS font directory).
    pub fn from_assets_dir(assets_dir: &Path) -> Self {
        let dir = assets_dir.join("fonts");
        let mut db = fontdb::Database::new();

        for name in BUNDLED_FONTS {
            let path = dir.join(name);
            if path.is_file() {
                let _ = db.load_font_file(&path);
            }
        }
        load_symbol_fallback(&mut db);

        db.set_sans_serif_family("Be Vietnam Pro");
        db.set_monospace_family("JetBrains Mono");

        let mut font_system = FontSystem::new_with_locale_and_db("en-US".to_owned(), db);
        let mut scratch = Buffer::new(&mut font_system, Metrics::new(16.0, 19.0));
        scratch.set_wrap(&mut font_system, Wrap::None);
        scratch.set_size(&mut font_system, None, None);

        Self {
            inner: Arc::new(Mutex::new(FontEngineInner {
                font_system,
                swash_cache: SwashCache::new(),
                scratch,
            })),
        }
    }

    pub fn loaded_face_count(&self) -> usize {
        let inner = self.inner.lock().expect("FontEngine lock poisoned");
        inner.font_system.db().len()
    }

    pub fn shares_engine_with(&self, other: &Self) -> bool {
        Arc::ptr_eq(&self.inner, &other.inner)
    }

    /// Shapes a single line and returns positioned glyph metadata for diagnostics/tests.
    pub fn shape_line(&mut self, text: &str, size_px: f32, bold: bool, mono: bool) -> Vec<ShapedGlyphInfo> {
        if text.is_empty() {
            return Vec::new();
        }
        let mut inner = self.inner.lock().expect("FontEngine lock poisoned");
        shape_scratch(&mut inner, text, size_px, bold, mono);
        let mut out = Vec::new();
        for run in inner.scratch.layout_runs() {
            for g in run.glyphs {
                out.push(ShapedGlyphInfo {
                    glyph_id: g.glyph_id,
                    x: g.x,
                    y: g.y,
                    w: g.w,
                    x_offset: g.x_offset * g.font_size,
                    y_offset: g.y_offset * g.font_size,
                });
            }
        }
        out
    }

    pub fn measure_width(&mut self, text: &str, size_px: f32, bold: bool, mono: bool) -> f32 {
        if text.is_empty() {
            return 0.0;
        }
        let mut inner = self.inner.lock().expect("FontEngine lock poisoned");
        shape_scratch(&mut inner, text, size_px, bold, mono);
        inner
            .scratch
            .layout_runs()
            .map(|run| run.line_w)
            .fold(0.0_f32, f32::max)
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
        if text.is_empty() || color[3] == 0 {
            return 0.0;
        }
        let mut inner = self.inner.lock().expect("FontEngine lock poisoned");
        let mut eff_size = size_px.max(6.0);
        shape_scratch(&mut inner, text, eff_size, bold, mono);
        let mut line_w = inner
            .scratch
            .layout_runs()
            .map(|r| r.line_w)
            .fold(0.0_f32, f32::max);

        if let Some(mw) = max_width
            && mw > 4.0
            && line_w > mw
        {
            eff_size = (eff_size * (mw / line_w)).max(6.0);
            shape_scratch(&mut inner, text, eff_size, bold, mono);
            line_w = inner
                .scratch
                .layout_runs()
                .map(|r| r.line_w)
                .fold(0.0_f32, f32::max);
        }

        let ox = match align {
            TextAlign::Left => x,
            TextAlign::Center => x - line_w * 0.5,
            TextAlign::Right => x - line_w,
        };

        let FontEngineInner {
            font_system,
            swash_cache,
            scratch,
        } = &mut *inner;

        for run in scratch.layout_runs() {
            let baseline_y = top_y + run.line_y;
            for glyph in run.glyphs {
                let phys = glyph.physical((ox, baseline_y), 1.0);
                let Some(img) = swash_cache.get_image(font_system, phys.cache_key) else {
                    continue;
                };
                let gx0 = phys.x + img.placement.left;
                let gy0 = phys.y - img.placement.top;
                let w = img.placement.width as i32;
                let h = img.placement.height as i32;
                match img.content {
                    SwashContent::Mask => {
                        let mut i = 0usize;
                        for dy in 0..h {
                            for dx in 0..w {
                                let cov = img.data[i] as u32;
                                i += 1;
                                if cov > 0 {
                                    let a = ((cov * color[3] as u32 + 127) / 255) as u8;
                                    if a > 0 {
                                        canvas.blend_pixel(gx0 + dx, gy0 + dy, [color[0], color[1], color[2], a]);
                                    }
                                }
                            }
                        }
                    }
                    SwashContent::Color => {
                        let mut i = 0usize;
                        for dy in 0..h {
                            for dx in 0..w {
                                let (r, g, b, pa) = (
                                    img.data[i],
                                    img.data[i + 1],
                                    img.data[i + 2],
                                    img.data[i + 3] as u32,
                                );
                                i += 4;
                                if pa > 0 {
                                    let a = ((pa * color[3] as u32 + 127) / 255) as u8;
                                    if a > 0 {
                                        canvas.blend_pixel(gx0 + dx, gy0 + dy, [r, g, b, a]);
                                    }
                                }
                            }
                        }
                    }
                    SwashContent::SubpixelMask => {
                        let mut i = 0usize;
                        for dy in 0..h {
                            for dx in 0..w {
                                let cov = ((img.data[i] as u32
                                    + img.data[i + 1] as u32
                                    + img.data[i + 2] as u32)
                                    / 3)
                                    .min(255);
                                i += 3;
                                if cov > 0 {
                                    let a = ((cov * color[3] as u32 + 127) / 255) as u8;
                                    if a > 0 {
                                        canvas.blend_pixel(gx0 + dx, gy0 + dy, [color[0], color[1], color[2], a]);
                                    }
                                }
                            }
                        }
                    }
                }
            }
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::canvas::hex;
    use std::path::PathBuf;

    fn assets_dir() -> PathBuf {
        PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../assets")
    }

    #[test]
    fn shapes_symbols_vietnamese_and_thai_without_notdef() {
        let mut fonts = FontEngine::from_assets_dir(&assets_dir());

        for (label, sample) in [
            ("math symbol", "≤ 300 MB"),
            ("check & notes", "✓ 11:30 · ♪ ♫"),
            ("vietnamese", "Tiếng Việt: Ă Ắ Ặ Đ Ơ Ư Ỹ"),
            ("thai", "ผู้จัดการ · ภาษาไทย"),
        ] {
            let glyphs = fonts.shape_line(sample, 24.0, false, false);
            assert!(!glyphs.is_empty(), "{label}: shaped glyphs must not be empty");
            let missing: Vec<_> = glyphs.iter().filter(|g| g.glyph_id == 0).collect();
            assert!(
                missing.is_empty(),
                "{label}: expected zero .notdef (glyph_id == 0) in {sample:?}, got {missing:?}"
            );
        }

        // Verify `≤` actually rasterizes visible ink pixels (not blank/.notdef).
        let mut c = Canvas2d::new(64, 64);
        c.fill(hex("#000000"));
        let w = fonts.draw_text(&mut c, "≤", 8.0, 12.0, 32.0, hex("#FFFFFF"), TextAlign::Left, None, false, false);
        assert!(w > 4.0, "expected positive advance width for ≤, got {w}");
        let surf = c.into_surface();
        assert!(
            surf.non_background_ratio(hex("#000000")) > 0.01,
            "expected ≤ to rasterize visible ink pixels"
        );
    }

    #[test]
    fn thai_combining_marks_receive_gpos_shaping_and_render_cleanly() {
        let mut fonts = FontEngine::from_assets_dir(&assets_dir());

        // "ผู้จัดการ · ภาษาไทย" contains combining vowel U+0E39 (ู) + tone mark U+0E49 (้)
        // plus "ที่" contains stacked above-base marks (ี + ่) that rely on OpenType GPOS.
        let shaped = fonts.shape_line("ผู้จัดการ · ภาษาไทย", 28.0, false, false);
        assert!(!shaped.is_empty());
        assert!(shaped.iter().all(|g| g.glyph_id != 0));

        // Combining marks must not advance horizontally like unshaped spacing glyphs.
        let zero_advance_marks = shaped.iter().filter(|g| g.w.abs() < 0.01).count();
        assert!(
            zero_advance_marks >= 2,
            "expected >= 2 zero-advance Thai combining marks in shaped run, got {zero_advance_marks}: {shaped:?}"
        );

        // Verify stacked Thai tone mark receives real GPOS offset on "ที่" (ท + ี + ่).
        let stacked = fonts.shape_line("ที่", 28.0, false, false);
        assert!(stacked.iter().all(|g| g.glyph_id != 0));
        assert!(
            stacked.iter().any(|g| g.y_offset.abs() > 0.5 || g.x_offset.abs() > 0.5),
            "expected non-zero GPOS offset for stacked Thai marks in 'ที่': {stacked:?}"
        );

        let mut c = Canvas2d::new(320, 64);
        c.fill(hex("#10141C"));
        let w = fonts.draw_text(
            &mut c,
            "ผู้จัดการ · ภาษาไทย",
            12.0,
            14.0,
            24.0,
            hex("#FFFFFF"),
            TextAlign::Left,
            None,
            false,
            false,
        );
        assert!(w > 80.0);
        let surf = c.into_surface();
        assert!(surf.non_background_ratio(hex("#10141C")) > 0.02);
    }

    #[test]
    fn font_engine_shares_single_bounded_font_system_across_clones() {
        let a = FontEngine::from_assets_dir(&assets_dir());
        let b = a.clone();
        assert!(a.shares_engine_with(&b), "cloned FontEngine must share underlying Arc<Mutex<...>>");

        let faces = a.loaded_face_count();
        assert!(
            (25..=40).contains(&faces),
            "expected only bundled fonts + lightweight symbol fallback (25..=40 faces, never 400+ system fonts), got {faces}"
        );
    }
}
