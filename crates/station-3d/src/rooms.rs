use crate::layout::LayoutSpec;
use crate::palette::{
    WorldPalette, spawn_box, spawn_box_rot, spawn_cyl, spawn_quad, spawn_sphere,
};
use bevy::prelude::*;
use station_canvas2d::{Canvas2d, FontEngine, TextAlign, hex};
use std::f32::consts::PI;

pub fn spawn_pot_plant(
    commands: &mut Commands,
    palette: &mut WorldPalette,
    materials: &mut Assets<StandardMaterial>,
    x: f32,
    y: f32,
    z: f32,
    scale: f32,
    tall_tree: bool,
) {
    let pot_m = palette.mat(materials, "#E4DDD0", 0.45, 0.0);
    let soil_m = palette.matte(materials, "#4A3527");
    let leaf_a = palette.mat(materials, "#4E8A3A", 0.5, 0.0);
    let leaf_b = palette.mat(materials, "#62A04A", 0.5, 0.0);
    let trunk_m = palette.mat(materials, "#6E4B32", 0.6, 0.0);

    spawn_cyl(commands, palette, pot_m, Vec3::new(x, y + 0.2 * scale, z), 0.44 * scale, 0.4 * scale);
    spawn_cyl(commands, palette, soil_m, Vec3::new(x, y + 0.41 * scale, z), 0.38 * scale, 0.03 * scale);

    if tall_tree {
        spawn_cyl(commands, palette, trunk_m, Vec3::new(x, y + 0.95 * scale, z), 0.1 * scale, 1.15 * scale);
        spawn_sphere(commands, palette, leaf_a, Vec3::new(x, y + 1.65 * scale, z), Vec3::splat(0.75 * scale));
        spawn_sphere(commands, palette, leaf_b, Vec3::new(x + 0.18 * scale, y + 1.48 * scale, z - 0.12 * scale), Vec3::splat(0.55 * scale));
    } else {
        spawn_sphere(commands, palette, leaf_a, Vec3::new(x, y + 0.68 * scale, z), Vec3::new(0.5 * scale, 0.55 * scale, 0.5 * scale));
        spawn_sphere(commands, palette, leaf_b, Vec3::new(x + 0.12 * scale, y + 0.62 * scale, z + 0.1 * scale), Vec3::splat(0.36 * scale));
    }
}

fn spawn_rug(
    commands: &mut Commands,
    palette: &mut WorldPalette,
    materials: &mut Assets<StandardMaterial>,
    x: f32,
    y: f32,
    z: f32,
    w: f32,
    d: f32,
    color: &str,
) {
    let m = palette.mat(materials, color, 0.92, 0.0);
    spawn_box(commands, palette, m, Vec3::new(x, y, z), Vec3::new(w, 0.022, d));
}

fn spawn_floor_zone(
    commands: &mut Commands,
    palette: &mut WorldPalette,
    materials: &mut Assets<StandardMaterial>,
    x0: f32,
    x1: f32,
    z0: f32,
    z1: f32,
    y: f32,
    color: &str,
    rough: f32,
) {
    let m = palette.mat(materials, color, rough, 0.0);
    spawn_box(
        commands,
        palette,
        m,
        Vec3::new((x0 + x1) * 0.5, y, (z0 + z1) * 0.5),
        Vec3::new((x1 - x0).abs(), 0.014, (z1 - z0).abs()),
    );
}

fn spawn_wall_window(
    commands: &mut Commands,
    palette: &mut WorldPalette,
    materials: &mut Assets<StandardMaterial>,
    x: f32,
    y: f32,
    z: f32,
    w: f32,
    h: f32,
    ry: f32,
) {
    let frame_m = palette.mat(materials, "#2F343E", 0.45, 0.3);
    let sill_m = palette.mat(materials, "#7A5A3E", 0.55, 0.0);
    let sky_m = palette.emissive(materials, "#9ED2F6", 0.65);
    let rot = Quat::from_rotation_y(ry);
    let fwd = rot * Vec3::Z;

    spawn_box_rot(commands, palette, frame_m.clone(), Vec3::new(x, y, z), Vec3::new(w + 0.18, h + 0.18, 0.12), rot);
    spawn_quad(commands, palette, sky_m, Vec3::new(x, y, z) + fwd * 0.068, Vec2::new(w, h), rot);
    spawn_box_rot(
        commands,
        palette,
        sill_m,
        Vec3::new(x, y - h * 0.5 - 0.11, z) + fwd * 0.12,
        Vec3::new(w + 0.36, 0.09, 0.32),
        rot,
    );
}

