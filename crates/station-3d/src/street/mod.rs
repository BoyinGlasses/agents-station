pub mod city_grid;
pub mod country_beach;
pub mod eat_com;
pub mod scenes_a;
pub mod scenes_b;
pub mod shops_walkin;
pub mod traffic;

use crate::backdrops::{BackdropAssets, BackdropEntity};
use crate::layout::LayoutSpec;
use crate::lights::LightingState;
use crate::palette::{WorldEntity, parse_hex_rgb, srgb_hex};
use bevy::asset::RenderAssetUsages;
use bevy::prelude::*;
use bevy::render::mesh::{Indices, PrimitiveTopology};
use station_canvas2d::street_surfaces::draw_street_signs_atlas;
use station_canvas2d::{BrandRegistry, FontEngine};
use std::f32::consts::{FRAC_PI_2, TAU};

/// Ground asphalt elevation matching Golden Oracle `terrace.js` (`GY = -0.62`).
pub const GY: f32 = -0.62;
/// Sidewalk top elevation (`GY + 0.12`).
pub const PAVE_Y: f32 = GY + 0.12;
/// Park lawn top elevation (`GY + 0.18`).
pub const LAWN_Y: f32 = GY + 0.18;

/// Deterministic PRNG matching Oracle `terrace.js` (`cr()` Park-Miller LCG).
#[derive(Debug, Clone)]
pub struct CityRng {
    state: u32,
}

impl CityRng {
    pub fn new(seed: u32) -> Self {
        Self {
            state: seed.max(1) % 2_147_483_647,
        }
    }

    pub fn next_f32(&mut self) -> f32 {
        self.state = ((self.state as u64 * 16_807) % 2_147_483_647) as u32;
        self.state as f32 / 2_147_483_647.0
    }

    pub fn range(&mut self, lo: f32, hi: f32) -> f32 {
        lo + self.next_f32() * (hi - lo)
    }

    pub fn range_i32(&mut self, lo: i32, hi: i32) -> i32 {
        if hi <= lo {
            return lo;
        }
        lo + (self.next_f32() * ((hi - lo + 1) as f32)).floor() as i32
    }

    pub fn pick<'a, T>(&mut self, slice: &'a [T]) -> &'a T {
        let idx = ((self.next_f32() * slice.len() as f32).floor() as usize).min(slice.len() - 1);
        &slice[idx]
    }
}

/// Converts hex `#RRGGBB` into linear RGBA float array for Bevy `ATTRIBUTE_COLOR`.
pub fn linear_rgba(hex: &str) -> [f32; 4] {
    let (r, g, b) = parse_hex_rgb(hex);
    let lin = Color::srgb_u8(r, g, b).to_linear();
    [lin.red, lin.green, lin.blue, 1.0]
}

/// Compact static vertex-colored mesh accumulator mirroring Oracle `mergeAll` + `vmat`.
/// Keeps thousands of boxes/cylinders/cones/spheres in a single low-RAM GPU buffer registered
/// inside `BackdropAssets` so `SwitchBackdrop` purges 100% cleanly.
#[derive(Default)]
pub struct StreetMeshBuilder {
    positions: Vec<[f32; 3]>,
    normals: Vec<[f32; 3]>,
    uvs: Vec<[f32; 2]>,
    uvs_1: Vec<[f32; 2]>,
    colors: Vec<[f32; 4]>,
    indices: Vec<u32>,
}

impl StreetMeshBuilder {
    pub fn is_empty(&self) -> bool {
        self.indices.is_empty()
    }

    pub fn push_quad(
        &mut self,
        v0: Vec3,
        v1: Vec3,
        v2: Vec3,
        v3: Vec3,
        normal: Vec3,
        uvs: [[f32; 2]; 4],
        color: [f32; 4],
    ) {
        let base = self.positions.len() as u32;
        let n = normal.normalize_or_zero().to_array();
        for (p, uv) in [(v0, uvs[0]), (v1, uvs[1]), (v2, uvs[2]), (v3, uvs[3])] {
            self.positions.push(p.to_array());
            self.normals.push(n);
            self.uvs.push(uv);
            self.colors.push(color);
        }
        self.indices
            .extend_from_slice(&[base, base + 1, base + 2, base, base + 2, base + 3]);
    }

    #[allow(clippy::too_many_arguments)]
    pub fn push_quad_uv1(
        &mut self,
        v0: Vec3,
        v1: Vec3,
        v2: Vec3,
        v3: Vec3,
        normal: Vec3,
        uvs: [[f32; 2]; 4],
        uvs_1: [[f32; 2]; 4],
        color: [f32; 4],
    ) {
        self.push_quad(v0, v1, v2, v3, normal, uvs, color);
        self.uvs_1.extend_from_slice(&uvs_1);
    }

    /// Appends an axis-aligned or rotated box centered at `center` with extents `size`.
    pub fn push_box_rot(&mut self, center: Vec3, size: Vec3, rot: Quat, hex: &str) {
        let col = linear_rgba(hex);
        let hx = size.x * 0.5;
        let hy = size.y * 0.5;
        let hz = size.z * 0.5;
        let xform = |p: Vec3| center + rot * p;
        let nrm = |n: Vec3| rot * n;
        let uvs = [[0.0, 0.0], [1.0, 0.0], [1.0, 1.0], [0.0, 1.0]];

        // +Z front
        self.push_quad(
            xform(Vec3::new(-hx, -hy, hz)),
            xform(Vec3::new(hx, -hy, hz)),
            xform(Vec3::new(hx, hy, hz)),
            xform(Vec3::new(-hx, hy, hz)),
            nrm(Vec3::Z),
            uvs,
            col,
        );
        // -Z back
        self.push_quad(
            xform(Vec3::new(hx, -hy, -hz)),
            xform(Vec3::new(-hx, -hy, -hz)),
            xform(Vec3::new(-hx, hy, -hz)),
            xform(Vec3::new(hx, hy, -hz)),
            nrm(Vec3::NEG_Z),
            uvs,
            col,
        );
        // +X right
        self.push_quad(
            xform(Vec3::new(hx, -hy, hz)),
            xform(Vec3::new(hx, -hy, -hz)),
            xform(Vec3::new(hx, hy, -hz)),
            xform(Vec3::new(hx, hy, hz)),
            nrm(Vec3::X),
            uvs,
            col,
        );
        // -X left
        self.push_quad(
            xform(Vec3::new(-hx, -hy, -hz)),
            xform(Vec3::new(-hx, -hy, hz)),
            xform(Vec3::new(-hx, hy, hz)),
            xform(Vec3::new(-hx, hy, -hz)),
            nrm(Vec3::NEG_X),
            uvs,
            col,
        );
        // +Y top
        self.push_quad(
            xform(Vec3::new(-hx, hy, hz)),
            xform(Vec3::new(hx, hy, hz)),
            xform(Vec3::new(hx, hy, -hz)),
            xform(Vec3::new(-hx, hy, -hz)),
            nrm(Vec3::Y),
            uvs,
            col,
        );
        // -Y bottom
        self.push_quad(
            xform(Vec3::new(-hx, -hy, -hz)),
            xform(Vec3::new(hx, -hy, -hz)),
            xform(Vec3::new(hx, -hy, hz)),
            xform(Vec3::new(-hx, -hy, hz)),
            nrm(Vec3::NEG_Y),
            uvs,
            col,
        );
    }

