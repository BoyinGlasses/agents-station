use crate::layout::LayoutSpec;
use crate::palette::{WorldPalette, parse_hex_rgb, srgb_hex, upload_gpu_image};
use bevy::asset::RenderAssetUsages;
use bevy::image::{ImageAddressMode, ImageFilterMode, ImageSampler, ImageSamplerDescriptor};
use bevy::math::Affine2;
use bevy::prelude::*;
use bevy::render::render_resource::{Extent3d, Face, TextureDimension, TextureFormat};
use chrono::Datelike;
use station_canvas2d::{BrandRegistry, FontEngine, RgbaSurface};
use std::collections::HashMap;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Resource, Default)]
pub enum BackdropKind {
    #[default]
    City,
    Countryside,
    Beach,
}

impl BackdropKind {
    pub fn label(self) -> &'static str {
        match self {
            Self::City => "city",
            Self::Countryside => "country",
            Self::Beach => "beach",
        }
    }
}

/// Marker for every entity belonging to the active exterior backdrop.
/// Switching backdrops despawns all `BackdropEntity`s and purges `BackdropAssets`.
#[derive(Component, Debug, Clone, Copy)]
pub struct BackdropEntity;

#[derive(Component)]
pub struct StreetVehicle {
    pub axis_x: bool,
    pub lane: f32,
    pub min_pos: f32,
    pub max_pos: f32,
    pub speed: f32,
    pub offset: f32,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
struct BackdropMatKey {
    r: u8,
    g: u8,
    b: u8,
    a: u8,
    rough_q: u8,
    metal_q: u8,
    flags: u8, // bit0: unlit, bit1: double_sided, bit2: emissive, bit3: additive
}

#[derive(Resource, Default)]
pub struct BackdropAssets {
    pub active: Option<BackdropKind>,
    pub owned_meshes: Vec<Handle<Mesh>>,
    pub owned_images: Vec<Handle<Image>>,
    pub owned_materials: Vec<Handle<StandardMaterial>>,
    mat_cache: HashMap<BackdropMatKey, Handle<StandardMaterial>>,
    uv_quad_cache: HashMap<(u16, u16, u16, u16), Handle<Mesh>>,
}

impl BackdropAssets {
    /// Registers an arbitrary procedural `Mesh` (merged static city geometry, decoded `cars.json`
    /// vehicle mesh, etc.) so `SwitchBackdrop` and `unload_full_office_world` purge it with 0 leaks.
    pub fn add_mesh(&mut self, meshes: &mut Assets<Mesh>, mesh: Mesh) -> Handle<Mesh> {
        let h = meshes.add(mesh);
        self.owned_meshes.push(h.clone());
        h
    }

    /// Deduplicated UV-sub-rect unit quad (`[-0.5..0.5]` XY, `+Z` normal) registered in
    /// `BackdropAssets::owned_meshes` so multi-panel atlases crop cleanly and purge 100%.
    pub fn uv_rect_quad(
        &mut self,
        meshes: &mut Assets<Mesh>,
        u0: f32,
        v0: f32,
        u1: f32,
        v1: f32,
    ) -> Handle<Mesh> {
        let q = |v: f32| (v.clamp(0.0, 1.0) * 65535.0).round() as u16;
        let key = (q(u0), q(v0), q(u1), q(v1));
        if let Some(h) = self.uv_quad_cache.get(&key) {
            return h.clone();
        }
        let h = WorldPalette::uv_rect_quad(meshes, u0, v0, u1, v1);
        self.uv_quad_cache.insert(key, h.clone());
        self.owned_meshes.push(h.clone());
        h
    }

    /// Pixel-coordinate helper over an atlas of `(atlas_w, atlas_h)` pixels.
    pub fn atlas_quad(
        &mut self,
        meshes: &mut Assets<Mesh>,
        atlas_w: f32,
        atlas_h: f32,
        x: f32,
        y: f32,
        w: f32,
        h: f32,
    ) -> Handle<Mesh> {
        self.uv_rect_quad(
            meshes,
            x / atlas_w,
            y / atlas_h,
            (x + w) / atlas_w,
            (y + h) / atlas_h,
        )
    }

    pub fn mat(
        &mut self,
        materials: &mut Assets<StandardMaterial>,
        hex: &str,
        roughness: f32,
        metallic: f32,
    ) -> Handle<StandardMaterial> {
        self.mat_opts(materials, hex, 1.0, roughness, metallic, false, false)
    }

    pub fn mat_double_sided(
        &mut self,
        materials: &mut Assets<StandardMaterial>,
        hex: &str,
        roughness: f32,
        metallic: f32,
    ) -> Handle<StandardMaterial> {
        self.mat_opts(materials, hex, 1.0, roughness, metallic, false, true)
    }