fn spawn_partition_segment(
    commands: &mut Commands,
    palette: &mut WorldPalette,
    materials: &mut Assets<StandardMaterial>,
    ax: f32,
    az: f32,
    bx: f32,
    bz: f32,
    height: f32,
    glass: bool,
    color: &str,
) {
    let dx = bx - ax;
    let dz = bz - az;
    let len = dx.hypot(dz);
    if len < 0.05 {
        return;
    }
    let ang = -dz.atan2(dx);
    let rot = Quat::from_rotation_y(ang);
    let mid = Vec3::new((ax + bx) * 0.5, height * 0.5, (az + bz) * 0.5);

    if glass {
        let gm = palette.glass(materials, "#BFD8EE", 0.22);
        let metal = palette.mat(materials, "#2B2F36", 0.35, 0.6);
        spawn_box_rot(commands, palette, gm, mid, Vec3::new(len, height, 0.04), rot);
        spawn_box_rot(commands, palette, metal.clone(), Vec3::new(mid.x, height, mid.z), Vec3::new(len, 0.06, 0.07), rot);
        spawn_box_rot(commands, palette, metal, Vec3::new(mid.x, 0.04, mid.z), Vec3::new(len, 0.08, 0.07), rot);
    } else {
        let wm = palette.mat(materials, color, 0.85, 0.0);
        let cap = palette.mat(materials, "#D9D1C3", 0.6, 0.0);
        spawn_box_rot(commands, palette, wm, mid, Vec3::new(len, height, 0.12), rot);
        spawn_box_rot(commands, palette, cap, Vec3::new(mid.x, height + 0.03, mid.z), Vec3::new(len + 0.04, 0.06, 0.16), rot);
    }
}

