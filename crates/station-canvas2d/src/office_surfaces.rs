use crate::canvas::{Canvas2d, RgbaSurface, hex, hex_rgba};
use crate::text::{FontEngine, TextAlign};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ScreenKind {
    Off = 0,
    Code = 1,
    Read = 2,
    Term = 3,
    Web = 4,
    Chat = 5,
    Wait = 6,
    Sleep = 7,
}

pub const ALL_SCREEN_KINDS: [ScreenKind; 8] = [
    ScreenKind::Off,
    ScreenKind::Code,
    ScreenKind::Read,
    ScreenKind::Term,
    ScreenKind::Web,
    ScreenKind::Chat,
    ScreenKind::Wait,
    ScreenKind::Sleep,
];

struct SeedRng(u64);
impl SeedRng {
    fn next_f32(&mut self) -> f32 {
        self.0 = (self.0 * 16_807) % 2_147_483_647;
        self.0 as f32 / 2_147_483_647.0
    }
}

fn draw_code_lines(
    c: &mut Canvas2d,
    rng: &mut SeedRng,
    x0: f32,
    y0: f32,
    rows: usize,
    palette: &[[u8; 4]],
    highlight: Option<usize>,
) {
    let w = c.width() as f32;
    for r in 0..rows {
        let y = y0 + (r as f32) * 20.0;
        if highlight == Some(r) {
            c.fill_rect(0.0, y - 4.0, w, 20.0, [255, 214, 102, 72]);
        }
        c.fill_rect(8.0, y + 3.0, 18.0, 8.0, hex("#4B5263"));
        let mut x = x0 + (rng.next_f32() * 4.0).floor() * 14.0;
        let n = 1 + (rng.next_f32() * 5.0).floor() as usize;
        for _ in 0..n {
            if x >= w - 20.0 {
                break;
            }
            let bw = 18.0 + rng.next_f32() * 70.0;
            let idx = (rng.next_f32() * palette.len() as f32).floor() as usize;
            c.fill_rect(x, y + 2.0, bw, 10.0, palette[idx.min(palette.len() - 1)]);
            x += bw + 8.0;
        }
    }
}

/// Exact Oracle `src/office/world/screens.js` 512×640 scrolling desk screen generator.
pub fn draw_desk_screen(kind: ScreenKind, fonts: &mut FontEngine) -> RgbaSurface {
    const W: u32 = 512;
    const H: u32 = 640;
    let mut c = Canvas2d::new(W, H);
    let mut rng = SeedRng(7 + kind as u64);

    match kind {
        ScreenKind::Off => {
            c.linear_gradient_rect(
                0.0,
                0.0,
                W as f32,
                H as f32,
                (0.0, 0.0),
                (W as f32, H as f32),
                &[(0.0, hex("#0C1622")), (1.0, hex("#101C2B"))],
            );
        }
        ScreenKind::Code => {
            c.fill(hex("#1E1F2B"));
            c.fill_rect(0.0, 0.0, 34.0, H as f32, hex("#262838"));
            let pal = [
                hex("#C792EA"),
                hex("#82AAFF"),
                hex("#C3E88D"),
                hex("#F78C6C"),
                hex("#89DDFF"),
                hex("#D6DEEB"),
                hex("#D6DEEB"),
            ];
            draw_code_lines(&mut c, &mut rng, 44.0, 10.0, 32, &pal, None);
        }
        ScreenKind::Read => {
            c.fill(hex("#1E1F2B"));
            c.fill_rect(0.0, 0.0, 34.0, H as f32, hex("#262838"));
            let pal = [hex("#C792EA"), hex("#82AAFF"), hex("#C3E88D"), hex("#D6DEEB")];
            draw_code_lines(&mut c, &mut rng, 44.0, 10.0, 32, &pal, Some(9));
            c.fill_rect(0.0, 10.0 + 25.0 * 20.0 - 4.0, W as f32, 20.0, [255, 214, 102, 72]);
        }
        ScreenKind::Term => {
            c.fill(hex("#0B0F14"));
            for r in 0..32 {
                let y = 10.0 + (r as f32) * 20.0;
                if rng.next_f32() < 0.3 {
                    c.fill_rect(10.0, y + 2.0, 10.0, 10.0, hex("#5BE37D"));
                    c.fill_rect(28.0, y + 2.0, 60.0 + rng.next_f32() * 220.0, 10.0, hex("#E6EDF3"));
                } else {
                    let v = rng.next_f32();
                    let col = if v < 0.12 {
                        hex("#FF7B72")
                    } else if v < 0.2 {
                        hex("#FFD166")
                    } else {
                        hex("#8B949E")
                    };
                    c.fill_rect(10.0, y + 2.0, 40.0 + rng.next_f32() * 380.0, 10.0, col);
                }
            }
        }
        ScreenKind::Web => {
            c.fill(hex("#F4F5F7"));
            for p in 0..2 {
                let y0 = (p * 320) as f32;
                c.fill_rect(0.0, y0, W as f32, 30.0, hex("#DDE1E7"));
                c.fill_rect(60.0, y0 + 7.0, W as f32 - 120.0, 16.0, hex("#FFFFFF"));
                c.fill_rect(24.0, y0 + 50.0, W as f32 - 48.0, 90.0, hex("#3F7CC4"));
                c.fill_rect(44.0, y0 + 72.0, 200.0, 16.0, hex("#FFFFFF"));
                c.fill_rect(44.0, y0 + 98.0, 140.0, 10.0, hex("#FFFFFF"));
                for r in 0..6 {
                    c.fill_rect(
                        24.0,
                        y0 + 160.0 + (r as f32) * 22.0,
                        280.0 + rng.next_f32() * 160.0,
                        9.0,
                        hex("#B8BEC8"),
                    );
                }
                c.fill_rect(340.0, y0 + 160.0, 148.0, 100.0, hex("#E8742A"));
            }
        }
        ScreenKind::Chat => {
            c.fill(hex("#F7F5F0"));
            for r in 0..8 {
                let y = 16.0 + (r as f32) * 78.0;
                let you = r % 2 == 0;
                let bw = 180.0 + rng.next_f32() * 200.0;
                let bh = 30.0 + (rng.next_f32() * 3.0).floor() * 14.0;
                c.fill_rect(
                    if you { W as f32 - bw - 16.0 } else { 16.0 },
                    y,
                    bw,
                    bh,
                    if you { hex("#E3D9F7") } else { hex("#FFFFFF") },
                );
                if !you {
                    c.fill_circle(30.0, y + 14.0, 8.0, hex("#D97757"));
                }
                let lines = ((bh / 14.0) as i32 - 1).max(1) as usize;
                for k in 0..lines {
                    let lw = (bw - 44.0 - rng.next_f32() * 40.0).max(20.0);
                    c.fill_rect(
                        if you { W as f32 - bw - 4.0 } else { 46.0 },
                        y + 10.0 + (k as f32) * 14.0,
                        lw,
                        6.0,
                        hex("#9AA0AA"),
                    );
                }
            }
        }
        ScreenKind::Wait => {
            c.fill(hex("#1E1F2B"));
            c.fill_rect(0.0, 0.0, 34.0, H as f32, hex("#262838"));
            let pal = [hex("#C792EA"), hex("#82AAFF"), hex("#C3E88D"), hex("#D6DEEB")];
            draw_code_lines(&mut c, &mut rng, 44.0, 10.0, 32, &pal, None);
            c.fill_rect(0.0, 0.0, W as f32, H as f32, [8, 10, 16, 140]);
            for y0 in [0.0_f32, 320.0_f32] {
                c.fill_rect(96.0, y0 + 90.0, 320.0, 140.0, hex("#2A2F3A"));
                c.fill_rect(96.0, y0 + 90.0, 320.0, 8.0, hex("#FFB23F"));
                c.fill_rect(118.0, y0 + 120.0, 220.0, 12.0, hex("#E6EDF3"));
                c.fill_rect(118.0, y0 + 144.0, 180.0, 8.0, hex("#8B949E"));
                c.fill_rect(232.0, y0 + 182.0, 80.0, 28.0, hex("#3DDC84"));
                c.fill_rect(322.0, y0 + 182.0, 80.0, 28.0, hex("#4B5263"));
            }
        }
        ScreenKind::Sleep => {
            c.linear_gradient_rect(
                0.0,
                0.0,
                W as f32,
                H as f32,
                (0.0, 0.0),
                (0.0, H as f32),
                &[
                    (0.0, hex("#141B33")),
                    (0.5, hex("#1D1640")),
                    (1.0, hex("#141B33")),
                ],
            );
            for _ in 0..60 {
                let a = ((0.2 + rng.next_f32() * 0.6) * 255.0) as u8;
                let sx = rng.next_f32() * W as f32;
                let sy = rng.next_f32() * H as f32;
                c.fill_rect(sx, sy, 2.0, 2.0, [255, 255, 255, a]);
            }
            let zc = [201, 184, 255, 217];
            fonts.draw_text(&mut c, "z", 300.0, 102.0, 54.0, zc, TextAlign::Left, None, true, false);
            fonts.draw_text(&mut c, "z", 350.0, 76.0, 38.0, zc, TextAlign::Left, None, true, false);
            fonts.draw_text(&mut c, "z", 385.0, 58.0, 26.0, zc, TextAlign::Left, None, true, false);
            fonts.draw_text(&mut c, "z", 300.0, 422.0, 54.0, zc, TextAlign::Left, None, true, false);
            fonts.draw_text(&mut c, "z", 350.0, 396.0, 38.0, zc, TextAlign::Left, None, true, false);
        }
    }

    c.into_surface()
}