    pub fn mat_unlit(
        &mut self,
        materials: &mut Assets<StandardMaterial>,
        hex: &str,
    ) -> Handle<StandardMaterial> {
        self.mat_opts(materials, hex, 1.0, 0.0, 0.0, true, false)
    }

    pub fn mat_blend(
        &mut self,
        materials: &mut Assets<StandardMaterial>,
        hex: &str,
        alpha: f32,
        roughness: f32,
        unlit: bool,
    ) -> Handle<StandardMaterial> {
        self.mat_opts(materials, hex, alpha, roughness, 0.0, unlit, true)
    }

    fn mat_opts(
        &mut self,
        materials: &mut Assets<StandardMaterial>,
        hex: &str,
        alpha: f32,
        roughness: f32,
        metallic: f32,
        unlit: bool,
        double_sided: bool,
    ) -> Handle<StandardMaterial> {
        let (r, g, b) = parse_hex_rgb(hex);
        let a = (alpha.clamp(0.02, 1.0) * 255.0).round() as u8;
        let flags = (unlit as u8) | ((double_sided as u8) << 1);
        let key = BackdropMatKey {
            r,
            g,
            b,
            a,
            rough_q: (roughness.clamp(0.0, 1.0) * 100.0).round() as u8,
            metal_q: (metallic.clamp(0.0, 1.0) * 100.0).round() as u8,
            flags,
        };
        if let Some(h) = self.mat_cache.get(&key) {
            return h.clone();
        }
        let h = materials.add(StandardMaterial {
            base_color: Color::srgba_u8(r, g, b, a),
            alpha_mode: if a < 255 {
                AlphaMode::Blend
            } else {
                AlphaMode::Opaque
            },
            perceptual_roughness: roughness.clamp(0.05, 1.0),
            metallic: metallic.clamp(0.0, 1.0),
            unlit,
            cull_mode: if double_sided {
                None
            } else {
                Some(Face::Back)
            },
            double_sided,
            ..default()
        });
        self.mat_cache.insert(key, h.clone());
        self.owned_materials.push(h.clone());
        h
    }

