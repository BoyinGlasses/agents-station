use crate::backdrops::BackdropKind;
use crate::hq_wing;
use crate::layout::LayoutSpec;
use crate::palette::{
    WorldEntity, WorldPalette, spawn_box, spawn_box_rot, spawn_cone_rot, spawn_cyl, spawn_cyl_rot,
    spawn_floor_quad, spawn_quad, spawn_sphere,
};
use crate::pro_floor;
use bevy::prelude::*;
use station_canvas2d::FontEngine;
use station_canvas2d::office_surfaces::{
    WindowViewKind, draw_blind_fabric, draw_checker_tile, draw_framed_photo, draw_plank_tile,
    draw_server_grid_tile, draw_speckle_rug_tile, draw_subway_tile, draw_wainscot_slats_tile,
    draw_window_painting,
};
use std::f32::consts::{FRAC_PI_2, PI};

/// Animated roller blind mounted at the top of every outer wall window (`windows.js:72-76`).
#[derive(Component, Debug, Clone, Copy)]
pub struct WindowBlind {
    pub cloth_entity: Entity,
    pub full_height: f32,
    pub lowered: f32,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PlantKind {
    Monstera,
    Snake,
    Bush,
    Tree,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TrinketKind {
    Books,
    Frame,
    Cactus,
}

/// Shared compact repeating tile handles (`< 600 KB` total VRAM) & shared architectural textures
/// reused across Cozy, Pro, and HQ rooms with zero duplicate allocations.
pub struct SharedSurfaceTiles {
    pub plank: Handle<Image>,
    pub checker: Handle<Image>,
    pub subway: Handle<Image>,
    pub server_grid: Handle<Image>,
    pub speckle: Handle<Image>,
    pub wainscot: Handle<Image>,
    pub window_mat: Handle<StandardMaterial>,
    pub blind_mat: Handle<StandardMaterial>,
    pub photo_mat: Handle<StandardMaterial>,
}

impl SharedSurfaceTiles {
    pub fn build(
        palette: &mut WorldPalette,
        materials: &mut Assets<StandardMaterial>,
        images: &mut Assets<Image>,
        backdrop: BackdropKind,
        night: bool,
    ) -> Self {
        let view_kind = match backdrop {
            BackdropKind::City => WindowViewKind::City,
            BackdropKind::Countryside => WindowViewKind::Country,
            BackdropKind::Beach => WindowViewKind::Beach,
        };
        let plank = palette.upload_repeating_surface(images, draw_plank_tile());
        let checker = palette.upload_repeating_surface(images, draw_checker_tile());
        let subway = palette.upload_repeating_surface(images, draw_subway_tile());
        let server_grid = palette.upload_repeating_surface(images, draw_server_grid_tile());
        let speckle = palette.upload_repeating_surface(images, draw_speckle_rug_tile());
        let wainscot = palette.upload_repeating_surface(images, draw_wainscot_slats_tile());
        let window_mat =
            palette.textured_unlit(images, materials, draw_window_painting(view_kind, night));
        let blind_mat =
            palette.textured_lit_double_sided(images, materials, draw_blind_fabric(), 0.98);
        let photo_mat = palette.textured_unlit(images, materials, draw_framed_photo());
        Self {
            plank,
            checker,
            subway,
            server_grid,
            speckle,
            wainscot,
            window_mat,
            blind_mat,
            photo_mat,
        }
    }
}

/// Exact Oracle `terrace.js:33-56` typed potted plant (`monstera`, `snake`, `bush`, `tree`).
#[allow(clippy::too_many_arguments)]
pub fn spawn_typed_plant(
    commands: &mut Commands,
    palette: &mut WorldPalette,
    materials: &mut Assets<StandardMaterial>,
    x: f32,
    y: f32,
    z: f32,
    s: f32,
    kind: PlantKind,
) {
    let pot_m = palette.mat(materials, "#E4DDD0", 0.45, 0.0);
    let rim_m = palette.mat(materials, "#D3CBBC", 0.48, 0.0);
    let soil_m = palette.matte(materials, "#4A3527");
    let trunk_m = palette.mat(materials, "#6E4B32", 0.6, 0.0);
    let leaves = [
        palette.mat(materials, "#4E8A3A", 0.5, 0.0),
        palette.mat(materials, "#62A04A", 0.5, 0.0),
        palette.mat(materials, "#3C7432", 0.5, 0.0),
        palette.mat(materials, "#78B256", 0.5, 0.0),
    ];

    spawn_cyl(commands, palette, pot_m, Vec3::new(x, y + 0.2 * s, z), 0.42 * s, 0.4 * s);
    spawn_cyl(commands, palette, rim_m, Vec3::new(x, y + 0.4 * s, z), 0.5 * s, 0.04 * s);
    spawn_cyl(commands, palette, soil_m, Vec3::new(x, y + 0.415 * s, z), 0.42 * s, 0.02 * s);

    match kind {
        PlantKind::Monstera => {
            for i in 0..8 {
                let a = (i as f32 / 8.0) * PI * 2.0 + ((i % 3) as f32 - 1.0) * 0.14;
                let h = (0.58 + (i % 3) as f32 * 0.16) * s;
                let r = (0.1 + (h / s) * 0.32) * s;
                let sx = a.sin();
                let cz = a.cos();
                let stem_rot = Quat::from_euler(EulerRot::XYZ, cz * 0.38, 0.0, -sx * 0.38);
                spawn_cyl_rot(
                    commands,
                    palette,
                    leaves[2].clone(),
                    Vec3::new(x + sx * r * 0.45, y + 0.42 * s + h * 0.5, z + cz * r * 0.45),
                    0.028 * s,
                    h,
                    stem_rot,
                );
                let leaf_rot = Quat::from_euler(EulerRot::YXZ, a, cz * 0.65, -sx * 0.14);
                commands.spawn((
                    WorldEntity,
                    Mesh3d(palette.unit_sphere.clone()),
                    MeshMaterial3d(leaves[i % 2].clone()),
                    Transform::from_translation(Vec3::new(x + sx * r, y + 0.42 * s + h, z + cz * r))
                        .with_rotation(leaf_rot)
                        .with_scale(Vec3::new(0.42 * s, 0.05 * s, 0.34 * s)),
                ));
            }
        }
        PlantKind::Snake => {
            for i in 0..9 {
                let a = (i as f32 / 9.0) * PI * 2.0 + 0.25;
                let rad = (0.04 + (i % 3) as f32 * 0.038) * s;
                let h = (0.56 + (i % 4) as f32 * 0.13) * s;
                let tilt = if i % 2 == 0 { 0.12 } else { -0.1 };
                let rot = Quat::from_euler(EulerRot::YXZ, a, tilt, -tilt * 0.6);
                spawn_cone_rot(
                    commands,
                    palette,
                    leaves[i % 3].clone(),
                    Vec3::new(x + a.sin() * rad, y + 0.42 * s + h * 0.5, z + a.cos() * rad),
                    Vec3::new(0.11 * s, h, 0.04 * s),
                    rot,
                );
            }
        }
        PlantKind::Tree => {
            spawn_cyl(commands, palette, trunk_m, Vec3::new(x, y + 1.05 * s, z), 0.12 * s, 1.3 * s);
            let offsets = [
                (0.0_f32, 1.92_f32, 0.0_f32, 0.84_f32),
                (0.24, 1.72, 0.14, 0.72),
                (-0.22, 1.76, 0.18, 0.70),
                (0.12, 1.68, -0.24, 0.68),
                (-0.2, 1.64, -0.18, 0.66),
            ];
            for (i, (ox, oy, oz, rad)) in offsets.iter().enumerate() {
                spawn_sphere(
                    commands,
                    palette,
                    leaves[i % 4].clone(),
                    Vec3::new(x + ox * s, y + oy * s, z + oz * s),
                    Vec3::new(rad * s, rad * 0.85 * s, rad * s),
                );
            }
        }
        PlantKind::Bush => {
            for i in 0..6 {
                let a = (i as f32 / 6.0) * PI * 2.0;
                let py = 0.62 + (i % 2) as f32 * 0.12;
                spawn_sphere(
                    commands,
                    palette,
                    leaves[i % 4].clone(),
                    Vec3::new(x + a.sin() * 0.15 * s, y + py * s, z + a.cos() * 0.15 * s),
                    Vec3::new(0.38 * s, 0.44 * s, 0.38 * s),
                );
            }
            spawn_sphere(
                commands,
                palette,
                leaves[1].clone(),
                Vec3::new(x, y + 0.86 * s, z),
                Vec3::splat(0.42 * s),
            );
        }
    }
}

/// Backward-compatible plant wrapper used by `furniture.rs`.
#[allow(clippy::too_many_arguments)]
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
    spawn_typed_plant(
        commands,
        palette,
        materials,
        x,
        y,
        z,
        scale,
        if tall_tree { PlantKind::Tree } else { PlantKind::Bush },
    );
}

/// Exact Oracle `trinkets.js` desk / credenza trinket (`books`, `frame`, `cactus`).
#[allow(clippy::too_many_arguments)]
pub fn spawn_trinket(
    commands: &mut Commands,
    palette: &mut WorldPalette,
    materials: &mut Assets<StandardMaterial>,
    tiles: &SharedSurfaceTiles,
    kind: TrinketKind,
    x: f32,
    y: f32,
    z: f32,
    ry: f32,
) {
    let rot = Quat::from_rotation_y(ry);
    let at = |lx: f32, ly: f32, lz: f32| Vec3::new(x, y, z) + rot * Vec3::new(lx, ly, lz);
    match kind {
        TrinketKind::Books => {
            let cols = ["#C8553D", "#3E8EDE", "#2A9D8F"];
            for (i, col) in cols.iter().enumerate() {
                let bm = palette.mat(materials, col, 0.45, 0.0);
                let yaw = ((i as f32) - 1.0) * 0.14;
                spawn_box_rot(
                    commands,
                    palette,
                    bm,
                    at((i as f32 - 1.0) * 0.008, 0.016 + i as f32 * 0.031, 0.0),
                    Vec3::new(0.17 - i as f32 * 0.015, 0.03, 0.12),
                    rot * Quat::from_rotation_y(yaw),
                );
            }
        }
        TrinketKind::Frame => {
            let fm = palette.mat(materials, "#2B2F36", 0.45, 0.0);
            let tilt = rot * Quat::from_rotation_x(-0.22);
            spawn_box_rot(commands, palette, fm, at(0.0, 0.065, 0.0), Vec3::new(0.15, 0.115, 0.016), tilt);
            spawn_quad(commands, palette, tiles.photo_mat.clone(), at(0.0, 0.066, 0.01), Vec2::new(0.12, 0.088), tilt);
        }
        TrinketKind::Cactus => {
            let pot = palette.mat(materials, "#C96F4A", 0.55, 0.0);
            let grn = palette.mat(materials, "#4F8A4A", 0.5, 0.0);
            let flw = palette.mat(materials, "#FF6F91", 0.45, 0.0);
            spawn_cyl(commands, palette, pot, at(0.0, 0.035, 0.0), 0.09, 0.07);
            spawn_cyl(commands, palette, grn.clone(), at(0.0, 0.13, 0.0), 0.06, 0.12);
            spawn_cyl_rot(commands, palette, grn, at(0.035, 0.13, 0.0), 0.032, 0.06, rot * Quat::from_rotation_z(-0.9));
            spawn_sphere(commands, palette, flw, at(0.0, 0.2, 0.0), Vec3::splat(0.028));
        }
    }
}

/// Exact Oracle `room-pieces.js:39-50` upholstered sofa with seat cushions, throw pillows, and legs.
#[allow(clippy::too_many_arguments)]
pub fn spawn_sofa(
    commands: &mut Commands,
    palette: &mut WorldPalette,
    materials: &mut Assets<StandardMaterial>,
    x: f32,
    z: f32,
    w: f32,
    ry: f32,
    color: &str,
    pillows: [&str; 2],
) {
    let rot = Quat::from_rotation_y(ry);
    let at = |lx: f32, ly: f32, lz: f32| Vec3::new(x, 0.0, z) + rot * Vec3::new(lx, ly, lz);
    let fabric = palette.mat(materials, color, 0.95, 0.0);
    let cushion = palette.mat(materials, lighten_hex(color, 0.14).as_str(), 0.95, 0.0);
    let leg_m = palette.mat(materials, "#6E4B32", 0.6, 0.0);
    let p0 = palette.mat(materials, pillows[0], 0.9, 0.0);
    let p1 = palette.mat(materials, pillows[1], 0.9, 0.0);

    spawn_box_rot(commands, palette, fabric.clone(), at(0.0, 0.33, 0.0), Vec3::new(w, 0.3, 0.82), rot);
    spawn_box_rot(commands, palette, fabric.clone(), at(0.0, 0.66, -0.3), Vec3::new(w, 0.66, 0.24), rot);
    for sx in [-1.0_f32, 1.0] {
        spawn_box_rot(commands, palette, fabric.clone(), at(sx * (w * 0.5 - 0.12), 0.5, 0.0), Vec3::new(0.24, 0.5, 0.82), rot);
        spawn_box_rot(
            commands,
            palette,
            cushion.clone(),
            at(sx * (w - 0.56) * 0.25, 0.53, 0.06),
            Vec3::new((w - 0.56) * 0.5, 0.12, 0.6),
            rot,
        );
    }
    spawn_box_rot(commands, palette, p0, at(-w * 0.5 + 0.5, 0.75, -0.12), Vec3::new(0.42, 0.36, 0.14), rot * Quat::from_rotation_z(0.15));
    spawn_box_rot(commands, palette, p1, at(w * 0.5 - 0.5, 0.74, -0.12), Vec3::new(0.38, 0.34, 0.14), rot * Quat::from_rotation_z(-0.12));
    for sx in [-1.0_f32, 1.0] {
        for sz in [-1.0_f32, 1.0] {
            spawn_cyl(commands, palette, leg_m.clone(), at(sx * (w * 0.5 - 0.15), 0.09, sz * 0.3), 0.07, 0.18);
        }
    }
}

pub fn lighten_hex(hex: &str, amount: f32) -> String {
    let (r, g, b) = crate::palette::parse_hex_rgb(hex);
    let lift = |v: u8| ((v as f32 + (255.0 - v as f32) * amount).clamp(0.0, 255.0).round()) as u8;
    format!("#{:02X}{:02X}{:02X}", lift(r), lift(g), lift(b))
}

/// Spawns a tiled horizontal floor zone using a shared repeating `Handle<Image>`.
#[allow(clippy::too_many_arguments)]
pub fn spawn_floor_zone_tiled(
    commands: &mut Commands,
    palette: &mut WorldPalette,
    materials: &mut Assets<StandardMaterial>,
    tex: Handle<Image>,
    x0: f32,
    x1: f32,
    z0: f32,
    z1: f32,
    y: f32,
    tile_meters: f32,
    tint_hex: &str,
    rough: f32,
) {
    let w = (x1 - x0).abs();
    let d = (z1 - z0).abs();
    let repeat = Vec2::new((w / tile_meters).max(0.5), (d / tile_meters).max(0.5));
    let mat = palette.tiled_mat_from_handle(materials, tex, repeat, tint_hex, rough);
    spawn_floor_quad(
        commands,
        palette,
        mat,
        Vec3::new((x0 + x1) * 0.5, y, (z0 + z1) * 0.5),
        Vec2::new(w, d),
    );
}

#[allow(clippy::too_many_arguments)]
pub fn spawn_rug_tiled(
    commands: &mut Commands,
    palette: &mut WorldPalette,
    materials: &mut Assets<StandardMaterial>,
    speckle: Handle<Image>,
    x: f32,
    y: f32,
    z: f32,
    w: f32,
    d: f32,
    color: &str,
) {
    let border_m = palette.mat(materials, color, 0.94, 0.0);
    spawn_box(commands, palette, border_m, Vec3::new(x, y - 0.008, z), Vec3::new(w, 0.016, d));
    let top_m = palette.tiled_mat_from_handle(
        materials,
        speckle,
        Vec2::new((w / 0.85).max(1.0), (d / 0.85).max(1.0)),
        color,
        0.92,
    );
    spawn_floor_quad(commands, palette, top_m, Vec3::new(x, y + 0.002, z), Vec2::new(w - 0.04, d - 0.04));
}

/// Exact Oracle `windows.js:64-78` outer wall window with mullions, sill, backdrop painting, and animated `WindowBlind`.
#[allow(clippy::too_many_arguments)]
pub fn spawn_wall_window(
    commands: &mut Commands,
    palette: &mut WorldPalette,
    materials: &mut Assets<StandardMaterial>,
    tiles: &SharedSurfaceTiles,
    x: f32,
    y: f32,
    z: f32,
    w: f32,
    h: f32,
    ry: f32,
    panes: usize,
) {
    let frame_m = palette.mat(materials, "#2F343E", 0.45, 0.3);
    let sill_m = palette.mat(materials, "#7A5A3E", 0.55, 0.0);
    let roller_m = palette.mat(materials, "#D9CDB4", 0.75, 0.0);
    let rot = Quat::from_rotation_y(ry);
    let at = |lx: f32, ly: f32, lz: f32| Vec3::new(x, y, z) + rot * Vec3::new(lx, ly, lz);

    spawn_box_rot(commands, palette, frame_m.clone(), at(0.0, 0.0, 0.0), Vec3::new(w + 0.2, h + 0.2, 0.12), rot);
    spawn_quad(commands, palette, tiles.window_mat.clone(), at(0.0, 0.0, 0.065), Vec2::new(w, h), rot);

    let p = panes.max(1);
    for i in 1..p {
        let mx = -w * 0.5 + (i as f32 * w) / p as f32;
        spawn_box_rot(commands, palette, frame_m.clone(), at(mx, 0.0, 0.09), Vec3::new(0.06, h, 0.06), rot);
    }
    spawn_box_rot(commands, palette, frame_m, at(0.0, h * 0.18, 0.09), Vec3::new(w, 0.06, 0.06), rot);
    spawn_box_rot(commands, palette, sill_m, at(0.0, -h * 0.5 - 0.12, 0.14), Vec3::new(w + 0.4, 0.1, 0.36), rot);

    // Roller blind housing + cloth entity (rolled up at top until nap lowers it)
    let blind_y = h * 0.5 + 0.06;
    let cloth_h = h + 0.1;
    let cloth_ent = spawn_quad(
        commands,
        palette,
        tiles.blind_mat.clone(),
        at(0.0, blind_y - 0.03, 0.16),
        Vec2::new(w + 0.08, 0.04),
        rot,
    );
    let valance = spawn_box_rot(
        commands,
        palette,
        roller_m,
        at(0.0, blind_y, 0.15),
        Vec3::new(w + 0.16, 0.09, 0.09),
        rot,
    );
    commands.entity(valance).insert(WindowBlind {
        cloth_entity: cloth_ent,
        full_height: cloth_h,
        lowered: 0.0,
    });
}

fn split_wall_segments(len: f32, gaps: &[(f32, f32)]) -> Vec<(f32, f32)> {
    let mut segs = Vec::new();
    let mut t = 0.0_f32;
    for &(g0, g1) in gaps {
        if g0 - t > 0.04 {
            segs.push((t, g0));
        }
        t = g1;
    }
    if len - t > 0.04 {
        segs.push((t, len));
    }
    segs
}

/// Exact Oracle `pro/pieces.js:63-74` framed glass partition with frosted center privacy stripe,
/// evenly spaced mullions, and doorway headers.
#[allow(clippy::too_many_arguments)]
pub fn spawn_glass_wall(
    commands: &mut Commands,
    palette: &mut WorldPalette,
    materials: &mut Assets<StandardMaterial>,
    ax: f32,
    az: f32,
    bx: f32,
    bz: f32,
    gaps: &[(f32, f32)],
    h: f32,
) {
    let dx = bx - ax;
    let dz = bz - az;
    let len = dx.hypot(dz);
    if len < 0.05 {
        return;
    }
    let ang = -dz.atan2(dx);
    let rot = Quat::from_rotation_y(ang);
    let at = |t: f32, y: f32| Vec3::new(ax, 0.0, az) + rot * Vec3::new(t, y, 0.0);

    let glass = palette.glass(materials, "#BFD8EE", 0.20);
    let frost = palette.glass(materials, "#FFFFFF", 0.48);
    let metal = palette.mat(materials, "#2B2F36", 0.35, 0.65);

    for (t0, t1) in split_wall_segments(len, gaps) {
        let w = t1 - t0;
        let cx = (t0 + t1) * 0.5;
        spawn_box_rot(commands, palette, glass.clone(), at(cx, h * 0.5), Vec3::new(w, h - 0.1, 0.03), rot);
        spawn_box_rot(commands, palette, frost.clone(), at(cx, 1.15), Vec3::new(w, 0.26, 0.036), rot);
        spawn_box_rot(commands, palette, metal.clone(), at(cx, 0.04), Vec3::new(w, 0.08, 0.08), rot);
        spawn_box_rot(commands, palette, metal.clone(), at(cx, h), Vec3::new(w, 0.06, 0.08), rot);
        let n = ((w / 1.25).round() as usize).max(1);
        for i in 0..=n {
            let tx = t0 + (i as f32 * w) / n as f32;
            spawn_box_rot(commands, palette, metal.clone(), at(tx, h * 0.5), Vec3::new(0.05, h, 0.07), rot);
        }
    }
    for &(g0, g1) in gaps {
        spawn_box_rot(
            commands,
            palette,
            metal.clone(),
            at((g0 + g1) * 0.5, h - 0.02),
            Vec3::new(g1 - g0 + 0.05, 0.12, 0.09),
            rot,
        );
    }
}

/// Exact Oracle `pro/pieces.js:79-91` architect cutaway partition (`1.3m` high, dark cut cap,
/// skirting, and doorway jambs + threshold).
#[allow(clippy::too_many_arguments)]
pub fn spawn_low_wall(
    commands: &mut Commands,
    palette: &mut WorldPalette,
    materials: &mut Assets<StandardMaterial>,
    ax: f32,
    az: f32,
    bx: f32,
    bz: f32,
    gaps: &[(f32, f32)],
    h: f32,
    wall_hex: &str,
) {
    let dx = bx - ax;
    let dz = bz - az;
    let len = dx.hypot(dz);
    if len < 0.05 {
        return;
    }
    let ang = -dz.atan2(dx);
    let rot = Quat::from_rotation_y(ang);
    let at = |t: f32, y: f32| Vec3::new(ax, 0.0, az) + rot * Vec3::new(t, y, 0.0);

    let wm = palette.mat(materials, wall_hex, 0.9, 0.0);
    let cut = palette.mat(materials, "#3B4049", 0.7, 0.0);
    let skirt = palette.mat(materials, "#8D7F70", 0.7, 0.0);
    let jamb = palette.mat(materials, "#6E6052", 0.6, 0.0);

    for (t0, t1) in split_wall_segments(len, gaps) {
        let w = t1 - t0;
        let cx = (t0 + t1) * 0.5;
        spawn_box_rot(commands, palette, wm.clone(), at(cx, h * 0.5), Vec3::new(w, h, 0.14), rot);
        spawn_box_rot(commands, palette, cut.clone(), at(cx, h + 0.012), Vec3::new(w + 0.005, 0.03, 0.145), rot);
        spawn_box_rot(commands, palette, skirt.clone(), at(cx, 0.05), Vec3::new(w, 0.1, 0.17), rot);
    }
    for &(g0, g1) in gaps {
        for t in [g0, g1] {
            spawn_box_rot(commands, palette, jamb.clone(), at(t, (h + 0.02) * 0.5), Vec3::new(0.07, h + 0.02, 0.2), rot);
        }
        spawn_box_rot(commands, palette, skirt.clone(), at((g0 + g1) * 0.5, 0.007), Vec3::new(g1 - g0, 0.014, 0.18), rot);
    }
}

fn spawn_railing(
    commands: &mut Commands,
    palette: &mut WorldPalette,
    materials: &mut Assets<StandardMaterial>,
    x1: f32,
    z1: f32,
    x2: f32,
    z2: f32,
) {
    let dx = x2 - x1;
    let dz = z2 - z1;
    let len = dx.hypot(dz);
    let ang = -dz.atan2(dx);
    let rot = Quat::from_rotation_y(ang);
    let mid = Vec3::new((x1 + x2) * 0.5, -0.27, (z1 + z2) * 0.5);
    let at = |lx: f32, ly: f32| mid + rot * Vec3::new(lx, ly, 0.0);

    let metal_m = palette.mat(materials, "#2B2F36", 0.35, 0.65);
    let rail_g = palette.glass(materials, "#B5D3F0", 0.16);

    spawn_box_rot(commands, palette, rail_g, at(0.0, 0.52), Vec3::new(len, 0.9, 0.03), rot);
    spawn_box_rot(commands, palette, metal_m.clone(), at(0.0, 0.99), Vec3::new(len, 0.06, 0.09), rot);

    let posts = ((len / 2.2).round() as usize).max(1);
    for i in 0..=posts {
        let px = -len * 0.5 + (i as f32 * len) / posts as f32;
        spawn_box_rot(commands, palette, metal_m.clone(), at(px, 0.5), Vec3::new(0.07, 1.02, 0.07), rot);
    }
}

fn spawn_terrace(
    commands: &mut Commands,
    palette: &mut WorldPalette,
    materials: &mut Assets<StandardMaterial>,
    tiles: &SharedSurfaceTiles,
    spec: &LayoutSpec,
) {
    let pro = spec.layout.is_pro();
    let hq = spec.layout.is_hq();
    let slab_m = palette.mat(materials, "#2B3038", 0.85, 0.05);
    let metal_m = palette.mat(materials, "#2B2F36", 0.35, 0.65);
    let bulb_m = palette.emissive(materials, "#EDE6D6", 0.85);

    let (pos, sz, repeat) = if hq {
        (Vec3::new(3.4, -0.42, 10.1), Vec3::new(24.6, 0.3, 37.2), Vec2::new(14.0, 22.0))
    } else if pro {
        (Vec3::new(3.4, -0.42, 1.6), Vec3::new(24.6, 0.3, 20.2), Vec2::new(14.0, 12.0))
    } else {
        (Vec3::new(1.6, -0.42, 2.0), Vec3::new(21.0, 0.3, 17.4), Vec2::new(12.0, 10.0))
    };
    spawn_box(commands, palette, slab_m, pos, sz);
    let stone_top = palette.tiled_mat_from_handle(
        materials,
        tiles.server_grid.clone(),
        repeat,
        "#484E58",
        0.85,
    );
    spawn_floor_quad(
        commands,
        palette,
        stone_top,
        Vec3::new(pos.x, -0.268, pos.z),
        Vec2::new(sz.x, sz.z),
    );

    let fz = if hq { 28.5 } else if pro { 11.5 } else { 10.5 };
    if pro {
        spawn_railing(commands, palette, materials, 15.5, -8.4, 15.5, fz);
        spawn_railing(commands, palette, materials, -8.8, fz, 15.5, fz);
    } else {
        spawn_railing(commands, palette, materials, 11.9, -6.8, 11.9, 10.5);
        spawn_railing(commands, palette, materials, -8.8, 10.5, 11.9, 10.5);
    }

    // Bollard lights (`terrace.js:92`)
    let bollards: Vec<(f32, f32)> = if pro {
        let mut b = vec![
            (15.0, -5.5),
            (15.0, 0.5),
            (15.0, 6.5),
            (-5.0, fz - 0.5),
            (1.0, fz - 0.5),
            (8.5, fz - 0.5),
        ];
        if hq {
            b.extend_from_slice(&[(15.0, 12.5), (15.0, 18.5), (15.0, 24.5)]);
        }
        b
    } else {
        vec![
            (11.3, -4.5),
            (11.3, 1.5),
            (11.3, 7.5),
            (-5.0, 9.9),
            (1.0, 9.9),
            (7.0, 9.9),
        ]
    };
    for (bx, bz) in bollards {
        spawn_cyl(commands, palette, metal_m.clone(), Vec3::new(bx, 0.0, bz), 0.2, 0.55);
        spawn_cyl(commands, palette, bulb_m.clone(), Vec3::new(bx, 0.3, bz), 0.17, 0.08);
    }

    // Corner planters with soil + trees/bushes (`terrace.js:57-61, 93-94`)
    let planters: Vec<(f32, f32, PlantKind)> = if pro {
        let mut p = vec![
            (14.9, -7.7, PlantKind::Tree),
            (14.9, 3.4, PlantKind::Bush),
            (-7.9, fz - 0.7, PlantKind::Tree),
            (4.6, fz - 0.7, PlantKind::Bush),
            (14.9, fz - 0.7, PlantKind::Tree),
        ];
        if hq {
            p.extend_from_slice(&[(14.9, 14.6, PlantKind::Bush), (14.9, 21.6, PlantKind::Tree)]);
        }
        p
    } else {
        vec![
            (10.8, -5.6, PlantKind::Tree),
            (10.8, 4.6, PlantKind::Tree),
            (-7.2, 9.4, PlantKind::Tree),
            (4.2, 9.4, PlantKind::Bush),
        ]
    };
    let box_m = palette.mat(materials, "#4A4F57", 0.7, 0.0);
    let soil_m = palette.matte(materials, "#3E2C20");
    for (px, pz, kind) in planters {
        spawn_box(commands, palette, box_m.clone(), Vec3::new(px, -0.02, pz), Vec3::new(1.0, 0.5, 1.0));
        spawn_box(commands, palette, soil_m.clone(), Vec3::new(px, 0.24, pz), Vec3::new(0.9, 0.04, 0.9));
        spawn_typed_plant(commands, palette, materials, px, -0.15, pz, 1.25, kind);
    }

    // Outdoor bistro table + 3 full chairs (`terrace.js:96-106`)
    let (ox, oz) = if hq { (11.6, 27.3) } else if pro { (11.6, 10.3) } else { (9.9, 0.9) };
    let top_m = palette.mat(materials, "#E9E4DA", 0.45, 0.0);
    let chair_m = palette.matte(materials, "#C9A27A");
    spawn_cyl(commands, palette, top_m, Vec3::new(ox, 0.42, oz), 0.9, 0.05);
    spawn_cyl(commands, palette, metal_m.clone(), Vec3::new(ox, 0.06, oz), 0.11, 0.66);
    for a in [0.6_f32, 2.4, 4.2] {
        let cx = ox + a.sin() * 0.85;
        let cz = oz + a.cos() * 0.85;
        let crot = Quat::from_rotation_y(a + PI);
        let cat = |lx: f32, ly: f32, lz: f32| Vec3::new(cx, -0.27, cz) + crot * Vec3::new(lx, ly, lz);
        spawn_box_rot(commands, palette, chair_m.clone(), cat(0.0, 0.48, 0.0), Vec3::new(0.46, 0.06, 0.44), crot);
        spawn_box_rot(commands, palette, chair_m.clone(), cat(0.0, 0.72, -0.2), Vec3::new(0.46, 0.42, 0.05), crot);
        for sx in [-1.0_f32, 1.0] {
            for sz in [-1.0_f32, 1.0] {
                spawn_cyl(commands, palette, metal_m.clone(), cat(sx * 0.19, 0.23, sz * 0.18), 0.04, 0.46);
            }
        }
    }
}

#[allow(clippy::too_many_arguments)]
pub fn spawn_office_shell(
    commands: &mut Commands,
    palette: &mut WorldPalette,
    materials: &mut Assets<StandardMaterial>,
    images: &mut Assets<Image>,
    fonts: &mut FontEngine,
    spec: &LayoutSpec,
    backdrop: BackdropKind,
    night: bool,
) {
    let tiles = SharedSurfaceTiles::build(palette, materials, images, backdrop, night);
    spawn_terrace(commands, palette, materials, &tiles, spec);

    if !spec.layout.is_pro() {
        // --- COZY 16×12m ROOM (`cozy-room.js` + `windows.js`) ---
        let slab = palette.mat(materials, "#7A5A3E", 0.65, 0.0);
        let wall = palette.mat(materials, "#F1E6D2", 0.9, 0.0);
        let trim = palette.mat(materials, "#8C6646", 0.6, 0.0);
        let cap = palette.mat(materials, "#E7DAC3", 0.75, 0.0);

        spawn_box(commands, palette, slab, Vec3::new(0.0, -0.25, 0.0), Vec3::new(16.6, 0.5, 12.6));
        spawn_floor_zone_tiled(
            commands,
            palette,
            materials,
            tiles.plank.clone(),
            -8.0,
            8.0,
            -6.0,
            6.0,
            0.002,
            3.2,
            "#D8AE7F",
            0.55,
        );

        spawn_box(commands, palette, wall.clone(), Vec3::new(0.0, 2.1, -6.15), Vec3::new(16.6, 4.2, 0.3));
        spawn_box(commands, palette, wall, Vec3::new(-8.15, 2.1, 0.0), Vec3::new(0.3, 4.2, 12.6));

        // Slatted wainscoting along both walls (tiled `128×128` beadboard texture!)
        let wain_back = palette.tiled_mat_from_handle(
            materials,
            tiles.wainscot.clone(),
            Vec2::new(16.0, 1.0),
            "#C9D5B2",
            0.85,
        );
        let wain_left = palette.tiled_mat_from_handle(
            materials,
            tiles.wainscot.clone(),
            Vec2::new(12.0, 1.0),
            "#C9D5B2",
            0.85,
        );
        let wain_base = palette.mat(materials, "#C9D5B2", 0.85, 0.0);
        spawn_box(commands, palette, wain_base.clone(), Vec3::new(0.0, 0.63, -5.99), Vec3::new(16.0, 1.25, 0.04));
        spawn_quad(commands, palette, wain_back, Vec3::new(0.0, 0.63, -5.968), Vec2::new(16.0, 1.25), Quat::IDENTITY);
        spawn_box(commands, palette, wain_base, Vec3::new(-7.99, 0.63, 0.0), Vec3::new(0.04, 1.25, 12.0));
        spawn_quad(commands, palette, wain_left, Vec3::new(-7.968, 0.63, 0.0), Vec2::new(12.0, 1.25), Quat::from_rotation_y(FRAC_PI_2));

        // Baseboard + chair rail trim + crown caps (`cozy-room.js:35-40`)
        spawn_box(commands, palette, trim.clone(), Vec3::new(0.0, 0.1, -5.96), Vec3::new(16.0, 0.2, 0.07));
        spawn_box(commands, palette, trim.clone(), Vec3::new(-7.96, 0.1, 0.0), Vec3::new(0.07, 0.2, 12.0));
        spawn_box(commands, palette, trim.clone(), Vec3::new(0.0, 1.28, -5.96), Vec3::new(16.0, 0.06, 0.08));
        spawn_box(commands, palette, trim, Vec3::new(-7.96, 1.28, 0.0), Vec3::new(0.08, 0.06, 12.0));
        spawn_box(commands, palette, cap.clone(), Vec3::new(0.0, 4.25, -6.15), Vec3::new(16.6, 0.14, 0.42));
        spawn_box(commands, palette, cap, Vec3::new(-8.15, 4.25, 0.0), Vec3::new(0.42, 0.14, 12.6));

        // Zone rugs (`cozy-room.js:44-52`)
        spawn_rug_tiled(commands, palette, materials, tiles.speckle.clone(), -5.2, 0.014, -4.25, 4.3, 2.3, "#D9CCE3");
        spawn_rug_tiled(commands, palette, materials, tiles.speckle.clone(), 0.6, 0.015, -4.35, 3.0, 2.0, "#C9DDEC");
        spawn_rug_tiled(commands, palette, materials, tiles.speckle.clone(), -6.85, 0.016, -1.83, 2.0, 2.45, "#CCE3D1");
        spawn_rug_tiled(commands, palette, materials, tiles.speckle.clone(), 0.0, 0.017, 0.35, 6.9, 5.7, "#E6D3AE");
        spawn_rug_tiled(commands, palette, materials, tiles.speckle.clone(), -5.5, 0.018, 4.4, 3.8, 2.3, "#EFC9A2");
        spawn_rug_tiled(commands, palette, materials, tiles.speckle.clone(), -7.45, 0.019, 1.5, 0.9, 1.5, "#8F6240");
        let srv_floor = palette.mat(materials, "#262A31", 0.6, 0.1);
        spawn_box(commands, palette, srv_floor, Vec3::new(5.55, 0.016, -4.85), Vec3::new(3.9, 0.03, 2.5));
        let meet_rug = palette.mat(materials, "#F0CDB6", 0.9, 0.0);
        spawn_cyl(commands, palette, meet_rug, Vec3::new(5.2, 0.016, 2.4), 4.5, 0.025);

        // Panoramic window + 2 sill plants (`windows.js:91-95`)
        spawn_wall_window(commands, palette, materials, &tiles, 0.9, 2.35, -5.98, 4.4, 2.3, 0.0, 3);
        spawn_typed_plant(commands, palette, materials, 1.9, 1.13, -5.82, 0.42, PlantKind::Bush);
        spawn_typed_plant(commands, palette, materials, -0.2, 1.13, -5.82, 0.38, PlantKind::Snake);
        return;
    }

    // --- PRO 22×17m COMPANY FLOOR (`pro/floor.js` & `pro/pieces.js`) ---
    pro_floor::spawn_pro_floor(commands, palette, materials, images, fonts, &tiles, spec);

    // --- HQ DEPARTMENT WING (`pro/wing.js`) ---
    if spec.layout.is_hq() {
        hq_wing::spawn_hq_wing(commands, palette, materials, images, fonts, &tiles, spec);
    }
}
