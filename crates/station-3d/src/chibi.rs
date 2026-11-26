use crate::chibi_meshes::{ChibiMeshCache, HairStyle};
use crate::palette::{WorldEntity, WorldPalette, parse_hex_rgb};
use bevy::prelude::*;
use std::f32::consts::{FRAC_PI_2, PI};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Hash)]
pub enum ChibiLook {
    Toy,
    #[default]
    Pixar,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum OutfitKind {
    Hoodie,
    Suit,
    Shirt,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WorkerAnim {
    Work,
    Wait,
    Rest,
    Sleep,
    Walk,
    Sing,
}

/// Exact character proportions ported 1:1 from `chibi-rig.js::DIMS`.
#[derive(Debug, Clone, Copy)]
pub struct ChibiDims {
    pub hip_y: f32,
    pub hip_x: f32,
    pub thigh: f32,
    pub shin: f32,
    pub leg_r: f32,
    pub shoe_r: f32,
    pub sh_x: f32,
    pub sh_y: f32,
    pub upper: f32,
    pub fore: f32,
    pub arm_r: f32,
    pub hand_r: f32,
    pub neck: f32,
    pub r: f32,
    pub sx: f32,
    pub sy: f32,
    pub sz: f32,
    pub torso_top: f32,
    pub torso_r: f32,
    pub pelvis_r: f32,
    pub torso_mid_r: f32,
    pub head_y: f32,
    pub sit_y: f32,
    pub cheek: f32,
    pub front: f32,
    pub chin: f32,
}

impl ChibiDims {
    pub const TOY_TORSO: &'static [[f32; 2]] = &[
        [0.0, -0.06],
        [0.23, -0.06],
        [0.25, 0.02],
        [0.256, 0.16],
        [0.242, 0.29],
        [0.205, 0.38],
        [0.13, 0.45],
        [0.0, 0.46],
    ];

    pub const PIXAR_TORSO: &'static [[f32; 2]] = &[
        [0.0, -0.06],
        [0.205, -0.06],
        [0.222, 0.04],
        [0.205, 0.17],
        [0.214, 0.30],
        [0.238, 0.42],
        [0.236, 0.49],
        [0.19, 0.55],
        [0.10, 0.585],
        [0.0, 0.59],
    ];

    pub const fn for_look(look: ChibiLook) -> Self {
        match look {
            ChibiLook::Toy => {
                let torso_top = 0.46;
                let neck = 0.03;
                let r = 0.44;
                let hip_y = 0.44;
                let leg_r = 0.105;
                Self {
                    hip_y,
                    hip_x: 0.112,
                    thigh: 0.17,
                    shin: 0.17,
                    leg_r,
                    shoe_r: 0.13,
                    sh_x: 0.215,
                    sh_y: 0.37,
                    upper: 0.17,
                    fore: 0.15,
                    arm_r: 0.068,
                    hand_r: 0.068,
                    neck,
                    r,
                    sx: 1.06,
                    sy: 0.96,
                    sz: 1.0,
                    torso_top,
                    torso_r: 0.25,
                    pelvis_r: 0.23,
                    torso_mid_r: 0.256,
                    head_y: torso_top + neck + r * 0.86,
                    sit_y: 0.49 + leg_r - hip_y,
                    cheek: 0.10,
                    front: 0.07,
                    chin: 0.05,
                }
            }
            ChibiLook::Pixar => {
                let torso_top = 0.59;
                let neck = 0.075;
                let r = 0.33;
                let hip_y = 0.58;
                let leg_r = 0.10;
                Self {
                    hip_y,
                    hip_x: 0.108,
                    thigh: 0.24,
                    shin: 0.24,
                    leg_r,
                    shoe_r: 0.12,
                    sh_x: 0.236,
                    sh_y: 0.49,
                    upper: 0.20,
                    fore: 0.18,
                    arm_r: 0.064,
                    hand_r: 0.062,
                    neck,
                    r,
                    sx: 1.0,
                    sy: 1.02,
                    sz: 0.98,
                    torso_top,
                    torso_r: 0.222,
                    pelvis_r: 0.205,
                    torso_mid_r: 0.205,
                    head_y: torso_top + neck + r * 0.86,
                    sit_y: 0.49 + leg_r - hip_y,
                    cheek: 0.06,
                    front: 0.06,
                    chin: 0.20,
                }
            }
        }
    }

    #[inline]
    pub fn shape_x(&self, t: f32) -> f32 {
        let lo = (-t).max(0.0);
        1.0 + self.cheek * (PI * (lo * 1.4).min(1.0)).sin() - self.chin * lo.powi(3)
    }

    #[inline]
    pub fn shape_z(&self, t: f32) -> f32 {
        1.0 + self.front * (PI * (-t).max(0.0)).sin()
    }

    /// Face surface depth at `(x, y)` on the sculpted skull (`chibi-rig.js::surf`).
    #[inline]
    pub fn surf(&self, x: f32, y: f32) -> f32 {
        let t = y / (self.r * self.sy);
        let nx = x / (self.r * self.sx * self.shape_x(t));
        self.r * self.sz * self.shape_z(t) * (1.0 - nx * nx - t * t).max(0.0).sqrt()
    }
}

#[derive(Debug, Clone)]
pub struct ChibiConfig {
    pub name: String,
    pub agent: String,
    pub skin: &'static str,
    pub shirt: &'static str,
    pub pants: &'static str,
    pub hair: &'static str,
    pub shoes: &'static str,
    pub iris: &'static str,
    pub tie: Option<&'static str>,
    pub hair_style: HairStyle,
    pub outfit: OutfitKind,
    pub accessory: Option<&'static str>, // "headphones" | "glasses" | "beanie" | "cap"
    pub look: ChibiLook,
    pub has_lanyard: bool,
    pub lanyard_color: &'static str,
    pub has_coat: bool,
    pub coat_color: &'static str,
    pub has_santa_hat: bool,
    pub has_visor: bool,
    pub seed: usize,
}

impl ChibiConfig {
    pub fn for_agent(agent: &str, name: &str, seed: usize) -> Self {
        const SKINS: [&str; 5] = ["#F7CBAA", "#F2BE98", "#E6AC82", "#F9D6BE", "#D49870"];
        const IRISES: [&str; 5] = ["#5B3A24", "#3F6E9C", "#4D7A49", "#7A5230", "#3A2E28"];
        const HAIRS: [&str; 6] = [
            "#1E1E24", "#3B2618", "#5A3A2A", "#2C2228", "#6B4A33", "#232836",
        ];
        const PANTS: [&str; 4] = ["#33415C", "#3A3A44", "#2E3B4E", "#4A4458"];
        let shirt = match agent {
            "claude" => "#D97757",
            "codex" => "#3E9B74",
            "opencode" => "#5F8FD8",
            "zcode" => "#9B6BCE",
            "antigravity" => "#E09A3E",
            _ => ["#E8865F", "#5F8FD8", "#4CAF7D", "#A66BCB", "#E0AE2A"][seed % 5],
        };
        let acc = match seed % 5 {
            0 => Some("headphones"),
            1 => Some("glasses"),
            2 => Some("cap"),
            3 => Some("beanie"),
            _ => None,
        };
        let outfit = if seed % 2 == 1 {
            OutfitKind::Hoodie
        } else {
            OutfitKind::Shirt
        };
        Self {
            name: name.to_owned(),
            agent: agent.to_owned(),
            skin: SKINS[seed % SKINS.len()],
            shirt,
            pants: PANTS[seed % PANTS.len()],
            hair: HAIRS[seed % HAIRS.len()],
            shoes: "#2D2A2E",
            iris: IRISES[seed % IRISES.len()],
            tie: None,
            hair_style: HairStyle::from_seed(seed),
            outfit,
            accessory: acc,
            look: ChibiLook::Pixar,
            has_lanyard: false,
            lanyard_color: "#2F6FB5",
            has_coat: false,
            coat_color: "#2F3B52",
            has_santa_hat: false,
            has_visor: false,
            seed,
        }
    }
}

#[derive(Component, Debug, Clone)]
pub struct ChibiWorker {
    pub name: String,
    pub agent: String,
    pub anim: WorkerAnim,
    pub base_pos: Vec3,
    pub base_yaw: f32,
    pub phase: f32,
    pub patrol_target: Option<Vec3>,
    pub sitting: bool,
}