    pub fn push_box(&mut self, center: Vec3, size: Vec3, hex: &str) {
        self.push_box_rot(center, size, Quat::IDENTITY, hex);
    }

    /// Appends a building volume with per-bay & per-floor repeating UVs on vertical sides
    /// (`UV_0` for 1×1 day tile, `UV_1` scaled by `(1/16, 1/32)` for the 16×32 night emissive atlas,
    /// and descending V so upper notched shop walls align seamlessly with full-height floors).
    #[allow(clippy::too_many_arguments)]
    pub fn push_facade_box(
        &mut self,
        cx: f32,
        cz: f32,
        w: f32,
        d: f32,
        y0: f32,
        h: f32,
        tint_hex: &str,
        off_u: f32,
        off_v: f32,
        bay_w: f32,
        floor_h: f32,
    ) {
        let col = linear_rgba(tint_hex);
        let hx = w * 0.5;
        let hz = d * 0.5;
        let yb = GY + y0;
        let yt = yb + h;
        let bays_x = (w / bay_w).round().max(1.0);
        let bays_z = (d / bay_w).round().max(1.0);
        let v_bot = off_v - y0 / floor_h;
        let v_top = off_v - (y0 + h) / floor_h;
        let side_uv = |bays: f32| {
            [
                [off_u, v_bot],
                [off_u + bays, v_bot],
                [off_u + bays, v_top],
                [off_u, v_top],
            ]
        };
        let to_uv1 = |uvs: [[f32; 2]; 4]| {
            [
                [uvs[0][0] / 16.0, uvs[0][1] / 32.0],
                [uvs[1][0] / 16.0, uvs[1][1] / 32.0],
                [uvs[2][0] / 16.0, uvs[2][1] / 32.0],
                [uvs[3][0] / 16.0, uvs[3][1] / 32.0],
            ]
        };
        let cap_uv = [[0.02, 0.02]; 4];
        let cap_uv1 = [[0.02 / 16.0, 0.02 / 32.0]; 4];
        let uvx = side_uv(bays_x);
        let uvz = side_uv(bays_z);

        // +Z
        self.push_quad_uv1(
            Vec3::new(cx - hx, yb, cz + hz),
            Vec3::new(cx + hx, yb, cz + hz),
            Vec3::new(cx + hx, yt, cz + hz),
            Vec3::new(cx - hx, yt, cz + hz),
            Vec3::Z,
            uvx,
            to_uv1(uvx),
            col,
        );
        // -Z
        self.push_quad_uv1(
            Vec3::new(cx + hx, yb, cz - hz),
            Vec3::new(cx - hx, yb, cz - hz),
            Vec3::new(cx - hx, yt, cz - hz),
            Vec3::new(cx + hx, yt, cz - hz),
            Vec3::NEG_Z,
            uvx,
            to_uv1(uvx),
            col,
        );
        // +X
        self.push_quad_uv1(
            Vec3::new(cx + hx, yb, cz + hz),
            Vec3::new(cx + hx, yb, cz - hz),
            Vec3::new(cx + hx, yt, cz - hz),
            Vec3::new(cx + hx, yt, cz + hz),
            Vec3::X,
            uvz,
            to_uv1(uvz),
            col,
        );
        // -X
        self.push_quad_uv1(
            Vec3::new(cx - hx, yb, cz - hz),
            Vec3::new(cx - hx, yb, cz + hz),
            Vec3::new(cx - hx, yt, cz + hz),
            Vec3::new(cx - hx, yt, cz - hz),
            Vec3::NEG_X,
            uvz,
            to_uv1(uvz),
            col,
        );
        // +Y top
        self.push_quad_uv1(
            Vec3::new(cx - hx, yt, cz + hz),
            Vec3::new(cx + hx, yt, cz + hz),
            Vec3::new(cx + hx, yt, cz - hz),
            Vec3::new(cx - hx, yt, cz - hz),
            Vec3::Y,
            cap_uv,
            cap_uv1,
            col,
        );
    }

