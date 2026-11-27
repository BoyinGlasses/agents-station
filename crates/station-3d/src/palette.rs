use bevy::asset::RenderAssetUsages;
use bevy::image::{ImageAddressMode, ImageFilterMode, ImageSampler, ImageSamplerDescriptor};
use bevy::math::Affine2;
use bevy::prelude::*;
use bevy::render::mesh::{Indices, PrimitiveTopology};
use bevy::render::render_resource::{Extent3d, TextureDimension, TextureFormat};
use station_canvas2d::RgbaSurface;
use std::collections::HashMap;
use std::f32::consts::FRAC_PI_2;

/// Marker for every 3D world entity so layout changes, 2D mode, and Tray minimize
/// can despawn and release the entire 3D scene cleanly.
#[derive(Component, Debug, Clone, Copy)]
pub struct WorldEntity;

/// Shared unit-primitive mesh handles & deduplicated PBR material cache.
/// Keeps 44 desks, walls, furniture, and 22 props down to ~6 GPU meshes and ~60 materials.
#[derive(Resource)]
pub struct WorldPalette {
    pub unit_box: Handle<Mesh>,
    pub unit_cyl: Handle<Mesh>,
    pub unit_sphere: Handle<Mesh>,
    pub unit_plane: Handle<Mesh>,
    pub unit_cone: Handle<Mesh>,
    pub unit_torus: Handle<Mesh>,
    pub thin_ring: Handle<Mesh>,
    materials: HashMap<MaterialKey, Handle<StandardMaterial>>,
    owned_materials: Vec<Handle<StandardMaterial>>,
    owned_images: Vec<Handle<Image>>,
    floor_tile_vram_bytes: usize,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
struct MaterialKey {
    r: u8,
    g: u8,
    b: u8,
    a: u8,
    rough_q: u8,
    metal_q: u8,
    emissive: bool,
    unlit: bool,
    double_sided: bool,
}

pub fn parse_hex_rgb(hex: &str) -> (u8, u8, u8) {
    let s = hex.trim().trim_start_matches('#');
    if s.len() == 6
        && let (Ok(r), Ok(g), Ok(b)) = (
            u8::from_str_radix(&s[0..2], 16),
            u8::from_str_radix(&s[2..4], 16),
            u8::from_str_radix(&s[4..6], 16),
        )
    {
        return (r, g, b);
    }
    (220, 220, 220)
}

pub fn srgb_hex(hex: &str) -> Color {
    let (r, g, b) = parse_hex_rgb(hex);
    Color::srgb_u8(r, g, b)
}

/// Uploads a `station-canvas2d` RGBA8 surface directly to GPU with `RenderAssetUsages::RENDER_WORLD`,
/// dropping the CPU buffer immediately after upload for minimum RAM usage.
pub fn upload_gpu_image(images: &mut Assets<Image>, surface: RgbaSurface) -> Handle<Image> {
    let img = Image::new(
        Extent3d {
            width: surface.width.max(1),
            height: surface.height.max(1),
            depth_or_array_layers: 1,
        },
        TextureDimension::D2,
        surface.rgba,
        TextureFormat::Rgba8UnormSrgb,
        RenderAssetUsages::RENDER_WORLD,
    );
    images.add(img)
}

impl WorldPalette {
    /// Builds a unit quad in the XY plane (`[-0.5..0.5]` X/Y, normal `+Z`) mapped to the exact
    /// UV sub-rectangle `[u0..u1] × [v0..v1]` (top-left origin matching `Canvas2d` / wgpu UVs).
    /// Callers creating backdrop quads MUST register the returned `Handle<Mesh>` in `BackdropAssets`
    /// (or call `BackdropAssets::uv_rect_quad`) so `SwitchBackdrop` purges it cleanly.
    pub fn uv_rect_quad(
        meshes: &mut Assets<Mesh>,
        u0: f32,
        v0: f32,
        u1: f32,
        v1: f32,
    ) -> Handle<Mesh> {
        let mut mesh = Mesh::new(
            PrimitiveTopology::TriangleList,
            RenderAssetUsages::MAIN_WORLD | RenderAssetUsages::RENDER_WORLD,
        );
        mesh.insert_attribute(
            Mesh::ATTRIBUTE_POSITION,
            vec![
                [-0.5, 0.5, 0.0],
                [0.5, 0.5, 0.0],
                [0.5, -0.5, 0.0],
                [-0.5, -0.5, 0.0],
            ],
        );
        mesh.insert_attribute(
            Mesh::ATTRIBUTE_NORMAL,
            vec![
                [0.0, 0.0, 1.0],
                [0.0, 0.0, 1.0],
                [0.0, 0.0, 1.0],
                [0.0, 0.0, 1.0],
            ],
        );
        mesh.insert_attribute(
            Mesh::ATTRIBUTE_UV_0,
            vec![[u0, v0], [u1, v0], [u1, v1], [u0, v1]],
        );
        mesh.insert_indices(Indices::U16(vec![0, 3, 2, 0, 2, 1]));
        meshes.add(mesh)
    }

