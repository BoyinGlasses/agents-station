pub mod audio;
pub mod backdrops;
pub mod camera;
pub mod cast;
pub mod chibi;
pub mod chibi_meshes;
pub mod chibi_props;
pub mod demo;
pub mod desks;
pub mod entrance;
pub mod furniture;
pub mod hq_wing;
pub mod ibl;
pub mod interact;
pub mod layout;
pub mod life;
pub mod life_systems;
pub mod lights;
pub mod nav;
pub mod palette;
pub mod people;
pub mod pro_floor;
pub mod props;
pub mod props_detail;
pub mod replay;
pub mod rooms;

use bevy::prelude::*;
pub use audio::SfxKind;
pub use backdrops::{BackdropAssets, BackdropEntity, BackdropKind};
pub use camera::{CameraControlSet, CameraPreset, Lite2dCamera, UiInputCapture};
pub use cast::{
    ActiveRoutine, BilliardsStage, CastClock, CastEvents, DutyChore, ErrandKind, FarewellStage,
    InternTenure, KARAOKE_SONGS, KaraokeSong, KaraokeStage, MealsBreaksState, OfficeGamesState,
    OfficeStaffNpc, OutingShopKind, PhotoboothFlashLight, PhotoboothStage, RoutineCoordinator,
    RoutineKind, SongMood, StaffCastState, StaffRole, StreetOutingsState, karaoke_score,
};
pub use chibi::{ChibiLook, ChibiRig, OutfitKind, solve_two_bone_ik};
pub use chibi_meshes::{ChibiMeshCache, HairStyle};
pub use chibi_props::{ActivePropEntity, HeldPropRoot, PropKind, set_active_prop};
pub use demo::{DemoEmittedEvent, DemoSimState, LiveDemoSession, demo_battery};
pub use desks::{DeskBedding, DeskKit, DeskMess, DeskScreen, LaptopLid};
pub use entrance::{ElevatorDoors, Stairwell};
pub use furniture::{DeployRocket, NeonSign, RocketLaunchQueue, ServerRackTelemetry, WallClock};
pub use hq_wing::{DeptSign, HqWing, SprintBoard};
pub use ibl::OfficeIbl;
pub use interact::WorkerSelected;
pub use layout::{LayoutSpec, OfficeLayout};
pub use life_systems::{
    AltarState, DrillEvacuating, DrillPhase, FireDrillState, IncidentState, LiftServicePhase,
    LiftServiceState, MAX_PHOTOSTRIPS, MAX_TEST_ROWS, MAX_TROPHY_SLOTS, NapState, PhotostripSlot,
    PhotostripsState, PostboxState, PostmanStage, QuotaBoardState, RAIN_POOL_CAP, TestRunStatus,
    TestsBoardState, TrophiesShelfState, TrophyEntry, WeatherVisualState, WorkdayKind,
    YourTurnState, sync_photostrips_from_shots_dir,
};
pub use lights::LightingSyncSet;
pub use replay::{ReplayEvent, ReplaySessionSnap, ReplayState, ReplayWorldSnapshot, TimelineSpan};
pub use nav::{NavGraph, NavSpot, StationKind};
pub use palette::{WorldEntity, WorldPalette};
pub use people::{
    ChibiScale, LiveSessionWorker, LiveSyncScratch, LiveWorkerSpec, NavWalker, SUBAGENT_SCALE,
    SleepZFloater, SubagentSpec, SubagentWorker, TeamLineDash, WorkerLifecycle,
};
pub use pro_floor::{
    CoolerScreen, DiningTable, FoosballTable, PhoneBooth, ReceptionCounter, RestroomStall,
};
pub use props_detail::{DeskPropTelemetry, DevServerSpec, PropsLiveTelemetry};
pub use rooms::WindowBlind;
use station_canvas2d::{BrandRegistry, FontEngine};
use std::path::{Path, PathBuf};

/// Lazily loaded, single shared `FontEngine` resource reused across all layout, look,
/// and backdrop switches so the 24 TTF files are never re-read from disk.
#[derive(Resource)]
pub struct SharedFontEngine {
    assets_dir: PathBuf,
    engine: Option<FontEngine>,
    loads: usize,
}

impl SharedFontEngine {
    pub fn new(assets_dir: PathBuf) -> Self {
        Self {
            assets_dir,
            engine: None,
            loads: 0,
        }
    }

    pub fn get_or_init(&mut self) -> &mut FontEngine {
        if self.engine.is_none() {
            self.engine = Some(FontEngine::from_assets_dir(&self.assets_dir));
            self.loads += 1;
        }
        self.engine.as_mut().expect("FontEngine just initialized")
    }

    pub fn load_count(&self) -> usize {
        self.loads
    }
}

/// Registers the shared 3D runtime resources, lazy `SharedFontEngine`, and update systems
/// without spawning 3D geometry on startup (used by `--lite` so switching `2D Lite -> 3D`
/// immediately has live camera controls and animations).
pub struct OfficeWorldRuntimePlugin {
    pub assets_dir: PathBuf,
}

impl Plugin for OfficeWorldRuntimePlugin {
    fn build(&self, app: &mut App) {
        register_office_runtime(
            app,
            OfficeLayout::Pro,
            BackdropKind::City,
            self.assets_dir.clone(),
            false,
        );
    }
}