#[derive(Debug, Clone)]
pub struct TvStatsInput {
    pub title: String,
    pub clock_hhmm: String,
    pub ram_pct: Option<u8>,
    pub ram_critical: bool,
    pub cpu_pct: u8,
    pub working: usize,
    pub waiting: usize,
    pub resting: usize,
    pub today_summary: String,
    pub plan_pct: Option<u8>,
    pub pulse_30m: [u32; 30],
}

/// Exact Oracle `src/office/world/props/tv.js` 512×280 live machine stats TV screen.
pub fn draw_stats_tv(stats: &TvStatsInput, fonts: &mut FontEngine) -> RgbaSurface {
    const W: u32 = 512;
    const H: u32 = 280;
    let mut c = Canvas2d::new(W, H);
    c.linear_gradient_rect(
        0.0,
        0.0,
        W as f32,
        H as f32,
        (0.0, 0.0),
        (0.0, H as f32),
        &[(0.0, hex("#0E1726")), (1.0, hex("#0A0F18"))],
    );

    fonts.draw_text(&mut c, &stats.title, 22.0, 18.0, 15.0, hex("#8D97AA"), TextAlign::Left, Some(340.0), true, false);
    fonts.draw_text(&mut c, &stats.clock_hhmm, W as f32 - 22.0, 18.0, 15.0, hex("#8D97AA"), TextAlign::Right, None, true, false);

    let ram_label = stats
        .ram_pct
        .map(|p| format!("{}%", p.min(100)))
        .unwrap_or_else(|| "–".to_owned());
    let ram_col = match stats.ram_pct {
        Some(_) if stats.ram_critical => hex("#FF5F5F"),
        Some(p) if p > 80 => hex("#F2B544"),
        Some(_) => hex("#3DDC84"),
        None => hex("#5E687B"),
    };
    fonts.draw_text(&mut c, &ram_label, 22.0, 48.0, 62.0, hex("#E8ECF4"), TextAlign::Left, None, true, false);
    fonts.draw_text(&mut c, "RAM", 26.0, 114.0, 14.0, hex("#8D97AA"), TextAlign::Left, None, false, false);
    c.fill_rect(22.0, 136.0, 210.0, 10.0, hex("#263040"));
    c.fill_rect(22.0, 136.0, 2.1 * stats.ram_pct.unwrap_or(0).min(100) as f32, 10.0, ram_col);

    let cpu = stats.cpu_pct.min(100);
    fonts.draw_text(&mut c, &format!("CPU {cpu}%"), 22.0, 158.0, 14.0, hex("#8D97AA"), TextAlign::Left, None, false, false);
    c.fill_rect(22.0, 180.0, 210.0, 6.0, hex("#263040"));
    c.fill_rect(22.0, 180.0, 2.1 * cpu as f32, 6.0, hex("#6AAEFF"));

    let rows = [
        (hex("#3DDC84"), format!("{} working", stats.working)),
        (hex("#F2B544"), format!("{} waiting", stats.waiting)),
        (hex("#5E687B"), format!("{} resting", stats.resting)),
    ];
    for (i, (col, label)) in rows.iter().enumerate() {
        let y = 70.0 + (i as f32) * 32.0;
        c.fill_circle(282.0, y, 6.0, *col);
        fonts.draw_text(&mut c, label, 298.0, y - 9.0, 18.0, hex("#E8ECF4"), TextAlign::Left, Some(195.0), true, false);
    }

    fonts.draw_text(&mut c, &stats.today_summary, 274.0, 158.0, 14.0, hex("#8D97AA"), TextAlign::Left, Some(220.0), false, false);
    if let Some(pp) = stats.plan_pct {
        let pcol = if pp >= 95 {
            hex("#FF5F5F")
        } else if pp >= 80 {
            hex("#F2B544")
        } else {
            hex("#8D97AA")
        };
        fonts.draw_text(&mut c, &format!("Plan {pp}%"), 274.0, 180.0, 14.0, pcol, TextAlign::Left, None, false, false);
    }

    let mx = stats.pulse_30m.iter().copied().max().unwrap_or(1).max(1) as f32;
    fonts.draw_text(&mut c, "ACTIVITY · 30m", 22.0, 202.0, 12.0, hex("#8D97AA"), TextAlign::Left, None, false, false);
    for (i, &v) in stats.pulse_30m.iter().enumerate() {
        let bh = if v > 0 { 4.0 + (v as f32 / mx) * 40.0 } else { 2.0 };
        let col = if v > 0 { hex("#3DDC84") } else { hex("#263040") };
        c.fill_rect(22.0 + (i as f32) * 15.6, 266.0 - bh, 10.0, bh, col);
    }

    c.into_surface()
}

/// Exact Oracle `src/office/world/props/arcade.js` 96×72 pixel invaders screen.
pub fn draw_arcade_frame(t: f32, insert_coin: bool, fonts: &mut FontEngine) -> RgbaSurface {
    let mut c = Canvas2d::new(96, 72);
    c.fill(hex("#05030C"));
    if insert_coin {
        if ((t * 4.0).floor() as i32) % 2 != 0 {
            fonts.draw_text(&mut c, "INSERT", 48.0, 18.0, 11.0, hex("#FFD166"), TextAlign::Center, None, true, true);
            fonts.draw_text(&mut c, "COIN", 48.0, 32.0, 11.0, hex("#FFD166"), TextAlign::Center, None, true, true);
        }
        fonts.draw_text(&mut c, "READY", 48.0, 56.0, 8.0, hex("#4CC9F0"), TextAlign::Center, None, false, true);
        return c.into_surface();
    }
    let sweep = (t * 0.8).sin() * 10.0;
    let row_cols = [hex("#F72585"), hex("#4CC9F0"), hex("#FFD166")];
    let frame = ((t * 2.0).floor() as i32).rem_euclid(2);
    for (r, &col) in row_cols.iter().enumerate() {
        for col_i in 0..6 {
            let x = 14.0 + (col_i as f32) * 12.0 + sweep;
            let y = 8.0 + (r as f32) * 9.0;
            c.fill_rect(x, y, 7.0, 4.0, col);
            c.fill_rect(x + if frame != 0 { 0.0 } else { 1.0 }, y + 4.0, 2.0, 2.0, col);
            c.fill_rect(x + if frame != 0 { 5.0 } else { 4.0 }, y + 4.0, 2.0, 2.0, col);
        }
    }
    let sx = 48.0 + (t * 1.3).sin() * 30.0;
    c.fill_rect(sx - 5.0, 62.0, 10.0, 4.0, hex("#7CFC9A"));
    c.fill_rect(sx - 1.0, 59.0, 2.0, 3.0, hex("#7CFC9A"));
    let by = 58.0 - ((t * 60.0).rem_euclid(50.0));
    c.fill_rect(sx, by, 1.0, 3.0, hex("#FFFFFF"));
    let score = format!("{:05}", ((t * 37.0).abs() as u32) % 99_999);
    fonts.draw_text(&mut c, &score, 3.0, 62.0, 8.0, hex("#FFD166"), TextAlign::Left, None, false, true);
    c.into_surface()
}

