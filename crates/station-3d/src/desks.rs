use crate::chibi::{ChibiWorker, WorkerAnim};
use crate::layout::LayoutSpec;
use crate::lights::LightingState;
use crate::palette::{
    WorldEntity, WorldPalette, spawn_box_rot, spawn_cone_rot, spawn_cyl, spawn_cyl_rot,
    spawn_quad, spawn_sphere, spawn_torus_rot,
};
use crate::people::{LiveSessionWorker, PreviewCastWorker};
use crate::rooms::{PlantKind, spawn_typed_plant};
use bevy::math::Affine2;
use bevy::prelude::*;
use station_canvas2d::FontEngine;
use station_canvas2d::office_surfaces::{
    ALL_SCREEN_KINDS, ScreenKind, draw_blanket_plaid, draw_desk_screen, draw_framed_photo,
};
use std::collections::HashMap;
use std::f32::consts::{FRAC_PI_2, PI, TAU};

/// Exact Oracle `desks.js:98` laptop lid tilt angle in radians.
pub const LAPTOP_LID_TILT_RAD: f32 = 0.28;
/// Exact Oracle `desks.js:155` uncommitted git file thresholds for the 4 desk-mess stages.
pub const MESS_THRESHOLDS: [usize; 4] = [1, 6, 16, 41];

/// Exact Oracle `desks.js:50-51` worker coat & backpack palette.
pub const COAT_COLORS: [&str; 8] = [
    "#2F3B52", "#6B4F3A", "#46553F", "#7D2E3A", "#C2A97E", "#26292E", "#4F6D8F", "#8A8F98",
];
pub const BAG_COLORS: [&str; 7] = [
    "#2B2F36", "#3F7CC4", "#E8742A", "#5E7A3A", "#8C4FA8", "#A0A8B2", "#C8553D",
];
/// Exact Oracle `bedding.js:31` nap blanket colors.
pub const BLANKET_COLORS: [&str; 6] = [
    "#F4C95D", "#F28FA8", "#F7A05A", "#9ED9A8", "#C7A6F2", "#7FD3D9",
];

#[derive(Component, Debug, Clone, Copy)]
pub struct DeskSlot {
    pub index: usize,
}

#[derive(Component, Debug, Clone, Copy)]
pub struct LaptopLid {
    pub desk_index: usize,
}

#[derive(Component, Debug, Clone, Copy)]
pub struct DeskScreen {
    pub desk_index: usize,
    pub is_laptop: bool,
}

#[derive(Component, Debug, Clone, Copy)]
pub struct ChairBackPivot {
    pub desk_index: usize,
    pub angle_x: f32,
}

#[derive(Component, Debug, Clone, Copy)]
pub struct DeskKit {
    pub desk_index: usize,
    pub coat_root: Entity,
    pub bag_root: Entity,
    pub occupied: bool,
}

#[derive(Component, Debug, Clone, Copy)]
pub struct DeskBedding {
    pub desk_index: usize,
    pub pillow: Entity,
    pub blanket: Entity,
    pub folded: Entity,
    pub napping: bool,
    pub progress: f32,
}

#[derive(Component, Debug, Clone)]
pub struct DeskMess {
    pub desk_index: usize,
    pub dirty_files: usize,
    pub level: usize,
    pub stage_roots: [Entity; 4],
}

#[derive(Component, Debug, Clone)]
pub struct DeskLampBulb {
    pub material: Handle<StandardMaterial>,
}

#[derive(Resource, Default)]
pub struct ScreenMaterials {
    pub by_kind: HashMap<ScreenKind, Handle<StandardMaterial>>,
}

/// Returns the 0..=4 desk mess level for `dirty` uncommitted git files (`desks.js:178`).
pub fn mess_level_for_dirty(dirty: usize) -> usize {
    MESS_THRESHOLDS.iter().filter(|&&th| dirty >= th).count()
}

/// Toggles the 4 `DeskMess` stage roots strictly according to `MESS_THRESHOLDS` (`desks.js:179-183`).
pub fn sync_desk_mess(
    mess: &mut DeskMess,
    dirty: usize,
    vis_q: &mut Query<&mut Visibility>,
) {
    let lv = mess_level_for_dirty(dirty);
    mess.dirty_files = dirty;
    if mess.level == lv {
        return;
    }
    mess.level = lv;
    for (k, &root) in mess.stage_roots.iter().enumerate() {
        if let Ok(mut vis) = vis_q.get_mut(root) {
            *vis = if k < lv {
                Visibility::Inherited
            } else {
                Visibility::Hidden
            };
        }
    }
}

/// Toggles worker coat (on chair back) and backpack (beside desk) visibility (`desks.js:67-72`).
pub fn sync_kit(kit: &mut DeskKit, occupied: bool, vis_q: &mut Query<&mut Visibility>) {
    if kit.occupied == occupied {
        return;
    }
    kit.occupied = occupied;
    let vis = if occupied {
        Visibility::Inherited
    } else {
        Visibility::Hidden
    };
    if let Ok(mut v) = vis_q.get_mut(kit.coat_root) {
        *v = vis;
    }
    if let Ok(mut v) = vis_q.get_mut(kit.bag_root) {
        *v = vis;
    }
}

/// Maps a live session's canonical status (`"work"`/`"wait"`/`"rest"`/`"approve"`/`"error"`)
/// and active tool to the corresponding desk monitor `ScreenKind`.
pub fn screen_kind_for_session(
    status: &str,
    tool_kind: Option<&str>,
    tool_label: Option<&str>,
) -> ScreenKind {
    match status {
        "wait" | "waiting" | "approve" | "approval" | "error" | "err" => return ScreenKind::Wait,
        "rest" | "idle" | "sleep" => return ScreenKind::Sleep,
        _ => {}
    }
    let kind_l = tool_kind.unwrap_or("").to_ascii_lowercase();
    let label_l = tool_label.unwrap_or("").to_ascii_lowercase();
    match kind_l.as_str() {
        "bash" | "term" | "terminal" | "shell" | "cmd" | "exec" | "deploy" => ScreenKind::Term,
        "read" | "grep" | "glob" | "search" | "skill" => ScreenKind::Read,
        "web" | "fetch" | "webfetch" | "websearch" | "mcp" => ScreenKind::Web,
        "chat" | "plan" | "ask" | "agent" | "task" => ScreenKind::Chat,
        "err" | "error" | "fail" | "approve" | "wait" => ScreenKind::Wait,
        "edit" | "write" | "multiedit" | "code" => ScreenKind::Code,
        "idle" | "sleep" | "rest" => ScreenKind::Sleep,
        _ => {
            if label_l.contains("bash")
                || label_l.contains("cargo")
                || label_l.contains("npm")
                || label_l.contains("term")
                || label_l.contains("deploy")
            {
                ScreenKind::Term
            } else if label_l.contains("read")
                || label_l.contains("grep")
                || label_l.contains("search")
                || label_l.contains("inspect")
            {
                ScreenKind::Read
            } else if label_l.contains("web") || label_l.contains("http") || label_l.contains("fetch")
            {
                ScreenKind::Web
            } else if label_l.contains("plan") || label_l.contains("ask") || label_l.contains("chat")
            {
                ScreenKind::Chat
            } else if label_l.contains("err") || label_l.contains("fail") || label_l.contains("approve")
            {
                ScreenKind::Wait
            } else {
                ScreenKind::Code
            }
        }
    }
}

/// Deterministic seeded PRNG matching `trinkets.js:12-14` (`propSeed = 77`).
struct TrinketRng(u64);
impl TrinketRng {
    fn next_f32(&mut self) -> f32 {
        self.0 = (self.0 * 16_807) % 2_147_483_647;
        self.0 as f32 / 2_147_483_647.0
    }
    fn pick<'a, T>(&mut self, slice: &'a [T]) -> &'a T {
        let idx = ((self.next_f32() * slice.len() as f32).floor() as usize).min(slice.len() - 1);
        &slice[idx]
    }
}

