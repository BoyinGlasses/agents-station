use crate::chibi_props::PropKind;
use crate::ibl::OfficeIbl;
use crate::layout::{LayoutSpec, OfficeLayout};
use crate::lights::{DAY_IBL_INTENSITY, NIGHT_IBL_INTENSITY};
use crate::palette::WorldEntity;
use bevy::core_pipeline::bloom::{Bloom, BloomCompositeMode, BloomPrefilter};
use bevy::core_pipeline::tonemapping::Tonemapping;
use bevy::ecs::system::SystemParam;
use bevy::input::mouse::{MouseMotion, MouseWheel};
use bevy::prelude::*;
use bevy::render::camera::{Exposure, ScalingMode};

/// 10 cm first-person collision cell resolution matching `first-person.js:23` (`CELL = 0.1`).
pub const FP_CELL_SIZE: f32 = 0.10;
pub const FP_PLAYER_RADIUS: f32 = 0.20;

/// 10 cm (`0.10 m`) 2D occupancy grid for first-person wall, partition, and furniture collision.
#[derive(Debug, Clone)]
pub struct ObstacleGrid {
    pub x0: f32,
    pub z0: f32,
    pub width: usize,
    pub height: usize,
    pub cells: Vec<u8>,
}

impl ObstacleGrid {
    /// Rasterizes office walls, interior glass/cutaway partitions (leaving exact doorway gaps open),
    /// desks, server racks, sofa, dining tables, WC stalls, billiards/karaoke/arcade/aquarium, and
    /// reception/director furniture at 10 cm (`0.10 m`) resolution matching `first-person.js:24-51`.
    pub fn for_layout(layout: OfficeLayout) -> Self {
        let spec = LayoutSpec::for_layout(layout);
        let (x0, x1, z0, z1) = match layout {
            OfficeLayout::Hq => (-7.85_f32, 15.3_f32, -7.85_f32, 28.3_f32),
            OfficeLayout::Pro => (-7.85, 15.3, -7.85, 11.3),
            OfficeLayout::Cozy => (-7.85, 11.7, -5.85, 10.3),
        };
        let width = ((x1 - x0) / FP_CELL_SIZE).ceil().max(1.0) as usize;
        let height = ((z1 - z0) / FP_CELL_SIZE).ceil().max(1.0) as usize;
        let mut grid = Self {
            x0,
            z0,
            width,
            height,
            cells: vec![0u8; width * height],
        };

        // 1. Interior glass & cutaway walls from single source of truth `nav::build_partition_walls`
        //    (doorway gaps remain 0 = passable!).
        for seg in crate::nav::build_partition_walls(layout) {
            grid.mark_segment(seg.a, seg.b, 0.08);
        }

        // 2. Desks (oriented `1.10 × 0.58 m` top footprints)
        for d in &spec.desks {
            let rot = Quat::from_rotation_y(d.ry);
            let corner_x = (rot * Vec3::new(0.55, 0.0, 0.0)).x.abs()
                + (rot * Vec3::new(0.0, 0.0, 0.29)).x.abs();
            let corner_z = (rot * Vec3::new(0.55, 0.0, 0.0)).z.abs()
                + (rot * Vec3::new(0.0, 0.0, 0.29)).z.abs();
            grid.mark_aabb(d.x - corner_x, d.x + corner_x, d.z - corner_z, d.z + corner_z);
        }

        // 3. Shared furniture & props (server racks, sofa, meeting table, coffee bar, bookshelf, arcade, aquarium, printer)
        grid.mark_rect_center(spec.racks[0], spec.racks[1], 0.55, 1.35);
        grid.mark_rect_center(spec.sofa[0], spec.sofa[1], spec.sofa[2] * 0.5, 0.45);
        grid.mark_rect_center(spec.meet[0], spec.meet[1], 1.15, 0.65);
        grid.mark_rect_center(spec.coffee[0], spec.coffee[1], 1.0, 0.45);
        grid.mark_rect_center(spec.shelf[0], spec.shelf[1], 1.35, 0.30);
        grid.mark_rect_center(spec.arc[0], spec.arc[1], 0.42, 0.42);
        grid.mark_rect_center(spec.aq[0], spec.aq[1], 1.05, 0.34);
        grid.mark_rect_center(spec.prn[0], spec.prn[1], 0.38, 0.32);

        if let Some([bx, bz]) = spec.bil {
            grid.mark_rect_center(bx, bz, 0.64, 1.10);
        }
        if let Some([kx, kz, _]) = spec.ktv {
            grid.mark_rect_center(kx, kz, 0.62, 0.64);
        }

        // 4. Pro & HQ room furniture: Director desk/credenza, reception counter, pantry dining tables, WC stalls, lift shaft
        if layout.is_pro() {
            // Elevator shaft enclosure (`x in -8.0..-6.95, z in door_z±1.05`), leaving `x > -6.95` lobby clear!
            grid.mark_aabb(-8.0, -6.98, spec.door_z - 1.05, spec.door_z + 1.05);
            // Director executive desk & credenza
            grid.mark_rect_center(-5.4, -5.8, 1.05, 0.48);
            grid.mark_rect_center(-5.3, -7.7, 1.65, 0.28);
            // Reception counter (`pro_floor.rs`)
            grid.mark_rect_center(-5.35, 2.05, 0.52, 1.05);
            // WC stalls along far left wall (`x in -8.0..-6.75, z in 5.5..8.2`)
            grid.mark_aabb(-8.0, -6.75, 5.55, 8.20);
            // Pantry dining tables (`pro_floor.rs`)
            for (tx, tz) in [(10.15_f32, -5.8_f32), (12.55, -5.8), (10.15, -3.2), (12.55, -3.2)] {
                grid.mark_rect_center(tx, tz, 0.58, 0.42);
            }
        }

        grid
    }

    pub fn mark_aabb(&mut self, min_x: f32, max_x: f32, min_z: f32, max_z: f32) {
        let i0 = ((min_x - self.x0) / FP_CELL_SIZE).floor().max(0.0) as usize;
        let i1 = (((max_x - self.x0) / FP_CELL_SIZE).floor() as isize)
            .clamp(-1, self.width as isize - 1);
        let j0 = ((min_z - self.z0) / FP_CELL_SIZE).floor().max(0.0) as usize;
        let j1 = (((max_z - self.z0) / FP_CELL_SIZE).floor() as isize)
            .clamp(-1, self.height as isize - 1);
        if i1 < i0 as isize || j1 < j0 as isize {
            return;
        }
        for j in j0..=(j1 as usize) {
            let row = j * self.width;
            for i in i0..=(i1 as usize) {
                self.cells[row + i] = 1;
            }
        }
    }

    pub fn mark_rect_center(&mut self, cx: f32, cz: f32, hx: f32, hz: f32) {
        self.mark_aabb(cx - hx, cx + hx, cz - hz, cz + hz);
    }

    pub fn mark_segment(&mut self, a: Vec2, b: Vec2, half_thick: f32) {
        let len = a.distance(b);
        let steps = ((len / (FP_CELL_SIZE * 0.5)).ceil() as usize).max(1);
        for s in 0..=steps {
            let t = s as f32 / steps as f32;
            let p = a.lerp(b, t);
            self.mark_rect_center(p.x, p.y, half_thick, half_thick);
        }
    }

    pub fn is_blocked_cell(&self, x: f32, z: f32) -> bool {
        let fx = (x - self.x0) / FP_CELL_SIZE;
        let fz = (z - self.z0) / FP_CELL_SIZE;
        if fx < 0.0 || fz < 0.0 {
            return true;
        }
        let ix = fx.floor() as usize;
        let iz = fz.floor() as usize;
        if ix >= self.width || iz >= self.height {
            return true;
        }
        self.cells[iz * self.width + ix] != 0
    }

    /// 9-point footprint clearance test matching `first-person.js:48-51`.
    pub fn can_stand(&self, x: f32, z: f32, radius: f32) -> bool {
        let offsets = [
            (0.0, 0.0),
            (-radius, -radius),
            (radius, -radius),
            (-radius, radius),
            (radius, radius),
            (radius, 0.0),
            (-radius, 0.0),
            (0.0, radius),
            (0.0, -radius),
        ];
        for (dx, dz) in offsets {
            if self.is_blocked_cell(x + dx, z + dz) {
                return false;
            }
        }
        true
    }