    pub fn mat_emissive(
        &mut self,
        materials: &mut Assets<StandardMaterial>,
        hex: &str,
        strength: f32,
    ) -> Handle<StandardMaterial> {
        let (r, g, b) = parse_hex_rgb(hex);
        let key = BackdropMatKey {
            r,
            g,
            b,
            a: (strength.clamp(0.1, 4.0) * 50.0).round() as u8,
            rough_q: 30,
            metal_q: 0,
            flags: 0b0100,
        };
        if let Some(h) = self.mat_cache.get(&key) {
            return h.clone();
        }
        let lin = Color::srgb_u8(r, g, b).to_linear();
        let h = materials.add(StandardMaterial {
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
        self.mat_cache.insert(key, h.clone());
        self.owned_materials.push(h.clone());
        h
    }

    /// Allocates a non-cached material tracked in `owned_materials` (for dynamic night/siren/signal tinting).
    pub fn add_material(
        &mut self,
        materials: &mut Assets<StandardMaterial>,
        mat: StandardMaterial,
    ) -> Handle<StandardMaterial> {
        let h = materials.add(mat);
        self.owned_materials.push(h.clone());
        h
    }

    /// Uploads a `RENDER_WORLD` GPU texture owned by this backdrop.
    pub fn upload_image(&mut self, images: &mut Assets<Image>, surface: RgbaSurface) -> Handle<Image> {
        let img = upload_gpu_image(images, surface);
        self.owned_images.push(img.clone());
        img
    }

    /// Uploads a repeating `RENDER_WORLD` GPU texture owned by this backdrop.
    pub fn upload_repeating_image(
        &mut self,
        images: &mut Assets<Image>,
        surface: RgbaSurface,
    ) -> Handle<Image> {
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
        handle
    }

    pub fn textured(
        &mut self,
        images: &mut Assets<Image>,
        materials: &mut Assets<StandardMaterial>,
        surface: RgbaSurface,
        unlit: bool,
        roughness: f32,
    ) -> Handle<StandardMaterial> {
        let img = self.upload_image(images, surface);
        self.textured_shared(materials, img, unlit, roughness, false)
    }

    pub fn textured_shared(
        &mut self,
        materials: &mut Assets<StandardMaterial>,
        img: Handle<Image>,
        unlit: bool,
        roughness: f32,
        double_sided: bool,
    ) -> Handle<StandardMaterial> {
        self.add_material(
            materials,
            StandardMaterial {
                base_color_texture: Some(img),
                unlit,
                perceptual_roughness: roughness,
                cull_mode: if double_sided { None } else { Some(Face::Back) },
                double_sided,
                ..default()
            },
        )
    }

    pub fn textured_tiled(
        &mut self,
        materials: &mut Assets<StandardMaterial>,
        img: Handle<Image>,
        repeat: Vec2,
        tint_hex: &str,
        roughness: f32,
    ) -> Handle<StandardMaterial> {
        self.add_material(
            materials,
            StandardMaterial {
                base_color: srgb_hex(tint_hex),
                base_color_texture: Some(img),
                uv_transform: Affine2::from_scale(Vec2::new(repeat.x.max(0.01), repeat.y.max(0.01))),
                perceptual_roughness: roughness.clamp(0.05, 1.0),
                ..default()
            },
        )
    }

    pub fn purge(
        &mut self,
        meshes: &mut Assets<Mesh>,
        materials: &mut Assets<StandardMaterial>,
        images: &mut Assets<Image>,
    ) {
        self.mat_cache.clear();
        self.uv_quad_cache.clear();
        for h in self.owned_meshes.drain(..) {
            meshes.remove(&h);
        }
        for h in self.owned_materials.drain(..) {
            materials.remove(&h);
        }
        for h in self.owned_images.drain(..) {
            images.remove(&h);
        }
        self.active = None;
    }
}

pub fn unload_backdrop(
    commands: &mut Commands,
    q: &Query<Entity, With<BackdropEntity>>,
    backdrop_assets: &mut BackdropAssets,
    meshes: &mut Assets<Mesh>,
    materials: &mut Assets<StandardMaterial>,
    images: &mut Assets<Image>,
) {
    for ent in q.iter() {
        commands.entity(ent).try_despawn();
    }
    commands.remove_resource::<crate::street::CityStreetNav>();
    backdrop_assets.purge(meshes, materials, images);
}

#[allow(clippy::too_many_arguments)]
pub fn spawn_backdrop(
    commands: &mut Commands,
    _pal: &mut WorldPalette,
    meshes: &mut Assets<Mesh>,
    materials: &mut Assets<StandardMaterial>,
    images: &mut Assets<Image>,
    backdrop_assets: &mut BackdropAssets,
    brands: &mut BrandRegistry,
    fonts: &mut FontEngine,
    spec: &LayoutSpec,
    kind: BackdropKind,
    night: bool,
    city: &str,
    temp_c: f32,
) {
    backdrop_assets.active = Some(kind);

    match kind {
        BackdropKind::City => {
            let is_friday = chrono::Local::now().weekday() == chrono::Weekday::Fri;
            crate::street::spawn_city_backdrop(
                commands,
                meshes,
                materials,
                images,
                backdrop_assets,
                brands,
                fonts,
                spec,
                city,
                night,
                temp_c,
                is_friday,
            );
        }
        BackdropKind::Countryside => {
            crate::street::country_beach::spawn_countryside_backdrop(
                commands,
                meshes,
                materials,
                images,
                backdrop_assets,
                fonts,
                spec,
                night,
            );
        }
        BackdropKind::Beach => {
            crate::street::country_beach::spawn_beach_backdrop(
                commands,
                meshes,
                materials,
                images,
                backdrop_assets,
                spec,
                night,
            );
        }
    }
}

pub fn animate_backdrop_traffic_system(
    time: Res<Time>,
    mut q: Query<(&StreetVehicle, &mut Transform)>,
) {
    let t = time.elapsed_secs();
    for (v, mut tf) in &mut q {
        let span = (v.max_pos - v.min_pos).max(1.0);
        let raw = (v.offset + t * v.speed).rem_euclid(span);
        let pos = v.min_pos + raw;
        if v.axis_x {
            tf.translation.x = pos;
        } else {
            tf.translation.z = pos;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::layout::OfficeLayout;
    use crate::palette::WorldEntity;
    use std::path::PathBuf;

    #[test]
    fn backdrop_true_unload_despawns_all_entities_and_purges_gpu_images_across_cycles() {
        let assets_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../assets");
        let spec = LayoutSpec::for_layout(OfficeLayout::Pro);

        let mut world = World::new();
        world.insert_resource(Assets::<Mesh>::default());
        world.insert_resource(Assets::<StandardMaterial>::default());
        world.insert_resource(Assets::<Image>::default());
        world.insert_resource(BackdropAssets::default());

        let pal = {
            let mut meshes = world.resource_mut::<Assets<Mesh>>();
            WorldPalette::new(&mut meshes)
        };
        world.insert_resource(pal);
        // Spawn one non-backdrop office entity to prove unload never touches interior
        world.spawn(WorldEntity);

        let mut fonts = FontEngine::from_assets_dir(&assets_dir);
        let mut brands = BrandRegistry::new(assets_dir);

        let baseline_meshes = world.resource::<Assets<Mesh>>().len();
        for kind in [BackdropKind::City, BackdropKind::Countryside, BackdropKind::Beach, BackdropKind::City] {
            world.resource_scope(|world, mut pal: Mut<WorldPalette>| {
                world.resource_scope(|world, mut meshes: Mut<Assets<Mesh>>| {
                    world.resource_scope(|world, mut mats: Mut<Assets<StandardMaterial>>| {
                        world.resource_scope(|world, mut imgs: Mut<Assets<Image>>| {
                            world.resource_scope(|world, mut ba: Mut<BackdropAssets>| {
                                let mut cmds = world.commands();
                                spawn_backdrop(&mut cmds, &mut pal, &mut meshes, &mut mats, &mut imgs, &mut ba, &mut brands, &mut fonts, &spec, kind, false, "hcm", 31.0);
                            });
                        });
                    });
                });
            });
            world.flush();

            let count_loaded = world.query_filtered::<Entity, With<BackdropEntity>>().iter(&world).count();
            assert!(count_loaded > 15, "{kind:?} must spawn backdrop geometry");
            if kind == BackdropKind::City {
                assert!(world.resource::<BackdropAssets>().owned_images.len() >= 10);
            }

            // Unload
            let to_despawn: Vec<Entity> = world.query_filtered::<Entity, With<BackdropEntity>>().iter(&world).collect();
            for e in to_despawn {
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

            assert_eq!(world.query_filtered::<Entity, With<BackdropEntity>>().iter(&world).count(), 0);
            assert_eq!(world.resource::<BackdropAssets>().owned_meshes.len(), 0);
            assert_eq!(world.resource::<BackdropAssets>().owned_images.len(), 0);
            assert_eq!(world.resource::<BackdropAssets>().owned_materials.len(), 0);
            assert_eq!(world.resource::<Assets<Mesh>>().len(), baseline_meshes);
            assert_eq!(world.resource::<Assets<Image>>().len(), 0);
            assert_eq!(world.resource::<Assets<StandardMaterial>>().len(), 0);
            // Interior entity must survive untouched
            assert_eq!(world.query_filtered::<Entity, With<WorldEntity>>().iter(&world).count(), 1);
        }
    }

    #[test]
    fn backdrop_switch_3x_leaks_zero_materials_and_zero_images() {
        let assets_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../assets");
        let spec = LayoutSpec::for_layout(OfficeLayout::Pro);

        let mut world = World::new();
        world.insert_resource(Assets::<Mesh>::default());
        world.insert_resource(Assets::<StandardMaterial>::default());
        world.insert_resource(Assets::<Image>::default());
        world.insert_resource(BackdropAssets::default());

        let pal = {
            let mut meshes = world.resource_mut::<Assets<Mesh>>();
            WorldPalette::new(&mut meshes)
        };
        world.insert_resource(pal);

        let mut fonts = FontEngine::from_assets_dir(&assets_dir);
        let mut brands = BrandRegistry::new(assets_dir);

        let baseline_meshes = world.resource::<Assets<Mesh>>().len();
        let baseline_mats = world.resource::<Assets<StandardMaterial>>().len();
        let baseline_imgs = world.resource::<Assets<Image>>().len();

        // Perform 3 full backdrop switch cycles (City -> Countryside -> Beach).
        for _ in 0..3 {
            for kind in [BackdropKind::City, BackdropKind::Countryside, BackdropKind::Beach] {
                world.resource_scope(|world, mut pal: Mut<WorldPalette>| {
                    world.resource_scope(|world, mut meshes: Mut<Assets<Mesh>>| {
                        world.resource_scope(|world, mut mats: Mut<Assets<StandardMaterial>>| {
                            world.resource_scope(|world, mut imgs: Mut<Assets<Image>>| {
                                world.resource_scope(|world, mut ba: Mut<BackdropAssets>| {
                                    let mut cmds = world.commands();
                                    spawn_backdrop(
                                        &mut cmds,
                                        &mut pal,
                                        &mut meshes,
                                        &mut mats,
                                        &mut imgs,
                                        &mut ba,
                                        &mut brands,
                                        &mut fonts,
                                        &spec,
                                        kind,
                                        false,
                                        "hcm",
                                        31.0,
                                    );
                                });
                            });
                        });
                    });
                });
                world.flush();

                let to_despawn: Vec<Entity> = world
                    .query_filtered::<Entity, With<BackdropEntity>>()
                    .iter(&world)
                    .collect();
                for e in to_despawn {
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

                assert_eq!(
                    world.resource::<Assets<Mesh>>().len(),
                    baseline_meshes,
                    "unloading {kind:?} must leak 0 Meshes"
                );
                assert_eq!(
                    world.resource::<Assets<Image>>().len(),
                    baseline_imgs,
                    "unloading {kind:?} must leak 0 Images"
                );
                assert_eq!(
                    world.resource::<Assets<StandardMaterial>>().len(),
                    baseline_mats,
                    "unloading {kind:?} must leak 0 StandardMaterials"
                );
            }
        }
    }
}
