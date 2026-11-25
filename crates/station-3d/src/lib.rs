pub mod camera;
pub mod entrance;
pub mod layout;
pub mod lights;
pub mod palette;
pub mod rooms;

use bevy::prelude::*;
pub use layout::{LayoutSpec, OfficeLayout};
pub use palette::{WorldEntity, WorldPalette};

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
