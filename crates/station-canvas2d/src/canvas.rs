use anyhow::{Context, Result};
use std::path::Path;
use tiny_skia::{
    Color, FillRule, GradientStop, LineCap, LineJoin, LinearGradient, Paint, PathBuilder, Pixmap,
    Point, RadialGradient, Rect, SpreadMode, Stroke, Transform,
};

#[derive(Debug, Clone)]
pub struct RgbaSurface {
    pub width: u32,
    pub height: u32,
    pub rgba: Vec<u8>,
}

impl RgbaSurface {
    pub fn save_png(&self, path: &Path) -> Result<()> {
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        image::save_buffer(
            path,
            &self.rgba,
            self.width,
            self.height,
            image::ColorType::Rgba8,
        )
        .with_context(|| format!("failed to write PNG {}", path.display()))
    }

    pub fn non_background_ratio(&self, bg: [u8; 4]) -> f32 {
        let total = (self.width as usize).saturating_mul(self.height as usize);
        if total == 0 {
            return 0.0;
        }
        let diff = self
            .rgba
            .chunks_exact(4)
            .filter(|px| {
                px[0].abs_diff(bg[0]) > 4
                    || px[1].abs_diff(bg[1]) > 4
                    || px[2].abs_diff(bg[2]) > 4
                    || px[3].abs_diff(bg[3]) > 4
            })
            .count();
        diff as f32 / total as f32
    }

    /// 2x2 box-filtered half-resolution copy (4x smaller GPU & staging footprint while keeping
    /// normalized UV rects identical and improving isometric anti-aliasing).
    pub fn downsample_half(&self) -> Self {
        let nw = (self.width / 2).max(1);
        let nh = (self.height / 2).max(1);
        let sw = self.width as usize;
        let sh = self.height as usize;
        let mut out = vec![0u8; (nw as usize) * (nh as usize) * 4];
        for y in 0..nh as usize {
            let sy0 = (y * 2).min(sh.saturating_sub(1));
            let sy1 = (sy0 + 1).min(sh.saturating_sub(1));
            for x in 0..nw as usize {
                let sx0 = (x * 2).min(sw.saturating_sub(1));
                let sx1 = (sx0 + 1).min(sw.saturating_sub(1));
                let idx = [
                    (sy0 * sw + sx0) * 4,
                    (sy0 * sw + sx1) * 4,
                    (sy1 * sw + sx0) * 4,
                    (sy1 * sw + sx1) * 4,
                ];
                let dst = (y * nw as usize + x) * 4;
                for c in 0..4 {
                    let sum: u32 = idx.iter().map(|&i| self.rgba[i + c] as u32).sum();
                    out[dst + c] = ((sum + 2) / 4) as u8;
                }
            }
        }
        Self {
            width: nw,
            height: nh,
            rgba: out,
        }
    }
}

pub fn hex_rgba(hex: &str, alpha: u8) -> [u8; 4] {
    let s = hex.trim().trim_start_matches('#');
    if s.len() == 6
        && let (Ok(r), Ok(g), Ok(b)) = (
            u8::from_str_radix(&s[0..2], 16),
            u8::from_str_radix(&s[2..4], 16),
            u8::from_str_radix(&s[4..6], 16),
        )
    {
        return [r, g, b, alpha];
    }
    [255, 255, 255, alpha]
}

pub fn hex(s: &str) -> [u8; 4] {
    hex_rgba(s, 255)
}

fn to_color(c: [u8; 4]) -> Color {
    Color::from_rgba8(c[0], c[1], c[2], c[3])
}

pub struct Canvas2d {
    pixmap: Pixmap,
}

impl Canvas2d {
    pub fn new(width: u32, height: u32) -> Self {
        let pixmap = Pixmap::new(width.max(1), height.max(1)).expect("valid pixmap size");
        Self { pixmap }
    }

    pub fn width(&self) -> u32 {
        self.pixmap.width()
    }

    pub fn height(&self) -> u32 {
        self.pixmap.height()
    }

    pub fn fill(&mut self, color: [u8; 4]) {
        self.pixmap.fill(to_color(color));
    }

    pub fn fill_rect(&mut self, x: f32, y: f32, w: f32, h: f32, color: [u8; 4]) {
        if w <= 0.0 || h <= 0.0 || !x.is_finite() || !y.is_finite() || !w.is_finite() || !h.is_finite() {
            return;
        }
        let max_w = self.pixmap.width() as f32;
        let max_h = self.pixmap.height() as f32;
        let x0 = x.clamp(0.0, max_w);
        let y0 = y.clamp(0.0, max_h);
        let x1 = (x + w).clamp(0.0, max_w);
        let y1 = (y + h).clamp(0.0, max_h);
        let cw = x1 - x0;
        let ch = y1 - y0;
        if cw < 0.01 || ch < 0.01 {
            return;
        }
        let Some(rect) = Rect::from_xywh(x0, y0, cw, ch) else {
            return;
        };
        let mut paint = Paint::default();
        paint.set_color(to_color(color));
        paint.anti_alias = false;
        self.pixmap.fill_rect(rect, &paint, Transform::identity(), None);
    }

