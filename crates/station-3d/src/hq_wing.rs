use crate::layout::LayoutSpec;
use crate::palette::{WorldPalette, spawn_box, spawn_box_rot, spawn_quad};
use crate::rooms::{
    PlantKind, SharedSurfaceTiles, TrinketKind, spawn_floor_zone_tiled, spawn_glass_wall,
    spawn_low_wall, spawn_sofa, spawn_trinket, spawn_typed_plant, spawn_wall_window,
};
use bevy::prelude::*;
use station_canvas2d::FontEngine;
use station_canvas2d::office_surfaces::{draw_room_plaque, draw_sprint_board};
use std::f32::consts::FRAC_PI_2;

const WING_Z0: f32 = 9.0;
const WING_Z1: f32 = 26.0;
const WING_COR_A: [f32; 2] = [9.0, 10.8];
const WING_COR_B: [f32; 2] = [2.3, 4.3];
const WING_SPLIT: f32 = 18.4;

const DEPT_ACCENTS: [&str; 4] = ["#3F7CC4", "#6A8042", "#D97757", "#8C4FA8"];
const DEPT_CARPETS: [&str; 4] = ["#5E6B7D", "#66735C", "#7B6A5E", "#655F7A"];

#[derive(Component, Debug, Clone, Copy)]
pub struct HqWing;

#[derive(Component, Debug, Clone)]
pub struct DeptSign {
    pub dept_index: usize,
    pub material: Handle<StandardMaterial>,
}

#[derive(Component, Debug, Clone, Copy)]
pub struct SprintBoard {
    pub dept_index: usize,
}

#[allow(clippy::too_many_arguments)]
fn spawn_dept_plaque(
    commands: &mut Commands,
    palette: &mut WorldPalette,
    materials: &mut Assets<StandardMaterial>,
    images: &mut Assets<Image>,
    fonts: &mut FontEngine,
    dept_index: usize,
    x: f32,
    y: f32,
    z: f32,
    ry: f32,
) {
    let title = format!("DEPARTMENT {}", dept_index + 1);
    let mat = palette.textured_unlit(
        images,
        materials,
        draw_room_plaque(
            &title,
            Some("free for the next project"),
            "#1E232B",
            "#9AA3B0",
            "#A9B2C0",
            Some(DEPT_ACCENTS[dept_index % 4]),
            fonts,
        ),
    );
    let ent = spawn_quad(
        commands,
        palette,
        mat.clone(),
        Vec3::new(x, y, z),
        Vec2::new(0.9, 0.28),
        Quat::from_rotation_y(ry),
    );
    commands.entity(ent).insert(DeptSign {
        dept_index,
        material: mat,
    });
}

#[allow(clippy::too_many_arguments)]
fn spawn_sprint_board(
    commands: &mut Commands,
    palette: &mut WorldPalette,
    materials: &mut Assets<StandardMaterial>,
    images: &mut Assets<Image>,
    fonts: &mut FontEngine,
    dept_index: usize,
    x: f32,
    y: f32,
    z: f32,
    ry: f32,
    w: f32,
    h: f32,
    stand: bool,
) {
    let rot = Quat::from_rotation_y(ry);
    let at = |lx: f32, ly: f32, lz: f32| Vec3::new(x, 0.0, z) + rot * Vec3::new(lx, ly, lz);

    let frame_m = palette.mat(materials, "#C9CED6", 0.5, 0.2);
    let tray_m = palette.mat(materials, "#9AA0AA", 0.45, 0.35);
    let metal = palette.mat(materials, "#2B2F36", 0.35, 0.65);

    let board_ent = spawn_box_rot(
        commands,
        palette,
        frame_m,
        at(0.0, y, 0.0),
        Vec3::new(w + 0.08, h + 0.08, 0.04),
        rot,
    );
    commands.entity(board_ent).insert(SprintBoard { dept_index });

    let surf = draw_sprint_board(DEPT_ACCENTS[dept_index % 4], dept_index as u64 + 1, fonts);
    let board_m = palette.textured_lit(images, materials, surf, 0.7);
    spawn_quad(commands, palette, board_m, at(0.0, y, 0.022), Vec2::new(w, h), rot);
    spawn_box_rot(commands, palette, tray_m, at(0.0, y - h * 0.5 - 0.05, 0.04), Vec3::new(w * 0.6, 0.03, 0.06), rot);

    if stand {
        let leg_h = (y - h * 0.5).max(0.2);
        for sx in [-1.0_f32, 1.0] {
            let lx = sx * (w * 0.5 - 0.05);
            spawn_box_rot(commands, palette, metal.clone(), at(lx, leg_h * 0.5, 0.0), Vec3::new(0.04, leg_h, 0.04), rot);
            spawn_box_rot(commands, palette, metal.clone(), at(lx, 0.015, 0.0), Vec3::new(0.06, 0.03, 0.42), rot);
        }
    }
}

