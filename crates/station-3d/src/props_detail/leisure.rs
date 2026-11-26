//! Detailed leisure props matching Golden Oracle `src/office/world/props/`:
//! - `tv.js` (`WallTv`)
//! - `game-tv.js` + `tv-games.js` (`GameTv`, 256×144 live pixel games + dynamic ambilight `PointLight`)
//! - `arcade.js` (`ArcadeCabinet`, 128×96 live in-place Space Invaders + coin insert)
//! - `billiards.js` (`BilliardsTable`, 16 numbered balls, 6 pockets, cue stand + chalk + break physics)
//! - `karaoke.js` (`KaraokeBooth`, hinged glass door, bench, dual speakers, 2 mics, headphones, scrolling lyric bar)

use crate::interact::{InteractiveProp, PropActivated};
use crate::layout::LayoutSpec;
use crate::lights::LightingState;
use crate::palette::{WorldEntity, WorldPalette};
use bevy::prelude::*;
use station_canvas2d::office_surfaces::{
    TvStatsInput, draw_arcade_marquee, draw_billiards_ball, draw_billiards_felt, draw_karaoke_band,
    draw_karaoke_qr_badge, draw_karaoke_screen, draw_radial_glow, draw_stats_tv,
};
use station_canvas2d::{
    ARCADE_H, ARCADE_W, ArcadeScreenSim, FontEngine, TV_H, TV_W, TvGameKind, TvGamesSim,
};
use std::f32::consts::{FRAC_PI_2, FRAC_PI_4, PI};

/* ---------- Components ---------- */

#[derive(Component)]
pub struct WallTv {
    pub screen_mat: Handle<StandardMaterial>,
}

#[derive(Component)]
pub struct GameTv {
    pub sim: TvGamesSim,
    pub screen_image: Handle<Image>,
    pub ambilight: Entity,
    pub glow_mat: Handle<StandardMaterial>,
    pub router_led_mat: Handle<StandardMaterial>,
    pub accum: f32,
    pub smooth_rgb: Vec3,
}

#[derive(Component)]
pub struct GameTvAmbilight;

#[derive(Component)]
pub struct ArcadeCabinet {
    pub sim: ArcadeScreenSim,
    pub screen_image: Handle<Image>,
    pub screen_mat: Handle<StandardMaterial>,
    pub marquee_mat: Handle<StandardMaterial>,
    pub accum: f32,
}

#[derive(Component)]
pub struct BilliardsTable {
    pub active: bool,
    pub rack_cooldown: f32,
}

#[derive(Component, Debug, Clone, Copy)]
pub struct BilliardsBall {
    pub number: u8,
    pub pos: Vec2,
    pub vel: Vec2,
    pub pocketed: bool,
}

#[derive(Component, Debug, Clone, Copy)]
pub struct BilliardsPocket;

#[derive(Component)]
pub struct KaraokeBooth {
    pub door_pivot: Entity,
    pub door_open: f32,
    pub target_open: f32,
    pub led_mat: Handle<StandardMaterial>,
    pub ceil_mat: Handle<StandardMaterial>,
}

#[derive(Component)]
pub struct KaraokeDoorPivot;

#[derive(Component)]
pub struct KaraokeMic;

#[derive(Component)]
pub struct KaraokeSpeaker;

#[derive(Component)]
pub struct KaraokeBench;

#[derive(Component)]
pub struct KaraokeLyricBar {
    pub phase: f32,
}

/* ---------- Billiards constants (exact Oracle billiards.js) ---------- */

const BIL_PX: f32 = 0.47;
const BIL_PZ: f32 = 0.93;
const BIL_CLOTH_Y: f32 = 0.79;
const BIL_BALL_R: f32 = 0.025;
const BIL_CUSH: f32 = 0.04;
const BIL_RAIL: f32 = 0.085;
const BIL_OX: f32 = BIL_PX + BIL_CUSH + BIL_RAIL;
const BIL_OZ: f32 = BIL_PZ + BIL_CUSH + BIL_RAIL;
const BIL_RAIL_Y: f32 = BIL_CLOTH_Y + 0.045;

const POCKET_SPECS: [(f32, f32, f32); 6] = [
    (-(BIL_PX + 0.012), -(BIL_PZ + 0.012), 0.056),
    (BIL_PX + 0.012, -(BIL_PZ + 0.012), 0.056),
    (-(BIL_PX + 0.03), 0.0, 0.050),
    (BIL_PX + 0.03, 0.0, 0.050),
    (-(BIL_PX + 0.012), BIL_PZ + 0.012, 0.056),
    (BIL_PX + 0.012, BIL_PZ + 0.012, 0.056),
];

fn racked_ball_pos(number: u8) -> Vec2 {
    if number == 0 {
        return Vec2::new(0.0, 0.50);
    }
    // Classic 5-row triangle rack at foot spot (z = -0.44), 8-ball centered in row 3
    let order: [u8; 15] = [1, 9, 2, 10, 8, 3, 11, 4, 12, 5, 13, 6, 14, 7, 15];
    let slot = order.iter().position(|&n| n == number).unwrap_or(0);
    let mut rem = slot;
    let mut row = 0usize;
    while rem > row {
        rem -= row + 1;
        row += 1;
    }
    let gap = BIL_BALL_R * 2.04;
    let x = (rem as f32 - (row as f32) * 0.5) * gap;
    let z = -0.44 - (row as f32) * gap * 0.866;
    Vec2::new(x, z)
}

/* ---------- Spawning ---------- */

pub fn spawn_leisure_props(
    commands: &mut Commands,
    palette: &mut WorldPalette,
    materials: &mut Assets<StandardMaterial>,
    images: &mut Assets<Image>,
    fonts: &mut FontEngine,
    spec: &LayoutSpec,
) {
    spawn_wall_tv(commands, palette, materials, images, fonts, spec);
    spawn_arcade(commands, palette, materials, images, fonts, spec);
    if let Some([gx, gz]) = spec.game_tv {
        spawn_game_tv(commands, palette, materials, images, gx, gz);
    }
    if let Some([bx, bz]) = spec.bil {
        spawn_billiards(commands, palette, materials, images, fonts, bx, bz);
    }
    if let Some([kx, kz, kry]) = spec.ktv {
        spawn_karaoke_booth(commands, palette, materials, images, fonts, kx, kz, kry);
    }
}

fn spawn_wall_tv(
    commands: &mut Commands,
    palette: &mut WorldPalette,
    materials: &mut Assets<StandardMaterial>,
    images: &mut Assets<Image>,
    fonts: &mut FontEngine,
    spec: &LayoutSpec,
) {
    let [tx, ty, tz, try_] = spec.tv;
    let rot = Quat::from_rotation_y(try_);
    let bezel = palette.mat(materials, "#15181E", 0.4, 0.15);
    let tv_surf = draw_stats_tv(
        &TvStatsInput {
            title: "AGENTS STATION · LIVE".into(),
            clock_hhmm: "16:20".into(),
            ram_pct: Some(38),
            ram_critical: false,
            cpu_pct: 14,
            working: 4,
            waiting: 1,
            resting: 2,
            today_summary: "72 prompts · 34 cmds".into(),
            plan_pct: Some(52),
            pulse_30m: [
                2, 3, 1, 4, 6, 5, 3, 2, 7, 8, 4, 3, 5, 6, 8, 9, 7, 5, 3, 4, 6, 7, 8, 5, 4, 3, 6, 8,
                7, 5,
            ],
        },
        fonts,
    );
    let screen_mat = palette.textured_unlit(images, materials, tv_surf);

    commands
        .spawn((
            WorldEntity,
            WallTv {
                screen_mat: screen_mat.clone(),
            },
            InteractiveProp::new("tv", "server", "Live Machine Stats TV"),
            Transform::from_xyz(tx, ty, tz).with_rotation(rot),
            Visibility::default(),
        ))
        .with_children(|g| {
            g.spawn((
                WorldEntity,
                Mesh3d(palette.unit_box.clone()),
                MeshMaterial3d(bezel),
                Transform::from_scale(Vec3::new(2.32, 1.34, 0.08)),
            ));
            g.spawn((
                WorldEntity,
                Mesh3d(palette.unit_plane.clone()),
                MeshMaterial3d(screen_mat),
                Transform::from_xyz(0.0, 0.0, 0.046).with_scale(Vec3::new(2.18, 1.2, 1.0)),
            ));
        });
}

