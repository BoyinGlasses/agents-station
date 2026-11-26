use crate::camera::MainOfficeCamera;
use crate::ibl::OfficeIbl;
use crate::layout::{LayoutSpec, OfficeLayout};
use crate::palette::{WorldEntity, srgb_hex};
use bevy::core_pipeline::bloom::Bloom;
use bevy::pbr::{CascadeShadowConfig, CascadeShadowConfigBuilder};
use bevy::prelude::*;
use bevy::render::camera::Exposure;

pub const DAY_SUN_LUX: f32 = 15_500.0;
pub const NIGHT_SUN_LUX: f32 = 800.0;
pub const DAY_BOUNCE_LUX: f32 = 1_800.0;
pub const NIGHT_BOUNCE_LUX: f32 = 220.0;
pub const DAY_IBL_INTENSITY: f32 = 280.0;
pub const NIGHT_IBL_INTENSITY: f32 = 180.0;
pub const DAY_AMBIENT_BRIGHTNESS: f32 = 190.0;
pub const NIGHT_AMBIENT_BRIGHTNESS: f32 = 95.0;
pub const ROOM_LAMP_NIGHT_LUMENS: f32 = 42_000.0;

#[derive(Resource, Debug, Clone)]
pub struct LightingState {
    pub night: bool,
    pub photo_hour: Option<f32>,
    pub room_lights_off: bool,
}

impl Default for LightingState {
    fn default() -> Self {
        Self {
            night: false,
            photo_hour: None,
            room_lights_off: false,
        }
    }
}

impl LightingState {
    pub fn new(night: bool) -> Self {
        Self {
            night,
            photo_hour: None,
            room_lights_off: false,
        }
    }

    pub fn is_night(&self) -> bool {
        self.photo_hour
            .map(|h| !(6.0..18.0).contains(&h))
            .unwrap_or(self.night)
    }
}

/// System set for `sync_lighting_system` in `Update`, allowing UI action handlers
/// (e.g. `process_ui_actions`) to order `.before(LightingSyncSet)` for zero-frame-latency sync.
#[derive(SystemSet, Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct LightingSyncSet;

#[derive(Component)]
pub struct SunLight;

#[derive(Component)]
pub struct BounceLight;

#[derive(Component)]
pub struct RoomLamp {
    pub base_intensity: f32,
}

pub fn cascade_shadow_config_for_layout(layout: OfficeLayout) -> CascadeShadowConfig {
    // Tight 2-cascade frustum bounds matched to lights.js:19 (±17m Cozy, ±21m Pro, ±28m HQ)
    // and isometric camera distance (~30–40m), covering both ortho overview and first-person walk.
    let (first_far, max_dist) = match layout {
        OfficeLayout::Cozy => (24.0, 56.0),
        OfficeLayout::Pro => (32.0, 66.0),
        OfficeLayout::Hq => (42.0, 88.0),
    };
    CascadeShadowConfigBuilder {
        num_cascades: 2,
        minimum_distance: 0.5,
        first_cascade_far_bound: first_far,
        maximum_distance: max_dist,
        overlap_proportion: 0.2,
    }
    .build()
}

