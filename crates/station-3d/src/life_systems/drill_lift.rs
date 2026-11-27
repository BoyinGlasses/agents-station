//! Quarterly *Diễn tập PCCC* fire drill (`drill.js`) and post-update lift maintenance
//! (`lift-service.js`), complete with wall-mounted horn-strobes, `NavGraph`-routed stairwell
//! evacuation, roll call (`"Có!"`), fire extinguisher demo, and lift maintenance barrier,
//! cones, open call panel, toolbox, technician, and `"ĐÃ KIỂM ĐỊNH"` inspection sticker.

use crate::entrance::{ElevatorDoors, LIFT_FRONT};
use crate::layout::LayoutSpec;
use crate::life::OfficeLifeState;
use crate::nav::NavGraph;
use crate::palette::{
    WorldEntity, WorldPalette, spawn_box, spawn_box_rot, spawn_cone_rot, spawn_cyl, spawn_quad,
};
use crate::replay::ReplayState;
use bevy::prelude::*;
use station_canvas2d::{Canvas2d, FontEngine, RgbaSurface, TextAlign, hex};
use std::f32::consts::{FRAC_PI_2, PI};

const ALARM_SECS: f32 = 8.0;
const MUSTER_SECS: f32 = 8.0;
const DEMO_SECS: f32 = 12.0;
const BACK_SECS: f32 = 5.0;
const WRAP_SECS: f32 = 3.0;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum DrillPhase {
    #[default]
    Idle,
    Alarm,
    Muster,
    Demo,
    Back,
}

#[derive(Resource, Debug, Clone)]
pub struct FireDrillState {
    pub enabled: bool,
    pub phase: DrillPhase,
    pub phase_elapsed: f32,
    pub sim_secs: f32,
    pub strobe_on: bool,
    pub fire_level: f32,
    pub powder_visible: bool,
    pub roll_call_count: usize,
    pub roll_call_responses: Vec<&'static str>,
    pub evacuated_paths_wall_free: bool,
}

impl Default for FireDrillState {
    fn default() -> Self {
        Self {
            enabled: true,
            phase: DrillPhase::Idle,
            phase_elapsed: 0.0,
            sim_secs: 0.0,
            strobe_on: false,
            fire_level: 0.0,
            powder_visible: false,
            roll_call_count: 0,
            roll_call_responses: Vec::new(),
            evacuated_paths_wall_free: true,
        }
    }
}

impl FireDrillState {
    pub fn trigger(&mut self) {
        if !self.enabled || self.phase != DrillPhase::Idle {
            return;
        }
        self.phase = DrillPhase::Alarm;
        self.phase_elapsed = 0.0;
        self.fire_level = 0.0;
        self.powder_visible = false;
        self.roll_call_count = 0;
        self.roll_call_responses.clear();
        self.evacuated_paths_wall_free = true;
    }