#[derive(Clone, Copy)]
enum DeskTrinketKind {
    Duck,
    Cactus,
    Frame,
    Can,
    Robot,
    Donut,
    Books,
}

const ALL_DESK_TRINKETS: [DeskTrinketKind; 7] = [
    DeskTrinketKind::Duck,
    DeskTrinketKind::Cactus,
    DeskTrinketKind::Frame,
    DeskTrinketKind::Can,
    DeskTrinketKind::Robot,
    DeskTrinketKind::Donut,
    DeskTrinketKind::Books,
];

/// Exact Oracle `desks.js:31-45` 5-caster reclining office chair (shared with Pro meeting room!).
/// Returns `(chair_root, backrest_pivot)`.
pub fn spawn_office_chair(
    commands: &mut Commands,
    palette: &mut WorldPalette,
    materials: &mut Assets<StandardMaterial>,
    pos: Vec3,
    ry: f32,
    seat_mat: Handle<StandardMaterial>,
    desk_index: Option<usize>,
) -> (Entity, Entity) {
    let metal = palette.mat(materials, "#2B2F36", 0.35, 0.65);
    let mut back_pivot = Entity::PLACEHOLDER;

    let chair_root = commands
        .spawn((
            WorldEntity,
            Transform::from_translation(pos).with_rotation(Quat::from_rotation_y(ry)),
            Visibility::default(),
        ))
        .with_children(|c| {
            // 5 star legs + 5 sphere casters (`desks.js:33-37`)
            for k in 0..5 {
                let a = (k as f32 / 5.0) * TAU;
                c.spawn((
                    WorldEntity,
                    Mesh3d(palette.unit_box.clone()),
                    MeshMaterial3d(metal.clone()),
                    Transform::from_xyz(a.sin() * 0.13, 0.07, a.cos() * 0.13)
                        .with_rotation(Quat::from_rotation_y(a))
                        .with_scale(Vec3::new(0.045, 0.035, 0.27)),
                ));
                c.spawn((
                    WorldEntity,
                    Mesh3d(palette.unit_sphere.clone()),
                    MeshMaterial3d(metal.clone()),
                    Transform::from_xyz(a.sin() * 0.26, 0.035, a.cos() * 0.26)
                        .with_scale(Vec3::splat(0.07)),
                ));
            }
            // Gas cylinder + seat cushion + spine bar
            c.spawn((
                WorldEntity,
                Mesh3d(palette.unit_cyl.clone()),
                MeshMaterial3d(metal.clone()),
                Transform::from_xyz(0.0, 0.25, 0.0).with_scale(Vec3::new(0.064, 0.32, 0.064)),
            ));
            c.spawn((
                WorldEntity,
                Mesh3d(palette.unit_box.clone()),
                MeshMaterial3d(seat_mat.clone()),
                Transform::from_xyz(0.0, 0.445, 0.0).with_scale(Vec3::new(0.54, 0.09, 0.52)),
            ));
            c.spawn((
                WorldEntity,
                Mesh3d(palette.unit_box.clone()),
                MeshMaterial3d(metal.clone()),
                Transform::from_xyz(0.0, 0.56, -0.25).with_scale(Vec3::new(0.06, 0.3, 0.05)),
            ));
            // Armrests (supports + pads)
            for sx in [-1.0_f32, 1.0] {
                c.spawn((
                    WorldEntity,
                    Mesh3d(palette.unit_box.clone()),
                    MeshMaterial3d(metal.clone()),
                    Transform::from_xyz(sx * 0.27, 0.52, -0.04)
                        .with_scale(Vec3::new(0.035, 0.14, 0.04)),
                ));
                c.spawn((
                    WorldEntity,
                    Mesh3d(palette.unit_box.clone()),
                    MeshMaterial3d(metal.clone()),
                    Transform::from_xyz(sx * 0.27, 0.60, -0.02)
                        .with_scale(Vec3::new(0.05, 0.035, 0.30)),
                ));
            }

            // Reclining backrest pivot (`desks.js:40-41`)
            let mut bp = c.spawn((
                WorldEntity,
                Transform::from_xyz(0.0, 0.50, -0.26).with_rotation(Quat::from_rotation_x(-0.10)),
                Visibility::default(),
            ));
            if let Some(idx) = desk_index {
                bp.insert(ChairBackPivot {
                    desk_index: idx,
                    angle_x: -0.10,
                });
            }
            bp.with_children(|p| {
                p.spawn((
                    WorldEntity,
                    Mesh3d(palette.unit_box.clone()),
                    MeshMaterial3d(seat_mat),
                    Transform::from_xyz(0.0, 0.30, 0.0).with_scale(Vec3::new(0.50, 0.56, 0.09)),
                ));
            });
            back_pivot = bp.id();
        })
        .id();

    (chair_root, back_pivot)
}

fn spawn_coat_on_back(
    commands: &mut Commands,
    palette: &mut WorldPalette,
    materials: &mut Assets<StandardMaterial>,
    back_pivot: Entity,
    coat_hex: &str,
    visible: bool,
) -> Entity {
    let coat_m = palette.mat(materials, coat_hex, 0.86, 0.0);
    let trim_m = palette.mat(materials, "#242830", 0.86, 0.0);
    let vis = if visible {
        Visibility::Inherited
    } else {
        Visibility::Hidden
    };

    let coat_root = commands
        .spawn((WorldEntity, Transform::IDENTITY, vis))
        .with_children(|g| {
            // Back drape + shoulder roll + collar (`desks.js:124-132`)
            g.spawn((
                WorldEntity,
                Mesh3d(palette.unit_box.clone()),
                MeshMaterial3d(coat_m.clone()),
                Transform::from_xyz(0.0, 0.35, -0.066).with_scale(Vec3::new(0.50, 0.44, 0.03)),
            ));
            g.spawn((
                WorldEntity,
                Mesh3d(palette.unit_cyl.clone()),
                MeshMaterial3d(coat_m.clone()),
                Transform::from_xyz(0.0, 0.585, -0.012)
                    .with_rotation(Quat::from_rotation_z(FRAC_PI_2))
                    .with_scale(Vec3::new(0.10, 0.50, 0.10)),
            ));
            g.spawn((
                WorldEntity,
                Mesh3d(palette.unit_cyl.clone()),
                MeshMaterial3d(trim_m.clone()),
                Transform::from_xyz(0.0, 0.592, -0.018)
                    .with_rotation(Quat::from_rotation_z(FRAC_PI_2))
                    .with_scale(Vec3::new(0.112, 0.20, 0.112)),
            ));
            // 2 hanging sleeves + cuffs
            for sx in [-1.0_f32, 1.0] {
                g.spawn((
                    WorldEntity,
                    Mesh3d(palette.unit_cyl.clone()),
                    MeshMaterial3d(coat_m.clone()),
                    Transform::from_xyz(sx * 0.215, 0.33, -0.088)
                        .with_rotation(Quat::from_rotation_z(sx * 0.10))
                        .with_scale(Vec3::new(0.076, 0.34, 0.076)),
                ));
                g.spawn((
                    WorldEntity,
                    Mesh3d(palette.unit_cyl.clone()),
                    MeshMaterial3d(trim_m.clone()),
                    Transform::from_xyz(sx * 0.232, 0.13, -0.088)
                        .with_scale(Vec3::new(0.082, 0.04, 0.082)),
                ));
            }
        })
        .id();
    commands.entity(back_pivot).add_child(coat_root);
    coat_root
}