/// Live lighting synchronizer reacting to `LightingState` changes (`ToggleNight` / `photo_hour`).
#[allow(clippy::too_many_arguments)]
pub fn sync_lighting_system(
    lighting: Option<Res<LightingState>>,
    ibl: Option<Res<OfficeIbl>>,
    mut ambient: Option<ResMut<AmbientLight>>,
    mut clear_color: Option<ResMut<ClearColor>>,
    mut sun_q: Query<&mut DirectionalLight, (With<SunLight>, Without<BounceLight>)>,
    mut bounce_q: Query<&mut DirectionalLight, (With<BounceLight>, Without<SunLight>)>,
    mut lamp_q: Query<(&RoomLamp, &mut PointLight)>,
    mut cam_q: Query<
        (
            Option<&mut EnvironmentMapLight>,
            Option<&mut Bloom>,
            Option<&mut Exposure>,
        ),
        With<MainOfficeCamera>,
    >,
) {
    let Some(lighting) = lighting else {
        return;
    };
    if !lighting.is_changed() {
        return;
    }
    let night = lighting.is_night();

    if let Some(ref mut amb) = ambient {
        amb.color = if night {
            srgb_hex("#8FA3D9")
        } else {
            srgb_hex("#FFF4DC")
        };
        amb.brightness = if night {
            NIGHT_AMBIENT_BRIGHTNESS
        } else {
            DAY_AMBIENT_BRIGHTNESS
        };
    }

    if !cam_q.is_empty()
        && let Some(ref mut cc) = clear_color
    {
        cc.0 = if night {
            srgb_hex("#121724")
        } else {
            srgb_hex("#CFE0EC")
        };
    }

    for mut sun in &mut sun_q {
        sun.color = if night {
            srgb_hex("#9DB2FF")
        } else {
            srgb_hex("#FFF0D6")
        };
        sun.illuminance = if night { NIGHT_SUN_LUX } else { DAY_SUN_LUX };
    }

    for mut bounce in &mut bounce_q {
        bounce.color = if night {
            srgb_hex("#2E3654")
        } else {
            srgb_hex("#A9B98A")
        };
        bounce.illuminance = if night {
            NIGHT_BOUNCE_LUX
        } else {
            DAY_BOUNCE_LUX
        };
    }

    for (lamp, mut pt) in &mut lamp_q {
        pt.intensity = if night && !lighting.room_lights_off {
            lamp.base_intensity
        } else {
            0.0
        };
    }

    for (env_opt, bloom_opt, exp_opt) in &mut cam_q {
        if let (Some(mut env), Some(ibl_res)) = (env_opt, ibl.as_deref()) {
            if night {
                env.diffuse_map = ibl_res.night_diffuse.clone();
                env.specular_map = ibl_res.night_specular.clone();
                env.intensity = NIGHT_IBL_INTENSITY;
            } else {
                env.diffuse_map = ibl_res.day_diffuse.clone();
                env.specular_map = ibl_res.day_specular.clone();
                env.intensity = DAY_IBL_INTENSITY;
            }
        }
        if let Some(mut bloom) = bloom_opt {
            *bloom = if night {
                crate::camera::night_bloom()
            } else {
                crate::camera::day_bloom()
            };
        }
        if let Some(mut exp) = exp_opt {
            exp.ev100 = if night {
                crate::camera::NIGHT_EV100
            } else {
                crate::camera::DAY_EV100
            };
        }
    }
}

