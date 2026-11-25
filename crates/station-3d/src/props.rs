use crate::furniture::ServerFan;
use crate::interact::InteractiveProp;
use crate::layout::LayoutSpec;
use crate::palette::{
    WorldEntity, WorldPalette, spawn_box, spawn_box_rot, spawn_cyl, spawn_quad, spawn_sphere,
};
use bevy::prelude::*;
use station_canvas2d::boards_and_social::{DutyBoardInput, DutyRowInput, draw_duty_board};
use station_canvas2d::office_surfaces::{
    TvStatsInput, draw_arcade_frame, draw_arcade_marquee, draw_dev_server_badge,
    draw_game_tv_standby, draw_karaoke_band, draw_karaoke_qr_badge, draw_karaoke_screen,
    draw_microwave_timer, draw_stats_tv,
};
use station_canvas2d::FontEngine;
use std::f32::consts::PI;

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

#[derive(Component)]
pub struct AquariumFish {
    pub center: Vec3,
    pub radius: f32,
    pub speed: f32,
    pub phase: f32,
}

#[derive(Component)]
pub struct RobotVacuum {
    pub origin: Vec3,
    pub radius: f32,
    pub speed: f32,
}

#[derive(Component)]
pub struct OfficeCat {
    pub base_y: f32,
}

