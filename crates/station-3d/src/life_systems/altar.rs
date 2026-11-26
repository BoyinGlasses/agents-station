//! Detailed Vietnamese *Bàn thờ Thần Tài – Thổ Địa* lobby shrine (`altar.js`).
//!
//! Includes upturned 2-tier roof, vertical couplets rendered with `Canvas2d` + `FontEngine`
//! (`RENDER_WORLD`), Ông Địa & Thần Tài figures, porcelain incense bowl, red altar lamps,
//! 3 water cups, golden toad, daily rose vases vs Mùng 1 / Rằm yellow chrysanthemums,
//! 5-fruit offering tray + xôi gấc + chè, and 3 burning incense sticks with animated smoke wisps.

use crate::layout::LayoutSpec;
use crate::lights::LightingState;
use crate::palette::{
    WorldEntity, WorldPalette, spawn_box, spawn_cone_rot, spawn_cyl, spawn_cyl_rot, spawn_quad,
    spawn_sphere,
};
use bevy::prelude::*;
use station_canvas2d::{Canvas2d, FontEngine, RgbaSurface, TextAlign, hex, hex_rgba};
use std::f32::consts::{FRAC_PI_2, FRAC_PI_4, PI};

pub const ALTAR_SMOKE_COUNT: usize = 7;
pub const ALTAR_BURN_SECS: f32 = 50.0 * 60.0;
pub const ALTAR_ORIGIN: Vec3 = Vec3::new(-6.85, 0.0, 0.84);
pub const ALTAR_PRAY_SPOT: Vec3 = Vec3::new(-6.85, 0.0, 1.74);

#[derive(Resource, Debug, Clone)]
pub struct AltarState {
    pub force_lunar: bool,
    pub offering_active: bool,
    pub incense_lit: bool,
    pub burn_elapsed_secs: f32,
    pub burn_duration_secs: f32,
    pub date_ymd: (i32, u32, u32),
}

impl Default for AltarState {
    fn default() -> Self {
        use chrono::Datelike;
        let today = chrono::Local::now().date_naive();
        Self {
            force_lunar: false,
            offering_active: false,
            incense_lit: false,
            burn_elapsed_secs: 0.0,
            burn_duration_secs: ALTAR_BURN_SECS,
            date_ymd: (today.year(), today.month(), today.day()),
        }
    }
}

impl AltarState {
    pub fn arrange_offerings(&mut self) {
        self.offering_active = true;
    }

    pub fn light_incense(&mut self) {
        self.offering_active = true;
        self.incense_lit = true;
        self.burn_elapsed_secs = 0.0;
    }
}

#[derive(Component, Debug, Clone, Copy)]
pub struct AltarOfferPart;

#[derive(Component, Debug, Clone, Copy)]
pub struct AltarDailyFlowers;

#[derive(Component, Debug, Clone, Copy)]
pub struct AltarIncenseStick {
    pub base_y: f32,
    pub full_height: f32,
}

#[derive(Component, Debug, Clone, Copy)]
pub struct AltarSmokeWisp {
    pub phase: f32,
    pub seed: f32,
}

#[derive(Component, Debug, Clone)]
pub struct AltarLamp {
    pub mat: Handle<StandardMaterial>,
}

/// Julian Day Number from proleptic Gregorian `(year, month, day)`.
fn jdn_from_ymd(year: i32, month: u32, day: u32) -> i64 {
    let a = (14 - month as i64) / 12;
    let y = year as i64 + 4800 - a;
    let m = month as i64 + 12 * a - 3;
    day as i64 + (153 * m + 2) / 5 + 365 * y + y / 4 - y / 100 + y / 400 - 32045
}

