//! Seasonal decorations (`seasons.js`: Halloween, Christmas, Tết, MidAutumn) and
//! live rain/weather visuals (`weather.js`, `rain.js`: pooled raindrops strictly outside the
//! office floor, `"SÀN ƯỚT"` wet-floor A-frame cone, umbrella stand, and `NavGraph` wet-strip detour).

use crate::layout::LayoutSpec;
use crate::life::{OfficeLifeState, RainDrop, SeasonTheme};
use crate::nav::NavGraph;
use crate::palette::{
    WorldEntity, WorldPalette, spawn_box, spawn_box_rot, spawn_cone_rot, spawn_cyl, spawn_cyl_rot,
    spawn_floor_quad, spawn_quad, spawn_sphere, spawn_torus_rot,
};
use bevy::prelude::*;
use station_canvas2d::{Canvas2d, FontEngine, RgbaSurface, TextAlign, hex};
use std::f32::consts::{FRAC_PI_2, FRAC_PI_4, PI};

/// Fixed raindrop pool cap: zero per-frame entity spawn/despawn and zero heap allocations.
pub const RAIN_POOL_CAP: usize = 240;
/// Fixed Christmas snowflake pool cap.
pub const SNOW_POOL_CAP: usize = 48;

#[derive(Resource, Debug, Default, Clone)]
pub struct WeatherVisualState {
    pub wet_strip_active: bool,
    pub lightning_flash: f32,
    pub next_flash_at: f32,
    pub sim_secs: f32,
}

#[derive(Component, Debug, Clone, Copy)]
pub struct WetFloorKitPart;

#[derive(Component, Debug, Clone, Copy)]
pub struct UmbrellaStandPart;

#[derive(Component, Debug, Clone, Copy)]
pub struct SeasonDecorPart;

#[derive(Component, Debug, Clone, Copy)]
pub struct HalloweenBat {
    pub cx: f32,
    pub cy: f32,
    pub cz: f32,
    pub radius: f32,
    pub speed: f32,
    pub phase: f32,
}

#[derive(Component, Debug, Clone, Copy)]
pub struct HalloweenGhost {
    pub base_y: f32,
}

#[derive(Component, Debug, Clone, Copy)]
pub struct SnowFlake {
    pub base_x: f32,
    pub base_z: f32,
    pub speed: f32,
    pub phase: f32,
}

#[inline]
fn hash_f32(seed: u32) -> f32 {
    let mut x = seed.wrapping_mul(747796405).wrapping_add(2891336453);
    x = ((x >> ((x >> 28).wrapping_add(4))) ^ x).wrapping_mul(277803737);
    x ^= x >> 22;
    (x & 0x00ff_ffff) as f32 / 16_777_215.0
}

fn draw_wet_floor_sign(fonts: &mut FontEngine) -> RgbaSurface {
    let mut c = Canvas2d::new(160, 256);
    c.fill(hex("#F5C518"));
    // Black warning triangle with yellow exclamation mark
    c.round_rect(26.0, 26.0, 108.0, 92.0, 12.0, hex("#1E1E1E"));
    fonts.draw_text(
        &mut c,
        "!",
        80.0,
        74.0,
        62.0,
        hex("#F5C518"),
        TextAlign::Center,
        Some(90.0),
        true,
        false,
    );
    fonts.draw_text(
        &mut c,
        "SÀN ƯỚT",
        80.0,
        166.0,
        28.0,
        hex("#1E1E1E"),
        TextAlign::Center,
        Some(144.0),
        true,
        false,
    );
    fonts.draw_text(
        &mut c,
        "CẨN THẬN",
        80.0,
        208.0,
        22.0,
        hex("#1E1E1E"),
        TextAlign::Center,
        Some(144.0),
        true,
        false,
    );
    c.into_surface()
}

fn draw_tet_banner(fonts: &mut FontEngine) -> RgbaSurface {
    let mut c = Canvas2d::new(512, 96);
    c.fill(hex("#B5161F"));
    c.stroke_rect(6.0, 6.0, 500.0, 84.0, 4.0, hex("#E9B949"));
    fonts.draw_text(
        &mut c,
        "CHÚC MỪNG NĂM MỚI",
        256.0,
        50.0,
        36.0,
        hex("#F6CF57"),
        TextAlign::Center,
        Some(470.0),
        true,
        false,
    );
    c.into_surface()
}

fn draw_tet_couplet(words: &[&str], fonts: &mut FontEngine) -> RgbaSurface {
    let mut c = Canvas2d::new(80, 320);
    c.fill(hex("#B5161F"));
    c.stroke_rect(5.0, 5.0, 70.0, 310.0, 3.0, hex("#E9B949"));
    let count = words.len().max(1);
    let step = if count > 1 {
        244.0 / (count - 1) as f32
    } else {
        0.0
    };
    for (i, w) in words.iter().enumerate() {
        fonts.draw_text(
            &mut c,
            w,
            40.0,
            38.0 + i as f32 * step,
            22.0,
            hex("#F6CF57"),
            TextAlign::Center,
            Some(66.0),
            true,
            false,
        );
    }
    c.into_surface()
}