/// Spawns all 22 interactive office props with exact layout coordinates and canvas2d textures.
pub fn spawn_all_props(
    commands: &mut Commands,
    palette: &mut WorldPalette,
    materials: &mut Assets<StandardMaterial>,
    images: &mut Assets<Image>,
    fonts: &mut FontEngine,
    spec: &LayoutSpec,
) {
    let metal = palette.mat(materials, "#2B2F36", 0.35, 0.65);

    // 1. Live stats TV (tv.js)
    {
        let [tx, ty, tz, try_] = spec.tv;
        let rot = Quat::from_rotation_y(try_);
        let fwd = rot * Vec3::Z;
        let bezel = palette.mat(materials, "#15181E", 0.4, 0.15);
        let tv_ent = spawn_box_rot(commands, palette, bezel, Vec3::new(tx, ty, tz), Vec3::new(2.32, 1.34, 0.08), rot);
        commands.entity(tv_ent).insert(InteractiveProp::new("tv", "server", "Live Machine Stats TV"));

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
                pulse_30m: [2, 3, 1, 4, 6, 5, 3, 2, 7, 8, 4, 3, 5, 6, 8, 9, 7, 5, 3, 4, 6, 7, 8, 5, 4, 3, 6, 8, 7, 5],
            },
            fonts,
        );
        let tv_mat = palette.textured_unlit(images, materials, tv_surf);
        spawn_quad(commands, palette, tv_mat, Vec3::new(tx, ty, tz) + fwd * 0.048, Vec2::new(2.18, 1.2), rot);
    }

    // 2. Arcade cabinet + marquee + invaders screen (arcade.js)
    {
        let [ax, az] = spec.arc;
        let cab_m = palette.mat(materials, "#4B2E83", 0.55, 0.05);
        let trim_m = palette.emissive(materials, "#F72585", 1.2);
        let deck_m = palette.mat(materials, "#2A1B4F", 0.5, 0.05);
        let arc_ent = spawn_box(commands, palette, cab_m, Vec3::new(ax, 0.875, az), Vec3::new(0.8, 1.75, 0.7));
        commands.entity(arc_ent).insert(InteractiveProp::new("arcade", "lounge", "Arcade Cabinet"));
        for sx in [-0.405_f32, 0.405] {
            spawn_box(commands, palette, trim_m.clone(), Vec3::new(ax + sx, 0.95, az), Vec3::new(0.02, 1.5, 0.5));
        }
        spawn_box(commands, palette, deck_m, Vec3::new(ax, 0.98, az + 0.42), Vec3::new(0.76, 0.1, 0.32));
        let scr_mat = palette.textured_unlit(images, materials, draw_arcade_frame(1.8, false, fonts));
        spawn_quad(commands, palette, scr_mat, Vec3::new(ax, 1.38, az + 0.36), Vec2::new(0.58, 0.44), Quat::from_rotation_x(-0.18));
        let marq_mat = palette.textured_unlit(images, materials, draw_arcade_marquee(fonts));
        spawn_quad(commands, palette, marq_mat, Vec3::new(ax, 1.66, az + 0.36), Vec2::new(0.74, 0.19), Quat::IDENTITY);
    }

    // 3. Aquarium + animated fish (aquarium.js)
    {
        let [qx, qz] = spec.aq;
        let stand_m = palette.mat(materials, "#2B2F36", 0.55, 0.1);
        let water_m = palette.glass(materials, "#5FD4E8", 0.36);
        let tank = spawn_box(commands, palette, stand_m, Vec3::new(qx, 0.42, qz), Vec3::new(1.3, 0.84, 0.5));
        commands.entity(tank).insert(InteractiveProp::new("aquarium", "lounge", "Office Aquarium"));
        spawn_box(commands, palette, water_m, Vec3::new(qx, 1.12, qz), Vec3::new(1.24, 0.56, 0.44));
        let fish_cols = ["#FF7A3D", "#FFD166", "#5FA8FF", "#FF5F8F"];
        for (i, col) in fish_cols.iter().enumerate() {
            let fm = palette.emissive(materials, col, 0.9);
            commands.spawn((
                WorldEntity,
                AquariumFish {
                    center: Vec3::new(qx, 1.04 + (i % 2) as f32 * 0.14, qz),
                    radius: 0.35 + (i % 2) as f32 * 0.1,
                    speed: 1.2 + i as f32 * 0.35,
                    phase: i as f32 * 1.6,
                },
                Mesh3d(palette.unit_sphere.clone()),
                MeshMaterial3d(fm),
                Transform::from_xyz(qx, 1.1, qz).with_scale(Vec3::new(0.11, 0.06, 0.05)),
            ));
        }
    }

    // 4. Pro/HQ Lounge Game-TV + PS5 + controllers (game-tv.js, tv-games.js)
    if let Some([gx, gz]) = spec.game_tv {
        let walnut = palette.mat(materials, "#6B4A33", 0.6, 0.0);
        let oak = palette.mat(materials, "#B58A60", 0.55, 0.0);
        let black = palette.mat(materials, "#16181C", 0.35, 0.2);
        let white = palette.mat(materials, "#F4F5F7", 0.35, 0.0);
        let blue_led = palette.emissive(materials, "#6AAEFF", 1.5);

        let unit = spawn_box(commands, palette, walnut, Vec3::new(gx, 0.29, gz), Vec3::new(1.8, 0.42, 0.42));
        commands.entity(unit).insert(InteractiveProp::new("game-tv", "lounge", "Lounge Console TV"));
        spawn_box(commands, palette, oak, Vec3::new(gx, 0.515, gz), Vec3::new(1.82, 0.03, 0.44));
        spawn_box(commands, palette, black.clone(), Vec3::new(gx, 0.995, gz - 0.06), Vec3::new(1.25, 0.72, 0.04));
        let gtv_mat = palette.textured_unlit(images, materials, draw_game_tv_standby(fonts));
        spawn_quad(commands, palette, gtv_mat, Vec3::new(gx, 0.995, gz - 0.035), Vec2::new(1.22, 0.69), Quat::IDENTITY);
        // PS5 console + blue light strip + 2 controllers
        spawn_box(commands, palette, white.clone(), Vec3::new(gx + 0.74, 0.73, gz - 0.04), Vec3::new(0.14, 0.4, 0.26));
        spawn_box(commands, palette, blue_led, Vec3::new(gx + 0.81, 0.73, gz + 0.08), Vec3::new(0.015, 0.32, 0.015));
        spawn_box(commands, palette, white, Vec3::new(gx - 0.57, 0.55, gz + 0.11), Vec3::new(0.15, 0.03, 0.07));
        spawn_box(commands, palette, black, Vec3::new(gx + 0.55, 0.55, gz + 0.15), Vec3::new(0.15, 0.03, 0.07));
    }

    // 5. Pro/HQ Billiards table (billiards.js)
    if let Some([bx, bz]) = spec.bil {
        let wood = palette.mat(materials, "#5C3A24", 0.55, 0.0);
        let felt = palette.mat(materials, "#1F6E43", 0.9, 0.0);
        let tbl = spawn_box(commands, palette, wood, Vec3::new(bx, 0.78, bz), Vec3::new(1.42, 0.16, 2.32));
        commands.entity(tbl).insert(InteractiveProp::new("billiards", "lounge", "Pool Table"));
        spawn_box(commands, palette, felt, Vec3::new(bx, 0.87, bz), Vec3::new(1.22, 0.03, 2.12));
        for (ox, oz) in [(-0.55_f32, -0.95_f32), (0.55, -0.95), (-0.55, 0.95), (0.55, 0.95)] {
            spawn_cyl(commands, palette, metal.clone(), Vec3::new(bx + ox, 0.38, bz + oz), 0.1, 0.76);
        }
        for (i, col) in ["#FFFFFF", "#E63946", "#FFD166", "#3F7CC4", "#222222"].iter().enumerate() {
            let bm = palette.mat(materials, col, 0.2, 0.0);
            spawn_sphere(commands, palette, bm, Vec3::new(bx - 0.12 + (i % 3) as f32 * 0.1, 0.91, bz - 0.25 + (i / 3) as f32 * 0.15), Vec3::splat(0.065));
        }
    }

    // 6. Pro/HQ Karaoke booth (karaoke.js)
    if let Some([kx, kz, kry]) = spec.ktv {
        let rot = Quat::from_rotation_y(kry);
        let at = |lx: f32, ly: f32, lz: f32| Vec3::new(kx, 0.0, kz) + rot * Vec3::new(lx, ly, lz);
        let frame = palette.mat(materials, "#1D1F26", 0.35, 0.5);
        let felt = palette.mat(materials, "#3B2A5C", 0.95, 0.0);
        let glass = palette.glass(materials, "#D7E6F5", 0.16);
        let led = palette.emissive(materials, "#FF4FB0", 1.8);

        let ktv_ent = spawn_box_rot(commands, palette, frame.clone(), at(0.0, 2.24, 0.0), Vec3::new(1.28, 0.08, 1.32), rot);
        commands.entity(ktv_ent).insert(InteractiveProp::new("karaoke", "lounge", "Karaoke Booth"));
        spawn_box_rot(commands, palette, frame.clone(), at(0.0, 0.04, 0.0), Vec3::new(1.26, 0.08, 1.3), rot);
        spawn_box_rot(commands, palette, felt.clone(), at(0.0, 1.14, -0.59), Vec3::new(1.14, 2.13, 0.05), rot);
        spawn_box_rot(commands, palette, felt, at(0.57, 1.14, 0.0), Vec3::new(0.05, 2.13, 1.18), rot);
        spawn_box_rot(commands, palette, glass, at(-0.6, 1.17, 0.0), Vec3::new(0.02, 1.5, 1.18), rot);
        spawn_box_rot(commands, palette, led, at(0.0, 2.2, 0.66), Vec3::new(1.29, 0.025, 0.02), rot);

        let band_m = palette.textured_unlit(images, materials, draw_karaoke_band("KARAOKE", fonts));
        spawn_quad(commands, palette, band_m.clone(), at(-0.64, 2.05, 0.0), Vec2::new(1.14, 0.2), rot * Quat::from_rotation_y(-PI * 0.5));
        spawn_quad(commands, palette, band_m, at(0.0, 2.08, 0.64), Vec2::new(1.1, 0.2), rot);

        let qr_m = palette.textured_unlit(images, materials, draw_karaoke_qr_badge(fonts));
        spawn_quad(commands, palette, qr_m, at(-0.62, 0.62, 0.3), Vec2::new(0.3, 0.15), rot * Quat::from_rotation_y(-PI * 0.5));

        let ktv_scr = palette.textured_unlit(
            images,
            materials,
            draw_karaoke_screen("Agents Station KTV", "Lạc trôi giữa codebase Rust ♪", 0.55, fonts),
        );
        spawn_quad(commands, palette, ktv_scr, at(0.0, 1.42, -0.53), Vec2::new(0.76, 0.43), rot);
    }

    // 7. Pro/HQ Pantry Duty Board & Cup Pile (duty-board.js, cups.js) + Microwave (microwave.js)
    if spec.layout.is_pro() {
        let board_surf = draw_duty_board(
            &DutyBoardInput {
                title: "TRỰC NHẬT".into(),
                date: "25/11".into(),
                off_label: None,
                rows: vec![
                    DutyRowInput { chore_key: "cups".into(), chore_label: "Rửa ly".into(), person: "Mai".into(), done_at: Some("11:30".into()), busy: false },
                    DutyRowInput { chore_key: "trash".into(), chore_label: "Đổ rác".into(), person: "Long".into(), done_at: None, busy: true },
                    DutyRowInput { chore_key: "plants".into(), chore_label: "Tưới cây".into(), person: "An".into(), done_at: None, busy: false },
                    DutyRowInput { chore_key: "table".into(), chore_label: "Lau bàn".into(), person: "Huy".into(), done_at: None, busy: false },
                ],
                owed_footer: None,
            },
            fonts,
        );
        let db_mat = palette.textured_lit(images, materials, board_surf, 0.35);
        let db_ent = spawn_quad(commands, palette, db_mat, Vec3::new(11.0, 3.08, -7.94), Vec2::new(1.6, 1.09), Quat::IDENTITY);
        commands.entity(db_ent).insert(InteractiveProp::new("duty-board", "pantry", "Pantry Duty Board"));

        // Cups by sink
        let cup_m = palette.mat(materials, "#FFFFFF", 0.3, 0.0);
        for (i, ox) in [-0.14_f32, 0.0, 0.14].iter().enumerate() {
            spawn_cyl(commands, palette, cup_m.clone(), Vec3::new(12.55 + ox, 1.04 + (i % 2) as f32 * 0.02, -7.52), 0.08, 0.09);
        }

        // Microwave with green timer display
        let mw_m = palette.mat(materials, "#262A31", 0.4, 0.35);
        spawn_box(commands, palette, mw_m, Vec3::new(9.2, 1.15, -7.55), Vec3::new(0.56, 0.34, 0.4));
        let timer_m = palette.textured_unlit(images, materials, draw_microwave_timer(90, fonts));
        spawn_quad(commands, palette, timer_m, Vec3::new(9.38, 1.21, -7.34), Vec2::new(0.14, 0.06), Quat::IDENTITY);
    }

    // 8. Printer & Shredder (printer.js, shredder.js)
    {
        let [px, pz] = spec.prn;
        let prn_m = palette.mat(materials, "#E4E6EA", 0.5, 0.05);
        let dark_m = palette.mat(materials, "#2B2F36", 0.5, 0.2);
        let prn = spawn_box(commands, palette, prn_m, Vec3::new(px, 0.52, pz), Vec3::new(0.68, 0.92, 0.56));
        commands.entity(prn).insert(InteractiveProp::new("printer", "office", "Office Printer"));
        spawn_box(commands, palette, dark_m.clone(), Vec3::new(px + 0.55, 0.3, pz), Vec3::new(0.32, 0.56, 0.28)); // shredder
        spawn_cyl(commands, palette, dark_m, Vec3::new(px - 0.55, 0.22, pz), 0.28, 0.42); // bin
    }

    // 9. Neon Sign (neon.js) + String Lights (string-lights.js) + Wall Switch (switches.js)
    {
        let [nx, ny, nz, nry] = spec.neon;
        let rot = Quat::from_rotation_y(nry);
        let neon_m = palette.emissive(materials, "#FF7A4D", 2.2);
        let neon_ent = spawn_box_rot(commands, palette, neon_m, Vec3::new(nx, ny, nz), Vec3::new(2.1, 0.14, 0.05), rot);
        commands.entity(neon_ent).insert(InteractiveProp::new("neon", "office", "Neon Sign"));

        let fairy_m = palette.emissive(materials, "#FFE2A0", 1.8);
        for k in 0..8 {
            let lx = spec.bounds.x0 + 1.4 + k as f32 * 1.8;
            spawn_sphere(commands, palette, fairy_m.clone(), Vec3::new(lx, 3.85, spec.bounds.z0 + 0.18), Vec3::splat(0.08));
        }

        let sw_m = palette.mat(materials, "#F2EFEA", 0.4, 0.0);
        spawn_box(commands, palette, sw_m, Vec3::new(-7.95, 1.35, spec.door_z - 0.8), Vec3::new(0.03, 0.14, 0.1));
    }

    // 10. Mini Dev Server box with port badge (dev-servers.js)
    {
        let [lx, lz, lry] = spec.lost;
        let rot = Quat::from_rotation_y(lry);
        let body_m = palette.mat(materials, "#232830", 0.5, 0.45);
        let led_m = palette.emissive(materials, "#3DDC84", 1.6);
        let dev_ent = spawn_box_rot(commands, palette, body_m, Vec3::new(lx, 0.18, lz), Vec3::new(0.26, 0.36, 0.28), rot);
        commands.entity(dev_ent).insert(InteractiveProp::new("dev-servers", "server", "Dev Server :4310"));
        spawn_box_rot(commands, palette, led_m, Vec3::new(lx, 0.37, lz), Vec3::new(0.18, 0.02, 0.05), rot);
        let badge_m = palette.textured_unlit(images, materials, draw_dev_server_badge(Some(4310), fonts));
        spawn_quad(commands, palette, badge_m, Vec3::new(lx, 0.28, lz) + rot * Vec3::new(0.0, 0.0, 0.15), Vec2::new(0.22, 0.07), rot);
    }

    // 11. Office Cat (cat.js) + Patrolling Robot Vacuum (vacuum.js)
    {
        let cat_m = palette.mat(materials, "#E09145", 0.85, 0.0);
        let ear_m = palette.mat(materials, "#C4722C", 0.85, 0.0);
        let [sx, sz, _, _] = spec.sofa;
        let cat_pos = Vec3::new(sx - 1.35, 0.14, sz + 0.45);
        let cat_ent = commands
            .spawn((
                WorldEntity,
                OfficeCat { base_y: 0.14 },
                InteractiveProp::new("cat", "lounge", "Office Cat"),
                Mesh3d(palette.unit_sphere.clone()),
                MeshMaterial3d(cat_m),
                Transform::from_translation(cat_pos).with_scale(Vec3::new(0.34, 0.22, 0.26)),
            ))
            .id();
        let _ = cat_ent;
        spawn_sphere(commands, palette, ear_m, cat_pos + Vec3::new(0.16, 0.09, 0.0), Vec3::splat(0.16));

        let vac_m = palette.mat(materials, "#262A31", 0.35, 0.4);
        let vac_led = palette.emissive(materials, "#4CC9F0", 1.5);
        let vac_origin = Vec3::new(spec.center.x, 0.045, spec.center.y + 1.8);
        commands.spawn((
            WorldEntity,
            RobotVacuum { origin: vac_origin, radius: 1.5, speed: 0.45 },
            InteractiveProp::new("vacuum", "office", "Robot Vacuum"),
            Mesh3d(palette.unit_cyl.clone()),
            MeshMaterial3d(vac_m),
            Transform::from_translation(vac_origin).with_scale(Vec3::new(0.36, 0.08, 0.36)),
        ));
        spawn_sphere(commands, palette, vac_led, vac_origin + Vec3::Y * 0.05, Vec3::splat(0.05));
    }
}