fn spawn_terrace(
    commands: &mut Commands,
    palette: &mut WorldPalette,
    materials: &mut Assets<StandardMaterial>,
    spec: &LayoutSpec,
) {
    let pro = spec.layout.is_pro();
    let hq = spec.layout.is_hq();
    let stone_m = palette.mat(materials, "#3A3F47", 0.85, 0.05);
    let metal_m = palette.mat(materials, "#2B2F36", 0.35, 0.65);
    let rail_g = palette.glass(materials, "#B5D3F0", 0.2);
    let bulb_m = palette.emissive(materials, "#FFE2A8", 1.1);

    let (pos, sz) = if hq {
        (Vec3::new(3.4, -0.42, 10.1), Vec3::new(24.6, 0.3, 37.2))
    } else if pro {
        (Vec3::new(3.4, -0.42, 1.6), Vec3::new(24.6, 0.3, 20.2))
    } else {
        (Vec3::new(1.6, -0.42, 2.0), Vec3::new(21.0, 0.3, 17.4))
    };
    spawn_box(commands, palette, stone_m, pos, sz);

    let fz = if hq { 28.5 } else if pro { 11.5 } else { 10.5 };
    let rx = if pro { 15.5 } else { 11.9 };
    let rz0 = if pro { -8.4 } else { -6.8 };

    // Right railing + front railing
    let r_len = fz - rz0;
    spawn_box(commands, palette, rail_g.clone(), Vec3::new(rx, 0.25, (rz0 + fz) * 0.5), Vec3::new(0.03, 0.9, r_len));
    spawn_box(commands, palette, metal_m.clone(), Vec3::new(rx, 0.72, (rz0 + fz) * 0.5), Vec3::new(0.08, 0.06, r_len));
    let f_len = rx - (-8.8);
    spawn_box(commands, palette, rail_g, Vec3::new((-8.8 + rx) * 0.5, 0.25, fz), Vec3::new(f_len, 0.9, 0.03));
    spawn_box(commands, palette, metal_m.clone(), Vec3::new((-8.8 + rx) * 0.5, 0.72, fz), Vec3::new(f_len, 0.06, 0.08));

    // Bollard lights
    let bollards: Vec<(f32, f32)> = if pro {
        let mut b = vec![(15.0, -5.5), (15.0, 0.5), (15.0, 6.5), (-5.0, fz - 0.5), (1.0, fz - 0.5), (8.5, fz - 0.5)];
        if hq {
            b.extend_from_slice(&[(15.0, 12.5), (15.0, 18.5), (15.0, 24.5)]);
        }
        b
    } else {
        vec![(11.3, -4.5), (11.3, 1.5), (11.3, 7.5), (-5.0, 9.9), (1.0, 9.9), (7.0, 9.9)]
    };
    for (bx, bz) in bollards {
        spawn_cyl(commands, palette, metal_m.clone(), Vec3::new(bx, 0.0, bz), 0.18, 0.55);
        spawn_cyl(commands, palette, bulb_m.clone(), Vec3::new(bx, 0.3, bz), 0.17, 0.08);
    }

    // Corner planters with trees/bushes
    let planters: Vec<(f32, f32, bool)> = if pro {
        let mut p = vec![(14.9, -7.7, true), (14.9, 3.4, false), (-7.9, fz - 0.7, true), (4.6, fz - 0.7, false), (14.9, fz - 0.7, true)];
        if hq {
            p.extend_from_slice(&[(14.9, 14.6, false), (14.9, 21.6, true)]);
        }
        p
    } else {
        vec![(10.8, -5.6, true), (10.8, 4.6, true), (-7.2, 9.4, true), (4.2, 9.4, false)]
    };
    let box_m = palette.mat(materials, "#4A4F57", 0.7, 0.0);
    for (px, pz, tree) in planters {
        spawn_box(commands, palette, box_m.clone(), Vec3::new(px, -0.02, pz), Vec3::new(1.0, 0.5, 1.0));
        spawn_pot_plant(commands, palette, materials, px, -0.15, pz, 1.15, tree);
    }

    // Outdoor bistro table
    let (ox, oz) = if hq { (11.6, 27.3) } else if pro { (11.6, 10.3) } else { (9.9, 0.9) };
    let top_m = palette.mat(materials, "#E9E4DA", 0.45, 0.0);
    let chair_m = palette.matte(materials, "#C9A27A");
    spawn_cyl(commands, palette, top_m, Vec3::new(ox, 0.42, oz), 0.9, 0.05);
    spawn_cyl(commands, palette, metal_m, Vec3::new(ox, 0.06, oz), 0.1, 0.66);
    for a in [0.6_f32, 2.4, 4.2] {
        let cx = ox + a.sin() * 0.85;
        let cz = oz + a.cos() * 0.85;
        spawn_box_rot(commands, palette, chair_m.clone(), Vec3::new(cx, 0.21, cz), Vec3::new(0.46, 0.06, 0.44), Quat::from_rotation_y(a + PI));
    }
}

