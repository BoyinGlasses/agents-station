use crate::palette::{WorldEntity, WorldPalette};
use bevy::prelude::*;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum ChibiLook {
    Toy,
    #[default]
    Pixar,
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

#[derive(Debug, Clone)]
pub struct ChibiConfig {
    pub name: String,
    pub agent: String,
    pub skin: &'static str,
    pub shirt: &'static str,
    pub pants: &'static str,
    pub hair: &'static str,
    pub shoes: &'static str,
    pub accessory: Option<&'static str>, // "headphones" | "glasses" | "cap"
    pub look: ChibiLook,
}

impl ChibiConfig {
    pub fn for_agent(agent: &str, name: &str, seed: usize) -> Self {
        const SKINS: [&str; 5] = ["#F7CBAA", "#F2BE98", "#E6AC82", "#F9D6BE", "#D49870"];
        const HAIRS: [&str; 6] = ["#1E1E24", "#3B2618", "#5A3A2A", "#2C2228", "#6B4A33", "#232836"];
        const PANTS: [&str; 4] = ["#33415C", "#3A3A44", "#2E3B4E", "#4A4458"];
        let shirt = match agent {
            "claude" => "#D97757",
            "codex" => "#3E9B74",
            "opencode" => "#5F8FD8",
            "zcode" => "#9B6BCE",
            "antigravity" => "#E09A3E",
            _ => ["#E8865F", "#5F8FD8", "#4CAF7D", "#A66BCB", "#E0AE2A"][seed % 5],
        };
        let acc = match seed % 4 {
            0 => Some("headphones"),
            1 => Some("glasses"),
            2 => Some("cap"),
            _ => None,
        };
        Self {
            name: name.to_owned(),
            agent: agent.to_owned(),
            skin: SKINS[seed % SKINS.len()],
            shirt,
            pants: PANTS[seed % PANTS.len()],
            hair: HAIRS[seed % HAIRS.len()],
            shoes: "#2D2A2E",
            accessory: acc,
            look: ChibiLook::Pixar,
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

/// Spawns a hierarchical multi-part Chibi character using shared `WorldPalette` meshes.
pub fn spawn_chibi(
    commands: &mut Commands,
    palette: &mut WorldPalette,
    materials: &mut Assets<StandardMaterial>,
    cfg: &ChibiConfig,
    pos: Vec3,
    yaw: f32,
    anim: WorkerAnim,
    phase: f32,
    sitting: bool,
) -> Entity {
    let skin_m = palette.mat(materials, cfg.skin, 0.52, 0.0);
    let shirt_m = palette.mat(materials, cfg.shirt, 0.78, 0.0);
    let pants_m = palette.mat(materials, cfg.pants, 0.85, 0.0);
    let hair_m = palette.mat(materials, cfg.hair, 0.45, 0.05);
    let shoe_m = palette.mat(materials, cfg.shoes, 0.5, 0.0);
    let eye_m = palette.mat(materials, "#1B1918", 0.25, 0.0);
    let cheek_m = palette.mat(materials, "#F598A8", 0.8, 0.0);
    let acc_m = palette.mat(materials, "#2B2F36", 0.35, 0.4);
    let badge_m = halo_material_for_anim(palette, materials, anim);

    let (head_r, torso_h, hip_y) = match cfg.look {
        ChibiLook::Toy => (0.48_f32, 0.44_f32, if sitting { 0.45 } else { 0.4 }),
        ChibiLook::Pixar => (0.42_f32, 0.52_f32, if sitting { 0.48 } else { 0.5 }),
    };

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
            },
            Transform::from_translation(pos).with_rotation(Quat::from_rotation_y(yaw)),
            Visibility::Visible,
        ))
        .id();

    let mut add_part = |mesh: Handle<Mesh>, mat: Handle<StandardMaterial>, p: Vec3, s: Vec3| {
        let child = commands
            .spawn((
                WorldEntity,
                Mesh3d(mesh),
                MeshMaterial3d(mat),
                Transform::from_translation(p).with_scale(s),
            ))
            .id();
        commands.entity(root).add_child(child);
    };

    // Legs & sneakers
    for sx in [-0.11_f32, 0.11] {
        if sitting {
            add_part(palette.unit_cyl.clone(), pants_m.clone(), Vec3::new(sx, hip_y - 0.02, 0.14), Vec3::new(0.14, 0.24, 0.22));
            add_part(palette.unit_cyl.clone(), pants_m.clone(), Vec3::new(sx, hip_y - 0.2, 0.22), Vec3::new(0.13, 0.26, 0.13));
            add_part(palette.unit_sphere.clone(), shoe_m.clone(), Vec3::new(sx, 0.09, 0.26), Vec3::new(0.16, 0.12, 0.22));
        } else {
            add_part(palette.unit_cyl.clone(), pants_m.clone(), Vec3::new(sx, hip_y * 0.5, 0.0), Vec3::new(0.14, hip_y, 0.14));
            add_part(palette.unit_sphere.clone(), shoe_m.clone(), Vec3::new(sx, 0.08, 0.05), Vec3::new(0.16, 0.12, 0.22));
        }
    }

    // Torso
    let torso_y = hip_y + torso_h * 0.5;
    add_part(palette.unit_cyl.clone(), shirt_m.clone(), Vec3::new(0.0, torso_y, 0.0), Vec3::new(0.42, torso_h, 0.34));

    // Arms & hands (reaching forward toward keyboard when sitting)
    for sx in [-0.24_f32, 0.24] {
        let az = if sitting { 0.18 } else { 0.02 };
        let ay = if sitting { torso_y + 0.02 } else { torso_y - 0.04 };
        add_part(palette.unit_cyl.clone(), shirt_m.clone(), Vec3::new(sx, ay, az * 0.5), Vec3::new(0.11, 0.28, 0.12));
        add_part(palette.unit_sphere.clone(), skin_m.clone(), Vec3::new(sx * 0.85, ay - 0.04, az + 0.09), Vec3::splat(0.11));
    }

    // Head, hair cap, eyes, rosy cheeks
    let head_y = hip_y + torso_h + head_r * 0.62;
    add_part(palette.unit_sphere.clone(), skin_m, Vec3::new(0.0, head_y, 0.0), Vec3::new(head_r * 1.06, head_r, head_r));
    add_part(
        palette.unit_sphere.clone(),
        hair_m.clone(),
        Vec3::new(0.0, head_y + head_r * 0.16, -head_r * 0.06),
        Vec3::new(head_r * 1.14, head_r * 0.94, head_r * 1.08),
    );
    // Bean eyes + blush cheeks on +Z face
    for sx in [-0.09_f32, 0.09] {
        add_part(palette.unit_sphere.clone(), eye_m.clone(), Vec3::new(sx, head_y - 0.02, head_r * 0.46), Vec3::new(0.055, 0.075, 0.035));
        add_part(palette.unit_sphere.clone(), cheek_m.clone(), Vec3::new(sx * 1.65, head_y - 0.07, head_r * 0.42), Vec3::new(0.075, 0.045, 0.02));
    }

    // Optional accessory
    match cfg.accessory {
        Some("headphones") => {
            add_part(palette.unit_torus.clone(), acc_m.clone(), Vec3::new(0.0, head_y + 0.08, 0.0), Vec3::new(head_r * 1.25, head_r * 1.25, 0.14));
            for sx in [-head_r * 0.56, head_r * 0.56] {
                add_part(palette.unit_cyl.clone(), acc_m.clone(), Vec3::new(sx, head_y, 0.0), Vec3::new(0.08, 0.14, 0.14));
            }
        }
        Some("glasses") => {
            for sx in [-0.095_f32, 0.095] {
                add_part(palette.unit_box.clone(), acc_m.clone(), Vec3::new(sx, head_y - 0.02, head_r * 0.5), Vec3::new(0.11, 0.08, 0.02));
            }
        }
        Some("cap") => {
            let cap_m = palette.mat(materials, cfg.shirt, 0.7, 0.0);
            add_part(palette.unit_box.clone(), cap_m, Vec3::new(0.0, head_y + head_r * 0.34, head_r * 0.3), Vec3::new(0.32, 0.04, 0.24));
        }
        _ => {}
    }

    // Subtle glowing status halo dot above head (work=green, wait=amber, sleep=lavender)
    let halo = commands
        .spawn((
            WorldEntity,
            StatusHalo,
            Mesh3d(palette.unit_sphere.clone()),
            MeshMaterial3d(badge_m),
            Transform::from_translation(Vec3::new(0.0, head_y + head_r * 0.72, 0.0))
                .with_scale(Vec3::splat(0.11)),
        ))
        .id();
    commands.entity(root).add_child(halo);

    root
}