/// Computes the exact Julian Day Number (at UTC+7, Asia/Ho_Chi_Minh) of the `k`-th new moon
/// since J2000 (Ho Ngoc Duc / Jean Meeus algorithm used across Vietnamese lunar calendars).
fn new_moon_day_vn(k: i64) -> i64 {
    let kf = k as f64;
    let t = kf / 1236.85;
    let t2 = t * t;
    let t3 = t2 * t;
    let dr = std::f64::consts::PI / 180.0;
    let mut jd1 = 2415020.75933 + 29.53058868 * kf + 0.0001178 * t2 - 0.000000155 * t3;
    jd1 += 0.00033 * ((166.56 + 132.87 * t - 0.009173 * t2) * dr).sin();
    let m = 359.2242 + 29.10535608 * kf - 0.0000333 * t2 - 0.00000347 * t3;
    let mpr = 306.0253 + 385.81691806 * kf + 0.0107306 * t2 + 0.00001236 * t3;
    let f = 21.2964 + 390.67050646 * kf - 0.0016528 * t2 - 0.00000239 * t3;
    let mut c1 = (0.1734 - 0.000393 * t) * (m * dr).sin() + 0.0021 * (2.0 * m * dr).sin();
    c1 -= 0.4068 * (mpr * dr).sin() + 0.0161 * (2.0 * mpr * dr).sin();
    c1 -= 0.0004 * (3.0 * mpr * dr).sin();
    c1 += 0.0104 * (2.0 * f * dr).sin() - 0.0051 * ((m + mpr) * dr).sin();
    c1 -= 0.0074 * ((m - mpr) * dr).sin() + 0.0004 * ((2.0 * f + m) * dr).sin();
    c1 -= 0.0004 * ((2.0 * f - m) * dr).sin() - 0.0006 * ((2.0 * f + mpr) * dr).sin();
    c1 += 0.0010 * ((2.0 * f - mpr) * dr).sin() + 0.0005 * ((2.0 * mpr + m) * dr).sin();
    let delta_t = if t < -11.0 {
        0.001 + 0.000839 * t + 0.0002261 * t2 - 0.00000845 * t3 - 0.000000081 * t * t3
    } else {
        -0.000278 + 0.000265 * t + 0.000262 * t2
    };
    let jd_new = jd1 + c1 - delta_t;
    (jd_new + 0.5 + 7.0 / 24.0).floor() as i64
}

/// Returns the Vietnamese lunar day of the month (`1..=30`, UTC+7) for a Gregorian date.
pub fn vietnamese_lunar_day(year: i32, month: u32, day: u32) -> u8 {
    let day_num = jdn_from_ymd(year, month, day);
    let k = ((day_num as f64 - 2415021.076998695) / 29.530588853).floor() as i64;
    let mut month_start = new_moon_day_vn(k + 1);
    if month_start > day_num {
        month_start = new_moon_day_vn(k);
    }
    ((day_num - month_start + 1).clamp(1, 30)) as u8
}

/// True on Mùng 1 (1st) or Rằm (15th) of the Vietnamese lunar month (`altar.js:27`).
pub fn is_offering_day(year: i32, month: u32, day: u32) -> bool {
    matches!(vietnamese_lunar_day(year, month, day), 1 | 15)
}

fn draw_altar_back_panel(fonts: &mut FontEngine) -> RgbaSurface {
    let mut c = Canvas2d::new(256, 256);
    c.fill(hex("#A51E1E"));
    c.stroke_rect(8.0, 8.0, 240.0, 240.0, 6.0, hex("#D9A93A"));
    c.stroke_rect(20.0, 20.0, 216.0, 216.0, 2.0, hex("#E8BE5A"));

    // 24-ray golden sunburst behind Ông Địa & Thần Tài
    let cx = 128.0_f32;
    let cy = 158.0_f32;
    let ray_col = hex_rgba("#E8BE5A", 92);
    for i in 0..24 {
        let ang = (i as f32) * (PI / 12.0);
        let x1 = cx + ang.cos() * 98.0;
        let y1 = cy + ang.sin() * 98.0;
        c.stroke_line(cx, cy, x1, y1, 3.0, ray_col);
    }
    c.fill_circle(cx, cy, 24.0, hex_rgba("#F2CB62", 110));

    fonts.draw_text(
        &mut c,
        "THẦN TÀI · THỔ ĐỊA",
        128.0,
        42.0,
        22.0,
        hex("#F2CB62"),
        TextAlign::Center,
        Some(220.0),
        true,
        false,
    );
    c.into_surface()
}

fn draw_altar_couplet(words: &[&str], fonts: &mut FontEngine) -> RgbaSurface {
    let mut c = Canvas2d::new(64, 384);
    c.fill(hex("#7E1717"));
    c.stroke_rect(3.0, 3.0, 58.0, 378.0, 2.0, hex("#D9A93A"));
    let count = words.len().max(1);
    let step = if count > 1 {
        312.0 / (count - 1) as f32
    } else {
        0.0
    };
    for (i, word) in words.iter().enumerate() {
        let y = 36.0 + i as f32 * step;
        fonts.draw_text(
            &mut c,
            word,
            32.0,
            y,
            16.0,
            hex("#F2CB62"),
            TextAlign::Center,
            Some(54.0),
            true,
            false,
        );
    }
    c.into_surface()
}

