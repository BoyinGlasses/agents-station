//! 1:1 Vietnamese `Countryside` & `Beach` exterior backdrops ported from Golden Oracle
//! `src/office/world/backdrops/{countryside,beach,shared}.js`.
//!
//! Every `Mesh`, `StandardMaterial`, and `Image` (`RenderAssetUsages::RENDER_WORLD`) is registered
//! inside `BackdropAssets`, and every entity carries `WorldEntity + BackdropEntity` so backdrop
//! switching and `unload_full_office_world` leak 0 GPU/CPU assets.

use crate::backdrops::{BackdropAssets, BackdropEntity};
use crate::layout::LayoutSpec;
use crate::lights::LightingState;
use crate::palette::{WorldEntity, parse_hex_rgb, srgb_hex};
use crate::street::city_grid::{rect_contains_padded, terrace_exclusion};
use crate::street::{CityRng, GY, StreetMeshBuilder, linear_rgba};
use bevy::asset::RenderAssetUsages;
use bevy::ecs::system::SystemParam;
use bevy::prelude::*;
use bevy::render::mesh::{Indices, PrimitiveTopology, VertexAttributeValues};
use station_canvas2d::street_surfaces::draw_lamp_pool_tex;
use station_canvas2d::{Canvas2d, FontEngine, RgbaSurface, TextAlign};
use std::f32::consts::{FRAC_PI_2, FRAC_PI_4, PI, TAU};

pub const COUNTRY_FIREFLY_COUNT: usize = 260;
pub const BEACH_PALM_COUNT: usize = 70;

/* ---------- Animated & Night-Sync Components ---------- */

#[derive(Component)]
pub struct CountryGround;

#[derive(Component)]
pub struct CountryWater {
    pub scroll_speed: Vec2,
}

#[derive(Component)]
pub struct CountryVillageGate;

#[derive(Component)]
pub struct CountryBuffalo {
    pub head_pivot: Entity,
    pub phase: f32,
}

#[derive(Component)]
pub struct CountryFlyer {
    pub left_wing: Entity,
    pub right_wing: Entity,
    pub center: Vec2,
    pub radius: f32,
    pub altitude: f32,
    pub phase: f32,
    pub speed: f32,
    pub flap_freq: f32,
    pub flap_amp: f32,
}

#[derive(Component)]
pub struct CountryKite {
    pub hand_anchor: Vec3,
    pub base_pos: Vec3,
    pub phase: f32,
    pub string_entity: Entity,
}

#[derive(Component)]
pub struct CountryFireflies {
    pub mesh: Handle<Mesh>,
    pub base_positions: Vec<Vec3>,
    pub phases: Vec<f32>,
}

#[derive(Component)]
pub struct CountryNightLit;

#[derive(Component)]
pub struct CountryNightPool;

#[derive(Component)]
pub struct CountryLanternBulb;

#[derive(Component)]
pub struct BeachOcean;

#[derive(Component)]
pub struct BeachShore;

#[derive(Component)]
pub struct BeachShallow;

#[derive(Component)]
pub struct BeachWave {
    pub phase: f32,
}

#[derive(Component)]
pub struct BeachPalmMeta {
    pub count: usize,
}

#[derive(Component)]
pub struct BeachBoat {
    pub base_y: f32,
    pub phase: f32,
    pub spin: f32,
}

#[derive(Component)]
pub struct BeachBoatLight;

#[derive(Component)]
pub struct BeachHutLights;

#[derive(Component)]
pub struct BeachLighthouseLamp;

#[derive(Component)]
pub struct LighthouseBeam;

#[derive(Component)]
pub struct BeachMoonGlint;

#[derive(Component)]
pub struct BeachSeagull;

/* ---------- Shared Screen-Space Placement Helpers (`shared.js`) ---------- */

#[inline]
pub fn sd_to_xz(s: f32, d: f32) -> (f32, f32) {
    ((d + s) * 0.5, (d - s) * 0.5)
}

#[inline]
fn river_d(s: f32) -> f32 {
    -31.0 + 6.0 * (s / 17.0).sin() + 2.0 * (s / 7.0).sin()
}

#[inline]
fn shore_d(s: f32) -> f32 {
    -27.0 + 2.5 * (s / 15.0).sin() + 1.2 * (s / 6.0).sin()
}

#[inline]
fn near_office(excl: [f32; 4], x: f32, z: f32, pad: f32) -> bool {
    rect_contains_padded(excl, x, z, pad)
}

#[inline]
fn tall_ok(excl: [f32; 4], x: f32, z: f32, h: f32) -> bool {
    if near_office(excl, x, z, 2.5) {
        return false;
    }
    let d = x + z;
    let s = x - z;
    d < -20.0
        || (s.abs() > 27.0 && d < 14.0)
        || (d >= 14.0 && s.abs() > 21.0 && h < 4.0)
        || (d > 26.0 && h < 2.5)
}

/// Catmull-Rom spline sampling matching `shared.js:32` (`curvePts`).
fn sample_catmull_rom(ctrl: &[(f32, f32)], step: f32) -> Vec<(f32, f32)> {
    if ctrl.len() < 2 {
        return ctrl.to_vec();
    }
    let n = ctrl.len();
    let mut approx_len = 0.0_f32;
    for i in 0..n - 1 {
        approx_len += (ctrl[i + 1].0 - ctrl[i].0).hypot(ctrl[i + 1].1 - ctrl[i].1);
    }
    let total_steps = ((approx_len / step).round() as usize).max(8);
    let segs = n - 1;
    let mut out = Vec::with_capacity(total_steps + 1);
    for k in 0..=total_steps {
        let u = (k as f32 / total_steps as f32) * segs as f32;
        let i = (u.floor() as usize).min(segs - 1);
        let t = (u - i as f32).clamp(0.0, 1.0);
        let p0 = ctrl[i.saturating_sub(1)];
        let p1 = ctrl[i];
        let p2 = ctrl[(i + 1).min(n - 1)];
        let p3 = ctrl[(i + 2).min(n - 1)];
        let t2 = t * t;
        let t3 = t2 * t;
        let cat = |a: f32, b: f32, c: f32, d: f32| {
            0.5 * ((2.0 * b)
                + (-a + c) * t
                + (2.0 * a - 5.0 * b + 4.0 * c - d) * t2
                + (-a + 3.0 * b - 3.0 * c + d) * t3)
        };
        out.push((cat(p0.0, p1.0, p2.0, p3.0), cat(p0.1, p1.1, p2.1, p3.1)));
    }
    out
}

fn dist_to_polyline(x: f32, z: f32, pts: &[(f32, f32)]) -> f32 {
    let mut best = f32::INFINITY;
    for w in pts.windows(2) {
        let (ax, az) = w[0];
        let (bx, bz) = w[1];
        let dx = bx - ax;
        let dz = bz - az;
        let l2 = dx * dx + dz * dz;
        let t = if l2 > 1e-6 {
            (((x - ax) * dx + (z - az) * dz) / l2).clamp(0.0, 1.0)
        } else {
            0.0
        };
        best = best.min((x - ax - t * dx).hypot(z - az - t * dz));
    }
    best
}

/* ---------- Procedural Surface Generators (CPU -> RENDER_WORLD GPU) ---------- */

fn draw_water_tex(base_hex: &str, ripple_hex: &str, seed: u32) -> RgbaSurface {
    let mut c = Canvas2d::new(256, 256);
    let (br, bg, bb) = parse_hex_rgb(base_hex);
    let (rr, rg, rb) = parse_hex_rgb(ripple_hex);
    c.fill([br, bg, bb, 255]);
    let mut rng = CityRng::new(seed);
    for _ in 0..42 {
        let x0 = rng.range(0.0, 256.0);
        let y0 = rng.range(0.0, 256.0);
        let w = rng.range(16.0, 42.0);
        let alpha = (rng.range(0.14, 0.36) * 255.0) as u8;
        let col = [rr, rg, rb, alpha];
        for s in 0..10 {
            let t0 = s as f32 / 10.0;
            let t1 = (s + 1) as f32 / 10.0;
            let px0 = (x0 + t0 * w).rem_euclid(256.0);
            let px1 = (x0 + t1 * w).rem_euclid(256.0);
            let py0 = y0 - (t0 * PI).sin() * 2.5;
            let py1 = y0 - (t1 * PI).sin() * 2.5;
            if (px1 - px0).abs() < 48.0 {
                c.stroke_line(px0, py0, px1, py1, 1.5, col);
            }
        }
    }
    c.into_surface()
}

fn draw_field_furrow_tex() -> RgbaSurface {
    let mut c = Canvas2d::new(64, 64);
    c.fill([255, 255, 255, 255]);
    for x in (0..64).step_by(8) {
        c.fill_rect(x as f32, 0.0, 3.0, 64.0, [214, 214, 214, 255]);
    }
    c.into_surface()
}

fn draw_village_gate_sign(fonts: &mut FontEngine) -> RgbaSurface {
    let mut c = Canvas2d::new(256, 64);
    c.fill([168, 42, 34, 255]);
    c.fill_rect(4.0, 4.0, 248.0, 56.0, [218, 178, 72, 255]);
    c.fill_rect(8.0, 8.0, 240.0, 48.0, [148, 34, 28, 255]);
    fonts.draw_text(
        &mut c,
        "LÀNG VĂN HÓA",
        128.0,
        20.0,
        24.0,
        [252, 232, 168, 255],
        TextAlign::Center,
        Some(228.0),
        true,
        false,
    );
    c.into_surface()
}

fn draw_foam_grad_tex() -> RgbaSurface {
    let mut rgba = vec![0u8; 64 * 32 * 4];
    for y in 0..32 {
        let v = y as f32 / 31.0;
        let a = if v < 0.55 {
            (v / 0.55) * 0.92
        } else {
            ((1.0 - v) / 0.45) * 0.92
        };
        let ab = (a.clamp(0.0, 1.0) * 255.0) as u8;
        for x in 0..64 {
            let ripple = ((x as f32 * 0.35).sin() * 0.08 + 0.92).clamp(0.0, 1.0);
            let idx = (y * 64 + x) * 4;
            rgba[idx] = 255;
            rgba[idx + 1] = 255;
            rgba[idx + 2] = 255;
            rgba[idx + 3] = (ab as f32 * ripple) as u8;
        }
    }
    RgbaSurface {
        width: 64,
        height: 32,
        rgba,
    }
}

fn draw_beam_grad_tex() -> RgbaSurface {
    let mut rgba = vec![0u8; 16 * 128 * 4];
    for y in 0..128 {
        let t = y as f32 / 127.0;
        let alpha = ((1.0 - t).powf(1.35) * 0.68 * 255.0) as u8;
        for x in 0..16 {
            let idx = (y * 16 + x) * 4;
            rgba[idx] = 255;
            rgba[idx + 1] = 246;
            rgba[idx + 2] = 208;
            rgba[idx + 3] = alpha;
        }
    }
    RgbaSurface {
        width: 16,
        height: 128,
        rgba,
    }
}

fn draw_moon_glint_tex() -> RgbaSurface {
    let mut c = Canvas2d::new(64, 256);
    c.fill([0, 0, 0, 0]);
    let mut rng = CityRng::new(9911);
    for _ in 0..220 {
        let y = rng.range(0.0, 256.0);
        let spread = 6.0 + (y / 256.0) * 24.0;
        let x = 32.0 + rng.range(-1.0, 1.0) * spread * rng.next_f32();
        let w = rng.range(3.0, 10.0);
        let a = (rng.range(0.24, 0.88) * (0.4 + y / 450.0)).clamp(0.0, 1.0);
        c.fill_rect(x - w * 0.5, y, w, 1.8, [255, 246, 214, (a * 255.0) as u8]);
    }
    c.into_surface()
}

/* ---------- Outward-Wound Procedural Mesh Builders ---------- */

fn build_ribbon_mesh(pts: &[(f32, f32)], width: f32, y: f32, hex: &str) -> Mesh {
    let n = pts.len();
    let col = linear_rgba(hex);
    let mut positions = Vec::with_capacity(n * 2);
    let mut normals = Vec::with_capacity(n * 2);
    let mut uvs = Vec::with_capacity(n * 2);
    let mut colors = Vec::with_capacity(n * 2);
    let mut indices = Vec::with_capacity(n.saturating_sub(1) * 6);

    let mut len = 0.0_f32;
    for i in 0..n {
        let (x, z) = pts[i];
        let (px, pz) = pts[i.saturating_sub(1)];
        let (nx, nz) = pts[(i + 1).min(n - 1)];
        let mut tx = nx - px;
        let mut tz = nz - pz;
        let tl = tx.hypot(tz).max(1e-5);
        tx /= tl;
        tz /= tl;
        if i > 0 {
            len += (x - pts[i - 1].0).hypot(z - pts[i - 1].1);
        }
        for &k in &[-1.0_f32, 1.0] {
            positions.push([x - tz * k * width * 0.5, y, z + tx * k * width * 0.5]);
            normals.push([0.0, 1.0, 0.0]);
            uvs.push([(k + 1.0) * 0.5, len / width]);
            colors.push(col);
        }
    }
    for i in 0..n.saturating_sub(1) {
        let a = (i * 2) as u32;
        indices.extend_from_slice(&[a, a + 1, a + 2, a + 1, a + 3, a + 2]);
    }

    let mut mesh = Mesh::new(
        PrimitiveTopology::TriangleList,
        RenderAssetUsages::MAIN_WORLD | RenderAssetUsages::RENDER_WORLD,
    );
    mesh.insert_attribute(Mesh::ATTRIBUTE_POSITION, positions);
    mesh.insert_attribute(Mesh::ATTRIBUTE_NORMAL, normals);
    mesh.insert_attribute(Mesh::ATTRIBUTE_UV_0, uvs);
    mesh.insert_attribute(Mesh::ATTRIBUTE_COLOR, colors);
    mesh.insert_indices(Indices::U32(indices));
    mesh
}

