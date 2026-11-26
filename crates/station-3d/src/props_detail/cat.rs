use crate::interact::{InteractiveProp, PropActivated};
use crate::layout::LayoutSpec;
use crate::palette::{WorldEntity, WorldPalette};
use bevy::prelude::*;
use std::f32::consts::{FRAC_PI_2, PI, TAU};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum CatMode {
    Walk,
    #[default]
    Sit,
    Sleep,
    Hop,
}

#[derive(Component, Debug, Clone)]
pub struct OfficeCat {
    pub mode: CatMode,
    pub pos: Vec2,
    pub y: f32,
    pub face: f32,
    pub until: f32,
    pub waypoint_idx: usize,
    pub waypoints: Vec<Vec2>,
    pub sofa_pos: Vec3,
    pub sofa_via: Vec2,
    pub hop_t: f32,
    pub hop_from: Vec3,
    pub hop_to: Vec3,
    pub hop_to_sleep: bool,
    pub restless: bool,
    pub heart_mesh: Handle<Mesh>,
    pub heart_mat: Handle<StandardMaterial>,
}

#[derive(Component, Debug, Clone, Copy)]
pub struct CatBody;

#[derive(Component, Debug, Clone, Copy)]
pub struct CatHead;

#[derive(Component, Debug, Clone, Copy)]
pub struct CatLeg {
    pub index: usize,
}

#[derive(Component, Debug, Clone, Copy)]
pub struct CatTailSeg {
    pub index: usize,
}

#[derive(Component, Debug, Clone, Copy)]
pub struct CatHeart {
    pub life: f32,
    pub max_life: f32,
}

fn wrap_angle(mut a: f32) -> f32 {
    while a > PI {
        a -= TAU;
    }
    while a < -PI {
        a += TAU;
    }
    a
}

fn sofa_at(spec: &LayoutSpec, lx: f32, lz: f32) -> Vec2 {
    let [sx, sz, _, sry] = spec.sofa;
    let c = sry.cos();
    let s = sry.sin();
    Vec2::new(sx + lx * c + lz * s, sz - lx * s + lz * c)
}