fn spawn_backpack(
    commands: &mut Commands,
    palette: &mut WorldPalette,
    materials: &mut Assets<StandardMaterial>,
    pos: Vec3,
    rot: Quat,
    bag_hex: &str,
    visible: bool,
) -> Entity {
    let bag_m = palette.mat(materials, bag_hex, 0.70, 0.0);
    let strap_m = palette.mat(materials, "#2B2F36", 0.75, 0.0);
    let vis = if visible {
        Visibility::Inherited
    } else {
        Visibility::Hidden
    };

    commands
        .spawn((
            WorldEntity,
            Transform::from_translation(pos)
                .with_rotation(rot * Quat::from_euler(EulerRot::YXZ, 0.4, -0.06, 0.0)),
            vis,
        ))
        .with_children(|g| {
            // Exact Oracle `desks.js:133-139` backpack silhouette
            g.spawn((
                WorldEntity,
                Mesh3d(palette.unit_box.clone()),
                MeshMaterial3d(bag_m.clone()),
                Transform::from_xyz(0.0, 0.19, 0.0).with_scale(Vec3::new(0.30, 0.38, 0.15)),
            ));
            g.spawn((
                WorldEntity,
                Mesh3d(palette.unit_box.clone()),
                MeshMaterial3d(bag_m),
                Transform::from_xyz(0.0, 0.12, 0.085).with_scale(Vec3::new(0.22, 0.15, 0.05)),
            ));
            g.spawn((
                WorldEntity,
                Mesh3d(palette.unit_box.clone()),
                MeshMaterial3d(strap_m.clone()),
                Transform::from_xyz(0.0, 0.21, 0.111).with_scale(Vec3::new(0.23, 0.012, 0.012)),
            ));
            g.spawn((
                WorldEntity,
                Mesh3d(palette.unit_torus.clone()),
                MeshMaterial3d(strap_m.clone()),
                Transform::from_xyz(0.0, 0.385, 0.0)
                    .with_rotation(Quat::from_rotation_x(FRAC_PI_2))
                    .with_scale(Vec3::new(0.08, 0.08, 0.05)),
            ));
            for sx in [-1.0_f32, 1.0] {
                g.spawn((
                    WorldEntity,
                    Mesh3d(palette.unit_box.clone()),
                    MeshMaterial3d(strap_m.clone()),
                    Transform::from_xyz(sx * 0.08, 0.20, -0.083)
                        .with_scale(Vec3::new(0.045, 0.30, 0.02)),
                ));
            }
        })
        .id()
}

fn spawn_nap_bedding(
    commands: &mut Commands,
    palette: &mut WorldPalette,
    materials: &mut Assets<StandardMaterial>,
    chair_root: Entity,
    back_pivot: Entity,
    blanket_mat: Handle<StandardMaterial>,
) -> (Entity, Entity, Entity) {
    let pillow_m = palette.mat(materials, "#F6F1E7", 0.95, 0.0);

    // Pillow attached to chair backrest (`bedding.js:43`)
    let pillow = commands
        .spawn((
            WorldEntity,
            Mesh3d(palette.unit_box.clone()),
            MeshMaterial3d(pillow_m),
            Transform::from_xyz(0.0, 0.52, 0.13).with_scale(Vec3::new(0.42, 0.24, 0.16)),
            Visibility::Hidden,
        ))
        .id();
    commands.entity(back_pivot).add_child(pillow);

    // Draped blanket group over lap + knees + shins (`bedding.js:8-28, 41-42`)
    let blanket = commands
        .spawn((WorldEntity, Transform::IDENTITY, Visibility::Hidden))
        .with_children(|g| {
            // Upper lap panel
            g.spawn((
                WorldEntity,
                Mesh3d(palette.unit_box.clone()),
                MeshMaterial3d(blanket_mat.clone()),
                Transform::from_xyz(0.0, 0.79, 0.30)
                    .with_rotation(Quat::from_rotation_x(0.28))
                    .with_scale(Vec3::new(0.64, 0.035, 0.42)),
            ));
            // Knee drape panel
            g.spawn((
                WorldEntity,
                Mesh3d(palette.unit_box.clone()),
                MeshMaterial3d(blanket_mat.clone()),
                Transform::from_xyz(0.0, 0.52, 0.56)
                    .with_rotation(Quat::from_rotation_x(1.38))
                    .with_scale(Vec3::new(0.62, 0.035, 0.42)),
            ));
            // Side lap skirts
            for sx in [-1.0_f32, 1.0] {
                g.spawn((
                    WorldEntity,
                    Mesh3d(palette.unit_box.clone()),
                    MeshMaterial3d(blanket_mat.clone()),
                    Transform::from_xyz(sx * 0.33, 0.64, 0.33)
                        .with_rotation(Quat::from_rotation_z(sx * 0.35))
                        .with_scale(Vec3::new(0.035, 0.28, 0.40)),
                ));
            }
        })
        .id();
    commands.entity(chair_root).add_child(blanket);

    // Folded blanket parcel (`bedding.js:44`)
    let folded = commands
        .spawn((
            WorldEntity,
            Mesh3d(palette.unit_box.clone()),
            MeshMaterial3d(blanket_mat),
            Transform::from_xyz(0.0, 0.72, 0.20).with_scale(Vec3::new(0.36, 0.12, 0.28)),
            Visibility::Hidden,
        ))
        .id();
    commands.entity(chair_root).add_child(folded);

    (pillow, blanket, folded)
}