    pub fn round_rect(&mut self, x: f32, y: f32, w: f32, h: f32, r: f32, color: [u8; 4]) {
        let Some(path) = build_round_rect_path(x, y, w, h, r) else {
            return;
        };
        let mut paint = Paint::default();
        paint.set_color(to_color(color));
        paint.anti_alias = true;
        self.pixmap
            .fill_path(&path, &paint, FillRule::Winding, Transform::identity(), None);
    }

    pub fn stroke_round_rect(
        &mut self,
        x: f32,
        y: f32,
        w: f32,
        h: f32,
        r: f32,
        width: f32,
        color: [u8; 4],
    ) {
        let Some(path) = build_round_rect_path(x, y, w, h, r) else {
            return;
        };
        let mut paint = Paint::default();
        paint.set_color(to_color(color));
        paint.anti_alias = true;
        let stroke = Stroke {
            width,
            line_cap: LineCap::Round,
            line_join: LineJoin::Round,
            ..Default::default()
        };
        self.pixmap
            .stroke_path(&path, &paint, &stroke, Transform::identity(), None);
    }

    pub fn stroke_rect(&mut self, x: f32, y: f32, w: f32, h: f32, width: f32, color: [u8; 4]) {
        self.stroke_round_rect(x, y, w, h, 0.0, width, color);
    }

    pub fn fill_circle(&mut self, cx: f32, cy: f32, r: f32, color: [u8; 4]) {
        if r <= 0.0 {
            return;
        }
        let Some(path) = PathBuilder::from_circle(cx, cy, r) else {
            return;
        };
        let mut paint = Paint::default();
        paint.set_color(to_color(color));
        paint.anti_alias = true;
        self.pixmap
            .fill_path(&path, &paint, FillRule::Winding, Transform::identity(), None);
    }

    pub fn fill_ellipse(&mut self, cx: f32, cy: f32, rx: f32, ry: f32, color: [u8; 4]) {
        if rx <= 0.0 || ry <= 0.0 {
            return;
        }
        let Some(rect) = Rect::from_xywh(cx - rx, cy - ry, rx * 2.0, ry * 2.0) else {
            return;
        };
        let Some(path) = PathBuilder::from_oval(rect) else {
            return;
        };
        let mut paint = Paint::default();
        paint.set_color(to_color(color));
        paint.anti_alias = true;
        self.pixmap
            .fill_path(&path, &paint, FillRule::Winding, Transform::identity(), None);
    }

    pub fn stroke_line(&mut self, x0: f32, y0: f32, x1: f32, y1: f32, width: f32, color: [u8; 4]) {
        let mut pb = PathBuilder::new();
        pb.move_to(x0, y0);
        pb.line_to(x1, y1);
        let Some(path) = pb.finish() else {
            return;
        };
        let mut paint = Paint::default();
        paint.set_color(to_color(color));
        paint.anti_alias = true;
        let stroke = Stroke {
            width,
            line_cap: LineCap::Round,
            line_join: LineJoin::Round,
            ..Default::default()
        };
        self.pixmap
            .stroke_path(&path, &paint, &stroke, Transform::identity(), None);
    }

    pub fn fill_polygon(&mut self, pts: &[(f32, f32)], color: [u8; 4]) {
        if pts.len() < 3 {
            return;
        }
        let mut pb = PathBuilder::new();
        pb.move_to(pts[0].0, pts[0].1);
        for &(x, y) in &pts[1..] {
            pb.line_to(x, y);
        }
        pb.close();
        let Some(path) = pb.finish() else {
            return;
        };
        let mut paint = Paint::default();
        paint.set_color(to_color(color));
        paint.anti_alias = true;
        self.pixmap
            .fill_path(&path, &paint, FillRule::Winding, Transform::identity(), None);
    }

    pub fn linear_gradient_rect(
        &mut self,
        x: f32,
        y: f32,
        w: f32,
        h: f32,
        start: (f32, f32),
        end: (f32, f32),
        stops: &[(f32, [u8; 4])],
    ) {
        let Some(rect) = Rect::from_xywh(x, y, w, h) else {
            return;
        };
        let grad_stops: Vec<GradientStop> = stops
            .iter()
            .map(|(pos, col)| GradientStop::new(*pos, to_color(*col)))
            .collect();
        let Some(shader) = LinearGradient::new(
            Point::from_xy(start.0, start.1),
            Point::from_xy(end.0, end.1),
            grad_stops,
            SpreadMode::Pad,
            Transform::identity(),
        ) else {
            return;
        };
        let mut paint = Paint::default();
        paint.shader = shader;
        paint.anti_alias = true;
        self.pixmap.fill_rect(rect, &paint, Transform::identity(), None);
    }

