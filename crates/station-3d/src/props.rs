use crate::interact::{InteractiveProp, PropActivated};
use crate::layout::{DeskCoord, LayoutSpec};
use crate::life::{OfficeLifeState, SeasonTheme};
use crate::lights::LightingState;
use crate::palette::{
    WorldEntity, WorldPalette, spawn_box, spawn_box_rot, spawn_cyl, spawn_quad, spawn_sphere,
};
use crate::props_detail::PropsLiveTelemetry;
use bevy::ecs::system::SystemParam;
use bevy::prelude::*;
use station_canvas2d::FontEngine;
use station_canvas2d::boards_and_social::{DutyBoardInput, DutyRowInput, draw_duty_board};
use station_canvas2d::office_surfaces::{
    TvStatsInput, draw_arcade_frame, draw_arcade_marquee, draw_dev_server_badge,
    draw_game_tv_standby, draw_karaoke_band, draw_karaoke_qr_badge, draw_karaoke_screen,
    draw_stats_tv,
};
use std::f32::consts::{FRAC_PI_2, PI, TAU};

pub use crate::props_detail::cat::OfficeCat;

pub const ALL_PROP_IDS: [&str; 20] = [
    "tv",
    "arcade",
    "aquarium",
    "billiards",
    "karaoke",
    "game-tv",
    "duty-board",
    "cups",
    "microwave",
    "printer",
    "shredder",
    "cat",
    "vacuum",
    "dev-servers",
    "neon",
    "bins",
    "switches",
    "string-lights",
    "trinkets",
    "tv-games",
];

// Deterministic PRNG matching `trinkets.js` (`propSeed = 77`) for aquarium flora & fish
struct PropRng(u64);
impl PropRng {
    fn next_f32(&mut self) -> f32 {
        self.0 = (self.0 * 16_807) % 2_147_483_647;
        self.0 as f32 / 2_147_483_647.0
    }
    fn pick<'a, T>(&mut self, slice: &'a [T]) -> &'a T {
        let idx = ((self.next_f32() * slice.len() as f32).floor() as usize).min(slice.len() - 1);
        &slice[idx]
    }
}

#[derive(Component, Debug, Clone)]
pub struct AquariumTank {
    pub center: Vec2,
    pub feed_left: f32,
    pub feed_k: f32,
    pub light_mat: Handle<StandardMaterial>,
    pub flake_mesh: Handle<Mesh>,
    pub flake_mat: Handle<StandardMaterial>,
}

#[derive(Component, Debug, Clone)]
pub struct AquariumFish {
    pub center: Vec2,
    pub ax: f32,
    pub ay: f32,
    pub az: f32,
    pub y0: f32,
    pub speed: f32,
    pub phase: f32,
}

#[derive(Component, Debug, Clone, Copy)]
pub struct AquariumFishTail {
    pub phase: f32,
}

#[derive(Component, Debug, Clone, Copy)]
pub struct AquariumWeed {
    pub phase: f32,
}

#[derive(Component, Debug, Clone, Copy)]
pub struct AquariumBubble {
    pub index: usize,
    pub base_x: f32,
}

#[derive(Component, Debug, Clone, Copy)]
pub struct AquariumFlake {
    pub fall_speed: f32,
    pub life: f32,
}

#[derive(Component, Debug, Clone)]
pub struct RobotVacuum {
    pub waypoints: Vec<Vec2>,
    pub cur_idx: usize,
    pub progress: f32,
    pub spin_left: f32,
    pub led_mat: Handle<StandardMaterial>,
}

#[derive(Component, Debug, Clone, Copy)]
pub struct VacuumLed;

#[derive(Component, Debug, Clone, Copy)]
pub struct VacuumBrush {
    pub dir: f32,
}

#[derive(Component, Debug, Clone)]
pub struct OfficePrinter {
    pub queue: u32,
    pub last_trigger: u32,
    pub timer: f32,
    pub paper_ent: Entity,
    pub led_mat: Handle<StandardMaterial>,
}

#[derive(Component, Debug, Clone, Copy)]
pub struct PrinterPaper;

#[derive(Component, Debug, Clone)]
pub struct DevServerBox {
    pub slot: usize,
    pub lost: bool,
    pub cpu_pct: f32,
    pub phase: f32,
    pub led_mat: Handle<StandardMaterial>,
    pub ring_ent: Entity,
    pub lost_anchor: Vec3,
    pub lost_ry: f32,
    pub desks: Vec<DeskCoord>,
}

#[derive(Component, Debug, Clone, Copy)]
pub struct DevServerRing;

#[derive(Component, Debug, Clone)]
pub struct StringLightsRoot {
    pub bulb_mats: [Handle<StandardMaterial>; 4],
    pub applied_season: Option<SeasonTheme>,
}

#[derive(Component, Debug, Clone, Copy)]
pub struct WallSwitch {
    pub off: bool,
    pub lever: Entity,
}

#[derive(Component, Debug, Clone, Copy)]
pub struct SwitchLever;

fn wrap_angle(mut a: f32) -> f32 {
    while a > PI {
        a -= TAU;
    }
    while a < -PI {
        a += TAU;
    }
    a
}

fn desk_local(d: &DeskCoord, lx: f32, ly: f32, lz: f32) -> Vec3 {
    Vec3::new(d.x, 0.0, d.z) + Quat::from_rotation_y(d.ry) * Vec3::new(lx, ly, lz)
}

fn season_bulb_colors(season: SeasonTheme) -> [LinearRgba; 4] {
    match season {
        SeasonTheme::Tet => [
            LinearRgba::new(2.9, 0.4, 0.35, 1.0),
            LinearRgba::new(2.9, 2.1, 0.6, 1.0),
            LinearRgba::new(2.9, 0.4, 0.35, 1.0),
            LinearRgba::new(2.9, 2.1, 0.6, 1.0),
        ],
        SeasonTheme::Christmas => [
            LinearRgba::new(2.8, 0.45, 0.45, 1.0),
            LinearRgba::new(0.5, 2.5, 0.9, 1.0),
            LinearRgba::new(2.9, 2.2, 0.9, 1.0),
            LinearRgba::new(2.5, 2.5, 2.7, 1.0),
        ],
        SeasonTheme::MidAutumn => [
            LinearRgba::new(2.9, 1.5, 0.3, 1.0),
            LinearRgba::new(2.8, 2.2, 0.55, 1.0),
            LinearRgba::new(2.6, 0.7, 0.3, 1.0),
            LinearRgba::new(2.9, 2.0, 0.7, 1.0),
        ],
        SeasonTheme::Normal => [
            LinearRgba::new(2.6, 1.8, 0.9, 1.0),
            LinearRgba::new(2.4, 1.2, 1.5, 1.0),
            LinearRgba::new(1.4, 2.2, 2.6, 1.0),
            LinearRgba::new(2.8, 2.2, 1.2, 1.0),
        ],
    }
}

