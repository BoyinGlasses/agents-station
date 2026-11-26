use crate::chibi::{
    ChibiConfig, ChibiDims, ChibiLook, ChibiRig, ChibiWorker, OutfitKind, StatusHalo, WorkerAnim,
    compute_chibi_pose, halo_material_for_anim, spawn_chibi,
};
use crate::chibi_meshes::{ChibiMeshCache, HairStyle};
use crate::chibi_props::{ActivePropEntity, PropKind, set_active_prop};
use crate::desks::{DeskScreen, ScreenMaterials, screen_kind_for_session};
use crate::layout::{LayoutSpec, OfficeLayout};
use crate::nav::{NavGraph, StationKind};
use crate::palette::{WorldEntity, WorldPalette};
use bevy::prelude::*;
use station_canvas2d::office_surfaces::ScreenKind;
use std::collections::{HashMap, HashSet};
use std::f32::consts::{FRAC_PI_2, PI};

/// Oracle subagent chibi scale (`workers.js:100`: `scale: 0.78`).
pub const SUBAGENT_SCALE: f32 = 0.78;
/// Number of glowing dashed segments rendered per active parent -> subagent `team-lines` link.
pub const TEAM_LINE_DASHES: usize = 8;

/// Marker on static preview seated agents spawned by `spawn_office_cast` so live session sync
/// despawns them cleanly while `--capture-scene` static renders keep their preview cast.
#[derive(Component, Debug, Clone, Copy)]
pub struct PreviewCastWorker;

/// Marker on dynamically synced 3D workers tied 1:1 to a live `OfficeState` session.
#[derive(Component, Debug, Clone)]
pub struct LiveSessionWorker {
    pub session_id: String,
    pub agent: String,
    pub name: String,
    pub desk_index: usize,
    pub status: String,
    pub tool_label: Option<String>,
    pub git_dirty: usize,
}

/// Snapshot of an active subagent belonging to a parent live session.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SubagentSpec {
    pub id: String,
    pub label: String,
    pub doing: String,
}

/// Snapshot describing a live session to reflect onto a 3D desk worker and monitor screen.
#[derive(Debug, Clone)]
pub struct LiveWorkerSpec {
    pub session_id: String,
    pub agent: String,
    pub name: String,
    pub status: String,
    pub desk_hint: usize,
    pub tool_kind: Option<String>,
    pub tool_label: Option<String>,
    pub git_dirty: usize,
    pub subagents: Vec<SubagentSpec>,
}

/// Marker on a `0.78x`-scaled subagent chibi linked to a parent live session worker.
#[derive(Component, Debug, Clone)]
pub struct SubagentWorker {
    pub subagent_id: String,
    pub parent_session_id: String,
    pub parent_entity: Entity,
    pub label: String,
    pub doing: String,
}

/// Dashed glowing segment connecting a parent worker to one of its active subagents (`team-lines.js`).
#[derive(Component, Debug, Clone, Copy)]
pub struct TeamLineDash {
    pub parent_entity: Entity,
    pub subagent_entity: Entity,
    pub dash_index: usize,
    pub dash_count: usize,
}

/// Per-entity uniform chibi scale preserved during animation (`1.0` for adults, `0.78` for subagents).
#[derive(Component, Debug, Clone, Copy)]
pub struct ChibiScale(pub f32);

/// Waypoint walker following a `NavGraph` doorway path without crossing partition walls.
#[derive(Component, Debug, Clone)]
pub struct NavWalker {
    pub waypoints: Vec<Vec3>,
    pub cursor: usize,
    pub speed: f32,
    pub arrive_yaw: f32,
    pub arrive_sit: bool,
    pub arrive_anim: WorkerAnim,
    pub despawn_on_arrive: bool,
}

impl NavWalker {
    pub fn idle_at(yaw: f32, sit: bool, anim: WorkerAnim) -> Self {
        Self {
            waypoints: Vec::new(),
            cursor: 0,
            speed: 2.7,
            arrive_yaw: yaw,
            arrive_sit: sit,
            arrive_anim: anim,
            despawn_on_arrive: false,
        }
    }

    pub fn walk_to(
        nav: &NavGraph,
        from: Vec3,
        to: Vec3,
        arrive_yaw: f32,
        arrive_sit: bool,
        arrive_anim: WorkerAnim,
        speed: f32,
    ) -> Self {
        let waypoints = nav.shortest_path(from, to);
        Self {
            waypoints,
            cursor: 0,
            speed: speed.max(0.4),
            arrive_yaw,
            arrive_sit,
            arrive_anim,
            despawn_on_arrive: false,
        }
    }

    pub fn set_destination(
        &mut self,
        nav: &NavGraph,
        from: Vec3,
        to: Vec3,
        arrive_yaw: f32,
        arrive_sit: bool,
        arrive_anim: WorkerAnim,
    ) {
        self.waypoints = nav.shortest_path(from, to);
        self.cursor = 0;
        self.arrive_yaw = arrive_yaw;
        self.arrive_sit = arrive_sit;
        self.arrive_anim = arrive_anim;
        self.despawn_on_arrive = false;
    }

    pub fn is_walking(&self) -> bool {
        self.cursor < self.waypoints.len()
    }
}

/// High-level lifecycle stage for a live session worker (`workers.js`).
#[derive(Component, Debug, Clone, Copy, PartialEq, Eq)]
pub enum WorkerLifecycle {
    Arriving,
    Seated,
    StationWalk(StationKind),
    Resting,
    Farewell,
}

/// Floating `"z"` sleep indicator above a napping chibi (`workers.js:56-69`).
#[derive(Component, Debug, Clone, Copy)]
pub struct SleepZFloater {
    pub owner: Entity,
    pub base_x: f32,
    pub base_z: f32,
    pub age: f32,
    pub max_age: f32,
}

/// Reusable scratch buffers for `sync_live_workers_and_screens` (zero per-frame heap churn).
#[derive(Default)]
pub struct LiveSyncScratch {
    assigned_desks: HashSet<usize>,
    desired_indices: HashMap<String, (usize, usize)>,
    desk_screen_kinds: HashMap<usize, ScreenKind>,
    ordered_indices: Vec<usize>,
    existing_ids: HashSet<String>,
    parent_entities: HashMap<String, (Entity, Vec3, f32)>,
    desired_sub_ids: HashMap<String, (String, usize)>,
    existing_sub_ids: HashSet<String>,
    despawned_subs: HashSet<Entity>,
}

pub fn anim_for_status(status: &str) -> WorkerAnim {
    match status {
        "work" | "working" => WorkerAnim::Work,
        "wait" | "waiting" => WorkerAnim::Wait,
        "rest" | "idle" | "sleep" => WorkerAnim::Sleep,
        _ => WorkerAnim::Rest,
    }
}

