//! 1:1 City supergrid, stepped skyscrapers, rooftop clutter, streetlamps, utility poles & wires,
//! and live phase-counting traffic signals (`city.js`, `street-wires.js`, `street-signals.js`).

use crate::backdrops::{BackdropAssets, BackdropEntity};
use crate::layout::{LayoutSpec, OfficeLayout};
use crate::palette::{WorldEntity, srgb_hex};
use crate::street::{CityRng, GY, StreetMeshBuilder, linear_rgba};
use bevy::prelude::*;
use station_canvas2d::street_surfaces::{
    SignalColor, draw_asphalt_tile, draw_facade_day_tile, draw_facade_night_atlas,
    draw_generic_shop_strip, draw_lamp_pool_tex, draw_traffic_countdown_atlas,
};
use std::collections::HashSet;
use std::f32::consts::{FRAC_PI_2, FRAC_PI_4, PI};

pub const STREET_W: f32 = 4.4;
pub const BLOCK: f32 = 15.0;
pub const FLOOR_H: f32 = 3.0;
pub const BAY_W: f32 = 1.9;
pub const SHOPS_COUNT: usize = 12;
pub const SHOP_W: f32 = 6.6;
pub const SHOP_CEIL: f32 = 2.75;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ShopDir {
    PosX,
    PosZ,
}

#[derive(Debug, Clone)]
pub struct ShopFront {
    pub dir: ShopDir,
    pub x: f32,
    pub z: f32,
    pub len: f32,
    pub deep: f32,
}

#[derive(Debug, Clone, Copy)]
pub struct SuperBlock {
    pub x0: f32,
    pub x1: f32,
    pub z0: f32,
    pub z1: f32,
}

impl SuperBlock {
    pub fn contains(&self, x: f32, z: f32) -> bool {
        x > self.x0 && x < self.x1 && z > self.z0 && z < self.z1
    }
}

#[derive(Debug, Clone, Copy)]
pub struct PickleSite {
    pub axis_x: bool,
    pub x0: f32,
    pub x1: f32,
    pub z0: f32,
    pub z1: f32,
}

#[derive(Debug, Clone, Copy)]
pub struct SignalPole {
    pub x: f32,
    pub z: f32,
    pub i: usize,
    pub j: usize,
    pub axis_x: bool,
    pub fx: f32,
    pub fz: f32,
}

#[derive(Debug, Clone)]
pub struct CityGridData {
    pub layout: OfficeLayout,
    pub terrace_excl: [f32; 4],
    pub xs: Vec<f32>,
    pub zs: Vec<f32>,
    pub sb: SuperBlock,
    pub office_mid: Vec2,
    pub shop_fronts: Vec<ShopFront>,
    pub plain_fronts: Vec<ShopFront>,
    pub parks: Vec<[f32; 4]>,
    pub plazas: Vec<[f32; 4]>,
    pub trees: Vec<[f32; 3]>,
    pub solids: Vec<[f32; 5]>,
    pub pickle: Option<PickleSite>,
    pub signal_poles: Vec<SignalPole>,
    pub lamp_positions: Vec<[f32; 2]>,
}

/// Exact terrace + railing clearance rectangle `[x0, x1, z0, z1]` matching `rooms.rs:627-631`
/// so streetlamps, pedestrian tracks, bus stop, EV scooters, and CSGT never clip the terrace.
pub fn terrace_exclusion(layout: OfficeLayout) -> [f32; 4] {
    match layout {
        OfficeLayout::Cozy => [-9.4, 12.6, -7.3, 11.2],
        OfficeLayout::Pro => [-9.4, 16.0, -8.9, 12.1],
        OfficeLayout::Hq => [-9.4, 16.0, -8.9, 29.1],
    }
}

pub fn rect_contains_padded(r: [f32; 4], x: f32, z: f32, pad: f32) -> bool {
    x > r[0] - pad && x < r[1] + pad && z > r[2] - pad && z < r[3] + pad
}

impl CityGridData {
    pub fn in_terrace(&self, x: f32, z: f32, pad: f32) -> bool {
        rect_contains_padded(self.terrace_excl, x, z, pad)
    }
}

/// Marker components for day/night & animated city infrastructure updates.
#[derive(Component)]
pub struct CityNightAsphalt;
#[derive(Component)]
pub struct CityNightFacade;
#[derive(Component)]
pub struct CityNightSidewalk;
#[derive(Component)]
pub struct CityNightLampBulb;
#[derive(Component)]
pub struct CityNightLampPool;
#[derive(Component)]
pub struct RooftopBeacon {
    pub phase: f32,
}

/// Live traffic signal lamp & countdown display component (`street-signals.js`).
#[derive(Component)]
pub struct TrafficSignalLamp {
    pub i: usize,
    pub j: usize,
    pub axis_x: bool,
    pub color: SignalColor,
}

#[derive(Component)]
pub struct TrafficCountdownDisplay {
    pub i: usize,
    pub j: usize,
    pub axis_x: bool,
    pub last_frame: u8,
    pub frames: [Handle<Mesh>; 22],
}

/// Phase function matching Oracle `street-life.js` & `street-signals.js`:
/// 22s cycle: `[0..8)` green, `[8..10)` amber, `[10..22)` red.
pub fn signal_phase(i: usize, j: usize, t: f32, axis_x: bool) -> f32 {
    let off = ((i * 5 + j * 7) % 11) as f32 * 2.0 + if axis_x { 0.0 } else { 11.0 };
    (t + off).rem_euclid(22.0)
}

pub fn signal_color_of(phase: f32) -> SignalColor {
    if phase < 8.0 {
        SignalColor::Green
    } else if phase < 10.0 {
        SignalColor::Amber
    } else {
        SignalColor::Red
    }
}

pub fn shop_depth(deep: f32) -> f32 {
    (deep - 1.4).min(5.0).max(2.6)
}

fn office_middle(layout: OfficeLayout) -> Vec2 {
    match layout {
        OfficeLayout::Hq => Vec2::new(3.0, 9.0),
        OfficeLayout::Pro => Vec2::new(3.0, 0.7),
        OfficeLayout::Cozy => Vec2::new(0.0, 0.2),
    }
}

fn whole_park(cx: f32, cz: f32) -> bool {
    let depth = cx + cz;
    let side = cx - cz;
    depth >= -20.0 && !(side.abs() > 27.0 && depth < 6.0) && !(depth >= 6.0 && side.abs() > 34.0)
}

fn dist_to_rect(p: Vec2, r: [f32; 4]) -> f32 {
    let dx = p.x - p.x.clamp(r[0], r[1]);
    let dz = p.y - p.y.clamp(r[2], r[3]);
    dx.hypot(dz)
}