pub fn draw_arcade_marquee(fonts: &mut FontEngine) -> RgbaSurface {
    let mut c = Canvas2d::new(256, 64);
    c.fill(hex("#12081F"));
    fonts.draw_text(&mut c, "ARCADE", 128.0, 14.0, 38.0, hex_rgba("#F72585", 140), TextAlign::Center, None, true, false);
    fonts.draw_text(&mut c, "ARCADE", 128.0, 12.0, 38.0, hex("#FFD6F0"), TextAlign::Center, None, true, false);
    c.into_surface()
}

/// Exact Oracle `src/office/world/props/karaoke.js` 512×64 lit KARAOKE header band.
pub fn draw_karaoke_band(text: &str, fonts: &mut FontEngine) -> RgbaSurface {
    let mut c = Canvas2d::new(512, 64);
    c.linear_gradient_rect(
        0.0,
        0.0,
        512.0,
        64.0,
        (0.0, 0.0),
        (512.0, 0.0),
        &[
            (0.0, hex("#FF3D9A")),
            (0.5, hex("#A23CFF")),
            (1.0, hex("#3D7BFF")),
        ],
    );
    fonts.draw_text(&mut c, text, 256.0, 12.0, 40.0, hex("#FFFFFF"), TextAlign::Center, Some(380.0), true, false);
    fonts.draw_text(&mut c, "♪", 40.0, 14.0, 32.0, hex("#FFFFFF"), TextAlign::Center, None, true, false);
    fonts.draw_text(&mut c, "♫", 472.0, 14.0, 32.0, hex("#FFFFFF"), TextAlign::Center, None, true, false);
    c.into_surface()
}

pub fn draw_karaoke_qr_badge(fonts: &mut FontEngine) -> RgbaSurface {
    let mut c = Canvas2d::new(256, 128);
    c.round_rect(4.0, 4.0, 248.0, 120.0, 14.0, hex_rgba("#FFFFFF", 224));
    fonts.draw_text(&mut c, "5.000đ", 18.0, 24.0, 26.0, hex("#A23CFF"), TextAlign::Left, None, true, false);
    fonts.draw_text(&mut c, "/ bài · QR", 20.0, 60.0, 17.0, hex("#4A4458"), TextAlign::Left, None, false, false);
    for i in 0..7 {
        for j in 0..7 {
            if (i * 7 + j * 3 + i * j) % 3 != 0 || i % 6 == 0 || j % 6 == 0 {
                c.fill_rect(160.0 + (i as f32) * 11.0, 22.0 + (j as f32) * 11.0, 10.0, 10.0, hex("#15161A"));
            }
        }
    }
    c.into_surface()
}

pub fn draw_karaoke_screen(title: &str, lyric: &str, progress: f32, fonts: &mut FontEngine) -> RgbaSurface {
    let mut c = Canvas2d::new(512, 288);
    c.linear_gradient_rect(
        0.0,
        0.0,
        512.0,
        288.0,
        (0.0, 0.0),
        (512.0, 288.0),
        &[(0.0, hex("#1C0E34")), (0.5, hex("#35145B")), (1.0, hex("#111936"))],
    );
    fonts.draw_text(&mut c, title, 256.0, 40.0, 24.0, hex("#FF8AD4"), TextAlign::Center, Some(460.0), true, false);
    fonts.draw_text(&mut c, lyric, 256.0, 126.0, 30.0, hex("#FFFFFF"), TextAlign::Center, Some(470.0), true, false);
    let pct = progress.clamp(0.0, 1.0);
    c.round_rect(56.0, 220.0, 400.0, 10.0, 5.0, hex_rgba("#FFFFFF", 50));
    c.round_rect(56.0, 220.0, 400.0 * pct, 10.0, 5.0, hex("#FF4FB0"));
    c.into_surface()
}

pub fn draw_game_tv_standby(fonts: &mut FontEngine) -> RgbaSurface {
    let mut c = Canvas2d::new(384, 216);
    c.linear_gradient_rect(
        0.0,
        0.0,
        384.0,
        216.0,
        (0.0, 0.0),
        (384.0, 216.0),
        &[(0.0, hex("#0E1528")), (1.0, hex("#192848"))],
    );
    fonts.draw_text(&mut c, "STATION CONSOLE", 192.0, 74.0, 24.0, hex("#E8ECF4"), TextAlign::Center, None, true, false);
    fonts.draw_text(&mut c, "PRESS START · 2P", 192.0, 118.0, 15.0, hex("#5FA8FF"), TextAlign::Center, None, true, true);
    c.into_surface()
}

pub fn draw_radial_glow() -> RgbaSurface {
    let mut c = Canvas2d::new(128, 128);
    c.radial_gradient_circle(
        64.0,
        64.0,
        64.0,
        &[
            (0.0, [255, 255, 255, 255]),
            (0.55, [255, 255, 255, 90]),
            (1.0, [255, 255, 255, 0]),
        ],
    );
    c.into_surface()
}

pub fn draw_dev_server_badge(port: Option<u16>, fonts: &mut FontEngine) -> RgbaSurface {
    let mut c = Canvas2d::new(96, 32);
    let txt = port.map(|p| format!(":{p}")).unwrap_or_else(|| "dev".to_owned());
    fonts.draw_text(&mut c, &txt, 48.0, 5.0, 20.0, hex("#9FE8B8"), TextAlign::Center, Some(90.0), true, true);
    c.into_surface()
}

pub fn draw_microwave_timer(seconds: u32, fonts: &mut FontEngine) -> RgbaSurface {
    let mut c = Canvas2d::new(128, 48);
    c.fill(hex("#0A120E"));
    let txt = format!("{:02}:{:02}", seconds / 60, seconds % 60);
    fonts.draw_text(&mut c, &txt, 64.0, 10.0, 28.0, hex("#5BE37D"), TextAlign::Center, None, true, true);
    c.into_surface()
}

/// Exact Oracle `microwave.js:192-204` 256×180 A5 paper rule note taped to the microwave cabinet.
pub fn draw_microwave_note(fonts: &mut FontEngine) -> RgbaSurface {
    let mut c = Canvas2d::new(256, 180);
    c.fill(hex("#FBFAF4"));
    // Two masking-tape strips along the top edge
    c.fill_rect(18.0, 0.0, 48.0, 16.0, [240, 226, 160, 192]);
    c.fill_rect(190.0, 0.0, 48.0, 16.0, [240, 226, 160, 192]);
    let red = hex("#C62828");
    fonts.draw_text(&mut c, "MỞ NẮP HỘP", 128.0, 32.0, 26.0, red, TextAlign::Center, Some(232.0), true, false);
    fonts.draw_text(&mut c, "TRƯỚC KHI QUAY", 128.0, 82.0, 22.0, red, TextAlign::Center, Some(232.0), true, false);
    fonts.draw_text(&mut c, "CẢM ƠN!", 128.0, 128.0, 22.0, red, TextAlign::Center, Some(232.0), true, false);
    c.into_surface()
}