/// Maps active tool kind/label to an office station (`Read` -> Library, `Web` -> Globe,
/// `Bash` -> Server Racks, `Plan`/`Todo` -> Whiteboard, `Coffee`/`Break` -> Pantry/Coffee).
pub fn tool_station_for(tool_kind: Option<&str>, tool_label: Option<&str>) -> Option<StationKind> {
    let k = tool_kind.unwrap_or_default().trim().to_ascii_lowercase();
    match k.as_str() {
        "read" | "grep" | "glob" | "search" => return Some(StationKind::Library),
        "web" | "fetch" | "browse" => return Some(StationKind::Globe),
        "bash" | "cmd" | "shell" | "run" | "test" => return Some(StationKind::Server),
        "plan" | "todo" => return Some(StationKind::Board),
        "coffee" | "break" => return Some(StationKind::Coffee),
        _ => {}
    }
    let l = tool_label.unwrap_or_default().trim().to_ascii_lowercase();
    if l.contains("read") || l.contains("grep") || l.contains("glob") {
        Some(StationKind::Library)
    } else if l.contains("web") || l.contains("http") || l.contains("fetch") {
        Some(StationKind::Globe)
    } else if l.contains("bash") || l.contains("cargo") || l.contains("test") || l.contains("npm") {
        Some(StationKind::Server)
    } else if l.contains("plan") || l.contains("todo") {
        Some(StationKind::Board)
    } else if l.contains("coffee") || l.contains("break") {
        Some(StationKind::Coffee)
    } else {
        None
    }
}

fn prop_for_worker_state(station: Option<StationKind>, anim: WorkerAnim) -> Option<PropKind> {
    match station {
        Some(StationKind::Library) => Some(PropKind::Book),
        Some(StationKind::Board) => Some(PropKind::Folder),
        Some(StationKind::Coffee) => Some(PropKind::Coffee),
        None if anim == WorkerAnim::Rest => Some(PropKind::Mug),
        _ => None,
    }
}

/// Starts a farewell walk along `NavGraph` back to the lift entrance, despawning on arrival.
pub fn trigger_worker_farewell(
    nav: &NavGraph,
    walker: &mut NavWalker,
    lifecycle: &mut WorkerLifecycle,
    current_pos: Vec3,
) {
    let door = nav.door_in();
    walker.set_destination(
        nav,
        current_pos,
        door.pos,
        door.face,
        false,
        WorkerAnim::Rest,
    );
    walker.despawn_on_arrive = true;
    *lifecycle = WorkerLifecycle::Farewell;
}

fn stable_seed(session_id: &str) -> usize {
    session_id.bytes().fold(0usize, |acc, b| {
        acc.wrapping_mul(31).wrapping_add(b as usize)
    })
}

pub const KARAOKE_SONGS: [(&str, &str); 5] = [
    ("Lạc Trôi", "Người theo hương hoa mây mù giăng lối ♪"),
    ("Sóng Gió", "Hồng trần trên đôi cánh tay ♫"),
    ("Nàng Thơ", "Em, ngày em đánh rơi nụ cười vào anh ♪"),
    ("Có Chàng Trai Viết Lên Cây", "Có chàng trai viết lên cây lời yêu thương ♫"),
    ("Bật Tình Yêu Lên", "Rót mật ngọt vào tai, bật tình yêu lên ♪"),
];

pub const TEAM_CHAT_LINES: [&str; 6] = [
    "Build xanh rồi, đi làm ly cà phê sữa đá thôi!",
    "Test pass 100%, chuẩn bị ship bản release!",
    "Trưa nay cơm tấm hay bún bò Huế đây cả nhà?",
    "Ai quên tắt dev server port 3000 kìa!",
    "RAM chạy còn dưới 200 MB luôn, mượt quá!",
    "Chiều nay 17h30 ra sân pickleball nhé!",
];

/// Spawns the active preview agent cast seated at desks + preview lounge/karaoke figures.
/// Full office staff NPCs (`Director`, `HR`, `Accountant`, `Receptionist`, `Cleaner`, `Courier`,
/// `Guard`, `Interns`, `Farewell`) are spawned by `crate::cast::staff_roles`.
pub fn spawn_office_cast(
    commands: &mut Commands,
    chibi_cache: &ChibiMeshCache,
    palette: &mut WorldPalette,
    materials: &mut Assets<StandardMaterial>,
    spec: &LayoutSpec,
    look: ChibiLook,
) {
    let cast = [
        ("claude", "Claude", WorkerAnim::Work, HairStyle::Bob, OutfitKind::Hoodie),
        ("codex", "Codex", WorkerAnim::Work, HairStyle::Spiky, OutfitKind::Shirt),
        ("opencode", "OpenCode", WorkerAnim::Work, HairStyle::Plain, OutfitKind::Hoodie),
        ("zcode", "ZCode", WorkerAnim::Wait, HairStyle::Ponytail, OutfitKind::Shirt),
        ("antigravity", "Antigravity", WorkerAnim::Work, HairStyle::Curly, OutfitKind::Hoodie),
        ("claude", "Mai", WorkerAnim::Rest, HairStyle::Bun, OutfitKind::Shirt),
    ];

    for (i, (agent, name, anim, hair_style, outfit)) in cast.iter().enumerate() {
        let Some(desk) = spec.desks.get(i) else {
            break;
        };
        let mut cfg = ChibiConfig::for_agent(agent, name, i);
        cfg.look = look;
        cfg.hair_style = *hair_style;
        cfg.outfit = *outfit;
        let rot = Quat::from_rotation_y(desk.ry);
        let seat_pos = Vec3::new(desk.x, 0.0, desk.z) + rot * Vec3::new(0.0, 0.0, -0.62);
        let ent = spawn_chibi(
            commands,
            chibi_cache,
            palette,
            materials,
            &cfg,
            seat_pos,
            desk.ry,
            *anim,
            i as f32 * 0.9,
            true,
        );
        if *anim == WorkerAnim::Rest {
            set_active_prop(
                commands,
                chibi_cache,
                palette,
                materials,
                ent,
                Some(PropKind::Mug),
            );
        }
        commands.entity(ent).insert(PreviewCastWorker);
    }

    // Pro / HQ lounge gamer + karaoke singer
    if spec.layout.is_pro() {
        let mut gamer_cfg = ChibiConfig::for_agent("npc", "Huy", 12);
        gamer_cfg.shirt = "#3F7CC4";
        gamer_cfg.hair_style = HairStyle::Spiky;
        gamer_cfg.outfit = OutfitKind::Hoodie;
        gamer_cfg.accessory = Some("headphones");
        gamer_cfg.look = look;
        let gamer_ent = spawn_chibi(
            commands,
            chibi_cache,
            palette,
            materials,
            &gamer_cfg,
            Vec3::new(10.7, 0.0, 3.85),
            PI,
            WorkerAnim::Rest,
            2.2,
            true,
        );
        set_active_prop(
            commands,
            chibi_cache,
            palette,
            materials,
            gamer_ent,
            Some(PropKind::Gamepad),
        );

        if let Some([kx, kz, kry]) = spec.ktv {
            let mut sing_cfg = ChibiConfig::for_agent("npc", "An", 13);
            sing_cfg.shirt = "#FF5F9E";
            sing_cfg.hair_style = HairStyle::Bun;
            sing_cfg.outfit = OutfitKind::Shirt;
            sing_cfg.accessory = None;
            sing_cfg.look = look;
            let sing_ent = spawn_chibi(
                commands,
                chibi_cache,
                palette,
                materials,
                &sing_cfg,
                Vec3::new(kx, 0.08, kz),
                kry + PI,
                WorkerAnim::Sing,
                3.1,
                false,
            );
            set_active_prop(
                commands,
                chibi_cache,
                palette,
                materials,
                sing_ent,
                Some(PropKind::Mic),
            );
        }
    }
}

#[derive(Component)]
pub struct PatrolPath {
    pub a: Vec3,
    pub b: Vec3,
    pub speed: f32,
}

