use crate::interact::{InteractiveProp, PropActivated};
use crate::layout::{DeskCoord, LayoutSpec};
use crate::palette::{WorldEntity, WorldPalette};
use crate::props_detail::PropsLiveTelemetry;
use bevy::prelude::*;
use std::f32::consts::{FRAC_PI_2, PI};

const REAMS: usize = 7;
const FULL_CTX_THRESHOLD: f32 = 0.85;
const PAPER_HEXES: [&str; 7] = [
    "#FBFBF7", "#F4F1E8", "#FBFBF7", "#E9D7A8", "#FBFBF7", "#CFE3F5", "#F4F1E8",
];

#[derive(Component, Debug, Clone)]
pub struct PaperShredder {
    pub fill: f32,
    pub run_left: f32,
    pub run_total: f32,
    pub pile_ent: Entity,
    pub feed_ent: Entity,
    pub led_mat: Handle<StandardMaterial>,
}

#[derive(Component, Debug, Clone, Copy)]
pub struct ShredderStrip {
    pub index: usize,
}

#[derive(Component, Debug, Clone)]
pub struct DeskContextStack {
    pub desk_index: usize,
    pub reams: [Entity; 7],
    pub sheets: [Entity; 3],
    pub red_flag: Entity,
    pub level: usize,
    pub over_85: bool,
}

#[derive(Component, Debug, Clone)]
pub struct WasteBin {
    pub desk_index: usize,
    pub balls: [Entity; 7],
    pub spill: [Entity; 3],
    pub squash: f32,
}

#[derive(Component, Debug, Clone)]
pub struct CacheCup {
    pub desk_index: usize,
    pub warmth: f32,
    pub cup_root: Entity,
    pub steam_root: Entity,
}

#[derive(Component, Debug, Clone, Copy)]
pub struct CacheSteamWisp {
    pub desk_index: usize,
    pub phase: f32,
    pub base_x: f32,
    pub base_z: f32,
}

fn desk_local(d: &DeskCoord, lx: f32, ly: f32, lz: f32) -> Vec3 {
    Vec3::new(d.x, 0.0, d.z) + Quat::from_rotation_y(d.ry) * Vec3::new(lx, ly, lz)
}

fn tray_of(is_pro: bool, i: usize) -> (f32, f32, f32) {
    if !is_pro {
        (-0.6, 0.2, -0.1)
    } else if i % 3 == 2 {
        (-0.6, -0.21, -0.1)
    } else {
        (0.6, -0.21, 0.1)
    }
}

fn preview_ctx_pct(desk_index: usize) -> f32 {
    match desk_index {
        0 => 0.52,
        1 => 0.91,
        2 => 0.36,
        3 => 0.72,
        4 => 0.28,
        5 => 0.44,
        _ => 0.0,
    }
}

fn preview_cup_warmth(desk_index: usize) -> f32 {
    match desk_index {
        3 => 0.65,
        5 => 0.90,
        _ => -1.0,
    }
}

/// Spawns the transparent-window `PaperShredder` (`shredder.js`), per-desk context-window
/// paper stacks + `>85%` red warning flags (`shredder.js`), per-desk metallic `WasteBin`s (`bins.js`),
/// and per-desk terracotta prompt-cache `CacheCup`s with rising steam wisps (`cups.js`).
pub fn spawn_shredder_bins_and_cups(
    commands: &mut Commands,
    palette: &mut WorldPalette,
    materials: &mut Assets<StandardMaterial>,
    spec: &LayoutSpec,
) {
    spawn_shredder(commands, palette, materials, spec);
    spawn_desk_stacks_bins_and_cups(commands, palette, materials, spec);
}