fn spawn_arcade(
    commands: &mut Commands,
    palette: &mut WorldPalette,
    materials: &mut Assets<StandardMaterial>,
    images: &mut Assets<Image>,
    fonts: &mut FontEngine,
    spec: &LayoutSpec,
) {
    let [ax, az] = spec.arc;
    let cab_m = palette.mat(materials, "#4B2E83", 0.55, 0.05);
    let trim_m = palette.emissive(materials, "#F72585", 1.25);
    let deck_m = palette.mat(materials, "#2A1B4F", 0.5, 0.05);
    let shaft_m = palette.mat(materials, "#222222", 0.4, 0.4);
    let knob_m = palette.mat(materials, "#E63946", 0.3, 0.0);

    let mut sim = ArcadeScreenSim::new();
    sim.draw_arcade_frame(1.8, false);
    let (screen_image, screen_mat) = palette.dynamic_pixel_screen(
        images,
        materials,
        ARCADE_W as u32,
        ARCADE_H as u32,
        sim.rgba(),
    );
    let marquee_mat = palette.textured_unlit(images, materials, draw_arcade_marquee(fonts));

    commands
        .spawn((
            WorldEntity,
            ArcadeCabinet {
                sim,
                screen_image,
                screen_mat: screen_mat.clone(),
                marquee_mat: marquee_mat.clone(),
                accum: 0.0,
            },
            InteractiveProp::new("arcade", "lounge", "Arcade Cabinet"),
            Transform::from_xyz(ax, 0.0, az),
            Visibility::default(),
        ))
        .with_children(|g| {
            // Main cabinet + neon side strips
            g.spawn((
                WorldEntity,
                Mesh3d(palette.unit_box.clone()),
                MeshMaterial3d(cab_m),
                Transform::from_xyz(0.0, 0.875, 0.0).with_scale(Vec3::new(0.8, 1.75, 0.7)),
            ));
            for sx in [-0.405_f32, 0.405] {
                g.spawn((
                    WorldEntity,
                    Mesh3d(palette.unit_box.clone()),
                    MeshMaterial3d(trim_m.clone()),
                    Transform::from_xyz(sx, 0.95, 0.0).with_scale(Vec3::new(0.02, 1.5, 0.5)),
                ));
            }
            // Angled control deck + joystick + 3 buttons (`arcade.js:44-47`)
            g.spawn((
                WorldEntity,
                Mesh3d(palette.unit_box.clone()),
                MeshMaterial3d(deck_m),
                Transform::from_xyz(0.0, 0.98, 0.42)
                    .with_rotation(Quat::from_rotation_x(0.25))
                    .with_scale(Vec3::new(0.76, 0.1, 0.32)),
            ));
            g.spawn((
                WorldEntity,
                Mesh3d(palette.unit_cyl.clone()),
                MeshMaterial3d(shaft_m),
                Transform::from_xyz(-0.18, 1.06, 0.44).with_scale(Vec3::new(0.024, 0.10, 0.024)),
            ));
            g.spawn((
                WorldEntity,
                Mesh3d(palette.unit_sphere.clone()),
                MeshMaterial3d(knob_m),
                Transform::from_xyz(-0.18, 1.12, 0.44).with_scale(Vec3::splat(0.07)),
            ));
            for (k, hex) in [(0, "#FFD166"), (1, "#4CC9F0"), (2, "#7CFC9A")] {
                let bm = palette.mat(materials, hex, 0.35, 0.0);
                g.spawn((
                    WorldEntity,
                    Mesh3d(palette.unit_cyl.clone()),
                    MeshMaterial3d(bm),
                    Transform::from_xyz(0.04 + (k as f32) * 0.08, 1.05, 0.46)
                        .with_scale(Vec3::new(0.05, 0.02, 0.05)),
                ));
            }
            // Live nearest-neighbor invaders screen + glowing marquee (`arcade.js:56-60`)
            g.spawn((
                WorldEntity,
                Mesh3d(palette.unit_plane.clone()),
                MeshMaterial3d(screen_mat),
                Transform::from_xyz(0.0, 1.38, 0.356)
                    .with_rotation(Quat::from_rotation_x(-0.18))
                    .with_scale(Vec3::new(0.58, 0.44, 1.0)),
            ));
            g.spawn((
                WorldEntity,
                Mesh3d(palette.unit_plane.clone()),
                MeshMaterial3d(marquee_mat),
                Transform::from_xyz(0.0, 1.66, 0.358).with_scale(Vec3::new(0.74, 0.19, 1.0)),
            ));
        });
}