    pub fn new(meshes: &mut Assets<Mesh>) -> Self {
        Self {
            unit_box: meshes.add(Cuboid::new(1.0, 1.0, 1.0)),
            unit_cyl: meshes.add(Cylinder::new(0.5, 1.0)),
            unit_sphere: meshes.add(Sphere::new(0.5)),
            unit_plane: meshes.add(Plane3d::new(Vec3::Z, Vec2::splat(0.5))),
            unit_cone: meshes.add(Cone::new(0.5, 1.0)),
            unit_torus: meshes.add(Torus::new(0.32, 0.5)),
            thin_ring: meshes.add(Torus::new(0.478, 0.522)),
            materials: HashMap::new(),
            owned_materials: Vec::new(),
            owned_images: Vec::new(),
            floor_tile_vram_bytes: 0,
        }
    }

    pub fn mat(
        &mut self,
        assets: &mut Assets<StandardMaterial>,
        hex: &str,
        roughness: f32,
        metallic: f32,
    ) -> Handle<StandardMaterial> {
        let (r, g, b) = parse_hex_rgb(hex);
        self.mat_rgb(assets, r, g, b, roughness, metallic, false)
    }

    pub fn mat_double_sided(
        &mut self,
        assets: &mut Assets<StandardMaterial>,
        hex: &str,
        roughness: f32,
        metallic: f32,
    ) -> Handle<StandardMaterial> {
        let (r, g, b) = parse_hex_rgb(hex);
        self.mat_rgb(assets, r, g, b, roughness, metallic, true)
    }

    pub fn mat_rgb(
        &mut self,
        assets: &mut Assets<StandardMaterial>,
        r: u8,
        g: u8,
        b: u8,
        roughness: f32,
        metallic: f32,
        double_sided: bool,
    ) -> Handle<StandardMaterial> {
        let key = MaterialKey {
            r,
            g,
            b,
            a: 255,
            rough_q: (roughness.clamp(0.0, 1.0) * 100.0).round() as u8,
            metal_q: (metallic.clamp(0.0, 1.0) * 100.0).round() as u8,
            emissive: false,
            unlit: false,
            double_sided,
        };
        if let Some(h) = self.materials.get(&key) {
            return h.clone();
        }
        let h = assets.add(StandardMaterial {
            base_color: Color::srgb_u8(r, g, b),
            perceptual_roughness: roughness.clamp(0.05, 1.0),
            metallic: metallic.clamp(0.0, 1.0),
            cull_mode: if double_sided { None } else { Some(bevy::render::render_resource::Face::Back) },
            double_sided,
            ..default()
        });
        self.materials.insert(key, h.clone());
        h
    }