    /// Appends a tapered vertical or oriented cylinder (`r_top`, `r_bot`, `height`, `segs`).
    #[allow(clippy::too_many_arguments)]
    pub fn push_cyl_rot(
        &mut self,
        center: Vec3,
        r_top: f32,
        r_bot: f32,
        height: f32,
        segs: usize,
        rot: Quat,
        hex: &str,
    ) {
        let segs = segs.max(3);
        let col = linear_rgba(hex);
        let hh = height * 0.5;
        let xform = |p: Vec3| center + rot * p;
        let nrm = |n: Vec3| (rot * n).normalize_or_zero();

        for i in 0..segs {
            let a0 = (i as f32 / segs as f32) * TAU;
            let a1 = ((i + 1) as f32 / segs as f32) * TAU;
            let (s0, c0) = a0.sin_cos();
            let (s1, c1) = a1.sin_cos();
            let b0 = xform(Vec3::new(c0 * r_bot, -hh, s0 * r_bot));
            let b1 = xform(Vec3::new(c1 * r_bot, -hh, s1 * r_bot));
            let t1 = xform(Vec3::new(c1 * r_top, hh, s1 * r_top));
            let t0 = xform(Vec3::new(c0 * r_top, hh, s0 * r_top));
            let am = (a0 + a1) * 0.5;
            let side_n = nrm(Vec3::new(am.cos(), 0.0, am.sin()));
            // Counter-clockwise winding viewed from outside (+side_n outward!)
            self.push_quad(
                b1,
                b0,
                t0,
                t1,
                side_n,
                [[0.0, 0.0], [1.0, 0.0], [1.0, 1.0], [0.0, 1.0]],
                col,
            );
        }
        // Top cap
        if r_top > 1e-4 {
            let top_c = xform(Vec3::new(0.0, hh, 0.0));
            let tn = nrm(Vec3::Y).to_array();
            let c_idx = self.positions.len() as u32;
            self.positions.push(top_c.to_array());
            self.normals.push(tn);
            self.uvs.push([0.5, 0.5]);
            self.colors.push(col);
            for i in 0..segs {
                let a = (i as f32 / segs as f32) * TAU;
                let (s, c) = a.sin_cos();
                self.positions
                    .push(xform(Vec3::new(c * r_top, hh, s * r_top)).to_array());
                self.normals.push(tn);
                self.uvs.push([0.5 + 0.5 * c, 0.5 + 0.5 * s]);
                self.colors.push(col);
            }
            for i in 0..segs {
                let a = c_idx + 1 + i as u32;
                let b = c_idx + 1 + ((i + 1) % segs) as u32;
                self.indices.extend_from_slice(&[c_idx, b, a]);
            }
        }
    }

    pub fn push_cyl(
        &mut self,
        center: Vec3,
        r_top: f32,
        r_bot: f32,
        height: f32,
        segs: usize,
        hex: &str,
    ) {
        self.push_cyl_rot(center, r_top, r_bot, height, segs, Quat::IDENTITY, hex);
    }

    /// Appends a segment bar between `a` and `b` with radius `r` (for cables, limbs, rails).
    pub fn push_bar(&mut self, a: Vec3, b: Vec3, r: f32, segs: usize, hex: &str) {
        let diff = b - a;
        let len = diff.length();
        if len < 1e-4 {
            return;
        }
        let dir = diff / len;
        let rot = Quat::from_rotation_arc(Vec3::Y, dir);
        self.push_cyl_rot((a + b) * 0.5, r, r, len, segs, rot, hex);
    }

    /// Appends a low-poly faceted foliage/canopy sphere (octahedral/UV sphere).
    pub fn push_sphere(&mut self, center: Vec3, scale: Vec3, hex: &str) {
        let col = linear_rgba(hex);
        let rings = 5usize;
        let segs = 8usize;
        let base = self.positions.len() as u32;
        for r in 0..=rings {
            let phi = (r as f32 / rings as f32) * std::f32::consts::PI;
            let (sp, cp) = phi.sin_cos();
            for s in 0..=segs {
                let theta = (s as f32 / segs as f32) * TAU;
                let (st, ct) = theta.sin_cos();
                let n = Vec3::new(sp * ct, cp, sp * st);
                let p = center + n * scale;
                self.positions.push(p.to_array());
                self.normals.push(n.to_array());
                self.uvs.push([s as f32 / segs as f32, r as f32 / rings as f32]);
                self.colors.push(col);
            }
        }
        let stride = (segs + 1) as u32;
        for r in 0..rings as u32 {
            for s in 0..segs as u32 {
                let a = base + r * stride + s;
                let b = a + 1;
                let c = a + stride;
                let d = c + 1;
                self.indices.extend_from_slice(&[a, b, d, a, d, c]);
            }
        }
    }

    /// Appends a horizontal ground quad facing `+Y` at `center` with `(w, d)` in XZ.
    pub fn push_flat(&mut self, center: Vec3, w: f32, d: f32, hex: &str) {
        let hx = w * 0.5;
        let hz = d * 0.5;
        self.push_quad(
            Vec3::new(center.x - hx, center.y, center.z + hz),
            Vec3::new(center.x + hx, center.y, center.z + hz),
            Vec3::new(center.x + hx, center.y, center.z - hz),
            Vec3::new(center.x - hx, center.y, center.z - hz),
            Vec3::Y,
            [[0.0, 0.0], [1.0, 0.0], [1.0, 1.0], [0.0, 1.0]],
            linear_rgba(hex),
        );
    }

    pub fn build(self) -> Mesh {
        let mut mesh = Mesh::new(
            PrimitiveTopology::TriangleList,
            RenderAssetUsages::MAIN_WORLD | RenderAssetUsages::RENDER_WORLD,
        );
        mesh.insert_attribute(Mesh::ATTRIBUTE_POSITION, self.positions);
        mesh.insert_attribute(Mesh::ATTRIBUTE_NORMAL, self.normals);
        mesh.insert_attribute(Mesh::ATTRIBUTE_UV_0, self.uvs);
        if !self.uvs_1.is_empty() {
            mesh.insert_attribute(Mesh::ATTRIBUTE_UV_1, self.uvs_1);
        }
        mesh.insert_attribute(Mesh::ATTRIBUTE_COLOR, self.colors);
        mesh.insert_indices(Indices::U32(self.indices));
        mesh
    }

    /// Finalizes the merged mesh, registers it in `BackdropAssets`, and spawns a single
    /// `(WorldEntity, BackdropEntity)` entity. Returns `None` if empty.
    pub fn spawn(
        self,
        commands: &mut Commands,
        meshes: &mut Assets<Mesh>,
        ba: &mut BackdropAssets,
        mat: Handle<StandardMaterial>,
    ) -> Option<Entity> {
        if self.is_empty() {
            return None;
        }
        let handle = ba.add_mesh(meshes, self.build());
        Some(
            commands
                .spawn((
                    WorldEntity,
                    BackdropEntity,
                    Mesh3d(handle),
                    MeshMaterial3d(mat),
                    Transform::IDENTITY,
                ))
                .id(),
        )
    }
}

