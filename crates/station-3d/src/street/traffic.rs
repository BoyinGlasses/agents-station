//! 1:1 Street traffic engine: decodes all 11 Kenney Car Kit models + wheel from `cars.json`,
//! adds procedural VinFast VF3, Xanh SM, city buses, Grab/Xanh SM scooters, sunny `"ninja Lead"`
//! sun-suits, 6 bulky loads, 115/114 sirens, ≥33°C shade-stopping, and zebra pedestrians
//! (`street-life.js`, `street-riders.js`, `street-shade.js`, `street-siren.js`, `street-folk.js`).

use crate::backdrops::{BackdropAssets, BackdropEntity};
use crate::palette::WorldEntity;
use crate::street::city_grid::{CityGridData, STREET_W, signal_color_of, signal_phase};
use crate::street::{CityRng, GY, PAVE_Y, StreetMeshBuilder, spawn_backdrop_uv_quad};
use bevy::asset::RenderAssetUsages;
use bevy::prelude::*;
use bevy::render::mesh::{Indices, PrimitiveTopology};
use serde::Deserialize;
use station_canvas2d::street_surfaces::SignalColor;
use std::collections::HashMap;
use std::f32::consts::{FRAC_PI_2, PI};

const CARS_JSON: &str = include_str!("../../../../assets/vehicles/cars.json");

pub const VEHICLE_SPECS_COUNT: usize = 15;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum VehicleKind {
    Vios,
    City,
    Morning,
    Fortuner,
    Vf8,
    Ranger,
    Xpander,
    Delivery,
    Taxi,
    Ambulance,
    Firetruck,
    Vf3,
    XanhSm,
    Bus,
    Scooter,
}

impl VehicleKind {
    pub const ALL: [VehicleKind; 15] = [
        Self::Vios,
        Self::City,
        Self::Morning,
        Self::Fortuner,
        Self::Vf8,
        Self::Ranger,
        Self::Xpander,
        Self::Delivery,
        Self::Taxi,
        Self::Ambulance,
        Self::Firetruck,
        Self::Vf3,
        Self::XanhSm,
        Self::Bus,
        Self::Scooter,
    ];

    pub const CAR_SPECS_13: [VehicleKind; 13] = [
        Self::Vios,
        Self::City,
        Self::Morning,
        Self::Fortuner,
        Self::Vf8,
        Self::Ranger,
        Self::Xpander,
        Self::Delivery,
        Self::Taxi,
        Self::Ambulance,
        Self::Firetruck,
        Self::Vf3,
        Self::XanhSm,
    ];