fn spawn_pumpkin(
    commands: &mut Commands,
    pal: &mut WorldPalette,
    materials: &mut Assets<StandardMaterial>,
    pos: Vec3,
    scale: f32,
) {
    let orange = pal.mat(materials, "#E46F1E", 0.55, 0.0);
    let stem = pal.matte(materials, "#5B7A2E");
    let glow = pal.emissive(materials, "#FFB22E", 1.8);
    let body = spawn_sphere(
        commands,
        pal,
        orange,
        pos + Vec3::new(0.0, 0.16 * scale, 0.0),
        Vec3::new(0.44 * scale, 0.31 * scale, 0.44 * scale),
    );
    let st = spawn_cyl(
        commands,
        pal,
        stem,
        pos + Vec3::new(0.01 * scale, 0.33 * scale, 0.0),
        0.04 * scale,
        0.08 * scale,
    );
    let face = spawn_box(
        commands,
        pal,
        glow,
        pos + Vec3::new(0.0, 0.16 * scale, 0.21 * scale),
        Vec3::new(0.16 * scale, 0.08 * scale, 0.02 * scale),
    );
    for e in [body, st, face] {
        commands.entity(e).insert(SeasonDecorPart);
    }
}

fn spawn_tet_lantern(
    commands: &mut Commands,
    pal: &mut WorldPalette,
    materials: &mut Assets<StandardMaterial>,
    pos: Vec3,
    scale: f32,
) {
    let red = pal.emissive(materials, "#D21F2B", 0.9);
    let gold = pal.mat(materials, "#E9B949", 0.3, 0.6);
    let body = spawn_sphere(commands, pal, red.clone(), pos, Vec3::new(0.24, 0.28, 0.24) * scale);
    let cap_t = spawn_cyl(commands, pal, gold.clone(), pos + Vec3::new(0.0, 0.15 * scale, 0.0), 0.09 * scale, 0.03 * scale);
    let cap_b = spawn_cyl(commands, pal, gold, pos + Vec3::new(0.0, -0.15 * scale, 0.0), 0.09 * scale, 0.03 * scale);
    let tassel = spawn_cyl(commands, pal, red, pos + Vec3::new(0.0, -0.24 * scale, 0.0), 0.024 * scale, 0.14 * scale);
    for e in [body, cap_t, cap_b, tassel] {
        commands.entity(e).insert(SeasonDecorPart);
    }
}

