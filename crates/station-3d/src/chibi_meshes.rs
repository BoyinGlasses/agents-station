use crate::chibi::{ChibiDims, ChibiLook};
use bevy::asset::RenderAssetUsages;
use bevy::prelude::*;
use bevy::render::mesh::{Indices, PrimitiveTopology, VertexAttributeValues};
use bevy::render::render_resource::{Extent3d, TextureDimension, TextureFormat};
use std::f32::consts::{FRAC_PI_2, PI, TAU};

/// All 6 sculpted hairstyles ported 1:1 from `chibi-rig.js`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum HairStyle {
    Bob,
    Plain,
    Spiky,
    Ponytail,
    Bun,
    Curly,
}

impl HairStyle {
    pub const ALL: [HairStyle; 6] = [
        HairStyle::Bob,
        HairStyle::Plain,
        HairStyle::Spiky,
        HairStyle::Ponytail,
        HairStyle::Bun,
        HairStyle::Curly,
    ];

    pub fn from_seed(seed: usize) -> Self {
        Self::ALL[seed % Self::ALL.len()]
    }

    pub fn idx(self) -> usize {
        match self {
            HairStyle::Bob => 0,
            HairStyle::Plain => 1,
            HairStyle::Spiky => 2,
            HairStyle::Ponytail => 3,
            HairStyle::Bun => 4,
            HairStyle::Curly => 5,
        }
    }
}

/// Parameters controlling `hair_shell` sculpting, ported 1:1 from `chibi-rig.js`.
#[derive(Debug, Clone, Copy)]
pub struct HairShellOpts {
    pub locks: f32,
    pub tip: f32,
    pub bulge: f32,
    pub phase: f32,
    pub sweep: f32,
    pub tuck: f32,
    pub round: f32,
    pub bumps: f32,
    pub twist: f32,
}

fn hair_spec(style: HairStyle, look: ChibiLook) -> ([f32; 3], HairShellOpts) {
    let toy = look == ChibiLook::Toy;
    let (base_line, base_locks, tip_opt, bulge_opt, phase, sweep, round_opt, bumps, twist_opt) =
        match style {
            HairStyle::Bob => ([1.12, 1.98, 2.15], 10.0, None, None, 0.0, 0.0, None, 0.0, None),
            HairStyle::Plain => (
                [1.10, 1.60, 1.95],
                9.0,
                None,
                None,
                0.3,
                0.16,
                None,
                0.0,
                None,
            ),
            HairStyle::Spiky => (
                [1.02, 1.50, 1.85],
                9.0,
                Some(0.20),
                None,
                0.0,
                0.0,
                Some(2.0),
                0.0,
                None,
            ),
            HairStyle::Ponytail => (
                [1.12, 1.72, 1.82],
                10.0,
                None,
                None,
                0.0,
                -0.12,
                None,
                0.0,
                None,
            ),
            HairStyle::Bun => ([1.10, 1.60, 1.80], 10.0, None, None, 0.0, 0.0, None, 0.0, None),
            HairStyle::Curly => (
                [1.15, 1.85, 2.05],
                15.0,
                Some(0.10),
                Some(0.06),
                0.0,
                0.0,
                Some(0.5),
                0.045,
                Some(0.0),
            ),
        };
    let line = if toy {
        [base_line[0] + 0.2, base_line[1] + 0.06, base_line[2]]
    } else {
        base_line
    };
    let opts = if toy {
        HairShellOpts {
            locks: (base_locks * 0.75_f32).round(),
            tip: tip_opt.unwrap_or(0.12),
            bulge: bulge_opt.unwrap_or(0.055),
            phase,
            sweep,
            tuck: 0.08,
            round: round_opt.unwrap_or(0.7),
            bumps,
            twist: twist_opt.unwrap_or(0.55),
        }
    } else {
        HairShellOpts {
            locks: base_locks,
            tip: tip_opt.unwrap_or(0.17),
            bulge: bulge_opt.unwrap_or(0.035),
            phase,
            sweep,
            tuck: 0.08,
            round: round_opt.unwrap_or(1.3),
            bumps,
            twist: twist_opt.unwrap_or(0.35),
        }
    };
    (line, opts)
}

/// Shared indexed mesh, texture, and material cache for every ChibiRig in the office.
/// Keeps ALL character geometry (even 44+ workers, subagents, and NPCs) well under 1.5 MB.
#[derive(Resource)]
pub struct ChibiMeshCache {
    pub hair_pixar: [Handle<Mesh>; 6],
    pub hair_toy: [Handle<Mesh>; 6],
    pub spiky_cluster_pixar: Handle<Mesh>,
    pub spiky_cluster_toy: Handle<Mesh>,
    pub ponytail_pixar: Handle<Mesh>,
    pub ponytail_toy: Handle<Mesh>,
    pub ahoge_pixar: Handle<Mesh>,
    pub ahoge_toy: Handle<Mesh>,
    pub skull_pixar: Handle<Mesh>,
    pub skull_toy: Handle<Mesh>,
    pub torso_pixar: Handle<Mesh>,
    pub torso_toy: Handle<Mesh>,
    pub coat_pixar: Handle<Mesh>,
    pub coat_toy: Handle<Mesh>,
    pub suit_v_pixar: Handle<Mesh>,
    pub suit_v_toy: Handle<Mesh>,

    // Shared compact indexed primitives for limbs, eyes, outfits, accessories, and 15+ props
    pub sphere: Handle<Mesh>,
    pub box_mesh: Handle<Mesh>,
    pub cyl: Handle<Mesh>,
    pub cyl_open: Handle<Mesh>,
    pub capsule: Handle<Mesh>,
    pub cone: Handle<Mesh>,
    pub plane: Handle<Mesh>,
    pub hemi_dome: Handle<Mesh>,
    pub half_brim: Handle<Mesh>,
    pub tri_prism: Handle<Mesh>,
    pub cake_wedge: Handle<Mesh>,
    pub torus_thin: Handle<Mesh>,
    pub torus_med: Handle<Mesh>,
    pub torus_thick: Handle<Mesh>,
    pub arc_pi_thin: Handle<Mesh>,
    pub arc_pi_med: Handle<Mesh>,
    pub arc_pi_thick: Handle<Mesh>,
    pub arc_brow: Handle<Mesh>,
    pub arc_crease: Handle<Mesh>,
    pub arc_collar: Handle<Mesh>,