/// Exact Oracle `src/office/scene/lights.js` sun, hemisphere bounce fill, and warm night room lamps.
pub fn spawn_office_lights(commands: &mut Commands, spec: &LayoutSpec, night: bool) {
    let tz = if spec.layout.is_hq() {
        9.0
    } else if spec.layout.is_pro() {
        0.5
    } else {
        0.0
    };
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
        color: if night {
            srgb_hex("#8FA3D9")
        } else {
            srgb_hex("#FFF4DC")
        },
        brightness: if night {
            NIGHT_AMBIENT_BRIGHTNESS
        } else {
            DAY_AMBIENT_BRIGHTNESS
        },
        ..default()
    });

    commands.spawn((
        WorldEntity,
        SunLight,
        DirectionalLight {
            color: if night {
                srgb_hex("#9DB2FF")
            } else {
                srgb_hex("#FFF0D6")
            },
            illuminance: if night { NIGHT_SUN_LUX } else { DAY_SUN_LUX },
            shadows_enabled: true,
            shadow_depth_bias: 0.02,
            shadow_normal_bias: 1.1,
            ..default()
        },
        cascade_shadow_config_for_layout(spec.layout),
        Transform::from_translation(sun_pos).looking_at(sun_target, Vec3::Y),
    ));

    // Upward ground-bounce directional fill matching Three.js HemisphereLight groundColor (#A9B98A / #2E3654).
    commands.spawn((
        WorldEntity,
        BounceLight,
        DirectionalLight {
            color: if night {
                srgb_hex("#2E3654")
            } else {
                srgb_hex("#A9B98A")
            },
            illuminance: if night {
                NIGHT_BOUNCE_LUX
            } else {
                DAY_BOUNCE_LUX
            },
            shadows_enabled: false,
            ..default()
        },
        Transform::from_xyz(-8.0, -14.0, -6.0).looking_at(Vec3::ZERO, Vec3::Y),
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
            RoomLamp {
                base_intensity: ROOM_LAMP_NIGHT_LUMENS,
            },
            PointLight {
                color: srgb_hex("#FFC27A"),
                intensity: if night { ROOM_LAMP_NIGHT_LUMENS } else { 0.0 },
                range: 11.0,
                shadows_enabled: false,
                ..default()
            },
            Transform::from_xyz(x, y, z),
        ));
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::camera::{MainOfficeCamera, spawn_office_camera_with_ibl};
    use crate::ibl::OfficeIbl;
    use bevy::core_pipeline::tonemapping::Tonemapping;

    #[test]
    fn sync_lighting_system_toggles_sun_bounce_lamps_ibl_and_bloom_between_day_and_night() {
        let mut app = App::new();
        let mut images = Assets::<Image>::default();
        let ibl = OfficeIbl::generate(&mut images);
        let day_diff = ibl.day_diffuse.clone();
        let day_spec = ibl.day_specular.clone();
        let night_diff = ibl.night_diffuse.clone();
        let night_spec = ibl.night_specular.clone();

        app.insert_resource(images)
            .insert_resource(ibl.clone())
            .insert_resource(LightingState::new(false))
            .add_systems(Update, sync_lighting_system);

        let spec = LayoutSpec::for_layout(OfficeLayout::Pro);
        let mut cmds = app.world_mut().commands();
        spawn_office_lights(&mut cmds, &spec, false);
        spawn_office_camera_with_ibl(&mut cmds, &spec, &ibl, false);
        app.world_mut().flush();

        // Verify initial day state
        app.update();
        let sun = app
            .world_mut()
            .query_filtered::<&DirectionalLight, (With<SunLight>, Without<BounceLight>)>()
            .single(app.world())
            .expect("SunLight present");
        assert!((sun.illuminance - DAY_SUN_LUX).abs() < 1.0);

        let bounce_count = app
            .world_mut()
            .query_filtered::<&DirectionalLight, With<BounceLight>>()
            .iter(app.world())
            .count();
        assert_eq!(bounce_count, 1, "upward ground-bounce directional light must exist");

        let (msaa, tm, env, bloom, exp) = app
            .world_mut()
            .query_filtered::<(
                &Msaa,
                &Tonemapping,
                &EnvironmentMapLight,
                &Bloom,
                &Exposure,
            ), With<MainOfficeCamera>>()
            .single(app.world())
            .expect("MainOfficeCamera with Msaa, Tonemapping, EnvironmentMapLight, Bloom, Exposure");
        assert_eq!(*msaa, Msaa::Sample4);
        assert_eq!(*tm, Tonemapping::AcesFitted);
        assert_eq!(env.diffuse_map, day_diff);
        assert_eq!(env.specular_map, day_spec);
        assert_eq!(bloom.intensity, 0.0, "Bloom must be off during day");
        assert!((exp.ev100 - crate::camera::DAY_EV100).abs() < 1e-4);

        let bounce_day = app
            .world_mut()
            .query_filtered::<&DirectionalLight, (With<BounceLight>, Without<SunLight>)>()
            .single(app.world())
            .unwrap();
        assert!((bounce_day.illuminance - DAY_BOUNCE_LUX).abs() < 1.0);
        assert!((app.world().resource::<AmbientLight>().brightness - DAY_AMBIENT_BRIGHTNESS).abs() < 1.0);
        assert_eq!(app.world().resource::<ClearColor>().0, srgb_hex("#CFE0EC"));

        // Flip to night
        app.world_mut().resource_mut::<LightingState>().night = true;
        app.update();

        let sun_night = app
            .world_mut()
            .query_filtered::<&DirectionalLight, (With<SunLight>, Without<BounceLight>)>()
            .single(app.world())
            .unwrap();
        assert!(
            (sun_night.illuminance - NIGHT_SUN_LUX).abs() < 1.0,
            "SunLight must dim from 15_500 to 800 lux at night, got {}",
            sun_night.illuminance
        );

        let bounce_night = app
            .world_mut()
            .query_filtered::<&DirectionalLight, (With<BounceLight>, Without<SunLight>)>()
            .single(app.world())
            .unwrap();
        assert!((bounce_night.illuminance - NIGHT_BOUNCE_LUX).abs() < 1.0);
        assert!(
            (app.world().resource::<AmbientLight>().brightness - NIGHT_AMBIENT_BRIGHTNESS).abs()
                < 1.0
        );
        assert_eq!(app.world().resource::<ClearColor>().0, srgb_hex("#121724"));

        let lamps_night: Vec<f32> = app
            .world_mut()
            .query_filtered::<&PointLight, With<RoomLamp>>()
            .iter(app.world())
            .map(|p| p.intensity)
            .collect();
        assert!(!lamps_night.is_empty());
        assert!(
            lamps_night.iter().all(|&i| i >= 20_000.0),
            "RoomLamps must boost at night, got {lamps_night:?}"
        );

        let (env_night, bloom_night, exp_night) = app
            .world_mut()
            .query_filtered::<(&EnvironmentMapLight, &Bloom, &Exposure), With<MainOfficeCamera>>()
            .single(app.world())
            .unwrap();
        assert_eq!(env_night.diffuse_map, night_diff);
        assert_eq!(env_night.specular_map, night_spec);
        assert!(
            bloom_night.intensity > 0.05,
            "Night bloom must activate, got {}",
            bloom_night.intensity
        );
        assert!((exp_night.ev100 - crate::camera::NIGHT_EV100).abs() < 1e-4);

        // Flip back to day
        app.world_mut().resource_mut::<LightingState>().night = false;
        app.update();

        let sun_restored = app
            .world_mut()
            .query_filtered::<&DirectionalLight, (With<SunLight>, Without<BounceLight>)>()
            .single(app.world())
            .unwrap();
        assert!((sun_restored.illuminance - DAY_SUN_LUX).abs() < 1.0);
        let bounce_restored = app
            .world_mut()
            .query_filtered::<&DirectionalLight, (With<BounceLight>, Without<SunLight>)>()
            .single(app.world())
            .unwrap();
        assert!((bounce_restored.illuminance - DAY_BOUNCE_LUX).abs() < 1.0);
        assert!((app.world().resource::<AmbientLight>().brightness - DAY_AMBIENT_BRIGHTNESS).abs() < 1.0);
        assert_eq!(app.world().resource::<ClearColor>().0, srgb_hex("#CFE0EC"));

        let lamps_day: Vec<f32> = app
            .world_mut()
            .query_filtered::<&PointLight, With<RoomLamp>>()
            .iter(app.world())
            .map(|p| p.intensity)
            .collect();
        assert!(lamps_day.iter().all(|&i| i == 0.0));

        let (env_day, bloom_day, exp_day) = app
            .world_mut()
            .query_filtered::<(&EnvironmentMapLight, &Bloom, &Exposure), With<MainOfficeCamera>>()
            .single(app.world())
            .unwrap();
        assert_eq!(env_day.diffuse_map, day_diff);
        assert_eq!(env_day.specular_map, day_spec);
        assert_eq!(bloom_day.intensity, 0.0);
        assert!((exp_day.ev100 - crate::camera::DAY_EV100).abs() < 1e-4);

        // photo_hour override forces night at 22:00 even when night=false, and day at 12:00 when night=true
        app.world_mut().resource_mut::<LightingState>().photo_hour = Some(22.0);
        app.update();
        let sun_photo_night = app
            .world_mut()
            .query_filtered::<&DirectionalLight, (With<SunLight>, Without<BounceLight>)>()
            .single(app.world())
            .unwrap();
        assert!((sun_photo_night.illuminance - NIGHT_SUN_LUX).abs() < 1.0);
        assert_eq!(app.world().resource::<ClearColor>().0, srgb_hex("#121724"));

        {
            let mut st = app.world_mut().resource_mut::<LightingState>();
            st.night = true;
            st.photo_hour = Some(12.0);
        }
        app.update();
        let sun_photo_day = app
            .world_mut()
            .query_filtered::<&DirectionalLight, (With<SunLight>, Without<BounceLight>)>()
            .single(app.world())
            .unwrap();
        assert!((sun_photo_day.illuminance - DAY_SUN_LUX).abs() < 1.0);
        assert_eq!(app.world().resource::<ClearColor>().0, srgb_hex("#CFE0EC"));
    }

    #[test]
    fn sync_lighting_system_preserves_lite_2d_clear_color_without_main_camera() {
        let lite_bg = Color::srgb(0.06, 0.075, 0.105);
        let mut app = App::new();
        app.insert_resource(ClearColor(lite_bg))
            .insert_resource(LightingState::new(false))
            .add_systems(Update, sync_lighting_system);

        app.update();
        assert_eq!(
            app.world().resource::<ClearColor>().0,
            lite_bg,
            "Lite2D/PanelCompact ClearColor must not be overwritten when MainOfficeCamera is absent"
        );

        app.world_mut().resource_mut::<LightingState>().night = true;
        app.update();
        assert_eq!(app.world().resource::<ClearColor>().0, lite_bg);
    }

    #[test]
    fn cascade_shadow_config_tightens_per_layout() {
        let cozy = cascade_shadow_config_for_layout(OfficeLayout::Cozy);
        let pro = cascade_shadow_config_for_layout(OfficeLayout::Pro);
        let hq = cascade_shadow_config_for_layout(OfficeLayout::Hq);
        let cozy_max = *cozy.bounds.last().unwrap();
        let pro_max = *pro.bounds.last().unwrap();
        let hq_max = *hq.bounds.last().unwrap();
        assert!(cozy_max < pro_max && pro_max < hq_max);
        assert!(hq_max <= 95.0, "HQ shadow cascade must stay tight, got {hq_max}");
    }
}