/// Spawns the 4 uncommitted-git mess stage roots (`desks.js:158-176`) hidden initially.
fn spawn_desk_mess_stages(
    commands: &mut Commands,
    palette: &mut WorldPalette,
    materials: &mut Assets<StandardMaterial>,
    desk_pos: Vec3,
    desk_rot: Quat,
) -> [Entity; 4] {
    let paper_m = palette.mat(materials, "#FBFBF7", 0.85, 0.0);
    let stack_m = palette.mat(materials, "#F2F0E8", 0.85, 0.0);
    let ball_m = palette.mat(materials, "#EDEAE2", 0.9, 0.0);
    let note_y = palette.mat(materials, "#FFE787", 0.85, 0.0);
    let note_p = palette.mat(materials, "#FFB3C7", 0.85, 0.0);
    let cup_m = palette.mat(materials, "#C8A27C", 0.75, 0.0);
    let lid_m = palette.mat(materials, "#3B3B3B", 0.65, 0.0);
    let can_teal = palette.mat(materials, "#2EC4B6", 0.4, 0.25);
    let can_red = palette.mat(materials, "#E63946", 0.4, 0.25);
    let nb_blue = palette.mat(materials, "#3F7CC4", 0.8, 0.0);
    let pizza_box = palette.mat(materials, "#D9A066", 0.88, 0.0);
    let pizza_stripe = palette.mat(materials, "#C8553D", 0.85, 0.0);

    let s = 1.25_f32; // Oracle 1.25x visibility scale (`desks.js:161`)

    // Stage 0 (1..=5 dirty files): loose paper sheet + 2 sticky notes on desk
    let stage0 = commands
        .spawn((
            WorldEntity,
            Transform::from_translation(desk_pos).with_rotation(desk_rot),
            Visibility::Hidden,
        ))
        .with_children(|g| {
            g.spawn((
                WorldEntity,
                Mesh3d(palette.unit_box.clone()),
                MeshMaterial3d(paper_m.clone()),
                Transform::from_xyz(-0.45, 0.779, -0.17)
                    .with_rotation(Quat::from_rotation_y(0.35))
                    .with_scale(Vec3::new(0.21 * s, 0.004 * s, 0.29 * s)),
            ));
            g.spawn((
                WorldEntity,
                Mesh3d(palette.unit_box.clone()),
                MeshMaterial3d(note_y),
                Transform::from_xyz(0.33, 0.779, -0.30)
                    .with_rotation(Quat::from_rotation_y(0.2))
                    .with_scale(Vec3::new(0.07 * s, 0.003 * s, 0.07 * s)),
            ));
            g.spawn((
                WorldEntity,
                Mesh3d(palette.unit_box.clone()),
                MeshMaterial3d(note_p),
                Transform::from_xyz(0.40, 0.779, -0.20)
                    .with_rotation(Quat::from_rotation_y(-0.3))
                    .with_scale(Vec3::new(0.07 * s, 0.003 * s, 0.07 * s)),
            ));
        })
        .id();

    // Stage 1 (6..=15 dirty files): paper stack + crumpled ball + coffee cup
    let stage1 = commands
        .spawn((
            WorldEntity,
            Transform::from_translation(desk_pos).with_rotation(desk_rot),
            Visibility::Hidden,
        ))
        .with_children(|g| {
            g.spawn((
                WorldEntity,
                Mesh3d(palette.unit_box.clone()),
                MeshMaterial3d(stack_m.clone()),
                Transform::from_xyz(-0.63, 0.784, 0.04)
                    .with_rotation(Quat::from_rotation_y(-0.15))
                    .with_scale(Vec3::new(0.21 * s, 0.014 * s, 0.29 * s)),
            ));
            g.spawn((
                WorldEntity,
                Mesh3d(palette.unit_box.clone()),
                MeshMaterial3d(paper_m.clone()),
                Transform::from_xyz(-0.62, 0.793, 0.05)
                    .with_rotation(Quat::from_rotation_y(0.10))
                    .with_scale(Vec3::new(0.21 * s, 0.004 * s, 0.29 * s)),
            ));
            g.spawn((
                WorldEntity,
                Mesh3d(palette.unit_sphere.clone()),
                MeshMaterial3d(ball_m.clone()),
                Transform::from_xyz(0.24, 0.81, -0.31).with_scale(Vec3::splat(0.07 * s)),
            ));
            // Takeaway coffee cup
            g.spawn((
                WorldEntity,
                Mesh3d(palette.unit_cyl.clone()),
                MeshMaterial3d(cup_m.clone()),
                Transform::from_xyz(-0.24, 0.835, -0.29)
                    .with_scale(Vec3::new(0.068 * s, 0.11 * s, 0.068 * s)),
            ));
            g.spawn((
                WorldEntity,
                Mesh3d(palette.unit_cyl.clone()),
                MeshMaterial3d(lid_m.clone()),
                Transform::from_xyz(-0.24, 0.896, -0.29)
                    .with_scale(Vec3::new(0.076 * s, 0.012 * s, 0.076 * s)),
            ));
        })
        .id();

    // Stage 2 (16..=40 dirty files): 3 floor paper balls + 2 soda cans + open notebook
    let stage2 = commands
        .spawn((
            WorldEntity,
            Transform::from_translation(desk_pos).with_rotation(desk_rot),
            Visibility::Hidden,
        ))
        .with_children(|g| {
            for &(bx, by, bz, r) in &[
                (0.36_f32, 0.035_f32, -0.78_f32, 0.08_f32),
                (0.52, 0.035, -0.60, 0.08),
                (-0.32, 0.035, -0.92, 0.076),
            ] {
                g.spawn((
                    WorldEntity,
                    Mesh3d(palette.unit_sphere.clone()),
                    MeshMaterial3d(ball_m.clone()),
                    Transform::from_xyz(bx, by, bz).with_scale(Vec3::splat(r * s)),
                ));
            }
            g.spawn((
                WorldEntity,
                Mesh3d(palette.unit_cyl.clone()),
                MeshMaterial3d(can_teal),
                Transform::from_xyz(0.17, 0.83, 0.12)
                    .with_scale(Vec3::new(0.06 * s, 0.11 * s, 0.06 * s)),
            ));
            g.spawn((
                WorldEntity,
                Mesh3d(palette.unit_cyl.clone()),
                MeshMaterial3d(can_red),
                Transform::from_xyz(0.08, 0.83, 0.21)
                    .with_scale(Vec3::new(0.06 * s, 0.11 * s, 0.06 * s)),
            ));
            let nb_rot = Quat::from_euler(EulerRot::YXZ, 0.15, 0.08, 0.0);
            g.spawn((
                WorldEntity,
                Mesh3d(palette.unit_box.clone()),
                MeshMaterial3d(nb_blue),
                Transform::from_xyz(-0.66, 0.79, -0.24)
                    .with_rotation(nb_rot)
                    .with_scale(Vec3::new(0.16 * s, 0.012 * s, 0.22 * s)),
            ));
            g.spawn((
                WorldEntity,
                Mesh3d(palette.unit_box.clone()),
                MeshMaterial3d(paper_m.clone()),
                Transform::from_xyz(-0.66, 0.80, -0.24)
                    .with_rotation(nb_rot)
                    .with_scale(Vec3::new(0.15 * s, 0.008 * s, 0.21 * s)),
            ));
        })
        .id();

    // Stage 3 (41+ dirty files): floor sheets + floor balls + tall desk stack + 2nd cup + pizza box
    let stage3 = commands
        .spawn((
            WorldEntity,
            Transform::from_translation(desk_pos).with_rotation(desk_rot),
            Visibility::Hidden,
        ))
        .with_children(|g| {
            for &(sx, sz, ry) in &[
                (0.20_f32, -0.98_f32, 0.7_f32),
                (-0.46, -0.76, -0.4),
                (0.58, -0.92, 1.1),
                (-0.10, -1.12, -1.2),
            ] {
                g.spawn((
                    WorldEntity,
                    Mesh3d(palette.unit_box.clone()),
                    MeshMaterial3d(paper_m.clone()),
                    Transform::from_xyz(sx, 0.003, sz)
                        .with_rotation(Quat::from_rotation_y(ry))
                        .with_scale(Vec3::new(0.21 * s, 0.004 * s, 0.29 * s)),
                ));
            }
            for &(bx, by, bz, r) in &[
                (0.05_f32, 0.035_f32, -1.08_f32, 0.08_f32),
                (-0.62, 0.04, -1.02, 0.09),
                (0.70, 0.03, -0.70, 0.07),
                (0.12, 0.81, -0.33, 0.07),
                (0.03, 0.805, -0.27, 0.06),
            ] {
                g.spawn((
                    WorldEntity,
                    Mesh3d(palette.unit_sphere.clone()),
                    MeshMaterial3d(ball_m.clone()),
                    Transform::from_xyz(bx, by, bz).with_scale(Vec3::splat(r * s)),
                ));
            }
            g.spawn((
                WorldEntity,
                Mesh3d(palette.unit_box.clone()),
                MeshMaterial3d(stack_m),
                Transform::from_xyz(-0.65, 0.81, -0.27)
                    .with_rotation(Quat::from_rotation_y(0.3))
                    .with_scale(Vec3::new(0.21 * s, 0.05 * s, 0.29 * s)),
            ));
            g.spawn((
                WorldEntity,
                Mesh3d(palette.unit_cyl.clone()),
                MeshMaterial3d(cup_m),
                Transform::from_xyz(0.30, 0.835, 0.20)
                    .with_scale(Vec3::new(0.068 * s, 0.11 * s, 0.068 * s)),
            ));
            g.spawn((
                WorldEntity,
                Mesh3d(palette.unit_cyl.clone()),
                MeshMaterial3d(lid_m),
                Transform::from_xyz(0.30, 0.896, 0.20)
                    .with_scale(Vec3::new(0.076 * s, 0.012 * s, 0.076 * s)),
            ));
            // Pizza box on floor beside desk (`desks.js:173`)
            g.spawn((
                WorldEntity,
                Mesh3d(palette.unit_box.clone()),
                MeshMaterial3d(pizza_box),
                Transform::from_xyz(0.75, 0.023, -0.45)
                    .with_rotation(Quat::from_rotation_y(0.3))
                    .with_scale(Vec3::new(0.36 * s, 0.045 * s, 0.36 * s)),
            ));
            g.spawn((
                WorldEntity,
                Mesh3d(palette.unit_box.clone()),
                MeshMaterial3d(pizza_stripe),
                Transform::from_xyz(0.75, 0.052, -0.45)
                    .with_rotation(Quat::from_rotation_y(0.3))
                    .with_scale(Vec3::new(0.20 * s, 0.005 * s, 0.06 * s)),
            ));
        })
        .id();

    [stage0, stage1, stage2, stage3]
}