/// Spawns all interactive office props with exact Oracle layout coordinates, parent-child hierarchies,
/// and `WorldPalette` deduplicated/tracked assets.
pub fn spawn_all_props(
    commands: &mut Commands,
    palette: &mut WorldPalette,
    materials: &mut Assets<StandardMaterial>,
    images: &mut Assets<Image>,
    fonts: &mut FontEngine,
    spec: &LayoutSpec,
) {
    let metal = palette.mat(materials, "#2B2F36", 0.35, 0.65);

    // 1. Live stats TV (tv.js — upgraded in Task 6)
    {
        let [tx, ty, tz, try_] = spec.tv;
        let rot = Quat::from_rotation_y(try_);
        let fwd = rot * Vec3::Z;
        let bezel = palette.mat(materials, "#15181E", 0.4, 0.15);
        let tv_ent = spawn_box_rot(
            commands,
            palette,
            bezel,
            Vec3::new(tx, ty, tz),
            Vec3::new(2.32, 1.34, 0.08),
            rot,
        );
        commands
            .entity(tv_ent)
            .insert(InteractiveProp::new("tv", "server", "Live Machine Stats TV"));

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
                    2, 3, 1, 4, 6, 5, 3, 2, 7, 8, 4, 3, 5, 6, 8, 9, 7, 5, 3, 4, 6, 7, 8, 5, 4, 3,
                    6, 8, 7, 5,
                ],
            },
            fonts,
        );
        let tv_mat = palette.textured_unlit(images, materials, tv_surf);
        spawn_quad(
            commands,
            palette,
            tv_mat,
            Vec3::new(tx, ty, tz) + fwd * 0.048,
            Vec2::new(2.18, 1.2),
            rot,
        );
    }

    // 2. Arcade cabinet + marquee + invaders screen (arcade.js — upgraded in Task 6)
    {
        let [ax, az] = spec.arc;
        let cab_m = palette.mat(materials, "#4B2E83", 0.55, 0.05);
        let trim_m = palette.emissive(materials, "#F72585", 1.2);
        let deck_m = palette.mat(materials, "#2A1B4F", 0.5, 0.05);
        let arc_ent = spawn_box(
            commands,
            palette,
            cab_m,
            Vec3::new(ax, 0.875, az),
            Vec3::new(0.8, 1.75, 0.7),
        );
        commands
            .entity(arc_ent)
            .insert(InteractiveProp::new("arcade", "lounge", "Arcade Cabinet"));
        for sx in [-0.405_f32, 0.405] {
            spawn_box(
                commands,
                palette,
                trim_m.clone(),
                Vec3::new(ax + sx, 0.95, az),
                Vec3::new(0.02, 1.5, 0.5),
            );
        }
        spawn_box(
            commands,
            palette,
            deck_m,
            Vec3::new(ax, 0.98, az + 0.42),
            Vec3::new(0.76, 0.1, 0.32),
        );
        let scr_mat = palette.textured_unlit(images, materials, draw_arcade_frame(1.8, false, fonts));
        spawn_quad(
            commands,
            palette,
            scr_mat,
            Vec3::new(ax, 1.38, az + 0.36),
            Vec2::new(0.58, 0.44),
            Quat::from_rotation_x(-0.18),
        );
        let marq_mat = palette.textured_unlit(images, materials, draw_arcade_marquee(fonts));
        spawn_quad(
            commands,
            palette,
            marq_mat,
            Vec3::new(ax, 1.66, az + 0.36),
            Vec2::new(0.74, 0.19),
            Quat::IDENTITY,
        );
    }

    // 3. Detailed Aquarium: gravel bed, rocks, 9 swaying seaweeds, 7 tangent-facing fish with wagging tails, 10 bubbles (`aquarium.js`)
    spawn_aquarium(commands, palette, materials, spec);

    // 4. Pro/HQ Lounge Game-TV + PS5 + controllers (game-tv.js — upgraded in Task 6)
    if let Some([gx, gz]) = spec.game_tv {
        let walnut = palette.mat(materials, "#6B4A33", 0.6, 0.0);
        let oak = palette.mat(materials, "#B58A60", 0.55, 0.0);
        let black = palette.mat(materials, "#16181C", 0.35, 0.2);
        let white = palette.mat(materials, "#F4F5F7", 0.35, 0.0);
        let blue_led = palette.emissive(materials, "#6AAEFF", 1.5);

        let unit = spawn_box(
            commands,
            palette,
            walnut,
            Vec3::new(gx, 0.29, gz),
            Vec3::new(1.8, 0.42, 0.42),
        );
        commands
            .entity(unit)
            .insert(InteractiveProp::new("game-tv", "lounge", "Lounge Console TV"));
        spawn_box(
            commands,
            palette,
            oak,
            Vec3::new(gx, 0.515, gz),
            Vec3::new(1.82, 0.03, 0.44),
        );
        spawn_box(
            commands,
            palette,
            black.clone(),
            Vec3::new(gx, 0.995, gz - 0.06),
            Vec3::new(1.25, 0.72, 0.04),
        );
        let gtv_mat = palette.textured_unlit(images, materials, draw_game_tv_standby(fonts));
        spawn_quad(
            commands,
            palette,
            gtv_mat,
            Vec3::new(gx, 0.995, gz - 0.035),
            Vec2::new(1.22, 0.69),
            Quat::IDENTITY,
        );
        spawn_box(
            commands,
            palette,
            white.clone(),
            Vec3::new(gx + 0.74, 0.73, gz - 0.04),
            Vec3::new(0.14, 0.4, 0.26),
        );
        spawn_box(
            commands,
            palette,
            blue_led,
            Vec3::new(gx + 0.81, 0.73, gz + 0.08),
            Vec3::new(0.015, 0.32, 0.015),
        );
        spawn_box(
            commands,
            palette,
            white,
            Vec3::new(gx - 0.57, 0.55, gz + 0.11),
            Vec3::new(0.15, 0.03, 0.07),
        );
        spawn_box(
            commands,
            palette,
            black,
            Vec3::new(gx + 0.55, 0.55, gz + 0.15),
            Vec3::new(0.15, 0.03, 0.07),
        );
    }

    // 5. Pro/HQ Billiards table (billiards.js — upgraded in Task 6)
    if let Some([bx, bz]) = spec.bil {
        let wood = palette.mat(materials, "#5C3A24", 0.55, 0.0);
        let felt = palette.mat(materials, "#1F6E43", 0.9, 0.0);
        let tbl = spawn_box(
            commands,
            palette,
            wood,
            Vec3::new(bx, 0.78, bz),
            Vec3::new(1.42, 0.16, 2.32),
        );
        commands
            .entity(tbl)
            .insert(InteractiveProp::new("billiards", "lounge", "Pool Table"));
        spawn_box(
            commands,
            palette,
            felt,
            Vec3::new(bx, 0.87, bz),
            Vec3::new(1.22, 0.03, 2.12),
        );
        for (ox, oz) in [
            (-0.55_f32, -0.95_f32),
            (0.55, -0.95),
            (-0.55, 0.95),
            (0.55, 0.95),
        ] {
            spawn_cyl(
                commands,
                palette,
                metal.clone(),
                Vec3::new(bx + ox, 0.38, bz + oz),
                0.1,
                0.76,
            );
        }
        for (i, col) in ["#FFFFFF", "#E63946", "#FFD166", "#3F7CC4", "#222222"]
            .iter()
            .enumerate()
        {
            let bm = palette.mat(materials, col, 0.2, 0.0);
            spawn_sphere(
                commands,
                palette,
                bm,
                Vec3::new(
                    bx - 0.12 + (i % 3) as f32 * 0.1,
                    0.91,
                    bz - 0.25 + (i / 3) as f32 * 0.15,
                ),
                Vec3::splat(0.065),
            );
        }
    }

    // 6. Pro/HQ Karaoke booth (karaoke.js — upgraded in Task 6)
    if let Some([kx, kz, kry]) = spec.ktv {
        let rot = Quat::from_rotation_y(kry);
        let at = |lx: f32, ly: f32, lz: f32| Vec3::new(kx, 0.0, kz) + rot * Vec3::new(lx, ly, lz);
        let frame = palette.mat(materials, "#1D1F26", 0.35, 0.5);
        let felt = palette.mat(materials, "#3B2A5C", 0.95, 0.0);
        let glass = palette.glass(materials, "#D7E6F5", 0.16);
        let led = palette.emissive(materials, "#FF4FB0", 1.8);

        let ktv_ent = spawn_box_rot(
            commands,
            palette,
            frame.clone(),
            at(0.0, 2.24, 0.0),
            Vec3::new(1.28, 0.08, 1.32),
            rot,
        );
        commands
            .entity(ktv_ent)
            .insert(InteractiveProp::new("karaoke", "lounge", "Karaoke Booth"));
        spawn_box_rot(
            commands,
            palette,
            frame,
            at(0.0, 0.04, 0.0),
            Vec3::new(1.26, 0.08, 1.3),
            rot,
        );
        spawn_box_rot(
            commands,
            palette,
            felt.clone(),
            at(0.0, 1.14, -0.59),
            Vec3::new(1.14, 2.13, 0.05),
            rot,
        );
        spawn_box_rot(
            commands,
            palette,
            felt,
            at(0.57, 1.14, 0.0),
            Vec3::new(0.05, 2.13, 1.18),
            rot,
        );
        spawn_box_rot(
            commands,
            palette,
            glass,
            at(-0.6, 1.17, 0.0),
            Vec3::new(0.02, 1.5, 1.18),
            rot,
        );
        spawn_box_rot(
            commands,
            palette,
            led,
            at(0.0, 2.2, 0.66),
            Vec3::new(1.29, 0.025, 0.02),
            rot,
        );

        let band_m = palette.textured_unlit(images, materials, draw_karaoke_band("KARAOKE", fonts));
        spawn_quad(
            commands,
            palette,
            band_m.clone(),
            at(-0.64, 2.05, 0.0),
            Vec2::new(1.14, 0.2),
            rot * Quat::from_rotation_y(-FRAC_PI_2),
        );
        spawn_quad(
            commands,
            palette,
            band_m,
            at(0.0, 2.08, 0.64),
            Vec2::new(1.1, 0.2),
            rot,
        );

        let qr_m = palette.textured_unlit(images, materials, draw_karaoke_qr_badge(fonts));
        spawn_quad(
            commands,
            palette,
            qr_m,
            at(-0.62, 0.62, 0.3),
            Vec2::new(0.3, 0.15),
            rot * Quat::from_rotation_y(-FRAC_PI_2),
        );

        let ktv_scr = palette.textured_unlit(
            images,
            materials,
            draw_karaoke_screen("Agents Station KTV", "Lạc trôi giữa codebase Rust ♪", 0.55, fonts),
        );
        spawn_quad(
            commands,
            palette,
            ktv_scr,
            at(0.0, 1.42, -0.53),
            Vec2::new(0.76, 0.43),
            rot,
        );
    }

    // 7. Pro/HQ Pantry Duty Board (aluminum frame + markers + sink cups, `duty-board.js`)
    if spec.layout.is_pro() {
        spawn_duty_board_and_sink_cups(commands, palette, materials, images, fonts);
    }

    // 8. Dual hollow Microwaves + lunchboxes + rule note + dish soap (`props_detail::microwave`)
    crate::props_detail::microwave::spawn_microwaves(
        commands, palette, materials, images, fonts, spec,
    );

    // 9. Office Printer with ejecting paper (`printer.js`)
    spawn_printer(commands, palette, materials, spec);

    // 10. Transparent-window Shredder + per-desk context stacks/flags + per-desk waste bins + per-desk terracotta cache cups
    crate::props_detail::shredder_cups::spawn_shredder_bins_and_cups(
        commands, palette, materials, spec,
    );

    // 11. Sagging seasonal StringLights (`string-lights.js`) + working WallSwitch (`switches.js`)
    spawn_string_lights(commands, palette, materials, spec);
    spawn_wall_switch(commands, palette, materials, spec);

    // 12. Live per-owner & forgotten DevServer mini boxes (`dev-servers.js`)
    spawn_dev_server_pool(commands, palette, materials, images, fonts, spec);

    // 13. 20+ part articulated tabby OfficeCat (`props_detail::cat`) + Parented-LED patrolling RobotVacuum (`vacuum.js`)
    crate::props_detail::cat::spawn_office_cat(commands, palette, materials, spec);
    spawn_robot_vacuum(commands, palette, materials, spec);
}

