//! Zero-allocation 256×144 pixel-art TV console games (`GOAL STARS`, `SAIGON RUSH`, `ROOFTOP FIGHT`)
//! and 128×96 live Arcade invaders renderer.
//!
//! Both `TvGamesSim` and `ArcadeScreenSim` preallocate ONE RGBA8 pixel buffer at construction
//! and mutate it strictly in-place on every frame (zero heap churn per tick).

use std::f32::consts::PI;

pub const TV_W: usize = 256;
pub const TV_H: usize = 144;
pub const TV_BUF_LEN: usize = TV_W * TV_H * 4;

pub const ARCADE_W: usize = 128;
pub const ARCADE_H: usize = 96;
pub const ARCADE_BUF_LEN: usize = ARCADE_W * ARCADE_H * 4;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TvGameKind {
    Title,
    Football,
    Racing,
    Fighting,
}

#[derive(Debug, Clone, Copy, Default)]
pub struct PadInput {
    pub x: f32,
    pub y: f32,
    pub a: bool,
    pub b: bool,
}

/* ---------- Deterministic Mulberry32 PRNG (matches tv-games.js `dice`) ---------- */

#[derive(Clone, Copy)]
struct Rng(u32);

impl Rng {
    fn new(seed: u32) -> Self {
        Self(if seed == 0 { 0x9E37_79B9 } else { seed })
    }

    fn next_f32(&mut self) -> f32 {
        self.0 = self.0.wrapping_add(0x6D2B_79F5);
        let mut t = (self.0 ^ (self.0 >> 15)).wrapping_mul(self.0 | 1);
        t ^= t.wrapping_add((t ^ (t >> 7)).wrapping_mul(t | 61));
        ((t ^ (t >> 14)) as f32) / 4_294_967_296.0
    }
}

/* ---------- Zero-allocation in-place RGBA pixel surface ---------- */

struct PixBuf<'a> {
    data: &'a mut [u8],
    w: i32,
    h: i32,
    clip_y0: i32,
    clip_y1: i32,
    ox: i32,
    oy: i32,
}

impl<'a> PixBuf<'a> {
    fn new(data: &'a mut [u8], w: usize, h: usize) -> Self {
        Self {
            data,
            w: w as i32,
            h: h as i32,
            clip_y0: 0,
            clip_y1: h as i32,
            ox: 0,
            oy: 0,
        }
    }

    fn set_clip_y(&mut self, y0: i32, y1: i32) {
        self.clip_y0 = y0.clamp(0, self.h);
        self.clip_y1 = y1.clamp(self.clip_y0, self.h);
    }

    fn reset_clip(&mut self) {
        self.clip_y0 = 0;
        self.clip_y1 = self.h;
    }

    fn clear(&mut self, col: [u8; 3]) {
        for px in self.data.chunks_exact_mut(4) {
            px[0] = col[0];
            px[1] = col[1];
            px[2] = col[2];
            px[3] = 255;
        }
    }

    #[inline]
    fn put_px(&mut self, x: i32, y: i32, col: [u8; 3]) {
        let px = x + self.ox;
        let py = y + self.oy;
        if px < 0 || px >= self.w || py < self.clip_y0 || py >= self.clip_y1 {
            return;
        }
        let idx = ((py * self.w + px) as usize) * 4;
        self.data[idx] = col[0];
        self.data[idx + 1] = col[1];
        self.data[idx + 2] = col[2];
        self.data[idx + 3] = 255;
    }

    fn fill_rect(&mut self, x: i32, y: i32, w: i32, h: i32, col: [u8; 3]) {
        if w <= 0 || h <= 0 {
            return;
        }
        let x0 = (x + self.ox).clamp(0, self.w);
        let x1 = (x + self.ox + w).clamp(0, self.w);
        let y0 = (y + self.oy).clamp(self.clip_y0, self.clip_y1);
        let y1 = (y + self.oy + h).clamp(self.clip_y0, self.clip_y1);
        if x0 >= x1 || y0 >= y1 {
            return;
        }
        let stride = self.w as usize * 4;
        for py in y0..y1 {
            let row = py as usize * stride;
            for px in x0..x1 {
                let idx = row + px as usize * 4;
                self.data[idx] = col[0];
                self.data[idx + 1] = col[1];
                self.data[idx + 2] = col[2];
                self.data[idx + 3] = 255;
            }
        }
    }

    fn fill_rect_alpha(&mut self, x: i32, y: i32, w: i32, h: i32, col: [u8; 3], alpha: f32) {
        if w <= 0 || h <= 0 || alpha <= 0.01 {
            return;
        }
        let a = (alpha.clamp(0.0, 1.0) * 255.0) as u32;
        let inv = 255 - a;
        let x0 = (x + self.ox).clamp(0, self.w);
        let x1 = (x + self.ox + w).clamp(0, self.w);
        let y0 = (y + self.oy).clamp(self.clip_y0, self.clip_y1);
        let y1 = (y + self.oy + h).clamp(self.clip_y0, self.clip_y1);
        if x0 >= x1 || y0 >= y1 {
            return;
        }
        let stride = self.w as usize * 4;
        for py in y0..y1 {
            let row = py as usize * stride;
            for px in x0..x1 {
                let idx = row + px as usize * 4;
                self.data[idx] = ((col[0] as u32 * a + self.data[idx] as u32 * inv) / 255) as u8;
                self.data[idx + 1] =
                    ((col[1] as u32 * a + self.data[idx + 1] as u32 * inv) / 255) as u8;
                self.data[idx + 2] =
                    ((col[2] as u32 * a + self.data[idx + 2] as u32 * inv) / 255) as u8;
                self.data[idx + 3] = 255;
            }
        }
    }

    fn fill_rect_f(&mut self, x: f32, y: f32, w: f32, h: f32, col: [u8; 3]) {
        self.fill_rect(
            x.round() as i32,
            y.round() as i32,
            w.round().max(1.0) as i32,
            h.round().max(1.0) as i32,
            col,
        );
    }

    fn fill_circle(&mut self, cx: i32, cy: i32, r: i32, col: [u8; 3]) {
        let r = r.max(1);
        let r2 = (r * r) as f32;
        for dy in -r..=r {
            let hw = (r2 - (dy * dy) as f32).max(0.0).sqrt().round() as i32;
            self.fill_rect(cx - hw, cy + dy, hw * 2 + 1, 1, col);
        }
    }

    fn fill_vgrad(&mut self, y0: i32, y1: i32, c0: [u8; 3], c1: [u8; 3], c2: [u8; 3], mid: f32) {
        let span = (y1 - y0).max(1) as f32;
        for y in y0..y1 {
            let p = ((y - y0) as f32 / span).clamp(0.0, 1.0);
            let col = if p < mid {
                lerp_rgb(c0, c1, p / mid.max(0.01))
            } else {
                lerp_rgb(c1, c2, (p - mid) / (1.0 - mid).max(0.01))
            };
            self.fill_rect(0, y, self.w, 1, col);
        }
    }

    /// Horizontal-edge trapezoid scanline filler (y_top <= y_bot) for pseudo-3D road strips.
    #[allow(clippy::too_many_arguments)]
    fn fill_trapezoid(
        &mut self,
        x_bot_l: f32,
        x_bot_r: f32,
        y_bot: i32,
        x_top_l: f32,
        x_top_r: f32,
        y_top: i32,
        col: [u8; 3],
    ) {
        if y_bot <= y_top {
            return;
        }
        let dy = (y_bot - y_top) as f32;
        let ys = y_top.max(self.clip_y0 - self.oy);
        let ye = y_bot.min(self.clip_y1 - self.oy);
        for y in ys..ye {
            let t = (y - y_top) as f32 / dy;
            let xl = (x_top_l + (x_bot_l - x_top_l) * t).round() as i32;
            let xr = (x_top_r + (x_bot_r - x_top_r) * t).round() as i32;
            if xr > xl {
                self.fill_rect(xl, y, xr - xl, 1, col);
            }
        }
    }

    #[allow(clippy::too_many_arguments)]
    fn draw_text(
        &mut self,
        s: &str,
        x: i32,
        y: i32,
        col: [u8; 3],
        sc: i32,
        align: Align,
        shadow: Option<[u8; 3]>,
    ) {
        let sc = sc.max(1);
        let len = s.bytes().len() as i32;
        if len == 0 {
            return;
        }
        let w = len * 4 * sc - sc;
        let bx = match align {
            Align::Left => x,
            Align::Center => x - w / 2,
            Align::Right => x - w,
        };
        if let Some(sh) = shadow {
            let off = (sc >> 1).max(1);
            self.draw_text_raw(s, bx + off, y + off, sh, sc);
        }
        self.draw_text_raw(s, bx, y, col, sc);
    }

    fn draw_text_raw(&mut self, s: &str, ox: i32, oy: i32, col: [u8; 3], sc: i32) {
        for (i, ch) in s.bytes().enumerate() {
            let glyph = glyph_3x5(ch.to_ascii_uppercase());
            let gx = ox + (i as i32) * 4 * sc;
            for (r, &bits) in glyph.iter().enumerate() {
                if bits == 0 {
                    continue;
                }
                let gy = oy + (r as i32) * sc;
                if bits == 7 {
                    self.fill_rect(gx, gy, 3 * sc, sc, col);
                } else if bits == 6 {
                    self.fill_rect(gx, gy, 2 * sc, sc, col);
                } else if bits == 3 {
                    self.fill_rect(gx + sc, gy, 2 * sc, sc, col);
                } else {
                    if bits & 4 != 0 {
                        self.fill_rect(gx, gy, sc, sc, col);
                    }
                    if bits & 2 != 0 {
                        self.fill_rect(gx + sc, gy, sc, sc, col);
                    }
                    if bits & 1 != 0 {
                        self.fill_rect(gx + 2 * sc, gy, sc, sc, col);
                    }
                }
            }
        }
    }

    fn draw_banner(&mut self, s: &str, y: i32, col: [u8; 3], sc: i32) {
        let out = [20, 16, 28];
        let o = ((sc + 1) / 2).max(1);
        let cx = self.w / 2;
        for (dx, dy) in [(-1, 0), (1, 0), (0, -1), (0, 1), (1, 1), (-1, 1)] {
            self.draw_text(s, cx + dx * o, y + dy * o, out, sc, Align::Center, None);
        }
        self.draw_text(s, cx, y, col, sc, Align::Center, None);
    }
}

#[derive(Clone, Copy)]
enum Align {
    Left,
    Center,
    Right,
}

fn glyph_3x5(c: u8) -> [u8; 5] {
    match c {
        b'A' => [2, 5, 7, 5, 5],
        b'B' => [6, 5, 6, 5, 6],
        b'C' => [3, 4, 4, 4, 3],
        b'D' => [6, 5, 5, 5, 6],
        b'E' => [7, 4, 6, 4, 7],
        b'F' => [7, 4, 6, 4, 4],
        b'G' => [3, 4, 5, 5, 3],
        b'H' => [5, 5, 7, 5, 5],
        b'I' => [7, 2, 2, 2, 7],
        b'J' => [1, 1, 1, 5, 2],
        b'K' => [5, 5, 6, 5, 5],
        b'L' => [4, 4, 4, 4, 7],
        b'M' => [5, 7, 7, 5, 5],
        b'N' => [6, 5, 5, 5, 5],
        b'O' => [7, 5, 5, 5, 7],
        b'P' => [6, 5, 6, 4, 4],
        b'Q' => [2, 5, 5, 6, 3],
        b'R' => [6, 5, 6, 5, 5],
        b'S' => [3, 4, 7, 1, 6],
        b'T' => [7, 2, 2, 2, 2],
        b'U' => [5, 5, 5, 5, 7],
        b'V' => [5, 5, 5, 5, 2],
        b'W' => [5, 5, 7, 7, 5],
        b'X' => [5, 5, 2, 5, 5],
        b'Y' => [5, 5, 2, 2, 2],
        b'Z' => [7, 1, 2, 4, 7],
        b'0' => [7, 5, 5, 5, 7],
        b'1' => [2, 6, 2, 2, 7],
        b'2' => [6, 1, 2, 4, 7],
        b'3' => [6, 1, 2, 1, 6],
        b'4' => [5, 5, 7, 1, 1],
        b'5' => [7, 4, 6, 1, 6],
        b'6' => [3, 4, 7, 5, 7],
        b'7' => [7, 1, 2, 2, 2],
        b'8' => [7, 5, 7, 5, 7],
        b'9' => [7, 5, 7, 1, 6],
        b'!' => [2, 2, 2, 0, 2],
        b'.' => [0, 0, 0, 0, 2],
        b':' => [0, 2, 0, 2, 0],
        b'\'' => [2, 2, 0, 0, 0],
        b'/' => [1, 1, 2, 4, 4],
        b'-' => [0, 0, 7, 0, 0],
        b'+' => [0, 2, 7, 2, 0],
        b' ' => [0, 0, 0, 0, 0],
        _ => [6, 1, 2, 0, 2],
    }
}

fn shade(c: [u8; 3], f: f32) -> [u8; 3] {
    let apply = |v: u8| -> u8 {
        let fv = v as f32;
        let out = if f < 0.0 {
            fv * (1.0 + f)
        } else {
            fv + (255.0 - fv) * f
        };
        out.clamp(0.0, 255.0).round() as u8
    };
    [apply(c[0]), apply(c[1]), apply(c[2])]
}

fn lerp_rgb(a: [u8; 3], b: [u8; 3], p: f32) -> [u8; 3] {
    let p = p.clamp(0.0, 1.0);
    [
        (a[0] as f32 + (b[0] as f32 - a[0] as f32) * p).round() as u8,
        (a[1] as f32 + (b[1] as f32 - a[1] as f32) * p).round() as u8,
        (a[2] as f32 + (b[2] as f32 - a[2] as f32) * p).round() as u8,
    ]
}

fn lerp(a: f32, b: f32, p: f32) -> f32 {
    a + (b - a) * p
}

/// Zero-allocation stack string formatter for scoreboard/HUD numbers.
struct StackStr {
    buf: [u8; 32],
    len: usize,
}

impl StackStr {
    fn new() -> Self {
        Self {
            buf: [0; 32],
            len: 0,
        }
    }
    fn push_byte(&mut self, b: u8) {
        if self.len < self.buf.len() {
            self.buf[self.len] = b;
            self.len += 1;
        }
    }
    fn push_str(&mut self, s: &str) {
        for &b in s.as_bytes() {
            self.push_byte(b);
        }
    }
    fn push_u32(&mut self, mut n: u32, min_digits: usize) {
        let mut tmp = [0u8; 10];
        let mut k = 0usize;
        if n == 0 {
            tmp[0] = b'0';
            k = 1;
        } else {
            while n > 0 && k < 10 {
                tmp[k] = b'0' + (n % 10) as u8;
                n /= 10;
                k += 1;
            }
        }
        while k < min_digits && k < 10 {
            tmp[k] = b'0';
            k += 1;
        }
        for i in (0..k).rev() {
            self.push_byte(tmp[i]);
        }
    }
    fn as_str(&self) -> &str {
        std::str::from_utf8(&self.buf[..self.len]).unwrap_or("")
    }
}

/* ---------- Shared City Skyline Backdrop (precomputed once) ---------- */

#[derive(Clone, Copy)]
struct Building {
    x: i32,
    w: i32,
    h: i32,
    seed: u32,
}

