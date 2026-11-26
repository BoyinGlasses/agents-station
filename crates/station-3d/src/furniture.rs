use crate::desks::spawn_office_chair;
use crate::interact::InteractiveProp;
use crate::layout::LayoutSpec;
use crate::lights::{LightingState, RoomLamp};
use crate::palette::{
    WorldEntity, WorldPalette, spawn_box, spawn_box_rot, spawn_cyl, spawn_quad, spawn_sphere,
    spawn_torus_rot,
};
use crate::rooms::{PlantKind, spawn_sofa, spawn_typed_plant};
use bevy::asset::RenderAssetUsages;
use bevy::prelude::*;
use bevy::render::render_resource::{Extent3d, TextureDimension, TextureFormat};
use chrono::{Local, Timelike};
use station_canvas2d::boards_and_social::{
    WhiteboardInput, WhiteboardTask, draw_sticky_note, draw_whiteboard,
};
use station_canvas2d::office_surfaces::{
    draw_cozy_sun_poster, draw_globe_map, draw_neon_ship_it,
};
use station_canvas2d::{FontEngine, hex};
use std::f32::consts::{FRAC_PI_2, PI, TAU};

const CONFETTI_COLORS: [&str; 6] = [
    "#ED7A13", "#FFE787", "#6A8042", "#3F7CC4", "#E8A0B4", "#8C4FA8",
];
const CONFETTI_COUNT: usize = 70;

#[derive(Component)]
pub struct ServerFan {
    pub speed: f32,
}

#[derive(Component, Debug, Clone)]
pub struct ServerLedBank {
    pub material: Handle<StandardMaterial>,
    pub base_hex: &'static str,
    pub phase: f32,
    pub lit: bool,
}

#[derive(Component)]
pub struct WhiteboardSurface;

#[derive(Component, Debug, Clone)]
pub struct NeonSign {
    pub material: Handle<StandardMaterial>,
}

#[derive(Component, Debug, Clone)]
pub struct FloorLampBulb {
    pub material: Handle<StandardMaterial>,
}

#[derive(Component, Debug, Clone, Copy)]
pub struct SpinningGlobe {
    pub speed: f32,
}

#[derive(Component, Debug, Clone, Copy)]
pub struct WallClock {
    pub hour_hand: Entity,
    pub min_hand: Entity,
    pub sec_hand: Entity,
}

