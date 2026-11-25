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
}