/// Exact Oracle `trinkets.js:16-52` spawning all 7 deterministic desk trinkets.
#[allow(clippy::too_many_arguments)]
fn spawn_desk_trinket(
    commands: &mut Commands,
    palette: &mut WorldPalette,
    materials: &mut Assets<StandardMaterial>,
    photo_mat: &Handle<StandardMaterial>,
    rng: &mut TrinketRng,
    kind: DeskTrinketKind,
    pos: Vec3,
    rot: Quat,
) {
    let at = |lx: f32, ly: f32, lz: f32| pos + rot * Vec3::new(lx, ly, lz);
    match kind {
        DeskTrinketKind::Duck => {
            let yel = palette.mat(materials, "#FFD43B", 0.45, 0.0);
            let beak = palette.mat(materials, "#F08A24", 0.45, 0.0);
            let eye = palette.mat(materials, "#141414", 0.3, 0.0);
            commands.spawn((
                WorldEntity,
                Mesh3d(palette.unit_sphere.clone()),
                MeshMaterial3d(yel.clone()),
                Transform::from_translation(at(0.0, 0.05, 0.0))
                    .with_rotation(rot)
                    .with_scale(Vec3::new(0.14, 0.112, 0.175)),
            ));
            spawn_sphere(commands, palette, yel, at(0.0, 0.13, 0.045), Vec3::splat(0.096));
            spawn_cone_rot(
                commands,
                palette,
                beak,
                at(0.0, 0.125, 0.10),
                Vec3::new(0.044, 0.05, 0.044),
                rot * Quat::from_rotation_x(FRAC_PI_2),
            );
            for s in [-1.0_f32, 1.0] {
                spawn_sphere(commands, palette, eye.clone(), at(s * 0.022, 0.145, 0.085), Vec3::splat(0.016));
            }
        }
        DeskTrinketKind::Cactus => {
            let pot = palette.mat(materials, "#C96F4A", 0.5, 0.0);
            let grn = palette.mat(materials, "#4F8A4A", 0.48, 0.0);
            let flw = palette.mat(materials, "#FF6F91", 0.45, 0.0);
            spawn_cyl(commands, palette, pot, at(0.0, 0.035, 0.0), 0.09, 0.07);
            spawn_cyl(commands, palette, grn.clone(), at(0.0, 0.13, 0.0), 0.06, 0.12);
            spawn_cyl_rot(
                commands,
                palette,
                grn,
                at(0.035, 0.13, 0.0),
                0.032,
                0.06,
                rot * Quat::from_rotation_z(-0.9),
            );
            spawn_sphere(commands, palette, flw, at(0.0, 0.20, 0.0), Vec3::splat(0.028));
        }
        DeskTrinketKind::Frame => {
            let fm = palette.mat(materials, "#2B2F36", 0.45, 0.0);
            let tilt = rot * Quat::from_rotation_x(-0.22);
            spawn_box_rot(commands, palette, fm, at(0.0, 0.065, 0.0), Vec3::new(0.15, 0.115, 0.016), tilt);
            spawn_quad(commands, palette, photo_mat.clone(), at(0.0, 0.066, 0.01), Vec2::new(0.12, 0.088), tilt);
        }
        DeskTrinketKind::Can => {
            let col = *rng.pick(&["#2EC4B6", "#E63946", "#F4A261", "#6C5CE7"]);
            let body = palette.mat(materials, col, 0.4, 0.15);
            let rim = palette.mat(materials, "#C8CDD2", 0.3, 0.7);
            spawn_cyl(commands, palette, body, at(0.0, 0.058, 0.0), 0.064, 0.115);
            spawn_cyl(commands, palette, rim, at(0.0, 0.12, 0.0), 0.06, 0.012);
        }
        DeskTrinketKind::Robot => {
            let dark = palette.mat(materials, "#2B2F36", 0.45, 0.2);
            let torso = palette.mat(materials, "#B8C2CC", 0.4, 0.4);
            let head = palette.mat(materials, "#D9E1E8", 0.4, 0.3);
            let eye = palette.emissive(materials, "#66F0FF", 1.8);
            let tip = palette.emissive(materials, "#FF5F5F", 1.4);
            spawn_cyl(commands, palette, dark.clone(), at(0.0, 0.008, 0.0), 0.105, 0.015);
            spawn_box_rot(commands, palette, torso, at(0.0, 0.055, 0.0), Vec3::new(0.07, 0.07, 0.05), rot);
            spawn_box_rot(commands, palette, head, at(0.0, 0.125, 0.0), Vec3::new(0.075, 0.06, 0.06), rot);
            for s in [-1.0_f32, 1.0] {
                spawn_sphere(commands, palette, eye.clone(), at(s * 0.018, 0.13, 0.031), Vec3::splat(0.022));
            }
            spawn_cyl(commands, palette, dark, at(0.0, 0.18, 0.0), 0.008, 0.05);
            spawn_sphere(commands, palette, tip, at(0.0, 0.205, 0.0), Vec3::splat(0.02));
        }
        DeskTrinketKind::Donut => {
            let plate = palette.mat(materials, "#FFFFFF", 0.35, 0.0);
            let dough = palette.mat(materials, "#C68B59", 0.55, 0.0);
            let icing_col = *rng.pick(&["#F28AB2", "#7B4B2A", "#FFF1C1"]);
            let icing = palette.mat(materials, icing_col, 0.4, 0.0);
            spawn_cyl(commands, palette, plate, at(0.0, 0.005, 0.0), 0.16, 0.01);
            spawn_torus_rot(
                commands,
                palette,
                dough,
                at(0.0, 0.03, 0.0),
                Vec3::new(0.11, 0.09, 0.11),
                rot,
            );
            spawn_torus_rot(
                commands,
                palette,
                icing,
                at(0.0, 0.04, 0.0),
                Vec3::new(0.105, 0.045, 0.105),
                rot,
            );
        }
        DeskTrinketKind::Books => {
            let cols = ["#C8553D", "#3E8EDE", "#2A9D8F", "#F2B33D", "#6C5CE7"];
            for k in 0..3 {
                let col = *rng.pick(&cols);
                let bm = palette.mat(materials, col, 0.45, 0.0);
                let ox = (rng.next_f32() - 0.5) * 0.02;
                let yaw = (rng.next_f32() - 0.5) * 0.3;
                spawn_box_rot(
                    commands,
                    palette,
                    bm,
                    at(ox, 0.016 + k as f32 * 0.031, 0.0),
                    Vec3::new(0.17 - k as f32 * 0.015, 0.03, 0.12),
                    rot * Quat::from_rotation_y(yaw),
                );
            }
        }
    }
}