fn push_ribbon_into(b: &mut StreetMeshBuilder, pts: &[(f32, f32)], width: f32, y: f32, hex: &str) {
    if pts.len() < 2 {
        return;
    }
    let col = linear_rgba(hex);
    let mut prev_l = Vec3::ZERO;
    let mut prev_r = Vec3::ZERO;
    for i in 0..pts.len() {
        let (x, z) = pts[i];
        let (px, pz) = pts[i.saturating_sub(1)];
        let (nx, nz) = pts[(i + 1).min(pts.len() - 1)];
        let mut tx = nx - px;
        let mut tz = nz - pz;
        let tl = tx.hypot(tz).max(1e-5);
        tx /= tl;
        tz /= tl;
        let l = Vec3::new(x + tz * width * 0.5, y, z - tx * width * 0.5);
        let r = Vec3::new(x - tz * width * 0.5, y, z + tx * width * 0.5);
        if i > 0 {
            b.push_quad(
                prev_l,
                prev_r,
                r,
                l,
                Vec3::Y,
                [[0.0, 0.0], [1.0, 0.0], [1.0, 1.0], [0.0, 1.0]],
                col,
            );
        }
        prev_l = l;
        prev_r = r;
    }
}

/// Outward-wound triangular gable prism (ridge along local Z of length `d`, base width `w` along X, height `h` along Y).
fn push_prism(b: &mut StreetMeshBuilder, center: Vec3, w: f32, h: f32, d: f32, rot: Quat, hex: &str) {
    let col = linear_rgba(hex);
    let hw = w * 0.5;
    let hd = d * 0.5;
    let xf = |p: Vec3| center + rot * p;
    let fl = xf(Vec3::new(-hw, 0.0, hd));
    let fr = xf(Vec3::new(hw, 0.0, hd));
    let ft = xf(Vec3::new(0.0, h, hd));
    let bl = xf(Vec3::new(-hw, 0.0, -hd));
    let br = xf(Vec3::new(hw, 0.0, -hd));
    let bt = xf(Vec3::new(0.0, h, -hd));
    let uv = [[0.0, 0.0], [1.0, 0.0], [1.0, 1.0], [0.0, 1.0]];

    // Right slope (+X/+Y)
    let rn = (rot * Vec3::new(h, hw, 0.0)).normalize_or_zero();
    b.push_quad(fr, br, bt, ft, rn, uv, col);
    // Left slope (-X/+Y)
    let ln = (rot * Vec3::new(-h, hw, 0.0)).normalize_or_zero();
    b.push_quad(bl, fl, ft, bt, ln, uv, col);
    // Front gable (+Z) degenerate quad -> triangle
    let fnrm = (rot * Vec3::Z).normalize_or_zero();
    b.push_quad(fl, fr, ft, ft, fnrm, uv, col);
    // Back gable (-Z)
    let bnrm = (rot * Vec3::NEG_Z).normalize_or_zero();
    b.push_quad(br, bl, bt, bt, bnrm, uv, col);
}

/// Outward-wound organic limestone karst hill (`shared.js:58-64`).
fn push_karst(b: &mut StreetMeshBuilder, x: f32, z: f32, r: f32, h: f32, hex: &str) {
    // Base organic karst dome + cap
    b.push_sphere(Vec3::new(x, GY - h * 0.08, z), Vec3::new(r, h, r * 0.92), hex);
    b.push_sphere(
        Vec3::new(x + r * 0.22, GY + h * 0.45, z - r * 0.14),
        Vec3::new(r * 0.65, h * 0.58, r * 0.65),
        hex,
    );
    b.push_sphere(
        Vec3::new(x + r * 0.14, GY + h * 0.78, z),
        Vec3::new(r * 0.46, h * 0.24, r * 0.46),
        "#6E9455",
    );
}

/// Outward-wound round village tree (`shared.js:38-43`).
fn push_round_tree(b: &mut StreetMeshBuilder, rng: &mut CityRng, x: f32, z: f32, s: f32) {
    b.push_cyl(Vec3::new(x, GY + 0.8 * s, z), 0.14 * s, 0.20 * s, 1.6 * s, 6, "#6A4E36");
    let c = *rng.pick(&["#4E7F3B", "#5B8C43", "#46743A", "#6A9A4C"]);
    b.push_sphere(Vec3::new(x, GY + 2.2 * s, z), Vec3::splat(1.1 * s), c);
    b.push_sphere(
        Vec3::new(x + 0.5 * s, GY + 2.8 * s, z - 0.3 * s),
        Vec3::splat(0.8 * s),
        c,
    );
}

/// Outward-wound curving coconut palm with 7 drooping fronds & coconuts (`shared.js:44-57`).
fn push_coconut_palm(b: &mut StreetMeshBuilder, rng: &mut CityRng, x: f32, z: f32, s: f32) {
    let lean = rng.range(-0.35, 0.35);
    let dir = rng.range(0.0, TAU);
    let mut px = x;
    let mut pz = z;
    let mut py = GY;
    for i in 0..7 {
        let frac = i as f32 / 6.0;
        let nx = px + dir.cos() * lean * 0.35 * s * frac;
        let nz = pz + dir.sin() * lean * 0.35 * s * frac;
        let r_top = 0.17 * s * (1.0 - i as f32 * 0.06);
        let r_bot = 0.20 * s * (1.0 - i as f32 * 0.06);
        let seg_col = if i % 2 == 1 { "#8A6B48" } else { "#7A5D3E" };
        let seg_vec = Vec3::new(nx - px, 1.0 * s, nz - pz);
        let seg_rot = Quat::from_rotation_arc(Vec3::Y, seg_vec.normalize_or_zero());
        b.push_cyl_rot(
            Vec3::new((px + nx) * 0.5, py + 0.5 * s, (pz + nz) * 0.5),
            r_top,
            r_bot,
            seg_vec.length() * 1.04,
            7,
            seg_rot,
            seg_col,
        );
        px = nx;
        pz = nz;
        py += 1.0 * s;
    }
    for k in 0..7 {
        let a = (k as f32 / 7.0) * TAU + rng.range(-0.2, 0.2);
        let len = rng.range(2.1, 2.8) * s;
        let droop = -rng.range(0.25, 0.55);
        let rot = Quat::from_rotation_y(a) * Quat::from_rotation_z(droop);
        let offset = rot * Vec3::new(len * 0.5, 0.0, 0.0);
        let col = *rng.pick(&["#4F8A3A", "#5E9A40", "#457C35"]);
        b.push_box_rot(
            Vec3::new(px, py, pz) + offset,
            Vec3::new(len, 0.05, 0.55 * s),
            rot,
            col,
        );
    }
    // Coconuts under crown
    b.push_sphere(Vec3::new(px, py - 0.15, pz), Vec3::splat(0.30 * s), "#6B5232");
}

/// Outward-wound horizontal ring (gunwale of *thuyền thúng*) built via segmented torus bars.
fn push_horizontal_ring(
    b: &mut StreetMeshBuilder,
    center: Vec3,
    radius: f32,
    tube_r: f32,
    segs: usize,
    hex: &str,
) {
    for i in 0..segs {
        let a0 = (i as f32 / segs as f32) * TAU;
        let a1 = ((i + 1) as f32 / segs as f32) * TAU;
        let p0 = center + Vec3::new(a0.cos() * radius, 0.0, a0.sin() * radius);
        let p1 = center + Vec3::new(a1.cos() * radius, 0.0, a1.sin() * radius);
        b.push_bar(p0, p1, tube_r, 6, hex);
    }
}

/// Hollow woven *thuyền thúng* basket boat (`beach.js:48-53`).
fn push_basket_boat(b: &mut StreetMeshBuilder, x: f32, y: f32, z: f32) {
    // Tapered woven hull bowl (lower rim -> upper rim)
    b.push_cyl(Vec3::new(x, y + 0.30, z), 1.08, 0.72, 0.52, 14, "#5B4532");
    // Dark tarred interior base disk
    b.push_cyl(Vec3::new(x, y + 0.32, z), 0.94, 0.94, 0.06, 14, "#3E2F22");
    // Blue woven gunwale rim
    push_horizontal_ring(b, Vec3::new(x, y + 0.56, z), 1.08, 0.075, 16, "#2E6FA8");
    // Wooden thwart seat plank across basket
    b.push_box(Vec3::new(x, y + 0.50, z), Vec3::new(1.75, 0.06, 0.20), "#7A5A3E");
}

/// Outward-wound vertex-colored golden wet/dry shoreline strip (`beach.js:18-27`).
fn build_beach_shore_mesh() -> Mesh {
    let s0 = -200.0_f32;
    let s1 = 200.0_f32;
    let step = 4.0_f32;
    let rows = [-1.6_f32, 0.0, 2.5, 7.0, 220.0];
    let n = ((s1 - s0) / step) as usize + 1;
    let r_len = rows.len();

    let dry_col = linear_rgba("#E6CF9D");
    let wet_col = linear_rgba("#C7AF7E");

    let mut positions = Vec::with_capacity(n * r_len);
    let mut normals = Vec::with_capacity(n * r_len);
    let mut uvs = Vec::with_capacity(n * r_len);
    let mut colors = Vec::with_capacity(n * r_len);
    let mut indices = Vec::with_capacity((n - 1) * (r_len - 1) * 6);

    for i in 0..n {
        let s = s0 + i as f32 * step;
        let sd = shore_d(s);
        for (j, &r) in rows.iter().enumerate() {
            let (x, z) = sd_to_xz(s, sd + r);
            let y = if r < 0.0 {
                GY - 0.32
            } else if r < 2.5 {
                GY - 0.08
            } else {
                GY
            };
            positions.push([x, y, z]);
            normals.push([0.0, 1.0, 0.0]);
            uvs.push([i as f32 / (n - 1) as f32, j as f32 / (r_len - 1) as f32]);
            colors.push(if r < 2.5 { wet_col } else { dry_col });
        }
    }
    for i in 0..n - 1 {
        for j in 0..r_len - 1 {
            let a = (i * r_len + j) as u32;
            let b = ((i + 1) * r_len + j) as u32;
            indices.extend_from_slice(&[a, a + 1, b, b, a + 1, b + 1]);
        }
    }

    let mut mesh = Mesh::new(
        PrimitiveTopology::TriangleList,
        RenderAssetUsages::MAIN_WORLD | RenderAssetUsages::RENDER_WORLD,
    );
    mesh.insert_attribute(Mesh::ATTRIBUTE_POSITION, positions);
    mesh.insert_attribute(Mesh::ATTRIBUTE_NORMAL, normals);
    mesh.insert_attribute(Mesh::ATTRIBUTE_UV_0, uvs);
    mesh.insert_attribute(Mesh::ATTRIBUTE_COLOR, colors);
    mesh.insert_indices(Indices::U32(indices));
    mesh
}

/// Builds a single dynamic quad-cloud mesh for all 260 fireflies (`1` draw call, zero runtime allocs).
fn build_firefly_cloud_mesh(base_positions: &[Vec3]) -> Mesh {
    let count = base_positions.len();
    let mut positions = Vec::with_capacity(count * 4);
    let mut normals = Vec::with_capacity(count * 4);
    let mut uvs = Vec::with_capacity(count * 4);
    let mut colors = Vec::with_capacity(count * 4);
    let mut indices = Vec::with_capacity(count * 6);
    let r = 0.085_f32;

    for (i, &p) in base_positions.iter().enumerate() {
        let base = (i * 4) as u32;
        positions.push([p.x - r, p.y - r, p.z]);
        positions.push([p.x + r, p.y - r, p.z]);
        positions.push([p.x + r, p.y + r, p.z]);
        positions.push([p.x - r, p.y + r, p.z]);
        for _ in 0..4 {
            normals.push([0.0, 1.0, 0.0]);
            colors.push([1.6, 2.4, 0.5, 1.0]);
        }
        uvs.push([0.0, 0.0]);
        uvs.push([1.0, 0.0]);
        uvs.push([1.0, 1.0]);
        uvs.push([0.0, 1.0]);
        indices.extend_from_slice(&[base, base + 1, base + 2, base, base + 2, base + 3]);
    }

    let mut mesh = Mesh::new(
        PrimitiveTopology::TriangleList,
        RenderAssetUsages::MAIN_WORLD | RenderAssetUsages::RENDER_WORLD,
    );
    mesh.insert_attribute(Mesh::ATTRIBUTE_POSITION, positions);
    mesh.insert_attribute(Mesh::ATTRIBUTE_NORMAL, normals);
    mesh.insert_attribute(Mesh::ATTRIBUTE_UV_0, uvs);
    mesh.insert_attribute(Mesh::ATTRIBUTE_COLOR, colors);
    mesh.insert_indices(Indices::U32(indices));
    mesh
}

/// Builds a single diamond kite sail mesh (`countryside.js:154`) + 5 tail bows.
fn build_diamond_kite_mesh(hex: &str) -> Mesh {
    let mut b = StreetMeshBuilder::default();
    let col = linear_rgba(hex);
    let white = linear_rgba("#FFFFFF");
    let uv = [[0.0, 0.0], [1.0, 0.0], [1.0, 1.0], [0.0, 1.0]];
    let top = Vec3::new(0.0, 0.9, 0.0);
    let left = Vec3::new(-0.65, 0.0, 0.0);
    let bot = Vec3::new(0.0, -1.0, 0.0);
    let right = Vec3::new(0.65, 0.0, 0.0);
    b.push_quad(left, bot, right, top, Vec3::Z, uv, col);
    for t in 1..=5 {
        let y = -1.0 - t as f32 * 0.45;
        let c = if t % 2 == 1 { white } else { col };
        b.push_quad(
            Vec3::new(-0.11, y - 0.06, 0.0),
            Vec3::new(0.11, y - 0.06, 0.0),
            Vec3::new(0.11, y + 0.06, 0.0),
            Vec3::new(-0.11, y + 0.06, 0.0),
            Vec3::Z,
            uv,
            c,
        );
    }
    b.build()
}