/// Spawns seasonal decorations (Halloween / Christmas / Tết / MidAutumn), entrance umbrella stand,
/// rain-activated `"SÀN ƯỚT"` wet-floor cone + drying umbrella, and the fixed `RAIN_POOL_CAP` pool.
pub fn spawn_seasons_and_weather(
    commands: &mut Commands,
    pal: &mut WorldPalette,
    materials: &mut Assets<StandardMaterial>,
    images: &mut Assets<Image>,
    fonts: &mut FontEngine,
    spec: &LayoutSpec,
    life: &OfficeLifeState,
) {
    let is_pro = spec.layout.is_pro();

    // 1. Always-present entrance umbrella stand (`rain.js:55-64`)
    let stand_pos = if is_pro {
        Vec3::new(-7.74, 0.0, 2.0)
    } else {
        Vec3::new(-7.60, 0.0, 2.75)
    };
    let stand_m = pal.mat(materials, "#3A3F47", 0.45, 0.3);
    let metal_m = pal.mat(materials, "#B4BCC6", 0.35, 0.65);
    let u_handle = pal.mat(materials, "#4A3628", 0.5, 0.0);
    let s1 = spawn_cyl(commands, pal, stand_m, stand_pos + Vec3::new(0.0, 0.25, 0.0), 0.24, 0.50);
    let s2 = spawn_torus_rot(
        commands,
        pal,
        metal_m.clone(),
        stand_pos + Vec3::new(0.0, 0.50, 0.0),
        Vec3::splat(0.26),
        Quat::from_rotation_x(FRAC_PI_2),
    );
    commands.entity(s1).insert(UmbrellaStandPart);
    commands.entity(s2).insert(UmbrellaStandPart);
    for (dx, dz, hex_col, tilt) in [
        (-0.04_f32, 0.03_f32, "#2F3B52", 0.10_f32),
        (0.05, -0.02, "#C8283A", -0.14),
        (0.0, -0.05, "#26292E", 0.05),
    ] {
        let um = pal.mat(materials, hex_col, 0.7, 0.0);
        let rot = Quat::from_euler(EulerRot::XYZ, tilt, 0.0, -tilt);
        let ue = spawn_cyl_rot(
            commands,
            pal,
            um,
            stand_pos + Vec3::new(dx, 0.42, dz),
            0.06,
            0.62,
            rot,
        );
        let uh = spawn_cyl_rot(
            commands,
            pal,
            u_handle.clone(),
            stand_pos + Vec3::new(dx, 0.80, dz),
            0.018,
            0.16,
            rot,
        );
        commands.entity(ue).insert(UmbrellaStandPart);
        commands.entity(uh).insert(UmbrellaStandPart);
    }

    // 2. Rain-activated wet-floor kit: drying umbrella, water puddles, yellow A-frame "SÀN ƯỚT" cone
    let (umb_pos, umb_ry, sign_pos, sign_ry) = if is_pro {
        (Vec3::new(-6.25, 0.0, 4.12), 0.5_f32, Vec3::new(-4.20, 0.0, 3.78), 0.6_f32)
    } else {
        (Vec3::new(-6.90, 0.0, 3.20), 0.9_f32, Vec3::new(-5.60, 0.0, 0.40), 0.4_f32)
    };
    let initial_wet_vis = if life.rain_intensity > 0.0 {
        Visibility::Visible
    } else {
        Visibility::Hidden
    };

    // Open drying umbrella tilted on floor
    let canopy_m = pal.mat_double_sided(materials, "#E33B40", 0.5, 0.0);
    let umb_rot = Quat::from_rotation_y(umb_ry) * Quat::from_rotation_z(1.05);
    let u_canopy = spawn_cone_rot(
        commands,
        pal,
        canopy_m,
        umb_pos + Vec3::new(0.0, 0.36, 0.0),
        Vec3::new(0.90, 0.24, 0.90),
        umb_rot,
    );
    let u_shaft = spawn_cyl_rot(
        commands,
        pal,
        metal_m,
        umb_pos + Vec3::new(0.18, 0.24, 0.0),
        0.016,
        0.70,
        umb_rot,
    );
    commands.entity(u_canopy).insert((WetFloorKitPart, initial_wet_vis));
    commands.entity(u_shaft).insert((WetFloorKitPart, initial_wet_vis));

    // Puddles around drying umbrella
    let puddle_m = pal.mat_blend(materials, "#BFD9EA", 0.45, false);
    for i in 0..6u32 {
        let px = umb_pos.x + (hash_f32(101 + i * 7) - 0.5) * 0.85;
        let pz = umb_pos.z + (hash_f32(211 + i * 11) - 0.5) * 0.65;
        let r = 0.12 + hash_f32(307 + i * 13) * 0.10;
        let pd = spawn_floor_quad(commands, pal, puddle_m.clone(), Vec3::new(px, 0.005, pz), Vec2::splat(r));
        commands.entity(pd).insert((WetFloorKitPart, initial_wet_vis));
    }

    // Yellow A-frame "SÀN ƯỚT" wet-floor sign (`rain.js:33-49`)
    let yellow_m = pal.mat(materials, "#F5C518", 0.5, 0.0);
    let dark_m = pal.matte(materials, "#1E1E1E");
    let wet_tex_m = pal.textured_lit(images, materials, draw_wet_floor_sign(fonts), 0.55);
    let sign_rot = Quat::from_rotation_y(sign_ry);
    let cap = spawn_box_rot(
        commands,
        pal,
        dark_m,
        sign_pos + Vec3::new(0.0, 0.63, 0.0),
        Vec3::new(0.06, 0.04, 0.05),
        sign_rot,
    );
    commands.entity(cap).insert((WetFloorKitPart, initial_wet_vis));
    for s in [-1.0_f32, 1.0] {
        let leaf_rot = sign_rot * Quat::from_rotation_x(s * 0.20);
        let offset = sign_rot * Vec3::new(0.0, 0.31, s * 0.07);
        let leaf = spawn_box_rot(
            commands,
            pal,
            yellow_m.clone(),
            sign_pos + offset,
            Vec3::new(0.30, 0.62, 0.015),
            leaf_rot,
        );
        commands.entity(leaf).insert((WetFloorKitPart, initial_wet_vis));
        let face_rot = leaf_rot * if s > 0.0 { Quat::IDENTITY } else { Quat::from_rotation_y(PI) };
        let face_off = sign_rot * Vec3::new(0.0, 0.31, s * 0.082);
        let q = spawn_quad(
            commands,
            pal,
            wet_tex_m.clone(),
            sign_pos + face_off,
            Vec2::new(0.26, 0.42),
            face_rot,
        );
        commands.entity(q).insert((WetFloorKitPart, initial_wet_vis));
    }

    // 3. Fixed pooled RainDrops strictly outside office bounds (`weather.js:36-44`)
    let drop_m = pal.mat_blend(materials, "#B8CCEA", 0.55, true);
    let bx0 = spec.bounds.x0 - 0.8;
    let bx1 = spec.bounds.x1 + 0.8;
    let bz0 = spec.bounds.z0 - 0.8;
    let bz1 = spec.bounds.z1 + 0.8;
    let active_drops = ((RAIN_POOL_CAP as f32) * life.rain_intensity.clamp(0.0, 1.0)).round() as usize;

    for i in 0..RAIN_POOL_CAP {
        let mut x = 0.0_f32;
        let mut z = 0.0_f32;
        for attempt in 0..16u32 {
            let s = (i as u32) * 31 + attempt * 97 + 17;
            let cx = -34.0 + hash_f32(s) * 68.0;
            let cz = -34.0 + hash_f32(s + 13) * 68.0;
            if !(cx > bx0 && cx < bx1 && cz > bz0 && cz < bz1) {
                x = cx;
                z = cz;
                break;
            }
        }
        if x > bx0 && x < bx1 && z > bz0 && z < bz1 {
            x = bx0 - 2.0 - (i % 12) as f32;
        }
        let y = 1.0 + hash_f32((i as u32) * 43 + 7) * 14.0;
        let speed = 13.0 + hash_f32((i as u32) * 59 + 3) * 5.0;
        let vis = if i < active_drops {
            Visibility::Visible
        } else {
            Visibility::Hidden
        };
        commands.spawn((
            WorldEntity,
            RainDrop { x, z, speed },
            Mesh3d(pal.unit_box.clone()),
            MeshMaterial3d(drop_m.clone()),
            Transform::from_xyz(x, y, z).with_scale(Vec3::new(0.018, 0.52, 0.018)),
            vis,
        ));
    }

    // 4. Date-driven Seasonal Decorations (`seasons.js`)
    let centre = if is_pro {
        Vec3::new(5.8, 0.0, 7.1)
    } else {
        Vec3::new(6.1, 0.0, 4.55)
    };

    match life.season {
        SeasonTheme::Halloween => {
            // Hay bale centerpiece + stacked jack-o'-lantern pumpkins
            let hay_m = pal.matte(materials, "#D8B45C");
            let strap_m = pal.matte(materials, "#8A6A3A");
            let bale = spawn_box_rot(
                commands,
                pal,
                hay_m,
                centre + Vec3::new(0.0, 0.21, 0.0),
                Vec3::new(0.95, 0.42, 0.56),
                Quat::from_rotation_y(FRAC_PI_4),
            );
            commands.entity(bale).insert(SeasonDecorPart);
            for sx in [-0.24_f32, 0.24] {
                let s = spawn_box_rot(
                    commands,
                    pal,
                    strap_m.clone(),
                    centre + Quat::from_rotation_y(FRAC_PI_4) * Vec3::new(sx, 0.215, 0.0),
                    Vec3::new(0.03, 0.43, 0.57),
                    Quat::from_rotation_y(FRAC_PI_4),
                );
                commands.entity(s).insert(SeasonDecorPart);
            }
            spawn_pumpkin(commands, pal, materials, centre + Vec3::new(0.0, 0.42, 0.0), 1.35);
            spawn_pumpkin(commands, pal, materials, centre + Vec3::new(-0.58, 0.0, 0.30), 0.90);
            spawn_pumpkin(commands, pal, materials, centre + Vec3::new(0.56, 0.0, 0.32), 0.70);

            // Corner cobweb strands
            let web_m = pal.mat_blend(materials, "#F2F2F2", 0.55, true);
            let web_origin = Vec3::new(spec.bounds.x0 + 0.15, 3.85, spec.bounds.z0 + 0.15);
            for k in 0..5 {
                let ang = (k as f32) * 0.26;
                let strand = spawn_box_rot(
                    commands,
                    pal,
                    web_m.clone(),
                    web_origin + Vec3::new(0.35 * ang.cos(), -0.25, 0.35 * ang.sin()),
                    Vec3::new(0.85, 0.008, 0.008),
                    Quat::from_euler(EulerRot::XYZ, 0.0, -ang, -0.45),
                );
                commands.entity(strand).insert(SeasonDecorPart);
            }

            // Circling bats + lobby ghost
            let bat_m = pal.matte(materials, "#221E2A");
            let bat_centers: &[(f32, f32, f32, f32)] = if is_pro {
                &[(-5.6, 3.2, 3.3, 1.2), (11.3, 3.4, -3.8, 1.6), (2.5, 3.5, 3.6, 2.0)]
            } else {
                &[(-4.0, 3.2, 0.0, 1.4), (3.0, 3.3, -2.0, 1.6)]
            };
            for (k, &(cx, cy, cz, r)) in bat_centers.iter().enumerate() {
                for j in 0..3 {
                    let b = commands
                        .spawn((
                            WorldEntity,
                            SeasonDecorPart,
                            HalloweenBat {
                                cx,
                                cy,
                                cz,
                                radius: r * (0.75 + j as f32 * 0.2),
                                speed: (0.55 + j as f32 * 0.15) * if k % 2 == 1 { -1.0 } else { 1.0 },
                                phase: j as f32 * 2.1 + k as f32,
                            },
                            Mesh3d(pal.unit_sphere.clone()),
                            MeshMaterial3d(bat_m.clone()),
                            Transform::from_xyz(cx + r, cy, cz).with_scale(Vec3::new(0.28, 0.06, 0.10)),
                        ))
                        .id();
                    let _ = b;
                }
            }

            let ghost_pos = if is_pro {
                Vec3::new(-6.6, 2.05, 5.05)
            } else {
                Vec3::new(-6.4, 2.0, 4.2)
            };
            let ghost_m = pal.mat_blend(materials, "#F7F7FB", 0.88, false);
            commands.spawn((
                WorldEntity,
                SeasonDecorPart,
                HalloweenGhost { base_y: ghost_pos.y },
                Mesh3d(pal.unit_sphere.clone()),
                MeshMaterial3d(ghost_m),
                Transform::from_translation(ghost_pos).with_scale(Vec3::new(0.36, 0.56, 0.36)),
            ));
        }
        SeasonTheme::Christmas => {
            // 4-tier Christmas tree with red/gold pot, ornaments, topper star, gifts, wreath, snowflakes
            let pot_red = pal.mat(materials, "#B3263A", 0.45, 0.05);
            let gold = pal.mat(materials, "#E9B949", 0.3, 0.65);
            let trunk_m = pal.matte(materials, "#5A3B26");
            let pine_a = pal.matte(materials, "#1F5E3B");
            let pine_b = pal.matte(materials, "#24683F");
            let star_m = pal.emissive(materials, "#FFD24A", 2.2);

            for e in [
                spawn_cyl(commands, pal, pot_red.clone(), centre + Vec3::new(0.0, 0.18, 0.0), 0.56, 0.36),
                spawn_cyl(commands, pal, gold.clone(), centre + Vec3::new(0.0, 0.30, 0.0), 0.61, 0.05),
                spawn_cyl(commands, pal, trunk_m, centre + Vec3::new(0.0, 0.45, 0.0), 0.13, 0.30),
            ] {
                commands.entity(e).insert(SeasonDecorPart);
            }
            for (i, (rad, h, y)) in [
                (0.72_f32, 0.75_f32, 0.55_f32),
                (0.58, 0.66, 0.95),
                (0.44, 0.56, 1.32),
                (0.30, 0.46, 1.66),
            ]
            .into_iter()
            .enumerate()
            {
                let cone = spawn_cone_rot(
                    commands,
                    pal,
                    if i % 2 == 0 { pine_a.clone() } else { pine_b.clone() },
                    centre + Vec3::new(0.0, y + h * 0.5, 0.0),
                    Vec3::new(rad * 2.0, h, rad * 2.0),
                    Quat::IDENTITY,
                );
                commands.entity(cone).insert(SeasonDecorPart);
            }
            let star = spawn_sphere(commands, pal, star_m, centre + Vec3::new(0.0, 2.22, 0.0), Vec3::splat(0.16));
            commands.entity(star).insert(SeasonDecorPart);

            // Ornaments & wrapped gifts
            let orn_cols = ["#D7263D", "#E9B949", "#3F7CC4", "#F4F1EA"];
            for i in 0..16 {
                let t = i as f32 / 16.0;
                let ang = t * PI * 7.0;
                let r = 0.68 * (1.0 - t) + 0.10;
                let y = 0.65 + t * 1.35;
                let om = pal.emissive(materials, orn_cols[i % orn_cols.len()], 1.4);
                let e = spawn_sphere(commands, pal, om, centre + Vec3::new(ang.cos() * r, y, ang.sin() * r), Vec3::splat(0.075));
                commands.entity(e).insert(SeasonDecorPart);
            }
            for (gx, gz, gw, gh, gd, col) in [
                (0.50_f32, 0.35_f32, 0.30_f32, 0.24_f32, 0.26_f32, "#3F7CC4"),
                (-0.45, 0.42, 0.26, 0.20, 0.26, "#D7263D"),
                (0.05, 0.62, 0.22, 0.16, 0.20, "#2E7D4F"),
                (-0.56, -0.10, 0.24, 0.28, 0.24, "#E9B949"),
            ] {
                let gm = pal.mat(materials, col, 0.55, 0.05);
                let b = spawn_box(commands, pal, gm, centre + Vec3::new(gx, gh * 0.5, gz), Vec3::new(gw, gh, gd));
                commands.entity(b).insert(SeasonDecorPart);
            }

            // Wall wreath (Pro/HQ)
            if is_pro {
                let wreath = spawn_torus_rot(
                    commands,
                    pal,
                    pine_a,
                    Vec3::new(-2.05, 1.95, -7.90),
                    Vec3::splat(0.44),
                    Quat::IDENTITY,
                );
                commands.entity(wreath).insert(SeasonDecorPart);
            }

            // Gentle pooled exterior snowflakes
            let snow_m = pal.mat_unlit(materials, "#F4F8FF");
            for i in 0..SNOW_POOL_CAP {
                let sx = -18.0 + hash_f32(701 + i as u32 * 19) * 36.0;
                let sz = -18.0 + hash_f32(809 + i as u32 * 23) * 36.0;
                let sy = 1.0 + hash_f32(907 + i as u32 * 29) * 9.0;
                commands.spawn((
                    WorldEntity,
                    SeasonDecorPart,
                    SnowFlake {
                        base_x: sx,
                        base_z: sz,
                        speed: 0.9 + hash_f32(997 + i as u32 * 31) * 0.7,
                        phase: i as f32 * 0.41,
                    },
                    Mesh3d(pal.unit_sphere.clone()),
                    MeshMaterial3d(snow_m.clone()),
                    Transform::from_xyz(sx, sy, sz).with_scale(Vec3::splat(0.05)),
                ));
            }
        }
        SeasonTheme::Tet => {
            // Peach blossom tree (hoa đào) in blue-and-white pot + red lì xì + bánh chưng tray + lanterns + câu đối
            let pot_blue = pal.mat(materials, "#2E5FA3", 0.35, 0.1);
            let pot_rim = pal.mat(materials, "#F4F1EA", 0.35, 0.05);
            let bark = pal.matte(materials, "#4A2E1F");
            let pink_a = pal.emissive(materials, "#F7A8C4", 0.65);
            let pink_b = pal.emissive(materials, "#F28DB2", 0.75);
            let lixi_red = pal.mat(materials, "#D21F2B", 0.4, 0.1);
            let banh_green = pal.matte(materials, "#2F6B3A");
            let banh_string = pal.matte(materials, "#E9D9A8");

            for e in [
                spawn_cyl(commands, pal, pot_blue, centre + Vec3::new(0.0, 0.24, 0.0), 0.62, 0.48),
                spawn_torus_rot(commands, pal, pot_rim, centre + Vec3::new(0.0, 0.38, 0.0), Vec3::splat(0.64), Quat::from_rotation_x(FRAC_PI_2)),
                spawn_cyl(commands, pal, bark.clone(), centre + Vec3::new(0.0, 0.85, 0.0), 0.09, 0.78),
            ] {
                commands.entity(e).insert(SeasonDecorPart);
            }
            for (dx, dy, dz, r, alt) in [
                (0.0_f32, 1.38_f32, 0.0_f32, 0.48_f32, false),
                (0.32, 1.20, -0.18, 0.34, true),
                (-0.30, 1.24, 0.22, 0.36, false),
                (0.18, 1.56, 0.16, 0.28, true),
            ] {
                let b = spawn_sphere(
                    commands,
                    pal,
                    if alt { pink_b.clone() } else { pink_a.clone() },
                    centre + Vec3::new(dx, dy, dz),
                    Vec3::splat(r * 2.0),
                );
                commands.entity(b).insert(SeasonDecorPart);
            }
            for i in 0..4 {
                let ang = (i as f32) * FRAC_PI_2 + 0.3;
                let lx = spawn_box(
                    commands,
                    pal,
                    lixi_red.clone(),
                    centre + Vec3::new(ang.cos() * 0.34, 1.05, ang.sin() * 0.34),
                    Vec3::new(0.06, 0.09, 0.01),
                );
                commands.entity(lx).insert(SeasonDecorPart);
            }

            // Square green Bánh Chưng tray next to the blossom tree
            let bc = centre + Vec3::new(-0.55, 0.0, 0.38);
            for (dy, rot_y) in [(0.045_f32, 0.15_f32), (0.135, -0.12)] {
                let cake = spawn_box_rot(
                    commands,
                    pal,
                    banh_green.clone(),
                    bc + Vec3::new(0.0, dy, 0.0),
                    Vec3::new(0.24, 0.085, 0.24),
                    Quat::from_rotation_y(rot_y),
                );
                let tie = spawn_box_rot(
                    commands,
                    pal,
                    banh_string.clone(),
                    bc + Vec3::new(0.0, dy, 0.0),
                    Vec3::new(0.246, 0.09, 0.016),
                    Quat::from_rotation_y(rot_y),
                );
                commands.entity(cake).insert(SeasonDecorPart);
                commands.entity(tie).insert(SeasonDecorPart);
            }

            // Hanging red Tết lanterns + banner + vertical câu đối (`seasons.js:188-201`)
            if is_pro {
                spawn_tet_lantern(commands, pal, materials, Vec3::new(-1.22, 3.15, -7.80), 1.0);
                spawn_tet_lantern(commands, pal, materials, Vec3::new(1.62, 3.15, -7.80), 1.0);
                let banner_m = pal.textured_lit(images, materials, draw_tet_banner(fonts), 0.6);
                let b = spawn_quad(commands, pal, banner_m, Vec3::new(0.20, 2.90, -7.91), Vec2::new(2.4, 0.36), Quat::IDENTITY);
                commands.entity(b).insert(SeasonDecorPart);

                let left_c = ["CHÚC", "MỪNG", "NĂM", "MỚI"];
                let right_c = ["VẠN", "SỰ", "NHƯ", "Ý"];
                for (sz, words) in [(-1.0_f32, &left_c[..]), (1.0_f32, &right_c[..])] {
                    let cm = pal.textured_lit(images, materials, draw_tet_couplet(words, fonts), 0.6);
                    let sc = spawn_quad(
                        commands,
                        pal,
                        cm,
                        Vec3::new(-6.93, 1.45, spec.door_z + sz * 0.8),
                        Vec2::new(0.20, 0.90),
                        Quat::from_rotation_y(FRAC_PI_2),
                    );
                    commands.entity(sc).insert(SeasonDecorPart);
                }
            } else {
                spawn_tet_lantern(commands, pal, materials, Vec3::new(0.0, 3.20, -5.80), 0.95);
            }
        }
        SeasonTheme::MidAutumn => {
            spawn_tet_lantern(commands, pal, materials, Vec3::new(spec.center.x, 3.35, spec.center.y), 1.1);
        }
        SeasonTheme::Normal => {}
    }
}

