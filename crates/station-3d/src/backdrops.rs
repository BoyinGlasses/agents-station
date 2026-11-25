use crate::layout::LayoutSpec;
use crate::palette::{WorldEntity, WorldPalette};
use bevy::prelude::*;
use station_canvas2d::street_surfaces::{
    SignalColor, draw_atm_atlas, draw_ev_cabinet_atlas, draw_photo_studio_atlas,
    draw_pickleball_atlas, draw_shop_cell, draw_traffic_countdown,
};
use station_canvas2d::{BrandRegistry, FontEngine};
use std::f32::consts::PI;

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

#[derive(Resource, Default)]
pub struct BackdropAssets {
    pub active: Option<BackdropKind>,
    pub owned_images: Vec<Handle<Image>>,
}

impl BackdropAssets {
    pub fn purge(&mut self, images: &mut Assets<Image>) {
        for h in self.owned_images.drain(..) {
            images.remove(&h);
        }
        self.active = None;
    }
}

fn b_box(
    commands: &mut Commands,
    pal: &WorldPalette,
    mat: Handle<StandardMaterial>,
    pos: Vec3,
    size: Vec3,
) -> Entity {
    commands
        .spawn((
            WorldEntity,
            BackdropEntity,
            Mesh3d(pal.unit_box.clone()),
            MeshMaterial3d(mat),
            Transform::from_translation(pos).with_scale(size),
        ))
        .id()
}

fn b_cyl(
    commands: &mut Commands,
    pal: &WorldPalette,
    mat: Handle<StandardMaterial>,
    pos: Vec3,
    diam: f32,
    h: f32,
) -> Entity {
    commands
        .spawn((
            WorldEntity,
            BackdropEntity,
            Mesh3d(pal.unit_cyl.clone()),
            MeshMaterial3d(mat),
            Transform::from_translation(pos).with_scale(Vec3::new(diam, h, diam)),
        ))
        .id()
}

fn b_sphere(
    commands: &mut Commands,
    pal: &WorldPalette,
    mat: Handle<StandardMaterial>,
    pos: Vec3,
    scale: Vec3,
) -> Entity {
    commands
        .spawn((
            WorldEntity,
            BackdropEntity,
            Mesh3d(pal.unit_sphere.clone()),
            MeshMaterial3d(mat),
            Transform::from_translation(pos).with_scale(scale),
        ))
        .id()
}

fn b_quad(
    commands: &mut Commands,
    pal: &WorldPalette,
    mat: Handle<StandardMaterial>,
    pos: Vec3,
    size: Vec2,
    rot: Quat,
) -> Entity {
    commands
        .spawn((
            WorldEntity,
            BackdropEntity,
            Mesh3d(pal.unit_plane.clone()),
            MeshMaterial3d(mat),
            Transform::from_translation(pos)
                .with_rotation(rot)
                .with_scale(Vec3::new(size.x, size.y, 1.0)),
        ))
        .id()
}

pub fn unload_backdrop(
    commands: &mut Commands,
    q: &Query<Entity, With<BackdropEntity>>,
    backdrop_assets: &mut BackdropAssets,
    images: &mut Assets<Image>,
) {
    for ent in q.iter() {
        commands.entity(ent).despawn();
    }
    backdrop_assets.purge(images);
}