fn build_skyline(seed: u32, span: i32, min_h: i32, max_h: i32) -> Vec<Building> {
    let mut r = Rng::new(seed);
    let mut out = Vec::with_capacity(48);
    let mut x = -20;
    while x < span {
        let w = 8 + (r.next_f32() * 18.0) as i32;
        let h = min_h + (r.next_f32() * (max_h - min_h) as f32) as i32;
        out.push(Building {
            x,
            w,
            h,
            seed: (r.next_f32() * 65535.0) as u32,
        });
        x += w + 1 + (r.next_f32() * 4.0) as i32;
    }
    out
}

/* ---------- 1. FOOTBALL ("GOAL STARS", 3v3 pitch) ---------- */

const FB_P0: f32 = 8.0;
const FB_P1: f32 = 248.0;
const FB_T0: f32 = 20.0;
const FB_T1: f32 = 136.0;
const FB_MID: f32 = 78.0;
const FB_GOAL: f32 = 32.0;
const FB_CX: f32 = 128.0;
const FB_FORM: [[f32; 2]; 3] = [[0.03, 0.50], [0.26, 0.30], [0.44, 0.68]];
const TEAM_KIT: [[u8; 3]; 2] = [[232, 116, 42], [63, 124, 196]];
const SKINS: [[u8; 3]; 4] = [
    [242, 201, 160],
    [217, 164, 122],
    [184, 127, 90],
    [245, 213, 181],
];
const HAIRS: [[u8; 3]; 4] = [[43, 33, 24], [74, 48, 32], [21, 21, 21], [138, 90, 43]];

#[derive(Clone, Copy)]
struct FbMan {
    team: usize,
    k: usize,
    keeper: bool,
    x: f32,
    y: f32,
    vx: f32,
    vy: f32,
    fx: f32,
    fy: f32,
    run: f32,
    cd: f32,
    dive: f32,
    skin: [u8; 3],
    hair: [u8; 3],
}

#[derive(Clone, Copy)]
struct FbBall {
    x: f32,
    y: f32,
    z: f32,
    vx: f32,
    vy: f32,
    vz: f32,
    owner: Option<usize>,
    by_team: Option<usize>,
    spin: f32,
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum FbPhase {
    Intro,
    Play,
    Goal,
    End,
}

struct FootballState {
    rng: Rng,
    men: [FbMan; 6],
    ball: FbBall,
    ctl: [usize; 2],
    think: [f32; 2],
    prev_a: [bool; 2],
    prev_b: [bool; 2],
    phase: FbPhase,
    phase_t: f32,
    slow: f32,
    scorer: usize,
    hold: f32,
    score: [u32; 2],
    match_t: f32,
    t: f32,
    shake: f32,
    flash: f32,
    crowd: Vec<(i32, i32, [u8; 3])>,
}

impl FootballState {
    fn new(seed: u32) -> Self {
        let mut rng = Rng::new(seed);
        let mut men = [FbMan {
            team: 0,
            k: 0,
            keeper: true,
            x: 0.0,
            y: 0.0,
            vx: 0.0,
            vy: 0.0,
            fx: 1.0,
            fy: 0.0,
            run: 0.0,
            cd: 0.0,
            dive: 0.0,
            skin: SKINS[0],
            hair: HAIRS[0],
        }; 6];
        let mut idx = 0;
        for team in 0..2 {
            for k in 0..3 {
                men[idx] = FbMan {
                    team,
                    k,
                    keeper: k == 0,
                    x: 0.0,
                    y: 0.0,
                    vx: 0.0,
                    vy: 0.0,
                    fx: if team == 1 { -1.0 } else { 1.0 },
                    fy: 0.0,
                    run: 0.0,
                    cd: 0.0,
                    dive: 0.0,
                    skin: SKINS[(rng.next_f32() * 4.0) as usize & 3],
                    hair: HAIRS[(rng.next_f32() * 4.0) as usize & 3],
                };
                idx += 1;
            }
        }
        let mut cr = Rng::new(7);
        let crowd_cols = [
            [232, 116, 42],
            [63, 124, 196],
            [242, 237, 228],
            [230, 197, 71],
            [201, 79, 124],
            [127, 179, 232],
            [155, 208, 140],
        ];
        let mut crowd = Vec::with_capacity(140);
        for _ in 0..140 {
            crowd.push((
                (cr.next_f32() * 128.0) as i32 * 2,
                10 + (cr.next_f32() * 4.0) as i32 * 2,
                crowd_cols[(cr.next_f32() * crowd_cols.len() as f32) as usize % crowd_cols.len()],
            ));
        }
        let mut s = Self {
            rng,
            men,
            ball: FbBall {
                x: FB_CX,
                y: FB_MID,
                z: 0.0,
                vx: 0.0,
                vy: 0.0,
                vz: 0.0,
                owner: None,
                by_team: None,
                spin: 0.0,
            },
            ctl: [2, 5],
            think: [0.0, 0.0],
            prev_a: [false, false],
            prev_b: [false, false],
            phase: FbPhase::Intro,
            phase_t: 0.6,
            slow: 0.0,
            scorer: 0,
            hold: 0.0,
            score: [0, 0],
            match_t: 0.0,
            t: 0.0,
            shake: 0.0,
            flash: 0.0,
            crowd,
        };
        let first = if s.rng.next_f32() < 0.5 { 0 } else { 1 };
        s.kickoff(first);
        s
    }

    fn own_x(team: usize) -> f32 {
        if team == 1 { FB_P1 } else { FB_P0 }
    }
    fn aim_x(team: usize) -> f32 {
        if team == 1 { FB_P0 } else { FB_P1 }
    }
    fn dir(team: usize) -> f32 {
        if team == 1 { -1.0 } else { 1.0 }
    }
    fn home(m: &FbMan) -> (f32, f32) {
        let [fx, fy] = FB_FORM[m.k];
        (
            Self::own_x(m.team) + Self::dir(m.team) * fx * (FB_P1 - FB_P0),
            FB_T0 + fy * (FB_T1 - FB_T0),
        )
    }

    fn kickoff(&mut self, team: usize) {
        for m in &mut self.men {
            let (hx, hy) = Self::home(m);
            m.x = hx;
            m.y = hy;
            m.vx = 0.0;
            m.vy = 0.0;
            m.cd = 0.0;
            m.dive = 0.0;
            m.fx = Self::dir(m.team);
            m.fy = 0.0;
        }
        let fwd_idx = team * 3 + 2;
        self.men[fwd_idx].x = FB_CX - Self::dir(team) * 4.0;
        self.men[fwd_idx].y = FB_MID;
        self.ball = FbBall {
            x: FB_CX,
            y: FB_MID,
            z: 0.0,
            vx: 0.0,
            vy: 0.0,
            vz: 0.0,
            owner: Some(fwd_idx),
            by_team: Some(team),
            spin: 0.0,
        };
        self.hold = 0.0;
    }

    fn nearest(&self, team: usize, x: f32, y: f32, include_keeper: bool) -> usize {
        let mut best = team * 3 + 2;
        let mut bd = 1e9_f32;
        for (i, m) in self.men.iter().enumerate() {
            if m.team == team && (include_keeper || !m.keeper) {
                let d = (m.x - x).hypot(m.y - y);
                if d < bd {
                    bd = d;
                    best = i;
                }
            }
        }
        best
    }

    fn kick(&mut self, idx: usize, tx: f32, ty: f32, speed: f32, lift: f32) {
        let d = (tx - self.ball.x).hypot(ty - self.ball.y).max(1.0);
        self.ball.owner = None;
        self.ball.by_team = Some(self.men[idx].team);
        self.ball.vx = (tx - self.ball.x) / d * speed;
        self.ball.vy = (ty - self.ball.y) / d * speed;
        self.ball.vz = lift;
        self.ball.spin = speed / 22.0;
        self.men[idx].cd = 0.3;
    }

    fn pass(&mut self, idx: usize, long: bool) {
        let m = self.men[idx];
        let d_sign = Self::dir(m.team);
        let mut best = idx;
        let mut bs = -1e9_f32;
        for (j, t) in self.men.iter().enumerate() {
            if t.team != m.team || j == idx {
                continue;
            }
            let fwd = (t.x - m.x) * d_sign;
            let mut room = 1e9_f32;
            for e in &self.men {
                if e.team != m.team {
                    room = room.min((e.x - t.x).hypot(e.y - t.y));
                }
            }
            let s = fwd * if long { 1.0 } else { 0.5 }
                + room.min(40.0)
                - if t.keeper { 60.0 } else { 0.0 }
                + self.rng.next_f32() * 16.0;
            if s > bs {
                bs = s;
                best = j;
            }
        }
        let bt = self.men[best];
        let dist = (m.x - bt.x).hypot(m.y - bt.y);
        let sp = (dist * 1.9 + 50.0).clamp(75.0, 175.0);
        self.kick(
            idx,
            bt.x + bt.vx * 0.35 + d_sign * 4.0,
            bt.y + bt.vy * 0.35,
            sp,
            if long { 72.0 } else { 18.0 },
        );
    }

    fn shoot(&mut self, idx: usize) {
        let m = self.men[idx];
        let gx = Self::aim_x(m.team);
        let far = (gx - m.x).abs() / 240.0;
        let ty = FB_MID + (self.rng.next_f32() - 0.5) * FB_GOAL * (0.75 + far * 1.2);
        let sp = 165.0 + self.rng.next_f32() * 48.0;
        let lift = 20.0 + self.rng.next_f32() * 85.0 * far;
        self.kick(idx, gx + Self::dir(m.team) * 3.0, ty, sp, lift);
        self.flash = self.flash.max(0.12);
    }

    fn ai(&mut self, team: usize) -> PadInput {
        let idx = self.ctl[team];
        let mut c = PadInput::default();
        if self.phase != FbPhase::Play {
            return c;
        }
        let m = self.men[idx];
        let d_sign = Self::dir(team);
        let gx = Self::aim_x(team);
        let (tx, ty) = if self.ball.owner == Some(idx) {
            let mut ty = FB_MID + (self.t * 0.8 + (team as f32) * 3.0).sin() * 18.0;
            let opp_idx = self.nearest(1 - team, m.x + d_sign * 10.0, m.y, true);
            let opp = self.men[opp_idx];
            let od = (opp.x - m.x).hypot(opp.y - m.y);
            let close = od < 22.0 && (opp.x - m.x) * d_sign > -3.0;
            if close {
                ty = m.y + if m.y > opp.y { 24.0 } else { -24.0 };
            }
            if self.t > self.think[team] {
                self.think[team] = self.t + 0.25 + self.rng.next_f32() * 0.2;
                let far = (gx - m.x).abs();
                if far < 72.0 && self.rng.next_f32() < 0.72 {
                    c.b = true;
                } else if close && od < 12.0 && self.rng.next_f32() < 0.55 {
                    c.a = true;
                } else if far > 120.0 && self.rng.next_f32() < 0.14 {
                    c.a = true;
                }
            }
            (gx, ty)
        } else {
            let mut tx = self.ball.x + self.ball.vx * 0.28;
            let mut ty = self.ball.y + self.ball.vy * 0.28;
            if let Some(o_idx) = self.ball.owner {
                let o = self.men[o_idx];
                if o.team == team {
                    tx = m.x + d_sign * 28.0;
                    ty = lerp(m.y, FB_MID, 0.3);
                } else if (o.x - m.x).hypot(o.y - m.y) < 10.0 && self.t > self.think[team] {
                    self.think[team] = self.t + 0.35;
                    c.a = self.rng.next_f32() < 0.6;
                }
            }
            (tx, ty)
        };
        let dx = tx - m.x;
        let dy = ty - m.y;
        let l = dx.hypot(dy);
        if l > 1.5 {
            c.x = dx / l;
            c.y = dy / l;
        }
        c
    }