fn spawn_game_tv(
    commands: &mut Commands,
    palette: &mut WorldPalette,
    materials: &mut Assets<StandardMaterial>,
    images: &mut Assets<Image>,
    gx: f32,
    gz: f32,
) {
    let walnut = palette.mat(materials, "#6B4A33", 0.6, 0.0);
    let dark = palette.mat(materials, "#2A2420", 0.7, 0.0);
    let oak = palette.mat(materials, "#B58A60", 0.55, 0.0);
    let black = palette.mat(materials, "#16181C", 0.35, 0.2);
    let steel = palette.mat(materials, "#2C3036", 0.4, 0.5);
    let white = palette.mat(materials, "#F4F5F7", 0.35, 0.0);
    let router_m = palette.mat(materials, "#F4F4F2", 0.5, 0.0);
    let router_led_mat = palette.owned_emissive(materials, "#3BD15A", 1.3);
    let con_led = palette.emissive(materials, "#8EC5FF", 1.8);
    let bar_m = palette.mat(materials, "#22252B", 0.5, 0.0);
    let grille_m = palette.mat(materials, "#3A3E46", 0.9, 0.0);
    let pot_m = palette.mat(materials, "#E8E2D6", 0.6, 0.0);
    let leaf_m = palette.mat(materials, "#4E8B4A", 0.8, 0.0);
    let pad_light = palette.mat(materials, "#F2F3F5", 0.45, 0.0);
    let pad_dark = palette.mat(materials, "#1F2329", 0.45, 0.0);
    let pad_trim = palette.mat(materials, "#1C1F25", 0.5, 0.0);
    let pad_bar = palette.emissive(materials, "#5FA8FF", 1.1);
    let power_led = palette.emissive(materials, "#3DDC84", 1.2);

    let mut sim = TvGamesSim::new(TvGameKind::Football, 2026);
    sim.step(0.8);
    let dom = sim.dominant_rgb();
    let (screen_image, screen_mat) = palette.dynamic_pixel_screen(
        images,
        materials,
        TV_W as u32,
        TV_H as u32,
        sim.rgba(),
    );

    // Soft additive wall glow behind TV (`game-tv.js:69-74`)
    let glow_tex = palette.upload_surface(images, draw_radial_glow());
    let glow_mat = palette.textured_additive_unlit(
        materials,
        glow_tex,
        Color::srgba(dom[0], dom[1], dom[2], 0.42),
    );

    let tv_y0 = 0.995_f32;
    let ambilight_ent = commands
        .spawn((
            WorldEntity,
            GameTvAmbilight,
            PointLight {
                color: Color::srgb(dom[0], dom[1], dom[2]),
                intensity: 1_500.0,
                range: 5.5,
                shadows_enabled: false,
                ..default()
            },
            Transform::from_xyz(gx, tv_y0 + 0.08, gz - 0.14),
        ))
        .id();

    let root = commands
        .spawn((
            WorldEntity,
            GameTv {
                sim,
                screen_image,
                ambilight: ambilight_ent,
                glow_mat: glow_mat.clone(),
                router_led_mat: router_led_mat.clone(),
                accum: 0.0,
                smooth_rgb: Vec3::new(dom[0], dom[1], dom[2]),
            },
            InteractiveProp::new("game-tv", "lounge", "Lounge Console TV"),
            Transform::from_xyz(gx, 0.0, gz),
            Visibility::default(),
        ))
        .id();

    commands.entity(root).with_children(|g| {
        // 4 steel legs + walnut media unit boards + open cubbies + oak top (`game-tv.js:33-45`)
        for sx in [-1.0_f32, 1.0] {
            for sz in [-1.0_f32, 1.0] {
                g.spawn((
                    WorldEntity,
                    Mesh3d(palette.unit_cyl.clone()),
                    MeshMaterial3d(steel.clone()),
                    Transform::from_xyz(sx * 0.82, 0.04, sz * 0.16)
                        .with_scale(Vec3::new(0.032, 0.08, 0.032)),
                ));
            }
        }
        g.spawn((
            WorldEntity,
            Mesh3d(palette.unit_box.clone()),
            MeshMaterial3d(walnut.clone()),
            Transform::from_xyz(0.0, 0.095, 0.0).with_scale(Vec3::new(1.8, 0.03, 0.42)),
        ));
        g.spawn((
            WorldEntity,
            Mesh3d(palette.unit_box.clone()),
            MeshMaterial3d(walnut.clone()),
            Transform::from_xyz(0.0, 0.29, -0.20).with_scale(Vec3::new(1.8, 0.42, 0.02)),
        ));
        for sx in [-1.0_f32, 1.0] {
            g.spawn((
                WorldEntity,
                Mesh3d(palette.unit_box.clone()),
                MeshMaterial3d(walnut.clone()),
                Transform::from_xyz(sx * 0.885, 0.29, 0.0).with_scale(Vec3::new(0.03, 0.42, 0.42)),
            ));
            g.spawn((
                WorldEntity,
                Mesh3d(palette.unit_box.clone()),
                MeshMaterial3d(walnut.clone()),
                Transform::from_xyz(sx * 0.30, 0.30, 0.0).with_scale(Vec3::new(0.03, 0.39, 0.40)),
            ));
            // Slatted side doors + steel handles
            g.spawn((
                WorldEntity,
                Mesh3d(palette.unit_box.clone()),
                MeshMaterial3d(walnut.clone()),
                Transform::from_xyz(sx * 0.5925, 0.30, 0.20)
                    .with_scale(Vec3::new(0.555, 0.38, 0.02)),
            ));
            for i in 0..9 {
                g.spawn((
                    WorldEntity,
                    Mesh3d(palette.unit_box.clone()),
                    MeshMaterial3d(oak.clone()),
                    Transform::from_xyz(sx * (0.36 + (i as f32) * 0.058), 0.30, 0.215)
                        .with_scale(Vec3::new(0.035, 0.36, 0.012)),
                ));
            }
            g.spawn((
                WorldEntity,
                Mesh3d(palette.unit_box.clone()),
                MeshMaterial3d(steel.clone()),
                Transform::from_xyz(sx * 0.335, 0.30, 0.228)
                    .with_scale(Vec3::new(0.012, 0.09, 0.012)),
            ));
        }
        g.spawn((
            WorldEntity,
            Mesh3d(palette.unit_box.clone()),
            MeshMaterial3d(oak),
            Transform::from_xyz(0.0, 0.515, 0.0).with_scale(Vec3::new(1.82, 0.03, 0.44)),
        ));
        g.spawn((
            WorldEntity,
            Mesh3d(palette.unit_box.clone()),
            MeshMaterial3d(dark),
            Transform::from_xyz(0.0, 0.30, -0.183).with_scale(Vec3::new(0.57, 0.39, 0.01)),
        ));
        g.spawn((
            WorldEntity,
            Mesh3d(palette.unit_box.clone()),
            MeshMaterial3d(walnut),
            Transform::from_xyz(0.0, 0.30, 0.0).with_scale(Vec3::new(0.57, 0.02, 0.38)),
        ));

        // Router + blinking LED + 4 standing game boxes (`game-tv.js:46-49`)
        g.spawn((
            WorldEntity,
            Mesh3d(palette.unit_box.clone()),
            MeshMaterial3d(router_m),
            Transform::from_xyz(-0.06, 0.13, 0.0).with_scale(Vec3::new(0.22, 0.04, 0.15)),
        ));
        g.spawn((
            WorldEntity,
            Mesh3d(palette.unit_box.clone()),
            MeshMaterial3d(router_led_mat),
            Transform::from_xyz(-0.06, 0.135, 0.077).with_scale(Vec3::new(0.06, 0.008, 0.004)),
        ));
        for (i, hex) in ["#2B6CB0", "#C53030", "#2F855A", "#E0A526"]
            .into_iter()
            .enumerate()
        {
            let bm = palette.mat(materials, hex, 0.6, 0.0);
            let rot = if i == 3 {
                Quat::from_rotation_z(0.18)
            } else {
                Quat::IDENTITY
            };
            g.spawn((
                WorldEntity,
                Mesh3d(palette.unit_box.clone()),
                MeshMaterial3d(bm),
                Transform::from_xyz(-0.20 + (i as f32) * 0.026, 0.396, 0.02)
                    .with_rotation(rot)
                    .with_scale(Vec3::new(0.022, 0.17, 0.13)),
            ));
        }

        // TV pedestal, bezel, back, power LED, live 256×144 screen, wall glow (`game-tv.js:52-74`)
        g.spawn((
            WorldEntity,
            Mesh3d(palette.unit_box.clone()),
            MeshMaterial3d(steel.clone()),
            Transform::from_xyz(0.0, 0.536, -0.02).with_scale(Vec3::new(0.34, 0.012, 0.20)),
        ));
        g.spawn((
            WorldEntity,
            Mesh3d(palette.unit_box.clone()),
            MeshMaterial3d(steel),
            Transform::from_xyz(0.0, 0.60, -0.07).with_scale(Vec3::new(0.07, 0.11, 0.03)),
        ));
        g.spawn((
            WorldEntity,
            Mesh3d(palette.unit_box.clone()),
            MeshMaterial3d(black.clone()),
            Transform::from_xyz(0.0, tv_y0, -0.06).with_scale(Vec3::new(1.25, 0.72, 0.03)),
        ));
        g.spawn((
            WorldEntity,
            Mesh3d(palette.unit_box.clone()),
            MeshMaterial3d(black.clone()),
            Transform::from_xyz(0.0, tv_y0, -0.095).with_scale(Vec3::new(1.12, 0.59, 0.04)),
        ));
        g.spawn((
            WorldEntity,
            Mesh3d(palette.unit_plane.clone()),
            MeshMaterial3d(screen_mat),
            Transform::from_xyz(0.0, tv_y0, -0.043).with_scale(Vec3::new(1.22, 0.69, 1.0)),
        ));
        g.spawn((
            WorldEntity,
            Mesh3d(palette.unit_box.clone()),
            MeshMaterial3d(power_led),
            Transform::from_xyz(0.50, tv_y0 - 0.354, -0.043)
                .with_scale(Vec3::new(0.014, 0.006, 0.004)),
        ));
        g.spawn((
            WorldEntity,
            Mesh3d(palette.unit_plane.clone()),
            MeshMaterial3d(glow_mat),
            Transform::from_xyz(0.0, tv_y0 + 0.05, -0.195).with_scale(Vec3::new(2.3, 1.45, 1.0)),
        ));

        // Standing console (black core, 2 white wings, blue LED, base, `game-tv.js:77-82`)
        g.spawn((
            WorldEntity,
            Mesh3d(palette.unit_box.clone()),
            MeshMaterial3d(black.clone()),
            Transform::from_xyz(0.74, 0.73, -0.04).with_scale(Vec3::new(0.10, 0.38, 0.25)),
        ));
        for sx in [-1.0_f32, 1.0] {
            g.spawn((
                WorldEntity,
                Mesh3d(palette.unit_box.clone()),
                MeshMaterial3d(white.clone()),
                Transform::from_xyz(0.74 + sx * 0.055, 0.74, -0.04)
                    .with_scale(Vec3::new(0.02, 0.40, 0.27)),
            ));
        }
        g.spawn((
            WorldEntity,
            Mesh3d(palette.unit_box.clone()),
            MeshMaterial3d(con_led),
            Transform::from_xyz(0.808, 0.75, 0.08).with_scale(Vec3::new(0.006, 0.30, 0.006)),
        ));
        g.spawn((
            WorldEntity,
            Mesh3d(palette.unit_box.clone()),
            MeshMaterial3d(black),
            Transform::from_xyz(0.74, 0.54, -0.04).with_scale(Vec3::new(0.16, 0.02, 0.14)),
        ));

        // Soundbar + grille + ceramic pot plant (`game-tv.js:83-86`)
        g.spawn((
            WorldEntity,
            Mesh3d(palette.unit_box.clone()),
            MeshMaterial3d(bar_m),
            Transform::from_xyz(0.0, 0.56, 0.13).with_scale(Vec3::new(0.86, 0.06, 0.09)),
        ));
        g.spawn((
            WorldEntity,
            Mesh3d(palette.unit_box.clone()),
            MeshMaterial3d(grille_m),
            Transform::from_xyz(0.0, 0.56, 0.176).with_scale(Vec3::new(0.80, 0.045, 0.004)),
        ));
        g.spawn((
            WorldEntity,
            Mesh3d(palette.unit_cyl.clone()),
            MeshMaterial3d(pot_m),
            Transform::from_xyz(-0.78, 0.60, 0.02).with_scale(Vec3::new(0.15, 0.15, 0.15)),
        ));
        for i in 0..7 {
            let a = (i as f32) * 0.9;
            g.spawn((
                WorldEntity,
                Mesh3d(palette.unit_sphere.clone()),
                MeshMaterial3d(leaf_m.clone()),
                Transform::from_xyz(
                    -0.78 + a.cos() * 0.05,
                    0.72 + ((i % 3) as f32) * 0.04,
                    0.02 + a.sin() * 0.05,
                )
                .with_rotation(Quat::from_rotation_y(a))
                .with_scale(Vec3::new(0.09, 0.05, 0.15)),
            ));
        }

        // Two detailed gamepads on unit (`game-tv.js:14-23, 89-90`)
        for (px, pz, ry, body_m) in [
            (-0.57_f32, 0.11_f32, 0.30_f32, pad_light),
            (0.55, 0.15, -0.22, pad_dark),
        ] {
            g.spawn((
                WorldEntity,
                Transform::from_xyz(px, 0.53, pz).with_rotation(Quat::from_rotation_y(ry)),
                Visibility::default(),
            ))
            .with_children(|p| {
                p.spawn((
                    WorldEntity,
                    Mesh3d(palette.unit_box.clone()),
                    MeshMaterial3d(body_m.clone()),
                    Transform::from_xyz(0.0, 0.016, 0.0).with_scale(Vec3::new(0.15, 0.03, 0.068)),
                ));
                for sx in [-1.0_f32, 1.0] {
                    p.spawn((
                        WorldEntity,
                        Mesh3d(palette.unit_sphere.clone()),
                        MeshMaterial3d(body_m.clone()),
                        Transform::from_xyz(sx * 0.058, 0.016, 0.03)
                            .with_scale(Vec3::new(0.05, 0.03, 0.075)),
                    ));
                    p.spawn((
                        WorldEntity,
                        Mesh3d(palette.unit_cyl.clone()),
                        MeshMaterial3d(pad_trim.clone()),
                        Transform::from_xyz(sx * 0.032, 0.034, 0.012)
                            .with_scale(Vec3::new(0.022, 0.008, 0.022)),
                    ));
                }
                p.spawn((
                    WorldEntity,
                    Mesh3d(palette.unit_box.clone()),
                    MeshMaterial3d(pad_trim.clone()),
                    Transform::from_xyz(0.0, 0.032, -0.01)
                        .with_scale(Vec3::new(0.05, 0.004, 0.026)),
                ));
                p.spawn((
                    WorldEntity,
                    Mesh3d(palette.unit_box.clone()),
                    MeshMaterial3d(pad_bar.clone()),
                    Transform::from_xyz(0.0, 0.026, -0.034)
                        .with_scale(Vec3::new(0.044, 0.004, 0.005)),
                ));
            });
        }
    });
}