/// Spawns the exact Oracle `cat.js` 20+ part articulated tabby `OfficeCat`
/// (body, cream belly, 3 tabby stripes, collar + bell, head, muzzle, 2 outer/inner ears,
/// 2 eyes + glints, pink nose, 4 articulated leg+paw pivots, and 6-joint chained tail).
pub fn spawn_office_cat(
    commands: &mut Commands,
    palette: &mut WorldPalette,
    materials: &mut Assets<StandardMaterial>,
    spec: &LayoutSpec,
) {
    let fur = palette.mat(materials, "#E59A4E", 0.85, 0.0);
    let cream = palette.mat(materials, "#F7DDBB", 0.9, 0.0);
    let dark = palette.mat(materials, "#B96A2A", 0.85, 0.0);
    let pink = palette.mat(materials, "#F4A7B9", 0.6, 0.0);
    let eye_m = palette.mat(materials, "#1E1A16", 0.2, 0.0);
    let glint_m = palette.emissive(materials, "#FFFFFF", 1.1);
    let collar_m = palette.mat(materials, "#D94A4A", 0.5, 0.0);
    let bell_m = palette.mat(materials, "#FFD166", 0.25, 0.65);
    let heart_mat = palette.emissive(materials, "#FF5C8A", 1.5);

    let (start_pos, sofa_pos, sofa_via, waypoints) = if spec.layout.is_pro() {
        let s_top = sofa_at(spec, 0.05, 1.12);
        let s_via = sofa_at(spec, 0.05, 1.95);
        (
            Vec2::new(8.6, 6.0),
            Vec3::new(s_top.x, 0.46, s_top.y),
            s_via,
            vec![
                Vec2::new(8.6, 6.0),
                Vec2::new(11.3, 7.5),
                Vec2::new(2.4, 7.9),
                Vec2::new(-5.6, 4.0),
                Vec2::new(-2.9, -1.7),
                Vec2::new(4.6, -5.4),
                Vec2::new(11.6, -2.9),
                Vec2::new(-6.1, -4.4),
            ],
        )
    } else {
        (
            Vec2::new(-0.9, 4.3),
            Vec3::new(-2.1, 0.46, -4.72),
            Vec2::new(-2.1, -3.85),
            vec![
                Vec2::new(-0.9, 4.3),
                Vec2::new(3.0, 4.4),
                Vec2::new(6.2, 0.6),
                Vec2::new(5.0, -2.8),
                Vec2::new(1.0, -3.9),
                Vec2::new(-6.2, -0.6),
                Vec2::new(-6.1, 3.4),
            ],
        )
    };

    let sph = palette.unit_sphere.clone();
    let cyl = palette.unit_cyl.clone();
    let cone = palette.unit_cone.clone();
    let torus = palette.unit_torus.clone();
    let ring = palette.thin_ring.clone();

    let cat_root = commands
        .spawn((
            WorldEntity,
            OfficeCat {
                mode: CatMode::Walk,
                pos: start_pos,
                y: 0.0,
                face: 0.0,
                until: 4.0,
                waypoint_idx: 1,
                waypoints,
                sofa_pos,
                sofa_via,
                hop_t: 0.0,
                hop_from: Vec3::ZERO,
                hop_to: Vec3::ZERO,
                hop_to_sleep: false,
                restless: false,
                heart_mesh: sph.clone(),
                heart_mat,
            },
            InteractiveProp::new("cat", "lounge", "Office Cat"),
            Transform::from_xyz(start_pos.x, 0.0, start_pos.y),
            Visibility::default(),
        ))
        .id();

    let body_ent = commands
        .spawn((
            WorldEntity,
            CatBody,
            Transform::default(),
            Visibility::default(),
            ChildOf(cat_root),
        ))
        .with_children(|b| {
            // 1. Torso (`cat.js:77`)
            b.spawn((
                WorldEntity,
                Mesh3d(sph.clone()),
                MeshMaterial3d(fur.clone()),
                Transform::from_xyz(0.0, 0.22, 0.0).with_scale(Vec3::new(0.34, 0.288, 0.54)),
            ));
            // 2. Cream belly (`cat.js:78`)
            b.spawn((
                WorldEntity,
                Mesh3d(sph.clone()),
                MeshMaterial3d(cream.clone()),
                Transform::from_xyz(0.0, 0.17, 0.05).with_scale(Vec3::new(0.24, 0.18, 0.33)),
            ));
            // 3-5. Three tabby back stripes (`cat.js:79`) — rotated upright in XY plane to arch over torso
            for i in 0..3 {
                b.spawn((
                    WorldEntity,
                    Mesh3d(torus.clone()),
                    MeshMaterial3d(dark.clone()),
                    Transform::from_xyz(0.0, 0.22, -0.12 + i as f32 * 0.1)
                        .with_rotation(Quat::from_rotation_x(FRAC_PI_2))
                        .with_scale(Vec3::new(0.36, 0.10, 0.31)),
                ));
            }
            // 6-7. Collar + gold bell
            b.spawn((
                WorldEntity,
                Mesh3d(ring.clone()),
                MeshMaterial3d(collar_m),
                Transform::from_xyz(0.0, 0.295, 0.19)
                    .with_rotation(Quat::from_rotation_x(0.45))
                    .with_scale(Vec3::new(0.18, 0.18, 0.18)),
            ));
            b.spawn((
                WorldEntity,
                Mesh3d(sph.clone()),
                MeshMaterial3d(bell_m),
                Transform::from_xyz(0.0, 0.255, 0.24).with_scale(Vec3::splat(0.036)),
            ));

            // Head group (`cat.js:80-89`)
            b.spawn((
                WorldEntity,
                CatHead,
                Transform::from_xyz(0.0, 0.36, 0.26),
                Visibility::default(),
            ))
            .with_children(|h| {
                // 8. Cranium
                h.spawn((
                    WorldEntity,
                    Mesh3d(sph.clone()),
                    MeshMaterial3d(fur.clone()),
                    Transform::default().with_scale(Vec3::new(0.28, 0.244, 0.247)),
                ));
                // 9. Cream muzzle
                h.spawn((
                    WorldEntity,
                    Mesh3d(sph.clone()),
                    MeshMaterial3d(cream.clone()),
                    Transform::from_xyz(0.0, -0.04, 0.09).with_scale(Vec3::new(0.156, 0.104, 0.104)),
                ));
                // 10-17. Left/Right ears, inner pink ears, dark eyes, white glints
                for s in [-1.0_f32, 1.0] {
                    h.spawn((
                        WorldEntity,
                        Mesh3d(cone.clone()),
                        MeshMaterial3d(fur.clone()),
                        Transform::from_xyz(s * 0.075, 0.11, -0.01)
                            .with_rotation(Quat::from_rotation_z(-s * 0.3))
                            .with_scale(Vec3::new(0.10, 0.09, 0.10)),
                    ));
                    h.spawn((
                        WorldEntity,
                        Mesh3d(cone.clone()),
                        MeshMaterial3d(pink.clone()),
                        Transform::from_xyz(s * 0.073, 0.105, 0.012)
                            .with_rotation(Quat::from_rotation_z(-s * 0.3))
                            .with_scale(Vec3::new(0.056, 0.06, 0.056)),
                    ));
                    h.spawn((
                        WorldEntity,
                        Mesh3d(sph.clone()),
                        MeshMaterial3d(eye_m.clone()),
                        Transform::from_xyz(s * 0.048, 0.02, 0.112).with_scale(Vec3::splat(0.04)),
                    ));
                    h.spawn((
                        WorldEntity,
                        Mesh3d(sph.clone()),
                        MeshMaterial3d(glint_m.clone()),
                        Transform::from_xyz(s * 0.044, 0.028, 0.128).with_scale(Vec3::splat(0.014)),
                    ));
                }
                // 18. Pink nose
                h.spawn((
                    WorldEntity,
                    Mesh3d(sph.clone()),
                    MeshMaterial3d(pink.clone()),
                    Transform::from_xyz(0.0, -0.015, 0.142).with_scale(Vec3::splat(0.028)),
                ));
            });

            // 19-26. Four articulated legs + cream paws (`cat.js:90`)
            let leg_offsets = [
                (-0.08_f32, 0.16_f32),
                (0.08, 0.16),
                (-0.08, -0.16),
                (0.08, -0.16),
            ];
            for (index, (lx, lz)) in leg_offsets.into_iter().enumerate() {
                b.spawn((
                    WorldEntity,
                    CatLeg { index },
                    Transform::from_xyz(lx, 0.16, lz),
                    Visibility::default(),
                ))
                .with_children(|l| {
                    l.spawn((
                        WorldEntity,
                        Mesh3d(cyl.clone()),
                        MeshMaterial3d(fur.clone()),
                        Transform::from_xyz(0.0, -0.075, 0.0).with_scale(Vec3::new(0.07, 0.15, 0.07)),
                    ));
                    l.spawn((
                        WorldEntity,
                        Mesh3d(sph.clone()),
                        MeshMaterial3d(cream.clone()),
                        Transform::from_xyz(0.0, -0.15, 0.01).with_scale(Vec3::new(0.076, 0.046, 0.091)),
                    ));
                });
            }
        })
        .id();

    // 27-32. Nested 6-joint wagging tail chain (`cat.js:91-93`)
    let mut parent = commands
        .spawn((
            WorldEntity,
            Transform::from_xyz(0.0, 0.27, -0.26),
            Visibility::default(),
            ChildOf(body_ent),
        ))
        .id();
    for i in 0..6 {
        let local = if i == 0 {
            Vec3::ZERO
        } else {
            Vec3::new(0.0, 0.055, -0.02)
        };
        let d = 2.0 * (0.034 - i as f32 * 0.002);
        let seg_mat = if i > 3 { dark.clone() } else { fur.clone() };
        let seg = commands
            .spawn((
                WorldEntity,
                CatTailSeg { index: i },
                Transform::from_translation(local),
                Visibility::default(),
                ChildOf(parent),
            ))
            .with_children(|s| {
                s.spawn((
                    WorldEntity,
                    Mesh3d(sph.clone()),
                    MeshMaterial3d(seg_mat),
                    Transform::from_xyz(0.0, 0.028, 0.0).with_scale(Vec3::splat(d)),
                ));
            })
            .id();
        parent = seg;
    }
}