#[allow(clippy::too_many_arguments, clippy::type_complexity)]
pub fn update_seasons_and_weather_system(
    time: Res<Time>,
    life: Option<Res<OfficeLifeState>>,
    mut wx: Option<ResMut<WeatherVisualState>>,
    mut nav: Option<ResMut<NavGraph>>,
    mut q_rain: Query<
        (&RainDrop, &mut Transform, &mut Visibility),
        (
            Without<WetFloorKitPart>,
            Without<HalloweenBat>,
            Without<HalloweenGhost>,
            Without<SnowFlake>,
        ),
    >,
    mut q_wet: Query<
        &mut Visibility,
        (
            With<WetFloorKitPart>,
            Without<RainDrop>,
            Without<HalloweenBat>,
            Without<HalloweenGhost>,
            Without<SnowFlake>,
        ),
    >,
    mut q_bats: Query<
        (&HalloweenBat, &mut Transform),
        (Without<RainDrop>, Without<HalloweenGhost>, Without<SnowFlake>),
    >,
    mut q_ghost: Query<
        (&HalloweenGhost, &mut Transform),
        (Without<RainDrop>, Without<HalloweenBat>, Without<SnowFlake>),
    >,
    mut q_snow: Query<
        (&SnowFlake, &mut Transform),
        (Without<RainDrop>, Without<HalloweenBat>, Without<HalloweenGhost>),
    >,
) {
    let dt = time.delta_secs().clamp(0.0, 0.25);
    let rain_intensity = life.as_deref().map(|l| l.rain_intensity).unwrap_or(0.0);
    let storm = life.as_deref().map(|l| l.storm).unwrap_or(false);
    let raining = rain_intensity > 0.01;

    let sim_t = if let Some(w) = wx.as_deref_mut() {
        w.sim_secs += dt;
        // Hook wet-floor strip detour into NavGraph only on state transition (zero per-frame churn)
        if w.wet_strip_active != raining {
            w.wet_strip_active = raining;
            if let Some(graph) = nav.as_deref_mut() {
                if raining {
                    graph.add_wet_strip(Vec2::new(-4.65, 3.35), Vec2::new(-3.75, 4.20));
                } else {
                    graph.clear_wet_strips();
                }
            }
        }
        if storm {
            if w.next_flash_at <= 0.0 {
                w.next_flash_at = w.sim_secs + 8.0;
            }
            if w.sim_secs >= w.next_flash_at {
                w.lightning_flash = 0.35;
                w.next_flash_at = w.sim_secs + 12.0;
            }
        }
        if w.lightning_flash > 0.0 {
            w.lightning_flash = (w.lightning_flash - dt).max(0.0);
        }
        w.sim_secs
    } else {
        time.elapsed_secs()
    };

    // Toggle wet-floor cone + drying umbrella + puddles
    let wet_vis = if raining { Visibility::Visible } else { Visibility::Hidden };
    for mut v in &mut q_wet {
        if *v != wet_vis {
            *v = wet_vis;
        }
    }

    // Step fixed rain pool in-place (zero allocations)
    let active_drops = if raining {
        ((RAIN_POOL_CAP as f32) * rain_intensity.clamp(0.0, 1.0)).round() as usize
    } else {
        0
    };
    for (idx, (drop, mut tf, mut vis)) in q_rain.iter_mut().enumerate() {
        let want = if idx < active_drops {
            Visibility::Visible
        } else {
            Visibility::Hidden
        };
        if *vis != want {
            *vis = want;
        }
        if idx < active_drops {
            tf.translation.y -= drop.speed * dt;
            if tf.translation.y < -0.3 {
                tf.translation.y = 14.5;
                tf.translation.x = drop.x;
                tf.translation.z = drop.z;
            }
        }
    }

    // Animate Halloween bats & ghost
    for (bat, mut tf) in &mut q_bats {
        let a = bat.phase + sim_t * bat.speed;
        let x = bat.cx + a.cos() * bat.radius;
        let z = bat.cz + a.sin() * bat.radius * 0.7;
        let y = bat.cy + (sim_t * 1.7 + bat.phase).sin() * 0.18;
        let yaw = (-a.sin() * bat.radius * bat.speed).atan2(a.cos() * bat.radius * 0.7 * bat.speed);
        tf.translation = Vec3::new(x, y, z);
        tf.rotation = Quat::from_rotation_y(yaw);
    }
    for (ghost, mut tf) in &mut q_ghost {
        tf.translation.y = ghost.base_y + (sim_t * 1.4).sin() * 0.09;
        tf.rotation = Quat::from_euler(
            EulerRot::YXZ,
            FRAC_PI_4 + (sim_t * 0.45).sin() * 0.5,
            0.0,
            (sim_t * 1.1).sin() * 0.08,
        );
    }

    // Animate Christmas snowflakes
    for (flake, mut tf) in &mut q_snow {
        tf.translation.y -= flake.speed * dt;
        tf.translation.x = flake.base_x + (sim_t * 0.9 + flake.phase).sin() * 0.35;
        tf.translation.z = flake.base_z + (sim_t * 0.7 + flake.phase).cos() * 0.35;
        if tf.translation.y < 0.0 {
            tf.translation.y = 9.5;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::layout::OfficeLayout;
    use std::path::PathBuf;

    #[test]
    fn rain_toggles_pooled_raindrops_wet_floor_cone_umbrella_stand_and_nav_wet_strip() {
        let assets_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../assets");
        let mut app = App::new();
        let mut life = OfficeLifeState::default();
        life.season = SeasonTheme::Tet;
        app.insert_resource(Time::<()>::default())
            .insert_resource(Assets::<Mesh>::default())
            .insert_resource(Assets::<StandardMaterial>::default())
            .insert_resource(Assets::<Image>::default())
            .insert_resource(NavGraph::for_layout(OfficeLayout::Pro))
            .insert_resource(WeatherVisualState::default())
            .insert_resource(life.clone())
            .add_systems(Update, update_seasons_and_weather_system);

        let mut fonts = FontEngine::from_assets_dir(&assets_dir);
        let spec = LayoutSpec::for_layout(OfficeLayout::Pro);
        app.world_mut().resource_scope(|world, mut meshes: Mut<Assets<Mesh>>| {
            world.resource_scope(|world, mut mats: Mut<Assets<StandardMaterial>>| {
                world.resource_scope(|world, mut imgs: Mut<Assets<Image>>| {
                    let mut pal = WorldPalette::new(&mut meshes);
                    let mut cmds = world.commands();
                    spawn_seasons_and_weather(&mut cmds, &mut pal, &mut mats, &mut imgs, &mut fonts, &spec, &life);
                    world.insert_resource(pal);
                });
            });
        });
        app.world_mut().flush();

        // Fixed rain pool capped at RAIN_POOL_CAP, all outside office floor bounds
        let drops: Vec<(f32, f32)> = app
            .world_mut()
            .query::<&RainDrop>()
            .iter(app.world())
            .map(|r| (r.x, r.z))
            .collect();
        assert_eq!(drops.len(), RAIN_POOL_CAP);
        for (x, z) in drops {
            assert!(!spec.inside(x, z, -0.2), "raindrop ({x},{z}) must fall outside office floor");
        }
        assert!(app.world_mut().query::<&UmbrellaStandPart>().iter(app.world()).count() >= 2);
        assert!(app.world_mut().query::<&WetFloorKitPart>().iter(app.world()).count() >= 3);

        // Start heavy rain -> wet floor cone + raindrops visible + NavGraph wet strip active
        app.world_mut().resource_mut::<OfficeLifeState>().apply_open_meteo(95, 26.0);
        app.world_mut().resource_mut::<Time<()>>().advance_by(std::time::Duration::from_millis(200));
        app.update();

        let vis_drops = app
            .world_mut()
            .query_filtered::<&Visibility, With<RainDrop>>()
            .iter(app.world())
            .filter(|v| **v == Visibility::Visible)
            .count();
        assert_eq!(vis_drops, RAIN_POOL_CAP);
        let vis_wet = app
            .world_mut()
            .query_filtered::<&Visibility, With<WetFloorKitPart>>()
            .iter(app.world())
            .filter(|v| **v == Visibility::Visible)
            .count();
        assert!(vis_wet >= 3);
        assert!(app.world().resource::<WeatherVisualState>().wet_strip_active);

        // Clear sky -> hides rain & wet kit, clears wet strip
        app.world_mut().resource_mut::<OfficeLifeState>().apply_open_meteo(0, 32.0);
        app.world_mut().resource_mut::<Time<()>>().advance_by(std::time::Duration::from_millis(200));
        app.update();
        let vis_drops_after = app
            .world_mut()
            .query_filtered::<&Visibility, With<RainDrop>>()
            .iter(app.world())
            .filter(|v| **v == Visibility::Visible)
            .count();
        assert_eq!(vis_drops_after, 0);
        assert!(!app.world().resource::<WeatherVisualState>().wet_strip_active);
    }

    #[test]
    fn all_four_seasons_spawn_decor_and_purge_with_zero_leaks_across_layouts() {
        let assets_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../assets");
        let mut fonts = FontEngine::from_assets_dir(&assets_dir);
        for layout in [OfficeLayout::Cozy, OfficeLayout::Pro, OfficeLayout::Hq] {
            let spec = LayoutSpec::for_layout(layout);
            for season in [
                SeasonTheme::Halloween,
                SeasonTheme::Christmas,
                SeasonTheme::Tet,
                SeasonTheme::Normal,
            ] {
                let mut world = World::new();
                world.insert_resource(Assets::<Mesh>::default());
                world.insert_resource(Assets::<StandardMaterial>::default());
                world.insert_resource(Assets::<Image>::default());
                let mut life = OfficeLifeState::default();
                life.season = season;
                world.resource_scope(|world, mut meshes: Mut<Assets<Mesh>>| {
                    world.resource_scope(|world, mut mats: Mut<Assets<StandardMaterial>>| {
                        world.resource_scope(|world, mut imgs: Mut<Assets<Image>>| {
                            let mut pal = WorldPalette::new(&mut meshes);
                            let mut cmds = world.commands();
                            spawn_seasons_and_weather(&mut cmds, &mut pal, &mut mats, &mut imgs, &mut fonts, &spec, &life);
                            world.insert_resource(pal);
                        });
                    });
                });
                world.flush();
                let decor = world.query::<&SeasonDecorPart>().iter(&world).count();
                if season != SeasonTheme::Normal {
                    assert!(decor > 0, "expected seasonal decor for {season:?} in {layout:?}");
                }
                let ents: Vec<Entity> = world.query_filtered::<Entity, With<WorldEntity>>().iter(&world).collect();
                for e in ents {
                    world.despawn(e);
                }
                let mut pal = world.remove_resource::<WorldPalette>().unwrap();
                world.resource_scope(|world, mut meshes: Mut<Assets<Mesh>>| {
                    world.resource_scope(|world, mut mats: Mut<Assets<StandardMaterial>>| {
                        world.resource_scope(|_world, mut imgs: Mut<Assets<Image>>| {
                            pal.purge_all(&mut meshes, &mut mats, &mut imgs);
                        });
                    });
                });
                assert_eq!(world.resource::<Assets<Mesh>>().len(), 0);
                assert_eq!(world.resource::<Assets<StandardMaterial>>().len(), 0);
                assert_eq!(world.resource::<Assets<Image>>().len(), 0);
            }
        }
    }
}