#[allow(clippy::too_many_arguments)]
pub fn spawn_backdrop(
    commands: &mut Commands,
    pal: &mut WorldPalette,
    materials: &mut Assets<StandardMaterial>,
    images: &mut Assets<Image>,
    backdrop_assets: &mut BackdropAssets,
    brands: &mut BrandRegistry,
    fonts: &mut FontEngine,
    spec: &LayoutSpec,
    kind: BackdropKind,
) {
    const GY: f32 = -0.62;
    backdrop_assets.active = Some(kind);

    match kind {
        BackdropKind::City => {
            // Ground asphalt & pavements around the office terrace
            let road_m = pal.mat(materials, "#343942", 0.9, 0.0);
            let pave_m = pal.mat(materials, "#B6B3AC", 0.85, 0.0);
            let line_m = pal.mat(materials, "#F0EFEA", 0.7, 0.0);
            let grass_m = pal.mat(materials, "#5F9454", 0.92, 0.0);

            b_box(commands, pal, road_m, Vec3::new(4.0, GY - 0.1, 4.0), Vec3::new(110.0, 0.2, 110.0));

            let front_z = spec.reach.z1 + 4.5;
            let right_x = spec.reach.x1 + 4.8;

            // Road lane dashes along front avenue and right avenue
            for i in -8..=10 {
                b_box(commands, pal, line_m.clone(), Vec3::new(i as f32 * 3.8, GY + 0.02, front_z - 1.2), Vec3::new(1.8, 0.02, 0.16));
                b_box(commands, pal, line_m.clone(), Vec3::new(right_x - 1.2, GY + 0.02, -14.0 + (i + 8) as f32 * 3.8), Vec3::new(0.16, 0.02, 1.8));
            }

            // Pavements across the front & right streets
            b_box(commands, pal, pave_m.clone(), Vec3::new(3.0, GY + 0.07, front_z + 3.2), Vec3::new(60.0, 0.14, 4.2));
            b_box(commands, pal, pave_m, Vec3::new(right_x + 3.2, GY + 0.07, 4.0), Vec3::new(4.2, 0.14, 60.0));

            // Vietnamese shophouses with real brand SVG/canvas2d facades along the right & front avenues
            let shop_wall_cols = ["#E9DEC9", "#D7E2E8", "#EFE4D6", "#DFD7E8", "#E6D8CC", "#D8E4D8"];
            let chains_front = ["gs25", "circlek", "highlands", "pho", "mixue"];
            for (idx, chain) in chains_front.iter().enumerate() {
                let sx = -9.5 + idx as f32 * 6.8;
                let sz = front_z + 6.4;
                let h = 6.2 + (idx % 3) as f32 * 2.1;
                let wm = pal.mat(materials, shop_wall_cols[idx % shop_wall_cols.len()], 0.82, 0.0);
                b_box(commands, pal, wm, Vec3::new(sx, GY + h * 0.5, sz), Vec3::new(6.2, h, 4.6));

                let shop_img = pal.upload_surface(images, draw_shop_cell(chain, brands, fonts));
                backdrop_assets.owned_images.push(shop_img.clone());
                let shop_mat = materials.add(StandardMaterial {
                    base_color_texture: Some(shop_img),
                    perceptual_roughness: 0.45,
                    ..default()
                });
                b_quad(commands, pal, shop_mat, Vec3::new(sx, GY + 1.85, sz - 2.34), Vec2::new(5.8, 2.9), Quat::from_rotation_y(PI));
            }

            let chains_right = ["bhx", "tgdd", "longchau", "cong"];
            for (idx, chain) in chains_right.iter().enumerate() {
                let sx = right_x + 6.4;
                let sz = -6.0 + idx as f32 * 6.8;
                let h = 6.6 + ((idx + 1) % 3) as f32 * 2.0;
                let wm = pal.mat(materials, shop_wall_cols[(idx + 2) % shop_wall_cols.len()], 0.82, 0.0);
                b_box(commands, pal, wm, Vec3::new(sx, GY + h * 0.5, sz), Vec3::new(4.6, h, 6.2));

                let shop_img = pal.upload_surface(images, draw_shop_cell(chain, brands, fonts));
                backdrop_assets.owned_images.push(shop_img.clone());
                let shop_mat = materials.add(StandardMaterial {
                    base_color_texture: Some(shop_img),
                    perceptual_roughness: 0.45,
                    ..default()
                });
                b_quad(commands, pal, shop_mat, Vec3::new(sx - 2.34, GY + 1.85, sz), Vec2::new(5.8, 2.9), Quat::from_rotation_y(-PI * 0.5));
            }

            // Corner Park with Vietcombank ATM, V-Green EV swap cabinet, Korean Photo Studio & Pickleball court!
            let park_x = right_x + 7.2;
            let park_z = front_z + 7.2;
            b_box(commands, pal, grass_m, Vec3::new(park_x, GY + 0.09, park_z), Vec3::new(15.0, 0.16, 14.0));

            // Vietcombank ATM booth
            let atm_img = pal.upload_surface(images, draw_atm_atlas(fonts));
            backdrop_assets.owned_images.push(atm_img.clone());
            let atm_m = materials.add(StandardMaterial { base_color_texture: Some(atm_img), unlit: true, ..default() });
            let vcb_g = pal.mat(materials, "#009A49", 0.5, 0.0);
            b_box(commands, pal, vcb_g, Vec3::new(park_x - 5.2, GY + 1.35, park_z - 4.8), Vec3::new(1.5, 2.5, 1.3));
            b_quad(commands, pal, atm_m, Vec3::new(park_x - 5.2, GY + 1.35, park_z - 5.47), Vec2::new(1.42, 1.42), Quat::from_rotation_y(PI));

            // V-Green EV battery-swap cabinet
            let ev_img = pal.upload_surface(images, draw_ev_cabinet_atlas(fonts));
            backdrop_assets.owned_images.push(ev_img.clone());
            let ev_m = materials.add(StandardMaterial { base_color_texture: Some(ev_img), unlit: true, ..default() });
            let ev_body = pal.mat(materials, "#F4F6F5", 0.45, 0.1);
            b_box(commands, pal, ev_body, Vec3::new(park_x - 2.4, GY + 1.15, park_z - 4.8), Vec3::new(1.8, 2.1, 0.7));
            b_quad(commands, pal, ev_m, Vec3::new(park_x - 2.4, GY + 1.15, park_z - 5.17), Vec2::new(1.72, 1.9), Quat::from_rotation_y(PI));

            // Korean Photo Studio pink container
            let photo_img = pal.upload_surface(images, draw_photo_studio_atlas(fonts));
            backdrop_assets.owned_images.push(photo_img.clone());
            let photo_m = materials.add(StandardMaterial { base_color_texture: Some(photo_img), unlit: true, ..default() });
            let pink_m = pal.mat(materials, "#F5B8CC", 0.65, 0.0);
            b_box(commands, pal, pink_m, Vec3::new(park_x + 2.8, GY + 1.45, park_z - 4.4), Vec3::new(4.6, 2.7, 2.3));
            b_quad(commands, pal, photo_m, Vec3::new(park_x + 2.8, GY + 1.55, park_z - 5.58), Vec2::new(4.2, 2.1), Quat::from_rotation_y(PI));

            // Pickleball court (blue inner court on green surround + white net + gate board)
            let court_blue = pal.mat(materials, "#2D5E9E", 0.85, 0.0);
            let net_white = pal.mat(materials, "#F2F4F7", 0.6, 0.0);
            b_box(commands, pal, court_blue, Vec3::new(park_x + 0.5, GY + 0.2, park_z + 2.2), Vec3::new(9.5, 0.04, 5.2));
            b_box(commands, pal, net_white, Vec3::new(park_x + 0.5, GY + 0.65, park_z + 2.2), Vec3::new(0.06, 0.88, 5.4));
            let pb_img = pal.upload_surface(images, draw_pickleball_atlas(fonts));
            backdrop_assets.owned_images.push(pb_img.clone());
            let pb_m = materials.add(StandardMaterial { base_color_texture: Some(pb_img), unlit: true, ..default() });
            b_quad(commands, pal, pb_m, Vec3::new(park_x - 4.2, GY + 1.3, park_z + 2.2), Vec2::new(2.2, 1.1), Quat::from_rotation_y(-PI * 0.5));

            // Traffic light pole + 7-segment LED countdown
            let pole_m = pal.mat(materials, "#2B2F36", 0.45, 0.5);
            let sig_img = pal.upload_surface(images, draw_traffic_countdown(16, SignalColor::Green));
            backdrop_assets.owned_images.push(sig_img.clone());
            let sig_m = materials.add(StandardMaterial { base_color_texture: Some(sig_img), unlit: true, ..default() });
            b_cyl(commands, pal, pole_m, Vec3::new(right_x - 3.0, GY + 1.9, front_z - 3.0), 0.14, 3.8);
            b_quad(commands, pal, sig_m, Vec3::new(right_x - 2.6, GY + 3.1, front_z - 3.0), Vec2::new(0.55, 0.35), Quat::from_rotation_y(PI * 0.25));

            // Animated street traffic (Grab green scooter, Xanh SM cyan car, red/white scooters)
            let vehicles = [
                (true, front_z - 2.1, -18.0, 24.0, 5.5, 0.0, "#00B14F", Vec3::new(1.5, 0.9, 0.65)),
                (true, front_z + 0.1, -18.0, 24.0, -6.2, 14.0, "#00C2D1", Vec3::new(3.2, 1.25, 1.5)),
                (false, right_x - 2.1, -16.0, 24.0, 4.8, 6.0, "#E63946", Vec3::new(0.65, 0.9, 1.5)),
                (false, right_x + 0.1, -16.0, 24.0, -5.8, 18.0, "#F4F5F7", Vec3::new(1.5, 1.25, 3.2)),
            ];
            for (axis_x, lane, min_p, max_p, spd, off, col, sz) in vehicles {
                let vm = pal.mat(materials, col, 0.35, 0.25);
                let init_pos = if axis_x {
                    Vec3::new(min_p + off, GY + sz.y * 0.5, lane)
                } else {
                    Vec3::new(lane, GY + sz.y * 0.5, min_p + off)
                };
                let ent = b_box(commands, pal, vm, init_pos, sz);
                commands.entity(ent).insert(StreetVehicle {
                    axis_x,
                    lane,
                    min_pos: min_p,
                    max_pos: max_p,
                    speed: spd,
                    offset: off,
                });
            }
        }
        BackdropKind::Countryside => {
            // Emerald & golden terraced Vietnamese rice paddies + limestone karst hills + red-tile village houses
            let grass = pal.mat(materials, "#7DAE4E", 0.92, 0.0);
            let paddy_g = pal.mat(materials, "#94C152", 0.85, 0.0);
            let paddy_y = pal.mat(materials, "#D6BD5E", 0.85, 0.0);
            let water = pal.mat(materials, "#5B9AA8", 0.25, 0.1);
            let hill_a = pal.mat(materials, "#668A6E", 0.92, 0.0);
            let hill_b = pal.mat(materials, "#4D735A", 0.92, 0.0);
            let wall = pal.mat(materials, "#F1E3B5", 0.85, 0.0);
            let roof = pal.mat(materials, "#B5533C", 0.78, 0.0);
            let trunk = pal.mat(materials, "#6E4B32", 0.7, 0.0);
            let leaf = pal.mat(materials, "#457E38", 0.75, 0.0);

            b_box(commands, pal, grass, Vec3::new(4.0, GY - 0.1, 4.0), Vec3::new(120.0, 0.2, 120.0));

            let front_z = spec.reach.z1 + 5.5;
            let right_x = spec.reach.x1 + 6.0;
            for i in 0..4 {
                let pm = if i % 2 == 0 { paddy_g.clone() } else { paddy_y.clone() };
                b_box(commands, pal, pm, Vec3::new(-10.0 + i as f32 * 9.0, GY + 0.04, front_z + 4.5), Vec3::new(8.2, 0.08, 10.5));
                b_box(commands, pal, paddy_g.clone(), Vec3::new(right_x + 5.0, GY + 0.04, -10.0 + i as f32 * 8.5), Vec3::new(11.0, 0.08, 7.6));
            }
            // Irrigation canal
            b_box(commands, pal, water, Vec3::new(3.0, GY + 0.02, front_z - 1.5), Vec3::new(65.0, 0.05, 1.4));

            // Karst hills along far horizon
            for (hx, hz, sx, sy, sz, dark) in [
                (-22.0_f32, -24.0_f32, 14.0_f32, 13.0_f32, 12.0_f32, false),
                (-4.0, -26.0, 18.0, 17.0, 14.0, true),
                (18.0, -24.0, 16.0, 14.5, 13.0, false),
                (34.0, -6.0, 14.0, 12.0, 16.0, true),
            ] {
                b_sphere(commands, pal, if dark { hill_b.clone() } else { hill_a.clone() }, Vec3::new(hx, GY + sy * 0.35, hz), Vec3::new(sx, sy, sz));
            }

            // Red-tile village houses & bamboo trees
            for (vx, vz) in [(right_x + 4.5, front_z + 5.5), (right_x + 9.0, -4.0), (-10.0, front_z + 11.5)] {
                b_box(commands, pal, wall.clone(), Vec3::new(vx, GY + 1.2, vz), Vec3::new(4.2, 2.4, 3.4));
                b_box(commands, pal, roof.clone(), Vec3::new(vx, GY + 2.65, vz), Vec3::new(4.8, 0.65, 4.0));
                b_cyl(commands, pal, trunk.clone(), Vec3::new(vx - 3.2, GY + 1.5, vz - 1.2), 0.35, 3.0);
                b_sphere(commands, pal, leaf.clone(), Vec3::new(vx - 3.2, GY + 3.4, vz - 1.2), Vec3::splat(2.4));
            }
        }
        BackdropKind::Beach => {
            // Golden sand + turquoise shallow water + deep ocean + islands + palm trees + colorful umbrellas
            let sand = pal.mat(materials, "#EBD5A7", 0.9, 0.0);
            let shallow = pal.mat(materials, "#56C6D8", 0.22, 0.05);
            let deep = pal.mat(materials, "#2B7FA8", 0.2, 0.1);
            let foam = pal.mat(materials, "#F2FAFC", 0.5, 0.0);
            let trunk = pal.mat(materials, "#8C6239", 0.75, 0.0);
            let frond = pal.mat(materials, "#3B8E42", 0.65, 0.0);
            let island = pal.mat(materials, "#628E68", 0.88, 0.0);

            b_box(commands, pal, sand, Vec3::new(4.0, GY - 0.1, 4.0), Vec3::new(120.0, 0.2, 120.0));
            let front_z = spec.reach.z1 + 6.0;
            let right_x = spec.reach.x1 + 6.5;

            // Shoreline foam, shallow turquoise bay, deep ocean
            b_box(commands, pal, foam, Vec3::new(6.0, GY + 0.02, front_z + 2.2), Vec3::new(90.0, 0.03, 0.8));
            b_box(commands, pal, shallow.clone(), Vec3::new(6.0, GY + 0.01, front_z + 10.5), Vec3::new(100.0, 0.04, 16.0));
            b_box(commands, pal, deep, Vec3::new(6.0, GY, front_z + 30.0), Vec3::new(120.0, 0.04, 24.0));
            b_box(commands, pal, shallow, Vec3::new(right_x + 14.0, GY + 0.01, 4.0), Vec3::new(22.0, 0.04, 90.0));

            // Distant bay islands
            b_sphere(commands, pal, island.clone(), Vec3::new(28.0, GY + 2.5, front_z + 14.0), Vec3::new(10.0, 6.5, 8.0));
            b_sphere(commands, pal, island, Vec3::new(-12.0, GY + 2.0, front_z + 18.0), Vec3::new(12.0, 5.5, 9.0));

            // Palm trees & striped beach umbrellas along the sand
            let umb_cols = ["#E63946", "#3F7CC4", "#FFD166", "#FF6B9A"];
            for (i, &(px, pz)) in [
                (-6.0_f32, front_z - 1.2_f32),
                (4.0, front_z - 1.0),
                (14.0, front_z - 1.2),
                (right_x + 1.2, -4.0),
                (right_x + 1.2, 5.0),
            ]
            .iter()
            .enumerate()
            {
                b_cyl(commands, pal, trunk.clone(), Vec3::new(px, GY + 2.2, pz), 0.28, 4.4);
                b_sphere(commands, pal, frond.clone(), Vec3::new(px, GY + 4.4, pz), Vec3::new(2.8, 0.75, 2.8));
                let um = pal.mat(materials, umb_cols[i % umb_cols.len()], 0.7, 0.0);
                let pole = pal.mat(materials, "#FFFFFF", 0.4, 0.2);
                b_cyl(commands, pal, pole, Vec3::new(px + 2.4, GY + 1.0, pz + 0.6), 0.07, 2.0);
                b_sphere(commands, pal, um, Vec3::new(px + 2.4, GY + 2.0, pz + 0.6), Vec3::new(2.1, 0.45, 2.1));
            }
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

        for kind in [BackdropKind::City, BackdropKind::Countryside, BackdropKind::Beach, BackdropKind::City] {
            world.resource_scope(|world, mut pal: Mut<WorldPalette>| {
                world.resource_scope(|world, mut mats: Mut<Assets<StandardMaterial>>| {
                    world.resource_scope(|world, mut imgs: Mut<Assets<Image>>| {
                        world.resource_scope(|world, mut ba: Mut<BackdropAssets>| {
                            let mut cmds = world.commands();
                            spawn_backdrop(&mut cmds, &mut pal, &mut mats, &mut imgs, &mut ba, &mut brands, &mut fonts, &spec, kind);
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
                let mut imgs = world.resource_mut::<Assets<Image>>();
                ba.purge(&mut imgs);
            });

            assert_eq!(world.query_filtered::<Entity, With<BackdropEntity>>().iter(&world).count(), 0);
            assert_eq!(world.resource::<BackdropAssets>().owned_images.len(), 0);
            assert_eq!(world.resource::<Assets<Image>>().len(), 0);
            // Interior entity must survive untouched
            assert_eq!(world.query_filtered::<Entity, With<WorldEntity>>().iter(&world).count(), 1);
        }
    }
}