fn find_pickle_site(sb: SuperBlock, parks: &[[f32; 4]]) -> Option<PickleSite> {
    let half = STREET_W * 0.5;
    let p = 1.3;
    let quiet_x = |v: f32| v > sb.x0 && v < sb.x1;
    let quiet_z = |v: f32| v > sb.z0 && v < sb.z1;
    for &[x0, x1, z0, z1] in parks {
        if quiet_x(x0 - half) {
            return Some(PickleSite {
                axis_x: true,
                x0: x0 - STREET_W,
                x1,
                z0: z0 + p,
                z1: z1 - p,
            });
        }
        if quiet_x(x1 + half) {
            return Some(PickleSite {
                axis_x: true,
                x0,
                x1: x1 + STREET_W,
                z0: z0 + p,
                z1: z1 - p,
            });
        }
        if quiet_z(z0 - half) {
            return Some(PickleSite {
                axis_x: false,
                x0: x0 + p,
                x1: x1 - p,
                z0: z0 - STREET_W,
                z1,
            });
        }
        if quiet_z(z1 + half) {
            return Some(PickleSite {
                axis_x: false,
                x0: x0 + p,
                x1: x1 - p,
                z0,
                z1: z1 + STREET_W,
            });
        }
    }
    None
}

struct LowerBlock {
    style: usize,
    x0: f32,
    x1: f32,
    z0: f32,
    z1: f32,
    h: f32,
    tint: &'static str,
    off_u: f32,
    off_v: f32,
    notched: bool,
}

struct CandidateFront {
    front: ShopFront,
    bid: usize,
    plain_slot: Option<usize>,
    is_store: bool,
}