/// Spawns a single UV-cropped backdrop quad with `WorldEntity + BackdropEntity`.
#[allow(clippy::too_many_arguments)]
pub fn spawn_backdrop_uv_quad(
    commands: &mut Commands,
    meshes: &mut Assets<Mesh>,
    ba: &mut BackdropAssets,
    mat: Handle<StandardMaterial>,
    atlas_w: f32,
    atlas_h: f32,
    rect: [f32; 4],
    pos: Vec3,
    size: Vec2,
    rot: Quat,
) -> Entity {
    let quad = ba.atlas_quad(meshes, atlas_w, atlas_h, rect[0], rect[1], rect[2], rect[3]);
    commands
        .spawn((
            WorldEntity,
            BackdropEntity,
            Mesh3d(quad),
            MeshMaterial3d(mat),
            Transform::from_translation(pos)
                .with_rotation(rot)
                .with_scale(Vec3::new(size.x, size.y, 1.0)),
        ))
        .id()
}

/// Local coordinate frame along a park's sidewalk edge (matching `street-spots.js`).
#[derive(Debug, Clone, Copy)]
pub struct SpotFrame {
    pub origin: Vec3,
    pub along: Vec2, // unit XZ along the kerb
    pub out: Vec2,   // unit XZ pointing out toward the street
    pub yaw: f32,
}

impl SpotFrame {
    pub fn at(&self, lx: f32, lz: f32, ly: f32) -> Vec3 {
        Vec3::new(
            self.origin.x + self.along.x * lx + self.out.x * lz,
            self.origin.y + ly,
            self.origin.z + self.along.y * lx + self.out.y * lz,
        )
    }

    pub fn rot(&self) -> Quat {
        Quat::from_rotation_y(self.yaw)
    }

    /// Rotation facing outward along `self.out` (+Z local aligns with `self.out`).
    pub fn face_out_rot(&self) -> Quat {
        Quat::from_rotation_y(self.out.x.atan2(self.out.y))
    }

    pub fn horizontal_rot(&self) -> Quat {
        self.face_out_rot() * Quat::from_rotation_x(-FRAC_PI_2)
    }
}

/// Park-edge placement allocator mirroring `street-spots.js` so `eat_com`, `atm`, `ev`,
/// `photo`, `morning`, `corner`, `repair`, and `nhau` never overlap each other or trees!
pub struct SpotFinder {
    pub office_mid: Vec2,
    pub parks: Vec<[f32; 4]>,
    pub plazas: Vec<[f32; 4]>,
    pub trees: Vec<[f32; 3]>,
    pub taken: Vec<[f32; 3]>,
}

impl SpotFinder {
    pub fn new(
        office_mid: Vec2,
        parks: Vec<[f32; 4]>,
        plazas: Vec<[f32; 4]>,
        trees: Vec<[f32; 3]>,
    ) -> Self {
        Self {
            office_mid,
            parks,
            plazas,
            trees,
            taken: Vec::new(),
        }
    }

    pub fn reserve(&mut self, x: f32, z: f32, r: f32) {
        self.taken.push([x, z, r]);
    }

    pub fn find(&mut self, len: f32, depth: f32, d_in: f32, lawn: bool, near: f32) -> Option<SpotFrame> {
        let mut best: Option<(f32, SpotFrame, f32)> = None;
        let radius = len.max(depth) * 0.5 + 0.8;

        for &[x0, x1, z0, z1] in &self.parks {
            let cx = (x0 + x1) * 0.5;
            let cz = (z0 + z1) * 0.5;
            let dist_park = Vec2::new(
                self.office_mid.x - self.office_mid.x.clamp(x0, x1),
                self.office_mid.y - self.office_mid.y.clamp(z0, z1),
            )
            .length();
            if dist_park > near {
                continue;
            }
            let has_square = self
                .plazas
                .iter()
                .any(|q| q[0] > x0 && q[1] < x1 && q[2] > z0 && q[3] < z1);
            if has_square && d_in + depth * 0.5 > 2.5 {
                continue;
            }

            let edges = [
                (Vec2::new(0.0, 1.0), Vec2::new(cx, z1), (x1 - x0) * 0.5),
                (Vec2::new(0.0, -1.0), Vec2::new(cx, z0), (x1 - x0) * 0.5),
                (Vec2::new(1.0, 0.0), Vec2::new(x1, cz), (z1 - z0) * 0.5),
                (Vec2::new(-1.0, 0.0), Vec2::new(x0, cz), (z1 - z0) * 0.5),
            ];
            for (out, mid, half) in edges {
                let along = Vec2::new(out.y, -out.x);
                let lim = half - 1.8 - len * 0.5;
                if lim < 0.0 {
                    continue;
                }
                let mut a = -lim;
                while a <= lim + 1e-4 {
                    let px = mid.x + along.x * a - out.x * d_in;
                    let pz = mid.y + along.y * a - out.y * d_in;
                    let clash_tree = self
                        .trees
                        .iter()
                        .any(|t| (t[0] - px).hypot(t[1] - pz) < 0.9 + 0.4 * t[2]);
                    let clash_taken = self
                        .taken
                        .iter()
                        .any(|t| (t[0] - px).hypot(t[1] - pz) < t[2] + radius * 0.65);
                    if !clash_tree && !clash_taken {
                        let dist = (px - self.office_mid.x).hypot(pz - self.office_mid.y);
                        let cam_bias = if out.x + out.y > 0.0 { 0.0 } else { 4.0 };
                        let score = dist + cam_bias + a.abs() * 0.35;
                        let y = if lawn { LAWN_Y } else { PAVE_Y };
                        let frame = SpotFrame {
                            origin: Vec3::new(px, y, pz),
                            along,
                            out,
                            yaw: (-along.y).atan2(along.x),
                        };
                        if best.as_ref().is_none_or(|b| score < b.0) {
                            best = Some((score, frame, radius));
                        }
                    }
                    a += 0.8;
                }
            }
        }
        if let Some((_, frame, r)) = best {
            self.taken.push([frame.origin.x, frame.origin.z, r]);
            Some(frame)
        } else {
            None
        }
    }
}