fn spawn_team_line_dashes(
    commands: &mut Commands,
    palette: &mut WorldPalette,
    materials: &mut Assets<StandardMaterial>,
    parent_entity: Entity,
    subagent_entity: Entity,
    parent_pos: Vec3,
    sub_pos: Vec3,
) {
    let dash_mat = palette.emissive(materials, "#8EC2FF", 1.35);
    let a = parent_pos + Vec3::Y * 1.1;
    let b = sub_pos + Vec3::Y * 0.8;
    let delta = b - a;
    let dist = delta.length();
    let dir = if dist > 1e-4 { delta / dist } else { Vec3::Z };
    let rot = Quat::from_rotation_arc(Vec3::Z, dir);
    let seg_len = (dist / TEAM_LINE_DASHES as f32 * 0.56).clamp(0.05, 0.22);

    for i in 0..TEAM_LINE_DASHES {
        let t = (i as f32 + 0.5) / TEAM_LINE_DASHES as f32;
        let pos = a.lerp(b, t);
        commands.spawn((
            WorldEntity,
            TeamLineDash {
                parent_entity,
                subagent_entity,
                dash_index: i,
                dash_count: TEAM_LINE_DASHES,
            },
            Mesh3d(palette.unit_box.clone()),
            MeshMaterial3d(dash_mat.clone()),
            Transform::from_translation(pos)
                .with_rotation(rot)
                .with_scale(Vec3::new(0.022, 0.022, seg_len)),
            if dist >= 0.15 {
                Visibility::Visible
            } else {
                Visibility::Hidden
            },
        ));
    }
}