/// Spawns a flapping bird entity (white egret or seagull) with articulated left & right wings.
#[allow(clippy::too_many_arguments)]
fn spawn_flapping_bird(
    commands: &mut Commands,
    body_mesh: Handle<Mesh>,
    wing_mesh: Handle<Mesh>,
    mat: Handle<StandardMaterial>,
    span: f32,
    center: Vec2,
    radius: f32,
    altitude: f32,
    phase: f32,
    speed: f32,
    flap_freq: f32,
    flap_amp: f32,
    night: bool,
    is_seagull: bool,
) -> Entity {
    let mut left_wing = Entity::PLACEHOLDER;
    let mut right_wing = Entity::PLACEHOLDER;
    let x = center.x + phase.cos() * radius;
    let z = center.y + phase.sin() * radius;
    let root = commands
        .spawn((
            WorldEntity,
            BackdropEntity,
            Transform::from_xyz(x, altitude, z).with_rotation(Quat::from_rotation_y(-phase)),
            if night { Visibility::Hidden } else { Visibility::Inherited },
        ))
        .with_children(|g| {
            g.spawn((
                WorldEntity,
                BackdropEntity,
                Mesh3d(body_mesh),
                MeshMaterial3d(mat.clone()),
                Transform::from_scale(Vec3::new(0.30, 0.24, 0.68)),
            ));
            left_wing = g
                .spawn((
                    WorldEntity,
                    BackdropEntity,
                    Transform::IDENTITY,
                    Visibility::Inherited,
                ))
                .with_children(|w| {
                    w.spawn((
                        WorldEntity,
                        BackdropEntity,
                        Mesh3d(wing_mesh.clone()),
                        MeshMaterial3d(mat.clone()),
                        Transform::from_xyz(-span * 0.5, 0.0, 0.0)
                            .with_scale(Vec3::new(span, 0.03, 0.30)),
                    ));
                })
                .id();
            right_wing = g
                .spawn((
                    WorldEntity,
                    BackdropEntity,
                    Transform::IDENTITY,
                    Visibility::Inherited,
                ))
                .with_children(|w| {
                    w.spawn((
                        WorldEntity,
                        BackdropEntity,
                        Mesh3d(wing_mesh),
                        MeshMaterial3d(mat),
                        Transform::from_xyz(span * 0.5, 0.0, 0.0)
                            .with_scale(Vec3::new(span, 0.03, 0.30)),
                    ));
                })
                .id();
        })
        .id();

    commands.entity(root).insert(CountryFlyer {
        left_wing,
        right_wing,
        center,
        radius,
        altitude,
        phase,
        speed,
        flap_freq,
        flap_amp,
    });
    if is_seagull {
        commands.entity(root).insert(BeachSeagull);
    }
    root
}

/* ============================================================================
 * 1:1 COUNTRYSIDE BACKDROP (`countryside.js`)
 * ============================================================================ */