    pub fn abort(&mut self) {
        self.phase = DrillPhase::Idle;
        self.phase_elapsed = 0.0;
        self.strobe_on = false;
        self.fire_level = 0.0;
        self.powder_visible = false;
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum LiftServicePhase {
    #[default]
    Idle,
    Wait,
    Work,
    Wrap,
    Done,
}

#[derive(Resource, Debug, Clone, Default)]
pub struct LiftServiceState {
    pub phase: LiftServicePhase,
    pub remaining_secs: f32,
    pub new_build: bool,
    pub sticker_installed: bool,
    pub work_step: u8,
    pub work_timer: f32,
    pub light_flicker_off: bool,
}

impl LiftServiceState {
    pub fn start(&mut self, duration_secs: f32, new_build: bool) {
        self.phase = LiftServicePhase::Wait;
        self.remaining_secs = duration_secs.max(WRAP_SECS + 1.0);
        self.new_build = new_build;
        self.work_step = 0;
        self.work_timer = 0.0;
        self.light_flicker_off = false;
    }

    pub fn clear(&mut self) {
        self.phase = LiftServicePhase::Idle;
        self.remaining_secs = 0.0;
        self.light_flicker_off = false;
    }
}

#[derive(Component, Debug, Clone)]
pub struct DrillStrobeLens {
    pub mat: Handle<StandardMaterial>,
}

#[derive(Component, Debug, Clone, Copy)]
pub struct DrillKitPart;

#[derive(Component, Debug, Clone, Copy)]
pub struct DrillFlame {
    pub base_height: f32,
    pub idx: usize,
}

#[derive(Component, Debug, Clone, Copy)]
pub struct DrillPowder;

#[derive(Component, Debug, Clone, Copy)]
pub struct LiftBarrierPart;

#[derive(Component, Debug, Clone, Copy)]
pub struct LiftInspectionSticker;

#[derive(Component, Debug, Clone, Copy)]
pub struct LiftTechPart {
    pub base_pos: Vec3,
}

/// Computes a wall-free evacuation path from `from` to the stairwell/exit node via `NavGraph`.
pub fn compute_evacuation_path(nav: &NavGraph, from: Vec3) -> Vec<Vec3> {
    let target = nav
        .node_pos("I")
        .or_else(|| nav.node_pos("O"))
        .unwrap_or_else(|| nav.door_out().pos);
    let path = nav.shortest_path(from, target);
    if path.len() >= 2 {
        path
    } else {
        vec![from, target]
    }
}

fn draw_assembly_sign(fonts: &mut FontEngine) -> RgbaSurface {
    let mut c = Canvas2d::new(256, 320);
    c.fill(hex("#0E8A4A"));
    c.fill_rect(14.0, 14.0, 228.0, 176.0, hex("#FFFFFF"));
    // Green assembly emblem: 4 inward arrows around group silhouettes
    let gcol = hex("#0E8A4A");
    c.fill_circle(128.0, 96.0, 14.0, gcol);
    c.fill_rect(114.0, 114.0, 28.0, 34.0, gcol);
    c.fill_circle(98.0, 102.0, 11.0, gcol);
    c.fill_rect(88.0, 116.0, 20.0, 28.0, gcol);
    c.fill_circle(158.0, 102.0, 11.0, gcol);
    c.fill_rect(148.0, 116.0, 20.0, 28.0, gcol);
    c.stroke_line(34.0, 34.0, 74.0, 74.0, 8.0, gcol);
    c.stroke_line(222.0, 34.0, 182.0, 74.0, 8.0, gcol);
    c.stroke_line(34.0, 170.0, 74.0, 130.0, 8.0, gcol);
    c.stroke_line(222.0, 170.0, 182.0, 130.0, 8.0, gcol);

    fonts.draw_text(
        &mut c,
        "ĐIỂM TẬP KẾT",
        128.0,
        232.0,
        28.0,
        hex("#FFFFFF"),
        TextAlign::Center,
        Some(232.0),
        true,
        false,
    );
    fonts.draw_text(
        &mut c,
        "PCCC · AN TOÀN",
        128.0,
        276.0,
        18.0,
        hex("#E4F7EC"),
        TextAlign::Center,
        Some(232.0),
        true,
        false,
    );
    c.into_surface()
}

fn draw_lift_maintenance_sign(fonts: &mut FontEngine) -> RgbaSurface {
    let mut c = Canvas2d::new(512, 192);
    c.fill(hex("#FFD21F"));
    // Top & bottom black hazard stripes
    for y in [0.0_f32, 166.0] {
        c.fill_rect(0.0, y, 512.0, 26.0, hex("#1C1C1C"));
        for x in (0..512).step_by(48) {
            c.fill_rect(x as f32, y, 22.0, 26.0, hex("#FFD21F"));
        }
    }
    // Warning triangle on left
    c.fill_polygon(&[(76.0, 44.0), (124.0, 142.0), (28.0, 142.0)], hex("#1C1C1C"));
    c.fill_polygon(&[(76.0, 62.0), (110.0, 132.0), (42.0, 132.0)], hex("#FFD21F"));
    fonts.draw_text(
        &mut c,
        "!",
        76.0,
        108.0,
        44.0,
        hex("#1C1C1C"),
        TextAlign::Center,
        Some(40.0),
        true,
        false,
    );
    fonts.draw_text(
        &mut c,
        "ĐANG BẢO TRÌ",
        312.0,
        78.0,
        44.0,
        hex("#1C1C1C"),
        TextAlign::Center,
        Some(340.0),
        true,
        false,
    );
    fonts.draw_text(
        &mut c,
        "Xin lỗi quý khách · Vui lòng đi thang bộ",
        312.0,
        128.0,
        18.0,
        hex("#1C1C1C"),
        TextAlign::Center,
        Some(350.0),
        true,
        false,
    );
    c.into_surface()
}

fn draw_inspection_sticker(fonts: &mut FontEngine) -> RgbaSurface {
    let mut c = Canvas2d::new(192, 240);
    c.fill(hex("#FFFFFF"));
    c.stroke_rect(4.0, 4.0, 184.0, 232.0, 6.0, hex("#1F4E9E"));
    c.fill_rect(4.0, 4.0, 184.0, 48.0, hex("#1F4E9E"));
    fonts.draw_text(
        &mut c,
        "ĐÃ KIỂM ĐỊNH",
        96.0,
        28.0,
        20.0,
        hex("#FFFFFF"),
        TextAlign::Center,
        Some(172.0),
        true,
        false,
    );
    c.fill_circle(96.0, 102.0, 30.0, hex("#C8102E"));
    fonts.draw_text(
        &mut c,
        "OK",
        96.0,
        104.0,
        24.0,
        hex("#FFFFFF"),
        TextAlign::Center,
        Some(52.0),
        true,
        false,
    );
    fonts.draw_text(
        &mut c,
        "AN TOÀN THANG MÁY",
        96.0,
        162.0,
        15.0,
        hex("#1C1F24"),
        TextAlign::Center,
        Some(172.0),
        true,
        false,
    );
    fonts.draw_text(
        &mut c,
        "AGENTS STATION",
        96.0,
        196.0,
        13.0,
        hex("#5E6670"),
        TextAlign::Center,
        Some(172.0),
        false,
        false,
    );
    c.into_surface()
}

/// Spawns PCCC wall horn-strobes, street assembly drill kit, and Pro/HQ lift maintenance kit.
pub fn spawn_drill_and_lift_service(
    commands: &mut Commands,
    pal: &mut WorldPalette,
    materials: &mut Assets<StandardMaterial>,
    images: &mut Assets<Image>,
    fonts: &mut FontEngine,
    spec: &LayoutSpec,
) {
    let is_pro = spec.layout.is_pro();

    // 1. Wall-mounted fire alarm horn-strobes (`drill.js:53-65`)
    let strobe_red = pal.mat(materials, "#C8261B", 0.5, 0.05);
    let strobe_grille = pal.matte(materials, "#E9ECEF");
    let lens_mat = pal.owned_emissive(materials, "#E9D9D2", 0.1);
    let strobe_mounts: &[(f32, f32, f32, f32)] = if is_pro {
        &[
            (-7.95, 2.55, 2.1, FRAC_PI_2),
            (-7.95, 2.55, 6.3, FRAC_PI_2),
            (0.3, 2.85, -7.95, 0.0),
            (4.2, 2.85, -7.95, 0.0),
            (13.52, 2.62, -7.95, 0.0),
        ]
    } else {
        &[(-7.95, 2.55, 0.8, FRAC_PI_2)]
    };

    for &(x, y, z, ry) in strobe_mounts {
        let rot = Quat::from_rotation_y(ry);
        let base = Vec3::new(x, y, z);
        spawn_box_rot(commands, pal, strobe_red.clone(), base + rot * Vec3::new(0.0, 0.0, 0.025), Vec3::new(0.15, 0.20, 0.05), rot);
        spawn_box_rot(commands, pal, strobe_grille.clone(), base + rot * Vec3::new(0.0, -0.045, 0.055), Vec3::new(0.11, 0.05, 0.02), rot);
        let lens = spawn_box_rot(commands, pal, lens_mat.clone(), base + rot * Vec3::new(0.0, 0.045, 0.06), Vec3::new(0.10, 0.05, 0.03), rot);
        commands.entity(lens).insert(DrillStrobeLens { mat: lens_mat.clone() });
    }

    // 2. Street PCCC assembly kit: "ĐIỂM TẬP KẾT" standee, practice fire tray, flames, extinguisher, powder
    let stand_pos = Vec3::new(-9.8, 0.0, 2.2);
    let tray_pos = Vec3::new(-9.8, 0.0, 5.2);
    let dark_base = pal.matte(materials, "#2B2F36");
    let pole_m = pal.mat(materials, "#9AA1A9", 0.4, 0.5);
    let steel_m = pal.mat(materials, "#6E7680", 0.4, 0.4);
    let soot_m = pal.matte(materials, "#1E1F23");
    let sign_m = pal.textured_lit_double_sided(images, materials, draw_assembly_sign(fonts), 0.55);

    let kit_ents = [
        spawn_cyl(commands, pal, dark_base.clone(), stand_pos + Vec3::new(0.0, 0.025, 0.0), 0.42, 0.05),
        spawn_cyl(commands, pal, pole_m, stand_pos + Vec3::new(0.0, 0.60, 0.0), 0.04, 1.20),
        spawn_quad(commands, pal, sign_m, stand_pos + Vec3::new(0.0, 1.45, 0.0), Vec2::new(0.50, 0.625), Quat::from_rotation_y(FRAC_PI_2)),
        // Steel fire practice tray + fuel soot
        spawn_box(commands, pal, steel_m.clone(), tray_pos + Vec3::new(0.0, 0.05, 0.0), Vec3::new(0.64, 0.10, 0.64)),
        spawn_box(commands, pal, soot_m.clone(), tray_pos + Vec3::new(0.0, 0.105, 0.0), Vec3::new(0.56, 0.012, 0.56)),
        // Red 4 kg extinguisher standing beside tray
        spawn_cyl(commands, pal, strobe_red.clone(), tray_pos + Vec3::new(0.0, 0.24, -0.95), 0.15, 0.42),
        spawn_cyl(commands, pal, soot_m, tray_pos + Vec3::new(0.0, 0.48, -0.95), 0.07, 0.08),
    ];
    for e in kit_ents {
        commands.entity(e).insert((DrillKitPart, Visibility::Hidden));
    }

    let flame_m = pal.emissive(materials, "#FF7A18", 2.6);
    for (idx, (fx, fz, fh)) in [(0.0_f32, 0.0_f32, 0.50_f32), (-0.12, 0.08, 0.36), (0.13, -0.06, 0.40)]
        .into_iter()
        .enumerate()
    {
        let fl = spawn_cone_rot(
            commands,
            pal,
            flame_m.clone(),
            tray_pos + Vec3::new(fx, 0.10 + fh * 0.5, fz),
            Vec3::new(0.24, fh, 0.24),
            Quat::IDENTITY,
        );
        commands.entity(fl).insert((
            DrillFlame {
                base_height: fh,
                idx,
            },
            Visibility::Hidden,
        ));
    }

    let powder_m = pal.mat_blend(materials, "#F4F2EC", 0.55, true);
    let powder = spawn_cone_rot(
        commands,
        pal,
        powder_m,
        tray_pos + Vec3::new(0.0, 0.26, -0.45),
        Vec3::new(0.45, 0.90, 0.45),
        Quat::from_rotation_x(FRAC_PI_2),
    );
    commands.entity(powder).insert((DrillPowder, Visibility::Hidden));

    // 3. Post-update Lift Maintenance barrier, cones, open call panel, toolbox, tech, sticker (`lift-service.js`)
    if !is_pro {
        return;
    }

    let dz = spec.door_z;
    let bx = LIFT_FRONT + 0.25;
    let za = dz - 0.62;
    let zb = dz + 0.35;
    let zmid = (za + zb) * 0.5;
    let yellow_m = pal.mat(materials, "#FFD21F", 0.45, 0.05);
    let black_m = pal.matte(materials, "#1C1C1C");
    let cone_orange = pal.mat(materials, "#F05A1A", 0.5, 0.0);
    let white_band = pal.matte(materials, "#F4F4F2");
    let toolbox_red = pal.mat(materials, "#C42B22", 0.4, 0.25);
    let wire_r = pal.matte(materials, "#D62F2F");
    let wire_b = pal.matte(materials, "#2E6FD1");
    let wire_y = pal.matte(materials, "#F2C200");
    let barrier_sign_m = pal.textured_lit_double_sided(images, materials, draw_lift_maintenance_sign(fonts), 0.5);

    let mut barrier_ents = [Entity::PLACEHOLDER; 24];
    let mut bi = 0usize;
    let mut push_b = |e: Entity| {
        barrier_ents[bi] = e;
        bi += 1;
    };

    for z in [za, zb] {
        push_b(spawn_cyl(commands, pal, dark_base.clone(), Vec3::new(bx, 0.02, z), 0.34, 0.04));
        push_b(spawn_cyl(commands, pal, yellow_m.clone(), Vec3::new(bx, 0.52, z), 0.06, 0.96));
    }
    push_b(spawn_box(commands, pal, yellow_m.clone(), Vec3::new(bx, 1.00, zmid), Vec3::new(0.04, 0.05, zb - za)));
    push_b(spawn_box(commands, pal, black_m.clone(), Vec3::new(bx, 0.38, zmid), Vec3::new(0.04, 0.05, zb - za)));
    push_b(spawn_quad(
        commands,
        pal,
        barrier_sign_m,
        Vec3::new(bx + 0.028, 0.69, zmid),
        Vec2::new(zb - za - 0.08, (zb - za - 0.08) * 0.375),
        Quat::from_rotation_y(FRAC_PI_2),
    ));

    // Two orange safety cones with white reflective band
    for (cx, cz) in [(bx + 0.08, za - 0.26), (bx + 0.36, za - 0.08)] {
        push_b(spawn_box(commands, pal, cone_orange.clone(), Vec3::new(cx, 0.015, cz), Vec3::new(0.32, 0.03, 0.32)));
        push_b(spawn_cone_rot(commands, pal, cone_orange.clone(), Vec3::new(cx, 0.28, cz), Vec3::new(0.24, 0.50, 0.24), Quat::IDENTITY));
        push_b(spawn_cyl(commands, pal, white_band.clone(), Vec3::new(cx, 0.27, cz), 0.17, 0.07));
    }

    // Opened call panel wires + red open toolbox
    let pz = dz + 0.76;
    push_b(spawn_box(commands, pal, black_m, Vec3::new(LIFT_FRONT + 0.016, 1.12, pz), Vec3::new(0.014, 0.22, 0.10)));
    push_b(spawn_box(commands, pal, wire_r, Vec3::new(LIFT_FRONT + 0.07, 1.16, pz - 0.02), Vec3::new(0.12, 0.012, 0.012)));
    push_b(spawn_box(commands, pal, wire_b, Vec3::new(LIFT_FRONT + 0.07, 1.11, pz + 0.015), Vec3::new(0.12, 0.012, 0.012)));
    push_b(spawn_box(commands, pal, wire_y, Vec3::new(LIFT_FRONT + 0.07, 1.07, pz - 0.005), Vec3::new(0.12, 0.012, 0.012)));
    push_b(spawn_box(commands, pal, toolbox_red, Vec3::new(LIFT_FRONT + 0.18, 0.10, dz + 0.58), Vec3::new(0.24, 0.20, 0.44)));

    for e in &barrier_ents[..bi] {
        commands.entity(*e).insert((LiftBarrierPart, Visibility::Hidden));
    }

    // Blue-uniform lift technician at the call panel (visible during Work/Wrap)
    let tech_pos = Vec3::new(LIFT_FRONT + 0.38, 0.0, dz + 0.88);
    let tech_shirt = pal.mat(materials, "#1F5FA8", 0.65, 0.0);
    let tech_pants = pal.matte(materials, "#1E3A66");
    let tech_skin = pal.mat(materials, "#EBC3A0", 0.55, 0.0);
    let tech_cap = pal.matte(materials, "#1E3A66");
    let meter_y = pal.mat(materials, "#F2C200", 0.4, 0.1);
    let tech_root = commands
        .spawn((
            WorldEntity,
            LiftTechPart { base_pos: tech_pos },
            Transform::from_translation(tech_pos).with_rotation(Quat::from_rotation_y(-FRAC_PI_2)),
            Visibility::Hidden,
        ))
        .with_children(|p| {
            p.spawn((
                WorldEntity,
                Mesh3d(pal.unit_cyl.clone()),
                MeshMaterial3d(tech_pants),
                Transform::from_xyz(0.0, 0.28, 0.0).with_scale(Vec3::new(0.24, 0.56, 0.24)),
            ));
            p.spawn((
                WorldEntity,
                Mesh3d(pal.unit_cyl.clone()),
                MeshMaterial3d(tech_shirt),
                Transform::from_xyz(0.0, 0.78, 0.0).with_scale(Vec3::new(0.32, 0.46, 0.26)),
            ));
            p.spawn((
                WorldEntity,
                Mesh3d(pal.unit_sphere.clone()),
                MeshMaterial3d(tech_skin),
                Transform::from_xyz(0.0, 1.18, 0.0).with_scale(Vec3::splat(0.32)),
            ));
            p.spawn((
                WorldEntity,
                Mesh3d(pal.unit_cyl.clone()),
                MeshMaterial3d(tech_cap),
                Transform::from_xyz(0.0, 1.32, 0.0).with_scale(Vec3::new(0.34, 0.08, 0.36)),
            ));
            p.spawn((
                WorldEntity,
                Mesh3d(pal.unit_box.clone()),
                MeshMaterial3d(meter_y),
                Transform::from_xyz(0.18, 0.86, 0.18).with_scale(Vec3::new(0.08, 0.14, 0.04)),
            ));
        })
        .id();
    let _ = tech_root;

    // Inspection sticker inside elevator car back wall (`lift-service.js:93-95`)
    let sticker_m = pal.textured_lit(images, materials, draw_inspection_sticker(fonts), 0.45);
    let sticker = spawn_quad(
        commands,
        pal,
        sticker_m,
        Vec3::new(-7.94, 1.50, dz - 0.42),
        Vec2::new(0.14, 0.175),
        Quat::from_rotation_y(FRAC_PI_2),
    );
    commands.entity(sticker).insert((LiftInspectionSticker, Visibility::Hidden));
}

#[allow(clippy::too_many_arguments, clippy::type_complexity)]
pub fn update_drill_and_lift_system(
    time: Res<Time>,
    mut drill: Option<ResMut<FireDrillState>>,
    mut lift_svc: Option<ResMut<LiftServiceState>>,
    mut life: Option<ResMut<OfficeLifeState>>,
    replay: Option<Res<ReplayState>>,
    nav: Option<Res<NavGraph>>,
    mut materials: Option<ResMut<Assets<StandardMaterial>>>,
    q_strobes: Query<&DrillStrobeLens>,
    mut q_kit: Query<
        &mut Visibility,
        (
            With<DrillKitPart>,
            Without<DrillFlame>,
            Without<DrillPowder>,
            Without<LiftBarrierPart>,
            Without<LiftInspectionSticker>,
            Without<LiftTechPart>,
        ),
    >,
    mut q_flames: Query<
        (&DrillFlame, &mut Visibility, &mut Transform),
        (
            Without<DrillKitPart>,
            Without<DrillPowder>,
            Without<LiftBarrierPart>,
            Without<LiftInspectionSticker>,
            Without<LiftTechPart>,
        ),
    >,
    mut q_powder: Query<
        &mut Visibility,
        (
            With<DrillPowder>,
            Without<DrillKitPart>,
            Without<DrillFlame>,
            Without<LiftBarrierPart>,
            Without<LiftInspectionSticker>,
            Without<LiftTechPart>,
        ),
    >,
    mut q_barrier: Query<
        &mut Visibility,
        (
            With<LiftBarrierPart>,
            Without<DrillKitPart>,
            Without<DrillFlame>,
            Without<DrillPowder>,
            Without<LiftInspectionSticker>,
            Without<LiftTechPart>,
        ),
    >,
    mut q_sticker: Query<
        &mut Visibility,
        (
            With<LiftInspectionSticker>,
            Without<DrillKitPart>,
            Without<DrillFlame>,
            Without<DrillPowder>,
            Without<LiftBarrierPart>,
            Without<LiftTechPart>,
        ),
    >,
    mut q_tech: Query<
        (&LiftTechPart, &mut Visibility, &mut Transform),
        (
            Without<DrillKitPart>,
            Without<DrillFlame>,
            Without<DrillPowder>,
            Without<LiftBarrierPart>,
            Without<LiftInspectionSticker>,
        ),
    >,
    mut q_doors: Query<&mut ElevatorDoors>,
    mut q_door_leaves: Query<&mut Transform, (Without<DrillFlame>, Without<LiftTechPart>)>,
) {
    let dt = time.delta_secs().clamp(0.0, 0.25);
    let in_replay = replay.as_deref().is_some_and(|r| r.active);

    // --- 1. Quarterly PCCC Fire Drill ---
    if let Some(d) = drill.as_deref_mut() {
        if in_replay && d.phase != DrillPhase::Idle {
            d.abort();
        }
        d.sim_secs += dt;
        if d.phase != DrillPhase::Idle {
            d.phase_elapsed += dt;
            match d.phase {
                DrillPhase::Idle => {}
                DrillPhase::Alarm => {
                    if let Some(graph) = nav.as_deref() {
                        if let Some(spot) = graph.desk_spot(0) {
                            let path = compute_evacuation_path(graph, spot.pos);
                            d.evacuated_paths_wall_free =
                                path.windows(2).all(|w| !graph.crosses_wall(w[0], w[1]));
                        }
                    }
                    if d.phase_elapsed >= ALARM_SECS {
                        d.phase = DrillPhase::Muster;
                        d.phase_elapsed = 0.0;
                        d.roll_call_count = 4;
                        d.roll_call_responses.clear();
                        d.roll_call_responses.push("Có!");
                    }
                }
                DrillPhase::Muster => {
                    if d.phase_elapsed >= MUSTER_SECS {
                        d.phase = DrillPhase::Demo;
                        d.phase_elapsed = 0.0;
                        d.fire_level = 0.1;
                        d.powder_visible = false;
                    }
                }
                DrillPhase::Demo => {
                    // Light (0..4s) -> Shake/Pin/Aim (4..6s) -> Spray powder (6..9.5s) -> Clap (9.5..12s)
                    if d.phase_elapsed < 6.0 {
                        d.fire_level = (d.fire_level + dt * 0.55).min(1.0);
                        d.powder_visible = false;
                    } else if d.phase_elapsed < 9.5 {
                        d.powder_visible = true;
                        d.fire_level = (d.fire_level - dt * 0.35).max(0.0);
                    } else {
                        d.powder_visible = false;
                        d.fire_level = 0.0;
                    }
                    if d.phase_elapsed >= DEMO_SECS {
                        d.phase = DrillPhase::Back;
                        d.phase_elapsed = 0.0;
                    }
                }
                DrillPhase::Back => {
                    d.powder_visible = false;
                    d.fire_level = 0.0;
                    if d.phase_elapsed >= BACK_SECS {
                        d.phase = DrillPhase::Idle;
                        d.phase_elapsed = 0.0;
                    }
                }
            }
        }

        let active = d.phase != DrillPhase::Idle;
        if let Some(l) = life.as_deref_mut() {
            l.drill_active = active;
        }

        // Flash horn-strobe lenses at 1 Hz white blink during Alarm/Muster/Demo
        let want_strobe = active && d.phase != DrillPhase::Back && (d.sim_secs % 1.0) < 0.16;
        if want_strobe != d.strobe_on {
            d.strobe_on = want_strobe;
            if let Some(mats) = materials.as_deref_mut() {
                if let Some(lens) = q_strobes.iter().next() {
                    if let Some(m) = mats.get_mut(&lens.mat) {
                        m.emissive = if want_strobe {
                            LinearRgba::new(6.0, 6.0, 6.0, 1.0)
                        } else {
                            LinearRgba::new(0.1, 0.09, 0.08, 1.0)
                        };
                    }
                }
            }
        }

        let kit_vis = if active { Visibility::Visible } else { Visibility::Hidden };
        for mut v in &mut q_kit {
            if *v != kit_vis {
                *v = kit_vis;
            }
        }
        let flame_on = active && d.fire_level > 0.02;
        let flame_vis = if flame_on { Visibility::Visible } else { Visibility::Hidden };
        for (fl, mut vis, mut tf) in &mut q_flames {
            if *vis != flame_vis {
                *vis = flame_vis;
            }
            if flame_on {
                let flicker = 0.85 + 0.25 * (d.sim_secs * (11.0 + fl.idx as f32 * 3.0)).sin();
                let h = (fl.base_height * d.fire_level * flicker).max(0.01);
                tf.scale.y = h;
                tf.translation.y = 0.10 + h * 0.5;
            }
        }
        let powder_vis = if active && d.powder_visible {
            Visibility::Visible
        } else {
            Visibility::Hidden
        };
        for mut v in &mut q_powder {
            if *v != powder_vis {
                *v = powder_vis;
            }
        }
    }

    // --- 2. Post-Update Lift Maintenance Technician ---
    if let Some(svc) = lift_svc.as_deref_mut() {
        match svc.phase {
            LiftServicePhase::Idle | LiftServicePhase::Done => {}
            LiftServicePhase::Wait => {
                svc.phase = LiftServicePhase::Work;
            }
            LiftServicePhase::Work => {
                svc.remaining_secs = (svc.remaining_secs - dt).max(0.0);
                svc.work_timer += dt;
                if svc.work_timer >= 3.5 {
                    svc.work_timer = 0.0;
                    svc.work_step = (svc.work_step + 1) % 4;
                }
                svc.light_flicker_off = (svc.remaining_secs * 7.0).sin() > 0.82;
                if svc.remaining_secs <= WRAP_SECS {
                    svc.phase = LiftServicePhase::Wrap;
                    svc.light_flicker_off = false;
                }
            }
            LiftServicePhase::Wrap => {
                svc.remaining_secs = (svc.remaining_secs - dt).max(0.0);
                if svc.remaining_secs <= 0.0 {
                    if svc.new_build {
                        svc.sticker_installed = true;
                    }
                    svc.phase = LiftServicePhase::Done;
                }
            }
        }

        let busy = matches!(svc.phase, LiftServicePhase::Work | LiftServicePhase::Wrap);
        if let Some(l) = life.as_deref_mut() {
            l.lift_in_service = busy;
        }

        let barrier_vis = if svc.phase == LiftServicePhase::Work {
            Visibility::Visible
        } else {
            Visibility::Hidden
        };
        for mut v in &mut q_barrier {
            if *v != barrier_vis {
                *v = barrier_vis;
            }
        }

        let tech_vis = if busy { Visibility::Visible } else { Visibility::Hidden };
        for (tp, mut vis, mut tf) in &mut q_tech {
            if *vis != tech_vis {
                *vis = tech_vis;
            }
            if busy {
                let sway = (svc.work_timer * 2.4).sin() * 0.04;
                tf.translation = tp.base_pos + Vec3::new(0.0, 0.0, sway);
                tf.rotation = Quat::from_rotation_y(if svc.work_step == 3 { PI } else { -FRAC_PI_2 });
            }
        }

        let mut doors_open = 0.0_f32;
        for mut d in &mut q_doors {
            let prev = d.open;
            if svc.phase == LiftServicePhase::Work {
                d.open = 0.42;
            } else if svc.phase == LiftServicePhase::Wrap && svc.new_build {
                d.open = 1.0;
            } else if svc.phase == LiftServicePhase::Done && d.open > 0.0 {
                d.open = (d.open - dt * 1.5).max(0.0);
            }
            if (d.open - prev).abs() > 1e-5 {
                let open = d.open.clamp(0.0, 1.0);
                if let Ok(mut tf) = q_door_leaves.get_mut(d.left_door) {
                    tf.translation.z = d.center_z - (0.25 + open * 0.46);
                }
                if let Ok(mut tf) = q_door_leaves.get_mut(d.right_door) {
                    tf.translation.z = d.center_z + (0.25 + open * 0.46);
                }
            }
            doors_open = d.open;
        }

        let sticker_vis = if svc.sticker_installed && (doors_open > 0.02 || q_doors.is_empty()) {
            Visibility::Visible
        } else {
            Visibility::Hidden
        };
        for mut v in &mut q_sticker {
            if *v != sticker_vis {
                *v = sticker_vis;
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::layout::OfficeLayout;
    use std::path::PathBuf;

    #[test]
    fn fire_drill_steps_alarm_evacuation_roll_call_co_extinguisher_demo_and_returns_cleanly() {
        let assets_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../assets");
        let mut fonts = FontEngine::from_assets_dir(&assets_dir);
        let nav = NavGraph::for_layout(OfficeLayout::Pro);

        // Verify evacuation path from every desk to stairwell is wall-free
        for i in 0..nav.desk_count() {
            let desk = nav.desk_spot(i).expect("desk").pos;
            let path = compute_evacuation_path(&nav, desk);
            assert!(path.len() >= 2, "evacuation path must route to stairwell");
            for win in path.windows(2) {
                assert!(!nav.crosses_wall(win[0], win[1]), "evacuation segment crossed wall for desk {i}");
            }
        }

        let mut app = App::new();
        app.insert_resource(Time::<()>::default())
            .insert_resource(Assets::<Mesh>::default())
            .insert_resource(Assets::<StandardMaterial>::default())
            .insert_resource(Assets::<Image>::default())
            .insert_resource(nav)
            .insert_resource(FireDrillState::default())
            .insert_resource(LiftServiceState::default())
            .insert_resource(OfficeLifeState::default())
            .insert_resource(ReplayState::default())
            .add_systems(Update, update_drill_and_lift_system);

        let spec = LayoutSpec::for_layout(OfficeLayout::Pro);
        app.world_mut().resource_scope(|world, mut meshes: Mut<Assets<Mesh>>| {
            world.resource_scope(|world, mut mats: Mut<Assets<StandardMaterial>>| {
                world.resource_scope(|world, mut imgs: Mut<Assets<Image>>| {
                    let mut pal = WorldPalette::new(&mut meshes);
                    let mut cmds = world.commands();
                    spawn_drill_and_lift_service(&mut cmds, &mut pal, &mut mats, &mut imgs, &mut fonts, &spec);
                    world.insert_resource(pal);
                });
            });
        });
        app.world_mut().flush();

        assert!(app.world_mut().query::<&DrillStrobeLens>().iter(app.world()).count() >= 5);
        app.world_mut().resource_mut::<FireDrillState>().trigger();
        assert_eq!(app.world().resource::<FireDrillState>().phase, DrillPhase::Alarm);

        let mut saw_muster_co = false;
        let mut saw_demo_spray = false;
        for _ in 0..160 {
            app.world_mut().resource_mut::<Time<()>>().advance_by(std::time::Duration::from_millis(250));
            app.update();
            let d = app.world().resource::<FireDrillState>();
            if d.phase == DrillPhase::Muster && d.roll_call_responses.contains(&"Có!") {
                saw_muster_co = true;
            }
            if d.phase == DrillPhase::Demo && d.powder_visible {
                saw_demo_spray = true;
            }
        }
        assert!(saw_muster_co, "must complete roll call with 'Có!' response");
        assert!(saw_demo_spray, "must run extinguisher powder spray demo");
        assert_eq!(app.world().resource::<FireDrillState>().phase, DrillPhase::Idle);
        assert!(!app.world().resource::<OfficeLifeState>().drill_active);
    }

    #[test]
    fn lift_maintenance_shows_barrier_tech_and_clears_cleanly_with_new_build_sticker() {
        let assets_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../assets");
        let mut fonts = FontEngine::from_assets_dir(&assets_dir);
        let mut app = App::new();
        app.insert_resource(Time::<()>::default())
            .insert_resource(Assets::<Mesh>::default())
            .insert_resource(Assets::<StandardMaterial>::default())
            .insert_resource(Assets::<Image>::default())
            .insert_resource(NavGraph::for_layout(OfficeLayout::Pro))
            .insert_resource(FireDrillState::default())
            .insert_resource(LiftServiceState::default())
            .insert_resource(OfficeLifeState::default())
            .insert_resource(ReplayState::default())
            .add_systems(Update, update_drill_and_lift_system);

        let spec = LayoutSpec::for_layout(OfficeLayout::Pro);
        let (left_door, right_door) = app.world_mut().resource_scope(|world, mut meshes: Mut<Assets<Mesh>>| {
            world.resource_scope(|world, mut mats: Mut<Assets<StandardMaterial>>| {
                world.resource_scope(|world, mut imgs: Mut<Assets<Image>>| {
                    let mut pal = WorldPalette::new(&mut meshes);
                    let mut cmds = world.commands();
                    spawn_drill_and_lift_service(&mut cmds, &mut pal, &mut mats, &mut imgs, &mut fonts, &spec);
                    let ld = cmds
                        .spawn((
                            WorldEntity,
                            Transform::from_xyz(LIFT_FRONT - 0.06, 1.065, spec.door_z - 0.25),
                        ))
                        .id();
                    let rd = cmds
                        .spawn((
                            WorldEntity,
                            Transform::from_xyz(LIFT_FRONT - 0.06, 1.065, spec.door_z + 0.25),
                        ))
                        .id();
                    let dm = pal.matte(&mut mats, "#15181E");
                    cmds.spawn((
                        WorldEntity,
                        ElevatorDoors {
                            left_door: ld,
                            right_door: rd,
                            display_mat: dm,
                            center_z: spec.door_z,
                            open: 0.0,
                        },
                    ));
                    world.insert_resource(pal);
                    (ld, rd)
                })
            })
        });
        app.world_mut().flush();

        app.world_mut().resource_mut::<LiftServiceState>().start(10.0, true);
        app.world_mut().resource_mut::<Time<()>>().advance_by(std::time::Duration::from_millis(250));
        app.update();

        assert_eq!(app.world().resource::<LiftServiceState>().phase, LiftServicePhase::Work);
        assert!(app.world().resource::<OfficeLifeState>().lift_in_service);
        let left_z_work = app.world().get::<Transform>(left_door).unwrap().translation.z;
        let right_z_work = app.world().get::<Transform>(right_door).unwrap().translation.z;
        let expect_left = spec.door_z - (0.25 + 0.42 * 0.46);
        let expect_right = spec.door_z + (0.25 + 0.42 * 0.46);
        assert!(
            (left_z_work - expect_left).abs() < 1e-3,
            "left elevator door ({left_z_work}) must slide to {expect_left} during Work"
        );
        assert!(
            (right_z_work - expect_right).abs() < 1e-3,
            "right elevator door ({right_z_work}) must slide to {expect_right} during Work"
        );
        let vis_barriers = app
            .world_mut()
            .query_filtered::<&Visibility, With<LiftBarrierPart>>()
            .iter(app.world())
            .filter(|v| **v == Visibility::Visible)
            .count();
        assert!(vis_barriers >= 3, "barrier, cones, and sign must be visible during Work");

        // Run to completion -> barrier hides, new-build inspection sticker installed, lift_in_service=false
        for _ in 0..55 {
            app.world_mut().resource_mut::<Time<()>>().advance_by(std::time::Duration::from_millis(250));
            app.update();
        }
        let svc = app.world().resource::<LiftServiceState>();
        assert_eq!(svc.phase, LiftServicePhase::Done);
        assert!(svc.sticker_installed);
        assert!(!app.world().resource::<OfficeLifeState>().lift_in_service);
        let vis_barriers_after = app
            .world_mut()
            .query_filtered::<&Visibility, With<LiftBarrierPart>>()
            .iter(app.world())
            .filter(|v| **v == Visibility::Visible)
            .count();
        assert_eq!(vis_barriers_after, 0);
        let left_z_done = app.world().get::<Transform>(left_door).unwrap().translation.z;
        let right_z_done = app.world().get::<Transform>(right_door).unwrap().translation.z;
        assert!(
            (left_z_done - (spec.door_z - 0.25)).abs() < 1e-2,
            "left elevator door ({left_z_done}) must slide back closed after Done"
        );
        assert!(
            (right_z_done - (spec.door_z + 0.25)).abs() < 1e-2,
            "right elevator door ({right_z_done}) must slide back closed after Done"
        );

        // Zero leak check
        let ents: Vec<Entity> = app.world_mut().query_filtered::<Entity, With<WorldEntity>>().iter(app.world()).collect();
        for e in ents {
            app.world_mut().despawn(e);
        }
        let mut pal = app.world_mut().remove_resource::<WorldPalette>().unwrap();
        app.world_mut().resource_scope(|world, mut meshes: Mut<Assets<Mesh>>| {
            world.resource_scope(|world, mut mats: Mut<Assets<StandardMaterial>>| {
                world.resource_scope(|_world, mut imgs: Mut<Assets<Image>>| {
                    pal.purge_all(&mut meshes, &mut mats, &mut imgs);
                });
            });
        });
        assert_eq!(app.world().resource::<Assets<Mesh>>().len(), 0);
        assert_eq!(app.world().resource::<Assets<StandardMaterial>>().len(), 0);
        assert_eq!(app.world().resource::<Assets<Image>>().len(), 0);
    }
}