/// Synchronizes dynamic 3D `LiveSessionWorker` entities, `0.78x` `SubagentWorker` chibis,
/// dashed `TeamLineDash` connectors, and `DeskScreen` monitor materials against live sessions.
#[allow(clippy::too_many_arguments, clippy::type_complexity)]
pub fn sync_live_workers_and_screens(
    commands: &mut Commands,
    chibi_cache: &ChibiMeshCache,
    palette: &mut WorldPalette,
    materials: &mut Assets<StandardMaterial>,
    screen_mats: &ScreenMaterials,
    nav: &NavGraph,
    _layout: OfficeLayout,
    look: ChibiLook,
    specs: &[LiveWorkerSpec],
    scratch: &mut LiveSyncScratch,
    preview_q: &Query<Entity, With<PreviewCastWorker>>,
    workers_q: &mut Query<(
        Entity,
        &mut LiveSessionWorker,
        &mut ChibiWorker,
        Option<&mut NavWalker>,
        Option<&mut WorkerLifecycle>,
        Option<&ActivePropEntity>,
        Option<&Children>,
    )>,
    subagents_q: &mut Query<
        (Entity, &mut SubagentWorker, &ChibiWorker),
        Without<LiveSessionWorker>,
    >,
    dashes_q: &Query<(Entity, &TeamLineDash)>,
    halo_q: &mut Query<&mut MeshMaterial3d<StandardMaterial>, (With<StatusHalo>, Without<DeskScreen>)>,
    screens_q: &mut Query<(&DeskScreen, &mut MeshMaterial3d<StandardMaterial>), Without<StatusHalo>>,
) {
    // 1. Remove any static preview seated workers once live sync takes ownership.
    for ent in preview_q.iter() {
        commands.entity(ent).try_despawn();
    }

    let desk_count = nav.desk_count().max(1);

    // 2. Assign each live session to a deterministic, non-colliding desk slot (reusing scratch buffers).
    scratch.assigned_desks.clear();
    scratch.desired_indices.clear();
    scratch.desk_screen_kinds.clear();
    scratch.ordered_indices.clear();
    scratch.existing_ids.clear();
    scratch.parent_entities.clear();
    scratch.desired_sub_ids.clear();
    scratch.existing_sub_ids.clear();
    scratch.despawned_subs.clear();

    scratch.ordered_indices.extend(0..specs.len());
    scratch.ordered_indices.sort_by(|&ia, &ib| {
        specs[ia]
            .desk_hint
            .cmp(&specs[ib].desk_hint)
            .then_with(|| specs[ia].session_id.cmp(&specs[ib].session_id))
    });

    for &spec_idx in &scratch.ordered_indices {
        let s = &specs[spec_idx];
        if scratch.desired_indices.contains_key(s.session_id.as_str()) {
            continue;
        }
        let preferred = s.desk_hint % desk_count;
        let desk_idx = if scratch.assigned_desks.insert(preferred) {
            preferred
        } else if let Some(free) = (0..desk_count)
            .find(|&d| d != 6 && d != 7 && !scratch.assigned_desks.contains(&d))
            .or_else(|| (0..desk_count).find(|d| !scratch.assigned_desks.contains(d)))
        {
            scratch.assigned_desks.insert(free);
            free
        } else {
            continue;
        };
        scratch
            .desired_indices
            .insert(s.session_id.clone(), (spec_idx, desk_idx));
        scratch.desk_screen_kinds.insert(
            desk_idx,
            screen_kind_for_session(
                &s.status,
                s.tool_kind.as_deref(),
                s.tool_label.as_deref(),
            ),
        );
    }

    // 3. Reconcile existing LiveSessionWorker entities: update in-place or despawn ended ones.
    for (ent, mut live, mut chibi, walker, lifecycle, active_prop, children) in workers_q.iter_mut() {
        let Some(&(spec_idx, target_desk)) = scratch.desired_indices.get(live.session_id.as_str()) else {
            commands.entity(ent).try_despawn();
            continue;
        };
        let spec = &specs[spec_idx];
        if live.desk_index != target_desk || live.agent != spec.agent {
            commands.entity(ent).try_despawn();
            continue;
        }
        scratch.existing_ids.insert(live.session_id.clone());
        let next_anim = anim_for_status(&spec.status);
        let desired_station = if spec.status == "work" {
            tool_station_for(spec.tool_kind.as_deref(), spec.tool_label.as_deref())
        } else {
            None
        };

        // Route via NavGraph if the active tool station or resting state changed.
        if let (Some(mut wlk), Some(mut lc)) = (walker, lifecycle) {
            let current_station = match *lc {
                WorkerLifecycle::StationWalk(st) => Some(st),
                _ => None,
            };
            let is_arriving = *lc == WorkerLifecycle::Arriving && wlk.is_walking();
            if !is_arriving {
                if let Some(st) = desired_station {
                    if current_station != Some(st) {
                        let spot = nav.station_spot_for(st, stable_seed(&spec.session_id));
                        wlk.set_destination(
                            nav,
                            chibi.base_pos,
                            spot.pos,
                            spot.face,
                            spot.sit,
                            WorkerAnim::Work,
                        );
                        *lc = WorkerLifecycle::StationWalk(st);
                    }
                } else if current_station.is_some() {
                    if let Some(desk_spot) = nav.desk_spot(target_desk) {
                        wlk.set_destination(
                            nav,
                            chibi.base_pos,
                            desk_spot.pos,
                            desk_spot.face,
                            true,
                            next_anim,
                        );
                    }
                    *lc = if next_anim == WorkerAnim::Sleep {
                        WorkerLifecycle::Resting
                    } else {
                        WorkerLifecycle::Seated
                    };
                } else {
                    if !wlk.is_walking() && chibi.anim != next_anim {
                        chibi.anim = next_anim;
                    }
                    *lc = if next_anim == WorkerAnim::Sleep {
                        WorkerLifecycle::Resting
                    } else {
                        WorkerLifecycle::Seated
                    };
                }
            }
        } else if chibi.anim != next_anim {
            chibi.anim = next_anim;
        }

        // Update halo color only if needed.
        if let Some(kids) = children {
            let halo_mat = halo_material_for_anim(palette, materials, next_anim);
            for child in kids.iter() {
                if let Ok(mut m) = halo_q.get_mut(child)
                    && m.0 != halo_mat
                {
                    m.0 = halo_mat.clone();
                }
            }
        }

        // Lazy prop attachment: ONLY call set_active_prop when desired prop actually changed.
        let desired_prop = prop_for_worker_state(desired_station, next_anim);
        if active_prop.map(|p| p.kind) != desired_prop {
            set_active_prop(commands, chibi_cache, palette, materials, ent, desired_prop);
        }

        if live.status != spec.status {
            live.status.clone_from(&spec.status);
        }
        if live.tool_label != spec.tool_label {
            live.tool_label.clone_from(&spec.tool_label);
        }
        if live.name != spec.name {
            live.name.clone_from(&spec.name);
        }
        live.git_dirty = spec.git_dirty;
        if chibi.name != spec.name {
            chibi.name.clone_from(&spec.name);
        }

        let desk_spot_pos = nav
            .desk_spot(target_desk)
            .map(|d| (d.pos, d.face))
            .unwrap_or((chibi.base_pos, chibi.base_yaw));
        scratch
            .parent_entities
            .insert(live.session_id.clone(), (ent, desk_spot_pos.0, desk_spot_pos.1));
    }

    // 4. Spawn newly arrived live session workers at lift entrance walking via NavGraph to desk.
    let door_in = nav.door_in();
    for (sid, &(spec_idx, desk_idx)) in &scratch.desired_indices {
        if scratch.existing_ids.contains(sid.as_str()) {
            continue;
        }
        let spec = &specs[spec_idx];
        let Some(desk_spot) = nav.desk_spot(desk_idx) else {
            continue;
        };
        let seed = stable_seed(&spec.session_id);
        let mut cfg = ChibiConfig::for_agent(&spec.agent, &spec.name, seed);
        cfg.look = look;
        let anim = anim_for_status(&spec.status);
        let root = spawn_chibi(
            commands,
            chibi_cache,
            palette,
            materials,
            &cfg,
            door_in.pos,
            FRAC_PI_2,
            WorkerAnim::Walk,
            (desk_idx as f32) * 0.75,
            false,
        );
        let walker = NavWalker::walk_to(
            nav,
            door_in.pos,
            desk_spot.pos,
            desk_spot.face,
            true,
            anim,
            2.7,
        );
        commands.entity(root).insert((
            LiveSessionWorker {
                session_id: spec.session_id.clone(),
                agent: spec.agent.clone(),
                name: spec.name.clone(),
                desk_index: desk_idx,
                status: spec.status.clone(),
                tool_label: spec.tool_label.clone(),
                git_dirty: spec.git_dirty,
            },
            ChibiScale(1.0),
            walker,
            WorkerLifecycle::Arriving,
        ));
        scratch
            .parent_entities
            .insert(spec.session_id.clone(), (root, desk_spot.pos, desk_spot.face));
    }

    // 5. Reconcile 0.78x SubagentWorker chibis & dashed TeamLineDash links.
    for &(spec_idx, _) in scratch.desired_indices.values() {
        let spec = &specs[spec_idx];
        for (sub_idx, sub) in spec.subagents.iter().enumerate() {
            scratch
                .desired_sub_ids
                .insert(sub.id.clone(), (spec.session_id.clone(), sub_idx));
        }
    }

    for (ent, mut sub, _chibi) in subagents_q.iter_mut() {
        let keep = scratch
            .desired_sub_ids
            .get(sub.subagent_id.as_str())
            .and_then(|(parent_sid, sub_idx)| {
                let &(parent_ent, _, _) = scratch.parent_entities.get(parent_sid.as_str())?;
                if sub.parent_entity == parent_ent && sub.parent_session_id == *parent_sid {
                    Some((parent_sid.as_str(), *sub_idx))
                } else {
                    None
                }
            });
        if let Some((parent_sid, sub_idx)) = keep {
            scratch.existing_sub_ids.insert(sub.subagent_id.clone());
            if let Some(&(spec_idx, _)) = scratch.desired_indices.get(parent_sid)
                && let Some(sub_spec) = specs[spec_idx].subagents.get(sub_idx)
            {
                if sub.label != sub_spec.label {
                    sub.label.clone_from(&sub_spec.label);
                }
                if sub.doing != sub_spec.doing {
                    sub.doing.clone_from(&sub_spec.doing);
                }
            }
        } else {
            scratch.despawned_subs.insert(ent);
            commands.entity(ent).try_despawn();
        }
    }

    // Despawn TeamLineDash entities belonging to removed subagents or missing parents.
    for (dash_ent, dash) in dashes_q.iter() {
        let parent_alive = scratch
            .parent_entities
            .values()
            .any(|&(pe, _, _)| pe == dash.parent_entity);
        if scratch.despawned_subs.contains(&dash.subagent_entity) || !parent_alive {
            commands.entity(dash_ent).try_despawn();
        }
    }

    // Spawn newly arrived 0.78x subagent chibis walking from lift -> parent briefing spot + dashed team-lines.
    for &(spec_idx, _) in scratch.desired_indices.values() {
        let spec = &specs[spec_idx];
        let Some(&(parent_ent, parent_pos, parent_yaw)) =
            scratch.parent_entities.get(spec.session_id.as_str())
        else {
            continue;
        };
        for (sub_idx, sub_spec) in spec.subagents.iter().enumerate() {
            if !scratch.existing_sub_ids.insert(sub_spec.id.clone()) {
                continue;
            }
            let seed = stable_seed(&sub_spec.id);
            let mut cfg = ChibiConfig::for_agent(&spec.agent, &sub_spec.label, seed);
            cfg.look = look;
            cfg.pants = "#4C566A";
            cfg.accessory = if (seed % 3) == 0 {
                Some("glasses")
            } else {
                None
            };
            let brief_spot = nav.near_boss_spot(parent_pos, parent_yaw, sub_idx);
            let sub_ent = spawn_chibi(
                commands,
                chibi_cache,
                palette,
                materials,
                &cfg,
                door_in.pos,
                FRAC_PI_2,
                WorkerAnim::Walk,
                (sub_idx as f32) * 0.65,
                false,
            );
            let walker = NavWalker::walk_to(
                nav,
                door_in.pos,
                brief_spot.pos,
                brief_spot.face,
                false,
                WorkerAnim::Wait,
                2.85,
            );
            commands.entity(sub_ent).insert((
                SubagentWorker {
                    subagent_id: sub_spec.id.clone(),
                    parent_session_id: spec.session_id.clone(),
                    parent_entity: parent_ent,
                    label: sub_spec.label.clone(),
                    doing: sub_spec.doing.clone(),
                },
                ChibiScale(SUBAGENT_SCALE),
                Transform::from_translation(door_in.pos)
                    .with_rotation(Quat::from_rotation_y(FRAC_PI_2))
                    .with_scale(Vec3::splat(SUBAGENT_SCALE)),
                walker,
            ));
            spawn_team_line_dashes(
                commands,
                palette,
                materials,
                parent_ent,
                sub_ent,
                parent_pos,
                door_in.pos,
            );
        }
    }

    // 6. Swap DeskScreen materials based on each desk's active session tool (or Off if vacant).
    if let Some(off_mat) = screen_mats.by_kind.get(&ScreenKind::Off) {
        for (ds, mut mat) in screens_q.iter_mut() {
            let kind = scratch
                .desk_screen_kinds
                .get(&ds.desk_index)
                .copied()
                .unwrap_or(ScreenKind::Off);
            let desired_mat = screen_mats.by_kind.get(&kind).unwrap_or(off_mat);
            if mat.0 != *desired_mat {
                mat.0 = desired_mat.clone();
            }
        }
    }
}