fn spawn_duty_board_and_sink_cups(
    commands: &mut Commands,
    palette: &mut WorldPalette,
    materials: &mut Assets<StandardMaterial>,
    images: &mut Assets<Image>,
    fonts: &mut FontEngine,
) {
    let bx = 11.0_f32;
    let by = 3.08_f32;
    let bz = -7.955_f32;
    let bw = 1.6_f32;
    let bh = 1.09_f32;

    let frame_m = palette.mat(materials, "#BFC6CE", 0.4, 0.5);
    let red_mk = palette.mat(materials, "#C8261B", 0.5, 0.0);
    let blue_mk = palette.mat(materials, "#1F4E9E", 0.5, 0.0);
    let board_surf = draw_duty_board(
        &DutyBoardInput {
            title: "TRỰC NHẬT".into(),
            date: "25/11".into(),
            off_label: None,
            rows: vec![
                DutyRowInput {
                    chore_key: "cups".into(),
                    chore_label: "Rửa ly".into(),
                    person: "Mai".into(),
                    done_at: Some("11:30".into()),
                    busy: false,
                },
                DutyRowInput {
                    chore_key: "trash".into(),
                    chore_label: "Đổ rác".into(),
                    person: "Long".into(),
                    done_at: None,
                    busy: true,
                },
                DutyRowInput {
                    chore_key: "plants".into(),
                    chore_label: "Tưới cây".into(),
                    person: "An".into(),
                    done_at: None,
                    busy: false,
                },
                DutyRowInput {
                    chore_key: "table".into(),
                    chore_label: "Lau bàn".into(),
                    person: "Huy".into(),
                    done_at: None,
                    busy: false,
                },
            ],
            owed_footer: None,
        },
        fonts,
    );
    let db_mat = palette.textured_lit(images, materials, board_surf, 0.35);

    commands
        .spawn((
            WorldEntity,
            InteractiveProp::new("duty-board", "pantry", "Pantry Duty Board"),
            Transform::from_xyz(bx, by, bz),
            Visibility::default(),
        ))
        .with_children(|g| {
            // Aluminum frame + bottom marker tray (`duty-board.js:88-92`)
            g.spawn((
                WorldEntity,
                Mesh3d(palette.unit_box.clone()),
                MeshMaterial3d(frame_m.clone()),
                Transform::from_xyz(0.0, bh * 0.5 + 0.0175, 0.0)
                    .with_scale(Vec3::new(bw + 0.06, 0.035, 0.03)),
            ));
            g.spawn((
                WorldEntity,
                Mesh3d(palette.unit_box.clone()),
                MeshMaterial3d(frame_m.clone()),
                Transform::from_xyz(0.0, -bh * 0.5 - 0.0175, 0.0)
                    .with_scale(Vec3::new(bw + 0.06, 0.035, 0.03)),
            ));
            for sx in [-1.0_f32, 1.0] {
                g.spawn((
                    WorldEntity,
                    Mesh3d(palette.unit_box.clone()),
                    MeshMaterial3d(frame_m.clone()),
                    Transform::from_xyz(sx * (bw * 0.5 + 0.015), 0.0, 0.0)
                        .with_scale(Vec3::new(0.03, bh, 0.03)),
                ));
            }
            g.spawn((
                WorldEntity,
                Mesh3d(palette.unit_box.clone()),
                MeshMaterial3d(frame_m),
                Transform::from_xyz(0.2, -bh * 0.5 - 0.045, 0.03)
                    .with_scale(Vec3::new(0.6, 0.025, 0.07)),
            ));
            // Red & blue markers on tray (`duty-board.js:97`)
            for (mx, mk) in [(0.1_f32, red_mk), (0.24, blue_mk)] {
                g.spawn((
                    WorldEntity,
                    Mesh3d(palette.unit_cyl.clone()),
                    MeshMaterial3d(mk),
                    Transform::from_xyz(mx, -bh * 0.5 - 0.022, 0.045)
                        .with_rotation(Quat::from_rotation_z(FRAC_PI_2))
                        .with_scale(Vec3::new(0.024, 0.13, 0.024)),
                ));
            }
            g.spawn((
                WorldEntity,
                Mesh3d(palette.unit_plane.clone()),
                MeshMaterial3d(db_mat),
                Transform::from_xyz(0.0, 0.0, 0.016).with_scale(Vec3::new(bw, bh, 1.0)),
            ));
        });

    // Cups waiting by the pantry sink (`duty-board.js:70-76, 102`)
    let cup_colors = ["#FFFFFF", "#F3E9D8", "#3F7CC4", "#D6ECF5", "#E8742A"];
    for (i, (cx, cz)) in [
        (12.41_f32, -7.46_f32),
        (12.55, -7.46),
        (12.69, -7.46),
        (12.41, -7.63),
        (12.86, -7.50),
    ]
    .into_iter()
    .enumerate()
    {
        let cm = palette.mat(materials, cup_colors[i % cup_colors.len()], 0.35, 0.0);
        spawn_cyl(commands, palette, cm, Vec3::new(cx, 1.045, cz), 0.08, 0.09);
    }
}

fn spawn_aquarium(
    commands: &mut Commands,
    palette: &mut WorldPalette,
    materials: &mut Assets<StandardMaterial>,
    spec: &LayoutSpec,
) {
    let [qx, qz] = spec.aq;
    let mut rng = PropRng(77_109);

    let cab_m = palette.mat(materials, "#5B4636", 0.7, 0.0);
    let gravel_m = palette.mat(materials, "#E6D3A0", 0.95, 0.0);
    let water_m = palette.glass(materials, "#56AED6", 0.30);
    let glass_m = palette.glass(materials, "#D8F2FF", 0.16);
    let frame_m = palette.mat(materials, "#23272E", 0.5, 0.25);
    let light_mat = palette.owned_emissive(materials, "#BFE8FF", 1.2);
    let bubble_m = palette.glass(materials, "#E8F8FF", 0.72);
    let flake_mat = palette.mat(materials, "#E7A44B", 0.8, 0.0);

    let tank_root = commands
        .spawn((
            WorldEntity,
            AquariumTank {
                center: Vec2::new(qx, qz),
                feed_left: 0.0,
                feed_k: 0.0,
                light_mat: light_mat.clone(),
                flake_mesh: palette.unit_box.clone(),
                flake_mat,
            },
            InteractiveProp::new("aquarium", "lounge", "Office Aquarium"),
            Transform::from_xyz(qx, 0.0, qz),
            Visibility::default(),
        ))
        .id();

    commands.entity(tank_root).with_children(|g| {
        // Low wood cabinet + gravel bed (`aquarium.js:20-21`)
        g.spawn((
            WorldEntity,
            Mesh3d(palette.unit_box.clone()),
            MeshMaterial3d(cab_m),
            Transform::from_xyz(0.0, 0.36, 0.0).with_scale(Vec3::new(2.15, 0.72, 0.62)),
        ));
        g.spawn((
            WorldEntity,
            Mesh3d(palette.unit_box.clone()),
            MeshMaterial3d(gravel_m),
            Transform::from_xyz(0.0, 0.76, 0.0).with_scale(Vec3::new(1.94, 0.06, 0.50)),
        ));
        // 3 decorative aquarium rocks (`aquarium.js:22`)
        for (ox, s, hex) in [
            (-0.6_f32, 0.16_f32, "#8A8F98"),
            (-0.45, 0.11, "#6F747C"),
            (0.55, 0.14, "#9AA0A8"),
        ] {
            let rm = palette.mat(materials, hex, 0.9, 0.0);
            g.spawn((
                WorldEntity,
                Mesh3d(palette.unit_sphere.clone()),
                MeshMaterial3d(rm),
                Transform::from_xyz(ox, 0.80, -0.05).with_scale(Vec3::new(s * 1.1, s * 0.85, s)),
            ));
        }
        // 9 swaying seaweeds pivoted at gravel bed (`aquarium.js:23`)
        let weed_cols = ["#3E8E4A", "#5DAA52", "#2F7A44"];
        for _ in 0..9 {
            let h = 0.25 + rng.next_f32() * 0.30;
            let wm = palette.mat(materials, rng.pick(&weed_cols), 0.8, 0.0);
            let wx = -0.85 + rng.next_f32() * 1.70;
            let wz = -0.15 + rng.next_f32() * 0.30;
            let ph = rng.next_f32() * TAU;
            g.spawn((
                WorldEntity,
                AquariumWeed { phase: ph },
                Transform::from_xyz(wx, 0.79, wz),
                Visibility::default(),
            ))
            .with_children(|w| {
                w.spawn((
                    WorldEntity,
                    Mesh3d(palette.unit_cone.clone()),
                    MeshMaterial3d(wm),
                    Transform::from_xyz(0.0, h * 0.5, 0.0).with_scale(Vec3::new(0.06, h, 0.06)),
                ));
            });
        }

        // Translucent water + outer glass + black corner frame + lid + LED bar (`aquarium.js:24-34`)
        g.spawn((
            WorldEntity,
            Mesh3d(palette.unit_box.clone()),
            MeshMaterial3d(water_m),
            Transform::from_xyz(0.0, 1.06, 0.0).with_scale(Vec3::new(1.94, 0.66, 0.50)),
        ));
        g.spawn((
            WorldEntity,
            Mesh3d(palette.unit_box.clone()),
            MeshMaterial3d(glass_m),
            Transform::from_xyz(0.0, 1.12, 0.0).with_scale(Vec3::new(2.0, 0.80, 0.56)),
        ));
        for sx in [-1.0_f32, 1.0] {
            for sz in [-1.0_f32, 1.0] {
                g.spawn((
                    WorldEntity,
                    Mesh3d(palette.unit_box.clone()),
                    MeshMaterial3d(frame_m.clone()),
                    Transform::from_xyz(sx * 0.99, 1.12, sz * 0.27)
                        .with_scale(Vec3::new(0.03, 0.80, 0.03)),
                ));
            }
        }
        for sz in [-1.0_f32, 1.0] {
            g.spawn((
                WorldEntity,
                Mesh3d(palette.unit_box.clone()),
                MeshMaterial3d(frame_m.clone()),
                Transform::from_xyz(0.0, 0.735, sz * 0.27).with_scale(Vec3::new(2.0, 0.03, 0.03)),
            ));
        }
        g.spawn((
            WorldEntity,
            Mesh3d(palette.unit_box.clone()),
            MeshMaterial3d(frame_m),
            Transform::from_xyz(0.0, 1.545, 0.0).with_scale(Vec3::new(2.06, 0.05, 0.60)),
        ));
        g.spawn((
            WorldEntity,
            Mesh3d(palette.unit_box.clone()),
            MeshMaterial3d(light_mat),
            Transform::from_xyz(0.0, 1.515, 0.0).with_scale(Vec3::new(1.80, 0.015, 0.06)),
        ));

        // 7 swimming fish with parent-child wagging tail (`aquarium.js:36-42`)
        let fish_cols = [
            "#FF8C42", "#FFD166", "#4CC9F0", "#F72585", "#FFFFFF", "#FF6B6B",
        ];
        for _ in 0..7 {
            let fm = palette.emissive(materials, rng.pick(&fish_cols), 0.45);
            let ph = rng.next_f32() * TAU;
            let speed = 0.35 + rng.next_f32() * 0.35;
            let ax = 0.55 + rng.next_f32() * 0.30;
            let ay = 0.08 + rng.next_f32() * 0.14;
            let y0 = 0.95 + rng.next_f32() * 0.30;
            let az = 0.06 + rng.next_f32() * 0.10;
            g.spawn((
                WorldEntity,
                AquariumFish {
                    center: Vec2::new(qx, qz),
                    ax,
                    ay,
                    az,
                    y0,
                    speed,
                    phase: ph,
                },
                Transform::from_xyz(ph.sin() * ax, y0, (ph * 0.7).cos() * az),
                Visibility::default(),
            ))
            .with_children(|f| {
                // Fish body aligned along local +X (nose +X, tail -X)
                f.spawn((
                    WorldEntity,
                    Mesh3d(palette.unit_sphere.clone()),
                    MeshMaterial3d(fm.clone()),
                    Transform::default().with_scale(Vec3::new(0.135, 0.085, 0.045)),
                ));
                f.spawn((
                    WorldEntity,
                    AquariumFishTail { phase: ph },
                    Mesh3d(palette.unit_cone.clone()),
                    MeshMaterial3d(fm),
                    Transform::from_xyz(-0.085, 0.0, 0.0)
                        .with_rotation(Quat::from_rotation_z(-FRAC_PI_2))
                        .with_scale(Vec3::new(0.07, 0.06, 0.025)),
                ));
            });
        }

        // 10 rising air bubbles (`aquarium.js:43`)
        for i in 0..10 {
            g.spawn((
                WorldEntity,
                AquariumBubble {
                    index: i,
                    base_x: 0.70,
                },
                Mesh3d(palette.unit_sphere.clone()),
                MeshMaterial3d(bubble_m.clone()),
                Transform::from_xyz(0.70, 0.80 + i as f32 * 0.056, 0.0).with_scale(Vec3::splat(0.028)),
            ));
        }
    });
}

