use crate::layout::LayoutSpec;
use crate::palette::WorldEntity;
use bevy::input::mouse::{MouseMotion, MouseWheel};
use bevy::prelude::*;
use bevy::render::camera::ScalingMode;

#[derive(Component)]
pub struct MainOfficeCamera;

/// System set for `camera_control_system` in `PostUpdate`, ordered before
/// `TransformSystem::TransformPropagate` and after `egui`'s `EndPass` when `station-ui` is loaded.
#[derive(SystemSet, Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct CameraControlSet;

/// Marker for the bare 2D Lite camera (never tagged `WorldEntity` so unloading 3D never strands a black screen).
#[derive(Component, Debug, Clone, Copy)]
pub struct Lite2dCamera;

/// Tracks whether an overlay UI (`egui`) currently owns keyboard or pointer input.
/// Updated every frame by `station-ui` from `ctx.wants_keyboard_input()` and `ctx.wants_pointer_input()`.
#[derive(Resource, Debug, Default, Clone, Copy, PartialEq, Eq)]
pub struct UiInputCapture {
    pub wants_keyboard: bool,
    pub wants_pointer: bool,
}

#[derive(Resource, Debug, Clone)]
pub struct CameraRig {
    pub target: Vec3,
    pub yaw: f32,
    pub pitch: f32,
    pub distance: f32,
    pub viewport_height: f32,
    pub min_height: f32,
    pub max_height: f32,
    pub first_person: bool,
    pub fp_pos: Vec3,
    pub fp_yaw: f32,
}

impl CameraRig {
    pub fn from_layout(spec: &LayoutSpec) -> Self {
        let offset = spec.cam_offset;
        let dist = offset.length();
        let yaw = offset.x.atan2(offset.z);
        let pitch = (offset.y / dist).asin().clamp(0.42, 1.15);
        Self {
            target: spec.home,
            yaw,
            pitch,
            distance: dist,
            viewport_height: spec.viewport_height,
            min_height: 5.5,
            max_height: if spec.layout.is_hq() { 38.0 } else { 28.0 },
            first_person: false,
            fp_pos: Vec3::new(spec.home.x, 1.65, spec.home.z + 3.0),
            fp_yaw: 0.0,
        }
    }

    pub fn compute_transform(&self) -> Transform {
        if self.first_person {
            let look = self.fp_pos + Vec3::new(self.fp_yaw.sin(), -0.05, -self.fp_yaw.cos());
            return Transform::from_translation(self.fp_pos).looking_at(look, Vec3::Y);
        }
        let horiz = self.distance * self.pitch.cos();
        let pos = self.target
            + Vec3::new(
                horiz * self.yaw.sin(),
                self.distance * self.pitch.sin(),
                horiz * self.yaw.cos(),
            );
        Transform::from_translation(pos).looking_at(self.target, Vec3::Y)
    }
}

pub fn spawn_office_camera(commands: &mut Commands, spec: &LayoutSpec) {
    let rig = CameraRig::from_layout(spec);
    let tf = rig.compute_transform();
    let proj = Projection::Orthographic(OrthographicProjection {
        scaling_mode: ScalingMode::FixedVertical {
            viewport_height: rig.viewport_height,
        },
        near: -80.0,
        far: 180.0,
        ..OrthographicProjection::default_3d()
    });
    commands.insert_resource(rig);
    commands.spawn((
        WorldEntity,
        MainOfficeCamera,
        Camera3d::default(),
        proj,
        Msaa::Off,
        tf,
    ));
}

/// Spawns the bare camera used in 2D Lite mode (never tagged `WorldEntity`).
pub fn spawn_lite_2d_camera(commands: &mut Commands) -> Entity {
    commands
        .spawn((
            Lite2dCamera,
            Camera3d::default(),
            bevy::core_pipeline::tonemapping::Tonemapping::None,
            Msaa::Off,
            Transform::from_xyz(0.0, 0.0, 5.0).looking_at(Vec3::ZERO, Vec3::Y),
        ))
        .id()
}

