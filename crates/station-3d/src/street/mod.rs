pub mod city_grid;
pub mod eat_com;
pub mod shops_walkin;
pub mod traffic;

use crate::backdrops::{BackdropAssets, BackdropEntity};
use crate::palette::{WorldEntity, parse_hex_rgb};
use bevy::asset::RenderAssetUsages;
use bevy::prelude::*;
use bevy::render::mesh::{Indices, PrimitiveTopology};
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
    /// (matching `city.js:132-140` `addBox`) and neutral top/bottom UVs.
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
        let v0 = off_v + y0 / floor_h;
        let v1 = v0 + h / floor_h;
        let side_uv = |bays: f32| {
            [
                [off_u, v1],
                [off_u + bays, v1],
                [off_u + bays, v0],
                [off_u, v0],
            ]
        };
        let cap_uv = [[0.02, 0.02]; 4];

        // +Z
        self.push_quad(
            Vec3::new(cx - hx, yb, cz + hz),
            Vec3::new(cx + hx, yb, cz + hz),
            Vec3::new(cx + hx, yt, cz + hz),
            Vec3::new(cx - hx, yt, cz + hz),
            Vec3::Z,
            side_uv(bays_x),
            col,
        );
        // -Z
        self.push_quad(
            Vec3::new(cx + hx, yb, cz - hz),
            Vec3::new(cx - hx, yb, cz - hz),
            Vec3::new(cx - hx, yt, cz - hz),
            Vec3::new(cx + hx, yt, cz - hz),
            Vec3::NEG_Z,
            side_uv(bays_x),
            col,
        );
        // +X
        self.push_quad(
            Vec3::new(cx + hx, yb, cz + hz),
            Vec3::new(cx + hx, yb, cz - hz),
            Vec3::new(cx + hx, yt, cz - hz),
            Vec3::new(cx + hx, yt, cz + hz),
            Vec3::X,
            side_uv(bays_z),
            col,
        );
        // -X
        self.push_quad(
            Vec3::new(cx - hx, yb, cz - hz),
            Vec3::new(cx - hx, yb, cz + hz),
            Vec3::new(cx - hx, yt, cz + hz),
            Vec3::new(cx - hx, yt, cz - hz),
            Vec3::NEG_X,
            side_uv(bays_z),
            col,
        );
        // +Y top
        self.push_quad(
            Vec3::new(cx - hx, yt, cz + hz),
            Vec3::new(cx + hx, yt, cz + hz),
            Vec3::new(cx + hx, yt, cz - hz),
            Vec3::new(cx - hx, yt, cz - hz),
            Vec3::Y,
            cap_uv,
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
            self.push_quad(
                b0,
                b1,
                t1,
                t0,
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