fn spawn_printer(
    commands: &mut Commands,
    palette: &mut WorldPalette,
    materials: &mut Assets<StandardMaterial>,
    spec: &LayoutSpec,
) {
    let [px, pz] = spec.prn;
    let cab_m = palette.mat(materials, "#D9D4C7", 0.7, 0.0);
    let body_m = palette.mat(materials, "#ECEFF2", 0.4, 0.05);
    let tray_m = palette.mat(materials, "#C9CED6", 0.4, 0.15);
    let paper_m = palette.mat(materials, "#FBFBF7", 0.9, 0.0);
    let led_mat = palette.owned_emissive(materials, "#3DDC84", 1.5);

    let root = commands
        .spawn((
            WorldEntity,
            InteractiveProp::new("printer", "office", "Office Printer"),
            Transform::from_xyz(px, 0.0, pz),
            Visibility::default(),
        ))
        .id();

    let mut paper_ent = Entity::PLACEHOLDER;
    commands.entity(root).with_children(|g| {
        // Cabinet, printer unit, tilted output tray, status LED (`printer.js:18-24`)
        g.spawn((
            WorldEntity,
            Mesh3d(palette.unit_box.clone()),
            MeshMaterial3d(cab_m),
            Transform::from_xyz(0.0, 0.36, 0.0).with_scale(Vec3::new(0.72, 0.72, 0.56)),
        ));
        g.spawn((
            WorldEntity,
            Mesh3d(palette.unit_box.clone()),
            MeshMaterial3d(body_m),
            Transform::from_xyz(0.0, 0.85, 0.0).with_scale(Vec3::new(0.60, 0.26, 0.44)),
        ));
        g.spawn((
            WorldEntity,
            Mesh3d(palette.unit_box.clone()),
            MeshMaterial3d(tray_m),
            Transform::from_xyz(0.0, 0.80, 0.28)
                .with_rotation(Quat::from_rotation_x(-0.15))
                .with_scale(Vec3::new(0.46, 0.03, 0.16)),
        ));
        g.spawn((
            WorldEntity,
            Mesh3d(palette.unit_sphere.clone()),
            MeshMaterial3d(led_mat.clone()),
            Transform::from_xyz(0.22, 0.95, 0.20).with_scale(Vec3::splat(0.03)),
        ));
        paper_ent = g
            .spawn((
                WorldEntity,
                PrinterPaper,
                Mesh3d(palette.unit_box.clone()),
                MeshMaterial3d(paper_m),
                Transform::from_xyz(0.0, 0.83, 0.05).with_scale(Vec3::new(0.30, 0.004, 0.40)),
                Visibility::Hidden,
            ))
            .id();
    });

    commands.entity(root).insert(OfficePrinter {
        queue: 0,
        last_trigger: 0,
        timer: -1.0,
        paper_ent,
        led_mat,
    });
}

fn spawn_string_lights(
    commands: &mut Commands,
    palette: &mut WorldPalette,
    materials: &mut Assets<StandardMaterial>,
    spec: &LayoutSpec,
) {
    let wire_m = palette.mat(materials, "#22252B", 0.7, 0.2);
    let bulb_mats: [Handle<StandardMaterial>; 4] = [
        palette.owned_emissive(materials, "#FFE2A0", 1.8),
        palette.owned_emissive(materials, "#F2A0B8", 1.8),
        palette.owned_emissive(materials, "#8CE0FF", 1.8),
        palette.owned_emissive(materials, "#FFE88A", 1.8),
    ];

    let b = spec.bounds;
    let x_end = if spec.layout.is_pro() {
        b.x1 - 0.3
    } else {
        3.4
    };

    // Build exact catenary bulb positions along back wall (`z = z0 + 0.1`) & left wall (`x = x0 + 0.1`)
    let mut back_pts = Vec::new();
    let mut x = b.x0 + 0.3;
    while x <= x_end {
        let f = ((x - b.x0 - 0.3) % 1.6) / 1.6;
        back_pts.push(Vec3::new(x, 3.98 - (f * PI).sin() * 0.16, b.z0 + 0.1));
        x += 0.48;
    }
    let mut left_pts = Vec::new();
    let mut z = b.z0 + 0.4;
    while z <= b.z1 - 0.2 {
        let f = ((z - b.z0 - 0.4) % 1.6) / 1.6;
        left_pts.push(Vec3::new(b.x0 + 0.1, 3.98 - (f * PI).sin() * 0.16, z));
        z += 0.48;
    }

    commands
        .spawn((
            WorldEntity,
            StringLightsRoot {
                bulb_mats: bulb_mats.clone(),
                applied_season: None,
            },
            Transform::default(),
            Visibility::default(),
        ))
        .with_children(|g| {
            let mut idx = 0usize;
            for strand in [&back_pts, &left_pts] {
                for (i, &p) in strand.iter().enumerate() {
                    g.spawn((
                        WorldEntity,
                        Mesh3d(palette.unit_sphere.clone()),
                        MeshMaterial3d(bulb_mats[idx % 4].clone()),
                        Transform::from_translation(p).with_scale(Vec3::splat(0.068)),
                    ));
                    idx += 1;
                    if let Some(&next) = strand.get(i + 1) {
                        let mid = (p + next) * 0.5;
                        let diff = next - p;
                        let len = diff.length();
                        if len > 1e-3 {
                            let rot = Quat::from_rotation_arc(Vec3::X, diff / len);
                            g.spawn((
                                WorldEntity,
                                Mesh3d(palette.unit_box.clone()),
                                MeshMaterial3d(wire_m.clone()),
                                Transform::from_translation(mid)
                                    .with_rotation(rot)
                                    .with_scale(Vec3::new(len, 0.012, 0.012)),
                            ));
                        }
                    }
                }
            }
        });
}

fn spawn_wall_switch(
    commands: &mut Commands,
    palette: &mut WorldPalette,
    materials: &mut Assets<StandardMaterial>,
    spec: &LayoutSpec,
) {
    // Exact Oracle `switches.js:23`: Pro low wall by lobby doors, Cozy left wall beside door
    let pos = if spec.layout.is_pro() {
        Vec3::new(-3.515, 1.0, 2.3)
    } else {
        Vec3::new(-7.93, 1.2, 0.2)
    };
    let plate_m = palette.mat(materials, "#F4F1EA", 0.5, 0.0);
    let lever_m = palette.mat(materials, "#E2DED5", 0.4, 0.0);

    let root = commands
        .spawn((
            WorldEntity,
            InteractiveProp::new("switches", "office", "Light Switch"),
            Transform::from_translation(pos).with_rotation(Quat::from_rotation_y(FRAC_PI_2)),
            Visibility::default(),
        ))
        .id();

    let mut lever_ent = Entity::PLACEHOLDER;
    commands.entity(root).with_children(|g| {
        g.spawn((
            WorldEntity,
            Mesh3d(palette.unit_box.clone()),
            MeshMaterial3d(plate_m),
            Transform::default().with_scale(Vec3::new(0.11, 0.16, 0.016)),
        ));
        lever_ent = g
            .spawn((
                WorldEntity,
                SwitchLever,
                Transform::from_xyz(0.0, 0.0, 0.01)
                    .with_rotation(Quat::from_rotation_x(-0.45)),
                Visibility::default(),
            ))
            .with_children(|l| {
                l.spawn((
                    WorldEntity,
                    Mesh3d(palette.unit_box.clone()),
                    MeshMaterial3d(lever_m),
                    Transform::from_xyz(0.0, 0.012, 0.004)
                        .with_scale(Vec3::new(0.032, 0.05, 0.018)),
                ));
            })
            .id();
    });

    commands.entity(root).insert(WallSwitch {
        off: false,
        lever: lever_ent,
    });
}