fn register_office_runtime(
    app: &mut App,
    layout: OfficeLayout,
    backdrop: BackdropKind,
    assets_dir: PathBuf,
    night: bool,
) {
    app.insert_resource(layout)
        .insert_resource(backdrop)
        .insert_resource(lights::LightingState::new(night))
        .insert_resource(life::OfficeLifeState::default())
        .insert_resource(demo::DemoSimState::default())
        .insert_resource(replay::ReplayState::default())
        .insert_resource(audio::SfxEngine::new(true))
        .insert_resource(interact::PlaceCardState::default())
        .insert_resource(furniture::RocketLaunchQueue::default())
        .insert_resource(furniture::ServerRackTelemetry::default())
        .insert_resource(props_detail::PropsLiveTelemetry::default())
        .insert_resource(cast::StaffCastState::default())
        .insert_resource(cast::CastClock::default())
        .insert_resource(cast::CastEvents::default())
        .insert_resource(cast::RoutineCoordinator::default())
        .insert_resource(cast::OfficeGamesState::default())
        .insert_resource(cast::MealsBreaksState::default())
        .insert_resource(cast::StreetOutingsState::default())
        .insert_resource(SharedFontEngine::new(assets_dir))
        .init_resource::<UiInputCapture>()
        .add_event::<interact::PropActivated>()
        .add_event::<interact::WorkerSelected>();
    life_systems::register_life_resources(app);
    interact::register_picking_observers(app);
    app.add_systems(
            Update,
            (
                lights::sync_lighting_system.in_set(lights::LightingSyncSet),
                desks::animate_desks_system,
                furniture::animate_furniture_system,
                props::animate_props_system.before(lights::LightingSyncSet),
                props_detail::cat::animate_cat_system,
                props_detail::leisure::animate_leisure_system,
                props_detail::microwave::animate_microwaves_system,
                props_detail::shredder_cups::animate_shredder_bins_and_cups_system,
                cast::step_staff_roles_system.before(people::step_nav_walkers_system),
                cast::begin_cast_tick_system.before(cast::step_office_games_system),
                cast::step_office_games_system.before(cast::step_meals_breaks_system),
                cast::step_meals_breaks_system.before(cast::step_street_outings_system),
                cast::step_street_outings_system.before(people::step_nav_walkers_system),
                people::step_nav_walkers_system.before(people::animate_chibis_system),
                people::animate_chibis_system,
                people::update_team_lines_system.after(people::step_nav_walkers_system),
                people::animate_sleep_z_system,
                backdrops::animate_backdrop_traffic_system,
            ),
        )
        .add_systems(
            Update,
            (
                life_systems::update_altar_system,
                life_systems::update_drill_and_lift_system.before(people::step_nav_walkers_system),
                entrance::animate_elevator_doors_system.after(life_systems::update_drill_and_lift_system),
                life_systems::update_seasons_and_weather_system,
                life_systems::update_workflow_life_system,
                life_systems::update_nap_system,
            ),
        )
        .add_systems(
            PostUpdate,
            camera::camera_control_system
                .in_set(camera::CameraControlSet)
                .before(bevy::transform::TransformSystem::TransformPropagate),
        );
}

#[allow(clippy::too_many_arguments)]
pub fn spawn_full_office_world(
    commands: &mut Commands,
    meshes: &mut Assets<Mesh>,
    materials: &mut Assets<StandardMaterial>,
    images: &mut Assets<Image>,
    fonts: &mut FontEngine,
    assets_dir: &Path,
    layout: OfficeLayout,
    backdrop: BackdropKind,
    look: ChibiLook,
    night: bool,
) {
    let default_life = life::OfficeLifeState::default();
    spawn_full_office_world_with_life(
        commands,
        meshes,
        materials,
        images,
        fonts,
        assets_dir,
        layout,
        backdrop,
        look,
        night,
        &default_life,
    );
}

#[allow(clippy::too_many_arguments)]
pub fn spawn_full_office_world_with_life(
    commands: &mut Commands,
    meshes: &mut Assets<Mesh>,
    materials: &mut Assets<StandardMaterial>,
    images: &mut Assets<Image>,
    fonts: &mut FontEngine,
    assets_dir: &Path,
    layout: OfficeLayout,
    backdrop: BackdropKind,
    look: ChibiLook,
    night: bool,
    life_state: &life::OfficeLifeState,
) {
    let spec = LayoutSpec::for_layout(layout);
    let mut pal = WorldPalette::new(meshes);
    let chibi_cache = ChibiMeshCache::build(meshes, materials, images);
    let mut brands = BrandRegistry::new(assets_dir.to_path_buf());
    let mut backdrop_assets = BackdropAssets::default();

    let ibl = ibl::OfficeIbl::generate(images);
    lights::spawn_office_lights(commands, &spec, night);
    camera::spawn_office_camera_with_ibl(commands, &spec, &ibl, night);
    rooms::spawn_office_shell(
        commands,
        &mut pal,
        materials,
        images,
        fonts,
        &spec,
        backdrop,
        night,
    );
    entrance::spawn_entrance(commands, &mut pal, materials, images, fonts, &spec);
    let scr_mats = desks::spawn_desks(commands, &mut pal, materials, images, fonts, &spec);
    furniture::spawn_furniture(commands, &mut pal, materials, images, fonts, &spec);
    props::spawn_all_props(commands, &mut pal, materials, images, fonts, &spec);
    people::spawn_office_cast(commands, &chibi_cache, &mut pal, materials, &spec, look);
    cast::spawn_office_staff_cast(commands, &chibi_cache, &mut pal, materials, &spec, look);
    life::spawn_life_elements(
        commands,
        &mut pal,
        materials,
        images,
        fonts,
        &spec,
        life_state,
    );
    backdrops::spawn_backdrop(
        commands,
        &mut pal,
        materials,
        images,
        &mut backdrop_assets,
        &mut brands,
        fonts,
        &spec,
        backdrop,
    );
    brands.clear();
    interact::spawn_hover_ring(commands, &mut pal, materials);

    commands.insert_resource(layout);
    commands.insert_resource(NavGraph::for_layout(layout));
    commands.insert_resource(backdrop);
    commands.insert_resource(lights::LightingState::new(night));
    commands.insert_resource(ibl);
    commands.insert_resource(scr_mats);
    commands.insert_resource(backdrop_assets);
    commands.insert_resource(chibi_cache);
    commands.insert_resource(pal);
}