fn spawn_shredder(
    commands: &mut Commands,
    palette: &mut WorldPalette,
    materials: &mut Assets<StandardMaterial>,
    spec: &LayoutSpec,
) {
    let (sx, sz, ry) = if spec.layout.is_pro() {
        (-3.2_f32, 1.45_f32, FRAC_PI_2)
    } else {
        (-2.3_f32, 5.55_f32, PI)
    };
    let body_m = palette.mat(materials, "#3A3F47", 0.5, 0.15);
    let dark_m = palette.mat(materials, "#23272E", 0.45, 0.2);
    let slot_m = palette.mat(materials, "#0E1014", 0.5, 0.1);
    let win_m = palette.glass(materials, "#9FB3C4", 0.28);
    let pile_m = palette.mat(materials, "#F1EEE6", 0.95, 0.0);
    let strip_m = palette.mat(materials, "#FBFBF7", 0.9, 0.0);
    let led_mat = palette.owned_emissive(materials, "#3DDC84", 1.4);

    let root = commands
        .spawn((
            WorldEntity,
            InteractiveProp::new("shredder", "office", "Paper Shredder"),
            Transform::from_xyz(sx, 0.0, sz).with_rotation(Quat::from_rotation_y(ry)),
            Visibility::default(),
        ))
        .id();

    let mut pile_ent = Entity::PLACEHOLDER;
    let mut feed_ent = Entity::PLACEHOLDER;

    commands.entity(root).with_children(|g| {
        // Hollow bin base, back, sides, lower front lip (`shredder.js:59-62`)
        g.spawn((
            WorldEntity,
            Mesh3d(palette.unit_box.clone()),
            MeshMaterial3d(body_m.clone()),
            Transform::from_xyz(0.0, 0.01, 0.0).with_scale(Vec3::new(0.42, 0.02, 0.30)),
        ));
        g.spawn((
            WorldEntity,
            Mesh3d(palette.unit_box.clone()),
            MeshMaterial3d(body_m.clone()),
            Transform::from_xyz(0.0, 0.29, -0.135).with_scale(Vec3::new(0.42, 0.56, 0.03)),
        ));
        for side in [-1.0_f32, 1.0] {
            g.spawn((
                WorldEntity,
                Mesh3d(palette.unit_box.clone()),
                MeshMaterial3d(body_m.clone()),
                Transform::from_xyz(side * 0.195, 0.29, 0.0).with_scale(Vec3::new(0.03, 0.56, 0.30)),
            ));
        }
        g.spawn((
            WorldEntity,
            Mesh3d(palette.unit_box.clone()),
            MeshMaterial3d(body_m),
            Transform::from_xyz(0.0, 0.06, 0.135).with_scale(Vec3::new(0.42, 0.10, 0.03)),
        ));
        // Transparent front window (`shredder.js:63-64`)
        g.spawn((
            WorldEntity,
            Mesh3d(palette.unit_plane.clone()),
            MeshMaterial3d(win_m),
            Transform::from_xyz(0.0, 0.34, 0.148).with_scale(Vec3::new(0.36, 0.44, 1.0)),
        ));
        // Cutting head + paper feed slot + status LED (`shredder.js:65-68`)
        g.spawn((
            WorldEntity,
            Mesh3d(palette.unit_box.clone()),
            MeshMaterial3d(dark_m),
            Transform::from_xyz(0.0, 0.64, 0.0).with_scale(Vec3::new(0.46, 0.14, 0.34)),
        ));
        g.spawn((
            WorldEntity,
            Mesh3d(palette.unit_box.clone()),
            MeshMaterial3d(slot_m),
            Transform::from_xyz(0.0, 0.712, 0.02).with_scale(Vec3::new(0.30, 0.012, 0.025)),
        ));
        g.spawn((
            WorldEntity,
            Mesh3d(palette.unit_sphere.clone()),
            MeshMaterial3d(led_mat.clone()),
            Transform::from_xyz(0.17, 0.712, 0.12).with_scale(Vec3::splat(0.024)),
        ));
        // Shredded paper pile visible inside transparent window (`shredder.js:70-71`)
        pile_ent = g
            .spawn((
                WorldEntity,
                Mesh3d(palette.unit_box.clone()),
                MeshMaterial3d(pile_m),
                Transform::from_xyz(0.0, 0.06, -0.005).with_scale(Vec3::new(0.34, 0.08, 0.24)),
            ))
            .id();
        // 8 falling paper strips (`shredder.js:72-73`)
        for k in 0..8 {
            g.spawn((
                WorldEntity,
                ShredderStrip { index: k },
                Mesh3d(palette.unit_box.clone()),
                MeshMaterial3d(strip_m.clone()),
                Transform::from_xyz(-0.12 + k as f32 * 0.034, 0.42, 0.02)
                    .with_scale(Vec3::new(0.008, 0.09, 0.003)),
                Visibility::Hidden,
            ));
        }
        // Feeding stack sinking into slot (`shredder.js:75-76`)
        feed_ent = g
            .spawn((
                WorldEntity,
                Mesh3d(palette.unit_box.clone()),
                MeshMaterial3d(strip_m),
                Transform::from_xyz(0.0, 0.75, 0.02).with_scale(Vec3::new(0.20, 0.08, 0.27)),
                Visibility::Hidden,
            ))
            .id();
    });

    commands.entity(root).insert(PaperShredder {
        fill: 0.20,
        run_left: 0.0,
        run_total: 2.4,
        pile_ent,
        feed_ent,
        led_mat,
    });
}