/// Animates `OfficeCat` navigation, sofa hopping, walking legs, head bob, 6-joint wagging tail,
/// and floating hearts spawned on `"cat"` `PropActivated` clicks (with automatic cleanup).
#[allow(clippy::too_many_arguments, clippy::type_complexity)]
pub fn animate_cat_system(
    mut commands: Commands,
    time: Res<Time>,
    mut prop_ev: EventReader<PropActivated>,
    mut cats: Query<
        (&mut OfficeCat, &mut Transform),
        (
            Without<CatBody>,
            Without<CatHead>,
            Without<CatLeg>,
            Without<CatTailSeg>,
            Without<CatHeart>,
        ),
    >,
    mut bodies: Query<
        &mut Transform,
        (
            With<CatBody>,
            Without<OfficeCat>,
            Without<CatHead>,
            Without<CatLeg>,
            Without<CatTailSeg>,
            Without<CatHeart>,
        ),
    >,
    mut heads: Query<
        &mut Transform,
        (
            With<CatHead>,
            Without<OfficeCat>,
            Without<CatBody>,
            Without<CatLeg>,
            Without<CatTailSeg>,
            Without<CatHeart>,
        ),
    >,
    mut legs: Query<
        (&CatLeg, &mut Transform, &mut Visibility),
        (
            Without<OfficeCat>,
            Without<CatBody>,
            Without<CatHead>,
            Without<CatTailSeg>,
            Without<CatHeart>,
        ),
    >,
    mut tails: Query<
        (&CatTailSeg, &mut Transform),
        (
            Without<OfficeCat>,
            Without<CatBody>,
            Without<CatHead>,
            Without<CatLeg>,
            Without<CatHeart>,
        ),
    >,
    mut hearts: Query<
        (Entity, &mut CatHeart, &mut Transform),
        (
            Without<OfficeCat>,
            Without<CatBody>,
            Without<CatHead>,
            Without<CatLeg>,
            Without<CatTailSeg>,
        ),
    >,
) {
    let dt = time.delta_secs().min(0.1);
    let t = time.elapsed_secs();

    let mut petted = false;
    for ev in prop_ev.read() {
        if ev.id == "cat" {
            petted = true;
        }
    }

    let mut cat_mode = CatMode::Sit;

    for (mut c, mut tf) in &mut cats {
        if petted {
            for i in 0..4 {
                let ox = ((i as f32 * 1.7 + t).sin()) * 0.14;
                let oz = ((i as f32 * 2.3 + t).cos()) * 0.14;
                let life = 1.6 + i as f32 * 0.25;
                commands.spawn((
                    WorldEntity,
                    CatHeart {
                        life,
                        max_life: life,
                    },
                    Mesh3d(c.heart_mesh.clone()),
                    MeshMaterial3d(c.heart_mat.clone()),
                    Transform::from_xyz(c.pos.x + ox, c.y + 0.55 + i as f32 * 0.05, c.pos.y + oz)
                        .with_scale(Vec3::new(0.11, 0.10, 0.06)),
                ));
            }
            c.restless = true;
            if c.mode != CatMode::Walk && c.mode != CatMode::Hop {
                c.mode = CatMode::Sit;
                c.until = t + 3.5;
            }
        }

        match c.mode {
            CatMode::Walk => {
                let want_sofa = c.waypoint_idx % 4 == 3;
                let target = if want_sofa || c.waypoints.is_empty() {
                    c.sofa_via
                } else {
                    c.waypoints[c.waypoint_idx % c.waypoints.len()]
                };
                let delta = target - c.pos;
                let dist = delta.length();
                let step = 0.85 * dt;
                if dist <= step || dist < 1e-4 {
                    c.pos = target;
                    if want_sofa && c.pos.distance(c.sofa_via) < 0.25 {
                        // Local ~0.85m jump from `sofa_via` onto the coffee table to nap
                        c.hop_t = 0.0;
                        c.hop_from = Vec3::new(c.pos.x, 0.0, c.pos.y);
                        c.hop_to = c.sofa_pos;
                        c.hop_to_sleep = true;
                        c.mode = CatMode::Hop;
                    } else {
                        c.mode = CatMode::Sit;
                        c.until = t + 5.0;
                    }
                } else {
                    c.pos += (delta / dist) * step;
                    c.face = delta.x.atan2(delta.y);
                }
            }
            CatMode::Hop => {
                c.hop_t = (c.hop_t + dt * 1.8).min(1.0);
                let k = c.hop_t;
                let p = c.hop_from.lerp(c.hop_to, k);
                c.pos = Vec2::new(p.x, p.z);
                c.y = p.y + (k * PI).sin() * 0.35;
                let d = c.hop_to - c.hop_from;
                if d.x.abs() + d.z.abs() > 1e-4 {
                    c.face = d.x.atan2(d.z);
                }
                if k >= 1.0 {
                    if c.hop_to_sleep {
                        c.mode = CatMode::Sleep;
                        c.until = t + 14.0;
                        c.face = 0.4;
                    } else {
                        c.y = 0.0;
                        c.waypoint_idx = c.waypoint_idx.wrapping_add(1);
                        c.mode = CatMode::Walk;
                    }
                }
            }
            CatMode::Sit | CatMode::Sleep => {
                if t >= c.until {
                    if c.y > 0.01 {
                        c.hop_t = 0.0;
                        c.hop_from = Vec3::new(c.pos.x, c.y, c.pos.y);
                        c.hop_to = Vec3::new(c.sofa_via.x, 0.0, c.sofa_via.y);
                        c.hop_to_sleep = false;
                        c.mode = CatMode::Hop;
                    } else {
                        c.waypoint_idx = c.waypoint_idx.wrapping_add(1);
                        c.restless = false;
                        c.mode = CatMode::Walk;
                    }
                }
            }
        }

        let walking = c.mode == CatMode::Walk;
        let bob = if walking {
            (t * 12.0).sin().abs() * 0.015
        } else {
            0.0
        };
        tf.translation = Vec3::new(c.pos.x, c.y + bob, c.pos.y);
        let cur_yaw = tf.rotation.to_euler(EulerRot::YXZ).0;
        let next_yaw = cur_yaw + wrap_angle(c.face - cur_yaw) * (dt * 8.0).min(1.0);
        tf.rotation = Quat::from_rotation_y(next_yaw);
        cat_mode = c.mode;
    }

    let walking = cat_mode == CatMode::Walk;
    let sleeping = cat_mode == CatMode::Sleep;

    for mut btf in &mut bodies {
        btf.translation.y = if sleeping {
            -0.12
        } else if cat_mode == CatMode::Sit {
            -0.03
        } else {
            0.0
        };
        btf.scale = if sleeping {
            Vec3::new(1.1, 0.8, 0.85)
        } else {
            Vec3::ONE
        };
    }

    for mut htf in &mut heads {
        let rx = if sleeping { 0.45 } else { (t * 0.7).sin() * 0.08 };
        let ry = (t * 0.4).sin() * 0.3;
        htf.rotation = Quat::from_euler(EulerRot::YXZ, ry, rx, 0.0);
    }

    for (leg, mut ltf, mut vis) in &mut legs {
        let phase = if leg.index % 2 == 1 { PI } else { 0.0 }
            + if leg.index > 1 { FRAC_PI_2 } else { 0.0 };
        let rx = if walking {
            (t * 12.0 + phase).sin() * 0.5
        } else {
            0.0
        };
        ltf.rotation = Quat::from_rotation_x(rx);
        *vis = if sleeping {
            Visibility::Hidden
        } else {
            Visibility::Inherited
        };
    }

    for (seg, mut stf) in &mut tails {
        let i = seg.index as f32;
        let rx = if sleeping {
            0.5
        } else {
            -0.25 + (t * 2.2 - i * 0.6).sin() * 0.12
        };
        let rz = (t * if sleeping { 0.8 } else { 2.6 } - i * 0.5).sin()
            * if sleeping { 0.05 } else { 0.22 };
        stf.rotation = Quat::from_euler(EulerRot::XYZ, rx, 0.0, rz);
    }

    for (ent, mut h, mut htf) in &mut hearts {
        h.life -= dt;
        if h.life <= 0.0 {
            commands.entity(ent).try_despawn();
            continue;
        }
        htf.translation.y += dt * 0.45;
        let k = (h.life / h.max_life.max(0.01)).clamp(0.0, 1.0);
        htf.scale = Vec3::new(0.11, 0.10, 0.06) * k;
    }
}