    fn step_fixed(&mut self, mut h: f32, p1: Option<PadInput>, p2: Option<PadInput>) {
        self.t += h;
        match self.phase {
            FbPhase::End => {
                self.phase_t -= h;
                if self.phase_t <= 0.0 {
                    self.score = [0, 0];
                    self.match_t = 0.0;
                    self.phase = FbPhase::Intro;
                    self.phase_t = 0.8;
                    self.kickoff(0);
                }
                return;
            }
            FbPhase::Intro => {
                self.phase_t -= h;
                if self.phase_t <= 0.0 {
                    self.phase = FbPhase::Play;
                }
                return;
            }
            FbPhase::Goal => {
                self.phase_t -= h;
                if self.slow > 0.0 {
                    self.slow -= h;
                    h *= 0.3;
                }
                if self.phase_t <= 0.0 {
                    let next = 1 - self.scorer;
                    self.kickoff(next);
                    self.phase = FbPhase::Play;
                }
            }
            FbPhase::Play => {
                self.match_t += h;
                if self.match_t >= 90.0 {
                    self.match_t = 90.0;
                    self.phase = FbPhase::End;
                    self.phase_t = 4.0;
                    self.flash = 0.45;
                    return;
                }
            }
        }

        let play = self.phase == FbPhase::Play;
        for team in 0..2 {
            let has = self
                .ball
                .owner
                .filter(|&o| self.men[o].team == team && !self.men[o].keeper);
            self.ctl[team] = has.unwrap_or_else(|| {
                self.nearest(
                    team,
                    self.ball.x + self.ball.vx * 0.25,
                    self.ball.y + self.ball.vy * 0.25,
                    false,
                )
            });
        }

        let c0 = p1.unwrap_or_else(|| self.ai(0));
        let c1 = p2.unwrap_or_else(|| self.ai(1));
        let cs = [c0, c1];
        let mut pa = [false; 2];
        let mut pb = [false; 2];
        for i in 0..2 {
            pa[i] = cs[i].a && !self.prev_a[i];
            pb[i] = cs[i].b && !self.prev_b[i];
            self.prev_a[i] = cs[i].a;
            self.prev_b[i] = cs[i].b;
        }

        // Move players
        for i in 0..6 {
            self.men[i].cd = (self.men[i].cd - h).max(0.0);
            self.men[i].dive = (self.men[i].dive - h).max(0.0);
            let m = self.men[i];
            let sp = if self.ball.owner == Some(i) { 36.0 } else { 42.0 };
            let (wx, wy) = if play && self.ctl[m.team] == i {
                let sx = cs[m.team].x.clamp(-1.0, 1.0);
                let sy = cs[m.team].y.clamp(-1.0, 1.0);
                let l = sx.hypot(sy);
                if l > 0.15 {
                    (sx / l.max(1.0) * sp, sy / l.max(1.0) * sp)
                } else {
                    (0.0, 0.0)
                }
            } else {
                let (tx, ty, s2) = self.target_for(i);
                let dx = tx - m.x;
                let dy = ty - m.y;
                let l = dx.hypot(dy);
                if l > 1.2 {
                    let v = s2.min(l * 4.0);
                    (dx / l * v, dy / l * v)
                } else {
                    (0.0, 0.0)
                }
            };
            let acc = (h * 9.0).min(1.0);
            self.men[i].vx += (wx - self.men[i].vx) * acc;
            self.men[i].vy += (wy - self.men[i].vy) * acc;
            self.men[i].x = (self.men[i].x + self.men[i].vx * h).clamp(FB_P0 - 4.0, FB_P1 + 4.0);
            self.men[i].y = (self.men[i].y + self.men[i].vy * h).clamp(FB_T0 + 1.0, FB_T1 - 1.0);
            let v = self.men[i].vx.hypot(self.men[i].vy);
            if v > 4.0 {
                self.men[i].fx = self.men[i].vx / v;
                self.men[i].fy = self.men[i].vy / v;
                self.men[i].run += h * v / 9.0;
            }
        }

        // Ball ownership / physics
        if let Some(o_idx) = self.ball.owner {
            let m = self.men[o_idx];
            self.ball.x = m.x + m.fx * 3.5;
            self.ball.y = m.y + m.fy * 2.2 + 0.5;
            self.ball.z = 0.0;
            self.ball.vx = m.vx;
            self.ball.vy = m.vy;
            self.ball.spin = m.vx.hypot(m.vy) / 18.0;
            if m.keeper {
                self.hold += h;
                if self.hold > 0.9 {
                    self.hold = 0.0;
                    self.pass(o_idx, true);
                }
            } else if play {
                if pb[m.team] {
                    self.shoot(o_idx);
                } else if pa[m.team] {
                    self.pass(o_idx, false);
                }
            }
            if self.ball.owner == Some(o_idx) && play && !m.keeper {
                for e_idx in 0..6 {
                    let e = self.men[e_idx];
                    if e.team == m.team || e.keeper || e.cd > 0.0 {
                        continue;
                    }
                    if (e.x - m.x).hypot(e.y - m.y) > 7.0 {
                        continue;
                    }
                    let tried = pa[e.team] && self.ctl[e.team] == e_idx;
                    if self.rng.next_f32() < if tried { 0.55 } else { h * 1.4 } {
                        self.ball.owner = Some(e_idx);
                        self.ball.by_team = Some(e.team);
                        self.men[o_idx].cd = 0.7;
                        self.men[e_idx].cd = 0.15;
                        break;
                    }
                    if tried {
                        self.men[e_idx].cd = 0.45;
                    }
                }
            }
        } else {
            self.ball.x += self.ball.vx * h;
            self.ball.y += self.ball.vy * h;
            if self.ball.z > 0.0 || self.ball.vz > 0.0 {
                self.ball.vz -= 280.0 * h;
                self.ball.z += self.ball.vz * h;
                if self.ball.z <= 0.0 {
                    self.ball.z = 0.0;
                    self.ball.vz = if self.ball.vz < -35.0 {
                        -self.ball.vz * 0.4
                    } else {
                        0.0
                    };
                }
            }
            let fr = if self.ball.z > 0.0 { 0.1 } else { 1.25 };
            let damp = (1.0 - fr * h).max(0.0);
            self.ball.vx *= damp;
            self.ball.vy *= damp;
            self.ball.spin *= (1.0 - h).max(0.0);
            if self.ball.y < FB_T0 + 2.0 || self.ball.y > FB_T1 - 2.0 {
                self.ball.y = self.ball.y.clamp(FB_T0 + 2.0, FB_T1 - 2.0);
                self.ball.vy *= -0.7;
            }
            for s in 0..2 {
                let lx = if s == 1 { FB_P1 } else { FB_P0 };
                let past = if s == 1 {
                    self.ball.x > lx
                } else {
                    self.ball.x < lx
                };
                if !past {
                    continue;
                }
                let mouth = (self.ball.y - FB_MID).abs() < FB_GOAL * 0.5 - 1.0 && self.ball.z < 10.0;
                if mouth && play {
                    let team = if s == 1 { 0 } else { 1 };
                    self.score[team] += 1;
                    self.scorer = team;
                    self.phase = FbPhase::Goal;
                    self.phase_t = 2.6;
                    self.slow = 0.8;
                    self.flash = 0.9;
                    self.shake = 5.0;
                } else if mouth || (self.phase == FbPhase::Goal && (self.ball.y - FB_MID).abs() < FB_GOAL * 0.5 + 2.0) {
                    let back = lx + if s == 1 { 6.0 } else { -6.0 };
                    if (s == 1 && self.ball.x > back) || (s == 0 && self.ball.x < back) {
                        self.ball.x = back;
                        self.ball.vx *= -0.15;
                        self.ball.vy *= 0.5;
                    }
                } else {
                    self.ball.x = lx + if s == 1 { -1.0 } else { 1.0 };
                    self.ball.vx *= -0.6;
                }
            }
            if play && self.ball.z < 6.0 {
                let fast = self.ball.vx.hypot(self.ball.vy);
                for i in 0..6 {
                    let m = self.men[i];
                    let rad = if m.keeper { 7.5 } else { 4.8 };
                    if m.cd > 0.0 || (m.x - self.ball.x).hypot(m.y - self.ball.y) > rad {
                        continue;
                    }
                    if m.keeper && self.ball.by_team != Some(m.team) && fast > 80.0 {
                        self.men[i].cd = 0.4;
                        if self.rng.next_f32() > (0.72 - fast / 600.0 + if m.dive > 0.0 { 0.1 } else { 0.0 }).clamp(0.25, 0.8) {
                            continue;
                        }
                        if fast > 140.0 && self.rng.next_f32() < 0.55 {
                            self.ball.vx *= -0.45;
                            self.ball.vy = (self.rng.next_f32() - 0.5) * 120.0;
                            self.ball.vz = 42.0;
                            self.ball.by_team = Some(m.team);
                            self.shake = 2.0;
                            break;
                        }
                    }
                    self.ball.owner = Some(i);
                    self.ball.by_team = Some(m.team);
                    if m.keeper {
                        self.hold = 0.0;
                    }
                    break;
                }
            }
        }
    }

    fn target_for(&mut self, i: usize) -> (f32, f32, f32) {
        let m = self.men[i];
        let d_sign = Self::dir(m.team);
        let (hx, hy) = Self::home(&m);
        if self.phase == FbPhase::Goal {
            return if m.team == self.scorer {
                (
                    Self::aim_x(m.team) - d_sign * 10.0,
                    if m.k == 1 { FB_T0 + 10.0 } else { FB_T1 - 10.0 - m.k as f32 * 4.0 },
                    48.0,
                )
            } else {
                (hx, hy, 22.0)
            };
        }
        if m.keeper {
            if self.ball.owner == Some(i) {
                return (m.x, m.y, 40.0);
            }
            let lx = Self::own_x(m.team) + d_sign * 4.0;
            let mut ty = lerp(FB_MID, self.ball.y, 0.55)
                .clamp(FB_MID - FB_GOAL * 0.5 + 2.0, FB_MID + FB_GOAL * 0.5 - 2.0);
            let mut sp = 36.0;
            if self.ball.vx * d_sign < -40.0
                && (self.ball.x - lx).abs() < 75.0
                && self.ball.owner.is_none()
            {
                ty = (self.ball.y + self.ball.vy * ((lx - self.ball.x) / self.ball.vx))
                    .clamp(FB_MID - FB_GOAL * 0.5 - 2.0, FB_MID + FB_GOAL * 0.5 + 2.0);
                sp = 64.0;
                if (ty - m.y).abs() > 4.0 && (self.ball.x - lx).abs() < 34.0 {
                    self.men[i].dive = 0.35;
                }
            }
            return (lx, ty, sp);
        }
        let has = self.ball.owner.is_some_and(|o| self.men[o].team == m.team);
        let tx = (hx + (self.ball.x - FB_CX) * 0.5 + if has { d_sign * if m.k == 2 { 34.0 } else { 14.0 } } else { 0.0 })
            .clamp(FB_P0 + 6.0, FB_P1 - 6.0);
        let ty = lerp(hy, self.ball.y, 0.28).clamp(FB_T0 + 6.0, FB_T1 - 6.0);
        (tx, ty, 42.0)
    }