    pub fn mat_unlit(
        &mut self,
        assets: &mut Assets<StandardMaterial>,
        hex: &str,
    ) -> Handle<StandardMaterial> {
        let (r, g, b) = parse_hex_rgb(hex);
        let key = MaterialKey {
            r,
            g,
            b,
            a: 255,
            rough_q: 0,
            metal_q: 0,
            emissive: false,
            unlit: true,
            double_sided: false,
        };
        if let Some(h) = self.materials.get(&key) {
            return h.clone();
        }
        let h = assets.add(StandardMaterial {
            base_color: Color::srgb_u8(r, g, b),
            unlit: true,
            ..default()
        });
        self.materials.insert(key, h.clone());
        h
    }

    pub fn mat_blend(
        &mut self,
        assets: &mut Assets<StandardMaterial>,
        hex: &str,
        alpha: f32,
        unlit: bool,
    ) -> Handle<StandardMaterial> {
        let (r, g, b) = parse_hex_rgb(hex);
        let a = (alpha.clamp(0.02, 0.98) * 255.0).round() as u8;
        let key = MaterialKey {
            r,
            g,
            b,
            a,
            rough_q: 50,
            metal_q: 0,
            emissive: false,
            unlit,
            double_sided: false,
        };
        if let Some(h) = self.materials.get(&key) {
            return h.clone();
        }
        let h = assets.add(StandardMaterial {
            base_color: Color::srgba_u8(r, g, b, a),
            alpha_mode: AlphaMode::Blend,
            unlit,
            perceptual_roughness: 0.5,
            ..default()
        });
        self.materials.insert(key, h.clone());
        h
    }

    pub fn matte(
        &mut self,
        assets: &mut Assets<StandardMaterial>,
        hex: &str,
    ) -> Handle<StandardMaterial> {
        self.mat(assets, hex, 0.78, 0.0)
    }

    pub fn glass(
        &mut self,
        assets: &mut Assets<StandardMaterial>,
        hex: &str,
        alpha: f32,
    ) -> Handle<StandardMaterial> {
        let (r, g, b) = parse_hex_rgb(hex);
        let a = (alpha.clamp(0.02, 0.98) * 255.0).round() as u8;
        let key = MaterialKey {
            r,
            g,
            b,
            a,
            rough_q: 8,
            metal_q: 25,
            emissive: false,
            unlit: false,
            double_sided: false,
        };
        if let Some(h) = self.materials.get(&key) {
            return h.clone();
        }
        let h = assets.add(StandardMaterial {
            base_color: Color::srgba_u8(r, g, b, a),
            alpha_mode: AlphaMode::Blend,
            perceptual_roughness: 0.08,
            metallic: 0.25,
            ..default()
        });
        self.materials.insert(key, h.clone());
        h
    }

    pub fn emissive(
        &mut self,
        assets: &mut Assets<StandardMaterial>,
        hex: &str,
        strength: f32,
    ) -> Handle<StandardMaterial> {
        let (r, g, b) = parse_hex_rgb(hex);
        let key = MaterialKey {
            r,
            g,
            b,
            a: (strength.clamp(0.1, 4.0) * 50.0).round() as u8,
            rough_q: 30,
            metal_q: 0,
            emissive: true,
            unlit: false,
            double_sided: false,
        };
        if let Some(h) = self.materials.get(&key) {
            return h.clone();
        }
        let lin = Color::srgb_u8(r, g, b).to_linear();
        let h = assets.add(StandardMaterial {
            base_color: Color::srgb_u8(r, g, b),
            emissive: LinearRgba::new(
                lin.red * strength,
                lin.green * strength,
                lin.blue * strength,
                1.0,
            ),
            perceptual_roughness: 0.3,
            ..default()
        });
        self.materials.insert(key, h.clone());
        h
    }

    /// Allocates a non-deduplicated emissive material tracked in `owned_materials` so dynamic
    /// per-entity indicators (e.g. each restroom stall lock light) can mutate independently and
    /// still purge cleanly on `unload_full_office_world`.
    pub fn owned_emissive(
        &mut self,
        assets: &mut Assets<StandardMaterial>,
        hex: &str,
        strength: f32,
    ) -> Handle<StandardMaterial> {
        let (r, g, b) = parse_hex_rgb(hex);
        let lin = Color::srgb_u8(r, g, b).to_linear();
        let h = assets.add(StandardMaterial {
            base_color: Color::srgb_u8(r, g, b),
            emissive: LinearRgba::new(
                lin.red * strength,
                lin.green * strength,
                lin.blue * strength,
                1.0,
            ),
            perceptual_roughness: 0.3,
            ..default()
        });
        self.owned_materials.push(h.clone());
        h
    }