/// Direct `World` helper used in unit tests and headless synchronization.
pub fn sync_live_workers_in_world(world: &mut World, specs: &[LiveWorkerSpec], look: ChibiLook) {
    use bevy::ecs::system::SystemState;

    #[allow(clippy::type_complexity)]
    let mut state: SystemState<(
        Commands,
        Option<Res<ChibiMeshCache>>,
        Option<ResMut<WorldPalette>>,
        ResMut<Assets<StandardMaterial>>,
        Option<Res<ScreenMaterials>>,
        Option<Res<NavGraph>>,
        Option<Res<OfficeLayout>>,
        Query<Entity, With<PreviewCastWorker>>,
        Query<(
            Entity,
            &mut LiveSessionWorker,
            &mut ChibiWorker,
            Option<&mut NavWalker>,
            Option<&mut WorkerLifecycle>,
            Option<&ActivePropEntity>,
            Option<&Children>,
        )>,
        Query<(Entity, &mut SubagentWorker, &ChibiWorker), Without<LiveSessionWorker>>,
        Query<(Entity, &TeamLineDash)>,
        Query<&mut MeshMaterial3d<StandardMaterial>, (With<StatusHalo>, Without<DeskScreen>)>,
        Query<(&DeskScreen, &mut MeshMaterial3d<StandardMaterial>), Without<StatusHalo>>,
    )> = SystemState::new(world);

    let mut scratch = LiveSyncScratch::default();
    {
        let (
            mut commands,
            chibi_cache,
            pal,
            mut mats,
            scr_mats,
            nav_res,
            layout,
            preview_q,
            mut workers_q,
            mut subagents_q,
            dashes_q,
            mut halo_q,
            mut screens_q,
        ) = state.get_mut(world);
        if let (Some(cache), Some(mut pal), Some(scr_mats)) = (chibi_cache, pal, scr_mats) {
            let lay = layout.map(|l| *l).unwrap_or(OfficeLayout::Pro);
            let fallback_nav;
            let nav_ref: &NavGraph = match nav_res.as_deref() {
                Some(n) => n,
                None => {
                    fallback_nav = NavGraph::for_layout(lay);
                    &fallback_nav
                }
            };
            sync_live_workers_and_screens(
                &mut commands,
                &cache,
                &mut pal,
                &mut mats,
                &scr_mats,
                nav_ref,
                lay,
                look,
                specs,
                &mut scratch,
                &preview_q,
                &mut workers_q,
                &mut subagents_q,
                &dashes_q,
                &mut halo_q,
                &mut screens_q,
            );
        }
    }
    state.apply(world);
    world.flush();
}

/// Advances all `NavWalker` entities (live workers, `0.78x` subagents, and office staff NPCs)
/// smoothly along their `NavGraph` doorway waypoints with zero per-frame allocations.
pub fn step_nav_walkers_system(
    time: Res<Time>,
    mut commands: Commands,
    mut walkers: Query<(
        Entity,
        &mut NavWalker,
        &mut ChibiWorker,
        Option<&mut WorkerLifecycle>,
    )>,
) {
    let dt = time.delta_secs().clamp(0.0, 0.1);
    if dt <= 0.0 {
        return;
    }
    for (ent, mut walker, mut chibi, lifecycle) in &mut walkers {
        if !walker.is_walking() {
            continue;
        }
        let target = walker.waypoints[walker.cursor];
        let dx = target.x - chibi.base_pos.x;
        let dz = target.z - chibi.base_pos.z;
        let dist = dx.hypot(dz);
        let step = walker.speed * dt;

        chibi.sitting = false;
        chibi.anim = WorkerAnim::Walk;

        if dist <= step {
            chibi.base_pos.x = target.x;
            chibi.base_pos.z = target.z;
            walker.cursor += 1;
            if !walker.is_walking() {
                walker.waypoints.clear();
                walker.cursor = 0;
                if walker.despawn_on_arrive {
                    commands.entity(ent).try_despawn();
                    continue;
                }
                chibi.base_yaw = walker.arrive_yaw;
                chibi.sitting = walker.arrive_sit;
                chibi.anim = walker.arrive_anim;
                if let Some(mut lc) = lifecycle
                    && *lc == WorkerLifecycle::Arriving
                {
                    *lc = if walker.arrive_anim == WorkerAnim::Sleep {
                        WorkerLifecycle::Resting
                    } else {
                        WorkerLifecycle::Seated
                    };
                }
            }
        } else {
            chibi.base_pos.x += (dx / dist) * step;
            chibi.base_pos.z += (dz / dist) * step;
            chibi.base_yaw = dx.atan2(dz);
        }
    }
}

/// Updates dashed `TeamLineDash` transforms to track live parent worker & subagent positions
/// (`team-lines.js`: `y = 1.1` on parent -> `y = 0.8` on subagent).
pub fn update_team_lines_system(
    mut commands: Commands,
    workers_q: Query<&ChibiWorker>,
    mut dashes_q: Query<(Entity, &TeamLineDash, &mut Transform, &mut Visibility)>,
) {
    for (ent, dash, mut tf, mut vis) in &mut dashes_q {
        let (Ok(parent), Ok(sub)) = (
            workers_q.get(dash.parent_entity),
            workers_q.get(dash.subagent_entity),
        ) else {
            commands.entity(ent).try_despawn();
            continue;
        };
        let a = parent.base_pos + Vec3::Y * 1.1;
        let b = sub.base_pos + Vec3::Y * 0.8;
        let delta = b - a;
        let dist = delta.length();
        if dist < 0.15 {
            *vis = Visibility::Hidden;
            continue;
        }
        *vis = Visibility::Visible;
        let dir = delta / dist;
        let count = dash.dash_count.max(1) as f32;
        let t = (dash.dash_index as f32 + 0.5) / count;
        let seg_len = (dist / count * 0.56).clamp(0.05, 0.22);
        tf.translation = a.lerp(b, t);
        tf.rotation = Quat::from_rotation_arc(Vec3::Z, dir);
        tf.scale = Vec3::new(0.022, 0.022, seg_len);
    }
}