/// Spawns all 9 (Cozy) / 12 (Pro) / 44 (HQ) desks, tilted laptop lids + screens, 5-caster
/// reclining chairs, coats/backpacks, nap bedding, 4-stage git mess, and 7 desk trinkets.
pub fn spawn_desks(
    commands: &mut Commands,
    palette: &mut WorldPalette,
    materials: &mut Assets<StandardMaterial>,
    images: &mut Assets<Image>,
    fonts: &mut FontEngine,
    spec: &LayoutSpec,
) -> ScreenMaterials {
    let mut screen_mats = ScreenMaterials::default();
    for kind in ALL_SCREEN_KINDS {
        let surf = draw_desk_screen(kind, fonts);
        let mat = palette.repeating_screen_mat(images, materials, surf);
        screen_mats.by_kind.insert(kind, mat);
    }

    // Shared trinket photo + shared plaid blanket texture (`RENDER_WORLD`, 0 leaks)
    let photo_mat = palette.textured_unlit(images, materials, draw_framed_photo());
    let blanket_tex = palette.upload_surface(images, draw_blanket_plaid());
    let blanket_mats: Vec<Handle<StandardMaterial>> = BLANKET_COLORS
        .iter()
        .map(|hex| palette.tinted_textured_double_sided(materials, blanket_tex.clone(), hex, 0.98))
        .collect();

    let desk_top = palette.mat(materials, "#F4F0E8", 0.45, 0.0);
    let desk_mat = palette.mat(materials, "#2C323C", 0.8, 0.0);
    let metal = palette.mat(materials, "#2B2F36", 0.35, 0.65);
    let laptop_m = palette.mat(materials, "#C9CED6", 0.35, 0.5);
    let emblem_m = palette.mat(materials, "#F7F3EA", 0.35, 0.0);
    let monitor_m = palette.mat(materials, "#1B1F26", 0.4, 0.1);
    let ped_m = palette.mat(materials, "#D8D3CA", 0.6, 0.0);
    let mug_m = palette.mat(materials, "#FFFFFF", 0.35, 0.0);
    let chair_m = palette.mat(materials, "#596170", 0.88, 0.0);
    let divider_m = palette.mat(materials, "#6A7C8E", 0.9, 0.0);

    // Pod fabric screens between facing pairs on Pro/HQ
    for &[cx, cz] in &spec.pods {
        spawn_box_rot(
            commands,
            palette,
            divider_m.clone(),
            Vec3::new(cx, 0.98, cz),
            Vec3::new(2.96, 0.42, 0.045),
            Quat::IDENTITY,
        );
    }

    let preview_kinds = [
        ScreenKind::Code,
        ScreenKind::Term,
        ScreenKind::Read,
        ScreenKind::Chat,
        ScreenKind::Web,
        ScreenKind::Wait,
        ScreenKind::Code,
        ScreenKind::Sleep,
    ];
    let mut rng = TrinketRng(77);

    for (i, d) in spec.desks.iter().enumerate() {
        let desk_pos = Vec3::new(d.x, 0.0, d.z);
        let rot = Quat::from_rotation_y(d.ry);
        let at = |lx: f32, ly: f32, lz: f32| desk_pos + rot * Vec3::new(lx, ly, lz);

        // Desk top, legs + side runner bars, modesty panel, desk mat (`desks.js:90-96`)
        let desk_ent = spawn_box_rot(
            commands,
            palette,
            desk_top.clone(),
            at(0.0, 0.75, 0.0),
            Vec3::new(1.5, 0.05, 0.74),
            rot,
        );

        for sx in [-0.68_f32, 0.68] {
            spawn_box_rot(commands, palette, metal.clone(), at(sx, 0.37, 0.3), Vec3::new(0.05, 0.72, 0.05), rot);
            spawn_box_rot(commands, palette, metal.clone(), at(sx, 0.37, -0.3), Vec3::new(0.05, 0.72, 0.05), rot);
            spawn_box_rot(commands, palette, metal.clone(), at(sx, 0.04, 0.0), Vec3::new(0.05, 0.05, 0.64), rot);
        }
        spawn_box_rot(commands, palette, metal.clone(), at(0.0, 0.56, 0.33), Vec3::new(1.36, 0.3, 0.025), rot);
        spawn_box_rot(commands, palette, desk_mat.clone(), at(0.0, 0.778, -0.08), Vec3::new(0.95, 0.008, 0.42), rot);

        // Laptop base + tilted `LaptopLid` parent with child `DeskScreen`, emblem, & sticky notes
        spawn_box_rot(commands, palette, laptop_m.clone(), at(0.0, 0.795, -0.04), Vec3::new(0.56, 0.03, 0.38), rot);

        let scr_kind = if i < 6 {
            preview_kinds[i % preview_kinds.len()]
        } else {
            ScreenKind::Off
        };
        let scr_mat = screen_mats.by_kind[&scr_kind].clone();
        let lid_rot = rot * Quat::from_rotation_x(LAPTOP_LID_TILT_RAD);

        // Pre-pick sticky note presence & colors from seeded RNG (`trinkets.js:62-67`)
        let note_specs = [
            (-0.14_f32, 0.28_f32, "#FFE787"),
            (0.12, 0.22, *rng.pick(&["#FFB3C7", "#B9F2C9", "#BFDDF7"])),
        ];
        let mut lid_notes: Vec<(f32, f32, Handle<StandardMaterial>, f32)> = Vec::new();
        for (ox, oy, col) in note_specs {
            if rng.next_f32() >= 0.35 {
                let tilt_z = (rng.next_f32() - 0.5) * 0.25;
                let nm = palette.mat(materials, col, 0.9, 0.0);
                lid_notes.push((ox, oy, nm, tilt_z));
            }
        }

        commands
            .spawn((
                WorldEntity,
                LaptopLid { desk_index: i },
                Transform::from_translation(at(0.0, 0.81, 0.15)).with_rotation(lid_rot),
                Visibility::default(),
            ))
            .with_children(|lid| {
                // Tilted aluminum lid shell (z spans [-0.0125, +0.0125])
                lid.spawn((
                    WorldEntity,
                    Mesh3d(palette.unit_box.clone()),
                    MeshMaterial3d(laptop_m.clone()),
                    Transform::from_xyz(0.0, 0.19, 0.0).with_scale(Vec3::new(0.56, 0.38, 0.025)),
                ));
                // Screen quad sitting strictly in front of lid front face (`z = -0.014 < -0.0125`)
                // and rotated by PI around Y so its +Z normal points along lid `-Z` (dot == 1.0).
                lid.spawn((
                    WorldEntity,
                    DeskScreen {
                        desk_index: i,
                        is_laptop: true,
                    },
                    Mesh3d(palette.unit_plane.clone()),
                    MeshMaterial3d(scr_mat.clone()),
                    Transform::from_xyz(0.0, 0.19, -0.014)
                        .with_rotation(Quat::from_rotation_y(PI))
                        .with_scale(Vec3::new(0.50, 0.32, 1.0)),
                ));
                // Back-of-lid emblem dot (`desks.js:102`)
                lid.spawn((
                    WorldEntity,
                    Mesh3d(palette.unit_cyl.clone()),
                    MeshMaterial3d(emblem_m.clone()),
                    Transform::from_xyz(0.0, 0.20, 0.014)
                        .with_rotation(Quat::from_rotation_x(FRAC_PI_2))
                        .with_scale(Vec3::new(0.075, 0.004, 0.075)),
                ));
                // Back-of-lid pastel sticky notes (`trinkets.js:61-67`)
                for (ox, oy, nm, tilt_z) in lid_notes {
                    lid.spawn((
                        WorldEntity,
                        Mesh3d(palette.unit_plane.clone()),
                        MeshMaterial3d(nm),
                        Transform::from_xyz(ox, oy, 0.0155)
                            .with_rotation(Quat::from_rotation_z(tilt_z))
                            .with_scale(Vec3::new(0.075, 0.075, 1.0)),
                    ));
                }
            });

        // Ceramic mug (`desks.js:104`)
        spawn_cyl(commands, palette, mug_m.clone(), at(0.52, 0.83, 0.16), 0.105, 0.11);

        // Pro/HQ: secondary monitor & drawer pedestal with 3 handles (`desks.js:105-111`)
        if spec.layout.is_pro() {
            let mon_rot = rot * Quat::from_rotation_y(0.45);
            let mon_at = |lx: f32, ly: f32, lz: f32| at(-0.5, 0.775, 0.2) + mon_rot * Vec3::new(lx, ly, lz);
            spawn_box_rot(commands, palette, metal.clone(), mon_at(0.0, 0.01, 0.0), Vec3::new(0.2, 0.02, 0.14), mon_rot);
            spawn_box_rot(commands, palette, metal.clone(), mon_at(0.0, 0.13, 0.02), Vec3::new(0.04, 0.24, 0.03), mon_rot);
            spawn_box_rot(commands, palette, monitor_m.clone(), mon_at(0.0, 0.33, 0.0), Vec3::new(0.5, 0.31, 0.03), mon_rot);
            let mon_scr = spawn_quad(
                commands,
                palette,
                scr_mat,
                mon_at(0.0, 0.33, -0.017),
                Vec2::new(0.46, 0.27),
                mon_rot * Quat::from_rotation_y(PI),
            );
            commands.entity(mon_scr).insert(DeskScreen {
                desk_index: i,
                is_laptop: false,
            });
            spawn_box_rot(commands, palette, ped_m.clone(), at(0.45, 0.29, 0.04), Vec3::new(0.4, 0.58, 0.55), rot);
            for hy in [0.18_f32, 0.36, 0.52] {
                spawn_box_rot(commands, palette, metal.clone(), at(0.45, hy, -0.24), Vec3::new(0.1, 0.015, 0.02), rot);
            }
        } else if i % 3 == 0 {
            let p = at(-0.58, 0.775, -0.18);
            spawn_typed_plant(
                commands,
                palette,
                materials,
                p.x,
                p.y,
                p.z,
                0.32,
                if i % 2 == 1 { PlantKind::Snake } else { PlantKind::Bush },
            );
        }

        // Desk lamp on every 3rd desk (`desks.js:113-119`)
        if i % 3 == 2 {
            spawn_cyl(commands, palette, metal.clone(), at(0.6, 0.79, -0.2), 0.17, 0.03);
            spawn_cyl(commands, palette, metal.clone(), at(0.6, 0.96, -0.2), 0.03, 0.34);
            spawn_cone_rot(
                commands,
                palette,
                metal.clone(),
                at(0.6, 1.15, -0.2),
                Vec3::new(0.22, 0.14, 0.22),
                Quat::IDENTITY,
            );
            let bulb_mat = palette.owned_emissive(materials, "#EDE6D6", 0.35);
            let bulb_ent = spawn_sphere(commands, palette, bulb_mat.clone(), at(0.6, 1.09, -0.2), Vec3::splat(0.09));
            commands.entity(bulb_ent).insert(DeskLampBulb { material: bulb_mat });
        }

        // Deterministic desk trinkets (`trinkets.js:56-59`)
        let t_kind = *rng.pick(&ALL_DESK_TRINKETS);
        let t_ry = rng.next_f32() * 0.8 - 0.4;
        spawn_desk_trinket(
            commands,
            palette,
            materials,
            &photo_mat,
            &mut rng,
            t_kind,
            at(0.5, 0.775, -0.22),
            rot * Quat::from_rotation_y(t_ry),
        );
        if !spec.layout.is_pro() && rng.next_f32() < 0.6 {
            let t2 = *rng.pick(&ALL_DESK_TRINKETS);
            let t2_ry = rng.next_f32() * 0.8 - 0.4;
            spawn_desk_trinket(
                commands,
                palette,
                materials,
                &photo_mat,
                &mut rng,
                t2,
                at(-0.33, 0.775, 0.26),
                rot * Quat::from_rotation_y(t2_ry),
            );
        }

        // 5-caster reclining office chair + draped coat + backpack + nap bedding + 4-stage mess
        let (chair_root, back_pivot) = spawn_office_chair(
            commands,
            palette,
            materials,
            at(0.0, 0.0, -0.66),
            d.ry,
            chair_m.clone(),
            Some(i),
        );
        let preview_occupied = i < 6;
        let coat_root = spawn_coat_on_back(
            commands,
            palette,
            materials,
            back_pivot,
            COAT_COLORS[i % COAT_COLORS.len()],
            preview_occupied,
        );
        let bag_root = spawn_backpack(
            commands,
            palette,
            materials,
            at(-0.52, 0.0, -0.56),
            rot,
            BAG_COLORS[i % BAG_COLORS.len()],
            preview_occupied,
        );
        let (pillow, blanket, folded) = spawn_nap_bedding(
            commands,
            palette,
            materials,
            chair_root,
            back_pivot,
            blanket_mats[i % blanket_mats.len()].clone(),
        );
        let stage_roots = spawn_desk_mess_stages(commands, palette, materials, desk_pos, rot);

        commands.entity(desk_ent).insert((
            DeskSlot { index: i },
            DeskKit {
                desk_index: i,
                coat_root,
                bag_root,
                occupied: preview_occupied,
            },
            DeskBedding {
                desk_index: i,
                pillow,
                blanket,
                folded,
                napping: false,
                progress: 0.0,
            },
            DeskMess {
                desk_index: i,
                dirty_files: 0,
                level: 0,
                stage_roots,
            },
        ));
    }

    screen_mats
}