    /// Steps along `delta` in 5 cm substeps resolving X and Z independently so diagonal wall
    /// contact slides cleanly and fast steps never tunnel through thin 10 cm glass partitions.
    pub fn try_move(&self, pos: Vec2, delta: Vec2, radius: f32) -> Vec2 {
        let dist = delta.length();
        if dist < 1e-6 {
            return pos;
        }
        let steps = ((dist / 0.05).ceil() as usize).max(1);
        let step = delta / steps as f32;
        let mut cur = pos;
        for _ in 0..steps {
            let stuck = !self.can_stand(cur.x, cur.y, radius);
            if stuck || self.can_stand(cur.x + step.x, cur.y, radius) {
                cur.x += step.x;
            }
            if stuck || self.can_stand(cur.x, cur.y + step.y, radius) {
                cur.y += step.y;
            }
        }
        cur
    }
}

#[derive(Component, Debug, Clone, Copy)]
pub struct FirstPersonAvatar;

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum FpFadeState {
    Idle,
    FadingOut {
        timer: f32,
        duration: f32,
        to_street: bool,
    },
    FadingIn {
        timer: f32,
        duration: f32,
    },
}

#[derive(Debug, Clone, PartialEq)]
pub enum FirstPersonPrompt {
    LiftToStreet {
        stairs: bool,
    },
    LiftToOffice,
    ShopTake {
        chain: &'static str,
        shop_name: &'static str,
        item_id: &'static str,
        price_k: u32,
        carry: PropKind,
    },
    ShopPay {
        chain: &'static str,
        shop_name: &'static str,
        default_item_id: &'static str,
        default_price_k: u32,
        default_carry: PropKind,
        basket_count: usize,
        basket_total_k: u32,
    },
    ShopSit {
        shop_name: &'static str,
        seat_pos: Vec2,
        seat_yaw: f32,
        is_stool: bool,
    },
    BilliardsPlay,
    TvGamesPlay,
    OfficeProp {
        id: &'static str,
        place: &'static str,
    },
    OfficeWorker {
        session_id: String,
        name: String,
        asleep: bool,
    },
}

#[derive(Debug, Clone, PartialEq)]
pub enum FirstPersonActivity {
    Walking,
    SeatedShop {
        shop_name: &'static str,
        seat_pos: Vec2,
        seat_yaw: f32,
    },
    PlayingBilliards {
        aim_yaw: f32,
        power: f32,
        charge_dir: f32,
        charging: bool,
        shots: u32,
        potted: u32,
    },
    PlayingTvGames {
        game_index: usize,
        seat_pos: Vec2,
        seat_yaw: f32,
    },
}

#[derive(Debug, Clone)]
pub struct ShopBasketItem {
    pub item_id: &'static str,
    pub price_k: u32,
    pub carry: PropKind,
}

#[derive(Debug, Clone)]
pub struct FpToastNotice {
    pub i18n_key: &'static str,
    pub vars: Vec<(&'static str, String)>,
}

#[derive(Resource, Debug, Clone)]
pub struct FirstPersonController {
    pub active: bool,
    pub third_person: bool,
    pub on_street: bool,
    pub pos: Vec2,
    pub ground_y: f32,
    pub yaw: f32,
    pub pitch: f32,
    pub moving: bool,
    pub elapsed: f32,
    pub grid: ObstacleGrid,
    pub fade: FpFadeState,
    pub fade_alpha: f32,
    pub near_prompt: Option<FirstPersonPrompt>,
    pub activity: FirstPersonActivity,
    pub basket: Vec<ShopBasketItem>,
    pub carrying: Option<PropKind>,
    pub carry_until: f32,
    pub interact_requested: bool,
    pub pending_billiards_shot: Option<(Vec2, f32)>,
    pub pending_tv_game_index: Option<usize>,
    pub pending_toasts: Vec<FpToastNotice>,
    pub pending_sfx: Vec<crate::audio::SfxKind>,
}

impl FirstPersonController {
    pub fn from_layout(spec: &LayoutSpec) -> Self {
        let start = if spec.layout.is_pro() {
            Vec2::new(-6.15, spec.door_z)
        } else {
            Vec2::new(-6.35, spec.door_z)
        };
        Self {
            active: false,
            third_person: false,
            on_street: false,
            pos: start,
            ground_y: 0.0,
            yaw: std::f32::consts::FRAC_PI_2,
            pitch: -0.06,
            moving: false,
            elapsed: 0.0,
            grid: ObstacleGrid::for_layout(spec.layout),
            fade: FpFadeState::Idle,
            fade_alpha: 0.0,
            near_prompt: None,
            activity: FirstPersonActivity::Walking,
            basket: Vec::new(),
            carrying: None,
            carry_until: 0.0,
            interact_requested: false,
            pending_billiards_shot: None,
            pending_tv_game_index: None,
            pending_toasts: Vec::new(),
            pending_sfx: Vec::new(),
        }
    }

    pub fn format_vnd(price_k: u32) -> String {
        let amount = price_k * 1000;
        let s = amount.to_string();
        let mut out = String::with_capacity(s.len() + 4);
        for (i, ch) in s.chars().enumerate() {
            let rem = s.len() - i;
            if i > 0 && rem.is_multiple_of(3) {
                out.push('.');
            }
            out.push(ch);
        }
        out.push('đ');
        out
    }

    pub fn start_fade_transition(&mut self, to_street: bool, stairs: bool) {
        if !matches!(self.fade, FpFadeState::Idle) {
            return;
        }
        let dur = if stairs { 0.55 } else { 0.22 };
        self.fade = FpFadeState::FadingOut {
            timer: 0.0,
            duration: dur,
            to_street,
        };
        self.pending_sfx.push(if stairs {
            crate::audio::SfxKind::Click
        } else {
            crate::audio::SfxKind::Ding
        });
        if stairs {
            self.pending_toasts.push(FpToastNotice {
                i18n_key: if to_street { "fp.stairsDown" } else { "fp.stairsUp" },
                vars: Vec::new(),
            });
        }
    }

    pub fn return_to_office_now(&mut self, spec: &LayoutSpec) {
        self.on_street = false;
        self.ground_y = 0.0;
        self.pos = Vec2::new(if spec.layout.is_pro() { -6.15 } else { -6.35 }, spec.door_z);
        self.yaw = std::f32::consts::FRAC_PI_2;
        self.pitch = -0.06;
        self.activity = FirstPersonActivity::Walking;
        self.near_prompt = None;
        self.fade = FpFadeState::Idle;
        self.fade_alpha = 0.0;
    }
}

/// Spawns the visible `"Bạn"` 3rd-person `ChibiRig` avatar using shared `ChibiMeshCache` and
/// `WorldPalette` so `unload_full_office_world` leaves 0 leaked assets (`first-person.js:52-58`).
pub fn spawn_fp_avatar(
    commands: &mut Commands,
    cache: &crate::chibi_meshes::ChibiMeshCache,
    palette: &mut crate::palette::WorldPalette,
    materials: &mut Assets<StandardMaterial>,
    spec: &LayoutSpec,
    look: crate::chibi::ChibiLook,
) -> Entity {
    let mut cfg = crate::chibi::ChibiConfig::for_agent("claude", "Bạn", 0);
    cfg.look = look;
    cfg.shirt = "#F2B33D";
    cfg.hair = "#2B2420";
    cfg.hair_style = crate::chibi_meshes::HairStyle::Plain;
    cfg.outfit = crate::chibi::OutfitKind::Hoodie;
    cfg.pants = "#33415C";
    cfg.accessory = None;
    let start_x = if spec.layout.is_pro() { -6.15 } else { -6.35 };
    let ent = crate::chibi::spawn_chibi(
        commands,
        cache,
        palette,
        materials,
        &cfg,
        Vec3::new(start_x, 0.0, spec.door_z),
        std::f32::consts::FRAC_PI_2,
        crate::chibi::WorkerAnim::Rest,
        0.0,
        false,
    );
    commands
        .entity(ent)
        .insert((FirstPersonAvatar, Visibility::Hidden));
    ent
}

/// Calibrated EV100 matching Three.js `ACESFilmicToneMapping` day/night exposure ratio (`room-update.js:162`):
/// Day (`10.45`) prevents `15_500` lux highlight washout while preserving crisp shadow contrast;
/// Night (`9.54`, ~`1.12x` over Blender 9.7 baseline) lifts warm room lamps and emissive bloom cleanly.
pub const DAY_EV100: f32 = 10.45;
pub const NIGHT_EV100: f32 = 9.54;

/// Day bloom stays at intensity `0.0` so `BloomNode` skips all passes (matching `room-update.js:163`).
pub fn day_bloom() -> Bloom {
    Bloom {
        intensity: 0.0,
        low_frequency_boost: 0.45,
        low_frequency_boost_curvature: 0.85,
        high_pass_frequency: 1.0,
        prefilter: BloomPrefilter {
            threshold: 1.6,
            threshold_softness: 0.25,
        },
        composite_mode: BloomCompositeMode::Additive,
        ..default()
    }
}

/// Night bloom highlights lamps, monitors, neon signs, and streetlamps (`room-update.js:163-164`).
pub fn night_bloom() -> Bloom {
    Bloom {
        intensity: 0.22,
        low_frequency_boost: 0.45,
        low_frequency_boost_curvature: 0.85,
        high_pass_frequency: 1.0,
        prefilter: BloomPrefilter {
            threshold: 0.85,
            threshold_softness: 0.25,
        },
        composite_mode: BloomCompositeMode::Additive,
        ..default()
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub enum CameraPreset {
    #[default]
    Home,
    Desks,
    Meeting,
    Server,
}

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
    pub base_viewport_height: f32,
    pub min_height: f32,
    pub max_height: f32,
    pub active_preset: CameraPreset,
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
            base_viewport_height: spec.viewport_height,
            min_height: 5.5,
            max_height: if spec.layout.is_hq() { 38.0 } else { 28.0 },
            active_preset: CameraPreset::Home,
            first_person: false,
            fp_pos: Vec3::new(spec.home.x, 1.65, spec.home.z + 3.0),
            fp_yaw: 0.0,
        }
    }