fn spawn_credenza(
    commands: &mut Commands,
    palette: &mut WorldPalette,
    materials: &mut Assets<StandardMaterial>,
    tiles: &SharedSurfaceTiles,
    x: f32,
    z: f32,
) {
    let body_m = palette.mat(materials, "#E4DFD6", 0.6, 0.0);
    let door_m = palette.mat(materials, "#D6D0C5", 0.6, 0.0);
    let wood_mid = palette.mat(materials, "#9A6E48", 0.55, 0.0);
    spawn_box(commands, palette, body_m, Vec3::new(x, 0.34, z), Vec3::new(1.8, 0.68, 0.48));
    for sx in [-0.6_f32, 0.0, 0.6] {
        spawn_box(commands, palette, door_m.clone(), Vec3::new(x + sx, 0.36, z + 0.245), Vec3::new(0.56, 0.56, 0.02));
    }
    spawn_box(commands, palette, wood_mid, Vec3::new(x, 0.7, z), Vec3::new(1.84, 0.04, 0.52));
    spawn_trinket(commands, palette, materials, tiles, TrinketKind::Books, x - 0.5, 0.72, z, 0.2);
    spawn_trinket(commands, palette, materials, tiles, TrinketKind::Frame, x + 0.45, 0.72, z, -0.15);
}