#[allow(clippy::too_many_arguments)]
pub fn spawn_countryside_backdrop(
    commands: &mut Commands,
    meshes: &mut Assets<Mesh>,
    materials: &mut Assets<StandardMaterial>,
    images: &mut Assets<Image>,
    ba: &mut BackdropAssets,
    fonts: &mut FontEngine,
    spec: &LayoutSpec,
    night: bool,
) {
    let mut rng = CityRng::new(712);
    let excl = terrace_exclusion(spec.layout);

    // 1. Lush grass ground plane (`320 × 320`)
    let ground_mesh = ba.add_mesh(meshes, Mesh::from(Plane3d::new(Vec3::Y, Vec2::splat(160.0))));
    let ground_mat = ba.add_material(
        materials,
        StandardMaterial {
            base_color: srgb_hex(if night { "#5E7D45" } else { "#86A85B" }),
            perceptual_roughness: 0.96,
            ..default()
        },
    );
    commands.spawn((
        WorldEntity,
        BackdropEntity,
        CountryGround,
        Mesh3d(ground_mesh),
        MeshMaterial3d(ground_mat),
        Transform::from_xyz(0.0, GY, 0.0),
    ));

    let mut solid = StreetMeshBuilder::default();
    let mut greens = StreetMeshBuilder::default();
    let mut dikes = StreetMeshBuilder::default();
    let mut crops = StreetMeshBuilder::default();
    let mut floods = StreetMeshBuilder::default();
    let mut lit = StreetMeshBuilder::default();
    let mut hay = StreetMeshBuilder::default();
    let mut lanterns = StreetMeshBuilder::default();
    let mut lantern_bulbs = StreetMeshBuilder::default();

    // 2. Meandering river behind the office + mossy riverbanks
    let mut river_pts = Vec::with_capacity(115);
    let mut s = -170.0_f32;
    while s <= 170.0 {
        river_pts.push(sd_to_xz(s, river_d(s)));
        s += 3.0;
    }
    push_ribbon_into(&mut greens, &river_pts, 10.0, GY + 0.015, "#6E8E48");

    let river_img = ba.upload_repeating_image(images, draw_water_tex("#5E9FC0", "#CFE8F2", 713));
    let river_mat = ba.textured_tiled(materials, river_img, Vec2::new(1.0, 0.4), "#FFFFFF", 0.16);
    let river_mesh = ba.add_mesh(meshes, build_ribbon_mesh(&river_pts, 7.0, GY + 0.03, "#FFFFFF"));
    commands.spawn((
        WorldEntity,
        BackdropEntity,
        CountryWater {
            scroll_speed: Vec2::new(0.0, -0.04),
        },
        Mesh3d(river_mesh),
        MeshMaterial3d(river_mat),
        Transform::IDENTITY,
    ));

    // 3. Winding dirt path from office entrance over wooden bridge to the village gate
    let s_b = -16.0_f32;
    let d_b = river_d(s_b);
    let path_ctrl = [
        (-8.9, 1.5),
        (-13.0, 1.2),
        sd_to_xz(-13.0, -16.0),
        sd_to_xz(-15.0, -24.0),
        sd_to_xz(s_b, d_b),
        sd_to_xz(-17.0, -42.0),
        sd_to_xz(-19.0, -52.0),
        sd_to_xz(-26.0, -66.0),
    ];
    let path_pts = sample_catmull_rom(&path_ctrl, 1.5);
    push_ribbon_into(&mut greens, &path_pts, 2.4, GY + 0.04, "#C9AB78");

    // Wooden footbridge over the river (`countryside.js:29-32`)
    let (bx, bz) = sd_to_xz(s_b, d_b);
    let bridge_rot = Quat::from_rotation_y(FRAC_PI_4);
    solid.push_box_rot(Vec3::new(bx, GY + 0.55, bz), Vec3::new(2.8, 0.30, 11.0), bridge_rot, "#8B6B4A");
    solid.push_box_rot(Vec3::new(bx, GY + 0.72, bz), Vec3::new(2.9, 0.12, 11.0), bridge_rot, "#A07C55");
    for &k in &[-1.0_f32, 1.0] {
        let mut t = -5.0_f32;
        while t <= 5.01 {
            let ox = FRAC_PI_4.cos() * k * 1.35 + FRAC_PI_4.sin() * t;
            let oz = -FRAC_PI_4.sin() * k * 1.35 + FRAC_PI_4.cos() * t;
            solid.push_box(Vec3::new(bx + ox, GY + 1.1, bz + oz), Vec3::new(0.12, 0.70, 0.12), "#6B4F35");
            t += 1.25;
        }
        solid.push_box_rot(
            Vec3::new(bx + FRAC_PI_4.cos() * k * 1.35, GY + 1.42, bz - FRAC_PI_4.sin() * k * 1.35),
            Vec3::new(0.10, 0.10, 11.0),
            bridge_rot,
            "#6B4F35",
        );
    }

    // 4. Red-tile *Cổng Làng* village gate with arched terracotta roof & Canvas2d signage
    let (gate_x, gate_z) = sd_to_xz(-19.0, -52.0);
    {
        let (gx, gz) = (gate_x, gate_z);
        let yaw = FRAC_PI_4;
        let gate_rot = Quat::from_rotation_y(yaw);
        let roof_rot = Quat::from_rotation_y(yaw + FRAC_PI_2);
        let piece = |b: &mut StreetMeshBuilder, w: f32, h: f32, d: f32, ox: f32, oy: f32, col: &str| {
            b.push_box_rot(
                Vec3::new(gx + yaw.cos() * ox, GY + oy, gz - yaw.sin() * ox),
                Vec3::new(w, h, d),
                gate_rot,
                col,
            );
        };
        for &k in &[-1.0_f32, 1.0] {
            piece(&mut solid, 0.9, 4.2, 0.9, k * 2.1, 2.1, "#D8CBB0");
            piece(&mut solid, 1.1, 0.4, 1.1, k * 2.1, 4.3, "#B5533C");
            // Small side arch shoulders
            piece(&mut solid, 0.65, 0.65, 0.88, k * 1.35, 3.35, "#D8CBB0");
        }
        piece(&mut solid, 5.6, 0.75, 1.0, 0.0, 4.0, "#D8CBB0");
        push_prism(&mut solid, Vec3::new(gx, GY + 4.36, gz), 1.75, 1.15, 6.6, roof_rot, "#B5533C");

        let sign_mat = ba.textured(images, materials, draw_village_gate_sign(fonts), false, 0.45);
        let sign_quad = ba.uv_rect_quad(meshes, 0.0, 0.0, 1.0, 1.0);
        let sign_n = gate_rot * Vec3::Z;
        commands.spawn((
            WorldEntity,
            BackdropEntity,
            CountryVillageGate,
            Mesh3d(sign_quad),
            MeshMaterial3d(sign_mat),
            Transform::from_translation(Vec3::new(gx, GY + 4.0, gz) + sign_n * 0.52)
                .with_rotation(gate_rot)
                .with_scale(Vec3::new(3.1, 0.62, 1.0)),
        ));
    }

    // 5. Patchwork rice paddies with furrows, flooded mirror fields, raised dykes & rice tufts
    let villages: [(f32, f32, f32); 6] = [
        (-46.0, -60.0, 13.0),
        (34.0, -56.0, 12.0),
        (0.0, -86.0, 15.0),
        (-48.0, -16.0, 10.0),
        (-8.0, -48.0, 9.0),
        (24.0, -44.0, 7.0),
    ];
    let in_village = |x: f32, z: f32, pad: f32| -> bool {
        villages.iter().any(|&(vs, vd, vr)| {
            let (vx, vz) = sd_to_xz(vs, vd);
            (x - vx).hypot(z - vz) < vr + pad
        })
    };

    let mut gx = -100.0_f32;
    while gx < 64.0 {
        let mut gz = -100.0_f32;
        while gz < 60.0 {
            let w = 9.0 - 0.4;
            let d = 7.5 - 0.4;
            let cx = gx + 4.5;
            let cz = gz + 3.75;
            if !near_office(excl, cx, cz, 4.0)
                && !in_village(cx, cz, 4.0)
                && dist_to_polyline(cx, cz, &path_pts) >= 5.5
            {
                let sv = cx - cz;
                let dv = cx + cz;
                if (dv - river_d(sv)).abs() >= 9.0 {
                    let kind = rng.next_f32();
                    let y = GY + 0.05;
                    if kind < 0.16 {
                        floods.push_box(Vec3::new(cx, y, cz), Vec3::new(w, 0.04, d), "#FFFFFF");
                    } else {
                        let col = if kind < 0.52 {
                            "#8DC04B"
                        } else if kind < 0.74 {
                            "#6EA93E"
                        } else if kind < 0.92 {
                            "#D7B54C"
                        } else {
                            "#A88B5A"
                        };
                        let hx = w * 0.5;
                        let hz = d * 0.5;
                        let uv_x = w / 1.2;
                        let uv_z = d / 1.2;
                        crops.push_quad(
                            Vec3::new(cx - hx, y + 0.03, cz + hz),
                            Vec3::new(cx + hx, y + 0.03, cz + hz),
                            Vec3::new(cx + hx, y + 0.03, cz - hz),
                            Vec3::new(cx - hx, y + 0.03, cz - hz),
                            Vec3::Y,
                            [[0.0, 0.0], [uv_x, 0.0], [uv_x, uv_z], [0.0, uv_z]],
                            linear_rgba(col),
                        );
                        // Rice clump tufts on nearby paddies (merged into greens -> 1 draw call!)
                        if kind < 0.92 && (cx - 2.0).hypot(cz - 2.0) < 44.0 {
                            for _ in 0..14 {
                                let rx = cx + rng.range(-hx + 0.4, hx - 0.4);
                                let rz = cz + rng.range(-hz + 0.4, hz - 0.4);
                                let rh = rng.range(0.38, 0.62);
                                greens.push_cyl(Vec3::new(rx, GY + 0.07 + rh * 0.5, rz), 0.02, 0.12, rh, 4, col);
                            }
                        }
                    }
                    // Raised dirt embankments/dykes around every field
                    for (ox, oz, lw, ld) in [
                        (0.0, -d * 0.5 - 0.2, w + 0.4, 0.42),
                        (0.0, d * 0.5 + 0.2, w + 0.4, 0.42),
                        (-w * 0.5 - 0.2, 0.0, 0.42, d),
                        (w * 0.5 + 0.2, 0.0, 0.42, d),
                    ] {
                        dikes.push_box(Vec3::new(cx + ox, GY + 0.12, cz + oz), Vec3::new(lw, 0.24, ld), "#7C9A4B");
                    }
                }
            }
            gz += 7.5;
        }
        gx += 9.0;
    }

    let field_img = ba.upload_repeating_image(images, draw_field_furrow_tex());
    let crop_mat = ba.add_material(
        materials,
        StandardMaterial {
            base_color: Color::WHITE,
            base_color_texture: Some(field_img),
            perceptual_roughness: 0.90,
            ..default()
        },
    );
    crops.spawn(commands, meshes, ba, crop_mat);

    let flood_img = ba.upload_repeating_image(images, draw_water_tex("#9CC6D6", "#E8F6FA", 714));
    let flood_mat = ba.textured_tiled(materials, flood_img, Vec2::splat(0.18), "#FFFFFF", 0.08);
    if let Some(fe) = floods.spawn(commands, meshes, ba, flood_mat) {
        commands.entity(fe).insert(CountryWater {
            scroll_speed: Vec2::new(0.004, 0.0),
        });
    }

    let dike_mat = ba.mat(materials, "#FFFFFF", 0.95, 0.0);
    dikes.spawn(commands, meshes, ba, dike_mat);

    // 6. Villages: red-tile cottages, pastel town houses, bamboo groves, banana & areca palms, haystacks
    let push_house = |rng: &mut CityRng,
                      solid: &mut StreetMeshBuilder,
                      lit: &mut StreetMeshBuilder,
                      x: f32,
                      z: f32,
                      yaw: f32| {
        let w = rng.range(4.4, 5.6);
        let d = rng.range(3.4, 4.0);
        let h = rng.range(2.4, 2.8);
        let wall = *rng.pick(&["#F1E3B5", "#EDD9A4", "#F4EEDF", "#E9C98D"]);
        let roof = *rng.pick(&["#B5533C", "#C0623F", "#A84A36"]);
        let rot = Quat::from_rotation_y(yaw);
        let pt = |ox: f32, oy: f32, oz: f32| Vec3::new(x, GY + oy, z) + rot * Vec3::new(ox, 0.0, oz);
        solid.push_box_rot(pt(0.0, h * 0.5, 0.0), Vec3::new(w, h, d), rot, wall);
        push_prism(solid, pt(0.0, h, 0.0), d + 1.0, 1.6, w + 0.8, rot * Quat::from_rotation_y(FRAC_PI_2), roof);
        solid.push_box_rot(pt(0.0, 0.12, d * 0.5 + 0.6), Vec3::new(w + 0.2, 0.25, 1.3), rot, "#BFB39C");
        for &k in &[-1.0_f32, 1.0] {
            solid.push_box_rot(pt(k * (w * 0.5 - 0.2), h * 0.5, d * 0.5 + 1.1), Vec3::new(0.16, h, 0.16), rot, "#7A5A3E");
            lit.push_box_rot(pt(k * w * 0.3, 1.4, d * 0.5 + 0.04), Vec3::new(0.8, 0.7, 0.03), rot, "#FFFFFF");
        }
        solid.push_box_rot(pt(0.0, 0.85, d * 0.5 + 0.03), Vec3::new(0.9, 1.7, 0.06), rot, "#5A3A26");
    };

    let push_townhouse = |rng: &mut CityRng,
                          solid: &mut StreetMeshBuilder,
                          lit: &mut StreetMeshBuilder,
                          x: f32,
                          z: f32,
                          yaw: f32| {
        let w = rng.range(3.6, 4.4);
        let d = rng.range(6.0, 8.0);
        let fl = rng.range_i32(2, 3);
        let h = fl as f32 * 3.0;
        let col = *rng.pick(&["#F6C1C1", "#BFE3D0", "#F9E3A1", "#C9DAF2", "#F4D0A9", "#E6E1F5"]);
        let rot = Quat::from_rotation_y(yaw);
        let pt = |ox: f32, oy: f32, oz: f32| Vec3::new(x, GY + oy, z) + rot * Vec3::new(ox, 0.0, oz);
        solid.push_box_rot(pt(0.0, h * 0.5, 0.0), Vec3::new(w, h, d), rot, col);
        solid.push_box_rot(pt(0.0, h + 0.25, 0.0), Vec3::new(w + 0.2, 0.5, d + 0.2), rot, "#E9E4DA");
        for f in 1..fl {
            solid.push_box_rot(pt(0.0, f as f32 * 3.0, d * 0.5 + 0.5), Vec3::new(w, 0.18, 1.0), rot, "#E9E4DA");
        }
        solid.push_cyl_rot(
            pt(w * 0.15, h + 1.1, -d * 0.25),
            0.5,
            0.5,
            1.1,
            10,
            rot * Quat::from_rotation_z(FRAC_PI_2),
            "#C8CDD2",
        );
        for f in 0..fl {
            lit.push_box_rot(pt(0.0, f as f32 * 3.0 + 1.6, d * 0.5 + 0.04), Vec3::new(w * 0.55, 1.3, 0.03), rot, "#FFFFFF");
        }
    };

    let push_bamboo = |rng: &mut CityRng, solid: &mut StreetMeshBuilder, greens: &mut StreetMeshBuilder, x: f32, z: f32, n: i32| {
        for _ in 0..n {
            let ox = x + rng.range(-1.6, 1.6);
            let oz = z + rng.range(-1.6, 1.6);
            let h = rng.range(6.0, 9.0);
            let lean = rng.range(-0.12, 0.12);
            let stalk_c = *rng.pick(&["#9DB65A", "#8FAA4E", "#A8BF63"]);
            let leaf_c = *rng.pick(&["#5E8F3A", "#6FA046", "#557F35"]);
            let rot = Quat::from_rotation_z(lean);
            let top = Vec3::new(ox - lean.sin() * h, GY + h * 0.95, oz);
            solid.push_cyl_rot(
                Vec3::new(ox - lean.sin() * h * 0.5, GY + h * 0.5, oz),
                0.06,
                0.09,
                h,
                5,
                rot,
                stalk_c,
            );
            let r = rng.range(0.9, 1.4);
            greens.push_sphere(top, Vec3::new(r * 1.1, r * 0.55, r * 1.1), leaf_c);
        }
    };

    let push_banana = |rng: &mut CityRng, solid: &mut StreetMeshBuilder, greens: &mut StreetMeshBuilder, x: f32, z: f32| {
        solid.push_cyl(Vec3::new(x, GY + 1.1, z), 0.16, 0.22, 2.2, 7, "#7E9B55");
        for k in 0..6 {
            let a = (k as f32 / 6.0) * TAU + rng.range(-0.3, 0.3);
            let rot = Quat::from_rotation_y(a) * Quat::from_rotation_z(rng.range(0.1, 0.6));
            let off = rot * Vec3::new(0.95, 0.0, 0.0);
            let col = *rng.pick(&["#6DAA3E", "#7FBA48"]);
            greens.push_box_rot(Vec3::new(x, GY + 2.2, z) + off, Vec3::new(1.9, 0.05, 0.6), rot, col);
        }
    };

    let push_areca = |rng: &mut CityRng, solid: &mut StreetMeshBuilder, greens: &mut StreetMeshBuilder, x: f32, z: f32| {
        let h = rng.range(6.0, 8.0);
        solid.push_cyl(Vec3::new(x, GY + h * 0.5, z), 0.10, 0.13, h, 6, "#9E9A86");
        for k in 0..6 {
            let a = (k as f32 / 6.0) * TAU;
            let rot = Quat::from_rotation_y(a) * Quat::from_rotation_z(-0.35);
            let off = rot * Vec3::new(0.75, 0.0, 0.0);
            greens.push_box_rot(Vec3::new(x, GY + h, z) + off, Vec3::new(1.5, 0.05, 0.4), rot, "#5E9440");
        }
    };

    let push_haystack = |hay: &mut StreetMeshBuilder, x: f32, z: f32| {
        hay.push_cyl(Vec3::new(x, GY + 0.5, z), 0.9, 1.0, 1.0, 10, "#D2AE55");
        hay.push_cyl(Vec3::new(x, GY + 1.8, z), 0.04, 1.0, 1.6, 10, "#DDBA62");
        hay.push_cyl(Vec3::new(x, GY + 2.9, z), 0.05, 0.05, 0.8, 4, "#6B4F35");
    };

    for &(vs, vd, vr) in &villages {
        let (vx, vz) = sd_to_xz(vs, vd);
        let items = (vr * 0.7).round() as usize;
        for _ in 0..items {
            let a = rng.range(0.0, TAU);
            let r = rng.range(2.0, (vr - 2.0).max(2.5));
            let x = vx + a.cos() * r;
            let z = vz + a.sin() * r;
            let yaw = *rng.pick(&[0.0, FRAC_PI_2, FRAC_PI_4, -FRAC_PI_4]) + rng.range(-0.15, 0.15);
            if !tall_ok(excl, x, z, 8.0)
                || ((x + z) - river_d(x - z)).abs() < 6.5
                || (x - gate_x).hypot(z - gate_z) < 7.0
                || dist_to_polyline(x, z, &path_pts) < 4.5
            {
                continue;
            }
            let k = rng.next_f32();
            if k < 0.45 {
                push_house(&mut rng, &mut solid, &mut lit, x, z, yaw);
            } else if k < 0.70 {
                push_townhouse(&mut rng, &mut solid, &mut lit, x, z, yaw);
            } else if k < 0.80 {
                push_haystack(&mut hay, x, z);
            } else if k < 0.90 {
                push_banana(&mut rng, &mut solid, &mut greens, x, z);
            } else {
                push_areca(&mut rng, &mut solid, &mut greens, x, z);
            }
        }
        for i in 0..9 {
            let a = (i as f32 / 9.0) * TAU + rng.range(-0.2, 0.2);
            let x = vx + a.cos() * (vr + 1.5);
            let z = vz + a.sin() * (vr + 1.5);
            if tall_ok(excl, x, z, 9.0)
                && (x - gate_x).hypot(z - gate_z) > 6.5
                && dist_to_polyline(x, z, &path_pts) > 4.2
            {
                let n = rng.range_i32(6, 11);
                push_bamboo(&mut rng, &mut solid, &mut greens, x, z, n);
            }
        }
    }

    // Bamboo & trees along the river and dykes + haystacks in harvested fields
    let mut rs = -150.0_f32;
    while rs <= 150.0 {
        let side = if rng.next_f32() < 0.5 { -6.5 } else { 6.5 };
        let (x, z) = sd_to_xz(rs, river_d(rs) + side);
        if tall_ok(excl, x, z, 9.0)
            && (x - gate_x).hypot(z - gate_z) > 6.5
            && dist_to_polyline(x, z, &path_pts) > 4.2
        {
            if rng.next_f32() < 0.5 {
                let n = rng.range_i32(5, 9);
                push_bamboo(&mut rng, &mut solid, &mut greens, x, z, n);
            } else {
                let sc = rng.range(0.9, 1.3);
                push_round_tree(&mut greens, &mut rng, x, z, sc);
            }
        }
        rs += rng.range(9.0, 16.0);
    }
    for _ in 0..26 {
        let x = rng.range(-95.0, 60.0);
        let z = rng.range(-95.0, 60.0);
        if tall_ok(excl, x, z, 3.0) && !in_village(x, z, 0.0) && ((x + z) - river_d(x - z)).abs() > 7.0 {
            if rng.next_f32() < 0.5 {
                push_haystack(&mut hay, x, z);
            } else {
                let sc = rng.range(0.8, 1.2);
                push_round_tree(&mut greens, &mut rng, x, z, sc);
            }
        }
    }

    // 7. Distant limestone karst hills in the haze (`countryside.js:117-118`)
    for _ in 0..22 {
        let ks = rng.range(-150.0, 150.0);
        let kd = rng.range(-140.0, -95.0);
        let (x, z) = sd_to_xz(ks, kd);
        let r = rng.range(5.0, 11.0);
        let h = rng.range(14.0, 30.0);
        let c = *rng.pick(&["#557A49", "#4D7044", "#5E8250"]);
        push_karst(&mut greens, x, z, r, h, c);
    }
    for &side in &[-1.0_f32, 1.0] {
        for _ in 0..6 {
            let ks = side * rng.range(80.0, 130.0);
            let kd = rng.range(-70.0, 20.0);
            let (x, z) = sd_to_xz(ks, kd);
            let r = rng.range(5.0, 9.0);
            let h = rng.range(12.0, 22.0);
            push_karst(&mut greens, x, z, r, h, "#557A49");
        }
    }

    // Standing egrets in the paddies (`countryside.js:143-145`)
    for _ in 0..14 {
        let x = rng.range(-90.0, 55.0);
        let z = rng.range(-90.0, 55.0);
        if !tall_ok(excl, x, z, 1.5) || in_village(x, z, 0.0) {
            continue;
        }
        solid.push_sphere(Vec3::new(x, GY + 0.75, z), Vec3::new(0.35, 0.22, 0.22), "#F6F6F2");
        solid.push_cyl_rot(
            Vec3::new(x + 0.30, GY + 1.05, z),
            0.04,
            0.05,
            0.60,
            5,
            Quat::from_rotation_z(-0.5),
            "#F6F6F2",
        );
        solid.push_cyl(Vec3::new(x, GY + 0.33, z), 0.02, 0.02, 0.55, 4, "#222222");
    }

    // 8. Path lanterns + glowing bulbs + night pools (`countryside.js:124-129`)
    let mut lamp_spots = Vec::new();
    let mut i = 6usize;
    while i + 4 < path_pts.len() {
        let (x, z) = path_pts[i];
        if !near_office(excl, x + 1.6, z, 0.5) {
            lanterns.push_cyl(Vec3::new(x + 1.6, GY + 1.2, z), 0.05, 0.06, 2.4, 5, "#3B3029");
            lantern_bulbs.push_sphere(Vec3::new(x + 1.6, GY + 2.5, z), Vec3::splat(0.22), "#FFFFFF");
            lamp_spots.push((x + 1.2, z));
        }
        i += 9;
    }

    // Finalize merged static terrain/village meshes
    let solid_mat = ba.mat(materials, "#FFFFFF", 0.80, 0.0);
    solid.spawn(commands, meshes, ba, solid_mat);
    let green_mat = ba.mat(materials, "#FFFFFF", 0.95, 0.0);
    greens.spawn(commands, meshes, ba, green_mat);
    let hay_mat = ba.mat(materials, "#FFFFFF", 1.0, 0.0);
    hay.spawn(commands, meshes, ba, hay_mat);
    let lantern_mat = ba.mat(materials, "#FFFFFF", 0.60, 0.1);
    lanterns.spawn(commands, meshes, ba, lantern_mat);

    let lit_mat = ba.add_material(
        materials,
        StandardMaterial {
            base_color: if night {
                Color::srgb(1.0, 0.80, 0.45)
            } else {
                Color::srgb(0.08, 0.08, 0.09)
            },
            emissive: if night {
                LinearRgba::new(2.2, 1.55, 0.75, 1.0)
            } else {
                LinearRgba::BLACK
            },
            unlit: true,
            ..default()
        },
    );
    if let Some(le) = lit.spawn(commands, meshes, ba, lit_mat) {
        commands.entity(le).insert(CountryNightLit);
    }

    let bulb_mat = ba.add_material(
        materials,
        StandardMaterial {
            base_color: srgb_hex(if night { "#FFE5AA" } else { "#EDE6D6" }),
            emissive: if night {
                LinearRgba::new(2.8, 2.1, 1.1, 1.0)
            } else {
                LinearRgba::BLACK
            },
            unlit: true,
            ..default()
        },
    );
    if let Some(be) = lantern_bulbs.spawn(commands, meshes, ba, bulb_mat) {
        commands.entity(be).insert(CountryLanternBulb);
    }

    if !lamp_spots.is_empty() {
        let mut pool_b = StreetMeshBuilder::default();
        for &(px, pz) in &lamp_spots {
            pool_b.push_flat(Vec3::new(px, GY + 0.06, pz), 5.0, 5.0, "#FFFFFF");
        }
        let pool_img = ba.upload_image(images, draw_lamp_pool_tex());
        let pool_mat = ba.add_material(
            materials,
            StandardMaterial {
                base_color: Color::srgba(1.0, 0.88, 0.62, 0.85),
                base_color_texture: Some(pool_img),
                alpha_mode: AlphaMode::Add,
                unlit: true,
                cull_mode: None,
                double_sided: true,
                ..default()
            },
        );
        if let Some(pe) = pool_b.spawn(commands, meshes, ba, pool_mat) {
            commands
                .entity(pe)
                .insert((CountryNightPool, if night { Visibility::Inherited } else { Visibility::Hidden }));
        }
    }

    // 9. Grazing water buffalo (articulated body, 4 legs, tail, ears, curved horns, bobbing head)
    let buf_body_mesh = ba.add_mesh(meshes, Mesh::from(Cuboid::new(2.2, 1.1, 1.05)));
    let buf_leg_mesh = ba.add_mesh(meshes, Mesh::from(Cylinder::new(0.12, 0.80)));
    let buf_tail_mesh = ba.add_mesh(meshes, Mesh::from(Cylinder::new(0.03, 0.62)));
    let buf_head_mesh = ba.add_mesh(meshes, Mesh::from(Cuboid::new(0.75, 0.55, 0.60)));
    let buf_ear_mesh = ba.add_mesh(meshes, Mesh::from(Cuboid::new(0.18, 0.10, 0.24)));
    let mut horn_b = StreetMeshBuilder::default();
    for &k in &[-1.0_f32, 1.0] {
        // Curved buffalo horn arc sweeping outward & upward cleanly (deterministic outward winding)
        let pts = [
            Vec3::new(0.15, 0.16, k * 0.26),
            Vec3::new(0.04, 0.26, k * 0.46),
            Vec3::new(-0.10, 0.34, k * 0.58),
            Vec3::new(-0.22, 0.38, k * 0.52),
        ];
        for seg in pts.windows(2) {
            horn_b.push_bar(seg[0], seg[1], 0.048, 6, "#D9D2C2");
        }
    }
    let buf_horns_mesh = ba.add_mesh(meshes, horn_b.build());
    let buf_mat = ba.mat(materials, "#4B4A50", 0.85, 0.0);
    let horn_mat = ba.mat(materials, "#FFFFFF", 0.60, 0.0);

    for &(bs, bd) in &[(-30.0_f32, -40.0_f32), (22.0, -42.0), (40.0, -24.0), (-58.0, -30.0), (8.0, -60.0)] {
        let (x, z) = sd_to_xz(bs, bd);
        if !tall_ok(excl, x, z, 2.0) {
            continue;
        }
        let yaw = rng.range(0.0, TAU);
        let ph = rng.range(0.0, 6.0);
        let mut head_pivot = Entity::PLACEHOLDER;
        let root = commands
            .spawn((
                WorldEntity,
                BackdropEntity,
                Transform::from_xyz(x, GY, z).with_rotation(Quat::from_rotation_y(yaw)),
                Visibility::Inherited,
            ))
            .with_children(|g| {
                g.spawn((
                    WorldEntity,
                    BackdropEntity,
                    Mesh3d(buf_body_mesh.clone()),
                    MeshMaterial3d(buf_mat.clone()),
                    Transform::from_xyz(0.0, 1.15, 0.0),
                ));
                for &(lx, lz) in &[(-0.75_f32, -0.35_f32), (0.75, -0.35), (-0.75, 0.35), (0.75, 0.35)] {
                    g.spawn((
                        WorldEntity,
                        BackdropEntity,
                        Mesh3d(buf_leg_mesh.clone()),
                        MeshMaterial3d(buf_mat.clone()),
                        Transform::from_xyz(lx, 0.40, lz),
                    ));
                }
                g.spawn((
                    WorldEntity,
                    BackdropEntity,
                    Mesh3d(buf_tail_mesh.clone()),
                    MeshMaterial3d(buf_mat.clone()),
                    Transform::from_xyz(-1.18, 0.92, 0.0).with_rotation(Quat::from_rotation_z(-0.35)),
                ));
                head_pivot = g
                    .spawn((
                        WorldEntity,
                        BackdropEntity,
                        Transform::from_xyz(1.15, 1.25, 0.0).with_rotation(Quat::from_rotation_z(-0.55)),
                        Visibility::Inherited,
                    ))
                    .with_children(|h| {
                        h.spawn((
                            WorldEntity,
                            BackdropEntity,
                            Mesh3d(buf_head_mesh.clone()),
                            MeshMaterial3d(buf_mat.clone()),
                            Transform::from_xyz(0.35, -0.20, 0.0),
                        ));
                        for &k in &[-1.0_f32, 1.0] {
                            h.spawn((
                                WorldEntity,
                                BackdropEntity,
                                Mesh3d(buf_ear_mesh.clone()),
                                MeshMaterial3d(buf_mat.clone()),
                                Transform::from_xyz(0.08, -0.02, k * 0.36),
                            ));
                        }
                        h.spawn((
                            WorldEntity,
                            BackdropEntity,
                            Mesh3d(buf_horns_mesh.clone()),
                            MeshMaterial3d(horn_mat.clone()),
                            Transform::IDENTITY,
                        ));
                    })
                    .id();
            })
            .id();
        commands.entity(root).insert(CountryBuffalo { head_pivot, phase: ph });
    }

    // 10. 5 flapping white egrets circling overhead (`countryside.js:146`)
    let bird_body = ba.add_mesh(meshes, Mesh::from(Sphere::new(0.5)));
    let bird_wing = ba.add_mesh(meshes, Mesh::from(Cuboid::new(1.0, 1.0, 1.0)));
    let egret_mat = ba.mat_double_sided(materials, "#F6F6F2", 0.70, 0.0);
    for i in 0..5 {
        let r = rng.range(18.0, 30.0);
        let cx = rng.range(-40.0, 10.0);
        let cz = rng.range(-50.0, -25.0);
        let y = rng.range(9.0, 14.0);
        let sp = rng.range(0.12, 0.20);
        spawn_flapping_bird(
            commands,
            bird_body.clone(),
            bird_wing.clone(),
            egret_mat.clone(),
            1.0,
            Vec2::new(cx, cz),
            r,
            y,
            i as f32 * 1.3,
            sp,
            7.0,
            0.50,
            night,
            false,
        );
    }

    // 11. 3 kids flying colorful diamond kites with live kite strings (`countryside.js:147-160`)
    let string_mesh = ba.add_mesh(meshes, Mesh::from(Cylinder::new(0.015, 1.0)));
    let string_mat = ba.mat_blend(materials, "#F4F1E8", 0.72, 0.5, true);
    let kite_mat = ba.add_material(
        materials,
        StandardMaterial {
            base_color: Color::WHITE,
            unlit: true,
            cull_mode: None,
            double_sided: true,
            ..default()
        },
    );
    let mut kids_b = StreetMeshBuilder::default();
    for &(ks, kd, col) in &[(-34.0_f32, -20.0_f32, "#E8473F"), (30.0, -34.0, "#F2B33D"), (-6.0, -46.0, "#3E8EDE")] {
        let (kx, kz) = sd_to_xz(ks, kd);
        if !tall_ok(excl, kx, kz, 2.0) {
            continue;
        }
        // Chibi-proportioned kid figure holding kite string up
        let rot = Quat::from_rotation_y(-PI * 0.75);
        kids_b.push_cyl(Vec3::new(kx - 0.11, GY + 0.22, kz), 0.07, 0.07, 0.44, 6, "#2F3B52");
        kids_b.push_cyl(Vec3::new(kx + 0.11, GY + 0.22, kz), 0.07, 0.07, 0.44, 6, "#2F3B52");
        kids_b.push_cyl(Vec3::new(kx, GY + 0.64, kz), 0.18, 0.21, 0.46, 8, col);
        kids_b.push_sphere(Vec3::new(kx, GY + 1.08, kz), Vec3::splat(0.24), "#F5D0B5");
        kids_b.push_sphere(Vec3::new(kx, GY + 1.15, kz - 0.02), Vec3::new(0.25, 0.22, 0.25), "#2B2420");
        // Raised right arm holding string
        kids_b.push_bar(
            Vec3::new(kx + 0.16, GY + 0.78, kz),
            Vec3::new(kx + 0.25, GY + 1.05, kz),
            0.055,
            5,
            col,
        );

        let hand = Vec3::new(kx + 0.25, GY + 1.05, kz);
        let base_pos = Vec3::new(kx - 9.0, GY + rng.range(17.0, 22.0), kz - 9.0);
        let ph = rng.range(0.0, 6.0);
        let k_mesh = ba.add_mesh(meshes, build_diamond_kite_mesh(col));
        let str_ent = commands
            .spawn((
                WorldEntity,
                BackdropEntity,
                Mesh3d(string_mesh.clone()),
                MeshMaterial3d(string_mat.clone()),
                Transform::from_translation((hand + base_pos) * 0.5),
            ))
            .id();
        let _ = rot;
        commands.spawn((
            WorldEntity,
            BackdropEntity,
            CountryKite {
                hand_anchor: hand,
                base_pos,
                phase: ph,
                string_entity: str_ent,
            },
            Mesh3d(k_mesh),
            MeshMaterial3d(kite_mat.clone()),
            Transform::from_translation(base_pos).with_rotation(Quat::from_rotation_y(FRAC_PI_4)),
        ));
    }
    let kids_mat = ba.mat(materials, "#FFFFFF", 0.75, 0.0);
    kids_b.spawn(commands, meshes, ba, kids_mat);

    // 12. 260 night fireflies drifting & pulsing over fields and bamboo (`countryside.js:162-179`)
    let mut ff_base = Vec::with_capacity(COUNTRY_FIREFLY_COUNT);
    let mut ff_phases = Vec::with_capacity(COUNTRY_FIREFLY_COUNT);
    for _ in 0..COUNTRY_FIREFLY_COUNT {
        let mut x = 0.0_f32;
        let mut z = 0.0_f32;
        for _ in 0..32 {
            x = rng.range(-70.0, 40.0);
            z = rng.range(-70.0, 40.0);
            if !near_office(excl, x, z, 1.0) {
                break;
            }
        }
        ff_base.push(Vec3::new(x, GY + rng.range(0.4, 2.6), z));
        ff_phases.push(rng.range(0.0, 10.0));
    }
    let ff_mesh = ba.add_mesh(meshes, build_firefly_cloud_mesh(&ff_base));
    let ff_mat = ba.add_material(
        materials,
        StandardMaterial {
            base_color: Color::WHITE,
            emissive: LinearRgba::new(1.5, 2.2, 0.45, 1.0),
            alpha_mode: AlphaMode::Add,
            unlit: true,
            cull_mode: None,
            double_sided: true,
            ..default()
        },
    );
    commands.spawn((
        WorldEntity,
        BackdropEntity,
        CountryFireflies {
            mesh: ff_mesh.clone(),
            base_positions: ff_base,
            phases: ff_phases,
        },
        Mesh3d(ff_mesh),
        MeshMaterial3d(ff_mat),
        Transform::IDENTITY,
        if night { Visibility::Inherited } else { Visibility::Hidden },
    ));
}