#[allow(clippy::too_many_arguments)]
pub fn camera_control_system(
    time: Res<Time>,
    keys: Res<ButtonInput<KeyCode>>,
    buttons: Res<ButtonInput<MouseButton>>,
    mut motion_ev: EventReader<MouseMotion>,
    mut wheel_ev: EventReader<MouseWheel>,
    ui_capture: Option<Res<UiInputCapture>>,
    rig: Option<ResMut<CameraRig>>,
    mut q: Query<(&mut Transform, &mut Projection), With<MainOfficeCamera>>,
) {
    let Some(mut rig) = rig else {
        motion_ev.clear();
        wheel_ev.clear();
        return;
    };
    let Ok((mut tf, mut proj)) = q.single_mut() else {
        motion_ev.clear();
        wheel_ev.clear();
        return;
    };

    let block_keyboard = ui_capture.as_ref().is_some_and(|c| c.wants_keyboard);
    let block_pointer = ui_capture.as_ref().is_some_and(|c| c.wants_pointer);

    if !block_keyboard && keys.just_pressed(KeyCode::KeyF) {
        rig.first_person = !rig.first_person;
        if rig.first_person {
            *proj = Projection::Perspective(PerspectiveProjection {
                fov: 70.0_f32.to_radians(),
                near: 0.05,
                far: 240.0,
                ..default()
            });
        } else {
            *proj = Projection::Orthographic(OrthographicProjection {
                scaling_mode: ScalingMode::FixedVertical {
                    viewport_height: rig.viewport_height,
                },
                near: -80.0,
                far: 180.0,
                ..OrthographicProjection::default_3d()
            });
        }
    }

    let mut delta = Vec2::ZERO;
    for ev in motion_ev.read() {
        delta += ev.delta;
    }

    if rig.first_person {
        if !block_pointer
            && (buttons.pressed(MouseButton::Right) || buttons.pressed(MouseButton::Left))
        {
            rig.fp_yaw -= delta.x * 0.004;
        }
        if !block_keyboard {
            let speed = 5.0 * time.delta_secs();
            let fwd = Vec3::new(rig.fp_yaw.sin(), 0.0, -rig.fp_yaw.cos());
            let right = Vec3::new(rig.fp_yaw.cos(), 0.0, rig.fp_yaw.sin());
            if keys.pressed(KeyCode::KeyW) {
                rig.fp_pos += fwd * speed;
            }
            if keys.pressed(KeyCode::KeyS) {
                rig.fp_pos -= fwd * speed;
            }
            if keys.pressed(KeyCode::KeyD) {
                rig.fp_pos += right * speed;
            }
            if keys.pressed(KeyCode::KeyA) {
                rig.fp_pos -= right * speed;
            }
        }
        wheel_ev.clear();
    } else if !block_pointer {
        if buttons.pressed(MouseButton::Right) {
            rig.yaw = (rig.yaw - delta.x * 0.005).clamp(0.12, 1.45);
            rig.pitch = (rig.pitch + delta.y * 0.005).clamp(0.42, 1.15);
        }
        if buttons.pressed(MouseButton::Left) && delta != Vec2::ZERO {
            let pan_scale = rig.viewport_height * 0.0014;
            let right = Vec3::new(rig.yaw.cos(), 0.0, -rig.yaw.sin());
            let fwd = Vec3::new(-rig.yaw.sin(), 0.0, -rig.yaw.cos());
            rig.target -= right * (delta.x * pan_scale);
            rig.target += fwd * (delta.y * pan_scale);
        }
        for ev in wheel_ev.read() {
            rig.viewport_height = (rig.viewport_height * (1.0 - ev.y * 0.08))
                .clamp(rig.min_height, rig.max_height);
            if let Projection::Orthographic(ref mut o) = *proj {
                o.scaling_mode = ScalingMode::FixedVertical {
                    viewport_height: rig.viewport_height,
                };
            }
        }
    } else {
        wheel_ev.clear();
    }

    *tf = rig.compute_transform();
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::layout::OfficeLayout;
    use bevy::input::mouse::MouseScrollUnit;
    use std::time::Duration;

    fn make_camera_test_app() -> App {
        let mut app = App::new();
        app.insert_resource(Time::<()>::default())
            .insert_resource(ButtonInput::<KeyCode>::default())
            .insert_resource(ButtonInput::<MouseButton>::default())
            .insert_resource(UiInputCapture::default())
            .add_event::<MouseMotion>()
            .add_event::<MouseWheel>()
            .add_systems(Update, camera_control_system);

        let spec = LayoutSpec::for_layout(OfficeLayout::Pro);
        let mut cmds = app.world_mut().commands();
        spawn_office_camera(&mut cmds, &spec);
        app.world_mut().flush();
        app
    }

    #[test]
    fn camera_control_ignores_keyboard_and_pointer_when_ui_captures_input() {
        let mut app = make_camera_test_app();
        let initial = app.world().resource::<CameraRig>().clone();

        // 1. Typing `f` and `wasd` while egui wants keyboard input must NOT alter CameraRig.
        app.world_mut().resource_mut::<UiInputCapture>().wants_keyboard = true;
        {
            let mut keys = app.world_mut().resource_mut::<ButtonInput<KeyCode>>();
            keys.press(KeyCode::KeyF);
            keys.press(KeyCode::KeyW);
            keys.press(KeyCode::KeyD);
        }
        app.world_mut()
            .resource_mut::<Time>()
            .advance_by(Duration::from_millis(50));
        app.update();

        let after_kb = app.world().resource::<CameraRig>().clone();
        assert_eq!(after_kb.first_person, initial.first_person);
        assert_eq!(after_kb.fp_pos, initial.fp_pos);
        assert_eq!(after_kb.target, initial.target);

        // Also check WASD while already in first-person mode with egui keyboard capture active.
        app.world_mut().resource_mut::<CameraRig>().first_person = true;
        let fp_start = app.world().resource::<CameraRig>().fp_pos;
        app.world_mut()
            .resource_mut::<Time>()
            .advance_by(Duration::from_millis(50));
        app.update();
        assert_eq!(
            app.world().resource::<CameraRig>().fp_pos,
            fp_start,
            "WASD must not move first-person camera while egui wants keyboard input"
        );

        // 2. Dragging or scrolling while egui wants pointer input must NOT alter CameraRig.
        app.world_mut().resource_mut::<CameraRig>().first_person = false;
        app.world_mut().resource_mut::<ButtonInput<KeyCode>>().reset_all();
        {
            let mut cap = app.world_mut().resource_mut::<UiInputCapture>();
            cap.wants_keyboard = false;
            cap.wants_pointer = true;
        }
        app.world_mut()
            .resource_mut::<ButtonInput<MouseButton>>()
            .press(MouseButton::Right);
        app.world_mut().send_event(MouseMotion {
            delta: Vec2::new(80.0, 40.0),
        });
        app.world_mut().send_event(MouseWheel {
            unit: MouseScrollUnit::Line,
            x: 0.0,
            y: 4.0,
            window: Entity::PLACEHOLDER,
        });
        app.update();

        let after_ptr = app.world().resource::<CameraRig>().clone();
        assert_eq!(after_ptr.yaw, initial.yaw);
        assert_eq!(after_ptr.pitch, initial.pitch);
        assert_eq!(after_ptr.viewport_height, initial.viewport_height);

        // 3. Releasing UI capture restores normal keyboard control.
        {
            let mut cap = app.world_mut().resource_mut::<UiInputCapture>();
            cap.wants_keyboard = false;
            cap.wants_pointer = false;
        }
        app.world_mut()
            .resource_mut::<ButtonInput<MouseButton>>()
            .reset_all();
        app.world_mut()
            .resource_mut::<ButtonInput<KeyCode>>()
            .press(KeyCode::KeyF);
        app.update();
        assert!(app.world().resource::<CameraRig>().first_person);
    }
}