    // Radial drop shadow & hot-food steam shared assets (RENDER_WORLD textures)
    pub shadow_quad: Handle<Mesh>,
    pub shadow_image: Handle<Image>,
    pub shadow_mat: Handle<StandardMaterial>,
    pub steam_image: Handle<Image>,
    pub steam_mat: Handle<StandardMaterial>,

    owned_meshes: Vec<Handle<Mesh>>,
    owned_images: Vec<Handle<Image>>,
    owned_materials: Vec<Handle<StandardMaterial>>,
}

impl ChibiMeshCache {
    pub fn build(
        meshes: &mut Assets<Mesh>,
        materials: &mut Assets<StandardMaterial>,
        images: &mut Assets<Image>,
    ) -> Self {
        let mut owned_meshes = Vec::with_capacity(48);
        let mut add_mesh = |m: Mesh| -> Handle<Mesh> {
            let h = meshes.add(m);
            owned_meshes.push(h.clone());
            h
        };

        let d_pixar = ChibiDims::for_look(ChibiLook::Pixar);
        let d_toy = ChibiDims::for_look(ChibiLook::Toy);

        // 1. All 6 sculpted hairstyles for Pixar & Toy via exact hair_shell(NI=48, NJ=16)
        let hr_pixar = d_pixar.r * 1.12;
        let hr_toy = d_toy.r * 1.10;
        let hair_pixar = HairStyle::ALL.map(|style| {
            let (line, opts) = hair_spec(style, ChibiLook::Pixar);
            add_mesh(hair_shell(hr_pixar, line, opts, 48, 16))
        });
        let hair_toy = HairStyle::ALL.map(|style| {
            let (line, opts) = hair_spec(style, ChibiLook::Toy);
            add_mesh(hair_shell(hr_toy, line, opts, 48, 16))
        });

        // 2. Swept hair attachments (spiky 6-lock cluster, ponytail tail, stray ahoge strand)
        let spiky_cluster_pixar = add_mesh(build_spiky_cluster(d_pixar.r, false));
        let spiky_cluster_toy = add_mesh(build_spiky_cluster(d_toy.r, true));
        let ponytail_pixar = add_mesh(build_ponytail_sweep(d_pixar.r));
        let ponytail_toy = add_mesh(build_ponytail_sweep(d_toy.r));
        let ahoge_pixar = add_mesh(build_ahoge_sweep(d_pixar.r));
        let ahoge_toy = add_mesh(build_ahoge_sweep(d_toy.r));

        // 3. Sculpted skulls (cheek/chin/front volume)
        let skull_pixar = add_mesh(build_sculpted_skull(ChibiLook::Pixar, 28, 20));
        let skull_toy = add_mesh(build_sculpted_skull(ChibiLook::Toy, 28, 20));

        // 4. Lathe torsos & open-front coats
        let torso_pixar = add_mesh(lathe_profile(ChibiDims::PIXAR_TORSO, 28, 0.0, TAU));
        let torso_toy = add_mesh(lathe_profile(ChibiDims::TOY_TORSO, 28, 0.0, TAU));
        let coat_pixar = add_mesh(build_coat_lathe(ChibiDims::PIXAR_TORSO, d_pixar.torso_top));
        let coat_toy = add_mesh(build_coat_lathe(ChibiDims::TOY_TORSO, d_toy.torso_top));
        let suit_v_pixar = add_mesh(build_suit_v_mesh(d_pixar.torso_r, d_pixar.torso_top));
        let suit_v_toy = add_mesh(build_suit_v_mesh(d_toy.torso_r, d_toy.torso_top));

        // 5. Compact shared indexed primitives
        let sphere = add_mesh(build_uv_sphere(1.0, 20, 14));
        let box_mesh = add_mesh(Mesh::from(Cuboid::new(1.0, 1.0, 1.0)));
        let cyl = add_mesh(build_cylinder(1.0, 1.0, 1.0, 18, false, 0.0, TAU));
        let cyl_open = add_mesh(build_cylinder(1.0, 1.0, 1.0, 20, true, 0.0, TAU));
        let capsule = add_mesh(Mesh::from(Capsule3d::new(0.5, 1.0)));
        let cone = add_mesh(Mesh::from(Cone::new(0.5, 1.0)));
        let plane = add_mesh(Mesh::from(Plane3d::new(Vec3::Z, Vec2::splat(0.5))));
        let hemi_dome = add_mesh(build_hemisphere(1.0, 24, 12));
        let half_brim = add_mesh(build_cylinder(1.0, 1.0, 1.0, 18, false, -FRAC_PI_2, PI));
        let tri_prism = add_mesh(build_cylinder(1.0, 1.0, 1.0, 3, false, 0.0, TAU));
        let cake_wedge = add_mesh(build_cylinder(1.0, 1.0, 1.0, 10, false, 0.0, PI / 2.2));

        let torus_thin = add_mesh(torus_arc(1.0, 0.08, 8, 22, TAU));
        let torus_med = add_mesh(torus_arc(1.0, 0.16, 8, 22, TAU));
        let torus_thick = add_mesh(torus_arc(1.0, 0.28, 8, 20, TAU));
        let arc_pi_thin = add_mesh(torus_arc(1.0, 0.15, 6, 18, PI));
        let arc_pi_med = add_mesh(torus_arc(1.0, 0.22, 6, 18, PI));
        let arc_pi_thick = add_mesh(torus_arc(1.0, 0.30, 6, 16, PI));
        let arc_brow = add_mesh(torus_arc(1.0, 0.20, 6, 14, PI * 0.6));
        let arc_crease = add_mesh(torus_arc(1.0, 0.055, 5, 14, PI * 0.7));
        let arc_collar = add_mesh(torus_arc(1.0, 0.17, 6, 20, PI * 1.55));

        // 6. Radial drop shadow quad + shared 64x64 RENDER_WORLD radial alpha image
        let shadow_quad = add_mesh(Mesh::from(Plane3d::new(Vec3::Y, Vec2::splat(0.475))));
        let shadow_image = images.add(build_radial_shadow_image());
        let shadow_mat = materials.add(StandardMaterial {
            base_color: Color::WHITE,
            base_color_texture: Some(shadow_image.clone()),
            alpha_mode: AlphaMode::Blend,
            unlit: true,
            cull_mode: None,
            ..default()
        });

        let steam_image = images.add(build_steam_image());
        let steam_mat = materials.add(StandardMaterial {
            base_color: Color::srgba(1.0, 1.0, 1.0, 0.45),
            base_color_texture: Some(steam_image.clone()),
            alpha_mode: AlphaMode::Blend,
            unlit: true,
            cull_mode: None,
            ..default()
        });

        let owned_images = vec![shadow_image.clone(), steam_image.clone()];
        let owned_materials = vec![shadow_mat.clone(), steam_mat.clone()];

        Self {
            hair_pixar,
            hair_toy,
            spiky_cluster_pixar,
            spiky_cluster_toy,
            ponytail_pixar,
            ponytail_toy,
            ahoge_pixar,
            ahoge_toy,
            skull_pixar,
            skull_toy,
            torso_pixar,
            torso_toy,
            coat_pixar,
            coat_toy,
            suit_v_pixar,
            suit_v_toy,
            sphere,
            box_mesh,
            cyl,
            cyl_open,
            capsule,
            cone,
            plane,
            hemi_dome,
            half_brim,
            tri_prism,
            cake_wedge,
            torus_thin,
            torus_med,
            torus_thick,
            arc_pi_thin,
            arc_pi_med,
            arc_pi_thick,
            arc_brow,
            arc_crease,
            arc_collar,
            shadow_quad,
            shadow_image,
            shadow_mat,
            steam_image,
            steam_mat,
            owned_meshes,
            owned_images,
            owned_materials,
        }
    }