/// True Unload helper: despawns every `WorldEntity` and purges every `WorldPalette`
/// mesh/material/image, `ChibiMeshCache` mesh/material/image, `BackdropAssets` material/image,
/// and `OfficeIbl` cubemap so switching `3D -> 2D Lite`, rebuilding layouts, or minimizing to
/// System Tray leaks zero GPU/CPU assets.
#[allow(clippy::too_many_arguments)]
pub fn unload_full_office_world(
    commands: &mut Commands,
    world_entities: impl IntoIterator<Item = Entity>,
    pal: Option<&mut WorldPalette>,
    backdrop_assets: Option<&mut BackdropAssets>,
    ibl: Option<&OfficeIbl>,
    chibi_meshes: Option<&ChibiMeshCache>,
    meshes: &mut Assets<Mesh>,
    materials: &mut Assets<StandardMaterial>,
    images: &mut Assets<Image>,
) {
    for ent in world_entities {
        commands.entity(ent).try_despawn();
    }
    if let Some(p) = pal {
        p.purge_all(meshes, materials, images);
    }
    if let Some(ba) = backdrop_assets {
        ba.purge(materials, images);
    }
    if let Some(env_ibl) = ibl {
        env_ibl.purge(images);
    }
    if let Some(cm) = chibi_meshes {
        cm.purge(meshes, materials, images);
    }
    commands.remove_resource::<WorldPalette>();
    commands.remove_resource::<BackdropAssets>();
    commands.remove_resource::<OfficeIbl>();
    commands.remove_resource::<ChibiMeshCache>();
    commands.remove_resource::<NavGraph>();
    commands.remove_resource::<desks::ScreenMaterials>();
    commands.remove_resource::<camera::CameraRig>();
    commands.insert_resource(cast::RoutineCoordinator::default());
    commands.insert_resource(cast::OfficeGamesState::default());
    commands.insert_resource(cast::MealsBreaksState::default());
    commands.insert_resource(cast::StreetOutingsState::default());
    life_systems::reset_life_resources(commands);
}

pub struct OfficeWorldPlugin {
    pub layout: OfficeLayout,
    pub backdrop: BackdropKind,
    pub assets_dir: PathBuf,
    pub night: bool,
}

impl Plugin for OfficeWorldPlugin {
    fn build(&self, app: &mut App) {
        let layout = self.layout;
        let backdrop = self.backdrop;
        let assets_dir = self.assets_dir.clone();
        let night = self.night;

        register_office_runtime(app, layout, backdrop, assets_dir.clone(), night);
        app.add_systems(
            Startup,
            move |mut commands: Commands,
                  mut meshes: ResMut<Assets<Mesh>>,
                  mut materials: ResMut<Assets<StandardMaterial>>,
                  mut images: ResMut<Assets<Image>>,
                  mut shared_fonts: ResMut<SharedFontEngine>,
                  life: Option<Res<life::OfficeLifeState>>| {
                let fonts = shared_fonts.get_or_init();
                let fallback_life = life::OfficeLifeState::default();
                let active_life = life.as_deref().unwrap_or(&fallback_life);
                spawn_full_office_world_with_life(
                    &mut commands,
                    &mut meshes,
                    &mut materials,
                    &mut images,
                    fonts,
                    &assets_dir,
                    layout,
                    backdrop,
                    ChibiLook::Pixar,
                    night,
                    active_life,
                );
            },
        );
    }
}

/// Legacy system wrapper kept for callers that only have `WorldPalette` and `Assets<Image>`.
pub fn unload_office_world(
    mut commands: Commands,
    q: Query<Entity, With<WorldEntity>>,
    mut pal: Option<ResMut<WorldPalette>>,
    mut images: ResMut<Assets<Image>>,
) {
    for ent in &q {
        commands.entity(ent).try_despawn();
    }
    if let Some(p) = pal.as_mut() {
        p.purge_transient_images(&mut images);
    }
}

pub struct SmokeScenePlugin;

impl Plugin for SmokeScenePlugin {
    fn build(&self, app: &mut App) {
        app.insert_resource(ClearColor(Color::srgb(0.08, 0.09, 0.12)))
            .insert_resource(AmbientLight {
                color: Color::srgb(0.95, 0.92, 0.88),
                brightness: 320.0,
                ..default()
            })
            .add_systems(Startup, setup_smoke_scene);
    }
}