/* ============================================================================
 * 1:1 BEACH BACKDROP (`beach.js`)
 * ============================================================================ */

#[allow(clippy::too_many_arguments)]
pub fn spawn_beach_backdrop(
    commands: &mut Commands,
    meshes: &mut Assets<Mesh>,
    materials: &mut Assets<StandardMaterial>,
    images: &mut Assets<Image>,
    ba: &mut BackdropAssets,
    spec: &LayoutSpec,
    night: bool,
) {
    let mut rng = CityRng::new(4242);
    let excl = terrace_exclusion(spec.layout);

    // 1. Turquoise-to-deep-blue ocean plane (`420 × 420` at `GY - 0.14`)
    let sea_img = ba.upload_repeating_image(images, draw_water_tex("#2D8FB8", "#BFE7F3", 4243));
    let sea_mat = ba.add_material(
        materials,
        StandardMaterial {
            base_color: srgb_hex(if night { "#7C93B8" } else { "#FFFFFF" }),
            base_color_texture: Some(sea_img),
            uv_transform: bevy::math::Affine2::from_scale(Vec2::splat(16.0)),
            perceptual_roughness: 0.14,
            metallic: 0.14,
            ..default()
        },
    );
    let sea_mesh = ba.add_mesh(meshes, Mesh::from(Plane3d::new(Vec3::Y, Vec2::splat(210.0))));
    commands.spawn((
        WorldEntity,
        BackdropEntity,
        BeachOcean,
        CountryWater {
            scroll_speed: Vec2::new(0.006, 0.004),
        },
        Mesh3d(sea_mesh),
        MeshMaterial3d(sea_mat),
        Transform::from_xyz(0.0, GY - 0.14, 0.0),
    ));

    // 2. Vertex-colored wet/dry golden shoreline strip (`beach.js:18-27`)
    let shore_mesh = ba.add_mesh(meshes, build_beach_shore_mesh());
    let shore_mat = ba.add_material(
        materials,
        StandardMaterial {
            base_color: srgb_hex(if night { "#8C93A8" } else { "#FFFFFF" }),
            perceptual_roughness: 0.96,
            ..default()
        },
    );
    commands.spawn((
        WorldEntity,
        BackdropEntity,
        BeachShore,
        Mesh3d(shore_mesh),
        MeshMaterial3d(shore_mat),
        Transform::IDENTITY,
    ));

    // 3. Shallow turquoise translucent band & 2 animated rolling surf foam waves (`beach.js:29-33`)
    let mut shore_pts = Vec::with_capacity(136);
    let mut s = -200.0_f32;
    while s <= 200.0 {
        shore_pts.push(sd_to_xz(s, shore_d(s)));
        s += 3.0;
    }
    let shallow_pts: Vec<(f32, f32)> = shore_pts.iter().map(|&(x, z)| (x - 3.0, z - 3.0)).collect();
    let shallow_mesh = ba.add_mesh(meshes, build_ribbon_mesh(&shallow_pts, 9.0, GY - 0.11, "#7FD6DA"));
    let shallow_mat = ba.add_material(
        materials,
        StandardMaterial {
            base_color: Color::srgba(0.50, 0.84, 0.86, if night { 0.14 } else { 0.45 }),
            alpha_mode: AlphaMode::Blend,
            unlit: true,
            cull_mode: None,
            double_sided: true,
            ..default()
        },
    );
    commands.spawn((
        WorldEntity,
        BackdropEntity,
        BeachShallow,
        Mesh3d(shallow_mesh),
        MeshMaterial3d(shallow_mat),
        Transform::IDENTITY,
    ));

    let foam_img = ba.upload_repeating_image(images, draw_foam_grad_tex());
    let foam_mesh = ba.add_mesh(meshes, build_ribbon_mesh(&shore_pts, 1.8, GY - 0.07, "#FFFFFF"));
    for k in 0..2 {
        let foam_mat = ba.add_material(
            materials,
            StandardMaterial {
                base_color: Color::srgba(1.0, 1.0, 1.0, 0.72),
                base_color_texture: Some(foam_img.clone()),
                alpha_mode: AlphaMode::Blend,
                unlit: true,
                cull_mode: None,
                double_sided: true,
                ..default()
            },
        );
        commands.spawn((
            WorldEntity,
            BackdropEntity,
            BeachWave {
                phase: k as f32 * PI,
            },
            Mesh3d(foam_mesh.clone()),
            MeshMaterial3d(foam_mat),
            Transform::IDENTITY,
        ));
    }

    let mut solid = StreetMeshBuilder::default();
    let mut greens = StreetMeshBuilder::default();
    let mut lit = StreetMeshBuilder::default();
    let mut hut_lights = StreetMeshBuilder::default();
    let on_sand = |x: f32, z: f32, pad: f32| -> bool { (x + z) > shore_d(x - z) + pad };

    // 4. 70 curved leaning coconut palms on sand clear of the office/terrace (`beach.js:37`)
    let mut placed_palms = 0usize;
    for _ in 0..600 {
        if placed_palms >= BEACH_PALM_COUNT {
            break;
        }
        let ps = rng.range(-120.0, 120.0);
        let pd = rng.range(-26.0, 60.0);
        let (x, z) = sd_to_xz(ps, pd);
        if on_sand(x, z, 3.0) && tall_ok(excl, x, z, 7.0) {
            let sc = rng.range(0.95, 1.25);
            push_coconut_palm(&mut greens, &mut rng, x, z, sc);
            placed_palms += 1;
        }
    }
    commands.spawn((
        WorldEntity,
        BackdropEntity,
        BeachPalmMeta { count: placed_palms },
    ));

    // 5. Beach umbrellas, loungers & colorful round beach towels (`beach.js:39-46`)
    let push_umbrella = |solid: &mut StreetMeshBuilder, rng: &mut CityRng, x: f32, z: f32| {
        let umb_c = *rng.pick(&["#E8473F", "#F2B33D", "#3E8EDE", "#2BB3A3", "#F07FA8"]);
        let towel_c = *rng.pick(&["#3E8EDE", "#F2B33D", "#E8473F"]);
        solid.push_cyl(Vec3::new(x, GY + 1.25, z), 0.05, 0.05, 2.5, 6, "#EDEDED");
        // Striped canopy (alternating color & cream cap)
        solid.push_cyl(Vec3::new(x, GY + 2.48, z), 0.12, 1.50, 0.68, 10, umb_c);
        solid.push_cyl(Vec3::new(x, GY + 2.84, z), 0.04, 0.26, 0.12, 8, "#FFFFFF");
        for &k in &[-1.0_f32, 1.0] {
            solid.push_box_rot(
                Vec3::new(x + k * 0.9, GY + 0.35, z + 1.4),
                Vec3::new(0.7, 0.12, 1.9),
                Quat::from_rotation_x(-0.12),
                "#F4F1EA",
            );
        }
        solid.push_cyl(Vec3::new(x + 0.4, GY + 0.02, z + 2.4), 0.60, 0.60, 0.03, 12, towel_c);
    };
    let mut us = -80.0_f32;
    while us <= 80.0 {
        let ud = shore_d(us) + rng.range(6.0, 12.0);
        let (x, z) = sd_to_xz(us, ud);
        if on_sand(x, z, 4.0) && tall_ok(excl, x, z, 3.0) {
            push_umbrella(&mut solid, &mut rng, x, z);
        }
        us += rng.range(7.0, 11.0);
    }
    // Flanking beach lounge umbrellas on open sand beside the terrace (clear of office/terrace)
    for &(fs, fd) in &[(-24.0_f32, 8.0_f32), (28.0, 2.0), (26.0, 14.0), (-18.0, 22.0)] {
        let (x, z) = sd_to_xz(fs, fd);
        if on_sand(x, z, 4.0) && !near_office(excl, x, z, 3.2) {
            push_umbrella(&mut solid, &mut rng, x, z);
        }
    }

    // 6. *Thuyền thúng* round woven Vietnamese basket boats on wet sand & shallows (`beach.js:48-54`)
    let mut bs = -70.0_f32;
    while bs <= 70.0 {
        let bd = shore_d(bs) + rng.range(0.5, 3.0);
        let (x, z) = sd_to_xz(bs, bd);
        if tall_ok(excl, x, z, 1.5) {
            push_basket_boat(&mut solid, x, GY - 0.10, z);
        }
        bs += rng.range(9.0, 15.0);
    }

    // 7. Thatched beachfront huts with warm string lights (`beach.js:56-64`)
    for &(hs, hd) in &[(-40.0_f32, -10.0_f32), (-52.0, 4.0), (42.0, -12.0), (56.0, 2.0), (-34.0, -18.0), (36.0, -20.0)] {
        let (x, z) = sd_to_xz(hs, hd);
        if !tall_ok(excl, x, z, 5.0) {
            continue;
        }
        for &(a, b) in &[(-1.6_f32, -1.6_f32), (1.6, -1.6), (-1.6, 1.6), (1.6, 1.6)] {
            solid.push_cyl(Vec3::new(x + a, GY + 1.3, z + b), 0.10, 0.12, 2.6, 6, "#7A5A3E");
        }
        solid.push_cyl_rot(
            Vec3::new(x, GY + 3.4, z),
            0.08,
            3.2,
            1.8,
            4,
            Quat::from_rotation_y(FRAC_PI_4),
            "#C9A65C",
        );
        solid.push_box(Vec3::new(x, GY + 0.45, z + 1.0), Vec3::new(2.6, 0.9, 1.0), "#8B6B4A");
        lit.push_box(Vec3::new(x, GY + 1.0, z + 1.53), Vec3::new(2.4, 0.6, 0.03), "#FFFFFF");
        for k in 0..9 {
            let a = (k as f32 / 9.0) * TAU;
            let lx = x + a.cos() * 2.4;
            let ly = GY + 2.5 - (a * 2.0).sin().abs() * 0.25;
            let lz = z + a.sin() * 2.4;
            hut_lights.push_sphere(Vec3::new(lx, ly, lz), Vec3::splat(0.11), "#FFE0A0");
        }
    }

    // 8. Coastal lighthouse on rocky outcrop + rotating night volumetric beam (`beach.js:65-73`)
    let (lx, lz) = sd_to_xz(-58.0, -34.0);
    for _ in 0..9 {
        let rx = lx + rng.range(-5.0, 5.0);
        let rz = lz + rng.range(-5.0, 5.0);
        let rr = rng.range(1.8, 3.2);
        let rh = rng.range(1.5, 3.2);
        let rc = *rng.pick(&["#6E6A64", "#7B766F", "#625E59"]);
        push_karst(&mut solid, rx, rz, rr, rh, rc);
    }
    for i in 0..6 {
        let col = if i % 2 == 1 { "#D9473C" } else { "#F4F1EA" };
        solid.push_cyl(
            Vec3::new(lx, GY + 2.6 + i as f32 * 1.6, lz),
            0.95 - i as f32 * 0.05,
            1.00 - i as f32 * 0.05,
            1.6,
            14,
            col,
        );
    }
    solid.push_cyl(Vec3::new(lx, GY + 12.3, lz), 1.05, 1.05, 0.2, 14, "#2B2F36");
    solid.push_cyl(Vec3::new(lx, GY + 14.0, lz), 0.06, 0.85, 1.0, 14, "#2B2F36");

    let lamp_mesh = ba.add_mesh(meshes, Mesh::from(Cylinder::new(0.7, 1.1)));
    let lamp_mat = ba.add_material(
        materials,
        StandardMaterial {
            base_color: srgb_hex("#FFF6D8"),
            emissive: if night {
                LinearRgba::new(3.0, 2.7, 1.8, 1.0)
            } else {
                LinearRgba::new(0.4, 0.35, 0.25, 1.0)
            },
            unlit: true,
            ..default()
        },
    );
    commands.spawn((
        WorldEntity,
        BackdropEntity,
        BeachLighthouseLamp,
        Mesh3d(lamp_mesh),
        MeshMaterial3d(lamp_mat),
        Transform::from_xyz(lx, GY + 12.95, lz),
    ));

    // Dual opposing volumetric rotating cones (`beach.js:71-73`)
    let beam_img = ba.upload_image(images, draw_beam_grad_tex());
    let beam_mat = ba.add_material(
        materials,
        StandardMaterial {
            base_color: Color::srgba(1.0, 0.96, 0.82, 0.65),
            base_color_texture: Some(beam_img),
            alpha_mode: AlphaMode::Add,
            unlit: true,
            cull_mode: None,
            double_sided: true,
            ..default()
        },
    );
    let beam_cone = ba.add_mesh(meshes, Mesh::from(Cone::new(2.6, 34.0)));
    commands
        .spawn((
            WorldEntity,
            BackdropEntity,
            LighthouseBeam,
            Transform::from_xyz(lx, GY + 12.95, lz),
            if night { Visibility::Inherited } else { Visibility::Hidden },
        ))
        .with_children(|b| {
            for &ang in &[0.0_f32, PI] {
                b.spawn((
                    WorldEntity,
                    BackdropEntity,
                    Mesh3d(beam_cone.clone()),
                    MeshMaterial3d(beam_mat.clone()),
                    Transform::from_rotation(Quat::from_rotation_y(ang) * Quat::from_rotation_z(-FRAC_PI_2))
                        .with_translation(Quat::from_rotation_y(ang) * Vec3::new(17.0, 0.0, 0.0)),
                ));
            }
        });

    // 9. 18 Hạ Long-style limestone sea islands (`beach.js:74`)
    for _ in 0..18 {
        let is = rng.range(-150.0, 150.0);
        let id = rng.range(-150.0, -70.0);
        let (x, z) = sd_to_xz(is, id);
        let r = rng.range(4.0, 9.0);
        let h = rng.range(10.0, 24.0);
        let col = *rng.pick(&["#557A49", "#4D7044", "#647F55"]);
        push_karst(&mut greens, x, z, r, h, col);
    }

    // 10. 6 bobbing wooden fishing boats with blue hull, red band, white cabin & red Vietnam flags
    let mut boat_b = StreetMeshBuilder::default();
    boat_b.push_cyl_rot(
        Vec3::new(0.0, 0.40, 0.0),
        0.95,
        0.65,
        5.4,
        8,
        Quat::from_rotation_z(FRAC_PI_2),
        "#2F6FB3",
    );
    boat_b.push_box(Vec3::new(0.0, 0.92, 0.0), Vec3::new(5.1, 0.16, 1.72), "#C8383A");
    boat_b.push_box(Vec3::new(-0.6, 1.48, 0.0), Vec3::new(1.4, 0.90, 1.20), "#F1EDE4");
    boat_b.push_cyl(Vec3::new(-0.6, 2.30, 0.0), 0.03, 0.03, 1.20, 5, "#6B4F35");
    // Red Vietnam flag + tiny gold star accent
    boat_b.push_box(Vec3::new(-0.6, 2.68, 0.34), Vec3::new(0.04, 0.42, 0.64), "#D9302C");
    boat_b.push_box(Vec3::new(-0.6, 2.68, 0.34), Vec3::new(0.05, 0.12, 0.12), "#F7D336");
    let boat_mesh = ba.add_mesh(meshes, boat_b.build());
    let boat_mat = ba.mat(materials, "#FFFFFF", 0.62, 0.05);
    let boat_light_mesh = ba.add_mesh(meshes, Mesh::from(Sphere::new(0.14)));
    let boat_light_mat = ba.add_material(
        materials,
        StandardMaterial {
            base_color: Color::srgb(1.0, 0.88, 0.56),
            emissive: LinearRgba::new(2.6, 2.1, 1.2, 1.0),
            unlit: true,
            ..default()
        },
    );

    for _ in 0..6 {
        let bs = rng.range(-70.0, 70.0);
        let bd = rng.range(-70.0, -40.0);
        let (x, z) = sd_to_xz(bs, bd);
        let yaw = rng.range(0.0, TAU);
        let ph = rng.range(0.0, 6.0);
        let sp = rng.range(-0.25, 0.25);
        commands
            .spawn((
                WorldEntity,
                BackdropEntity,
                BeachBoat {
                    base_y: GY - 0.12,
                    phase: ph,
                    spin: sp,
                },
                Mesh3d(boat_mesh.clone()),
                MeshMaterial3d(boat_mat.clone()),
                Transform::from_xyz(x, GY - 0.12, z).with_rotation(Quat::from_rotation_y(yaw)),
            ))
            .with_children(|g| {
                g.spawn((
                    WorldEntity,
                    BackdropEntity,
                    BeachBoatLight,
                    Mesh3d(boat_light_mesh.clone()),
                    MeshMaterial3d(boat_light_mat.clone()),
                    Transform::from_xyz(-0.6, 2.2, 0.0),
                    if night { Visibility::Inherited } else { Visibility::Hidden },
                ));
            });
    }

    // Finalize merged solid, palms/islands, lit hut windows, and warm string lights
    let solid_mat = ba.mat(materials, "#FFFFFF", 0.80, 0.0);
    solid.spawn(commands, meshes, ba, solid_mat);
    let green_mat = ba.mat(materials, "#FFFFFF", 0.95, 0.0);
    greens.spawn(commands, meshes, ba, green_mat);

    let lit_mat = ba.add_material(
        materials,
        StandardMaterial {
            base_color: if night {
                Color::srgb(1.0, 0.80, 0.45)
            } else {
                Color::srgb(0.08, 0.08, 0.09)
            },
            emissive: if night {
                LinearRgba::new(2.2, 1.55, 0.75, 1.0)
            } else {
                LinearRgba::BLACK
            },
            unlit: true,
            ..default()
        },
    );
    if let Some(le) = lit.spawn(commands, meshes, ba, lit_mat) {
        commands.entity(le).insert(CountryNightLit);
    }

    let str_mat = ba.add_material(
        materials,
        StandardMaterial {
            base_color: Color::srgb(1.0, 0.84, 0.48),
            emissive: LinearRgba::new(2.6, 1.9, 1.0, 1.0),
            unlit: true,
            ..default()
        },
    );
    if let Some(se) = hut_lights.spawn(commands, meshes, ba, str_mat) {
        commands
            .entity(se)
            .insert((BeachHutLights, if night { Visibility::Inherited } else { Visibility::Hidden }));
    }

    // 11. Night moonlight shimmer across the bay (`beach.js:94-97`)
    let glint_img = ba.upload_repeating_image(images, draw_moon_glint_tex());
    let glint_mat = ba.add_material(
        materials,
        StandardMaterial {
            base_color: Color::WHITE,
            base_color_texture: Some(glint_img),
            alpha_mode: AlphaMode::Add,
            unlit: true,
            cull_mode: None,
            double_sided: true,
            ..default()
        },
    );
    let glint_mesh = ba.add_mesh(meshes, Mesh::from(Plane3d::new(Vec3::Y, Vec2::new(4.5, 35.0))));
    let (gx, gz) = sd_to_xz(0.0, -66.0);
    commands.spawn((
        WorldEntity,
        BackdropEntity,
        BeachMoonGlint,
        CountryWater {
            scroll_speed: Vec2::new(0.0, 0.03),
        },
        Mesh3d(glint_mesh),
        MeshMaterial3d(glint_mat),
        Transform::from_xyz(gx, GY - 0.09, gz).with_rotation(Quat::from_rotation_y(FRAC_PI_4)),
        if night { Visibility::Inherited } else { Visibility::Hidden },
    ));

    // 12. 6 flapping seagulls circling overhead (`beach.js:98`)
    let bird_body = ba.add_mesh(meshes, Mesh::from(Sphere::new(0.5)));
    let bird_wing = ba.add_mesh(meshes, Mesh::from(Cuboid::new(1.0, 1.0, 1.0)));
    let gull_mat = ba.mat_double_sided(materials, "#ECEFF2", 0.70, 0.0);
    for i in 0..6 {
        let r = rng.range(14.0, 26.0);
        let cx = rng.range(-30.0, 20.0);
        let cz = rng.range(-50.0, -30.0);
        let y = rng.range(8.0, 13.0);
        let sp = rng.range(0.15, 0.25);
        spawn_flapping_bird(
            commands,
            bird_body.clone(),
            bird_wing.clone(),
            gull_mat.clone(),
            0.90,
            Vec2::new(cx, cz),
            r,
            y,
            i as f32 * 1.1,
            sp,
            6.0,
            0.45,
            night,
            true,
        );
    }
}