    /// Uploads a `station-canvas2d` RGBA8 surface directly to GPU with `RenderAssetUsages::RENDER_WORLD`,
    /// dropping the CPU buffer immediately after upload for minimum RAM usage.
    pub fn upload_surface(
        &mut self,
        images: &mut Assets<Image>,
        surface: RgbaSurface,
    ) -> Handle<Image> {
        let handle = upload_gpu_image(images, surface);
        self.owned_images.push(handle.clone());
        handle
    }

    /// Replaces a previously owned GPU `Image` handle in-place, removing the old handle from BOTH
    /// `Assets<Image>` and `self.owned_images` so repeated dynamic uploads never leak VRAM/RAM.
    pub fn replace_owned_image(
        &mut self,
        images: &mut Assets<Image>,
        old: Option<&Handle<Image>>,
        surface: RgbaSurface,
    ) -> Handle<Image> {
        if let Some(old_h) = old {
            self.remove_owned_image(images, old_h);
        }
        self.upload_surface(images, surface)
    }

    /// Removes a single previously owned GPU `Image` handle from BOTH `Assets<Image>` and `self.owned_images`.
    pub fn remove_owned_image(&mut self, images: &mut Assets<Image>, old: &Handle<Image>) {
        let old_id = old.id();
        images.remove(old_id);
        self.owned_images.retain(|h| h.id() != old_id);
    }

    pub fn textured_unlit(
        &mut self,
        images: &mut Assets<Image>,
        materials: &mut Assets<StandardMaterial>,
        surface: RgbaSurface,
    ) -> Handle<StandardMaterial> {
        let tex = self.upload_surface(images, surface);
        let mat = materials.add(StandardMaterial {
            base_color_texture: Some(tex),
            unlit: true,
            ..default()
        });
        self.owned_materials.push(mat.clone());
        mat
    }

    pub fn textured_lit(
        &mut self,
        images: &mut Assets<Image>,
        materials: &mut Assets<StandardMaterial>,
        surface: RgbaSurface,
        roughness: f32,
    ) -> Handle<StandardMaterial> {
        let tex = self.upload_surface(images, surface);
        let mat = materials.add(StandardMaterial {
            base_color_texture: Some(tex),
            perceptual_roughness: roughness,
            ..default()
        });
        self.owned_materials.push(mat.clone());
        mat
    }

    pub fn textured_lit_double_sided(
        &mut self,
        images: &mut Assets<Image>,
        materials: &mut Assets<StandardMaterial>,
        surface: RgbaSurface,
        roughness: f32,
    ) -> Handle<StandardMaterial> {
        let tex = self.upload_surface(images, surface);
        let mat = materials.add(StandardMaterial {
            base_color_texture: Some(tex),
            perceptual_roughness: roughness,
            cull_mode: None,
            double_sided: true,
            ..default()
        });
        self.owned_materials.push(mat.clone());
        mat
    }

    /// Uploads a `RENDER_WORLD` emissive canvas texture (e.g. `SHIP IT` neon sign) tracked in
    /// `owned_materials` / `owned_images` so it purges cleanly on unload.
    pub fn textured_emissive(
        &mut self,
        images: &mut Assets<Image>,
        materials: &mut Assets<StandardMaterial>,
        surface: RgbaSurface,
        strength: f32,
    ) -> Handle<StandardMaterial> {
        let tex = self.upload_surface(images, surface);
        let mat = materials.add(StandardMaterial {
            base_color: Color::WHITE,
            base_color_texture: Some(tex.clone()),
            emissive: LinearRgba::new(strength, strength, strength, 1.0),
            emissive_texture: Some(tex),
            perceptual_roughness: 0.35,
            ..default()
        });
        self.owned_materials.push(mat.clone());
        mat
    }