/// Spawns the detailed *Bàn thờ Thần Tài – Thổ Địa* in the Pro/HQ lobby corner (`altar.js`).
/// Cozy skips the lobby shrine (matching `altar.js:172` `if (PRO) build()`).
pub fn spawn_than_tai_altar(
    commands: &mut Commands,
    pal: &mut WorldPalette,
    materials: &mut Assets<StandardMaterial>,
    images: &mut Assets<Image>,
    fonts: &mut FontEngine,
    spec: &LayoutSpec,
) {
    if !spec.layout.is_pro() {
        return;
    }

    let o = ALTAR_ORIGIN;
    let red = pal.mat(materials, "#9C1F1F", 0.35, 0.05);
    let red_dark = pal.mat(materials, "#6E1414", 0.45, 0.05);
    let gold = pal.mat(materials, "#D9A93A", 0.3, 0.6);
    let wood = pal.mat(materials, "#4A2A1A", 0.5, 0.0);
    let skin = pal.mat(materials, "#F1C9A0", 0.55, 0.0);
    let porcelain = pal.mat(materials, "#F4F1EA", 0.25, 0.05);
    let blue_band = pal.mat(materials, "#2F5DA8", 0.3, 0.05);
    let ash = pal.matte(materials, "#D9C9A8");
    let stub = pal.matte(materials, "#C0392B");
    let black = pal.matte(materials, "#1E1E22");
    let orange_robe = pal.mat(materials, "#D9822B", 0.5, 0.0);
    let crimson_robe = pal.mat(materials, "#B4232A", 0.45, 0.05);
    let jade_belt = pal.mat(materials, "#2E7D4F", 0.5, 0.05);
    let fan_gold = pal.mat(materials, "#E8C15A", 0.4, 0.25);

    // 1. Base & red lacquered cabinet
    spawn_box(commands, pal, wood, o + Vec3::new(0.0, 0.05, 0.0), Vec3::new(0.72, 0.10, 0.54));
    spawn_box(commands, pal, red.clone(), o + Vec3::new(0.0, 0.12, 0.0), Vec3::new(0.62, 0.04, 0.46));
    spawn_box(commands, pal, gold.clone(), o + Vec3::new(0.0, 0.142, 0.225), Vec3::new(0.64, 0.015, 0.02));
    spawn_box(commands, pal, red.clone(), o + Vec3::new(0.0, 0.45, -0.215), Vec3::new(0.62, 0.62, 0.03));
    for sx in [-1.0_f32, 1.0] {
        spawn_box(commands, pal, red.clone(), o + Vec3::new(sx * 0.295, 0.45, 0.0), Vec3::new(0.03, 0.62, 0.46));
    }

    // 2. Back panel with sunburst & Vietnamese header (`RENDER_WORLD` tracked in `WorldPalette`)
    let back_mat = pal.textured_lit(images, materials, draw_altar_back_panel(fonts), 0.55);
    spawn_quad(commands, pal, back_mat, o + Vec3::new(0.0, 0.45, -0.197), Vec2::new(0.54, 0.54), Quat::IDENTITY);

    // 3. Front pillars + vertical couplets (`Thổ năng sinh bạch ngọc / Địa khả xuất hoàng kim`)
    let left_words = ["THỔ", "NĂNG", "SINH", "BẠCH", "NGỌC"];
    let right_words = ["ĐỊA", "KHẢ", "XUẤT", "HOÀNG", "KIM"];
    for (sx, words) in [(-1.0_f32, &left_words[..]), (1.0_f32, &right_words[..])] {
        spawn_box(commands, pal, red.clone(), o + Vec3::new(sx * 0.29, 0.46, 0.205), Vec3::new(0.06, 0.64, 0.05));
        let cm = pal.textured_lit(images, materials, draw_altar_couplet(words, fonts), 0.6);
        spawn_quad(commands, pal, cm, o + Vec3::new(sx * 0.29, 0.46, 0.232), Vec2::new(0.044, 0.50), Quat::IDENTITY);
    }

    // 4. Two-tier upturned roof + 4 upturned gold eave tips + gold ridge pearl
    spawn_box(commands, pal, red.clone(), o + Vec3::new(0.0, 0.78, 0.0), Vec3::new(0.74, 0.04, 0.56));
    spawn_box(commands, pal, gold.clone(), o + Vec3::new(0.0, 0.805, 0.0), Vec3::new(0.76, 0.018, 0.58));
    spawn_cone_rot(
        commands,
        pal,
        red.clone(),
        o + Vec3::new(0.0, 0.865, 0.0),
        Vec3::new(0.84, 0.11, 0.66),
        Quat::from_rotation_y(FRAC_PI_4),
    );
    spawn_cone_rot(
        commands,
        pal,
        red,
        o + Vec3::new(0.0, 0.95, 0.0),
        Vec3::new(0.54, 0.09, 0.44),
        Quat::from_rotation_y(FRAC_PI_4),
    );
    for sx in [-1.0_f32, 1.0] {
        for sz in [-1.0_f32, 1.0] {
            let rot = Quat::from_euler(EulerRot::XYZ, sz * 0.7, 0.0, -sx * 0.7);
            spawn_cone_rot(
                commands,
                pal,
                gold.clone(),
                o + Vec3::new(sx * 0.40, 0.84, sz * 0.31),
                Vec3::new(0.036, 0.08, 0.036),
                rot,
            );
        }
    }
    spawn_sphere(commands, pal, gold.clone(), o + Vec3::new(0.0, 1.01, 0.0), Vec3::splat(0.06));

    // 5. Inner step + Ông Địa (left) & Thần Tài (right)
    spawn_box(commands, pal, red_dark, o + Vec3::new(0.0, 0.165, -0.10), Vec3::new(0.50, 0.05, 0.18));
    // Ông Địa: round belly, cheerful head, golden fan
    let od = o + Vec3::new(-0.13, 0.19, -0.10);
    spawn_sphere(commands, pal, orange_robe, od + Vec3::new(0.0, 0.06, 0.0), Vec3::new(0.15, 0.142, 0.128));
    spawn_sphere(commands, pal, skin.clone(), od + Vec3::new(0.0, 0.05, 0.045), Vec3::splat(0.09));
    spawn_sphere(commands, pal, skin.clone(), od + Vec3::new(0.0, 0.165, 0.0), Vec3::splat(0.094));
    spawn_cyl_rot(
        commands,
        pal,
        fan_gold,
        od + Vec3::new(0.06, 0.12, 0.05),
        0.07,
        0.006,
        Quat::from_euler(EulerRot::XYZ, 1.2, 0.0, 0.4),
    );
    // Thần Tài: crimson robe, jade belt, beard, winged black official hat, gold ingot
    let tt = o + Vec3::new(0.13, 0.19, -0.10);
    spawn_cyl(commands, pal, crimson_robe, tt + Vec3::new(0.0, 0.075, 0.0), 0.13, 0.15);
    spawn_box(commands, pal, jade_belt, tt + Vec3::new(0.0, 0.11, 0.045), Vec3::new(0.07, 0.03, 0.02));
    spawn_sphere(commands, pal, skin, tt + Vec3::new(0.0, 0.19, 0.0), Vec3::splat(0.08));
    spawn_box(commands, pal, black.clone(), tt + Vec3::new(0.0, 0.165, 0.038), Vec3::new(0.03, 0.035, 0.01));
    spawn_box(commands, pal, black.clone(), tt + Vec3::new(0.0, 0.235, -0.005), Vec3::new(0.075, 0.035, 0.06));
    for sx in [-1.0_f32, 1.0] {
        spawn_box(commands, pal, black.clone(), tt + Vec3::new(sx * 0.06, 0.232, -0.02), Vec3::new(0.05, 0.008, 0.012));
    }
    spawn_box(commands, pal, gold.clone(), tt + Vec3::new(0.0, 0.075, 0.07), Vec3::new(0.06, 0.02, 0.03));

    // 6. Porcelain incense bowl, ash, stubs, pair of red altar lamps, 3 cups, golden toad
    spawn_cyl(commands, pal, porcelain.clone(), o + Vec3::new(0.0, 0.17, 0.10), 0.086, 0.06);
    spawn_cyl(commands, pal, blue_band, o + Vec3::new(0.0, 0.185, 0.10), 0.092, 0.012);
    spawn_cyl(commands, pal, ash, o + Vec3::new(0.0, 0.20, 0.10), 0.08, 0.004);
    for (sx, sz) in [(-0.018, -0.012), (0.015, 0.014), (-0.008, 0.018), (0.02, -0.009)] {
        spawn_cyl(commands, pal, stub.clone(), o + Vec3::new(sx, 0.22, 0.10 + sz), 0.005, 0.04);
    }

    let lamp_mat = pal.owned_emissive(materials, "#E04A34", 1.1);
    for sx in [-1.0_f32, 1.0] {
        spawn_cyl(commands, pal, gold.clone(), o + Vec3::new(sx * 0.22, 0.15, 0.12), 0.056, 0.02);
        let bulb = spawn_cyl(commands, pal, lamp_mat.clone(), o + Vec3::new(sx * 0.22, 0.195, 0.12), 0.04, 0.07);
        commands.entity(bulb).insert(AltarLamp { mat: lamp_mat.clone() });
        spawn_sphere(commands, pal, gold.clone(), o + Vec3::new(sx * 0.22, 0.23, 0.12), Vec3::splat(0.04));
    }

    for cx in [-0.06_f32, 0.0, 0.06] {
        spawn_cyl(commands, pal, porcelain.clone(), o + Vec3::new(cx, 0.152, 0.185), 0.026, 0.02);
    }
    // Golden toad (Thiềm Thừ)
    spawn_sphere(commands, pal, gold.clone(), o + Vec3::new(0.18, 0.165, 0.20), Vec3::new(0.07, 0.042, 0.077));

    // 7. Daily pink rose vases (hidden on Mùng 1 / Rằm) vs yellow chrysanthemum vases (shown on offering days)
    let rose_mat = pal.mat(materials, "#E35D7A", 0.7, 0.0);
    let mum_mat = pal.mat(materials, "#F2C12E", 0.7, 0.0);
    let vase_mat = pal.mat(materials, "#E9E3D6", 0.25, 0.05);
    for sx in [-1.0_f32, 1.0] {
        let dv = spawn_cyl(commands, pal, vase_mat.clone(), o + Vec3::new(sx * 0.14, 0.17, 0.05), 0.04, 0.06);
        commands.entity(dv).insert(AltarDailyFlowers);
        for i in 0..3 {
            let a = (i as f32 / 3.0) * PI * 2.0;
            let f = spawn_sphere(
                commands,
                pal,
                rose_mat.clone(),
                o + Vec3::new(sx * 0.14 + a.cos() * 0.02, 0.225 + (i % 2) as f32 * 0.012, 0.05 + a.sin() * 0.02),
                Vec3::splat(0.036),
            );
            commands.entity(f).insert(AltarDailyFlowers);
        }

        let ov = spawn_cyl(commands, pal, vase_mat.clone(), o + Vec3::new(sx * 0.14, 0.185, 0.05), 0.044, 0.09);
        commands.entity(ov).insert((AltarOfferPart, Visibility::Hidden));
        for i in 0..5 {
            let a = (i as f32 / 5.0) * PI * 2.0;
            let f = spawn_sphere(
                commands,
                pal,
                mum_mat.clone(),
                o + Vec3::new(sx * 0.14 + a.cos() * 0.026, 0.255 + (i % 2) as f32 * 0.015, 0.05 + a.sin() * 0.026),
                Vec3::splat(0.046),
            );
            commands.entity(f).insert((AltarOfferPart, Visibility::Hidden));
        }
    }

    // 8. Offering tray on floor in front: 5-fruit dish (bananas, pomelo, dragon fruit, mango, oranges), xôi gấc, chè
    let tray_o = o + Vec3::new(0.0, 0.0, 0.42);
    let tray_red = pal.mat(materials, "#B22222", 0.35, 0.05);
    let banana_m = pal.mat(materials, "#F2D04B", 0.6, 0.0);
    let pomelo_m = pal.mat(materials, "#7DAF3C", 0.5, 0.0);
    let dragon_m = pal.mat(materials, "#E2456B", 0.5, 0.0);
    let mango_m = pal.mat(materials, "#F3A93B", 0.5, 0.0);
    let orange_m = pal.mat(materials, "#F28C28", 0.5, 0.0);
    let xoi_m = pal.mat(materials, "#C8302A", 0.7, 0.0);

    let mut offer_ents = [Entity::PLACEHOLDER; 16];
    let mut oi = 0usize;
    let mut push_offer = |ent: Entity| {
        offer_ents[oi] = ent;
        oi += 1;
    };
    push_offer(spawn_cyl(commands, pal, tray_red, tray_o + Vec3::new(0.0, 0.01, 0.0), 0.40, 0.02));
    push_offer(spawn_cyl(commands, pal, gold.clone(), tray_o + Vec3::new(-0.06, 0.06, 0.0), 0.08, 0.08));
    push_offer(spawn_cyl(commands, pal, gold, tray_o + Vec3::new(-0.06, 0.105, 0.0), 0.22, 0.015));
    for i in 0..5 {
        let b = spawn_cyl_rot(
            commands,
            pal,
            banana_m.clone(),
            tray_o + Vec3::new(-0.06 + (i as f32 - 2.0) * 0.022, 0.13, 0.06),
            0.024,
            0.075,
            Quat::from_euler(EulerRot::XYZ, FRAC_PI_2, 0.0, 0.25),
        );
        push_offer(b);
    }
    push_offer(spawn_sphere(commands, pal, pomelo_m, tray_o + Vec3::new(-0.06, 0.17, -0.005), Vec3::splat(0.10)));
    push_offer(spawn_sphere(commands, pal, dragon_m, tray_o + Vec3::new(-0.12, 0.14, -0.04), Vec3::new(0.06, 0.075, 0.06)));
    push_offer(spawn_sphere(commands, pal, mango_m, tray_o + Vec3::new(0.0, 0.14, -0.05), Vec3::new(0.076, 0.054, 0.054)));
    push_offer(spawn_sphere(commands, pal, orange_m.clone(), tray_o + Vec3::new(-0.12, 0.14, 0.03), Vec3::splat(0.062)));
    push_offer(spawn_sphere(commands, pal, orange_m, tray_o + Vec3::new(0.0, 0.135, 0.03), Vec3::splat(0.056)));
    push_offer(spawn_cyl(commands, pal, porcelain.clone(), tray_o + Vec3::new(0.11, 0.025, 0.02), 0.12, 0.01));
    push_offer(spawn_sphere(commands, pal, xoi_m, tray_o + Vec3::new(0.11, 0.048, 0.02), Vec3::new(0.10, 0.065, 0.10)));
    push_offer(spawn_cyl(commands, pal, porcelain, tray_o + Vec3::new(0.13, 0.035, -0.10), 0.056, 0.03));
    for ent in &offer_ents[..oi] {
        commands.entity(*ent).insert((AltarOfferPart, Visibility::Hidden));
    }

    // 9. Three incense sticks + fixed pool of 7 rising smoke wisps (zero per-frame allocations)
    let stick_m = pal.matte(materials, "#B0302A");
    let tip_m = pal.emissive(materials, "#FF7A28", 2.4);
    for (dx, lean) in [(-0.014_f32, -0.12_f32), (0.0, 0.0), (0.014, 0.12)] {
        let stick = commands
            .spawn((
                WorldEntity,
                AltarIncenseStick {
                    base_y: o.y + 0.20,
                    full_height: 0.20,
                },
                Mesh3d(pal.unit_cyl.clone()),
                MeshMaterial3d(stick_m.clone()),
                Transform::from_translation(o + Vec3::new(dx, 0.30, 0.10))
                    .with_rotation(Quat::from_rotation_z(lean))
                    .with_scale(Vec3::new(0.005, 0.20, 0.005)),
                Visibility::Hidden,
            ))
            .with_children(|p| {
                p.spawn((
                    WorldEntity,
                    Mesh3d(pal.unit_sphere.clone()),
                    MeshMaterial3d(tip_m.clone()),
                    Transform::from_xyz(0.0, 0.5, 0.0).with_scale(Vec3::new(2.0, 0.05, 2.0)),
                ));
            })
            .id();
        let _ = stick;
    }

    let smoke_m = pal.mat_blend(materials, "#E6E3DC", 0.38, true);
    for i in 0..ALTAR_SMOKE_COUNT {
        commands.spawn((
            WorldEntity,
            AltarSmokeWisp {
                phase: i as f32 / ALTAR_SMOKE_COUNT as f32,
                seed: i as f32 * 1.37,
            },
            Mesh3d(pal.unit_sphere.clone()),
            MeshMaterial3d(smoke_m.clone()),
            Transform::from_translation(o + Vec3::new(0.0, 0.40, 0.10)).with_scale(Vec3::splat(0.04)),
            Visibility::Hidden,
        ));
    }
}