/// Full hierarchical joint entity handles for a spawned Chibi character.
#[derive(Component, Debug, Clone, Copy)]
pub struct ChibiRig {
    pub look: ChibiLook,
    pub spine: Entity,
    pub torso: Entity,
    pub head: Entity,
    pub hip_l: Entity,
    pub hip_r: Entity,
    pub knee_l: Entity,
    pub knee_r: Entity,
    pub shoulder_l: Entity,
    pub shoulder_r: Entity,
    pub elbow_l: Entity,
    pub elbow_r: Entity,
    pub hand_l: Entity,
    pub hand_r: Entity,
    pub eye_ball_l: Entity,
    pub eye_ball_r: Entity,
    pub lash_l: Option<Entity>,
    pub lash_r: Option<Entity>,
    pub mouth_smile: Entity,
    pub mouth_o: Entity,
    pub shadow: Entity,
}

#[derive(Component, Debug, Clone, Copy)]
pub struct StatusHalo;

pub fn halo_material_for_anim(
    palette: &mut WorldPalette,
    materials: &mut Assets<StandardMaterial>,
    anim: WorkerAnim,
) -> Handle<StandardMaterial> {
    match anim {
        WorkerAnim::Work => palette.emissive(materials, "#3DDC84", 1.4),
        WorkerAnim::Wait => palette.emissive(materials, "#FFB23F", 1.6),
        WorkerAnim::Sleep => palette.emissive(materials, "#A08BD8", 1.0),
        _ => palette.emissive(materials, "#6AAEFF", 0.8),
    }
}

fn darken_rgb(hex: &str, k: f32) -> (u8, u8, u8) {
    let (r, g, b) = parse_hex_rgb(hex);
    (
        ((r as f32) * k).clamp(0.0, 255.0).round() as u8,
        ((g as f32) * k).clamp(0.0, 255.0).round() as u8,
        ((b as f32) * k).clamp(0.0, 255.0).round() as u8,
    )
}

fn lighten_rgb(hex: &str, k: f32) -> (u8, u8, u8) {
    let (r, g, b) = parse_hex_rgb(hex);
    (
        (r as f32 + (255.0 - r as f32) * k).clamp(0.0, 255.0).round() as u8,
        (g as f32 + (255.0 - g as f32) * k).clamp(0.0, 255.0).round() as u8,
        (b as f32 + (255.0 - b as f32) * k).clamp(0.0, 255.0).round() as u8,
    )
}

/// Analytical 2-bone arm IK solver using law of cosines and pole-plane alignment.
/// Assumes rest limbs point along local `-Y` (`shoulder -> (0,-upper_len,0) -> (0,-fore_len,0)`).
/// Returns `(upper_rot, fore_local_rot)` placing the end effector at `target` (within `< 1e-4m`
/// for any reachable target).
pub fn solve_two_bone_ik(
    shoulder: Vec3,
    target: Vec3,
    pole: Vec3,
    upper_len: f32,
    fore_len: f32,
) -> (Quat, Quat) {
    let to_target = target - shoulder;
    let raw_dist = to_target.length();
    let dir = if raw_dist > 1e-6 {
        to_target / raw_dist
    } else {
        Vec3::NEG_Y
    };

    let min_d = (upper_len - fore_len).abs() + 1e-4;
    let max_d = (upper_len + fore_len - 1e-4).max(min_d + 1e-4);
    let d = raw_dist.clamp(min_d, max_d);

    let to_pole = pole - shoulder;
    let mut bend_dir = to_pole - dir * to_pole.dot(dir);
    if bend_dir.length_squared() < 1e-8 {
        let fallback = if dir.z.abs() < 0.9 { Vec3::NEG_Z } else { Vec3::X };
        bend_dir = fallback - dir * fallback.dot(dir);
    }
    let bend_dir = bend_dir.normalize_or(Vec3::NEG_Z);

    let cos_a =
        ((upper_len * upper_len + d * d - fore_len * fore_len) / (2.0 * upper_len * d))
            .clamp(-1.0, 1.0);
    let sin_a = (1.0 - cos_a * cos_a).max(0.0).sqrt();

    let elbow_pos = shoulder + dir * (upper_len * cos_a) + bend_dir * (upper_len * sin_a);
    let clamped_target = shoulder + dir * d;

    let u_dir = (elbow_pos - shoulder).normalize_or(Vec3::NEG_Y);
    let f_dir = (clamped_target - elbow_pos).normalize_or(u_dir);

    // Orthonormal right-handed basis where local -Y maps to u_dir and local +X aligns with hinge axis.
    let col_y = -u_dir;
    let mut axis = dir.cross(bend_dir);
    if axis.length_squared() < 1e-8 {
        axis = if col_y.x.abs() < 0.9 { Vec3::X } else { Vec3::Z }.cross(col_y);
    }
    let col_x = axis.normalize_or(Vec3::X);
    let col_z = col_x.cross(col_y).normalize_or(Vec3::Z);
    let upper_rot = Quat::from_mat3(&Mat3::from_cols(col_x, col_y, col_z)).normalize();

    // Forearm local direction in shoulder space lies in the local YZ plane (pure X hinge).
    let f_local = (upper_rot.inverse() * f_dir).normalize_or(Vec3::NEG_Y);
    let fore_angle = (-f_local.z).atan2(-f_local.y);
    let fore_rot = Quat::from_rotation_x(fore_angle).normalize();

    (upper_rot, fore_rot)
}

/// Computed joint pose for one animation frame of a `ChibiRig`.
#[derive(Debug, Clone, Copy)]
pub struct ChibiPose {
    pub root_y_offset: f32,
    pub hip_l: f32,
    pub hip_r: f32,
    pub knee_l: f32,
    pub knee_r: f32,
    pub spine_rot: Quat,
    pub head_rot: Quat,
    pub upper_l: Quat,
    pub fore_l: Quat,
    pub upper_r: Quat,
    pub fore_r: Quat,
    pub lid: f32,
    pub mouth_o: bool,
    pub breathe: f32,
    pub shadow_visible: bool,
}

fn reach_spine_ik(
    d: &ChibiDims,
    side: f32,
    target_root_world: Vec3,
    root_y: f32,
    lean: f32,
) -> (Quat, Quat) {
    let shoulder = Vec3::new(side * d.sh_x, d.sh_y, 0.0);
    let rel_spine = Vec3::new(
        target_root_world.x,
        target_root_world.y - root_y - d.hip_y,
        target_root_world.z,
    );
    let target_spine = Quat::from_rotation_x(-lean) * rel_spine;
    let pole = Vec3::new(side * (d.sh_x + 0.14), d.sh_y - 0.24, -0.36);
    solve_two_bone_ik(
        shoulder,
        target_spine,
        pole,
        d.upper,
        d.fore + d.hand_r * 0.55,
    )
}

fn euler_arm(rx: f32, rz: f32, ex: f32) -> (Quat, Quat) {
    (
        Quat::from_euler(EulerRot::XYZ, rx, 0.0, rz),
        Quat::from_rotation_x(ex),
    )
}