    pub fn hair_shell_handle(&self, look: ChibiLook, style: HairStyle) -> Handle<Mesh> {
        match look {
            ChibiLook::Pixar => self.hair_pixar[style.idx()].clone(),
            ChibiLook::Toy => self.hair_toy[style.idx()].clone(),
        }
    }

    pub fn skull_handle(&self, look: ChibiLook) -> Handle<Mesh> {
        match look {
            ChibiLook::Pixar => self.skull_pixar.clone(),
            ChibiLook::Toy => self.skull_toy.clone(),
        }
    }

    pub fn torso_handle(&self, look: ChibiLook) -> Handle<Mesh> {
        match look {
            ChibiLook::Pixar => self.torso_pixar.clone(),
            ChibiLook::Toy => self.torso_toy.clone(),
        }
    }

    pub fn coat_handle(&self, look: ChibiLook) -> Handle<Mesh> {
        match look {
            ChibiLook::Pixar => self.coat_pixar.clone(),
            ChibiLook::Toy => self.coat_toy.clone(),
        }
    }

    pub fn suit_v_handle(&self, look: ChibiLook) -> Handle<Mesh> {
        match look {
            ChibiLook::Pixar => self.suit_v_pixar.clone(),
            ChibiLook::Toy => self.suit_v_toy.clone(),
        }
    }

    pub fn spiky_cluster_handle(&self, look: ChibiLook) -> Handle<Mesh> {
        match look {
            ChibiLook::Pixar => self.spiky_cluster_pixar.clone(),
            ChibiLook::Toy => self.spiky_cluster_toy.clone(),
        }
    }

    pub fn ponytail_handle(&self, look: ChibiLook) -> Handle<Mesh> {
        match look {
            ChibiLook::Pixar => self.ponytail_pixar.clone(),
            ChibiLook::Toy => self.ponytail_toy.clone(),
        }
    }

    pub fn ahoge_handle(&self, look: ChibiLook) -> Handle<Mesh> {
        match look {
            ChibiLook::Pixar => self.ahoge_pixar.clone(),
            ChibiLook::Toy => self.ahoge_toy.clone(),
        }
    }

    pub fn total_geometry_bytes(&self, meshes: &Assets<Mesh>) -> usize {
        self.owned_meshes
            .iter()
            .filter_map(|h| meshes.get(h))
            .map(mesh_byte_size)
            .sum()
    }

    pub fn owned_mesh_count(&self) -> usize {
        self.owned_meshes.len()
    }

    /// Removes 100% of meshes, materials, and images owned by `ChibiMeshCache`.
    pub fn purge(
        &self,
        meshes: &mut Assets<Mesh>,
        materials: &mut Assets<StandardMaterial>,
        images: &mut Assets<Image>,
    ) {
        for h in &self.owned_meshes {
            meshes.remove(h);
        }
        for h in &self.owned_materials {
            materials.remove(h);
        }
        for h in &self.owned_images {
            images.remove(h);
        }
    }
}

#[inline]
fn smoothstep(x: f32) -> f32 {
    x * x * (3.0 - 2.0 * x)
}

/// Builds a sculpted indexed hair shell matching `chibi-rig.js::hairShell` with seamless
/// azimuth wrap and smooth averaged unit vertex normals.
pub fn hair_shell(r: f32, line: [f32; 3], opts: HairShellOpts, ni: usize, nj: usize) -> Mesh {
    let ni = ni.max(8);
    let nj = nj.max(4);
    let mut positions = Vec::with_capacity((nj + 1) * ni);
    let mut uvs = Vec::with_capacity((nj + 1) * ni);

    for j in 0..=nj {
        let u = j as f32 / nj as f32;
        for i in 0..ni {
            let ph = -PI + TAU * (i as f32 / ni as f32);
            let a = ph.abs() / PI;
            let mut edge = if a < 0.5 {
                line[0] + (line[1] - line[0]) * smoothstep(a / 0.5)
            } else {
                line[1] + (line[2] - line[1]) * smoothstep((a - 0.5) / 0.5)
            };
            edge += opts.sweep * ph.sin() * (1.0 - a * 2.0).max(0.0);
            let f = (((ph + opts.twist * u) / TAU) * opts.locks + opts.phase).rem_euclid(1.0);
            let lock = 1.0 - (2.0 * f - 1.0).abs();
            let th = u * (edge + opts.tip * (lock.powf(opts.round) - 0.5));
            let mut rad = r
                * (1.0 + opts.bulge * (PI * lock).sin() * (u * 2.2).min(1.0))
                * (1.0 - opts.tuck * smoothstep(((u - 0.72) / 0.28).max(0.0)));
            if opts.bumps > 0.0 {
                rad *= 1.0
                    + opts.bumps
                        * (ph * 9.0 + u * 11.0).sin()
                        * (u * 17.0 + ph * 3.0).sin()
                        * (u * 4.0).min(1.0);
            }
            let st = th.sin();
            positions.push([rad * st * ph.sin(), rad * th.cos(), rad * st * ph.cos()]);
            uvs.push([i as f32 / ni as f32, u]);
        }
    }

    let mut indices = Vec::with_capacity(nj * ni * 6);
    for j in 0..nj {
        let row = j * ni;
        let next_row = (j + 1) * ni;
        for i in 0..ni {
            let i_next = (i + 1) % ni;
            let k = (row + i) as u16;
            let k1 = (row + i_next) as u16;
            let n = (next_row + i) as u16;
            let n1 = (next_row + i_next) as u16;
            indices.extend_from_slice(&[k, n, k1, n, n1, k1]);
        }
    }

    let mut normals = compute_smooth_normals(&positions, &indices);
    // Average the crown pole ring (j = 0) so every crown vertex shares one smooth unit normal.
    let mut crown_sum = Vec3::ZERO;
    for n in normals.iter().take(ni) {
        crown_sum += Vec3::from_array(*n);
    }
    let crown_n = crown_sum.normalize_or_zero();
    let crown_arr = if crown_n.length_squared() > 0.5 {
        crown_n.to_array()
    } else {
        [0.0, 1.0, 0.0]
    };
    for n in normals.iter_mut().take(ni) {
        *n = crown_arr;
    }

    build_mesh_from_buffers(positions, normals, uvs, indices)
}