    fn draw(&self, fb: &mut PixBuf<'_>) {
        // Crowd stand
        fb.fill_rect(0, 9, TV_W as i32, 11, [20, 26, 36]);
        let tick = (self.t * 5.0) as i32;
        let jump = self.phase == FbPhase::Goal;
        for (i, &(x, y, c)) in self.crowd.iter().enumerate() {
            if (i as i32 * 7 + tick) % 17 == 0 {
                continue;
            }
            let dy = if jump && ((i as i32 + (self.t * 10.0) as i32) & 1) != 0 {
                -1
            } else {
                0
            };
            fb.fill_rect(x, y + dy, 2, 2, c);
        }
        // Mown pitch stripes
        let mut x = 0;
        let mut k = 0;
        while x < TV_W as i32 {
            let col = if k % 2 == 1 {
                [61, 151, 70]
            } else {
                [70, 166, 80]
            };
            fb.fill_rect(x, 18, 16, 120, col);
            x += 16;
            k += 1;
        }
        // Pitch lines
        let line_c = [232, 242, 228];
        let p0 = FB_P0 as i32;
        let p1 = FB_P1 as i32;
        let t0 = FB_T0 as i32;
        let t1 = FB_T1 as i32;
        let mid = FB_MID as i32;
        let cx = FB_CX as i32;
        let gh = (FB_GOAL * 0.5) as i32;
        // Outer rect + center line
        fb.fill_rect(p0, t0, p1 - p0, 1, line_c);
        fb.fill_rect(p0, t1, p1 - p0 + 1, 1, line_c);
        fb.fill_rect(p0, t0, 1, t1 - t0, line_c);
        fb.fill_rect(p1, t0, 1, t1 - t0, line_c);
        fb.fill_rect(cx, t0, 1, t1 - t0, line_c);
        // Center circle
        for a in (0..360).step_by(6) {
            let rad = (a as f32) * PI / 180.0;
            fb.put_px(
                cx + (rad.cos() * 15.0).round() as i32,
                mid + (rad.sin() * 15.0).round() as i32,
                line_c,
            );
        }
        fb.fill_rect(cx - 1, mid - 1, 3, 3, line_c);
        // Penalty boxes
        for (bx, bw, bh) in [
            (p0, 30, 54),
            (p1 - 30, 30, 54),
            (p0, 11, 30),
            (p1 - 11, 11, 30),
        ] {
            let by = mid - bh / 2;
            fb.fill_rect(bx, by, bw, 1, line_c);
            fb.fill_rect(bx, by + bh, bw + 1, 1, line_c);
            fb.fill_rect(bx, by, 1, bh, line_c);
            fb.fill_rect(bx + bw, by, 1, bh, line_c);
        }
        // Goals
        for s in 0..2 {
            let gx = if s == 1 { p1 + 1 } else { p0 - 6 };
            fb.fill_rect_alpha(gx, mid - gh, 6, gh * 2, [240, 244, 248], 0.2);
            for gy in (mid - gh..=mid + gh).step_by(3) {
                fb.fill_rect_alpha(gx, gy, 6, 1, [240, 244, 248], 0.45);
            }
            for gxx in (gx..gx + 6).step_by(2) {
                fb.fill_rect_alpha(gxx, mid - gh, 1, gh * 2, [240, 244, 248], 0.45);
            }
            let px = if s == 1 { p1 } else { p0 - 1 };
            fb.fill_rect(px, mid - gh - 1, 2, 2, [255, 255, 255]);
            fb.fill_rect(px, mid + gh, 2, 2, [255, 255, 255]);
        }

        // Bottom scrolling LED ad board
        fb.fill_rect(0, 137, TV_W as i32, 7, [16, 22, 30]);
        let ad = "MY OFFICE  +  PHO 24  +  CLAUDE CODE  +  BANH MI  +  ";
        let aw = (ad.len() as i32) * 4;
        let off = ((self.t * 14.0) as i32).rem_euclid(aw);
        let mut ax = -off;
        while ax < TV_W as i32 {
            fb.draw_text(ad, ax, 138, [242, 166, 90], 1, Align::Left, None);
            ax += aw;
        }

        // Y-sorted players & ball (fixed 7-element sort on stack, zero alloc)
        let mut order: [(f32, u8); 7] = [(0.0, 0); 7];
        for (i, m) in self.men.iter().enumerate() {
            order[i] = (m.y, i as u8);
        }
        order[6] = (self.ball.y, 6);
        order.sort_by(|a, b| a.0.total_cmp(&b.0));

        for &(_, id) in &order {
            if id == 6 {
                let bx = self.ball.x.round() as i32;
                let by = self.ball.y.round() as i32;
                let bz = self.ball.z.round() as i32;
                fb.fill_rect_alpha(bx - 1, by + 1, 3, 1, [0, 0, 0], 0.35);
                fb.fill_rect(bx - 1, by - bz - 2, 3, 3, [255, 255, 255]);
                let dot = ((self.t * self.ball.spin) as i32).rem_euclid(3);
                fb.fill_rect(bx - 1 + dot, by - bz - 1, 1, 1, [26, 26, 26]);
            } else {
                let idx = id as usize;
                let m = &self.men[idx];
                let party = self.phase == FbPhase::Goal && m.team == self.scorer;
                let hop = if party && m.vx.hypot(m.vy) < 10.0 {
                    ((self.t * 10.0 + m.k as f32).sin() * 2.0).abs().round() as i32
                } else {
                    0
                };
                let mx = m.x.round() as i32;
                let my = m.y.round() as i32;
                let top = my - hop;
                let f = if m.vx.hypot(m.vy) > 6.0 {
                    (m.run as i32) & 1
                } else {
                    0
                };
                let shirt = if m.keeper {
                    if m.team == 1 {
                        [63, 207, 142]
                    } else {
                        [230, 211, 63]
                    }
                } else {
                    TEAM_KIT[m.team]
                };
                fb.fill_rect_alpha(mx - 2, my, 5, 2, [0, 0, 0], 0.3);
                if m.dive > 0.0 {
                    fb.fill_rect(mx - 4, my - 4, 8, 3, shirt);
                    fb.fill_rect(mx + if m.fy > 0.0 { 4 } else { -7 }, my - 5, 3, 3, m.skin);
                } else {
                    fb.fill_rect(mx - 2 + f, top - 2, 1, 2, m.skin);
                    fb.fill_rect(mx + 1 - f, top - 2, 1, 2, m.skin);
                    fb.fill_rect(mx - 2, top - 4, 5, 2, shade(shirt, -0.45));
                    fb.fill_rect(mx - 2, top - 7, 5, 3, shirt);
                    fb.fill_rect(mx - 2, top - 10, 4, 3, m.skin);
                    fb.fill_rect(mx - 2, top - 10, 4, 1, m.hair);
                    if self.ctl[m.team] == idx && self.phase != FbPhase::End {
                        let ac = shade(TEAM_KIT[m.team], 0.45);
                        fb.fill_rect(mx - 1, top - 13, 3, 1, ac);
                        fb.fill_rect(mx, top - 12, 1, 1, ac);
                    }
                }
            }
        }

        // Scoreboard header
        fb.fill_rect(0, 0, TV_W as i32, 9, [11, 14, 20]);
        fb.fill_rect(3, 2, 5, 5, TEAM_KIT[0]);
        fb.draw_text("CLAUDE", 10, 2, [243, 239, 230], 1, Align::Left, Some([0, 0, 0]));
        fb.fill_rect(TV_W as i32 - 8, 2, 5, 5, TEAM_KIT[1]);
        fb.draw_text("CODEX", TV_W as i32 - 11, 2, [243, 239, 230], 1, Align::Right, Some([0, 0, 0]));

        let mut sc = StackStr::new();
        sc.push_u32(self.score[0], 1);
        sc.push_byte(b'-');
        sc.push_u32(self.score[1], 1);
        fb.draw_text(sc.as_str(), cx, 2, [255, 255, 255], 1, Align::Center, Some([0, 0, 0]));

        let mut ck = StackStr::new();
        ck.push_u32(self.match_t.min(90.0) as u32, 1);
        ck.push_byte(b'\'');
        fb.fill_rect(cx - 10, 10, 20, 7, [11, 14, 20]);
        fb.draw_text(ck.as_str(), cx, 11, [247, 211, 84], 1, Align::Center, None);

        match self.phase {
            FbPhase::Intro => {
                fb.fill_rect_alpha(0, 0, TV_W as i32, TV_H as i32, [8, 10, 16], 0.35);
                fb.draw_banner("KICK OFF", 50, [247, 211, 84], 3);
            }
            FbPhase::Goal => {
                fb.draw_banner("GOAL!", 48, [247, 211, 84], 4);
            }
            FbPhase::End => {
                fb.fill_rect_alpha(0, 0, TV_W as i32, TV_H as i32, [8, 10, 16], 0.55);
                fb.draw_banner("FULL TIME", 36, [255, 255, 255], 3);
                fb.draw_banner(sc.as_str(), 66, [247, 211, 84], 4);
            }
            FbPhase::Play => {}
        }
    }
}

/* ---------- 2. RACING ("SAIGON RUSH", split-screen pseudo-3D road) ---------- */

const RC_SEG: f32 = 200.0;
const RC_ROADW: f32 = 2000.0;
const RC_CAMH: f32 = 1000.0;
const RC_DEPTH: f32 = 0.8391; // 1 / tan(50 deg)
const RC_DRAW: usize = 44;
const RC_VMAX: f32 = 9000.0;
const RC_LAPS: i32 = 3;
const RC_HALF: i32 = 70;
const RC_AHEAD: f32 = RC_CAMH * RC_DEPTH;

#[derive(Clone, Copy)]
enum RoadDeco {
    None,
    Palm(f32),
    Tree(f32),
    Sign(u8, f32),
    Gate,
}

#[derive(Clone, Copy)]
struct RoadSeg {
    curve: f32,
    y1: f32,
    y2: f32,
    deco: RoadDeco,
}

#[derive(Clone, Copy, Default)]
struct ProjSeg {
    idx: usize,
    x1: f32,
    y1: f32,
    w1: f32,
    x2: f32,
    y2: f32,
    w2: f32,
    k1: f32,
    k2: f32,
    road: bool,
}

#[derive(Clone, Copy)]
struct RaceCar {
    col: [u8; 3],
    name: &'static str,
    d: f32,
    x: f32,
    speed: f32,
    nitro: f32,
    lap: i32,
    pos: usize,
    top: f32,
    sky: f32,
    steer: f32,
    boost: bool,
    nos: bool,
    spin: f32,
    cd: f32,
    shake: f32,
}

#[derive(Clone, Copy)]
struct TrafficBike {
    d: f32,
    x: f32,
    speed: f32,
    kind: u8, // 0=grab, 1=nonla, 2=box, 3=be
}

struct RacingState {
    rng: Rng,
    segs: Vec<RoadSeg>,
    skyline: Vec<Building>,
    cars: [RaceCar; 4],
    bikes: [TrafficBike; 10],
    race_t: f32,
    t: f32,
    count_t: f32,
    flash: f32,
    shake: f32,
}

impl RacingState {
    fn new(seed: u32) -> Self {
        let mut rng = Rng::new(seed);
        let mut segs = Vec::with_capacity(420);
        let plan: [(usize, usize, usize, f32, f32); 6] = [
            (15, 30, 15, 0.0, 0.0),
            (20, 40, 20, -3.0, 18.0),
            (15, 25, 15, 4.0, -14.0),
            (15, 30, 15, -2.5, 22.0),
            (20, 40, 20, 3.2, -16.0),
            (15, 30, 15, -2.8, -10.0),
        ];
        let mut last_y = 0.0_f32;
        for (a, b, c, curve, hill) in plan {
            let y0 = last_y;
            let y1 = y0 + hill * RC_SEG;
            let n = (a + b + c) as f32;
            let mut step = 0usize;
            for k in 0..a {
                step += 1;
                let p = k as f32 / a as f32;
                let ny = y0 + (y1 - y0) * (0.5 - (step as f32 / n * PI).cos() * 0.5);
                segs.push(RoadSeg { curve: curve * p * p, y1: last_y, y2: ny, deco: RoadDeco::None });
                last_y = ny;
            }
            for _ in 0..b {
                step += 1;
                let ny = y0 + (y1 - y0) * (0.5 - (step as f32 / n * PI).cos() * 0.5);
                segs.push(RoadSeg { curve, y1: last_y, y2: ny, deco: RoadDeco::None });
                last_y = ny;
            }
            for k in 0..c {
                step += 1;
                let p = k as f32 / c as f32;
                let ny = y0 + (y1 - y0) * (0.5 - (step as f32 / n * PI).cos() * 0.5);
                segs.push(RoadSeg {
                    curve: curve * (0.5 + (p * PI).cos() * 0.5),
                    y1: last_y,
                    y2: ny,
                    deco: RoadDeco::None,
                });
                last_y = ny;
            }
        }
        // Return smoothly to y=0
        let y0 = last_y;
        let ret_n = 40;
        for k in 0..ret_n {
            let ny = y0 * (0.5 + ((k + 1) as f32 / ret_n as f32 * PI).cos() * 0.5);
            segs.push(RoadSeg { curve: 0.0, y1: last_y, y2: ny, deco: RoadDeco::None });
            last_y = ny;
        }
        let n = segs.len();
        segs[0].deco = RoadDeco::Gate;
        let mut i = 8;
        while i + 4 < n {
            let side = if rng.next_f32() < 0.5 { -1.0 } else { 1.0 };
            let ox = side * (1.3 + rng.next_f32() * 1.2);
            segs[i].deco = if rng.next_f32() < 0.65 {
                RoadDeco::Palm(ox)
            } else {
                RoadDeco::Tree(ox)
            };
            i += 3 + (rng.next_f32() * 4.0) as usize;
        }
        let mut sk = 0u8;
        let mut j = 32;
        while j + 20 < n {
            segs[j].deco = RoadDeco::Sign(sk % 4, if sk % 2 == 0 { -1.5 } else { 1.5 });
            sk = sk.wrapping_add(1);
            j += 52;
        }

        let len = n as f32 * RC_SEG;
        let cars = [
            RaceCar {
                col: TEAM_KIT[0],
                name: "CLAUDE",
                d: len - RC_SEG * 4.0,
                x: -0.45,
                speed: 2200.0,
                nitro: 1.0,
                lap: 0,
                pos: 3,
                top: 1.0,
                sky: 0.0,
                steer: 0.0,
                boost: false,
                nos: false,
                spin: 0.0,
                cd: 0.0,
                shake: 0.0,
            },
            RaceCar {
                col: TEAM_KIT[1],
                name: "CODEX",
                d: len - RC_SEG * 3.2,
                x: 0.45,
                speed: 2200.0,
                nitro: 1.0,
                lap: 0,
                pos: 4,
                top: 1.0,
                sky: 0.0,
                steer: 0.0,
                boost: false,
                nos: false,
                spin: 0.0,
                cd: 0.0,
                shake: 0.0,
            },
            RaceCar {
                col: [79, 180, 119],
                name: "CPU 1",
                d: RC_SEG * 2.0,
                x: -0.4,
                speed: 2600.0,
                nitro: 0.8,
                lap: 0,
                pos: 1,
                top: 0.92,
                sky: 0.0,
                steer: 0.0,
                boost: false,
                nos: false,
                spin: 0.0,
                cd: 0.0,
                shake: 0.0,
            },
            RaceCar {
                col: [176, 107, 214],
                name: "CPU 2",
                d: RC_SEG * 4.0,
                x: 0.4,
                speed: 2600.0,
                nitro: 0.8,
                lap: 0,
                pos: 2,
                top: 0.95,
                sky: 0.0,
                steer: 0.0,
                boost: false,
                nos: false,
                spin: 0.0,
                cd: 0.0,
                shake: 0.0,
            },
        ];

        let mut bikes = [TrafficBike { d: 0.0, x: 0.0, speed: 2400.0, kind: 0 }; 10];
        let lanes = [-0.62_f32, 0.05, 0.6];
        for (k, b) in bikes.iter_mut().enumerate() {
            b.d = ((k as f32 + 0.4 + rng.next_f32() * 0.3) * len / 10.0).rem_euclid(len);
            b.x = lanes[k % 3] + (rng.next_f32() - 0.5) * 0.14;
            b.speed = RC_VMAX * (0.26 + rng.next_f32() * 0.1);
            b.kind = (k % 4) as u8;
        }

        Self {
            rng,
            segs,
            skyline: build_skyline(11, 560, 6, 22),
            cars,
            bikes,
            race_t: 0.0,
            t: 0.0,
            count_t: 0.4,
            flash: 0.0,
            shake: 0.0,
        }
    }

    fn track_len(&self) -> f32 {
        self.segs.len() as f32 * RC_SEG
    }

    fn seg_idx(&self, d: f32) -> usize {
        let len = self.track_len();
        let w = d.rem_euclid(len);
        ((w / RC_SEG) as usize) % self.segs.len()
    }

    fn ai_for(&mut self, i: usize, level: f32) -> PadInput {
        let c = self.cars[i];
        let ahead = self.segs[self.seg_idx(c.d + RC_SEG * 6.0)];
        let pct = c.speed / RC_VMAX;
        let mut want = (-ahead.curve * 0.13).clamp(-0.5, 0.5);
        let len = self.track_len();
        let mut bd = RC_SEG * (5.0 + level * 9.0);
        let mut block_x: Option<f32> = None;
        for b in &self.bikes {
            let dz = (b.d - c.d).rem_euclid(len);
            if dz > 0.0 && dz < bd && (b.x - c.x).abs() < 0.52 && b.speed < c.speed + 400.0 {
                bd = dz;
                block_x = Some(b.x);
            }
        }
        for (j, e) in self.cars.iter().enumerate() {
            if j == i {
                continue;
            }
            let dz = (e.d - c.d).rem_euclid(len);
            if dz > 0.0 && dz < bd && (e.x - c.x).abs() < 0.52 && e.speed < c.speed + 400.0 {
                bd = dz;
                block_x = Some(e.x);
            }
        }
        if let Some(bx) = block_x {
            want = if bx > 0.15 {
                bx - 0.7
            } else if bx < -0.15 {
                bx + 0.7
            } else if c.x >= bx {
                bx + 0.7
            } else {
                bx - 0.7
            };
        }
        want += (self.t * 0.6 + (i as f32) * 2.1).sin() * (1.0 - level) * 0.35;
        let cur_curve = self.segs[self.seg_idx(c.d)].curve;
        let steer = ((want - c.x) * 3.0 + cur_curve * pct * 0.25).clamp(-1.0, 1.0);
        let mut nos = c.nos;
        if !nos && c.nitro > 0.85 && ahead.curve.abs() < 1.2 && self.rng.next_f32() < 0.015 {
            nos = true;
        }
        if c.nitro < 0.08 {
            nos = false;
        }
        self.cars[i].nos = nos;
        PadInput {
            x: steer,
            y: 0.0,
            a: true,
            b: nos,
        }
    }

    fn step_fixed(&mut self, h: f32, p1: Option<PadInput>, p2: Option<PadInput>) {
        self.t += h;
        if self.count_t > 0.0 {
            self.count_t -= h;
            return;
        }
        self.race_t += h;
        let len = self.track_len();
        for b in &mut self.bikes {
            b.d = (b.d + b.speed * h).rem_euclid(len);
        }
        let lead = self
            .cars
            .iter()
            .map(|c| c.d)
            .fold(f32::NEG_INFINITY, f32::max);

        for i in 0..4 {
            let inp = match i {
                0 => p1.unwrap_or_else(|| self.ai_for(0, 0.72)),
                1 => p2.unwrap_or_else(|| self.ai_for(1, 0.70)),
                2 => self.ai_for(2, 0.56),
                _ => self.ai_for(3, 0.66),
            };
            let seg = self.segs[self.seg_idx(self.cars[i].d)];
            let c = &mut self.cars[i];
            c.cd = (c.cd - h).max(0.0);
            c.shake = (c.shake - h * 14.0).max(0.0);
            let pct = c.speed / RC_VMAX;
            let dx = h * 2.2 * pct;
            c.steer = if c.spin > 0.0 {
                0.0
            } else {
                inp.x.clamp(-1.0, 1.0)
            };
            if c.spin > 0.0 {
                c.spin -= h;
                c.x += (c.spin * 18.0).sin() * dx * 0.6;
            }
            c.x += dx * c.steer - dx * pct * seg.curve * 0.25;
            c.sky += seg.curve * pct * h * 22.0;
            c.boost = inp.b && c.nitro > 0.02 && c.spin <= 0.0;
            c.nitro = (c.nitro + if c.boost { -0.4 } else { 0.06 } * h).clamp(0.0, 1.0);
            let mut top = RC_VMAX
                * c.top
                * if c.boost { 1.22 } else { 1.0 }
                * (1.0 + ((lead - c.d) / len).clamp(0.0, 0.25) * 0.3);
            if c.x.abs() > 1.05 {
                top = top.min(RC_VMAX * 0.38);
            }
            if inp.y > 0.5 {
                c.speed -= RC_VMAX * 1.1 * h;
            } else if c.speed < top {
                c.speed += RC_VMAX * if c.boost { 0.52 } else { 0.32 } * (1.2 - c.speed / top) * h;
            } else {
                c.speed -= RC_VMAX * 0.7 * h;
            }
            c.speed = c.speed.clamp(0.0, RC_VMAX * 1.3);
            c.x = c.x.clamp(-2.1, 2.1);

            for b in &self.bikes {
                let dz = (b.d - c.d).rem_euclid(len);
                if dz < RC_SEG * 0.55 && (b.x - c.x).abs() < 0.27 && c.speed > b.speed && c.cd <= 0.0 {
                    c.cd = 1.2;
                    c.speed *= 0.4;
                    c.spin = 0.6;
                    c.shake = 4.0;
                    c.x += if c.x > b.x { 0.12 } else { -0.12 };
                }
            }
            c.d += c.speed * h;
            c.lap = ((c.d / len).floor() as i32).rem_euclid(RC_LAPS);
        }

        // Update standings (fixed 4-element sort, zero alloc)
        let mut ord = [0usize, 1, 2, 3];
        ord.sort_by(|&a, &b| self.cars[b].d.total_cmp(&self.cars[a].d));
        for (rank, &ci) in ord.iter().enumerate() {
            self.cars[ci].pos = rank + 1;
        }
    }