    fn kit_key(self) -> Option<&'static str> {
        match self {
            Self::Vios => Some("sedan"),
            Self::City => Some("sedan-sports"),
            Self::Morning => Some("hatchback-sports"),
            Self::Fortuner => Some("suv"),
            Self::Vf8 => Some("suv-luxury"),
            Self::Ranger => Some("truck"),
            Self::Xpander => Some("van"),
            Self::Delivery => Some("delivery"),
            Self::Taxi => Some("taxi"),
            Self::Ambulance => Some("ambulance"),
            Self::Firetruck => Some("firetruck"),
            Self::Vf3 | Self::XanhSm | Self::Bus | Self::Scooter => None,
        }
    }

    pub fn paints(self) -> &'static [&'static str] {
        match self {
            Self::Vios => &["#F2F2F2", "#C9CED6", "#2B2F36", "#8E2B2B", "#5C6B7A"],
            Self::City => &["#F2F2F2", "#B3121E", "#3B3B3B", "#9AA2AD", "#1F3A5F"],
            Self::Morning => &["#F2F2F2", "#D9534F", "#F2C200", "#2E86DE", "#7FB77E"],
            Self::Fortuner => &["#F2F2F2", "#2B2F36", "#6B4F3A", "#8A939B"],
            Self::Vf8 => &["#F2F2F2", "#24272B", "#1F4E8C", "#9A1B2F", "#7FB7A8"],
            Self::Ranger => &["#E0662B", "#F2F2F2", "#2B2F36", "#1F4E8C"],
            Self::Xpander => &["#F2F2F2", "#B9BEC6", "#6B4F3A", "#2B2F36"],
            Self::Delivery => &["#F2F2F2", "#E0662B", "#2E86DE"],
            Self::Taxi => &["#1E9E4A", "#F5F5F5"],
            Self::Ambulance => &["#F5F7FA"],
            Self::Firetruck => &["#C8102E"],
            Self::Vf3 => &["#F4D03F", "#7EC8E3", "#F2A7B8", "#B8D84A", "#F2F2F2", "#E0662B"],
            Self::XanhSm => &["#22B8BE"],
            Self::Bus => &["#2E9E4A", "#F0B323", "#D94B3D"],
            Self::Scooter => &["#D9534F", "#F2F2F2", "#2E86DE", "#3B3B3B", "#00B14F", "#22B8BE", "#F2C200"],
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BulkyLoadKind {
    None,
    GrabFoodBox,
    Fridge,
    Ladder,
    GasCylinder,
    PottedTree,
    ChickenCage,
    StyrofoamStack,
}

#[derive(Component, Debug, Clone)]
pub struct CityVehicle {
    pub axis_x: bool,
    pub dir: f32,
    pub lane_coord: f32,
    pub pos: f32,
    pub min_pos: f32,
    pub max_pos: f32,
    pub speed: f32,
    pub stop_cross: f32,
    pub sig_i: usize,
    pub sig_j: usize,
    pub is_scooter: bool,
    pub is_siren: bool,
    pub is_bus: bool,
    pub bus_stop_pos: f32,
    pub shade_offset: f32,
    pub wait_timer: f32,
}

#[derive(Component)]
pub struct SirenLightPair {
    pub is_red: bool,
}

#[derive(Component)]
pub struct StreetPedestrian {
    pub axis_x: bool,
    pub dir: f32,
    pub side_coord: f32,
    pub pos: f32,
    pub min_pos: f32,
    pub max_pos: f32,
    pub speed: f32,
    pub phase: f32,
}

pub struct VehicleMeshPair {
    pub paint: Handle<Mesh>,
    pub rest: Handle<Mesh>,
}

#[derive(Deserialize)]
struct CarsCatalog {
    scale: f32,
    models: HashMap<String, CarModelJson>,
    wheel: PackedMeshJson,
}

#[derive(Deserialize)]
struct CarModelJson {
    paint: PackedMeshJson,
    rest: PackedMeshJson,
    wheels: Vec<WheelXformJson>,
    #[serde(rename = "box")]
    bounds: [f32; 6],
}

#[derive(Deserialize)]
struct PackedMeshJson {
    pos: String,
    nrm: String,
    col: String,
    idx: String,
    verts: usize,
}

#[derive(Deserialize)]
struct WheelXformJson {
    t: [f32; 3],
    r: [f32; 4],
    s: [f32; 3],
}

fn decode_b64(input: &str) -> Vec<u8> {
    let mut out = Vec::with_capacity((input.len() * 3) / 4);
    let mut buf = 0u32;
    let mut bits = 0u8;
    for b in input.bytes() {
        let val = match b {
            b'A'..=b'Z' => b - b'A',
            b'a'..=b'z' => b - b'a' + 26,
            b'0'..=b'9' => b - b'0' + 52,
            b'+' => 62,
            b'/' => 63,
            b'=' => break,
            _ => continue,
        };
        buf = (buf << 6) | (val as u32);
        bits += 6;
        if bits >= 8 {
            bits -= 8;
            out.push(((buf >> bits) & 0xFF) as u8);
        }
    }
    out
}

struct DecodedSubMesh {
    positions: Vec<[f32; 3]>,
    normals: Vec<[f32; 3]>,
    colors: Vec<[f32; 4]>,
    indices: Vec<u32>,
}

fn decode_packed(p: &PackedMeshJson) -> DecodedSubMesh {
    let pos_bytes = decode_b64(&p.pos);
    let nrm_bytes = decode_b64(&p.nrm);
    let col_bytes = decode_b64(&p.col);
    let idx_bytes = decode_b64(&p.idx);

    let mut positions = Vec::with_capacity(p.verts);
    for chunk in pos_bytes.chunks_exact(6) {
        let x = i16::from_le_bytes([chunk[0], chunk[1]]) as f32 / 2000.0;
        let y = i16::from_le_bytes([chunk[2], chunk[3]]) as f32 / 2000.0;
        let z = i16::from_le_bytes([chunk[4], chunk[5]]) as f32 / 2000.0;
        positions.push([x, y, z]);
    }
    let mut normals = Vec::with_capacity(p.verts);
    for chunk in nrm_bytes.chunks_exact(3) {
        let nx = (chunk[0] as i8) as f32 / 127.0;
        let ny = (chunk[1] as i8) as f32 / 127.0;
        let nz = (chunk[2] as i8) as f32 / 127.0;
        normals.push(Vec3::new(nx, ny, nz).normalize_or_zero().to_array());
    }
    let mut colors = Vec::with_capacity(p.verts);
    for chunk in col_bytes.chunks_exact(3) {
        let lin = Color::srgb_u8(chunk[0], chunk[1], chunk[2]).to_linear();
        colors.push([lin.red, lin.green, lin.blue, 1.0]);
    }
    let mut indices = Vec::with_capacity(idx_bytes.len() / 2);
    for chunk in idx_bytes.chunks_exact(2) {
        indices.push(u16::from_le_bytes([chunk[0], chunk[1]]) as u32);
    }

    DecodedSubMesh {
        positions,
        normals,
        colors,
        indices,
    }
}

fn append_wheel_to_rest(
    dst: &mut DecodedSubMesh,
    wheel: &DecodedSubMesh,
    wf: &WheelXformJson,
    scale: f32,
) {
    let base = dst.positions.len() as u32;
    let q = Quat::from_xyzw(wf.r[0], wf.r[1], wf.r[2], wf.r[3]).normalize();
    let t = Vec3::from_array(wf.t);
    let s = Vec3::from_array(wf.s);
    // build-cars.mjs:59 `ours([x, y, z]) = [z * scale, y * scale, -x * scale]`
    for i in 0..wheel.positions.len() {
        let lp = Vec3::from_array(wheel.positions[i]) * s;
        let wp = q * lp + t;
        dst.positions.push([wp.z * scale, wp.y * scale, -wp.x * scale]);
        let wn = q * Vec3::from_array(wheel.normals[i]);
        dst.normals.push(Vec3::new(wn.z, wn.y, -wn.x).normalize_or_zero().to_array());
        dst.colors.push(wheel.colors[i]);
    }
    for &idx in &wheel.indices {
        dst.indices.push(base + idx);
    }
}

fn decoded_to_mesh(sub: DecodedSubMesh) -> Mesh {
    let uvs = vec![[0.0_f32, 0.0_f32]; sub.positions.len()];
    let mut mesh = Mesh::new(
        PrimitiveTopology::TriangleList,
        RenderAssetUsages::MAIN_WORLD | RenderAssetUsages::RENDER_WORLD,
    );
    mesh.insert_attribute(Mesh::ATTRIBUTE_POSITION, sub.positions);
    mesh.insert_attribute(Mesh::ATTRIBUTE_NORMAL, sub.normals);
    mesh.insert_attribute(Mesh::ATTRIBUTE_UV_0, uvs);
    mesh.insert_attribute(Mesh::ATTRIBUTE_COLOR, sub.colors);
    mesh.insert_indices(Indices::U32(sub.indices));
    mesh
}

fn push_wheels_procedural(b: &mut StreetMeshBuilder, xs: &[f32], half_z: f32, r: f32, w: f32) {
    let rot = Quat::from_rotation_x(FRAC_PI_2);
    for &x in xs {
        for s in [-1.0_f32, 1.0] {
            b.push_cyl_rot(Vec3::new(x, r, s * half_z), r, r, w, 10, rot, "#16181C");
            b.push_cyl_rot(
                Vec3::new(x, r, s * half_z),
                r * 0.5,
                r * 0.5,
                w + 0.02,
                8,
                rot,
                "#AEB4BC",
            );
        }
    }
}

fn push_lights_procedural(b: &mut StreetMeshBuilder, front: f32, back: f32, y: f32, z: f32, w: f32) {
    for s in [-1.0_f32, 1.0] {
        b.push_box(Vec3::new(front, y, s * z), Vec3::new(0.05, 0.12, w), "#FFF4D6");
        b.push_box(Vec3::new(back, y + 0.04, s * z), Vec3::new(0.05, 0.12, w * 0.9), "#C0392B");
    }
}

fn build_procedural_vehicle(kind: VehicleKind) -> (Mesh, Mesh) {
    let mut body = StreetMeshBuilder::default();
    let mut trim = StreetMeshBuilder::default();
    match kind {
        VehicleKind::Vf3 => {
            body.push_box(Vec3::new(0.0, 0.66, 0.0), Vec3::new(2.96, 0.66, 1.60), "#FFFFFF");
            body.push_box(Vec3::new(-0.2, 1.66, 0.0), Vec3::new(2.04, 0.12, 1.56), "#FFFFFF");
            trim.push_box(Vec3::new(-0.2, 1.28, 0.0), Vec3::new(1.98, 0.64, 1.52), "#1C232B");
            trim.push_box(Vec3::new(0.0, 0.36, 0.0), Vec3::new(3.04, 0.20, 1.66), "#1E2125");
            trim.push_box(Vec3::new(1.0, 0.50, 0.0), Vec3::new(0.72, 0.32, 1.68), "#1E2125");
            trim.push_box(Vec3::new(-1.0, 0.50, 0.0), Vec3::new(0.72, 0.32, 1.68), "#1E2125");
            trim.push_box(Vec3::new(1.50, 0.92, 0.0), Vec3::new(0.04, 0.05, 1.24), "#F4F7FA");
            trim.push_box(Vec3::new(1.52, 0.50, 0.0), Vec3::new(0.03, 0.12, 0.32), "#F2F2F2");
            trim.push_box(Vec3::new(-1.52, 0.52, 0.0), Vec3::new(0.03, 0.12, 0.32), "#F2F2F2");
            push_wheels_procedural(&mut trim, &[1.0, -1.0], 0.74, 0.34, 0.24);
            push_lights_procedural(&mut trim, 1.50, -1.50, 0.74, 0.62, 0.24);
        }
        VehicleKind::XanhSm => {
            body.push_box(Vec3::new(0.0, 0.70, 0.0), Vec3::new(3.52, 0.56, 1.56), "#FFFFFF");
            body.push_box(Vec3::new(-0.26, 1.46, 0.0), Vec3::new(2.12, 0.10, 1.44), "#FFFFFF");
            trim.push_box(Vec3::new(-0.26, 1.18, 0.0), Vec3::new(2.20, 0.50, 1.46), "#1C232B");
            trim.push_box(Vec3::new(0.0, 0.38, 0.0), Vec3::new(3.56, 0.22, 1.60), "#1E2125");
            // Yellow commercial plates + VinFast nose light bar
            trim.push_box(Vec3::new(1.79, 0.44, 0.0), Vec3::new(0.04, 0.15, 0.34), "#F2C200");
            trim.push_box(Vec3::new(-1.79, 0.48, 0.0), Vec3::new(0.04, 0.15, 0.34), "#F2C200");
            trim.push_box(Vec3::new(1.77, 0.84, 0.0), Vec3::new(0.04, 0.05, 1.18), "#F4F7FA");
            push_wheels_procedural(&mut trim, &[1.14, -1.14], 0.72, 0.32, 0.22);
            push_lights_procedural(&mut trim, 1.77, -1.77, 0.68, 0.54, 0.24);
        }
        VehicleKind::Bus => {
            body.push_box(Vec3::new(0.0, 1.60, 0.0), Vec3::new(7.4, 2.46, 2.16), "#FFFFFF");
            trim.push_box(Vec3::new(-0.3, 2.10, 0.0), Vec3::new(6.6, 0.85, 2.20), "#22303D");
            trim.push_box(Vec3::new(3.71, 2.00, 0.0), Vec3::new(0.06, 1.25, 1.96), "#22303D");
            trim.push_box(Vec3::new(3.72, 2.64, 0.0), Vec3::new(0.07, 0.24, 1.38), "#FFB23F");
            trim.push_box(Vec3::new(-1.0, 2.94, 0.0), Vec3::new(2.2, 0.22, 1.36), "#C9CED6");
            trim.push_box(Vec3::new(0.0, 0.50, 0.0), Vec3::new(7.42, 0.30, 2.18), "#2B2F36");
            push_wheels_procedural(&mut trim, &[2.4, -2.3], 0.98, 0.46, 0.28);
            push_lights_procedural(&mut trim, 3.72, -3.72, 0.74, 0.78, 0.30);
        }
        VehicleKind::Scooter => {
            // Step-through Vietnamese scooter (`street-life.js:122-141`)
            body.push_box_rot(Vec3::new(0.47, 0.74, 0.0), Vec3::new(0.16, 0.66, 0.42), Quat::from_rotation_z(0.22), "#FFFFFF");
            body.push_box(Vec3::new(0.57, 1.10, 0.0), Vec3::new(0.24, 0.15, 0.46), "#FFFFFF");
            body.push_box_rot(Vec3::new(0.66, 0.60, 0.0), Vec3::new(0.40, 0.07, 0.16), Quat::from_rotation_z(-0.35), "#FFFFFF");
            body.push_box(Vec3::new(-0.36, 0.62, 0.0), Vec3::new(0.86, 0.34, 0.34), "#FFFFFF");
            trim.push_box(Vec3::new(0.10, 0.36, 0.0), Vec3::new(0.52, 0.05, 0.34), "#3A3F47");
            trim.push_box(Vec3::new(-0.30, 0.84, 0.0), Vec3::new(0.76, 0.10, 0.32), "#1C1E22");
            trim.push_bar(Vec3::new(0.64, 0.30, 0.0), Vec3::new(0.55, 1.04, 0.0), 0.03, 6, "#2B2F36");
            trim.push_bar(Vec3::new(0.50, 1.13, -0.30), Vec3::new(0.50, 1.13, 0.30), 0.02, 6, "#2B2F36");
            trim.push_box(Vec3::new(0.69, 1.09, 0.0), Vec3::new(0.04, 0.07, 0.17), "#FFF4D6");
            trim.push_box(Vec3::new(-0.80, 0.72, 0.0), Vec3::new(0.04, 0.06, 0.18), "#C0392B");
            trim.push_box(Vec3::new(-0.81, 0.52, 0.0), Vec3::new(0.02, 0.09, 0.17), "#F2F2F2");
            let wrot = Quat::from_rotation_x(FRAC_PI_2);
            for x in [0.62_f32, -0.60] {
                trim.push_cyl_rot(Vec3::new(x, 0.27, 0.0), 0.27, 0.27, 0.10, 12, wrot, "#16181C");
                trim.push_cyl_rot(Vec3::new(x, 0.27, 0.0), 0.13, 0.13, 0.11, 8, wrot, "#9AA2AD");
            }
            // Seated driver (hands on bars, helmet, trousers)
            trim.push_box_rot(Vec3::new(-0.14, 1.20, 0.0), Vec3::new(0.30, 0.48, 0.38), Quat::from_rotation_z(-0.10), "#3A5A8C");
            trim.push_sphere(Vec3::new(-0.10, 1.58, 0.0), Vec3::splat(0.125), "#E8C2A0");
            trim.push_sphere(Vec3::new(-0.11, 1.63, 0.0), Vec3::new(0.14, 0.11, 0.14), "#2B2F36");
            for s in [-1.0_f32, 1.0] {
                trim.push_bar(Vec3::new(-0.10, 1.38, s * 0.20), Vec3::new(0.47, 1.12, s * 0.27), 0.052, 6, "#3A5A8C");
                trim.push_bar(Vec3::new(-0.12, 0.98, s * 0.11), Vec3::new(0.22, 0.43, s * 0.14), 0.068, 6, "#2F3A4C");
            }
        }
        _ => unreachable!(),
    }
    (body.build(), trim.build())
}

fn build_scooter_extra_mesh(ninja: bool, load: BulkyLoadKind, jacket_hex: &str) -> Option<Mesh> {
    let mut b = StreetMeshBuilder::default();
    if ninja {
        // Sunny "ninja Lead" pastel floral sun-suit + wrap skirt + dark sunglasses (`street-riders.js:30-40`)
        b.push_box_rot(Vec3::new(-0.14, 1.20, 0.0), Vec3::new(0.35, 0.56, 0.46), Quat::from_rotation_z(-0.10), jacket_hex);
        b.push_sphere(Vec3::new(-0.10, 1.58, 0.0), Vec3::splat(0.145), jacket_hex);
        b.push_box(Vec3::new(0.04, 1.04, 0.0), Vec3::new(0.42, 0.08, 0.45), jacket_hex);
        b.push_box(Vec3::new(0.25, 0.72, 0.0), Vec3::new(0.08, 0.62, 0.45), jacket_hex);
        b.push_box(Vec3::new(0.03, 1.60, 0.0), Vec3::new(0.04, 0.055, 0.22), "#15171A");
    }
    match load {
        BulkyLoadKind::None => {}
        BulkyLoadKind::GrabFoodBox => {
            // Grab green jacket & insulated delivery box on rear rack
            b.push_box_rot(Vec3::new(-0.14, 1.20, 0.0), Vec3::new(0.33, 0.50, 0.42), Quat::from_rotation_z(-0.10), "#00B14F");
            b.push_sphere(Vec3::new(-0.11, 1.64, 0.0), Vec3::new(0.15, 0.11, 0.15), "#00B14F");
            b.push_box(Vec3::new(-0.68, 1.18, 0.0), Vec3::new(0.48, 0.48, 0.46), "#00B14F");
            b.push_box(Vec3::new(-0.68, 1.43, 0.0), Vec3::new(0.50, 0.05, 0.48), "#FFFFFF");
        }
        BulkyLoadKind::Fridge => {
            b.push_box(Vec3::new(-0.78, 1.52, 0.0), Vec3::new(0.60, 1.25, 0.62), "#E9EDF0");
            b.push_box(Vec3::new(-0.78, 1.25, 0.0), Vec3::new(0.62, 0.04, 0.64), "#2B2F36");
            b.push_box(Vec3::new(-0.78, 1.78, 0.0), Vec3::new(0.62, 0.04, 0.64), "#2B2F36");
        }
        BulkyLoadKind::Ladder => {
            for x in [-0.82_f32, -0.50] {
                b.push_box(Vec3::new(x, 1.0, 0.0), Vec3::new(0.05, 0.05, 2.4), "#C9CED6");
            }
            for k in -3..=3 {
                b.push_box(Vec3::new(-0.66, 1.0, k as f32 * 0.32), Vec3::new(0.32, 0.03, 0.04), "#AEB4BC");
            }
        }
        BulkyLoadKind::GasCylinder => {
            b.push_cyl(Vec3::new(0.13, 0.62, 0.0), 0.15, 0.15, 0.50, 10, "#C8102E");
            b.push_cyl(Vec3::new(0.13, 0.91, 0.0), 0.09, 0.15, 0.08, 10, "#C8102E");
            b.push_cyl(Vec3::new(0.13, 0.99, 0.0), 0.05, 0.05, 0.08, 8, "#5E6670");
        }
        BulkyLoadKind::PottedTree => {
            b.push_cyl(Vec3::new(-0.62, 1.06, 0.0), 0.24, 0.17, 0.32, 10, "#B5643C");
            b.push_cyl(Vec3::new(-0.62, 1.55, 0.0), 0.03, 0.045, 0.70, 6, "#6B4A33");
            b.push_sphere(Vec3::new(-0.62, 2.00, 0.0), Vec3::splat(0.42), "#4E8B3A");
            b.push_sphere(Vec3::new(-0.50, 2.28, 0.10), Vec3::splat(0.28), "#5E9B48");
        }
        BulkyLoadKind::ChickenCage => {
            b.push_box(Vec3::new(-0.78, 0.92, 0.0), Vec3::new(0.60, 0.04, 1.10), "#C9A86A");
            b.push_box(Vec3::new(-0.78, 1.36, 0.0), Vec3::new(0.60, 0.04, 1.10), "#C9A86A");
            for s in [-0.52_f32, 0.0, 0.52] {
                b.push_box(Vec3::new(-1.05, 1.14, s), Vec3::new(0.03, 0.44, 0.03), "#B08D52");
                b.push_box(Vec3::new(-0.51, 1.14, s), Vec3::new(0.03, 0.44, 0.03), "#B08D52");
            }
            b.push_sphere(Vec3::new(-0.80, 1.08, -0.22), Vec3::new(0.14, 0.11, 0.11), "#F2F2F2");
            b.push_sphere(Vec3::new(-0.74, 1.08, 0.22), Vec3::new(0.14, 0.11, 0.11), "#B5703C");
        }
        BulkyLoadKind::StyrofoamStack => {
            for k in 0..3 {
                b.push_box(Vec3::new(-0.74, 1.12 + k as f32 * 0.40, 0.0), Vec3::new(0.56, 0.36, 0.46), "#F5F7F8");
                b.push_box(Vec3::new(-0.74, 1.12 + k as f32 * 0.40, 0.0), Vec3::new(0.58, 0.03, 0.48), "#2B2F36");
            }
        }
    }
    if b.is_empty() {
        None
    } else {
        Some(b.build())
    }
}

/// Decodes ALL 11 Kenney Car Kit models + wheel from `cars.json` and builds procedural
/// VinFast VF3, Xanh SM, City Bus, and Scooter meshes, registering every mesh in `BackdropAssets`.
pub fn build_all_vehicle_meshes(
    meshes: &mut Assets<Mesh>,
    ba: &mut BackdropAssets,
) -> Vec<(VehicleKind, VehicleMeshPair)> {
    let catalog: CarsCatalog =
        serde_json::from_str(CARS_JSON).expect("embedded cars.json must parse");
    let wheel = decode_packed(&catalog.wheel);
    let mut out = Vec::with_capacity(VehicleKind::ALL.len());

    for kind in VehicleKind::ALL {
        let (paint_mesh, rest_mesh) = if let Some(key) = kind.kit_key() {
            let spec = catalog
                .models
                .get(key)
                .unwrap_or_else(|| panic!("cars.json missing model {key}"));
            let paint_sub = decode_packed(&spec.paint);
            let mut rest_sub = decode_packed(&spec.rest);
            for w in &spec.wheels {
                append_wheel_to_rest(&mut rest_sub, &wheel, w, catalog.scale);
            }
            // Add VinFast V-shaped nose light bar on VF8 (`street-life.js:106-109`)
            if kind == VehicleKind::Vf8 {
                let mut vf = StreetMeshBuilder::default();
                let x = spec.bounds[3] + 0.01;
                let y = spec.bounds[4] * 0.56;
                vf.push_bar(Vec3::new(x, y - 0.06, 0.0), Vec3::new(x - 0.03, y, 0.68), 0.022, 4, "#F4FAFF");
                vf.push_bar(Vec3::new(x, y - 0.06, 0.0), Vec3::new(x - 0.03, y, -0.68), 0.022, 4, "#F4FAFF");
                let vf_base = rest_sub.positions.len() as u32;
                rest_sub.positions.extend(vf.positions);
                rest_sub.normals.extend(vf.normals);
                rest_sub.colors.extend(vf.colors);
                rest_sub.indices.extend(vf.indices.into_iter().map(|i| vf_base + i));
            }
            (decoded_to_mesh(paint_sub), decoded_to_mesh(rest_sub))
        } else {
            build_procedural_vehicle(kind)
        };
        let pair = VehicleMeshPair {
            paint: ba.add_mesh(meshes, paint_mesh),
            rest: ba.add_mesh(meshes, rest_mesh),
        };
        out.push((kind, pair));
    }
    out
}

fn build_pedestrian_mesh(shirt: &str, pants: &str, hair: &str, conical_hat: bool) -> Mesh {
    let mut b = StreetMeshBuilder::default();
    b.push_box(Vec3::new(0.0, 0.36, -0.09), Vec3::new(0.14, 0.72, 0.14), pants);
    b.push_box(Vec3::new(0.0, 0.36, 0.09), Vec3::new(0.14, 0.72, 0.14), pants);
    b.push_box(Vec3::new(0.0, 0.98, 0.0), Vec3::new(0.24, 0.54, 0.36), shirt);
    b.push_sphere(Vec3::new(0.0, 1.40, 0.0), Vec3::splat(0.13), "#E8C2A0");
    if conical_hat {
        // Vietnamese nón lá
        b.push_cyl(Vec3::new(0.0, 1.56, 0.0), 0.02, 0.24, 0.18, 12, "#E3C88B");
    } else {
        b.push_sphere(Vec3::new(-0.02, 1.45, 0.0), Vec3::new(0.135, 0.10, 0.135), hair);
    }
    b.build()
}

/// Spawns live city traffic (all 13+ vehicle kinds, 115/114 sirens, Grab/XanhSM scooters,
/// sunny "ninja Lead" riders, 6 bulky loads, bus stop, ≥33°C shade-stopping, and zebra pedestrians).
pub fn spawn_street_traffic(
    commands: &mut Commands,
    meshes: &mut Assets<Mesh>,
    materials: &mut Assets<StandardMaterial>,
    ba: &mut BackdropAssets,
    grid: &CityGridData,
    signs_mat: Handle<StandardMaterial>,
    temp_c: f32,
) {
    let mut rng = CityRng::new(2026_7711);
    let catalog = build_all_vehicle_meshes(meshes, ba);
    let mesh_map: HashMap<VehicleKind, &VehicleMeshPair> =
        catalog.iter().map(|(k, p)| (*k, p)).collect();
    let trim_mat = ba.mat(materials, "#FFFFFF", 0.45, 0.25);

    // Pick streets closest to the office superblock
    let front_z = grid
        .zs
        .iter()
        .copied()
        .find(|&z| (z - (grid.sb.z1 + STREET_W * 0.5)).abs() < 1.0)
        .unwrap_or(16.2);
    let right_x = grid
        .xs
        .iter()
        .copied()
        .find(|&x| (x - (grid.sb.x1 + STREET_W * 0.5)).abs() < 1.0)
        .unwrap_or(18.0);
    let back_z = grid
        .zs
        .iter()
        .copied()
        .find(|&z| (z - (grid.sb.z0 - STREET_W * 0.5)).abs() < 1.0)
        .unwrap_or(-13.8);
    let left_x = grid
        .xs
        .iter()
        .copied()
        .find(|&x| (x - (grid.sb.x0 - STREET_W * 0.5)).abs() < 1.0)
        .unwrap_or(-12.0);

    let shade_offset = if temp_c >= 33.0 { 3.2 } else { 0.0 };

    // Bus stop shelter along the front avenue
    let bus_stop_x = grid.office_mid.x + 2.5;
    let mut stop_b = StreetMeshBuilder::default();
    stop_b.push_box(Vec3::new(bus_stop_x, PAVE_Y + 2.35, front_z - STREET_W * 0.5 - 0.85), Vec3::new(3.2, 0.08, 1.1), "#1F4E8C");
    for dx in [-1.45_f32, 1.45] {
        stop_b.push_box(Vec3::new(bus_stop_x + dx, PAVE_Y + 1.15, front_z - STREET_W * 0.5 - 1.25), Vec3::new(0.08, 2.3, 0.08), "#5E6670");
    }
    stop_b.push_box(Vec3::new(bus_stop_x, PAVE_Y + 0.44, front_z - STREET_W * 0.5 - 1.05), Vec3::new(2.2, 0.06, 0.36), "#C9CED6");
    stop_b.push_cyl(Vec3::new(bus_stop_x + 1.9, PAVE_Y + 1.15, front_z - STREET_W * 0.5 - 0.55), 0.03, 0.03, 2.3, 8, "#2B2F36");
    stop_b.push_box(Vec3::new(bus_stop_x + 1.9, PAVE_Y + 2.15, front_z - STREET_W * 0.5 - 0.55), Vec3::new(0.45, 0.55, 0.06), "#1E6FD1");
    let stop_mat = ba.mat(materials, "#FFFFFF", 0.55, 0.2);
    stop_b.spawn(commands, meshes, ba, stop_mat);

    // Siren beacon shared mesh + red/blue emissive materials
    let beacon_mesh = ba.add_mesh(meshes, Mesh::from(Sphere::new(0.11)));
    let siren_red = ba.mat_emissive(materials, "#FF261B", 3.2);
    let siren_blue = ba.mat_emissive(materials, "#2484FF", 3.2);

    // Curated roster ensuring ALL 13+ vehicle kinds + ninja Lead + bulky loads appear around the office!
    let specs: [(VehicleKind, bool, f32, f32, f32, f32, bool, BulkyLoadKind); 22] = [
        (VehicleKind::Vf3, true, 1.0, front_z + 1.05, -24.0, 5.4, false, BulkyLoadKind::None),
        (VehicleKind::XanhSm, true, 1.0, front_z + 1.05, -8.0, 6.2, false, BulkyLoadKind::None),
        (VehicleKind::Vf8, true, -1.0, front_z - 1.05, 6.0, 6.0, false, BulkyLoadKind::None),
        (VehicleKind::Bus, true, -1.0, front_z - 1.05, 22.0, 4.5, false, BulkyLoadKind::None),
        (VehicleKind::Vios, false, 1.0, right_x - 1.05, -16.0, 5.8, false, BulkyLoadKind::None),
        (VehicleKind::City, false, 1.0, right_x - 1.05, 4.0, 6.4, false, BulkyLoadKind::None),
        (VehicleKind::Morning, false, -1.0, right_x + 1.05, 18.0, 5.6, false, BulkyLoadKind::None),
        (VehicleKind::Fortuner, false, -1.0, right_x + 1.05, -2.0, 5.9, false, BulkyLoadKind::None),
        (VehicleKind::Ranger, true, 1.0, back_z + 1.05, -18.0, 5.5, false, BulkyLoadKind::None),
        (VehicleKind::Xpander, true, -1.0, back_z - 1.05, 12.0, 5.3, false, BulkyLoadKind::None),
        (VehicleKind::Delivery, false, 1.0, left_x - 1.05, -14.0, 4.9, false, BulkyLoadKind::None),
        (VehicleKind::Taxi, false, -1.0, left_x + 1.05, 10.0, 6.2, false, BulkyLoadKind::None),
        (VehicleKind::Ambulance, true, 1.0, front_z + 0.15, -30.0, 7.8, false, BulkyLoadKind::None),
        (VehicleKind::Firetruck, false, -1.0, right_x + 0.15, 28.0, 7.2, false, BulkyLoadKind::None),
        // Vietnamese scooter fleet: GrabFood, ninja Lead sun-suits, and all 6 bulky loads!
        (VehicleKind::Scooter, true, 1.0, front_z + 1.55, -14.0, 5.6, false, BulkyLoadKind::GrabFoodBox),
        (VehicleKind::Scooter, true, -1.0, front_z - 1.55, 14.0, 5.2, true, BulkyLoadKind::GasCylinder),
        (VehicleKind::Scooter, false, 1.0, right_x - 1.55, -10.0, 5.4, true, BulkyLoadKind::Fridge),
        (VehicleKind::Scooter, false, -1.0, right_x + 1.55, 8.0, 5.0, false, BulkyLoadKind::Ladder),
        (VehicleKind::Scooter, true, 1.0, front_z + 1.55, 4.0, 5.1, false, BulkyLoadKind::PottedTree),
        (VehicleKind::Scooter, false, 1.0, right_x - 1.55, 12.0, 4.9, false, BulkyLoadKind::ChickenCage),
        (VehicleKind::Scooter, true, -1.0, front_z - 1.55, -4.0, 5.3, false, BulkyLoadKind::StyrofoamStack),
        (VehicleKind::Scooter, false, -1.0, right_x + 1.55, -14.0, 5.7, true, BulkyLoadKind::GrabFoodBox),
    ];

    let pastel_suits = ["#F6C9D4", "#CFE3F5", "#F4E3B5", "#D7EFD2", "#E6D5F2"];

    for (idx, (kind, axis_x, dir, lane_coord, init_pos, speed, ninja, load)) in specs.into_iter().enumerate() {
        let pair = mesh_map[&kind];
        let paints = kind.paints();
        let paint_hex = paints[idx % paints.len()];
        let paint_mat = ba.mat(materials, paint_hex, 0.32, 0.25);

        let is_siren = matches!(kind, VehicleKind::Ambulance | VehicleKind::Firetruck);
        let is_scooter = kind == VehicleKind::Scooter;
        let is_bus = kind == VehicleKind::Bus;
        let yaw = if axis_x {
            if dir > 0.0 { 0.0 } else { PI }
        } else if dir > 0.0 {
            -FRAC_PI_2
        } else {
            FRAC_PI_2
        };
        let world_pos = if axis_x {
            Vec3::new(init_pos, GY, lane_coord)
        } else {
            Vec3::new(lane_coord, GY, init_pos)
        };
        let stop_cross = if axis_x { right_x } else { front_z };

        let root = commands
            .spawn((
                WorldEntity,
                BackdropEntity,
                CityVehicle {
                    axis_x,
                    dir,
                    lane_coord,
                    pos: init_pos,
                    min_pos: -38.0,
                    max_pos: 38.0,
                    speed,
                    stop_cross,
                    sig_i: idx % 4,
                    sig_j: (idx / 2) % 4,
                    is_scooter,
                    is_siren,
                    is_bus,
                    bus_stop_pos: bus_stop_x,
                    shade_offset: if is_scooter { shade_offset } else { 0.0 },
                    wait_timer: 0.0,
                },
                Transform::from_translation(world_pos).with_rotation(Quat::from_rotation_y(yaw)),
                Visibility::Inherited,
            ))
            .id();

        let paint_child = commands
            .spawn((
                WorldEntity,
                BackdropEntity,
                Mesh3d(pair.paint.clone()),
                MeshMaterial3d(paint_mat),
                Transform::IDENTITY,
            ))
            .id();
        let rest_child = commands
            .spawn((
                WorldEntity,
                BackdropEntity,
                Mesh3d(pair.rest.clone()),
                MeshMaterial3d(trim_mat.clone()),
                Transform::IDENTITY,
            ))
            .id();
        commands.entity(root).add_children(&[paint_child, rest_child]);

        if is_scooter {
            let suit_col = pastel_suits[idx % pastel_suits.len()];
            if let Some(extra_mesh) = build_scooter_extra_mesh(ninja, load, suit_col) {
                let h = ba.add_mesh(meshes, extra_mesh);
                let extra_child = commands
                    .spawn((
                        WorldEntity,
                        BackdropEntity,
                        Mesh3d(h),
                        MeshMaterial3d(trim_mat.clone()),
                        Transform::IDENTITY,
                    ))
                    .id();
                commands.entity(root).add_child(extra_child);
            }
        }

        if is_siren {
            let (ly, lz, rect, sign_y) = if kind == VehicleKind::Ambulance {
                (1.74, 0.36, [256.0, 256.0, 256.0, 128.0], 1.38)
            } else {
                (1.98, 0.56, [0.0, 384.0, 256.0, 128.0], 1.24)
            };
            let red_e = commands
                .spawn((
                    WorldEntity,
                    BackdropEntity,
                    SirenLightPair { is_red: true },
                    Mesh3d(beacon_mesh.clone()),
                    MeshMaterial3d(siren_red.clone()),
                    Transform::from_xyz(0.95, ly, lz),
                ))
                .id();
            let blue_e = commands
                .spawn((
                    WorldEntity,
                    BackdropEntity,
                    SirenLightPair { is_red: false },
                    Mesh3d(beacon_mesh.clone()),
                    MeshMaterial3d(siren_blue.clone()),
                    Transform::from_xyz(0.95, ly, -lz),
                ))
                .id();
            // Side 115 / 114 lettering placards (UV-cropped from street_signs_atlas!)
            let side_p = spawn_backdrop_uv_quad(
                commands,
                meshes,
                ba,
                signs_mat.clone(),
                512.0,
                512.0,
                rect,
                Vec3::new(-0.6, sign_y, 0.94),
                Vec2::new(1.22, 0.48),
                Quat::IDENTITY,
            );
            let side_n = spawn_backdrop_uv_quad(
                commands,
                meshes,
                ba,
                signs_mat.clone(),
                512.0,
                512.0,
                rect,
                Vec3::new(-0.6, sign_y, -0.94),
                Vec2::new(1.22, 0.48),
                Quat::from_rotation_y(PI),
            );
            commands
                .entity(root)
                .add_children(&[red_e, blue_e, side_p, side_n]);
        }
    }

    // Sidewalk & zebra-crossing pedestrians (`street-folk.js`, some with nón lá conical hats)
    let ped_looks = [
        ("#E8742A", "#2F3A4C", "#1E1A18", true),
        ("#3F7CC4", "#363D4C", "#2B2420", false),
        ("#F2F2F2", "#1F2430", "#3A332E", true),
        ("#2A9D8F", "#2F3A4C", "#211C18", false),
        ("#D64550", "#4A4A52", "#241E1A", false),
        ("#F28FA8", "#363D4C", "#3B2618", true),
    ];
    let ped_meshes: Vec<Handle<Mesh>> = ped_looks
        .iter()
        .map(|&(s, p, h, hat)| ba.add_mesh(meshes, build_pedestrian_mesh(s, p, h, hat)))
        .collect();
    let ped_mat = ba.mat(materials, "#FFFFFF", 0.78, 0.0);

    let ped_tracks = [
        (true, 1.0_f32, front_z + STREET_W * 0.5 + 0.85, -18.0_f32, 22.0_f32),
        (true, -1.0, front_z - STREET_W * 0.5 - 0.85, -14.0, 20.0),
        (false, 1.0, right_x + STREET_W * 0.5 + 0.85, -14.0, 22.0),
        (false, -1.0, right_x - STREET_W * 0.5 - 0.85, -14.0, 20.0),
        // Zebra crossing pedestrians across front & right streets
        (false, 1.0, right_x - STREET_W * 0.5 - 1.3, front_z - 3.6, front_z + 3.6),
        (true, 1.0, front_z - STREET_W * 0.5 - 1.3, right_x - 3.6, right_x + 3.6),
    ];
    for (idx, &(axis_x, dir, side_coord, min_p, max_p)) in ped_tracks.iter().enumerate() {
        for k in 0..2 {
            let init = min_p + (max_p - min_p) * ((k as f32 + 0.25 + rng.next_f32() * 0.25).fract());
            let world_p = if axis_x {
                Vec3::new(init, PAVE_Y, side_coord)
            } else {
                Vec3::new(side_coord, PAVE_Y, init)
            };
            let yaw = if axis_x {
                if dir > 0.0 { 0.0 } else { PI }
            } else if dir > 0.0 {
                -FRAC_PI_2
            } else {
                FRAC_PI_2
            };
            commands.spawn((
                WorldEntity,
                BackdropEntity,
                StreetPedestrian {
                    axis_x,
                    dir,
                    side_coord,
                    pos: init,
                    min_pos: min_p,
                    max_pos: max_p,
                    speed: 1.15 + (k as f32) * 0.22,
                    phase: (idx * 2 + k) as f32 * 0.9,
                },
                Mesh3d(ped_meshes[(idx + k) % ped_meshes.len()].clone()),
                MeshMaterial3d(ped_mat.clone()),
                Transform::from_translation(world_p).with_rotation(Quat::from_rotation_y(yaw)),
            ));
        }
    }
}

/// Zero-allocation Update system advancing all street vehicles, stopping at red lights/shade/bus
/// stops, flashing 115/114 siren lights, and walking sidewalk/zebra pedestrians.
pub fn animate_street_traffic_system(
    time: Res<Time>,
    mut vehicles: Query<(&mut CityVehicle, &mut Transform), Without<StreetPedestrian>>,
    mut sirens: Query<(&SirenLightPair, &mut Visibility)>,
    mut peds: Query<(&mut StreetPedestrian, &mut Transform), Without<CityVehicle>>,
) {
    let dt = time.delta_secs().min(0.05);
    let t = time.elapsed_secs();

    for (mut v, mut tf) in &mut vehicles {
        let mut factor = 1.0_f32;
        if !v.is_siren {
            let stop_line = v.stop_cross - v.dir * (4.8 + v.shade_offset);
            let dist_to_stop = (stop_line - v.pos) * v.dir;
            let sig = signal_color_of(signal_phase(v.sig_i, v.sig_j, t, v.axis_x));
            if sig == SignalColor::Red && (0.0..2.2).contains(&dist_to_stop) {
                factor = 0.0;
            }
        }
        if v.is_bus {
            let dist_bus = (v.bus_stop_pos - v.pos).abs();
            if dist_bus < 0.35 && v.wait_timer < 4.0 {
                v.wait_timer += dt;
                factor = 0.0;
            } else if dist_bus > 2.0 {
                v.wait_timer = 0.0;
            }
        }
        v.pos += v.dir * v.speed * factor * dt;
        if v.pos > v.max_pos {
            v.pos = v.min_pos;
        } else if v.pos < v.min_pos {
            v.pos = v.max_pos;
        }
        if v.axis_x {
            tf.translation.x = v.pos;
            tf.translation.z = v.lane_coord;
        } else {
            tf.translation.x = v.lane_coord;
            tf.translation.z = v.pos;
        }
    }

    let flip = ((t * 3.0) as u32) % 2 == 0;
    for (siren, mut vis) in &mut sirens {
        let on = if siren.is_red { flip } else { !flip };
        *vis = if on {
            Visibility::Inherited
        } else {
            Visibility::Hidden
        };
    }

    for (mut p, mut tf) in &mut peds {
        p.pos += p.dir * p.speed * dt;
        if p.pos > p.max_pos {
            p.pos = p.max_pos;
            p.dir = -1.0;
        } else if p.pos < p.min_pos {
            p.pos = p.min_pos;
            p.dir = 1.0;
        }
        let bob = (t * 7.5 + p.phase).sin().abs() * 0.045;
        let yaw = if p.axis_x {
            if p.dir > 0.0 { 0.0 } else { PI }
        } else if p.dir > 0.0 {
            -FRAC_PI_2
        } else {
            FRAC_PI_2
        };
        if p.axis_x {
            tf.translation = Vec3::new(p.pos, PAVE_Y + bob, p.side_coord);
        } else {
            tf.translation = Vec3::new(p.side_coord, PAVE_Y + bob, p.pos);
        }
        tf.rotation = Quat::from_rotation_y(yaw);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::backdrops::BackdropEntity;
    use crate::layout::{LayoutSpec, OfficeLayout};
    use crate::street::city_grid::spawn_city_grid;

    #[test]
    fn traffic_builds_meshes_for_all_13_vehicle_specs_plus_bus_and_scooter_and_purges_cleanly() {
        let mut meshes = Assets::<Mesh>::default();
        let mut materials = Assets::<StandardMaterial>::default();
        let mut images = Assets::<Image>::default();
        let mut ba = BackdropAssets::default();

        let built = build_all_vehicle_meshes(&mut meshes, &mut ba);
        assert_eq!(built.len(), VehicleKind::ALL.len());
        for kind in VehicleKind::CAR_SPECS_13 {
            let pair = built
                .iter()
                .find(|(k, _)| *k == kind)
                .map(|(_, p)| p)
                .unwrap_or_else(|| panic!("missing vehicle kind {kind:?}"));
            let paint_m = meshes.get(&pair.paint).expect("paint mesh present");
            let rest_m = meshes.get(&pair.rest).expect("rest mesh present");
            assert!(paint_m.count_vertices() > 12, "{kind:?} paint verts");
            assert!(rest_m.count_vertices() > 12, "{kind:?} rest verts");
        }

        ba.purge(&mut meshes, &mut materials, &mut images);
        assert_eq!(meshes.len(), 0, "all vehicle meshes must purge cleanly");
    }

    #[test]
    fn traffic_spawns_vehicles_sirens_pedestrians_and_purges_zero_leaks() {
        let spec = LayoutSpec::for_layout(OfficeLayout::Pro);
        let mut world = World::new();
        world.insert_resource(Assets::<Mesh>::default());
        world.insert_resource(Assets::<StandardMaterial>::default());
        world.insert_resource(Assets::<Image>::default());
        world.insert_resource(BackdropAssets::default());

        world.resource_scope(|world, mut meshes: Mut<Assets<Mesh>>| {
            world.resource_scope(|world, mut mats: Mut<Assets<StandardMaterial>>| {
                world.resource_scope(|world, mut imgs: Mut<Assets<Image>>| {
                    world.resource_scope(|world, mut ba: Mut<BackdropAssets>| {
                        let mut cmds = world.commands();
                        let grid = spawn_city_grid(&mut cmds, &mut meshes, &mut mats, &mut imgs, &mut ba, &spec, false);
                        let signs_mat = ba.mat(&mut mats, "#FFFFFF", 0.5, 0.0);
                        spawn_street_traffic(&mut cmds, &mut meshes, &mut mats, &mut ba, &grid, signs_mat, 35.0);
                    });
                });
            });
        });
        world.flush();

        assert!(world.query::<&CityVehicle>().iter(&world).count() >= 20);
        assert!(world.query::<&SirenLightPair>().iter(&world).count() >= 4);
        assert!(world.query::<&StreetPedestrian>().iter(&world).count() >= 10);

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
