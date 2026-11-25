use crate::layout::LayoutSpec;
use crate::palette::{WorldPalette, spawn_box};
use bevy::prelude::*;

/// Cozy wooden entrance door OR Pro/HQ elevator shaft & green-exit stairwell door.
pub fn spawn_entrance(
    commands: &mut Commands,
    palette: &mut WorldPalette,
    materials: &mut Assets<StandardMaterial>,
    spec: &LayoutSpec,
) {
    if !spec.layout.is_pro() {
        let wood = palette.mat(materials, "#7A5236", 0.55, 0.0);
        let frame = palette.mat(materials, "#4E3422", 0.6, 0.0);
        let handle = palette.mat(materials, "#D4B160", 0.3, 0.7);
        spawn_box(commands, palette, frame, Vec3::new(-7.97, 1.1, spec.door_z), Vec3::new(0.08, 2.2, 1.12));
        spawn_box(commands, palette, wood, Vec3::new(-7.95, 1.07, spec.door_z), Vec3::new(0.06, 2.12, 0.98));
        spawn_box(commands, palette, handle, Vec3::new(-7.9, 1.02, spec.door_z + 0.36), Vec3::new(0.06, 0.04, 0.14));
        return;
    }

    // Pro / HQ Elevator shaft (z = 3.3) + stairwell fire door (z = 4.91)
    let steel = palette.mat(materials, "#B6BCC4", 0.32, 0.68);
    let trim = palette.mat(materials, "#343942", 0.4, 0.5);
    let led_g = palette.emissive(materials, "#3DDC84", 1.3);
    let exit_g = palette.emissive(materials, "#2EC866", 1.1);

    // Lift frame & twin sliding doors
    spawn_box(commands, palette, trim.clone(), Vec3::new(-7.45, 1.25, spec.door_z), Vec3::new(1.0, 2.5, 1.5));
    spawn_box(commands, palette, steel.clone(), Vec3::new(-6.93, 1.08, spec.door_z - 0.3), Vec3::new(0.05, 2.1, 0.58));
    spawn_box(commands, palette, steel, Vec3::new(-6.93, 1.08, spec.door_z + 0.3), Vec3::new(0.05, 2.1, 0.58));
    spawn_box(commands, palette, led_g, Vec3::new(-6.91, 2.3, spec.door_z), Vec3::new(0.04, 0.14, 0.36));

    // Fire stairwell door + green EXIT sign
    let fire_door = palette.mat(materials, "#6A727B", 0.5, 0.35);
    spawn_box(commands, palette, trim, Vec3::new(-7.95, 1.08, 4.91), Vec3::new(0.08, 2.16, 0.92));
    spawn_box(commands, palette, fire_door, Vec3::new(-7.92, 1.05, 4.91), Vec3::new(0.05, 2.08, 0.82));
    spawn_box(commands, palette, exit_g, Vec3::new(-7.9, 2.32, 4.91), Vec3::new(0.05, 0.16, 0.42));
}