/// Exact 1:1 port of `src/office/world/pro/wing.js`.
#[allow(clippy::too_many_arguments)]
pub fn spawn_hq_wing(
    commands: &mut Commands,
    palette: &mut WorldPalette,
    materials: &mut Assets<StandardMaterial>,
    images: &mut Assets<Image>,
    fonts: &mut FontEngine,
    tiles: &SharedSurfaceTiles,
    spec: &LayoutSpec,
) {
    let len = WING_Z1 - WING_Z0;
    let mid = (WING_Z0 + WING_Z1) * 0.5 + 0.15;

    let slab = palette.mat(materials, "#7A5A3E", 0.65, 0.0);
    let wall = palette.mat(materials, "#EEE8DE", 0.9, 0.0);
    let cap = palette.mat(materials, "#E2D9C8", 0.75, 0.0);
    let skirt = palette.mat(materials, "#6E5440", 0.65, 0.0);

    let wing_slab = spawn_box(commands, palette, slab, Vec3::new(3.0, -0.25, mid), Vec3::new(22.6, 0.5, len));
    commands.entity(wing_slab).insert(HqWing);
    spawn_box(commands, palette, wall, Vec3::new(-8.15, 2.1, mid), Vec3::new(0.3, 4.2, len));
    spawn_box(commands, palette, cap, Vec3::new(-8.15, 4.25, mid), Vec3::new(0.42, 0.14, len));
    spawn_box(commands, palette, skirt, Vec3::new(-7.97, 0.09, mid), Vec3::new(0.06, 0.18, len));

    for z in [12.4_f32, 16.8, 20.6, 24.4] {
        spawn_wall_window(commands, palette, materials, tiles, -7.98, 2.1, z, 1.5, 1.5, FRAC_PI_2, 2);
    }

    // Corridors A & B + 4 speckled department carpets
    spawn_floor_zone_tiled(commands, palette, materials, tiles.server_grid.clone(), -8.0, 14.0, WING_COR_A[0], WING_COR_A[1], 0.002, 1.2, "#D7D2CA", 0.65);
    spawn_floor_zone_tiled(commands, palette, materials, tiles.server_grid.clone(), WING_COR_B[0], WING_COR_B[1], WING_COR_A[1], WING_Z1, 0.002, 1.2, "#D7D2CA", 0.65);
    for (i, d) in spec.depts.iter().enumerate() {
        spawn_floor_zone_tiled(commands, palette, materials, tiles.speckle.clone(), d.x0, d.x1, d.z0, d.z1, 0.003, 0.8, DEPT_CARPETS[i % 4], 0.88);
    }

    // Wing pod top trim rails + end storage cabinets (`pro/floor.js:291-298` shared pods)
    let metal = palette.mat(materials, "#2B2F36", 0.35, 0.65);
    let stor_m = palette.mat(materials, "#E4DFD6", 0.6, 0.0);
    let stor_d = palette.mat(materials, "#D6D0C5", 0.6, 0.0);
    for &[cx, cz] in spec.pods.iter().skip(3) {
        spawn_box(commands, palette, metal.clone(), Vec3::new(cx, 1.205, cz), Vec3::new(3.08, 0.025, 0.06));
        spawn_box(commands, palette, stor_m.clone(), Vec3::new(cx + 1.72, 0.31, cz), Vec3::new(0.42, 0.62, 1.4));
        for dz in [-0.35_f32, 0.35] {
            spawn_box(commands, palette, stor_d.clone(), Vec3::new(cx + 1.94, 0.31, cz + dz), Vec3::new(0.02, 0.5, 0.6));
        }
    }

    // Partitions (`pro/wing.js:95-105`)
    let [dw, de] = [-2.4_f32, 7.4_f32];
    spawn_low_wall(commands, palette, materials, -8.0, WING_Z0, -3.6, WING_Z0, &[], 1.3, "#ECE6DC");
    spawn_low_wall(
        commands,
        palette,
        materials,
        -3.6,
        WING_Z0,
        8.6,
        WING_Z0,
        &[(dw - 0.6 + 3.6, dw + 0.6 + 3.6), (de - 0.6 + 3.6, de + 0.6 + 3.6)],
        1.3,
        "#ECE6DC",
    );
    spawn_low_wall(commands, palette, materials, 8.6, WING_Z0, 14.0, WING_Z0, &[], 1.3, "#ECE6DC");

    // North glass walls of back 2 departments (doors at x = -2.4 and x = 9.15)
    spawn_glass_wall(commands, palette, materials, -8.0, 10.8, 2.3, 10.8, &[(-2.4 - 0.6 + 8.0, -2.4 + 0.6 + 8.0)], 2.5);
    spawn_glass_wall(commands, palette, materials, 4.3, 10.8, 14.0, 10.8, &[(9.15 - 0.6 - 4.3, 9.15 + 0.6 - 4.3)], 2.5);

    // Corridor B glass walls with doors at z = 22.2
    let door_b = 22.2 - WING_COR_A[1];
    spawn_glass_wall(commands, palette, materials, WING_COR_B[0], WING_COR_A[1], WING_COR_B[0], WING_Z1, &[(door_b - 0.6, door_b + 0.6)], 2.5);
    spawn_glass_wall(commands, palette, materials, WING_COR_B[1], WING_COR_A[1], WING_COR_B[1], WING_Z1, &[(door_b - 0.6, door_b + 0.6)], 2.5);

    // Split low walls between front & back departments
    spawn_low_wall(commands, palette, materials, -8.0, WING_SPLIT, WING_COR_B[0], WING_SPLIT, &[], 1.3, "#ECE6DC");
    spawn_low_wall(commands, palette, materials, WING_COR_B[1], WING_SPLIT, 14.0, WING_SPLIT, &[], 1.3, "#ECE6DC");

    // 4 dynamic department door plaques (`pro/wing.js:108-111`)
    spawn_dept_plaque(commands, palette, materials, images, fonts, 0, -2.4 + 1.35, 1.62, 10.845, 0.0);
    spawn_dept_plaque(commands, palette, materials, images, fonts, 1, 9.15 + 1.35, 1.62, 10.845, 0.0);
    spawn_dept_plaque(commands, palette, materials, images, fonts, 2, WING_COR_B[0] + 0.045, 1.62, 22.2 - 1.35, FRAC_PI_2);
    spawn_dept_plaque(commands, palette, materials, images, fonts, 3, WING_COR_B[1] + 0.045, 1.62, 22.2 + 1.35, FRAC_PI_2);

    // 4 Kanban sprint boards (`pro/wing.js:114-117`)
    spawn_sprint_board(commands, palette, materials, images, fonts, 0, -7.96, 1.5, 14.6, FRAC_PI_2, 1.4, 0.9, false);
    spawn_sprint_board(commands, palette, materials, images, fonts, 1, 12.2, 1.3, 11.3, 0.0, 1.3, 0.8, true);
    spawn_sprint_board(commands, palette, materials, images, fonts, 2, -5.4, 0.78, WING_SPLIT + 0.1, 0.0, 1.3, 0.72, false);
    spawn_sprint_board(commands, palette, materials, images, fonts, 3, 11.9, 0.78, WING_SPLIT + 0.1, 0.0, 1.3, 0.72, false);

    // 4 credenzas (`pro/wing.js:118`)
    for (cx, cz) in [(-6.3_f32, 11.3_f32), (6.2, 11.3), (-1.3, WING_SPLIT + 0.45), (7.3, WING_SPLIT + 0.45)] {
        spawn_credenza(commands, palette, materials, tiles, cx, cz);
    }

    // 14 typed department corner plants (`pro/wing.js:119-121`)
    let corner_plants = [
        (-7.5_f32, 11.25_f32, 1.0_f32, PlantKind::Monstera),
        (1.75, 11.3, 0.9, PlantKind::Snake),
        (-7.5, 17.85, 0.95, PlantKind::Bush),
        (1.75, 17.85, 0.9, PlantKind::Snake),
        (4.75, 11.3, 0.9, PlantKind::Snake),
        (13.55, 11.3, 1.0, PlantKind::Monstera),
        (4.75, 17.85, 0.9, PlantKind::Bush),
        (13.55, 17.85, 0.95, PlantKind::Snake),
        (-7.5, 18.9, 0.9, PlantKind::Snake),
        (13.55, 18.9, 0.9, PlantKind::Snake),
        (-7.45, 25.5, 1.05, PlantKind::Monstera),
        (1.6, 25.5, 0.95, PlantKind::Bush),
        (5.2, 25.5, 0.95, PlantKind::Bush),
        (13.5, 25.5, 1.05, PlantKind::Monstera),
    ];
    for (px, pz, s, kind) in corner_plants {
        spawn_typed_plant(commands, palette, materials, px, 0.0, pz, s, kind);
    }

    // 2 wing sofas (`pro/wing.js:122-123`)
    spawn_sofa(commands, palette, materials, -5.4, 25.35, 1.9, 0.0, "#4F6D8F", ["#F2B33D", "#E8742A"]);
    spawn_sofa(commands, palette, materials, 10.2, 25.35, 1.9, 0.0, "#6B4F3A", ["#BDE0FE", "#E2B04A"]);

    // Corridor A green panel + HEADQUARTERS sign + 3 corridor end plants (`pro/wing.js:125-129`)
    let cor_mid = (WING_COR_A[0] + WING_COR_A[1]) * 0.5;
    let green_m = palette.mat(materials, "#2F4A3D", 0.9, 0.0);
    spawn_box(commands, palette, green_m, Vec3::new(-7.985, 1.35, cor_mid), Vec3::new(0.03, 2.7, 1.6));
    let hq_sign = palette.textured_unlit(
        images,
        materials,
        draw_room_plaque("HEADQUARTERS", Some("Departments"), "#2F4A3D", "#F3EFE8", "#B9D6C4", None, fonts),
    );
    spawn_quad(commands, palette, hq_sign, Vec3::new(-7.965, 1.95, cor_mid), Vec2::new(1.36, 0.42), Quat::from_rotation_y(FRAC_PI_2));

    spawn_typed_plant(commands, palette, materials, 13.55, 0.0, 9.9, 1.1, PlantKind::Monstera);
    spawn_typed_plant(commands, palette, materials, 2.7, 0.0, 25.55, 0.95, PlantKind::Bush);
    spawn_typed_plant(commands, palette, materials, 3.9, 0.0, 25.55, 0.95, PlantKind::Snake);
}