/// Spawns the complete 1:1 City exterior backdrop: multi-block supergrid, stepped skyscrapers,
/// rooftop clutter, streetlamps & night pools, utility wires, countdown traffic signals, all 10
/// walk-in 3D Vietnamese shops, *Quán cơm bình dân* / Friday *Bún đậu*, all 13+ `cars.json` +
/// scooter traffic & pedestrians, UV-cropped ATM / V-Green / Photobooth / Pickleball, and all 8
/// cultural street life scenes. Every asset registers inside `BackdropAssets` for 0-leak purge.
#[allow(clippy::too_many_arguments)]
pub fn spawn_city_backdrop(
    commands: &mut Commands,
    meshes: &mut Assets<Mesh>,
    materials: &mut Assets<StandardMaterial>,
    images: &mut Assets<Image>,
    ba: &mut BackdropAssets,
    brands: &mut BrandRegistry,
    fonts: &mut FontEngine,
    spec: &LayoutSpec,
    city: &str,
    night: bool,
    temp_c: f32,
    is_friday: bool,
) {
    let grid = city_grid::spawn_city_grid(commands, meshes, materials, images, ba, spec, night);
    let signs_mat = ba.textured(images, materials, draw_street_signs_atlas(fonts), false, 0.45);
    shops_walkin::spawn_walkin_shops(
        commands,
        meshes,
        materials,
        images,
        ba,
        brands,
        fonts,
        &grid.shop_fronts,
        city,
        night,
    );
    traffic::spawn_street_traffic(commands, meshes, materials, ba, &grid, signs_mat.clone(), temp_c);
    let mut finder = SpotFinder::new(
        grid.office_mid,
        grid.parks.clone(),
        grid.plazas.clone(),
        grid.trees.clone(),
    );
    eat_com::spawn_eat_com(commands, meshes, materials, images, ba, fonts, &mut finder, is_friday);
    scenes_a::spawn_street_scenes_a(
        commands,
        meshes,
        materials,
        images,
        ba,
        fonts,
        &grid,
        &mut finder,
        night,
    );
    scenes_b::spawn_street_scenes_b(
        commands,
        meshes,
        materials,
        ba,
        &grid,
        &mut finder,
        signs_mat,
        night,
    );
}