/// Spawns and animates lightweight floating `"z"` bars above napping workers (`workers.js:56-69`).
pub fn animate_sleep_z_system(
    time: Res<Time>,
    mut commands: Commands,
    mut palette: Option<ResMut<WorldPalette>>,
    mut materials: Option<ResMut<Assets<StandardMaterial>>>,
    workers: Query<(Entity, &ChibiWorker)>,
    mut floaters: Query<(Entity, &mut SleepZFloater, &mut Transform)>,
    mut spawn_accum: Local<f32>,
) {
    let dt = time.delta_secs().clamp(0.0, 0.1);
    let mut active_count = 0usize;

    for (ent, mut z, mut tf) in &mut floaters {
        z.age += dt;
        if z.age >= z.max_age || workers.get(z.owner).is_err() {
            commands.entity(ent).try_despawn();
            continue;
        }
        active_count += 1;
        let life_left = (z.max_age - z.age).max(0.0);
        tf.translation.y = 1.95 + z.age * 0.35;
        tf.translation.x = z.base_x + 0.25 + (life_left * 3.0).sin() * 0.08;
        tf.translation.z = z.base_z + 0.15;
        let s = (0.09 + z.age * 0.03) * (life_left / 0.6).min(1.0);
        tf.scale = Vec3::new(s, s * 0.35, s * 0.35);
    }

    *spawn_accum += dt;
    if *spawn_accum < 1.4 || active_count >= 16 {
        return;
    }
    *spawn_accum = 0.0;

    let (Some(pal), Some(mats)) = (palette.as_deref_mut(), materials.as_deref_mut()) else {
        return;
    };
    let z_mat = pal.emissive(mats, "#DCE8FF", 1.15);
    for (ent, w) in &workers {
        if active_count >= 16 {
            break;
        }
        if w.anim != WorkerAnim::Sleep {
            continue;
        }
        active_count += 1;
        commands.spawn((
            WorldEntity,
            SleepZFloater {
                owner: ent,
                base_x: w.base_pos.x,
                base_z: w.base_pos.z,
                age: 0.0,
                max_age: 2.2,
            },
            Mesh3d(pal.unit_box.clone()),
            MeshMaterial3d(z_mat.clone()),
            Transform::from_xyz(w.base_pos.x + 0.25, 1.95, w.base_pos.z + 0.15)
                .with_scale(Vec3::new(0.09, 0.03, 0.03)),
        ));
    }
}