    /// Uploads a vertically-repeating 2×-height scrolling desk screen (`RENDER_WORLD`) with a
    /// `0.5` V-window `uv_transform` so `animate_desks_system` can scroll it in-place with zero allocations.
    pub fn repeating_screen_mat(
        &mut self,
        images: &mut Assets<Image>,
        materials: &mut Assets<StandardMaterial>,
        surface: RgbaSurface,
    ) -> Handle<StandardMaterial> {
        let mut img = Image::new(
            Extent3d {
                width: surface.width.max(1),
                height: surface.height.max(1),
                depth_or_array_layers: 1,
            },
            TextureDimension::D2,
            surface.rgba,
            TextureFormat::Rgba8UnormSrgb,
            RenderAssetUsages::RENDER_WORLD,
        );
        img.sampler = ImageSampler::Descriptor(ImageSamplerDescriptor {
            address_mode_u: ImageAddressMode::ClampToEdge,
            address_mode_v: ImageAddressMode::Repeat,
            mag_filter: ImageFilterMode::Linear,
            min_filter: ImageFilterMode::Linear,
            mipmap_filter: ImageFilterMode::Linear,
            ..default()
        });
        let tex = images.add(img);
        self.owned_images.push(tex.clone());
        let mat = materials.add(StandardMaterial {
            base_color_texture: Some(tex),
            unlit: true,
            uv_transform: Affine2::from_scale(Vec2::new(1.0, 0.5)),
            ..default()
        });
        self.owned_materials.push(mat.clone());
        mat
    }

    /// Uploads a `MAIN_WORLD | RENDER_WORLD` nearest-neighbor pixel-art texture tracked in
    /// `owned_images` / `owned_materials` so systems can mutate `Image.data` in-place via
    /// `copy_from_slice` with zero allocations per frame and zero leaks on unload.
    pub fn dynamic_pixel_screen(
        &mut self,
        images: &mut Assets<Image>,
        materials: &mut Assets<StandardMaterial>,
        width: u32,
        height: u32,
        initial_rgba: &[u8],
    ) -> (Handle<Image>, Handle<StandardMaterial>) {
        let mut img = Image::new(
            Extent3d {
                width: width.max(1),
                height: height.max(1),
                depth_or_array_layers: 1,
            },
            TextureDimension::D2,
            initial_rgba.to_vec(),
            TextureFormat::Rgba8UnormSrgb,
            RenderAssetUsages::MAIN_WORLD | RenderAssetUsages::RENDER_WORLD,
        );
        img.sampler = ImageSampler::Descriptor(ImageSamplerDescriptor {
            address_mode_u: ImageAddressMode::ClampToEdge,
            address_mode_v: ImageAddressMode::ClampToEdge,
            mag_filter: ImageFilterMode::Nearest,
            min_filter: ImageFilterMode::Nearest,
            mipmap_filter: ImageFilterMode::Nearest,
            ..default()
        });
        let tex = images.add(img);
        self.owned_images.push(tex.clone());
        let mat = materials.add(StandardMaterial {
            base_color_texture: Some(tex.clone()),
            unlit: true,
            ..default()
        });
        self.owned_materials.push(mat.clone());
        (tex, mat)
    }

    /// Creates an unlit additive-blended textured material reusing an uploaded image handle,
    /// tracked in `owned_materials` so `unload_full_office_world` leaves zero leaked assets.
    pub fn textured_additive_unlit(
        &mut self,
        materials: &mut Assets<StandardMaterial>,
        tex: Handle<Image>,
        tint: Color,
    ) -> Handle<StandardMaterial> {
        let mat = materials.add(StandardMaterial {
            base_color: tint,
            base_color_texture: Some(tex),
            alpha_mode: AlphaMode::Add,
            unlit: true,
            ..default()
        });
        self.owned_materials.push(mat.clone());
        mat
    }

