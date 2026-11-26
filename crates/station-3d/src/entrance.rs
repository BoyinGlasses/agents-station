use crate::layout::LayoutSpec;
use crate::palette::{
    WorldEntity, WorldPalette, spawn_box, spawn_box_rot, spawn_cyl_rot, spawn_quad, spawn_sphere,
};
use bevy::prelude::*;
use station_canvas2d::FontEngine;
use station_canvas2d::office_surfaces::{
    draw_exit_sign, draw_lift_display, draw_stair_door_plate, draw_stair_floor_sign,
};
use std::f32::consts::FRAC_PI_2;

pub const LIFT_FRONT: f32 = -6.95;
pub const STAIR_Z: f32 = 4.91;
pub const STAIR_W: f32 = 0.82;
pub const STAIR_H: f32 = 2.1;

/// Sliding steel elevator doors & LED floor display (`entrance.js:240-275`).
#[derive(Component, Debug, Clone)]
pub struct ElevatorDoors {
    pub left_door: Entity,
    pub right_door: Entity,
    pub display_mat: Handle<StandardMaterial>,
    pub center_z: f32,
    pub open: f32,
}

/// Concrete stairwell behind the fire door with 6 steps, handrail, and EXIT sign (`entrance.js:110-167`).
#[derive(Component, Debug, Clone, Copy)]
pub struct Stairwell {
    pub door_pivot: Entity,
    pub open: f32,
}