    fn draw_half(&self, fb: &mut PixBuf<'_>, car_idx: usize, y0: i32) {
        let c = &self.cars[car_idx];
        let w = TV_W as i32;
        let h = RC_HALF;
        let cx = w as f32 * 0.5;
        let hor = y0 + h / 2;
        fb.set_clip_y(y0, y0 + h);

        // Saigon sunset sky gradient
        fb.fill_vgrad(y0, hor + 3, [43, 34, 87], [210, 102, 94], [245, 181, 108], 0.65);

        // Banded sunset sun
        let sx = ((w as f32 * 0.68 - c.sky * 0.06).rem_euclid(w as f32 + 60.0) - 30.0) as i32;
        for r in -11..=11_i32 {
            if r > 2 && (r & 3) == 0 {
                continue;
            }
            let hw = ((121 - r * r).max(0) as f32).sqrt().round() as i32;
            fb.fill_rect(sx - hw, hor - 10 + r, hw * 2, 1, [255, 226, 163]);
        }

        // Parallax Saigon skyline
        let off = (c.sky * 0.25).round() as i32;
        for b in &self.skyline {
            let bx = (b.x - off).rem_euclid(560) - 140;
            if bx > w || bx + b.w < 0 {
                continue;
            }
            fb.fill_rect(bx, hor + 2 - b.h, b.w, b.h, [58, 43, 87]);
            let mut wr = Rng::new(b.seed);
            for _ in 0..((b.w * b.h) / 48).max(1) {
                let wx = (wr.next_f32() * (b.w - 1).max(1) as f32) as i32;
                let wy = (wr.next_f32() * (b.h - 1).max(1) as f32) as i32;
                fb.put_px(bx + wx, hor + 2 - b.h + wy, [244, 194, 107]);
            }
        }

        // Project pseudo-3D road segments into stack buffer (ZERO heap allocations!)
        let len = self.track_len();
        let n_segs = self.segs.len();
        let cam = (c.d - RC_AHEAD).rem_euclid(len);
        let base_i = self.seg_idx(cam);
        let bp = (cam % RC_SEG) / RC_SEG;
        let ps = self.segs[self.seg_idx(c.d)];
        let cam_y = lerp(ps.y1, ps.y2, (c.d.rem_euclid(RC_SEG)) / RC_SEG) + RC_CAMH;
        let cam_x = c.x * RC_ROADW;

        let mut max_y = (y0 + h) as f32;
        let mut x = 0.0_f32;
        let mut dx = -self.segs[base_i].curve * bp;
        let mut proj = [ProjSeg::default(); RC_DRAW];
        let mut vis_len = 0usize;

        for n in 0..RC_DRAW {
            let si = (base_i + n) % n_segs;
            let s = self.segs[si];
            let z1 = si as f32 * RC_SEG + if si < base_i { len } else { 0.0 } - cam;
            let z2 = z1 + RC_SEG;
            let x1 = x;
            let x2 = x + dx;
            x += dx;
            dx += s.curve;
            if z1 <= RC_DEPTH {
                continue;
            }
            let k1 = RC_DEPTH / z1;
            let k2 = RC_DEPTH / z2;
            let sx1 = cx + k1 * (x1 - cam_x) * cx;
            let sx2 = cx + k2 * (x2 - cam_x) * cx;
            let sy1 = hor as f32 - k1 * (s.y1 - cam_y) * (h as f32 * 0.5);
            let sy2 = hor as f32 - k2 * (s.y2 - cam_y) * (h as f32 * 0.5);
            let sw1 = k1 * RC_ROADW * cx;
            let sw2 = k2 * RC_ROADW * cx;
            let road = sy2 < sy1 && sy2 < max_y;
            proj[vis_len] = ProjSeg {
                idx: si,
                x1: sx1,
                y1: sy1,
                w1: sw1,
                x2: sx2,
                y2: sy2,
                w2: sw2,
                k1,
                k2,
                road,
            };
            vis_len += 1;
            if road {
                max_y = sy2;
            }
        }

        // Draw far to near
        for n in (0..vis_len).rev() {
            let q = proj[n];
            let y1 = q.y1.round() as i32;
            let y2 = (q.y2.round() as i32).min(y1 - 1);
            if q.road {
                let lit = ((q.idx / 3) & 1) != 0;
                let grass = if lit { [94, 158, 72] } else { [84, 142, 64] };
                let rum = if lit { [236, 228, 214] } else { [201, 72, 63] };
                let road_c = if q.idx < 2 {
                    if lit { [242, 242, 242] } else { [32, 32, 32] }
                } else if lit {
                    [108, 107, 114]
                } else {
                    [100, 99, 105]
                };
                fb.fill_rect(0, y2, w, y1 - y2, grass);
                fb.fill_trapezoid(q.x1 - q.w1 * 1.14, q.x1 - q.w1, y1, q.x2 - q.w2 * 1.14, q.x2 - q.w2, y2, rum);
                fb.fill_trapezoid(q.x1 + q.w1, q.x1 + q.w1 * 1.14, y1, q.x2 + q.w2, q.x2 + q.w2 * 1.14, y2, rum);
                fb.fill_trapezoid(q.x1 - q.w1, q.x1 + q.w1, y1, q.x2 - q.w2, q.x2 + q.w2, y2, road_c);
                if lit && n < 28 {
                    for l in [-0.33_f32, 0.33] {
                        fb.fill_trapezoid(
                            q.x1 + q.w1 * (l - 0.025),
                            q.x1 + q.w1 * (l + 0.025),
                            y1,
                            q.x2 + q.w2 * (l - 0.025),
                            q.x2 + q.w2 * (l + 0.025),
                            y2,
                            [237, 231, 218],
                        );
                    }
                }
            }

            // Roadside props
            let k = q.k1 * cx;
            match self.segs[q.idx].deco {
                RoadDeco::Gate => {
                    let hgt = 1500.0 * k;
                    if hgt >= 3.0 {
                        let gl = q.x1 - q.w1 * 1.1;
                        let gw = q.w1 * 2.2;
                        fb.fill_rect_f(gl, q.y1 - hgt, (90.0 * k).max(1.0), hgt, [44, 47, 54]);
                        fb.fill_rect_f(gl + gw - 90.0 * k, q.y1 - hgt, (90.0 * k).max(1.0), hgt, [44, 47, 54]);
                        let beam = (240.0 * k).max(2.0);
                        for i in 0..12 {
                            let col = if i % 2 == 0 { [244, 244, 244] } else { [28, 28, 28] };
                            fb.fill_rect_f(gl + i as f32 * gw / 12.0, q.y1 - hgt - beam, gw / 12.0 + 1.0, beam, col);
                        }
                    }
                }
                RoadDeco::Palm(ox) => {
                    let px = q.x1 + q.k1 * ox * RC_ROADW * cx;
                    let hh = 2300.0 * k;
                    if hh >= 3.0 {
                        let tw = (110.0 * k).max(1.0);
                        fb.fill_rect_f(px - tw * 0.5, q.y1 - hh, tw, hh, [124, 91, 59]);
                        let r = (680.0 * k).max(2.0);
                        fb.fill_rect_f(px - r, q.y1 - hh - r * 0.25, r * 2.0, r * 0.45, [46, 138, 74]);
                        fb.fill_rect_f(px - r * 0.65, q.y1 - hh - r * 0.55, r * 1.3, r * 0.35, [63, 160, 90]);
                    }
                }
                RoadDeco::Tree(ox) => {
                    let px = q.x1 + q.k1 * ox * RC_ROADW * cx;
                    let hh = 1650.0 * k;
                    if hh >= 3.0 {
                        fb.fill_rect_f(px - 60.0 * k, q.y1 - hh * 0.45, (120.0 * k).max(1.0), hh * 0.45, [107, 74, 42]);
                        fb.fill_circle(
                            px.round() as i32,
                            (q.y1 - hh * 0.7).round() as i32,
                            (hh * 0.3).round().max(1.0) as i32,
                            [62, 140, 62],
                        );
                    }
                }
                RoadDeco::Sign(kind, ox) => {
                    let px = q.x1 + q.k1 * ox * RC_ROADW * cx;
                    let sw = 1900.0 * k;
                    let sh = 850.0 * k;
                    if sw >= 4.0 {
                        let lift = 750.0 * k;
                        let bg = match kind {
                            0 => [200, 50, 43],
                            1 => [28, 38, 32],
                            2 => [242, 193, 78],
                            _ => [217, 119, 87],
                        };
                        fb.fill_rect_f(px - sw * 0.4, q.y1 - lift, (70.0 * k).max(1.0), lift, [52, 54, 61]);
                        fb.fill_rect_f(px + sw * 0.35, q.y1 - lift, (70.0 * k).max(1.0), lift, [52, 54, 61]);
                        fb.fill_rect_f(px - sw * 0.5, q.y1 - lift - sh, sw, sh, bg);
                    }
                }
                RoadDeco::None => {}
            }

            // Traffic bikes & rival cars on this segment
            for b in &self.bikes {
                if self.seg_idx(b.d) == q.idx && (b.d - cam).rem_euclid(len) >= RC_AHEAD - RC_SEG * 0.25 {
                    let f = (b.d.rem_euclid(RC_SEG)) / RC_SEG;
                    let sk = lerp(q.k1, q.k2, f);
                    let bx = lerp(q.x1, q.x2, f) + sk * b.x * RC_ROADW * cx;
                    let by = lerp(q.y1, q.y2, f);
                    draw_bike(fb, bx, by, sk * cx, b.kind);
                }
            }
            for (oi, e) in self.cars.iter().enumerate() {
                if oi == car_idx {
                    continue;
                }
                if self.seg_idx(e.d) == q.idx && (e.d - cam).rem_euclid(len) >= RC_AHEAD - RC_SEG * 0.25 {
                    let f = (e.d.rem_euclid(RC_SEG)) / RC_SEG;
                    let sk = lerp(q.k1, q.k2, f);
                    let ex = lerp(q.x1, q.x2, f) + sk * e.x * RC_ROADW * cx;
                    let ey = lerp(q.y1, q.y2, f);
                    draw_car_back(fb, ex, ey, sk * cx, e.col, e.boost, self.t, e.steer);
                }
            }
        }

        // Player car
        draw_car_back(fb, cx + c.steer * 2.5, (y0 + h - 2) as f32, 34.0 / 620.0, c.col, c.boost, self.t, c.steer);

        // HUD
        fb.fill_rect(3, y0 + 2, 5, 5, c.col);
        fb.draw_text(c.name, 10, y0 + 2, [255, 255, 255], 1, Align::Left, Some([0, 0, 0]));
        let ord = ["1ST", "2ND", "3RD", "4TH"][(c.pos.saturating_sub(1)).min(3)];
        fb.draw_text(
            ord,
            3,
            y0 + 10,
            if c.pos == 1 { [247, 211, 84] } else { [255, 255, 255] },
            1,
            Align::Left,
            Some([0, 0, 0]),
        );
        let mut lap_s = StackStr::new();
        lap_s.push_str("LAP ");
        lap_s.push_u32((c.lap + 1) as u32, 1);
        lap_s.push_byte(b'/');
        lap_s.push_u32(RC_LAPS as u32, 1);
        fb.draw_text(lap_s.as_str(), w - 3, y0 + 2, [255, 255, 255], 1, Align::Right, Some([0, 0, 0]));

        let mut kmh = StackStr::new();
        kmh.push_u32(((c.speed / RC_VMAX) * 240.0).round() as u32, 1);
        kmh.push_str(" KM/H");
        fb.draw_text(kmh.as_str(), w - 3, y0 + h - 8, [255, 255, 255], 1, Align::Right, Some([0, 0, 0]));

        fb.fill_rect_alpha(3, y0 + h - 8, 36, 5, [0, 0, 0], 0.55);
        fb.fill_rect(
            4,
            y0 + h - 7,
            (34.0 * c.nitro).round() as i32,
            3,
            if c.boost { [255, 210, 74] } else { [63, 200, 255] },
        );
        fb.draw_text("NOS", 42, y0 + h - 8, [63, 200, 255], 1, Align::Left, Some([0, 0, 0]));

        fb.reset_clip();
    }

    fn draw(&self, fb: &mut PixBuf<'_>) {
        self.draw_half(fb, 0, 0);
        self.draw_half(fb, 1, TV_H as i32 - RC_HALF);
        fb.fill_rect(0, RC_HALF, TV_W as i32, TV_H as i32 - 2 * RC_HALF, [10, 12, 16]);
        if self.count_t > 0.0 {
            fb.draw_banner("GO!", TV_H as i32 / 2 - 10, [124, 252, 124], 4);
        }
    }
}

#[allow(clippy::too_many_arguments)]
fn draw_car_back(fb: &mut PixBuf<'_>, x: f32, y: f32, k: f32, col: [u8; 3], boost: bool, t: f32, steer: f32) {
    let w = 620.0 * k;
    let h = 300.0 * k;
    if w < 3.0 {
        return;
    }
    let bx = x - w * 0.5;
    let by = y - h;
    let lean = steer * w * 0.04;
    fb.fill_rect_alpha(
        (bx - w * 0.04).round() as i32,
        (y - h * 0.12).round() as i32,
        (w * 1.08).round().max(1.0) as i32,
        (h * 0.16).round().max(1.0) as i32,
        [0, 0, 0],
        0.35,
    );
    fb.fill_rect_f(bx + w * 0.02, by + h * 0.62, w * 0.2, h * 0.38, [21, 23, 28]);
    fb.fill_rect_f(bx + w * 0.78, by + h * 0.62, w * 0.2, h * 0.38, [21, 23, 28]);
    fb.fill_rect_f(bx, by + h * 0.5, w, h * 0.32, shade(col, -0.28));
    fb.fill_rect_f(bx + w * 0.02, by + h * 0.3, w * 0.96, h * 0.26, col);
    fb.fill_rect_f(bx + w * 0.17 + lean, by, w * 0.66, h * 0.32, shade(col, -0.38));
    fb.fill_rect_f(bx + w * 0.22 + lean, by + h * 0.05, w * 0.56, h * 0.2, [38, 48, 64]);
    fb.fill_rect_f(bx + w * 0.04, by + h * 0.36, w * 0.18, h * 0.11, [255, 75, 62]);
    fb.fill_rect_f(bx + w * 0.78, by + h * 0.36, w * 0.18, h * 0.11, [255, 75, 62]);
    fb.fill_rect_f(bx + w * 0.38, by + h * 0.58, w * 0.24, h * 0.1, [233, 226, 210]);
    if boost {
        let f = if ((t * 30.0) as i32 & 1) != 0 { 1.0 } else { 0.0 };
        for px in [0.24_f32, 0.68] {
            fb.fill_rect_f(bx + w * px, y - h * 0.18, w * 0.08, h * (0.2 + f * 0.12), [255, 122, 46]);
            fb.fill_rect_f(bx + w * (px + 0.015), y - h * 0.16, w * 0.05, h * (0.1 + f * 0.08), [255, 230, 109]);
        }
    }
}

fn draw_bike(fb: &mut PixBuf<'_>, x: f32, y: f32, k: f32, kind: u8) {
    let w = 230.0 * k;
    let h = 520.0 * k;
    if h < 3.0 {
        return;
    }
    let bx = x - w * 0.5;
    let by = y - h;
    let jacket = match kind {
        0 => [0, 168, 89],    // Grab
        1 => [140, 106, 78],  // Non la
        2 => [43, 108, 176],  // Box
        _ => [246, 197, 27],  // Be
    };
    fb.fill_rect_f(x - w * 0.12, y - h * 0.34, w * 0.24, h * 0.34, [22, 23, 27]);
    fb.fill_rect_f(x - w * 0.12, y - h * 0.4, w * 0.24, h * 0.06, [227, 75, 63]);
    fb.fill_rect_f(bx + w * 0.12, by + h * 0.26, w * 0.76, h * 0.32, jacket);
    if kind == 2 {
        fb.fill_rect_f(bx + w * 0.04, by + h * 0.3, w * 0.92, h * 0.26, [232, 116, 42]);
    }
    if kind == 1 {
        fb.fill_rect_f(bx - w * 0.15, by + h * 0.08, w * 1.3, h * 0.12, [233, 220, 168]);
    } else {
        let helm = match kind {
            0 => [0, 168, 89],
            3 => [246, 197, 27],
            _ => [242, 242, 242],
        };
        fb.fill_rect_f(bx + w * 0.24, by, w * 0.52, h * 0.24, helm);
    }
}

/* ---------- 3. FIGHTING ("ROOFTOP FIGHT", twilight Saigon skyline) ---------- */

const FT_AW: f32 = 346.0;
const FT_FLOOR: i32 = 122;
const FT_ROUND_T: f32 = 40.0;

#[derive(Clone, Copy, PartialEq, Eq)]
enum FtAct {
    Idle,
    Walk,
    Jump,
    Block,
    Punch,
    Kick,
    Special,
    Hurt,
    Ko,
    Win,
}

#[derive(Clone, Copy)]
struct Fighter {
    i: usize,
    x: f32,
    y: f32,
    vx: f32,
    vy: f32,
    face: f32,
    hp: f32,
    ghost: f32,
    meter: f32,
    st: FtAct,
    st_t: f32,
    hit: bool,
    stun: f32,
    wins: u32,
}

#[derive(Clone, Copy, Default)]
struct Fireball {
    active: bool,
    owner: usize,
    x: f32,
    y: f32,
    vx: f32,
}

#[derive(Clone, Copy, Default)]
struct Spark {
    active: bool,
    x: f32,
    y: f32,
    t: f32,
    blocked: bool,
    dust: bool,
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum FtPhase {
    Intro,
    Fight,
    Ko,
    End,
}

struct FightingState {
    rng: Rng,
    f: [Fighter; 2],
    shots: [Fireball; 8],
    sparks: [Spark; 24],
    far_towers: Vec<Building>,
    near_towers: Vec<Building>,
    think: [f32; 2],
    plan_move: [f32; 2],
    plan_block: [bool; 2],
    prev_a: [bool; 2],
    prev_b: [bool; 2],
    phase: FtPhase,
    phase_t: f32,
    round: u32,
    round_t: f32,
    freeze: f32,
    t: f32,
    shake: f32,
    flash: f32,
}

impl FightingState {
    fn new(seed: u32) -> Self {
        let make = |i: usize| Fighter {
            i,
            x: FT_AW * 0.5 + if i == 1 { 36.0 } else { -36.0 },
            y: 0.0,
            vx: 0.0,
            vy: 0.0,
            face: if i == 1 { -1.0 } else { 1.0 },
            hp: 100.0,
            ghost: 100.0,
            meter: 0.35,
            st: FtAct::Idle,
            st_t: 0.0,
            hit: false,
            stun: 0.0,
            wins: 0,
        };
        Self {
            rng: Rng::new(seed),
            f: [make(0), make(1)],
            shots: [Fireball::default(); 8],
            sparks: [Spark::default(); 24],
            far_towers: build_skyline(5, 460, 34, 72),
            near_towers: build_skyline(9, 460, 22, 50),
            think: [0.0, 0.0],
            plan_move: [1.0, -1.0],
            plan_block: [false, false],
            prev_a: [false, false],
            prev_b: [false, false],
            phase: FtPhase::Intro,
            phase_t: 0.6,
            round: 1,
            round_t: FT_ROUND_T,
            freeze: 0.0,
            t: 0.0,
            shake: 0.0,
            flash: 0.0,
        }
    }

