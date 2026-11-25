use crate::layout::LayoutSpec;
use crate::palette::{WorldEntity, srgb_hex};
use bevy::prelude::*;

#[derive(Resource, Debug, Clone)]
pub struct LightingState {
    pub night: bool,
}

impl Default for LightingState {
    fn default() -> Self {
        Self { night: false }
    }
}

#[derive(Component)]
pub struct SunLight;

#[derive(Component)]
pub struct RoomLamp {
    pub base_intensity: f32,
}

/// Exact Oracle `src/office/scene/lights.js` sun + warm daylight / night room lamps.
pub fn spawn_office_lights(commands: &mut Commands, spec: &LayoutSpec, night: bool) {
    let tz = if spec.layout.is_hq() { 9.0 } else if spec.layout.is_pro() { 0.5 } else { 0.0 };
    let sun_pos = if spec.layout.is_pro() {
        Vec3::new(13.0, 18.0, tz + 8.5)
    } else {
        Vec3::new(10.0, 18.0, 8.0)
    };
    let sun_target = Vec3::new(spec.center.x, 0.0, tz);

    commands.insert_resource(ClearColor(if night {
        srgb_hex("#121724")
    } else {
        srgb_hex("#CFE0EC")
    }));

    commands.insert_resource(AmbientLight {
        color: if night { srgb_hex("#5A6E9E") } else { srgb_hex("#FFF4DC") },
        brightness: if night { 180.0 } else { 620.0 },
        ..default()
    });

    commands.spawn((
        WorldEntity,
        SunLight,
        DirectionalLight {
            color: if night { srgb_hex("#8EB0FF") } else { srgb_hex("#FFF0D6") },
            illuminance: if night { 1_800.0 } else { 15_500.0 },
            shadows_enabled: true,
            ..default()
        },
        Transform::from_translation(sun_pos).looking_at(sun_target, Vec3::Y),
    ));

    let lamp_coords: Vec<[f32; 3]> = if spec.layout.is_pro() {
        let mut pts = vec![
            [2.4, 2.8, 3.6],
            [-5.4, 2.6, -5.4],
            [0.2, 2.6, -5.5],
            [5.8, 2.6, -5.5],
            [11.3, 2.6, -4.0],
            [11.3, 2.6, 4.4],
            [-5.8, 2.6, 3.3],
            [-5.8, 2.6, -1.0],
        ];
        if spec.layout.is_hq() {
            pts.push([-2.85, 2.7, 18.4]);
            pts.push([9.15, 2.7, 18.4]);
        }
        pts
    } else {
        vec![
            [0.0, 2.8, 0.3],
            [-5.5, 2.4, 4.0],
            [5.5, 2.6, -3.4],
            [5.2, 2.6, 2.4],
            [-5.2, 2.6, -3.6],
        ]
    };

    for [x, y, z] in lamp_coords {
        commands.spawn((
            WorldEntity,
            RoomLamp { base_intensity: 28_000.0 },
            PointLight {
                color: srgb_hex("#FFC27A"),
                intensity: if night { 28_000.0 } else { 3_500.0 },
                range: 11.0,
                shadows_enabled: false,
                ..default()
            },
            Transform::from_xyz(x, y, z),
        ));
    }
}