fn spawn_billiards(
    commands: &mut Commands,
    palette: &mut WorldPalette,
    materials: &mut Assets<StandardMaterial>,
    images: &mut Assets<Image>,
    fonts: &mut FontEngine,
    bx: f32,
    bz: f32,
) {
    let walnut = palette.mat(materials, "#5A3A26", 0.45, 0.0);
    let body_m = palette.mat(materials, "#3E281B", 0.55, 0.0);
    let leather = palette.mat(materials, "#1F1612", 0.6, 0.0);
    let cush_m = palette.mat(materials, "#256B43", 0.85, 0.0);
    let black = palette.mat(materials, "#060607", 0.9, 0.0);
    let pearl = palette.mat(materials, "#F3EBDD", 0.3, 0.2);
    let brass = palette.mat(materials, "#B8923E", 0.35, 0.7);
    let chalk_m = palette.mat(materials, "#3B6FB6", 0.8, 0.0);
    let rug_m = palette.mat(materials, "#7A2E2A", 0.92, 0.0);
    let felt_m = palette.textured_lit(images, materials, draw_billiards_felt(), 0.95);

    let tbl = commands
        .spawn((
            WorldEntity,
            BilliardsTable {
                active: false,
                rack_cooldown: 0.0,
            },
            InteractiveProp::new("billiards", "lounge", "Pool Table"),
            Transform::from_xyz(bx, 0.0, bz),
            Visibility::default(),
        ))
        .id();

    commands.entity(tbl).with_children(|g| {
        // Burgundy rug under pool table (`billiards.js:47`)
        g.spawn((
            WorldEntity,
            Mesh3d(palette.unit_plane.clone()),
            MeshMaterial3d(rug_m),
            Transform::from_xyz(0.0, 0.008, 0.0)
                .with_rotation(Quat::from_rotation_x(-FRAC_PI_2))
                .with_scale(Vec3::new(2.4, 3.2, 1.0)),
        ));
        // Apron, lip, 6 turned walnut legs + collars/feet, stretcher (`billiards.js:50-58`)
        g.spawn((
            WorldEntity,
            Mesh3d(palette.unit_box.clone()),
            MeshMaterial3d(body_m),
            Transform::from_xyz(0.0, BIL_CLOTH_Y - 0.10, 0.0)
                .with_scale(Vec3::new(BIL_OX * 2.0, 0.16, BIL_OZ * 2.0)),
        ));
        g.spawn((
            WorldEntity,
            Mesh3d(palette.unit_box.clone()),
            MeshMaterial3d(walnut.clone()),
            Transform::from_xyz(0.0, BIL_CLOTH_Y - 0.18, 0.0)
                .with_scale(Vec3::new(BIL_OX * 2.0 + 0.02, 0.025, BIL_OZ * 2.0 + 0.02)),
        ));
        for sx in [-1.0_f32, 1.0] {
            for z in [-1.0_f32, 0.0, 1.0] {
                let lx = sx * (BIL_OX - 0.10);
                let lz = z * (BIL_OZ - 0.12);
                g.spawn((
                    WorldEntity,
                    Mesh3d(palette.unit_cyl.clone()),
                    MeshMaterial3d(walnut.clone()),
                    Transform::from_xyz(lx, 0.33, lz).with_scale(Vec3::new(0.10, 0.50, 0.10)),
                ));
                g.spawn((
                    WorldEntity,
                    Mesh3d(palette.unit_cyl.clone()),
                    MeshMaterial3d(walnut.clone()),
                    Transform::from_xyz(lx, 0.56, lz).with_scale(Vec3::new(0.14, 0.06, 0.14)),
                ));
                g.spawn((
                    WorldEntity,
                    Mesh3d(palette.unit_cyl.clone()),
                    MeshMaterial3d(walnut.clone()),
                    Transform::from_xyz(lx, 0.035, lz).with_scale(Vec3::new(0.13, 0.07, 0.13)),
                ));
            }
        }
        g.spawn((
            WorldEntity,
            Mesh3d(palette.unit_box.clone()),
            MeshMaterial3d(walnut.clone()),
            Transform::from_xyz(0.0, 0.20, 0.0)
                .with_scale(Vec3::new(0.07, 0.07, BIL_OZ * 2.0 - 0.30)),
        ));

        // Green felt playing surface (`billiards.js:61-62`)
        g.spawn((
            WorldEntity,
            Mesh3d(palette.unit_plane.clone()),
            MeshMaterial3d(felt_m),
            Transform::from_xyz(0.0, BIL_CLOTH_Y, 0.0)
                .with_rotation(Quat::from_rotation_x(-FRAC_PI_2))
                .with_scale(Vec3::new(
                    BIL_PX * 2.0 + BIL_CUSH * 2.0,
                    BIL_PZ * 2.0 + BIL_CUSH * 2.0,
                    1.0,
                )),
        ));

        // Cushions & walnut rails broken at the 6 pockets (`billiards.js:63-73`)
        let gap_c = 0.07_f32;
        let gap_m = 0.055_f32;
        for sx in [-1.0_f32, 1.0] {
            for sz in [-1.0_f32, 1.0] {
                let len = (BIL_PZ - gap_c) - gap_m;
                let zc = sz * (gap_m + len * 0.5);
                g.spawn((
                    WorldEntity,
                    Mesh3d(palette.unit_box.clone()),
                    MeshMaterial3d(cush_m.clone()),
                    Transform::from_xyz(sx * (BIL_PX + BIL_CUSH * 0.5), BIL_CLOTH_Y + 0.018, zc)
                        .with_scale(Vec3::new(BIL_CUSH, 0.035, len)),
                ));
                g.spawn((
                    WorldEntity,
                    Mesh3d(palette.unit_box.clone()),
                    MeshMaterial3d(walnut.clone()),
                    Transform::from_xyz(
                        sx * (BIL_PX + BIL_CUSH + BIL_RAIL * 0.5),
                        BIL_RAIL_Y - 0.02,
                        zc,
                    )
                    .with_scale(Vec3::new(BIL_RAIL, 0.05, len + 0.05)),
                ));
            }
        }
        for sz in [-1.0_f32, 1.0] {
            let len = (BIL_PX - gap_c) * 2.0;
            g.spawn((
                WorldEntity,
                Mesh3d(palette.unit_box.clone()),
                MeshMaterial3d(cush_m.clone()),
                Transform::from_xyz(0.0, BIL_CLOTH_Y + 0.018, sz * (BIL_PZ + BIL_CUSH * 0.5))
                    .with_scale(Vec3::new(len, 0.035, BIL_CUSH)),
            ));
            g.spawn((
                WorldEntity,
                Mesh3d(palette.unit_box.clone()),
                MeshMaterial3d(walnut.clone()),
                Transform::from_xyz(
                    0.0,
                    BIL_RAIL_Y - 0.02,
                    sz * (BIL_PZ + BIL_CUSH + BIL_RAIL * 0.5),
                )
                .with_scale(Vec3::new(len + 0.05, 0.05, BIL_RAIL)),
            ));
        }

        // 6 dark pockets + leather hoods + brass lip (`billiards.js:77-83`)
        for (px, pz, pr) in POCKET_SPECS {
            g.spawn((
                WorldEntity,
                BilliardsPocket,
                Mesh3d(palette.unit_cyl.clone()),
                MeshMaterial3d(black.clone()),
                Transform::from_xyz(px, BIL_CLOTH_Y - 0.042, pz)
                    .with_scale(Vec3::new(pr * 2.0, 0.09, pr * 2.0)),
            ));
            g.spawn((
                WorldEntity,
                Mesh3d(palette.unit_torus.clone()),
                MeshMaterial3d(leather.clone()),
                Transform::from_xyz(px, BIL_RAIL_Y + 0.004, pz)
                    .with_scale(Vec3::new(pr * 2.7, 0.03, pr * 2.7)),
            ));
            g.spawn((
                WorldEntity,
                Mesh3d(palette.thin_ring.clone()),
                MeshMaterial3d(brass.clone()),
                Transform::from_xyz(px, BIL_RAIL_Y + 0.009, pz)
                    .with_scale(Vec3::new(pr * 2.1, 0.02, pr * 2.1)),
            ));
        }

        // Pearl diamonds on rails + 2 blue chalk cubes (`billiards.js:85-91`)
        for sx in [-1.0_f32, 1.0] {
            for k in [-3, -2, -1, 1, 2, 3] {
                g.spawn((
                    WorldEntity,
                    Mesh3d(palette.unit_box.clone()),
                    MeshMaterial3d(pearl.clone()),
                    Transform::from_xyz(
                        sx * (BIL_PX + BIL_CUSH + BIL_RAIL * 0.5),
                        BIL_RAIL_Y + 0.006,
                        (k as f32) * BIL_PZ * 0.25,
                    )
                    .with_rotation(Quat::from_rotation_y(FRAC_PI_4))
                    .with_scale(Vec3::new(0.016, 0.004, 0.016)),
                ));
            }
        }
        for sz in [-1.0_f32, 1.0] {
            for k in [-1, 0, 1] {
                g.spawn((
                    WorldEntity,
                    Mesh3d(palette.unit_box.clone()),
                    MeshMaterial3d(pearl.clone()),
                    Transform::from_xyz(
                        (k as f32) * BIL_PX * 0.5,
                        BIL_RAIL_Y + 0.006,
                        sz * (BIL_PZ + BIL_CUSH + BIL_RAIL * 0.5),
                    )
                    .with_rotation(Quat::from_rotation_y(FRAC_PI_4))
                    .with_scale(Vec3::new(0.016, 0.004, 0.016)),
                ));
            }
        }
        for (cx, cz) in [
            (BIL_PX + BIL_CUSH + BIL_RAIL * 0.5, -BIL_PZ * 0.62),
            (-(BIL_PX + BIL_CUSH + BIL_RAIL * 0.5), BIL_PZ * 0.62),
        ] {
            g.spawn((
                WorldEntity,
                Mesh3d(palette.unit_box.clone()),
                MeshMaterial3d(chalk_m.clone()),
                Transform::from_xyz(cx, BIL_RAIL_Y + 0.016, cz)
                    .with_scale(Vec3::new(0.022, 0.02, 0.022)),
            ));
        }

        // 16 numbered/striped balls (`billiards.js:94-98`)
        for n in 0..=15u8 {
            let p = racked_ball_pos(n);
            let ball_m = palette.textured_lit(images, materials, draw_billiards_ball(n, fonts), 0.18);
            g.spawn((
                WorldEntity,
                BilliardsBall {
                    number: n,
                    pos: p,
                    vel: Vec2::ZERO,
                    pocketed: false,
                },
                Mesh3d(palette.unit_sphere.clone()),
                MeshMaterial3d(ball_m),
                Transform::from_xyz(p.x, BIL_CLOTH_Y + BIL_BALL_R, p.y)
                    .with_scale(Vec3::splat(BIL_BALL_R * 2.0)),
            ));
        }
    });

    // Free-standing round walnut cue stand with 6 cues (`billiards.js:101-123`)
    let shaft_m = palette.mat(materials, "#E2C99A", 0.45, 0.0);
    let wrap_m = palette.mat(materials, "#1C1D21", 0.8, 0.0);
    let butt_a = palette.mat(materials, "#2B2F36", 0.4, 0.0);
    let butt_b = palette.mat(materials, "#7A4A2A", 0.4, 0.0);
    let ring_m = palette.mat(materials, "#C9A227", 0.3, 0.6);
    let fer_m = palette.mat(materials, "#F5F2EA", 0.4, 0.0);

    commands
        .spawn((
            WorldEntity,
            Transform::from_xyz(bx - 0.3, 0.0, bz + 2.25),
            Visibility::default(),
        ))
        .with_children(|r| {
            r.spawn((
                WorldEntity,
                Mesh3d(palette.unit_cyl.clone()),
                MeshMaterial3d(walnut.clone()),
                Transform::from_xyz(0.0, 0.03, 0.0).with_scale(Vec3::new(0.36, 0.06, 0.36)),
            ));
            r.spawn((
                WorldEntity,
                Mesh3d(palette.unit_cyl.clone()),
                MeshMaterial3d(walnut.clone()),
                Transform::from_xyz(0.0, 0.53, 0.0).with_scale(Vec3::new(0.05, 1.0, 0.05)),
            ));
            r.spawn((
                WorldEntity,
                Mesh3d(palette.unit_torus.clone()),
                MeshMaterial3d(walnut),
                Transform::from_xyz(0.0, 0.95, 0.0).with_scale(Vec3::new(0.24, 0.04, 0.24)),
            ));
            for k in 0..6 {
                let a = (k as f32 / 6.0) * std::f32::consts::TAU;
                let butt_m = if k % 2 == 1 { butt_b.clone() } else { butt_a.clone() };
                r.spawn((
                    WorldEntity,
                    Transform::from_xyz(a.sin() * 0.13, 0.06, a.cos() * 0.13)
                        .with_rotation(Quat::from_euler(EulerRot::XYZ, a.cos() * 0.02, 0.0, -a.sin() * 0.02)),
                    Visibility::default(),
                ))
                .with_children(|c| {
                    c.spawn((
                        WorldEntity,
                        Mesh3d(palette.unit_cyl.clone()),
                        MeshMaterial3d(butt_m),
                        Transform::from_xyz(0.0, 0.075, 0.0).with_scale(Vec3::new(0.036, 0.15, 0.036)),
                    ));
                    c.spawn((
                        WorldEntity,
                        Mesh3d(palette.unit_cyl.clone()),
                        MeshMaterial3d(wrap_m.clone()),
                        Transform::from_xyz(0.0, 0.30, 0.0).with_scale(Vec3::new(0.033, 0.30, 0.033)),
                    ));
                    c.spawn((
                        WorldEntity,
                        Mesh3d(palette.unit_cyl.clone()),
                        MeshMaterial3d(ring_m.clone()),
                        Transform::from_xyz(0.0, 0.455, 0.0).with_scale(Vec3::new(0.032, 0.012, 0.032)),
                    ));
                    c.spawn((
                        WorldEntity,
                        Mesh3d(palette.unit_cyl.clone()),
                        MeshMaterial3d(shaft_m.clone()),
                        Transform::from_xyz(0.0, 0.95, 0.0).with_scale(Vec3::new(0.024, 1.0, 0.024)),
                    ));
                    c.spawn((
                        WorldEntity,
                        Mesh3d(palette.unit_cyl.clone()),
                        MeshMaterial3d(fer_m.clone()),
                        Transform::from_xyz(0.0, 1.44, 0.0).with_scale(Vec3::new(0.018, 0.02, 0.018)),
                    ));
                    c.spawn((
                        WorldEntity,
                        Mesh3d(palette.unit_cyl.clone()),
                        MeshMaterial3d(chalk_m.clone()),
                        Transform::from_xyz(0.0, 1.454, 0.0).with_scale(Vec3::new(0.018, 0.008, 0.018)),
                    ));
                });
            }
        });
}