    pub fn zoom_pct(&self) -> u32 {
        if self.viewport_height <= 0.0 || self.base_viewport_height <= 0.0 {
            return 100;
        }
        ((self.base_viewport_height / self.viewport_height) * 100.0)
            .round()
            .clamp(25.0, 500.0) as u32
    }

    /// Applies an office camera preset matching `my-station/src/office/ui/camera-bar.js:25-27`.
    pub fn apply_preset(&mut self, preset: CameraPreset, layout: OfficeLayout) {
        let (target, zoom_factor) = match (layout, preset) {
            (OfficeLayout::Hq, CameraPreset::Home) => (Vec3::new(3.0, 0.3, 9.0), 1.0_f32),
            (OfficeLayout::Hq, CameraPreset::Desks) => (Vec3::new(3.0, 0.6, 18.2), 1.6),
            (OfficeLayout::Hq, CameraPreset::Meeting) => (Vec3::new(-1.2, 0.6, -5.4), 2.8),
            (OfficeLayout::Hq, CameraPreset::Server) => (Vec3::new(-5.6, 1.0, -0.6), 3.3),
            (OfficeLayout::Pro, CameraPreset::Home) => (Vec3::new(3.0, 0.3, 0.7), 1.0),
            (OfficeLayout::Pro, CameraPreset::Desks) => (Vec3::new(2.3, 0.6, 3.6), 1.9),
            (OfficeLayout::Pro, CameraPreset::Meeting) => (Vec3::new(-1.2, 0.6, -5.4), 2.0),
            (OfficeLayout::Pro, CameraPreset::Server) => (Vec3::new(-5.6, 1.0, -0.6), 2.4),
            (OfficeLayout::Cozy, CameraPreset::Home) => (Vec3::new(0.0, 0.3, 0.2), 1.0),
            (OfficeLayout::Cozy, CameraPreset::Desks) => (Vec3::new(0.0, 0.6, 0.4), 1.75),
            (OfficeLayout::Cozy, CameraPreset::Meeting) => (Vec3::new(5.2, 0.6, 2.4), 2.3),
            (OfficeLayout::Cozy, CameraPreset::Server) => (Vec3::new(5.4, 1.0, -4.4), 2.2),
        };
        self.target = target;
        self.viewport_height = (self.base_viewport_height / zoom_factor)
            .clamp(self.min_height, self.max_height);
        self.first_person = false;
        self.active_preset = preset;
    }

    /// Zooms by `factor` (`> 1.0` zooms in, `< 1.0` zooms out), clamped inside `[min_height, max_height]`.
    pub fn zoom_by(&mut self, factor: f32) {
        if !factor.is_finite() || factor <= 0.0 {
            return;
        }
        self.viewport_height =
            (self.viewport_height / factor).clamp(self.min_height, self.max_height);
    }

    pub fn reset_zoom(&mut self) {
        self.viewport_height = self
            .base_viewport_height
            .clamp(self.min_height, self.max_height);
    }

