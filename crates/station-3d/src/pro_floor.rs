use crate::entrance::{STAIR_H, STAIR_W, STAIR_Z};
use crate::layout::LayoutSpec;
use crate::palette::{
    WorldEntity, WorldPalette, spawn_box, spawn_box_rot, spawn_cone_rot, spawn_cyl, spawn_cyl_rot,
    spawn_quad, spawn_sphere,
};
use crate::rooms::{
    PlantKind, SharedSurfaceTiles, TrinketKind, lighten_hex, spawn_floor_zone_tiled,
    spawn_glass_wall, spawn_low_wall, spawn_rug_tiled, spawn_sofa, spawn_trinket,
    spawn_typed_plant, spawn_wall_window,
};
use bevy::prelude::*;
use station_canvas2d::FontEngine;
use station_canvas2d::office_surfaces::{
    draw_cooler_screen, draw_my_office_logo, draw_poster_ship_often, draw_poster_team_outing,
    draw_reception_tag, draw_room_plaque, draw_sprint_review_slide, draw_wc_sign,
};
use std::f32::consts::{FRAC_PI_2, PI};

#[derive(Component, Debug, Clone, Copy)]
pub struct ReceptionCounter;

#[derive(Component, Debug, Clone)]
pub struct RestroomStall {
    pub index: usize,
    pub door_pivot: Entity,
    pub indicator_mat: Handle<StandardMaterial>,
    pub open: f32,
}

#[derive(Component, Debug, Clone, Copy)]
pub struct DiningTable {
    pub index: usize,
}

#[derive(Component, Debug, Clone, Copy)]
pub struct PhoneBooth {
    pub index: usize,
}

#[derive(Component, Debug, Clone, Copy)]
pub struct FoosballTable;

#[derive(Component, Debug, Clone)]
pub struct CoolerScreen {
    pub material: Handle<StandardMaterial>,
}

fn spawn_back_panel(
    commands: &mut Commands,
    palette: &mut WorldPalette,
    materials: &mut Assets<StandardMaterial>,
    x0: f32,
    x1: f32,
    color: &str,
    y0: f32,
    y1: f32,
) {
    let m = palette.mat(materials, color, 0.9, 0.0);
    spawn_box(
        commands,
        palette,
        m,
        Vec3::new((x0 + x1) * 0.5, (y0 + y1) * 0.5, -7.985),
        Vec3::new((x1 - x0).abs(), (y1 - y0).abs(), 0.03),
    );
}

fn spawn_left_panel(
    commands: &mut Commands,
    palette: &mut WorldPalette,
    materials: &mut Assets<StandardMaterial>,
    z0: f32,
    z1: f32,
    color: &str,
    y0: f32,
    y1: f32,
) {
    if z1 - z0 < 0.02 || y1 - y0 < 0.02 {
        return;
    }
    let m = palette.mat(materials, color, 0.9, 0.0);
    spawn_box(
        commands,
        palette,
        m,
        Vec3::new(-7.985, (y0 + y1) * 0.5, (z0 + z1) * 0.5),
        Vec3::new(0.03, (y1 - y0).abs(), (z1 - z0).abs()),
    );
}

/// Exact Oracle `pro/pieces.js:104-112` upholstered visitor/waiting chair.
#[allow(clippy::too_many_arguments)]
fn spawn_chair_uph(
    commands: &mut Commands,
    palette: &mut WorldPalette,
    materials: &mut Assets<StandardMaterial>,
    x: f32,
    z: f32,
    face: f32,
    color: &str,
    s: f32,
) {
    let rot = Quat::from_rotation_y(face);
    let at = |lx: f32, ly: f32, lz: f32| Vec3::new(x, 0.0, z) + rot * Vec3::new(lx * s, ly * s, lz * s);
    let m = palette.mat(materials, color, 0.95, 0.0);
    let wood = palette.mat(materials, "#6E4B32", 0.6, 0.0);
    spawn_box_rot(commands, palette, m.clone(), at(0.0, 0.42, 0.0), Vec3::new(0.56, 0.16, 0.54) * s, rot);
    spawn_box_rot(
        commands,
        palette,
        m.clone(),
        at(0.0, 0.72, -0.24),
        Vec3::new(0.56, 0.52, 0.12) * s,
        rot * Quat::from_rotation_x(-0.12),
    );
    for sx in [-1.0_f32, 1.0] {
        spawn_box_rot(commands, palette, m.clone(), at(sx * 0.29, 0.56, 0.0), Vec3::new(0.08, 0.22, 0.5) * s, rot);
        for sz in [-1.0_f32, 1.0] {
            spawn_cyl(commands, palette, wood.clone(), at(sx * 0.22, 0.18, sz * 0.2), 0.04 * s, 0.36 * s);
        }
    }
}

/// Exact Oracle `pro/pieces.js:113-122` deep library reading armchair.
#[allow(clippy::too_many_arguments)]
fn spawn_armchair(
    commands: &mut Commands,
    palette: &mut WorldPalette,
    materials: &mut Assets<StandardMaterial>,
    x: f32,
    z: f32,
    face: f32,
    color: &str,
) {
    let rot = Quat::from_rotation_y(face);
    let at = |lx: f32, ly: f32, lz: f32| Vec3::new(x, 0.0, z) + rot * Vec3::new(lx, ly, lz);
    let m = palette.mat(materials, color, 0.9, 0.0);
    let cush = palette.mat(materials, lighten_hex(color, 0.18).as_str(), 0.9, 0.0);
    let wood = palette.mat(materials, "#6E4B32", 0.6, 0.0);
    spawn_box_rot(commands, palette, m.clone(), at(0.0, 0.3, 0.0), Vec3::new(0.86, 0.3, 0.78), rot);
    spawn_box_rot(commands, palette, cush, at(0.0, 0.5, 0.06), Vec3::new(0.76, 0.12, 0.62), rot);
    spawn_box_rot(commands, palette, m.clone(), at(0.0, 0.72, -0.3), Vec3::new(0.86, 0.66, 0.2), rot);
    for sx in [-1.0_f32, 1.0] {
        spawn_box_rot(commands, palette, m.clone(), at(sx * 0.36, 0.5, 0.0), Vec3::new(0.16, 0.46, 0.78), rot);
        for sz in [-1.0_f32, 1.0] {
            spawn_cyl(commands, palette, wood.clone(), at(sx * 0.34, 0.07, sz * 0.3), 0.055, 0.14);
        }
    }
}