/// Smoothly animates rigged Chibi characters: 2-bone arm IK for typing/resting/singing,
/// walking leg & arm swing, natural periodic blinking, breathing, and mouth expressions.
#[allow(clippy::type_complexity)]
pub fn animate_chibis_system(
    time: Res<Time>,
    workers: Query<(
        Entity,
        &ChibiWorker,
        Option<&ChibiRig>,
        Option<&PatrolPath>,
        Option<&NavWalker>,
        Option<&ChibiScale>,
        Option<&ActivePropEntity>,
    )>,
    mut transforms: Query<&mut Transform>,
    mut visibilities: Query<&mut Visibility>,
) {
    let t = time.elapsed_secs();
    for (root, w, rig, patrol, walker, scale_comp, active_prop) in &workers {
        let ph = t + w.phase;
        let blink_cycle = (t + w.phase * 1.7).rem_euclid(3.8);
        let blink = if blink_cycle < 0.14 {
            (blink_cycle / 0.14 * PI).sin()
        } else {
            0.0
        };

        let walking = patrol.is_some() || walker.is_some_and(NavWalker::is_walking);
        let anim = if walking { WorkerAnim::Walk } else { w.anim };
        let look = rig.map(|r| r.look).unwrap_or(ChibiLook::Pixar);
        let scale = scale_comp.map(|s| s.0).unwrap_or(1.0);
        let pose = compute_chibi_pose(look, anim, w.sitting && !walking, ph, blink);

        if let Ok(mut root_tf) = transforms.get_mut(root) {
            root_tf.scale = Vec3::splat(scale);
            if let Some(p) = patrol {
                let u = ((t * p.speed + w.phase).sin() * 0.5 + 0.5).clamp(0.0, 1.0);
                let next = p.a.lerp(p.b, u);
                let dir = if (t * p.speed + w.phase).cos() >= 0.0 {
                    p.b - p.a
                } else {
                    p.a - p.b
                };
                root_tf.translation = next + Vec3::Y * (pose.root_y_offset * scale);
                root_tf.rotation = Quat::from_rotation_y(dir.x.atan2(dir.z));
            } else {
                root_tf.translation = w.base_pos + Vec3::Y * (pose.root_y_offset * scale);
                let extra_yaw = if anim == WorkerAnim::Sing {
                    (t * 2.2 + w.phase).sin() * 0.18
                } else {
                    0.0
                };
                root_tf.rotation = Quat::from_rotation_y(w.base_yaw + extra_yaw);
            }
        }

        let Some(rig) = rig else {
            continue;
        };
        let d = ChibiDims::for_look(rig.look);
        let k_eye = (d.r * 0.21) / 0.068;

        if let Ok(mut tf) = transforms.get_mut(rig.hip_l) {
            tf.rotation = Quat::from_rotation_x(pose.hip_l);
        }
        if let Ok(mut tf) = transforms.get_mut(rig.hip_r) {
            tf.rotation = Quat::from_rotation_x(pose.hip_r);
        }
        if let Ok(mut tf) = transforms.get_mut(rig.knee_l) {
            tf.rotation = Quat::from_rotation_x(pose.knee_l);
        }
        if let Ok(mut tf) = transforms.get_mut(rig.knee_r) {
            tf.rotation = Quat::from_rotation_x(pose.knee_r);
        }
        if let Ok(mut tf) = transforms.get_mut(rig.spine) {
            tf.rotation = pose.spine_rot;
        }
        if let Ok(mut tf) = transforms.get_mut(rig.torso) {
            tf.scale.y = pose.breathe;
        }
        if let Ok(mut tf) = transforms.get_mut(rig.head) {
            tf.rotation = pose.head_rot;
        }
        if let Ok(mut tf) = transforms.get_mut(rig.shoulder_l) {
            tf.rotation = pose.upper_l;
        }
        if let Ok(mut tf) = transforms.get_mut(rig.elbow_l) {
            tf.rotation = pose.fore_l;
        }
        if let Ok(mut tf) = transforms.get_mut(rig.shoulder_r) {
            tf.rotation = pose.upper_r;
        }
        if let Ok(mut tf) = transforms.get_mut(rig.elbow_r) {
            tf.rotation = pose.fore_r;
        }
        if let Some(ap) = active_prop {
            match ap.kind.mount() {
                crate::chibi_props::PropMount::RightHand => {
                    if let Ok(mut tf) = transforms.get_mut(ap.entity) {
                        tf.rotation = (pose.spine_rot * pose.upper_r * pose.fore_r).inverse();
                    }
                }
                crate::chibi_props::PropMount::LeftHand => {
                    if let Ok(mut tf) = transforms.get_mut(ap.entity) {
                        tf.rotation = (pose.spine_rot * pose.upper_l * pose.fore_l).inverse();
                    }
                }
                _ => {}
            }
        }

        for ball in [rig.eye_ball_l, rig.eye_ball_r] {
            if let Ok(mut tf) = transforms.get_mut(ball) {
                tf.scale.y = 1.0 - 0.9 * pose.lid;
            }
        }
        for lash_opt in [rig.lash_l, rig.lash_r] {
            if let Some(lash) = lash_opt
                && let Ok(mut tf) = transforms.get_mut(lash)
            {
                tf.translation.y = -0.02 * k_eye * pose.lid;
                tf.scale.y = 0.07 * k_eye * 1.06 * (1.0 - 0.58 * pose.lid);
            }
        }
        if let Ok(mut v) = visibilities.get_mut(rig.mouth_smile) {
            *v = if pose.mouth_o {
                Visibility::Hidden
            } else {
                Visibility::Visible
            };
        }
        if let Ok(mut v) = visibilities.get_mut(rig.mouth_o) {
            *v = if pose.mouth_o {
                Visibility::Visible
            } else {
                Visibility::Hidden
            };
        }
        if let Ok(mut v) = visibilities.get_mut(rig.shadow) {
            *v = if pose.shadow_visible {
                Visibility::Visible
            } else {
                Visibility::Hidden
            };
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn chibi_configs_cover_all_five_agents_and_karaoke_songs() {
        for (i, ag) in ["claude", "codex", "opencode", "zcode", "antigravity"].iter().enumerate() {
            let c = ChibiConfig::for_agent(ag, ag, i);
            assert_eq!(c.agent, *ag);
            assert!(c.shirt.starts_with('#'));
        }
        assert_eq!(KARAOKE_SONGS.len(), 5);
        assert_eq!(TEAM_CHAT_LINES.len(), 6);
    }

    #[test]
    fn live_worker_sync_spawns_updates_screens_and_despawns_cleanly_without_leaks() {
        use crate::backdrops::BackdropKind;
        use crate::desks::{DeskScreen, ScreenMaterials};
        use crate::layout::OfficeLayout;
        use crate::palette::WorldEntity;
        use crate::{SharedFontEngine, spawn_full_office_world};
        use station_canvas2d::office_surfaces::ScreenKind;
        use std::path::PathBuf;

        let assets_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../assets");
        let mut world = World::new();
        world.insert_resource(Assets::<Mesh>::default());
        world.insert_resource(Assets::<StandardMaterial>::default());
        world.insert_resource(Assets::<Image>::default());
        world.insert_resource(SharedFontEngine::new(assets_dir.clone()));

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

        // 1. Sync with 0 live sessions -> despawns static preview cast, leaves 0 LiveSessionWorkers, all screens Off.
        sync_live_workers_in_world(&mut world, &[], ChibiLook::Pixar);
        let empty_entities = world.query_filtered::<Entity, With<WorldEntity>>().iter(&world).count();
        assert_eq!(world.query::<&LiveSessionWorker>().iter(&world).count(), 0);
        let off_mat = world.resource::<ScreenMaterials>().by_kind[&ScreenKind::Off].clone();
        assert!(
            world
                .query::<(&DeskScreen, &MeshMaterial3d<StandardMaterial>)>()
                .iter(&world)
                .all(|(_, m)| m.0 == off_mat),
            "all desk screens must turn Off when 0 live sessions are active"
        );

        // 2. Sync with 1 live working session using bash -> spawns 1 LiveSessionWorker and swaps desk 0 screen to Term.
        let s1 = LiveWorkerSpec {
            session_id: "sess-live-1".to_owned(),
            agent: "claude".to_owned(),
            name: "agents-station".to_owned(),
            status: "work".to_owned(),
            desk_hint: 0,
            tool_kind: Some("bash".to_owned()),
            tool_label: Some("cargo test".to_owned()),
            git_dirty: 0,
            subagents: Vec::new(),
        };
        sync_live_workers_in_world(&mut world, std::slice::from_ref(&s1), ChibiLook::Pixar);
        assert_eq!(world.query::<&LiveSessionWorker>().iter(&world).count(), 1);
        let term_mat = world.resource::<ScreenMaterials>().by_kind[&ScreenKind::Term].clone();
        assert!(
            world
                .query::<(&DeskScreen, &MeshMaterial3d<StandardMaterial>)>()
                .iter(&world)
                .filter(|(ds, _)| ds.desk_index == 0)
                .all(|(_, m)| m.0 == term_mat),
            "desk 0 screens must switch to Term material for bash tool"
        );

        // 3. Clear all sessions -> despawns worker + children cleanly back to exact baseline entity count.
        sync_live_workers_in_world(&mut world, &[], ChibiLook::Pixar);
        assert_eq!(world.query::<&LiveSessionWorker>().iter(&world).count(), 0);
        assert_eq!(
            world.query_filtered::<Entity, With<WorldEntity>>().iter(&world).count(),
            empty_entities,
            "clearing live sessions must despawn every worker root & child with 0 leaked entities"
        );

        // 4. Repeat spawn/despawn cycle -> 0 leaked meshes, materials, or images.
        let mats_after_first = world.resource::<Assets<StandardMaterial>>().len();
        let imgs_after_first = world.resource::<Assets<Image>>().len();
        let meshes_after_first = world.resource::<Assets<Mesh>>().len();
        sync_live_workers_in_world(&mut world, &[s1], ChibiLook::Pixar);
        sync_live_workers_in_world(&mut world, &[], ChibiLook::Pixar);
        assert_eq!(world.resource::<Assets<Mesh>>().len(), meshes_after_first);
        assert_eq!(world.resource::<Assets<StandardMaterial>>().len(), mats_after_first);
        assert_eq!(world.resource::<Assets<Image>>().len(), imgs_after_first);
    }

    #[test]
    fn subagent_spawns_at_078_scale_with_dashed_team_lines_and_despawns_cleanly() {
        use crate::backdrops::BackdropKind;
        use crate::layout::OfficeLayout;
        use crate::nav::NavGraph;
        use crate::palette::WorldEntity;
        use crate::{SharedFontEngine, spawn_full_office_world};
        use std::path::PathBuf;

        let assets_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../assets");
        let mut world = World::new();
        world.insert_resource(Assets::<Mesh>::default());
        world.insert_resource(Assets::<StandardMaterial>::default());
        world.insert_resource(Assets::<Image>::default());
        world.insert_resource(SharedFontEngine::new(assets_dir.clone()));

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

        sync_live_workers_in_world(&mut world, &[], ChibiLook::Pixar);
        let base_entities = world
            .query_filtered::<Entity, With<WorldEntity>>()
            .iter(&world)
            .count();

        let with_sub = LiveWorkerSpec {
            session_id: "sess-parent".to_owned(),
            agent: "claude".to_owned(),
            name: "core".to_owned(),
            status: "work".to_owned(),
            desk_hint: 0,
            tool_kind: Some("edit".to_owned()),
            tool_label: Some("nav.rs".to_owned()),
            git_dirty: 1,
            subagents: vec![SubagentSpec {
                id: "sub-alpha".to_owned(),
                label: "Explore".to_owned(),
                doing: "scanning nav".to_owned(),
            }],
        };
        sync_live_workers_in_world(&mut world, std::slice::from_ref(&with_sub), ChibiLook::Pixar);

        // Parent worker must have a wall-safe NavWalker from lift to desk 0
        let nav = world.resource::<NavGraph>().clone();
        let (parent_ent, parent_walker) = world
            .query_filtered::<(Entity, &NavWalker), With<LiveSessionWorker>>()
            .single(&world)
            .expect("parent worker must have NavWalker from lift to desk");
        assert!(
            parent_walker.waypoints.len() >= 2,
            "parent arrival path should traverse NavGraph doorways"
        );
        let mut prev = nav.door_in().pos;
        for &wp in &parent_walker.waypoints {
            assert!(!nav.crosses_wall(prev, wp), "parent walk crossed a wall!");
            prev = wp;
        }

        // Subagent must spawn at 0.78x scale with NavWalker and dashed TeamLineDash links to parent
        let (sub_ent, sub, sub_tf, sub_walker) = world
            .query::<(Entity, &SubagentWorker, &Transform, &NavWalker)>()
            .single(&world)
            .expect("must spawn 1 SubagentWorker with NavWalker");
        assert_eq!(sub.subagent_id, "sub-alpha");
        assert_eq!(sub.parent_session_id, "sess-parent");
        assert_eq!(sub.parent_entity, parent_ent);
        assert!(
            (sub_tf.scale.x - SUBAGENT_SCALE).abs() < 1e-3
                && (sub_tf.scale.y - SUBAGENT_SCALE).abs() < 1e-3
                && (sub_tf.scale.z - SUBAGENT_SCALE).abs() < 1e-3,
            "subagent scale must be 0.78x, got {:?}",
            sub_tf.scale
        );
        assert!(sub_walker.is_walking(), "new subagent must walk from lift to parent briefing spot");
        let mut prev_sub = nav.door_in().pos;
        for &wp in &sub_walker.waypoints {
            assert!(!nav.crosses_wall(prev_sub, wp), "subagent walk crossed a wall!");
            prev_sub = wp;
        }

        let dashes: Vec<TeamLineDash> = world
            .query::<&TeamLineDash>()
            .iter(&world)
            .copied()
            .collect();
        assert_eq!(dashes.len(), TEAM_LINE_DASHES);
        assert!(
            dashes
                .iter()
                .all(|d| d.parent_entity == parent_ent && d.subagent_entity == sub_ent),
            "every TeamLineDash must link parent worker and subagent"
        );

        // Verify step_nav_walkers_system and update_team_lines_system advance positions & orient dashes cleanly
        let mut sched = Schedule::default();
        sched.add_systems((step_nav_walkers_system, update_team_lines_system).chain());
        world.insert_resource(Time::<()>::default());
        world.resource_mut::<Time>().advance_by(std::time::Duration::from_millis(50));
        sched.run(&mut world);
        let dash_tf = world
            .query_filtered::<&Transform, With<TeamLineDash>>()
            .iter(&world)
            .next()
            .expect("dash transform");
        assert!(dash_tf.translation.y >= 0.75 && dash_tf.translation.y <= 1.15);

        // Remove subagent while keeping parent -> subagent + dashes must despawn cleanly
        let mut without_sub = with_sub.clone();
        without_sub.subagents.clear();
        sync_live_workers_in_world(&mut world, &[without_sub], ChibiLook::Pixar);
        assert_eq!(world.query::<&SubagentWorker>().iter(&world).count(), 0);
        assert_eq!(world.query::<&TeamLineDash>().iter(&world).count(), 0);
        assert_eq!(world.query::<&LiveSessionWorker>().iter(&world).count(), 1);

        // Clear all sessions -> back to exact entity baseline and 0 leaked assets on second cycle
        sync_live_workers_in_world(&mut world, &[], ChibiLook::Pixar);
        assert_eq!(
            world
                .query_filtered::<Entity, With<WorldEntity>>()
                .iter(&world)
                .count(),
            base_entities
        );
        let m0 = world.resource::<Assets<Mesh>>().len();
        let mat0 = world.resource::<Assets<StandardMaterial>>().len();
        let img0 = world.resource::<Assets<Image>>().len();
        sync_live_workers_in_world(&mut world, &[with_sub], ChibiLook::Pixar);
        sync_live_workers_in_world(&mut world, &[], ChibiLook::Pixar);
        assert_eq!(world.resource::<Assets<Mesh>>().len(), m0);
        assert_eq!(world.resource::<Assets<StandardMaterial>>().len(), mat0);
        assert_eq!(world.resource::<Assets<Image>>().len(), img0);
    }

    #[test]
    fn live_worker_tool_station_walk_and_farewell_route_via_nav_graph() {
        let nav = NavGraph::for_layout(OfficeLayout::Pro);
        assert_eq!(tool_station_for(Some("read"), None), Some(StationKind::Library));
        assert_eq!(tool_station_for(Some("web"), None), Some(StationKind::Globe));
        assert_eq!(tool_station_for(Some("bash"), None), Some(StationKind::Server));
        assert_eq!(tool_station_for(Some("plan"), None), Some(StationKind::Board));
        assert_eq!(tool_station_for(Some("coffee"), None), Some(StationKind::Coffee));
        assert_eq!(tool_station_for(Some("edit"), Some("main.rs")), None);

        let desk0 = nav.desk_spot(0).unwrap().pos;
        let mut walker = NavWalker::idle_at(0.0, true, WorkerAnim::Work);
        let mut lc = WorkerLifecycle::Seated;
        trigger_worker_farewell(&nav, &mut walker, &mut lc, desk0);
        assert_eq!(lc, WorkerLifecycle::Farewell);
        assert!(walker.despawn_on_arrive);
        assert!(walker.is_walking());
        let mut prev = desk0;
        for &wp in &walker.waypoints {
            assert!(!nav.crosses_wall(prev, wp), "farewell path crossed a wall!");
            prev = wp;
        }
    }

    #[test]
    fn duplicate_subagent_ids_dedupe_and_sleep_z_floaters_enforce_hard_cap() {
        use crate::backdrops::BackdropKind;
        use crate::{SharedFontEngine, spawn_full_office_world};
        use std::path::PathBuf;
        use std::time::Duration;

        let assets_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../assets");
        let mut world = World::new();
        world.insert_resource(Assets::<Mesh>::default());
        world.insert_resource(Assets::<StandardMaterial>::default());
        world.insert_resource(Assets::<Image>::default());
        world.insert_resource(SharedFontEngine::new(assets_dir.clone()));
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
                            OfficeLayout::Hq,
                            BackdropKind::City,
                            ChibiLook::Pixar,
                            false,
                        );
                    });
                });
            });
        });
        world.flush();

        // 1. Duplicate subagent IDs in a single batch must spawn exactly 1 SubagentWorker.
        let dup_spec = LiveWorkerSpec {
            session_id: "sess-dup".to_owned(),
            agent: "claude".to_owned(),
            name: "dup".to_owned(),
            status: "work".to_owned(),
            desk_hint: 0,
            tool_kind: None,
            tool_label: None,
            git_dirty: 0,
            subagents: vec![
                SubagentSpec {
                    id: "sub-same".to_owned(),
                    label: "A".to_owned(),
                    doing: "x".to_owned(),
                },
                SubagentSpec {
                    id: "sub-same".to_owned(),
                    label: "B".to_owned(),
                    doing: "y".to_owned(),
                },
            ],
        };
        sync_live_workers_in_world(&mut world, &[dup_spec], ChibiLook::Pixar);
        assert_eq!(world.query::<&SubagentWorker>().iter(&world).count(), 1);
        assert_eq!(world.query::<&TeamLineDash>().iter(&world).count(), TEAM_LINE_DASHES);

        // 2. Even with 24 sleeping workers, animate_sleep_z_system must cap SleepZFloater at <= 16.
        for i in 0..24 {
            world.spawn((
                WorldEntity,
                ChibiWorker {
                    name: format!("sleeper-{i}"),
                    agent: "claude".to_owned(),
                    anim: WorkerAnim::Sleep,
                    base_pos: Vec3::new(i as f32 * 0.5, 0.0, 0.0),
                    base_yaw: 0.0,
                    phase: 0.0,
                    patrol_target: None,
                    sitting: true,
                },
            ));
        }
        let mut sched = Schedule::default();
        sched.add_systems(animate_sleep_z_system);
        for _ in 0..20 {
            world
                .resource_mut::<Time>()
                .advance_by(Duration::from_millis(100));
            sched.run(&mut world);
            world.flush();
        }
        let z_count = world.query::<&SleepZFloater>().iter(&world).count();
        assert!(z_count > 0 && z_count <= 16, "SleepZFloater count must stay in 1..=16, got {z_count}");
    }
}