    /// Creates a double-sided PBR material reusing an uploaded texture handle tinted by `tint_hex`.
    pub fn tinted_textured_double_sided(
        &mut self,
        materials: &mut Assets<StandardMaterial>,
        tex: Handle<Image>,
        tint_hex: &str,
        roughness: f32,
    ) -> Handle<StandardMaterial> {
        let mat = materials.add(StandardMaterial {
            base_color: srgb_hex(tint_hex),
            base_color_texture: Some(tex),
            perceptual_roughness: roughness.clamp(0.05, 1.0),
            cull_mode: None,
            double_sided: true,
            ..default()
        });
        self.owned_materials.push(mat.clone());
        mat
    }

    /// Uploads a compact repeating tile (`RENDER_WORLD` + `ImageAddressMode::Repeat` on U & V)
    /// and tracks its GPU footprint in `floor_tile_vram_bytes`.
    pub fn upload_repeating_surface(
        &mut self,
        images: &mut Assets<Image>,
        surface: RgbaSurface,
    ) -> Handle<Image> {
        let bytes = surface.rgba.len();
        let mut img = Image::new(
            Extent3d {
                width: surface.width.max(1),
                height: surface.height.max(1),
                depth_or_array_layers: 1,
            },
            TextureDimension::D2,
            surface.rgba,
            TextureFormat::Rgba8UnormSrgb,
            RenderAssetUsages::RENDER_WORLD,
        );
        img.sampler = ImageSampler::Descriptor(ImageSamplerDescriptor {
            address_mode_u: ImageAddressMode::Repeat,
            address_mode_v: ImageAddressMode::Repeat,
            mag_filter: ImageFilterMode::Linear,
            min_filter: ImageFilterMode::Linear,
            mipmap_filter: ImageFilterMode::Linear,
            ..default()
        });
        let handle = images.add(img);
        self.owned_images.push(handle.clone());
        self.floor_tile_vram_bytes = self.floor_tile_vram_bytes.saturating_add(bytes);
        handle
    }

    /// Creates a PBR material reusing an already-uploaded repeating tile handle with custom
    /// UV repeat scale (`Affine2::from_scale`) and albedo tint (`base_color`).
    pub fn tiled_mat_from_handle(
        &mut self,
        materials: &mut Assets<StandardMaterial>,
        tex: Handle<Image>,
        repeat: Vec2,
        tint_hex: &str,
        roughness: f32,
    ) -> Handle<StandardMaterial> {
        let mat = materials.add(StandardMaterial {
            base_color: srgb_hex(tint_hex),
            base_color_texture: Some(tex),
            uv_transform: Affine2::from_scale(Vec2::new(repeat.x.max(0.01), repeat.y.max(0.01))),
            perceptual_roughness: roughness.clamp(0.05, 1.0),
            ..default()
        });
        self.owned_materials.push(mat.clone());
        mat
    }

    /// Uploads a repeating tile surface (`RENDER_WORLD` + `ImageAddressMode::Repeat`)
    /// and builds a UV-scaled PBR material.
    pub fn tiled_mat(
        &mut self,
        images: &mut Assets<Image>,
        materials: &mut Assets<StandardMaterial>,
        surface: RgbaSurface,
        repeat: Vec2,
        tint_hex: &str,
        roughness: f32,
    ) -> Handle<StandardMaterial> {
        let tex = self.upload_repeating_surface(images, surface);
        self.tiled_mat_from_handle(materials, tex, repeat, tint_hex, roughness)
    }

    pub fn floor_tile_vram_bytes(&self) -> usize {
        self.floor_tile_vram_bytes
    }

    pub fn material_count(&self) -> usize {
        self.materials.len()
    }

    pub fn purge_transient_images(&mut self, images: &mut Assets<Image>) {
        for h in self.owned_images.drain(..) {
            images.remove(&h);
        }
        self.floor_tile_vram_bytes = 0;
    }