    fn busy(st: FtAct) -> bool {
        matches!(
            st,
            FtAct::Punch | FtAct::Kick | FtAct::Special | FtAct::Hurt | FtAct::Ko | FtAct::Win
        )
    }

    fn push_spark(&mut self, x: f32, y: f32, blocked: bool, dust: bool) {
        for s in &mut self.sparks {
            if !s.active {
                *s = Spark {
                    active: true,
                    x,
                    y,
                    t: 0.0,
                    blocked,
                    dust,
                };
                return;
            }
        }
    }

    fn push_shot(&mut self, owner: usize, x: f32, y: f32, vx: f32) {
        for s in &mut self.shots {
            if !s.active {
                *s = Fireball {
                    active: true,
                    owner,
                    x,
                    y,
                    vx,
                };
                return;
            }
        }
    }

    fn hurt(&mut self, victim: usize, attacker: usize, dmg: f32, stun: f32, push: f32, dir: f32) {
        let v = &mut self.f[victim];
        if v.st == FtAct::Block && v.y <= 0.0 && dir * v.face < 0.0 {
            v.hp = (v.hp - 1.0).max(1.0);
            v.vx = dir * push * 0.55;
            let sx = v.x - v.face * 3.0;
            let sy = 20.0 + self.rng.next_f32() * 10.0;
            self.push_spark(sx, sy, true, false);
            return;
        }
        v.hp = (v.hp - dmg).max(0.0);
        v.st = FtAct::Hurt;
        v.st_t = 0.0;
        v.stun = stun;
        v.vx = dir * push;
        if v.y > 0.0 {
            v.vy = 95.0;
        }
        let sx = v.x - v.face * 3.0;
        let dead = v.hp <= 0.0;
        self.f[attacker].meter = (self.f[attacker].meter + 0.14).min(1.0);
        self.f[victim].meter = (self.f[victim].meter + 0.09).min(1.0);
        let sy = 20.0 + self.rng.next_f32() * 10.0;
        self.push_spark(sx, sy, false, false);
        self.shake = self.shake.max(if dmg >= 6.0 { 4.0 } else { 2.0 });
        if dead {
            self.f[victim].st = FtAct::Ko;
            self.f[victim].st_t = 0.0;
            self.f[victim].vy = 135.0;
            self.f[victim].y = self.f[victim].y.max(0.1);
            self.f[victim].vx = self.f[attacker].face * 100.0;
            self.f[attacker].wins += 1;
            if self.f[attacker].y <= 0.0 {
                self.f[attacker].st = FtAct::Win;
                self.f[attacker].st_t = 0.0;
                self.f[attacker].vx = 0.0;
            }
            self.phase = FtPhase::Ko;
            self.phase_t = 2.4;
            self.freeze = 0.7;
            self.flash = 0.85;
            self.shake = 6.0;
            for s in &mut self.shots {
                s.active = false;
            }
        }
    }

    fn ai_for(&mut self, i: usize) -> PadInput {
        let mut c = PadInput::default();
        if self.phase != FtPhase::Fight {
            return c;
        }
        let f = self.f[i];
        let e = self.f[1 - i];
        let d = (e.x - f.x).abs();
        let to = if e.x >= f.x { 1.0 } else { -1.0 };
        if self.t > self.think[i] {
            self.think[i] = self.t + 0.14 + self.rng.next_f32() * 0.14;
            let inc = self
                .shots
                .iter()
                .any(|s| s.active && s.owner != i && (f.x - s.x) * s.vx.signum() > 0.0 && (f.x - s.x).abs() < 60.0);
            let threat = matches!(e.st, FtAct::Punch | FtAct::Kick) && e.st_t < 0.1 && d < 36.0;
            self.plan_block[i] = (threat && self.rng.next_f32() < 0.65) || (inc && self.rng.next_f32() < 0.35);
            if inc && !self.plan_block[i] && self.rng.next_f32() < 0.65 {
                c.y = -1.0;
            }
            if d < 28.0 && self.rng.next_f32() < 0.7 {
                if self.rng.next_f32() < 0.55 {
                    c.a = true;
                } else {
                    c.b = true;
                }
            } else if d > 65.0 && f.meter >= 1.0 && self.rng.next_f32() < 0.7 {
                c.b = true;
                self.plan_move[i] = 0.0;
            } else {
                self.plan_move[i] = if d > 24.0 {
                    to
                } else if d < 14.0 {
                    -to
                } else {
                    0.0
                };
            }
        }
        if self.plan_block[i] {
            c.y = 1.0;
        } else {
            c.x = self.plan_move[i];
        }
        c
    }

    fn step_fixed(&mut self, mut h: f32, p1: Option<PadInput>, p2: Option<PadInput>) {
        self.t += h;
        if self.freeze > 0.0 {
            self.freeze -= h;
            h *= 0.12;
        }
        match self.phase {
            FtPhase::Fight => {
                self.round_t = (self.round_t - h).max(0.0);
                let c0 = p1.unwrap_or_else(|| self.ai_for(0));
                let c1 = p2.unwrap_or_else(|| self.ai_for(1));
                let cs = [c0, c1];
                for i in 0..2 {
                    let pa = cs[i].a && !self.prev_a[i];
                    let pb = cs[i].b && !self.prev_b[i];
                    self.prev_a[i] = cs[i].a;
                    self.prev_b[i] = cs[i].b;
                    let ox = self.f[1 - i].x;
                    let f = &mut self.f[i];
                    if Self::busy(f.st) {
                        continue;
                    }
                    if f.y <= 0.0 {
                        f.face = if ox > f.x { 1.0 } else { -1.0 };
                    }
                    let air = f.y > 0.0;
                    let mx = cs[i].x.clamp(-1.0, 1.0);
                    if pb {
                        f.st = if !air && f.meter >= 1.0 && mx.abs() < 0.3 {
                            FtAct::Special
                        } else {
                            FtAct::Kick
                        };
                        f.st_t = 0.0;
                        f.hit = false;
                        if !air {
                            f.vx = 0.0;
                        }
                    } else if pa && !air {
                        f.st = FtAct::Punch;
                        f.st_t = 0.0;
                        f.hit = false;
                        f.vx = 0.0;
                    } else if !air {
                        if cs[i].y < -0.5 {
                            f.st = FtAct::Jump;
                            f.vy = 210.0;
                            f.y = 0.1;
                            f.vx = mx * 68.0;
                        } else if cs[i].y > 0.5 {
                            f.st = FtAct::Block;
                            f.vx = 0.0;
                        } else {
                            f.vx = if mx.abs() > 0.2 {
                                mx * if mx * f.face > 0.0 { 66.0 } else { 48.0 }
                            } else {
                                0.0
                            };
                            f.st = if f.vx != 0.0 { FtAct::Walk } else { FtAct::Idle };
                        }
                    }
                }
            }
            FtPhase::Intro => {
                self.phase_t -= h;
                if self.phase_t <= 0.0 {
                    self.phase = FtPhase::Fight;
                }
            }
            FtPhase::Ko | FtPhase::End => {
                self.phase_t -= h;
                if self.phase_t <= 0.0 {
                    if self.phase == FtPhase::Ko && self.f[0].wins < 2 && self.f[1].wins < 2 && self.round < 3 {
                        self.round += 1;
                        self.phase = FtPhase::Intro;
                        self.phase_t = 1.0;
                        self.round_t = FT_ROUND_T;
                    } else {
                        self.round = 1;
                        self.f[0].wins = 0;
                        self.f[1].wins = 0;
                        self.phase = FtPhase::Intro;
                        self.phase_t = 0.8;
                        self.round_t = FT_ROUND_T;
                    }
                    for i in 0..2 {
                        self.f[i].x = FT_AW * 0.5 + if i == 1 { 36.0 } else { -36.0 };
                        self.f[i].y = 0.0;
                        self.f[i].vx = 0.0;
                        self.f[i].vy = 0.0;
                        self.f[i].hp = 100.0;
                        self.f[i].ghost = 100.0;
                        self.f[i].st = FtAct::Idle;
                        self.f[i].st_t = 0.0;
                    }
                }
            }
        }

        // Advance fighter bodies & moves
        for i in 0..2 {
            self.f[i].st_t += h;
            self.f[i].stun = (self.f[i].stun - h).max(0.0);
            self.f[i].ghost = (self.f[i].ghost - h * 24.0).max(self.f[i].hp);
            if self.phase == FtPhase::Fight {
                self.f[i].meter = (self.f[i].meter + h * 0.06).min(1.0);
            }
            let st = self.f[i].st;
            let timing = match st {
                FtAct::Punch => Some((0.06_f32, 0.07_f32, 0.14_f32, 22.0_f32, 5.0_f32, 0.22_f32, 48.0_f32)),
                FtAct::Kick => Some((0.10, 0.08, 0.22, 28.0, 7.0, 0.30, 80.0)),
                FtAct::Special => Some((0.20, 0.05, 0.30, 0.0, 0.0, 0.0, 0.0)),
                _ => None,
            };
            if let Some((a, b, c, reach, dmg, stun, push)) = timing {
                if !self.f[i].hit && self.f[i].st_t >= a {
                    self.f[i].hit = true;
                    if st == FtAct::Special {
                        self.f[i].meter = 0.0;
                        let sx = self.f[i].x + self.f[i].face * 15.0;
                        let sv = self.f[i].face * 145.0;
                        self.push_shot(i, sx, 20.0, sv);
                    } else {
                        let o = self.f[1 - i];
                        let d = (o.x - self.f[i].x) * self.f[i].face;
                        if d >= 0.0 && d <= reach + 6.0 && (o.y - self.f[i].y).abs() <= 24.0 && o.st != FtAct::Ko {
                            let dir = self.f[i].face;
                            self.hurt(1 - i, i, dmg, stun, push, dir);
                        }
                    }
                }
                if self.f[i].st_t >= a + b + c {
                    self.f[i].st = if self.f[i].y > 0.0 { FtAct::Jump } else { FtAct::Idle };
                }
            }
            if self.f[i].st == FtAct::Hurt && self.f[i].stun <= 0.0 {
                self.f[i].st = if self.f[i].y > 0.0 { FtAct::Jump } else { FtAct::Idle };
            }
            if self.f[i].y > 0.0 || self.f[i].vy > 0.0 {
                self.f[i].vy -= 640.0 * h;
                self.f[i].y += self.f[i].vy * h;
                if self.f[i].y <= 0.0 {
                    self.f[i].y = 0.0;
                    self.f[i].vy = 0.0;
                    let fx = self.f[i].x;
                    self.push_spark(fx, 2.0, false, true);
                    if matches!(self.f[i].st, FtAct::Jump | FtAct::Kick) {
                        self.f[i].st = FtAct::Idle;
                    }
                    if self.f[i].st != FtAct::Ko {
                        self.f[i].vx = 0.0;
                    }
                }
            }
            self.f[i].x = (self.f[i].x + self.f[i].vx * h).clamp(14.0, FT_AW - 14.0);
            if matches!(self.f[i].st, FtAct::Hurt | FtAct::Ko | FtAct::Block) {
                self.f[i].vx *= (1.0 - h * 6.0).max(0.0);
            }
        }

        // Prevent overlap
        let gap = (self.f[0].x - self.f[1].x).abs();
        if gap < 16.0 && (self.f[0].y - self.f[1].y).abs() < 28.0 && self.f[0].st != FtAct::Ko && self.f[1].st != FtAct::Ko {
            let s = if self.f[0].x <= self.f[1].x { -1.0 } else { 1.0 };
            let p = (16.0 - gap) * 0.5;
            self.f[0].x = (self.f[0].x + s * p).clamp(14.0, FT_AW - 14.0);
            self.f[1].x = (self.f[1].x - s * p).clamp(14.0, FT_AW - 14.0);
        }

        // Fireballs
        for s_idx in 0..self.shots.len() {
            if !self.shots[s_idx].active {
                continue;
            }
            self.shots[s_idx].x += self.shots[s_idx].vx * h;
            let sx = self.shots[s_idx].x;
            let owner = self.shots[s_idx].owner;
            let enemy = 1 - owner;
            if (self.f[enemy].x - sx).abs() < 10.0 && self.f[enemy].y < 30.0 && self.f[enemy].st != FtAct::Ko {
                let dir = self.shots[s_idx].vx.signum();
                self.shots[s_idx].active = false;
                self.hurt(enemy, owner, 10.0, 0.35, 95.0, dir);
            } else if sx < -20.0 || sx > FT_AW + 20.0 {
                self.shots[s_idx].active = false;
            }
        }
        // Sparks
        for sp in &mut self.sparks {
            if sp.active {
                sp.t += h;
                if sp.t >= 0.35 {
                    sp.active = false;
                }
            }
        }
        if self.phase == FtPhase::Fight && self.round_t <= 0.0 {
            self.phase = FtPhase::End;
            self.phase_t = 2.0;
        }
    }