/// Exact 1:1 port of `src/office/world/entrance.js`:
/// Cozy wooden entrance door OR Pro/HQ animated elevator shaft + concrete stairwell behind fire door.
pub fn spawn_entrance(
    commands: &mut Commands,
    palette: &mut WorldPalette,
    materials: &mut Assets<StandardMaterial>,
    images: &mut Assets<Image>,
    fonts: &mut FontEngine,
    spec: &LayoutSpec,
) {
    let dz = spec.door_z;
    if !spec.layout.is_pro() {
        let frame = palette.mat(materials, "#4A3426", 0.6, 0.0);
        let recess = palette.mat(materials, "#1E1813", 0.8, 0.0);
        let wood = palette.mat(materials, "#A8784D", 0.55, 0.0);
        let panel = palette.mat(materials, "#956A43", 0.58, 0.0);
        let knob = palette.mat(materials, "#E8C170", 0.3, 0.7);

        spawn_box(commands, palette, frame, Vec3::new(-7.96, 1.33, dz), Vec3::new(0.14, 2.66, 1.76));
        spawn_box(commands, palette, recess, Vec3::new(-7.91, 1.21, dz), Vec3::new(0.06, 2.42, 1.46));
        spawn_box(commands, palette, wood, Vec3::new(-7.845, 1.2, dz - 0.01), Vec3::new(0.07, 2.4, 1.42));
        spawn_box(commands, palette, panel.clone(), Vec3::new(-7.805, 1.75, dz - 0.01), Vec3::new(0.02, 0.7, 1.1));
        spawn_box(commands, palette, panel, Vec3::new(-7.805, 0.65, dz - 0.01), Vec3::new(0.02, 0.9, 1.1));
        spawn_sphere(commands, palette, knob, Vec3::new(-7.77, 1.1, dz + 0.56), Vec3::splat(0.12));
        return;
    }

    // --- PRO / HQ ELEVATOR SHAFT (`entrance.js:240-275`) ---
    let x0 = -8.0_f32;
    let xf = LIFT_FRONT;
    let half = 1.1_f32;
    let h = 3.0_f32;
    let dw = 0.5_f32;
    let dh = 2.15_f32;
    let zc = dz;

    let clad = palette.mat(materials, "#3A3F47", 0.55, 0.1);
    let dark_trim = palette.mat(materials, "#2B2F36", 0.5, 0.25);
    let brass = palette.mat(materials, "#C9A227", 0.32, 0.7);
    let steel = palette.mat(materials, "#BCC4CC", 0.26, 0.78);
    let groove = palette.mat(materials, "#9AA3AD", 0.4, 0.6);
    let car_m = palette.mat(materials, "#E4DCCF", 0.55, 0.0);
    let car_floor = palette.mat(materials, "#55595F", 0.7, 0.0);
    let wood = palette.mat(materials, "#8E6B4E", 0.5, 0.0);
    let car_light = palette.emissive(materials, "#FFF2DB", 1.3);
    let bezel = palette.mat(materials, "#15181E", 0.4, 0.2);
    let btn_off = palette.mat(materials, "#4A4F57", 0.4, 0.2);
    let alarm_gold = palette.mat(materials, "#C8A227", 0.35, 0.6);

    // Shaft outer cladding, cap, brass line, plinth, brass door frame
    for s in [-1.0_f32, 1.0] {
        spawn_box(commands, palette, clad.clone(), Vec3::new((x0 + xf) * 0.5, h * 0.5, zc + s * (half - 0.05)), Vec3::new(xf - x0, h, 0.1));
        spawn_box(commands, palette, clad.clone(), Vec3::new(xf - 0.06, h * 0.5, zc + s * (dw + (half - dw) * 0.5)), Vec3::new(0.12, h, half - dw));
        spawn_box(commands, palette, brass.clone(), Vec3::new(xf + 0.015, dh * 0.5, zc + s * (dw + 0.03)), Vec3::new(0.05, dh + 0.05, 0.06));
    }
    spawn_box(commands, palette, clad, Vec3::new(xf - 0.06, dh + (h - dh) * 0.5, zc), Vec3::new(0.12, h - dh, 2.0 * dw + 0.02));
    spawn_box(commands, palette, dark_trim.clone(), Vec3::new((x0 + xf) * 0.5, h + 0.05, zc), Vec3::new(xf - x0 + 0.06, 0.1, 2.0 * half + 0.06));
    spawn_box(commands, palette, brass.clone(), Vec3::new(xf + 0.025, h + 0.02, zc), Vec3::new(0.03, 0.035, 2.0 * half + 0.07));
    spawn_box(commands, palette, dark_trim.clone(), Vec3::new((x0 + xf) * 0.5, 0.06, zc), Vec3::new(xf - x0 + 0.02, 0.12, 2.0 * half + 0.02));
    spawn_box(commands, palette, brass, Vec3::new(xf + 0.015, dh + 0.03, zc), Vec3::new(0.05, 0.06, 2.0 * dw + 0.12));

    // Car interior (visible when doors open)
    spawn_box(commands, palette, car_floor, Vec3::new((x0 + xf) * 0.5 - 0.06, 0.125, zc), Vec3::new(xf - x0 - 0.14, 0.02, 2.0 * half - 0.22));
    spawn_box(commands, palette, wood, Vec3::new(x0 + 0.03, dh * 0.5, zc), Vec3::new(0.03, dh, 2.0 * half - 0.22));
    for s in [-1.0_f32, 1.0] {
        spawn_box(commands, palette, car_m.clone(), Vec3::new((x0 + xf) * 0.5 - 0.06, dh * 0.5, zc + s * (half - 0.11)), Vec3::new(xf - x0 - 0.16, dh, 0.02));
    }
    spawn_box(commands, palette, steel.clone(), Vec3::new(x0 + 0.1, 0.95, zc), Vec3::new(0.04, 0.04, 1.5));
    spawn_box(commands, palette, car_light, Vec3::new((x0 + xf) * 0.5 - 0.06, dh - 0.02, zc), Vec3::new(xf - x0 - 0.2, 0.03, 2.0 * half - 0.3));

    // Inside operating panel (8 floor buttons + gold alarm)
    let zp = zc - half + 0.135;
    spawn_box(commands, palette, steel.clone(), Vec3::new(xf - 0.32, 1.25, zp), Vec3::new(0.2, 0.86, 0.02));
    for k in 0..8 {
        spawn_box(
            commands,
            palette,
            dark_trim.clone(),
            Vec3::new(xf - 0.36 + (k % 2) as f32 * 0.08, 0.98 + (k / 2) as f32 * 0.09, zp + 0.012),
            Vec3::new(0.035, 0.035, 0.012),
        );
    }
    spawn_box(commands, palette, alarm_gold, Vec3::new(xf - 0.32, 1.56, zp + 0.012), Vec3::new(0.05, 0.05, 0.012));

    // Twin sliding steel doors with vertical seam groove
    let spawn_leaf = |commands: &mut Commands, sign: f32| -> Entity {
        commands
            .spawn((
                WorldEntity,
                Mesh3d(palette.unit_box.clone()),
                MeshMaterial3d(steel.clone()),
                Transform::from_xyz(xf - 0.06, (dh - 0.02) * 0.5, zc + sign * dw * 0.5)
                    .with_scale(Vec3::new(0.03, dh - 0.02, dw + 0.01)),
            ))
            .with_children(|p| {
                p.spawn((
                    WorldEntity,
                    Mesh3d(palette.unit_box.clone()),
                    MeshMaterial3d(groove.clone()),
                    Transform::from_xyz(0.0, 0.0, 0.0).with_scale(Vec3::new(1.16, 0.86, 0.025)),
                ));
            })
            .id()
    };
    let left_door = spawn_leaf(commands, -1.0);
    let right_door = spawn_leaf(commands, 1.0);

    // LED floor display (`▼ 12`) over doors + call button panel beside doors
    let disp_mat = palette.textured_unlit(images, materials, draw_lift_display("▼ 12", fonts));
    let disp_housing = spawn_box(commands, palette, bezel, Vec3::new(xf + 0.02, dh + 0.36, zc), Vec3::new(0.04, 0.24, 0.6));
    spawn_quad(commands, palette, disp_mat.clone(), Vec3::new(xf + 0.045, dh + 0.36, zc), Vec2::new(0.54, 0.2), Quat::from_rotation_y(FRAC_PI_2));
    commands.entity(disp_housing).insert(ElevatorDoors {
        left_door,
        right_door,
        display_mat: disp_mat,
        center_z: zc,
        open: 0.0,
    });

    spawn_box(commands, palette, steel, Vec3::new(xf + 0.015, 1.12, zc + dw + 0.26), Vec3::new(0.03, 0.26, 0.12));
    for by in [1.18_f32, 1.06] {
        spawn_cyl_rot(
            commands,
            palette,
            btn_off.clone(),
            Vec3::new(xf + 0.032, by, zc + dw + 0.26),
            0.052,
            0.01,
            Quat::from_rotation_z(FRAC_PI_2),
        );
    }

    // --- CONCRETE STAIRWELL BEHIND FIRE DOOR (`entrance.js:110-167`) ---
    let z0 = STAIR_Z - STAIR_W * 0.5;
    let z1 = STAIR_Z + STAIR_W * 0.5;
    let sh = STAIR_H;

    let st_steel = palette.mat(materials, "#5E656E", 0.45, 0.4);
    let st_wall = palette.mat(materials, "#E6E3DA", 0.9, 0.0);
    let st_dado = palette.mat(materials, "#86A58F", 0.85, 0.0);
    let concrete = palette.mat(materials, "#A6A8A6", 0.95, 0.0);
    let ceil_light = palette.emissive(materials, "#FFF6DE", 0.9);

    // Doorway frame + threshold on lobby side
    for z in [z0 - 0.025, z1 + 0.025] {
        spawn_box(commands, palette, st_steel.clone(), Vec3::new(-7.95, (sh + 0.06) * 0.5, z), Vec3::new(0.06, sh + 0.06, 0.05));
    }
    spawn_box(commands, palette, st_steel.clone(), Vec3::new(-7.95, sh + 0.03, STAIR_Z), Vec3::new(0.06, 0.06, STAIR_W + 0.1));
    spawn_box(commands, palette, st_steel.clone(), Vec3::new(-8.15, 0.012, STAIR_Z), Vec3::new(0.3, 0.02, STAIR_W));

    // Concrete landing enclosure behind outer left wall
    let bx = -9.45_f32;
    let cz0 = z0 - 0.4;
    let cz1 = z1 + 0.3;
    let depth = -8.3 - bx;
    let mid_x = (bx - 8.3) * 0.5;
    let mid_z = (cz0 + cz1) * 0.5;
    spawn_box(commands, palette, concrete.clone(), Vec3::new(mid_x, -0.01, mid_z), Vec3::new(depth, 0.04, cz1 - cz0));
    spawn_box(commands, palette, st_wall.clone(), Vec3::new(bx - 0.05, 1.65, mid_z), Vec3::new(0.1, 3.3, cz1 - cz0 + 0.2));
    for z in [cz0 - 0.05, cz1 + 0.05] {
        spawn_box(commands, palette, st_wall.clone(), Vec3::new(mid_x, 1.65, z), Vec3::new(depth + 0.1, 3.3, 0.1));
    }
    spawn_box(commands, palette, st_wall, Vec3::new(mid_x, 3.3, mid_z), Vec3::new(depth + 0.1, 0.08, cz1 - cz0 + 0.2));
    spawn_box(commands, palette, st_dado, Vec3::new(bx + 0.01, 0.55, mid_z), Vec3::new(0.02, 1.1, cz1 - cz0));

    // 6 concrete steps + sloped steel handrail + 2 posts + ceiling lamp
    let run = 0.27_f32;
    let rise = 0.17_f32;
    let sx = bx + 0.24;
    for k in 0..6usize {
        let step_z = cz1 - 0.14 - k as f32 * run;
        if step_z < cz0 + 0.1 {
            break;
        }
        let step_h = rise * (k + 1) as f32;
        spawn_box(commands, palette, concrete.clone(), Vec3::new(sx, step_h * 0.5, step_z), Vec3::new(0.46, step_h, run));
    }
    let rail_len = (5.0 * run).hypot(5.0 * rise);
    spawn_box_rot(
        commands,
        palette,
        st_steel.clone(),
        Vec3::new(sx + 0.25, 0.95 + 3.5 * rise, cz1 - 0.14 - 2.5 * run),
        Vec3::new(0.04, 0.04, rail_len),
        Quat::from_rotation_x(rise.atan2(run)),
    );
    for k in [0usize, 5] {
        spawn_box(
            commands,
            palette,
            st_steel.clone(),
            Vec3::new(sx + 0.25, rise * (k + 1) as f32 + 0.47, cz1 - 0.14 - k as f32 * run),
            Vec3::new(0.03, 0.95, 0.03),
        );
    }
    spawn_box(commands, palette, ceil_light, Vec3::new(-8.85, 3.24, STAIR_Z), Vec3::new(0.4, 0.03, 0.18));

    // Lit green EXIT sign above doorway + landing "12 FLOOR" wall sign
    let exit_housing = palette.mat(materials, "#E8ECEF", 0.45, 0.1);
    let exit_mat = palette.textured_unlit(images, materials, draw_exit_sign(fonts));
    let floor_mat = palette.textured_unlit(images, materials, draw_stair_floor_sign(12, fonts));
    spawn_box(commands, palette, exit_housing, Vec3::new(-7.95, sh + 0.2, STAIR_Z), Vec3::new(0.06, 0.15, 0.42));
    spawn_quad(commands, palette, exit_mat, Vec3::new(-7.915, sh + 0.2, STAIR_Z), Vec2::new(0.4, 0.075), Quat::from_rotation_y(FRAC_PI_2));
    spawn_quad(commands, palette, floor_mat, Vec3::new(bx + 0.025, 1.62, STAIR_Z - 0.15), Vec2::new(0.6, 0.38), Quat::from_rotation_y(FRAC_PI_2));

    // Pivoting grey steel fire door with wired glass pane, kick plate, push bar, and STAIRS plate
    let leaf_m = palette.mat(materials, "#7D848C", 0.45, 0.3);
    let pane_m = palette.mat(materials, "#33495A", 0.25, 0.4);
    let kick_m = palette.mat(materials, "#9AA1A9", 0.35, 0.55);
    let bar_m = palette.mat(materials, "#C9CED6", 0.3, 0.65);
    let bracket_m = palette.mat(materials, "#3A3F47", 0.45, 0.3);
    let plate_m = palette.textured_lit(images, materials, draw_stair_door_plate(12, fonts), 0.5);
    let leaf_l = STAIR_W - 0.016;

    let door_pivot = commands
        .spawn((
            WorldEntity,
            Transform::from_xyz(-8.05, 0.0, z0 + 0.008),
            Visibility::default(),
        ))
        .with_children(|p| {
            p.spawn((
                WorldEntity,
                Mesh3d(palette.unit_box.clone()),
                MeshMaterial3d(leaf_m),
                Transform::from_xyz(0.0225, (sh - 0.015) * 0.5, leaf_l * 0.5)
                    .with_scale(Vec3::new(0.045, sh - 0.015, leaf_l)),
            ));
            p.spawn((
                WorldEntity,
                Mesh3d(palette.unit_box.clone()),
                MeshMaterial3d(pane_m),
                Transform::from_xyz(0.0225, 1.5, leaf_l * 0.74).with_scale(Vec3::new(0.05, 0.55, 0.15)),
            ));
            p.spawn((
                WorldEntity,
                Mesh3d(palette.unit_box.clone()),
                MeshMaterial3d(kick_m),
                Transform::from_xyz(0.0225, 0.13, leaf_l * 0.5).with_scale(Vec3::new(0.05, 0.2, leaf_l - 0.06)),
            ));
            p.spawn((
                WorldEntity,
                Mesh3d(palette.unit_box.clone()),
                MeshMaterial3d(bar_m),
                Transform::from_xyz(0.075, 1.0, leaf_l * 0.5).with_scale(Vec3::new(0.035, 0.05, leaf_l * 0.72)),
            ));
            for f in [0.17_f32, 0.83] {
                p.spawn((
                    WorldEntity,
                    Mesh3d(palette.unit_box.clone()),
                    MeshMaterial3d(bracket_m.clone()),
                    Transform::from_xyz(0.06, 1.0, leaf_l * f).with_scale(Vec3::new(0.04, 0.09, 0.04)),
                ));
            }
            p.spawn((
                WorldEntity,
                Mesh3d(palette.unit_plane.clone()),
                MeshMaterial3d(plate_m),
                Transform::from_xyz(0.047, 1.45, leaf_l * 0.3)
                    .with_rotation(Quat::from_rotation_y(FRAC_PI_2))
                    .with_scale(Vec3::new(0.22, 0.137, 1.0)),
            ));
        })
        .id();
    commands.entity(door_pivot).insert(Stairwell {
        door_pivot,
        open: 0.0,
    });
}
