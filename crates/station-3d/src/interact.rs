use crate::palette::{WorldEntity, WorldPalette};
use bevy::prelude::*;

#[derive(Component, Debug, Clone)]
pub struct InteractiveProp {
    pub id: &'static str,
    pub place: &'static str,
    pub label: &'static str,
}

impl InteractiveProp {
    pub const fn new(id: &'static str, place: &'static str, label: &'static str) -> Self {
        Self { id, place, label }
    }
}

#[derive(Event, Debug, Clone)]
pub struct PropActivated {
    pub id: &'static str,
    pub place: &'static str,
    pub label: &'static str,
}

#[derive(Resource, Debug, Default, Clone)]
pub struct PlaceCardState {
    pub active_place: Option<String>,
    pub active_prop_label: Option<String>,
}

#[derive(Component)]
pub struct HoverRing;

pub fn spawn_hover_ring(
    commands: &mut Commands,
    palette: &mut WorldPalette,
    materials: &mut Assets<StandardMaterial>,
) {
    let ring_m = palette.emissive(materials, "#FFB23F", 1.4);
    commands.spawn((
        WorldEntity,
        HoverRing,
        Mesh3d(palette.unit_torus.clone()),
        MeshMaterial3d(ring_m),
        Transform::from_xyz(0.0, -50.0, 0.0).with_scale(Vec3::new(0.8, 0.15, 0.8)),
        Visibility::Hidden,
    ));
}