    pub fn projection(&self) -> Projection {
        if self.first_person {
            Projection::Perspective(PerspectiveProjection {
                fov: 70.0_f32.to_radians(),
                near: 0.05,
                far: 240.0,
                ..default()
            })
        } else {
            Projection::Orthographic(OrthographicProjection {
                scaling_mode: ScalingMode::FixedVertical {
                    viewport_height: self.viewport_height,
                },
                near: -80.0,
                far: 180.0,
                ..OrthographicProjection::default_3d()
            })
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
    let proj = rig.projection();
    commands.insert_resource(rig);
    commands.spawn((
        WorldEntity,
        MainOfficeCamera,
        Camera {
            hdr: true,
            ..default()
        },
        Camera3d::default(),
        proj,
        Msaa::Sample4,
        Tonemapping::AcesFitted,
        Exposure { ev100: DAY_EV100 },
        day_bloom(),
        tf,
    ));
}

pub fn spawn_office_camera_with_ibl(
    commands: &mut Commands,
    spec: &LayoutSpec,
    ibl: &OfficeIbl,
    night: bool,
) {
    let rig = CameraRig::from_layout(spec);
    let tf = rig.compute_transform();
    let proj = rig.projection();
    let env_map = EnvironmentMapLight {
        diffuse_map: if night {
            ibl.night_diffuse.clone()
        } else {
            ibl.day_diffuse.clone()
        },
        specular_map: if night {
            ibl.night_specular.clone()
        } else {
            ibl.day_specular.clone()
        },
        intensity: if night {
            NIGHT_IBL_INTENSITY
        } else {
            DAY_IBL_INTENSITY
        },
        ..default()
    };
    commands.insert_resource(rig);
    commands.spawn((
        WorldEntity,
        MainOfficeCamera,
        Camera {
            hdr: true,
            ..default()
        },
        Camera3d::default(),
        proj,
        Msaa::Sample4,
        Tonemapping::AcesFitted,
        Exposure {
            ev100: if night { NIGHT_EV100 } else { DAY_EV100 },
        },
        env_map,
        if night { night_bloom() } else { day_bloom() },
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
    mut fp: Option<ResMut<FirstPersonController>>,
    street_nav: Option<Res<crate::street::CityStreetNav>>,
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

    // Sync `CameraRig::first_person` with `FirstPersonController::active`
    let was_fp = rig.first_person;
    if !block_keyboard && keys.just_pressed(KeyCode::KeyF) {
        rig.first_person = !rig.first_person;
        *proj = rig.projection();
    } else if !block_keyboard && rig.first_person && keys.just_pressed(KeyCode::Escape) {
        // When in a mini-activity (billiards / tv-games / seated), Esc exits the activity first;
        // otherwise Esc exits first-person mode cleanly.
        let in_activity = fp
            .as_deref()
            .is_some_and(|c| !matches!(c.activity, FirstPersonActivity::Walking));
        if !in_activity {
            rig.first_person = false;
            *proj = rig.projection();
        }
    }
    if let Some(c) = fp.as_deref_mut() {
        c.active = rig.first_person;
        if !c.active && was_fp {
            c.activity = FirstPersonActivity::Walking;
            c.near_prompt = None;
            c.fade = FpFadeState::Idle;
            c.fade_alpha = 0.0;
        }
    }

    let mut delta = Vec2::ZERO;
    for ev in motion_ev.read() {
        delta += ev.delta;
    }

    if rig.first_person {
        let dt = time.delta_secs().clamp(0.0, 0.1);
        if let Some(mut c) = fp {
            if !block_keyboard && keys.just_pressed(KeyCode::KeyV) {
                c.third_person = !c.third_person;
            }
            if !block_pointer
                && (buttons.pressed(MouseButton::Right) || buttons.pressed(MouseButton::Left))
            {
                c.yaw -= delta.x * 0.0042;
                c.pitch = (c.pitch - delta.y * 0.0042).clamp(-1.05, 0.85);
            }
            if !block_keyboard && matches!(c.activity, FirstPersonActivity::Walking) {
                let turn = (keys.pressed(KeyCode::ArrowLeft) as i32
                    - keys.pressed(KeyCode::ArrowRight) as i32) as f32;
                c.yaw += turn * dt * 2.2;
            }

            // Walking movement with 10 cm ObstacleGrid or CityStreetNav slide collision
            let mut f = 0.0_f32;
            let mut s = 0.0_f32;
            if !block_keyboard {
                f = (keys.pressed(KeyCode::KeyW) || keys.pressed(KeyCode::ArrowUp)) as i32 as f32
                    - (keys.pressed(KeyCode::KeyS) || keys.pressed(KeyCode::ArrowDown)) as i32 as f32;
                s = (keys.pressed(KeyCode::KeyD) as i32 - keys.pressed(KeyCode::KeyA) as i32) as f32;
            }
            if matches!(c.activity, FirstPersonActivity::SeatedShop { .. }) && (f.abs() > 0.1 || s.abs() > 0.1) {
                c.activity = FirstPersonActivity::Walking;
            }
            if matches!(c.activity, FirstPersonActivity::Walking)
                && matches!(c.fade, FpFadeState::Idle)
            {
                let mut mv = Vec2::new(s, f);
                if mv.length_squared() > 1.0 {
                    mv = mv.normalize();
                }
                let sprint = keys.pressed(KeyCode::ShiftLeft) || keys.pressed(KeyCode::ShiftRight);
                let speed = if sprint { 3.6 } else { 2.2 };
                let fwd = Vec2::new(c.yaw.sin(), c.yaw.cos());
                let right = Vec2::new(-c.yaw.cos(), c.yaw.sin());
                let step_delta = (fwd * mv.y + right * mv.x) * speed * dt;
                c.moving = step_delta.length_squared() > 1e-8;
                if c.moving {
                    if c.on_street {
                        if let Some(sn) = street_nav.as_deref() {
                            let cur = c.pos;
                            let stuck = sn.blocked_at(cur.x, cur.y, FP_PLAYER_RADIUS);
                            let mut next = cur;
                            if stuck || !sn.blocked_at(cur.x + step_delta.x, cur.y, FP_PLAYER_RADIUS) {
                                next.x += step_delta.x;
                            }
                            if stuck || !sn.blocked_at(next.x, cur.y + step_delta.y, FP_PLAYER_RADIUS) {
                                next.y += step_delta.y;
                            }
                            c.pos = next;
                            c.ground_y = sn.ground_at(c.pos.x, c.pos.y);
                        }
                    } else {
                        c.pos = c.grid.try_move(c.pos, step_delta, FP_PLAYER_RADIUS);
                        c.ground_y = 0.0;
                    }
                }
            } else {
                c.moving = false;
            }

            c.elapsed += dt;
            let bob = if c.moving { (c.elapsed * 9.0).sin() * 0.025 } else { 0.0 };
            let seated = matches!(
                c.activity,
                FirstPersonActivity::SeatedShop { .. } | FirstPersonActivity::PlayingTvGames { .. }
            );
            let eye_y = c.ground_y + if seated { 1.18 } else { 1.52 } + bob;
            rig.fp_pos = Vec3::new(c.pos.x, eye_y, c.pos.y);
            rig.fp_yaw = c.yaw;

            // Perspective FOV narrows to 44° while playing TV games (`gaming.js:116`)
            let want_fov = if matches!(c.activity, FirstPersonActivity::PlayingTvGames { .. }) {
                44.0_f32.to_radians()
            } else {
                70.0_f32.to_radians()
            };
            match &mut *proj {
                Projection::Perspective(p) => {
                    if (p.fov - want_fov).abs() > 1e-3 {
                        p.fov = want_fov;
                    }
                }
                _ => {
                    *proj = Projection::Perspective(PerspectiveProjection {
                        fov: want_fov,
                        near: 0.05,
                        far: 240.0,
                        ..default()
                    });
                }
            }

            let dir = Vec3::new(
                c.yaw.sin() * c.pitch.cos(),
                c.pitch.sin(),
                c.yaw.cos() * c.pitch.cos(),
            );
            let show_third = c.third_person || !matches!(c.fade, FpFadeState::Idle);
            if show_third {
                let tgt = Vec3::new(c.pos.x, eye_y - 0.12, c.pos.y);
                let mut back = 3.0_f32;
                let mut d = 0.6_f32;
                while d <= 3.0 {
                    let px = tgt.x - dir.x * d;
                    let pz = tgt.z - dir.z * d;
                    let hit = if c.on_street {
                        street_nav.as_deref().is_some_and(|sn| sn.blocked_at(px, pz, 0.05))
                    } else {
                        c.grid.is_blocked_cell(px, pz)
                    };
                    if hit {
                        back = (d - 0.25).max(0.6);
                        break;
                    }
                    d += 0.2;
                }
                let cam_pos = tgt - dir * back + Vec3::Y * 0.52;
                *tf = Transform::from_translation(cam_pos).looking_at(tgt + dir * 2.0, Vec3::Y);
            } else {
                let eye = Vec3::new(c.pos.x + c.yaw.sin() * 0.12, eye_y, c.pos.y + c.yaw.cos() * 0.12);
                *tf = Transform::from_translation(eye).looking_at(eye + dir * 2.0, Vec3::Y);
            }
            wheel_ev.clear();
            return;
        }

        // Legacy fallback when `FirstPersonController` resource is absent (unit tests)
        if !block_pointer
            && (buttons.pressed(MouseButton::Right) || buttons.pressed(MouseButton::Left))
        {
            rig.fp_yaw -= delta.x * 0.004;
        }
        if !block_keyboard {
            let speed = 5.0 * dt;
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

#[derive(SystemParam)]
pub struct FpWorldContext<'w, 's> {
    ui_capture: Option<Res<'w, UiInputCapture>>,
    layout: Option<Res<'w, OfficeLayout>>,
    backdrop: Option<Res<'w, crate::backdrops::BackdropKind>>,
    street_nav: Option<Res<'w, crate::street::CityStreetNav>>,
    lift_service: Option<Res<'w, crate::life_systems::LiftServiceState>>,
    chibi_cache: Option<Res<'w, crate::chibi_meshes::ChibiMeshCache>>,
    props_q: Query<'w, 's, (&'static crate::interact::InteractiveProp, &'static GlobalTransform)>,
    workers_q: Query<'w, 's, (&'static crate::people::LiveSessionWorker, &'static GlobalTransform)>,
}

/// Drives first-person `fp-fade` elevator transitions (`Office ↔ Street`), near-prompt detection
/// (lift, walk-in shops, billiards, TV games, office props/workers), `E` interaction, billiards
/// Spacebar shot charging/release, sofa `tv-games` controls, and `"Bạn"` 3rd-person avatar sync.
#[allow(clippy::too_many_arguments, clippy::type_complexity)]
pub fn first_person_update_system(
    mut commands: Commands,
    time: Res<Time>,
    keys: Res<ButtonInput<KeyCode>>,
    ctx: FpWorldContext,
    fp: Option<ResMut<FirstPersonController>>,
    mut sfx: Option<ResMut<crate::audio::SfxEngine>>,
    mut doors_q: Query<&mut crate::entrance::ElevatorDoors>,
    mut avatar_q: Query<
        (Entity, &mut Transform, &mut Visibility, &mut crate::chibi::ChibiWorker),
        With<FirstPersonAvatar>,
    >,
    mut palette: Option<ResMut<crate::palette::WorldPalette>>,
    mut materials: Option<ResMut<Assets<StandardMaterial>>>,
    mut last_avatar_prop: Local<Option<PropKind>>,
) {
    let Some(mut fp) = fp else {
        return;
    };
    let layout = ctx.layout.as_deref().copied().unwrap_or(OfficeLayout::Pro);
    let spec = LayoutSpec::for_layout(layout);
    let dt = time.delta_secs().clamp(0.0, 0.1);
    let block_kb = ctx.ui_capture.as_ref().is_some_and(|u| u.wants_keyboard);
    let city_active = ctx.backdrop.as_deref().copied() == Some(crate::backdrops::BackdropKind::City)
        && ctx.street_nav.is_some();
    let lift_serviced = ctx
        .lift_service
        .as_deref()
        .is_some_and(|ls| ls.phase != crate::life_systems::LiftServicePhase::Idle);

    // If city backdrop is unloaded while walking on street, snap cleanly back inside the office
    if fp.on_street && !city_active {
        fp.return_to_office_now(&spec);
    }

    // Drain any queued SFX
    if let Some(eng) = sfx.as_deref_mut() {
        for k in fp.pending_sfx.drain(..) {
            eng.play_sfx(k);
        }
    } else {
        fp.pending_sfx.clear();
    }

    // Expire temporary held shop item after carry_until
    if fp.carrying.is_some() && fp.carry_until > 0.0 && fp.elapsed > fp.carry_until {
        fp.carrying = None;
        fp.carry_until = 0.0;
    }

    // 1. Step `fp-fade` elevator transition (`FadeOut -> Teleport -> FadeIn`)
    match fp.fade {
        FpFadeState::Idle => {
            fp.fade_alpha = 0.0;
        }
        FpFadeState::FadingOut {
            mut timer,
            duration,
            to_street,
        } => {
            timer += dt;
            fp.fade_alpha = (timer / duration.max(0.05)).clamp(0.0, 1.0);
            for mut d in &mut doors_q {
                d.open = (d.open + dt * 4.0).min(1.0);
            }
            if timer >= duration {
                if to_street {
                    if let Some(sn) = ctx.street_nav.as_deref() {
                        fp.on_street = true;
                        fp.pos = sn.lobby_exit + sn.lobby_normal * 0.35;
                        fp.ground_y = sn.ground_at(fp.pos.x, fp.pos.y);
                        fp.yaw = sn.lobby_normal.x.atan2(sn.lobby_normal.y);
                        fp.pitch = -0.04;
                        fp.activity = FirstPersonActivity::Walking;
                        fp.near_prompt = None;
                    }
                } else {
                    fp.on_street = false;
                    fp.pos = Vec2::new(if layout.is_pro() { -6.15 } else { -6.35 }, spec.door_z);
                    fp.ground_y = 0.0;
                    fp.yaw = std::f32::consts::FRAC_PI_2;
                    fp.pitch = -0.06;
                    fp.activity = FirstPersonActivity::Walking;
                    fp.near_prompt = None;
                }
                fp.fade = FpFadeState::FadingIn {
                    timer: 0.0,
                    duration: 0.22,
                };
                fp.fade_alpha = 1.0;
            } else {
                fp.fade = FpFadeState::FadingOut {
                    timer,
                    duration,
                    to_street,
                };
            }
        }
        FpFadeState::FadingIn {
            mut timer,
            duration,
        } => {
            timer += dt;
            fp.fade_alpha = (1.0 - timer / duration.max(0.05)).clamp(0.0, 1.0);
            if timer >= duration {
                fp.fade = FpFadeState::Idle;
                fp.fade_alpha = 0.0;
                for mut d in &mut doors_q {
                    d.open = 0.0;
                }
            } else {
                fp.fade = FpFadeState::FadingIn { timer, duration };
            }
        }
    }

    // 2. Step active mini-activities (Billiards Spacebar power bar & TV-Games sofa controls)
    if fp.active && !block_kb {
        match &mut fp.activity {
            FirstPersonActivity::PlayingBilliards {
                aim_yaw,
                power,
                charge_dir,
                charging,
                shots,
                ..
            } => {
                if keys.just_pressed(KeyCode::Escape) {
                    fp.activity = FirstPersonActivity::Walking;
                    fp.pending_toasts.push(FpToastNotice {
                        i18n_key: "fp.bil.up",
                        vars: Vec::new(),
                    });
                } else {
                    let fine = keys.pressed(KeyCode::ShiftLeft) || keys.pressed(KeyCode::ShiftRight);
                    let rate = if fine { 0.45 } else { 1.45 };
                    if keys.pressed(KeyCode::ArrowLeft) || keys.pressed(KeyCode::KeyA) {
                        *aim_yaw += rate * dt;
                    }
                    if keys.pressed(KeyCode::ArrowRight) || keys.pressed(KeyCode::KeyD) {
                        *aim_yaw -= rate * dt;
                    }
                    if keys.pressed(KeyCode::Space) {
                        *charging = true;
                        *power += *charge_dir * dt * 1.15;
                        if *power >= 1.0 {
                            *power = 1.0;
                            *charge_dir = -1.0;
                        } else if *power <= 0.12 {
                            *power = 0.12;
                            *charge_dir = 1.0;
                        }
                    } else if *charging {
                        let shot_power = (*power).clamp(0.12, 1.0);
                        let shot_dir = Vec2::new(aim_yaw.sin(), aim_yaw.cos());
                        *charging = false;
                        *power = 0.0;
                        *charge_dir = 1.0;
                        *shots += 1;
                        fp.pending_billiards_shot = Some((shot_dir, shot_power));
                        fp.pending_sfx.push(crate::audio::SfxKind::Pock);
                    }
                }
            }
            FirstPersonActivity::PlayingTvGames { game_index, .. } => {
                if keys.just_pressed(KeyCode::Escape) || keys.just_pressed(KeyCode::KeyE) {
                    fp.activity = FirstPersonActivity::Walking;
                    fp.pending_toasts.push(FpToastNotice {
                        i18n_key: "fp.tv.up",
                        vars: Vec::new(),
                    });
                } else if keys.just_pressed(KeyCode::ArrowLeft) || keys.just_pressed(KeyCode::KeyA) {
                    *game_index = (*game_index + 2) % 3;
                    fp.pending_tv_game_index = Some(*game_index);
                    fp.pending_sfx.push(crate::audio::SfxKind::Click);
                } else if keys.just_pressed(KeyCode::ArrowRight)
                    || keys.just_pressed(KeyCode::KeyD)
                    || keys.just_pressed(KeyCode::Space)
                {
                    *game_index = (*game_index + 1) % 3;
                    fp.pending_tv_game_index = Some(*game_index);
                    fp.pending_sfx.push(crate::audio::SfxKind::Beep);
                }
            }
            FirstPersonActivity::SeatedShop { .. } => {
                if keys.just_pressed(KeyCode::Escape) {
                    fp.activity = FirstPersonActivity::Walking;
                }
            }
            FirstPersonActivity::Walking => {}
        }
    }

    // 3. Near-prompt detection (`E` contextual prompt) when actively walking
    if !fp.active || !matches!(fp.fade, FpFadeState::Idle) {
        fp.near_prompt = None;
    } else if matches!(fp.activity, FirstPersonActivity::Walking) {
        let mut best: Option<(f32, FirstPersonPrompt)> = None;
        let consider = |best: &mut Option<(f32, FirstPersonPrompt)>, d: f32, p: FirstPersonPrompt| {
            if best.as_ref().is_none_or(|(bd, _)| d < *bd) {
                *best = Some((d, p));
            }
        };

        if fp.on_street {
            if let Some(sn) = ctx.street_nav.as_deref() {
                let d_exit = fp.pos.distance(sn.lobby_exit);
                if d_exit < 2.4 {
                    consider(&mut best, d_exit, FirstPersonPrompt::LiftToOffice);
                }
                for spot in &sn.shop_spots {
                    let d = fp.pos.distance(spot.pos);
                    if d < 2.2 {
                        let prompt = match spot.kind {
                            crate::street::shops_walkin::ShopSpotKind::Take {
                                item_id,
                                price_k,
                                carry,
                            } => FirstPersonPrompt::ShopTake {
                                chain: spot.chain,
                                shop_name: spot.shop_name,
                                item_id,
                                price_k,
                                carry,
                            },
                            crate::street::shops_walkin::ShopSpotKind::Pay {
                                default_item_id,
                                default_price_k,
                                default_carry,
                            } => FirstPersonPrompt::ShopPay {
                                chain: spot.chain,
                                shop_name: spot.shop_name,
                                default_item_id,
                                default_price_k,
                                default_carry,
                                basket_count: fp.basket.len(),
                                basket_total_k: fp.basket.iter().map(|i| i.price_k).sum(),
                            },
                            crate::street::shops_walkin::ShopSpotKind::Sit {
                                seat_yaw,
                                is_stool,
                            } => FirstPersonPrompt::ShopSit {
                                shop_name: spot.shop_name,
                                seat_pos: spot.pos,
                                seat_yaw,
                                is_stool,
                            },
                        };
                        consider(&mut best, d, prompt);
                    }
                }
            }
        } else {
            // Inside office: check Lift / Entrance door -> Street (when City backdrop is active)
            if city_active {
                let lift_pos = Vec2::new(if layout.is_pro() { -6.95 } else { -7.65 }, spec.door_z);
                let d_lift = fp.pos.distance(lift_pos);
                if d_lift < 2.2 {
                    consider(
                        &mut best,
                        d_lift,
                        FirstPersonPrompt::LiftToStreet {
                            stairs: lift_serviced,
                        },
                    );
                }
            }
            // Billiards table (`spec.bil`)
            if let Some([bx, bz]) = spec.bil {
                let d_bil = fp.pos.distance(Vec2::new(bx, bz));
                if d_bil < 2.3 {
                    consider(&mut best, d_bil, FirstPersonPrompt::BilliardsPlay);
                }
            }
            // Lounge Game TV / Sofa (`spec.game_tv`)
            if let Some([gx, gz]) = spec.game_tv {
                let d_tv = fp.pos.distance(Vec2::new(gx, gz));
                let d_sofa = fp.pos.distance(Vec2::new(spec.sofa[0], spec.sofa[1]));
                let d = d_tv.min(d_sofa);
                if d < 2.4 {
                    consider(&mut best, d, FirstPersonPrompt::TvGamesPlay);
                }
            }
            // Live workers
            for (w, gtf) in &ctx.workers_q {
                let wp = Vec2::new(gtf.translation().x, gtf.translation().z);
                let d = fp.pos.distance(wp);
                if d < 2.2 {
                    consider(
                        &mut best,
                        d,
                        FirstPersonPrompt::OfficeWorker {
                            session_id: w.session_id.clone(),
                            name: w.name.clone(),
                            asleep: w.status == "rest",
                        },
                    );
                }
            }
            // Interactive office props
            for (prop, gtf) in &ctx.props_q {
                let pp = Vec2::new(gtf.translation().x, gtf.translation().z);
                let d = fp.pos.distance(pp);
                if d < 2.1 {
                    consider(
                        &mut best,
                        d,
                        FirstPersonPrompt::OfficeProp {
                            id: prop.id,
                            place: prop.place,
                        },
                    );
                }
            }
        }
        fp.near_prompt = best.map(|(_, p)| p);
    } else {
        fp.near_prompt = None;
    }

    // 4. Execute `E` interaction (keyboard `KeyE` or UI `fp-near` click via `interact_requested`)
    let pressed_e = fp.active
        && ((!block_kb && keys.just_pressed(KeyCode::KeyE)) || fp.interact_requested);
    fp.interact_requested = false;

    if pressed_e && matches!(fp.activity, FirstPersonActivity::Walking) {
        if let Some(prompt) = fp.near_prompt.clone() {
            match prompt {
                FirstPersonPrompt::LiftToStreet { stairs } => {
                    fp.start_fade_transition(true, stairs);
                    fp.fade_alpha = 0.08;
                    for mut d in &mut doors_q {
                        d.open = d.open.max(0.25);
                    }
                }
                FirstPersonPrompt::LiftToOffice => {
                    fp.start_fade_transition(false, lift_serviced);
                    fp.fade_alpha = 0.08;
                    for mut d in &mut doors_q {
                        d.open = d.open.max(0.25);
                    }
                }
                FirstPersonPrompt::ShopTake {
                    item_id,
                    price_k,
                    carry,
                    ..
                } => {
                    fp.basket.push(ShopBasketItem {
                        item_id,
                        price_k,
                        carry,
                    });
                    fp.pending_sfx.push(crate::audio::SfxKind::Pop);
                    fp.pending_toasts.push(FpToastNotice {
                        i18n_key: "fp.added",
                        vars: vec![("item", format!("fp.items.{item_id}"))],
                    });
                }
                FirstPersonPrompt::ShopPay {
                    shop_name,
                    default_item_id,
                    default_price_k,
                    default_carry,
                    ..
                } => {
                    if fp.basket.is_empty() {
                        fp.basket.push(ShopBasketItem {
                            item_id: default_item_id,
                            price_k: default_price_k,
                            carry: default_carry,
                        });
                    }
                    let total_k: u32 = fp.basket.iter().map(|i| i.price_k).sum();
                    let last_carry = fp.basket.last().map(|i| i.carry).unwrap_or(default_carry);
                    fp.basket.clear();
                    fp.carrying = Some(last_carry);
                    fp.carry_until = fp.elapsed + 120.0;
                    fp.pending_sfx.push(crate::audio::SfxKind::Beep);
                    fp.pending_toasts.push(FpToastNotice {
                        i18n_key: "fp.paid",
                        vars: vec![
                            ("total", FirstPersonController::format_vnd(total_k)),
                            ("shop", shop_name.to_owned()),
                        ],
                    });
                }
                FirstPersonPrompt::ShopSit {
                    shop_name,
                    seat_pos,
                    seat_yaw,
                    is_stool,
                } => {
                    fp.pos = seat_pos;
                    fp.yaw = seat_yaw;
                    fp.pitch = -0.05;
                    fp.activity = FirstPersonActivity::SeatedShop {
                        shop_name,
                        seat_pos,
                        seat_yaw,
                    };
                    fp.pending_sfx.push(crate::audio::SfxKind::Click);
                    fp.pending_toasts.push(FpToastNotice {
                        i18n_key: if is_stool { "fp.sitStoolDone" } else { "fp.sitDone" },
                        vars: Vec::new(),
                    });
                }
                FirstPersonPrompt::BilliardsPlay => {
                    let aim_yaw = if let Some([bx, bz]) = spec.bil {
                        (bx - fp.pos.x).atan2(bz - fp.pos.y)
                    } else {
                        fp.yaw
                    };
                    fp.yaw = aim_yaw;
                    fp.pitch = -0.22;
                    fp.activity = FirstPersonActivity::PlayingBilliards {
                        aim_yaw,
                        power: 0.0,
                        charge_dir: 1.0,
                        charging: false,
                        shots: 0,
                        potted: 0,
                    };
                    fp.pending_sfx.push(crate::audio::SfxKind::Pock);
                    fp.pending_toasts.push(FpToastNotice {
                        i18n_key: "fp.bil.alone",
                        vars: Vec::new(),
                    });
                }
                FirstPersonPrompt::TvGamesPlay => {
                    let seat_pos = Vec2::new(spec.sofa[0], spec.sofa[1]);
                    let seat_yaw = if let Some([gx, gz]) = spec.game_tv {
                        (gx - seat_pos.x).atan2(gz - seat_pos.y)
                    } else {
                        spec.sofa[3]
                    };
                    fp.pos = seat_pos;
                    fp.yaw = seat_yaw;
                    fp.pitch = -0.18;
                    fp.activity = FirstPersonActivity::PlayingTvGames {
                        game_index: 0,
                        seat_pos,
                        seat_yaw,
                    };
                    fp.pending_tv_game_index = Some(0);
                    fp.pending_sfx.push(crate::audio::SfxKind::Beep);
                    fp.pending_toasts.push(FpToastNotice {
                        i18n_key: "fp.tv.sat",
                        vars: Vec::new(),
                    });
                }
                FirstPersonPrompt::OfficeProp { id, .. } => {
                    let kind = match id {
                        "cat" => crate::audio::SfxKind::Meow,
                        "arcade" => crate::audio::SfxKind::Coin,
                        "aquarium" => crate::audio::SfxKind::Bloop,
                        _ => crate::audio::SfxKind::Pop,
                    };
                    fp.pending_sfx.push(kind);
                }
                FirstPersonPrompt::OfficeWorker { .. } => {
                    fp.pending_sfx.push(crate::audio::SfxKind::Pop);
                }
            }
        }
    }

    // 5. Sync visible `"Bạn"` 3rd-person avatar position, pose, visibility & lazy held prop
    let desired_prop = match fp.activity {
        FirstPersonActivity::PlayingBilliards { .. } => Some(PropKind::Cue),
        FirstPersonActivity::PlayingTvGames { .. } => Some(PropKind::Gamepad),
        _ => fp.carrying,
    };
    let show_avatar = fp.active
        && (fp.third_person
            || !matches!(fp.fade, FpFadeState::Idle)
            || !matches!(fp.activity, FirstPersonActivity::Walking));
    for (ent, mut tf, mut vis, mut worker) in &mut avatar_q {
        tf.translation = Vec3::new(fp.pos.x, fp.ground_y, fp.pos.y);
        tf.rotation = Quat::from_rotation_y(fp.yaw);
        *vis = if show_avatar {
            Visibility::Inherited
        } else {
            Visibility::Hidden
        };
        worker.anim = match fp.activity {
            FirstPersonActivity::PlayingTvGames { .. } => crate::chibi::WorkerAnim::Work,
            FirstPersonActivity::PlayingBilliards { .. } => crate::chibi::WorkerAnim::Wait,
            FirstPersonActivity::SeatedShop { .. } => crate::chibi::WorkerAnim::Rest,
            FirstPersonActivity::Walking => {
                if fp.moving {
                    crate::chibi::WorkerAnim::Walk
                } else {
                    crate::chibi::WorkerAnim::Rest
                }
            }
        };
        if *last_avatar_prop != desired_prop {
            *last_avatar_prop = desired_prop;
            if let (Some(cache), Some(pal), Some(mats)) = (
                ctx.chibi_cache.as_deref(),
                palette.as_deref_mut(),
                materials.as_deref_mut(),
            ) {
                crate::chibi_props::set_active_prop(
                    &mut commands,
                    cache,
                    pal,
                    mats,
                    ent,
                    desired_prop,
                );
            }
        }
    }
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

        // 4. Esc exits first-person mode cleanly.
        app.world_mut()
            .resource_mut::<ButtonInput<KeyCode>>()
            .reset_all();
        app.world_mut()
            .resource_mut::<ButtonInput<KeyCode>>()
            .press(KeyCode::Escape);
        app.update();
        assert!(!app.world().resource::<CameraRig>().first_person);
    }

    #[test]
    fn camera_presets_and_zoom_update_target_and_percentage_cleanly() {
        let spec = LayoutSpec::for_layout(OfficeLayout::Pro);
        let mut rig = CameraRig::from_layout(&spec);
        assert_eq!(rig.zoom_pct(), 100);
        assert_eq!(rig.active_preset, CameraPreset::Home);

        rig.first_person = true;
        rig.apply_preset(CameraPreset::Server, OfficeLayout::Pro);
        assert!(!rig.first_person);
        assert_eq!(rig.active_preset, CameraPreset::Server);
        assert!((rig.target - Vec3::new(-5.6, 1.0, -0.6)).length() < 1e-4);
        assert!(rig.zoom_pct() >= 200);

        rig.apply_preset(CameraPreset::Home, OfficeLayout::Pro);
        assert_eq!(rig.zoom_pct(), 100);

        rig.zoom_by(1.2);
        assert!(rig.zoom_pct() > 110);
        rig.reset_zoom();
        assert_eq!(rig.zoom_pct(), 100);
        rig.zoom_by(0.8);
        assert!(rig.zoom_pct() < 90);
    }

    #[test]
    fn obstacle_grid_10cm_blocks_interior_walls_and_desks_and_allows_doorways_across_all_layouts() {
        // Pro layout: glass partition at z = -3.0 (Director/Main wall) must block crossing at x = -5.5,
        // while the open Director doorway gap at x = -3.1, z = -3.0 and lobby double doors at (-3.6, 3.3) must pass!
        let pro = ObstacleGrid::for_layout(OfficeLayout::Pro);
        assert!(
            !pro.can_stand(-5.5, -3.0, FP_PLAYER_RADIUS),
            "Pro interior glass partition at (-5.5, -3.0) must block player"
        );
        let moved_wall = pro.try_move(Vec2::new(-5.5, -2.5), Vec2::new(0.0, -1.2), FP_PLAYER_RADIUS);
        assert!(
            moved_wall.y > -3.0,
            "try_move must not cross Pro interior wall at z = -3.0 (got {moved_wall:?})"
        );
        // Diagonal slide along wall: X moves freely while Z stops at wall
        let slid = pro.try_move(Vec2::new(-5.5, -2.5), Vec2::new(0.45, -1.2), FP_PLAYER_RADIUS);
        assert!(slid.x > -5.2, "diagonal wall collision must slide smoothly along X");
        assert!(slid.y > -3.0, "diagonal slide must still stop before wall Z");

        // Open lobby double doors at (-3.6, 3.3) must allow standing & walking straight through
        assert!(
            pro.can_stand(-3.6, 3.3, FP_PLAYER_RADIUS),
            "Pro lobby double doorway (-3.6, 3.3) must be passable"
        );
        let through_door = pro.try_move(Vec2::new(-4.1, 3.3), Vec2::new(0.8, 0.0), FP_PLAYER_RADIUS);
        assert!(
            through_door.x > -3.4,
            "walking through open lobby double doors must succeed (got {through_door:?})"
        );

        // Desk footprint in Pro must block
        let spec_pro = LayoutSpec::for_layout(OfficeLayout::Pro);
        let d0 = spec_pro.desks[0];
        assert!(
            !pro.can_stand(d0.x, d0.z, FP_PLAYER_RADIUS),
            "Pro desk at ({}, {}) must be blocked",
            d0.x,
            d0.z
        );

        // HQ layout: wing wall at z = 9.0 blocks at x = 0.0, doorway at x = -2.4 passes
        let hq = ObstacleGrid::for_layout(OfficeLayout::Hq);
        assert!(
            !hq.can_stand(0.0, 9.0, FP_PLAYER_RADIUS),
            "HQ wing wall at (0.0, 9.0) must block player"
        );
        assert!(
            hq.can_stand(-2.4, 9.0, FP_PLAYER_RADIUS),
            "HQ wing doorway at (-2.4, 9.0) must be passable"
        );

        // Cozy layout: desk blocks, open floor passes
        let cozy = ObstacleGrid::for_layout(OfficeLayout::Cozy);
        let cozy_spec = LayoutSpec::for_layout(OfficeLayout::Cozy);
        assert!(
            !cozy.can_stand(cozy_spec.desks[0].x, cozy_spec.desks[0].z, FP_PLAYER_RADIUS),
            "Cozy desk must be blocked"
        );
        assert!(
            cozy.can_stand(-5.0, 1.5, FP_PLAYER_RADIUS),
            "Cozy open entrance walkway must be passable"
        );
    }

    #[test]
    fn first_person_elevator_office_to_street_fade_shop_pool_tv_and_zero_leak_unload() {
        use crate::backdrops::BackdropKind;
        use crate::chibi::ChibiLook;
        use crate::props_detail::leisure::{BilliardsBall, GameTv, animate_leisure_system};
        use crate::street::CityStreetNav;
        use crate::street::shops_walkin::ShopSpotKind;
        use station_canvas2d::FontEngine;
        use std::path::PathBuf;

        let assets_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../assets");
        let mut fonts = FontEngine::from_assets_dir(&assets_dir);

        let mut app = App::new();
        app.insert_resource(Time::<()>::default())
            .insert_resource(ButtonInput::<KeyCode>::default())
            .insert_resource(ButtonInput::<MouseButton>::default())
            .insert_resource(UiInputCapture::default())
            .insert_resource(Assets::<Mesh>::default())
            .insert_resource(Assets::<StandardMaterial>::default())
            .insert_resource(Assets::<Image>::default())
            .insert_resource(crate::audio::SfxEngine::new(false))
            .add_event::<MouseMotion>()
            .add_event::<MouseWheel>()
            .add_event::<crate::interact::PropActivated>()
            .add_systems(
                Update,
                (
                    first_person_update_system.before(animate_leisure_system),
                    animate_leisure_system,
                    crate::entrance::animate_elevator_doors_system.after(first_person_update_system),
                ),
            );

        app.world_mut().resource_scope(|world, mut meshes: Mut<Assets<Mesh>>| {
            world.resource_scope(|world, mut mats: Mut<Assets<StandardMaterial>>| {
                world.resource_scope(|world, mut imgs: Mut<Assets<Image>>| {
                    let mut cmds = world.commands();
                    crate::spawn_full_office_world(
                        &mut cmds,
                        &mut meshes,
                        &mut mats,
                        &mut imgs,
                        &mut fonts,
                        &assets_dir,
                        OfficeLayout::Pro,
                        BackdropKind::City,
                        ChibiLook::Pixar,
                        false,
                    );
                });
            });
        });
        app.world_mut().flush();

        // Activate FP mode near the Pro elevator (`(-6.95, 3.3)`)
        {
            let mut fp = app.world_mut().resource_mut::<FirstPersonController>();
            fp.active = true;
            fp.pos = Vec2::new(-6.35, 3.3);
        }
        app.world_mut().resource_mut::<Time>().advance_by(Duration::from_millis(50));
        app.update();

        assert!(
            matches!(
                app.world().resource::<FirstPersonController>().near_prompt,
                Some(FirstPersonPrompt::LiftToStreet { .. })
            ),
            "Standing near Pro elevator in City backdrop must offer LiftToStreet prompt"
        );

        // Trigger E -> fade out -> open ElevatorDoors -> teleport to street -> fade in
        app.world_mut().resource_mut::<FirstPersonController>().interact_requested = true;
        app.world_mut().resource_mut::<Time>().advance_by(Duration::from_millis(80));
        app.update();
        assert!(
            app.world().resource::<FirstPersonController>().fade_alpha > 0.0,
            "Lift transition must raise fp-fade alpha"
        );
        let door_open = app
            .world_mut()
            .query::<&crate::entrance::ElevatorDoors>()
            .single(app.world())
            .unwrap()
            .open;
        assert!(door_open > 0.0, "ElevatorDoors must slide open during lift ride");
        // Avatar must be visible during lift transition
        let av_vis = *app
            .world_mut()
            .query_filtered::<&Visibility, With<FirstPersonAvatar>>()
            .single(app.world())
            .unwrap();
        assert_eq!(av_vis, Visibility::Inherited, "Bạn avatar must be visible during lift transition");

        for _ in 0..8 {
            app.world_mut().resource_mut::<Time>().advance_by(Duration::from_millis(80));
            app.update();
        }
        {
            let fp = app.world().resource::<FirstPersonController>();
            assert!(fp.on_street, "Player must arrive on city street after lift fade");
            assert_eq!(fp.fade, FpFadeState::Idle);
            assert!(fp.fade_alpha < 1e-3);
        }

        // Walk inside a walk-in shop: verify Take -> Pay -> carrying prop equipped!
        let (take_pos, pay_pos) = {
            let nav = app.world().resource::<CityStreetNav>();
            let take = nav
                .shop_spots
                .iter()
                .find(|s| matches!(s.kind, ShopSpotKind::Take { .. }))
                .expect("City must have walk-in shop Take spots")
                .pos;
            let pay = nav
                .shop_spots
                .iter()
                .find(|s| matches!(s.kind, ShopSpotKind::Pay { .. }))
                .expect("City must have walk-in shop Pay spots")
                .pos;
            (take, pay)
        };
        app.world_mut().resource_mut::<FirstPersonController>().pos = take_pos;
        app.world_mut().resource_mut::<Time>().advance_by(Duration::from_millis(50));
        app.update();
        assert!(matches!(
            app.world().resource::<FirstPersonController>().near_prompt,
            Some(FirstPersonPrompt::ShopTake { .. })
        ));
        app.world_mut().resource_mut::<FirstPersonController>().interact_requested = true;
        app.update();
        assert_eq!(app.world().resource::<FirstPersonController>().basket.len(), 1);

        app.world_mut().resource_mut::<FirstPersonController>().pos = pay_pos;
        app.update();
        assert!(matches!(
            app.world().resource::<FirstPersonController>().near_prompt,
            Some(FirstPersonPrompt::ShopPay { basket_count: 1, .. })
        ));
        app.world_mut().resource_mut::<FirstPersonController>().interact_requested = true;
        app.update();
        assert!(app.world().resource::<FirstPersonController>().basket.is_empty());
        assert!(
            app.world().resource::<FirstPersonController>().carrying.is_some(),
            "Paying at walk-in shop must equip a held prop on Bạn"
        );

        // Return to office via street lobby exit
        let exit_pos = app.world().resource::<CityStreetNav>().lobby_exit;
        app.world_mut().resource_mut::<FirstPersonController>().pos = exit_pos;
        app.update();
        assert!(matches!(
            app.world().resource::<FirstPersonController>().near_prompt,
            Some(FirstPersonPrompt::LiftToOffice)
        ));
        app.world_mut().resource_mut::<FirstPersonController>().interact_requested = true;
        for _ in 0..10 {
            app.world_mut().resource_mut::<Time>().advance_by(Duration::from_millis(80));
            app.update();
        }
        assert!(
            !app.world().resource::<FirstPersonController>().on_street,
            "LiftToOffice must bring player back inside office"
        );

        // Billiards interaction: E -> hold Space -> release -> live cue ball gains velocity!
        let spec = LayoutSpec::for_layout(OfficeLayout::Pro);
        let [bx, bz] = spec.bil.unwrap();
        app.world_mut().resource_mut::<FirstPersonController>().pos = Vec2::new(bx - 1.1, bz);
        app.update();
        assert!(matches!(
            app.world().resource::<FirstPersonController>().near_prompt,
            Some(FirstPersonPrompt::BilliardsPlay)
        ));
        app.world_mut().resource_mut::<FirstPersonController>().interact_requested = true;
        app.update();
        assert!(matches!(
            app.world().resource::<FirstPersonController>().activity,
            FirstPersonActivity::PlayingBilliards { .. }
        ));
        app.world_mut().resource_mut::<ButtonInput<KeyCode>>().press(KeyCode::Space);
        for _ in 0..5 {
            app.world_mut().resource_mut::<Time>().advance_by(Duration::from_millis(80));
            app.update();
        }
        let charged = match app.world().resource::<FirstPersonController>().activity {
            FirstPersonActivity::PlayingBilliards { power, .. } => power,
            _ => 0.0,
        };
        assert!(charged > 0.25, "Holding Space must charge billiards power bar (got {charged})");
        app.world_mut().resource_mut::<ButtonInput<KeyCode>>().release(KeyCode::Space);
        app.world_mut().resource_mut::<Time>().advance_by(Duration::from_millis(16));
        app.update();
        let cue_speed = app
            .world_mut()
            .query::<&BilliardsBall>()
            .iter(app.world())
            .find(|b| b.number == 0)
            .map(|b| b.vel.length())
            .unwrap_or(0.0);
        assert!(cue_speed > 0.5, "Releasing Space must strike live cue ball (speed={cue_speed})");

        // Exit billiards with Esc, then sit on sofa to play tv-games
        app.world_mut().resource_mut::<ButtonInput<KeyCode>>().press(KeyCode::Escape);
        app.update();
        app.world_mut().resource_mut::<ButtonInput<KeyCode>>().reset_all();
        app.world_mut().resource_mut::<FirstPersonController>().pos =
            Vec2::new(spec.sofa[0], spec.sofa[1]);
        app.update();
        assert!(matches!(
            app.world().resource::<FirstPersonController>().near_prompt,
            Some(FirstPersonPrompt::TvGamesPlay)
        ));
        app.world_mut().resource_mut::<FirstPersonController>().interact_requested = true;
        app.update();
        assert!(matches!(
            app.world().resource::<FirstPersonController>().activity,
            FirstPersonActivity::PlayingTvGames { game_index: 0, .. }
        ));
        app.world_mut().resource_mut::<ButtonInput<KeyCode>>().press(KeyCode::ArrowRight);
        app.update();
        let kind_after = app
            .world_mut()
            .query::<&GameTv>()
            .single(app.world())
            .unwrap()
            .sim
            .kind();
        assert_eq!(kind_after, station_canvas2d::TvGameKind::Racing);

        // Full unload must purge Bạn avatar, held props, and ALL GPU assets with 0 leaks!
        let world_ents: Vec<Entity> = app
            .world_mut()
            .query_filtered::<Entity, With<WorldEntity>>()
            .iter(app.world())
            .collect();
        app.world_mut().resource_scope(|world, mut meshes: Mut<Assets<Mesh>>| {
            world.resource_scope(|world, mut mats: Mut<Assets<StandardMaterial>>| {
                world.resource_scope(|world, mut imgs: Mut<Assets<Image>>| {
                    let mut pal = world.remove_resource::<crate::palette::WorldPalette>();
                    let mut ba = world.remove_resource::<crate::backdrops::BackdropAssets>();
                    let ibl = world.remove_resource::<crate::ibl::OfficeIbl>();
                    let cm = world.remove_resource::<crate::chibi_meshes::ChibiMeshCache>();
                    let mut cmds = world.commands();
                    crate::unload_full_office_world(
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
        app.world_mut().flush();

        assert_eq!(app.world().resource::<Assets<Mesh>>().len(), 0, "0 leaked Meshes");
        assert_eq!(app.world().resource::<Assets<StandardMaterial>>().len(), 0, "0 leaked Materials");
        assert_eq!(app.world().resource::<Assets<Image>>().len(), 0, "0 leaked Images");
    }
}