    pub fn radial_gradient_circle(
        &mut self,
        cx: f32,
        cy: f32,
        radius: f32,
        stops: &[(f32, [u8; 4])],
    ) {
        let Some(rect) = Rect::from_xywh(cx - radius, cy - radius, radius * 2.0, radius * 2.0) else {
            return;
        };
        let grad_stops: Vec<GradientStop> = stops
            .iter()
            .map(|(pos, col)| GradientStop::new(*pos, to_color(*col)))
            .collect();
        let Some(shader) = RadialGradient::new(
            Point::from_xy(cx, cy),
            Point::from_xy(cx, cy),
            radius.max(0.01),
            grad_stops,
            SpreadMode::Pad,
            Transform::identity(),
        ) else {
            return;
        };
        let mut paint = Paint::default();
        paint.shader = shader;
        paint.anti_alias = true;
        self.pixmap.fill_rect(rect, &paint, Transform::identity(), None);
    }

    /// Alpha-blends a single straight-RGBA pixel onto the premultiplied pixmap.
    #[inline]
    pub fn blend_pixel(&mut self, x: i32, y: i32, color: [u8; 4]) {
        if x < 0 || y < 0 {
            return;
        }
        let (x, y) = (x as u32, y as u32);
        let (w, h) = (self.pixmap.width(), self.pixmap.height());
        if x >= w || y >= h || color[3] == 0 {
            return;
        }
        let idx = ((y * w + x) * 4) as usize;
        let data = self.pixmap.data_mut();
        let sa = color[3] as u32;
        let inv = 255 - sa;
        let sr = (color[0] as u32 * sa + 127) / 255;
        let sg = (color[1] as u32 * sa + 127) / 255;
        let sb = (color[2] as u32 * sa + 127) / 255;

        data[idx] = (sr + (data[idx] as u32 * inv + 127) / 255).min(255) as u8;
        data[idx + 1] = (sg + (data[idx + 1] as u32 * inv + 127) / 255).min(255) as u8;
        data[idx + 2] = (sb + (data[idx + 2] as u32 * inv + 127) / 255).min(255) as u8;
        data[idx + 3] = (sa + (data[idx + 3] as u32 * inv + 127) / 255).min(255) as u8;
    }

    /// Blits a straight-RGBA surface into destination rectangle `(dx, dy, dw, dh)` with bilinear-ish nearest scaling.
    pub fn blit_rgba(&mut self, src: &RgbaSurface, dx: i32, dy: i32, dw: u32, dh: u32) {
        if src.width == 0 || src.height == 0 || dw == 0 || dh == 0 {
            return;
        }
        for oy in 0..dh {
            let sy = ((oy as u64 * src.height as u64) / dh as u64).min(src.height as u64 - 1) as u32;
            for ox in 0..dw {
                let sx = ((ox as u64 * src.width as u64) / dw as u64).min(src.width as u64 - 1) as u32;
                let si = ((sy * src.width + sx) * 4) as usize;
                let px = [src.rgba[si], src.rgba[si + 1], src.rgba[si + 2], src.rgba[si + 3]];
                if px[3] > 0 {
                    self.blend_pixel(dx + ox as i32, dy + oy as i32, px);
                }
            }
        }
    }

    /// Un-premultiplies `tiny-skia` premultiplied RGBA into standard straight sRGB RGBA8 for GPU / PNG upload.
    pub fn into_surface(self) -> RgbaSurface {
        let width = self.pixmap.width();
        let height = self.pixmap.height();
        let raw = self.pixmap.take();
        let mut rgba = Vec::with_capacity(raw.len());
        for px in raw.chunks_exact(4) {
            let a = px[3];
            if a == 0 {
                rgba.extend_from_slice(&[0, 0, 0, 0]);
            } else if a == 255 {
                rgba.extend_from_slice(px);
            } else {
                let af = a as f32 / 255.0;
                rgba.push(((px[0] as f32 / af).round()).min(255.0) as u8);
                rgba.push(((px[1] as f32 / af).round()).min(255.0) as u8);
                rgba.push(((px[2] as f32 / af).round()).min(255.0) as u8);
                rgba.push(a);
            }
        }
        RgbaSurface {
            width,
            height,
            rgba,
        }
    }
}

fn build_round_rect_path(x: f32, y: f32, w: f32, h: f32, r: f32) -> Option<tiny_skia::Path> {
    if w <= 0.0 || h <= 0.0 {
        return None;
    }
    let rr = r.clamp(0.0, (w * 0.5).min(h * 0.5));
    let mut pb = PathBuilder::new();
    if rr <= 0.001 {
        let rect = Rect::from_xywh(x, y, w, h)?;
        pb.push_rect(rect);
        return pb.finish();
    }
    const K: f32 = 0.552_284_8;
    let c = rr * K;
    pb.move_to(x + rr, y);
    pb.line_to(x + w - rr, y);
    pb.cubic_to(x + w - rr + c, y, x + w, y + rr - c, x + w, y + rr);
    pb.line_to(x + w, y + h - rr);
    pb.cubic_to(
        x + w,
        y + h - rr + c,
        x + w - rr + c,
        y + h,
        x + w - rr,
        y + h,
    );
    pb.line_to(x + rr, y + h);
    pb.cubic_to(x + rr - c, y + h, x, y + h - rr + c, x, y + h - rr);
    pb.line_to(x, y + rr);
    pb.cubic_to(x, y + rr - c, x + rr - c, y, x + rr, y);
    pb.close();
    pb.finish()
}