pub fn spawn_office_shell(
    commands: &mut Commands,
    palette: &mut WorldPalette,
    materials: &mut Assets<StandardMaterial>,
    images: &mut Assets<Image>,
    fonts: &mut FontEngine,
    spec: &LayoutSpec,
) {
    spawn_terrace(commands, palette, materials, spec);

    if !spec.layout.is_pro() {
        // --- COZY 16×12m ROOM ---
        let slab = palette.mat(materials, "#7A5A3E", 0.65, 0.0);
        let floor = palette.mat(materials, "#C99F72", 0.58, 0.0);
        let wall = palette.mat(materials, "#F1E6D2", 0.9, 0.0);
        let wain = palette.mat(materials, "#C9D5B2", 0.85, 0.0);
        let trim = palette.mat(materials, "#8C6646", 0.6, 0.0);
        let cap = palette.mat(materials, "#E7DAC3", 0.75, 0.0);

        spawn_box(commands, palette, slab, Vec3::new(0.0, -0.25, 0.0), Vec3::new(16.6, 0.5, 12.6));
        spawn_box(commands, palette, floor, Vec3::new(0.0, 0.005, 0.0), Vec3::new(16.0, 0.01, 12.0));
        spawn_box(commands, palette, wall.clone(), Vec3::new(0.0, 2.1, -6.15), Vec3::new(16.6, 4.2, 0.3));
        spawn_box(commands, palette, wall, Vec3::new(-8.15, 2.1, 0.0), Vec3::new(0.3, 4.2, 12.6));
        spawn_box(commands, palette, wain.clone(), Vec3::new(0.0, 0.63, -5.99), Vec3::new(16.0, 1.25, 0.04));
        spawn_box(commands, palette, wain, Vec3::new(-7.99, 0.63, 0.0), Vec3::new(0.04, 1.25, 12.0));
        spawn_box(commands, palette, trim.clone(), Vec3::new(0.0, 1.28, -5.96), Vec3::new(16.0, 0.06, 0.08));
        spawn_box(commands, palette, trim, Vec3::new(-7.96, 1.28, 0.0), Vec3::new(0.08, 0.06, 12.0));
        spawn_box(commands, palette, cap.clone(), Vec3::new(0.0, 4.25, -6.15), Vec3::new(16.6, 0.14, 0.42));
        spawn_box(commands, palette, cap, Vec3::new(-8.15, 4.25, 0.0), Vec3::new(0.42, 0.14, 12.6));

        // Zone rugs
        spawn_rug(commands, palette, materials, -5.2, 0.015, -4.25, 4.3, 2.3, "#D9CCE3");
        spawn_rug(commands, palette, materials, 0.6, 0.016, -4.35, 3.0, 2.0, "#C9DDEC");
        spawn_rug(commands, palette, materials, -6.85, 0.017, -1.83, 2.0, 2.45, "#CCE3D1");
        spawn_rug(commands, palette, materials, 0.0, 0.018, 0.35, 6.9, 5.7, "#E6D3AE");
        spawn_rug(commands, palette, materials, -5.5, 0.019, 4.4, 3.8, 2.3, "#EFC9A2");
        spawn_rug(commands, palette, materials, -7.45, 0.02, 1.5, 0.9, 1.5, "#8F6240");
        let srv_floor = palette.mat(materials, "#262A31", 0.6, 0.1);
        spawn_box(commands, palette, srv_floor, Vec3::new(5.55, 0.018, -4.85), Vec3::new(3.9, 0.03, 2.5));
        let meet_rug = palette.mat(materials, "#F0CDB6", 0.9, 0.0);
        spawn_cyl(commands, palette, meet_rug, Vec3::new(5.2, 0.018, 2.4), 4.5, 0.025);

        spawn_wall_window(commands, palette, materials, 0.9, 2.35, -5.98, 4.4, 2.3, 0.0);
        spawn_pot_plant(commands, palette, materials, 1.9, 1.13, -5.82, 0.42, false);
        spawn_pot_plant(commands, palette, materials, -0.2, 1.13, -5.82, 0.38, false);
        return;
    }

    // --- PRO 22×17m COMPANY FLOOR ---
    let slab = palette.mat(materials, "#7A5A3E", 0.65, 0.0);
    let wall = palette.mat(materials, "#EEE8DE", 0.9, 0.0);
    let cap = palette.mat(materials, "#E2D9C8", 0.75, 0.0);
    let skirt = palette.mat(materials, "#6E5440", 0.65, 0.0);

    spawn_box(commands, palette, slab.clone(), Vec3::new(3.0, -0.25, 0.5), Vec3::new(22.6, 0.5, 17.6));
    spawn_box(commands, palette, wall.clone(), Vec3::new(3.0, 2.1, -8.15), Vec3::new(22.6, 4.2, 0.3));
    spawn_box(commands, palette, wall.clone(), Vec3::new(-8.15, 2.1, 0.5), Vec3::new(0.3, 4.2, 17.6));
    spawn_box(commands, palette, cap.clone(), Vec3::new(3.0, 4.25, -8.15), Vec3::new(22.6, 0.14, 0.42));
    spawn_box(commands, palette, cap.clone(), Vec3::new(-8.15, 4.25, 0.5), Vec3::new(0.42, 0.14, 17.6));
    spawn_box(commands, palette, skirt.clone(), Vec3::new(3.0, 0.09, -7.97), Vec3::new(22.0, 0.18, 0.06));

    // Room floor zones
    spawn_floor_zone(commands, palette, materials, -3.6, 8.6, -1.6, 9.0, 0.004, "#DFC8A2", 0.62);
    spawn_floor_zone(commands, palette, materials, -3.6, 8.6, -3.0, -1.6, 0.004, "#D7D2CA", 0.72);
    spawn_floor_zone(commands, palette, materials, -8.0, -2.6, -8.0, -3.0, 0.004, "#4A5566", 0.88);
    spawn_floor_zone(commands, palette, materials, -2.6, 3.0, -8.0, -3.0, 0.004, "#8D939C", 0.86);
    spawn_floor_zone(commands, palette, materials, 3.0, 8.6, -8.0, -3.0, 0.004, "#8E6A49", 0.58);
    spawn_floor_zone(commands, palette, materials, 8.6, 14.0, -8.0, 0.6, 0.004, "#EFE7D8", 0.65);
    spawn_floor_zone(commands, palette, materials, 8.6, 14.0, 0.6, 9.0, 0.004, "#C59F76", 0.6);
    spawn_floor_zone(commands, palette, materials, -8.0, -3.6, -3.0, 0.55, 0.004, "#A3ABB6", 0.55);
    spawn_floor_zone(commands, palette, materials, -8.0, -3.6, 0.55, 5.4, 0.004, "#ECE8E1", 0.5);
    spawn_floor_zone(commands, palette, materials, -8.0, -3.6, 5.4, 9.0, 0.004, "#E3EBEF", 0.45);

    // Carpet tiles under each core pod
    for &[cx, cz] in spec.pods.iter().take(3) {
        spawn_floor_zone(commands, palette, materials, cx - 2.0, cx + 2.0, cz - 1.75, cz + 1.75, 0.009, "#9AA2AE", 0.88);
    }
    spawn_rug(commands, palette, materials, -5.4, 0.015, -5.3, 3.6, 2.9, "#B9A486");
    spawn_rug(commands, palette, materials, 5.75, 0.016, -5.4, 3.4, 2.4, "#9E4A3F");
    spawn_rug(commands, palette, materials, 11.0, 0.017, 2.55, 3.8, 3.1, "#D8C6A8");

    // Wall accent panels
    let dir_wall = palette.mat(materials, "#6F5440", 0.78, 0.0);
    let lib_wall = palette.mat(materials, "#C9D5B2", 0.85, 0.0);
    let srv_wall = palette.mat(materials, "#38404C", 0.8, 0.0);
    let lob_wall = palette.mat(materials, "#2F4A3D", 0.82, 0.0);
    spawn_box(commands, palette, dir_wall.clone(), Vec3::new(-5.3, 1.375, -7.98), Vec3::new(5.4, 2.75, 0.04));
    spawn_box(commands, palette, dir_wall, Vec3::new(-7.98, 1.375, -5.5), Vec3::new(0.04, 2.75, 5.0));
    spawn_box(commands, palette, lib_wall, Vec3::new(5.8, 1.375, -7.98), Vec3::new(5.6, 2.75, 0.04));
    spawn_box(commands, palette, srv_wall, Vec3::new(-7.98, 2.1, -0.9), Vec3::new(0.04, 4.2, 4.2));
    spawn_box(commands, palette, lob_wall, Vec3::new(-7.98, 2.1, 3.3), Vec3::new(0.04, 4.2, 4.2));

    // Windows
    spawn_wall_window(commands, palette, materials, -5.3, 2.05, -7.98, 2.6, 1.5, 0.0);
    spawn_wall_window(commands, palette, materials, -7.98, 2.05, -5.9, 1.6, 1.5, PI * 0.5);
    spawn_wall_window(commands, palette, materials, 7.7, 2.1, -7.98, 1.3, 1.7, 0.0);
    spawn_wall_window(commands, palette, materials, 12.55, 2.35, -7.98, 0.95, 1.0, 0.0);
    spawn_wall_window(commands, palette, materials, -7.98, 2.6, 7.4, 1.4, 0.7, PI * 0.5);

    // Glass & low partitions
    spawn_partition_segment(commands, palette, materials, -8.0, -3.0, -3.5, -3.0, 2.6, true, "");
    spawn_partition_segment(commands, palette, materials, -2.6, -8.0, -2.6, -3.0, 2.6, true, "");
    spawn_partition_segment(commands, palette, materials, -2.6, -3.0, 1.9, -3.0, 2.6, true, "");
    spawn_partition_segment(commands, palette, materials, 3.0, -8.0, 3.0, -3.0, 2.6, true, "");
    spawn_partition_segment(commands, palette, materials, 4.3, -3.0, 8.6, -3.0, 1.15, false, "#E6E0D4");
    spawn_partition_segment(commands, palette, materials, 8.6, -8.0, 8.6, -3.0, 1.15, false, "#E6E0D4");
    spawn_partition_segment(commands, palette, materials, -3.6, -3.0, -3.6, -2.0, 2.6, true, "");
    spawn_partition_segment(commands, palette, materials, -3.6, -1.1, -3.6, 0.55, 2.6, true, "");
    spawn_partition_segment(commands, palette, materials, -8.0, 0.55, -3.6, 0.55, 2.6, true, "");
    spawn_partition_segment(commands, palette, materials, -8.0, 5.4, -5.4, 5.4, 1.3, false, "#DDE8EE");
    spawn_partition_segment(commands, palette, materials, -3.6, 5.4, -3.6, 9.0, 1.3, false, "#DDE8EE");
    spawn_partition_segment(commands, palette, materials, -3.6, 0.6, -3.6, 2.7, 1.15, false, "#E6E0D4");
    spawn_partition_segment(commands, palette, materials, -3.6, 3.9, -3.6, 5.33, 1.15, false, "#E6E0D4");
    spawn_partition_segment(commands, palette, materials, 8.6, -1.6, 8.6, 4.2, 1.15, false, "#E6E0D4");
    spawn_partition_segment(commands, palette, materials, 8.6, 6.4, 8.6, 9.0, 1.15, false, "#E6E0D4");
    spawn_partition_segment(commands, palette, materials, 9.9, 0.6, 14.0, 0.6, 1.15, false, "#E6E0D4");

    // Director's walnut desk & credenza + Meeting room Sprint Review screen + Pantry fridge/sink
    let walnut = palette.mat(materials, "#4A3628", 0.5, 0.0);
    spawn_box(commands, palette, walnut.clone(), Vec3::new(-5.3, 0.37, -7.7), Vec3::new(3.3, 0.74, 0.5));
    spawn_box(commands, palette, walnut, Vec3::new(-5.4, 0.76, -5.8), Vec3::new(2.1, 0.06, 0.95));
    let exec_m = palette.mat(materials, "#22262C", 0.45, 0.0);
    spawn_box(commands, palette, exec_m, Vec3::new(-5.4, 0.6, -6.75), Vec3::new(0.62, 0.85, 0.56));

    // Sprint Review slide on meeting room back wall
    let mut slide_cv = Canvas2d::new(512, 288);
    slide_cv.fill(hex("#F7F5F0"));
    slide_cv.fill_rect(0.0, 0.0, 12.0, 288.0, hex("#D97757"));
    fonts.draw_text(&mut slide_cv, "Sprint review", 36.0, 26.0, 34.0, hex("#1E232B"), TextAlign::Left, None, true, false);
    fonts.draw_text(&mut slide_cv, "Agents Station · Velocity", 36.0, 68.0, 18.0, hex("#7A8494"), TextAlign::Left, None, false, false);
    for (i, &v) in [0.42_f32, 0.58, 0.51, 0.74, 0.66, 0.88].iter().enumerate() {
        let bh = v * 150.0;
        let col = if i == 5 { hex("#D97757") } else { hex("#3F7CC4") };
        slide_cv.fill_rect(48.0 + i as f32 * 70.0, 256.0 - bh, 42.0, bh, col);
    }
    let slide_mat = palette.textured_unlit(images, materials, slide_cv.into_surface());
    let bezel = palette.mat(materials, "#15181E", 0.4, 0.1);
    spawn_box(commands, palette, bezel, Vec3::new(0.2, 1.75, -7.95), Vec3::new(2.7, 1.55, 0.07));
    spawn_quad(commands, palette, slide_mat, Vec3::new(0.2, 1.75, -7.9), Vec2::new(2.56, 1.42), Quat::IDENTITY);

    // Pantry fridge + sink counter + cupboards
    let fridge = palette.mat(materials, "#D5DADF", 0.3, 0.45);
    let cab = palette.mat(materials, "#E6E1D8", 0.7, 0.0);
    let top_c = palette.mat(materials, "#3B3F46", 0.35, 0.1);
    spawn_box(commands, palette, fridge, Vec3::new(13.52, 1.0, -7.6), Vec3::new(0.86, 2.0, 0.74));
    spawn_box(commands, palette, cab.clone(), Vec3::new(12.55, 0.46, -7.6), Vec3::new(0.95, 0.92, 0.75));
    spawn_box(commands, palette, top_c, Vec3::new(12.55, 0.955, -7.6), Vec3::new(1.0, 0.07, 0.84));
    spawn_box(commands, palette, cab, Vec3::new(10.6, 2.1, -7.8), Vec3::new(2.9, 0.72, 0.36));

    // --- HQ DEPARTMENT WING (z: 9..26) ---
    if spec.layout.is_hq() {
        let len = 17.0;
        let mid_z = 17.65;
        spawn_box(commands, palette, slab, Vec3::new(3.0, -0.25, mid_z), Vec3::new(22.6, 0.5, len));
        spawn_box(commands, palette, wall, Vec3::new(-8.15, 2.1, mid_z), Vec3::new(0.3, 4.2, len));
        spawn_box(commands, palette, cap, Vec3::new(-8.15, 4.25, mid_z), Vec3::new(0.42, 0.14, len));
        spawn_box(commands, palette, skirt, Vec3::new(-7.97, 0.09, mid_z), Vec3::new(0.06, 0.18, len));
        for z in [12.4_f32, 16.8, 20.6, 24.4] {
            spawn_wall_window(commands, palette, materials, -7.98, 2.1, z, 1.5, 1.5, PI * 0.5);
        }

        // Corridors A & B
        spawn_floor_zone(commands, palette, materials, -8.0, 14.0, 9.0, 10.8, 0.004, "#D7D2CA", 0.72);
        spawn_floor_zone(commands, palette, materials, 2.3, 4.3, 10.8, 26.0, 0.004, "#D7D2CA", 0.72);

        let carpets = ["#5E6B7D", "#66735C", "#7B6A5E", "#655F7A"];
        for (i, d) in spec.depts.iter().enumerate() {
            spawn_floor_zone(commands, palette, materials, d.x0, d.x1, d.z0, d.z1, 0.005, carpets[i % 4], 0.88);
            spawn_pot_plant(commands, palette, materials, d.x0 + 0.55, 0.0, d.z0 + 0.5, 0.9, false);
            spawn_pot_plant(commands, palette, materials, d.x1 - 0.55, 0.0, d.z1 - 0.55, 0.9, false);
        }

        // Glass walls along corridors + mid split half walls
        spawn_partition_segment(commands, palette, materials, -8.0, 9.0, -3.6, 9.0, 1.15, false, "#E6E0D4");
        spawn_partition_segment(commands, palette, materials, 8.6, 9.0, 14.0, 9.0, 1.15, false, "#E6E0D4");
        spawn_partition_segment(commands, palette, materials, -8.0, 10.8, -3.2, 10.8, 2.6, true, "");
        spawn_partition_segment(commands, palette, materials, -1.6, 10.8, 2.3, 10.8, 2.6, true, "");
        spawn_partition_segment(commands, palette, materials, 4.3, 10.8, 8.4, 10.8, 2.6, true, "");
        spawn_partition_segment(commands, palette, materials, 9.9, 10.8, 14.0, 10.8, 2.6, true, "");
        spawn_partition_segment(commands, palette, materials, 2.3, 10.8, 2.3, 21.4, 2.6, true, "");
        spawn_partition_segment(commands, palette, materials, 2.3, 23.0, 2.3, 26.0, 2.6, true, "");
        spawn_partition_segment(commands, palette, materials, 4.3, 10.8, 4.3, 21.4, 2.6, true, "");
        spawn_partition_segment(commands, palette, materials, 4.3, 23.0, 4.3, 26.0, 2.6, true, "");
        spawn_partition_segment(commands, palette, materials, -8.0, 18.4, 2.3, 18.4, 1.15, false, "#E6E0D4");
        spawn_partition_segment(commands, palette, materials, 4.3, 18.4, 14.0, 18.4, 1.15, false, "#E6E0D4");
    }
}