/// Evaluates a Catmull-Rom spline through `points` at normalized parameter `t in [0, 1]`.
fn eval_catmull_rom(points: &[Vec3], t: f32) -> Vec3 {
    let n = points.len();
    if n == 0 {
        return Vec3::ZERO;
    }
    if n == 1 {
        return points[0];
    }
    let t = t.clamp(0.0, 1.0);
    let segs = (n - 1) as f32;
    let scaled = t * segs;
    let idx = (scaled.floor() as usize).min(n - 2);
    let u = scaled - idx as f32;

    let p1 = points[idx];
    let p2 = points[idx + 1];
    let p0 = if idx > 0 {
        points[idx - 1]
    } else {
        p1 * 2.0 - p2
    };
    let p3 = if idx + 2 < n {
        points[idx + 2]
    } else {
        p2 * 2.0 - p1
    };

    let u2 = u * u;
    let u3 = u2 * u;
    0.5 * ((2.0 * p1)
        + (-p0 + p2) * u
        + (2.0 * p0 - 5.0 * p1 + 4.0 * p2 - p3) * u2
        + (-p0 + 3.0 * p1 - 3.0 * p2 + p3) * u3)
}

/// Sweeps a circular tube with varying `radius_fn(t)` along a Catmull-Rom curve,
/// matching `chibi-rig.js::sweep` with smooth rotation-minimizing (Bishop) frames.
pub fn sweep(
    points: &[Vec3],
    radius_fn: impl Fn(f32) -> f32,
    segs: usize,
    radial: usize,
) -> (Vec<[f32; 3]>, Vec<[f32; 2]>, Vec<u16>) {
    let segs = segs.max(2);
    let radial = radial.max(5);

    let mut centers = Vec::with_capacity(segs + 1);
    let mut tangents = Vec::with_capacity(segs + 1);
    for i in 0..=segs {
        let t = i as f32 / segs as f32;
        centers.push(eval_catmull_rom(points, t));
    }
    for i in 0..=segs {
        let dir = if i == 0 {
            centers[1] - centers[0]
        } else if i == segs {
            centers[segs] - centers[segs - 1]
        } else {
            centers[i + 1] - centers[i - 1]
        };
        tangents.push(dir.normalize_or(Vec3::Y));
    }

    // Parallel transport frames (equivalent to Three.js computeFrenetFrames)
    let mut normals_f = Vec::with_capacity(segs + 1);
    let mut binormals_f = Vec::with_capacity(segs + 1);

    let t0 = tangents[0];
    let ax = if t0.x.abs() <= t0.y.abs() && t0.x.abs() <= t0.z.abs() {
        Vec3::X
    } else if t0.y.abs() <= t0.z.abs() {
        Vec3::Y
    } else {
        Vec3::Z
    };
    let n0 = t0.cross(ax).normalize_or(Vec3::X);
    let b0 = t0.cross(n0).normalize_or(Vec3::Z);
    normals_f.push(n0);
    binormals_f.push(b0);

    for i in 1..=segs {
        let q = Quat::from_rotation_arc(tangents[i - 1], tangents[i]);
        let mut ni_vec = q * normals_f[i - 1];
        ni_vec = (ni_vec - tangents[i] * ni_vec.dot(tangents[i])).normalize_or(n0);
        let bi_vec = tangents[i].cross(ni_vec).normalize_or(b0);
        normals_f.push(ni_vec);
        binormals_f.push(bi_vec);
    }

    let mut positions = Vec::with_capacity((segs + 1) * radial);
    let mut uvs = Vec::with_capacity((segs + 1) * radial);
    for i in 0..=segs {
        let t = i as f32 / segs as f32;
        let c = centers[i];
        let rad = radius_fn(t).max(0.0005);
        let n_vec = normals_f[i];
        let b_vec = binormals_f[i];
        for j in 0..radial {
            let a = (j as f32 / radial as f32) * TAU;
            let p = c + n_vec * (a.cos() * rad) + b_vec * (a.sin() * rad);
            positions.push(p.to_array());
            uvs.push([j as f32 / radial as f32, t]);
        }
    }

    let mut indices = Vec::with_capacity(segs * radial * 6);
    for i in 0..segs {
        let row = i * radial;
        let next_row = (i + 1) * radial;
        for j in 0..radial {
            let j_next = (j + 1) % radial;
            let k = (row + j) as u16;
            let k1 = (row + j_next) as u16;
            let n = (next_row + j) as u16;
            let n1 = (next_row + j_next) as u16;
            // Outward winding for (N, B, T) right-handed frame: (+B) x (+T) = +N
            indices.extend_from_slice(&[k, k1, n, n, k1, n1]);
        }
    }

    (positions, uvs, indices)
}

