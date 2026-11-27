//! *Quán cơm bình dân* with 8 UV-cropped Vietnamese dish trays, inox tables, plastic stools,
//! market umbrella, and Friday *Bún đậu mắm tôm* swap (`eat-com.js`).

use crate::backdrops::{BackdropAssets, BackdropEntity};
use crate::palette::WorldEntity;
use crate::street::{SpotFinder, StreetMeshBuilder, spawn_backdrop_uv_quad};
use bevy::prelude::*;
use station_canvas2d::FontEngine;
use station_canvas2d::street_surfaces::draw_com_binh_dan_atlas;
use std::f32::consts::FRAC_PI_2;

const TOP: f32 = 0.66;
const STOOL_H: f32 = 0.44;
const INOX: &str = "#C9CED6";

#[derive(Component)]
pub struct EatComStall {
    pub is_friday_bun_dau: bool,
}

#[derive(Component)]
pub struct ComSteamPuff {
    pub base_pos: Vec3,
    pub phase: f32,
}

#[allow(clippy::too_many_arguments)]
pub fn spawn_eat_com(
    commands: &mut Commands,
    meshes: &mut Assets<Mesh>,
    materials: &mut Assets<StandardMaterial>,
    images: &mut Assets<Image>,
    ba: &mut BackdropAssets,
    fonts: &mut FontEngine,
    finder: &mut SpotFinder,
    is_friday: bool,
) -> Option<Vec3> {
    let f = finder
        .find(5.8, 2.4, 2.6, true, 44.0)
        .or_else(|| finder.find(5.8, 2.4, 2.6, true, 64.0))?;
    let rot = f.face_out_rot();

    let mut b = StreetMeshBuilder::default();
    let cx = -1.85;
    let cz = 0.65;

    // Cart body + stainless countertop + 4 wheels
    b.push_box_rot(f.at(cx, cz, 0.44), Vec3::new(1.30, 0.72, 0.62), rot, "#D8DCE1");
    b.push_box_rot(f.at(cx, cz, 0.81), Vec3::new(1.34, 0.04, 0.66), rot, INOX);
    for (wx, wz) in [(-0.5_f32, -0.33_f32), (0.5, -0.33), (-0.5, 0.33), (0.5, 0.33)] {
        b.push_cyl_rot(
            f.at(cx + wx, cz + wz, 0.07),
            0.07,
            0.07,
            0.04,
            10,
            rot * Quat::from_rotation_x(FRAC_PI_2),
            "#2B2F36",
        );
    }
    // Display cabinet corner posts, top & glass shelf, orange awning roof
    for (px, pz) in [(-0.63_f32, -0.29_f32), (0.63, -0.29), (-0.63, 0.29), (0.63, 0.29)] {
        b.push_box_rot(f.at(cx + px, cz + pz, 1.12), Vec3::new(0.025, 0.58, 0.025), rot, "#B9C0C8");
        b.push_box_rot(f.at(cx + px * 1.08, cz + pz * 1.25, 1.43), Vec3::new(0.03, 1.20, 0.03), rot, "#8E959E");
    }
    b.push_box_rot(f.at(cx, cz, 1.41), Vec3::new(1.28, 0.02, 0.58), rot, "#B9C0C8");
    b.push_box_rot(f.at(cx, cz, 1.08), Vec3::new(1.24, 0.015, 0.52), rot, "#E4F1F5");
    b.push_box_rot(f.at(cx, cz, 2.04), Vec3::new(1.62, 0.04, 0.96), rot, "#E8742A");
    b.push_box_rot(f.at(cx, cz + 0.48, 2.00), Vec3::new(1.62, 0.06, 0.03), rot, "#C85A1A");
    // Compartment trays stacked on top of cabinet
    for k in 0..5 {
        b.push_box_rot(f.at(cx + 0.40, cz - 0.12, 1.43 + k as f32 * 0.02), Vec3::new(0.32, 0.016, 0.24), rot, INOX);
    }

    // 8 stainless dish trays in 2 rows (UV-cropped dish tops placed below)
    let mut tray_slots: Vec<(f32, f32, usize)> = Vec::with_capacity(8);
    for (ty, row) in [(0.84_f32, [2usize, 4, 5, 0]), (1.10, [0, 1, 3, 7])] {
        for (i, &dish_k) in row.iter().enumerate() {
            let tx = cx - 0.45 + i as f32 * 0.30;
            b.push_box_rot(f.at(tx, cz, ty), Vec3::new(0.27, 0.035, 0.22), rot, INOX);
            tray_slots.push((tx, ty + 0.02, dish_k));
        }
    }

    // Big stainless rice pot on stand + side table with steaming canh chua pot
    let (rx, rz) = (-2.62, 0.20);
    b.push_box_rot(f.at(rx, rz, 0.15), Vec3::new(0.50, 0.30, 0.50), rot, "#3A3F47");
    b.push_cyl(f.at(rx, rz, 0.52), 0.22, 0.20, 0.44, 12, "#D8DCE1");
    b.push_cyl(f.at(rx, rz, 0.745), 0.205, 0.205, 0.015, 12, "#F7F5EE");

    let (sx, sz) = (-0.95, 0.25);
    b.push_box_rot(f.at(sx, sz, 0.64), Vec3::new(0.50, 0.035, 0.46), rot, INOX);
    for (lx, lz) in [(-0.22_f32, -0.20_f32), (0.22, -0.20), (-0.22, 0.20), (0.22, 0.20)] {
        b.push_box_rot(f.at(sx + lx, sz + lz, 0.315), Vec3::new(0.025, 0.63, 0.025), rot, "#9AA2AD");
    }
    b.push_cyl(f.at(sx + 0.06, sz, 0.78), 0.165, 0.15, 0.24, 12, "#D8DCE1");
    b.push_cyl(f.at(sx - 0.17, sz - 0.10, 0.73), 0.035, 0.035, 0.14, 8, "#7A1F12");
    b.push_cyl(f.at(sx - 0.17, sz + 0.04, 0.72), 0.03, 0.03, 0.12, 8, "#C8261B");

    // 2 stainless tables + trà đá jugs + 10 red & blue plastic stools + market umbrella
    let tables = [(0.35_f32, 0.20_f32, 1.30_f32), (2.15, 0.05, 0.90)];
    for &(tx, tz, tw) in &tables {
        b.push_box_rot(f.at(tx, tz, TOP), Vec3::new(tw, 0.03, 0.62), rot, INOX);
        for (lx, lz) in [(-1.0_f32, -1.0_f32), (-1.0, 1.0), (1.0, -1.0), (1.0, 1.0)] {
            b.push_box_rot(f.at(tx + lx * (tw * 0.5 - 0.05), tz + lz * 0.26, TOP * 0.5), Vec3::new(0.03, TOP, 0.03), rot, "#9AA2AD");
        }
        b.push_cyl(f.at(tx + tw * 0.5 - 0.09, tz, TOP + 0.115), 0.07, 0.065, 0.20, 10, "#B9772E");
        b.push_cyl(f.at(tx - tw * 0.5 + 0.08, tz - 0.08, TOP + 0.075), 0.035, 0.035, 0.12, 8, "#E8E2D0");

        if is_friday {
            // Friday Bún đậu mắm tôm: bamboo mẹt trays on banana leaf + fried tofu + purple mắm tôm bowl!
            for dx in [-0.26_f32, 0.22] {
                if dx.abs() < tw * 0.42 {
                    b.push_cyl(f.at(tx + dx, tz, TOP + 0.025), 0.19, 0.18, 0.02, 12, "#C9A86A");
                    b.push_cyl(f.at(tx + dx, tz, TOP + 0.038), 0.17, 0.17, 0.008, 12, "#4E8B3A");
                    b.push_box_rot(f.at(tx + dx - 0.06, tz - 0.03, TOP + 0.06), Vec3::new(0.09, 0.035, 0.09), rot, "#F7F5EE"); // bún lá
                    b.push_box_rot(f.at(tx + dx + 0.05, tz + 0.03, TOP + 0.06), Vec3::new(0.08, 0.035, 0.08), rot, "#E8B03C"); // đậu rán
                    b.push_cyl(f.at(tx + dx + 0.08, tz - 0.08, TOP + 0.06), 0.04, 0.035, 0.035, 8, "#6E3A78"); // mắm tôm
                }
            }
        }
    }
    if is_friday {
        // Frying tofu wok beside cart on Fridays
        b.push_cyl(f.at(cx - 0.85, cz + 0.55, 0.36), 0.24, 0.16, 0.14, 12, "#2B2F36");
        b.push_cyl(f.at(cx - 0.85, cz + 0.55, 0.43), 0.21, 0.21, 0.015, 12, "#E8B838");
    }

    let stools = [
        (0.02_f32, -0.36_f32), (0.68, -0.36), (0.02, 0.76), (0.68, 0.76),
        (1.91, -0.51), (2.39, -0.51), (1.91, 0.61), (2.39, 0.61),
    ];
    for (i, &(stx, stz)) in stools.iter().enumerate() {
        let col = if i % 2 == 0 { "#D62F2F" } else { "#2E6FD1" };
        b.push_cyl(f.at(stx, stz, STOOL_H * 0.5), 0.15, 0.18, STOOL_H, 10, col);
    }

    // Big red-and-white market umbrella over second table
    let (ux, uz) = (2.72, -0.35);
    b.push_cyl(f.at(ux, uz, 1.15), 0.022, 0.022, 2.30, 8, "#E8E8E8");
    b.push_cyl(f.at(ux, uz, 0.03), 0.18, 0.20, 0.06, 10, "#3A3F47");
    b.push_cyl(f.at(ux, uz, 2.36), 0.06, 1.12, 0.36, 12, "#C8102E");

    // A-frame menu stand frame on pavement
    let (ax, az) = (-0.55, 1.55);
    b.push_box_rot(f.at(ax, az, 0.46), Vec3::new(0.62, 0.86, 0.06), rot * Quat::from_rotation_x(-0.18), "#2B2F36");

    let solid_mat = ba.mat(materials, "#FFFFFF", 0.62, 0.15);
    b.spawn(commands, meshes, ba, solid_mat);

    // Upload single shared half-res (512×256) cơm bình dân atlas and mount UV-cropped quads!
    let com_mat = ba.textured(
        images,
        materials,
        draw_com_binh_dan_atlas(fonts).downsample_half(),
        false,
        0.50,
    );

    // Overhead sign [0, 0, 512, 160]
    spawn_backdrop_uv_quad(
        commands,
        meshes,
        ba,
        com_mat.clone(),
        1024.0,
        512.0,
        [0.0, 0.0, 512.0, 160.0],
        f.at(cx, cz + 0.49, 2.32),
        Vec2::new(1.56, 0.48),
        rot,
    );

    // A-frame menu board: weekday [512, 0, 256, 256] vs Friday Bún đậu [768, 0, 256, 256]
    let board_rect = if is_friday {
        [768.0, 0.0, 256.0, 256.0]
    } else {
        [512.0, 0.0, 256.0, 256.0]
    };
    spawn_backdrop_uv_quad(
        commands,
        meshes,
        ba,
        com_mat.clone(),
        1024.0,
        512.0,
        board_rect,
        f.at(ax, az + 0.04, 0.48),
        Vec2::new(0.58, 0.78),
        rot * Quat::from_rotation_x(-0.18),
    );

    // 8 UV-cropped dish tops lying horizontally inside the glass cabinet trays
    let tray_rot = f.horizontal_rot();
    for (tx, ty, dish_k) in tray_slots {
        spawn_backdrop_uv_quad(
            commands,
            meshes,
            ba,
            com_mat.clone(),
            1024.0,
            512.0,
            [(dish_k % 8) as f32 * 128.0, 256.0, 128.0, 96.0],
            f.at(tx, cz, ty),
            Vec2::new(0.25, 0.20),
            tray_rot,
        );
    }

    // Animated rising steam puffs over canh chua & rice pot
    let puff_mesh = ba.add_mesh(meshes, Mesh::from(Sphere::new(0.11)));
    let puff_mat = ba.mat_blend(materials, "#F4F4F4", 0.42, 0.9, true);
    for k in 0..3 {
        let base_pos = f.at(sx + 0.06, sz, 0.92);
        commands.spawn((
            WorldEntity,
            BackdropEntity,
            ComSteamPuff {
                base_pos,
                phase: k as f32 / 3.0,
            },
            Mesh3d(puff_mesh.clone()),
            MeshMaterial3d(puff_mat.clone()),
            Transform::from_translation(base_pos),
        ));
    }

    commands.spawn((
        WorldEntity,
        BackdropEntity,
        EatComStall {
            is_friday_bun_dau: is_friday,
        },
        Transform::from_translation(f.origin),
    ));

    Some(f.origin)
}