#[allow(clippy::too_many_arguments)]
fn spawn_karaoke_booth(
    commands: &mut Commands,
    palette: &mut WorldPalette,
    materials: &mut Assets<StandardMaterial>,
    images: &mut Assets<Image>,
    fonts: &mut FontEngine,
    kx: f32,
    kz: f32,
    kry: f32,
) {
    const HX: f32 = 0.60;
    const HZ: f32 = 0.62;
    const TOP: f32 = 2.20;
    const H: f32 = TOP - 0.07;
    const MID_Y: f32 = 0.07 + H * 0.5;
    const DW: f32 = 0.60;

    let frame = palette.mat(materials, "#1D1F26", 0.35, 0.5);
    let felt = palette.mat(materials, "#3B2A5C", 0.95, 0.0);
    let base = palette.mat(materials, "#24222A", 0.6, 0.0);
    let floor_m = palette.mat(materials, "#141418", 0.3, 0.2);
    let roof_cap = palette.mat(materials, "#E6DDF5", 0.6, 0.0);
    let glass = palette.glass(materials, "#D7E6F5", 0.14);
    let ceil_mat = palette.owned_emissive(materials, "#B98CFF", 1.3);
    let led_mat = palette.owned_emissive(materials, "#FF4FB0", 1.9);
    let bezel_m = palette.mat(materials, "#0C0D10", 0.3, 0.1);
    let spk_m = palette.mat(materials, "#17181C", 0.5, 0.0);
    let cone_m = palette.mat(materials, "#2C2E35", 0.4, 0.4);
    let shelf_m = palette.mat(materials, "#2A2530", 0.5, 0.0);
    let mic_body = palette.mat(materials, "#1C1D21", 0.5, 0.0);
    let mic_grille = palette.mat(materials, "#AEB4BC", 0.45, 0.6);
    let hp_m = palette.mat(materials, "#F2F2F4", 0.5, 0.0);
    let cushion_m = palette.mat(materials, "#C2477F", 0.85, 0.0);
    let chrome = palette.mat(materials, "#C9CDD3", 0.25, 0.7);
    let lyric_m = palette.emissive(materials, "#FF4FB0", 1.8);

    let band_m = palette.textured_unlit(images, materials, draw_karaoke_band("KARAOKE", fonts));
    let qr_m = palette.textured_unlit(images, materials, draw_karaoke_qr_badge(fonts));
    let ktv_scr = palette.textured_unlit(
        images,
        materials,
        draw_karaoke_screen("Agents Station KTV", "Lạc trôi giữa codebase Rust ♪", 0.45, fonts),
    );

    let mut door_pivot_id = Entity::PLACEHOLDER;
    let root = commands
        .spawn((
            WorldEntity,
            InteractiveProp::new("karaoke", "lounge", "Karaoke Booth"),
            Transform::from_xyz(kx, 0.0, kz).with_rotation(Quat::from_rotation_y(kry)),
            Visibility::default(),
        ))
        .with_children(|g| {
            // Base, interior floor, roof, lavender roof cap, ceiling panel (`karaoke.js:36-41`)
            g.spawn((
                WorldEntity,
                Mesh3d(palette.unit_box.clone()),
                MeshMaterial3d(base.clone()),
                Transform::from_xyz(0.0, 0.035, 0.0)
                    .with_scale(Vec3::new(HX * 2.0 + 0.06, 0.07, HZ * 2.0 + 0.06)),
            ));
            g.spawn((
                WorldEntity,
                Mesh3d(palette.unit_box.clone()),
                MeshMaterial3d(floor_m),
                Transform::from_xyz(0.0, 0.076, 0.0)
                    .with_scale(Vec3::new(HX * 2.0 - 0.08, 0.012, HZ * 2.0 - 0.08)),
            ));
            g.spawn((
                WorldEntity,
                Mesh3d(palette.unit_box.clone()),
                MeshMaterial3d(frame.clone()),
                Transform::from_xyz(0.0, TOP + 0.04, 0.0)
                    .with_scale(Vec3::new(HX * 2.0 + 0.08, 0.08, HZ * 2.0 + 0.08)),
            ));
            g.spawn((
                WorldEntity,
                Mesh3d(palette.unit_box.clone()),
                MeshMaterial3d(roof_cap),
                Transform::from_xyz(0.0, TOP + 0.085, 0.0)
                    .with_scale(Vec3::new(HX * 2.0 - 0.02, 0.012, HZ * 2.0 - 0.02)),
            ));
            g.spawn((
                WorldEntity,
                Mesh3d(palette.unit_box.clone()),
                MeshMaterial3d(ceil_mat.clone()),
                Transform::from_xyz(0.0, TOP - 0.006, 0.0)
                    .with_scale(Vec3::new(HX * 2.0 - 0.30, 0.012, HZ * 2.0 - 0.30)),
            ));
            // Perimeter magenta LED strips (`karaoke.js:43-46`)
            for y in [TOP + 0.003, 0.074] {
                for sz in [-1.0_f32, 1.0] {
                    g.spawn((
                        WorldEntity,
                        Mesh3d(palette.unit_box.clone()),
                        MeshMaterial3d(led_mat.clone()),
                        Transform::from_xyz(0.0, y, sz * (HZ + 0.045))
                            .with_scale(Vec3::new(HX * 2.0 + 0.09, 0.018, 0.012)),
                    ));
                }
                for sx in [-1.0_f32, 1.0] {
                    g.spawn((
                        WorldEntity,
                        Mesh3d(palette.unit_box.clone()),
                        MeshMaterial3d(led_mat.clone()),
                        Transform::from_xyz(sx * (HX + 0.045), y, 0.0)
                            .with_scale(Vec3::new(0.012, 0.018, HZ * 2.0 + 0.09)),
                    ));
                }
            }
            // 4 corner posts (`karaoke.js:47`)
            for sx in [-1.0_f32, 1.0] {
                for sz in [-1.0_f32, 1.0] {
                    g.spawn((
                        WorldEntity,
                        Mesh3d(palette.unit_box.clone()),
                        MeshMaterial3d(frame.clone()),
                        Transform::from_xyz(sx * HX, MID_Y, sz * HZ)
                            .with_scale(Vec3::new(0.06, H, 0.06)),
                    ));
                }
            }

            // Back acoustic felt wall + TV screen + scrolling lyric bar (`karaoke.js:50-54`)
            g.spawn((
                WorldEntity,
                Mesh3d(palette.unit_box.clone()),
                MeshMaterial3d(felt.clone()),
                Transform::from_xyz(0.0, MID_Y, -HZ + 0.03)
                    .with_scale(Vec3::new(HX * 2.0 - 0.06, H, 0.05)),
            ));
            g.spawn((
                WorldEntity,
                Mesh3d(palette.unit_box.clone()),
                MeshMaterial3d(bezel_m),
                Transform::from_xyz(0.0, 1.42, -HZ + 0.07).with_scale(Vec3::new(0.82, 0.50, 0.035)),
            ));
            g.spawn((
                WorldEntity,
                Mesh3d(palette.unit_plane.clone()),
                MeshMaterial3d(ktv_scr),
                Transform::from_xyz(0.0, 1.42, -HZ + 0.089).with_scale(Vec3::new(0.76, 0.43, 1.0)),
            ));
            g.spawn((
                WorldEntity,
                KaraokeLyricBar { phase: 0.0 },
                Mesh3d(palette.unit_box.clone()),
                MeshMaterial3d(lyric_m),
                Transform::from_xyz(-0.18, 1.29, -HZ + 0.093)
                    .with_scale(Vec3::new(0.28, 0.016, 0.004)),
            ));

            // Dual wall speakers (`karaoke.js:55-58`)
            for sx in [-1.0_f32, 1.0] {
                g.spawn((
                    WorldEntity,
                    KaraokeSpeaker,
                    Transform::from_xyz(sx * 0.49, 1.42, -HZ + 0.10),
                    Visibility::default(),
                ))
                .with_children(|sp| {
                    sp.spawn((
                        WorldEntity,
                        Mesh3d(palette.unit_box.clone()),
                        MeshMaterial3d(spk_m.clone()),
                        Transform::from_scale(Vec3::new(0.11, 0.32, 0.08)),
                    ));
                    for (cy, d) in [(0.08_f32, 0.052_f32), (-0.07, 0.080)] {
                        sp.spawn((
                            WorldEntity,
                            Mesh3d(palette.unit_cyl.clone()),
                            MeshMaterial3d(cone_m.clone()),
                            Transform::from_xyz(0.0, cy, 0.041)
                                .with_rotation(Quat::from_rotation_x(FRAC_PI_2))
                                .with_scale(Vec3::new(d, 0.01, d)),
                        ));
                    }
                });
            }

            // Shelf + 2 handheld mics (`karaoke.js:59-66`)
            g.spawn((
                WorldEntity,
                Mesh3d(palette.unit_box.clone()),
                MeshMaterial3d(shelf_m),
                Transform::from_xyz(0.0, 0.98, -HZ + 0.12).with_scale(Vec3::new(0.56, 0.03, 0.14)),
            ));
            for mx in [-0.12_f32, 0.12] {
                g.spawn((
                    WorldEntity,
                    KaraokeMic,
                    Transform::from_xyz(mx, 1.01, -HZ + 0.12)
                        .with_rotation(Quat::from_rotation_z(FRAC_PI_2 - 0.1)),
                    Visibility::default(),
                ))
                .with_children(|m| {
                    m.spawn((
                        WorldEntity,
                        Mesh3d(palette.unit_cyl.clone()),
                        MeshMaterial3d(mic_body.clone()),
                        Transform::from_scale(Vec3::new(0.030, 0.13, 0.030)),
                    ));
                    m.spawn((
                        WorldEntity,
                        Mesh3d(palette.unit_sphere.clone()),
                        MeshMaterial3d(mic_grille.clone()),
                        Transform::from_xyz(0.0, 0.09, 0.0).with_scale(Vec3::splat(0.062)),
                    ));
                });
            }

            // Hanging headphones on wall hook (`karaoke.js:67-70`)
            g.spawn((
                WorldEntity,
                Transform::from_xyz(-0.48, 1.0, -HZ + 0.08),
                Visibility::default(),
            ))
            .with_children(|hp| {
                hp.spawn((
                    WorldEntity,
                    Mesh3d(palette.unit_cyl.clone()),
                    MeshMaterial3d(frame.clone()),
                    Transform::from_xyz(0.0, 0.05, 0.0)
                        .with_rotation(Quat::from_rotation_x(FRAC_PI_2))
                        .with_scale(Vec3::new(0.016, 0.05, 0.016)),
                ));
                hp.spawn((
                    WorldEntity,
                    Mesh3d(palette.unit_torus.clone()),
                    MeshMaterial3d(hp_m.clone()),
                    Transform::from_xyz(0.0, 0.01, 0.04)
                        .with_rotation(Quat::from_rotation_x(FRAC_PI_2))
                        .with_scale(Vec3::new(0.14, 0.02, 0.14)),
                ));
                for sx in [-1.0_f32, 1.0] {
                    hp.spawn((
                        WorldEntity,
                        Mesh3d(palette.unit_cyl.clone()),
                        MeshMaterial3d(hp_m.clone()),
                        Transform::from_xyz(sx * 0.07, -0.01, 0.04)
                            .with_rotation(Quat::from_rotation_z(FRAC_PI_2))
                            .with_scale(Vec3::new(0.07, 0.025, 0.07)),
                    ));
                }
            });

            // Felt side wall (+x) + cushioned bench (`karaoke.js:73-75`)
            g.spawn((
                WorldEntity,
                Mesh3d(palette.unit_box.clone()),
                MeshMaterial3d(felt.clone()),
                Transform::from_xyz(HX - 0.03, MID_Y, 0.0)
                    .with_scale(Vec3::new(0.05, H, HZ * 2.0 - 0.06)),
            ));
            g.spawn((
                WorldEntity,
                KaraokeBench,
                Transform::from_xyz(HX - 0.20, 0.0, 0.04),
                Visibility::default(),
            ))
            .with_children(|b| {
                b.spawn((
                    WorldEntity,
                    Mesh3d(palette.unit_box.clone()),
                    MeshMaterial3d(base),
                    Transform::from_xyz(0.0, 0.26, 0.0)
                        .with_scale(Vec3::new(0.30, 0.38, HZ * 2.0 - 0.30)),
                ));
                b.spawn((
                    WorldEntity,
                    Mesh3d(palette.unit_box.clone()),
                    MeshMaterial3d(cushion_m),
                    Transform::from_xyz(0.0, 0.48, 0.0)
                        .with_scale(Vec3::new(0.32, 0.08, HZ * 2.0 - 0.28)),
                ));
            });

            // Glass side wall (-x) + kick panel + rails + KARAOKE marquee + QR price (`karaoke.js:78-89`)
            g.spawn((
                WorldEntity,
                Mesh3d(palette.unit_box.clone()),
                MeshMaterial3d(felt.clone()),
                Transform::from_xyz(-HX, 0.245, 0.0)
                    .with_scale(Vec3::new(0.04, 0.35, HZ * 2.0 - 0.06)),
            ));
            g.spawn((
                WorldEntity,
                Mesh3d(palette.unit_box.clone()),
                MeshMaterial3d(glass.clone()),
                Transform::from_xyz(-HX, 1.17, 0.0)
                    .with_scale(Vec3::new(0.012, 1.48, HZ * 2.0 - 0.06)),
            ));
            for y in [0.42_f32, 1.92] {
                g.spawn((
                    WorldEntity,
                    Mesh3d(palette.unit_box.clone()),
                    MeshMaterial3d(frame.clone()),
                    Transform::from_xyz(-HX, y, 0.0).with_scale(Vec3::new(0.05, 0.04, HZ * 2.0)),
                ));
            }
            g.spawn((
                WorldEntity,
                Mesh3d(palette.unit_box.clone()),
                MeshMaterial3d(frame.clone()),
                Transform::from_xyz(-HX - 0.026, 2.05, 0.0)
                    .with_scale(Vec3::new(0.02, 0.24, HZ * 2.0 - 0.06)),
            ));
            g.spawn((
                WorldEntity,
                Mesh3d(palette.unit_plane.clone()),
                MeshMaterial3d(band_m.clone()),
                Transform::from_xyz(-HX - 0.042, 2.05, 0.0)
                    .with_rotation(Quat::from_rotation_y(-FRAC_PI_2))
                    .with_scale(Vec3::new(HZ * 2.0 - 0.10, 0.20, 1.0)),
            ));
            g.spawn((
                WorldEntity,
                Mesh3d(palette.unit_plane.clone()),
                MeshMaterial3d(qr_m),
                Transform::from_xyz(-HX - 0.010, 0.62, 0.30)
                    .with_rotation(Quat::from_rotation_y(-FRAC_PI_2))
                    .with_scale(Vec3::new(0.30, 0.15, 1.0)),
            ));

            // Door side (+z): jambs, header, KARAOKE band, hinged glass door pivot (`karaoke.js:92-103`)
            for sx in [-1.0_f32, 1.0] {
                g.spawn((
                    WorldEntity,
                    Mesh3d(palette.unit_box.clone()),
                    MeshMaterial3d(felt.clone()),
                    Transform::from_xyz(sx * (HX + DW * 0.5 + 0.02) * 0.5, MID_Y, HZ - 0.03)
                        .with_scale(Vec3::new((HX * 2.0 - DW) * 0.5 - 0.04, H, 0.05)),
                ));
            }
            g.spawn((
                WorldEntity,
                Mesh3d(palette.unit_box.clone()),
                MeshMaterial3d(felt),
                Transform::from_xyz(0.0, TOP - 0.11, HZ - 0.03)
                    .with_scale(Vec3::new(DW + 0.04, 0.22, 0.05)),
            ));
            g.spawn((
                WorldEntity,
                Mesh3d(palette.unit_plane.clone()),
                MeshMaterial3d(band_m),
                Transform::from_xyz(0.0, 2.08, HZ + 0.006)
                    .with_scale(Vec3::new(HX * 2.0 - 0.10, 0.20, 1.0)),
            ));

            door_pivot_id = g
                .spawn((
                    WorldEntity,
                    KaraokeDoorPivot,
                    Transform::from_xyz(DW * 0.5, 0.0, HZ - 0.02),
                    Visibility::default(),
                ))
                .with_children(|d| {
                    d.spawn((
                        WorldEntity,
                        Mesh3d(palette.unit_box.clone()),
                        MeshMaterial3d(frame.clone()),
                        Transform::from_xyz(-0.018, 1.06, 0.0)
                            .with_scale(Vec3::new(0.035, 1.98, 0.035)),
                    ));
                    d.spawn((
                        WorldEntity,
                        Mesh3d(palette.unit_box.clone()),
                        MeshMaterial3d(frame.clone()),
                        Transform::from_xyz(-DW + 0.018, 1.06, 0.0)
                            .with_scale(Vec3::new(0.035, 1.98, 0.035)),
                    ));
                    for y in [0.09_f32, 2.03] {
                        d.spawn((
                            WorldEntity,
                            Mesh3d(palette.unit_box.clone()),
                            MeshMaterial3d(frame.clone()),
                            Transform::from_xyz(-DW * 0.5, y, 0.0)
                                .with_scale(Vec3::new(DW, 0.04, 0.035)),
                        ));
                    }
                    d.spawn((
                        WorldEntity,
                        Mesh3d(palette.unit_box.clone()),
                        MeshMaterial3d(glass),
                        Transform::from_xyz(-DW * 0.5, 1.06, 0.0)
                            .with_scale(Vec3::new(DW - 0.04, 1.90, 0.01)),
                    ));
                    d.spawn((
                        WorldEntity,
                        Mesh3d(palette.unit_cyl.clone()),
                        MeshMaterial3d(chrome.clone()),
                        Transform::from_xyz(-DW + 0.07, 1.05, 0.045)
                            .with_scale(Vec3::new(0.024, 0.50, 0.024)),
                    ));
                    for y in [1.0_f32, 1.1] {
                        d.spawn((
                            WorldEntity,
                            Mesh3d(palette.unit_box.clone()),
                            MeshMaterial3d(chrome.clone()),
                            Transform::from_xyz(-DW + 0.07, y, 0.025)
                                .with_scale(Vec3::new(0.012, 0.012, 0.04)),
                        ));
                    }
                })
                .id();
        })
        .id();

    commands.entity(root).insert(KaraokeBooth {
        door_pivot: door_pivot_id,
        door_open: 0.0,
        target_open: 0.0,
        led_mat,
        ceil_mat,
    });
}