/// Studio close-up showcase scene for `--capture-scene chibi-closeup`, displaying all 6 sculpted
/// hairstyles, Pixar & Toy proportions, layered eyes, Hoodie/Suit/Shirt outfits, lazy held props,
/// and a foreground seated desk worker in 2-bone arm IK typing pose.
pub struct ChibiCloseupPlugin;

impl Plugin for ChibiCloseupPlugin {
    fn build(&self, app: &mut App) {
        app.insert_resource(ClearColor(Color::srgb(0.93, 0.90, 0.84)))
            .insert_resource(AmbientLight {
                color: Color::srgb(1.0, 0.96, 0.90),
                brightness: 520.0,
                ..default()
            })
            .add_systems(Startup, setup_chibi_closeup_scene)
            .add_systems(Update, people::animate_chibis_system);
    }
}

fn setup_chibi_closeup_scene(
    mut commands: Commands,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
    mut images: ResMut<Assets<Image>>,
) {
    use bevy::core_pipeline::bloom::Bloom;
    use bevy::core_pipeline::tonemapping::Tonemapping;
    use std::f32::consts::FRAC_PI_4;

    let mut pal = WorldPalette::new(&mut meshes);
    let cache = ChibiMeshCache::build(&mut meshes, &mut materials, &mut images);
    let ibl = ibl::OfficeIbl::generate(&mut images);

    // Warm oak plank floor + light cream studio backdrop wall
    let floor_mat = pal.tiled_mat(
        &mut images,
        &mut materials,
        station_canvas2d::office_surfaces::draw_plank_tile(),
        Vec2::new(6.0, 4.0),
        "#EFE6D8",
        0.72,
    );
    palette::spawn_floor_quad(&mut commands, &pal, floor_mat, Vec3::ZERO, Vec2::new(14.0, 10.0));

    let wall_mat = pal.matte(&mut materials, "#F4EFE6");
    palette::spawn_box(
        &mut commands,
        &pal,
        wall_mat,
        Vec3::new(0.0, 2.5, -2.6),
        Vec3::new(14.0, 5.0, 0.2),
    );

    // 3-point warm studio lighting + IBL
    commands.spawn((
        WorldEntity,
        DirectionalLight {
            color: Color::srgb(1.0, 0.96, 0.90),
            illuminance: 16_000.0,
            shadows_enabled: true,
            ..default()
        },
        Transform::from_xyz(3.5, 5.5, 4.5).looking_at(Vec3::new(0.0, 0.9, 0.0), Vec3::Y),
    ));
    commands.spawn((
        WorldEntity,
        PointLight {
            color: Color::srgb(0.95, 0.97, 1.0),
            intensity: 45_000.0,
            range: 14.0,
            shadows_enabled: false,
            ..default()
        },
        Transform::from_xyz(-3.8, 2.6, 3.2),
    ));
    commands.spawn((
        WorldEntity,
        PointLight {
            color: Color::srgb(1.0, 0.93, 0.82),
            intensity: 35_000.0,
            range: 12.0,
            shadows_enabled: false,
            ..default()
        },
        Transform::from_xyz(0.5, 2.4, -2.0),
    ));

    // Back showcase arc: all 6 hairstyles, all 3 outfits, Pixar + Toy looks, and lazy held props
    // Arranged cleanly so ALL 6 standing chibis + foreground seated typing IK chibi are 100% unobstructed.
    let lineup: [(&str, ChibiLook, HairStyle, OutfitKind, Option<&str>, &str, &str, chibi::WorkerAnim, Option<PropKind>, Vec3, f32); 6] = [
        ("Bob", ChibiLook::Pixar, HairStyle::Bob, OutfitKind::Hoodie, None, "#D97757", "#3B2618", chibi::WorkerAnim::Rest, Some(PropKind::Mug), Vec3::new(-2.70, 0.0, -0.55), 0.28),
        ("Director", ChibiLook::Pixar, HairStyle::Plain, OutfitKind::Suit, Some("glasses"), "#2C3E50", "#1E1E24", chibi::WorkerAnim::Rest, Some(PropKind::Folder), Vec3::new(-1.82, 0.0, -0.88), 0.18),
        ("Spiky", ChibiLook::Pixar, HairStyle::Spiky, OutfitKind::Shirt, Some("headphones"), "#3E9B74", "#232836", chibi::WorkerAnim::Rest, Some(PropKind::Noodle), Vec3::new(-0.96, 0.0, -1.08), 0.10),
        ("Ponytail", ChibiLook::Pixar, HairStyle::Ponytail, OutfitKind::Hoodie, None, "#5F8FD8", "#5A3A2A", chibi::WorkerAnim::Rest, Some(PropKind::Paddle), Vec3::new(0.96, 0.0, -1.08), -0.68),
        ("Bun", ChibiLook::Pixar, HairStyle::Bun, OutfitKind::Shirt, None, "#E06D94", "#2C2228", chibi::WorkerAnim::Sing, Some(PropKind::Mic), Vec3::new(1.82, 0.0, -0.88), -0.18),
        ("ToyCurly", ChibiLook::Toy, HairStyle::Curly, OutfitKind::Hoodie, None, "#E09A3E", "#6B4A33", chibi::WorkerAnim::Rest, Some(PropKind::Boba), Vec3::new(2.70, 0.0, -0.55), -0.28),
    ];

    for (i, (name, look, hair_style, outfit, acc, shirt, hair, anim, prop, pos, yaw)) in
        lineup.into_iter().enumerate()
    {
        let mut cfg = chibi::ChibiConfig::for_agent("claude", name, i + 1);
        cfg.look = look;
        cfg.hair_style = hair_style;
        cfg.outfit = outfit;
        cfg.accessory = acc;
        cfg.shirt = shirt;
        cfg.hair = hair;
        cfg.has_lanyard = name == "Bun";
        let ent = chibi::spawn_chibi(
            &mut commands,
            &cache,
            &mut pal,
            &mut materials,
            &cfg,
            pos,
            yaw,
            anim,
            i as f32 * 0.7,
            false,
        );
        if prop.is_some() {
            set_active_prop(&mut commands, &cache, &mut pal, &mut materials, ent, prop);
        }
    }

    // Foreground desk + laptop + chair + seated Pixar chibi demonstrating 2-bone arm IK typing pose
    let desk_pos = Vec3::new(0.18, 0.0, 1.15);
    let desk_yaw = FRAC_PI_4 * 0.95;
    let desk_rot = Quat::from_rotation_y(desk_yaw);
    let at = |lx: f32, ly: f32, lz: f32| desk_pos + desk_rot * Vec3::new(lx, ly, lz);

    let top_m = pal.mat(&mut materials, "#D5B184", 0.65, 0.0);
    let leg_m = pal.mat(&mut materials, "#2E333C", 0.45, 0.35);
    let laptop_m = pal.mat(&mut materials, "#C9CED6", 0.35, 0.50);
    let screen_m = pal.emissive(&mut materials, "#4EA8DE", 0.85);
    let chair_m = pal.mat(&mut materials, "#596170", 0.85, 0.0);

    palette::spawn_box_rot(
        &mut commands,
        &pal,
        top_m,
        at(0.0, 0.76, 0.0),
        Vec3::new(1.15, 0.04, 0.62),
        desk_rot,
    );
    for (lx, lz) in [(-0.52, -0.25), (0.52, -0.25), (-0.52, 0.25), (0.52, 0.25)] {
        palette::spawn_box_rot(
            &mut commands,
            &pal,
            leg_m.clone(),
            at(lx, 0.37, lz),
            Vec3::new(0.045, 0.74, 0.045),
            desk_rot,
        );
    }
    // Laptop base + tilted lid on desk
    palette::spawn_box_rot(
        &mut commands,
        &pal,
        laptop_m.clone(),
        at(0.0, 0.795, -0.04),
        Vec3::new(0.46, 0.024, 0.32),
        desk_rot,
    );
    let lid_rot = desk_rot * Quat::from_rotation_x(0.28);
    palette::spawn_box_rot(
        &mut commands,
        &pal,
        laptop_m,
        at(0.0, 0.93, 0.14),
        Vec3::new(0.46, 0.30, 0.018),
        lid_rot,
    );
    palette::spawn_box_rot(
        &mut commands,
        &pal,
        screen_m,
        at(0.0, 0.93, 0.129),
        Vec3::new(0.42, 0.26, 0.004),
        lid_rot,
    );
    // Chair cushion & backrest
    let seat_pos = at(0.0, 0.0, -0.62);
    desks::spawn_office_chair(
        &mut commands,
        &mut pal,
        &mut materials,
        seat_pos,
        desk_yaw,
        chair_m,
        None,
    );

    let mut typer_cfg = chibi::ChibiConfig::for_agent("claude", "Claude", 0);
    typer_cfg.look = ChibiLook::Pixar;
    typer_cfg.hair_style = HairStyle::Plain;
    typer_cfg.outfit = OutfitKind::Hoodie;
    typer_cfg.accessory = Some("glasses");
    typer_cfg.shirt = "#D97757";
    typer_cfg.hair = "#3B2618";
    chibi::spawn_chibi(
        &mut commands,
        &cache,
        &mut pal,
        &mut materials,
        &typer_cfg,
        seat_pos,
        desk_yaw,
        chibi::WorkerAnim::Work,
        0.4,
        true,
    );

    // Tight studio camera framing all 6 standing chibis + foreground seated typing IK chibi
    commands.spawn((
        WorldEntity,
        Camera3d::default(),
        Projection::Perspective(PerspectiveProjection {
            fov: 35.0_f32.to_radians(),
            ..default()
        }),
        Msaa::Sample4,
        Tonemapping::AcesFitted,
        Bloom::NATURAL,
        EnvironmentMapLight {
            diffuse_map: ibl.day_diffuse.clone(),
            specular_map: ibl.day_specular.clone(),
            intensity: 750.0,
            ..default()
        },
        Transform::from_xyz(0.0, 1.88, 5.55).looking_at(Vec3::new(0.0, 0.92, -0.25), Vec3::Y),
    ));

    commands.insert_resource(ibl);
    commands.insert_resource(cache);
    commands.insert_resource(pal);
}