/* ============================================================================
 * ZERO-ALLOCATION UPDATE SYSTEM (Animations + Live Day/Night Sync)
 * ============================================================================ */

#[derive(SystemParam)]
pub struct CountryBeachNightMats<'w, 's> {
    added_country: Query<'w, 's, (), Added<CountryGround>>,
    added_beach: Query<'w, 's, (), Added<BeachOcean>>,
    country_ground_q: Query<'w, 's, &'static MeshMaterial3d<StandardMaterial>, With<CountryGround>>,
    lit_q: Query<'w, 's, &'static MeshMaterial3d<StandardMaterial>, With<CountryNightLit>>,
    bulb_q: Query<'w, 's, &'static MeshMaterial3d<StandardMaterial>, With<CountryLanternBulb>>,
    beach_ocean_q: Query<'w, 's, &'static MeshMaterial3d<StandardMaterial>, With<BeachOcean>>,
    beach_shore_q: Query<'w, 's, &'static MeshMaterial3d<StandardMaterial>, With<BeachShore>>,
    beach_shallow_q: Query<'w, 's, &'static MeshMaterial3d<StandardMaterial>, With<BeachShallow>>,
    beach_lamp_q: Query<'w, 's, &'static MeshMaterial3d<StandardMaterial>, With<BeachLighthouseLamp>>,
}