pub fn sweep_mesh(
    points: &[Vec3],
    radius_fn: impl Fn(f32) -> f32,
    segs: usize,
    radial: usize,
) -> Mesh {
    let (positions, uvs, indices) = sweep(points, radius_fn, segs, radial);
    let normals = compute_smooth_normals(&positions, &indices);
    build_mesh_from_buffers(positions, normals, uvs, indices)
}

fn build_spiky_cluster(r: f32, toy: bool) -> Mesh {
    let mut positions = Vec::new();
    let mut uvs = Vec::new();
    let mut indices = Vec::new();

    for i in 0..6 {
        let a = (i as f32 - 2.5) * 0.42;
        let sx = a.sin();
        let pts = [
            Vec3::new(sx * r * 0.50, r * 0.75, r * 0.10),
            Vec3::new(sx * r * 0.66, r * 1.02, -r * 0.10),
            Vec3::new(sx * r * 0.82, r * 1.15, -r * 0.42),
        ];
        let base = positions.len() as u16;
        let (p, u, idx) = sweep(
            &pts,
            |t| r * (if toy { 0.30 } else { 0.25 }) * (1.0 - t).max(0.0).powf(0.8),
            10,
            8,
        );
        positions.extend(p);
        uvs.extend(u);
        indices.extend(idx.into_iter().map(|v| v + base));
    }

    let normals = compute_smooth_normals(&positions, &indices);
    build_mesh_from_buffers(positions, normals, uvs, indices)
}

fn build_ponytail_sweep(r: f32) -> Mesh {
    let pts = [
        Vec3::new(0.0, r * 0.18, -r * 0.90),
        Vec3::new(0.0, r * 0.05, -r * 1.25),
        Vec3::new(r * 0.02, -r * 0.55, -r * 1.38),
        Vec3::new(r * 0.08, -r * 1.20, -r * 1.18),
        Vec3::new(r * 0.15, -r * 1.62, -r * 0.90),
    ];
    sweep_mesh(
        &pts,
        |t| r * 0.28 * (PI * (0.28 + 0.72 * t)).sin() + 0.004,
        20,
        12,
    )
}

fn build_ahoge_sweep(r: f32) -> Mesh {
    let pts = [
        Vec3::new(r * 0.05, r * 1.00, r * 0.05),
        Vec3::new(r * 0.12, r * 1.28, r * 0.20),
        Vec3::new(0.0, r * 1.42, r * 0.40),
        Vec3::new(-r * 0.15, r * 1.35, r * 0.50),
    ];
    sweep_mesh(&pts, |t| r * 0.06 * (1.0 - t) + 0.002, 12, 6)
}

/// Builds an indexed surface of revolution around the Y axis matching Three.js `LatheGeometry`.
pub fn lathe_profile(profile: &[[f32; 2]], segs: usize, phi_start: f32, phi_len: f32) -> Mesh {
    let segs = segs.max(6);
    let closed = phi_len >= TAU - 1e-4;
    let cols = if closed { segs } else { segs + 1 };
    let rows = profile.len();

    let mut positions = Vec::with_capacity(cols * rows);
    let mut uvs = Vec::with_capacity(cols * rows);

    for i in 0..cols {
        let u = i as f32 / segs as f32;
        let phi = phi_start + u * phi_len;
        let sp = phi.sin();
        let cp = phi.cos();
        for (j, &[rad, y]) in profile.iter().enumerate() {
            positions.push([rad * sp, y, rad * cp]);
            uvs.push([u, j as f32 / (rows.saturating_sub(1).max(1)) as f32]);
        }
    }

    let mut indices = Vec::with_capacity(segs * (rows.saturating_sub(1)) * 6);
    for i in 0..segs {
        let i_next = if closed { (i + 1) % cols } else { i + 1 };
        for j in 0..rows.saturating_sub(1) {
            let k = (i * rows + j) as u16;
            let k_up = (i * rows + j + 1) as u16;
            let n = (i_next * rows + j) as u16;
            let n_up = (i_next * rows + j + 1) as u16;
            indices.extend_from_slice(&[k, n, k_up, n, n_up, k_up]);
        }
    }

    let normals = compute_smooth_normals(&positions, &indices);
    build_mesh_from_buffers(positions, normals, uvs, indices)
}

fn build_coat_lathe(torso_profile: &[[f32; 2]], torso_top: f32) -> Mesh {
    let mut prof = Vec::with_capacity(torso_profile.len());
    prof.push([torso_profile[1][0] * 1.14 + 0.006, -0.17]);
    for &[x, y] in &torso_profile[1..] {
        if y <= torso_top * 0.94 {
            prof.push([x * 1.09 + 0.006, y]);
        }
    }
    let gap = 0.36;
    lathe_profile(&prof, 24, gap, TAU - gap * 2.0)
}

fn build_suit_v_mesh(tr: f32, h: f32) -> Mesh {
    let positions = vec![[-tr * 0.36, 0.0, 0.0], [tr * 0.36, 0.0, 0.0], [0.0, -h * 0.46, 0.0]];
    let normals = vec![[0.0, 0.0, 1.0]; 3];
    let uvs = vec![[0.0, 1.0], [1.0, 1.0], [0.5, 0.0]];
    let indices = vec![0u16, 2, 1];
    build_mesh_from_buffers(positions, normals, uvs, indices)
}

/// Builds the sculpted chibi skull matching `chibi-rig.js` cheek/chin/front deformation.
fn build_sculpted_skull(look: ChibiLook, w_segs: usize, h_segs: usize) -> Mesh {
    let d = ChibiDims::for_look(look);
    let r = d.r;
    let mut positions = Vec::with_capacity((h_segs + 1) * w_segs);
    let mut uvs = Vec::with_capacity((h_segs + 1) * w_segs);

    for j in 0..=h_segs {
        let v = j as f32 / h_segs as f32;
        let theta = v * PI;
        let st = theta.sin();
        let ct = theta.cos();
        for i in 0..w_segs {
            let u = i as f32 / w_segs as f32;
            let phi = u * TAU;
            let x0 = r * st * phi.sin();
            let y0 = r * ct;
            let z0 = r * st * phi.cos();
            let t = y0 / r;
            let sx = d.sx * d.shape_x(t);
            let sz = d.sz * if z0 > 0.0 { d.shape_z(t) } else { 1.0 };
            positions.push([x0 * sx, y0 * d.sy, z0 * sz]);
            uvs.push([u, v]);
        }
    }

    let mut indices = Vec::with_capacity(h_segs * w_segs * 6);
    for j in 0..h_segs {
        let row = j * w_segs;
        let next_row = (j + 1) * w_segs;
        for i in 0..w_segs {
            let i_next = (i + 1) % w_segs;
            let k = (row + i) as u16;
            let k1 = (row + i_next) as u16;
            let n = (next_row + i) as u16;
            let n1 = (next_row + i_next) as u16;
            indices.extend_from_slice(&[k, n, k1, n, n1, k1]);
        }
    }

    let mut normals = compute_smooth_normals(&positions, &indices);
    // Smooth top and bottom poles
    for n in normals.iter_mut().take(w_segs) {
        *n = [0.0, 1.0, 0.0];
    }
    let bot_start = h_segs * w_segs;
    for n in &mut normals[bot_start..bot_start + w_segs] {
        *n = [0.0, -1.0, 0.0];
    }
    build_mesh_from_buffers(positions, normals, uvs, indices)
}