fn spawn_dev_server_pool(
    commands: &mut Commands,
    palette: &mut WorldPalette,
    materials: &mut Assets<StandardMaterial>,
    images: &mut Assets<Image>,
    fonts: &mut FontEngine,
    spec: &LayoutSpec,
) {
    let [lx, lz, lry] = spec.lost;
    let body_m = palette.mat(materials, "#232830", 0.5, 0.45);
    let vent_m = palette.mat(materials, "#3A414C", 0.5, 0.25);
    let ring_m = palette.emissive(materials, "#FF4D4D", 1.3);
    let badge_m = palette.textured_unlit(images, materials, draw_dev_server_badge(Some(4310), fonts));

    // Reusable pool of 8 DevServer mini boxes (`dev-servers.js:15-30`); slot 0 shown in preview mode
    for slot in 0..8 {
        let led_mat = palette.owned_emissive(materials, "#3DDC84", 1.5);
        let init_vis = if slot == 0 {
            Visibility::Inherited
        } else {
            Visibility::Hidden
        };
        let root = commands
            .spawn((
                WorldEntity,
                InteractiveProp::new("dev-servers", "server", "Dev Server"),
                Transform::from_xyz(lx, 0.0, lz)
                    .with_rotation(Quat::from_rotation_y(lry))
                    .with_scale(Vec3::splat(1.7)),
                init_vis,
            ))
            .id();

        let mut ring_ent = Entity::PLACEHOLDER;
        commands.entity(root).with_children(|g| {
            g.spawn((
                WorldEntity,
                Mesh3d(palette.unit_box.clone()),
                MeshMaterial3d(body_m.clone()),
                Transform::from_xyz(0.0, 0.11, 0.0).with_scale(Vec3::new(0.16, 0.22, 0.18)),
            ));
            for r in 0..3 {
                g.spawn((
                    WorldEntity,
                    Mesh3d(palette.unit_box.clone()),
                    MeshMaterial3d(vent_m.clone()),
                    Transform::from_xyz(0.0, 0.06 + r as f32 * 0.035, 0.091)
                        .with_scale(Vec3::new(0.12, 0.012, 0.004)),
                ));
            }
            // Front & top status LED (`dev-servers.js:20-21`)
            g.spawn((
                WorldEntity,
                Mesh3d(palette.unit_box.clone()),
                MeshMaterial3d(led_mat.clone()),
                Transform::from_xyz(-0.045, 0.185, 0.091).with_scale(Vec3::new(0.03, 0.03, 0.006)),
            ));
            g.spawn((
                WorldEntity,
                Mesh3d(palette.unit_box.clone()),
                MeshMaterial3d(led_mat.clone()),
                Transform::from_xyz(0.0, 0.224, 0.03).with_scale(Vec3::new(0.12, 0.008, 0.035)),
            ));
            // Port badge quad (`dev-servers.js:26-27`)
            g.spawn((
                WorldEntity,
                Mesh3d(palette.unit_plane.clone()),
                MeshMaterial3d(badge_m.clone()),
                Transform::from_xyz(0.012, 0.185, 0.094).with_scale(Vec3::new(0.13, 0.043, 1.0)),
            ));
            // Pulsing red alert floor ring for forgotten servers (`dev-servers.js:22-23`)
            ring_ent = g
                .spawn((
                    WorldEntity,
                    DevServerRing,
                    Mesh3d(palette.thin_ring.clone()),
                    MeshMaterial3d(ring_m.clone()),
                    Transform::from_xyz(0.0, 0.008, 0.0).with_scale(Vec3::new(0.40, 0.04, 0.40)),
                    Visibility::Hidden,
                ))
                .id();
        });

        commands.entity(root).insert(DevServerBox {
            slot,
            lost: false,
            cpu_pct: 12.0,
            phase: slot as f32 * 0.9,
            led_mat,
            ring_ent,
            lost_anchor: Vec3::new(lx, 0.0, lz),
            lost_ry: lry,
            desks: spec.desks.clone(),
        });
    }
}

fn spawn_robot_vacuum(
    commands: &mut Commands,
    palette: &mut WorldPalette,
    materials: &mut Assets<StandardMaterial>,
    spec: &LayoutSpec,
) {
    // Exact Oracle `navigation.js` patrol loop waypoints for Pro/HQ & Cozy (`vacuum.js:13-31`)
    let waypoints = if spec.layout.is_pro() {
        vec![
            Vec2::new(-2.05, 3.65),
            Vec2::new(-0.51, 3.65),
            Vec2::new(2.85, 3.65),
            Vec2::new(6.21, 3.65),
            Vec2::new(7.75, 3.65),
            Vec2::new(7.75, 0.05),
            Vec2::new(2.85, 0.05),
            Vec2::new(-2.05, 0.05),
        ]
    } else {
        vec![
            Vec2::new(0.35, 4.05),
            Vec2::new(-3.05, 3.95),
            Vec2::new(-3.05, 1.45),
            Vec2::new(0.35, 1.45),
            Vec2::new(3.75, 1.45),
            Vec2::new(3.35, 4.45),
        ]
    };

    let start = waypoints[0];
    let body_m = palette.mat(materials, "#2E3238", 0.4, 0.3);
    let lid_m = palette.mat(materials, "#4A505A", 0.3, 0.4);
    let bumper_m = palette.mat(materials, "#1C1F24", 0.5, 0.15);
    let brush_m = palette.mat(materials, "#5A626E", 0.6, 0.1);
    let led_mat = palette.owned_emissive(materials, "#3DDC84", 1.5);

    commands
        .spawn((
            WorldEntity,
            RobotVacuum {
                waypoints,
                cur_idx: 0,
                progress: 0.0,
                spin_left: 0.0,
                led_mat: led_mat.clone(),
            },
            InteractiveProp::new("vacuum", "office", "Robot Vacuum"),
            Transform::from_xyz(start.x, 0.0, start.y),
            Visibility::default(),
        ))
        .with_children(|g| {
            // Chassis, top lid, front bumper, 2 spinning side brushes, and parented VacuumLed (`vacuum.js:26-29`)
            g.spawn((
                WorldEntity,
                Mesh3d(palette.unit_cyl.clone()),
                MeshMaterial3d(body_m),
                Transform::from_xyz(0.0, 0.045, 0.0).with_scale(Vec3::new(0.49, 0.08, 0.49)),
            ));
            g.spawn((
                WorldEntity,
                Mesh3d(palette.unit_cyl.clone()),
                MeshMaterial3d(lid_m),
                Transform::from_xyz(0.0, 0.09, 0.0).with_scale(Vec3::new(0.40, 0.012, 0.40)),
            ));
            g.spawn((
                WorldEntity,
                Mesh3d(palette.unit_box.clone()),
                MeshMaterial3d(bumper_m),
                Transform::from_xyz(0.0, 0.06, 0.20).with_scale(Vec3::new(0.30, 0.035, 0.05)),
            ));
            for (sx, dir) in [(-0.18_f32, 1.0_f32), (0.18, -1.0)] {
                g.spawn((
                    WorldEntity,
                    VacuumBrush { dir },
                    Mesh3d(palette.unit_box.clone()),
                    MeshMaterial3d(brush_m.clone()),
                    Transform::from_xyz(sx, 0.015, 0.15).with_scale(Vec3::new(0.10, 0.008, 0.018)),
                ));
            }
            g.spawn((
                WorldEntity,
                VacuumLed,
                Mesh3d(palette.unit_sphere.clone()),
                MeshMaterial3d(led_mat),
                Transform::from_xyz(0.0, 0.10, 0.12).with_scale(Vec3::splat(0.042)),
            ));
        });
}

#[derive(SystemParam)]
#[allow(clippy::type_complexity)]
pub struct AquariumAnimParams<'w, 's> {
    tanks: Query<'w, 's, &'static mut AquariumTank>,
    fish: Query<
        'w,
        's,
        (&'static AquariumFish, &'static mut Transform),
        (
            Without<AquariumFishTail>,
            Without<AquariumWeed>,
            Without<AquariumBubble>,
            Without<AquariumFlake>,
            Without<RobotVacuum>,
            Without<VacuumBrush>,
            Without<PrinterPaper>,
            Without<DevServerBox>,
            Without<DevServerRing>,
            Without<SwitchLever>,
        ),
    >,
    fish_tails: Query<
        'w,
        's,
        (&'static AquariumFishTail, &'static mut Transform),
        (
            Without<AquariumFish>,
            Without<AquariumWeed>,
            Without<AquariumBubble>,
            Without<AquariumFlake>,
            Without<RobotVacuum>,
            Without<VacuumBrush>,
            Without<PrinterPaper>,
            Without<DevServerBox>,
            Without<DevServerRing>,
            Without<SwitchLever>,
        ),
    >,
    weeds: Query<
        'w,
        's,
        (&'static AquariumWeed, &'static mut Transform),
        (
            Without<AquariumFish>,
            Without<AquariumFishTail>,
            Without<AquariumBubble>,
            Without<AquariumFlake>,
            Without<RobotVacuum>,
            Without<VacuumBrush>,
            Without<PrinterPaper>,
            Without<DevServerBox>,
            Without<DevServerRing>,
            Without<SwitchLever>,
        ),
    >,
    bubbles: Query<
        'w,
        's,
        (&'static AquariumBubble, &'static mut Transform),
        (
            Without<AquariumFish>,
            Without<AquariumFishTail>,
            Without<AquariumWeed>,
            Without<AquariumFlake>,
            Without<RobotVacuum>,
            Without<VacuumBrush>,
            Without<PrinterPaper>,
            Without<DevServerBox>,
            Without<DevServerRing>,
            Without<SwitchLever>,
        ),
    >,
    flakes: Query<
        'w,
        's,
        (Entity, &'static mut AquariumFlake, &'static mut Transform),
        (
            Without<AquariumFish>,
            Without<AquariumFishTail>,
            Without<AquariumWeed>,
            Without<AquariumBubble>,
            Without<RobotVacuum>,
            Without<VacuumBrush>,
            Without<PrinterPaper>,
            Without<DevServerBox>,
            Without<DevServerRing>,
            Without<SwitchLever>,
        ),
    >,
}