/// Live `Update` system scrolling active desk screens (zero per-frame allocations!),
/// toggling desk lamps at night, and syncing worker coats/backpacks, nap bedding + chair
/// recline, and 4-stage git-dirty desk mess against live workers.
#[allow(clippy::too_many_arguments)]
pub fn animate_desks_system(
    time: Res<Time>,
    lighting: Option<Res<LightingState>>,
    screen_mats: Option<Res<ScreenMaterials>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
    live_workers: Query<(&LiveSessionWorker, &ChibiWorker)>,
    preview_workers: Query<Entity, With<PreviewCastWorker>>,
    mut desks_q: Query<(&mut DeskKit, &mut DeskBedding, &mut DeskMess)>,
    mut backs_q: Query<(&mut ChairBackPivot, &mut Transform), Without<DeskLampBulb>>,
    mut vis_q: Query<&mut Visibility>,
    lamp_bulbs_q: Query<&DeskLampBulb>,
) {
    let dt = time.delta_secs();
    let t = time.elapsed_secs();

    // 1. Scroll 2×-height active desk screen UVs in-place (`screens.js:99`, zero allocations!)
    if let Some(scr) = screen_mats {
        for &(kind, speed) in &[
            (ScreenKind::Code, 0.045_f32),
            (ScreenKind::Read, 0.032),
            (ScreenKind::Term, 0.090),
            (ScreenKind::Web, 0.020),
            (ScreenKind::Chat, 0.038),
        ] {
            if let Some(handle) = scr.by_kind.get(&kind)
                && let Some(mat) = materials.get_mut(handle)
            {
                let offset_y = (t * speed).fract();
                mat.uv_transform = Affine2::from_scale_angle_translation(
                    Vec2::new(1.0, 0.5),
                    0.0,
                    Vec2::new(0.0, offset_y),
                );
            }
        }
    }

    // 2. Sync desk lamp bulb glow with day/night state
    if let Some(light) = lighting
        && light.is_changed()
    {
        let night = light.is_night();
        let (col, strength) = if night {
            (Color::srgb_u8(255, 224, 158), 2.4_f32)
        } else {
            (Color::srgb_u8(237, 230, 214), 0.35_f32)
        };
        let lin = col.to_linear();
        for bulb in &lamp_bulbs_q {
            if let Some(mat) = materials.get_mut(&bulb.material) {
                mat.base_color = col;
                mat.emissive = LinearRgba::new(lin.red * strength, lin.green * strength, lin.blue * strength, 1.0);
            }
        }
    }

    // 3. Collect live desk occupancy, nap state, and git-dirty file count
    let has_preview = !preview_workers.is_empty();
    let mut live_by_desk: HashMap<usize, (bool, usize)> = HashMap::new();
    for (lw, cw) in &live_workers {
        let napping = cw.anim == WorkerAnim::Sleep;
        live_by_desk.insert(lw.desk_index, (napping, lw.git_dirty));
    }

    let mut nap_by_desk: HashMap<usize, bool> = HashMap::new();
    for (mut kit, mut bed, mut mess) in &mut desks_q {
        let idx = kit.desk_index;
        let (occupied, napping, dirty) = if let Some(&(nap, dirty)) = live_by_desk.get(&idx) {
            (true, nap, dirty)
        } else if has_preview {
            (idx < 6, false, 0)
        } else {
            (false, false, 0)
        };

        sync_kit(&mut kit, occupied, &mut vis_q);
        sync_desk_mess(&mut mess, dirty, &mut vis_q);
        nap_by_desk.insert(idx, napping);

        // Animate nap pillow + blanket visibility & unfold (`bedding.js` / `room-update.js:121-145`)
        if napping != bed.napping {
            bed.napping = napping;
            if napping {
                bed.progress = 0.0;
                if let Ok(mut v) = vis_q.get_mut(bed.pillow) {
                    *v = Visibility::Inherited;
                }
                if let Ok(mut v) = vis_q.get_mut(bed.blanket) {
                    *v = Visibility::Inherited;
                }
                if let Ok(mut v) = vis_q.get_mut(bed.folded) {
                    *v = Visibility::Hidden;
                }
            } else {
                bed.progress = 1.0;
                if let Ok(mut v) = vis_q.get_mut(bed.pillow) {
                    *v = Visibility::Hidden;
                }
                if let Ok(mut v) = vis_q.get_mut(bed.blanket) {
                    *v = Visibility::Hidden;
                }
                if let Ok(mut v) = vis_q.get_mut(bed.folded) {
                    *v = Visibility::Inherited;
                }
            }
        } else if !bed.napping && bed.progress > 0.0 {
            bed.progress = (bed.progress - dt * 1.1).max(0.0);
            if bed.progress == 0.0
                && let Ok(mut v) = vis_q.get_mut(bed.folded)
            {
                *v = Visibility::Hidden;
            }
        }
    }

    // 4. Smoothly recline chair backrest for napping workers (`room-update.js:124`)
    for (mut back, mut tf) in &mut backs_q {
        let napping = nap_by_desk.get(&back.desk_index).copied().unwrap_or(false);
        let goal = if napping { -0.62 } else { -0.10 };
        let rate = if napping { 1.5 } else { 4.0 };
        back.angle_x += (goal - back.angle_x) * (dt * rate).min(1.0);
        tf.rotation = Quat::from_rotation_x(back.angle_x);
    }
}