/// Exact Oracle `pro/pieces.js:123-136` 4-seat pantry dining table with colored chairs & fruit bowl.
fn spawn_dining_set(
    commands: &mut Commands,
    palette: &mut WorldPalette,
    materials: &mut Assets<StandardMaterial>,
    index: usize,
    x: f32,
    z: f32,
) {
    let top_m = palette.mat(materials, "#F3EFE8", 0.4, 0.0);
    let wood_mid = palette.mat(materials, "#9A6E48", 0.55, 0.0);
    let bowl_m = palette.mat(materials, "#FFFFFF", 0.35, 0.0);
    let glass_m = palette.glass(materials, "#BFE3F2", 0.55);

    let tbl = spawn_box(commands, palette, top_m, Vec3::new(x, 0.745, z), Vec3::new(1.3, 0.05, 0.8));
    commands.entity(tbl).insert(DiningTable { index });

    for sx in [-1.0_f32, 1.0] {
        for sz in [-1.0_f32, 1.0] {
            spawn_cyl(commands, palette, wood_mid.clone(), Vec3::new(x + sx * 0.56, 0.36, z + sz * 0.32), 0.05, 0.72);
        }
    }

    let chair_colors = ["#F2B33D", "#3F7CC4", "#E8742A", "#6A8042"];
    for (sxi, sx) in [-1.0_f32, 1.0].iter().enumerate() {
        for (szi, sz) in [-1.0_f32, 1.0].iter().enumerate() {
            let cx = x + sx * 0.33;
            let cz = z + sz * 0.62;
            let rot = Quat::from_rotation_y(if *sz > 0.0 { PI } else { 0.0 });
            let at = |lx: f32, ly: f32, lz: f32| Vec3::new(cx, 0.0, cz) + rot * Vec3::new(lx, ly, lz);
            let cm = palette.mat(materials, chair_colors[(sxi * 2 + szi) % 4], 0.6, 0.0);
            spawn_box_rot(commands, palette, cm.clone(), at(0.0, 0.45, 0.0), Vec3::new(0.42, 0.06, 0.4), rot);
            spawn_box_rot(
                commands,
                palette,
                cm,
                at(0.0, 0.66, -0.2),
                Vec3::new(0.42, 0.34, 0.05),
                rot * Quat::from_rotation_x(-0.1),
            );
            for lx in [-1.0_f32, 1.0] {
                for lz in [-1.0_f32, 1.0] {
                    spawn_cyl(commands, palette, wood_mid.clone(), at(lx * 0.17, 0.22, lz * 0.16), 0.03, 0.44);
                }
            }
        }
    }

    // Fruit bowl + 3 fruits + water glass
    spawn_cyl(commands, palette, bowl_m, Vec3::new(x + 0.25, 0.8, z), 0.22, 0.07);
    for (dx, col) in [(0.2_f32, "#E63946"), (0.28, "#FFD166"), (0.24, "#7BC043")] {
        let fm = palette.mat(materials, col, 0.45, 0.0);
        spawn_sphere(commands, palette, fm, Vec3::new(x + dx, 0.85, z + (dx - 0.24) * 1.5), Vec3::splat(0.09));
    }
    spawn_cyl(commands, palette, glass_m, Vec3::new(x - 0.35, 0.83, z + 0.1), 0.09, 0.12);
}

fn spawn_water_cooler(
    commands: &mut Commands,
    palette: &mut WorldPalette,
    materials: &mut Assets<StandardMaterial>,
    x: f32,
    z: f32,
    taps: bool,
) {
    let body = palette.mat(materials, "#F1F1EE", 0.5, 0.0);
    let jug = palette.glass(materials, "#7FC4F0", 0.65);
    spawn_box(commands, palette, body, Vec3::new(x, 0.5, z), Vec3::new(0.36, 1.0, 0.36));
    spawn_cyl(commands, palette, jug, Vec3::new(x, 1.22, z), 0.3, 0.42);
    if taps {
        let cold = palette.mat(materials, "#3F7CC4", 0.45, 0.0);
        let hot = palette.mat(materials, "#E63946", 0.45, 0.0);
        spawn_box(commands, palette, cold, Vec3::new(x - 0.07, 0.78, z + 0.19), Vec3::new(0.06, 0.05, 0.04));
        spawn_box(commands, palette, hot, Vec3::new(x + 0.07, 0.78, z + 0.19), Vec3::new(0.06, 0.05, 0.04));
    }
}

fn spawn_office_chair(
    commands: &mut Commands,
    palette: &mut WorldPalette,
    materials: &mut Assets<StandardMaterial>,
    x: f32,
    z: f32,
    ry: f32,
    color: &str,
    high_back: bool,
) {
    let rot = Quat::from_rotation_y(ry);
    let at = |lx: f32, ly: f32, lz: f32| Vec3::new(x, 0.0, z) + rot * Vec3::new(lx, ly, lz);
    let metal = palette.mat(materials, "#2B2F36", 0.35, 0.65);
    let seat_m = palette.mat(materials, color, 0.5, 0.0);
    spawn_cyl(commands, palette, metal.clone(), at(0.0, 0.04, 0.0), 0.52, 0.04);
    spawn_cyl(commands, palette, metal, at(0.0, 0.24, 0.0), 0.06, 0.36);
    let back_scale = if high_back { Vec3::new(0.58, 0.78, 0.11) } else { Vec3::new(0.52, 0.56, 0.09) };
    let back_y = if high_back { 0.88 } else { 0.78 };
    spawn_box_rot(commands, palette, seat_m.clone(), at(0.0, 0.45, 0.0), Vec3::new(0.56, 0.09, 0.54), rot);
    spawn_box_rot(commands, palette, seat_m, at(0.0, back_y, -0.26), back_scale, rot);
}