pub fn animate_eat_com_steam_system(
    time: Res<Time>,
    mut puffs: Query<(&ComSteamPuff, &mut Transform)>,
) {
    let t = time.elapsed_secs();
    for (p, mut tf) in &mut puffs {
        let u = (t * 0.45 + p.phase).fract();
        tf.translation = p.base_pos
            + Vec3::new((t + p.phase * 6.0).sin() * 0.06 * u, u * 0.65, (t * 0.8 + p.phase * 6.0).cos() * 0.05 * u);
        let s = 0.45 + u * 0.85;
        tf.scale = Vec3::splat(s);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::backdrops::BackdropEntity;
    use crate::layout::{LayoutSpec, OfficeLayout};
    use crate::street::city_grid::spawn_city_grid;
    use std::path::PathBuf;

    #[test]
    fn eat_com_spawns_weekday_com_and_friday_bun_dau_swap_with_zero_leaks() {
        let assets_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../assets");
        let spec = LayoutSpec::for_layout(OfficeLayout::Pro);
        let mut fonts = FontEngine::from_assets_dir(&assets_dir);

        for is_friday in [false, true] {
            let mut world = World::new();
            world.insert_resource(Assets::<Mesh>::default());
            world.insert_resource(Assets::<StandardMaterial>::default());
            world.insert_resource(Assets::<Image>::default());
            world.insert_resource(BackdropAssets::default());

            let placed = world.resource_scope(|world, mut meshes: Mut<Assets<Mesh>>| {
                world.resource_scope(|world, mut mats: Mut<Assets<StandardMaterial>>| {
                    world.resource_scope(|world, mut imgs: Mut<Assets<Image>>| {
                        world.resource_scope(|world, mut ba: Mut<BackdropAssets>| {
                            let mut cmds = world.commands();
                            let grid = spawn_city_grid(&mut cmds, &mut meshes, &mut mats, &mut imgs, &mut ba, &spec, false);
                            let mut finder = SpotFinder::new(grid.office_mid, grid.parks, grid.plazas, grid.trees);
                            spawn_eat_com(&mut cmds, &mut meshes, &mut mats, &mut imgs, &mut ba, &mut fonts, &mut finder, is_friday)
                        })
                    })
                })
            });
            world.flush();

            assert!(placed.is_some(), "eat_com must place on park edge (friday={is_friday})");
            let stall = world.query::<&EatComStall>().single(&world).expect("EatComStall marker");
            assert_eq!(stall.is_friday_bun_dau, is_friday);
            assert!(world.query::<&ComSteamPuff>().iter(&world).count() >= 3);

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
}