fn spawn_desk_stacks_bins_and_cups(
    commands: &mut Commands,
    palette: &mut WorldPalette,
    materials: &mut Assets<StandardMaterial>,
    spec: &LayoutSpec,
) {
    let tray_m = palette.mat(materials, "#5E6670", 0.6, 0.1);
    let sheet_m = palette.mat(materials, "#FBFBF7", 0.9, 0.0);
    let flag_m = palette.emissive(materials, "#E63946", 0.7);
    let paper_mats: [Handle<StandardMaterial>; 7] =
        std::array::from_fn(|k| palette.mat(materials, PAPER_HEXES[k], 0.9, 0.0));

    let bin_m = palette.mat(materials, "#7C8794", 0.45, 0.35);
    let ball_m = palette.mat(materials, "#F4F1EA", 0.95, 0.0);

    let terra_m = palette.mat(materials, "#C65B3A", 0.4, 0.0);
    let coffee_m = palette.mat(materials, "#4A2E1C", 0.25, 0.0);
    let saucer_m = palette.mat(materials, "#E2DDD2", 0.35, 0.0);
    let steam_m = palette.glass(materials, "#FFFFFF", 0.36);

    let is_pro = spec.layout.is_pro();

    for (i, d) in spec.desks.iter().enumerate() {
        let rot = Quat::from_rotation_y(d.ry);
        let init_ctx = preview_ctx_pct(i);
        let init_n = if init_ctx <= 0.0 {
            0
        } else {
            ((init_ctx * REAMS as f32).round() as usize).clamp(1, REAMS)
        };
        let init_over = init_ctx > FULL_CTX_THRESHOLD;

        // 1. Per-desk context-window paper in-tray + 7 reams + 3 loose sheets + >85% red flag
        let (tx, tz, tr) = tray_of(is_pro, i);
        let sx = tx.signum();
        let tray_world = desk_local(d, tx, 0.0, tz);
        let tray_rot = rot * Quat::from_rotation_y(tr);
        let mut reams = [Entity::PLACEHOLDER; 7];
        let mut sheets = [Entity::PLACEHOLDER; 3];
        let mut red_flag = Entity::PLACEHOLDER;

        let stack_root = commands
            .spawn((
                WorldEntity,
                Transform::from_translation(tray_world).with_rotation(tray_rot),
                Visibility::default(),
            ))
            .with_children(|t| {
                // Tray floor + left/right/back walls (`shredder.js:164-166`)
                t.spawn((
                    WorldEntity,
                    Mesh3d(palette.unit_box.clone()),
                    MeshMaterial3d(tray_m.clone()),
                    Transform::from_xyz(0.0, 0.784, 0.0).with_scale(Vec3::new(0.25, 0.012, 0.32)),
                ));
                for side in [-1.0_f32, 1.0] {
                    t.spawn((
                        WorldEntity,
                        Mesh3d(palette.unit_box.clone()),
                        MeshMaterial3d(tray_m.clone()),
                        Transform::from_xyz(side * 0.125, 0.795, 0.0)
                            .with_scale(Vec3::new(0.012, 0.03, 0.32)),
                    ));
                }
                t.spawn((
                    WorldEntity,
                    Mesh3d(palette.unit_box.clone()),
                    MeshMaterial3d(tray_m.clone()),
                    Transform::from_xyz(0.0, 0.795, 0.155).with_scale(Vec3::new(0.25, 0.03, 0.012)),
                ));

                // 7 stacked paper reams (`shredder.js:38-42`)
                let seed = i as f32 * 7.31;
                for (k, slot) in reams.iter_mut().enumerate() {
                    let kf = k as f32;
                    let vis = if k < init_n {
                        Visibility::Inherited
                    } else {
                        Visibility::Hidden
                    };
                    *slot = t
                        .spawn((
                            WorldEntity,
                            Mesh3d(palette.unit_box.clone()),
                            MeshMaterial3d(paper_mats[(i + k) % PAPER_HEXES.len()].clone()),
                            Transform::from_xyz(
                                (seed + kf * 2.1).sin() * 0.008,
                                0.804 + kf * 0.027,
                                (seed + kf * 1.7).cos() * 0.008,
                            )
                            .with_rotation(Quat::from_rotation_y((seed + kf * 3.3).sin() * 0.07))
                            .with_scale(Vec3::new(0.20, 0.025, 0.27)),
                            vis,
                        ))
                        .id();
                }

                // 3 loose overflow sheets (`shredder.js:44-47`)
                let overflow_vis = if init_over {
                    Visibility::Inherited
                } else {
                    Visibility::Hidden
                };
                for (k, (ox, oz, ry_s)) in [
                    (-sx * 0.18_f32, 0.16_f32, 0.7_f32),
                    (sx * 0.04, 0.26, -0.5),
                    (-sx * 0.22, -0.08, 1.4),
                ]
                .into_iter()
                .enumerate()
                {
                    sheets[k] = t
                        .spawn((
                            WorldEntity,
                            Mesh3d(palette.unit_box.clone()),
                            MeshMaterial3d(sheet_m.clone()),
                            Transform::from_xyz(ox, 0.781 + k as f32 * 0.002, oz)
                                .with_rotation(Quat::from_rotation_y(ry_s))
                                .with_scale(Vec3::new(0.20, 0.002, 0.27)),
                            overflow_vis,
                        ))
                        .id();
                }

                // Red warning flag sticking out of >85% stack (`shredder.js:48-49`)
                red_flag = t
                    .spawn((
                        WorldEntity,
                        Mesh3d(palette.unit_box.clone()),
                        MeshMaterial3d(flag_m.clone()),
                        Transform::from_xyz(
                            sx * 0.07,
                            0.804 + init_n.max(1) as f32 * 0.027 + 0.03,
                            -0.10,
                        )
                        .with_rotation(Quat::from_euler(EulerRot::YXZ, 0.3, 0.15, 0.0))
                        .with_scale(Vec3::new(0.035, 0.09, 0.006)),
                        overflow_vis,
                    ))
                    .id();
            })
            .id();

        commands.entity(stack_root).insert(DeskContextStack {
            desk_index: i,
            reams,
            sheets,
            red_flag,
            level: init_n,
            over_85: init_over,
        });

        // 2. Per-desk waste bin beside chair (`bins.js:31-39`)
        let bin_pos = desk_local(d, 0.62, 0.0, -0.72);
        let mut balls = [Entity::PLACEHOLDER; 7];
        let mut spill = [Entity::PLACEHOLDER; 3];
        let bin_root = commands
            .spawn((
                WorldEntity,
                InteractiveProp::new("bins", "office", "Context Waste Bin"),
                Transform::from_translation(bin_pos).with_rotation(rot),
                Visibility::default(),
            ))
            .with_children(|b| {
                b.spawn((
                    WorldEntity,
                    Mesh3d(palette.unit_cyl.clone()),
                    MeshMaterial3d(bin_m.clone()),
                    Transform::from_xyz(0.0, 0.17, 0.0).with_scale(Vec3::new(0.28, 0.34, 0.28)),
                ));
                b.spawn((
                    WorldEntity,
                    Mesh3d(palette.unit_cyl.clone()),
                    MeshMaterial3d(bin_m.clone()),
                    Transform::from_xyz(0.0, 0.006, 0.0).with_scale(Vec3::new(0.24, 0.012, 0.24)),
                ));
                b.spawn((
                    WorldEntity,
                    Mesh3d(palette.thin_ring.clone()),
                    MeshMaterial3d(bin_m.clone()),
                    Transform::from_xyz(0.0, 0.34, 0.0).with_scale(Vec3::new(0.30, 0.30, 0.30)),
                ));
                for (k, slot) in balls.iter_mut().enumerate() {
                    let kf = k as f32;
                    let r = 0.095 + (k % 3) as f32 * 0.014;
                    let vis = if k < init_n {
                        Visibility::Inherited
                    } else {
                        Visibility::Hidden
                    };
                    *slot = b
                        .spawn((
                            WorldEntity,
                            Mesh3d(palette.unit_sphere.clone()),
                            MeshMaterial3d(ball_m.clone()),
                            Transform::from_xyz(
                                (kf * 2.4).sin() * 0.06,
                                0.06 + kf * 0.045,
                                (kf * 2.4).cos() * 0.06,
                            )
                            .with_scale(Vec3::splat(r)),
                            vis,
                        ))
                        .id();
                }
                let spill_vis = if init_over {
                    Visibility::Inherited
                } else {
                    Visibility::Hidden
                };
                for (k, slot) in spill.iter_mut().enumerate() {
                    let kf = k as f32;
                    *slot = b
                        .spawn((
                            WorldEntity,
                            Mesh3d(palette.unit_sphere.clone()),
                            MeshMaterial3d(ball_m.clone()),
                            Transform::from_xyz(0.22 + kf * 0.07, 0.045, -0.10 + kf * 0.12)
                                .with_scale(Vec3::splat(0.09)),
                            spill_vis,
                        ))
                        .id();
                }
            })
            .id();

        commands.entity(bin_root).insert(WasteBin {
            desk_index: i,
            balls,
            spill,
            squash: 0.0,
        });

        // 3. Per-desk terracotta prompt-cache cup + rising steam wisps (`cups.js:18-102`)
        let init_warmth = preview_cup_warmth(i);
        let cup_pos = desk_local(d, 0.39, 0.782, -0.1);
        let cup_vis = if init_warmth >= 0.0 {
            Visibility::Inherited
        } else {
            Visibility::Hidden
        };
        let steam_vis = if init_warmth > 0.0 {
            Visibility::Inherited
        } else {
            Visibility::Hidden
        };

        let cup_anchor = commands
            .spawn((
                WorldEntity,
                Transform::from_translation(cup_pos)
                    .with_rotation(Quat::from_rotation_y(d.ry + 0.5))
                    .with_scale(Vec3::splat(1.6)),
                Visibility::default(),
            ))
            .id();

        let cup_root = commands
            .spawn((
                WorldEntity,
                Transform::default(),
                cup_vis,
                ChildOf(cup_anchor),
            ))
            .with_children(|c| {
                // Saucer + terracotta mug + dark coffee + curved handle (`cups.js:61-66`)
                c.spawn((
                    WorldEntity,
                    Mesh3d(palette.unit_cyl.clone()),
                    MeshMaterial3d(saucer_m.clone()),
                    Transform::from_xyz(0.0, 0.003, 0.0).with_scale(Vec3::new(0.116, 0.006, 0.116)),
                ));
                c.spawn((
                    WorldEntity,
                    Mesh3d(palette.unit_cyl.clone()),
                    MeshMaterial3d(terra_m.clone()),
                    Transform::from_xyz(0.0, 0.044, 0.0).with_scale(Vec3::new(0.068, 0.075, 0.068)),
                ));
                c.spawn((
                    WorldEntity,
                    Mesh3d(palette.unit_cyl.clone()),
                    MeshMaterial3d(coffee_m.clone()),
                    Transform::from_xyz(0.0, 0.080, 0.0).with_scale(Vec3::new(0.062, 0.003, 0.062)),
                ));
                c.spawn((
                    WorldEntity,
                    Mesh3d(palette.unit_torus.clone()),
                    MeshMaterial3d(terra_m.clone()),
                    Transform::from_xyz(0.036, 0.045, 0.0)
                        .with_rotation(Quat::from_rotation_x(FRAC_PI_2))
                        .with_scale(Vec3::new(0.032, 0.012, 0.032)),
                ));
            })
            .id();

        let steam_root = commands
            .spawn((
                WorldEntity,
                Transform::from_xyz(0.0, 0.085, 0.0),
                steam_vis,
                ChildOf(cup_anchor),
            ))
            .with_children(|s| {
                for k in 0..3 {
                    let bx = (k as f32 - 1.0) * 0.014;
                    let bz = if k == 1 { 0.01 } else { -0.008 };
                    s.spawn((
                        WorldEntity,
                        CacheSteamWisp {
                            desk_index: i,
                            phase: k as f32 / 3.0 + (i as f32 * 0.17).fract(),
                            base_x: bx,
                            base_z: bz,
                        },
                        Mesh3d(palette.unit_sphere.clone()),
                        MeshMaterial3d(steam_m.clone()),
                        Transform::from_xyz(bx, 0.04, bz).with_scale(Vec3::new(0.018, 0.045, 0.018)),
                    ));
                }
            })
            .id();

        commands.entity(cup_anchor).insert(CacheCup {
            desk_index: i,
            warmth: init_warmth,
            cup_root,
            steam_root,
        });
    }
}