/// Computes the full hierarchical pose (including 2-bone arm IK for typing/resting/sleeping/singing).
pub fn compute_chibi_pose(
    look: ChibiLook,
    anim: WorkerAnim,
    sitting: bool,
    ph: f32,
    blink: f32,
) -> ChibiPose {
    let d = ChibiDims::for_look(look);
    let mut root_y = if sitting { d.sit_y } else { 0.0 };
    let mut hip_l = if sitting { -1.52 } else { 0.0 };
    let mut hip_r = if sitting { -1.52 } else { 0.0 };
    let mut knee_l = if sitting { 1.45 } else { 0.03 };
    let mut knee_r = if sitting { 1.45 } else { 0.03 };
    let mut lean = 0.0_f32;
    let mut tilt = 0.0_f32;
    let mut head_euler = Vec3::ZERO;
    let (mut upper_l, mut fore_l) = euler_arm(0.04, -0.08, -0.15);
    let (upper_r, fore_r): (Quat, Quat);
    let mut lid = blink.clamp(0.0, 1.0);
    let mut mouth_o = false;

    let shoulder_y = |ln: f32, ry: f32| ry + d.hip_y + d.sh_y * ln.cos();
    let head_world_y = |ln: f32, ry: f32| ry + d.hip_y + d.head_y * ln.cos();
    let head_world_z = |ln: f32| d.head_y * ln.sin();

    match anim {
        WorkerAnim::Walk => {
            let s = (ph * 9.0).sin();
            hip_l = s * 0.55;
            hip_r = -s * 0.55;
            knee_l = (-s).max(0.0) * 0.85 + 0.05;
            knee_r = s.max(0.0) * 0.85 + 0.05;
            (upper_l, fore_l) = euler_arm(-s * 0.5, -0.08, -0.35);
            (upper_r, fore_r) = euler_arm(s * 0.5, 0.08, -0.35);
            root_y = (ph * 9.0).cos().abs() * 0.03;
            lean = 0.05;
        }
        WorkerAnim::Work => {
            let big = d.r > 0.4;
            lean = if sitting {
                if big { 0.06 } else { 0.18 }
            } else {
                0.10
            };
            head_euler = Vec3::new(if big { 0.02 } else { 0.12 }, (ph * 0.9).sin() * 0.05, 0.0);
            let y = if sitting { 0.82 } else { 1.0 };
            let z = if sitting { 0.40 } else { 0.38 };
            let j1 = (ph * 22.0).sin() * 0.012;
            let j2 = (ph * 22.0 + 1.7).sin() * 0.012;
            (upper_l, fore_l) = reach_spine_ik(&d, -1.0, Vec3::new(-0.11, y + j1, z), root_y, lean);
            (upper_r, fore_r) = reach_spine_ik(&d, 1.0, Vec3::new(0.11, y + j2, z), root_y, lean);
        }
        WorkerAnim::Wait => {
            (upper_r, fore_r) = euler_arm(0.0, 2.55 + (ph * 7.0).sin() * 0.12, -0.25);
            if sitting {
                (upper_l, fore_l) =
                    reach_spine_ik(&d, -1.0, Vec3::new(-0.14, root_y + d.hip_y + 0.07, 0.24), root_y, lean);
            }
            head_euler = Vec3::new(-0.06, (ph * 1.4).sin() * 0.12, 0.08);
            mouth_o = (ph * 3.0).sin() > 0.2;
            if !sitting {
                root_y = (ph * 4.0).sin().abs() * 0.04;
            }
        }
        WorkerAnim::Sleep => {
            lid = 1.0;
            if sitting {
                lean = -0.34;
                head_euler = Vec3::new(-0.05 + (ph * 0.9).sin() * 0.02, 0.0, 0.26);
                (upper_l, fore_l) =
                    reach_spine_ik(&d, -1.0, Vec3::new(-0.09, 0.90, 0.24), root_y, lean);
                (upper_r, fore_r) =
                    reach_spine_ik(&d, 1.0, Vec3::new(0.07, 0.91, 0.26), root_y, lean);
            } else {
                lean = 0.08;
                head_euler = Vec3::new(-0.12, 0.0, 0.30);
                (upper_l, fore_l) = euler_arm(-0.25, 0.25, -1.0);
                (upper_r, fore_r) = euler_arm(-0.25, -0.25, -1.0);
            }
        }
        WorkerAnim::Sing => {
            let sw = (ph * 2.2).sin();
            let up = (ph * 0.7).sin().max(0.0);
            tilt = sw * 0.07;
            lean = -0.03;
            head_euler = Vec3::new(-0.12, 0.0, sw * 0.08);
            mouth_o = (ph * 7.5).sin() > -0.3;
            let hy = head_world_y(lean, root_y);
            let hz = head_world_z(lean);
            (upper_r, fore_r) = reach_spine_ik(
                &d,
                1.0,
                Vec3::new(0.04, hy - d.r * 0.62 - 0.10, hz + d.r * 0.85 + 0.05),
                root_y,
                lean,
            );
            (upper_l, fore_l) = euler_arm(-0.55 - up * 0.9, -0.45 - up * 0.45, -0.3);
            if !sitting {
                root_y = sw.abs() * 0.015;
            }
        }
        WorkerAnim::Rest => {
            if sitting {
                (upper_l, fore_l) = reach_spine_ik(
                    &d,
                    -1.0,
                    Vec3::new(-0.14, root_y + d.hip_y + 0.07, 0.24),
                    root_y,
                    lean,
                );
                (upper_r, fore_r) = reach_spine_ik(
                    &d,
                    1.0,
                    Vec3::new(0.14, root_y + d.hip_y + 0.07, 0.24),
                    root_y,
                    lean,
                );
            } else {
                let sy = shoulder_y(lean, root_y);
                (upper_l, fore_l) = euler_arm((ph * 1.1).sin() * 0.04, -0.08, -0.15);
                (upper_r, fore_r) =
                    reach_spine_ik(&d, 1.0, Vec3::new(0.16, sy - 0.28, 0.20), root_y, lean);
            }
            head_euler = Vec3::new(0.0, (ph * 0.8).sin() * 0.14, 0.0);
        }
    }

    ChibiPose {
        root_y_offset: root_y,
        hip_l,
        hip_r,
        knee_l,
        knee_r,
        spine_rot: Quat::from_euler(EulerRot::XYZ, lean, 0.0, tilt),
        head_rot: Quat::from_euler(EulerRot::XYZ, head_euler.x, head_euler.y, head_euler.z),
        upper_l,
        fore_l,
        upper_r,
        fore_r,
        lid,
        mouth_o,
        breathe: 1.0 + (ph * 2.2).sin() * 0.018,
        shadow_visible: !sitting,
    }
}

fn spawn_mesh(
    commands: &mut Commands,
    parent: Entity,
    mesh: Handle<Mesh>,
    mat: Handle<StandardMaterial>,
    tf: Transform,
) -> Entity {
    let e = commands
        .spawn((WorldEntity, Mesh3d(mesh), MeshMaterial3d(mat), tf))
        .id();
    commands.entity(parent).add_child(e);
    e
}

fn spawn_group(commands: &mut Commands, parent: Entity, tf: Transform) -> Entity {
    let e = commands
        .spawn((WorldEntity, tf, Visibility::Visible))
        .id();
    commands.entity(parent).add_child(e);
    e
}