pub fn count_desks_in_world(world: &mut World) -> usize {
    world.query_filtered::<&DeskSlot, With<WorldEntity>>().iter(world).count()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::layout::OfficeLayout;
    use std::path::PathBuf;

    #[test]
    fn desk_laptop_screen_normal_aligns_with_tilted_lid_without_penetration() {
        let assets_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../assets");
        let mut fonts = FontEngine::from_assets_dir(&assets_dir);
        let mut app = App::new();
        app.add_plugins(bevy::transform::TransformPlugin);
        app.insert_resource(Assets::<Mesh>::default());
        app.insert_resource(Assets::<StandardMaterial>::default());
        app.insert_resource(Assets::<Image>::default());

        let spec = LayoutSpec::for_layout(OfficeLayout::Pro);
        app.world_mut().resource_scope(|world, mut meshes: Mut<Assets<Mesh>>| {
            world.resource_scope(|world, mut mats: Mut<Assets<StandardMaterial>>| {
                world.resource_scope(|world, mut imgs: Mut<Assets<Image>>| {
                    let mut pal = WorldPalette::new(&mut meshes);
                    let mut cmds = world.commands();
                    let _ = spawn_desks(&mut cmds, &mut pal, &mut mats, &mut imgs, &mut fonts, &spec);
                    cmds.insert_resource(pal);
                });
            });
        });
        app.world_mut().flush();
        app.update();

        let lids: Vec<(Entity, usize, GlobalTransform)> = app
            .world_mut()
            .query::<(Entity, &LaptopLid, &GlobalTransform)>()
            .iter(app.world())
            .map(|(e, l, g)| (e, l.desk_index, *g))
            .collect();
        assert_eq!(lids.len(), spec.desks.len(), "every desk must spawn a LaptopLid parent entity");

        let laptop_screens: Vec<(usize, &Transform, GlobalTransform, Option<Entity>)> = app
            .world_mut()
            .query::<(&DeskScreen, &Transform, &GlobalTransform, Option<&ChildOf>)>()
            .iter(app.world())
            .filter(|(ds, _, _, _)| ds.is_laptop)
            .map(|(ds, tf, gtf, parent)| (ds.desk_index, tf, *gtf, parent.map(|p| p.parent())))
            .collect();
        assert_eq!(laptop_screens.len(), spec.desks.len());

        for (desk_idx, local_tf, scr_gtf, parent_opt) in laptop_screens {
            let (lid_ent, _, lid_gtf) = lids
                .iter()
                .find(|(_, idx, _)| *idx == desk_idx)
                .expect("matching LaptopLid for desk");
            assert_eq!(
                parent_opt,
                Some(*lid_ent),
                "laptop DeskScreen must be a child of its tilted LaptopLid"
            );
            let scr_normal = scr_gtf.rotation() * Vec3::Z;
            let lid_front_normal = lid_gtf.rotation() * Vec3::NEG_Z;
            let dot = scr_normal.dot(lid_front_normal);
            assert!(
                dot > 0.99,
                "desk {desk_idx} laptop screen normal must align with tilted lid (dot={dot:.4})"
            );
            assert!(
                local_tf.translation.z <= -0.013,
                "desk {desk_idx} laptop screen must sit in front of 0.025m lid front face (-0.0125), got z={}",
                local_tf.translation.z
            );
        }
    }

    #[test]
    fn git_dirty_desk_mess_activates_strictly_at_thresholds() {
        assert_eq!(mess_level_for_dirty(0), 0);
        assert_eq!(mess_level_for_dirty(1), 1);
        assert_eq!(mess_level_for_dirty(5), 1);
        assert_eq!(mess_level_for_dirty(6), 2);
        assert_eq!(mess_level_for_dirty(15), 2);
        assert_eq!(mess_level_for_dirty(16), 3);
        assert_eq!(mess_level_for_dirty(40), 3);
        assert_eq!(mess_level_for_dirty(41), 4);
        assert_eq!(mess_level_for_dirty(120), 4);

        let assets_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../assets");
        let mut fonts = FontEngine::from_assets_dir(&assets_dir);
        let mut world = World::new();
        world.insert_resource(Assets::<Mesh>::default());
        world.insert_resource(Assets::<StandardMaterial>::default());
        world.insert_resource(Assets::<Image>::default());

        let spec = LayoutSpec::for_layout(OfficeLayout::Cozy);
        world.resource_scope(|world, mut meshes: Mut<Assets<Mesh>>| {
            world.resource_scope(|world, mut mats: Mut<Assets<StandardMaterial>>| {
                world.resource_scope(|world, mut imgs: Mut<Assets<Image>>| {
                    let mut pal = WorldPalette::new(&mut meshes);
                    let mut cmds = world.commands();
                    let _ = spawn_desks(&mut cmds, &mut pal, &mut mats, &mut imgs, &mut fonts, &spec);
                    cmds.insert_resource(pal);
                });
            });
        });
        world.flush();

        let mess_ents: Vec<Entity> = world
            .query_filtered::<Entity, With<DeskMess>>()
            .iter(&world)
            .collect();
        assert_eq!(mess_ents.len(), spec.desks.len(), "every desk must have a 4-stage DeskMess component");

        let first = mess_ents[0];
        for (dirty, expected_visible) in [(0, 0), (1, 1), (6, 2), (16, 3), (41, 4), (0, 0)] {
            let mut mess = world.get::<DeskMess>(first).unwrap().clone();
            let mut st = bevy::ecs::system::SystemState::<Query<&mut Visibility>>::new(&mut world);
            {
                let mut q = st.get_mut(&mut world);
                sync_desk_mess(&mut mess, dirty, &mut q);
            }
            *world.get_mut::<DeskMess>(first).unwrap() = mess.clone();
            let vis_count = mess
                .stage_roots
                .iter()
                .filter(|&&e| *world.get::<Visibility>(e).unwrap() != Visibility::Hidden)
                .count();
            assert_eq!(
                vis_count, expected_visible,
                "dirty={dirty} must show exactly {expected_visible} mess stages"
            );
        }
    }
}