#[derive(SystemParam)]
#[allow(clippy::type_complexity)]
pub struct DevicePropsAnimParams<'w, 's> {
    vacs: Query<
        'w,
        's,
        (&'static mut RobotVacuum, &'static mut Transform),
        (
            Without<AquariumFish>,
            Without<AquariumFishTail>,
            Without<AquariumWeed>,
            Without<AquariumBubble>,
            Without<AquariumFlake>,
            Without<VacuumBrush>,
            Without<PrinterPaper>,
            Without<DevServerBox>,
            Without<DevServerRing>,
            Without<SwitchLever>,
        ),
    >,
    brushes: Query<
        'w,
        's,
        (&'static VacuumBrush, &'static mut Transform),
        (
            Without<AquariumFish>,
            Without<AquariumFishTail>,
            Without<AquariumWeed>,
            Without<AquariumBubble>,
            Without<AquariumFlake>,
            Without<RobotVacuum>,
            Without<PrinterPaper>,
            Without<DevServerBox>,
            Without<DevServerRing>,
            Without<SwitchLever>,
        ),
    >,
    printers: Query<'w, 's, &'static mut OfficePrinter>,
    papers: Query<
        'w,
        's,
        (&'static mut Transform, &'static mut Visibility),
        (
            With<PrinterPaper>,
            Without<AquariumFish>,
            Without<AquariumFishTail>,
            Without<AquariumWeed>,
            Without<AquariumBubble>,
            Without<AquariumFlake>,
            Without<RobotVacuum>,
            Without<VacuumBrush>,
            Without<DevServerBox>,
            Without<DevServerRing>,
            Without<SwitchLever>,
            Without<StringLightsRoot>,
        ),
    >,
    dev_boxes: Query<
        'w,
        's,
        (&'static mut DevServerBox, &'static mut Transform, &'static mut Visibility),
        (
            Without<AquariumFish>,
            Without<AquariumFishTail>,
            Without<AquariumWeed>,
            Without<AquariumBubble>,
            Without<AquariumFlake>,
            Without<RobotVacuum>,
            Without<VacuumBrush>,
            Without<PrinterPaper>,
            Without<DevServerRing>,
            Without<SwitchLever>,
            Without<StringLightsRoot>,
        ),
    >,
    dev_rings: Query<
        'w,
        's,
        (&'static mut Transform, &'static mut Visibility),
        (
            With<DevServerRing>,
            Without<AquariumFish>,
            Without<AquariumFishTail>,
            Without<AquariumWeed>,
            Without<AquariumBubble>,
            Without<AquariumFlake>,
            Without<RobotVacuum>,
            Without<VacuumBrush>,
            Without<PrinterPaper>,
            Without<DevServerBox>,
            Without<SwitchLever>,
            Without<StringLightsRoot>,
        ),
    >,
    string_lights: Query<
        'w,
        's,
        (&'static mut StringLightsRoot, &'static mut Visibility),
        (Without<PrinterPaper>, Without<DevServerBox>, Without<DevServerRing>),
    >,
    switches: Query<'w, 's, &'static mut WallSwitch>,
    levers: Query<
        'w,
        's,
        &'static mut Transform,
        (
            With<SwitchLever>,
            Without<AquariumFish>,
            Without<AquariumFishTail>,
            Without<AquariumWeed>,
            Without<AquariumBubble>,
            Without<AquariumFlake>,
            Without<RobotVacuum>,
            Without<VacuumBrush>,
            Without<PrinterPaper>,
            Without<DevServerBox>,
            Without<DevServerRing>,
        ),
    >,
}

/// Animates `RobotVacuum` (with parented `VacuumLed` & brushes), `Aquarium` (tangent-facing fish,
/// wagging tails, swaying seaweeds, rising bubbles, click flake feeding), `OfficePrinter` paper eject,
/// live `DevServerBox`es, seasonal `StringLights`, and working `WallSwitch`es (zero per-frame allocs).
#[allow(clippy::too_many_arguments)]
pub fn animate_props_system(
    mut commands: Commands,
    time: Res<Time>,
    mut prop_ev: EventReader<PropActivated>,
    mut lighting: Option<ResMut<LightingState>>,
    life: Option<Res<OfficeLifeState>>,
    telemetry: Option<Res<PropsLiveTelemetry>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
    mut aq: AquariumAnimParams,
    mut dev: DevicePropsAnimParams,
) {
    let dt = time.delta_secs().min(0.1);
    let t = time.elapsed_secs();

    let mut feed_fish = false;
    let mut spin_vac = false;
    let mut print_click = false;
    let mut toggle_switch = false;
    for ev in prop_ev.read() {
        match ev.id {
            "aquarium" => feed_fish = true,
            "vacuum" => spin_vac = true,
            "printer" => print_click = true,
            "switches" | "switch" => toggle_switch = true,
            _ => {}
        }
    }

    let is_night = lighting.as_deref().is_some_and(LightingState::is_night);
    let season = life.as_deref().map(|l| l.season).unwrap_or(SeasonTheme::Normal);

    // 1. Wall switches (`switches.js`): flip lever & toggle `LightingState.room_lights_off`
    if toggle_switch {
        let mut next_off = None;
        for mut sw in &mut dev.switches {
            sw.off = !sw.off;
            next_off = Some(sw.off);
            if let Ok(mut ltf) = dev.levers.get_mut(sw.lever) {
                ltf.rotation = Quat::from_rotation_x(if sw.off { 0.45 } else { -0.45 });
            }
        }
        if let (Some(off), Some(ref mut lstate)) = (next_off, lighting.as_mut()) {
            lstate.room_lights_off = off;
        }
    }

    // 2. Seasonal string lights (`string-lights.js`): auto-on at night or festive seasons
    for (mut sl, mut vis) in &mut dev.string_lights {
        *vis = if is_night || season != SeasonTheme::Normal {
            Visibility::Inherited
        } else {
            Visibility::Hidden
        };
        if sl.applied_season != Some(season) {
            sl.applied_season = Some(season);
            let cols = season_bulb_colors(season);
            for (k, mat_h) in sl.bulb_mats.iter().enumerate() {
                if let Some(m) = materials.get_mut(mat_h) {
                    m.emissive = cols[k];
                }
            }
        }
    }

    // 3. Aquarium: flake feeding, night light boost, tangent-facing fish, tails, seaweeds, bubbles
    let mut feed_k = 0.0_f32;
    for mut tank in &mut aq.tanks {
        if feed_fish {
            tank.feed_left = 7.0;
            for i in 0..12 {
                let fx = ((i as f32 * 1.9 + t).sin()) * 0.65;
                let fz = ((i as f32 * 2.7 + t).cos()) * 0.14;
                commands.spawn((
                    WorldEntity,
                    AquariumFlake {
                        fall_speed: 0.045 + (i % 4) as f32 * 0.012,
                        life: 5.0 + (i % 3) as f32 * 0.7,
                    },
                    Mesh3d(tank.flake_mesh.clone()),
                    MeshMaterial3d(tank.flake_mat.clone()),
                    Transform::from_xyz(
                        tank.center.x + fx,
                        1.42 + (i % 3) as f32 * 0.015,
                        tank.center.y + fz,
                    )
                    .with_scale(Vec3::new(0.03, 0.008, 0.03)),
                ));
            }
        }
        if tank.feed_left > 0.0 {
            tank.feed_left = (tank.feed_left - dt).max(0.0);
        }
        let target_k = if tank.feed_left > 0.0 { 1.0 } else { 0.0 };
        tank.feed_k += (target_k - tank.feed_k) * (dt * 1.6).min(1.0);
        feed_k = tank.feed_k;

        if let Some(m) = materials.get_mut(&tank.light_mat) {
            m.emissive = if is_night {
                LinearRgba::new(1.4, 2.0, 2.4, 1.0)
            } else {
                LinearRgba::new(0.65, 0.85, 0.95, 1.0)
            };
        }
    }

    for (f, mut tf) in &mut aq.fish {
        let a = t * f.speed + f.phase;
        let x = a.sin() * f.ax;
        let z = (a * 0.7).cos() * f.az;
        let fy = f.y0 + (a * 1.3).sin() * f.ay;
        let y = fy + (1.34 - fy) * feed_k * 0.9;
        tf.translation = Vec3::new(x, y, z);

        // Exact horizontal velocity tangent (`dx/dt`, `dz/dt`) -> fish +X nose faces tangent
        let vx = a.cos() * f.ax * f.speed;
        let vz = -0.7 * (a * 0.7).sin() * f.az * f.speed;
        if vx * vx + vz * vz > 1e-8 {
            tf.rotation = Quat::from_rotation_y((-vz).atan2(vx));
        }
    }
    for (tail, mut tf) in &mut aq.fish_tails {
        let wag = (t * 10.0 + tail.phase).sin() * 0.5;
        tf.rotation = Quat::from_rotation_y(wag) * Quat::from_rotation_z(-FRAC_PI_2);
    }
    for (w, mut tf) in &mut aq.weeds {
        tf.rotation = Quat::from_rotation_z((t * 0.9 + w.phase).sin() * 0.12);
    }
    for (b, mut tf) in &mut aq.bubbles {
        tf.translation.y += dt * (0.18 + (b.index % 3) as f32 * 0.05);
        tf.translation.x = b.base_x + (t * 3.0 + b.index as f32).sin() * 0.015;
        if tf.translation.y > 1.36 {
            tf.translation.y = 0.80;
        }
    }
    for (ent, mut fl, mut tf) in &mut aq.flakes {
        fl.life -= dt;
        if fl.life <= 0.0 {
            commands.entity(ent).try_despawn();
            continue;
        }
        tf.translation.y = (tf.translation.y - fl.fall_speed * dt).max(0.84);
        tf.rotate_y(dt * 1.2);
    }

    // 4. Patrolling RobotVacuum with parented VacuumLed & spinning side brushes (`vacuum.js`)
    for (mut v, mut tf) in &mut dev.vacs {
        if spin_vac {
            v.spin_left = 1.3;
        }
        if v.spin_left > 0.0 {
            v.spin_left = (v.spin_left - dt).max(0.0);
            tf.rotate_y(dt * 11.0);
            if let Some(m) = materials.get_mut(&v.led_mat) {
                let red = ((t * 10.0).floor() as i32) % 2 == 0;
                m.emissive = if red {
                    LinearRgba::new(2.0, 0.35, 0.35, 1.0)
                } else {
                    LinearRgba::new(0.3, 1.7, 0.7, 1.0)
                };
            }
            continue;
        }
        if v.waypoints.len() >= 2 {
            let n = v.waypoints.len();
            let a = v.waypoints[v.cur_idx % n];
            let b = v.waypoints[(v.cur_idx + 1) % n];
            let len = a.distance(b).max(0.1);
            v.progress += (0.42 * dt) / len;
            if v.progress >= 1.0 {
                v.progress -= 1.0;
                v.cur_idx = (v.cur_idx + 1) % n;
            }
            let p = v.waypoints[v.cur_idx % n];
            let q = v.waypoints[(v.cur_idx + 1) % n];
            let pos = p.lerp(q, v.progress);
            tf.translation = Vec3::new(pos.x, 0.0, pos.y);
            let dir = q - p;
            if dir.length_squared() > 1e-6 {
                let target_yaw = dir.x.atan2(dir.y);
                let cur_yaw = tf.rotation.to_euler(EulerRot::YXZ).0;
                let next_yaw = cur_yaw + wrap_angle(target_yaw - cur_yaw) * (dt * 4.0).min(1.0);
                tf.rotation = Quat::from_rotation_y(next_yaw);
            }
        }
        if let Some(m) = materials.get_mut(&v.led_mat) {
            let pulse = if (t * 2.0) % 2.0 < 1.6 {
                if is_night { 2.2 } else { 1.1 }
            } else {
                0.2
            };
            m.emissive = LinearRgba::new(0.24 * pulse, 0.86 * pulse, 0.52 * pulse, 1.0);
        }
    }
    for (brush, mut tf) in &mut dev.brushes {
        tf.rotate_y(dt * 9.0 * brush.dir);
    }

    // 5. OfficePrinter paper eject (`printer.js` & `update.js:28-38`)
    let ext_print = telemetry.as_deref().map(|tel| tel.print_requested).unwrap_or(0);
    for mut prn in &mut dev.printers {
        if print_click {
            prn.queue = (prn.queue + 1).min(3);
        }
        if ext_print != prn.last_trigger {
            prn.last_trigger = ext_print;
            prn.queue = (prn.queue + 1).min(3);
        }
        if prn.timer < 0.0 && prn.queue > 0 {
            prn.queue -= 1;
            prn.timer = 0.0;
            if let Ok((_, mut vis)) = dev.papers.get_mut(prn.paper_ent) {
                *vis = Visibility::Inherited;
            }
        }
        if prn.timer >= 0.0 {
            prn.timer += dt;
            let k = (prn.timer / 1.4).min(1.0);
            if let Ok((mut ptf, mut vis)) = dev.papers.get_mut(prn.paper_ent) {
                ptf.translation = Vec3::new(
                    0.0,
                    0.83 + if k > 0.85 { (k - 0.85) * 0.45 } else { 0.0 },
                    0.05 + k * 0.32,
                );
                if prn.timer > 2.9 {
                    *vis = Visibility::Hidden;
                }
            }
            if let Some(m) = materials.get_mut(&prn.led_mat) {
                let blink = ((t * 8.0).floor() as i32) % 2 == 0;
                m.emissive = if blink {
                    LinearRgba::new(0.24 * 1.8, 0.86 * 1.8, 0.52 * 1.8, 1.0)
                } else {
                    LinearRgba::new(0.10, 0.36, 0.22, 1.0)
                };
            }
            if prn.timer > 2.9 {
                prn.timer = -1.0;
                if let Some(m) = materials.get_mut(&prn.led_mat) {
                    m.emissive = LinearRgba::new(0.24 * 1.5, 0.86 * 1.5, 0.52 * 1.5, 1.0);
                }
            }
        }
    }

    // 6. Live per-owner & forgotten DevServer mini boxes (`dev-servers.js`)
    if let Some(ref tel) = telemetry
        && tel.live_synced
    {
        let mut per_desk = [0usize; 64];
        let mut lost_count = 0usize;
        for (mut bx, mut tf, mut vis) in &mut dev.dev_boxes {
            let Some(srv) = tel.dev_servers.get(bx.slot) else {
                *vis = Visibility::Hidden;
                continue;
            };
            bx.cpu_pct = srv.cpu_pct;
            if srv.alive
                && let Some(d_idx) = srv.desk_index
                && let Some(desk) = bx.desks.get(d_idx)
                && d_idx < per_desk.len()
                && per_desk[d_idx] < 2
            {
                let k = per_desk[d_idx];
                per_desk[d_idx] += 1;
                let pos = desk_local(desk, -0.60 + k as f32 * 0.22, 0.775, 0.25);
                tf.translation = pos;
                tf.rotation = Quat::from_rotation_y(desk.ry);
                tf.scale = Vec3::splat(1.25);
                bx.lost = false;
                *vis = Visibility::Inherited;
                if let Ok((_, mut rvis)) = dev.dev_rings.get_mut(bx.ring_ent) {
                    *rvis = Visibility::Hidden;
                }
            } else if !srv.alive && lost_count < 4 {
                tf.translation = Vec3::new(
                    bx.lost_anchor.x,
                    0.0,
                    bx.lost_anchor.z + lost_count as f32 * 0.36,
                );
                tf.rotation = Quat::from_rotation_y(bx.lost_ry);
                tf.scale = Vec3::splat(1.7);
                bx.lost = true;
                lost_count += 1;
                *vis = Visibility::Inherited;
                if let Ok((_, mut rvis)) = dev.dev_rings.get_mut(bx.ring_ent) {
                    *rvis = Visibility::Inherited;
                }
            } else {
                *vis = Visibility::Hidden;
            }
        }
    }

    for (bx, _, vis) in &dev.dev_boxes {
        if *vis == Visibility::Hidden {
            continue;
        }
        if let Some(m) = materials.get_mut(&bx.led_mat) {
            if bx.lost {
                let on = (t * 3.0 + bx.phase).sin() > 0.0;
                m.emissive = if on {
                    LinearRgba::new(2.2, 0.3, 0.3, 1.0)
                } else {
                    LinearRgba::new(0.35, 0.08, 0.08, 1.0)
                };
            } else {
                let busy = bx.cpu_pct > 20.0;
                let hi = !busy || (t * 14.0 + bx.phase).sin() > 0.0;
                m.emissive = if hi {
                    LinearRgba::new(0.24 * 1.6, 0.86 * 1.6, 0.52 * 1.6, 1.0)
                } else {
                    LinearRgba::new(0.08, 0.32, 0.18, 1.0)
                };
            }
        }
        if bx.lost
            && let Ok((mut rtf, _)) = dev.dev_rings.get_mut(bx.ring_ent)
        {
            let k = (t * 0.8 + bx.phase).fract();
            let s = 0.40 * (1.0 + k * 0.9);
            rtf.scale = Vec3::new(s, 0.04, s);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::backdrops::BackdropKind;
    use crate::chibi::ChibiLook;
    use crate::interact::PropActivated;
    use crate::layout::OfficeLayout;
    use crate::lights::{LightingState, RoomLamp, sync_lighting_system};
    use crate::props_detail::cat::{
        CatHeart, CatLeg, CatTailSeg, OfficeCat as ArticulatedCat, animate_cat_system,
    };
    use crate::props_detail::microwave::{
        ALL_LUNCHBOX_KINDS, MicrowaveDoorPivot, MicrowaveLunchbox, MicrowaveOven, MicrowavePlate,
        animate_microwaves_system,
    };
    use crate::props_detail::shredder_cups::{
        CacheCup, DeskContextStack, PaperShredder, WasteBin, animate_shredder_bins_and_cups_system,
    };
    use crate::props_detail::{DeskPropTelemetry, PropsLiveTelemetry};
    use crate::{
        BackdropAssets, OfficeIbl, SharedFontEngine, spawn_full_office_world,
        unload_full_office_world,
    };
    use bevy::transform::TransformPlugin;
    use std::path::PathBuf;
    use std::time::Duration;

    fn build_pro_test_app() -> App {
        let assets_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../assets");
        let mut app = App::new();
        app.add_plugins(TransformPlugin)
            .insert_resource(Time::<()>::default())
            .insert_resource(Assets::<Mesh>::default())
            .insert_resource(Assets::<StandardMaterial>::default())
            .insert_resource(Assets::<Image>::default())
            .insert_resource(SharedFontEngine::new(assets_dir.clone()))
            .insert_resource(PropsLiveTelemetry::default())
            .insert_resource(crate::life::OfficeLifeState::default())
            .add_event::<PropActivated>()
            .add_systems(
                Update,
                (
                    animate_props_system,
                    animate_cat_system,
                    animate_microwaves_system,
                    animate_shredder_bins_and_cups_system,
                    sync_lighting_system,
                ),
            );

        app.world_mut()
            .resource_scope(|world, mut meshes: Mut<Assets<Mesh>>| {
                world.resource_scope(|world, mut mats: Mut<Assets<StandardMaterial>>| {
                    world.resource_scope(|world, mut imgs: Mut<Assets<Image>>| {
                        world.resource_scope(|world, mut shared_fonts: Mut<SharedFontEngine>| {
                            let fonts = shared_fonts.get_or_init();
                            let mut cmds = world.commands();
                            spawn_full_office_world(
                                &mut cmds,
                                &mut meshes,
                                &mut mats,
                                &mut imgs,
                                fonts,
                                &assets_dir,
                                OfficeLayout::Pro,
                                BackdropKind::City,
                                ChibiLook::Pixar,
                                true,
                            );
                        });
                    });
                });
            });
        app.world_mut().flush();
        app
    }

    fn step_app(app: &mut App, dt_secs: f32) {
        app.world_mut()
            .resource_mut::<Time<()>>()
            .advance_by(Duration::from_secs_f32(dt_secs));
        app.update();
    }

    fn count_descendant_meshes(world: &World, root: Entity) -> usize {
        let mut stack = vec![root];
        let mut count = 0usize;
        while let Some(e) = stack.pop() {
            if e != root && world.get::<Mesh3d>(e).is_some() {
                count += 1;
            }
            if let Some(children) = world.get::<Children>(e) {
                stack.extend(children.iter());
            }
        }
        count
    }

    #[test]
    fn vacuum_led_is_child_of_vacuum_and_moves_together_in_update() {
        let mut app = build_pro_test_app();
        step_app(&mut app, 0.016);

        let (vac_ent, vac_start) = app
            .world_mut()
            .query_filtered::<(Entity, &GlobalTransform), With<RobotVacuum>>()
            .single(app.world())
            .map(|(e, gt)| (e, gt.translation()))
            .expect("RobotVacuum must exist");

        let (led_ent, led_parent) = app
            .world_mut()
            .query_filtered::<(Entity, &ChildOf), With<VacuumLed>>()
            .single(app.world())
            .expect("VacuumLed must exist and be a ChildOf RobotVacuum");
        assert_eq!(
            led_parent.parent(),
            vac_ent,
            "VacuumLed must be a direct ChildOf RobotVacuum"
        );

        for _ in 0..40 {
            step_app(&mut app, 0.1);
        }

        let vac_after = app
            .world()
            .get::<GlobalTransform>(vac_ent)
            .expect("vac gt")
            .translation();
        let led_after = app
            .world()
            .get::<GlobalTransform>(led_ent)
            .expect("led gt")
            .translation();

        assert!(
            vac_after.distance(vac_start) > 0.2,
            "RobotVacuum must patrol along walk graph, moved only {}",
            vac_after.distance(vac_start)
        );
        let sep = vac_after.distance(led_after);
        assert!(
            sep < 0.25,
            "VacuumLed must move with RobotVacuum (sep={sep:.3}m, vac={vac_after:?}, led={led_after:?})"
        );
    }

    #[test]
    fn office_cat_spawns_at_least_16_articulated_child_meshes_walks_wags_and_pets_with_cleanup() {
        let mut app = build_pro_test_app();
        step_app(&mut app, 0.016);

        let cat_ent = app
            .world_mut()
            .query_filtered::<Entity, With<ArticulatedCat>>()
            .single(app.world())
            .expect("Articulated OfficeCat must exist");

        let mesh_count = count_descendant_meshes(app.world(), cat_ent);
        assert!(
            mesh_count >= 16,
            "OfficeCat must spawn >= 16 articulated child meshes, got {mesh_count}"
        );
        let leg_count = app
            .world_mut()
            .query_filtered::<Entity, With<CatLeg>>()
            .iter(app.world())
            .count();
        assert_eq!(leg_count, 4, "OfficeCat must have 4 articulated legs");
        let tail_count = app
            .world_mut()
            .query_filtered::<Entity, With<CatTailSeg>>()
            .iter(app.world())
            .count();
        assert_eq!(tail_count, 6, "OfficeCat must have 6-joint wagging tail chain");

        let tail_rot_before: Vec<Quat> = app
            .world_mut()
            .query_filtered::<&Transform, With<CatTailSeg>>()
            .iter(app.world())
            .map(|t| t.rotation)
            .collect();

        for _ in 0..10 {
            step_app(&mut app, 0.12);
        }
        let tail_rot_after: Vec<Quat> = app
            .world_mut()
            .query_filtered::<&Transform, With<CatTailSeg>>()
            .iter(app.world())
            .map(|t| t.rotation)
            .collect();
        assert_ne!(
            tail_rot_before, tail_rot_after,
            "OfficeCat tail segments must wag during Update"
        );

        // Trigger pet via PropActivated("cat") -> spawns floating hearts that auto-clean up
        app.world_mut().send_event(PropActivated {
            id: "cat",
            place: "lounge",
            label: "Office Cat",
        });
        step_app(&mut app, 0.05);
        let hearts_spawned = app
            .world_mut()
            .query_filtered::<Entity, With<CatHeart>>()
            .iter(app.world())
            .count();
        assert!(
            hearts_spawned >= 3,
            "petting OfficeCat must spawn floating hearts, got {hearts_spawned}"
        );

        for _ in 0..30 {
            step_app(&mut app, 0.15);
        }
        let hearts_remaining = app
            .world_mut()
            .query_filtered::<Entity, With<CatHeart>>()
            .iter(app.world())
            .count();
        assert_eq!(
            hearts_remaining, 0,
            "floating CatHeart entities must despawn automatically after life expires"
        );
    }

    #[test]
    fn pro_spawns_dual_hollow_microwaves_with_door_pivot_plate_and_lunchboxes() {
        let mut app = build_pro_test_app();
        step_app(&mut app, 0.016);

        let ovens: Vec<(Entity, Entity, Entity)> = app
            .world_mut()
            .query::<(Entity, &MicrowaveOven)>()
            .iter(app.world())
            .map(|(e, mw)| (e, mw.door_pivot, mw.plate))
            .collect();
        assert_eq!(ovens.len(), 2, "Pro pantry must spawn 2 hollow MicrowaveOvens");
        assert_eq!(ALL_LUNCHBOX_KINDS.len(), 5);

        for (oven_ent, door_ent, plate_ent) in &ovens {
            assert_eq!(
                app.world().get::<ChildOf>(*door_ent).map(|c| c.parent()),
                Some(*oven_ent),
                "MicrowaveDoorPivot must be a ChildOf MicrowaveOven"
            );
            assert_eq!(
                app.world().get::<ChildOf>(*plate_ent).map(|c| c.parent()),
                Some(*oven_ent),
                "MicrowavePlate must be a ChildOf MicrowaveOven"
            );
            assert!(app.world().get::<MicrowaveDoorPivot>(*door_ent).is_some());
            assert!(app.world().get::<MicrowavePlate>(*plate_ent).is_some());
        }

        let lunchbox_count = app
            .world_mut()
            .query_filtered::<Entity, With<MicrowaveLunchbox>>()
            .iter(app.world())
            .count();
        assert!(
            lunchbox_count >= 5,
            "Pro microwaves/pantry must include all 5 typed lunchboxes, got {lunchbox_count}"
        );

        let plate_rot_0 = app.world().get::<Transform>(ovens[0].2).unwrap().rotation;
        step_app(&mut app, 0.4);
        let plate_rot_1 = app.world().get::<Transform>(ovens[0].2).unwrap().rotation;
        assert_ne!(
            plate_rot_0, plate_rot_1,
            "active MicrowavePlate must rotate during heating"
        );
    }

    #[test]
    fn per_desk_cups_bins_context_stacks_and_shredder_sync_and_unload_zero_leaks() {
        let mut app = build_pro_test_app();
        step_app(&mut app, 0.016);

        assert_eq!(
            app.world_mut().query::<&CacheCup>().iter(app.world()).count(),
            12,
            "Pro must spawn 12 per-desk terracotta CacheCups"
        );
        assert_eq!(
            app.world_mut().query::<&WasteBin>().iter(app.world()).count(),
            12,
            "Pro must spawn 12 per-desk WasteBins"
        );
        assert_eq!(
            app.world_mut()
                .query::<&DeskContextStack>()
                .iter(app.world())
                .count(),
            12,
            "Pro must spawn 12 per-desk DeskContextStacks"
        );
        assert_eq!(
            app.world_mut()
                .query::<&PaperShredder>()
                .iter(app.world())
                .count(),
            1,
            "Pro must spawn 1 transparent-window PaperShredder"
        );

        // Sync live telemetry: desk 0 >85% ctx & warm cache; desk 1 20% ctx & cold cache.
        {
            let mut tel = app.world_mut().resource_mut::<PropsLiveTelemetry>();
            tel.live_synced = true;
            tel.desks = vec![
                DeskPropTelemetry {
                    desk_index: 0,
                    occupied: true,
                    resting: true,
                    ctx_pct: 0.92,
                    cache_warmth: 0.8,
                },
                DeskPropTelemetry {
                    desk_index: 1,
                    occupied: true,
                    resting: true,
                    ctx_pct: 0.20,
                    cache_warmth: 0.0,
                },
            ];
        }
        step_app(&mut app, 0.1);

        let stacks: Vec<(usize, bool, Entity)> = app
            .world_mut()
            .query::<&DeskContextStack>()
            .iter(app.world())
            .map(|s| (s.desk_index, s.over_85, s.red_flag))
            .collect();
        let d0 = stacks.iter().find(|s| s.0 == 0).unwrap();
        let d1 = stacks.iter().find(|s| s.0 == 1).unwrap();
        assert!(d0.1, "desk 0 at 92% ctx must set over_85 red warning flag");
        assert_eq!(
            *app.world().get::<Visibility>(d0.2).unwrap(),
            Visibility::Inherited
        );
        assert!(!d1.1, "desk 1 at 20% ctx must not show red warning flag");
        assert_eq!(
            *app.world().get::<Visibility>(d1.2).unwrap(),
            Visibility::Hidden
        );

        // Full unload must purge 100% of entities, meshes, materials, and images with 0 leaks.
        let world_ents: Vec<Entity> = app
            .world_mut()
            .query_filtered::<Entity, With<WorldEntity>>()
            .iter(app.world())
            .collect();
        app.world_mut()
            .resource_scope(|world, mut meshes: Mut<Assets<Mesh>>| {
                world.resource_scope(|world, mut mats: Mut<Assets<StandardMaterial>>| {
                    world.resource_scope(|world, mut imgs: Mut<Assets<Image>>| {
                        let mut pal = world.remove_resource::<WorldPalette>();
                        let mut ba = world.remove_resource::<BackdropAssets>();
                        let ibl = world.remove_resource::<OfficeIbl>();
                        let mut cmds = world.commands();
                        unload_full_office_world(
                            &mut cmds,
                            world_ents,
                            pal.as_mut(),
                            ba.as_mut(),
                            ibl.as_ref(),
                            &mut meshes,
                            &mut mats,
                            &mut imgs,
                        );
                    });
                });
            });
        app.world_mut().flush();
        assert_eq!(
            app.world_mut()
                .query_filtered::<Entity, With<WorldEntity>>()
                .iter(app.world())
                .count(),
            0
        );
        assert_eq!(app.world().resource::<Assets<Mesh>>().len(), 0);
        assert_eq!(app.world().resource::<Assets<StandardMaterial>>().len(), 0);
        assert_eq!(app.world().resource::<Assets<Image>>().len(), 0);
    }

    #[test]
    fn aquarium_fish_face_velocity_tangent_and_switches_toggle_room_lamps() {
        let mut app = build_pro_test_app();
        step_app(&mut app, 0.016);

        let fish_count = app
            .world_mut()
            .query::<&AquariumFish>()
            .iter(app.world())
            .count();
        assert!(
            fish_count >= 5,
            "Aquarium must spawn >= 5 swimming fish, got {fish_count}"
        );
        assert!(
            app.world_mut()
                .query::<&AquariumWeed>()
                .iter(app.world())
                .count()
                >= 6,
            "Aquarium must spawn swaying seaweeds"
        );
        assert!(
            app.world_mut()
                .query::<&AquariumBubble>()
                .iter(app.world())
                .count()
                >= 8,
            "Aquarium must spawn rising air bubbles"
        );
        assert_eq!(
            app.world_mut()
                .query::<&StringLightsRoot>()
                .iter(app.world())
                .count(),
            1
        );
        assert_eq!(
            app.world_mut()
                .query::<&OfficePrinter>()
                .iter(app.world())
                .count(),
            1
        );

        // Check fish velocity tangent alignment between two frames
        let pos_a: Vec<(Entity, Vec3)> = app
            .world_mut()
            .query_filtered::<(Entity, &Transform), With<AquariumFish>>()
            .iter(app.world())
            .map(|(e, t)| (e, t.translation))
            .collect();
        step_app(&mut app, 0.08);
        let mut checked = 0usize;
        for (e, pa) in pos_a {
            let tf = app.world().get::<Transform>(e).unwrap();
            let vel = Vec2::new(tf.translation.x - pa.x, tf.translation.z - pa.z);
            if vel.length() > 1e-4 {
                let fwd3 = tf.rotation * Vec3::X;
                let fwd2 = Vec2::new(fwd3.x, fwd3.z).normalize_or_zero();
                let dot = fwd2.dot(vel.normalize());
                assert!(
                    dot > 0.85,
                    "AquariumFish must face its horizontal velocity tangent (dot={dot:.3})"
                );
                checked += 1;
            }
        }
        assert!(checked > 0);

        // Toggle wall switch at night -> flips WallSwitch.off & dims RoomLamps
        assert!(!app.world().resource::<LightingState>().room_lights_off);
        app.world_mut().send_event(PropActivated {
            id: "switches",
            place: "office",
            label: "Light Switch",
        });
        step_app(&mut app, 0.05);
        step_app(&mut app, 0.05);
        assert!(
            app.world().resource::<LightingState>().room_lights_off,
            "clicking wall switch must toggle LightingState.room_lights_off"
        );
        let all_lamps_off = app
            .world_mut()
            .query_filtered::<&PointLight, With<RoomLamp>>()
            .iter(app.world())
            .all(|l| l.intensity == 0.0);
        assert!(
            all_lamps_off,
            "flipping wall switch off must extinguish RoomLamps"
        );
    }
}