    /// Removes every unit mesh, deduplicated material, textured material, and GPU image
    /// allocated through this palette.
    pub fn purge_all(
        &mut self,
        meshes: &mut Assets<Mesh>,
        materials: &mut Assets<StandardMaterial>,
        images: &mut Assets<Image>,
    ) {
        for h in [
            &self.unit_box,
            &self.unit_cyl,
            &self.unit_sphere,
            &self.unit_plane,
            &self.unit_cone,
            &self.unit_torus,
            &self.thin_ring,
        ] {
            meshes.remove(h);
        }
        for (_, h) in self.materials.drain() {
            materials.remove(&h);
        }
        for h in self.owned_materials.drain(..) {
            materials.remove(&h);
        }
        self.purge_transient_images(images);
    }
}

/// Spawns a scaled unit box tagged with `WorldEntity`.
pub fn spawn_box(
    commands: &mut Commands,
    palette: &WorldPalette,
    mat: Handle<StandardMaterial>,
    pos: Vec3,
    size: Vec3,
) -> Entity {
    commands
        .spawn((
            WorldEntity,
            Mesh3d(palette.unit_box.clone()),
            MeshMaterial3d(mat),
            Transform::from_translation(pos).with_scale(size),
        ))
        .id()
}

pub fn spawn_box_rot(
    commands: &mut Commands,
    palette: &WorldPalette,
    mat: Handle<StandardMaterial>,
    pos: Vec3,
    size: Vec3,
    rot: Quat,
) -> Entity {
    commands
        .spawn((
            WorldEntity,
            Mesh3d(palette.unit_box.clone()),
            MeshMaterial3d(mat),
            Transform::from_translation(pos)
                .with_rotation(rot)
                .with_scale(size),
        ))
        .id()
}

pub fn spawn_cyl(
    commands: &mut Commands,
    palette: &WorldPalette,
    mat: Handle<StandardMaterial>,
    pos: Vec3,
    diameter: f32,
    height: f32,
) -> Entity {
    commands
        .spawn((
            WorldEntity,
            Mesh3d(palette.unit_cyl.clone()),
            MeshMaterial3d(mat),
            Transform::from_translation(pos).with_scale(Vec3::new(diameter, height, diameter)),
        ))
        .id()
}

pub fn spawn_sphere(
    commands: &mut Commands,
    palette: &WorldPalette,
    mat: Handle<StandardMaterial>,
    pos: Vec3,
    scale: Vec3,
) -> Entity {
    commands
        .spawn((
            WorldEntity,
            Mesh3d(palette.unit_sphere.clone()),
            MeshMaterial3d(mat),
            Transform::from_translation(pos).with_scale(scale),
        ))
        .id()
}

pub fn spawn_quad(
    commands: &mut Commands,
    palette: &WorldPalette,
    mat: Handle<StandardMaterial>,
    pos: Vec3,
    size: Vec2,
    rot: Quat,
) -> Entity {
    commands
        .spawn((
            WorldEntity,
            Mesh3d(palette.unit_plane.clone()),
            MeshMaterial3d(mat),
            Transform::from_translation(pos)
                .with_rotation(rot)
                .with_scale(Vec3::new(size.x, size.y, 1.0)),
        ))
        .id()
}

/// Spawns a horizontal floor quad lying in the XZ plane facing `+Y`.
pub fn spawn_floor_quad(
    commands: &mut Commands,
    palette: &WorldPalette,
    mat: Handle<StandardMaterial>,
    pos: Vec3,
    size: Vec2,
) -> Entity {
    spawn_quad(commands, palette, mat, pos, size, Quat::from_rotation_x(-FRAC_PI_2))
}

pub fn spawn_cyl_rot(
    commands: &mut Commands,
    palette: &WorldPalette,
    mat: Handle<StandardMaterial>,
    pos: Vec3,
    diameter: f32,
    height: f32,
    rot: Quat,
) -> Entity {
    commands
        .spawn((
            WorldEntity,
            Mesh3d(palette.unit_cyl.clone()),
            MeshMaterial3d(mat),
            Transform::from_translation(pos)
                .with_rotation(rot)
                .with_scale(Vec3::new(diameter, height, diameter)),
        ))
        .id()
}

pub fn spawn_cone_rot(
    commands: &mut Commands,
    palette: &WorldPalette,
    mat: Handle<StandardMaterial>,
    pos: Vec3,
    scale: Vec3,
    rot: Quat,
) -> Entity {
    commands
        .spawn((
            WorldEntity,
            Mesh3d(palette.unit_cone.clone()),
            MeshMaterial3d(mat),
            Transform::from_translation(pos)
                .with_rotation(rot)
                .with_scale(scale),
        ))
        .id()
}

pub fn spawn_torus_rot(
    commands: &mut Commands,
    palette: &WorldPalette,
    mat: Handle<StandardMaterial>,
    pos: Vec3,
    scale: Vec3,
    rot: Quat,
) -> Entity {
    commands
        .spawn((
            WorldEntity,
            Mesh3d(palette.unit_torus.clone()),
            MeshMaterial3d(mat),
            Transform::from_translation(pos)
                .with_rotation(rot)
                .with_scale(scale),
        ))
        .id()
}

#[cfg(test)]
mod tests {
    use super::*;
    use bevy::render::mesh::VertexAttributeValues;