fn setup_smoke_scene(
    mut commands: Commands,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
) {
    commands.spawn((
        Mesh3d(meshes.add(Plane3d::default().mesh().size(10.0, 10.0))),
        MeshMaterial3d(materials.add(StandardMaterial {
            base_color: Color::srgb(0.24, 0.25, 0.29),
            perceptual_roughness: 0.85,
            ..default()
        })),
        Transform::from_xyz(0.0, 0.0, 0.0),
    ));

    commands.spawn((
        Mesh3d(meshes.add(Cuboid::new(1.5, 1.5, 1.5))),
        MeshMaterial3d(materials.add(StandardMaterial {
            base_color: Color::srgb(0.84, 0.39, 0.25),
            perceptual_roughness: 0.45,
            metallic: 0.1,
            ..default()
        })),
        Transform::from_xyz(0.0, 0.75, 0.0).with_rotation(Quat::from_rotation_y(0.45)),
    ));

    commands.spawn((
        DirectionalLight {
            illuminance: 14_000.0,
            shadows_enabled: true,
            ..default()
        },
        Transform::from_xyz(5.0, 8.0, 5.0).looking_at(Vec3::ZERO, Vec3::Y),
    ));

    commands.spawn((
        Camera3d::default(),
        Msaa::Off,
        Transform::from_xyz(4.0, 3.0, 6.0).looking_at(Vec3::new(0.0, 0.6, 0.0), Vec3::Y),
    ));
}