/// Spawns a 1:1 sculpted hierarchical `ChibiRig` using shared indexed `ChibiMeshCache`
/// meshes and deduplicated `WorldPalette` materials.
#[allow(clippy::too_many_arguments)]
pub fn spawn_chibi(
    commands: &mut Commands,
    cache: &ChibiMeshCache,
    palette: &mut WorldPalette,
    materials: &mut Assets<StandardMaterial>,
    cfg: &ChibiConfig,
    pos: Vec3,
    yaw: f32,
    anim: WorkerAnim,
    phase: f32,
    sitting: bool,
) -> Entity {
    let d = ChibiDims::for_look(cfg.look);
    let toy = cfg.look == ChibiLook::Toy;
    let r = d.r;

    let skin_m = palette.mat(materials, cfg.skin, if toy { 0.55 } else { 0.50 }, 0.0);
    let (er, eg, eb) = darken_rgb(cfg.skin, 0.94);
    let ear_m = palette.mat_rgb(materials, er, eg, eb, 0.55, 0.0, false);
    let (nr, ng, nb) = darken_rgb(cfg.skin, 0.95);
    let nose_m = palette.mat_rgb(materials, nr, ng, nb, 0.50, 0.0, false);
    let (cr, cg, cb) = darken_rgb(cfg.skin, 0.86);
    let crease_m = palette.mat_rgb(materials, cr, cg, cb, 0.60, 0.0, false);

    let fabric_rough = if toy { 0.70 } else { 0.85 };
    let top_m = palette.mat(materials, cfg.shirt, fabric_rough, 0.0);
    let (tdr, tdg, tdb) = darken_rgb(cfg.shirt, 0.74);
    let top_dark_m = palette.mat_rgb(materials, tdr, tdg, tdb, fabric_rough, 0.0, false);
    let pants_m = palette.mat(materials, cfg.pants, fabric_rough, 0.0);
    let (pdr, pdg, pdb) = darken_rgb(cfg.pants, 0.85);
    let cuff_pants_m = if toy {
        palette.mat_rgb(materials, pdr, pdg, pdb, fabric_rough, 0.0, false)
    } else {
        pants_m.clone()
    };

    let hair_m = palette.mat_double_sided(
        materials,
        cfg.hair,
        if toy { 0.50 } else { 0.42 },
        0.04,
    );
    let (br, bg, bb) = darken_rgb(cfg.hair, 0.80);
    let brow_m = palette.mat_rgb(materials, br, bg, bb, 0.60, 0.0, false);
    let shoe_m = palette.mat(materials, cfg.shoes, 0.45, 0.0);
    let sole_m = palette.mat(materials, "#F4F1EA", 0.70, 0.0);
    let white_unlit = palette.mat_unlit(materials, "#FFFFFF");
    let lash_m = palette.mat(materials, "#2A1D18", 0.50, 0.0);
    let iris_m = palette.mat(materials, cfg.iris, 0.25, 0.0);
    let (ilr, ilg, ilb) = lighten_rgb(cfg.iris, 0.35);
    let iris_glow_m = palette.mat_rgb(materials, ilr, ilg, ilb, 0.30, 0.0, false);
    let lip_m = palette.mat(materials, if toy { "#A9483F" } else { "#B9544B" }, 0.45, 0.0);
    let mouth_o_m = palette.mat(materials, "#6E2B26", 0.55, 0.0);
    let blush_m = palette.mat_blend(materials, "#FF8E8E", if toy { 0.42 } else { 0.30 }, true);
    let badge_m = halo_material_for_anim(palette, materials, anim);

    let initial_pose = compute_chibi_pose(cfg.look, anim, sitting, phase, 0.0);

    let root = commands
        .spawn((
            WorldEntity,
            ChibiWorker {
                name: cfg.name.clone(),
                agent: cfg.agent.clone(),
                anim,
                base_pos: pos,
                base_yaw: yaw,
                phase,
                patrol_target: None,
                sitting,
            },
            Transform::from_translation(pos + Vec3::Y * initial_pose.root_y_offset)
                .with_rotation(Quat::from_rotation_y(yaw)),
            Visibility::Visible,
        ))
        .id();

    // Radial drop shadow quad
    let shadow = commands
        .spawn((
            WorldEntity,
            Mesh3d(cache.shadow_quad.clone()),
            MeshMaterial3d(cache.shadow_mat.clone()),
            Transform::from_xyz(0.0, 0.012, 0.0),
            if initial_pose.shadow_visible {
                Visibility::Visible
            } else {
                Visibility::Hidden
            },
        ))
        .id();
    commands.entity(root).add_child(shadow);

    // Pelvis
    spawn_mesh(
        commands,
        root,
        cache.sphere.clone(),
        pants_m.clone(),
        Transform::from_xyz(0.0, d.hip_y, 0.0).with_scale(Vec3::new(
            d.pelvis_r * 0.98,
            d.pelvis_r * 0.98 * 0.62,
            d.pelvis_r * 0.98 * 0.80,
        )),
    );

    // Legs: hip -> knee -> sneaker
    let build_leg = |commands: &mut Commands, sx: f32, hip_rot: f32, knee_rot: f32| -> (Entity, Entity) {
        let hip = spawn_group(
            commands,
            root,
            Transform::from_xyz(sx * d.hip_x, d.hip_y, 0.0)
                .with_rotation(Quat::from_rotation_x(hip_rot)),
        );
        let thigh_h = (d.thigh + d.leg_r) * 0.5;
        spawn_mesh(
            commands,
            hip,
            cache.capsule.clone(),
            pants_m.clone(),
            Transform::from_xyz(0.0, -d.thigh * 0.5, 0.0)
                .with_scale(Vec3::new(d.leg_r * 2.0, thigh_h, d.leg_r * 2.0)),
        );
        let knee = spawn_group(
            commands,
            hip,
            Transform::from_xyz(0.0, -d.thigh, 0.0)
                .with_rotation(Quat::from_rotation_x(knee_rot)),
        );
        spawn_mesh(
            commands,
            knee,
            cache.sphere.clone(),
            pants_m.clone(),
            Transform::from_scale(Vec3::splat(d.leg_r * 0.96)),
        );
        spawn_mesh(
            commands,
            knee,
            cache.cyl.clone(),
            pants_m.clone(),
            Transform::from_xyz(0.0, -d.shin * 0.5, 0.0)
                .with_scale(Vec3::new(d.leg_r * 0.96, d.shin, d.leg_r * 0.96)),
        );
        spawn_mesh(
            commands,
            knee,
            cache.cyl.clone(),
            cuff_pants_m.clone(),
            Transform::from_xyz(0.0, -d.shin + 0.02, 0.0).with_scale(Vec3::new(
                d.leg_r * 1.08,
                if toy { 0.05 } else { 0.035 },
                d.leg_r * 1.08,
            )),
        );
        let shoe = spawn_group(commands, knee, Transform::from_xyz(0.0, -d.shin, 0.035));
        let s = d.shoe_r;
        spawn_mesh(
            commands,
            shoe,
            cache.sphere.clone(),
            shoe_m.clone(),
            Transform::from_xyz(0.0, 0.0, 0.02)
                .with_scale(Vec3::new(s * 0.82, s * 0.58, s * 1.32)),
        );
        spawn_mesh(
            commands,
            shoe,
            cache.cyl.clone(),
            sole_m.clone(),
            Transform::from_xyz(0.0, -s * 0.58 - 0.008, 0.02)
                .with_scale(Vec3::new(s * 0.86, 0.045, s * 1.36)),
        );
        spawn_mesh(
            commands,
            shoe,
            cache.box_mesh.clone(),
            sole_m.clone(),
            Transform::from_xyz(0.0, s * 0.42, s * 0.38)
                .with_rotation(Quat::from_rotation_x(-0.35))
                .with_scale(Vec3::new(s * 0.70, 0.02, s * 0.75)),
        );
        (hip, knee)
    };
    let (hip_l, knee_l) = build_leg(commands, -1.0, initial_pose.hip_l, initial_pose.knee_l);
    let (hip_r, knee_r) = build_leg(commands, 1.0, initial_pose.hip_r, initial_pose.knee_r);

    // Spine (pivots at hip_y; everything above leans together)
    let spine = spawn_group(
        commands,
        root,
        Transform::from_xyz(0.0, d.hip_y, 0.0).with_rotation(initial_pose.spine_rot),
    );

    // Lathe torso + outfit details
    let torso = spawn_mesh(
        commands,
        spine,
        cache.torso_handle(cfg.look),
        top_m.clone(),
        Transform::from_scale(Vec3::new(1.0, initial_pose.breathe, 0.82)),
    );
    let h = d.torso_top;
    let tr = d.torso_r;
    // Proud-of-chest Z offset so outfit trims clear the upper chest bulge of both Toy and Pixar lathes.
    let chest_z = if toy { tr * 1.02 } else { 0.244 };
    match cfg.outfit {
        OutfitKind::Hoodie => {
            spawn_mesh(
                commands,
                torso,
                cache.torus_thin.clone(),
                top_dark_m.clone(),
                Transform::from_xyz(0.0, -0.03, 0.0)
                    .with_rotation(Quat::from_rotation_x(FRAC_PI_2))
                    .with_scale(Vec3::splat(tr * 0.93)),
            );
            spawn_mesh(
                commands,
                torso,
                cache.box_mesh.clone(),
                top_dark_m.clone(),
                Transform::from_xyz(0.0, h * 0.24, tr * 0.96)
                    .with_scale(Vec3::new(tr * 1.0, h * 0.20, 0.05)),
            );
            spawn_mesh(
                commands,
                torso,
                cache.torus_thick.clone(),
                top_dark_m.clone(),
                Transform::from_xyz(0.0, h * 0.93, -0.02)
                    .with_rotation(Quat::from_rotation_x(FRAC_PI_2 - 0.25))
                    .with_scale(Vec3::splat(h * 0.21)),
            );
            for sx in [-1.0_f32, 1.0] {
                spawn_mesh(
                    commands,
                    torso,
                    cache.cyl.clone(),
                    sole_m.clone(),
                    Transform::from_xyz(sx * 0.045, h * 0.72, chest_z)
                        .with_rotation(Quat::from_rotation_x(0.18))
                        .with_scale(Vec3::new(0.009, h * 0.24, 0.009)),
                );
            }
        }
        OutfitKind::Suit => {
            let shirt_w = palette.mat(materials, "#F4F1EA", 0.80, 0.0);
            let tie_m = palette.mat(materials, cfg.tie.unwrap_or("#B5323A"), 0.45, 0.0);
            spawn_mesh(
                commands,
                torso,
                cache.suit_v_handle(cfg.look),
                shirt_w.clone(),
                Transform::from_xyz(0.0, h * 0.96, chest_z + 0.004)
                    .with_rotation(Quat::from_rotation_x(-0.14)),
            );
            spawn_mesh(
                commands,
                torso,
                cache.torus_med.clone(),
                shirt_w,
                Transform::from_xyz(0.0, h * 0.95, 0.01)
                    .with_rotation(Quat::from_rotation_x(FRAC_PI_2 - 0.15))
                    .with_scale(Vec3::new(h * 0.18, h * 0.18, h * 0.18 * 0.9)),
            );
            spawn_mesh(
                commands,
                torso,
                cache.box_mesh.clone(),
                tie_m.clone(),
                Transform::from_xyz(0.0, h * 0.68, chest_z + 0.020)
                    .with_rotation(Quat::from_rotation_x(-0.10))
                    .with_scale(Vec3::new(0.048, h * 0.36, 0.014)),
            );
            spawn_mesh(
                commands,
                torso,
                cache.sphere.clone(),
                tie_m,
                Transform::from_xyz(0.0, h * 0.88, chest_z + 0.014)
                    .with_scale(Vec3::new(0.026 * 1.2, 0.026 * 0.9, 0.026 * 0.7)),
            );
            for sx in [-1.0_f32, 1.0] {
                spawn_mesh(
                    commands,
                    torso,
                    cache.box_mesh.clone(),
                    top_dark_m.clone(),
                    Transform::from_xyz(sx * tr * 0.24, h * 0.74, chest_z + 0.008)
                        .with_rotation(Quat::from_euler(EulerRot::XYZ, -0.10, 0.0, sx * 0.40))
                        .with_scale(Vec3::new(0.052, h * 0.44, 0.018)),
                );
            }
            for i in 0..2 {
                spawn_mesh(
                    commands,
                    torso,
                    cache.sphere.clone(),
                    top_dark_m.clone(),
                    Transform::from_xyz(0.0, h * (0.38 - i as f32 * 0.13), chest_z - 0.010)
                        .with_scale(Vec3::splat(0.015)),
                );
            }
            spawn_mesh(
                commands,
                torso,
                cache.torus_thin.clone(),
                top_dark_m.clone(),
                Transform::from_xyz(0.0, -0.03, 0.0)
                    .with_rotation(Quat::from_rotation_x(FRAC_PI_2))
                    .with_scale(Vec3::splat(tr * 0.93)),
            );
        }
        OutfitKind::Shirt => {
            let collar_m = palette.mat(materials, "#F4F1EA", 0.80, 0.0);
            spawn_mesh(
                commands,
                torso,
                cache.torus_med.clone(),
                collar_m.clone(),
                Transform::from_xyz(0.0, h * 0.95, 0.01)
                    .with_rotation(Quat::from_rotation_x(FRAC_PI_2 - 0.15))
                    .with_scale(Vec3::new(h * 0.18, h * 0.18, h * 0.18 * 0.9)),
            );
            for sx in [-1.0_f32, 1.0] {
                spawn_mesh(
                    commands,
                    torso,
                    cache.box_mesh.clone(),
                    collar_m.clone(),
                    Transform::from_xyz(sx * 0.055, h * 0.88, chest_z - 0.015)
                        .with_rotation(Quat::from_euler(
                            EulerRot::XYZ,
                            -0.42,
                            sx * -0.35,
                            sx * 0.55,
                        ))
                        .with_scale(Vec3::new(0.10, 0.07, 0.022)),
                );
            }
            spawn_mesh(
                commands,
                torso,
                cache.box_mesh.clone(),
                top_dark_m.clone(),
                Transform::from_xyz(0.0, h * 0.60, chest_z + 0.002)
                    .with_rotation(Quat::from_rotation_x(-0.08))
                    .with_scale(Vec3::new(0.042, h * 0.42, 0.014)),
            );
            for i in 0..3 {
                spawn_mesh(
                    commands,
                    torso,
                    cache.sphere.clone(),
                    collar_m.clone(),
                    Transform::from_xyz(0.0, h * (0.76 - i as f32 * 0.12), chest_z + 0.009)
                        .with_scale(Vec3::splat(0.012)),
                );
            }
            spawn_mesh(
                commands,
                torso,
                cache.torus_thin.clone(),
                top_dark_m.clone(),
                Transform::from_xyz(0.0, -0.03, 0.0)
                    .with_rotation(Quat::from_rotation_x(FRAC_PI_2))
                    .with_scale(Vec3::splat(tr * 0.93)),
            );
        }
    }

    // Optional open coat overlay
    if cfg.has_coat {
        let coat_m = palette.mat_double_sided(materials, cfg.coat_color, 0.82, 0.0);
        let (cdr, cdg, cdb) = darken_rgb(cfg.coat_color, 0.72);
        let coat_dark = palette.mat_rgb(materials, cdr, cdg, cdb, 0.85, 0.0, false);
        spawn_mesh(commands, torso, cache.coat_handle(cfg.look), coat_m, Transform::IDENTITY);
        spawn_mesh(
            commands,
            torso,
            cache.arc_collar.clone(),
            coat_dark,
            Transform::from_xyz(0.0, h * 0.93, -0.01)
                .with_rotation(Quat::from_euler(
                    EulerRot::XYZ,
                    FRAC_PI_2 - 0.30,
                    0.0,
                    PI * 0.725,
                ))
                .with_scale(Vec3::splat(h * 0.20)),
        );
    }

    // Optional ID lanyard (interns / farewell)
    if cfg.has_lanyard {
        let strap_m = palette.mat(materials, cfg.lanyard_color, 0.80, 0.0);
        let card_m = palette.mat(materials, "#FFFFFF", 0.50, 0.0);
        let zc = d.torso_mid_r * 0.82 + 0.024;
        let top_y = h * 0.97;
        let cy = h * 0.48;
        for sx in [-1.0_f32, 1.0] {
            let a = Vec3::new(sx * 0.075, top_y, zc * 0.30);
            let b = Vec3::new(sx * 0.012, cy + 0.06, zc);
            let diff = b - a;
            let len = diff.length();
            spawn_mesh(
                commands,
                spine,
                cache.cyl.clone(),
                strap_m.clone(),
                Transform::from_translation((a + b) * 0.5)
                    .with_rotation(Quat::from_rotation_arc(Vec3::Y, diff.normalize_or(Vec3::Y)))
                    .with_scale(Vec3::new(0.009, len, 0.009)),
            );
        }
        spawn_mesh(
            commands,
            spine,
            cache.box_mesh.clone(),
            card_m,
            Transform::from_xyz(0.0, cy, zc + 0.003).with_scale(Vec3::new(0.085, 0.11, 0.006)),
        );
    }

    // Neck cylinder
    let neck_r = r * if toy { 0.23 } else { 0.17 };
    spawn_mesh(
        commands,
        spine,
        cache.cyl.clone(),
        skin_m.clone(),
        Transform::from_xyz(0.0, h + d.neck * 0.5 - 0.02, 0.0)
            .with_scale(Vec3::new(neck_r, d.neck + 0.08, neck_r)),
    );

    // Arms: shoulder -> elbow -> mitten hand with thumb
    let build_arm = |commands: &mut Commands, sx: f32, u_rot: Quat, f_rot: Quat| -> (Entity, Entity, Entity) {
        let sh = spawn_group(
            commands,
            spine,
            Transform::from_xyz(sx * d.sh_x, d.sh_y, 0.0).with_rotation(u_rot),
        );
        spawn_mesh(
            commands,
            sh,
            cache.sphere.clone(),
            top_m.clone(),
            Transform::from_scale(Vec3::splat(d.arm_r * 1.01)),
        );
        spawn_mesh(
            commands,
            sh,
            cache.cyl.clone(),
            top_m.clone(),
            Transform::from_xyz(0.0, -d.upper * 0.5, 0.0)
                .with_scale(Vec3::new(d.arm_r, d.upper, d.arm_r)),
        );
        let el = spawn_group(
            commands,
            sh,
            Transform::from_xyz(0.0, -d.upper, 0.0).with_rotation(f_rot),
        );
        spawn_mesh(
            commands,
            el,
            cache.sphere.clone(),
            top_m.clone(),
            Transform::from_scale(Vec3::splat(d.arm_r * 0.96)),
        );
        spawn_mesh(
            commands,
            el,
            cache.cyl.clone(),
            top_m.clone(),
            Transform::from_xyz(0.0, -d.fore * 0.5, 0.0)
                .with_scale(Vec3::new(d.arm_r * 0.94, d.fore, d.arm_r * 0.94)),
        );
        spawn_mesh(
            commands,
            el,
            cache.cyl.clone(),
            top_dark_m.clone(),
            Transform::from_xyz(0.0, -d.fore + 0.025, 0.0)
                .with_scale(Vec3::new(d.arm_r * 1.06, 0.045, d.arm_r * 1.06)),
        );
        let hand = spawn_group(commands, el, Transform::from_xyz(0.0, -d.fore - d.hand_r * 0.55, 0.0));
        spawn_mesh(
            commands,
            hand,
            cache.sphere.clone(),
            skin_m.clone(),
            Transform::from_scale(Vec3::new(
                d.hand_r * 0.90,
                d.hand_r * 1.05,
                d.hand_r * 0.78,
            )),
        );
        spawn_mesh(
            commands,
            hand,
            cache.capsule.clone(),
            skin_m.clone(),
            Transform::from_xyz(-sx * d.hand_r * 0.72, d.hand_r * 0.20, d.hand_r * 0.35)
                .with_rotation(Quat::from_rotation_z(sx * 0.50))
                .with_scale(Vec3::new(
                    d.hand_r * 0.60,
                    d.hand_r * 0.55,
                    d.hand_r * 0.60,
                )),
        );
        (sh, el, hand)
    };
    let (shoulder_l, elbow_l, hand_l) =
        build_arm(commands, -1.0, initial_pose.upper_l, initial_pose.fore_l);
    let (shoulder_r, elbow_r, hand_r) =
        build_arm(commands, 1.0, initial_pose.upper_r, initial_pose.fore_r);

    // Head + sculpted skull + ears + layered eyes + brows + nose + mouth + blush + hair + accessories
    let head = spawn_group(
        commands,
        spine,
        Transform::from_xyz(0.0, d.head_y, 0.0).with_rotation(initial_pose.head_rot),
    );
    spawn_mesh(commands, head, cache.skull_handle(cfg.look), skin_m.clone(), Transform::IDENTITY);

    // Ears
    for sx in [-1.0_f32, 1.0] {
        let ex = sx * r * d.sx * 0.97;
        let er_rad = r * if toy { 0.17 } else { 0.20 };
        spawn_mesh(
            commands,
            head,
            cache.sphere.clone(),
            skin_m.clone(),
            Transform::from_xyz(ex, -r * 0.10, -r * 0.02)
                .with_scale(Vec3::new(er_rad * 0.45, er_rad, er_rad * 0.75)),
        );
        if !toy {
            let ir = r * 0.12;
            spawn_mesh(
                commands,
                head,
                cache.sphere.clone(),
                ear_m.clone(),
                Transform::from_xyz(ex + sx * r * 0.05, -r * 0.10, 0.0)
                    .with_scale(Vec3::new(ir * 0.30, ir * 0.80, ir * 0.55)),
            );
        }
    }

    // Layered Eyes & Brows
    let sclera_m = palette.mat(materials, "#FBFAF7", 0.30, 0.0);
    let limbal_m = palette.mat(materials, "#1E1612", 0.30, 0.0);
    let pupil_m = palette.mat(materials, "#14100E", 0.20, 0.0);
    let bean_eye_m = palette.mat(materials, "#2B1D18", 0.25, 0.0);

    let build_eye = |commands: &mut Commands, sx: f32| -> (Entity, Option<Entity>) {
        if toy {
            let x = sx * r * 0.37;
            let y = -r * 0.10;
            let eye = spawn_group(
                commands,
                head,
                Transform::from_xyz(x, y, d.surf(x, y) - r * 0.02)
                    .with_rotation(Quat::from_rotation_y(sx * 0.36)),
            );
            let ball = spawn_group(
                commands,
                eye,
                Transform::from_scale(Vec3::new(1.0, 1.0 - 0.9 * initial_pose.lid, 1.0)),
            );
            spawn_mesh(
                commands,
                ball,
                cache.sphere.clone(),
                bean_eye_m.clone(),
                Transform::from_scale(Vec3::new(
                    r * 0.12 * 0.84,
                    r * 0.12 * 1.12,
                    r * 0.12 * 0.42,
                )),
            );
            spawn_mesh(
                commands,
                ball,
                cache.sphere.clone(),
                white_unlit.clone(),
                Transform::from_xyz(-r * 0.03, r * 0.05, r * 0.04)
                    .with_scale(Vec3::new(r * 0.042, r * 0.042, r * 0.042 * 0.40)),
            );
            spawn_mesh(
                commands,
                ball,
                cache.sphere.clone(),
                white_unlit.clone(),
                Transform::from_xyz(r * 0.035, -r * 0.045, r * 0.042)
                    .with_scale(Vec3::new(r * 0.02, r * 0.02, r * 0.02 * 0.40)),
            );
            let bx = sx * r * 0.37;
            let by = r * 0.11;
            spawn_mesh(
                commands,
                head,
                cache.arc_brow.clone(),
                brow_m.clone(),
                Transform::from_xyz(bx, by, d.surf(bx, by) - r * 0.01)
                    .with_rotation(Quat::from_euler(EulerRot::XYZ, 0.0, sx * 0.36, PI * 0.2))
                    .with_scale(Vec3::splat(r * 0.08)),
            );
            (ball, None)
        } else {
            let x = sx * r * 0.35;
            let y = -r * 0.06;
            let e = r * 0.21;
            let k = e / 0.068;
            let eye = spawn_group(
                commands,
                head,
                Transform::from_xyz(x, y, d.surf(x, y) + r * 0.02 - e * 0.55)
                    .with_rotation(Quat::from_rotation_y(sx * 0.32)),
            );
            let ball = spawn_group(
                commands,
                eye,
                Transform::from_scale(Vec3::new(1.0, 1.0 - 0.9 * initial_pose.lid, 1.0)),
            );
            // Sclera
            spawn_mesh(
                commands,
                ball,
                cache.sphere.clone(),
                sclera_m.clone(),
                Transform::from_scale(Vec3::new(e, e * 1.06, e * 0.55)),
            );
            // Dark limbal ring
            spawn_mesh(
                commands,
                ball,
                cache.sphere.clone(),
                limbal_m.clone(),
                Transform::from_xyz(0.0, -0.004 * k, 0.021 * k).with_scale(Vec3::new(
                    0.052 * k,
                    0.052 * k * 1.06,
                    0.052 * k * 0.30,
                )),
            );
            // Colored iris
            spawn_mesh(
                commands,
                ball,
                cache.sphere.clone(),
                iris_m.clone(),
                Transform::from_xyz(0.0, -0.004 * k, 0.025 * k).with_scale(Vec3::new(
                    0.048 * k,
                    0.048 * k * 1.06,
                    0.048 * k * 0.30,
                )),
            );
            // Lower iris specular crescent
            spawn_mesh(
                commands,
                ball,
                cache.sphere.clone(),
                iris_glow_m.clone(),
                Transform::from_xyz(0.0, -0.026 * k, 0.031 * k).with_scale(Vec3::new(
                    0.03 * k * 1.2,
                    0.03 * k * 0.6,
                    0.03 * k * 0.25,
                )),
            );
            // Deep black pupil
            spawn_mesh(
                commands,
                ball,
                cache.sphere.clone(),
                pupil_m.clone(),
                Transform::from_xyz(0.0, -0.006 * k, 0.035 * k).with_scale(Vec3::new(
                    0.025 * k,
                    0.025 * k * 1.05,
                    0.025 * k * 0.30,
                )),
            );
            // Twin white specular catchlights
            spawn_mesh(
                commands,
                ball,
                cache.sphere.clone(),
                white_unlit.clone(),
                Transform::from_xyz(-0.016 * k, 0.022 * k, 0.042 * k).with_scale(Vec3::new(
                    0.014 * k,
                    0.014 * k,
                    0.014 * k * 0.40,
                )),
            );
            spawn_mesh(
                commands,
                ball,
                cache.sphere.clone(),
                white_unlit.clone(),
                Transform::from_xyz(0.014 * k, -0.020 * k, 0.041 * k).with_scale(Vec3::new(
                    0.007 * k,
                    0.007 * k,
                    0.007 * k * 0.40,
                )),
            );
            // Upper lash line + outer wing flick
            let lash = spawn_mesh(
                commands,
                eye,
                cache.arc_pi_thin.clone(),
                lash_m.clone(),
                Transform::from_xyz(0.0, -0.02 * k * initial_pose.lid, 0.012 * k).with_scale(
                    Vec3::new(
                        0.07 * k,
                        0.07 * k * 1.06 * (1.0 - 0.58 * initial_pose.lid),
                        0.07 * k * 0.70,
                    ),
                ),
            );
            spawn_mesh(
                commands,
                lash,
                cache.capsule.clone(),
                lash_m.clone(),
                Transform::from_xyz(sx * 1.07, 0.17, 0.0)
                    .with_rotation(Quat::from_rotation_z(sx * -1.0))
                    .with_scale(Vec3::new(0.22, 0.32, 0.22)),
            );
            // Upper eyelid crease
            spawn_mesh(
                commands,
                eye,
                cache.arc_crease.clone(),
                crease_m.clone(),
                Transform::from_xyz(0.0, 0.022 * k, 0.014 * k)
                    .with_rotation(Quat::from_rotation_z(PI * 0.15))
                    .with_scale(Vec3::new(0.068 * k, 0.068 * k * 1.06, 0.068 * k * 0.70)),
            );
            // Thick arched eyebrow
            let bx = sx * r * 0.35;
            let by = r * 0.21;
            spawn_mesh(
                commands,
                head,
                cache.arc_brow.clone(),
                brow_m.clone(),
                Transform::from_xyz(bx, by - r * 0.10, d.surf(bx, by) - r * 0.03)
                    .with_rotation(Quat::from_euler(EulerRot::XYZ, 0.0, sx * 0.32, PI * 0.2))
                    .with_scale(Vec3::splat(r * 0.17)),
            );
            (ball, Some(lash))
        }
    };
    let (eye_ball_l, lash_l) = build_eye(commands, -1.0);
    let (eye_ball_r, lash_r) = build_eye(commands, 1.0);

    // Nose
    let nose_y = if toy { -r * 0.25 } else { -r * 0.24 };
    let nr_rad = r * if toy { 0.055 } else { 0.08 };
    spawn_mesh(
        commands,
        head,
        cache.sphere.clone(),
        nose_m,
        Transform::from_xyz(0.0, nose_y, d.surf(0.0, nose_y) - r * 0.01)
            .with_scale(Vec3::new(nr_rad * 1.1, nr_rad * 0.85, nr_rad * 0.8)),
    );

    // Expressive Mouths (smile & open 'o')
    let m_y = if toy { -r * 0.38 } else { -r * 0.42 };
    let smile_r = r * if toy { 0.07 } else { 0.09 };
    let mouth_smile = commands
        .spawn((
            WorldEntity,
            Mesh3d(cache.arc_pi_thick.clone()),
            MeshMaterial3d(lip_m),
            Transform::from_xyz(0.0, m_y + r * 0.04, d.surf(0.0, m_y) - r * 0.005)
                .with_rotation(Quat::from_rotation_z(PI))
                .with_scale(Vec3::splat(smile_r)),
            if initial_pose.mouth_o {
                Visibility::Hidden
            } else {
                Visibility::Visible
            },
        ))
        .id();
    commands.entity(head).add_child(mouth_smile);

    let mouth_o = commands
        .spawn((
            WorldEntity,
            Mesh3d(cache.sphere.clone()),
            MeshMaterial3d(mouth_o_m),
            Transform::from_xyz(0.0, m_y, d.surf(0.0, m_y) - r * 0.01).with_scale(Vec3::new(
                r * 0.07 * 0.9,
                r * 0.07 * 1.1,
                r * 0.07 * 0.4,
            )),
            if initial_pose.mouth_o {
                Visibility::Visible
            } else {
                Visibility::Hidden
            },
        ))
        .id();
    commands.entity(head).add_child(mouth_o);

    // Rosy cheeks blush pads
    for sx in [-1.0_f32, 1.0] {
        let bx = sx * r * 0.52;
        let by = -r * 0.27;
        let br_rad = r * 0.14;
        spawn_mesh(
            commands,
            head,
            cache.sphere.clone(),
            blush_m.clone(),
            Transform::from_xyz(bx, by, d.surf(bx, by) + r * 0.02)
                .with_rotation(Quat::from_rotation_y(sx * 0.55))
                .with_scale(Vec3::new(br_rad * 1.5, br_rad * 0.75, br_rad * 0.3)),
        );
    }

    // Sculpted Hair Shell + style-specific swept locks / bun / ponytail / ahoge
    let under_hat = matches!(cfg.accessory, Some("cap" | "beanie"));
    let mut hair_scale = Vec3::new(d.sx, d.sy, d.sz);
    if under_hat {
        hair_scale *= 0.97;
        hair_scale.y *= 0.84;
    }
    spawn_mesh(
        commands,
        head,
        cache.hair_shell_handle(cfg.look, cfg.hair_style),
        hair_m.clone(),
        Transform::from_xyz(0.0, r * 0.02, -r * 0.02)
            .with_rotation(Quat::from_rotation_x(-0.08))
            .with_scale(hair_scale),
    );
    match cfg.hair_style {
        HairStyle::Spiky => {
            spawn_mesh(
                commands,
                head,
                cache.spiky_cluster_handle(cfg.look),
                hair_m.clone(),
                Transform::IDENTITY,
            );
        }
        HairStyle::Ponytail => {
            spawn_mesh(
                commands,
                head,
                cache.torus_thick.clone(),
                top_dark_m.clone(),
                Transform::from_xyz(0.0, r * 0.16, -r * 1.02)
                    .with_rotation(Quat::from_rotation_x(0.30))
                    .with_scale(Vec3::splat(r * 0.14)),
            );
            spawn_mesh(
                commands,
                head,
                cache.ponytail_handle(cfg.look),
                hair_m.clone(),
                Transform::IDENTITY,
            );
        }
        HairStyle::Bun => {
            spawn_mesh(
                commands,
                head,
                cache.sphere.clone(),
                hair_m.clone(),
                Transform::from_xyz(0.0, r * 1.05, -r * 0.35)
                    .with_scale(Vec3::new(r * 0.38, r * 0.38 * 0.9, r * 0.38)),
            );
            spawn_mesh(
                commands,
                head,
                cache.torus_thick.clone(),
                hair_m.clone(),
                Transform::from_xyz(0.0, r * 1.04, -r * 0.35)
                    .with_rotation(Quat::from_rotation_x(FRAC_PI_2 - 0.35))
                    .with_scale(Vec3::new(r * 0.30, r * 0.30, r * 0.27)),
            );
            spawn_mesh(
                commands,
                head,
                cache.torus_med.clone(),
                top_dark_m.clone(),
                Transform::from_xyz(0.0, r * 0.84, -r * 0.26)
                    .with_rotation(Quat::from_rotation_x(FRAC_PI_2 - 0.35))
                    .with_scale(Vec3::splat(r * 0.25)),
            );
        }
        _ => {}
    }
    if cfg.seed % 3 == 0
        && !matches!(cfg.hair_style, HairStyle::Spiky | HairStyle::Bun)
        && !under_hat
    {
        spawn_mesh(
            commands,
            head,
            cache.ahoge_handle(cfg.look),
            hair_m,
            Transform::IDENTITY,
        );
    }

    // Accessories (Headphones / Glasses / Beanie / Cap)
    let hr = r * if toy { 1.10 } else { 1.12 };
    match cfg.accessory {
        Some("headphones") => {
            let hp_m = palette.mat(materials, "#2B2F36", 0.35, 0.30);
            spawn_mesh(
                commands,
                head,
                cache.arc_pi_thin.clone(),
                hp_m.clone(),
                Transform::from_xyz(0.0, r * 0.05, 0.0).with_scale(Vec3::splat(hr * 1.08)),
            );
            for sx in [-1.0_f32, 1.0] {
                spawn_mesh(
                    commands,
                    head,
                    cache.cyl.clone(),
                    hp_m.clone(),
                    Transform::from_xyz(sx * hr * 1.04, -r * 0.08, 0.0)
                        .with_rotation(Quat::from_rotation_z(FRAC_PI_2))
                        .with_scale(Vec3::new(r * 0.27, r * 0.18, r * 0.27)),
                );
                spawn_mesh(
                    commands,
                    head,
                    cache.cyl.clone(),
                    top_m.clone(),
                    Transform::from_xyz(sx * hr * 1.14, -r * 0.08, 0.0)
                        .with_rotation(Quat::from_rotation_z(FRAC_PI_2))
                        .with_scale(Vec3::new(r * 0.20, r * 0.05, r * 0.20)),
                );
            }
        }
        Some("glasses") => {
            let frame_m = palette.mat(materials, "#2A211B", 0.35, 0.30);
            let gx = r * if toy { 0.37 } else { 0.35 };
            let gy = if toy { -r * 0.10 } else { -r * 0.06 };
            let gr = r * if toy { 0.20 } else { 0.24 };
            for sx in [-1.0_f32, 1.0] {
                spawn_mesh(
                    commands,
                    head,
                    cache.torus_thin.clone(),
                    frame_m.clone(),
                    Transform::from_xyz(sx * gx, gy, d.surf(gx, gy) + r * 0.06)
                        .with_rotation(Quat::from_rotation_y(sx * 0.25))
                        .with_scale(Vec3::splat(gr)),
                );
                spawn_mesh(
                    commands,
                    head,
                    cache.box_mesh.clone(),
                    frame_m.clone(),
                    Transform::from_xyz(sx * r * d.sx * 0.72, gy + r * 0.02, r * 0.48)
                        .with_rotation(Quat::from_rotation_y(sx * -0.28))
                        .with_scale(Vec3::new(r * 0.03, r * 0.03, r * 0.75)),
                );
            }
            spawn_mesh(
                commands,
                head,
                cache.arc_pi_thick.clone(),
                frame_m,
                Transform::from_xyz(0.0, gy + r * 0.01, d.surf(0.0, gy) + r * 0.07)
                    .with_scale(Vec3::splat(r * 0.07)),
            );
        }
        Some("beanie") => {
            let (bmr, bmg, bmb) = darken_rgb(cfg.shirt, 0.80);
            let bm = palette.mat_rgb(materials, bmr, bmg, bmb, fabric_rough, 0.0, false);
            spawn_mesh(
                commands,
                head,
                cache.hemi_dome.clone(),
                bm.clone(),
                Transform::from_xyz(0.0, r * 0.04, -r * 0.03)
                    .with_rotation(Quat::from_rotation_x(-0.28))
                    .with_scale(Vec3::new(hr * 1.12 * d.sx, hr * 1.12 * d.sy * 1.04, hr * 1.12 * d.sz)),
            );
            spawn_mesh(
                commands,
                head,
                cache.torus_thin.clone(),
                bm,
                Transform::from_xyz(0.0, r * 0.02 + hr * 0.28, -r * 0.03 - hr * 0.03)
                    .with_rotation(Quat::from_rotation_x(FRAC_PI_2 - 0.28))
                    .with_scale(Vec3::new(hr * 1.10 * d.sx, hr * 1.10, hr * 1.10 * d.sz)),
            );
            spawn_mesh(
                commands,
                head,
                cache.sphere.clone(),
                sole_m.clone(),
                Transform::from_xyz(0.0, hr * 1.18, -r * 0.32).with_scale(Vec3::splat(r * 0.20)),
            );
        }
        Some("cap") => {
            let (cmr, cmg, cmb) = darken_rgb(cfg.shirt, 0.85);
            let cm = palette.mat_rgb(materials, cmr, cmg, cmb, fabric_rough, 0.0, false);
            let (brr, brg, brb) = darken_rgb(cfg.shirt, 0.70);
            let brim_m = palette.mat_rgb(materials, brr, brg, brb, 0.80, 0.0, false);
            let cap_g = spawn_group(
                commands,
                head,
                Transform::from_xyz(0.0, r * 0.06, -r * 0.03)
                    .with_rotation(Quat::from_rotation_x(-0.18)),
            );
            spawn_mesh(
                commands,
                cap_g,
                cache.hemi_dome.clone(),
                cm.clone(),
                Transform::from_scale(Vec3::new(
                    hr * 1.08 * d.sx,
                    hr * 1.08 * d.sy * 0.86,
                    hr * 1.08 * d.sz,
                )),
            );
            spawn_mesh(
                commands,
                cap_g,
                cache.half_brim.clone(),
                brim_m,
                Transform::from_xyz(0.0, r * 0.02, hr * 0.78)
                    .with_rotation(Quat::from_rotation_x(0.10))
                    .with_scale(Vec3::new(hr * 0.66, r * 0.06, hr * 0.66)),
            );
            spawn_mesh(
                commands,
                cap_g,
                cache.sphere.clone(),
                cm,
                Transform::from_xyz(0.0, hr * 1.08 * d.sy * 0.86, 0.0)
                    .with_scale(Vec3::splat(r * 0.08)),
            );
        }
        _ => {}
    }

    // Optional pickleball sun visor
    if cfg.has_visor {
        let vm = palette.mat(materials, "#F6F6F2", 0.70, 0.0);
        let brim_m = palette.mat(materials, "#2B3A55", 0.70, 0.0);
        let vz = spawn_group(
            commands,
            head,
            Transform::from_xyz(0.0, r * 0.06, -r * 0.03)
                .with_rotation(Quat::from_rotation_x(-0.12)),
        );
        spawn_mesh(
            commands,
            vz,
            cache.cyl_open.clone(),
            vm,
            Transform::from_xyz(0.0, r * 0.08, 0.0).with_scale(Vec3::new(
                hr * 1.05 * d.sx,
                r * 0.20,
                hr * 1.05 * d.sz,
            )),
        );
        spawn_mesh(
            commands,
            vz,
            cache.half_brim.clone(),
            brim_m,
            Transform::from_xyz(0.0, 0.0, hr * 0.78)
                .with_rotation(Quat::from_rotation_x(0.28))
                .with_scale(Vec3::new(hr * 0.70, r * 0.05, hr * 0.70)),
        );
    }

    // Optional Santa hat (Christmas season)
    if cfg.has_santa_hat {
        let red_m = palette.mat(materials, "#C8283A", 0.85, 0.0);
        let fur_m = palette.mat(materials, "#F7F4EE", 0.95, 0.0);
        let hat_g = spawn_group(
            commands,
            head,
            Transform::from_xyz(0.0, r * 0.62, -r * 0.06)
                .with_rotation(Quat::from_euler(EulerRot::XYZ, -0.22, 0.0, 0.12)),
        );
        spawn_mesh(
            commands,
            hat_g,
            cache.cone.clone(),
            red_m,
            Transform::from_xyz(0.0, r * 0.45, 0.0).with_scale(Vec3::new(r * 1.65, r * 0.95, r * 1.65)),
        );
        spawn_mesh(
            commands,
            hat_g,
            cache.torus_med.clone(),
            fur_m.clone(),
            Transform::from_rotation(Quat::from_rotation_x(FRAC_PI_2))
                .with_scale(Vec3::splat(r * 0.86)),
        );
        spawn_mesh(
            commands,
            hat_g,
            cache.sphere.clone(),
            fur_m,
            Transform::from_xyz(r * 0.35, r * 0.88, -r * 0.10).with_scale(Vec3::splat(r * 0.15)),
        );
    }

    // Subtle glowing status halo dot above head (kept as direct child of root for live sync)
    let halo_y = d.hip_y + d.head_y + r * 1.48;
    let halo = commands
        .spawn((
            WorldEntity,
            StatusHalo,
            Mesh3d(cache.sphere.clone()),
            MeshMaterial3d(badge_m),
            Transform::from_xyz(0.0, halo_y, 0.0).with_scale(Vec3::splat(0.055)),
        ))
        .id();
    commands.entity(root).add_child(halo);

    commands.entity(root).insert(ChibiRig {
        look: cfg.look,
        spine,
        torso,
        head,
        hip_l,
        hip_r,
        knee_l,
        knee_r,
        shoulder_l,
        shoulder_r,
        elbow_l,
        elbow_r,
        hand_l,
        hand_r,
        eye_ball_l,
        eye_ball_r,
        lash_l,
        lash_r,
        mouth_smile,
        mouth_o,
        shadow,
    });

    root
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn chibi_solve_two_bone_ik_reaches_keyboard_and_desk_targets_within_1cm() {
        let upper = 0.20_f32;
        let fore = 0.18 + 0.062 * 0.6;
        let targets = [
            // Right hand typing on keyboard in spine-local space (reachable dist = 0.38m < 0.417m)
            (
                Vec3::new(0.236, 0.49, 0.0),
                Vec3::new(0.11, 0.28, 0.30),
                Vec3::new(0.32, 0.20, -0.30),
            ),
            // Left hand typing on keyboard
            (
                Vec3::new(-0.236, 0.49, 0.0),
                Vec3::new(-0.11, 0.28, 0.30),
                Vec3::new(-0.32, 0.20, -0.30),
            ),
            // Right hand sipping coffee near mouth (dist = 0.36m < 0.417m)
            (
                Vec3::new(0.236, 0.49, 0.0),
                Vec3::new(0.06, 0.62, 0.28),
                Vec3::new(0.40, 0.35, -0.25),
            ),
            // Toy proportions desk reach (dist = 0.30m < 0.361m)
            (
                Vec3::new(0.215, 0.37, 0.0),
                Vec3::new(0.11, 0.22, 0.24),
                Vec3::new(0.30, 0.15, -0.28),
            ),
        ];

        for (i, (shoulder, target, pole)) in targets.iter().enumerate() {
            let (u_len, f_len) = if i == 3 {
                (0.17, 0.15 + 0.068 * 0.6)
            } else {
                (upper, fore)
            };
            let (upper_q, fore_q) = solve_two_bone_ik(*shoulder, *target, *pole, u_len, f_len);
            assert!(upper_q.is_finite() && upper_q.is_normalized());
            assert!(fore_q.is_finite() && fore_q.is_normalized());

            let elbow = *shoulder + upper_q * Vec3::new(0.0, -u_len, 0.0);
            let hand = elbow + (upper_q * fore_q) * Vec3::new(0.0, -f_len, 0.0);
            let err = hand.distance(*target);
            assert!(
                err < 0.01,
                "target[{i}] hand error {err:.5}m exceeds 0.01m (hand={hand:?}, target={target:?})"
            );
        }
    }
}