    #[test]
    fn uv_rect_quad_binds_exact_subrect_uvs_and_plus_z_normals() {
        let mut meshes = Assets::<Mesh>::default();
        let h = WorldPalette::uv_rect_quad(&mut meshes, 0.0, 0.0, 0.5, 0.5);
        let mesh = meshes.get(&h).expect("uv_rect_quad must register mesh");

        let positions = match mesh.attribute(Mesh::ATTRIBUTE_POSITION) {
            Some(VertexAttributeValues::Float32x3(v)) => v,
            other => panic!("expected Float32x3 positions, got {other:?}"),
        };
        assert_eq!(positions.len(), 4);
        for p in positions {
            assert!((p[0].abs() - 0.5).abs() < 1e-5, "X must be +-0.5, got {}", p[0]);
            assert!((p[1].abs() - 0.5).abs() < 1e-5, "Y must be +-0.5, got {}", p[1]);
            assert!(p[2].abs() < 1e-5, "Z must be 0.0, got {}", p[2]);
        }

        let normals = match mesh.attribute(Mesh::ATTRIBUTE_NORMAL) {
            Some(VertexAttributeValues::Float32x3(v)) => v,
            other => panic!("expected Float32x3 normals, got {other:?}"),
        };
        for n in normals {
            assert!((n[2] - 1.0).abs() < 1e-5, "normal must face +Z, got {n:?}");
        }

        let uvs = match mesh.attribute(Mesh::ATTRIBUTE_UV_0) {
            Some(VertexAttributeValues::Float32x2(v)) => v,
            other => panic!("expected Float32x2 UVs, got {other:?}"),
        };
        assert_eq!(uvs.len(), 4);
        let (mut min_u, mut max_u) = (f32::INFINITY, f32::NEG_INFINITY);
        let (mut min_v, mut max_v) = (f32::INFINITY, f32::NEG_INFINITY);
        for uv in uvs {
            assert!(uv[0] >= 0.0 - 1e-6 && uv[0] <= 0.5 + 1e-6, "u out of [0..0.5]: {}", uv[0]);
            assert!(uv[1] >= 0.0 - 1e-6 && uv[1] <= 0.5 + 1e-6, "v out of [0..0.5]: {}", uv[1]);
            min_u = min_u.min(uv[0]);
            max_u = max_u.max(uv[0]);
            min_v = min_v.min(uv[1]);
            max_v = max_v.max(uv[1]);
        }
        assert!((min_u - 0.0).abs() < 1e-5 && (max_u - 0.5).abs() < 1e-5);
        assert!((min_v - 0.0).abs() < 1e-5 && (max_v - 0.5).abs() < 1e-5);
    }
}