/// Tileable 256×256 neutral-warm oak plank texture (256 KB VRAM).
/// Designed to wrap seamlessly in U & V and accept PBR `StandardMaterial::base_color`
/// tinting for Cozy warm oak, Pro dev pale oak, Library dark walnut, and Lounge wood.
pub fn draw_plank_tile() -> RgbaSurface {
    const W: u32 = 256;
    const H: u32 = 256;
    let mut c = Canvas2d::new(W, H);
    let mut rng = SeedRng(104729);
    let rows = 8u32;
    let rh = (H / rows) as f32;

    for r in 0..rows {
        let y = r as f32 * rh;
        let offset = ((r * 83 + 19) % W) as f32;
        let p0 = offset;
        let p1 = (offset + 128.0) % W as f32;
        let shade_a = (232.0 + (rng.next_f32() - 0.5) * 18.0).clamp(212.0, 248.0) as u8;
        let shade_b = (224.0 + (rng.next_f32() - 0.5) * 18.0).clamp(205.0, 244.0) as u8;
        c.fill_rect(0.0, y, W as f32, rh, [shade_a, shade_a.saturating_sub(4), shade_a.saturating_sub(10), 255]);

        // Second plank segment with seamless wrap across x = 256
        let seg_w = 128.0_f32;
        if p1 + seg_w <= W as f32 {
            c.fill_rect(p1, y, seg_w, rh, [shade_b, shade_b.saturating_sub(5), shade_b.saturating_sub(11), 255]);
        } else {
            let tail = W as f32 - p1;
            c.fill_rect(p1, y, tail, rh, [shade_b, shade_b.saturating_sub(5), shade_b.saturating_sub(11), 255]);
            c.fill_rect(0.0, y, seg_w - tail, rh, [shade_b, shade_b.saturating_sub(5), shade_b.saturating_sub(11), 255]);
        }

        // Subtle longitudinal wood grain lines
        for _ in 0..22 {
            let gx = rng.next_f32() * W as f32;
            let gy = y + 2.0 + rng.next_f32() * (rh - 4.0);
            let gw = 16.0 + rng.next_f32() * 54.0;
            let alpha = (10.0 + rng.next_f32() * 18.0) as u8;
            let col = [130, 102, 74, alpha];
            if gx + gw <= W as f32 {
                c.fill_rect(gx, gy, gw, 1.0, col);
            } else {
                let first = W as f32 - gx;
                c.fill_rect(gx, gy, first, 1.0, col);
                c.fill_rect(0.0, gy, gw - first, 1.0, col);
            }
        }

        // Vertical plank end-seams and horizontal plank seam
        let seam = [96, 72, 48, 88];
        c.fill_rect(p0, y, 1.5, rh, seam);
        c.fill_rect(p1, y, 1.5, rh, seam);
        c.fill_rect(0.0, y, W as f32, 1.5, [92, 68, 44, 96]);
    }
    c.into_surface()
}

/// Tileable 128×128 pantry cream/warm-stone 4×4 checker tile (`pro/floor.js:37`, 64 KB VRAM).
pub fn draw_checker_tile() -> RgbaSurface {
    const S: u32 = 128;
    let mut c = Canvas2d::new(S, S);
    let mut rng = SeedRng(22307);
    c.fill(hex("#EFE7D8"));
    let cell = 32.0;
    let alt = hex("#D8C3A3");
    for i in 0..4 {
        for j in 0..4 {
            if (i + j) % 2 == 1 {
                c.fill_rect(i as f32 * cell, j as f32 * cell, cell, cell, alt);
            }
        }
    }
    for _ in 0..420 {
        let x = rng.next_f32() * S as f32;
        let y = rng.next_f32() * S as f32;
        let a = (4.0 + rng.next_f32() * 10.0) as u8;
        c.fill_rect(x, y, 2.0, 2.0, [0, 0, 0, a]);
    }
    let grout = [150, 130, 100, 90];
    for i in 0..4 {
        let p = i as f32 * cell;
        c.fill_rect(p, 0.0, 1.5, S as f32, grout);
        c.fill_rect(0.0, p, S as f32, 1.5, grout);
    }
    c.into_surface()
}

/// Tileable 128×128 pantry subway backsplash tiles (`pro/floor.js:55-58`, 64 KB VRAM).
pub fn draw_subway_tile() -> RgbaSurface {
    const S: u32 = 128;
    let mut c = Canvas2d::new(S, S);
    c.fill(hex("#D9D4CA"));
    let rows = 4usize;
    let rh = 32.0;
    let tw = 64.0;
    for r in 0..rows {
        let y = r as f32 * rh;
        let off = if r % 2 == 1 { 32.0 } else { 0.0 };
        let col = if r % 2 == 1 { hex("#FBFAF6") } else { hex("#F7F5F0") };
        for k in -1..=2 {
            let x = k as f32 * tw + off;
            c.fill_rect(x + 2.0, y + 2.0, tw - 4.0, rh - 4.0, col);
        }
    }
    c.into_surface()
}

/// Tileable 128×128 neutral technical tile grid (64 KB VRAM), tintable for server raised tiles,
/// corridor stone, lobby stone, restroom mosaic, pod carpet tiles, and terrace stone.
pub fn draw_server_grid_tile() -> RgbaSurface {
    const S: u32 = 128;
    let mut c = Canvas2d::new(S, S);
    let mut rng = SeedRng(99173);
    c.fill(hex("#F3F5F7"));
    let cell = 64.0;
    // Slight subtle checker value modulation for natural tile look
    for i in 0..2 {
        for j in 0..2 {
            if (i + j) % 2 == 1 {
                c.fill_rect(i as f32 * cell, j as f32 * cell, cell, cell, [235, 238, 242, 255]);
            }
        }
    }
    for _ in 0..480 {
        let x = rng.next_f32() * S as f32;
        let y = rng.next_f32() * S as f32;
        let a = (5.0 + rng.next_f32() * 12.0) as u8;
        c.fill_rect(x, y, 2.0, 2.0, [0, 0, 0, a]);
    }
    let seam = [90, 100, 115, 95];
    for i in 0..2 {
        let p = i as f32 * cell;
        c.fill_rect(p, 0.0, 2.0, S as f32, seam);
        c.fill_rect(0.0, p, S as f32, 2.0, seam);
    }
    c.into_surface()
}

/// Tileable 128×128 neutral speckled carpet/rug texture (`pro/pieces.js:36`, 64 KB VRAM),
/// tintable to any room carpet or rug color via `StandardMaterial::base_color`.
pub fn draw_speckle_rug_tile() -> RgbaSurface {
    const S: u32 = 128;
    let mut c = Canvas2d::new(S, S);
    let mut rng = SeedRng(51827);
    c.fill(hex("#F0F0F0"));
    for _ in 0..1400 {
        let x = rng.next_f32() * S as f32;
        let y = rng.next_f32() * S as f32;
        let col = if rng.next_f32() < 0.5 {
            [0, 0, 0, 20]
        } else {
            [255, 255, 255, 22]
        };
        c.fill_rect(x, y, 2.0, 2.0, col);
    }
    c.into_surface()
}