#[allow(clippy::too_many_arguments, clippy::type_complexity)]
pub fn animate_countryside_and_beach_system(
    time: Res<Time>,
    lighting: Option<Res<LightingState>>,
    mut materials: Option<ResMut<Assets<StandardMaterial>>>,
    mut meshes: Option<ResMut<Assets<Mesh>>>,
    water_q: Query<(&CountryWater, &MeshMaterial3d<StandardMaterial>)>,
    buffalo_q: Query<&CountryBuffalo>,
    mut flyer_q: Query<
        (&CountryFlyer, &mut Transform, &mut Visibility),
        (
            Without<CountryKite>,
            Without<BeachWave>,
            Without<BeachBoat>,
            Without<LighthouseBeam>,
            Without<CountryFireflies>,
            Without<CountryNightPool>,
            Without<BeachHutLights>,
            Without<BeachBoatLight>,
            Without<BeachMoonGlint>,
        ),
    >,
    mut kite_q: Query<
        (&CountryKite, &mut Transform),
        (
            Without<CountryFlyer>,
            Without<BeachWave>,
            Without<BeachBoat>,
            Without<LighthouseBeam>,
        ),
    >,
    mut wave_q: Query<
        (&BeachWave, &mut Transform, &MeshMaterial3d<StandardMaterial>),
        (Without<CountryFlyer>, Without<CountryKite>, Without<BeachBoat>, Without<LighthouseBeam>),
    >,
    mut boat_q: Query<
        (&BeachBoat, &mut Transform),
        (Without<CountryFlyer>, Without<CountryKite>, Without<BeachWave>, Without<LighthouseBeam>),
    >,
    mut beam_q: Query<
        (&mut Transform, &mut Visibility),
        (
            With<LighthouseBeam>,
            Without<CountryFlyer>,
            Without<CountryKite>,
            Without<BeachWave>,
            Without<BeachBoat>,
            Without<CountryFireflies>,
            Without<CountryNightPool>,
            Without<BeachHutLights>,
            Without<BeachBoatLight>,
            Without<BeachMoonGlint>,
        ),
    >,
    mut child_tf_q: Query<
        &mut Transform,
        (
            Without<CountryFlyer>,
            Without<CountryKite>,
            Without<BeachWave>,
            Without<BeachBoat>,
            Without<LighthouseBeam>,
        ),
    >,
    mut fireflies_q: Query<
        (&CountryFireflies, &mut Visibility),
        (
            Without<CountryFlyer>,
            Without<LighthouseBeam>,
            Without<CountryNightPool>,
            Without<BeachHutLights>,
            Without<BeachBoatLight>,
            Without<BeachMoonGlint>,
        ),
    >,
    mut night_vis_q: Query<
        &mut Visibility,
        (
            Or<(
                With<CountryNightPool>,
                With<BeachHutLights>,
                With<BeachBoatLight>,
                With<BeachMoonGlint>,
            )>,
            Without<CountryFlyer>,
            Without<LighthouseBeam>,
            Without<CountryFireflies>,
        ),
    >,
    night_mats: CountryBeachNightMats,
    mut last_night: Local<Option<bool>>,
) {
    let dt = time.delta_secs().clamp(0.0, 0.1);
    let t = time.elapsed_secs();
    let is_night = lighting.as_ref().is_some_and(|l| l.is_night());

    // 1. Scroll river, flooded paddies, ocean & moon glint UV offsets in-place
    if let Some(mats) = materials.as_deref_mut() {
        for (w, mh) in &water_q {
            if let Some(m) = mats.get_mut(&mh.0) {
                m.uv_transform.translation.x = (m.uv_transform.translation.x + w.scroll_speed.x * dt).rem_euclid(1.0);
                m.uv_transform.translation.y = (m.uv_transform.translation.y + w.scroll_speed.y * dt).rem_euclid(1.0);
            }
        }
    }

    // 2. Grazing water buffalo head bob
    for b in &buffalo_q {
        if let Ok(mut htf) = child_tf_q.get_mut(b.head_pivot) {
            htf.rotation = Quat::from_rotation_z(-0.55 + (t * 0.7 + b.phase).sin() * 0.18);
        }
    }

    // 3. Flapping egrets & seagulls circling overhead
    for (f, mut tf, _) in &mut flyer_q {
        let a = t * f.speed + f.phase;
        tf.translation = Vec3::new(
            f.center.x + a.cos() * f.radius,
            f.altitude + (a * 2.0).sin() * 0.6,
            f.center.y + a.sin() * f.radius,
        );
        tf.rotation = Quat::from_rotation_y(-a);
        let flap = (t * f.flap_freq + f.phase).sin() * f.flap_amp;
        if let Ok(mut lw) = child_tf_q.get_mut(f.left_wing) {
            lw.rotation = Quat::from_rotation_z(flap);
        }
        if let Ok(mut rw) = child_tf_q.get_mut(f.right_wing) {
            rw.rotation = Quat::from_rotation_z(-flap);
        }
    }

    // 4. Kids' diamond kites swaying + live string transform connecting kid's hand to kite tail
    for (k, mut tf) in &mut kite_q {
        let kpos = Vec3::new(
            k.base_pos.x + (t * 0.5 + k.phase).sin() * 1.6,
            k.base_pos.y + (t * 0.9 + k.phase).sin() * 0.8,
            k.base_pos.z + (t * 0.4 + k.phase).cos() * 1.2,
        );
        tf.translation = kpos;
        tf.rotation = Quat::from_rotation_y(FRAC_PI_4)
            * Quat::from_rotation_z((t * 1.3 + k.phase).sin() * 0.25);
        let tail = Vec3::new(kpos.x, kpos.y - 1.0, kpos.z);
        let diff = tail - k.hand_anchor;
        let len = diff.length().max(0.05);
        if let Ok(mut stf) = child_tf_q.get_mut(k.string_entity) {
            stf.translation = (k.hand_anchor + tail) * 0.5;
            stf.rotation = Quat::from_rotation_arc(Vec3::Y, diff / len);
            stf.scale = Vec3::new(1.0, len, 1.0);
        }
    }

    // 5. Rolling beach surf foam waves
    for (w, mut tf, mh) in &mut wave_q {
        let k = ((t * 0.6 + w.phase).sin() + 1.0) * 0.5;
        tf.translation = Vec3::new(-k * 1.4, 0.0, -k * 1.4);
        if let Some(mats) = materials.as_deref_mut()
            && let Some(m) = mats.get_mut(&mh.0)
        {
            let alpha = 0.25 + 0.75 * (1.0 - k);
            m.base_color = Color::srgba(1.0, 1.0, 1.0, if is_night { alpha * 0.55 } else { alpha });
        }
    }

    // 6. Bobbing wooden fishing boats
    for (b, mut tf) in &mut boat_q {
        tf.translation.y = b.base_y + (t * 1.2 + b.phase).sin() * 0.08;
        let roll = (t * 0.9 + b.phase).sin() * 0.05;
        tf.rotation = tf.rotation * Quat::from_rotation_y(b.spin * dt * 0.05);
        let (yaw, _, _) = tf.rotation.to_euler(EulerRot::YXZ);
        tf.rotation = Quat::from_rotation_y(yaw) * Quat::from_rotation_z(roll);
    }

    // 7. Rotating lighthouse beam
    for (mut tf, _) in &mut beam_q {
        tf.rotation = Quat::from_rotation_y(t * 0.6);
    }

    // 8. Live night sync (runs on toggle OR immediately after SwitchBackdrop via Added<...>)
    let night_dirty = *last_night != Some(is_night)
        || !night_mats.added_country.is_empty()
        || !night_mats.added_beach.is_empty();
    if night_dirty {
        *last_night = Some(is_night);

        let night_vis = if is_night { Visibility::Inherited } else { Visibility::Hidden };
        let day_vis = if is_night { Visibility::Hidden } else { Visibility::Inherited };

        for (_, _, mut vis) in &mut flyer_q {
            *vis = day_vis;
        }
        for (_, mut vis) in &mut beam_q {
            *vis = night_vis;
        }
        for (_, mut vis) in &mut fireflies_q {
            *vis = night_vis;
        }
        for mut vis in &mut night_vis_q {
            *vis = night_vis;
        }

        if let Some(mats) = materials.as_deref_mut() {
            for mh in &night_mats.country_ground_q {
                if let Some(m) = mats.get_mut(&mh.0) {
                    m.base_color = srgb_hex(if is_night { "#5E7D45" } else { "#86A85B" });
                }
            }
            for mh in &night_mats.lit_q {
                if let Some(m) = mats.get_mut(&mh.0) {
                    m.base_color = if is_night {
                        Color::srgb(1.0, 0.80, 0.45)
                    } else {
                        Color::srgb(0.08, 0.08, 0.09)
                    };
                    m.emissive = if is_night {
                        LinearRgba::new(2.2, 1.55, 0.75, 1.0)
                    } else {
                        LinearRgba::BLACK
                    };
                }
            }
            for mh in &night_mats.bulb_q {
                if let Some(m) = mats.get_mut(&mh.0) {
                    m.base_color = srgb_hex(if is_night { "#FFE5AA" } else { "#EDE6D6" });
                    m.emissive = if is_night {
                        LinearRgba::new(2.8, 2.1, 1.1, 1.0)
                    } else {
                        LinearRgba::BLACK
                    };
                }
            }
            for mh in &night_mats.beach_ocean_q {
                if let Some(m) = mats.get_mut(&mh.0) {
                    m.base_color = srgb_hex(if is_night { "#7C93B8" } else { "#FFFFFF" });
                }
            }
            for mh in &night_mats.beach_shore_q {
                if let Some(m) = mats.get_mut(&mh.0) {
                    m.base_color = srgb_hex(if is_night { "#8C93A8" } else { "#FFFFFF" });
                }
            }
            for mh in &night_mats.beach_shallow_q {
                if let Some(m) = mats.get_mut(&mh.0) {
                    m.base_color = Color::srgba(0.50, 0.84, 0.86, if is_night { 0.14 } else { 0.45 });
                }
            }
            for mh in &night_mats.beach_lamp_q {
                if let Some(m) = mats.get_mut(&mh.0) {
                    m.emissive = if is_night {
                        LinearRgba::new(3.0, 2.7, 1.8, 1.0)
                    } else {
                        LinearRgba::new(0.4, 0.35, 0.25, 1.0)
                    };
                }
            }
        }
    }

    // 9. Animate 260 fireflies in-place at night (zero heap allocations!)
    if is_night
        && let Some(mesh_assets) = meshes.as_deref_mut()
    {
        for (ff, _) in &fireflies_q {
            if let Some(mesh) = mesh_assets.get_mut(&ff.mesh) {
                let r = 0.085_f32;
                if let Some(VertexAttributeValues::Float32x3(pos)) =
                    mesh.attribute_mut(Mesh::ATTRIBUTE_POSITION)
                    && pos.len() == ff.base_positions.len() * 4
                {
                    for (i, (&base, &ph)) in ff.base_positions.iter().zip(&ff.phases).enumerate() {
                        let px = base.x + (t * 0.3 + ph).sin() * 0.8;
                        let py = base.y + (t * 0.5 + ph * 2.0).sin() * 0.4;
                        let pz = base.z + (t * 0.27 + ph).cos() * 0.8;
                        let k = i * 4;
                        pos[k] = [px - r, py - r, pz];
                        pos[k + 1] = [px + r, py - r, pz];
                        pos[k + 2] = [px + r, py + r, pz];
                        pos[k + 3] = [px - r, py + r, pz];
                    }
                }
                if let Some(VertexAttributeValues::Float32x4(cols)) =
                    mesh.attribute_mut(Mesh::ATTRIBUTE_COLOR)
                    && cols.len() == ff.phases.len() * 4
                {
                    for (i, &ph) in ff.phases.iter().enumerate() {
                        let b = (t * 1.7 + ph * 3.0).sin().max(0.0).powi(3);
                        let c = [1.6 * b, 2.4 * b, 0.5 * b, 1.0];
                        let k = i * 4;
                        cols[k] = c;
                        cols[k + 1] = c;
                        cols[k + 2] = c;
                        cols[k + 3] = c;
                    }
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::layout::OfficeLayout;
    use std::path::PathBuf;
    use std::time::Duration;

    #[test]
    fn countryside_spawns_river_bridge_gate_buffalo_egrets_kites_and_260_fireflies_with_night_sync() {
        let assets_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../assets");
        let mut fonts = FontEngine::from_assets_dir(&assets_dir);
        let spec = LayoutSpec::for_layout(OfficeLayout::Pro);
        let excl = terrace_exclusion(OfficeLayout::Pro);

        let mut app = App::new();
        app.insert_resource(Time::<()>::default())
            .insert_resource(LightingState::new(false))
            .insert_resource(Assets::<Mesh>::default())
            .insert_resource(Assets::<StandardMaterial>::default())
            .insert_resource(Assets::<Image>::default())
            .insert_resource(BackdropAssets::default())
            .add_systems(Update, animate_countryside_and_beach_system);

        app.world_mut().resource_scope(|world, mut meshes: Mut<Assets<Mesh>>| {
            world.resource_scope(|world, mut mats: Mut<Assets<StandardMaterial>>| {
                world.resource_scope(|world, mut imgs: Mut<Assets<Image>>| {
                    world.resource_scope(|world, mut ba: Mut<BackdropAssets>| {
                        let mut cmds = world.commands();
                        spawn_countryside_backdrop(
                            &mut cmds,
                            &mut meshes,
                            &mut mats,
                            &mut imgs,
                            &mut ba,
                            &mut fonts,
                            &spec,
                            false,
                        );
                    });
                });
            });
        });
        app.world_mut().flush();

        // River, village gate, buffalo, egrets, kites, and 260 fireflies must all exist
        assert!(
            app.world_mut()
                .query_filtered::<Entity, With<CountryWater>>()
                .iter(app.world())
                .count()
                >= 2,
            "Countryside must spawn meandering river and flooded paddy water"
        );
        assert_eq!(
            app.world_mut()
                .query_filtered::<Entity, With<CountryVillageGate>>()
                .iter(app.world())
                .count(),
            1,
            "Countryside must spawn red-tile cổng làng village gate"
        );
        let buffalos: Vec<Vec3> = app
            .world_mut()
            .query_filtered::<&Transform, With<CountryBuffalo>>()
            .iter(app.world())
            .map(|t| t.translation)
            .collect();
        assert!(buffalos.len() >= 3, "Countryside must spawn grazing water buffalo");
        for pos in &buffalos {
            assert!(
                !rect_contains_padded(excl, pos.x, pos.z, 0.2),
                "buffalo at {pos:?} must clear Pro terrace"
            );
        }

        let flyers = app
            .world_mut()
            .query_filtered::<Entity, (With<CountryFlyer>, Without<BeachSeagull>)>()
            .iter(app.world())
            .count();
        assert_eq!(flyers, 5, "Countryside must spawn 5 flapping white egrets");

        let kites: Vec<Vec3> = app
            .world_mut()
            .query_filtered::<&CountryKite, With<BackdropEntity>>()
            .iter(app.world())
            .map(|k| k.hand_anchor)
            .collect();
        assert!(kites.len() >= 2, "Countryside must spawn kids flying diamond kites");
        for pos in &kites {
            assert!(
                !rect_contains_padded(excl, pos.x, pos.z, 0.2),
                "kite kid at {pos:?} must clear Pro terrace"
            );
        }

        let ff = app
            .world_mut()
            .query::<(&CountryFireflies, &Visibility)>()
            .single(app.world())
            .expect("Countryside must spawn CountryFireflies");
        assert_eq!(ff.0.base_positions.len(), COUNTRY_FIREFLY_COUNT);
        for p in &ff.0.base_positions {
            assert!(
                !rect_contains_padded(excl, p.x, p.z, 0.0),
                "firefly at {p:?} must stay outside terrace"
            );
        }
        assert_eq!(*ff.1, Visibility::Hidden, "fireflies must be hidden in daytime");

        // Advance day tick, then switch to night and verify live night sync
        app.world_mut()
            .resource_mut::<Time>()
            .advance_by(Duration::from_millis(250));
        app.update();

        app.world_mut().resource_mut::<LightingState>().night = true;
        app.world_mut()
            .resource_mut::<Time>()
            .advance_by(Duration::from_millis(250));
        app.update();

        let ff_vis = *app
            .world_mut()
            .query_filtered::<&Visibility, With<CountryFireflies>>()
            .single(app.world())
            .unwrap();
        assert_eq!(ff_vis, Visibility::Inherited, "fireflies must become visible at night");

        // Verify all images created are owned by BackdropAssets (0 stray assets)
        assert!(
            app.world().resource::<BackdropAssets>().owned_images.len() >= 3,
            "Countryside must register river/field/gate images in BackdropAssets"
        );
    }

    #[test]
    fn beach_spawns_shoreline_waves_70_palms_boats_huts_lighthouse_and_seagulls_with_night_sync() {
        let spec = LayoutSpec::for_layout(OfficeLayout::Pro);
        let excl = terrace_exclusion(OfficeLayout::Pro);

        let mut app = App::new();
        app.insert_resource(Time::<()>::default())
            .insert_resource(LightingState::new(false))
            .insert_resource(Assets::<Mesh>::default())
            .insert_resource(Assets::<StandardMaterial>::default())
            .insert_resource(Assets::<Image>::default())
            .insert_resource(BackdropAssets::default())
            .add_systems(Update, animate_countryside_and_beach_system);

        app.world_mut().resource_scope(|world, mut meshes: Mut<Assets<Mesh>>| {
            world.resource_scope(|world, mut mats: Mut<Assets<StandardMaterial>>| {
                world.resource_scope(|world, mut imgs: Mut<Assets<Image>>| {
                    world.resource_scope(|world, mut ba: Mut<BackdropAssets>| {
                        let mut cmds = world.commands();
                        spawn_beach_backdrop(
                            &mut cmds,
                            &mut meshes,
                            &mut mats,
                            &mut imgs,
                            &mut ba,
                            &spec,
                            false,
                        );
                    });
                });
            });
        });
        app.world_mut().flush();

        assert_eq!(
            app.world_mut()
                .query_filtered::<Entity, With<BeachShore>>()
                .iter(app.world())
                .count(),
            1,
            "Beach must spawn vertex-colored wet/dry golden shoreline"
        );
        assert_eq!(
            app.world_mut()
                .query_filtered::<Entity, With<BeachWave>>()
                .iter(app.world())
                .count(),
            2,
            "Beach must spawn 2 rolling surf foam wave ribbons"
        );
        let palm_meta = app
            .world_mut()
            .query::<&BeachPalmMeta>()
            .single(app.world())
            .expect("Beach must record palm count");
        assert_eq!(palm_meta.count, BEACH_PALM_COUNT, "Beach must place 70 coconut palms");

        let boats: Vec<Vec3> = app
            .world_mut()
            .query_filtered::<&Transform, With<BeachBoat>>()
            .iter(app.world())
            .map(|t| t.translation)
            .collect();
        assert_eq!(boats.len(), 6, "Beach must spawn 6 bobbing fishing boats");
        for pos in &boats {
            assert!(
                !rect_contains_padded(excl, pos.x, pos.z, 1.0),
                "boat at {pos:?} must be out on water clear of terrace"
            );
        }

        assert_eq!(
            app.world_mut()
                .query_filtered::<Entity, With<BeachSeagull>>()
                .iter(app.world())
                .count(),
            6,
            "Beach must spawn 6 flapping seagulls"
        );

        let beam_day_vis = *app
            .world_mut()
            .query_filtered::<&Visibility, With<LighthouseBeam>>()
            .single(app.world())
            .expect("Beach must spawn rotating LighthouseBeam");
        assert_eq!(beam_day_vis, Visibility::Hidden, "lighthouse beam hidden in day");

        // Switch to night and verify lighthouse beam turns visible and rotates
        app.world_mut().resource_mut::<LightingState>().night = true;
        app.world_mut()
            .resource_mut::<Time>()
            .advance_by(Duration::from_millis(500));
        app.update();

        let (beam_tf1, beam_vis) = app
            .world_mut()
            .query_filtered::<(&Transform, &Visibility), With<LighthouseBeam>>()
            .single(app.world())
            .unwrap();
        assert_eq!(*beam_vis, Visibility::Inherited, "lighthouse beam visible at night");
        let rot1 = beam_tf1.rotation;

        app.world_mut()
            .resource_mut::<Time>()
            .advance_by(Duration::from_millis(500));
        app.update();
        let rot2 = app
            .world_mut()
            .query_filtered::<&Transform, With<LighthouseBeam>>()
            .single(app.world())
            .unwrap()
            .rotation;
        assert!(rot1.angle_between(rot2) > 0.1, "lighthouse beam must rotate over time");
    }
}