/// Syncs per-desk context stacks (`>85%` red warning flags), waste bins, terracotta cache cups
/// with rising steam wisps, and transparent-window shredder animations (zero per-frame heap allocs).
#[allow(clippy::too_many_arguments, clippy::type_complexity)]
pub fn animate_shredder_bins_and_cups_system(
    time: Res<Time>,
    mut telemetry: Option<ResMut<PropsLiveTelemetry>>,
    mut prop_ev: EventReader<PropActivated>,
    mut materials: ResMut<Assets<StandardMaterial>>,
    mut shredders: Query<&mut PaperShredder>,
    mut strips: Query<
        (&ShredderStrip, &mut Transform, &mut Visibility),
        Without<CacheSteamWisp>,
    >,
    mut stacks: Query<&mut DeskContextStack>,
    mut bins: Query<&mut WasteBin>,
    mut cups: Query<&mut CacheCup>,
    mut steam_wisps: Query<
        (&CacheSteamWisp, &mut Transform),
        Without<ShredderStrip>,
    >,
    mut part_tf: Query<
        &mut Transform,
        (Without<ShredderStrip>, Without<CacheSteamWisp>),
    >,
    mut part_vis: Query<&mut Visibility, Without<ShredderStrip>>,
) {
    let dt = time.delta_secs().min(0.1);
    let t = time.elapsed_secs();

    let mut trigger_shred = false;
    for ev in prop_ev.read() {
        if ev.id == "shredder" {
            trigger_shred = true;
        }
    }
    if let Some(ref mut tel) = telemetry
        && tel.shred_requested
    {
        tel.shred_requested = false;
        trigger_shred = true;
    }

    // 1. Animate transparent-window PaperShredder
    let mut running = false;
    let mut shred_fill = 0.2_f32;
    for mut sh in &mut shredders {
        if trigger_shred && sh.run_left <= 0.0 {
            sh.run_left = sh.run_total;
        }
        if sh.run_left > 0.0 {
            sh.run_left = (sh.run_left - dt).max(0.0);
            let u = 1.0 - (sh.run_left / sh.run_total.max(0.01));
            running = true;
            if let Ok(mut ftf) = part_tf.get_mut(sh.feed_ent) {
                ftf.translation.y = 0.75 - u * 0.11;
                ftf.scale.y = (0.08 * (1.0 - u)).max(0.004);
            }
            if let Ok(mut fvis) = part_vis.get_mut(sh.feed_ent) {
                *fvis = Visibility::Inherited;
            }
            if let Some(m) = materials.get_mut(&sh.led_mat) {
                let on = (t * 18.0).sin() > 0.0;
                m.emissive = if on {
                    LinearRgba::new(0.31 * 1.8, 0.64 * 1.8, 1.0 * 1.8, 1.0)
                } else {
                    LinearRgba::new(0.12, 0.23, 0.4, 1.0)
                };
            }
            if sh.run_left <= 0.0 {
                sh.fill = (sh.fill + 0.18).min(1.0);
                if let Ok(mut fvis) = part_vis.get_mut(sh.feed_ent) {
                    *fvis = Visibility::Hidden;
                }
                if let Some(m) = materials.get_mut(&sh.led_mat) {
                    m.emissive = LinearRgba::new(0.24 * 1.4, 0.86 * 1.4, 0.52 * 1.4, 1.0);
                }
            }
        }
        shred_fill = sh.fill;
        let pile_h = (sh.fill * 0.42).max(0.04);
        if let Ok(mut ptf) = part_tf.get_mut(sh.pile_ent) {
            ptf.scale.y = pile_h;
            ptf.translation.y = 0.02 + pile_h * 0.5;
        }
    }

    let top = 0.56_f32;
    let bot = 0.04 + shred_fill * 0.42;
    for (st, mut tf, mut vis) in &mut strips {
        if !running {
            *vis = Visibility::Hidden;
            continue;
        }
        *vis = Visibility::Inherited;
        let k = st.index as f32;
        let v = (t * 1.6 + k * 0.125).fract();
        tf.translation = Vec3::new(
            -0.12 + k * 0.034,
            top - v * (top - bot),
            0.02 + (k * 3.1).sin() * 0.05,
        );
    }

    // 2. Sync per-desk context stacks, waste bins, and terracotta cache cups from live telemetry
    if let Some(ref tel) = telemetry
        && tel.live_synced
    {
        for mut stack in &mut stacks {
            let ctx = tel
                .desks
                .iter()
                .find(|d| d.desk_index == stack.desk_index && d.occupied)
                .map(|d| d.ctx_pct.clamp(0.0, 1.0))
                .unwrap_or(0.0);
            let n = if ctx <= 0.0 {
                0
            } else {
                ((ctx * REAMS as f32).round() as usize).clamp(1, REAMS)
            };
            let over = ctx > FULL_CTX_THRESHOLD;
            if stack.level != n || stack.over_85 != over {
                stack.level = n;
                stack.over_85 = over;
                for (k, &ent) in stack.reams.iter().enumerate() {
                    if let Ok(mut v) = part_vis.get_mut(ent) {
                        *v = if k < n {
                            Visibility::Inherited
                        } else {
                            Visibility::Hidden
                        };
                    }
                }
                let ov = if over {
                    Visibility::Inherited
                } else {
                    Visibility::Hidden
                };
                for &ent in &stack.sheets {
                    if let Ok(mut v) = part_vis.get_mut(ent) {
                        *v = ov;
                    }
                }
                if let Ok(mut v) = part_vis.get_mut(stack.red_flag) {
                    *v = ov;
                }
                if let Ok(mut tf) = part_tf.get_mut(stack.red_flag) {
                    tf.translation.y = 0.804 + n.max(1) as f32 * 0.027 + 0.03;
                }
            }
        }

        for mut bin in &mut bins {
            let ctx = tel
                .desks
                .iter()
                .find(|d| d.desk_index == bin.desk_index && d.occupied)
                .map(|d| d.ctx_pct.clamp(0.0, 1.0))
                .unwrap_or(0.0);
            let n = ((ctx * bin.balls.len() as f32).round() as usize).min(bin.balls.len());
            let over = ctx > FULL_CTX_THRESHOLD;
            for (k, &ent) in bin.balls.iter().enumerate() {
                if let Ok(mut v) = part_vis.get_mut(ent) {
                    *v = if k < n {
                        Visibility::Inherited
                    } else {
                        Visibility::Hidden
                    };
                }
            }
            let sv = if over {
                Visibility::Inherited
            } else {
                Visibility::Hidden
            };
            for &ent in &bin.spill {
                if let Ok(mut v) = part_vis.get_mut(ent) {
                    *v = sv;
                }
            }
            bin.squash = (bin.squash - dt * 4.0).max(0.0);
        }

        for mut cup in &mut cups {
            let next_warmth = tel
                .desks
                .iter()
                .find(|d| d.desk_index == cup.desk_index && d.occupied && d.resting)
                .map(|d| d.cache_warmth)
                .unwrap_or(-1.0);
            if (cup.warmth - next_warmth).abs() > 0.01 {
                cup.warmth = next_warmth;
                if let Ok(mut v) = part_vis.get_mut(cup.cup_root) {
                    *v = if next_warmth >= 0.0 {
                        Visibility::Inherited
                    } else {
                        Visibility::Hidden
                    };
                }
                if let Ok(mut v) = part_vis.get_mut(cup.steam_root) {
                    *v = if next_warmth > 0.0 {
                        Visibility::Inherited
                    } else {
                        Visibility::Hidden
                    };
                }
            }
        }
    }

    // 3. Animate rising terracotta prompt-cache steam wisps (stack array, zero allocs)
    let mut desk_warmth = [-1.0_f32; 64];
    for cup in &cups {
        if cup.desk_index < desk_warmth.len() {
            desk_warmth[cup.desk_index] = cup.warmth;
        }
    }
    for (wisp, mut tf) in &mut steam_wisps {
        let w = if wisp.desk_index < desk_warmth.len() {
            desk_warmth[wisp.desk_index]
        } else {
            -1.0
        };
        if w <= 0.0 {
            continue;
        }
        let strength = (0.25 + 0.75 * w.clamp(0.0, 1.0)).clamp(0.25, 1.0);
        let u = (t * 0.42 + wisp.phase).fract();
        tf.translation = Vec3::new(
            wisp.base_x + (u * 6.28 + wisp.phase * 6.28).sin() * 0.016 * u,
            0.01 + u * 0.16 * strength,
            wisp.base_z + (u * 4.2 + wisp.phase * 3.1).cos() * 0.012 * u,
        );
        let env = (u * PI).sin() * strength;
        tf.scale = Vec3::new(0.016 + u * 0.012, 0.036 + u * 0.024, 0.016 + u * 0.012) * env.max(0.05);
    }
}