/// Tileable 128×128 vertical wainscoting beadboard slats (64 KB VRAM).
pub fn draw_wainscot_slats_tile() -> RgbaSurface {
    const S: u32 = 128;
    let mut c = Canvas2d::new(S, S);
    c.fill(hex("#F4F6F1"));
    let slats = 8usize;
    let sw = S as f32 / slats as f32;
    for i in 0..slats {
        let x = i as f32 * sw;
        c.fill_rect(x, 0.0, 2.0, S as f32, [140, 152, 128, 80]);
        c.fill_rect(x + 2.0, 0.0, 1.5, S as f32, [255, 255, 255, 90]);
    }
    c.into_surface()
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum WindowViewKind {
    City,
    Country,
    Beach,
}

/// Exact Oracle `src/office/world/windows.js` 1024×512 day/night window backdrop painting.
pub fn draw_window_painting(kind: WindowViewKind, night: bool) -> RgbaSurface {
    const W: u32 = 1024;
    const H: u32 = 512;
    let mut c = Canvas2d::new(W, H);
    let mut rng = SeedRng(if night { 9012 } else { 4517 } + kind as u64 * 131);
    let wf = W as f32;
    let hf = H as f32;

    // Sky gradient
    let (top_col, bot_col) = match (kind, night) {
        (_, true) => (hex("#0A1430"), hex("#2A3A66")),
        (WindowViewKind::City, false) => (hex("#8EC8F2"), hex("#DCEFFB")),
        (_, false) => (hex("#8EC8F2"), hex("#E4F2F8")),
    };
    c.linear_gradient_rect(0.0, 0.0, wf, hf, (0.0, 0.0), (0.0, hf), &[(0.0, top_col), (1.0, bot_col)]);

    // Moon + stars at night, fluffy clouds by day
    if night {
        c.fill_circle(wf * 0.78, hf * 0.2, 26.0, hex("#FFF3C4"));
        for _ in 0..85 {
            let sx = rng.next_f32() * wf;
            let sy = rng.next_f32() * hf * 0.48;
            let a = ((0.3 + rng.next_f32() * 0.55) * 255.0) as u8;
            c.fill_rect(sx, sy, 2.0, 2.0, [255, 255, 255, a]);
        }
    } else {
        let cloud = [255, 255, 255, 230];
        for (cx, cy, r) in [
            (200.0_f32, 90.0_f32, 40.0_f32),
            (250.0, 80.0, 50.0),
            (300.0, 95.0, 36.0),
            (700.0, 130.0, 34.0),
            (745.0, 120.0, 44.0),
            (790.0, 132.0, 30.0),
        ] {
            c.fill_circle(cx, cy, r, cloud);
        }
    }

    let draw_hill = |c: &mut Canvas2d, base: f32, amp: f32, col: [u8; 4], seed: f32| {
        let mut pts = Vec::with_capacity(132);
        pts.push((0.0, hf));
        let mut x = 0.0_f32;
        while x <= wf {
            let t = x / wf;
            let y = base
                - amp * (t * 9.0 + seed).sin().abs().powi(3)
                - amp * 0.25 * (t * 23.0 + seed).sin();
            pts.push((x, y));
            x += 8.0;
        }
        pts.push((wf, hf));
        c.fill_polygon(&pts, col);
    };

    match kind {
        WindowViewKind::City => {
            for layer in [0usize, 1] {
                let mut x = -20.0_f32;
                let bcol = match (night, layer) {
                    (true, 0) => hex("#1A2340"),
                    (true, _) => hex("#111827"),
                    (false, 0) => hex("#B2C2D2"),
                    (false, _) => hex("#8FA3B8"),
                };
                while x < wf {
                    let bw = 50.0 + rng.next_f32() * 70.0;
                    let (min_f, max_f) = if layer == 1 { (0.22, 0.52) } else { (0.42, 0.82) };
                    let bh = hf * (min_f + rng.next_f32() * (max_f - min_f));
                    c.fill_rect(x, hf - bh, bw, bh, bcol);
                    let mut wy = hf - bh + 12.0;
                    while wy < hf - 10.0 {
                        let mut wx = x + 8.0;
                        while wx < x + bw - 10.0 {
                            let lit = rng.next_f32() < if night { 0.42 } else { 0.12 };
                            let wcol = match (night, lit) {
                                (true, true) => hex("#FFD27A"),
                                (true, false) => [255, 255, 255, 14],
                                (false, true) => [255, 255, 255, 192],
                                (false, false) => [255, 255, 255, 52],
                            };
                            c.fill_rect(wx, wy, 7.0, 9.0, wcol);
                            wx += 14.0;
                        }
                        wy += 18.0;
                    }
                    x += bw + 4.0 + rng.next_f32() * 14.0;
                }
            }
        }
        WindowViewKind::Country => {
            draw_hill(
                &mut c,
                hf * 0.62,
                hf * 0.32,
                hex(if night { "#1E2C3C" } else { "#9DB6B0" }),
                1.0,
            );
            draw_hill(
                &mut c,
                hf * 0.70,
                hf * 0.22,
                hex(if night { "#1A2633" } else { "#7E9E86" }),
                4.0,
            );
            c.fill_rect(0.0, hf * 0.72, wf, hf * 0.28, hex(if night { "#1B2A22" } else { "#8DBA55" }));
            let ridge = hex(if night { "#22352A" } else { "#C9B45A" });
            for r in 0..4 {
                c.fill_rect(0.0, hf * (0.76 + r as f32 * 0.06), wf, 3.0, ridge);
            }
            for _ in 0..6 {
                let hx = 40.0 + rng.next_f32() * (wf - 80.0);
                let roof = hex(if night { "#2A2420" } else { "#B5533C" });
                c.fill_polygon(&[(hx - 26.0, hf * 0.73), (hx, hf * 0.68), (hx + 26.0, hf * 0.73)], roof);
                let wall = if night {
                    if rng.next_f32() < 0.7 { hex("#FFD27A") } else { hex("#1E1A16") }
                } else {
                    hex("#F1E3B5")
                };
                c.fill_rect(hx - 20.0, hf * 0.73, 40.0, 16.0, wall);
            }
        }
        WindowViewKind::Beach => {
            let sea = hex(if night { "#16284A" } else { "#3FA2C8" });
            c.fill_rect(0.0, hf * 0.62, wf, hf * 0.38, sea);
            draw_hill(
                &mut c,
                hf * 0.63,
                hf * 0.22,
                hex(if night { "#1B2638" } else { "#86A8A6" }),
                2.0,
            );
            c.fill_rect(0.0, hf * 0.64, wf, hf * 0.36, sea);
            let glint = if night { [255, 240, 200, 90] } else { [255, 255, 255, 128] };
            for _ in 0..40 {
                let gx = rng.next_f32() * wf;
                let gy = hf * 0.66 + rng.next_f32() * hf * 0.24;
                c.fill_rect(gx, gy, 10.0 + rng.next_f32() * 30.0, 2.0, glint);
            }
            c.fill_rect(0.0, hf * 0.90, wf, hf * 0.10, hex(if night { "#3A3426" } else { "#E8D3A2" }));
        }
    }

    c.into_surface()
}

/// Exact Oracle `windows.js:86-90` 64×128 warm linen roller blind fabric.
pub fn draw_blind_fabric() -> RgbaSurface {
    let mut c = Canvas2d::new(64, 128);
    c.fill(hex("#EDE3CC"));
    for y in (0..128).step_by(8) {
        c.fill_rect(0.0, y as f32, 64.0, 2.0, [120, 96, 60, 26]);
    }
    c.fill_rect(0.0, 122.0, 64.0, 6.0, hex("#B9A47E"));
    c.into_surface()
}

/// Exact Oracle `pro/pieces.js:96-103` & `pro/wing.js:23-35` 512×160 door / department plaque.
pub fn draw_room_plaque(
    title: &str,
    sub: Option<&str>,
    bg_hex: &str,
    fg_hex: &str,
    sub_hex: &str,
    accent_hex: Option<&str>,
    fonts: &mut FontEngine,
) -> RgbaSurface {
    let mut c = Canvas2d::new(512, 160);
    c.fill(hex(bg_hex));
    let cx = if let Some(acc) = accent_hex {
        c.fill_rect(0.0, 0.0, 14.0, 160.0, hex(acc));
        263.0
    } else {
        256.0
    };
    let sub = sub.filter(|s| !s.is_empty());
    let title_y = if sub.is_some() { 34.0 } else { 52.0 };
    fonts.draw_text(&mut c, title, cx, title_y, 50.0, hex(fg_hex), TextAlign::Center, Some(450.0), true, false);
    if let Some(sub_txt) = sub {
        fonts.draw_text(&mut c, sub_txt, cx, 108.0, 24.0, hex(sub_hex), TextAlign::Center, Some(450.0), false, false);
    }
    c.into_surface()
}

/// Exact Oracle `pro/floor.js:277-285` 256×128 WC pictogram sign.
pub fn draw_wc_sign() -> RgbaSurface {
    let mut c = Canvas2d::new(256, 128);
    c.fill(hex("#1E232B"));
    let fg = hex("#F3EFE8");
    // Male silhouette (x = 70)
    c.fill_circle(70.0, 30.0, 11.0, fg);
    c.fill_rect(57.0, 46.0, 26.0, 46.0, fg);
    c.fill_rect(60.0, 92.0, 7.0, 26.0, fg);
    c.fill_rect(73.0, 92.0, 7.0, 26.0, fg);
    // Center divider
    c.fill_rect(126.0, 18.0, 4.0, 92.0, fg);
    // Female silhouette (x = 186)
    c.fill_circle(186.0, 30.0, 11.0, fg);
    c.fill_polygon(&[(166.0, 92.0), (186.0, 46.0), (206.0, 92.0)], fg);
    c.fill_rect(176.0, 92.0, 7.0, 26.0, fg);
    c.fill_rect(189.0, 92.0, 7.0, 26.0, fg);
    c.into_surface()
}

/// Exact Oracle `pro/floor.js:199-203` 1024×220 glowing `"My Office"` reception/lobby sign.
pub fn draw_my_office_logo(fonts: &mut FontEngine) -> RgbaSurface {
    let mut c = Canvas2d::new(1024, 220);
    let bg = hex("#1C2620");
    let mark = hex("#E8865F");
    c.fill(bg);

    // OFFICE_MARK (24×24 coordinate space scaled 6.2× around (150, 110))
    let ox = 150.0 - 12.0 * 6.2;
    let oy = 110.0 - 12.0 * 6.2;
    let sc = 6.2_f32;
    c.round_rect(ox + 4.0 * sc, oy + 2.0 * sc, 16.0 * sc, 20.0 * sc, 3.0 * sc, mark);
    // Four window cutouts + center doorway cutout
    for (wx, wy) in [(7.5_f32, 6.0_f32), (13.5, 6.0), (7.5, 11.0), (13.5, 11.0)] {
        c.fill_rect(ox + wx * sc, oy + wy * sc, 3.0 * sc, 3.0 * sc, bg);
    }
    c.fill_rect(ox + 10.2 * sc, oy + 17.0 * sc, 3.6 * sc, 5.0 * sc, bg);

    fonts.draw_text(&mut c, "My Office", 260.0, 62.0, 92.0, hex("#F6EFE4"), TextAlign::Left, Some(720.0), true, false);
    c.into_surface()
}

/// Exact Oracle `pro/floor.js:244` 256×64 reception desk sign.
pub fn draw_reception_tag(label: &str, fonts: &mut FontEngine) -> RgbaSurface {
    let mut c = Canvas2d::new(256, 64);
    c.fill(hex("#1C2620"));
    fonts.draw_text(&mut c, label, 128.0, 16.0, 28.0, hex("#F6EFE4"), TextAlign::Center, Some(236.0), true, false);
    c.into_surface()
}

/// Exact Oracle `pro/floor.js:133-139` 640×360 meeting room Sprint Review slide.
pub fn draw_sprint_review_slide(fonts: &mut FontEngine) -> RgbaSurface {
    let mut c = Canvas2d::new(640, 360);
    c.fill(hex("#F7F5F0"));
    c.fill_rect(0.0, 0.0, 14.0, 360.0, hex("#D97757"));
    fonts.draw_text(&mut c, "Sprint review", 44.0, 30.0, 40.0, hex("#1E232B"), TextAlign::Left, None, true, false);
    fonts.draw_text(&mut c, "This week's progress", 44.0, 80.0, 22.0, hex("#7A8494"), TextAlign::Left, None, false, false);
    for (i, &v) in [0.42_f32, 0.58, 0.51, 0.74, 0.66, 0.88].iter().enumerate() {
        let bh = v * 180.0;
        let col = if i == 5 { hex("#D97757") } else { hex("#3F7CC4") };
        c.fill_rect(60.0 + i as f32 * 88.0, 320.0 - bh, 52.0, bh, col);
    }
    c.fill_rect(44.0, 322.0, 552.0, 3.0, hex("#C9CED6"));
    c.into_surface()
}

/// Exact Oracle `pro/wing.js:45-61` 512×320 HQ department 3-column Kanban sprint board.
pub fn draw_sprint_board(accent_hex: &str, seed: u64, fonts: &mut FontEngine) -> RgbaSurface {
    const W: u32 = 512;
    const H: u32 = 320;
    let mut c = Canvas2d::new(W, H);
    let mut rng = SeedRng(73 + seed * 31);
    c.fill(hex("#FBFBF8"));
    fonts.draw_text(&mut c, "Sprint board", 18.0, 12.0, 28.0, hex("#1E232B"), TextAlign::Left, None, true, false);
    c.fill_rect(18.0, 50.0, 120.0, 5.0, hex(accent_hex));

    let cols = ["To do", "Doing", "Done"];
    let counts = [4usize, 3, 3];
    let stickies = [hex("#FFE27A"), hex("#FFB4A2"), hex("#A8E6CF"), hex("#BDE0FE")];
    let col_w = (W as f32 - 36.0) / 3.0;
    for (k, &col_name) in cols.iter().enumerate() {
        let cx = 18.0 + k as f32 * col_w;
        fonts.draw_text(&mut c, col_name, cx + 6.0, 68.0, 18.0, hex("#7A8494"), TextAlign::Left, None, true, false);
        if k < 2 {
            c.fill_rect(cx + col_w - 3.0, 68.0, 2.0, H as f32 - 84.0, [30, 35, 43, 30]);
        }
        for n in 0..counts[k] {
            let px = cx + 8.0 + (n % 2) as f32 * (col_w * 0.5 - 4.0);
            let py = 100.0 + (n / 2) as f32 * 64.0;
            if py + 52.0 > H as f32 - 8.0 {
                break;
            }
            let sw = col_w * 0.5 - 14.0;
            c.fill_rect(px, py, sw, 52.0, stickies[(n + k + seed as usize) % stickies.len()]);
            for line in 0..3 {
                let lw = (sw - 12.0) * (0.5 + rng.next_f32() * 0.5);
                c.fill_rect(px + 6.0, py + 10.0 + line as f32 * 12.0, lw, 4.0, [30, 35, 43, 90]);
            }
        }
    }
    c.into_surface()
}

/// Server room precision AC cooler live CPU / room temperature display (`pro/floor.js:192`).
pub fn draw_cooler_screen(cpu_pct: u8, temp_c: f32, fonts: &mut FontEngine) -> RgbaSurface {
    let mut c = Canvas2d::new(256, 150);
    c.fill(hex("#0A111A"));
    c.fill_rect(0.0, 0.0, 256.0, 26.0, hex("#132032"));
    fonts.draw_text(&mut c, "PRECISION AC · RACK", 12.0, 5.0, 13.0, hex("#6AAEFF"), TextAlign::Left, None, true, true);
    let temp_str = format!("{temp_c:.1}°C");
    fonts.draw_text(&mut c, &temp_str, 14.0, 36.0, 44.0, hex("#3DDC84"), TextAlign::Left, None, true, true);
    let cpu = cpu_pct.min(100);
    fonts.draw_text(&mut c, &format!("HOST CPU {cpu}%"), 14.0, 96.0, 15.0, hex("#B8C5D6"), TextAlign::Left, None, true, true);
    c.fill_rect(14.0, 122.0, 228.0, 12.0, hex("#1F2B3D"));
    let bar_col = if cpu > 80 { hex("#FF6B4A") } else { hex("#3DDC84") };
    c.fill_rect(14.0, 122.0, 2.28 * cpu as f32, 12.0, bar_col);
    c.into_surface()
}

/// Exact Oracle `pro/floor.js:326-331` lobby `"SHIP SMALL, SHIP OFTEN"` framed poster.
pub fn draw_poster_ship_often(fonts: &mut FontEngine) -> RgbaSurface {
    let mut c = Canvas2d::new(256, 349);
    c.fill(hex("#F6EFE4"));
    c.fill_rect(0.0, 0.0, 256.0, 10.0, hex("#D97757"));
    for (i, line) in ["SHIP", "SMALL,", "SHIP", "OFTEN"].iter().enumerate() {
        fonts.draw_text(&mut c, line, 128.0, 64.0 + i as f32 * 52.0, 32.0, hex("#1E232B"), TextAlign::Center, None, true, false);
    }
    fonts.draw_text(&mut c, "My Office", 128.0, 314.0, 16.0, hex("#7A8494"), TextAlign::Center, None, false, false);
    c.into_surface()
}

/// Exact Oracle `pro/floor.js:332-337` pantry `"Team outing 2026"` framed poster.
pub fn draw_poster_team_outing(fonts: &mut FontEngine) -> RgbaSurface {
    let mut c = Canvas2d::new(256, 186);
    c.linear_gradient_rect(0.0, 0.0, 256.0, 186.0, (0.0, 0.0), (0.0, 186.0), &[(0.0, hex("#9CC9E8")), (1.0, hex("#F3E3C2"))]);
    c.fill_rect(0.0, 130.0, 256.0, 56.0, hex("#6A8042"));
    let shirts = ["#6A8042", "#E8742A", "#3F7CC4", "#8C4FA8", "#D64550", "#2A9D8F", "#C9A227"];
    for (i, &col) in shirts.iter().enumerate() {
        let x = 22.0 + i as f32 * 34.0;
        c.fill_rect(x - 9.0, 96.0, 18.0, 30.0, hex(col));
        c.fill_circle(x, 86.0, 10.0, hex("#F2D3B3"));
    }
    fonts.draw_text(&mut c, "Team outing 2026", 12.0, 8.0, 14.0, hex("#FFFFFF"), TextAlign::Left, None, true, false);
    c.into_surface()
}

/// Exact Oracle `entrance.js:168-174` 192×72 elevator LED floor indicator (`"▼ 12"`).
pub fn draw_lift_display(text: &str, fonts: &mut FontEngine) -> RgbaSurface {
    let mut c = Canvas2d::new(192, 72);
    c.fill(hex("#0B0D10"));
    fonts.draw_text(&mut c, text, 96.0, 14.0, 42.0, hex("#FFB23F"), TextAlign::Center, Some(180.0), true, true);
    c.into_surface()
}

/// Exact Oracle `entrance.js:135-141` 512×96 lit green stairwell `EMERGENCY EXIT / STAIRS` sign.
pub fn draw_exit_sign(fonts: &mut FontEngine) -> RgbaSurface {
    let mut c = Canvas2d::new(512, 96);
    let green = hex("#0E8A4A");
    let white = hex("#FFFFFF");
    c.fill(green);
    c.fill_rect(10.0, 10.0, 76.0, 76.0, white);
    c.fill_circle(54.0, 24.0, 8.0, green);
    for ((x0, y0), (x1, y1)) in [
        ((50.0_f32, 34.0_f32), (42.0_f32, 56.0_f32)),
        ((42.0, 56.0), (56.0, 70.0)),
        ((56.0, 70.0), (52.0, 84.0)),
        ((42.0, 56.0), (30.0, 66.0)),
        ((48.0, 40.0), (62.0, 48.0)),
        ((48.0, 40.0), (32.0, 44.0)),
    ] {
        c.stroke_line(x0, y0, x1, y1, 8.0, green);
    }
    fonts.draw_text(&mut c, "EMERGENCY EXIT", 294.0, 14.0, 32.0, white, TextAlign::Center, Some(390.0), true, false);
    fonts.draw_text(&mut c, "STAIRS", 294.0, 56.0, 22.0, white, TextAlign::Center, Some(390.0), true, false);
    c.into_surface()
}

/// Exact Oracle `entrance.js:142-143` 256×160 stairwell landing floor number painted on back wall.
pub fn draw_stair_floor_sign(floor: u8, fonts: &mut FontEngine) -> RgbaSurface {
    let mut c = Canvas2d::new(256, 160);
    c.fill(hex("#E6E3DA"));
    fonts.draw_text(&mut c, &floor.to_string(), 128.0, 14.0, 88.0, hex("#2F5A3E"), TextAlign::Center, None, true, false);
    fonts.draw_text(&mut c, "FLOOR", 128.0, 118.0, 26.0, hex("#2F5A3E"), TextAlign::Center, None, true, false);
    c.into_surface()
}

/// Exact Oracle `entrance.js:144-145` 256×160 stairwell fire-door plate (`"STAIRS / Floor 12"`).
pub fn draw_stair_door_plate(floor: u8, fonts: &mut FontEngine) -> RgbaSurface {
    let mut c = Canvas2d::new(256, 160);
    c.fill(hex("#1F2328"));
    fonts.draw_text(&mut c, "STAIRS", 128.0, 36.0, 34.0, hex("#F3EFE8"), TextAlign::Center, Some(236.0), true, false);
    fonts.draw_text(&mut c, &format!("Floor {floor}"), 128.0, 94.0, 22.0, hex("#F3EFE8"), TextAlign::Center, Some(236.0), false, false);
    c.into_surface()
}

/// Exact Oracle `trinkets.js:55` 64×48 desk/credenza mini framed landscape photo.
pub fn draw_framed_photo() -> RgbaSurface {
    let mut c = Canvas2d::new(64, 48);
    c.linear_gradient_rect(0.0, 0.0, 64.0, 48.0, (0.0, 0.0), (0.0, 48.0), &[(0.0, hex("#8EC8F2")), (1.0, hex("#E4F2F8"))]);
    c.fill_circle(48.0, 12.0, 6.0, hex("#FFD166"));
    c.fill_polygon(&[(0.0, 48.0), (18.0, 24.0), (34.0, 40.0), (46.0, 28.0), (64.0, 48.0)], hex("#6A9A4C"));
    c.into_surface()
}

/// Exact Oracle `neon.js:14` 512×128 glowing `"SHIP IT ✦"` neon wall sign.
pub fn draw_neon_ship_it(fonts: &mut FontEngine) -> RgbaSurface {
    let mut c = Canvas2d::new(512, 128);
    c.fill(hex("#1C1F26"));
    // Soft pink neon back-glow behind "SHIP IT"
    for &(rx, ry, alpha) in &[(188.0, 46.0, 38u8), (156.0, 34.0, 56), (124.0, 24.0, 72)] {
        c.fill_ellipse(222.0, 64.0, rx, ry, hex_rgba("#FF4FA3", alpha));
    }
    // Soft cyan back-glow behind 4-point star
    for &(r, alpha) in &[(38.0, 44u8), (26.0, 74), (16.0, 105)] {
        c.fill_circle(438.0, 64.0, r, hex_rgba("#4CC9F0", alpha));
    }
    // Outer pink stroke halo + crisp bright neon core text
    for &(dx, dy) in &[(-2.5, 0.0), (2.5, 0.0), (0.0, -2.5), (0.0, 2.5)] {
        fonts.draw_text(
            &mut c,
            "SHIP IT",
            222.0 + dx,
            24.0 + dy,
            74.0,
            hex_rgba("#FF7DBE", 210),
            TextAlign::Center,
            Some(360.0),
            true,
            false,
        );
    }
    fonts.draw_text(
        &mut c,
        "SHIP IT",
        222.0,
        24.0,
        74.0,
        hex("#FFE3F1"),
        TextAlign::Center,
        Some(360.0),
        true,
        false,
    );
    // Crisp vector 4-point star `✦` at (438, 64)
    let star = |cx: f32, cy: f32, r_out: f32, r_in: f32| {
        [
            (cx, cy - r_out),
            (cx + r_in, cy - r_in),
            (cx + r_out, cy),
            (cx + r_in, cy + r_in),
            (cx, cy + r_out),
            (cx - r_in, cy + r_in),
            (cx - r_out, cy),
            (cx - r_in, cy - r_in),
        ]
    };
    c.fill_polygon(&star(438.0, 64.0, 26.0, 8.0), hex_rgba("#4CC9F0", 180));
    c.fill_polygon(&star(438.0, 64.0, 21.0, 6.0), hex("#BFF0FF"));
    c.into_surface()
}

/// Exact Oracle `room-pieces.js:68-75` 256×320 Cozy wall sun poster (`BAND 8.0`).
pub fn draw_cozy_sun_poster(fonts: &mut FontEngine) -> RgbaSurface {
    let mut c = Canvas2d::new(256, 320);
    c.fill(hex("#FFF5DF"));
    c.fill_circle(128.0, 135.0, 60.0, hex("#E8730E"));
    for i in 0..12 {
        let a = (i as f32 / 12.0) * std::f32::consts::TAU;
        c.stroke_line(
            128.0 + a.cos() * 80.0,
            135.0 + a.sin() * 80.0,
            128.0 + a.cos() * 102.0,
            135.0 + a.sin() * 102.0,
            10.0,
            hex("#E8730E"),
        );
    }
    c.fill_rect(0.0, 248.0, 256.0, 72.0, hex("#5F7A35"));
    fonts.draw_text(
        &mut c,
        "BAND 8.0",
        128.0,
        266.0,
        34.0,
        hex("#FFFADD"),
        TextAlign::Center,
        Some(232.0),
        true,
        false,
    );
    c.into_surface()
}

/// Exact Oracle `room-pieces.js:100` 256×128 stylized earth globe map.
pub fn draw_globe_map() -> RgbaSurface {
    let mut c = Canvas2d::new(256, 128);
    c.fill(hex("#4C9BE0"));
    let mut rng = SeedRng(42);
    for i in 0..16 {
        let col = if i % 4 != 0 { hex("#79BB57") } else { hex("#E5C783") };
        let cx = rng.next_f32() * 256.0;
        let cy = 20.0 + rng.next_f32() * 88.0;
        let rx = 10.0 + rng.next_f32() * 22.0;
        let ry = 6.0 + rng.next_f32() * 12.0;
        c.fill_ellipse(cx, cy, rx, ry, col);
    }
    c.into_surface()
}

/// Exact Oracle `bedding.js:53-60` 128×128 knitted fleece plaid blanket texture (white base for tinting).
pub fn draw_blanket_plaid() -> RgbaSurface {
    let mut c = Canvas2d::new(128, 128);
    c.fill(hex("#FFFFFF"));
    for i in (0..128).step_by(22) {
        let f = i as f32;
        c.fill_rect(f + 5.0, 0.0, 8.0, 128.0, [0, 0, 0, 23]);
        c.fill_rect(0.0, f + 5.0, 128.0, 8.0, [0, 0, 0, 23]);
        c.fill_rect(f + 16.0, 0.0, 1.5, 128.0, [255, 255, 255, 140]);
        c.fill_rect(0.0, f + 16.0, 128.0, 1.5, [255, 255, 255, 140]);
    }
    c.stroke_rect(3.0, 3.0, 122.0, 122.0, 6.0, [0, 0, 0, 50]);
    c.stroke_rect(8.0, 8.0, 112.0, 112.0, 1.5, [255, 255, 255, 128]);
    c.into_surface()
}

/// Exact Oracle `billiards.js:18-34` numbered/striped billiards ball skin (`64×32`, 8 KB each).
pub fn draw_billiards_ball(n: u8, fonts: &mut FontEngine) -> RgbaSurface {
    const COLORS: [&str; 9] = [
        "#F7F4EA", "#F2C230", "#2350A8", "#D2302A", "#5B2D8E", "#E9762B", "#1F7A45", "#7A1F2B",
        "#15161A",
    ];
    let n = n.min(15);
    let stripe = n > 8;
    let c_hex = COLORS[if stripe { (n - 8) as usize } else { n as usize }];
    let mut c = Canvas2d::new(64, 32);
    c.fill(hex(if stripe { "#F7F4EA" } else { c_hex }));
    if stripe {
        c.fill_rect(0.0, 8.5, 64.0, 15.0, hex(c_hex));
    }
    if n == 0 {
        for x in [16.0_f32, 48.0] {
            c.fill_circle(x, 16.0, 2.0, hex("#C8302A"));
        }
        return c.into_surface();
    }
    let label = n.to_string();
    for x in [16.0_f32, 48.0] {
        c.fill_ellipse(x, 16.0, 6.0, 7.0, hex("#FBFAF4"));
        let sz = if n > 9 { 8.0 } else { 9.5 };
        fonts.draw_text(
            &mut c,
            &label,
            x,
            16.0 - sz * 0.55,
            sz,
            hex("#15161A"),
            TextAlign::Center,
            None,
            true,
            false,
        );
    }
    c.into_surface()
}

/// Exact Oracle `billiards.js:36-40` green felt cloth (`128×256`) with fine grain + head/foot spots.
pub fn draw_billiards_felt() -> RgbaSurface {
    let mut c = Canvas2d::new(128, 256);
    c.fill(hex("#2C7A4E"));
    let mut s = 77_313u32;
    for _ in 0..1400 {
        s = s.wrapping_mul(1664525).wrapping_add(1013904223);
        let x = ((s >> 16) % 128) as f32;
        s = s.wrapping_mul(1664525).wrapping_add(1013904223);
        let y = ((s >> 16) % 256) as f32;
        let light = (s & 1) == 0;
        let col = if light { [255, 255, 255, 11] } else { [0, 0, 0, 11] };
        c.fill_rect(x, y, 2.0, 2.0, col);
    }
    for y in [64.0_f32, 192.0] {
        c.fill_circle(64.0, y, 2.0, [255, 255, 255, 190]);
    }
    c.into_surface()
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    fn fonts() -> FontEngine {
        FontEngine::from_assets_dir(&PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../assets"))
    }

    #[test]
    fn all_eight_desk_screens_and_interior_surfaces_render_deterministically() {
        let mut f = fonts();
        for kind in ALL_SCREEN_KINDS {
            let a = draw_desk_screen(kind, &mut f);
            let b = draw_desk_screen(kind, &mut f);
            assert_eq!((a.width, a.height), (512, 640));
            assert_eq!(a.rgba, b.rgba, "screen {kind:?} must be deterministic");
        }

        let tv = draw_stats_tv(
            &TvStatsInput {
                title: "AGENTS STATION".to_owned(),
                clock_hhmm: "15:45".to_owned(),
                ram_pct: Some(64),
                ram_critical: false,
                cpu_pct: 120, // extreme >100% clamp check
                working: 3,
                waiting: 1,
                resting: 2,
                today_summary: "42 prompts · 18 cmds".to_owned(),
                plan_pct: Some(82),
                pulse_30m: [2; 30],
            },
            &mut f,
        );
        assert_eq!((tv.width, tv.height), (512, 280));
        assert!(tv.non_background_ratio(hex("#0A0F18")) > 0.1);

        assert_eq!(draw_arcade_frame(1.5, false, &mut f).width, 96);
        assert_eq!(draw_arcade_frame(1.5, true, &mut f).height, 72);
        assert_eq!(draw_arcade_marquee(&mut f).width, 256);
        assert_eq!(draw_karaoke_band("KARAOKE", &mut f).width, 512);
        assert_eq!(draw_karaoke_qr_badge(&mut f).width, 256);
        assert_eq!(draw_karaoke_screen("Sóng Gió", "Hồng trần trên đôi cánh tay", 0.6, &mut f).height, 288);
        assert_eq!(draw_game_tv_standby(&mut f).width, 384);
        assert_eq!(draw_radial_glow().width, 128);
        assert_eq!(draw_dev_server_badge(Some(4310), &mut f).width, 96);
        assert_eq!(draw_microwave_timer(95, &mut f).width, 128);
    }

    #[test]
    fn tileable_floor_wall_rug_surfaces_fit_under_1mb_and_signs_render_cleanly() {
        let mut f = fonts();
        let plank = draw_plank_tile();
        let checker = draw_checker_tile();
        let subway = draw_subway_tile();
        let server = draw_server_grid_tile();
        let rug = draw_speckle_rug_tile();
        let wain = draw_wainscot_slats_tile();

        assert_eq!((plank.width, plank.height), (256, 256));
        assert_eq!((checker.width, checker.height), (128, 128));
        assert_eq!((subway.width, subway.height), (128, 128));
        assert_eq!((server.width, server.height), (128, 128));
        assert_eq!((rug.width, rug.height), (128, 128));
        assert_eq!((wain.width, wain.height), (128, 128));

        let total_tile_bytes = plank.rgba.len()
            + checker.rgba.len()
            + subway.rgba.len()
            + server.rgba.len()
            + rug.rgba.len()
            + wain.rgba.len();
        assert!(
            total_tile_bytes < 1_048_576,
            "combined repeating floor/wall/rug tiles must stay < 1 MB, got {total_tile_bytes}"
        );

        for kind in [WindowViewKind::City, WindowViewKind::Country, WindowViewKind::Beach] {
            let day = draw_window_painting(kind, false);
            let night = draw_window_painting(kind, true);
            assert_eq!((day.width, day.height), (1024, 512));
            assert_eq!((night.width, night.height), (1024, 512));
            assert_ne!(day.rgba, night.rgba);
        }

        assert_eq!((draw_blind_fabric().width, draw_blind_fabric().height), (64, 128));
        assert_eq!(draw_room_plaque("DIRECTOR", None, "#1E232B", "#F3EFE8", "#A9B2C0", None, &mut f).width, 512);
        assert_eq!(draw_wc_sign().width, 256);
        assert_eq!(draw_my_office_logo(&mut f).width, 1024);
        assert_eq!(draw_reception_tag("RECEPTION", &mut f).width, 256);
        assert_eq!(draw_sprint_review_slide(&mut f).width, 640);
        assert_eq!(draw_sprint_board("#3F7CC4", 1, &mut f).width, 512);
        assert_eq!(draw_cooler_screen(24, 21.5, &mut f).width, 256);
        assert_eq!(draw_poster_ship_often(&mut f).height, 349);
        assert_eq!(draw_poster_team_outing(&mut f).height, 186);
        assert_eq!(draw_lift_display("▼ 12", &mut f).width, 192);
        assert_eq!(draw_exit_sign(&mut f).width, 512);
        assert_eq!(draw_stair_floor_sign(12, &mut f).width, 256);
        assert_eq!(draw_stair_door_plate(12, &mut f).width, 256);
        assert_eq!(draw_framed_photo().width, 64);
        assert_eq!((draw_neon_ship_it(&mut f).width, draw_neon_ship_it(&mut f).height), (512, 128));
        assert_eq!((draw_cozy_sun_poster(&mut f).width, draw_cozy_sun_poster(&mut f).height), (256, 320));
        assert_eq!((draw_globe_map().width, draw_globe_map().height), (256, 128));
        assert_eq!((draw_blanket_plaid().width, draw_blanket_plaid().height), (128, 128));
        assert_eq!((draw_billiards_ball(0, &mut f).width, draw_billiards_ball(15, &mut f).height), (64, 32));
        assert_eq!((draw_billiards_felt().width, draw_billiards_felt().height), (128, 256));
    }
}