/// Exact 1:1 port of `src/office/world/pro/floor.js` & `pieces.js`.
#[allow(clippy::too_many_arguments)]
pub fn spawn_pro_floor(
    commands: &mut Commands,
    palette: &mut WorldPalette,
    materials: &mut Assets<StandardMaterial>,
    images: &mut Assets<Image>,
    fonts: &mut FontEngine,
    tiles: &SharedSurfaceTiles,
    spec: &LayoutSpec,
) {
    let metal = palette.mat(materials, "#2B2F36", 0.35, 0.65);
    let wood_dark = palette.mat(materials, "#6E4B32", 0.6, 0.0);
    let wood_mid = palette.mat(materials, "#9A6E48", 0.55, 0.0);

    /* 1. Shell: slab, outer back wall, left wall WITH stairwell doorway cutout, caps, skirting */
    let slab = palette.mat(materials, "#7A5A3E", 0.65, 0.0);
    let wall_m = palette.mat(materials, "#EEE8DE", 0.9, 0.0);
    let cap_m = palette.mat(materials, "#E2D9C8", 0.75, 0.0);
    let skirt_m = palette.mat(materials, "#6E5440", 0.65, 0.0);

    spawn_box(commands, palette, slab, Vec3::new(3.0, -0.25, 0.5), Vec3::new(22.6, 0.5, 17.6));
    spawn_box(commands, palette, wall_m.clone(), Vec3::new(3.0, 2.1, -8.15), Vec3::new(22.6, 4.2, 0.3));
    let stair_a = STAIR_Z - STAIR_W * 0.5;
    let stair_b = STAIR_Z + STAIR_W * 0.5;
    spawn_box(commands, palette, wall_m.clone(), Vec3::new(-8.15, 2.1, (stair_a - 8.3) * 0.5), Vec3::new(0.3, 4.2, stair_a + 8.3));
    spawn_box(commands, palette, wall_m.clone(), Vec3::new(-8.15, 2.1, (stair_b + 9.3) * 0.5), Vec3::new(0.3, 4.2, 9.3 - stair_b));
    spawn_box(commands, palette, wall_m, Vec3::new(-8.15, (4.2 + STAIR_H) * 0.5, STAIR_Z), Vec3::new(0.3, 4.2 - STAIR_H, STAIR_W));
    spawn_box(commands, palette, cap_m.clone(), Vec3::new(3.0, 4.25, -8.15), Vec3::new(22.6, 0.14, 0.42));
    spawn_box(commands, palette, cap_m, Vec3::new(-8.15, 4.25, 0.5), Vec3::new(0.42, 0.14, 17.6));
    spawn_box(commands, palette, skirt_m.clone(), Vec3::new(3.0, 0.09, -7.97), Vec3::new(22.0, 0.18, 0.06));
    for (z0, z1) in [(-8.0_f32, stair_a - 0.05), (stair_b + 0.05, 9.0)] {
        spawn_box(commands, palette, skirt_m.clone(), Vec3::new(-7.97, 0.09, (z0 + z1) * 0.5), Vec3::new(0.06, 0.18, z1 - z0));
    }

    /* 2. Tiled room floors (`pro/floor.js:30-46`) */
    spawn_floor_zone_tiled(commands, palette, materials, tiles.plank.clone(), -3.6, 8.6, -1.6, 9.0, 0.002, 3.0, "#E4D3BA", 0.58);
    spawn_floor_zone_tiled(commands, palette, materials, tiles.server_grid.clone(), -3.6, 8.6, -3.0, -1.6, 0.002, 1.2, "#D7D2CA", 0.65);
    spawn_floor_zone_tiled(commands, palette, materials, tiles.speckle.clone(), -8.0, -2.6, -8.0, -3.0, 0.002, 0.8, "#4A5566", 0.88);
    spawn_floor_zone_tiled(commands, palette, materials, tiles.speckle.clone(), -2.6, 3.0, -8.0, -3.0, 0.002, 0.8, "#8D939C", 0.86);
    spawn_floor_zone_tiled(commands, palette, materials, tiles.plank.clone(), 3.0, 8.6, -8.0, -3.0, 0.002, 2.8, "#8E6848", 0.56);
    spawn_floor_zone_tiled(commands, palette, materials, tiles.checker.clone(), 8.6, 14.0, -8.0, 0.6, 0.002, 1.6, "#FFFFFF", 0.60);
    spawn_floor_zone_tiled(commands, palette, materials, tiles.plank.clone(), 8.6, 14.0, 0.6, 9.0, 0.002, 2.8, "#C79C71", 0.58);
    spawn_floor_zone_tiled(commands, palette, materials, tiles.server_grid.clone(), -8.0, -3.6, -3.0, 0.55, 0.002, 1.2, "#A3ABB6", 0.55);
    spawn_floor_zone_tiled(commands, palette, materials, tiles.server_grid.clone(), -8.0, -3.6, 0.55, 5.4, 0.002, 1.6, "#ECE8E1", 0.50);
    spawn_floor_zone_tiled(commands, palette, materials, tiles.server_grid.clone(), -8.0, -3.6, 5.4, 9.0, 0.002, 0.6, "#E3EBEF", 0.45);

    // Carpet tiles under each core pod
    for &[cx, cz] in spec.pods.iter().take(3) {
        spawn_floor_zone_tiled(commands, palette, materials, tiles.server_grid.clone(), cx - 2.0, cx + 2.0, cz - 1.75, cz + 1.75, 0.006, 1.2, "#9AA2AE", 0.86);
    }
    spawn_rug_tiled(commands, palette, materials, tiles.speckle.clone(), -5.4, 0.014, -5.3, 3.6, 2.9, "#B9A486");
    spawn_rug_tiled(commands, palette, materials, tiles.speckle.clone(), 5.75, 0.015, -5.4, 3.4, 2.4, "#9E4A3F");
    spawn_rug_tiled(commands, palette, materials, tiles.speckle.clone(), 11.0, 0.016, 2.55, 3.8, 3.1, "#D8C6A8");

    /* 3. Room-by-room wall finishes + pantry subway tile backsplash (`pro/floor.js:48-63`) */
    spawn_back_panel(commands, palette, materials, -8.0, -2.6, "#6F5440", 0.0, 2.75);
    let rail_m = palette.mat(materials, "#4E3A2C", 0.6, 0.0);
    spawn_box(commands, palette, rail_m, Vec3::new(-5.3, 2.78, -7.96), Vec3::new(5.4, 0.06, 0.06));
    spawn_left_panel(commands, palette, materials, -8.0, -3.0, "#6F5440", 0.0, 2.75);
    spawn_back_panel(commands, palette, materials, -2.6, 3.0, "#DCE2E8", 0.0, 4.1);
    spawn_back_panel(commands, palette, materials, 3.0, 8.6, "#C9D5B2", 0.0, 2.75);
    spawn_back_panel(commands, palette, materials, 8.6, 14.0, "#F4E6CF", 0.0, 4.1);
    let subway_m = palette.tiled_mat_from_handle(materials, tiles.subway.clone(), Vec2::new(8.0, 1.0), "#FFFFFF", 0.35);
    spawn_quad(commands, palette, subway_m, Vec3::new(11.3, 1.3, -7.955), Vec2::new(5.36, 0.64), Quat::IDENTITY);
    spawn_left_panel(commands, palette, materials, -3.0, 1.2, "#38404C", 0.0, 4.1);
    let sa = stair_a - 0.05;
    let sb = stair_b + 0.05;
    spawn_left_panel(commands, palette, materials, 1.2, sa, "#2F4A3D", 0.0, 4.1);
    spawn_left_panel(commands, palette, materials, sa, sb, "#2F4A3D", STAIR_H + 0.06, 4.1);
    if sb < 5.38 {
        spawn_left_panel(commands, palette, materials, sb, 5.4, "#2F4A3D", 0.0, 4.1);
    }
    spawn_left_panel(commands, palette, materials, 5.4, 9.0, "#DDE8EE", 0.0, 2.0);

    /* 4. Five outer windows with animated WindowBlinds (`pro/floor.js:65-70`) */
    spawn_wall_window(commands, palette, materials, tiles, -5.3, 2.05, -7.98, 2.6, 1.5, 0.0, 3);
    spawn_wall_window(commands, palette, materials, tiles, -7.98, 2.05, -5.9, 1.6, 1.5, FRAC_PI_2, 2);
    spawn_wall_window(commands, palette, materials, tiles, 7.7, 2.1, -7.98, 1.3, 1.7, 0.0, 2);
    spawn_wall_window(commands, palette, materials, tiles, 12.55, 2.35, -7.98, 0.95, 1.0, 0.0, 2);
    spawn_wall_window(commands, palette, materials, tiles, -7.98, 2.6, 7.4, 1.4, 0.7, FRAC_PI_2, 2);

    /* 5. Exact glass & cutaway partitions (`pro/floor.js:72-86`) */
    spawn_glass_wall(commands, palette, materials, -8.0, -3.0, -2.6, -3.0, &[(4.5, 5.3)], 2.5);
    spawn_glass_wall(commands, palette, materials, -2.6, -8.0, -2.6, -3.0, &[], 2.5);
    spawn_glass_wall(commands, palette, materials, -2.6, -3.0, 3.0, -3.0, &[(4.5, 5.4)], 2.5);
    spawn_glass_wall(commands, palette, materials, 3.0, -8.0, 3.0, -3.0, &[], 2.5);
    spawn_low_wall(commands, palette, materials, 3.0, -3.0, 8.6, -3.0, &[(0.3, 1.3)], 1.3, "#ECE6DC");
    spawn_low_wall(commands, palette, materials, 8.6, -8.0, 8.6, -3.0, &[], 1.3, "#ECE6DC");
    spawn_glass_wall(commands, palette, materials, -3.6, -3.0, -3.6, 0.55, &[(1.0, 1.9)], 2.5);
    spawn_glass_wall(commands, palette, materials, -8.0, 0.55, -3.6, 0.55, &[], 2.5);
    spawn_low_wall(commands, palette, materials, -8.0, 5.4, -3.6, 5.4, &[(2.6, 3.6)], 1.3, "#DDE8EE");
    spawn_low_wall(commands, palette, materials, -3.6, 5.4, -3.6, 9.0, &[], 1.3, "#DDE8EE");
    spawn_low_wall(commands, palette, materials, -3.6, 0.6, -3.6, 5.33, &[(2.1, 3.3)], 1.3, "#ECE6DC");
    spawn_low_wall(commands, palette, materials, 8.6, -1.6, 8.6, 9.0, &[(5.8, 8.0)], 1.3, "#ECE6DC");
    spawn_low_wall(commands, palette, materials, 8.6, 0.6, 14.0, 0.6, &[(0.3, 1.3)], 1.3, "#ECE6DC");

    // Open glass double doors from lobby to dev floor (`pro/floor.js:88-96`)
    let door_glass = palette.glass(materials, "#BFD8EE", 0.22);
    for (z, s) in [(2.7_f32, 1.0_f32), (3.9, -1.0)] {
        let rot = Quat::from_rotation_y(s * 1.1);
        let at = |lx: f32, ly: f32, lz: f32| Vec3::new(-3.6, 0.0, z) + rot * Vec3::new(lx, ly, lz);
        spawn_box_rot(commands, palette, door_glass.clone(), at(0.0, 1.07, s * 0.3), Vec3::new(0.03, 2.1, 0.58), rot);
        for dz in [0.02_f32, 0.58] {
            spawn_box_rot(commands, palette, metal.clone(), at(0.0, 1.06, s * dz), Vec3::new(0.05, 2.12, 0.04), rot);
        }
        spawn_box_rot(commands, palette, metal.clone(), at(0.0, 2.1, s * 0.3), Vec3::new(0.05, 0.05, 0.6), rot);
        spawn_box_rot(commands, palette, metal.clone(), at(0.0, 0.03, s * 0.3), Vec3::new(0.05, 0.05, 0.6), rot);
        spawn_cyl(commands, palette, metal.clone(), at(0.05, 1.05, s * 0.5), 0.03, 0.5);
    }
    for z in [2.66_f32, 3.94] {
        spawn_box(commands, palette, metal.clone(), Vec3::new(-3.6, 1.1, z), Vec3::new(0.09, 2.2, 0.08));
    }
    spawn_box(commands, palette, metal.clone(), Vec3::new(-3.6, 2.17, 3.3), Vec3::new(0.09, 0.09, 1.36));

    // Door plaques (`pro/floor.js:98-101`)
    for (x, y, z, title) in [
        (-3.88_f32, 1.62_f32, -2.96_f32, "DIRECTOR"),
        (1.55, 1.62, -2.96, "MEETING ROOM"),
        (4.66, 0.8, -2.912, "LIBRARY"),
    ] {
        let pm = palette.textured_unlit(
            images,
            materials,
            draw_room_plaque(title, None, "#1E232B", "#F3EFE8", "#A9B2C0", None, fonts),
        );
        spawn_quad(commands, palette, pm, Vec3::new(x, y, z), Vec2::new(0.62, 0.2), Quat::IDENTITY);
    }

    /* 6. Director's office (`pro/floor.js:103-131`) */
    let cred_m = palette.mat(materials, "#5A4232", 0.6, 0.0);
    let cred_d = palette.mat(materials, "#6B4F3C", 0.6, 0.0);
    let walnut = palette.mat(materials, "#4A3628", 0.5, 0.0);
    spawn_box(commands, palette, cred_m, Vec3::new(-5.3, 0.37, -7.7), Vec3::new(3.3, 0.74, 0.5));
    for x in [-6.4_f32, -5.3, -4.2] {
        spawn_box(commands, palette, cred_d.clone(), Vec3::new(x, 0.4, -7.44), Vec3::new(1.0, 0.56, 0.02));
    }
    spawn_trinket(commands, palette, materials, tiles, TrinketKind::Books, -6.4, 0.74, -7.7, 0.2);
    spawn_trinket(commands, palette, materials, tiles, TrinketKind::Frame, -5.85, 0.74, -7.68, 0.1);
    spawn_typed_plant(commands, palette, materials, -4.0, 0.74, -7.7, 0.32, PlantKind::Snake);

    // Executive desk at (-5.4, 0, -5.8)
    let dx = -5.4_f32;
    let dz = -5.8_f32;
    spawn_box(commands, palette, walnut.clone(), Vec3::new(dx, 0.76, dz), Vec3::new(2.1, 0.06, 0.95));
    for sx in [-1.0_f32, 1.0] {
        spawn_box(commands, palette, walnut.clone(), Vec3::new(dx + sx * 0.79, 0.365, dz), Vec3::new(0.46, 0.73, 0.86));
    }
    spawn_box(commands, palette, walnut.clone(), Vec3::new(dx, 0.48, dz + 0.4), Vec3::new(1.2, 0.5, 0.04));
    let mon_m = palette.mat(materials, "#1B1F26", 0.4, 0.1);
    let scr_m = palette.emissive(materials, "#8FD0FF", 0.6);
    spawn_box(commands, palette, metal.clone(), Vec3::new(dx, 0.95, dz + 0.14), Vec3::new(0.06, 0.32, 0.06));
    spawn_box(commands, palette, mon_m, Vec3::new(dx, 1.22, dz + 0.16), Vec3::new(0.7, 0.42, 0.04));
    spawn_quad(commands, palette, scr_m, Vec3::new(dx, 1.22, dz + 0.135), Vec2::new(0.64, 0.36), Quat::from_rotation_y(PI));
    spawn_box(commands, palette, metal.clone(), Vec3::new(dx, 0.8, dz - 0.18), Vec3::new(0.46, 0.02, 0.15));
    let paper_m = palette.matte(materials, "#FBFBF7");
    spawn_box_rot(commands, palette, paper_m, Vec3::new(dx - 0.55, 0.796, dz - 0.1), Vec3::new(0.3, 0.012, 0.22), Quat::from_rotation_y(0.3));
    spawn_cyl(commands, palette, metal.clone(), Vec3::new(dx + 0.62, 0.84, dz + 0.1), 0.09, 0.1);
    // Banker's lamp
    let brass = palette.mat(materials, "#C9A227", 0.32, 0.7);
    let shade = palette.mat(materials, "#2F6B45", 0.45, 0.0);
    spawn_cyl(commands, palette, brass.clone(), Vec3::new(dx - 0.82, 0.805, dz + 0.12), 0.15, 0.03);
    spawn_cyl(commands, palette, brass.clone(), Vec3::new(dx - 0.82, 0.96, dz + 0.12), 0.024, 0.3);
    spawn_cone_rot(commands, palette, shade, Vec3::new(dx - 0.82, 1.12, dz + 0.12), Vec3::new(0.22, 0.09, 0.22), Quat::IDENTITY);
    // Gold-on-dark DIRECTOR nameplate
    let np_m = palette.textured_unlit(
        images,
        materials,
        draw_room_plaque("DIRECTOR", None, "#2B2F36", "#E2B04A", "#E2B04A", None, fonts),
    );
    let np_rot = Quat::from_rotation_x(-0.35);
    spawn_box_rot(commands, palette, metal.clone(), Vec3::new(dx + 0.5, 0.83, dz + 0.43), Vec3::new(0.44, 0.1, 0.05), np_rot);
    spawn_quad(commands, palette, np_m, Vec3::new(dx + 0.5, 0.84, dz + 0.46), Vec2::new(0.42, 0.11), np_rot);

    spawn_office_chair(commands, palette, materials, -5.4, -6.75, 0.0, "#22262C", true);
    spawn_chair_uph(commands, palette, materials, -5.95, -4.95, PI, "#8C6A4E", 0.95);
    spawn_chair_uph(commands, palette, materials, -4.85, -4.95, PI, "#8C6A4E", 0.95);
    spawn_sofa(commands, palette, materials, -7.45, -4.1, 1.7, FRAC_PI_2, "#7A5C48", ["#E2B04A", "#C8553D"]);
    spawn_box(commands, palette, walnut, Vec3::new(-6.55, 0.42, -4.1), Vec3::new(0.62, 0.05, 1.0));
    for sx in [-1.0_f32, 1.0] {
        for sz in [-1.0_f32, 1.0] {
            spawn_cyl(commands, palette, metal.clone(), Vec3::new(-6.55 + sx * 0.25, 0.2, -4.1 + sz * 0.42), 0.04, 0.4);
        }
    }

    /* 7. Meeting room Sprint Review slide (`pro/floor.js:132-141`) */
    let slide_m = palette.textured_unlit(images, materials, draw_sprint_review_slide(fonts));
    let bezel_m = palette.mat(materials, "#15181E", 0.4, 0.1);
    spawn_box(commands, palette, bezel_m, Vec3::new(0.2, 1.75, -7.95), Vec3::new(2.7, 1.55, 0.07));
    spawn_quad(commands, palette, slide_m, Vec3::new(0.2, 1.75, -7.91), Vec2::new(2.56, 1.42), Quat::IDENTITY);

    /* 8. Library armchairs, side table, 7-rung wooden ladder (`pro/floor.js:143-149`) */
    spawn_armchair(commands, palette, materials, 5.6, -4.1, 0.0, "#7A4E3A");
    spawn_armchair(commands, palette, materials, 6.9, -4.1, 0.0, "#3D5A80");
    spawn_cyl(commands, palette, wood_dark.clone(), Vec3::new(6.25, 0.52, -4.15), 0.44, 0.04);
    spawn_cyl(commands, palette, wood_dark, Vec3::new(6.25, 0.26, -4.15), 0.06, 0.5);
    spawn_trinket(commands, palette, materials, tiles, TrinketKind::Books, 6.25, 0.54, -4.15, 0.4);
    let lad_rot = Quat::from_rotation_x(-0.18);
    let lad_at = |lx: f32, ly: f32, lz: f32| Vec3::new(6.55, 0.0, -6.88) + lad_rot * Vec3::new(lx, ly, lz);
    for sx in [-1.0_f32, 1.0] {
        spawn_box_rot(commands, palette, wood_mid.clone(), lad_at(sx * 0.22, 1.25, 0.0), Vec3::new(0.05, 2.5, 0.05), lad_rot);
    }
    for i in 0..7 {
        spawn_box_rot(commands, palette, wood_mid.clone(), lad_at(0.0, 0.3 + i as f32 * 0.32, 0.0), Vec3::new(0.44, 0.04, 0.05), lad_rot);
    }

    /* 9. Pantry: sink+faucet, fridge, wall cupboards, water cooler, 4 dining sets (`pro/floor.js:151-166`) */
    let cab_m = palette.mat(materials, "#E6E1D8", 0.7, 0.0);
    let cab_d = palette.mat(materials, "#D9D2C6", 0.7, 0.0);
    let top_m = palette.mat(materials, "#3B3F46", 0.35, 0.1);
    let sink_m = palette.mat(materials, "#9AA3AD", 0.35, 0.6);
    let fridge_m = palette.mat(materials, "#D5DADF", 0.3, 0.45);
    spawn_box(commands, palette, cab_m.clone(), Vec3::new(12.55, 0.46, -7.6), Vec3::new(0.95, 0.92, 0.75));
    spawn_box(commands, palette, top_m, Vec3::new(12.55, 0.955, -7.6), Vec3::new(1.0, 0.07, 0.84));
    spawn_box(commands, palette, sink_m.clone(), Vec3::new(12.55, 0.99, -7.55), Vec3::new(0.5, 0.03, 0.36));
    spawn_cyl(commands, palette, metal.clone(), Vec3::new(12.55, 1.14, -7.86), 0.04, 0.3);
    spawn_cyl_rot(commands, palette, metal.clone(), Vec3::new(12.55, 1.29, -7.78), 0.032, 0.18, Quat::from_rotation_x(FRAC_PI_2));

    spawn_box(commands, palette, fridge_m, Vec3::new(13.52, 1.0, -7.6), Vec3::new(0.86, 2.0, 0.74));
    spawn_box(commands, palette, sink_m.clone(), Vec3::new(13.52, 1.32, -7.225), Vec3::new(0.86, 0.015, 0.02));
    for y in [0.85_f32, 1.65] {
        spawn_box(commands, palette, metal.clone(), Vec3::new(13.15, y, -7.2), Vec3::new(0.03, 0.34, 0.04));
    }
    spawn_box(commands, palette, cab_m, Vec3::new(10.6, 2.1, -7.8), Vec3::new(2.9, 0.72, 0.36));
    for x in [9.65_f32, 10.6, 11.55] {
        spawn_box(commands, palette, cab_d.clone(), Vec3::new(x, 2.1, -7.615), Vec3::new(0.9, 0.66, 0.02));
        spawn_box(commands, palette, metal.clone(), Vec3::new(x, 1.86, -7.6), Vec3::new(0.14, 0.03, 0.03));
    }
    spawn_water_cooler(commands, palette, materials, 8.92, -6.6, true);
    for (idx, (tx, tz)) in [(10.4_f32, -4.9_f32), (12.8, -4.9), (10.4, -0.9), (12.8, -0.9)].iter().enumerate() {
        spawn_dining_set(commands, palette, materials, idx, *tx, *tz);
    }

    /* 10. Lounge: beanbag + FoosballTable (`pro/floor.js:168-186`) */
    let bb_m = palette.mat(materials, "#E8742A", 0.95, 0.0);
    let bb_top = palette.mat(materials, lighten_hex("#E8742A", 0.15).as_str(), 0.95, 0.0);
    spawn_sphere(commands, palette, bb_m, Vec3::new(9.9, 0.26, 5.5), Vec3::new(0.92, 0.52, 0.88));
    spawn_sphere(commands, palette, bb_top, Vec3::new(9.85, 0.5, 5.38), Vec3::new(0.56, 0.28, 0.56));

    // Foosball table at (10.35, 0, 6.85)
    let fx = 10.35_f32;
    let fz = 6.85_f32;
    let fb_body = palette.mat(materials, "#8A5A3C", 0.55, 0.0);
    let fb_rim = palette.mat(materials, "#4A3426", 0.5, 0.0);
    let fb_pitch = palette.mat(materials, "#3F8F4A", 0.85, 0.0);
    let fb_line = palette.matte(materials, "#EDEDE6");
    let fb_goal = palette.matte(materials, "#111317");
    let fb_red = palette.mat(materials, "#E63946", 0.45, 0.0);
    let fb_blue = palette.mat(materials, "#3F7CC4", 0.45, 0.0);
    let l = 1.2_f32;
    let w = 0.7_f32;
    let top = 0.92_f32;
    let fb_ent = spawn_box(commands, palette, fb_body, Vec3::new(fx, top - 0.1, fz), Vec3::new(l, 0.2, w));
    commands.entity(fb_ent).insert(FoosballTable);
    spawn_box(commands, palette, fb_pitch, Vec3::new(fx, top - 0.08, fz), Vec3::new(l - 0.08, 0.02, w - 0.08));
    spawn_box(commands, palette, fb_line, Vec3::new(fx, top - 0.068, fz), Vec3::new(0.012, 0.004, w - 0.1));
    for sz in [-1.0_f32, 1.0] {
        spawn_box(commands, palette, fb_rim.clone(), Vec3::new(fx, top + 0.03, fz + sz * (w * 0.5 - 0.025)), Vec3::new(l, 0.12, 0.05));
    }
    for sx in [-1.0_f32, 1.0] {
        spawn_box(commands, palette, fb_rim.clone(), Vec3::new(fx + sx * (l * 0.5 - 0.025), top + 0.03, fz), Vec3::new(0.05, 0.12, w));
        spawn_box(commands, palette, fb_goal.clone(), Vec3::new(fx + sx * (l * 0.5 - 0.05), top - 0.02, fz), Vec3::new(0.03, 0.07, 0.2));
        for sz in [-1.0_f32, 1.0] {
            spawn_box(commands, palette, fb_rim.clone(), Vec3::new(fx + sx * (l * 0.5 - 0.08), (top - 0.2) * 0.5, fz + sz * (w * 0.5 - 0.08)), Vec3::new(0.09, top - 0.2, 0.09));
        }
    }
    spawn_box(commands, palette, fb_rim, Vec3::new(fx, 0.18, fz), Vec3::new(l - 0.2, 0.05, 0.05));
    // 4 steel rods with team players
    for (r_i, rx) in [-0.36_f32, -0.12, 0.12, 0.36].iter().enumerate() {
        spawn_cyl_rot(commands, palette, sink_m.clone(), Vec3::new(fx + rx, top + 0.02, fz), 0.016, w + 0.26, Quat::from_rotation_x(FRAC_PI_2));
        let team_m = if r_i % 2 == 0 { fb_red.clone() } else { fb_blue.clone() };
        for pz in [-0.16_f32, 0.0, 0.16] {
            spawn_box(commands, palette, team_m.clone(), Vec3::new(fx + rx, top - 0.01, fz + pz), Vec3::new(0.03, 0.09, 0.03));
        }
    }

    /* 11. Server room: precision AC cooler + live CPU/°C screen + fire extinguisher (`pro/floor.js:188-196`) */
    let ac_body = palette.mat(materials, "#E8EBEE", 0.5, 0.0);
    spawn_box(commands, palette, ac_body, Vec3::new(-7.58, 0.95, 0.3), Vec3::new(0.7, 1.9, 0.44));
    for i in 0..6 {
        spawn_box(commands, palette, sink_m.clone(), Vec3::new(-7.22, 1.2 + i as f32 * 0.1, 0.3), Vec3::new(0.02, 0.05, 0.36));
    }
    let ac_bezel = palette.mat(materials, "#1B1F26", 0.4, 0.1);
    spawn_box(commands, palette, ac_bezel, Vec3::new(-7.225, 0.72, 0.3), Vec3::new(0.02, 0.24, 0.36));
    let cooler_mat = palette.textured_unlit(images, materials, draw_cooler_screen(14, 21.4, fonts));
    let cooler_ent = spawn_quad(
        commands,
        palette,
        cooler_mat.clone(),
        Vec3::new(-7.21, 0.72, 0.3),
        Vec2::new(0.32, 0.19),
        Quat::from_rotation_y(FRAC_PI_2),
    );
    commands.entity(cooler_ent).insert(CoolerScreen { material: cooler_mat });
    let ext_m = palette.mat(materials, "#D62828", 0.4, 0.15);
    spawn_cyl(commands, palette, ext_m, Vec3::new(-3.88, 0.25, 0.3), 0.16, 0.5);
    spawn_cyl(commands, palette, metal.clone(), Vec3::new(-3.88, 0.54, 0.3), 0.06, 0.08);

    /* 12. Lobby: glowing My Office sign, walnut U reception counter, waiting corner (`pro/floor.js:198-257`) */
    let logo_mat = palette.textured_unlit(images, materials, draw_my_office_logo(fonts));
    let sign_bg = palette.mat(materials, "#1C2620", 0.5, 0.0);
    spawn_box(commands, palette, sign_bg.clone(), Vec3::new(-7.96, 3.6, spec.door_z), Vec3::new(0.06, 0.62, 2.7));
    spawn_quad(
        commands,
        palette,
        logo_mat.clone(),
        Vec3::new(-7.92, 3.6, spec.door_z),
        Vec2::new(2.6, 0.56),
        Quat::from_rotation_y(FRAC_PI_2),
    );

    // Reception counter at (-4.9, 0, 1.5)
    let rcx = -4.9_f32;
    let rcz = 1.5_f32;
    let rc_walnut = palette.mat(materials, "#7A5640", 0.5, 0.0);
    let rc_stripe = palette.mat(materials, "#D97757", 0.5, 0.0);
    let rc_groove = palette.mat(materials, "#5E412F", 0.6, 0.0);
    let rc_stone = palette.mat(materials, "#F2EEE8", 0.3, 0.0);
    let rc_oak = palette.mat(materials, "#E3DACB", 0.55, 0.0);

    let rc_ent = spawn_box(commands, palette, rc_walnut.clone(), Vec3::new(rcx, 0.51, rcz + 0.48), Vec3::new(2.1, 1.02, 0.1));
    commands.entity(rc_ent).insert(ReceptionCounter);
    spawn_box(commands, palette, rc_stripe, Vec3::new(rcx, 0.86, rcz + 0.49), Vec3::new(2.12, 0.06, 0.12));
    for gx in [-0.6_f32, 0.6] {
        spawn_box(commands, palette, rc_groove.clone(), Vec3::new(rcx + gx, 0.47, rcz + 0.535), Vec3::new(0.02, 0.92, 0.02));
    }
    spawn_quad(commands, palette, logo_mat, Vec3::new(rcx, 0.58, rcz + 0.538), Vec2::new(1.0, 0.215), Quat::IDENTITY);
    spawn_box(commands, palette, rc_walnut.clone(), Vec3::new(rcx + 1.0, 0.51, rcz + 0.05), Vec3::new(0.1, 1.02, 0.95));
    spawn_box(commands, palette, rc_stone, Vec3::new(rcx, 1.045, rcz + 0.4), Vec3::new(2.22, 0.05, 0.34));
    spawn_box(commands, palette, rc_oak.clone(), Vec3::new(rcx, 0.76, rcz + 0.12), Vec3::new(2.0, 0.04, 0.55));
    spawn_box(commands, palette, rc_walnut.clone(), Vec3::new(rcx - 0.72, 0.36, rcz + 0.12), Vec3::new(0.46, 0.72, 0.5));
    for k in 0..3 {
        spawn_box(commands, palette, metal.clone(), Vec3::new(rcx - 0.72, 0.18 + k as f32 * 0.22, rcz - 0.14), Vec3::new(0.12, 0.02, 0.02));
    }
    spawn_box(commands, palette, rc_walnut, Vec3::new(rcx - 0.96, 0.36, rcz + 0.12), Vec3::new(0.06, 0.72, 0.5));

    // Receptionist monitor, keyboard, mouse, desk phone, frame
    let scr_rot = Quat::from_rotation_y(PI - 0.2);
    let scr_at = |lx: f32, ly: f32, lz: f32| Vec3::new(rcx + 0.22, 0.78, rcz + 0.26) + scr_rot * Vec3::new(lx, ly, lz);
    let dark_body = palette.mat(materials, "#1B1F26", 0.4, 0.1);
    let rc_glow = palette.emissive(materials, "#CFE3F4", 0.75);
    spawn_box_rot(commands, palette, metal.clone(), scr_at(0.0, 0.006, 0.0), Vec3::new(0.16, 0.012, 0.12), scr_rot);
    spawn_cyl(commands, palette, metal.clone(), scr_at(0.0, 0.09, 0.0), 0.028, 0.16);
    spawn_box_rot(commands, palette, dark_body, scr_at(0.0, 0.3, 0.01), Vec3::new(0.5, 0.3, 0.03), scr_rot);
    spawn_quad(commands, palette, rc_glow, scr_at(0.0, 0.3, 0.027), Vec2::new(0.46, 0.26), scr_rot);
    spawn_box(commands, palette, metal.clone(), Vec3::new(rcx, 0.787, rcz - 0.02), Vec3::new(0.38, 0.016, 0.13));
    let mouse_m = palette.mat(materials, "#3A3F47", 0.45, 0.1);
    spawn_box(commands, palette, mouse_m, Vec3::new(rcx + 0.27, 0.786, rcz - 0.04), Vec3::new(0.07, 0.014, 0.1));
    let ph_rot = Quat::from_rotation_y(0.3);
    let ph_at = |lx: f32, ly: f32, lz: f32| Vec3::new(rcx - 0.5, 0.78, rcz) + ph_rot * Vec3::new(lx, ly, lz);
    let handset = palette.mat(materials, "#1E2126", 0.45, 0.1);
    spawn_box_rot(commands, palette, metal.clone(), ph_at(0.0, 0.025, 0.0), Vec3::new(0.2, 0.05, 0.16), ph_rot);
    spawn_box_rot(commands, palette, handset.clone(), ph_at(0.0, 0.065, -0.04), Vec3::new(0.2, 0.035, 0.05), ph_rot);
    spawn_trinket(commands, palette, materials, tiles, TrinketKind::Frame, rcx + 0.62, 0.78, rcz + 0.25, PI + 0.4);

    // Ledge props: brass bell, open visitor book + pen, glass vase with white lilies, RECEPTION desk tag
    spawn_cyl(commands, palette, metal.clone(), Vec3::new(rcx - 0.62, 1.08, rcz + 0.42), 0.11, 0.02);
    spawn_sphere(commands, palette, brass, Vec3::new(rcx - 0.62, 1.095, rcz + 0.42), Vec3::new(0.08, 0.05, 0.08));
    let bk_rot = Quat::from_rotation_y(0.15);
    let bk_at = |lx: f32, ly: f32, lz: f32| Vec3::new(rcx - 0.1, 1.072, rcz + 0.42) + bk_rot * Vec3::new(lx, ly, lz);
    let page_m = palette.mat(materials, "#FBF8F0", 0.9, 0.0);
    for sx in [-1.0_f32, 1.0] {
        spawn_box_rot(commands, palette, page_m.clone(), bk_at(sx * 0.082, 0.006, 0.0), Vec3::new(0.16, 0.012, 0.22), bk_rot * Quat::from_rotation_z(-sx * 0.06));
    }
    spawn_box_rot(commands, palette, handset, bk_at(0.06, 0.016, 0.0), Vec3::new(0.006, 0.02, 0.15), bk_rot * Quat::from_rotation_y(0.4));

    // Glass vase with white lilies
    let vx = rcx + 0.78;
    let vz = rcz + 0.4;
    let vase_m = palette.glass(materials, "#CFE8F0", 0.55);
    let stem_m = palette.mat(materials, "#4E7A3A", 0.6, 0.0);
    let lily_m = palette.mat(materials, "#FFFFFF", 0.6, 0.0);
    let leaf_m = palette.mat(materials, "#5E8C3A", 0.7, 0.0);
    spawn_cyl(commands, palette, vase_m, Vec3::new(vx, 1.17, vz), 0.08, 0.2);
    for (lx, ly, lz) in [(0.0_f32, 0.33_f32, 0.0_f32), (-0.06, 0.29, 0.03), (0.06, 0.3, -0.02), (0.02, 0.27, 0.07)] {
        spawn_cyl(commands, palette, stem_m.clone(), Vec3::new(vx + lx * 0.5, 1.17 + (ly - 0.1) * 0.5, vz + lz * 0.5), 0.008, ly - 0.1);
        spawn_cone_rot(commands, palette, lily_m.clone(), Vec3::new(vx + lx, 1.07 + ly, vz + lz), Vec3::new(0.09, 0.07, 0.09), Quat::from_rotation_x(PI));
    }
    for (lx, lz) in [(-0.07_f32, -0.03_f32), (0.07, 0.04)] {
        spawn_sphere(commands, palette, leaf_m.clone(), Vec3::new(vx + lx, 1.31, vz + lz), Vec3::new(0.07, 0.028, 0.11));
    }

    // Reception desk sign + receptionist chair
    let tag_m = palette.textured_unlit(images, materials, draw_reception_tag("RECEPTION", fonts));
    let tag_rot = Quat::from_rotation_x(-0.25);
    spawn_box_rot(commands, palette, sign_bg, Vec3::new(rcx + 0.35, 1.11, rcz + 0.47), Vec3::new(0.26, 0.075, 0.035), tag_rot);
    spawn_quad(commands, palette, tag_m, Vec3::new(rcx + 0.35, 1.112, rcz + 0.49), Vec2::new(0.24, 0.06), tag_rot);
    spawn_office_chair(commands, palette, materials, rcx, rcz - 0.45, 0.0, "#2F3A48", false);

    // Waiting corner: plant, green armchair, magazine side table + cactus
    spawn_typed_plant(commands, palette, materials, -3.95, 0.0, 5.0, 0.42, PlantKind::Bush);
    spawn_chair_uph(commands, palette, materials, -5.75, 4.88, PI, "#2F4A3D", 1.0);
    spawn_cyl(commands, palette, rc_oak, Vec3::new(-6.35, 0.5, 5.05), 0.44, 0.03);
    spawn_cyl(commands, palette, metal.clone(), Vec3::new(-6.35, 0.25, 5.05), 0.06, 0.48);
    spawn_cyl(commands, palette, metal.clone(), Vec3::new(-6.35, 0.01, 5.05), 0.3, 0.02);
    for (col, ry, y) in [("#D97757", 0.25_f32, 0.525_f32), ("#3F7CC4", -0.15, 0.537)] {
        let mag = palette.matte(materials, col);
        spawn_box_rot(commands, palette, mag, Vec3::new(-6.32, y, 5.03), Vec3::new(0.17, 0.01, 0.23), Quat::from_rotation_y(ry));
    }
    spawn_trinket(commands, palette, materials, tiles, TrinketKind::Cactus, -6.45, 0.515, 5.13, 0.0);

    /* 13. Restrooms: 2 pivoting stalls + indicators + basin/mirror + WC sign (`pro/floor.js:258-289`) */
    let tile_m = palette.mat(materials, "#F4F6F7", 0.4, 0.0);
    let part_m = palette.mat(materials, "#B9C7D1", 0.6, 0.0);
    let stall_door_m = palette.mat(materials, "#8FA6B6", 0.55, 0.0);
    for z in [5.45_f32, 6.75, 8.05] {
        spawn_box(commands, palette, part_m.clone(), Vec3::new(-7.38, 0.97, z), Vec3::new(1.25, 1.85, 0.05));
    }
    for i in 0..2usize {
        let z0 = 5.45 + i as f32 * 1.3;
        let zc = z0 + 0.65;
        for (a, b) in [(z0, zc - 0.36), (zc + 0.36, z0 + 1.3)] {
            spawn_box(commands, palette, part_m.clone(), Vec3::new(-6.75, 0.97, (a + b) * 0.5), Vec3::new(0.05, 1.85, b - a));
        }
        spawn_box(commands, palette, part_m.clone(), Vec3::new(-6.75, 1.92, zc), Vec3::new(0.05, 0.08, 0.74));
        // Toilet bowl + tank
        spawn_cyl(commands, palette, tile_m.clone(), Vec3::new(-7.5, 0.2, zc), 0.3, 0.4);
        spawn_box(commands, palette, tile_m.clone(), Vec3::new(-7.78, 0.55, zc), Vec3::new(0.18, 0.34, 0.36));
        // Pivoting door (vacant stalls sit slightly ajar inward like Oracle layout-pro.jpg) + green/red indicator
        let ind_m = palette.emissive(materials, "#3DDC84", 0.9);
        let pivot = commands
            .spawn((
                WorldEntity,
                Transform::from_xyz(-6.75, 0.0, zc - 0.36)
                    .with_rotation(Quat::from_rotation_y(-0.55)),
                Visibility::default(),
            ))
            .with_children(|p| {
                p.spawn((
                    WorldEntity,
                    Mesh3d(palette.unit_box.clone()),
                    MeshMaterial3d(stall_door_m.clone()),
                    Transform::from_xyz(0.0, 0.93, 0.355).with_scale(Vec3::new(0.04, 1.62, 0.71)),
                ));
                p.spawn((
                    WorldEntity,
                    Mesh3d(palette.unit_box.clone()),
                    MeshMaterial3d(ind_m.clone()),
                    Transform::from_xyz(0.03, 1.2, 0.62).with_scale(Vec3::new(0.02, 0.05, 0.12)),
                ));
            })
            .id();
        commands.entity(pivot).insert(RestroomStall {
            index: i,
            door_pivot: pivot,
            indicator_mat: ind_m,
            open: 1.0,
        });
    }
    let van_m = palette.mat(materials, "#EDE9E2", 0.5, 0.0);
    let mirror_m = palette.mat(materials, "#C9DCE8", 0.05, 0.65);
    spawn_box(commands, palette, van_m, Vec3::new(-7.72, 0.43, 8.5), Vec3::new(0.55, 0.86, 0.78));
    spawn_cyl(commands, palette, tile_m, Vec3::new(-7.7, 0.88, 8.5), 0.3, 0.08);
    spawn_cyl(commands, palette, metal.clone(), Vec3::new(-7.92, 0.98, 8.5), 0.03, 0.16);
    spawn_quad(commands, palette, mirror_m, Vec3::new(-7.965, 1.6, 8.5), Vec2::new(0.72, 0.85), Quat::from_rotation_y(FRAC_PI_2));

    let wc_m = palette.textured_unlit(images, materials, draw_wc_sign());
    spawn_quad(commands, palette, wc_m.clone(), Vec3::new(-4.0, 1.55, 5.435), Vec2::new(0.46, 0.23), Quat::IDENTITY);
    spawn_quad(commands, palette, wc_m, Vec3::new(-4.0, 1.55, 5.365), Vec2::new(0.46, 0.23), Quat::from_rotation_y(PI));
    spawn_box(commands, palette, metal.clone(), Vec3::new(-4.0, 1.38, 5.4), Vec3::new(0.04, 0.42, 0.04));

    /* 14. Dev floor: pod top rails + storage units, 2 phone booths, filing cabinet, 2 posters, water cooler */
    let stor_m = palette.mat(materials, "#E4DFD6", 0.6, 0.0);
    let stor_d = palette.mat(materials, "#D6D0C5", 0.6, 0.0);
    for &[cx, cz] in spec.pods.iter().take(3) {
        spawn_box(commands, palette, metal.clone(), Vec3::new(cx, 1.205, cz), Vec3::new(3.08, 0.025, 0.06));
        spawn_box(commands, palette, stor_m.clone(), Vec3::new(cx + 1.72, 0.31, cz), Vec3::new(0.42, 0.62, 1.4));
        for dz in [-0.35_f32, 0.35] {
            spawn_box(commands, palette, stor_d.clone(), Vec3::new(cx + 1.94, 0.31, cz + dz), Vec3::new(0.02, 0.5, 0.6));
        }
        spawn_typed_plant(commands, palette, materials, cx + 1.72, 0.62, cz + 0.32, 0.4, PlantKind::Snake);
        spawn_trinket(commands, palette, materials, tiles, TrinketKind::Books, cx + 1.72, 0.62, cz - 0.3, 0.3);
    }

    // 2 acoustic glass phone booths (placed cleanly alongside karaoke booth at z = 7.4)
    let pb_felt = palette.mat(materials, "#7E9A82", 0.95, 0.0);
    let pb_glass = palette.glass(materials, "#CFE3F2", 0.18);
    let pb_shelf = palette.mat(materials, "#B88A5E", 0.55, 0.0);
    let pb_stool = palette.mat(materials, "#E8742A", 0.9, 0.0);
    let pb_light = palette.emissive(materials, "#EDE6D6", 0.9);
    for (idx, pz) in [4.72_f32, 6.0].iter().enumerate() {
        let bx = -0.75_f32;
        let base_ent = spawn_box(commands, palette, metal.clone(), Vec3::new(bx, 0.04, *pz), Vec3::new(1.0, 0.08, 1.15));
        commands.entity(base_ent).insert(PhoneBooth { index: idx });
        spawn_box(commands, palette, metal.clone(), Vec3::new(bx, 2.2, *pz), Vec3::new(1.0, 0.1, 1.15));
        spawn_box(commands, palette, pb_felt.clone(), Vec3::new(bx - 0.47, 1.1, *pz), Vec3::new(0.06, 2.12, 1.15));
        for sz in [-1.0_f32, 1.0] {
            spawn_box(commands, palette, pb_felt.clone(), Vec3::new(bx, 1.1, pz + sz * 0.545), Vec3::new(1.0, 2.12, 0.06));
            spawn_box(commands, palette, metal.clone(), Vec3::new(bx + 0.48, 1.1, pz + sz * 0.52), Vec3::new(0.06, 2.12, 0.06));
        }
        spawn_box(commands, palette, pb_glass.clone(), Vec3::new(bx + 0.48, 1.12, *pz), Vec3::new(0.03, 2.0, 1.02));
        spawn_cyl(commands, palette, metal.clone(), Vec3::new(bx + 0.5, 1.1, pz + 0.4), 0.032, 0.42);
        spawn_box(commands, palette, pb_shelf.clone(), Vec3::new(bx - 0.22, 0.98, *pz), Vec3::new(0.42, 0.04, 0.36));
        spawn_cyl(commands, palette, pb_stool.clone(), Vec3::new(bx + 0.05, 0.225, *pz), 0.32, 0.45);
        spawn_box(commands, palette, pb_light.clone(), Vec3::new(bx, 2.13, *pz), Vec3::new(0.5, 0.03, 0.5));
    }

    // 4-drawer metallic filing cabinet (`pro/floor.js:316-318`)
    let fc_m = palette.mat(materials, "#B9BEC5", 0.45, 0.35);
    let fc_seam = palette.mat(materials, "#8E949C", 0.5, 0.3);
    spawn_box(commands, palette, fc_m, Vec3::new(-3.15, 0.65, -0.35), Vec3::new(0.46, 1.3, 0.6));
    for k in 0..4 {
        let y = 0.33 + k as f32 * 0.31;
        spawn_box(commands, palette, fc_seam.clone(), Vec3::new(-3.15, y, -0.045), Vec3::new(0.42, 0.005, 0.02));
        spawn_box(commands, palette, metal.clone(), Vec3::new(-3.15, 0.22 + k as f32 * 0.31, -0.04), Vec3::new(0.14, 0.025, 0.03));
    }

    // 2 framed posters (`pro/floor.js:320-337`)
    let p1_m = palette.textured_lit(images, materials, draw_poster_ship_often(fonts), 0.9);
    spawn_box(commands, palette, metal.clone(), Vec3::new(-7.47, 1.75, 4.42), Vec3::new(0.74, 0.98, 0.04));
    spawn_quad(commands, palette, p1_m, Vec3::new(-7.47, 1.75, 4.442), Vec2::new(0.66, 0.9), Quat::IDENTITY);

    let p2_m = palette.textured_lit(images, materials, draw_poster_team_outing(fonts), 0.9);
    spawn_box(commands, palette, metal, Vec3::new(9.6, 2.98, -7.96), Vec3::new(0.88, 0.66, 0.04));
    spawn_quad(commands, palette, p2_m, Vec3::new(9.6, 2.98, -7.938), Vec2::new(0.8, 0.58), Quat::IDENTITY);

    // Corridor water cooler (`pro/floor.js:340-342`)
    spawn_water_cooler(commands, palette, materials, 7.95, -1.95, false);
}
