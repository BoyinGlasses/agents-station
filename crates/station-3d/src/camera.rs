use crate::layout::LayoutSpec;
use crate::palette::WorldEntity;
use bevy::input::mouse::{MouseMotion, MouseWheel};
use bevy::prelude::*;
use bevy::render::camera::ScalingMode;

#[derive(Component)]
pub struct MainOfficeCamera;

/// Marker for the bare 2D Lite camera (never tagged `WorldEntity` so unloading 3D never strands a black screen).
#[derive(Component, Debug, Clone, Copy)]
pub struct Lite2dCamera;

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

pub fn camera_control_system(
    time: Res<Time>,
    keys: Res<ButtonInput<KeyCode>>,
    buttons: Res<ButtonInput<MouseButton>>,
    mut motion_ev: EventReader<MouseMotion>,
    mut wheel_ev: EventReader<MouseWheel>,
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

    if keys.just_pressed(KeyCode::KeyF) {
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
        if buttons.pressed(MouseButton::Right) || buttons.pressed(MouseButton::Left) {
            rig.fp_yaw -= delta.x * 0.004;
        }
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
    } else {
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
    }

    *tf = rig.compute_transform();
}