pub fn world3d_ready() -> bool {
    true
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn layouts_match_oracle_desk_counts_and_bounds() {
        let cozy = LayoutSpec::for_layout(OfficeLayout::Cozy);
        assert_eq!(cozy.desks.len(), 9);
        assert!(cozy.inside(0.0, 0.0, 0.4));
        assert!(!cozy.inside(10.0, 0.0, 0.4));
        assert!(cozy.game_tv.is_none());

        let pro = LayoutSpec::for_layout(OfficeLayout::Pro);
        assert_eq!(pro.desks.len(), 12);
        assert_eq!(pro.pods.len(), 3);
        assert!(pro.game_tv.is_some());
        assert!(pro.ktv.is_some());
        assert!(pro.bil.is_some());

        let hq = LayoutSpec::for_layout(OfficeLayout::Hq);
        assert_eq!(hq.desks.len(), 44);
        assert_eq!(hq.depts.len(), 4);
        assert_eq!(hq.pods.len(), 11);
        assert!(hq.inside(3.0, 20.0, 0.4));
    }

    #[test]
    fn world_palette_deduplicates_materials_for_low_memory() {
        let mut meshes = Assets::<Mesh>::default();
        let mut mats = Assets::<StandardMaterial>::default();
        let mut pal = WorldPalette::new(&mut meshes);

        let a = pal.matte(&mut mats, "#F4F0E8");
        let b = pal.matte(&mut mats, "#F4F0E8");
        let c = pal.matte(&mut mats, "#2C323C");
        assert_eq!(a, b);
        assert_ne!(a, c);
        assert_eq!(pal.material_count(), 2);
    }

    #[test]
    fn full_office_world_unload_and_rebuild_purges_all_assets_and_preserves_single_camera() {
        let assets_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../assets");
        let mut world = World::new();
        world.insert_resource(Assets::<Mesh>::default());
        world.insert_resource(Assets::<StandardMaterial>::default());
        world.insert_resource(Assets::<Image>::default());
        world.insert_resource(SharedFontEngine::new(assets_dir.clone()));

        // 1. Spawn full Pro 3D office world.
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

        assert!(world.query_filtered::<Entity, With<WorldEntity>>().iter(&world).count() > 50);
        assert_eq!(world.query_filtered::<Entity, With<Camera3d>>().iter(&world).count(), 1);
        assert_eq!(world.query_filtered::<Entity, With<camera::MainOfficeCamera>>().iter(&world).count(), 1);
        assert_eq!(world.query_filtered::<Entity, With<Lite2dCamera>>().iter(&world).count(), 0);
        assert_eq!(desks::count_desks_in_world(&mut world), 12);
        assert!(world.resource::<Assets<Mesh>>().len() >= 6);
        assert!(world.resource::<Assets<StandardMaterial>>().len() > 20);
        assert!(world.resource::<Assets<Image>>().len() > 10);

        assert!(world.get_resource::<OfficeIbl>().is_some());
        assert_eq!(
            *world
                .query_filtered::<&Msaa, With<camera::MainOfficeCamera>>()
                .single(&world)
                .unwrap(),
            Msaa::Sample4
        );

        // 2. Unload full 3D world and spawn Lite2dCamera (never tagged WorldEntity).
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
                    camera::spawn_lite_2d_camera(&mut cmds);
                });
            });
        });
        world.flush();

        assert_eq!(world.query_filtered::<Entity, With<WorldEntity>>().iter(&world).count(), 0);
        assert!(world.get_resource::<OfficeIbl>().is_none());
        assert_eq!(world.resource::<Assets<Mesh>>().len(), 0);
        assert_eq!(world.resource::<Assets<StandardMaterial>>().len(), 0);
        assert_eq!(world.resource::<Assets<Image>>().len(), 0);
        assert_eq!(world.query_filtered::<Entity, With<Camera3d>>().iter(&world).count(), 1);
        assert_eq!(world.query_filtered::<Entity, With<Lite2dCamera>>().iter(&world).count(), 1);
        assert_eq!(
            *world
                .query_filtered::<&Msaa, With<Lite2dCamera>>()
                .single(&world)
                .unwrap(),
            Msaa::Off
        );
        assert_eq!(world.query_filtered::<Entity, (With<Lite2dCamera>, With<WorldEntity>)>().iter(&world).count(), 0);

        // 3. Despawn Lite2dCamera and rebuild HQ 3D world with 44 desks.
        let lite_cams: Vec<Entity> = world
            .query_filtered::<Entity, With<Lite2dCamera>>()
            .iter(&world)
            .collect();
        for e in lite_cams {
            world.despawn(e);
        }
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
                            BackdropKind::Beach,
                            ChibiLook::Toy,
                            true,
                        );
                    });
                });
            });
        });
        world.flush();

        assert_eq!(world.query_filtered::<Entity, With<Camera3d>>().iter(&world).count(), 1);
        assert_eq!(world.query_filtered::<Entity, With<camera::MainOfficeCamera>>().iter(&world).count(), 1);
        assert_eq!(world.query_filtered::<Entity, With<Lite2dCamera>>().iter(&world).count(), 0);
        assert_eq!(desks::count_desks_in_world(&mut world), 44);
        assert_eq!(world.resource::<SharedFontEngine>().load_count(), 1);
    }

    #[test]
    fn tiled_mat_uses_repeat_sampler_render_world_and_stays_under_1mb_vram() {
        use bevy::asset::RenderAssetUsages;
        use bevy::image::{ImageAddressMode, ImageSampler};
        use station_canvas2d::office_surfaces::{
            draw_checker_tile, draw_plank_tile, draw_server_grid_tile, draw_speckle_rug_tile,
            draw_subway_tile, draw_wainscot_slats_tile,
        };

        let mut meshes = Assets::<Mesh>::default();
        let mut mats = Assets::<StandardMaterial>::default();
        let mut imgs = Assets::<Image>::default();
        let mut pal = WorldPalette::new(&mut meshes);

        let surfaces = [
            draw_plank_tile(),
            draw_checker_tile(),
            draw_subway_tile(),
            draw_server_grid_tile(),
            draw_speckle_rug_tile(),
            draw_wainscot_slats_tile(),
        ];

        let mut total_bytes = 0usize;
        for surf in surfaces {
            total_bytes += surf.rgba.len();
            let mat_h = pal.tiled_mat(
                &mut imgs,
                &mut mats,
                surf,
                Vec2::new(6.0, 4.0),
                "#FFFFFF",
                0.65,
            );
            let mat = mats.get(&mat_h).expect("tiled material registered");
            let tex_h = mat
                .base_color_texture
                .as_ref()
                .expect("tiled material has texture");
            let img = imgs.get(tex_h).expect("tiled image registered");
            assert_eq!(
                img.asset_usage,
                RenderAssetUsages::RENDER_WORLD,
                "tiled floor/wall textures must drop CPU buffers via RENDER_WORLD"
            );
            match &img.sampler {
                ImageSampler::Descriptor(desc) => {
                    assert!(matches!(desc.address_mode_u, ImageAddressMode::Repeat));
                    assert!(matches!(desc.address_mode_v, ImageAddressMode::Repeat));
                }
                other => panic!("expected ImageSampler::Descriptor with Repeat, got {other:?}"),
            }
        }
        assert!(
            total_bytes < 1_048_576,
            "all repeating room floor/wall/rug tiles combined must be < 1 MB VRAM, got {total_bytes} bytes"
        );

        pal.purge_all(&mut meshes, &mut mats, &mut imgs);
        assert_eq!(imgs.len(), 0);
        assert_eq!(mats.len(), 0);
    }

    #[test]
    fn pro_and_hq_architecture_spawns_all_landmarks_and_unloads_cleanly() {
        let assets_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../assets");
        let mut world = World::new();
        world.insert_resource(Assets::<Mesh>::default());
        world.insert_resource(Assets::<StandardMaterial>::default());
        world.insert_resource(Assets::<Image>::default());
        world.insert_resource(SharedFontEngine::new(assets_dir.clone()));

        // Spawn Pro layout
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

        assert_eq!(
            world
                .query_filtered::<Entity, With<pro_floor::ReceptionCounter>>()
                .iter(&world)
                .count(),
            1
        );
        let stall_mats: Vec<Handle<StandardMaterial>> = world
            .query::<&pro_floor::RestroomStall>()
            .iter(&world)
            .map(|s| s.indicator_mat.clone())
            .collect();
        assert_eq!(stall_mats.len(), 2);
        assert_ne!(
            stall_mats[0], stall_mats[1],
            "each RestroomStall must own a distinct indicator_mat so toggling one stall never mutates the other"
        );
        assert_eq!(
            world
                .query_filtered::<Entity, With<pro_floor::CoolerScreen>>()
                .iter(&world)
                .count(),
            1
        );
        assert_eq!(
            world
                .query_filtered::<Entity, With<pro_floor::DiningTable>>()
                .iter(&world)
                .count(),
            4
        );
        assert_eq!(
            world
                .query_filtered::<Entity, With<pro_floor::PhoneBooth>>()
                .iter(&world)
                .count(),
            2
        );
        assert_eq!(
            world
                .query_filtered::<Entity, With<pro_floor::FoosballTable>>()
                .iter(&world)
                .count(),
            1
        );
        assert_eq!(
            world
                .query_filtered::<Entity, With<entrance::ElevatorDoors>>()
                .iter(&world)
                .count(),
            1
        );
        assert_eq!(
            world
                .query_filtered::<Entity, With<entrance::Stairwell>>()
                .iter(&world)
                .count(),
            1
        );
        assert!(
            world
                .query_filtered::<Entity, With<rooms::WindowBlind>>()
                .iter(&world)
                .count()
                >= 5
        );
        assert!(
            world.resource::<WorldPalette>().floor_tile_vram_bytes() < 1_048_576,
            "Pro floor tile VRAM must stay < 1 MB"
        );

        // Unload Pro and verify 0 leaked assets
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
        assert_eq!(world.resource::<Assets<Mesh>>().len(), 0);
        assert_eq!(world.resource::<Assets<StandardMaterial>>().len(), 0);
        assert_eq!(world.resource::<Assets<Image>>().len(), 0);

        // Rebuild HQ and verify HQ wing landmarks + 4 dept signs + 4 sprint boards
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

        assert_eq!(
            world
                .query_filtered::<Entity, With<hq_wing::HqWing>>()
                .iter(&world)
                .count(),
            1
        );
        assert_eq!(
            world
                .query_filtered::<Entity, With<hq_wing::DeptSign>>()
                .iter(&world)
                .count(),
            4
        );
        assert_eq!(
            world
                .query_filtered::<Entity, With<hq_wing::SprintBoard>>()
                .iter(&world)
                .count(),
            4
        );

        // Unload HQ and verify 0 leaked meshes/materials/images
        let hq_ents: Vec<Entity> = world
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
                        hq_ents,
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
        assert_eq!(world.resource::<Assets<Mesh>>().len(), 0);
        assert_eq!(world.resource::<Assets<StandardMaterial>>().len(), 0);
        assert_eq!(world.resource::<Assets<Image>>().len(), 0);
    }

    #[test]
    fn full_world_with_all_life_systems_unloads_zero_leaks_across_cozy_pro_and_hq() {
        let assets_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../assets");
        let mut world = World::new();
        world.insert_resource(Assets::<Mesh>::default());
        world.insert_resource(Assets::<StandardMaterial>::default());
        world.insert_resource(Assets::<Image>::default());
        world.insert_resource(SharedFontEngine::new(assets_dir.clone()));

        for layout in [OfficeLayout::Cozy, OfficeLayout::Pro, OfficeLayout::Hq] {
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
                                layout,
                                BackdropKind::City,
                                ChibiLook::Pixar,
                                false,
                            );
                        });
                    });
                });
            });
            world.flush();

            // Verify rain pool, postbox, tests board, and photostrips spawned safely for every layout
            assert_eq!(
                world.query::<&life::RainDrop>().iter(&world).count(),
                life_systems::RAIN_POOL_CAP,
                "layout {layout:?} must have capped raindrop pool"
            );
            assert!(
                world.query::<&life_systems::workflow_life::TestLampLed>().iter(&world).count()
                    == life_systems::MAX_TEST_ROWS
            );
            assert!(
                world.query::<&life_systems::PhotostripSlot>().iter(&world).count()
                    == life_systems::MAX_PHOTOSTRIPS
            );

            // Unload and verify 0 WorldEntity, 0 meshes, 0 materials, 0 images
            let ents: Vec<Entity> = world
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
                            ents,
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
                world.query_filtered::<Entity, With<WorldEntity>>().iter(&world).count(),
                0,
                "layout {layout:?} leaked WorldEntity"
            );
            assert_eq!(world.resource::<Assets<Mesh>>().len(), 0, "layout {layout:?} leaked Mesh");
            assert_eq!(
                world.resource::<Assets<StandardMaterial>>().len(),
                0,
                "layout {layout:?} leaked StandardMaterial"
            );
            assert_eq!(world.resource::<Assets<Image>>().len(), 0, "layout {layout:?} leaked Image");
        }
    }

    #[test]
    fn office_world_plugin_uses_active_office_life_state_for_season_and_rain() {
        let assets_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../assets");
        let mut app = App::new();
        app.insert_resource(Time::<()>::default())
            .insert_resource(Assets::<Mesh>::default())
            .insert_resource(Assets::<StandardMaterial>::default())
            .insert_resource(Assets::<Image>::default())
            .add_plugins(OfficeWorldPlugin {
                layout: OfficeLayout::Pro,
                backdrop: BackdropKind::City,
                assets_dir,
                night: false,
            });
        // Override OfficeLifeState before Startup runs (simulating holiday season + live rain)
        {
            let mut life = app.world_mut().resource_mut::<life::OfficeLifeState>();
            life.season = life::SeasonTheme::Christmas;
            life.apply_open_meteo(95, 22.0);
        }
        app.world_mut().run_schedule(Startup);
        app.world_mut().flush();

        let decor_count = app
            .world_mut()
            .query::<&life_systems::seasons_weather::SeasonDecorPart>()
            .iter(app.world())
            .count();
        assert!(
            decor_count > 0,
            "Startup spawn must honor active OfficeLifeState.season (Christmas)"
        );
        let visible_drops = app
            .world_mut()
            .query_filtered::<&Visibility, With<life::RainDrop>>()
            .iter(app.world())
            .filter(|v| **v == Visibility::Visible)
            .count();
        assert_eq!(
            visible_drops,
            life_systems::RAIN_POOL_CAP,
            "Startup spawn must honor active OfficeLifeState.rain_intensity"
        );
    }
}
