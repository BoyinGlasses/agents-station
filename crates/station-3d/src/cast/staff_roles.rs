use crate::chibi::{
    ChibiConfig, ChibiLook, ChibiWorker, OutfitKind, WorkerAnim, spawn_chibi,
};
use crate::chibi_meshes::{ChibiMeshCache, HairStyle};
use crate::chibi_props::{ActivePropEntity, PropKind, set_active_prop};
use crate::entrance::LIFT_FRONT;
use crate::layout::LayoutSpec;
use crate::nav::{NavGraph, StationKind};
use crate::palette::{WorldPalette, spawn_box};
use crate::people::{ChibiScale, NavWalker};
use bevy::prelude::*;
use std::f32::consts::{FRAC_PI_2, PI};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum StaffRole {
    Director,
    Hr,
    Accountant,
    Receptionist,
    Cleaner,
    ShopeeCourier,
    Guard,
    Intern,
    FarewellGuest,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum FarewellStage {
    #[default]
    Idle,
    Packing,
    MeetingCard,
    Leaving,
    Unpacking,
}

/// Marker placed on every non-session office staff NPC entity.
#[derive(Component, Debug, Clone, Copy)]
pub struct OfficeStaffNpc {
    pub role: StaffRole,
    pub home_pos: Vec3,
    pub home_yaw: f32,
    pub home_sit: bool,
    pub round_step: usize,
    pub dwell_secs: f32,
    pub active_job: bool,
}

/// Intern tenure progress (`interns.js`: orange `#F28C28` lanyard until 20 turns across 2+ days).
#[derive(Component, Debug, Clone, Copy)]
pub struct InternTenure {
    pub lanyard_color: &'static str,
    pub turns_completed: u32,
    pub active_days: u32,
    pub permanent: bool,
}

/// Office-wide schedule & trigger state for Director, HR, Accountant, Staff, Guard, Interns & Farewell.
#[derive(Resource, Debug, Clone)]
pub struct StaffCastState {
    pub clock_minutes: u32,
    pub director_rounds_done: [bool; 2],
    pub director_prompt_desk: Option<usize>,
    pub hr_clock_in_count: usize,
    pub hr_newcomer_desk: Option<usize>,
    pub hr_birthday_active: bool,
    pub accountant_burn_pct: f32,
    pub accountant_report_done: bool,
    pub accountant_warned: bool,
    pub cleaner_round_done: bool,
    pub courier_parcel_active: bool,
    pub guard_night_shift: bool,
    pub intern_turns: u32,
    pub intern_days: u32,
    pub intern_permanent: bool,
    pub farewell_stage: FarewellStage,
}

impl Default for StaffCastState {
    fn default() -> Self {
        Self {
            clock_minutes: 9 * 60 + 30,
            director_rounds_done: [false; 2],
            director_prompt_desk: None,
            hr_clock_in_count: 0,
            hr_newcomer_desk: None,
            hr_birthday_active: false,
            accountant_burn_pct: 25.0,
            accountant_report_done: false,
            accountant_warned: false,
            cleaner_round_done: false,
            courier_parcel_active: false,
            guard_night_shift: false,
            intern_turns: 7,
            intern_days: 1,
            intern_permanent: false,
            farewell_stage: FarewellStage::Idle,
        }
    }
}

#[allow(clippy::too_many_arguments)]
fn spawn_staff_npc(
    commands: &mut Commands,
    chibi_cache: &ChibiMeshCache,
    palette: &mut WorldPalette,
    materials: &mut Assets<StandardMaterial>,
    cfg: &ChibiConfig,
    role: StaffRole,
    home_pos: Vec3,
    home_yaw: f32,
    home_sit: bool,
    anim: WorkerAnim,
    phase: f32,
) -> Entity {
    let ent = spawn_chibi(
        commands,
        chibi_cache,
        palette,
        materials,
        cfg,
        home_pos,
        home_yaw,
        anim,
        phase,
        home_sit,
    );
    commands.entity(ent).insert((
        OfficeStaffNpc {
            role,
            home_pos,
            home_yaw,
            home_sit,
            round_step: 0,
            dwell_secs: 0.0,
            active_job: false,
        },
        ChibiScale(1.0),
        NavWalker::idle_at(home_yaw, home_sit, anim),
    ));
    ent
}

/// Spawns the lift-side HR attendance fingerprint terminal (`hr.js:281-294`) using shared `WorldPalette`.
fn spawn_attendance_terminal(
    commands: &mut Commands,
    palette: &mut WorldPalette,
    materials: &mut Assets<StandardMaterial>,
    door_z: f32,
) {
    let x = LIFT_FRONT;
    let z = door_z + 0.98;
    let dark = palette.mat(materials, "#1B1F26", 0.4, 0.1);
    let screen = palette.emissive(materials, "#5EA8DC", 0.9);
    let pad = palette.emissive(materials, "#3BAA6A", 1.1);
    let plate = palette.mat(materials, "#B8912F", 0.35, 0.6);

    spawn_box(commands, palette, dark, Vec3::new(x + 0.025, 1.24, z), Vec3::new(0.05, 0.22, 0.13));
    spawn_box(commands, palette, screen, Vec3::new(x + 0.052, 1.285, z), Vec3::new(0.008, 0.079, 0.105));
    spawn_box(commands, palette, pad, Vec3::new(x + 0.052, 1.185, z), Vec3::new(0.008, 0.035, 0.05));
    spawn_box(commands, palette, plate, Vec3::new(x + 0.008, 0.92, z), Vec3::new(0.008, 0.08, 0.16));
}

/// Spawns the full office staff NPC cast (`Director`, `HR`, `Accountant`, `Receptionist`,
/// `Cleaner`, `ShopeeCourier`, `Guard`, `Intern`, `FarewellGuest`) with zero pre-spawned hidden props.
pub fn spawn_office_staff_cast(
    commands: &mut Commands,
    chibi_cache: &ChibiMeshCache,
    palette: &mut WorldPalette,
    materials: &mut Assets<StandardMaterial>,
    spec: &LayoutSpec,
    look: ChibiLook,
) {
    let pro = spec.layout.is_pro();

    if pro {
        spawn_attendance_terminal(commands, palette, materials, spec.door_z);

        // 1. Director in walnut corner office (`director.js:55`)
        let mut dir_cfg = ChibiConfig::for_agent("npc", "Giám đốc", 10);
        dir_cfg.shirt = "#2B3A55";
        dir_cfg.pants = "#262B35";
        dir_cfg.hair = "#3A3A40";
        dir_cfg.shoes = "#1B1B1E";
        dir_cfg.outfit = OutfitKind::Suit;
        dir_cfg.hair_style = HairStyle::Plain;
        dir_cfg.accessory = Some("glasses");
        dir_cfg.look = look;
        spawn_staff_npc(
            commands,
            chibi_cache,
            palette,
            materials,
            &dir_cfg,
            StaffRole::Director,
            Vec3::new(-5.4, 0.0, -6.75),
            0.0,
            true,
            WorkerAnim::Work,
            0.5,
        );

        // 2. HR Lan at admin corner (`hr.js:232, 335, 355`)
        let mut hr_cfg = ChibiConfig::for_agent("npc", "HR Lan", 11);
        hr_cfg.shirt = "#E7C6D3";
        hr_cfg.pants = "#3A3F4A";
        hr_cfg.hair = "#2B2420";
        hr_cfg.hair_style = HairStyle::Ponytail;
        hr_cfg.outfit = OutfitKind::Shirt;
        hr_cfg.accessory = None;
        hr_cfg.has_lanyard = true;
        hr_cfg.lanyard_color = "#2F6FB5";
        hr_cfg.look = look;
        spawn_staff_npc(
            commands,
            chibi_cache,
            palette,
            materials,
            &hr_cfg,
            StaffRole::Hr,
            Vec3::new(6.05, 0.0, 7.3),
            -FRAC_PI_2,
            true,
            WorkerAnim::Work,
            1.1,
        );

        // 3. Accountant Minh beside HR (`accountant.js:43, 218, 223`)
        let mut acc_cfg = ChibiConfig::for_agent("npc", "Kế toán Minh", 14);
        acc_cfg.shirt = "#F2B8B5";
        acc_cfg.pants = "#3A3F4F";
        acc_cfg.hair = "#3A2A20";
        acc_cfg.hair_style = HairStyle::Bob;
        acc_cfg.outfit = OutfitKind::Shirt;
        acc_cfg.accessory = Some("glasses");
        acc_cfg.look = look;
        spawn_staff_npc(
            commands,
            chibi_cache,
            palette,
            materials,
            &acc_cfg,
            StaffRole::Accountant,
            Vec3::new(6.11, 0.0, 6.15),
            -FRAC_PI_2,
            true,
            WorkerAnim::Work,
            1.8,
        );

        // 4. Receptionist behind walnut U-reception counter (`staff.js:335, 368`)
        let mut rec_cfg = ChibiConfig::for_agent("npc", "Lễ tân", 15);
        rec_cfg.shirt = "#D97757";
        rec_cfg.pants = "#2B2F36";
        rec_cfg.hair = "#1E1E24";
        rec_cfg.hair_style = HairStyle::Ponytail;
        rec_cfg.outfit = OutfitKind::Shirt;
        rec_cfg.accessory = None;
        rec_cfg.look = look;
        spawn_staff_npc(
            commands,
            chibi_cache,
            palette,
            materials,
            &rec_cfg,
            StaffRole::Receptionist,
            Vec3::new(-4.9, 0.0, 1.05),
            0.0,
            true,
            WorkerAnim::Work,
            2.3,
        );
    }

    // 5. Cleaner (`staff.js:389`)
    let mut cln_cfg = ChibiConfig::for_agent("npc", "Cô lao công", 16);
    cln_cfg.shirt = "#6FA8D6";
    cln_cfg.pants = "#3B5F86";
    cln_cfg.hair = "#2B2B2B";
    cln_cfg.hair_style = HairStyle::Bun;
    cln_cfg.outfit = OutfitKind::Shirt;
    cln_cfg.accessory = None;
    cln_cfg.look = look;
    let cleaner_home = if pro {
        Vec3::new(9.45, 0.0, -5.25)
    } else {
        Vec3::new(-6.75, 0.0, 4.2)
    };
    spawn_staff_npc(
        commands,
        chibi_cache,
        palette,
        materials,
        &cln_cfg,
        StaffRole::Cleaner,
        cleaner_home,
        -FRAC_PI_2,
        false,
        WorkerAnim::Rest,
        2.9,
    );

    // 6. Shopee delivery courier (`staff.js:380`)
    let mut shp_cfg = ChibiConfig::for_agent("npc", "Shipper Shopee", 17);
    shp_cfg.shirt = "#EE4D2D";
    shp_cfg.pants = "#2B2F36";
    shp_cfg.hair = "#2B2B2B";
    shp_cfg.hair_style = HairStyle::Plain;
    shp_cfg.outfit = OutfitKind::Shirt;
    shp_cfg.accessory = Some("cap");
    shp_cfg.look = look;
    let courier_home = if pro {
        Vec3::new(-6.3, 0.0, spec.door_z)
    } else {
        Vec3::new(-7.2, 0.0, spec.door_z)
    };
    spawn_staff_npc(
        commands,
        chibi_cache,
        palette,
        materials,
        &shp_cfg,
        StaffRole::ShopeeCourier,
        courier_home,
        FRAC_PI_2,
        false,
        WorkerAnim::Rest,
        3.4,
    );

    // 7. Street / entrance security guard (`guard.js:127`)
    let mut grd_cfg = ChibiConfig::for_agent("npc", "Chú bảo vệ", 18);
    grd_cfg.shirt = "#8FB8DE";
    grd_cfg.pants = "#2B3A55";
    grd_cfg.hair = "#3A332E";
    grd_cfg.hair_style = HairStyle::Plain;
    grd_cfg.outfit = OutfitKind::Shirt;
    grd_cfg.accessory = Some("cap");
    grd_cfg.look = look;
    let guard_home = if pro {
        Vec3::new(-8.55, 0.0, 5.2)
    } else {
        Vec3::new(-8.55, 0.0, 2.5)
    };
    spawn_staff_npc(
        commands,
        chibi_cache,
        palette,
        materials,
        &grd_cfg,
        StaffRole::Guard,
        guard_home,
        -FRAC_PI_2,
        false,
        WorkerAnim::Rest,
        4.0,
    );

    // 8. Intern with iconic orange `#F28C28` lanyard (`interns.js:37, 81`)
    let intern_desk_idx = if pro { 6 } else { 6.min(spec.desks.len().saturating_sub(1)) };
    let (intern_pos, intern_yaw) = spec
        .desks
        .get(intern_desk_idx)
        .map(|d| {
            let rot = Quat::from_rotation_y(d.ry);
            (Vec3::new(d.x, 0.0, d.z) + rot * Vec3::new(0.0, 0.0, -0.62), d.ry)
        })
        .unwrap_or((Vec3::new(2.5, 0.0, 4.8), 0.0));
    let mut int_cfg = ChibiConfig::for_agent("npc", "Thực tập sinh", 19);
    int_cfg.shirt = "#2A9D8F";
    int_cfg.hair = "#2C2228";
    int_cfg.hair_style = HairStyle::Spiky;
    int_cfg.outfit = OutfitKind::Hoodie;
    int_cfg.accessory = None;
    int_cfg.has_lanyard = true;
    int_cfg.lanyard_color = "#F28C28";
    int_cfg.look = look;
    let intern_ent = spawn_staff_npc(
        commands,
        chibi_cache,
        palette,
        materials,
        &int_cfg,
        StaffRole::Intern,
        intern_pos,
        intern_yaw,
        true,
        WorkerAnim::Work,
        4.6,
    );
    commands.entity(intern_ent).insert(InternTenure {
        lanyard_color: "#F28C28",
        turns_completed: 7,
        active_days: 1,
        permanent: false,
    });

    // 9. Farewell packing/unpacking guest (`farewell.js:157`)
    let fw_desk_idx = if pro { 7 } else { 7.min(spec.desks.len().saturating_sub(1)) };
    let (fw_pos, fw_yaw) = spec
        .desks
        .get(fw_desk_idx)
        .map(|d| {
            let rot = Quat::from_rotation_y(d.ry);
            (Vec3::new(d.x, 0.0, d.z) + rot * Vec3::new(0.0, 0.0, -0.62), d.ry)
        })
        .unwrap_or((Vec3::new(5.1, 0.0, 3.3), PI));
    let mut fw_cfg = ChibiConfig::for_agent("npc", "Cựu dự án", 20);
    fw_cfg.shirt = "#8C4FA8";
    fw_cfg.hair = "#3B2618";
    fw_cfg.hair_style = HairStyle::Curly;
    fw_cfg.outfit = OutfitKind::Shirt;
    fw_cfg.accessory = Some("glasses");
    fw_cfg.has_lanyard = true;
    fw_cfg.lanyard_color = "#2F6FB5";
    fw_cfg.look = look;
    spawn_staff_npc(
        commands,
        chibi_cache,
        palette,
        materials,
        &fw_cfg,
        StaffRole::FarewellGuest,
        fw_pos,
        fw_yaw,
        true,
        WorkerAnim::Rest,
        5.2,
    );
}

#[allow(clippy::too_many_arguments)]
fn sync_lazy_prop(
    commands: &mut Commands,
    chibi_cache: &ChibiMeshCache,
    palette: &mut WorldPalette,
    materials: &mut Assets<StandardMaterial>,
    ent: Entity,
    active_prop: Option<&ActivePropEntity>,
    desired: Option<PropKind>,
) {
    if active_prop.map(|p| p.kind) != desired {
        set_active_prop(commands, chibi_cache, palette, materials, ent, desired);
    }
}

/// Steps all office staff NPC routines (`Director`, `HR`, `Accountant`, `Receptionist`,
/// `Cleaner`, `ShopeeCourier`, `Guard`, `Intern`, `FarewellGuest`) over `NavGraph` with lazy props
/// and zero per-frame heap allocations.
#[allow(clippy::too_many_arguments, clippy::type_complexity)]
pub fn step_staff_roles_system(
    time: Res<Time>,
    mut commands: Commands,
    mut nav: Option<ResMut<NavGraph>>,
    mut state: Option<ResMut<StaffCastState>>,
    chibi_cache: Option<Res<ChibiMeshCache>>,
    mut palette: Option<ResMut<WorldPalette>>,
    mut materials: Option<ResMut<Assets<StandardMaterial>>>,
    mut staff_q: Query<(
        Entity,
        &mut OfficeStaffNpc,
        &mut ChibiWorker,
        &mut NavWalker,
        Option<&mut InternTenure>,
        Option<&ActivePropEntity>,
    )>,
) {
    let (Some(nav), Some(state), Some(cache), Some(pal), Some(mats)) = (
        nav.as_deref_mut(),
        state.as_deref_mut(),
        chibi_cache.as_deref(),
        palette.as_deref_mut(),
        materials.as_deref_mut(),
    ) else {
        return;
    };

    let dt = time.delta_secs().clamp(0.0, 0.1);
    let m = state.clock_minutes;
    let pro = nav.layout().is_pro();
    state.guard_night_shift = !(390..1320).contains(&m);

    for (ent, mut npc, chibi, mut walker, intern_tenure, active_prop) in &mut staff_q {
        if npc.dwell_secs > 0.0 {
            npc.dwell_secs = (npc.dwell_secs - dt).max(0.0);
        }

        let mut desired_prop: Option<PropKind> = None;

        match npc.role {
            StaffRole::Director => {
                // Prompt delivery takes priority over scheduled 10:00 / 15:30 rounds (`rounds.js:15`)
                if !walker.is_walking() && npc.dwell_secs <= 0.0 {
                    if let Some(desk_idx) = state.director_prompt_desk.take() {
                        if let Some(ds) = nav.desk_spot(desk_idx) {
                            walker.set_destination(
                                nav,
                                chibi.base_pos,
                                ds.pos,
                                ds.face,
                                false,
                                WorkerAnim::Wait,
                            );
                            npc.active_job = true;
                            npc.round_step = 99;
                        }
                    } else if npc.active_job {
                        // Advance through pod inspection stops or return to walnut office
                        let stops: [&str; 3] = ["P0b0", "P1b1", "P2f0"];
                        if npc.round_step < stops.len() {
                            if let Some(pt) = nav.node_pos(stops[npc.round_step]) {
                                walker.set_destination(
                                    nav,
                                    chibi.base_pos,
                                    pt,
                                    0.0,
                                    false,
                                    WorkerAnim::Rest,
                                );
                                npc.round_step += 1;
                                npc.dwell_secs = 2.0;
                            } else {
                                npc.round_step = stops.len();
                            }
                        } else {
                            walker.set_destination(
                                nav,
                                chibi.base_pos,
                                npc.home_pos,
                                npc.home_yaw,
                                true,
                                WorkerAnim::Work,
                            );
                            npc.active_job = false;
                            npc.round_step = 0;
                        }
                    } else {
                        // 10:00 (600) & 15:30 (930) scheduled rounds (`rounds.js:17`)
                        let round_slot = if !state.director_rounds_done[0] && (600..640).contains(&m) {
                            Some(0)
                        } else if !state.director_rounds_done[1] && (930..970).contains(&m) {
                            Some(1)
                        } else {
                            None
                        };
                        if let Some(slot) = round_slot {
                            state.director_rounds_done[slot] = true;
                            npc.active_job = true;
                            npc.round_step = 1;
                            if let Some(first_stop) = nav.node_pos("P0b0") {
                                walker.set_destination(
                                    nav,
                                    chibi.base_pos,
                                    first_stop,
                                    0.0,
                                    false,
                                    WorkerAnim::Rest,
                                );
                            }
                        }
                    }
                }
                if npc.active_job || walker.is_walking() {
                    desired_prop = Some(PropKind::Folder);
                }
            }

            StaffRole::Hr => {
                if state.hr_birthday_active {
                    if !npc.active_job && !walker.is_walking() {
                        let meet = nav.station_spot_for(StationKind::Meeting, 1);
                        walker.set_destination(
                            nav,
                            chibi.base_pos,
                            meet.pos,
                            meet.face,
                            false,
                            WorkerAnim::Rest,
                        );
                        npc.active_job = true;
                    }
                    desired_prop = Some(PropKind::Cake);
                } else if let Some(desk_idx) = state.hr_newcomer_desk.take() {
                    if let Some(ds) = nav.desk_spot(desk_idx) {
                        walker.set_destination(
                            nav,
                            chibi.base_pos,
                            ds.pos,
                            ds.face,
                            false,
                            WorkerAnim::Wait,
                        );
                        npc.active_job = true;
                        npc.dwell_secs = 1.8;
                        state.hr_clock_in_count += 1;
                    }
                    desired_prop = Some(PropKind::Folder);
                } else if npc.active_job && !walker.is_walking() && npc.dwell_secs <= 0.0 {
                    walker.set_destination(
                        nav,
                        chibi.base_pos,
                        npc.home_pos,
                        npc.home_yaw,
                        true,
                        WorkerAnim::Work,
                    );
                    npc.active_job = false;
                }
            }

            StaffRole::Accountant => {
                let report_due = pro && !state.accountant_report_done && (1050..1065).contains(&m);
                let warn_due = pro && !state.accountant_warned && state.accountant_burn_pct >= 80.0;
                if !walker.is_walking() && npc.dwell_secs <= 0.0 {
                    if report_due {
                        state.accountant_report_done = true;
                        npc.active_job = true;
                        npc.round_step = 1; // 1 = report
                        npc.dwell_secs = 2.0;
                        walker.set_destination(
                            nav,
                            chibi.base_pos,
                            Vec3::new(-4.05, 0.0, -6.12),
                            -FRAC_PI_2,
                            false,
                            WorkerAnim::Wait,
                        );
                    } else if warn_due {
                        state.accountant_warned = true;
                        npc.active_job = true;
                        npc.round_step = 2; // 2 = budget warn
                        npc.dwell_secs = 2.0;
                        if let Some(ds) = nav.desk_spot(0) {
                            walker.set_destination(
                                nav,
                                chibi.base_pos,
                                ds.pos,
                                ds.face,
                                false,
                                WorkerAnim::Wait,
                            );
                        }
                    } else if npc.active_job {
                        walker.set_destination(
                            nav,
                            chibi.base_pos,
                            npc.home_pos,
                            npc.home_yaw,
                            true,
                            WorkerAnim::Work,
                        );
                        npc.active_job = false;
                        npc.round_step = 0;
                    }
                }
                if npc.active_job || walker.is_walking() {
                    desired_prop = if npc.round_step == 2 {
                        Some(PropKind::Folder)
                    } else {
                        Some(PropKind::Paper)
                    };
                }
            }

            StaffRole::Receptionist => {
                // Behind walnut counter; waves when Shopee courier arrives
                if state.courier_parcel_active && !walker.is_walking() {
                    desired_prop = Some(PropKind::Paper);
                }
            }

            StaffRole::Cleaner => {
                let clean_due = !state.cleaner_round_done && (1110..1170).contains(&m);
                if !walker.is_walking() && npc.dwell_secs <= 0.0 {
                    if clean_due {
                        state.cleaner_round_done = true;
                        npc.active_job = true;
                        npc.round_step = 0;
                    }
                    if npc.active_job {
                        let aisle_nodes: &[&str] = if pro {
                            &["LB", "L2", "M2", "R2", "R1", "PA1"]
                        } else {
                            &["U0", "U2", "V2", "V0"]
                        };
                        if npc.round_step < aisle_nodes.len() {
                            if let Some(pt) = nav.node_pos(aisle_nodes[npc.round_step]) {
                                if npc.round_step > 0 {
                                    nav.clear_wet_strips();
                                    nav.add_wet_strip(
                                        Vec2::new(chibi.base_pos.x, chibi.base_pos.z),
                                        Vec2::new(pt.x, pt.z),
                                    );
                                }
                                walker.set_destination(
                                    nav,
                                    chibi.base_pos,
                                    pt,
                                    0.0,
                                    false,
                                    WorkerAnim::Work,
                                );
                                npc.round_step += 1;
                                npc.dwell_secs = 0.8;
                            } else {
                                npc.round_step = aisle_nodes.len();
                            }
                        } else {
                            nav.clear_wet_strips();
                            walker.set_destination(
                                nav,
                                chibi.base_pos,
                                npc.home_pos,
                                npc.home_yaw,
                                false,
                                WorkerAnim::Rest,
                            );
                            npc.active_job = false;
                            npc.round_step = 0;
                        }
                    }
                }
            }

            StaffRole::ShopeeCourier => {
                if state.courier_parcel_active {
                    if !npc.active_job && !walker.is_walking() {
                        let drop_pos = if pro {
                            Vec3::new(-5.35, 0.0, 2.45)
                        } else {
                            nav.door_in().pos
                        };
                        walker.set_destination(
                            nav,
                            chibi.base_pos,
                            drop_pos,
                            PI,
                            false,
                            WorkerAnim::Wait,
                        );
                        npc.active_job = true;
                        npc.dwell_secs = 2.0;
                    }
                    desired_prop = Some(PropKind::ParcelBox);
                } else if npc.active_job && !walker.is_walking() && npc.dwell_secs <= 0.0 {
                    walker.set_destination(
                        nav,
                        chibi.base_pos,
                        npc.home_pos,
                        npc.home_yaw,
                        false,
                        WorkerAnim::Rest,
                    );
                    npc.active_job = false;
                }
            }

            StaffRole::Guard => {
                // Exterior terrace guard — patrols along terrace during night shift without crossing walls
                if state.guard_night_shift && !walker.is_walking() && npc.dwell_secs <= 0.0 {
                    let target = if npc.round_step % 2 == 0 {
                        Vec3::new(npc.home_pos.x, 0.0, npc.home_pos.z + 2.8)
                    } else {
                        npc.home_pos
                    };
                    walker.waypoints.clear();
                    walker.waypoints.push(target);
                    walker.cursor = 0;
                    walker.arrive_yaw = -FRAC_PI_2;
                    walker.arrive_sit = false;
                    walker.arrive_anim = WorkerAnim::Rest;
                    npc.round_step = npc.round_step.wrapping_add(1);
                    npc.dwell_secs = 3.0;
                }
            }

            StaffRole::Intern => {
                if let Some(mut tenure) = intern_tenure {
                    tenure.turns_completed = state.intern_turns;
                    tenure.active_days = state.intern_days;
                    if !tenure.permanent && tenure.turns_completed >= 20 && tenure.active_days >= 2 {
                        tenure.permanent = true;
                        state.intern_permanent = true;
                        npc.active_job = true;
                        npc.dwell_secs = 2.5;
                        let ceremony_spot = if pro {
                            Vec3::new(5.5, 0.0, 8.22)
                        } else {
                            nav.station_spot_for(StationKind::Meeting, 0).pos
                        };
                        walker.set_destination(
                            nav,
                            chibi.base_pos,
                            ceremony_spot,
                            PI,
                            false,
                            WorkerAnim::Wait,
                        );
                    } else if npc.active_job && !walker.is_walking() && npc.dwell_secs <= 0.0 {
                        walker.set_destination(
                            nav,
                            chibi.base_pos,
                            npc.home_pos,
                            npc.home_yaw,
                            true,
                            WorkerAnim::Work,
                        );
                        npc.active_job = false;
                    }
                    if npc.active_job {
                        desired_prop = Some(PropKind::Paper);
                    }
                }
            }

            StaffRole::FarewellGuest => match state.farewell_stage {
                FarewellStage::Idle => {}
                FarewellStage::Packing => {
                    desired_prop = Some(PropKind::HomeBox);
                    if !walker.is_walking() && !npc.active_job {
                        let meet = nav.station_spot_for(StationKind::Meeting, 2);
                        walker.set_destination(
                            nav,
                            chibi.base_pos,
                            meet.pos,
                            meet.face,
                            false,
                            WorkerAnim::Rest,
                        );
                        npc.active_job = true;
                    }
                }
                FarewellStage::MeetingCard => {
                    desired_prop = Some(PropKind::Cake);
                }
                FarewellStage::Leaving => {
                    desired_prop = Some(PropKind::HomeBox);
                    if !walker.is_walking() {
                        let door = nav.door_in();
                        walker.set_destination(
                            nav,
                            chibi.base_pos,
                            door.pos,
                            door.face,
                            false,
                            WorkerAnim::Rest,
                        );
                    }
                }
                FarewellStage::Unpacking => {
                    desired_prop = Some(PropKind::HomeBox);
                    if !walker.is_walking() {
                        walker.set_destination(
                            nav,
                            chibi.base_pos,
                            npc.home_pos,
                            npc.home_yaw,
                            true,
                            WorkerAnim::Work,
                        );
                        state.farewell_stage = FarewellStage::Idle;
                        npc.active_job = false;
                    }
                }
            },
        }

        sync_lazy_prop(&mut commands, cache, pal, mats, ent, active_prop, desired_prop);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::backdrops::BackdropKind;
    use crate::chibi_props::PropKind;
    use crate::layout::OfficeLayout;
    use crate::palette::WorldEntity;
    use crate::people::step_nav_walkers_system;
    use crate::{
        BackdropAssets, OfficeIbl, SharedFontEngine, spawn_full_office_world,
        unload_full_office_world,
    };
    use std::path::PathBuf;
    use std::time::Duration;

    fn build_pro_world() -> World {
        let assets_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../assets");
        let mut world = World::new();
        world.insert_resource(Assets::<Mesh>::default());
        world.insert_resource(Assets::<StandardMaterial>::default());
        world.insert_resource(Assets::<Image>::default());
        world.insert_resource(SharedFontEngine::new(assets_dir.clone()));
        world.insert_resource(StaffCastState::default());
        world.insert_resource(Time::<()>::default());

        world.resource_scope(|world, mut meshes: Mut<Assets<Mesh>>| {
            world.resource_scope(|world, mut mats: Mut<Assets<StandardMaterial>>| {
                world.resource_scope(|world, mut imgs: Mut<Assets<Image>>| {
                    world.resource_scope(|world, mut shared_fonts: Mut<SharedFontEngine>| {
                        let fonts = shared_fonts.get_or_init();
                        let mut cmds = world.commands();
                        spawn_full_office_world(
                            &mut cmds,
                            &mut meshes,
                            &mut mats,
                            &mut imgs,
                            fonts,
                            &assets_dir,
                            OfficeLayout::Pro,
                            BackdropKind::City,
                            ChibiLook::Pixar,
                            false,
                        );
                    });
                });
            });
        });
        world.flush();
        world
    }

    #[test]
    fn spawn_staff_cast_spawns_all_office_roles_with_orange_intern_lanyard_and_zero_leaks_on_unload()
    {
        let mut world = build_pro_world();

        let roles: Vec<StaffRole> = world
            .query::<&OfficeStaffNpc>()
            .iter(&world)
            .map(|n| n.role)
            .collect();
        for expected in [
            StaffRole::Director,
            StaffRole::Hr,
            StaffRole::Accountant,
            StaffRole::Receptionist,
            StaffRole::Cleaner,
            StaffRole::ShopeeCourier,
            StaffRole::Guard,
            StaffRole::Intern,
            StaffRole::FarewellGuest,
        ] {
            assert!(
                roles.contains(&expected),
                "missing expected staff role {expected:?}; spawned={roles:?}"
            );
        }

        let intern = world
            .query::<&InternTenure>()
            .single(&world)
            .expect("Intern must have InternTenure component");
        assert_eq!(intern.lanyard_color, "#F28C28");
        assert!(!intern.permanent);

        // Full unload must leave 0 WorldEntity and 0 leaked meshes, materials, or images.
        let world_ents: Vec<Entity> = world
            .query_filtered::<Entity, With<WorldEntity>>()
            .iter(&world)
            .collect();
        world.resource_scope(|world, mut meshes: Mut<Assets<Mesh>>| {
            world.resource_scope(|world, mut mats: Mut<Assets<StandardMaterial>>| {
                world.resource_scope(|world, mut imgs: Mut<Assets<Image>>| {
                    let mut pal = world.remove_resource::<WorldPalette>();
                    let mut ba = world.remove_resource::<BackdropAssets>();
                    let ibl = world.remove_resource::<OfficeIbl>();
                    let cm = world.remove_resource::<ChibiMeshCache>();
                    let mut cmds = world.commands();
                    unload_full_office_world(
                        &mut cmds,
                        world_ents,
                        pal.as_mut(),
                        ba.as_mut(),
                        ibl.as_ref(),
                        cm.as_ref(),
                        &mut meshes,
                        &mut mats,
                        &mut imgs,
                    );
                });
            });
        });
        world.flush();

        assert_eq!(
            world
                .query_filtered::<Entity, With<WorldEntity>>()
                .iter(&world)
                .count(),
            0
        );
        assert_eq!(world.resource::<Assets<Mesh>>().len(), 0);
        assert_eq!(world.resource::<Assets<StandardMaterial>>().len(), 0);
        assert_eq!(world.resource::<Assets<Image>>().len(), 0);
    }

    #[test]
    fn staff_roles_routines_step_nav_walks_lazy_props_and_20_turn_intern_contract() {
        let mut world = build_pro_world();
        let mut sched = Schedule::default();
        sched.add_systems((step_staff_roles_system, step_nav_walkers_system).chain());

        // Trigger Director 10:00 rounds, HR birthday cake, Shopee parcel drop-off,
        // Intern 20-turn contract ceremony, and Farewell packing box.
        {
            let mut st = world.resource_mut::<StaffCastState>();
            st.clock_minutes = 10 * 60;
            st.hr_birthday_active = true;
            st.courier_parcel_active = true;
            st.intern_turns = 20;
            st.intern_days = 2;
            st.farewell_stage = FarewellStage::Packing;
        }
        world
            .resource_mut::<Time>()
            .advance_by(Duration::from_millis(50));
        sched.run(&mut world);
        world.flush();

        // Trigger 17:30 Accountant report on next tick.
        world.resource_mut::<StaffCastState>().clock_minutes = 17 * 60 + 30;
        world
            .resource_mut::<Time>()
            .advance_by(Duration::from_millis(50));
        sched.run(&mut world);
        world.flush();

        let nav = world.resource::<NavGraph>().clone();
        let mut checked_director = false;
        let mut checked_hr = false;
        let mut checked_accountant = false;
        let mut checked_courier = false;
        let mut checked_farewell = false;

        for (npc, chibi, walker, prop) in world
            .query::<(
                &OfficeStaffNpc,
                &ChibiWorker,
                &NavWalker,
                Option<&ActivePropEntity>,
            )>()
            .iter(&world)
        {
            // Every active walking staff route must be strictly wall-safe
            if walker.is_walking() {
                let mut prev = chibi.base_pos;
                for &wp in &walker.waypoints[walker.cursor..] {
                    assert!(
                        !nav.crosses_wall(prev, wp),
                        "{:?} nav walk crossed a partition wall: {prev:?} -> {wp:?}",
                        npc.role
                    );
                    prev = wp;
                }
            }
            match npc.role {
                StaffRole::Director => {
                    assert!(walker.is_walking(), "Director should walk 10:00 office rounds");
                    assert_eq!(prop.map(|p| p.kind), Some(PropKind::Folder));
                    checked_director = true;
                }
                StaffRole::Hr => {
                    assert!(
                        walker.is_walking(),
                        "HR should walk birthday cake to meeting room"
                    );
                    assert_eq!(prop.map(|p| p.kind), Some(PropKind::Cake));
                    checked_hr = true;
                }
                StaffRole::Accountant => {
                    assert!(
                        walker.is_walking(),
                        "Accountant should walk 17:30 report to Director"
                    );
                    assert_eq!(prop.map(|p| p.kind), Some(PropKind::Paper));
                    checked_accountant = true;
                }
                StaffRole::ShopeeCourier => {
                    assert!(
                        walker.is_walking(),
                        "Shopee courier should walk parcel to lobby"
                    );
                    assert_eq!(prop.map(|p| p.kind), Some(PropKind::ParcelBox));
                    checked_courier = true;
                }
                StaffRole::FarewellGuest => {
                    assert_eq!(prop.map(|p| p.kind), Some(PropKind::HomeBox));
                    checked_farewell = true;
                }
                _ => {}
            }
        }
        assert!(
            checked_director
                && checked_hr
                && checked_accountant
                && checked_courier
                && checked_farewell
        );
        assert!(
            world.resource::<StaffCastState>().intern_permanent,
            "Intern reaching 20 turns over 2+ days must receive permanent contract stamp"
        );
        let intern = world.query::<&InternTenure>().single(&world).unwrap();
        assert!(intern.permanent);
    }
}
