pub mod camera;
pub mod chibi;
pub mod desks;
pub mod entrance;
pub mod furniture;
pub mod interact;
pub mod layout;
pub mod lights;
pub mod palette;
pub mod people;
pub mod props;
pub mod rooms;

use bevy::prelude::*;
pub use layout::{LayoutSpec, OfficeLayout};
pub use palette::{WorldEntity, WorldPalette};
use station_canvas2d::FontEngine;
use std::path::PathBuf;

pub struct OfficeWorldPlugin {
    pub layout: OfficeLayout,
    pub assets_dir: PathBuf,
    pub night: bool,
}

impl Plugin for OfficeWorldPlugin {
    fn build(&self, app: &mut App) {
        let layout = self.layout;
        let assets_dir = self.assets_dir.clone();
        let night = self.night;

        app.insert_resource(layout)
            .insert_resource(lights::LightingState { night })
            .insert_resource(interact::PlaceCardState::default())
            .add_event::<interact::PropActivated>()
            .add_systems(
                Startup,
                move |mut commands: Commands,
                      mut meshes: ResMut<Assets<Mesh>>,
                      mut materials: ResMut<Assets<StandardMaterial>>,
                      mut images: ResMut<Assets<Image>>| {
                    let spec = LayoutSpec::for_layout(layout);
                    let mut pal = WorldPalette::new(&mut meshes);
                    let mut fonts = FontEngine::from_assets_dir(&assets_dir);

                    lights::spawn_office_lights(&mut commands, &spec, night);
                    camera::spawn_office_camera(&mut commands, &spec);
                    rooms::spawn_office_shell(
                        &mut commands,
                        &mut pal,
                        &mut materials,
                        &mut images,
                        &mut fonts,
                        &spec,
                    );
                    entrance::spawn_entrance(&mut commands, &mut pal, &mut materials, &spec);
                    let scr_mats = desks::spawn_desks(
                        &mut commands,
                        &mut pal,
                        &mut materials,
                        &mut images,
                        &mut fonts,
                        &spec,
                    );
                    furniture::spawn_furniture(
                        &mut commands,
                        &mut pal,
                        &mut materials,
                        &mut images,
                        &mut fonts,
                        &spec,
                    );
                    props::spawn_all_props(
                        &mut commands,
                        &mut pal,
                        &mut materials,
                        &mut images,
                        &mut fonts,
                        &spec,
                    );
                    interact::spawn_hover_ring(&mut commands, &mut pal, &mut materials);

                    commands.insert_resource(scr_mats);
                    commands.insert_resource(pal);
                },
            )
            .add_systems(
                Update,
                (camera::camera_control_system, props::animate_props_system),
            );
    }
}

/// True Unload helper: despawns every `WorldEntity` and purges transient GPU images.
/// Used when minimizing to System Tray, switching 3D -> 2D Lite, or rebuilding layouts.
pub fn unload_office_world(
    mut commands: Commands,
    q: Query<Entity, With<WorldEntity>>,
    mut pal: Option<ResMut<WorldPalette>>,
    mut images: ResMut<Assets<Image>>,
) {
    for ent in &q {
        commands.entity(ent).despawn();
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
}