fn build_uv_sphere(r: f32, w_segs: usize, h_segs: usize) -> Mesh {
    let mut positions = Vec::with_capacity((h_segs + 1) * w_segs);
    let mut uvs = Vec::with_capacity((h_segs + 1) * w_segs);
    for j in 0..=h_segs {
        let v = j as f32 / h_segs as f32;
        let theta = v * PI;
        let st = theta.sin();
        let ct = theta.cos();
        for i in 0..w_segs {
            let u = i as f32 / w_segs as f32;
            let phi = u * TAU;
            positions.push([r * st * phi.sin(), r * ct, r * st * phi.cos()]);
            uvs.push([u, v]);
        }
    }
    let mut indices = Vec::with_capacity(h_segs * w_segs * 6);
    for j in 0..h_segs {
        let row = j * w_segs;
        let next_row = (j + 1) * w_segs;
        for i in 0..w_segs {
            let i_next = (i + 1) % w_segs;
            let k = (row + i) as u16;
            let k1 = (row + i_next) as u16;
            let n = (next_row + i) as u16;
            let n1 = (next_row + i_next) as u16;
            indices.extend_from_slice(&[k, n, k1, n, n1, k1]);
        }
    }
    let normals: Vec<[f32; 3]> = positions
        .iter()
        .map(|p| Vec3::from_array(*p).normalize_or(Vec3::Y).to_array())
        .collect();
    build_mesh_from_buffers(positions, normals, uvs, indices)
}

fn build_hemisphere(r: f32, w_segs: usize, h_segs: usize) -> Mesh {
    let mut positions = Vec::with_capacity((h_segs + 1) * w_segs);
    let mut uvs = Vec::with_capacity((h_segs + 1) * w_segs);
    for j in 0..=h_segs {
        let v = j as f32 / h_segs as f32;
        let theta = v * FRAC_PI_2;
        let st = theta.sin();
        let ct = theta.cos();
        for i in 0..w_segs {
            let u = i as f32 / w_segs as f32;
            let phi = u * TAU;
            positions.push([r * st * phi.sin(), r * ct, r * st * phi.cos()]);
            uvs.push([u, v]);
        }
    }
    let mut indices = Vec::with_capacity(h_segs * w_segs * 6);
    for j in 0..h_segs {
        let row = j * w_segs;
        let next_row = (j + 1) * w_segs;
        for i in 0..w_segs {
            let i_next = (i + 1) % w_segs;
            let k = (row + i) as u16;
            let k1 = (row + i_next) as u16;
            let n = (next_row + i) as u16;
            let n1 = (next_row + i_next) as u16;
            indices.extend_from_slice(&[k, n, k1, n, n1, k1]);
        }
    }
    let normals: Vec<[f32; 3]> = positions
        .iter()
        .map(|p| Vec3::from_array(*p).normalize_or(Vec3::Y).to_array())
        .collect();
    build_mesh_from_buffers(positions, normals, uvs, indices)
}

fn build_cylinder(
    r_top: f32,
    r_bot: f32,
    height: f32,
    segs: usize,
    open_ended: bool,
    theta_start: f32,
    theta_len: f32,
) -> Mesh {
    let half_h = height * 0.5;
    let closed = theta_len >= TAU - 1e-4;
    let cols = if closed { segs } else { segs + 1 };

    let mut positions = Vec::new();
    let mut uvs = Vec::new();
    let mut indices = Vec::new();

    // Side band
    for i in 0..cols {
        let u = i as f32 / segs as f32;
        let th = theta_start + u * theta_len;
        let st = th.sin();
        let ct = th.cos();
        positions.push([r_bot * st, -half_h, r_bot * ct]);
        uvs.push([u, 0.0]);
        positions.push([r_top * st, half_h, r_top * ct]);
        uvs.push([u, 1.0]);
    }
    for i in 0..segs {
        let i_next = if closed { (i + 1) % cols } else { i + 1 };
        let b0 = (i * 2) as u16;
        let t0 = b0 + 1;
        let b1 = (i_next * 2) as u16;
        let t1 = b1 + 1;
        indices.extend_from_slice(&[b0, b1, t0, b1, t1, t0]);
    }

    if !open_ended {
        // Top cap
        let top_center = positions.len() as u16;
        positions.push([0.0, half_h, 0.0]);
        uvs.push([0.5, 0.5]);
        let top_ring = positions.len() as u16;
        for i in 0..cols {
            let u = i as f32 / segs as f32;
            let th = theta_start + u * theta_len;
            positions.push([r_top * th.sin(), half_h, r_top * th.cos()]);
            uvs.push([0.5 + 0.5 * th.sin(), 0.5 + 0.5 * th.cos()]);
        }
        for i in 0..segs {
            let i_next = if closed { (i + 1) % cols } else { i + 1 };
            indices.extend_from_slice(&[top_center, top_ring + i as u16, top_ring + i_next as u16]);
        }

        // Bottom cap
        let bot_center = positions.len() as u16;
        positions.push([0.0, -half_h, 0.0]);
        uvs.push([0.5, 0.5]);
        let bot_ring = positions.len() as u16;
        for i in 0..cols {
            let u = i as f32 / segs as f32;
            let th = theta_start + u * theta_len;
            positions.push([r_bot * th.sin(), -half_h, r_bot * th.cos()]);
            uvs.push([0.5 + 0.5 * th.sin(), 0.5 + 0.5 * th.cos()]);
        }
        for i in 0..segs {
            let i_next = if closed { (i + 1) % cols } else { i + 1 };
            indices.extend_from_slice(&[bot_center, bot_ring + i_next as u16, bot_ring + i as u16]);
        }
    }

    let normals = compute_smooth_normals(&positions, &indices);
    build_mesh_from_buffers(positions, normals, uvs, indices)
}

