use crate::interact::{InteractiveProp, PropActivated};
use crate::layout::LayoutSpec;
use crate::palette::{WorldEntity, WorldPalette, spawn_box, spawn_cyl_rot, spawn_quad};
use bevy::prelude::*;
use station_canvas2d::FontEngine;
use station_canvas2d::office_surfaces::{draw_microwave_note, draw_microwave_timer};
use std::f32::consts::{FRAC_PI_2, PI};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum LunchboxKind {
    Blue,
    Green,
    Pink,
    Glass,
    Inox,
}

pub const ALL_LUNCHBOX_KINDS: [LunchboxKind; 5] = [
    LunchboxKind::Blue,
    LunchboxKind::Green,
    LunchboxKind::Pink,
    LunchboxKind::Glass,
    LunchboxKind::Inox,
];

impl LunchboxKind {
    pub fn lid_hex(self) -> &'static str {
        match self {
            Self::Blue => "#2F7FD0",
            Self::Green => "#3FA35B",
            Self::Pink => "#E06B9A",
            Self::Glass => "#2E86C1",
            Self::Inox => "#C9CED6",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum MicrowaveStage {
    #[default]
    Idle,
    Run,
    Done,
}

#[derive(Component, Debug, Clone)]
pub struct MicrowaveOven {
    pub index: usize,
    pub stage: MicrowaveStage,
    pub open: f32,
    pub open_goal: f32,
    pub time_left: f32,
    pub smell: f32,
    pub lunchbox: LunchboxKind,
    pub door_pivot: Entity,
    pub plate: Entity,
    pub window_ent: Entity,
    pub liner_mat: Handle<StandardMaterial>,
    pub win_run_mat: Handle<StandardMaterial>,
    pub win_idle_mat: Handle<StandardMaterial>,
    pub timer_ent: Entity,
    pub timer_run_mat: Handle<StandardMaterial>,
    pub timer_idle_mat: Handle<StandardMaterial>,
}

#[derive(Component, Debug, Clone, Copy)]
pub struct MicrowaveDoorPivot {
    pub oven_index: usize,
}

#[derive(Component, Debug, Clone, Copy)]
pub struct MicrowavePlate {
    pub oven_index: usize,
}

#[derive(Component, Debug, Clone, Copy)]
pub struct MicrowaveLunchbox {
    pub kind: LunchboxKind,
}

#[derive(Component, Debug, Clone, Copy)]
pub struct MicrowaveSmellWisp {
    pub oven_index: usize,
    pub phase: f32,
    pub base_pos: Vec3,
}

/// Spawns one of the 5 exact Oracle `microwave.js:26-48` lunchbox variants (`blue`/`green`/`pink`/`glass`/`inox`)
/// as a `ChildOf(parent)` entity.
pub fn spawn_lunchbox_child(
    commands: &mut Commands,
    palette: &mut WorldPalette,
    materials: &mut Assets<StandardMaterial>,
    parent: Option<Entity>,
    kind: LunchboxKind,
    local_pos: Vec3,
    scale: f32,
) -> Entity {
    let lid_m = palette.mat(materials, kind.lid_hex(), 0.42, if kind == LunchboxKind::Inox { 0.55 } else { 0.05 });
    let root = commands
        .spawn((
            WorldEntity,
            MicrowaveLunchbox { kind },
            Transform::from_translation(local_pos).with_scale(Vec3::splat(scale)),
            Visibility::default(),
        ))
        .id();
    if let Some(p) = parent {
        commands.entity(root).insert(ChildOf(p));
    }

    if kind == LunchboxKind::Inox {
        let steel = palette.mat(materials, "#D7DBE0", 0.3, 0.72);
        let seam = palette.mat(materials, "#AEB4BC", 0.35, 0.65);
        let handle = palette.mat(materials, "#8E949C", 0.35, 0.65);
        commands.entity(root).with_children(|c| {
            // Tier 1 + seam + Tier 2 + arched handle + top lid (`microwave.js:31-35`)
            c.spawn((
                WorldEntity,
                Mesh3d(palette.unit_cyl.clone()),
                MeshMaterial3d(steel.clone()),
                Transform::from_xyz(0.0, 0.025, 0.0).with_scale(Vec3::new(0.118, 0.05, 0.118)),
            ));
            c.spawn((
                WorldEntity,
                Mesh3d(palette.unit_cyl.clone()),
                MeshMaterial3d(seam),
                Transform::from_xyz(0.0, 0.05, 0.0).with_scale(Vec3::new(0.122, 0.006, 0.122)),
            ));
            c.spawn((
                WorldEntity,
                Mesh3d(palette.unit_cyl.clone()),
                MeshMaterial3d(steel),
                Transform::from_xyz(0.0, 0.076, 0.0).with_scale(Vec3::new(0.114, 0.045, 0.114)),
            ));
            c.spawn((
                WorldEntity,
                Mesh3d(palette.unit_torus.clone()),
                MeshMaterial3d(handle),
                Transform::from_xyz(0.0, 0.105, 0.0)
                    .with_rotation(Quat::from_rotation_x(FRAC_PI_2))
                    .with_scale(Vec3::new(0.075, 0.028, 0.068)),
            ));
            c.spawn((
                WorldEntity,
                Mesh3d(palette.unit_cyl.clone()),
                MeshMaterial3d(lid_m),
                Transform::from_xyz(0.0, 0.104, 0.0).with_scale(Vec3::new(0.12, 0.012, 0.12)),
            ));
        });
    } else {
        let body_hex = if kind == LunchboxKind::Glass {
            "#DCEFF3"
        } else {
            "#EEF1F2"
        };
        let body_m = palette.mat(materials, body_hex, 0.35, 0.05);
        let rice_m = palette.mat(materials, "#F4EFE2", 0.85, 0.0);
        let braised_m = palette.mat(materials, "#8B4A26", 0.65, 0.0);
        commands.entity(root).with_children(|c| {
            // Container base, rice, braised dish, lid + 4 Lock&Lock side clips (`microwave.js:40-44`)
            c.spawn((
                WorldEntity,
                Mesh3d(palette.unit_box.clone()),
                MeshMaterial3d(body_m),
                Transform::from_xyz(0.0, 0.0275, 0.0).with_scale(Vec3::new(0.17, 0.055, 0.12)),
            ));
            c.spawn((
                WorldEntity,
                Mesh3d(palette.unit_box.clone()),
                MeshMaterial3d(rice_m),
                Transform::from_xyz(0.0, 0.038, 0.0).with_scale(Vec3::new(0.15, 0.026, 0.10)),
            ));
            c.spawn((
                WorldEntity,
                Mesh3d(palette.unit_box.clone()),
                MeshMaterial3d(braised_m),
                Transform::from_xyz(0.035, 0.052, 0.0).with_scale(Vec3::new(0.06, 0.012, 0.08)),
            ));
            c.spawn((
                WorldEntity,
                Mesh3d(palette.unit_box.clone()),
                MeshMaterial3d(lid_m.clone()),
                Transform::from_xyz(0.0, 0.062, 0.0).with_scale(Vec3::new(0.176, 0.014, 0.126)),
            ));
            for (cx, cz, sx, sz) in [
                (0.0_f32, 0.066_f32, 0.05_f32, 0.01_f32),
                (0.0, -0.066, 0.05, 0.01),
                (0.091, 0.0, 0.01, 0.05),
                (-0.091, 0.0, 0.01, 0.05),
            ] {
                c.spawn((
                    WorldEntity,
                    Mesh3d(palette.unit_box.clone()),
                    MeshMaterial3d(lid_m.clone()),
                    Transform::from_xyz(cx, 0.054, cz).with_scale(Vec3::new(sx, 0.022, sz)),
                ));
            }
        });
    }
    root
}

#[allow(clippy::too_many_arguments)]
fn spawn_single_hollow_oven(
    commands: &mut Commands,
    palette: &mut WorldPalette,
    materials: &mut Assets<StandardMaterial>,
    timer_run_mat: Handle<StandardMaterial>,
    timer_idle_mat: Handle<StandardMaterial>,
    index: usize,
    pos: Vec3,
    ry: f32,
    big: f32,
    running: bool,
    lunchbox: LunchboxKind,
) {
    // Exact Oracle `microwave.js:84` dimensions
    const W: f32 = 0.46;
    const H: f32 = 0.27;
    const D: f32 = 0.36;
    const CAV_X0: f32 = -0.215;
    const CAV_X1: f32 = 0.08;
    const CAV_Y0: f32 = 0.035;
    const CAV_Y1: f32 = 0.235;
    const CAV_Z0: f32 = -0.15;
    const CAV_Z1: f32 = D * 0.5 - 0.004;

    let white = palette.mat(materials, "#ECEDEE", 0.5, 0.05);
    let grey = palette.mat(materials, "#3A3F46", 0.4, 0.15);
    let key_m = palette.mat(materials, "#5B626B", 0.5, 0.1);
    let start_m = palette.mat(materials, "#4C8A5E", 0.5, 0.05);
    let dial_m = palette.mat(materials, "#C9CED6", 0.3, 0.5);
    let frame_m = palette.mat(materials, "#E3E5E7", 0.45, 0.05);
    let handle_m = palette.mat(materials, "#9AA1A9", 0.4, 0.35);
    let plate_m = palette.mat(materials, "#CFE3E8", 0.15, 0.05);
    let liner_mat = palette.owned_emissive(materials, "#FFC870", if running { 1.4 } else { 0.0 });
    let win_run_mat = palette.glass(materials, "#141A20", 0.30);
    let win_idle_mat = palette.glass(materials, "#141A20", 0.85);
    let wisp_mat = palette.glass(materials, "#9DB87A", 0.45);

    let oven_ent = commands
        .spawn((
            WorldEntity,
            InteractiveProp::new("microwave", "pantry", "Microwave Oven"),
            Transform::from_translation(pos)
                .with_rotation(Quat::from_rotation_y(ry))
                .with_scale(Vec3::splat(big)),
            Visibility::default(),
        ))
        .id();

    let mut timer_ent = Entity::PLACEHOLDER;
    commands.entity(oven_ent).with_children(|g| {
        // 5-piece hollow outer shell around interior cavity (`microwave.js:89-93`)
        g.spawn((
            WorldEntity,
            Mesh3d(palette.unit_box.clone()),
            MeshMaterial3d(white.clone()),
            Transform::from_xyz(0.0, 0.0175, 0.0).with_scale(Vec3::new(W, 0.035, D)),
        ));
        g.spawn((
            WorldEntity,
            Mesh3d(palette.unit_box.clone()),
            MeshMaterial3d(white.clone()),
            Transform::from_xyz(0.0, (H + CAV_Y1) * 0.5, 0.0).with_scale(Vec3::new(W, H - CAV_Y1, D)),
        ));
        g.spawn((
            WorldEntity,
            Mesh3d(palette.unit_box.clone()),
            MeshMaterial3d(white.clone()),
            Transform::from_xyz((CAV_X0 + CAV_X1) * 0.5, (CAV_Y0 + CAV_Y1) * 0.5, (-D * 0.5 + CAV_Z0) * 0.5)
                .with_scale(Vec3::new(CAV_X1 - CAV_X0, CAV_Y1 - CAV_Y0, D * 0.5 + CAV_Z0 + 0.002)),
        ));
        g.spawn((
            WorldEntity,
            Mesh3d(palette.unit_box.clone()),
            MeshMaterial3d(white.clone()),
            Transform::from_xyz((-W * 0.5 + CAV_X0) * 0.5, H * 0.5, 0.0)
                .with_scale(Vec3::new(W * 0.5 + CAV_X0 + 0.002, H, D)),
        ));
        g.spawn((
            WorldEntity,
            Mesh3d(palette.unit_box.clone()),
            MeshMaterial3d(white.clone()),
            Transform::from_xyz((CAV_X1 + W * 0.5) * 0.5, H * 0.5, 0.0)
                .with_scale(Vec3::new(W * 0.5 - CAV_X1, H, D)),
        ));

        // Glowing back/top/side interior cavity liner panels
        g.spawn((
            WorldEntity,
            Mesh3d(palette.unit_box.clone()),
            MeshMaterial3d(liner_mat.clone()),
            Transform::from_xyz((CAV_X0 + CAV_X1) * 0.5, (CAV_Y0 + CAV_Y1) * 0.5, CAV_Z0 + 0.004)
                .with_scale(Vec3::new(CAV_X1 - CAV_X0 - 0.006, CAV_Y1 - CAV_Y0 - 0.006, 0.004)),
        ));
        g.spawn((
            WorldEntity,
            Mesh3d(palette.unit_box.clone()),
            MeshMaterial3d(liner_mat.clone()),
            Transform::from_xyz((CAV_X0 + CAV_X1) * 0.5, CAV_Y1 - 0.004, (CAV_Z0 + CAV_Z1) * 0.5)
                .with_scale(Vec3::new(CAV_X1 - CAV_X0 - 0.006, 0.004, CAV_Z1 - CAV_Z0 - 0.01)),
        ));

        // Control panel dark strip, 3x3 keypad, start button, dial, side vents (`microwave.js:95-100`)
        let px = (CAV_X1 + W * 0.5) * 0.5;
        g.spawn((
            WorldEntity,
            Mesh3d(palette.unit_box.clone()),
            MeshMaterial3d(grey.clone()),
            Transform::from_xyz(px, H * 0.5, D * 0.5 + 0.002)
                .with_scale(Vec3::new(W * 0.5 - CAV_X1 - 0.02, H - 0.05, 0.004)),
        ));
        for r in 0..3 {
            for c in 0..3 {
                g.spawn((
                    WorldEntity,
                    Mesh3d(palette.unit_box.clone()),
                    MeshMaterial3d(key_m.clone()),
                    Transform::from_xyz(
                        CAV_X1 + 0.045 + c as f32 * 0.032,
                        0.165 - r as f32 * 0.022,
                        D * 0.5 + 0.005,
                    )
                    .with_scale(Vec3::new(0.026, 0.014, 0.006)),
                ));
            }
        }
        g.spawn((
            WorldEntity,
            Mesh3d(palette.unit_box.clone()),
            MeshMaterial3d(start_m),
            Transform::from_xyz(CAV_X1 + 0.077, 0.09, D * 0.5 + 0.005)
                .with_scale(Vec3::new(0.09, 0.02, 0.006)),
        ));
        g.spawn((
            WorldEntity,
            Mesh3d(palette.unit_cyl.clone()),
            MeshMaterial3d(dial_m),
            Transform::from_xyz(CAV_X1 + 0.077, 0.05, D * 0.5 + 0.007)
                .with_rotation(Quat::from_rotation_x(FRAC_PI_2))
                .with_scale(Vec3::new(0.044, 0.012, 0.044)),
        ));
        for k in 0..6 {
            g.spawn((
                WorldEntity,
                Mesh3d(palette.unit_box.clone()),
                MeshMaterial3d(grey.clone()),
                Transform::from_xyz(W * 0.5 + 0.001, 0.17 - k as f32 * 0.018, -0.05)
                    .with_scale(Vec3::new(0.004, 0.01, 0.06)),
            ));
        }

        // LED timer readout quad (`microwave.js:122`)
        timer_ent = g
            .spawn((
                WorldEntity,
                Mesh3d(palette.unit_plane.clone()),
                MeshMaterial3d(if running {
                    timer_run_mat.clone()
                } else {
                    timer_idle_mat.clone()
                }),
                Transform::from_xyz(CAV_X1 + 0.077, 0.218, D * 0.5 + 0.006)
                    .with_scale(Vec3::new(0.10, 0.032, 1.0)),
            ))
            .id();

        // 3 rising steam / smell wisps above the oven (`microwave.js:68-77`)
        for j in 0..3 {
            let bx = (CAV_X0 + CAV_X1) * 0.5 + (j as f32 - 1.0) * 0.06;
            let base_pos = Vec3::new(bx, H + 0.03, 0.0);
            g.spawn((
                WorldEntity,
                MicrowaveSmellWisp {
                    oven_index: index,
                    phase: j as f32 / 3.0,
                    base_pos,
                },
                Mesh3d(palette.unit_sphere.clone()),
                MeshMaterial3d(wisp_mat.clone()),
                Transform::from_translation(base_pos).with_scale(Vec3::new(0.035, 0.09, 0.035)),
                if running {
                    Visibility::Inherited
                } else {
                    Visibility::Hidden
                },
            ));
        }
    });

    // Rotating glass turntable plate + child lunchbox (`microwave.js:106-109`)
    let plate_ent = commands
        .spawn((
            WorldEntity,
            MicrowavePlate { oven_index: index },
            Transform::from_xyz(
                (CAV_X0 + CAV_X1) * 0.5,
                CAV_Y0 + 0.004,
                (CAV_Z0 + CAV_Z1) * 0.5 - 0.005,
            ),
            Visibility::default(),
            ChildOf(oven_ent),
        ))
        .with_children(|p| {
            p.spawn((
                WorldEntity,
                Mesh3d(palette.unit_cyl.clone()),
                MeshMaterial3d(plate_m),
                Transform::from_xyz(0.0, 0.004, 0.0).with_scale(Vec3::new(0.245, 0.008, 0.245)),
            ));
        })
        .id();
    spawn_lunchbox_child(
        commands,
        palette,
        materials,
        Some(plate_ent),
        lunchbox,
        Vec3::new(0.0, 0.008, 0.0),
        0.92,
    );

    // Hinged door pivot at left edge (`microwave.js:111-120`)
    let dw = CAV_X1 - (-W * 0.5) + 0.004;
    let dh = H - 0.02;
    let fx = dw * 0.5;
    let mut window_ent = Entity::PLACEHOLDER;
    let door_pivot = commands
        .spawn((
            WorldEntity,
            MicrowaveDoorPivot { oven_index: index },
            Transform::from_xyz(-W * 0.5, 0.0, D * 0.5),
            Visibility::default(),
            ChildOf(oven_ent),
        ))
        .with_children(|h| {
            // 4-part door frame + vertical handle
            h.spawn((
                WorldEntity,
                Mesh3d(palette.unit_box.clone()),
                MeshMaterial3d(frame_m.clone()),
                Transform::from_xyz(fx, 0.03, 0.01).with_scale(Vec3::new(dw, 0.04, 0.02)),
            ));
            h.spawn((
                WorldEntity,
                Mesh3d(palette.unit_box.clone()),
                MeshMaterial3d(frame_m.clone()),
                Transform::from_xyz(fx, dh - 0.01, 0.01).with_scale(Vec3::new(dw, 0.04, 0.02)),
            ));
            h.spawn((
                WorldEntity,
                Mesh3d(palette.unit_box.clone()),
                MeshMaterial3d(frame_m.clone()),
                Transform::from_xyz(0.0175, dh * 0.5 + 0.01, 0.01)
                    .with_scale(Vec3::new(0.035, dh - 0.06, 0.02)),
            ));
            h.spawn((
                WorldEntity,
                Mesh3d(palette.unit_box.clone()),
                MeshMaterial3d(frame_m),
                Transform::from_xyz(dw - 0.025, dh * 0.5 + 0.01, 0.01)
                    .with_scale(Vec3::new(0.05, dh - 0.06, 0.02)),
            ));
            h.spawn((
                WorldEntity,
                Mesh3d(palette.unit_box.clone()),
                MeshMaterial3d(handle_m),
                Transform::from_xyz(dw - 0.022, dh * 0.5 + 0.01, 0.03)
                    .with_scale(Vec3::new(0.016, 0.16, 0.022)),
            ));
            window_ent = h
                .spawn((
                    WorldEntity,
                    Mesh3d(palette.unit_plane.clone()),
                    MeshMaterial3d(if running {
                        win_run_mat.clone()
                    } else {
                        win_idle_mat.clone()
                    }),
                    Transform::from_xyz(0.035 + (dw - 0.085) * 0.5, dh * 0.5 + 0.01, 0.012)
                        .with_scale(Vec3::new(dw - 0.085, dh - 0.06, 1.0)),
                ))
                .id();
        })
        .id();

    commands.entity(oven_ent).insert(MicrowaveOven {
        index,
        stage: if running {
            MicrowaveStage::Run
        } else {
            MicrowaveStage::Idle
        },
        open: 0.0,
        open_goal: 0.0,
        time_left: if running { 90.0 } else { 0.0 },
        smell: if running { 1.0 } else { 0.0 },
        lunchbox,
        door_pivot,
        plate: plate_ent,
        window_ent,
        liner_mat,
        win_run_mat,
        win_idle_mat,
        timer_ent,
        timer_run_mat,
        timer_idle_mat,
    });
}

/// Spawns dual hollow microwaves on the pantry cabinet in Pro/HQ (and 1 in Cozy)
/// with rotating glass plates, swinging doors, 5 typed lunchboxes, kitchen roll, dish soap,
/// taped red rule note, and rising steam/smell wisps (`microwave.js`).
pub fn spawn_microwaves(
    commands: &mut Commands,
    palette: &mut WorldPalette,
    materials: &mut Assets<StandardMaterial>,
    images: &mut Assets<Image>,
    fonts: &mut FontEngine,
    spec: &LayoutSpec,
) {
    let timer_run_mat = palette.textured_unlit(images, materials, draw_microwave_timer(90, fonts));
    let timer_idle_mat = palette.textured_unlit(images, materials, draw_microwave_timer(0, fonts));
    let note_mat = palette.textured_lit(images, materials, draw_microwave_note(fonts), 0.9);
    let roll_m = palette.mat(materials, "#F7F6F2", 0.9, 0.0);
    let soap_m = palette.mat(materials, "#F2C230", 0.3, 0.05);
    let soap_cap = palette.mat(materials, "#E84A2A", 0.4, 0.0);

    if spec.layout.is_pro() {
        // Exact Oracle `microwave.js:208-221`: low pantry cabinet against library partition wall
        const X: f32 = 8.68;
        const Z0: f32 = -4.95;
        const Z1: f32 = -3.65;
        const TOP: f32 = 0.90;
        const BIG: f32 = 1.2;
        const H: f32 = 0.27;

        let cab_body = palette.mat(materials, "#E6E1D8", 0.7, 0.0);
        let cab_top = palette.mat(materials, "#3B3F46", 0.35, 0.15);
        let cab_door = palette.mat(materials, "#D9D2C6", 0.7, 0.0);

        spawn_box(
            commands,
            palette,
            cab_body,
            Vec3::new(X + 0.21, (TOP - 0.03) * 0.5, (Z0 + Z1) * 0.5),
            Vec3::new(0.42, TOP - 0.03, Z1 - Z0),
        );
        spawn_box(
            commands,
            palette,
            cab_top,
            Vec3::new(X + 0.23, TOP - 0.02, (Z0 + Z1) * 0.5),
            Vec3::new(0.46, 0.04, Z1 - Z0 + 0.04),
        );
        for z in [Z0 + 0.32, Z1 - 0.32] {
            spawn_box(
                commands,
                palette,
                cab_door.clone(),
                Vec3::new(X + 0.425, 0.44, z),
                Vec3::new(0.02, 0.62, 0.60),
            );
        }

        // Kitchen paper roll + dish soap bottle on top of ovens (`microwave.js:216-217`)
        spawn_cyl_rot(
            commands,
            palette,
            roll_m,
            Vec3::new(X + 0.16, TOP + H * BIG + 0.055, Z1 - 0.38),
            0.11,
            0.22,
            Quat::from_rotation_x(FRAC_PI_2),
        );
        spawn_cyl_rot(
            commands,
            palette,
            soap_m,
            Vec3::new(X + 0.12, TOP + H * BIG + 0.075, Z0 + 0.20),
            0.065,
            0.15,
            Quat::IDENTITY,
        );
        spawn_cyl_rot(
            commands,
            palette,
            soap_cap,
            Vec3::new(X + 0.12, TOP + H * BIG + 0.16, Z0 + 0.20),
            0.028,
            0.025,
            Quat::IDENTITY,
        );

        // Taped A5 red rule note on cabinet front (`microwave.js:218`)
        spawn_quad(
            commands,
            palette,
            note_mat,
            Vec3::new(X + 0.438, 0.58, Z1 - 0.32),
            Vec2::new(0.25, 0.176),
            Quat::from_rotation_y(FRAC_PI_2),
        );

        // Dual hollow ovens side-by-side (`microwave.js:221`)
        spawn_single_hollow_oven(
            commands,
            palette,
            materials,
            timer_run_mat.clone(),
            timer_idle_mat.clone(),
            0,
            Vec3::new(X + 0.2, TOP, Z0 + 0.32),
            FRAC_PI_2,
            BIG,
            true,
            LunchboxKind::Blue,
        );
        spawn_single_hollow_oven(
            commands,
            palette,
            materials,
            timer_run_mat,
            timer_idle_mat,
            1,
            Vec3::new(X + 0.2, TOP, Z0 + 0.96),
            FRAC_PI_2,
            BIG,
            false,
            LunchboxKind::Inox,
        );

        // Remaining 3 typed lunchboxes (`Green`, `Pink`, `Glass`) waiting on the pantry counter
        for (k, kind) in [LunchboxKind::Green, LunchboxKind::Pink, LunchboxKind::Glass]
            .into_iter()
            .enumerate()
        {
            spawn_lunchbox_child(
                commands,
                palette,
                materials,
                None,
                kind,
                Vec3::new(11.45 + k as f32 * 0.24, 0.98, -7.52),
                1.0,
            );
        }
    } else {
        // Cozy: single hollow microwave beside coffee bar + soap + rule note
        let [cx, cz, _] = spec.coffee;
        let ox = cx + 0.72;
        spawn_single_hollow_oven(
            commands,
            palette,
            materials,
            timer_run_mat,
            timer_idle_mat,
            0,
            Vec3::new(ox, 0.88, cz),
            PI,
            1.05,
            true,
            LunchboxKind::Blue,
        );
        spawn_cyl_rot(
            commands,
            palette,
            soap_m,
            Vec3::new(ox + 0.16, 0.88 + 0.27 * 1.05 + 0.075, cz),
            0.06,
            0.14,
            Quat::IDENTITY,
        );
        spawn_quad(
            commands,
            palette,
            note_mat,
            Vec3::new(ox, 0.58, cz - 0.28),
            Vec2::new(0.24, 0.17),
            Quat::from_rotation_y(PI),
        );
    }
}

/// Animates microwave door hinges, rotating glass turntables, glowing interior cavities,
/// timer displays, rising steam/smell wisps, and `"microwave"` click interactions.
#[allow(clippy::type_complexity)]
pub fn animate_microwaves_system(
    time: Res<Time>,
    mut prop_ev: EventReader<PropActivated>,
    mut materials: ResMut<Assets<StandardMaterial>>,
    mut ovens: Query<&mut MicrowaveOven>,
    mut doors: Query<
        (&MicrowaveDoorPivot, &mut Transform),
        (Without<MicrowavePlate>, Without<MicrowaveSmellWisp>),
    >,
    mut plates: Query<
        (&MicrowavePlate, &mut Transform),
        (Without<MicrowaveDoorPivot>, Without<MicrowaveSmellWisp>),
    >,
    mut wisps: Query<
        (&MicrowaveSmellWisp, &mut Transform, &mut Visibility),
        (Without<MicrowaveDoorPivot>, Without<MicrowavePlate>),
    >,
    mut mat_q: Query<&mut MeshMaterial3d<StandardMaterial>>,
) {
    let dt = time.delta_secs().min(0.1);
    let t = time.elapsed_secs();

    let mut clicked = false;
    for ev in prop_ev.read() {
        if ev.id == "microwave" {
            clicked = true;
        }
    }

    if clicked {
        // Toggle an idle oven's door open/closed, or restart if all busy (`interact/props.js:175`)
        let mut toggled = false;
        for mut o in &mut ovens {
            if o.stage != MicrowaveStage::Run {
                o.open_goal = if o.open_goal < 0.5 { 1.0 } else { 0.0 };
                toggled = true;
                break;
            }
        }
        if !toggled
            && let Some(mut first) = ovens.iter_mut().next()
        {
            first.open_goal = if first.open_goal < 0.5 { 1.0 } else { 0.0 };
        }
    }

    // Track up to 2 ovens on stack (zero heap allocations per frame)
    let mut oven_open = [0.0_f32; 2];
    let mut oven_run = [false; 2];
    let mut oven_smell = [0.0_f32; 2];

    for mut o in &mut ovens {
        if (o.open - o.open_goal).abs() > 1e-4 {
            let dir = (o.open_goal - o.open).signum();
            o.open = (o.open + dir * dt * 2.6).clamp(0.0, 1.0);
        }
        if o.stage == MicrowaveStage::Run {
            o.time_left -= dt;
            if o.time_left <= 0.0 {
                o.time_left = 90.0; // loop gentle demo heating cycle cleanly
                o.smell = 1.0;
            }
        } else {
            o.smell = (o.smell - dt / 45.0).max(0.0);
        }

        let lit = o.stage == MicrowaveStage::Run || o.open > 0.05;
        if let Some(m) = materials.get_mut(&o.liner_mat) {
            let strength = if lit { 1.4 } else { 0.0 };
            m.emissive = LinearRgba::new(1.0 * strength, 0.62 * strength, 0.19 * strength, 1.0);
        }
        if let Ok(mut win_m) = mat_q.get_mut(o.window_ent) {
            let want = if o.stage == MicrowaveStage::Run {
                &o.win_run_mat
            } else {
                &o.win_idle_mat
            };
            if win_m.0 != *want {
                win_m.0 = want.clone();
            }
        }
        if let Ok(mut tm) = mat_q.get_mut(o.timer_ent) {
            let want = if o.stage == MicrowaveStage::Run {
                &o.timer_run_mat
            } else {
                &o.timer_idle_mat
            };
            if tm.0 != *want {
                tm.0 = want.clone();
            }
        }

        if o.index < 2 {
            oven_open[o.index] = o.open;
            oven_run[o.index] = o.stage == MicrowaveStage::Run;
            oven_smell[o.index] = o.smell;
        }
    }

    for (door, mut tf) in &mut doors {
        if door.oven_index < 2 {
            tf.rotation = Quat::from_rotation_y(-oven_open[door.oven_index] * 1.85);
        }
    }
    for (plate, mut tf) in &mut plates {
        if plate.oven_index < 2 && oven_run[plate.oven_index] {
            tf.rotate_y(dt * 0.55);
        }
    }
    for (wisp, mut tf, mut vis) in &mut wisps {
        let smell = if wisp.oven_index < 2 {
            oven_smell[wisp.oven_index]
        } else {
            0.0
        };
        if smell <= 0.02 {
            *vis = Visibility::Hidden;
            continue;
        }
        *vis = Visibility::Inherited;
        let u = (t / 2.4 + wisp.phase).fract();
        tf.translation = Vec3::new(
            wisp.base_pos.x + (u * 6.0 + wisp.phase * 6.28).sin() * 0.022,
            wisp.base_pos.y + u * 0.36,
            wisp.base_pos.z,
        );
        let env = (u * PI).sin() * smell.min(1.0);
        tf.scale = Vec3::new(0.035, 0.09, 0.035) * (0.4 + env * 0.9);
    }
}
