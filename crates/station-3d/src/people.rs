use crate::chibi::{
    ChibiConfig, ChibiDims, ChibiLook, ChibiRig, ChibiWorker, OutfitKind, StatusHalo, WorkerAnim,
    compute_chibi_pose, halo_material_for_anim, spawn_chibi,
};
use crate::chibi_meshes::{ChibiMeshCache, HairStyle};
use crate::chibi_props::{PropKind, set_active_prop};
use crate::desks::{DeskScreen, ScreenMaterials, screen_kind_for_session};
use crate::layout::{LayoutSpec, OfficeLayout};
use crate::palette::WorldPalette;
use bevy::prelude::*;
use station_canvas2d::office_surfaces::ScreenKind;
use std::collections::{HashMap, HashSet};
use std::f32::consts::PI;

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
}

pub fn anim_for_status(status: &str) -> WorkerAnim {
    match status {
        "work" | "working" => WorkerAnim::Work,
        "wait" | "waiting" => WorkerAnim::Wait,
        "rest" | "idle" | "sleep" => WorkerAnim::Sleep,
        _ => WorkerAnim::Rest,
    }
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

/// Spawns the active agent cast seated at desks + office NPCs (director, HR, lounge gamers, karaoke singer).
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

    // Pro / HQ office NPCs: Director in walnut office (Suit), HR walking corridor, Lounge gamer, Karaoke singer
    if spec.layout.is_pro() {
        let mut dir_cfg = ChibiConfig::for_agent("npc", "Giám đốc", 10);
        dir_cfg.shirt = "#2C3E50";
        dir_cfg.outfit = OutfitKind::Suit;
        dir_cfg.hair_style = HairStyle::Plain;
        dir_cfg.accessory = Some("glasses");
        dir_cfg.look = look;
        spawn_chibi(
            commands,
            chibi_cache,
            palette,
            materials,
            &dir_cfg,
            Vec3::new(-5.4, 0.0, -6.55),
            0.0,
            WorkerAnim::Work,
            0.5,
            true,
        );

        let mut hr_cfg = ChibiConfig::for_agent("npc", "HR Lan", 11);
        hr_cfg.shirt = "#D96B90";
        hr_cfg.hair_style = HairStyle::Ponytail;
        hr_cfg.outfit = OutfitKind::Shirt;
        hr_cfg.accessory = None;
        hr_cfg.has_lanyard = true;
        hr_cfg.look = look;
        let hr_ent = spawn_chibi(
            commands,
            chibi_cache,
            palette,
            materials,
            &hr_cfg,
            Vec3::new(-1.5, 0.0, -2.3),
            PI * 0.5,
            WorkerAnim::Walk,
            1.4,
            false,
        );
        commands.entity(hr_ent).insert(PatrolPath {
            a: Vec3::new(-2.8, 0.0, -2.3),
            b: Vec3::new(7.5, 0.0, -2.3),
            speed: 0.75,
        });

        // Lounge gamer on sofa with lazy Gamepad prop
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

        // Karaoke singer inside booth with lazy Mic prop
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

/// Synchronizes dynamic 3D `LiveSessionWorker` entities and `DeskScreen` monitor materials
/// against the current live sessions list. Spawns new workers, updates status/halo/screen
/// in-place, and despawns ended workers cleanly without leaking entities or assets.
#[allow(clippy::too_many_arguments)]
pub fn sync_live_workers_and_screens(
    commands: &mut Commands,
    chibi_cache: &ChibiMeshCache,
    palette: &mut WorldPalette,
    materials: &mut Assets<StandardMaterial>,
    screen_mats: &ScreenMaterials,
    layout: OfficeLayout,
    look: ChibiLook,
    specs: &[LiveWorkerSpec],
    preview_q: &Query<Entity, With<PreviewCastWorker>>,
    workers_q: &mut Query<(Entity, &mut LiveSessionWorker, &mut ChibiWorker, Option<&Children>)>,
    halo_q: &mut Query<&mut MeshMaterial3d<StandardMaterial>, (With<StatusHalo>, Without<DeskScreen>)>,
    screens_q: &mut Query<(&DeskScreen, &mut MeshMaterial3d<StandardMaterial>), Without<StatusHalo>>,
) {
    // 1. Remove any static preview seated workers once live sync takes ownership.
    for ent in preview_q.iter() {
        commands.entity(ent).try_despawn();
    }

    let layout_spec = LayoutSpec::for_layout(layout);
    let desk_count = layout_spec.desks.len().max(1);

    // 2. Assign each live session to a deterministic, non-colliding desk slot.
    let mut assigned_desks: HashSet<usize> = HashSet::new();
    let mut desired: HashMap<&str, (&LiveWorkerSpec, usize)> = HashMap::with_capacity(specs.len());
    let mut desk_screen_kinds: HashMap<usize, ScreenKind> = HashMap::with_capacity(specs.len());

    let mut ordered: Vec<&LiveWorkerSpec> = specs.iter().collect();
    ordered.sort_by(|a, b| a.desk_hint.cmp(&b.desk_hint).then_with(|| a.session_id.cmp(&b.session_id)));

    for s in ordered {
        if desired.contains_key(s.session_id.as_str()) {
            continue;
        }
        let preferred = s.desk_hint % desk_count;
        let desk_idx = if assigned_desks.insert(preferred) {
            preferred
        } else if let Some(free) = (0..desk_count).find(|d| !assigned_desks.contains(d)) {
            assigned_desks.insert(free);
            free
        } else {
            // More sessions than physical desks: skip spawning overlapping chibi, keep first N seated.
            continue;
        };
        desired.insert(s.session_id.as_str(), (s, desk_idx));
        desk_screen_kinds.insert(
            desk_idx,
            screen_kind_for_session(
                &s.status,
                s.tool_kind.as_deref(),
                s.tool_label.as_deref(),
            ),
        );
    }

    // 3. Reconcile existing LiveSessionWorker entities: update in-place or despawn ended ones.
    let mut existing_ids: HashSet<String> = HashSet::new();
    for (ent, mut live, mut chibi, children) in workers_q.iter_mut() {
        let Some(&(spec, target_desk)) = desired.get(live.session_id.as_str()) else {
            commands.entity(ent).try_despawn();
            continue;
        };
        if live.desk_index != target_desk || live.agent != spec.agent {
            commands.entity(ent).try_despawn();
            continue;
        }
        existing_ids.insert(live.session_id.clone());
        let next_anim = anim_for_status(&spec.status);
        if chibi.anim != next_anim {
            chibi.anim = next_anim;
            if let Some(kids) = children {
                let halo_mat = halo_material_for_anim(palette, materials, next_anim);
                for child in kids.iter() {
                    if let Ok(mut m) = halo_q.get_mut(child) {
                        m.0 = halo_mat.clone();
                    }
                }
            }
        }
        live.status = spec.status.clone();
        live.tool_label = spec.tool_label.clone();
        live.name = spec.name.clone();
        live.git_dirty = spec.git_dirty;
        chibi.name = spec.name.clone();
    }

    // 4. Spawn newly arrived live session workers at their assigned DeskSlots.
    for (sid, (spec, desk_idx)) in &desired {
        if existing_ids.contains(*sid) {
            continue;
        }
        let Some(desk) = layout_spec.desks.get(*desk_idx) else {
            continue;
        };
        let seed = stable_seed(&spec.session_id);
        let mut cfg = ChibiConfig::for_agent(&spec.agent, &spec.name, seed);
        cfg.look = look;
        let rot = Quat::from_rotation_y(desk.ry);
        let seat_pos = Vec3::new(desk.x, 0.0, desk.z) + rot * Vec3::new(0.0, 0.0, -0.62);
        let anim = anim_for_status(&spec.status);
        let root = spawn_chibi(
            commands,
            chibi_cache,
            palette,
            materials,
            &cfg,
            seat_pos,
            desk.ry,
            anim,
            (*desk_idx as f32) * 0.75,
            true,
        );
        commands.entity(root).insert(LiveSessionWorker {
            session_id: spec.session_id.clone(),
            agent: spec.agent.clone(),
            name: spec.name.clone(),
            desk_index: *desk_idx,
            status: spec.status.clone(),
            tool_label: spec.tool_label.clone(),
            git_dirty: spec.git_dirty,
        });
    }

    // 5. Swap DeskScreen materials based on each desk's active session tool (or Off if vacant).
    if let Some(off_mat) = screen_mats.by_kind.get(&ScreenKind::Off) {
        for (ds, mut mat) in screens_q.iter_mut() {
            let kind = desk_screen_kinds
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

    let mut state: SystemState<(
        Commands,
        Option<Res<ChibiMeshCache>>,
        Option<ResMut<WorldPalette>>,
        ResMut<Assets<StandardMaterial>>,
        Option<Res<ScreenMaterials>>,
        Option<Res<OfficeLayout>>,
        Query<Entity, With<PreviewCastWorker>>,
        Query<(Entity, &mut LiveSessionWorker, &mut ChibiWorker, Option<&Children>)>,
        Query<&mut MeshMaterial3d<StandardMaterial>, (With<StatusHalo>, Without<DeskScreen>)>,
        Query<(&DeskScreen, &mut MeshMaterial3d<StandardMaterial>), Without<StatusHalo>>,
    )> = SystemState::new(world);

    {
        let (
            mut commands,
            chibi_cache,
            pal,
            mut mats,
            scr_mats,
            layout,
            preview_q,
            mut workers_q,
            mut halo_q,
            mut screens_q,
        ) = state.get_mut(world);
        if let (Some(cache), Some(mut pal), Some(scr_mats)) = (chibi_cache, pal, scr_mats) {
            let lay = layout.map(|l| *l).unwrap_or(OfficeLayout::Pro);
            sync_live_workers_and_screens(
                &mut commands,
                &cache,
                &mut pal,
                &mut mats,
                &scr_mats,
                lay,
                look,
                specs,
                &preview_q,
                &mut workers_q,
                &mut halo_q,
                &mut screens_q,
            );
        }
    }
    state.apply(world);
    world.flush();
}

/// Smoothly animates rigged Chibi characters: 2-bone arm IK for typing/resting/singing,
/// walking leg & arm swing, natural periodic blinking, breathing, and mouth expressions.
pub fn animate_chibis_system(
    time: Res<Time>,
    workers: Query<(
        Entity,
        &ChibiWorker,
        Option<&ChibiRig>,
        Option<&PatrolPath>,
        Option<&crate::chibi_props::ActivePropEntity>,
    )>,
    mut transforms: Query<&mut Transform>,
    mut visibilities: Query<&mut Visibility>,
) {
    let t = time.elapsed_secs();
    for (root, w, rig, patrol, active_prop) in &workers {
        let ph = t + w.phase;
        let blink_cycle = (t + w.phase * 1.7).rem_euclid(3.8);
        let blink = if blink_cycle < 0.14 {
            (blink_cycle / 0.14 * PI).sin()
        } else {
            0.0
        };

        let anim = if patrol.is_some() {
            WorkerAnim::Walk
        } else {
            w.anim
        };
        let look = rig.map(|r| r.look).unwrap_or(ChibiLook::Pixar);
        let pose = compute_chibi_pose(look, anim, w.sitting && patrol.is_none(), ph, blink);

        if let Ok(mut root_tf) = transforms.get_mut(root) {
            if let Some(p) = patrol {
                let u = ((t * p.speed + w.phase).sin() * 0.5 + 0.5).clamp(0.0, 1.0);
                let next = p.a.lerp(p.b, u);
                let dir = if (t * p.speed + w.phase).cos() >= 0.0 {
                    p.b - p.a
                } else {
                    p.a - p.b
                };
                root_tf.translation = next + Vec3::Y * pose.root_y_offset;
                root_tf.rotation = Quat::from_rotation_y(dir.x.atan2(dir.z));
            } else {
                root_tf.translation = w.base_pos + Vec3::Y * pose.root_y_offset;
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
        // Keep hand-held props (mug, cup, noodle, paddle, mic, bags) upright against arm IK rotations.
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
}