    fn draw_fighter(&self, fb: &mut PixBuf<'_>, f: &Fighter, cam: f32) {
        let c = TEAM_KIT[f.i];
        let dk = shade(c, -0.45);
        let lt = shade(c, 0.35);
        let skin = if f.i == 1 { [226, 174, 128] } else { [242, 203, 162] };
        let hair = if f.i == 1 { [28, 28, 36] } else { [58, 36, 22] };
        let px = (f.x - cam).round() as i32;
        let py = FT_FLOOR - f.y.round() as i32;
        let dir = if f.face < 0.0 { -1 } else { 1 };
        let mut rbox = |lx: i32, ly: i32, w: i32, h: i32, col: [u8; 3]| {
            let rx = if dir > 0 { px + lx } else { px - lx - w };
            fb.fill_rect(rx, py + ly, w, h, col);
        };

        if f.st == FtAct::Ko && f.y <= 0.0 {
            rbox(-15, -4, 10, 4, dk);
            rbox(-5, -6, 11, 6, c);
            rbox(6, -8, 8, 7, skin);
            return;
        }
        let crouch = if f.st == FtAct::Block { 4 } else { 0 };
        let bob = if matches!(f.st, FtAct::Idle | FtAct::Win) {
            ((self.t * 7.0 + f.i as f32 * 2.0).sin()).round() as i32
        } else {
            0
        };
        let step = if f.st == FtAct::Walk {
            ((self.t * 14.0).sin() * 2.0).round() as i32
        } else {
            0
        };
        let lean = if f.st == FtAct::Hurt { -2 } else if matches!(f.st, FtAct::Punch | FtAct::Kick) { 2 } else { 0 };

        // Legs
        if f.st == FtAct::Kick {
            rbox(-5, -11, 4, 11, dk);
            rbox(0, -12, 16, 4, dk);
            rbox(16, -12, 3, 4, skin);
        } else {
            rbox(-5 + step, -11 + crouch, 4, 11 - crouch, dk);
            rbox(1 - step, -11 + crouch, 4, 11 - crouch, dk);
            rbox(-6 + step, -2, 6, 2, [34, 34, 40]);
            rbox(1 - step, -2, 6, 2, [34, 34, 40]);
        }
        // Body + belt
        let by = -21 + crouch + bob;
        rbox(-5 + lean, by, 11, 11, c);
        rbox(-4 + lean, by + 1, 3, 5, lt);
        rbox(-5 + lean, by + 8, 11, 2, [26, 26, 32]);
        rbox(-1 + lean, by + 8, 2, 2, [247, 211, 84]);
        // Arms
        let sx = 4 + lean;
        match f.st {
            FtAct::Punch => {
                rbox(sx, by + 2, 14, 3, skin);
                rbox(sx + 14, by + 1, 4, 5, lt);
            }
            FtAct::Special => {
                rbox(sx, by + 2, 8, 3, skin);
                rbox(sx, by + 5, 8, 3, skin);
                rbox(sx + 8, by + 2, 4, 5, lt);
            }
            FtAct::Block => {
                rbox(sx - 1, by - 2, 4, 8, skin);
            }
            FtAct::Win => {
                rbox(sx - 1, by - 9, 3, 10, skin);
                rbox(sx - 2, by - 12, 5, 4, lt);
            }
            _ => {
                rbox(sx, by + 2, 4, 3, skin);
                rbox(sx + 3, by, 4, 4, lt);
            }
        }
        // Head + hair + team headband with flapping tail
        let hx = 1 + lean;
        let hy = by - 7;
        rbox(hx - 6, hy - 6, 12, 12, skin);
        rbox(hx - 6, hy - 7, 12, 4, hair);
        let fl = ((self.t * 12.0 + f.i as f32).sin() * 1.5).round() as i32;
        rbox(hx - 6, hy - 3, 13, 2, c);
        rbox(hx - 11, hy - 3 + fl, 5, 2, c);
        // Eyes
        rbox(hx + 1, hy - 1, 2, 3, [255, 255, 255]);
        rbox(hx + 4, hy - 1, 2, 3, [255, 255, 255]);
        rbox(hx + 2, hy, 1, 2, [17, 17, 17]);
        rbox(hx + 5, hy, 1, 2, [17, 17, 17]);
    }

    fn draw(&self, fb: &mut PixBuf<'_>) {
        let cam = ((self.f[0].x + self.f[1].x) * 0.5 - TV_W as f32 * 0.5).clamp(0.0, FT_AW - TV_W as f32);
        // Twilight Saigon sky
        fb.fill_vgrad(0, FT_FLOOR, [35, 25, 66], [184, 80, 95], [244, 162, 89], 0.55);
        // Banded sun
        let sx = (168.0 - cam * 0.08).round() as i32;
        for r in -20..=20_i32 {
            if r > 3 && r % 5 == 0 {
                continue;
            }
            let hw = ((400 - r * r).max(0) as f32).sqrt().round() as i32;
            fb.fill_rect(sx - hw, 78 + r, hw * 2, 1, [255, 208, 138]);
        }
        // Two parallax tower layers
        for (layer, (towers, par, col, wcol)) in [
            (&self.far_towers, 0.3_f32, [74, 53, 96], [199, 154, 122]),
            (&self.near_towers, 0.55_f32, [42, 30, 63], [244, 194, 107]),
        ]
        .into_iter()
        .enumerate()
        {
            let _ = layer;
            let bottom = FT_FLOOR - 6;
            for b in towers {
                let bx = (b.x as f32 - cam * par).round() as i32;
                if bx > TV_W as i32 || bx + b.w < 0 {
                    continue;
                }
                fb.fill_rect(bx, bottom - b.h, b.w, b.h, col);
                let mut wr = Rng::new(b.seed);
                for _ in 0..((b.w * b.h) / 55).max(1) {
                    let wx = 1 + (wr.next_f32() * (b.w - 2).max(1) as f32) as i32;
                    let wy = 2 + (wr.next_f32() * (b.h - 4).max(1) as f32) as i32;
                    fb.fill_rect(bx + wx, bottom - b.h + wy, 1, 2, wcol);
                }
            }
        }
        // Buzzing neon PHO sign on near tower
        let nx = (220.0 - cam * 0.55).round() as i32;
        if ((self.t * 7.0) as i32 % 23) != 0 {
            fb.fill_rect(nx - 14, 62, 28, 11, [42, 15, 20]);
            fb.draw_text("PHO", nx, 65, [255, 90, 78], 1, Align::Center, None);
        }
        // Rooftop parapet, railing, water tank, aerial, floor tiles
        fb.fill_rect(0, FT_FLOOR - 6, TV_W as i32, 6, [59, 51, 68]);
        fb.fill_rect(0, FT_FLOOR - 13, TV_W as i32, 2, [46, 40, 54]);
        let mut rx = (-cam as i32).rem_euclid(10);
        while rx < TV_W as i32 {
            fb.fill_rect(rx, FT_FLOOR - 13, 2, 7, [46, 40, 54]);
            rx += 10;
        }
        let tx = (48.0 - cam).round() as i32;
        fb.fill_rect(tx - 12, FT_FLOOR - 42, 24, 20, [154, 163, 173]);
        fb.fill_rect(tx - 12, FT_FLOOR - 35, 24, 2, [126, 135, 144]);
        let ax = (310.0 - cam).round() as i32;
        fb.fill_rect(ax, FT_FLOOR - 64, 2, 58, [58, 52, 64]);
        if ((self.t * 1.5) as i32 & 1) != 0 {
            fb.fill_rect(ax - 1, FT_FLOOR - 67, 3, 3, [255, 59, 48]);
        }
        fb.fill_rect(0, FT_FLOOR, TV_W as i32, TV_H as i32 - FT_FLOOR, [104, 98, 112]);
        fb.fill_rect(0, TV_H as i32 - 5, TV_W as i32, 5, [78, 72, 86]);

        // Ground shadows & fighters
        for f in &self.f {
            let sr = (9.0 - f.y / 14.0).max(4.0) as i32;
            fb.fill_rect_alpha((f.x - cam) as i32 - sr, FT_FLOOR - 1, sr * 2, 2, [0, 0, 0], 0.35);
        }
        self.draw_fighter(fb, &self.f[0], cam);
        self.draw_fighter(fb, &self.f[1], cam);

        // Fireballs & hit sparks
        for s in &self.shots {
            if !s.active {
                continue;
            }
            let x = (s.x - cam).round() as i32;
            let y = FT_FLOOR - s.y.round() as i32;
            let c = shade(TEAM_KIT[s.owner], 0.35);
            for k in (1..=3).rev() {
                fb.fill_rect_alpha(x - s.vx.signum() as i32 * k * 4 - 2, y - 2, 4, 4, c, 0.5 - k as f32 * 0.12);
            }
            fb.fill_circle(x, y, 4 + ((self.t * 20.0) as i32 & 1), c);
            fb.fill_circle(x, y, 2, [255, 255, 255]);
        }
        for sp in &self.sparks {
            if !sp.active {
                continue;
            }
            let x = (sp.x - cam).round() as i32;
            let y = FT_FLOOR - sp.y.round() as i32;
            let r = (2.0 + sp.t * 20.0) as i32;
            let a = (1.0 - sp.t / 0.35).clamp(0.0, 1.0);
            let c = if sp.dust {
                [210, 200, 190]
            } else if sp.blocked {
                [150, 220, 255]
            } else {
                [255, 236, 120]
            };
            for (dx, dy) in [(1, 0), (-1, 0), (0, 1), (0, -1)] {
                fb.fill_rect_alpha(x + dx * r, y + dy * r, 2, 2, c, a);
            }
            if !sp.dust && sp.t < 0.08 {
                fb.fill_rect(x - 2, y - 2, 5, 5, [255, 255, 255]);
            }
        }

        // Health & SUPER bars
        for f in &self.f {
            let left = f.i == 0;
            let bw = 96;
            let x0 = if left { 8 } else { TV_W as i32 - 8 - bw };
            let hw = (bw as f32 * f.hp / 100.0).round() as i32;
            let gw = (bw as f32 * f.ghost / 100.0).round() as i32;
            fb.fill_rect(x0 - 1, 5, bw + 2, 7, [11, 11, 16]);
            fb.fill_rect(x0, 6, bw, 5, [58, 20, 24]);
            if gw > 0 {
                fb.fill_rect(if left { x0 } else { x0 + bw - gw }, 6, gw, 5, [217, 64, 64]);
            }
            if hw > 0 {
                fb.fill_rect(if left { x0 } else { x0 + bw - hw }, 6, hw, 5, [247, 211, 84]);
            }
            fb.draw_text(
                if left { "CLAUDE" } else { "CODEX" },
                if left { x0 } else { x0 + bw },
                14,
                [255, 255, 255],
                1,
                if left { Align::Left } else { Align::Right },
                Some([0, 0, 0]),
            );
            for k in 0..2 {
                let px = if left { x0 + bw - 4 - k * 6 } else { x0 + k * 6 };
                fb.fill_rect(px, 14, 4, 4, if (k as u32) < f.wins { [247, 211, 84] } else { [51, 48, 58] });
            }
            let mx = if left { 8 } else { TV_W as i32 - 52 };
            fb.fill_rect(mx, TV_H as i32 - 9, 44, 4, [11, 11, 16]);
            let full = f.meter >= 1.0;
            let mc = if full {
                if ((self.t * 8.0) as i32 & 1) != 0 { [127, 227, 255] } else { [63, 184, 255] }
            } else {
                [47, 127, 208]
            };
            fb.fill_rect(mx + 1, TV_H as i32 - 8, (42.0 * f.meter) as i32, 2, mc);
            if full {
                fb.draw_text(
                    "SUPER",
                    if left { mx + 47 } else { mx - 3 },
                    TV_H as i32 - 9,
                    [127, 227, 255],
                    1,
                    if left { Align::Left } else { Align::Right },
                    None,
                );
            }
        }
        // Center round timer
        let mut tm = StackStr::new();
        tm.push_u32(self.round_t.ceil() as u32, 2);
        fb.fill_rect(TV_W as i32 / 2 - 10, 3, 20, 11, [11, 11, 16]);
        fb.draw_text(
            tm.as_str(),
            TV_W as i32 / 2,
            5,
            if self.round_t < 10.0 { [255, 107, 91] } else { [255, 255, 255] },
            1,
            Align::Center,
            None,
        );

        match self.phase {
            FtPhase::Intro => fb.draw_banner("FIGHT!", 48, [247, 211, 84], 4),
            FtPhase::Ko => fb.draw_banner("K.O.", 44, [255, 75, 62], 5),
            FtPhase::End => fb.draw_banner("TIME UP", 48, [255, 255, 255], 3),
            FtPhase::Fight => {}
        }
    }
}

/* ---------- Unified TvGamesSim (single preallocated 256×144 RGBA buffer) ---------- */

pub struct TvGamesSim {
    pixels: Vec<u8>,
    kind: TvGameKind,
    title_t: f32,
    football: FootballState,
    racing: RacingState,
    fighting: FightingState,
    title_sky: Vec<Building>,
}

impl TvGamesSim {
    pub fn new(kind: TvGameKind, seed: u32) -> Self {
        let mut sim = Self {
            pixels: vec![0u8; TV_BUF_LEN],
            kind,
            title_t: 0.0,
            football: FootballState::new(seed.wrapping_add(1)),
            racing: RacingState::new(seed.wrapping_add(2)),
            fighting: FightingState::new(seed.wrapping_add(3)),
            title_sky: build_skyline(13, 300, 10, 26),
        };
        sim.render();
        sim
    }

    pub fn kind(&self) -> TvGameKind {
        self.kind
    }

    pub fn set_game(&mut self, kind: TvGameKind) {
        self.kind = kind;
        self.render();
    }

    pub fn cycle_game(&mut self) -> TvGameKind {
        self.kind = match self.kind {
            TvGameKind::Football => TvGameKind::Racing,
            TvGameKind::Racing => TvGameKind::Fighting,
            TvGameKind::Fighting => TvGameKind::Title,
            TvGameKind::Title => TvGameKind::Football,
        };
        self.render();
        self.kind
    }

    pub fn step(&mut self, dt: f32) {
        self.step_with_input(dt, None, None);
    }

