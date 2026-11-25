pub mod audio;
pub mod backdrops;
pub mod camera;
pub mod chibi;
pub mod demo;
pub mod desks;
pub mod entrance;
pub mod furniture;
pub mod interact;
pub mod layout;
pub mod life;
pub mod lights;
pub mod palette;
pub mod people;
pub mod props;
pub mod replay;
pub mod rooms;

use bevy::prelude::*;
pub use audio::SfxKind;
pub use backdrops::{BackdropAssets, BackdropEntity, BackdropKind};
pub use camera::{CameraControlSet, Lite2dCamera, UiInputCapture};
pub use chibi::ChibiLook;
pub use desks::DeskScreen;
pub use interact::WorkerSelected;
pub use layout::{LayoutSpec, OfficeLayout};
pub use palette::{WorldEntity, WorldPalette};
pub use people::{LiveSessionWorker, LiveWorkerSpec};
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
        .insert_resource(lights::LightingState { night })
        .insert_resource(life::OfficeLifeState::default())
        .insert_resource(replay::ReplayState::default())
        .insert_resource(audio::SfxEngine::new(true))
        .insert_resource(interact::PlaceCardState::default())
        .insert_resource(SharedFontEngine::new(assets_dir))
        .init_resource::<UiInputCapture>()
        .add_event::<interact::PropActivated>()
        .add_event::<interact::WorkerSelected>();
    interact::register_picking_observers(app);
    app.add_systems(
            Update,
            (
                props::animate_props_system,
                people::animate_chibis_system,
                backdrops::animate_backdrop_traffic_system,
                life::update_rain_system,
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
    let spec = LayoutSpec::for_layout(layout);
    let mut pal = WorldPalette::new(meshes);
    let mut brands = BrandRegistry::new(assets_dir.to_path_buf());
    let mut backdrop_assets = BackdropAssets::default();
    let life_state = life::OfficeLifeState::default();

    lights::spawn_office_lights(commands, &spec, night);
    camera::spawn_office_camera(commands, &spec);
    rooms::spawn_office_shell(commands, &mut pal, materials, images, fonts, &spec);
    entrance::spawn_entrance(commands, &mut pal, materials, &spec);
    let scr_mats = desks::spawn_desks(commands, &mut pal, materials, images, fonts, &spec);
    furniture::spawn_furniture(commands, &mut pal, materials, images, fonts, &spec);
    props::spawn_all_props(commands, &mut pal, materials, images, fonts, &spec);
    people::spawn_office_cast(commands, &mut pal, materials, &spec, look);
    life::spawn_life_elements(
        commands,
        &mut pal,
        materials,
        images,
        fonts,
        &spec,
        &life_state,
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
    commands.insert_resource(backdrop);
    commands.insert_resource(lights::LightingState { night });
    commands.insert_resource(scr_mats);
    commands.insert_resource(backdrop_assets);
    commands.insert_resource(pal);
}

/// True Unload helper: despawns every `WorldEntity` and purges every `WorldPalette`
/// mesh/material/image and `BackdropAssets` material/image so switching `3D -> 2D Lite`,
/// rebuilding layouts, or minimizing to System Tray leaks zero GPU/CPU assets.
pub fn unload_full_office_world(
    commands: &mut Commands,
    world_entities: impl IntoIterator<Item = Entity>,
    pal: Option<&mut WorldPalette>,
    backdrop_assets: Option<&mut BackdropAssets>,
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
    commands.remove_resource::<WorldPalette>();
    commands.remove_resource::<BackdropAssets>();
    commands.remove_resource::<desks::ScreenMaterials>();
    commands.remove_resource::<camera::CameraRig>();
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
                  mut shared_fonts: ResMut<SharedFontEngine>| {
                let fonts = shared_fonts.get_or_init();
                spawn_full_office_world(
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
                    let mut cmds = world.commands();
                    unload_full_office_world(
                        &mut cmds,
                        world_ents,
                        pal.as_mut(),
                        ba.as_mut(),
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
        assert_eq!(world.resource::<Assets<Mesh>>().len(), 0);
        assert_eq!(world.resource::<Assets<StandardMaterial>>().len(), 0);
        assert_eq!(world.resource::<Assets<Image>>().len(), 0);
        assert_eq!(world.query_filtered::<Entity, With<Camera3d>>().iter(&world).count(), 1);
        assert_eq!(world.query_filtered::<Entity, With<Lite2dCamera>>().iter(&world).count(), 1);
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
}