#[allow(clippy::too_many_arguments, clippy::type_complexity)]
pub fn update_altar_system(
    time: Res<Time>,
    mut altar: Option<ResMut<AltarState>>,
    lighting: Option<Res<LightingState>>,
    mut materials: Option<ResMut<Assets<StandardMaterial>>>,
    mut q_offer: Query<
        &mut Visibility,
        (
            With<AltarOfferPart>,
            Without<AltarDailyFlowers>,
            Without<AltarIncenseStick>,
            Without<AltarSmokeWisp>,
        ),
    >,
    mut q_daily: Query<
        &mut Visibility,
        (
            With<AltarDailyFlowers>,
            Without<AltarOfferPart>,
            Without<AltarIncenseStick>,
            Without<AltarSmokeWisp>,
        ),
    >,
    mut q_sticks: Query<
        (&AltarIncenseStick, &mut Visibility, &mut Transform),
        (
            Without<AltarOfferPart>,
            Without<AltarDailyFlowers>,
            Without<AltarSmokeWisp>,
        ),
    >,
    mut q_smoke: Query<
        (&mut AltarSmokeWisp, &mut Visibility, &mut Transform),
        (
            Without<AltarOfferPart>,
            Without<AltarDailyFlowers>,
            Without<AltarIncenseStick>,
        ),
    >,
    q_lamps: Query<&AltarLamp>,
) {
    let Some(altar) = altar.as_deref_mut() else {
        return;
    };
    let dt = time.delta_secs().clamp(0.0, 0.25);

    // Update red altar lamp brightness on day/night toggle
    if let (Some(light), Some(mats)) = (lighting.as_ref(), materials.as_deref_mut()) {
        if light.is_changed() {
            let night = light.is_night();
            let strength = if night { 2.4 } else { 1.0 };
            if let Some(lamp) = q_lamps.iter().next() {
                if let Some(m) = mats.get_mut(&lamp.mat) {
                    m.emissive = LinearRgba::new(0.95 * strength, 0.22 * strength, 0.12 * strength, 1.0);
                }
            }
        }
    }

    let (y, mo, d) = altar.date_ymd;
    let should_offer = altar.force_lunar || is_offering_day(y, mo, d);
    if should_offer {
        altar.offering_active = true;
    } else if !altar.incense_lit {
        altar.offering_active = false;
    }

    let show_offer = altar.offering_active;
    let offer_vis = if show_offer { Visibility::Visible } else { Visibility::Hidden };
    let daily_vis = if show_offer { Visibility::Hidden } else { Visibility::Visible };

    for mut v in &mut q_offer {
        if *v != offer_vis {
            *v = offer_vis;
        }
    }
    for mut v in &mut q_daily {
        if *v != daily_vis {
            *v = daily_vis;
        }
    }

    let burning = show_offer && altar.incense_lit && altar.burn_elapsed_secs < altar.burn_duration_secs;
    if burning {
        altar.burn_elapsed_secs = (altar.burn_elapsed_secs + dt).min(altar.burn_duration_secs);
    } else if altar.incense_lit && altar.burn_elapsed_secs >= altar.burn_duration_secs {
        altar.incense_lit = false;
    }

    let k = if altar.burn_duration_secs > 0.0 {
        (altar.burn_elapsed_secs / altar.burn_duration_secs).clamp(0.0, 1.0)
    } else {
        1.0
    };
    let stick_vis = if burning { Visibility::Visible } else { Visibility::Hidden };
    let cur_h = 0.20 * (1.0 - 0.7 * k);
    let top_y = ALTAR_ORIGIN.y + 0.20 + cur_h;

    for (st, mut vis, mut tf) in &mut q_sticks {
        if *vis != stick_vis {
            *vis = stick_vis;
        }
        if burning {
            let h = st.full_height * (1.0 - 0.7 * k);
            tf.scale.y = h.max(0.02);
            tf.translation.y = st.base_y + h * 0.5;
        }
    }

    for (mut wisp, mut vis, mut tf) in &mut q_smoke {
        if *vis != stick_vis {
            *vis = stick_vis;
        }
        if burning {
            wisp.phase = (wisp.phase + dt * 0.18) % 1.0;
            let t = wisp.phase;
            tf.translation = Vec3::new(
                ALTAR_ORIGIN.x + (t * 7.0 + wisp.seed).sin() * 0.03 * t,
                top_y + t * 0.55,
                ALTAR_ORIGIN.z + 0.10 + (t * 5.0 + wisp.seed).cos() * 0.02 * t,
            );
            let s = 0.03 + t * 0.12;
            tf.scale = Vec3::splat(s);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::layout::OfficeLayout;
    use std::path::PathBuf;

    #[test]
    fn altar_lunar_mung1_and_ram_trigger_offering_tray_and_animated_smoke_without_leaks() {
        // Mùng 1 Tết 2026-02-17 -> lunar day 1; Rằm tháng Giêng 2026-03-03 -> lunar day 15
        assert_eq!(vietnamese_lunar_day(2026, 2, 17), 1);
        assert_eq!(vietnamese_lunar_day(2026, 3, 3), 15);
        assert_eq!(vietnamese_lunar_day(2025, 1, 29), 1);
        assert!(is_offering_day(2026, 2, 17));
        assert!(is_offering_day(2026, 3, 3));
        assert!(!is_offering_day(2026, 2, 20));

        let assets_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../assets");
        let mut app = App::new();
        app.insert_resource(Time::<()>::default())
            .insert_resource(Assets::<Mesh>::default())
            .insert_resource(Assets::<StandardMaterial>::default())
            .insert_resource(Assets::<Image>::default())
            .insert_resource(AltarState {
                date_ymd: (2026, 2, 17),
                incense_lit: true,
                ..default()
            })
            .add_systems(Update, update_altar_system);

        let mut fonts = FontEngine::from_assets_dir(&assets_dir);
        let spec = LayoutSpec::for_layout(OfficeLayout::Pro);
        app.world_mut().resource_scope(|world, mut meshes: Mut<Assets<Mesh>>| {
            world.resource_scope(|world, mut mats: Mut<Assets<StandardMaterial>>| {
                world.resource_scope(|world, mut imgs: Mut<Assets<Image>>| {
                    let mut pal = WorldPalette::new(&mut meshes);
                    let mut cmds = world.commands();
                    spawn_than_tai_altar(&mut cmds, &mut pal, &mut mats, &mut imgs, &mut fonts, &spec);
                    world.insert_resource(pal);
                });
            });
        });
        app.world_mut().flush();

        let smoke_count = app.world_mut().query::<&AltarSmokeWisp>().iter(app.world()).count();
        assert_eq!(smoke_count, ALTAR_SMOKE_COUNT);
        let offer_count = app.world_mut().query::<&AltarOfferPart>().iter(app.world()).count();
        assert!(offer_count >= 12, "expected fruit tray + offerings, got {offer_count}");

        app.world_mut().resource_mut::<Time<()>>().advance_by(std::time::Duration::from_millis(500));
        app.update();

        let state = app.world().resource::<AltarState>();
        assert!(state.offering_active);
        assert!(state.burn_elapsed_secs > 0.0);
        let visible_smoke = app
            .world_mut()
            .query_filtered::<&Visibility, With<AltarSmokeWisp>>()
            .iter(app.world())
            .filter(|v| **v == Visibility::Visible)
            .count();
        assert_eq!(visible_smoke, ALTAR_SMOKE_COUNT);

        // Zero leak check
        let ents: Vec<Entity> = app.world_mut().query_filtered::<Entity, With<WorldEntity>>().iter(app.world()).collect();
        for e in ents {
            app.world_mut().despawn(e);
        }
        let mut pal = app.world_mut().remove_resource::<WorldPalette>().unwrap();
        app.world_mut().resource_scope(|world, mut meshes: Mut<Assets<Mesh>>| {
            world.resource_scope(|world, mut mats: Mut<Assets<StandardMaterial>>| {
                world.resource_scope(|_world, mut imgs: Mut<Assets<Image>>| {
                    pal.purge_all(&mut meshes, &mut mats, &mut imgs);
                });
            });
        });
        assert_eq!(app.world().resource::<Assets<Mesh>>().len(), 0);
        assert_eq!(app.world().resource::<Assets<StandardMaterial>>().len(), 0);
        assert_eq!(app.world().resource::<Assets<Image>>().len(), 0);
    }
}