#[allow(clippy::too_many_arguments)]
pub fn spawn_city_grid(
    commands: &mut Commands,
    meshes: &mut Assets<Mesh>,
    materials: &mut Assets<StandardMaterial>,
    images: &mut Assets<Image>,
    ba: &mut BackdropAssets,
    spec: &LayoutSpec,
    night: bool,
) -> CityGridData {
    let mut rng = CityRng::new(2026_1026);
    let half = STREET_W * 0.5;

    // 1. Repeating asphalt ground plane (320×320, tiled 64×64)
    let asphalt_img = ba.upload_repeating_image(images, draw_asphalt_tile());
    let asphalt_mat = ba.textured_tiled(
        materials,
        asphalt_img,
        Vec2::splat(64.0),
        if night { "#1B1F27" } else { "#5B616A" },
        0.92,
    );
    let ground_mesh = ba.add_mesh(meshes, Mesh::from(Plane3d::new(Vec3::Y, Vec2::splat(160.0))));
    commands.spawn((
        WorldEntity,
        BackdropEntity,
        CityNightAsphalt,
        Mesh3d(ground_mesh),
        MeshMaterial3d(asphalt_mat),
        Transform::from_xyz(0.0, GY, 0.0),
    ));

    // 2. Supergrid coordinates XS & ZS matching city.js:34-37
    let xs: Vec<f32> = (-6..=4).map(|k| 18.0 + k as f32 * BLOCK).collect();
    let zs: Vec<f32> = (-6..=4).map(|k| 16.2 + k as f32 * BLOCK).collect();
    let sb = SuperBlock {
        x0: -12.0 + half,
        x1: 18.0 - half,
        z0: -13.8 + half,
        z1: (if spec.layout.is_hq() { 31.2 } else { 16.2 }) - half,
    };
    let office_mid = office_middle(spec.layout);
    let terrace_excl = terrace_exclusion(spec.layout);

    // 3. Identify park blocks, paved park squares (plazas), and pickleball site
    let mut all_parks: Vec<[f32; 4]> = Vec::new();
    for i in 0..xs.len() - 1 {
        for j in 0..zs.len() - 1 {
            let r = [xs[i] + half, xs[i + 1] - half, zs[j] + half, zs[j + 1] - half];
            let cx = (r[0] + r[1]) * 0.5;
            let cz = (r[2] + r[3]) * 0.5;
            if !sb.contains(cx, cz) && whole_park(cx, cz) {
                all_parks.push(r);
            }
        }
    }
    all_parks.sort_by(|a, b| {
        dist_to_rect(office_mid, *a)
            .partial_cmp(&dist_to_rect(office_mid, *b))
            .unwrap_or(std::cmp::Ordering::Equal)
    });
    let square_keys: HashSet<String> = all_parks
        .iter()
        .filter(|r| dist_to_rect(office_mid, **r) < 30.0)
        .skip(2)
        .take(2)
        .map(|r| format!("{:.1},{:.1}", r[0], r[2]))
        .collect();
    let pickle_candidates: Vec<[f32; 4]> = all_parks
        .iter()
        .skip(2)
        .filter(|r| !square_keys.contains(&format!("{:.1},{:.1}", r[0], r[2])))
        .copied()
        .collect();
    let pickle = find_pickle_site(sb, &pickle_candidates).or_else(|| find_pickle_site(sb, &all_parks));
    let on_court = |x: f32, z: f32, m: f32| {
        pickle.is_some_and(|p| x > p.x0 - m && x < p.x1 + m && z > p.z0 - m && z < p.z1 + m)
    };

    // 4. Facade style materials (day tile + night lit window emissive atlas)
    let style_specs = [(0.30_f32, 0.45_f32), (0.88, 0.0), (0.92, 0.0), (0.70, 0.05)];
    let mut style_mats = Vec::with_capacity(4);
    for (idx, &(rough, metal)) in style_specs.iter().enumerate() {
        let day_img = ba.upload_repeating_image(images, draw_facade_day_tile(idx));
        let night_img = ba.upload_repeating_image(images, draw_facade_night_atlas(idx));
        let em = if night { 0.9 } else { 0.0 };
        let mat = ba.add_material(
            materials,
            StandardMaterial {
                base_color: Color::WHITE,
                base_color_texture: Some(day_img),
                emissive: LinearRgba::new(em, em, em, 1.0),
                emissive_texture: Some(night_img),
                emissive_channel: bevy::pbr::UvChannel::Uv1,
                perceptual_roughness: rough,
                metallic: metal,
                ..default()
            },
        );
        style_mats.push(mat);
    }

    let facade_tints: [&[&str]; 4] = [
        &["#86A9BC", "#6F92A8", "#93ADA9", "#647F96"],
        &["#CFC8BC", "#BDB6AA", "#D8D1C3", "#ADA79B"],
        &["#A4573F", "#904B39", "#B26549", "#7D4535"],
        &["#DCD8D0", "#C3CACF", "#E4DED2", "#A3AEB6"],
    ];

    let mut facade_builders = [
        StreetMeshBuilder::default(),
        StreetMeshBuilder::default(),
        StreetMeshBuilder::default(),
        StreetMeshBuilder::default(),
    ];
    let mut roofs = StreetMeshBuilder::default();
    let mut clutter = StreetMeshBuilder::default();
    let mut greens = StreetMeshBuilder::default();
    let mut slabs = StreetMeshBuilder::default();
    let mut lanes = StreetMeshBuilder::default();
    let mut lamp_geo = StreetMeshBuilder::default();
    let mut bulb_geo = StreetMeshBuilder::default();
    let mut pool_geo = StreetMeshBuilder::default();
    let mut plain_shops = StreetMeshBuilder::default();

    let mut lowers: Vec<LowerBlock> = Vec::new();
    let mut candidates: Vec<CandidateFront> = Vec::new();
    let mut parks: Vec<[f32; 4]> = Vec::new();
    let mut plazas: Vec<[f32; 4]> = Vec::new();
    let mut trees: Vec<[f32; 3]> = Vec::new();
    let mut solids: Vec<[f32; 5]> = Vec::new();
    let mut beacon_positions: Vec<Vec3> = Vec::new();

    let add_rooftop = |rng: &mut CityRng,
                       roofs: &mut StreetMeshBuilder,
                       clutter: &mut StreetMeshBuilder,
                       beacons: &mut Vec<Vec3>,
                       x: f32,
                       z: f32,
                       w: f32,
                       d: f32,
                       y: f32,
                       floors: i32| {
        roofs.push_box(Vec3::new(x, GY + y + 0.2, z), Vec3::new(w + 0.24, 0.4, d + 0.24), "#3B4049");
        roofs.push_box(Vec3::new(x, GY + y + 0.42, z), Vec3::new((w - 0.5).max(0.5), 0.05, (d - 0.5).max(0.5)), "#4A505A");
        let top = GY + y + 0.44;
        let max_hvac = (((w * d) / 12.0).floor() as i32 + 1).clamp(1, 5);
        for _ in 0..rng.range_i32(1, max_hvac) {
            let ox = rng.range(-w * 0.5 + 0.7, (w * 0.5 - 0.7).max(-w * 0.5 + 0.71));
            let oz = rng.range(-d * 0.5 + 0.7, (d * 0.5 - 0.7).max(-d * 0.5 + 0.71));
            clutter.push_box(Vec3::new(x + ox, top + 0.31, z + oz), Vec3::new(0.9, 0.62, 0.9), "#8E949C");
        }
        if rng.next_f32() < 0.4 && w > 3.5 && d > 3.5 {
            let ox = rng.range(-w * 0.3, w * 0.3);
            let oz = rng.range(-d * 0.3, d * 0.3);
            clutter.push_box(Vec3::new(x + ox, top + 0.9, z + oz), Vec3::new(2.0, 1.8, 1.6), "#5C636D");
        }
        if rng.next_f32() < 0.3 && w > 3.0 && d > 3.0 {
            let tx = x + rng.range(-w * 0.25, w * 0.25);
            let tz = z + rng.range(-d * 0.25, d * 0.25);
            clutter.push_cyl(Vec3::new(tx, top + 1.55, tz), 0.72, 0.72, 1.45, 10, "#6E5B4B");
            clutter.push_cyl(Vec3::new(tx, top + 2.48, tz), 0.05, 0.78, 0.44, 10, "#5A4A3D");
            for (a, b) in [(-0.48, -0.48), (0.48, -0.48), (-0.48, 0.48), (0.48, 0.48)] {
                clutter.push_box(Vec3::new(tx + a, top + 0.4, tz + b), Vec3::new(0.08, 0.8, 0.08), "#3B3F45");
            }
        }
        if floors >= 8 && rng.next_f32() < 0.55 {
            let mh = rng.range(2.5, 5.0);
            let ax = x + rng.range(-w * 0.2, w * 0.2);
            let az = z + rng.range(-d * 0.2, d * 0.2);
            clutter.push_box(Vec3::new(ax, top + mh * 0.5, az), Vec3::new(0.1, mh, 0.1), "#2E3238");
            beacons.push(Vec3::new(ax, top + mh + 0.12, az));
        }
    };

    let add_tree = |rng: &mut CityRng,
                    greens: &mut StreetMeshBuilder,
                    trees: &mut Vec<[f32; 3]>,
                    x: f32,
                    z: f32,
                    s: f32| {
        trees.push([x, z, s]);
        greens.push_cyl(Vec3::new(x, GY + 0.12 + 0.7 * s, z), 0.12 * s, 0.16 * s, 1.4 * s, 6, "#5B4433");
        let leaf = *rng.pick(&["#3E6B3A", "#4C7A3F", "#36603A", "#58834A"]);
        greens.push_sphere(Vec3::new(x, GY + 0.12 + 1.9 * s, z), Vec3::splat(0.95 * s), leaf);
        greens.push_sphere(Vec3::new(x + 0.4 * s, GY + 0.12 + 2.5 * s, z - 0.2 * s), Vec3::splat(0.7 * s), leaf);
    };

    let add_park = |rng: &mut CityRng,
                    greens: &mut StreetMeshBuilder,
                    plazas: &mut Vec<[f32; 4]>,
                    trees: &mut Vec<[f32; 3]>,
                    x0: f32,
                    x1: f32,
                    z0: f32,
                    z1: f32,
                    square: bool| {
        greens.push_box(
            Vec3::new((x0 + x1) * 0.5, GY + 0.15, (z0 + z1) * 0.5),
            Vec3::new(x1 - x0, 0.06, z1 - z0),
            "#3B6439",
        );
        let sq = if square {
            let p = [x0 + 1.3, x1 - 1.3, z0 + 1.3, z1 - 1.3];
            let cx = (p[0] + p[1]) * 0.5;
            let cz = (p[2] + p[3]) * 0.5;
            let w = p[1] - p[0];
            let d = p[3] - p[2];
            greens.push_box(Vec3::new(cx, GY + 0.18, cz), Vec3::new(w, 0.02, d), "#BDB4A3");
            for k in 1..6 {
                greens.push_box(
                    Vec3::new(p[0] + w * k as f32 / 6.0, GY + 0.192, cz),
                    Vec3::new(0.03, 0.004, d - 0.2),
                    "#A69D8C",
                );
                greens.push_box(
                    Vec3::new(cx, GY + 0.192, p[2] + d * k as f32 / 6.0),
                    Vec3::new(w - 0.2, 0.004, 0.03),
                    "#A69D8C",
                );
            }
            for (bw, bd, bx, bz) in [
                (w, 0.1, cx, p[2] + 0.05),
                (w, 0.1, cx, p[3] - 0.05),
                (0.1, d, p[0] + 0.05, cz),
                (0.1, d, p[1] - 0.05, cz),
            ] {
                greens.push_box(Vec3::new(bx, GY + 0.19, bz), Vec3::new(bw, 0.03, bd), "#8E877B");
            }
            plazas.push(p);
            Some(p)
        } else {
            None
        };
        for _ in 0..rng.range_i32(3, 6) {
            let mut tx = rng.range(x0 + 1.0, x1 - 1.0);
            let mut tz = rng.range(z0 + 1.0, z1 - 1.0);
            let s = rng.range(0.8, 1.25);
            if let Some(p) = sq
                && tx > p[0] - 0.3
                && tx < p[1] + 0.3
                && tz > p[2] - 0.3
                && tz < p[3] + 0.3
            {
                let gaps = [tx - p[0], p[1] - tx, tz - p[2], p[3] - tz];
                let min_k = gaps
                    .iter()
                    .enumerate()
                    .min_by(|a, b| a.1.partial_cmp(b.1).unwrap_or(std::cmp::Ordering::Equal))
                    .map(|(k, _)| k)
                    .unwrap_or(0);
                match min_k {
                    0 => tx = x0 + 0.65,
                    1 => tx = x1 - 0.65,
                    2 => tz = z0 + 0.65,
                    _ => tz = z1 - 0.65,
                }
            }
            if !on_court(tx, tz, 1.2) {
                add_tree(rng, greens, trees, tx, tz, s);
            }
        }
    };

    // 5. Fill every city block outside the office superblock
    for i in 0..xs.len() - 1 {
        for j in 0..zs.len() - 1 {
            let x0 = xs[i] + half;
            let x1 = xs[i + 1] - half;
            let z0 = zs[j] + half;
            let z1 = zs[j + 1] - half;
            let cx = (x0 + x1) * 0.5;
            let cz = (z0 + z1) * 0.5;
            if sb.contains(cx, cz) {
                continue;
            }
            slabs.push_box(Vec3::new(cx, GY + 0.06, cz), Vec3::new(x1 - x0, 0.12, z1 - z0), "#8D939B");
            let m = 1.3;
            let bx0 = x0 + m;
            let bx1 = x1 - m;
            let bz0 = z0 + m;
            let bz1 = z1 - m;
            if whole_park(cx, cz) {
                let is_sq = square_keys.contains(&format!("{:.1},{:.1}", x0, z0));
                add_park(&mut rng, &mut greens, &mut plazas, &mut trees, bx0, bx1, bz0, bz1, is_sq);
                parks.push([x0, x1, z0, z1]);
                continue;
            }
            let depth = cx + cz;
            let side = cx - cz;
            let front_low = depth >= 6.0 && side.abs() > 34.0;
            let far = (-depth - 20.0).max(0.0);
            let gap = 0.5;
            let split = rng.next_f32();
            let mut lots: Vec<[f32; 4]> = Vec::new();
            if split < 0.25 {
                lots.push([bx0, bx1, bz0, bz1]);
            } else if split < 0.60 {
                let mx = rng.range(bx0 + 3.0, bx1 - 3.0);
                lots.push([bx0, mx - gap * 0.5, bz0, bz1]);
                lots.push([mx + gap * 0.5, bx1, bz0, bz1]);
            } else {
                let mx = (bx0 + bx1) * 0.5;
                let mz = (bz0 + bz1) * 0.5;
                lots.push([bx0, mx - gap * 0.5, bz0, mz - gap * 0.5]);
                lots.push([mx + gap * 0.5, bx1, bz0, mz - gap * 0.5]);
                lots.push([bx0, mx - gap * 0.5, mz + gap * 0.5, bz1]);
                lots.push([mx + gap * 0.5, bx1, mz + gap * 0.5, bz1]);
            }

            for [a, b, c, d] in lots {
                if rng.next_f32() < 0.10 {
                    add_park(&mut rng, &mut greens, &mut plazas, &mut trees, a, b, c, d, false);
                    continue;
                }
                let floors = if front_low {
                    1
                } else if far < 12.0 {
                    rng.range_i32(2, 4)
                } else {
                    rng.range_i32(3, 5)
                        + ((far - 12.0) * 0.2).floor().min(10.0) as i32
                        + if rng.next_f32() < 0.16 { rng.range_i32(5, 11) } else { 0 }
                };
                let w = b - a;
                let dep = d - c;
                let lx = (a + b) * 0.5;
                let lz = (c + d) * 0.5;
                solids.push([a, b, c, d, GY + floors as f32 * FLOOR_H]);

                let style = if floors >= 9 {
                    if rng.next_f32() < 0.7 { 0 } else { 3 }
                } else {
                    *rng.pick(&[1usize, 2, 3, 1, 2])
                };
                let tint = *rng.pick(facade_tints[style]);
                let off_u = rng.range_i32(0, 15) as f32;
                let off_v = rng.range_i32(0, 31) as f32;
                let n1 = if floors >= 8 && w.min(dep) > 5.5 {
                    ((floors as f32) * rng.range(0.4, 0.62)).round().max(3.0) as i32
                } else {
                    floors
                };
                let bid = lowers.len();
                lowers.push(LowerBlock {
                    style,
                    x0: a,
                    x1: b,
                    z0: c,
                    z1: d,
                    h: n1 as f32 * FLOOR_H,
                    tint,
                    off_u,
                    off_v,
                    notched: false,
                });

                let mut tx = lx;
                let mut tz = lz;
                let mut tw = w;
                let mut td = dep;
                if n1 < floors {
                    add_rooftop(&mut rng, &mut roofs, &mut clutter, &mut beacon_positions, lx, lz, w, dep, n1 as f32 * FLOOR_H, 0);
                    tw = w * rng.range(0.55, 0.75);
                    td = dep * rng.range(0.55, 0.75);
                    tx = lx + rng.range(-1.0, 1.0) * (w - tw) * 0.5;
                    tz = lz + rng.range(-1.0, 1.0) * (dep - td) * 0.5;
                    facade_builders[style].push_facade_box(
                        tx,
                        tz,
                        tw,
                        td,
                        n1 as f32 * FLOOR_H,
                        (floors - n1) as f32 * FLOOR_H,
                        tint,
                        off_u,
                        off_v,
                        BAY_W,
                        FLOOR_H,
                    );
                }
                add_rooftop(&mut rng, &mut roofs, &mut clutter, &mut beacon_positions, tx, tz, tw, td, floors as f32 * FLOOR_H, floors);

                let faces_px = (b - bx1).abs() < 0.05;
                let faces_pz = (d - bz1).abs() < 0.05;
                if faces_px {
                    let plain = if dep >= 3.0 && rng.next_f32() >= 0.35 {
                        Some(rng.range_i32(0, 7) as usize)
                    } else {
                        None
                    };
                    candidates.push(CandidateFront {
                        front: ShopFront { dir: ShopDir::PosX, x: b, z: lz, len: dep, deep: w },
                        bid,
                        plain_slot: plain,
                        is_store: false,
                    });
                }
                if faces_pz {
                    let plain = if w >= 3.0 && rng.next_f32() >= 0.35 {
                        Some(rng.range_i32(0, 7) as usize)
                    } else {
                        None
                    };
                    candidates.push(CandidateFront {
                        front: ShopFront { dir: ShopDir::PosZ, x: lx, z: d, len: w, deep: dep },
                        bid,
                        plain_slot: plain,
                        is_store: false,
                    });
                }
            }
        }
    }

    // 6. Pick the 12 nearest long street fronts (one per building) for walk-in Vietnamese shops
    //    and notch their lower ground-floor blocks so you can look/walk straight inside!
    let mut ranked: Vec<usize> = candidates
        .iter()
        .enumerate()
        .filter(|(_, c)| c.front.len >= 7.2 && c.front.deep >= 5.5)
        .map(|(idx, _)| idx)
        .collect();
    ranked.sort_by(|&a, &b| {
        let fa = &candidates[a].front;
        let fb = &candidates[b].front;
        let da = (fa.x - office_mid.x).hypot(fa.z - office_mid.y);
        let db = (fb.x - office_mid.x).hypot(fb.z - office_mid.y);
        da.partial_cmp(&db).unwrap_or(std::cmp::Ordering::Equal)
    });

    let mut used_bids = HashSet::new();
    let mut shop_fronts = Vec::new();
    for idx in ranked {
        let bid = candidates[idx].bid;
        if used_bids.contains(&bid) {
            continue;
        }
        used_bids.insert(bid);
        candidates[idx].is_store = true;
        let front = candidates[idx].front.clone();
        let b = &mut lowers[bid];
        b.notched = true;
        let d_cut = shop_depth(front.deep);
        let hw = SHOP_W * 0.5;
        let mut emit_piece = |a: f32, bb: f32, c: f32, dd: f32, y0: f32, h: f32| {
            if bb - a > 0.08 && dd - c > 0.08 && h > 0.08 {
                facade_builders[b.style].push_facade_box(
                    (a + bb) * 0.5,
                    (c + dd) * 0.5,
                    bb - a,
                    dd - c,
                    y0,
                    h,
                    b.tint,
                    b.off_u,
                    b.off_v,
                    BAY_W,
                    FLOOR_H,
                );
            }
        };
        // Upper floors above the shop room
        emit_piece(b.x0, b.x1, b.z0, b.z1, SHOP_CEIL, b.h - SHOP_CEIL);
        match front.dir {
            ShopDir::PosX => {
                let nx = b.x1 - d_cut;
                emit_piece(b.x0, nx, b.z0, b.z1, 0.0, SHOP_CEIL);
                emit_piece(nx, b.x1, b.z0, front.z - hw, 0.0, SHOP_CEIL);
                emit_piece(nx, b.x1, front.z + hw, b.z1, 0.0, SHOP_CEIL);
            }
            ShopDir::PosZ => {
                let nz = b.z1 - d_cut;
                emit_piece(b.x0, b.x1, b.z0, nz, 0.0, SHOP_CEIL);
                emit_piece(b.x0, front.x - hw, nz, b.z1, 0.0, SHOP_CEIL);
                emit_piece(front.x + hw, b.x1, nz, b.z1, 0.0, SHOP_CEIL);
            }
        }
        shop_fronts.push(front);
        if shop_fronts.len() >= SHOPS_COUNT {
            break;
        }
    }

    // Emit un-notched lower blocks
    for b in &lowers {
        if !b.notched {
            facade_builders[b.style].push_facade_box(
                (b.x0 + b.x1) * 0.5,
                (b.z0 + b.z1) * 0.5,
                b.x1 - b.x0,
                b.z1 - b.z0,
                0.0,
                b.h,
                b.tint,
                b.off_u,
                b.off_v,
                BAY_W,
                FLOOR_H,
            );
        }
    }

    // Emit generic neighborhood shop fronts on non-chain buildings
    let mut plain_fronts = Vec::new();
    for c in &candidates {
        if c.is_store {
            continue;
        }
        if let Some(slot) = c.plain_slot {
            let u0 = slot as f32 / 8.0;
            let u1 = (slot + 1) as f32 / 8.0;
            let w = (c.front.len - 0.6).max(1.8);
            let h = FLOOR_H * 0.82;
            let yb = GY + 0.12;
            let yt = yb + h;
            let col = [1.0, 1.0, 1.0, 1.0];
            match c.front.dir {
                ShopDir::PosX => {
                    let x = c.front.x + 0.03;
                    plain_shops.push_quad(
                        Vec3::new(x, yb, c.front.z + w * 0.5),
                        Vec3::new(x, yb, c.front.z - w * 0.5),
                        Vec3::new(x, yt, c.front.z - w * 0.5),
                        Vec3::new(x, yt, c.front.z + w * 0.5),
                        Vec3::X,
                        [[u0, 1.0], [u1, 1.0], [u1, 0.0], [u0, 0.0]],
                        col,
                    );
                }
                ShopDir::PosZ => {
                    let z = c.front.z + 0.03;
                    plain_shops.push_quad(
                        Vec3::new(c.front.x - w * 0.5, yb, z),
                        Vec3::new(c.front.x + w * 0.5, yb, z),
                        Vec3::new(c.front.x + w * 0.5, yt, z),
                        Vec3::new(c.front.x - w * 0.5, yt, z),
                        Vec3::Z,
                        [[u0, 1.0], [u1, 1.0], [u1, 0.0], [u0, 0.0]],
                        col,
                    );
                }
            }
            plain_fronts.push(c.front.clone());
        }
    }

    // Office superblock sidewalk slab
    slabs.push_box(
        Vec3::new((sb.x0 + sb.x1) * 0.5, GY + 0.06, (sb.z0 + sb.z1) * 0.5),
        Vec3::new(sb.x1 - sb.x0, 0.12, sb.z1 - sb.z0),
        "#8D939B",
    );

    // 7. Lane dashes + zebra crosswalks + stop lines
    let z_min = zs[0] - 6.0;
    let z_max = zs[zs.len() - 1] + 6.0;
    let x_min = xs[0] - 6.0;
    let x_max = xs[xs.len() - 1] + 6.0;

    for &x in &xs {
        let mut z = z_min;
        while z < z_max {
            if !sb.contains(x, z) {
                lanes.push_box(Vec3::new(x, GY + 0.012, z), Vec3::new(0.12, 0.01, 1.4), "#9EA4AD");
            }
            z += 3.0;
        }
    }
    for &z in &zs {
        let mut x = x_min;
        while x < x_max {
            if !sb.contains(x, z) {
                lanes.push_box(Vec3::new(x, GY + 0.012, z), Vec3::new(1.4, 0.01, 0.12), "#9EA4AD");
            }
            x += 3.0;
        }
    }

    // Zebra crosswalks & traffic signals at intersections near the office
    let mut signal_poles: Vec<SignalPole> = Vec::new();
    for (i, &ix) in xs.iter().enumerate() {
        for (j, &jz) in zs.iter().enumerate() {
            if sb.contains(ix, jz) {
                continue;
            }
            let dist = (ix - office_mid.x).hypot(jz - office_mid.y);
            if dist > 46.0 {
                continue;
            }
            // Zebra stripes on all 4 arms of intersection
            for s in [-1.0_f32, 1.0] {
                let arm_x = ix + s * (half + 1.3);
                if !sb.contains(arm_x, jz) {
                    for k in -2..=2 {
                        lanes.push_box(
                            Vec3::new(arm_x, GY + 0.015, jz + k as f32 * 0.72),
                            Vec3::new(1.6, 0.01, 0.34),
                            "#E4E6E8",
                        );
                    }
                    // Stop line
                    lanes.push_box(
                        Vec3::new(ix + s * (half + 2.65), GY + 0.015, jz + s * 1.1),
                        Vec3::new(0.22, 0.01, 1.9),
                        "#F0F2F4",
                    );
                }
                let arm_z = jz + s * (half + 1.3);
                if !sb.contains(ix, arm_z) {
                    for k in -2..=2 {
                        lanes.push_box(
                            Vec3::new(ix + k as f32 * 0.72, GY + 0.015, arm_z),
                            Vec3::new(0.34, 0.01, 1.6),
                            "#E4E6E8",
                        );
                    }
                    lanes.push_box(
                        Vec3::new(ix - s * 1.1, GY + 0.015, jz + s * (half + 2.65)),
                        Vec3::new(1.9, 0.01, 0.22),
                        "#F0F2F4",
                    );
                }
            }
            // Signal poles on 2 opposite corners near the office (clean, readable, low entity count)
            if dist < 34.0 {
                let corners = [
                    (ix + half + 0.45, jz + half + 0.45, true, -1.0_f32, 0.0_f32),
                    (ix - half - 0.45, jz - half - 0.45, false, 0.0_f32, 1.0_f32),
                ];
                for (px, pz, axis_x, fx, fz) in corners {
                    if !sb.contains(px, pz)
                        && !rect_contains_padded(terrace_excl, px, pz, 0.0)
                        && !on_court(px, pz, 0.6)
                    {
                        signal_poles.push(SignalPole {
                            x: px,
                            z: pz,
                            i,
                            j,
                            axis_x,
                            fx,
                            fz,
                        });
                    }
                }
            }
        }
    }

    // 8. Streetlamps every 9m on alternating sidewalks + night light pools (`city.js:264-273`)
    let mut lamp_positions: Vec<[f32; 2]> = Vec::new();
    let mut add_lamp = |x: f32, z: f32, ax: f32, az: f32| {
        if rect_contains_padded(terrace_excl, x, z, 0.0) || on_court(x, z, 0.3) {
            return;
        }
        lamp_positions.push([x, z]);
        lamp_geo.push_cyl(Vec3::new(x, GY + 1.9, z), 0.06, 0.08, 3.8, 6, "#2B2F36");
        lamp_geo.push_box(
            Vec3::new(x + ax * 0.45, GY + 3.75, z + az * 0.45),
            Vec3::new(if ax != 0.0 { 1.0 } else { 0.08 }, 0.07, if az != 0.0 { 1.0 } else { 0.08 }),
            "#2B2F36",
        );
        bulb_geo.push_box(
            Vec3::new(x + ax * 0.85, GY + 3.68, z + az * 0.85),
            Vec3::new(if ax != 0.0 { 0.4 } else { 0.18 }, 0.06, if az != 0.0 { 0.4 } else { 0.18 }),
            "#FFFFFF",
        );
        pool_geo.push_flat(Vec3::new(x + ax * 1.0, GY + 0.145, z + az * 1.0), 4.8, 4.8, "#FFFFFF");
    };
    for &x in &xs {
        let mut z = z_min + 4.0;
        let mut k = 0usize;
        while z < z_max {
            if !sb.contains(x, z) && (x - office_mid.x).hypot(z - office_mid.y) < 58.0 {
                if k % 2 == 1 {
                    add_lamp(x - half - 0.5, z, 1.0, 0.0);
                } else {
                    add_lamp(x + half + 0.5, z, -1.0, 0.0);
                }
            }
            z += 9.0;
            k += 1;
        }
    }
    for &z in &zs {
        let mut x = x_min + 4.0;
        let mut k = 0usize;
        while x < x_max {
            if !sb.contains(x, z) && (x - office_mid.x).hypot(z - office_mid.y) < 58.0 {
                if k % 2 == 1 {
                    add_lamp(x, z - half - 0.5, 0.0, 1.0);
                } else {
                    add_lamp(x, z + half + 0.5, 0.0, -1.0);
                }
            }
            x += 9.0;
            k += 1;
        }
    }

    // 9. Concrete utility poles, cross-arms, insulators, transformer box & sagging cables (`street-wires.js`)
    let mut wires = StreetMeshBuilder::default();
    let mut wrng = CityRng::new(7);
    let pole_h = 6.4;
    let d_pole = half + 0.32;
    let mut has_transformer = false;
    for axis_z in [true, false] {
        let (lines, cross) = if axis_z { (&xs, &zs) } else { (&zs, &xs) };
        let across = if axis_z { Vec2::new(1.0, 0.0) } else { Vec2::new(0.0, 1.0) };
        for &v in lines {
            let c_coord = if axis_z { office_mid.x } else { office_mid.y };
            if (v - c_coord).abs() > 38.0 {
                continue;
            }
            let mid_along = if axis_z { office_mid.y } else { office_mid.x };
            let plus_inside_sb = if axis_z {
                v + d_pole > sb.x0 && v + d_pole < sb.x1
            } else {
                v + d_pole > sb.z0 && v + d_pole < sb.z1
            };
            let side = if plus_inside_sb { -1.0_f32 } else { 1.0_f32 };
            let w = v + side * d_pole;
            let mut poles: Vec<(f32, f32, f32)> = Vec::new();
            for k in 0..cross.len() - 1 {
                for a in [cross[k] + 4.7, cross[k + 1] - 4.7] {
                    if (a - mid_along).abs() > 38.0 {
                        continue;
                    }
                    let (px, pz) = if axis_z { (w, a) } else { (a, w) };
                    let (sx, sz) = if axis_z { (v, a) } else { (a, v) };
                    if sb.contains(px, pz)
                        || sb.contains(sx, sz)
                        || rect_contains_padded(terrace_excl, px, pz, 0.0)
                        || on_court(px, pz, 0.8)
                    {
                        continue;
                    }
                    poles.push((px, pz, a));
                }
            }
            for &(px, pz, _) in &poles {
                wires.push_cyl_rot(
                    Vec3::new(px, GY + 0.12 + pole_h * 0.5, pz),
                    0.075,
                    0.12,
                    pole_h,
                    4,
                    Quat::from_rotation_y(FRAC_PI_4),
                    "#9A9C9A",
                );
                wires.push_box(
                    Vec3::new(px - across.x * side * 0.25, GY + 0.12 + pole_h - 0.5, pz - across.y * side * 0.25),
                    Vec3::new(if across.x != 0.0 { 1.0 } else { 0.08 }, 0.08, if across.y != 0.0 { 1.0 } else { 0.08 }),
                    "#8A8C8A",
                );
                for o in [-0.5_f32, -0.15, 0.2] {
                    wires.push_box(
                        Vec3::new(px + across.x * side * o, GY + 0.12 + pole_h - 0.38, pz + across.y * side * o),
                        Vec3::new(0.06, 0.12, 0.06),
                        "#E8E4DA",
                    );
                }
                // Spare coiled cable loop hanging on pole
                if wrng.next_f32() < 0.45 {
                    let cy = GY + 0.12 + pole_h - 1.35;
                    let cx = px + across.x * side * 0.14;
                    let cz = pz + across.y * side * 0.14;
                    let r_coil = 0.22;
                    for s in 0..8 {
                        let a0 = (s as f32 / 8.0) * PI * 2.0;
                        let a1 = ((s + 1) as f32 / 8.0) * PI * 2.0;
                        let p0 = Vec3::new(cx + across.y * a0.cos() * r_coil, cy + a0.sin() * r_coil, cz + across.x * a0.cos() * r_coil);
                        let p1 = Vec3::new(cx + across.y * a1.cos() * r_coil, cy + a1.sin() * r_coil, cz + across.x * a1.cos() * r_coil);
                        wires.push_bar(p0, p1, 0.022, 4, "#1A1C20");
                    }
                }
                // Pole-mounted transformer + meter box near the office
                if !has_transformer && (px - office_mid.x).hypot(pz - office_mid.y) < 24.0 {
                    has_transformer = true;
                    let tx = px - across.x * side * 0.25;
                    let tz = pz - across.y * side * 0.25;
                    wires.push_box(Vec3::new(tx, GY + 0.12 + pole_h - 1.9, tz), Vec3::new(0.42, 0.62, 0.32), "#7D8A86");
                    wires.push_box(Vec3::new(tx, GY + 0.12 + pole_h - 1.56, tz), Vec3::new(0.46, 0.06, 0.36), "#5E6670");
                    wires.push_box(Vec3::new(tx + across.y * 0.17, GY + 1.72, tz + across.x * 0.17), Vec3::new(0.3, 0.42, 0.04), "#B9BDB8");
                }
            }
            poles.sort_by(|a, b| a.2.partial_cmp(&b.2).unwrap_or(std::cmp::Ordering::Equal));
            for k in 1..poles.len() {
                let (ax, az, aa) = poles[k - 1];
                let (bx, bz, ba_a) = poles[k];
                let span = ba_a - aa;
                if span > 16.0 {
                    continue;
                }
                let cables = if wrng.next_f32() < 0.5 { 5 } else { 4 };
                for c in 0..cables {
                    let off = (c as f32 - (cables - 1) as f32 * 0.5) * 0.09;
                    let h0 = pole_h - 0.42 - (c % 3) as f32 * 0.13;
                    let sag = 0.30 + wrng.next_f32() * 0.35 + span * 0.02;
                    let segs = 6;
                    let mut prev: Option<Vec3> = None;
                    for s in 0..=segs {
                        let u = s as f32 / segs as f32;
                        let p = Vec3::new(
                            ax + (bx - ax) * u + across.x * off,
                            GY + 0.12 + h0 - 4.0 * sag * u * (1.0 - u),
                            az + (bz - az) * u + across.y * off,
                        );
                        if let Some(p0) = prev {
                            wires.push_bar(p0, p, 0.018, 4, "#1A1C20");
                        }
                        prev = Some(p);
                    }
                }
            }
        }
    }

    // 10. Spawn merged static meshes into BackdropAssets
    for (builder, mat) in facade_builders.into_iter().zip(style_mats) {
        if let Some(e) = builder.spawn(commands, meshes, ba, mat) {
            commands.entity(e).insert(CityNightFacade);
        }
    }
    let vmat_roof = ba.mat(materials, "#FFFFFF", 0.90, 0.0);
    roofs.spawn(commands, meshes, ba, vmat_roof);

    let vmat_clutter = ba.mat(materials, "#FFFFFF", 0.70, 0.15);
    clutter.spawn(commands, meshes, ba, vmat_clutter);

    let vmat_green = ba.mat(materials, "#FFFFFF", 0.95, 0.0);
    greens.spawn(commands, meshes, ba, vmat_green);

    let slab_mat = ba.add_material(
        materials,
        StandardMaterial {
            base_color: if night {
                Color::srgb(0.35, 0.35, 0.38)
            } else {
                Color::WHITE
            },
            perceptual_roughness: 0.92,
            ..default()
        },
    );
    if let Some(e) = slabs.spawn(commands, meshes, ba, slab_mat) {
        commands.entity(e).insert(CityNightSidewalk);
    }

    let lane_mat = ba.mat_unlit(materials, "#FFFFFF");
    lanes.spawn(commands, meshes, ba, lane_mat);

    let shop_strip_img = ba.upload_image(images, draw_generic_shop_strip());
    let em_shop = if night { 0.85 } else { 0.0 };
    let shop_strip_mat = ba.add_material(
        materials,
        StandardMaterial {
            base_color: Color::WHITE,
            base_color_texture: Some(shop_strip_img.clone()),
            emissive: LinearRgba::new(em_shop, em_shop, em_shop, 1.0),
            emissive_texture: Some(shop_strip_img),
            perceptual_roughness: 0.6,
            ..default()
        },
    );
    if let Some(e) = plain_shops.spawn(commands, meshes, ba, shop_strip_mat) {
        commands.entity(e).insert(CityNightFacade);
    }

    let vmat_lamp = ba.mat(materials, "#FFFFFF", 0.45, 0.5);
    lamp_geo.spawn(commands, meshes, ba, vmat_lamp);

    let bulb_mat = ba.add_material(
        materials,
        StandardMaterial {
            base_color: if night { srgb_hex("#FFE2A6") } else { srgb_hex("#5A5E66") },
            emissive: if night {
                LinearRgba::new(2.2, 1.65, 0.9, 1.0)
            } else {
                LinearRgba::BLACK
            },
            unlit: night,
            ..default()
        },
    );
    if let Some(e) = bulb_geo.spawn(commands, meshes, ba, bulb_mat) {
        commands.entity(e).insert(CityNightLampBulb);
    }

    let pool_img = ba.upload_image(images, draw_lamp_pool_tex());
    let pool_mat = ba.add_material(
        materials,
        StandardMaterial {
            base_color: Color::WHITE,
            base_color_texture: Some(pool_img),
            alpha_mode: AlphaMode::Add,
            unlit: true,
            ..default()
        },
    );
    if let Some(e) = pool_geo.spawn(commands, meshes, ba, pool_mat) {
        commands.entity(e).insert((
            CityNightLampPool,
            if night { Visibility::Inherited } else { Visibility::Hidden },
        ));
    }

    let vmat_wires = ba.mat(materials, "#FFFFFF", 0.85, 0.0);
    wires.spawn(commands, meshes, ba, vmat_wires);

    // 11. Spawn rooftop aviation beacons (pulsing red dots)
    if !beacon_positions.is_empty() {
        let beacon_mesh = ba.add_mesh(meshes, Mesh::from(Sphere::new(0.14)));
        let beacon_mat = ba.mat_emissive(materials, "#FF3024", 2.8);
        for (idx, pos) in beacon_positions.into_iter().enumerate() {
            commands.spawn((
                WorldEntity,
                BackdropEntity,
                RooftopBeacon { phase: idx as f32 * 0.37 },
                Mesh3d(beacon_mesh.clone()),
                MeshMaterial3d(beacon_mat.clone()),
                Transform::from_translation(pos),
            ));
        }
    }

    // 12. Spawn traffic signal poles, 3-color signal heads, and live second-by-second countdown LEDs (`street-signals.js`)
    if !signal_poles.is_empty() {
        let mut sig_bodies = StreetMeshBuilder::default();
        let lamp_mesh = ba.add_mesh(meshes, Mesh::from(Sphere::new(0.085)));
        let cd_atlas_mat = ba.textured(images, materials, draw_traffic_countdown_atlas(), true, 0.5);
        let cd_frames: [Handle<Mesh>; 22] = std::array::from_fn(|idx| {
            let ox = ((idx % 8) * 64) as f32;
            let oy = ((idx / 8) * 40) as f32;
            ba.atlas_quad(meshes, 512.0, 192.0, ox, oy, 64.0, 40.0)
        });

        let red_on = ba.mat_emissive(materials, "#FF3322", 2.4);
        let amber_on = ba.mat_emissive(materials, "#FFC422", 2.2);
        let green_on = ba.mat_emissive(materials, "#33FF66", 2.4);

        for p in &signal_poles {
            let yaw = p.fx.atan2(p.fz);
            let rot = Quat::from_rotation_y(yaw);
            // Pole + signal head housing + visor + countdown box
            sig_bodies.push_cyl(Vec3::new(p.x, GY + 1.85, p.z), 0.065, 0.08, 3.7, 8, "#2B2F36");
            let head_pos = Vec3::new(p.x, GY + 3.25, p.z) + rot * Vec3::new(0.0, 0.0, 0.14);
            sig_bodies.push_box_rot(head_pos, Vec3::new(0.26, 0.72, 0.22), rot, "#141518");
            // Countdown box to the right of signal head
            let cd_pos = Vec3::new(p.x, GY + 2.86, p.z) + rot * Vec3::new(0.32, 0.0, 0.14);
            sig_bodies.push_box_rot(cd_pos, Vec3::new(0.38, 0.26, 0.14), rot, "#141518");
            sig_bodies.push_box_rot(cd_pos + rot * Vec3::new(0.0, 0.13, 0.06), Vec3::new(0.40, 0.02, 0.12), rot, "#1C1E22");

            // 3 lamps (Red top, Amber mid, Green bottom)
            for (dy, col, mat) in [
                (0.21_f32, SignalColor::Red, red_on.clone()),
                (0.0, SignalColor::Amber, amber_on.clone()),
                (-0.21, SignalColor::Green, green_on.clone()),
            ] {
                let lp = head_pos + rot * Vec3::new(0.0, dy, 0.11);
                commands.spawn((
                    WorldEntity,
                    BackdropEntity,
                    TrafficSignalLamp {
                        i: p.i,
                        j: p.j,
                        axis_x: p.axis_x,
                        color: col,
                    },
                    Mesh3d(lamp_mesh.clone()),
                    MeshMaterial3d(mat),
                    Transform::from_translation(lp),
                ));
            }
            // Countdown LED quad swapped second-by-second across the 22 prebuilt UV-cropped frames
            let init_frame = (signal_phase(p.i, p.j, 0.0, p.axis_x).floor() as usize).min(21);
            commands.spawn((
                WorldEntity,
                BackdropEntity,
                TrafficCountdownDisplay {
                    i: p.i,
                    j: p.j,
                    axis_x: p.axis_x,
                    last_frame: init_frame as u8,
                    frames: cd_frames.clone(),
                },
                Mesh3d(cd_frames[init_frame].clone()),
                MeshMaterial3d(cd_atlas_mat.clone()),
                Transform::from_translation(cd_pos + rot * Vec3::new(0.0, 0.0, 0.075))
                    .with_rotation(rot)
                    .with_scale(Vec3::new(0.32, 0.20, 1.0)),
            ));
        }

        let sig_mat = ba.mat(materials, "#FFFFFF", 0.55, 0.25);
        sig_bodies.spawn(commands, meshes, ba, sig_mat);
    }

    // Reserve office_mid helper usage for clarity
    let _ = (FRAC_PI_2, linear_rgba("#FFFFFF"));

    CityGridData {
        layout: spec.layout,
        terrace_excl,
        xs,
        zs,
        sb,
        office_mid,
        shop_fronts,
        plain_fronts,
        parks,
        plazas,
        trees,
        solids,
        pickle,
        signal_poles,
        lamp_positions,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::backdrops::BackdropEntity;
    use crate::layout::OfficeLayout;

    #[test]
    fn city_grid_spawns_supergrid_shop_notches_wires_signals_and_purges_cleanly() {
        let spec = LayoutSpec::for_layout(OfficeLayout::Pro);
        let mut world = World::new();
        world.insert_resource(Assets::<Mesh>::default());
        world.insert_resource(Assets::<StandardMaterial>::default());
        world.insert_resource(Assets::<Image>::default());
        world.insert_resource(BackdropAssets::default());

        let grid = world.resource_scope(|world, mut meshes: Mut<Assets<Mesh>>| {
            world.resource_scope(|world, mut mats: Mut<Assets<StandardMaterial>>| {
                world.resource_scope(|world, mut imgs: Mut<Assets<Image>>| {
                    world.resource_scope(|world, mut ba: Mut<BackdropAssets>| {
                        let mut cmds = world.commands();
                        spawn_city_grid(&mut cmds, &mut meshes, &mut mats, &mut imgs, &mut ba, &spec, false)
                    })
                })
            })
        });
        world.flush();

        assert!(grid.shop_fronts.len() >= 10, "expected >= 10 shop fronts, got {}", grid.shop_fronts.len());
        assert!(!grid.parks.is_empty(), "city grid must leave open parks near office");
        assert!(!grid.plazas.is_empty(), "city grid must create paved park squares");
        assert!(!grid.signal_poles.is_empty(), "city grid must create traffic signals");
        assert!(grid.pickle.is_some(), "city grid must reserve a pickleball site");
        assert!(
            world.query_filtered::<Entity, With<BackdropEntity>>().iter(&world).count() >= 10
        );

        let ents: Vec<Entity> = world.query_filtered::<Entity, With<BackdropEntity>>().iter(&world).collect();
        for e in ents {
            world.despawn(e);
        }
        world.resource_scope(|world, mut ba: Mut<BackdropAssets>| {
            world.resource_scope(|world, mut meshes: Mut<Assets<Mesh>>| {
                world.resource_scope(|world, mut mats: Mut<Assets<StandardMaterial>>| {
                    let mut imgs = world.resource_mut::<Assets<Image>>();
                    ba.purge(&mut meshes, &mut mats, &mut imgs);
                });
            });
        });

        assert_eq!(world.resource::<Assets<Mesh>>().len(), 0);
        assert_eq!(world.resource::<Assets<StandardMaterial>>().len(), 0);
        assert_eq!(world.resource::<Assets<Image>>().len(), 0);
    }
}