/// Builds a torus or partial torus arc in the XY plane matching Three.js `TorusGeometry`.
pub fn torus_arc(
    major_r: f32,
    minor_r: f32,
    radial_segs: usize,
    tubular_segs: usize,
    arc: f32,
) -> Mesh {
    let closed_u = arc >= TAU - 1e-4;
    let u_cols = if closed_u {
        tubular_segs
    } else {
        tubular_segs + 1
    };
    let v_rows = radial_segs;

    let mut positions = Vec::with_capacity(u_cols * v_rows);
    let mut normals = Vec::with_capacity(u_cols * v_rows);
    let mut uvs = Vec::with_capacity(u_cols * v_rows);

    for i in 0..u_cols {
        let u_frac = i as f32 / tubular_segs as f32;
        let u = u_frac * arc;
        let cu = u.cos();
        let su = u.sin();
        for j in 0..v_rows {
            let v_frac = j as f32 / radial_segs as f32;
            let v = v_frac * TAU;
            let cv = v.cos();
            let sv = v.sin();
            let ring = major_r + minor_r * cv;
            positions.push([ring * cu, ring * su, minor_r * sv]);
            normals.push([cv * cu, cv * su, sv]);
            uvs.push([u_frac, v_frac]);
        }
    }

    let mut indices = Vec::with_capacity(tubular_segs * radial_segs * 6);
    for i in 0..tubular_segs {
        let i_next = if closed_u { (i + 1) % u_cols } else { i + 1 };
        for j in 0..radial_segs {
            let j_next = (j + 1) % radial_segs;
            let k = (i * v_rows + j) as u16;
            let k_v = (i * v_rows + j_next) as u16;
            let n = (i_next * v_rows + j) as u16;
            let n_v = (i_next * v_rows + j_next) as u16;
            indices.extend_from_slice(&[k, n, k_v, n, n_v, k_v]);
        }
    }

    build_mesh_from_buffers(positions, normals, uvs, indices)
}

fn compute_smooth_normals(positions: &[[f32; 3]], indices: &[u16]) -> Vec<[f32; 3]> {
    let mut acc = vec![Vec3::ZERO; positions.len()];
    for tri in indices.chunks_exact(3) {
        let i0 = tri[0] as usize;
        let i1 = tri[1] as usize;
        let i2 = tri[2] as usize;
        let p0 = Vec3::from_array(positions[i0]);
        let p1 = Vec3::from_array(positions[i1]);
        let p2 = Vec3::from_array(positions[i2]);
        let n = (p1 - p0).cross(p2 - p0);
        if n.length_squared() > 1e-14 {
            acc[i0] += n;
            acc[i1] += n;
            acc[i2] += n;
        }
    }
    acc.into_iter()
        .enumerate()
        .map(|(idx, n)| {
            let unit = n.normalize_or_zero();
            if unit.length_squared() > 0.5 {
                unit.to_array()
            } else {
                Vec3::from_array(positions[idx])
                    .normalize_or(Vec3::Y)
                    .to_array()
            }
        })
        .collect()
}

fn build_mesh_from_buffers(
    positions: Vec<[f32; 3]>,
    normals: Vec<[f32; 3]>,
    uvs: Vec<[f32; 2]>,
    indices: Vec<u16>,
) -> Mesh {
    let mut mesh = Mesh::new(
        PrimitiveTopology::TriangleList,
        RenderAssetUsages::default(),
    );
    mesh.insert_attribute(Mesh::ATTRIBUTE_POSITION, positions);
    mesh.insert_attribute(Mesh::ATTRIBUTE_NORMAL, normals);
    mesh.insert_attribute(Mesh::ATTRIBUTE_UV_0, uvs);
    mesh.insert_indices(Indices::U16(indices));
    mesh
}

fn build_radial_shadow_image() -> Image {
    let size = 64u32;
    let mut rgba = vec![0u8; (size * size * 4) as usize];
    for y in 0..size {
        for x in 0..size {
            let dx = x as f32 + 0.5 - 32.0;
            let dy = y as f32 + 0.5 - 32.0;
            let d = (dx * dx + dy * dy).sqrt() / 31.0;
            let a = if d >= 1.0 {
                0.0
            } else if d < 0.6 {
                0.42 + (0.18 - 0.42) * (d / 0.6)
            } else {
                0.18 * (1.0 - (d - 0.6) / 0.4)
            };
            let idx = ((y * size + x) * 4) as usize;
            rgba[idx] = 0;
            rgba[idx + 1] = 0;
            rgba[idx + 2] = 0;
            rgba[idx + 3] = (a.clamp(0.0, 1.0) * 255.0).round() as u8;
        }
    }
    Image::new(
        Extent3d {
            width: size,
            height: size,
            depth_or_array_layers: 1,
        },
        TextureDimension::D2,
        rgba,
        TextureFormat::Rgba8UnormSrgb,
        RenderAssetUsages::RENDER_WORLD,
    )
}

fn build_steam_image() -> Image {
    let size = 32u32;
    let mut rgba = vec![0u8; (size * size * 4) as usize];
    for y in 0..size {
        for x in 0..size {
            let dx = x as f32 + 0.5 - 16.0;
            let dy = y as f32 + 0.5 - 16.0;
            let d = ((dx * dx + dy * dy).sqrt() / 16.0).clamp(0.0, 1.0);
            let a = 0.9 * (1.0 - d);
            let idx = ((y * size + x) * 4) as usize;
            rgba[idx] = 255;
            rgba[idx + 1] = 255;
            rgba[idx + 2] = 255;
            rgba[idx + 3] = (a * 255.0).round() as u8;
        }
    }
    Image::new(
        Extent3d {
            width: size,
            height: size,
            depth_or_array_layers: 1,
        },
        TextureDimension::D2,
        rgba,
        TextureFormat::Rgba8UnormSrgb,
        RenderAssetUsages::RENDER_WORLD,
    )
}