/// Zero-allocation Update system driving live traffic signals, countdown LED color state,
/// aviation rooftop beacons, and city day/night material lighting transitions.
#[allow(clippy::too_many_arguments, clippy::type_complexity)]
pub fn animate_city_signals_and_night_system(
    time: Res<Time>,
    lighting: Option<Res<LightingState>>,
    mut materials: Option<ResMut<Assets<StandardMaterial>>>,
    mut lamps: Query<(&city_grid::TrafficSignalLamp, &mut Visibility), (Without<city_grid::RooftopBeacon>, Without<city_grid::CityNightLampPool>)>,
    mut countdowns: Query<(&mut city_grid::TrafficCountdownDisplay, &mut Mesh3d)>,
    mut beacons: Query<(&city_grid::RooftopBeacon, &mut Visibility), (Without<city_grid::TrafficSignalLamp>, Without<city_grid::CityNightLampPool>)>,
    mut pools: Query<&mut Visibility, (With<city_grid::CityNightLampPool>, Without<city_grid::TrafficSignalLamp>, Without<city_grid::RooftopBeacon>)>,
    added_city: Query<(), Added<city_grid::CityNightAsphalt>>,
    asphalt_q: Query<&MeshMaterial3d<StandardMaterial>, With<city_grid::CityNightAsphalt>>,
    facade_q: Query<&MeshMaterial3d<StandardMaterial>, With<city_grid::CityNightFacade>>,
    slab_q: Query<&MeshMaterial3d<StandardMaterial>, With<city_grid::CityNightSidewalk>>,
    bulb_q: Query<&MeshMaterial3d<StandardMaterial>, With<city_grid::CityNightLampBulb>>,
    shop_lightbox_q: Query<&MeshMaterial3d<StandardMaterial>, With<shops_walkin::ShopLightboxNight>>,
    nhau_bulb_q: Query<&MeshMaterial3d<StandardMaterial>, With<scenes_b::NhauBulbNight>>,
    mut last_night: Local<Option<bool>>,
) {
    let t = time.elapsed_secs();

    for (lamp, mut vis) in &mut lamps {
        let cur = city_grid::signal_color_of(city_grid::signal_phase(lamp.i, lamp.j, t, lamp.axis_x));
        *vis = if cur == lamp.color {
            Visibility::Inherited
        } else {
            Visibility::Hidden
        };
    }

    for (mut cd, mut mesh_h) in &mut countdowns {
        let frame = (city_grid::signal_phase(cd.i, cd.j, t, cd.axis_x).floor() as usize).min(21) as u8;
        if cd.last_frame != frame {
            cd.last_frame = frame;
            mesh_h.0 = cd.frames[frame as usize].clone();
        }
    }

    for (b, mut vis) in &mut beacons {
        let on = (t + b.phase).rem_euclid(1.6) < 0.28;
        *vis = if on {
            Visibility::Inherited
        } else {
            Visibility::Hidden
        };
    }

    if let Some(l) = lighting {
        let night = l.is_night();
        if *last_night != Some(night) || !added_city.is_empty() {
            *last_night = Some(night);
            for mut vis in &mut pools {
                *vis = if night {
                    Visibility::Inherited
                } else {
                    Visibility::Hidden
                };
            }
            if let Some(mats) = materials.as_deref_mut() {
                for mh in &asphalt_q {
                    if let Some(m) = mats.get_mut(&mh.0) {
                        m.base_color = srgb_hex(if night { "#1B1F27" } else { "#5B616A" });
                    }
                }
                let em = if night { 0.9 } else { 0.0 };
                for mh in &facade_q {
                    if let Some(m) = mats.get_mut(&mh.0) {
                        m.emissive = LinearRgba::new(em, em, em, 1.0);
                    }
                }
                for mh in &slab_q {
                    if let Some(m) = mats.get_mut(&mh.0) {
                        m.base_color = if night {
                            Color::srgb(0.35, 0.35, 0.38)
                        } else {
                            Color::WHITE
                        };
                    }
                }
                for mh in &bulb_q {
                    if let Some(m) = mats.get_mut(&mh.0) {
                        m.base_color = srgb_hex(if night { "#FFE2A6" } else { "#5A5E66" });
                        m.emissive = if night {
                            LinearRgba::new(2.2, 1.65, 0.9, 1.0)
                        } else {
                            LinearRgba::BLACK
                        };
                        m.unlit = night;
                    }
                }
                let shop_em = if night { 0.42 } else { 0.08 };
                for mh in &shop_lightbox_q {
                    if let Some(m) = mats.get_mut(&mh.0) {
                        m.emissive = LinearRgba::new(shop_em, shop_em, shop_em, 1.0);
                    }
                }
                let nhau_lin = srgb_hex("#FFD47A").to_linear();
                let nhau_str = if night { 2.6 } else { 0.4 };
                for mh in &nhau_bulb_q {
                    if let Some(m) = mats.get_mut(&mh.0) {
                        m.emissive = LinearRgba::new(
                            nhau_lin.red * nhau_str,
                            nhau_lin.green * nhau_str,
                            nhau_lin.blue * nhau_str,
                            1.0,
                        );
                    }
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use bevy::render::mesh::VertexAttributeValues;

    #[test]
    fn push_cyl_rot_emits_outward_wound_triangles_for_sides_and_cap() {
        let mut b = StreetMeshBuilder::default();
        b.push_cyl_rot(
            Vec3::new(1.0, 2.0, -3.0),
            0.35,
            0.50,
            1.6,
            12,
            Quat::from_euler(EulerRot::YXZ, 0.4, 0.25, -0.15),
            "#AABBCC",
        );
        let mesh = b.build();
        let positions = match mesh.attribute(Mesh::ATTRIBUTE_POSITION) {
            Some(VertexAttributeValues::Float32x3(v)) => v,
            other => panic!("expected Float32x3 positions, got {other:?}"),
        };
        let normals = match mesh.attribute(Mesh::ATTRIBUTE_NORMAL) {
            Some(VertexAttributeValues::Float32x3(v)) => v,
            other => panic!("expected Float32x3 normals, got {other:?}"),
        };
        let indices: Vec<usize> = match mesh.indices() {
            Some(Indices::U32(v)) => v.iter().map(|&i| i as usize).collect(),
            other => panic!("expected U32 indices, got {other:?}"),
        };
        assert!(!indices.is_empty());
        for (tri_idx, tri) in indices.chunks_exact(3).enumerate() {
            let p0 = Vec3::from_array(positions[tri[0]]);
            let p1 = Vec3::from_array(positions[tri[1]]);
            let p2 = Vec3::from_array(positions[tri[2]]);
            let n = Vec3::from_array(normals[tri[0]]);
            let cross = (p1 - p0).cross(p2 - p0);
            assert!(
                cross.dot(n) > 0.0,
                "triangle {tri_idx} has inverted winding: cross={cross:?}, normal={n:?}"
            );
        }
    }

    #[test]
    fn push_facade_box_populates_uv1_scaled_1_16_by_1_32_and_aligns_notched_v_seam() {
        let mut b = StreetMeshBuilder::default();
        b.push_facade_box(0.0, 0.0, 7.6, 5.7, 2.75, 6.25, "#CFC8BC", 3.0, 7.0, 1.9, 3.0);
        let mesh = b.build();

        let uv0 = match mesh.attribute(Mesh::ATTRIBUTE_UV_0) {
            Some(VertexAttributeValues::Float32x2(v)) => v,
            other => panic!("expected UV_0, got {other:?}"),
        };
        let uv1 = match mesh.attribute(Mesh::ATTRIBUTE_UV_1) {
            Some(VertexAttributeValues::Float32x2(v)) => v,
            other => panic!("expected UV_1 on facade mesh, got {other:?}"),
        };
        assert_eq!(uv0.len(), uv1.len());
        for (a, c) in uv0.iter().zip(uv1.iter()) {
            assert!((c[0] - a[0] / 16.0).abs() < 1e-5, "UV_1.u mismatch: {a:?} vs {c:?}");
            assert!((c[1] - a[1] / 32.0).abs() < 1e-5, "UV_1.v mismatch: {a:?} vs {c:?}");
        }
        // Bottom V of upper notched piece at y0=2.75 must equal off_v - 2.75/3.0
        let expected_v_bot = 7.0 - 2.75 / 3.0;
        let expected_v_top = 7.0 - (2.75 + 6.25) / 3.0;
        assert!((uv0[0][1] - expected_v_bot).abs() < 1e-5);
        assert!((uv0[2][1] - expected_v_top).abs() < 1e-5);
    }

    #[test]
    fn city_beach_city_cycle_while_night_on_applies_night_materials_and_toggles_live() {
        use crate::backdrops::{BackdropKind, spawn_backdrop};
        use crate::layout::OfficeLayout;
        use crate::palette::WorldPalette;
        use std::path::PathBuf;

        let assets_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../assets");
        let spec = LayoutSpec::for_layout(OfficeLayout::Pro);
        let mut fonts = FontEngine::from_assets_dir(&assets_dir);
        let mut brands = BrandRegistry::new(assets_dir);

        let mut app = App::new();
        app.insert_resource(Time::<()>::default());
        app.insert_resource(LightingState::new(true));
        app.insert_resource(Assets::<Mesh>::default());
        app.insert_resource(Assets::<StandardMaterial>::default());
        app.insert_resource(Assets::<Image>::default());
        app.insert_resource(BackdropAssets::default());
        let pal = {
            let mut meshes = app.world_mut().resource_mut::<Assets<Mesh>>();
            WorldPalette::new(&mut meshes)
        };
        app.insert_resource(pal);
        app.add_systems(Update, animate_city_signals_and_night_system);

        let spawn_kind = |app: &mut App, kind: BackdropKind, pass_night: bool, brands: &mut BrandRegistry, fonts: &mut FontEngine| {
            let w = app.world_mut();
            let to_despawn: Vec<Entity> = w
                .query_filtered::<Entity, With<BackdropEntity>>()
                .iter(w)
                .collect();
            for e in to_despawn {
                w.despawn(e);
            }
            w.resource_scope(|w, mut ba: Mut<BackdropAssets>| {
                w.resource_scope(|w, mut meshes: Mut<Assets<Mesh>>| {
                    w.resource_scope(|w, mut mats: Mut<Assets<StandardMaterial>>| {
                        let mut imgs = w.resource_mut::<Assets<Image>>();
                        ba.purge(&mut meshes, &mut mats, &mut imgs);
                    });
                });
            });
            w.resource_scope(|w, mut pal: Mut<WorldPalette>| {
                w.resource_scope(|w, mut meshes: Mut<Assets<Mesh>>| {
                    w.resource_scope(|w, mut mats: Mut<Assets<StandardMaterial>>| {
                        w.resource_scope(|w, mut imgs: Mut<Assets<Image>>| {
                            w.resource_scope(|w, mut ba: Mut<BackdropAssets>| {
                                let mut cmds = w.commands();
                                spawn_backdrop(
                                    &mut cmds,
                                    &mut pal,
                                    &mut meshes,
                                    &mut mats,
                                    &mut imgs,
                                    &mut ba,
                                    brands,
                                    fonts,
                                    &spec,
                                    kind,
                                    pass_night,
                                    "hcm",
                                    31.0,
                                );
                            });
                        });
                    });
                });
            });
            w.flush();
        };

        // 1. Spawn City while night=true -> run update (sets Local last_night = Some(true))
        spawn_kind(&mut app, BackdropKind::City, true, &mut brands, &mut fonts);
        app.update();

        // 2. Switch to Beach while night=true
        spawn_kind(&mut app, BackdropKind::Beach, true, &mut brands, &mut fonts);
        app.update();

        // 3. Switch back to City (even if initially spawned with day colors) while LightingState.night == true
        spawn_kind(&mut app, BackdropKind::City, false, &mut brands, &mut fonts);
        app.update();

        // Verify Added<CityNightAsphalt> re-applied night mode to the newly spawned City!
        let w = app.world_mut();
        let asphalt_handles: Vec<_> = w
            .query_filtered::<&MeshMaterial3d<StandardMaterial>, With<city_grid::CityNightAsphalt>>()
            .iter(w)
            .map(|m| m.0.clone())
            .collect();
        let facade_handles: Vec<_> = w
            .query_filtered::<&MeshMaterial3d<StandardMaterial>, With<city_grid::CityNightFacade>>()
            .iter(w)
            .map(|m| m.0.clone())
            .collect();
        let pool_vis: Vec<_> = w
            .query_filtered::<&Visibility, With<city_grid::CityNightLampPool>>()
            .iter(w)
            .copied()
            .collect();
        let shop_handles: Vec<_> = w
            .query_filtered::<&MeshMaterial3d<StandardMaterial>, With<shops_walkin::ShopLightboxNight>>()
            .iter(w)
            .map(|m| m.0.clone())
            .collect();
        let nhau_handles: Vec<_> = w
            .query_filtered::<&MeshMaterial3d<StandardMaterial>, With<scenes_b::NhauBulbNight>>()
            .iter(w)
            .map(|m| m.0.clone())
            .collect();

        let mats = w.resource::<Assets<StandardMaterial>>();
        assert!(!asphalt_handles.is_empty());
        for h in &asphalt_handles {
            assert_eq!(mats.get(h).unwrap().base_color, srgb_hex("#1B1F27"));
        }
        assert!(!facade_handles.is_empty());
        for h in &facade_handles {
            assert!((mats.get(h).unwrap().emissive.red - 0.9).abs() < 1e-4);
        }
        assert!(!pool_vis.is_empty());
        assert!(pool_vis.iter().all(|&v| v == Visibility::Inherited));
        assert!(!shop_handles.is_empty());
        for h in &shop_handles {
            assert!((mats.get(h).unwrap().emissive.red - 0.42).abs() < 1e-4);
        }
        assert!(!nhau_handles.is_empty());
        let expected_night_bulb = srgb_hex("#FFD47A").to_linear().red * 2.6;
        for h in &nhau_handles {
            assert!((mats.get(h).unwrap().emissive.red - expected_night_bulb).abs() < 1e-3);
        }

        // 4. Toggle Night -> Day live and verify shop lightboxes & Nhậu bulbs dim immediately
        app.world_mut().resource_mut::<LightingState>().night = false;
        app.update();
        let mats = app.world().resource::<Assets<StandardMaterial>>();
        for h in &shop_handles {
            assert!((mats.get(h).unwrap().emissive.red - 0.08).abs() < 1e-4);
        }
        let expected_day_bulb = srgb_hex("#FFD47A").to_linear().red * 0.4;
        for h in &nhau_handles {
            assert!((mats.get(h).unwrap().emissive.red - expected_day_bulb).abs() < 1e-3);
        }
    }

    #[test]
    fn zero_terrace_overlap_and_exact_signal_indices_across_cozy_pro_and_hq() {
        use crate::layout::OfficeLayout;
        use std::path::PathBuf;

        let assets_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../assets");
        let mut fonts = FontEngine::from_assets_dir(&assets_dir);
        let aabb_overlaps = |r: [f32; 4], cx: f32, cz: f32, hx: f32, hz: f32| -> bool {
            (cx + hx) > r[0] && (cx - hx) < r[1] && (cz + hz) > r[2] && (cz - hz) < r[3]
        };

        for layout in [OfficeLayout::Cozy, OfficeLayout::Pro, OfficeLayout::Hq] {
            let spec = LayoutSpec::for_layout(layout);
            let excl = city_grid::terrace_exclusion(layout);
            let mut world = World::new();
            world.insert_resource(Assets::<Mesh>::default());
            world.insert_resource(Assets::<StandardMaterial>::default());
            world.insert_resource(Assets::<Image>::default());
            world.insert_resource(BackdropAssets::default());

            let grid = world.resource_scope(|w, mut meshes: Mut<Assets<Mesh>>| {
                w.resource_scope(|w, mut mats: Mut<Assets<StandardMaterial>>| {
                    w.resource_scope(|w, mut imgs: Mut<Assets<Image>>| {
                        w.resource_scope(|w, mut ba: Mut<BackdropAssets>| {
                            let mut cmds = w.commands();
                            let grid = city_grid::spawn_city_grid(
                                &mut cmds,
                                &mut meshes,
                                &mut mats,
                                &mut imgs,
                                &mut ba,
                                &spec,
                                false,
                            );
                            let signs_mat = ba.textured(
                                &mut imgs,
                                &mut mats,
                                draw_street_signs_atlas(&mut fonts),
                                false,
                                0.45,
                            );
                            traffic::spawn_street_traffic(
                                &mut cmds,
                                &mut meshes,
                                &mut mats,
                                &mut ba,
                                &grid,
                                signs_mat.clone(),
                                34.0,
                            );
                            let mut finder = SpotFinder::new(
                                grid.office_mid,
                                grid.parks.clone(),
                                grid.plazas.clone(),
                                grid.trees.clone(),
                            );
                            scenes_a::spawn_street_scenes_a(
                                &mut cmds,
                                &mut meshes,
                                &mut mats,
                                &mut imgs,
                                &mut ba,
                                &mut fonts,
                                &grid,
                                &mut finder,
                                false,
                            );
                            scenes_b::spawn_street_scenes_b(
                                &mut cmds,
                                &mut meshes,
                                &mut mats,
                                &mut ba,
                                &grid,
                                &mut finder,
                                signs_mat,
                                false,
                            );
                            grid
                        })
                    })
                })
            });
            world.flush();

            // 1. Streetlamps & signal poles outside terrace
            assert!(!grid.lamp_positions.is_empty());
            for &[lx, lz] in &grid.lamp_positions {
                assert!(
                    !city_grid::rect_contains_padded(excl, lx, lz, 0.0),
                    "{layout:?}: streetlamp at ({lx}, {lz}) overlaps terrace {excl:?}"
                );
            }
            for sp in &grid.signal_poles {
                assert!(
                    !city_grid::rect_contains_padded(excl, sp.x, sp.z, 0.0),
                    "{layout:?}: signal pole at ({}, {}) overlaps terrace {excl:?}",
                    sp.x,
                    sp.z
                );
            }

            // 2. Pedestrian tracks outside terrace along entire walk segment
            let peds: Vec<_> = world.query::<&traffic::StreetPedestrian>().iter(&world).collect();
            assert!(!peds.is_empty());
            for p in peds {
                let (cx, cz, hx, hz) = if p.axis_x {
                    ((p.min_pos + p.max_pos) * 0.5, p.side_coord, (p.max_pos - p.min_pos) * 0.5, 0.15)
                } else {
                    (p.side_coord, (p.min_pos + p.max_pos) * 0.5, 0.15, (p.max_pos - p.min_pos) * 0.5)
                };
                assert!(
                    !aabb_overlaps(excl, cx, cz, hx, hz),
                    "{layout:?}: pedestrian track (axis_x={}, side={}, range={}..{}) overlaps terrace {excl:?}",
                    p.axis_x,
                    p.side_coord,
                    p.min_pos,
                    p.max_pos
                );
            }

            // 3. Bus stop shelter outside terrace
            let stops: Vec<_> = world.query::<&traffic::BusStopShelter>().iter(&world).collect();
            assert_eq!(stops.len(), 1);
            for s in stops {
                assert!(
                    !aabb_overlaps(excl, s.center.x, s.center.z, s.half_extents.x, s.half_extents.y),
                    "{layout:?}: bus stop {:?} overlaps terrace {excl:?}",
                    s.center
                );
            }

            // 4. EV scooter row outside terrace
            let ev_rows: Vec<_> = world.query::<&scenes_a::StreetEvScooterRow>().iter(&world).collect();
            assert_eq!(ev_rows.len(), 1);
            for r in ev_rows {
                assert!(
                    !aabb_overlaps(excl, r.center.x, r.center.z, r.half_extents.x, r.half_extents.y),
                    "{layout:?}: EV scooter row {:?} overlaps terrace {excl:?}",
                    r.center
                );
            }

            // 5. CSGT checkpoint outside terrace
            let csgt_list: Vec<_> = world
                .query::<(&scenes_b::StreetCultureScene, &Transform)>()
                .iter(&world)
                .filter(|(s, _)| **s == scenes_b::StreetCultureScene::CsgtCheckpoint)
                .map(|(_, tf)| tf.translation)
                .collect();
            assert_eq!(csgt_list.len(), 1);
            for pos in csgt_list {
                assert!(
                    !aabb_overlaps(excl, pos.x, pos.z, 1.8, 0.85),
                    "{layout:?}: CSGT checkpoint {pos:?} overlaps terrace {excl:?}"
                );
            }

            // 6. Important #6: every CityVehicle's (sig_i, sig_j) resolves its true stop_cross intersection
            let vehicles: Vec<_> = world.query::<&traffic::CityVehicle>().iter(&world).collect();
            assert!(!vehicles.is_empty());
            for v in vehicles {
                assert!(v.sig_i < grid.xs.len() && v.sig_j < grid.zs.len());
                if v.axis_x {
                    assert!(
                        (grid.xs[v.sig_i] - v.stop_cross).abs() < 0.01,
                        "{layout:?}: X-vehicle sig_i={} ({}) != stop_cross {}",
                        v.sig_i,
                        grid.xs[v.sig_i],
                        v.stop_cross
                    );
                    assert!(
                        (grid.zs[v.sig_j] - v.lane_coord).abs() < 2.2,
                        "{layout:?}: X-vehicle sig_j={} ({}) doesn't match lane {}",
                        v.sig_j,
                        grid.zs[v.sig_j],
                        v.lane_coord
                    );
                } else {
                    assert!(
                        (grid.zs[v.sig_j] - v.stop_cross).abs() < 0.01,
                        "{layout:?}: Z-vehicle sig_j={} ({}) != stop_cross {}",
                        v.sig_j,
                        grid.zs[v.sig_j],
                        v.stop_cross
                    );
                    assert!(
                        (grid.xs[v.sig_i] - v.lane_coord).abs() < 2.2,
                        "{layout:?}: Z-vehicle sig_i={} ({}) doesn't match lane {}",
                        v.sig_i,
                        grid.xs[v.sig_i],
                        v.lane_coord
                    );
                }
            }
        }
    }
}