pub fn animate_props_system(
    time: Res<Time>,
    mut fans: Query<(&ServerFan, &mut Transform), (Without<AquariumFish>, Without<RobotVacuum>, Without<OfficeCat>)>,
    mut fish: Query<(&AquariumFish, &mut Transform), (Without<ServerFan>, Without<RobotVacuum>, Without<OfficeCat>)>,
    mut vacs: Query<(&RobotVacuum, &mut Transform), (Without<ServerFan>, Without<AquariumFish>, Without<OfficeCat>)>,
    mut cats: Query<(&OfficeCat, &mut Transform), (Without<ServerFan>, Without<AquariumFish>, Without<RobotVacuum>)>,
) {
    let dt = time.delta_secs();
    let t = time.elapsed_secs();

    for (f, mut tf) in &mut fans {
        tf.rotate_y(f.speed * dt);
    }
    for (f, mut tf) in &mut fish {
        let a = t * f.speed + f.phase;
        tf.translation.x = f.center.x + a.cos() * f.radius;
        tf.translation.y = f.center.y + (a * 1.7).sin() * 0.05;
        tf.translation.z = f.center.z + (a * 0.7).sin() * 0.1;
    }
    for (v, mut tf) in &mut vacs {
        let a = t * v.speed;
        tf.translation.x = v.origin.x + a.cos() * v.radius;
        tf.translation.z = v.origin.z + (a * 1.3).sin() * (v.radius * 0.65);
    }
    for (c, mut tf) in &mut cats {
        tf.translation.y = c.base_y + (t * 2.1).sin() * 0.008;
    }
}