/// Computes total vertex attribute + index buffer bytes for a Bevy `Mesh`.
pub fn mesh_byte_size(mesh: &Mesh) -> usize {
    let mut bytes = 0usize;
    for (_, vals) in mesh.attributes() {
        bytes += match vals {
            VertexAttributeValues::Float32(v) => v.len() * 4,
            VertexAttributeValues::Float32x2(v) => v.len() * 8,
            VertexAttributeValues::Float32x3(v) => v.len() * 12,
            VertexAttributeValues::Float32x4(v) => v.len() * 16,
            VertexAttributeValues::Sint32(v) => v.len() * 4,
            VertexAttributeValues::Uint32(v) => v.len() * 4,
            VertexAttributeValues::Sint32x2(v) => v.len() * 8,
            VertexAttributeValues::Uint32x2(v) => v.len() * 8,
            VertexAttributeValues::Sint32x3(v) => v.len() * 12,
            VertexAttributeValues::Uint32x3(v) => v.len() * 12,
            VertexAttributeValues::Sint32x4(v) => v.len() * 16,
            VertexAttributeValues::Uint32x4(v) => v.len() * 16,
            VertexAttributeValues::Sint16x2(v) => v.len() * 4,
            VertexAttributeValues::Snorm16x2(v) => v.len() * 4,
            VertexAttributeValues::Uint16x2(v) => v.len() * 4,
            VertexAttributeValues::Unorm16x2(v) => v.len() * 4,
            VertexAttributeValues::Sint16x4(v) => v.len() * 8,
            VertexAttributeValues::Snorm16x4(v) => v.len() * 8,
            VertexAttributeValues::Uint16x4(v) => v.len() * 8,
            VertexAttributeValues::Unorm16x4(v) => v.len() * 8,
            VertexAttributeValues::Sint8x2(v) => v.len() * 2,
            VertexAttributeValues::Snorm8x2(v) => v.len() * 2,
            VertexAttributeValues::Uint8x2(v) => v.len() * 2,
            VertexAttributeValues::Unorm8x2(v) => v.len() * 2,
            VertexAttributeValues::Sint8x4(v) => v.len() * 4,
            VertexAttributeValues::Snorm8x4(v) => v.len() * 4,
            VertexAttributeValues::Uint8x4(v) => v.len() * 4,
            VertexAttributeValues::Unorm8x4(v) => v.len() * 4,
        };
    }
    if let Some(indices) = mesh.indices() {
        bytes += match indices {
            Indices::U16(v) => v.len() * 2,
            Indices::U32(v) => v.len() * 4,
        };
    }
    bytes
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn chibi_all_six_hair_shells_are_indexed_with_smooth_unit_normals_and_cache_under_1_5_mb() {
        let mut meshes = Assets::<Mesh>::default();
        let mut materials = Assets::<StandardMaterial>::default();
        let mut images = Assets::<Image>::default();

        let cache = ChibiMeshCache::build(&mut meshes, &mut materials, &mut images);
        assert!(
            cache.owned_mesh_count() >= 20,
            "ChibiMeshCache must precompute hair shells, skulls, lathes, limbs, outfits, and prop meshes"
        );

        for look in [ChibiLook::Pixar, ChibiLook::Toy] {
            for style in HairStyle::ALL {
                let handle = cache.hair_shell_handle(look, style);
                let mesh = meshes
                    .get(&handle)
                    .expect("hair shell mesh must exist in Assets<Mesh>");
                assert_eq!(mesh.primitive_topology(), PrimitiveTopology::TriangleList);

                let indices = mesh
                    .indices()
                    .expect("hair shell mesh must use indexed triangles");
                assert!(indices.len() >= 48 * 16 * 6, "expected 48x16 indexed grid");
                assert_eq!(indices.len() % 3, 0, "indices must form complete triangles");

                let Some(VertexAttributeValues::Float32x3(positions)) =
                    mesh.attribute(Mesh::ATTRIBUTE_POSITION)
                else {
                    panic!("missing ATTRIBUTE_POSITION on hair shell {style:?}");
                };
                let Some(VertexAttributeValues::Float32x3(normals)) =
                    mesh.attribute(Mesh::ATTRIBUTE_NORMAL)
                else {
                    panic!("missing ATTRIBUTE_NORMAL on hair shell {style:?}");
                };
                assert_eq!(positions.len(), normals.len());
                for (idx, p) in positions.iter().enumerate() {
                    assert!(
                        p[0].is_finite() && p[1].is_finite() && p[2].is_finite(),
                        "non-finite position at {idx} in {style:?}"
                    );
                }
                for (idx, n) in normals.iter().enumerate() {
                    let len = Vec3::from_array(*n).length();
                    assert!(
                        len.is_finite() && (len - 1.0).abs() < 1e-3,
                        "non-unit normal len={len} at {idx} in {style:?}"
                    );
                }
            }
        }

        let total_bytes = cache.total_geometry_bytes(&meshes);
        println!(
            "ChibiMeshCache total_geometry_bytes = {} bytes ({:.1} KB) across {} indexed meshes",
            total_bytes,
            total_bytes as f64 / 1024.0,
            cache.owned_mesh_count()
        );
        assert!(
            total_bytes > 50_000,
            "total_geometry_bytes unexpectedly tiny ({total_bytes}); cache not populated"
        );
        assert!(
            total_bytes < 1_500_000,
            "ChibiMeshCache geometry budget exceeded: {total_bytes} bytes >= 1_500_000"
        );
    }

    #[test]
    fn chibi_mesh_cache_purge_releases_100_percent_of_assets() {
        let mut meshes = Assets::<Mesh>::default();
        let mut materials = Assets::<StandardMaterial>::default();
        let mut images = Assets::<Image>::default();

        let cache = ChibiMeshCache::build(&mut meshes, &mut materials, &mut images);
        assert!(meshes.len() >= 20);
        assert!(images.len() >= 1, "radial drop shadow image must be uploaded");
        assert!(
            materials.len() >= 1,
            "radial drop shadow material must be created"
        );

        cache.purge(&mut meshes, &mut materials, &mut images);
        assert_eq!(meshes.len(), 0, "ChibiMeshCache::purge leaked meshes");
        assert_eq!(materials.len(), 0, "ChibiMeshCache::purge leaked materials");
        assert_eq!(images.len(), 0, "ChibiMeshCache::purge leaked images");
    }
}