    pub fn step_with_input(&mut self, dt: f32, p1: Option<PadInput>, p2: Option<PadInput>) {
        let clamped = dt.clamp(0.0, 0.1);
        let mut left = clamped;
        while left > 1e-5 {
            let h = left.min(1.0 / 60.0);
            left -= h;
            match self.kind {
                TvGameKind::Title => self.title_t += h,
                TvGameKind::Football => self.football.step_fixed(h, p1, p2),
                TvGameKind::Racing => self.racing.step_fixed(h, p1, p2),
                TvGameKind::Fighting => self.fighting.step_fixed(h, p1, p2),
            }
        }
        match self.kind {
            TvGameKind::Football => {
                self.football.shake = (self.football.shake - clamped * 18.0).max(0.0);
                self.football.flash = (self.football.flash - clamped * 2.2).max(0.0);
            }
            TvGameKind::Racing => {
                self.racing.shake = (self.racing.shake - clamped * 18.0).max(0.0);
                self.racing.flash = (self.racing.flash - clamped * 2.2).max(0.0);
            }
            TvGameKind::Fighting => {
                self.fighting.shake = (self.fighting.shake - clamped * 18.0).max(0.0);
                self.fighting.flash = (self.fighting.flash - clamped * 2.2).max(0.0);
            }
            TvGameKind::Title => {}
        }
        self.render();
    }

    fn render(&mut self) {
        let (shake, flash, seed_t) = match self.kind {
            TvGameKind::Football => (self.football.shake, self.football.flash, self.football.t),
            TvGameKind::Racing => (self.racing.shake, self.racing.flash, self.racing.t),
            TvGameKind::Fighting => (self.fighting.shake, self.fighting.flash, self.fighting.t),
            TvGameKind::Title => (0.0, 0.0, self.title_t),
        };
        let mut fb = PixBuf::new(&mut self.pixels, TV_W, TV_H);
        fb.clear([0, 0, 0]);
        if shake > 0.4 {
            fb.ox = ((seed_t * 97.0).sin() * shake * 0.5).round() as i32;
            fb.oy = ((seed_t * 113.0).cos() * shake * 0.5).round() as i32;
        }
        match self.kind {
            TvGameKind::Football => self.football.draw(&mut fb),
            TvGameKind::Racing => self.racing.draw(&mut fb),
            TvGameKind::Fighting => self.fighting.draw(&mut fb),
            TvGameKind::Title => draw_title(&mut fb, self.title_t, &self.title_sky),
        }
        fb.ox = 0;
        fb.oy = 0;
        if flash > 0.02 {
            fb.fill_rect_alpha(0, 0, TV_W as i32, TV_H as i32, [255, 255, 255], flash * 0.7);
        }
    }

    pub fn rgba(&self) -> &[u8] {
        &self.pixels
    }

    /// Computes normalized average RGB `[0..=1; 3]` across the frame for dynamic TV ambilight.
    pub fn dominant_rgb(&self) -> [f32; 3] {
        let mut r = 0u64;
        let mut g = 0u64;
        let mut b = 0u64;
        let mut n = 0u64;
        for y in (0..TV_H).step_by(4) {
            let row = y * TV_W * 4;
            for x in (0..TV_W).step_by(4) {
                let idx = row + x * 4;
                r += self.pixels[idx] as u64;
                g += self.pixels[idx + 1] as u64;
                b += self.pixels[idx + 2] as u64;
                n += 1;
            }
        }
        let denom = (n.max(1) as f32) * 255.0;
        [r as f32 / denom, g as f32 / denom, b as f32 / denom]
    }
}

fn draw_title(fb: &mut PixBuf<'_>, t: f32, sky: &[Building]) {
    let idx = ((t / 5.0).floor() as usize) % 3;
    match idx {
        0 => {
            fb.fill_vgrad(0, TV_H as i32, [15, 59, 34], [34, 96, 48], [47, 122, 58], 0.5);
            for i in 0..8 {
                let col = if i % 2 == 1 { [61, 151, 70] } else { [70, 166, 80] };
                fb.fill_rect(0, 80 + i * 8, TV_W as i32, 8, col);
            }
            fb.fill_rect(0, 80, TV_W as i32, 1, [232, 242, 228]);
            let bx = (128.0 + (t * 0.9).sin() * 84.0).round() as i32;
            let bz = ((t * 3.2).sin().abs() * 30.0).round() as i32;
            fb.fill_rect_alpha(bx - 4, 114, 9, 2, [0, 0, 0], 0.3);
            fb.fill_circle(bx, 108 - bz, 5, [255, 255, 255]);
            fb.fill_rect(bx - 1, 106 - bz, 3, 3, [26, 26, 26]);
            fb.draw_banner("GOAL STARS", 28, [255, 255, 255], 3);
        }
        1 => {
            fb.fill_vgrad(0, 80, [43, 34, 87], [210, 102, 94], [245, 181, 108], 0.65);
            for b in sky {
                if b.x < TV_W as i32 + 20 {
                    fb.fill_rect(b.x, 81 - b.h, b.w, b.h, [58, 43, 87]);
                }
            }
            fb.fill_rect(0, 80, TV_W as i32, 64, [84, 142, 64]);
            fb.fill_trapezoid(0.0, TV_W as f32, TV_H as i32, 112.0, 144.0, 80, [108, 107, 114]);
            for i in 0..7 {
                let p = ((i as f32 + t * 2.2) % 7.0) / 7.0;
                let lw = 1.0 + p * 3.5;
                fb.fill_rect_f(128.0 - lw * 0.5, 80.0 + p * p * 64.0, lw, 1.5 + p * 5.0, [237, 231, 218]);
            }
            draw_car_back(
                fb,
                128.0 + (t * 1.5).sin() * 14.0,
                126.0,
                46.0 / 620.0,
                TEAM_KIT[0],
                ((t * 2.0) as i32 & 1) == 0,
                t,
                0.0,
            );
            fb.draw_banner("SAIGON RUSH", 28, [255, 255, 255], 3);
        }
        _ => {
            fb.fill_vgrad(0, TV_H as i32, [35, 25, 66], [160, 82, 92], [224, 122, 95], 0.55);
            for b in sky {
                if b.x < TV_W as i32 + 20 {
                    fb.fill_rect(b.x + 12, 118 - b.h, b.w, b.h, [42, 30, 63]);
                }
            }
            fb.fill_rect(0, 118, TV_W as i32, 26, [94, 88, 100]);
            for s in [-1_i32, 1] {
                let x = 128 + s * 30;
                let bob = (t * 6.0 + s as f32).sin().round() as i32;
                fb.fill_rect(x - 5, 96 + bob, 10, 12, [21, 16, 30]);
                fb.fill_circle(x, 89 + bob, 6, [21, 16, 30]);
                fb.fill_rect(x - 5, 108, 4, 10, [21, 16, 30]);
                fb.fill_rect(x + 1, 108, 4, 10, [21, 16, 30]);
            }
            fb.draw_text("VS", 128, 92, [247, 211, 84], 2, Align::Center, Some([0, 0, 0]));
            fb.draw_banner("ROOFTOP FIGHT", 28, [255, 255, 255], 2);
        }
    }
    if ((t * 1.6) as i32 & 1) == 0 {
        fb.draw_text("PRESS START", 128, 56, [247, 211, 84], 1, Align::Center, Some([0, 0, 0]));
    }
    fb.fill_rect_alpha(0, 0, TV_W as i32, 9, [0, 0, 0], 0.4);
    fb.draw_text("MY OFFICE ARCADE", 4, 2, [255, 255, 255], 1, Align::Left, None);
    let tabs = [("GOAL", [70, 166, 80]), ("RUSH", [210, 102, 94]), ("FIGHT", [107, 79, 160])];
    for (i, (lbl, col)) in tabs.iter().enumerate() {
        let sel = i == idx;
        let x = 128 - 46 + (i as i32) * 32;
        let y = if sel { 130 } else { 132 };
        fb.fill_rect(x - 1, y - 1, 30, 11, if sel { [255, 255, 255] } else { [20, 20, 28] });
        fb.fill_rect(x, y, 28, 9, *col);
        fb.draw_text(lbl, x + 14, y + 2, [255, 255, 255], 1, Align::Center, None);
    }
}

/* ---------- Zero-allocation live Arcade Cabinet screen (128×96) ---------- */

pub struct ArcadeScreenSim {
    pixels: Vec<u8>,
    t: f32,
    coin_until: f32,
}

impl ArcadeScreenSim {
    pub fn new() -> Self {
        let mut s = Self {
            pixels: vec![0u8; ARCADE_BUF_LEN],
            t: 0.0,
            coin_until: 0.0,
        };
        s.draw_arcade_frame(0.0, false);
        s
    }

    pub fn insert_coin(&mut self) {
        self.coin_until = self.t + 3.5;
        let t = self.t;
        self.draw_arcade_frame(t, true);
    }

    pub fn step(&mut self, dt: f32) {
        self.t += dt.max(0.0);
        let t = self.t;
        let coin = self.coin_until > t;
        self.draw_arcade_frame(t, coin);
    }

    /// Renders Space Invaders / INSERT COIN directly into `self.pixels` with zero allocations.
    pub fn draw_arcade_frame(&mut self, t: f32, coin: bool) {
        let mut fb = PixBuf::new(&mut self.pixels, ARCADE_W, ARCADE_H);
        fb.clear([5, 3, 12]);
        if coin {
            if ((t * 4.0).floor() as i32 & 1) != 0 {
                fb.draw_text("INSERT", 64, 28, [255, 209, 102], 2, Align::Center, Some([0, 0, 0]));
                fb.draw_text("COIN", 64, 46, [255, 209, 102], 2, Align::Center, Some([0, 0, 0]));
            }
            fb.draw_text("READY 1P", 64, 74, [76, 201, 240], 1, Align::Center, None);
            return;
        }
        // Subtle starfield
        for i in 0..16_i32 {
            let sx = (i * 37 + 11) % ARCADE_W as i32;
            let sy = (i * 23 + (t * 8.0) as i32) % (ARCADE_H as i32 - 12);
            fb.put_px(sx, sy, [60, 52, 92]);
        }
        // 3×6 marching Space Invaders
        let sweep = (t * 0.8).sin() * 14.0;
        let row_cols = [[247, 37, 133], [76, 201, 240], [255, 209, 102]];
        let frame = ((t * 2.0).floor() as i32) & 1;
        for (r, &col) in row_cols.iter().enumerate() {
            for c in 0..6 {
                let x = (18.0 + c as f32 * 16.0 + sweep).round() as i32;
                let y = 10 + (r as i32) * 12;
                fb.fill_rect(x, y, 9, 5, col);
                fb.fill_rect(x + 2, y + 1, 1, 2, [5, 3, 12]);
                fb.fill_rect(x + 6, y + 1, 1, 2, [5, 3, 12]);
                fb.fill_rect(x + if frame != 0 { 0 } else { 2 }, y + 5, 2, 3, col);
                fb.fill_rect(x + if frame != 0 { 7 } else { 5 }, y + 5, 2, 3, col);
            }
        }
        // Player ship + laser bolt
        let sx = (64.0 + (t * 1.3).sin() * 40.0).round() as i32;
        fb.fill_rect(sx - 6, 80, 13, 5, [124, 252, 154]);
        fb.fill_rect(sx - 2, 76, 5, 4, [124, 252, 154]);
        let by = (74.0 - (t * 78.0).rem_euclid(64.0)).round() as i32;
        fb.fill_rect(sx, by, 2, 5, [255, 255, 255]);
        // Shields
        for bx in [24, 60, 96] {
            fb.fill_rect(bx, 68, 10, 4, [76, 201, 240]);
        }
        // Score HUD (stack formatted, zero alloc)
        let mut sc = StackStr::new();
        sc.push_u32(((t * 37.0).abs() as u32) % 99_999, 5);
        fb.draw_text(sc.as_str(), 4, 88, [255, 209, 102], 1, Align::Left, None);
        fb.draw_text("HI 99999", 124, 88, [247, 37, 133], 1, Align::Right, None);
    }

    pub fn rgba(&self) -> &[u8] {
        &self.pixels
    }
}

impl Default for ArcadeScreenSim {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn fnv1a_hash(bytes: &[u8]) -> u64 {
        let mut h = 0xcbf29ce484222325u64;
        for &b in bytes {
            h ^= b as u64;
            h = h.wrapping_mul(0x100000001b3);
        }
        h
    }

    fn non_black_pixels(rgba: &[u8]) -> usize {
        rgba.chunks_exact(4)
            .filter(|px| px[0] > 8 || px[1] > 8 || px[2] > 8)
            .count()
    }

    #[test]
    fn tv_games_football_racing_fighting_zero_alloc_and_distinct_frames() {
        for kind in [TvGameKind::Football, TvGameKind::Racing, TvGameKind::Fighting] {
            let mut sim = TvGamesSim::new(kind, 42);
            let initial_ptr = sim.rgba().as_ptr();
            assert_eq!(sim.rgba().len(), TV_W * TV_H * 4);

            let mut hashes = Vec::new();
            for tick in 0..120 {
                sim.step(1.0 / 30.0);
                assert_eq!(
                    sim.rgba().as_ptr(),
                    initial_ptr,
                    "{kind:?} reallocated pixel buffer on tick {tick}"
                );
                assert_eq!(sim.rgba().len(), TV_W * TV_H * 4);
                if tick % 15 == 0 {
                    hashes.push(fnv1a_hash(sim.rgba()));
                }
            }

            let non_black = non_black_pixels(sim.rgba());
            assert!(
                non_black > (TV_W * TV_H) / 4,
                "{kind:?} frame looks mostly black ({non_black} lit pixels)"
            );
            let unique_count = {
                let mut sorted = hashes.clone();
                sorted.sort_unstable();
                sorted.dedup();
                sorted.len()
            };
            assert!(
                unique_count >= 4,
                "{kind:?} must animate across 120 ticks (only {unique_count} unique frames)"
            );
            let dom = sim.dominant_rgb();
            assert!(
                dom[0] + dom[1] + dom[2] > 0.15,
                "{kind:?} dominant_rgb must be non-zero, got {dom:?}"
            );
        }
    }

    #[test]
    fn tv_games_title_cycles_and_arcade_screen_mutates_in_place() {
        let mut sim = TvGamesSim::new(TvGameKind::Title, 7);
        let ptr = sim.rgba().as_ptr();
        sim.step(0.5);
        let h0 = fnv1a_hash(sim.rgba());
        assert_eq!(sim.rgba().as_ptr(), ptr);
        sim.step(5.2);
        let h1 = fnv1a_hash(sim.rgba());
        assert_eq!(sim.rgba().as_ptr(), ptr);
        assert_ne!(h0, h1, "Title attract screen must evolve over time");

        let mut arc = ArcadeScreenSim::new();
        let aptr = arc.rgba().as_ptr();
        assert_eq!(arc.rgba().len(), ARCADE_W * ARCADE_H * 4);
        arc.draw_arcade_frame(0.5, false);
        let ah0 = fnv1a_hash(arc.rgba());
        assert_eq!(arc.rgba().as_ptr(), aptr);
        assert!(non_black_pixels(arc.rgba()) > 100);
        arc.draw_arcade_frame(2.0, false);
        let ah1 = fnv1a_hash(arc.rgba());
        assert_eq!(arc.rgba().as_ptr(), aptr);
        assert_ne!(ah0, ah1, "Arcade invaders frame must animate");
        arc.draw_arcade_frame(2.25, true);
        let ah_coin = fnv1a_hash(arc.rgba());
        assert_eq!(arc.rgba().as_ptr(), aptr);
        assert_ne!(ah1, ah_coin, "Arcade coin-insert mode must render distinct frame");
    }
}