#[derive(Component, Debug, Clone, Copy)]
pub struct DeployRocket {
    pub home: Vec3,
    pub flight_t: f32,
    pub confetti_spawned: bool,
    pub puff_accum: f32,
    pub flame: Entity,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ParticleKind {
    Puff,
    Confetti,
}

#[derive(Component, Debug, Clone, Copy)]
pub struct RocketParticle {
    pub kind: ParticleKind,
    pub vel: Vec3,
    pub spin: Vec3,
    pub life: f32,
    pub max_life: f32,
}

#[derive(Resource, Default, Debug, Clone)]
pub struct RocketLaunchQueue {
    pub pending: bool,
}

#[derive(Resource, Debug, Clone)]
pub struct ServerRackTelemetry {
    pub cpu_load: f32,
}

impl Default for ServerRackTelemetry {
    fn default() -> Self {
        Self { cpu_load: 0.25 }
    }
}

/// Optional deterministic clock override for tests (`hour, minute, second`).
#[derive(Resource, Debug, Clone, Copy)]
pub struct ClockTimeOverride {
    pub hour: u32,
    pub minute: u32,
    pub second: f32,
}

/// Exact Oracle `room-update.js:105-106` wall clock hand angles in radians `(hour_z, min_z, sec_z)`.
/// Clockwise rotation around `+Z` is negative (`15:30:00` -> `(-105°, -180°, 0°)`).
pub fn clock_hand_angles_rad(hour: u32, minute: u32, second: f32) -> (f32, f32, f32) {
    let sec_z = -(second / 60.0) * TAU;
    let min_f = minute as f32 + second / 60.0;
    let min_z = -(min_f / 60.0) * TAU;
    let hr_f = (hour % 12) as f32 + min_f / 60.0;
    let hr_z = -(hr_f / 12.0) * TAU;
    (hr_z, min_z, sec_z)
}

/// Applies exact clock hand angles to the 3 child pivot entities of `WallClock`.
pub fn apply_clock_time(
    clock: &WallClock,
    hour: u32,
    minute: u32,
    second: f32,
    tf_q: &mut Query<&mut Transform>,
) {
    let (hr_z, min_z, sec_z) = clock_hand_angles_rad(hour, minute, second);
    if let Ok(mut tf) = tf_q.get_mut(clock.hour_hand) {
        tf.rotation = Quat::from_rotation_z(hr_z);
    }
    if let Ok(mut tf) = tf_q.get_mut(clock.min_hand) {
        tf.rotation = Quat::from_rotation_z(min_z);
    }
    if let Ok(mut tf) = tf_q.get_mut(clock.sec_hand) {
        tf.rotation = Quat::from_rotation_z(sec_z);
    }
}

struct FurnRng(u64);
impl FurnRng {
    fn next_f32(&mut self) -> f32 {
        self.0 = (self.0 * 16_807) % 2_147_483_647;
        self.0 as f32 / 2_147_483_647.0
    }
    fn range(&mut self, lo: f32, hi: f32) -> f32 {
        lo + self.next_f32() * (hi - lo)
    }
}

/// Redraws the 3D whiteboard texture in-place on its existing `Image` asset (zero leaked assets).
pub fn redraw_whiteboard_surface(
    text: &str,
    streamer: bool,
    wb_mat_handle: &Handle<StandardMaterial>,
    materials: &mut Assets<StandardMaterial>,
    images: &mut Assets<Image>,
    fonts: &mut FontEngine,
) -> bool {
    let Some(mat) = materials.get(wb_mat_handle) else {
        return false;
    };
    let Some(tex_handle) = mat.base_color_texture.clone() else {
        return false;
    };

    let mut tasks = Vec::new();
    let mut open_todos = 0usize;
    let mut total_todos = 0usize;
    for raw_line in text.lines() {
        let trimmed = raw_line.trim();
        if trimmed.is_empty() {
            continue;
        }
        let body = trimmed
            .trim_start_matches("- ")
            .trim_start_matches("* ")
            .trim();
        let done = body.starts_with("[x]") || body.starts_with("[X]");
        let clean = body
            .trim_start_matches("[x]")
            .trim_start_matches("[X]")
            .trim_start_matches("[ ]")
            .trim();
        if clean.is_empty() {
            continue;
        }
        total_todos += 1;
        if !done {
            open_todos += 1;
        }
        if tasks.len() < 4 {
            tasks.push(WhiteboardTask {
                name: format!("Note {}", tasks.len() + 1),
                color: if done { hex("#3DDC84") } else { hex("#D97757") },
                state: if done { "done".into() } else { "doing".into() },
                prompt: clean.to_owned(),
                todos_done: usize::from(done),
                todos_total: 1,
            });
        }
    }

    let surf = draw_whiteboard(
        &WhiteboardInput {
            title: "Whiteboard".into(),
            yours_title: "Yours".into(),
            streamer,
            tasks,
            open_todos,
            total_todos,
        },
        fonts,
    );
    let new_img = Image::new(
        Extent3d {
            width: surf.width.max(1),
            height: surf.height.max(1),
            depth_or_array_layers: 1,
        },
        TextureDimension::D2,
        surf.rgba,
        TextureFormat::Rgba8UnormSrgb,
        RenderAssetUsages::RENDER_WORLD,
    );
    // Always re-insert under the existing Handle<Image> id: `RenderAssetUsages::RENDER_WORLD`
    // drops the CPU copy from `Assets<Image>` after frame-1 GPU extraction, so `get_mut` would
    // return `None` at runtime while `insert` re-queues GPU upload under the same handle.
    images.insert(&tex_handle, new_img);
    true
}

/// Exact Oracle `plants-lamps.js:11-20` floor lamp with metal stem, linen shade, night bulb, & PointLight.
#[allow(clippy::too_many_arguments)]
fn spawn_floor_lamp(
    commands: &mut Commands,
    palette: &mut WorldPalette,
    materials: &mut Assets<StandardMaterial>,
    x: f32,
    z: f32,
    h: f32,
) {
    let metal = palette.mat(materials, "#2B2F36", 0.35, 0.65);
    let shade = palette.mat(materials, "#F6E3BE", 0.9, 0.0);
    let bulb_m = palette.owned_emissive(materials, "#EDE6D6", 0.4);

    spawn_cyl(commands, palette, metal.clone(), Vec3::new(x, 0.03, z), 0.48, 0.06);
    spawn_cyl(commands, palette, metal, Vec3::new(x, h * 0.5, z), 0.06, h);
    spawn_cyl(commands, palette, shade, Vec3::new(x, h + 0.1, z), 0.56, 0.38);
    let bulb_ent = spawn_sphere(commands, palette, bulb_m.clone(), Vec3::new(x, h - 0.02, z), Vec3::splat(0.22));
    commands.entity(bulb_ent).insert(FloorLampBulb { material: bulb_m });
    commands.spawn((
        WorldEntity,
        RoomLamp {
            base_intensity: 26_000.0,
        },
        PointLight {
            color: crate::palette::srgb_hex("#FFB866"),
            intensity: 0.0,
            range: 7.5,
            shadows_enabled: false,
            ..default()
        },
        Transform::from_xyz(x, h + 0.05, z),
    ));
}

/// Spawns 1:1 whiteboard, `SHIP IT` neon sign, 3-hand analog `WallClock`, Cozy framed art,
/// upgraded coffee corner, meeting table + chairs, CPU-scaled server racks + 5-blade fans +
/// blinking LEDs, open bookshelf, lounge sofa + coffee table, floor lamps, room plants,
/// spinning globe, and `DeployRocket` launch pad.
pub fn spawn_furniture(
    commands: &mut Commands,
    palette: &mut WorldPalette,
    materials: &mut Assets<StandardMaterial>,
    images: &mut Assets<Image>,
    fonts: &mut FontEngine,
    spec: &LayoutSpec,
) {
    let metal = palette.mat(materials, "#2B2F36", 0.35, 0.65);
    let wood_dark = palette.mat(materials, "#6E4B32", 0.6, 0.0);
    let wood_mid = palette.mat(materials, "#9A6E48", 0.55, 0.0);

    // 1. Whiteboard + live station-canvas2d surface + 3 sticky notes (`board.js`)
    {
        let [bx, by, bz, bry] = spec.board;
        let rot = Quat::from_rotation_y(bry);
        let frame_m = palette.mat(materials, "#9AA3AD", 0.45, 0.4);
        spawn_box_rot(commands, palette, frame_m.clone(), Vec3::new(bx, by, bz), Vec3::new(0.08, 1.75, 3.25), rot);
        spawn_box_rot(commands, palette, frame_m, Vec3::new(bx + 0.1, by - 0.86, bz + 0.6), Vec3::new(0.14, 0.05, 1.2), rot);

        let wb_surf = draw_whiteboard(
            &WhiteboardInput {
                title: "Whiteboard".into(),
                yours_title: "Yours".into(),
                streamer: false,
                tasks: vec![
                    WhiteboardTask {
                        name: "Claude".into(),
                        color: hex("#D97757"),
                        state: "doing".into(),
                        prompt: "Build native Rust Agents Station 3D office".into(),
                        todos_done: 4,
                        todos_total: 6,
                    },
                    WhiteboardTask {
                        name: "Antigravity".into(),
                        color: hex("#5E9BF0"),
                        state: "done".into(),
                        prompt: "Verify G2 procedural 2D texture parity".into(),
                        todos_done: 5,
                        todos_total: 5,
                    },
                ],
                open_todos: 2,
                total_todos: 3,
            },
            fonts,
        );
        let wb_mat = palette.textured_lit(images, materials, wb_surf, 0.35);
        let wb_ent = spawn_quad(
            commands,
            palette,
            wb_mat,
            Vec3::new(bx + 0.05, by, bz),
            Vec2::new(3.1, 1.55),
            rot * Quat::from_rotation_y(FRAC_PI_2),
        );
        commands.entity(wb_ent).insert((
            WhiteboardSurface,
            InteractiveProp::new("whiteboard", "office", "Office Whiteboard"),
        ));

        let stickies = [
            ("Ship P3 3D world", false, hex("#FFE787"), 0.25_f32, -0.47_f32),
            ("Verify RAM ≤ 300MB", true, hex("#BDE7C8"), 0.25, -0.89),
            ("Mua cà phê sữa đá", false, hex("#FFB5C2"), -0.18, -0.47),
        ];
        for (txt, done, col, sy, sz) in stickies {
            let st_surf = draw_sticky_note(txt, done, col, false, fonts);
            let st_mat = palette.textured_lit(images, materials, st_surf, 0.85);
            spawn_quad(
                commands,
                palette,
                st_mat,
                Vec3::new(bx + 0.075, by + sy, bz + sz),
                Vec2::new(0.36, 0.36),
                rot * Quat::from_rotation_y(FRAC_PI_2),
            );
        }
    }

    // 2. `SHIP IT ✦` Glowing Neon Sign (`neon.js`)
    {
        let [nx, ny, nz, nry] = spec.neon;
        let rot = Quat::from_rotation_y(nry);
        let fwd = rot * Vec3::Z;
        let backing_m = palette.mat(materials, "#1C1F26", 0.6, 0.1);
        let neon_surf = draw_neon_ship_it(fonts);
        let neon_mat = palette.textured_emissive(images, materials, neon_surf, 1.25);
        let neon_back = spawn_box_rot(
            commands,
            palette,
            backing_m,
            Vec3::new(nx, ny, nz),
            Vec3::new(2.6, 0.72, 0.05),
            rot,
        );
        commands.entity(neon_back).insert((
            NeonSign {
                material: neon_mat.clone(),
            },
            InteractiveProp::new("neon", "office", "Neon Sign"),
        ));
        spawn_quad(
            commands,
            palette,
            neon_mat,
            Vec3::new(nx, ny, nz) + fwd * 0.032,
            Vec2::new(2.5, 0.62),
            rot,
        );
    }

    // 3. Ticking 3-Hand Analog WallClock (`room-pieces.js:10-14, 60-66`)
    {
        let [cx, cy, cz, cry] = spec.clock;
        let bezel_m = palette.mat(materials, "#2B2F36", 0.45, 0.2);
        let face_m = palette.mat(materials, "#FBF8F0", 0.75, 0.0);
        let tick_m = palette.mat(materials, "#2B2F36", 0.6, 0.0);
        let sec_m = palette.mat(materials, "#E8730E", 0.4, 0.0);

        let now = Local::now();
        let (hr_z, min_z, sec_z) = clock_hand_angles_rad(now.hour(), now.minute(), now.second() as f32);

        let mut hour_hand = Entity::PLACEHOLDER;
        let mut min_hand = Entity::PLACEHOLDER;
        let mut sec_hand = Entity::PLACEHOLDER;

        let clock_root = commands
            .spawn((
                WorldEntity,
                Transform::from_xyz(cx, cy, cz).with_rotation(Quat::from_rotation_y(cry)),
                Visibility::default(),
            ))
            .with_children(|g| {
                // Dark rim bezel + cream face
                g.spawn((
                    WorldEntity,
                    Mesh3d(palette.unit_cyl.clone()),
                    MeshMaterial3d(bezel_m),
                    Transform::IDENTITY
                        .with_rotation(Quat::from_rotation_x(FRAC_PI_2))
                        .with_scale(Vec3::new(0.84, 0.07, 0.84)),
                ));
                g.spawn((
                    WorldEntity,
                    Mesh3d(palette.unit_cyl.clone()),
                    MeshMaterial3d(face_m),
                    Transform::from_xyz(0.0, 0.0, 0.036)
                        .with_rotation(Quat::from_rotation_x(FRAC_PI_2))
                        .with_scale(Vec3::new(0.70, 0.01, 0.70)),
                ));
                // 12 hour markers
                for i in 0..12 {
                    let a = (i as f32 / 12.0) * TAU;
                    let d = if i % 3 == 0 { 0.048 } else { 0.028 };
                    g.spawn((
                        WorldEntity,
                        Mesh3d(palette.unit_cyl.clone()),
                        MeshMaterial3d(tick_m.clone()),
                        Transform::from_xyz(a.sin() * 0.29, a.cos() * 0.29, 0.044)
                            .with_rotation(Quat::from_rotation_x(FRAC_PI_2))
                            .with_scale(Vec3::new(d, 0.006, d)),
                    ));
                }
                // Center cap pin
                g.spawn((
                    WorldEntity,
                    Mesh3d(palette.unit_cyl.clone()),
                    MeshMaterial3d(sec_m.clone()),
                    Transform::from_xyz(0.0, 0.0, 0.056)
                        .with_rotation(Quat::from_rotation_x(FRAC_PI_2))
                        .with_scale(Vec3::new(0.044, 0.012, 0.044)),
                ));
                // 3 hand pivot children pivoted at clock center (`room-pieces.js:11, 66`)
                hour_hand = g
                    .spawn((
                        WorldEntity,
                        Transform::from_xyz(0.0, 0.0, 0.047)
                            .with_rotation(Quat::from_rotation_z(hr_z)),
                        Visibility::default(),
                    ))
                    .with_children(|h| {
                        h.spawn((
                            WorldEntity,
                            Mesh3d(palette.unit_box.clone()),
                            MeshMaterial3d(tick_m.clone()),
                            Transform::from_xyz(0.0, 0.07, 0.0)
                                .with_scale(Vec3::new(0.045, 0.20, 0.004)),
                        ));
                    })
                    .id();
                min_hand = g
                    .spawn((
                        WorldEntity,
                        Transform::from_xyz(0.0, 0.0, 0.050)
                            .with_rotation(Quat::from_rotation_z(min_z)),
                        Visibility::default(),
                    ))
                    .with_children(|h| {
                        h.spawn((
                            WorldEntity,
                            Mesh3d(palette.unit_box.clone()),
                            MeshMaterial3d(tick_m),
                            Transform::from_xyz(0.0, 0.115, 0.0)
                                .with_scale(Vec3::new(0.030, 0.29, 0.004)),
                        ));
                    })
                    .id();
                sec_hand = g
                    .spawn((
                        WorldEntity,
                        Transform::from_xyz(0.0, 0.0, 0.053)
                            .with_rotation(Quat::from_rotation_z(sec_z)),
                        Visibility::default(),
                    ))
                    .with_children(|h| {
                        h.spawn((
                            WorldEntity,
                            Mesh3d(palette.unit_box.clone()),
                            MeshMaterial3d(sec_m),
                            Transform::from_xyz(0.0, 0.125, 0.0)
                                .with_scale(Vec3::new(0.012, 0.31, 0.004)),
                        ));
                    })
                    .id();
            })
            .id();

        commands.entity(clock_root).insert(WallClock {
            hour_hand,
            min_hand,
            sec_hand,
        });
    }

    // 4. Framed Sun Poster & Above-Door Shelf on Cozy (`room-pieces.js:68-82`)
    if !spec.layout.is_pro() {
        let frame_m = palette.mat(materials, "#3B2C20", 0.6, 0.0);
        spawn_box(commands, palette, frame_m, Vec3::new(-7.95, 2.45, 4.3), Vec3::new(0.06, 1.66, 1.36));
        let poster_mat = palette.textured_lit(images, materials, draw_cozy_sun_poster(fonts), 0.9);
        spawn_quad(
            commands,
            palette,
            poster_mat,
            Vec3::new(-7.91, 2.45, 4.3),
            Vec2::new(1.2, 1.5),
            Quat::from_rotation_y(FRAC_PI_2),
        );
        // Shelf with plants above the Cozy entrance door
        spawn_box(commands, palette, wood_mid.clone(), Vec3::new(-7.82, 3.05, 1.5), Vec3::new(0.34, 0.05, 1.5));
        spawn_typed_plant(commands, palette, materials, -7.82, 3.08, 1.1, 0.35, PlantKind::Bush);
        spawn_typed_plant(commands, palette, materials, -7.82, 3.08, 1.9, 0.35, PlantKind::Snake);
    }

    // 5. Coffee corner (`coffee.js`: cabinet + 3 doors + handles, espresso, portafilter, mugs, torus donuts, snake plant)
    {
        let [cx, cz, cry] = spec.coffee;
        let rot = Quat::from_rotation_y(cry);
        let at = |lx: f32, ly: f32, lz: f32| Vec3::new(cx, 0.0, cz) + rot * Vec3::new(lx, ly, lz);
        let cab_col = if spec.layout.is_pro() { "#E6E1D8" } else { "#3F5A46" };
        let door_col = if spec.layout.is_pro() { "#D9D2C6" } else { "#47654F" };
        let top_col = if spec.layout.is_pro() { "#3B3F46" } else { "#ECE6DA" };
        let cab_m = palette.mat(materials, cab_col, 0.7, 0.0);
        let door_m = palette.mat(materials, door_col, 0.7, 0.0);
        let top_m = palette.mat(materials, top_col, 0.35, 0.05);
        let mach_m = palette.mat(materials, "#24282F", 0.35, 0.4);
        let led_m = palette.emissive(materials, "#FF6B4A", 1.5);
        let white_m = palette.mat(materials, "#FFFFFF", 0.35, 0.0);

        spawn_box_rot(commands, palette, cab_m, at(0.0, 0.46, 0.0), Vec3::new(2.9, 0.92, 0.75), rot);
        for dx in [-1.0_f32, 0.0, 1.0] {
            spawn_box_rot(commands, palette, door_m.clone(), at(dx, 0.47, 0.38), Vec3::new(0.9, 0.7, 0.02), rot);
            spawn_box_rot(commands, palette, metal.clone(), at(dx, 0.7, 0.4), Vec3::new(0.2, 0.03, 0.03), rot);
        }
        spawn_box_rot(commands, palette, top_m, at(0.0, 0.955, 0.0), Vec3::new(3.0, 0.07, 0.84), rot);
        spawn_box_rot(commands, palette, mach_m, at(-0.85, 1.3, 0.08), Vec3::new(0.52, 0.62, 0.46), rot);
        spawn_box_rot(commands, palette, metal.clone(), at(-0.85, 1.17, -0.2), Vec3::new(0.3, 0.05, 0.14), rot);
        spawn_cyl(commands, palette, white_m.clone(), at(-0.85, 1.06, -0.18), 0.12, 0.08);
        spawn_sphere(commands, palette, led_m, at(-0.69, 1.5, -0.15), Vec3::splat(0.06));
        for (k, col) in ["#FFFFFF", "#FFE787", "#BFDDF7"].iter().enumerate() {
            let mm = palette.mat(materials, col, 0.35, 0.0);
            spawn_cyl(commands, palette, mm, at(-0.05 + k as f32 * 0.22, 1.06, 0.12), 0.125, 0.13);
        }
        spawn_cyl(commands, palette, white_m, at(0.95, 1.005, 0.0), 0.5, 0.03);
        for (k, col) in ["#E8A0B4", "#8B5A3C", "#FFD166"].iter().enumerate() {
            let dm = palette.matte(materials, col);
            spawn_torus_rot(
                commands,
                palette,
                dm,
                at(0.89 + k as f32 * 0.11, 1.05, -0.06 + (k % 2) as f32 * 0.12),
                Vec3::new(0.18, 0.14, 0.18),
                rot,
            );
        }
        let plant_p = at(1.4, 0.99, 0.05);
        spawn_typed_plant(commands, palette, materials, plant_p.x, plant_p.y, plant_p.z, 0.35, PlantKind::Snake);
    }

    // 6. Meeting table & chairs (`meeting.js`)
    {
        let [mx, mz] = spec.meet;
        let chair_m = palette.mat(materials, if spec.layout.is_pro() { "#3C4656" } else { "#4E7D5B" }, 0.95, 0.0);
        let paper_m = palette.mat(materials, "#FBFBF7", 0.85, 0.0);
        if spec.layout.is_pro() {
            let tbl_m = palette.mat(materials, "#F2EFEA", 0.4, 0.0);
            let glass_cup = palette.mat(materials, "#BFE3F2", 0.25, 0.05);
            spawn_box(commands, palette, tbl_m, Vec3::new(mx, 0.76, mz), Vec3::new(3.6, 0.07, 1.3));
            for sx in [-1.35_f32, 1.35] {
                spawn_box(commands, palette, metal.clone(), Vec3::new(mx + sx, 0.37, mz), Vec3::new(0.1, 0.72, 0.9));
                spawn_box(commands, palette, metal.clone(), Vec3::new(mx + sx, 0.015, mz), Vec3::new(0.5, 0.03, 0.95));
            }
            let phone_m = palette.mat(materials, "#2B2F36", 0.4, 0.2);
            spawn_box(commands, palette, phone_m, Vec3::new(mx, 0.82, mz), Vec3::new(0.28, 0.05, 0.28));
            for &(dx, dz) in &[(-1.0_f32, 0.4_f32), (0.3, 0.42), (1.2, -0.38), (-0.5, -0.4)] {
                spawn_box_rot(
                    commands,
                    palette,
                    paper_m.clone(),
                    Vec3::new(mx + dx, 0.802, mz + dz),
                    Vec3::new(0.3, 0.012, 0.22),
                    Quat::from_rotation_y(dx * 0.3),
                );
            }
            for &(dx, dz) in &[(-0.75_f32, 0.3_f32), (0.55, -0.32), (1.55, 0.3)] {
                spawn_cyl(commands, palette, glass_cup.clone(), Vec3::new(mx + dx, 0.86, mz + dz), 0.065, 0.12);
            }
            spawn_typed_plant(commands, palette, materials, mx - 0.9, 0.8, mz - 0.1, 0.28, PlantKind::Bush);
            let seats = [
                (-0.95_f32, -4.35_f32, PI),
                (0.2, -4.35, PI),
                (1.35, -4.35, PI),
                (-0.95, -6.85, 0.0),
                (0.2, -6.85, 0.0),
                (1.35, -6.85, 0.0),
                (-2.15, -5.6, FRAC_PI_2),
            ];
            for (sx, sz, face) in seats {
                let _ = spawn_office_chair(
                    commands,
                    palette,
                    materials,
                    Vec3::new(sx, 0.0, sz),
                    face,
                    chair_m.clone(),
                    None,
                );
            }
        } else {
            let tbl_m = palette.mat(materials, "#A87B52", 0.45, 0.0);
            let mug_m = palette.mat(materials, "#7EC8E3", 0.35, 0.0);
            spawn_cyl(commands, palette, tbl_m, Vec3::new(mx, 0.76, mz), 2.04, 0.08);
            spawn_cyl(commands, palette, wood_dark.clone(), Vec3::new(mx, 0.36, mz), 0.30, 0.70);
            spawn_cyl(commands, palette, wood_dark.clone(), Vec3::new(mx, 0.03, mz), 1.05, 0.05);
            spawn_typed_plant(commands, palette, materials, mx + 0.25, 0.8, mz - 0.2, 0.32, PlantKind::Bush);
            spawn_box_rot(
                commands,
                palette,
                paper_m,
                Vec3::new(mx - 0.35, 0.81, mz + 0.25),
                Vec3::new(0.42, 0.02, 0.3),
                Quat::from_rotation_y(-0.4),
            );
            spawn_cyl(commands, palette, mug_m, Vec3::new(mx - 0.05, 0.86, mz + 0.45), 0.115, 0.12);
            for a in [PI, -FRAC_PI_2, FRAC_PI_2, 0.0_f32] {
                let sx = mx + a.sin() * 1.35;
                let sz = mz + a.cos() * 1.35;
                let face = (mx - sx).atan2(mz - sz);
                let rot = Quat::from_rotation_y(face);
                let cat = |lx: f32, ly: f32, lz: f32| Vec3::new(sx, 0.0, sz) + rot * Vec3::new(lx, ly, lz);
                spawn_box_rot(commands, palette, chair_m.clone(), cat(0.0, 0.44, 0.0), Vec3::new(0.52, 0.12, 0.5), rot);
                spawn_box_rot(
                    commands,
                    palette,
                    chair_m.clone(),
                    cat(0.0, 0.72, -0.24),
                    Vec3::new(0.52, 0.5, 0.1),
                    rot * Quat::from_rotation_x(-0.12),
                );
                for lx in [-0.2_f32, 0.2] {
                    for lz in [-0.19_f32, 0.19] {
                        spawn_cyl(commands, palette, wood_dark.clone(), cat(lx, 0.2, lz), 0.045, 0.4);
                    }
                }
            }
        }
    }

    // 7. CPU-Scaled Server Racks (`server-racks.js`: 3 racks, glass doors, 84 blinking LEDs, 6x 5-blade fans)
    {
        let [rx, rz, rry] = spec.racks;
        let rot = Quat::from_rotation_y(rry);
        let at = |lx: f32, ly: f32, lz: f32| Vec3::new(rx, 0.0, rz) + rot * Vec3::new(lx, ly, lz);
        let rack_m = palette.mat(materials, "#1E2229", 0.55, 0.3);
        let blade_m = palette.mat(materials, "#2C323B", 0.5, 0.35);
        let rack_glass = palette.glass(materials, "#0E1116", 0.45);
        let fan_m = palette.mat(materials, "#9AA3AD", 0.4, 0.5);
        let shroud_m = palette.mat(materials, "#15181D", 0.6, 0.2);

        // 6 shared blink-bank owned_emissive materials drive all 84 rack LEDs cleanly (0 leaks!)
        let led_specs: [(&'static str, f32); 6] = [
            ("#5BE37D", 0.0),
            ("#5BE37D", 1.7),
            ("#FFB347", 0.9),
            ("#FFB347", 2.6),
            ("#4FB6FF", 1.3),
            ("#4FB6FF", 3.1),
        ];
        let mut led_banks: Vec<Handle<StandardMaterial>> = Vec::with_capacity(6);
        for (hex_col, phase) in led_specs {
            let h = palette.owned_emissive(materials, hex_col, 1.6);
            commands.spawn((
                WorldEntity,
                ServerLedBank {
                    material: h.clone(),
                    base_hex: hex_col,
                    phase,
                    lit: true,
                },
            ));
            led_banks.push(h);
        }

        for i in 0..3 {
            let x = -1.05 + i as f32 * 1.05;
            spawn_box_rot(commands, palette, rack_m.clone(), at(x, 1.2, 0.0), Vec3::new(0.95, 2.4, 0.8), rot);
            for r in 0..7 {
                let y = 0.34 + r as f32 * 0.29;
                spawn_box_rot(commands, palette, blade_m.clone(), at(x, y, 0.41), Vec3::new(0.8, 0.22, 0.03), rot);
                for k in 0..4 {
                    let bank_idx = match k {
                        3 => 4 + ((i + r) % 2),
                        1 => 2 + ((i + r) % 2),
                        _ => (i + r + k) % 2,
                    };
                    spawn_box_rot(
                        commands,
                        palette,
                        led_banks[bank_idx].clone(),
                        at(x + 0.12 + k as f32 * 0.08, y, 0.436),
                        Vec3::new(0.05, 0.05, 0.02),
                        rot,
                    );
                }
            }
            // Translucent smoked-glass front door
            spawn_box_rot(commands, palette, rack_glass.clone(), at(x, 1.2, 0.43), Vec3::new(0.9, 2.3, 0.018), rot);

            // 2 top exhaust fan housings + shrouds + 5-blade spinning hubs (`server-racks.js:49-58`)
            for fx in [-0.22_f32, 0.22] {
                let fp = at(x + fx, 2.415, 0.2);
                spawn_cyl(commands, palette, shroud_m.clone(), fp, 0.30, 0.03);
                spawn_torus_rot(
                    commands,
                    palette,
                    metal.clone(),
                    at(x + fx, 2.435, 0.2),
                    Vec3::new(0.28, 0.05, 0.28),
                    rot,
                );
                spawn_cyl(commands, palette, metal.clone(), at(x + fx, 2.445, 0.2), 0.06, 0.02);
                commands
                    .spawn((
                        WorldEntity,
                        ServerFan {
                            speed: 1.0 + i as f32 * 0.12,
                        },
                        Transform::from_translation(at(x + fx, 2.43, 0.2)).with_rotation(rot),
                        Visibility::default(),
                    ))
                    .with_children(|hub| {
                        for b in 0..5 {
                            let a = (b as f32 / 5.0) * TAU;
                            hub.spawn((
                                WorldEntity,
                                Mesh3d(palette.unit_box.clone()),
                                MeshMaterial3d(fan_m.clone()),
                                Transform::from_xyz(a.cos() * 0.065, 0.0, a.sin() * 0.065)
                                    .with_rotation(Quat::from_rotation_y(-a))
                                    .with_scale(Vec3::new(0.11, 0.008, 0.04)),
                            ));
                        }
                    });
            }
        }
        // Overhead cable tray (`server-racks.js:47`)
        spawn_box_rot(commands, palette, metal.clone(), at(0.0, 2.5, -0.05), Vec3::new(3.3, 0.06, 0.4), rot);

        // Cozy server-corner glass partition (`server-racks.js:60-64`)
        if !spec.layout.is_pro() {
            let pglass = palette.glass(materials, "#BFD8EE", 0.16);
            spawn_box(commands, palette, pglass, Vec3::new(3.75, 1.3, -4.95), Vec3::new(0.03, 2.6, 2.1));
            for pz in [-6.0_f32, -3.9] {
                spawn_box(commands, palette, metal.clone(), Vec3::new(3.75, 1.3, pz), Vec3::new(0.06, 2.6, 0.06));
            }
            spawn_box(commands, palette, metal.clone(), Vec3::new(3.75, 2.6, -4.95), Vec3::new(0.06, 0.06, 2.16));
        }
    }

    // 8. 3.7m Open Bookshelf + colorful varied books + top plants (`room-pieces.js:19-37`)
    {
        let [sx, sz, sry] = spec.shelf;
        let w = 3.7_f32;
        let rot = Quat::from_rotation_y(sry);
        let at = |lx: f32, ly: f32, lz: f32| Vec3::new(sx, 0.0, sz) + rot * Vec3::new(lx, ly, lz);

        spawn_box_rot(commands, palette, wood_dark.clone(), at(0.0, 1.35, -0.28), Vec3::new(w, 2.7, 0.08), rot);
        spawn_box_rot(commands, palette, wood_dark.clone(), at(-w * 0.5 + 0.03, 1.35, 0.0), Vec3::new(0.12, 2.7, 0.62), rot);
        spawn_box_rot(commands, palette, wood_dark.clone(), at(w * 0.5 - 0.03, 1.35, 0.0), Vec3::new(0.12, 2.7, 0.62), rot);
        for sy in [0.06_f32, 0.92, 1.78, 2.64] {
            spawn_box_rot(commands, palette, wood_dark.clone(), at(0.0, sy, 0.0), Vec3::new(w, 0.08, 0.62), rot);
        }

        let book_cols = [
            "#C8553D", "#F28F3B", "#588B8B", "#2F4858", "#FFD166",
            "#8E5BB5", "#6A8042", "#E07A5F", "#3D5A80", "#EDE6D6",
        ];
        let mut brng = FurnRng(313);
        for sy in [0.10_f32, 0.96, 1.82] {
            let mut bx = -w * 0.5 + 0.15;
            while bx < w * 0.5 - 0.30 {
                let bw = brng.range(0.10, 0.19);
                let bh = brng.range(0.48, 0.74);
                let bd = brng.range(0.40, 0.50);
                let ci = ((brng.next_f32() * book_cols.len() as f32).floor() as usize).min(book_cols.len() - 1);
                let bm = palette.matte(materials, book_cols[ci]);
                let tilt = if brng.next_f32() < 0.07 { 0.22 } else { 0.0 };
                spawn_box_rot(
                    commands,
                    palette,
                    bm,
                    at(bx + bw * 0.5, sy + bh * 0.5, 0.02),
                    Vec3::new(bw, bh, bd),
                    rot * Quat::from_rotation_z(tilt),
                );
                bx += bw + brng.range(0.006, 0.03);
            }
        }
        let p_left = at(-w * 0.32, 2.68, 0.0);
        let p_right = at(w * 0.30, 2.68, 0.0);
        spawn_typed_plant(commands, palette, materials, p_left.x, p_left.y, p_left.z, 0.45, PlantKind::Monstera);
        spawn_typed_plant(commands, palette, materials, p_right.x, p_right.y, p_right.z, 0.40, PlantKind::Snake);
        let box_m = palette.matte(materials, "#C8B49A");
        spawn_box_rot(commands, palette, box_m, at(0.1, 2.79, 0.0), Vec3::new(0.5, 0.22, 0.34), rot);
    }

    // 9. Lounge Sofa + Round Coffee Table (`room-pieces.js:39-50, 85-92`, Cozy crisp `#EFECE5`!)
    {
        let [sx, sz, sw, sry] = spec.sofa;
        let (sofa_col, pillows) = if spec.layout.is_pro() {
            ("#5C6B7F", ["#F2B33D", "#E8742A"])
        } else {
            ("#EFECE5", ["#4E7D5B", "#E8742A"])
        };
        spawn_sofa(commands, palette, materials, sx, sz, sw, sry, sofa_col, pillows);

        // Round coffee table in front of sofa (`sofaAt(0, 1.05)`)
        let rot = Quat::from_rotation_y(sry);
        let sofa_at = |lx: f32, ly: f32, lz: f32| Vec3::new(sx, 0.0, sz) + rot * Vec3::new(lx, ly, lz);
        spawn_cyl(commands, palette, wood_mid, sofa_at(0.0, 0.44, 1.05), 0.90, 0.06);
        spawn_cyl(commands, palette, wood_dark.clone(), sofa_at(0.0, 0.21, 1.05), 0.14, 0.42);
        let book_m = palette.matte(materials, "#3D5A80");
        let mug_m = palette.mat(materials, "#FFFFFF", 0.35, 0.0);
        spawn_box_rot(
            commands,
            palette,
            book_m,
            sofa_at(-0.1, 0.495, 1.1),
            Vec3::new(0.3, 0.05, 0.22),
            rot * Quat::from_rotation_y(0.4),
        );
        spawn_cyl(commands, palette, mug_m, sofa_at(0.2, 0.52, 0.95), 0.105, 0.10);
    }

    // 10. Floor Lamps & Room Plants (`plants-lamps.js:23-32`)
    {
        if spec.layout.is_pro() {
            let hq = spec.layout.is_hq();
            let plants = [
                (11.95_f32, 8.55_f32, 1.2_f32, PlantKind::Monstera),
                (-3.15, -7.5, 1.0, PlantKind::Monstera),
                (-4.1, 4.95, 0.95, PlantKind::Snake),
                (if hq { -1.25 } else { -2.0 }, if hq { 8.6 } else { 8.55 }, if hq { 0.9 } else { 0.95 }, PlantKind::Bush),
                (8.2, -3.55, 0.9, PlantKind::Snake),
                (2.65, -7.55, 0.85, PlantKind::Monstera),
                (13.55, -2.9, 0.95, PlantKind::Bush),
                (if hq { 8.28 } else { 7.95 }, if hq { 8.72 } else { 8.55 }, if hq { 0.7 } else { 1.0 }, PlantKind::Snake),
            ];
            for (px, pz, s, kind) in plants {
                spawn_typed_plant(commands, palette, materials, px, 0.0, pz, s, kind);
            }
            for &(lx, lz, h) in &[
                (-7.5_f32, -5.45_f32, 2.1_f32),
                (6.25, -4.75, 1.7),
                (13.6, 4.4, 2.1),
                (-7.6, 1.6, 2.1),
            ] {
                spawn_floor_lamp(commands, palette, materials, lx, lz, h);
            }
        } else {
            let plants = [
                (7.3_f32, 5.3_f32, 1.25_f32, PlantKind::Monstera),
                (2.75, -5.4, 1.05, PlantKind::Monstera),
                (-7.45, 5.45, 0.95, PlantKind::Snake),
                (7.4, -1.4, 1.0, PlantKind::Bush),
                (-3.6, -5.5, 0.9, PlantKind::Snake),
            ];
            for (px, pz, s, kind) in plants {
                spawn_typed_plant(commands, palette, materials, px, 0.0, pz, s, kind);
            }
            for &(lx, lz, h) in &[
                (-0.75_f32, -5.45_f32, 2.1_f32),
                (7.55, 3.9, 2.1),
                (-7.4, 3.3, 2.1),
            ] {
                spawn_floor_lamp(commands, palette, materials, lx, lz, h);
            }
        }
    }

    // 11. Spinning Globe (`room-pieces.js:93-106`)
    {
        let [gx, gz] = spec.globe;
        let brass = palette.mat(materials, "#B08D57", 0.35, 0.6);
        let globe_mat = palette.textured_lit(images, materials, draw_globe_map(), 0.35);

        spawn_cyl(commands, palette, wood_dark, Vec3::new(gx, 0.05, gz), 0.70, 0.10);
        spawn_cyl(commands, palette, brass.clone(), Vec3::new(gx, 0.55, gz), 0.07, 0.95);
        commands
            .spawn((
                WorldEntity,
                Transform::from_xyz(gx, 1.4, gz).with_rotation(Quat::from_rotation_z(0.38)),
                Visibility::default(),
            ))
            .with_children(|tilt| {
                tilt.spawn((
                    WorldEntity,
                    SpinningGlobe { speed: 0.25 },
                    Mesh3d(palette.unit_sphere.clone()),
                    MeshMaterial3d(globe_mat),
                    Transform::IDENTITY.with_scale(Vec3::splat(0.88)),
                ));
                tilt.spawn((
                    WorldEntity,
                    Mesh3d(palette.unit_torus.clone()),
                    MeshMaterial3d(brass),
                    Transform::IDENTITY
                        .with_rotation(Quat::from_euler(EulerRot::YXZ, FRAC_PI_2, 0.0, -0.45))
                        .with_scale(Vec3::new(1.0, 0.12, 1.0)),
                ));
            });
    }

    // 12. DeployRocket on Launch Pad (`rocket.js:11-22`)
    {
        let [rx, rz] = spec.rocket;
        let home = Vec3::new(rx, 0.12, rz);
        let pad_m = palette.mat(materials, "#3A3F48", 0.6, 0.2);
        let ring_m = palette.emissive(materials, "#FFD166", 1.3);
        let body_m = palette.mat(materials, "#F7F3EA", 0.35, 0.2);
        let cone_m = palette.mat(materials, "#E8730E", 0.4, 0.1);
        let fin_m = palette.mat(materials, "#C8553D", 0.45, 0.1);
        let win_m = palette.mat(materials, "#7EC8E3", 0.1, 0.3);
        let flame_m = palette.owned_emissive(materials, "#FF7A24", 2.6);

        spawn_cyl(commands, palette, pad_m, Vec3::new(rx, 0.06, rz), 1.15, 0.12);
        spawn_torus_rot(
            commands,
            palette,
            ring_m,
            Vec3::new(rx, 0.125, rz),
            Vec3::new(0.90, 0.16, 0.90),
            Quat::IDENTITY,
        );

        let mut flame_ent = Entity::PLACEHOLDER;
        let rocket_ent = commands
            .spawn((
                WorldEntity,
                InteractiveProp::new("rocket", "deploy", "Deploy Rocket"),
                Transform::from_translation(home),
                Visibility::default(),
            ))
            .with_children(|r| {
                r.spawn((
                    WorldEntity,
                    Mesh3d(palette.unit_cyl.clone()),
                    MeshMaterial3d(body_m),
                    Transform::from_xyz(0.0, 0.55, 0.0).with_scale(Vec3::new(0.42, 0.80, 0.42)),
                ));
                r.spawn((
                    WorldEntity,
                    Mesh3d(palette.unit_cone.clone()),
                    MeshMaterial3d(cone_m),
                    Transform::from_xyz(0.0, 1.14, 0.0).with_scale(Vec3::new(0.40, 0.38, 0.40)),
                ));
                for k in 0..3 {
                    let a = (k as f32 / 3.0) * TAU;
                    r.spawn((
                        WorldEntity,
                        Mesh3d(palette.unit_box.clone()),
                        MeshMaterial3d(fin_m.clone()),
                        Transform::from_xyz(a.sin() * 0.22, 0.26, a.cos() * 0.22)
                            .with_rotation(Quat::from_rotation_y(a))
                            .with_scale(Vec3::new(0.05, 0.32, 0.22)),
                    ));
                }
                r.spawn((
                    WorldEntity,
                    Mesh3d(palette.unit_sphere.clone()),
                    MeshMaterial3d(win_m),
                    Transform::from_xyz(0.0, 0.70, 0.19).with_scale(Vec3::splat(0.14)),
                ));
                flame_ent = r
                    .spawn((
                        WorldEntity,
                        Mesh3d(palette.unit_cone.clone()),
                        MeshMaterial3d(flame_m),
                        Transform::from_xyz(0.0, 0.04, 0.0)
                            .with_rotation(Quat::from_rotation_x(PI))
                            .with_scale(Vec3::new(0.26, 0.46, 0.26)),
                        Visibility::Hidden,
                    ))
                    .id();
            })
            .id();

        commands.entity(rocket_ent).insert(DeployRocket {
            home,
            flight_t: -1.0,
            confetti_spawned: false,
            puff_accum: 0.0,
            flame: flame_ent,
        });
    }
}

/// Live `Update` animation system for core furniture:
/// - Ticking 3-hand `WallClock`
/// - `SpinningGlobe`
/// - CPU-scaled `ServerFan` & blinking `ServerLedBank`
/// - Night glow on `NeonSign` & `FloorLampBulb`
/// - `DeployRocket` launch rumble/ascent + smoke puffs + 70-piece gravity/spin confetti lifecycle.
#[allow(clippy::too_many_arguments, clippy::type_complexity)]
pub fn animate_furniture_system(
    mut commands: Commands,
    time: Res<Time>,
    lighting: Option<Res<LightingState>>,
    clock_override: Option<Res<ClockTimeOverride>>,
    telemetry: Option<Res<ServerRackTelemetry>>,
    mut rocket_queue: Option<ResMut<RocketLaunchQueue>>,
    mut palette: Option<ResMut<WorldPalette>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
    clocks_q: Query<&WallClock>,
    mut led_banks_q: Query<&mut ServerLedBank>,
    neon_q: Query<&NeonSign>,
    lamp_bulbs_q: Query<&FloorLampBulb>,
    mut tfs: ParamSet<(
        Query<(&SpinningGlobe, &mut Transform)>,
        Query<(&ServerFan, &mut Transform)>,
        Query<(&mut DeployRocket, &mut Transform)>,
        Query<(Entity, &mut RocketParticle, &mut Transform)>,
        Query<&mut Transform>,
    )>,
    mut vis_q: Query<&mut Visibility>,
) {
    let dt = time.delta_secs();
    let t = time.elapsed_secs();

    // 1. Ticking 3-hand analog WallClock (`room-update.js:105-106`)
    if !clocks_q.is_empty() {
        let (hour, minute, second) = if let Some(ov) = clock_override {
            (ov.hour, ov.minute, ov.second)
        } else {
            let now = Local::now();
            (now.hour(), now.minute(), now.second() as f32)
        };
        let mut hand_tf_q = tfs.p4();
        for clock in &clocks_q {
            apply_clock_time(clock, hour, minute, second, &mut hand_tf_q);
        }
    }

    // 2. Spinning Globe (`room-update.js:58`)
    for (g, mut tf) in &mut tfs.p0() {
        tf.rotate_y(dt * g.speed);
    }

    // 3. CPU-scaled Server Rack Fans & Blinking LEDs (`room-update.js:59-64`)
    let cpu_load = telemetry.as_ref().map(|m| m.cpu_load.clamp(0.0, 1.0)).unwrap_or(0.25);
    let fan_omega = 3.0 + cpu_load * 30.0;
    for (fan, mut tf) in &mut tfs.p1() {
        tf.rotate_y(fan_omega * fan.speed * dt);
    }
    let led_freq = 2.5 + cpu_load * 12.0;
    for mut bank in &mut led_banks_q {
        let pulse = (t * led_freq + bank.phase * 2.3).sin();
        let next_lit = pulse > -0.25;
        if next_lit != bank.lit {
            bank.lit = next_lit;
            if let Some(mat) = materials.get_mut(&bank.material) {
                let lin = crate::palette::srgb_hex(bank.base_hex).to_linear();
                let intensity = if next_lit { 1.6 } else { 0.14 };
                mat.emissive = LinearRgba::new(lin.red * intensity, lin.green * intensity, lin.blue * intensity, 1.0);
            }
        }
    }

    // 4. Night glow on SHIP IT NeonSign & FloorLampBulbs
    if let Some(light) = lighting
        && light.is_changed()
    {
        let night = light.is_night();
        let neon_str = if night { 2.6 } else { 1.25 };
        for n in &neon_q {
            if let Some(mat) = materials.get_mut(&n.material) {
                mat.emissive = LinearRgba::new(neon_str, neon_str, neon_str, 1.0);
            }
        }
        let (bulb_col, bulb_str) = if night {
            (Color::srgb_u8(255, 224, 158), 2.5_f32)
        } else {
            (Color::srgb_u8(237, 230, 214), 0.4_f32)
        };
        let blin = bulb_col.to_linear();
        for b in &lamp_bulbs_q {
            if let Some(mat) = materials.get_mut(&b.material) {
                mat.base_color = bulb_col;
                mat.emissive = LinearRgba::new(blin.red * bulb_str, blin.green * bulb_str, blin.blue * bulb_str, 1.0);
            }
        }
    }

    // 5. DeployRocket launch trigger + ascent + smoke puffs + 70-piece confetti burst (`room-update.js:85-99`)
    let mut trigger_now = false;
    if let Some(ref mut q) = rocket_queue
        && q.pending
    {
        q.pending = false;
        trigger_now = true;
    }

    let mut puff_origins: Vec<Vec3> = Vec::new();
    let mut confetti_origins: Vec<Vec3> = Vec::new();

    for (mut rocket, mut tf) in &mut tfs.p2() {
        if trigger_now && rocket.flight_t < 0.0 {
            rocket.flight_t = 0.0;
            rocket.confetti_spawned = false;
            rocket.puff_accum = 0.0;
            if let Ok(mut v) = vis_q.get_mut(rocket.flame) {
                *v = Visibility::Inherited;
            }
        }
        if rocket.flight_t >= 0.0 {
            rocket.flight_t += dt;
            let tt = rocket.flight_t;
            if tt < 0.6 {
                tf.translation = Vec3::new(
                    rocket.home.x + (tt * 70.0).sin() * 0.025,
                    rocket.home.y,
                    rocket.home.z,
                );
            } else {
                let u = tt - 0.6;
                tf.translation = Vec3::new(
                    rocket.home.x,
                    rocket.home.y + u * u * 4.5,
                    rocket.home.z,
                );
                rocket.puff_accum += dt;
                if rocket.puff_accum >= 0.06 {
                    rocket.puff_accum = 0.0;
                    puff_origins.push(tf.translation);
                }
            }
            if tt > 0.9 && !rocket.confetti_spawned {
                rocket.confetti_spawned = true;
                confetti_origins.push(Vec3::new(
                    rocket.home.x - 1.6,
                    4.5,
                    rocket.home.z + 1.0,
                ));
            }
            if tt > 4.2 {
                rocket.flight_t = -1.0;
                tf.translation = rocket.home;
                tf.scale = Vec3::splat(0.01);
                if let Ok(mut v) = vis_q.get_mut(rocket.flame) {
                    *v = Visibility::Hidden;
                }
            }
        } else if tf.scale.x < 1.0 {
            tf.scale = Vec3::splat((tf.scale.x + dt * 1.2).min(1.0));
        }
    }

    if let Some(ref mut pal) = palette {
        if !puff_origins.is_empty() {
            let puff_m = pal.mat(&mut materials, "#F1F1F1", 0.95, 0.0);
            let mut prng = FurnRng(((t * 1000.0) as u64).wrapping_add(19) | 1);
            for origin in puff_origins {
                let jitter = Vec3::new(prng.range(-0.08, 0.08), 0.0, prng.range(-0.08, 0.08));
                let vel = Vec3::new(prng.range(-0.3, 0.3), -0.4, prng.range(-0.3, 0.3));
                commands.spawn((
                    WorldEntity,
                    RocketParticle {
                        kind: ParticleKind::Puff,
                        vel,
                        spin: Vec3::ZERO,
                        life: 1.2,
                        max_life: 1.2,
                    },
                    Mesh3d(pal.unit_sphere.clone()),
                    MeshMaterial3d(puff_m.clone()),
                    Transform::from_translation(origin + jitter).with_scale(Vec3::splat(0.24)),
                ));
            }
        }
        if !confetti_origins.is_empty() {
            let conf_mats: Vec<Handle<StandardMaterial>> = CONFETTI_COLORS
                .iter()
                .map(|c| pal.matte(&mut materials, c))
                .collect();
            let mut crng = FurnRng(((t * 1000.0) as u64).wrapping_add(97) | 1);
            for origin in confetti_origins {
                for i in 0..CONFETTI_COUNT {
                    let vel = Vec3::new(
                        crng.range(-3.0, 3.0),
                        crng.range(1.0, 4.0),
                        crng.range(-3.0, 3.0),
                    );
                    let spin = Vec3::new(
                        crng.range(-8.0, 8.0),
                        crng.range(-8.0, 8.0),
                        crng.range(-8.0, 8.0),
                    );
                    commands.spawn((
                        WorldEntity,
                        RocketParticle {
                            kind: ParticleKind::Confetti,
                            vel,
                            spin,
                            life: 3.2,
                            max_life: 3.2,
                        },
                        Mesh3d(pal.unit_box.clone()),
                        MeshMaterial3d(conf_mats[i % conf_mats.len()].clone()),
                        Transform::from_translation(origin)
                            .with_scale(Vec3::new(0.14, 0.09, 0.008)),
                    ));
                }
            }
        }
    }

    // 6. Step smoke puffs & gravity/spin confetti (`room-update.js:93-99`)
    for (ent, mut p, mut tf) in &mut tfs.p3() {
        p.life -= dt;
        if p.life <= 0.0 {
            commands.entity(ent).try_despawn();
            continue;
        }
        match p.kind {
            ParticleKind::Puff => {
                tf.translation += p.vel * dt;
                let s = 0.24 * (1.0 + (1.0 - p.life / p.max_life) * 1.8);
                tf.scale = Vec3::splat(s);
            }
            ParticleKind::Confetti => {
                if tf.translation.y > 0.03 {
                    p.vel.y -= 6.0 * dt;
                    let drag = (1.0 - dt * 0.6).max(0.0);
                    p.vel *= drag;
                    tf.translation += p.vel * dt;
                    tf.rotate_x(p.spin.x * dt);
                    tf.rotate_y(p.spin.y * dt);
                } else {
                    tf.translation.y = 0.03;
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    #[test]
    fn redraw_whiteboard_surface_succeeds_after_render_world_extraction() {
        let assets_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../assets");
        let mut fonts = FontEngine::from_assets_dir(&assets_dir);
        let mut meshes = Assets::<Mesh>::default();
        let mut materials = Assets::<StandardMaterial>::default();
        let mut images = Assets::<Image>::default();
        let mut pal = WorldPalette::new(&mut meshes);

        let initial_surf = draw_whiteboard(
            &WhiteboardInput {
                title: "Whiteboard".into(),
                yours_title: "Yours".into(),
                streamer: false,
                tasks: Vec::new(),
                open_todos: 0,
                total_todos: 0,
            },
            &mut fonts,
        );
        let wb_mat = pal.textured_lit(&mut images, &mut materials, initial_surf, 0.35);
        let tex_handle = materials
            .get(&wb_mat)
            .and_then(|m| m.base_color_texture.clone())
            .expect("whiteboard material must have base_color_texture");

        // Simulate Bevy frame-1 RenderAssetUsages::RENDER_WORLD extraction removing the CPU Image.
        images.remove(&tex_handle);
        assert!(images.get(&tex_handle).is_none());

        let ok = redraw_whiteboard_surface(
            "- Ship Fix 1\n- [x] Verify redraw",
            false,
            &wb_mat,
            &mut materials,
            &mut images,
            &mut fonts,
        );
        assert!(ok, "redraw_whiteboard_surface must succeed after RENDER_WORLD extraction");
        let reinserted = images
            .get(&tex_handle)
            .expect("redraw must reinsert updated Image under the same Handle<Image> id");
        assert!(reinserted.width() > 0 && reinserted.height() > 0);
        assert_eq!(images.len(), 1, "redraw must not leak extra Image entries");
        assert_eq!(materials.len(), 1, "redraw must not leak extra StandardMaterial entries");
    }

    #[test]
    fn wall_clock_hands_are_children_and_match_15_30_00_angles() {
        use crate::layout::OfficeLayout;

        let (hr_z, min_z, sec_z) = clock_hand_angles_rad(15, 30, 0.0);
        let expected_hr = -105.0_f32.to_radians();
        let expected_min = -180.0_f32.to_radians();
        let expected_sec = 0.0_f32;
        assert!(
            (hr_z - expected_hr).abs() < 1e-4,
            "15:30:00 hour hand must be -105° ({expected_hr:.5} rad), got {hr_z:.5}"
        );
        assert!(
            (min_z - expected_min).abs() < 1e-4,
            "15:30:00 minute hand must be -180° ({expected_min:.5} rad), got {min_z:.5}"
        );
        assert!(
            (sec_z - expected_sec).abs() < 1e-4,
            "15:30:00 second hand must be 0° ({expected_sec:.5} rad), got {sec_z:.5}"
        );

        let assets_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../assets");
        let mut fonts = FontEngine::from_assets_dir(&assets_dir);
        let mut world = World::new();
        world.insert_resource(Assets::<Mesh>::default());
        world.insert_resource(Assets::<StandardMaterial>::default());
        world.insert_resource(Assets::<Image>::default());

        let spec = LayoutSpec::for_layout(OfficeLayout::Pro);
        world.resource_scope(|world, mut meshes: Mut<Assets<Mesh>>| {
            world.resource_scope(|world, mut mats: Mut<Assets<StandardMaterial>>| {
                world.resource_scope(|world, mut imgs: Mut<Assets<Image>>| {
                    let mut pal = WorldPalette::new(&mut meshes);
                    let mut cmds = world.commands();
                    spawn_furniture(&mut cmds, &mut pal, &mut mats, &mut imgs, &mut fonts, &spec);
                    cmds.insert_resource(pal);
                });
            });
        });
        world.flush();

        let (clock_ent, clock) = world
            .query::<(Entity, &WallClock)>()
            .single(&world)
            .map(|(e, c)| (e, *c))
            .expect("WallClock entity must be spawned");

        for (label, hand_ent) in [
            ("hour", clock.hour_hand),
            ("minute", clock.min_hand),
            ("second", clock.sec_hand),
        ] {
            let parent = world
                .get::<ChildOf>(hand_ent)
                .map(|c| c.parent());
            assert_eq!(
                parent,
                Some(clock_ent),
                "{label} hand must be a direct child entity of WallClock"
            );
        }

        let mut st = bevy::ecs::system::SystemState::<Query<&mut Transform>>::new(&mut world);
        {
            let mut tf_q = st.get_mut(&mut world);
            apply_clock_time(&clock, 15, 30, 0.0, &mut tf_q);
        }
        for (label, hand_ent, expected) in [
            ("hour", clock.hour_hand, expected_hr),
            ("minute", clock.min_hand, expected_min),
            ("second", clock.sec_hand, expected_sec),
        ] {
            let tf = world.get::<Transform>(hand_ent).unwrap();
            let (_, _, z_rot) = tf.rotation.to_euler(EulerRot::XYZ);
            let diff = ((z_rot - expected + PI).rem_euclid(2.0 * PI)) - PI;
            assert!(
                diff.abs() < 1e-4,
                "{label} hand transform Z rotation mismatch: expected {expected:.4}, got {z_rot:.4}"
            );
        }
    }

    #[test]
    fn deploy_rocket_launch_spawns_70_confetti_that_fall_under_gravity_and_despawn_cleanly() {
        use crate::layout::OfficeLayout;
        use std::time::Duration;

        let assets_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../assets");
        let mut fonts = FontEngine::from_assets_dir(&assets_dir);
        let mut app = App::new();
        app.insert_resource(Time::<()>::default());
        app.insert_resource(Assets::<Mesh>::default());
        app.insert_resource(Assets::<StandardMaterial>::default());
        app.insert_resource(Assets::<Image>::default());
        app.insert_resource(RocketLaunchQueue { pending: true });
        app.insert_resource(ServerRackTelemetry::default());
        app.add_systems(Update, animate_furniture_system);

        let spec = LayoutSpec::for_layout(OfficeLayout::Pro);
        app.world_mut().resource_scope(|world, mut meshes: Mut<Assets<Mesh>>| {
            world.resource_scope(|world, mut mats: Mut<Assets<StandardMaterial>>| {
                world.resource_scope(|world, mut imgs: Mut<Assets<Image>>| {
                    let mut pal = WorldPalette::new(&mut meshes);
                    let mut cmds = world.commands();
                    spawn_furniture(&mut cmds, &mut pal, &mut mats, &mut imgs, &mut fonts, &spec);
                    cmds.insert_resource(pal);
                });
            });
        });
        app.world_mut().flush();

        // Advance simulation to t = 1.0s (past 0.9s confetti burst trigger)
        for _ in 0..20 {
            app.world_mut()
                .resource_mut::<Time>()
                .advance_by(Duration::from_millis(50));
            app.update();
        }

        let rocket_y = app
            .world_mut()
            .query::<(&DeployRocket, &Transform)>()
            .single(app.world())
            .expect("DeployRocket entity must exist")
            .1
            .translation
            .y;
        assert!(
            rocket_y > 0.5,
            "DeployRocket must ascend during launch, got y={rocket_y:.3}"
        );

        let confetti_initial: Vec<(Entity, Vec3, Vec3)> = app
            .world_mut()
            .query_filtered::<(Entity, &RocketParticle, &Transform), With<WorldEntity>>()
            .iter(app.world())
            .filter(|(_, p, _)| p.kind == ParticleKind::Confetti)
            .map(|(e, p, tf)| (e, tf.translation, p.vel))
            .collect();
        assert_eq!(
            confetti_initial.len(),
            70,
            "DeployRocket launch must spawn exactly 70 confetti pieces"
        );

        // Step 0.25s and verify confetti accelerate downward under gravity
        for _ in 0..5 {
            app.world_mut()
                .resource_mut::<Time>()
                .advance_by(Duration::from_millis(50));
            app.update();
        }
        let mut fell_count = 0usize;
        for (ent, _, init_vel) in &confetti_initial {
            if let Some(p) = app.world().get::<RocketParticle>(*ent)
                && p.vel.y < init_vel.y - 0.5
            {
                fell_count += 1;
            }
        }
        assert!(
            fell_count >= 60,
            "confetti must accelerate downward under gravity, only {fell_count}/70 decreased vel.y"
        );

        // Step another 4.5s so all particles expire and despawn cleanly
        for _ in 0..90 {
            app.world_mut()
                .resource_mut::<Time>()
                .advance_by(Duration::from_millis(50));
            app.update();
        }
        let remaining = app
            .world_mut()
            .query::<&RocketParticle>()
            .iter(app.world())
            .count();
        assert_eq!(
            remaining, 0,
            "all smoke puffs and confetti must despawn after lifetime expires, found {remaining}"
        );
    }
}