/* ---------- Live Animation System (zero-allocation in-place buffer updates) ---------- */

#[allow(clippy::too_many_arguments, clippy::type_complexity)]
pub fn animate_leisure_system(
    time: Res<Time>,
    lighting: Option<Res<LightingState>>,
    mut prop_events: EventReader<PropActivated>,
    mut images: Option<ResMut<Assets<Image>>>,
    mut materials: Option<ResMut<Assets<StandardMaterial>>>,
    mut game_tv_q: Query<&mut GameTv>,
    mut ambilight_q: Query<&mut PointLight, With<GameTvAmbilight>>,
    mut arcade_q: Query<&mut ArcadeCabinet>,
    mut bil_tbl_q: Query<&mut BilliardsTable>,
    mut bil_balls_q: Query<(&mut BilliardsBall, &mut Transform, &mut Visibility)>,
    mut ktv_q: Query<&mut KaraokeBooth>,
    mut ktv_door_q: Query<&mut Transform, (With<KaraokeDoorPivot>, Without<BilliardsBall>, Without<KaraokeLyricBar>)>,
    mut ktv_bar_q: Query<(&mut KaraokeLyricBar, &mut Transform), (Without<BilliardsBall>, Without<KaraokeDoorPivot>)>,
) {
    let dt = time.delta_secs().clamp(0.0, 0.1);
    let t = time.elapsed_secs();
    let night = lighting.as_ref().is_some_and(|l| l.night);

    let mut cycle_tv = false;
    let mut coin_arcade = false;
    let mut break_billiards = false;
    let mut toggle_karaoke = false;
    for ev in prop_events.read() {
        match ev.id {
            "game-tv" | "game_tv" | "tv" => cycle_tv = true,
            "arcade" => coin_arcade = true,
            "billiards" => break_billiards = true,
            "karaoke" => toggle_karaoke = true,
            _ => {}
        }
    }

    // 1. GameTv: throttled ~15 Hz zero-allocation in-place 256×144 texture mutation + ambilight sync
    for mut gtv in &mut game_tv_q {
        if cycle_tv {
            gtv.sim.cycle_game();
            gtv.accum = 1.0; // flush immediately on game switch
        }
        gtv.accum += dt;
        const TV_STEP: f32 = 1.0 / 15.0;
        if gtv.accum >= TV_STEP {
            let step_dt = gtv.accum.min(0.1);
            gtv.accum = 0.0;
            gtv.sim.step(step_dt);
            if let Some(imgs) = images.as_deref_mut()
                && let Some(img) = imgs.get_mut(&gtv.screen_image)
                && let Some(data) = img.data.as_mut()
            {
                let src = gtv.sim.rgba();
                if data.len() == src.len() {
                    data.copy_from_slice(src);
                }
            }
        }
        let dom = gtv.sim.dominant_rgb();
        let target = Vec3::new(dom[0], dom[1], dom[2]);
        let blend = (dt * 6.0).clamp(0.08, 1.0);
        gtv.smooth_rgb = gtv.smooth_rgb.lerp(target, blend);
        let c = gtv.smooth_rgb;
        if let Ok(mut pl) = ambilight_q.get_mut(gtv.ambilight) {
            pl.color = Color::srgb(c.x.clamp(0.05, 1.0), c.y.clamp(0.05, 1.0), c.z.clamp(0.05, 1.0));
            pl.intensity = if night { 3_600.0 } else { 1_450.0 };
        }
        if let Some(mats) = materials.as_deref_mut() {
            if let Some(gm) = mats.get_mut(&gtv.glow_mat) {
                gm.base_color = Color::srgba(c.x, c.y, c.z, if night { 0.65 } else { 0.35 });
            }
            if let Some(rm) = mats.get_mut(&gtv.router_led_mat) {
                let blink = if ((t * 4.0) as i32 & 1) == 0 { 1.4 } else { 0.35 };
                rm.emissive = LinearRgba::new(0.15 * blink, 0.85 * blink, 0.25 * blink, 1.0);
            }
        }
    }

    // 2. ArcadeCabinet: throttled ~14 Hz zero-allocation in-place 128×96 texture mutation
    for mut arc in &mut arcade_q {
        if coin_arcade {
            arc.sim.insert_coin();
            arc.accum = 1.0;
        }
        arc.accum += dt;
        const ARC_STEP: f32 = 1.0 / 14.0;
        if arc.accum >= ARC_STEP {
            let step_dt = arc.accum.min(0.1);
            arc.accum = 0.0;
            arc.sim.step(step_dt);
            if let Some(imgs) = images.as_deref_mut()
                && let Some(img) = imgs.get_mut(&arc.screen_image)
                && let Some(data) = img.data.as_mut()
            {
                let src = arc.sim.rgba();
                if data.len() == src.len() {
                    data.copy_from_slice(src);
                }
            }
        }
        if let Some(mats) = materials.as_deref_mut() {
            let mult = if night { 1.5 } else { 1.0 };
            if let Some(mm) = mats.get_mut(&arc.marquee_mat) {
                mm.base_color = Color::srgb(mult, mult, mult);
            }
            if let Some(sm) = mats.get_mut(&arc.screen_mat) {
                let smult = if night { 1.3 } else { 1.0 };
                sm.base_color = Color::srgb(smult, smult, smult);
            }
        }
    }

    // 3. BilliardsTable: interactive break shot + 16-ball elastic collisions & pocket drops
    if let Ok(mut tbl) = bil_tbl_q.single_mut() {
        if break_billiards {
            tbl.active = true;
            tbl.rack_cooldown = 0.0;
            for (mut b, mut tf, mut vis) in &mut bil_balls_q {
                b.pos = racked_ball_pos(b.number);
                b.pocketed = false;
                b.vel = if b.number == 0 {
                    Vec2::new(0.03, -2.5)
                } else {
                    Vec2::ZERO
                };
                tf.translation = Vec3::new(b.pos.x, BIL_CLOTH_Y + BIL_BALL_R, b.pos.y);
                *vis = Visibility::Inherited;
            }
        }
        if tbl.active {
            // Fixed small snapshot for pairwise collisions (16 balls max)
            let mut state: [(u8, Vec2, Vec2, bool); 16] = [(0, Vec2::ZERO, Vec2::ZERO, true); 16];
            let mut count = 0usize;
            for (b, _, _) in bil_balls_q.iter() {
                if (b.number as usize) < 16 {
                    state[b.number as usize] = (b.number, b.pos, b.vel, b.pocketed);
                    count += 1;
                }
            }
            if count == 16 {
                let damp = (1.0 - 1.15 * dt).max(0.0);
                let lim_x = BIL_PX - BIL_BALL_R;
                let lim_z = BIL_PZ - BIL_BALL_R;
                let min_d = BIL_BALL_R * 2.0;
                for s in &mut state {
                    if s.3 {
                        continue;
                    }
                    s.1 += s.2 * dt;
                    s.2 *= damp;
                    if s.2.length_squared() < 0.0004 {
                        s.2 = Vec2::ZERO;
                    }
                    // Check pocket drop
                    for &(px, pz, pr) in &POCKET_SPECS {
                        if s.1.distance(Vec2::new(px, pz)) < pr {
                            if s.0 == 0 {
                                // Scratch: respot cue ball
                                s.1 = Vec2::new(0.0, 0.50);
                                s.2 = Vec2::ZERO;
                            } else {
                                s.3 = true;
                                s.2 = Vec2::ZERO;
                            }
                            break;
                        }
                    }
                    if s.3 {
                        continue;
                    }
                    if s.1.x < -lim_x {
                        s.1.x = -lim_x;
                        s.2.x = s.2.x.abs() * 0.82;
                    } else if s.1.x > lim_x {
                        s.1.x = lim_x;
                        s.2.x = -s.2.x.abs() * 0.82;
                    }
                    if s.1.y < -lim_z {
                        s.1.y = -lim_z;
                        s.2.y = s.2.y.abs() * 0.82;
                    } else if s.1.y > lim_z {
                        s.1.y = lim_z;
                        s.2.y = -s.2.y.abs() * 0.82;
                    }
                }
                for i in 0..16 {
                    if state[i].3 {
                        continue;
                    }
                    for j in (i + 1)..16 {
                        if state[j].3 {
                            continue;
                        }
                        let delta = state[j].1 - state[i].1;
                        let dist = delta.length();
                        if dist > 1e-5 && dist < min_d {
                            let n = delta / dist;
                            let overlap = (min_d - dist) * 0.5;
                            state[i].1 -= n * overlap;
                            state[j].1 += n * overlap;
                            let rel = (state[i].2 - state[j].2).dot(n);
                            if rel > 0.0 {
                                let imp = n * rel * 0.94;
                                state[i].2 -= imp;
                                state[j].2 += imp;
                            }
                        }
                    }
                }
                let mut any_moving = false;
                for (mut b, mut tf, mut vis) in &mut bil_balls_q {
                    let s = state[b.number as usize];
                    b.pos = s.1;
                    b.vel = s.2;
                    b.pocketed = s.3;
                    if !b.pocketed && b.vel.length_squared() > 0.0004 {
                        any_moving = true;
                    }
                    tf.translation = Vec3::new(b.pos.x, BIL_CLOTH_Y + BIL_BALL_R, b.pos.y);
                    *vis = if b.pocketed {
                        Visibility::Hidden
                    } else {
                        Visibility::Inherited
                    };
                }
                if !any_moving {
                    tbl.active = false;
                }
            }
        }
    }

    // 4. KaraokeBooth: hinged glass door swing + neon pulse + scrolling lyric progress bar
    for mut ktv in &mut ktv_q {
        if toggle_karaoke {
            ktv.target_open = if ktv.target_open > 0.2 { 0.0 } else { 1.15 };
        }
        ktv.door_open += (ktv.target_open - ktv.door_open) * (dt * 6.0).min(1.0);
        if let Ok(mut dtf) = ktv_door_q.get_mut(ktv.door_pivot) {
            dtf.rotation = Quat::from_rotation_y(-ktv.door_open);
        }
        let pulse = 0.85 + 0.15 * (t * 2.6).sin();
        let boost = if night { 2.2 } else { 1.4 } * pulse;
        if let Some(mats) = materials.as_deref_mut() {
            if let Some(lm) = mats.get_mut(&ktv.led_mat) {
                lm.emissive = LinearRgba::new(1.0 * boost, 0.22 * boost, 0.65 * boost, 1.0);
            }
            if let Some(cm) = mats.get_mut(&ktv.ceil_mat) {
                let hue = 0.5 + 0.5 * (t * 0.9).sin();
                cm.emissive = LinearRgba::new(
                    (0.55 + 0.4 * hue) * boost * 0.7,
                    0.35 * boost * 0.7,
                    (0.95 - 0.3 * hue) * boost * 0.7,
                    1.0,
                );
            }
        }
    }
    for (mut bar, mut tf) in &mut ktv_bar_q {
        bar.phase = (bar.phase + dt * 0.22) % 1.0;
        let fill = 0.08 + bar.phase * 0.54;
        tf.scale.x = fill;
        tf.translation.x = -0.31 + fill * 0.5;
    }
    let _ = PI;
}
