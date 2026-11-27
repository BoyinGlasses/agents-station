//! Remaining 8 Vietnamese street life modules: *CSGT* checkpoint (`street-csgt`), morning
//! *xôi/bánh cuốn* (`street-morning`), *xe ôm*/hammock (`street-corner`), barber/tire-repair
//! (`street-repair`), lunar votive bins (`street-lunar`), *công viên dưỡng sinh/đá cầu/cờ tướng*
//! (`street-park`), evening *quán nhậu* (`street-nhau`), and weekend *rạp cưới* (`street-wedding`).

use crate::backdrops::{BackdropAssets, BackdropEntity};
use crate::palette::WorldEntity;
use crate::street::city_grid::{CityGridData, STREET_W, ShopDir};
use crate::street::{GY, LAWN_Y, PAVE_Y, SpotFinder, StreetMeshBuilder, spawn_backdrop_uv_quad};
use bevy::prelude::*;
use std::f32::consts::{FRAC_PI_2, PI};

#[derive(Component, Debug, Clone, Copy, PartialEq, Eq)]
pub enum StreetCultureScene {
    CsgtCheckpoint,
    MorningVendors,
    XeOmHammock,
    BarberRepair,
    LunarVotive,
    ParkActivities,
    QuanNhau,
    WeddingTent,
}

#[derive(Component)]
pub struct LunarFlame {
    pub base_scale: Vec3,
    pub phase: f32,
}

#[derive(Component)]
pub struct HammockSway {
    pub base_rot: Quat,
}

#[derive(Component)]
pub struct ParkShuttle {
    pub a: Vec3,
    pub b: Vec3,
    pub peak: f32,
    pub speed: f32,
}

/// Marker on Quán Nhậu overhead string bulbs so Day↔Night toggling updates their glow live.
#[derive(Component)]
pub struct NhauBulbNight;

fn push_street_person(b: &mut StreetMeshBuilder, pos: Vec3, yaw: f32, shirt: &str, pants: &str, hat: Option<&str>) {
    let rot = Quat::from_rotation_y(yaw);
    b.push_box_rot(pos + rot * Vec3::new(0.0, 0.35, -0.08), Vec3::new(0.13, 0.70, 0.13), rot, pants);
    b.push_box_rot(pos + rot * Vec3::new(0.0, 0.35, 0.08), Vec3::new(0.13, 0.70, 0.13), rot, pants);
    b.push_box_rot(pos + rot * Vec3::new(0.0, 0.96, 0.0), Vec3::new(0.24, 0.52, 0.36), rot, shirt);
    b.push_sphere(pos + Vec3::new(0.0, 1.38, 0.0), Vec3::splat(0.125), "#E8C2A0");
    if let Some(hcol) = hat {
        b.push_cyl(pos + Vec3::new(0.0, 1.54, 0.0), 0.02, 0.22, 0.16, 10, hcol);
    } else {
        b.push_sphere(pos + Vec3::new(0.0, 1.43, 0.0), Vec3::new(0.13, 0.09, 0.13), "#241E1A");
    }
}

fn push_parked_scooter(b: &mut StreetMeshBuilder, pos: Vec3, yaw: f32, paint: &str) {
    let rot = Quat::from_rotation_y(yaw);
    b.push_box_rot(pos + rot * Vec3::new(-0.25, 0.56, 0.0), Vec3::new(0.95, 0.32, 0.34), rot, paint);
    b.push_box_rot(pos + rot * Vec3::new(0.45, 0.74, 0.0), Vec3::new(0.20, 0.62, 0.40), rot, paint);
    b.push_box_rot(pos + rot * Vec3::new(-0.22, 0.78, 0.0), Vec3::new(0.70, 0.10, 0.30), rot, "#1C1E22");
    for x in [-0.52_f32, 0.52] {
        b.push_cyl_rot(
            pos + rot * Vec3::new(x, 0.25, 0.0),
            0.25,
            0.25,
            0.10,
            10,
            rot * Quat::from_rotation_x(FRAC_PI_2),
            "#16181C",
        );
    }
}

#[allow(clippy::too_many_arguments)]
pub fn spawn_street_scenes_b(
    commands: &mut Commands,
    meshes: &mut Assets<Mesh>,
    materials: &mut Assets<StandardMaterial>,
    ba: &mut BackdropAssets,
    grid: &CityGridData,
    finder: &mut SpotFinder,
    signs_mat: Handle<StandardMaterial>,
    night: bool,
) {
    let mut b = StreetMeshBuilder::default();

    // ==================== 1. CSGT TRAFFIC POLICE CHECKPOINT (`street-csgt.js`) ====================
    let csgt_z = if grid.layout.is_hq() {
        grid.sb.z1 + STREET_W + 1.25
    } else {
        grid.sb.z1 - 0.65
    };
    let csgt_pos = Vec3::new(grid.sb.x1 - 1.2, PAVE_Y, csgt_z);
    for dx in [-1.8_f32, -0.6, 0.6, 1.8] {
        // Orange traffic cones along the kerb
        b.push_cyl(csgt_pos + Vec3::new(dx, 0.22, 0.35), 0.04, 0.16, 0.44, 8, "#F26419");
        b.push_cyl(csgt_pos + Vec3::new(dx, 0.26, 0.35), 0.09, 0.11, 0.08, 8, "#F4F6F8");
    }
    // 2 white C.S.G.T patrol motorbikes + 2 yellow-uniform officers (reflective vest at night)
    push_parked_scooter(&mut b, csgt_pos + Vec3::new(-1.3, 0.0, -0.35), 0.15, "#F2F4F6");
    push_parked_scooter(&mut b, csgt_pos + Vec3::new(-0.3, 0.0, -0.35), -0.10, "#F2F4F6");
    let officer_top = if night { "#C9F23A" } else { "#D3AF48" };
    push_street_person(&mut b, csgt_pos + Vec3::new(0.85, 0.0, 0.0), FRAC_PI_2, officer_top, "#D3AF48", Some("#D3AF48"));
    // Traffic baton in officer's hand
    b.push_cyl(csgt_pos + Vec3::new(1.05, 1.28, 0.12), 0.022, 0.022, 0.44, 6, "#C8102E");
    push_street_person(&mut b, csgt_pos + Vec3::new(-0.8, 0.0, -0.85), 0.4, officer_top, "#D3AF48", Some("#D3AF48"));
    // Checkpoint A-frame sign [0, 256, 256, 128]
    b.push_box(csgt_pos + Vec3::new(1.65, 0.48, -0.15), Vec3::new(0.84, 0.56, 0.06), "#1C4FA6");
    spawn_backdrop_uv_quad(
        commands, meshes, ba, signs_mat.clone(), 512.0, 512.0, [0.0, 256.0, 256.0, 128.0],
        csgt_pos + Vec3::new(1.65, 0.48, -0.11), Vec2::new(0.80, 0.48), Quat::IDENTITY,
    );
    commands.spawn((WorldEntity, BackdropEntity, StreetCultureScene::CsgtCheckpoint, Transform::from_translation(csgt_pos)));

    // ==================== 2. MORNING XÔI & BÁNH CUỐN (`street-morning.js`) ====================
    if let Some(f) = finder
        .find(3.8, 1.2, 1.95, true, 52.0)
        .or_else(|| finder.find(3.8, 1.2, 1.95, true, 78.0))
    {
        let rot = f.face_out_rot();
        // Gánh xôi: 2 woven baskets with sticky rice & banana leaf + bamboo shoulder pole
        for bx in [-1.55_f32, -0.75] {
            b.push_cyl(f.at(bx, 0.18, 0.11), 0.24, 0.20, 0.22, 10, "#B98A4E");
            b.push_sphere(f.at(bx, 0.18, 0.22), Vec3::new(0.21, 0.09, 0.21), "#F4F1E6");
            b.push_sphere(f.at(bx + 0.03, 0.16, 0.25), Vec3::new(0.18, 0.05, 0.15), "#4E8B3A");
        }
        b.push_box_rot(f.at(-1.15, -0.52, 0.04), Vec3::new(1.75, 0.04, 0.05), rot, "#C9A86A");
        // Bánh cuốn brazier + steaming pot + low blue plastic table & red/blue stools
        b.push_cyl(f.at(0.55, -0.12, 0.13), 0.19, 0.22, 0.26, 10, "#3B3F45");
        b.push_cyl(f.at(0.55, -0.12, 0.36), 0.27, 0.25, 0.22, 12, "#C9CED6");
        b.push_cyl(f.at(0.55, -0.12, 0.48), 0.25, 0.25, 0.015, 12, "#F4F4F4");
        b.push_box_rot(f.at(1.38, 0.12, 0.31), Vec3::new(0.54, 0.04, 0.46), rot, "#2F6FB5");
        b.push_cyl(f.at(1.38, -0.40, 0.12), 0.13, 0.11, 0.24, 8, "#2E6FD1");
        b.push_cyl(f.at(1.92, 0.12, 0.12), 0.13, 0.11, 0.24, 8, "#D62F2F");
        push_street_person(&mut b, f.at(-1.15, -0.30, 0.0), f.yaw, "#8E5B9A", "#2B2B2B", Some("#E3C88B"));
        push_street_person(&mut b, f.at(0.55, -0.58, 0.0), f.yaw, "#E8D9C4", "#5B4B3B", None);
        // UV-cropped "XÔI · BÁNH CUỐN" sign [0, 0, 256, 128]
        spawn_backdrop_uv_quad(
            commands, meshes, ba, signs_mat.clone(), 512.0, 512.0, [0.0, 0.0, 256.0, 128.0],
            f.at(-0.25, 0.56, 0.26), Vec2::new(0.62, 0.30), rot * Quat::from_rotation_x(-0.28),
        );
        commands.spawn((WorldEntity, BackdropEntity, StreetCultureScene::MorningVendors, Transform::from_translation(f.origin)));
    }

    // ==================== 3. XE ÔM CORNER & SWAYING HAMMOCK (`street-corner.js`) ====================
    if let Some(f) = finder
        .find(3.4, 0.8, 0.65, false, 52.0)
        .or_else(|| finder.find(3.4, 0.8, 0.65, false, 78.0))
    {
        for (k, (bx, col)) in [(-1.1_f32, "#7A1F2B"), (0.0, "#2B3A67"), (1.1, "#3B3B3B")].iter().enumerate() {
            push_parked_scooter(&mut b, f.at(*bx, 0.0, 0.0), f.yaw + (k as f32 - 1.0) * 0.12, col);
            push_street_person(&mut b, f.at(*bx - 0.15, -0.45, 0.0), f.yaw, "#8C7B5A", "#4A4A52", Some("#6B7A4B"));
        }
        // Swaying striped hammock (võng) slung between two park trees or wooden posts
        let h_center = f.at(0.0, -2.1, 0.72);
        b.push_cyl(f.at(-1.25, -2.1, 0.65), 0.06, 0.08, 1.30, 6, "#5B4433");
        b.push_cyl(f.at(1.25, -2.1, 0.65), 0.06, 0.08, 1.30, 6, "#5B4433");
        let mut ham_b = StreetMeshBuilder::default();
        ham_b.push_box(Vec3::new(0.0, -0.05, 0.0), Vec3::new(2.15, 0.08, 0.68), "#2E6FD1");
        ham_b.push_box(Vec3::new(0.0, 0.0, 0.0), Vec3::new(1.85, 0.03, 0.22), "#F2C230");
        ham_b.push_bar(Vec3::new(-1.22, 0.45, 0.0), Vec3::new(-1.02, -0.02, 0.0), 0.015, 4, "#C9A86A");
        ham_b.push_bar(Vec3::new(1.22, 0.45, 0.0), Vec3::new(1.02, -0.02, 0.0), 0.015, 4, "#C9A86A");
        let ham_mesh = ba.add_mesh(meshes, ham_b.build());
        let ham_mat = ba.mat(materials, "#FFFFFF", 0.85, 0.0);
        let base_rot = f.face_out_rot();
        commands.spawn((
            WorldEntity,
            BackdropEntity,
            HammockSway { base_rot },
            Mesh3d(ham_mesh),
            MeshMaterial3d(ham_mat),
            Transform::from_translation(h_center).with_rotation(base_rot),
        ));
        commands.spawn((WorldEntity, BackdropEntity, StreetCultureScene::XeOmHammock, Transform::from_translation(f.origin)));
    }

    // ==================== 4. SIDEWALK BARBER & VÁ XE TIRE REPAIR (`street-repair.js`) ====================
    if let Some(f) = finder
        .find(4.6, 1.3, 2.0, true, 56.0)
        .or_else(|| finder.find(4.6, 1.3, 2.0, true, 82.0))
    {
        let rot = f.face_out_rot();
        // Barber mirror on post + old red metal barber chair + waiting stool
        b.push_box_rot(f.at(-1.75, 0.0, 0.95), Vec3::new(0.10, 1.90, 0.10), rot, "#7A5A3A");
        b.push_box_rot(f.at(-1.68, 0.0, 1.45), Vec3::new(0.04, 0.48, 0.38), rot, "#8A6A4A");
        b.push_box_rot(f.at(-1.65, 0.0, 1.45), Vec3::new(0.02, 0.40, 0.30), rot, "#D3E6F0");
        b.push_box_rot(f.at(-1.05, 0.0, 0.36), Vec3::new(0.42, 0.06, 0.42), rot, "#B5122B");
        b.push_box_rot(f.at(-0.84, 0.0, 0.62), Vec3::new(0.06, 0.48, 0.40), rot, "#B5122B");
        push_street_person(&mut b, f.at(-0.45, 0.0, 0.0), f.yaw + PI, "#F2F4F7", "#2B2F36", None);

        // Sửa xe / Vá xe: stacked black tires, red air compressor, water basin, scooter on stand
        for k in 0..3 {
            b.push_cyl(f.at(0.55, -0.25, 0.08 + k as f32 * 0.16), 0.24, 0.24, 0.14, 12, "#1C1E22");
        }
        b.push_cyl_rot(f.at(1.15, -0.35, 0.26), 0.16, 0.16, 0.56, 10, rot * Quat::from_rotation_z(FRAC_PI_2), "#C8102E");
        b.push_cyl(f.at(1.15, 0.25, 0.10), 0.22, 0.18, 0.18, 10, "#2E86DE");
        push_parked_scooter(&mut b, f.at(1.80, 0.15, 0.0), f.yaw, "#D9534F");
        // UV-cropped "VÁ XE" board [256, 0, 256, 128]
        b.push_box_rot(f.at(1.55, 0.72, 0.42), Vec3::new(0.76, 0.44, 0.05), rot, "#1E2125");
        spawn_backdrop_uv_quad(
            commands, meshes, ba, signs_mat.clone(), 512.0, 512.0, [256.0, 0.0, 256.0, 128.0],
            f.at(1.55, 0.76, 0.42), Vec2::new(0.72, 0.38), rot,
        );
        commands.spawn((WorldEntity, BackdropEntity, StreetCultureScene::BarberRepair, Transform::from_translation(f.origin)));
    }

    // ==================== 5. LUNAR VOTIVE BURNING BIN (`street-lunar.js`) ====================
    let lunar_origin = if let Some(pf) = grid.plain_fronts.first() {
        Vec3::new(pf.x + if pf.dir == ShopDir::PosX { 0.85 } else { 0.0 }, PAVE_Y, pf.z + if pf.dir == ShopDir::PosZ { 0.85 } else { 0.0 })
    } else {
        Vec3::new(grid.sb.x0 + 1.4, PAVE_Y, grid.sb.z1 - 0.75)
    };
    // Perforated steel drum on 3 bricks + gold/red votive bundles + red offering table with fruit & incense
    b.push_cyl(lunar_origin + Vec3::new(0.0, 0.28, 0.0), 0.21, 0.19, 0.42, 12, "#4A4C50");
    for (bx, bz) in [(-0.12_f32, -0.08_f32), (0.12, -0.08), (0.0, 0.13)] {
        b.push_box(lunar_origin + Vec3::new(bx, 0.04, bz), Vec3::new(0.10, 0.08, 0.10), "#A0522D");
    }
    b.push_box(lunar_origin + Vec3::new(0.42, 0.08, 0.0), Vec3::new(0.24, 0.14, 0.17), "#E8C14A");
    // Red offering table
    let tab = lunar_origin + Vec3::new(-0.72, 0.0, 0.0);
    b.push_box(tab + Vec3::new(0.0, 0.50, 0.0), Vec3::new(0.50, 0.03, 0.36), "#B5122B");
    for (lx, lz) in [(-0.21_f32, -0.15_f32), (-0.21, 0.15), (0.21, -0.15), (0.21, 0.15)] {
        b.push_box(tab + Vec3::new(lx, 0.25, lz), Vec3::new(0.03, 0.50, 0.03), "#8E0B1E");
    }
    b.push_cyl(tab + Vec3::new(-0.12, 0.54, 0.0), 0.11, 0.09, 0.03, 10, "#F2F2F2");
    b.push_sphere(tab + Vec3::new(-0.12, 0.60, 0.0), Vec3::splat(0.055), "#F28C28");
    b.push_cyl(tab + Vec3::new(0.12, 0.60, 0.04), 0.04, 0.05, 0.16, 8, "#3A6FB0");
    b.push_sphere(tab + Vec3::new(0.12, 0.72, 0.04), Vec3::splat(0.075), "#F2C200");
    // Animated additive flickering flame cones in the votive drum
    let flame_mesh = ba.add_mesh(meshes, Mesh::from(Cone::new(0.08, 0.38)));
    let flame_mat_a = ba.mat_emissive(materials, "#FF8A24", 2.8);
    let flame_mat_b = ba.mat_emissive(materials, "#FFD84A", 2.8);
    for k in 0..4 {
        let ang = (k as f32 / 4.0) * PI * 2.0;
        let mat = if k % 2 == 0 { flame_mat_a.clone() } else { flame_mat_b.clone() };
        commands.spawn((
            WorldEntity,
            BackdropEntity,
            LunarFlame {
                base_scale: Vec3::ONE,
                phase: k as f32 * 1.4,
            },
            Mesh3d(flame_mesh.clone()),
            MeshMaterial3d(mat),
            Transform::from_translation(lunar_origin + Vec3::new(ang.cos() * 0.07, 0.62, ang.sin() * 0.07)),
        ));
    }
    commands.spawn((WorldEntity, BackdropEntity, StreetCultureScene::LunarVotive, Transform::from_translation(lunar_origin)));

    // ==================== 6. PARK ACTIVITIES: DƯỠNG SINH, ĐÁ CẦU, BADMINTON & CỜ TƯỚNG (`street-park.js`) ====================
    let park_center = if let Some(plaza) = grid.plazas.first() {
        Vec3::new((plaza[0] + plaza[1]) * 0.5, GY + 0.20, (plaza[2] + plaza[3]) * 0.5)
    } else {
        Vec3::new(grid.office_mid.x + 14.0, LAWN_Y, grid.office_mid.y + 14.0)
    };
    // Stone park benches around square
    for &(dx, dz, bw, bd) in &[
        (0.0_f32, -3.6_f32, 1.8_f32, 0.42_f32),
        (0.0, 3.6, 1.8, 0.42),
        (-3.6, 0.0, 0.42, 1.8),
        (3.6, 0.0, 0.42, 1.8),
    ] {
        b.push_box(park_center + Vec3::new(dx, 0.22, dz), Vec3::new(bw, 0.44, bd), "#C9C5BC");
    }
    // Morning dưỡng sinh exercise line + bluetooth speaker
    for k in 0..4 {
        push_street_person(&mut b, park_center + Vec3::new(-2.4 + k as f32 * 1.1, 0.0, -2.0), FRAC_PI_2, "#F2F2F2", "#363D4C", None);
    }
    b.push_box(park_center + Vec3::new(-0.8, 0.14, -1.2), Vec3::new(0.24, 0.28, 0.18), "#22262E");
    // Badminton / đá cầu net + 2 players + animated flying shuttlecock
    b.push_cyl(park_center + Vec3::new(0.0, 0.78, -1.5), 0.025, 0.025, 1.56, 6, "#2B2F36");
    b.push_cyl(park_center + Vec3::new(0.0, 0.78, 1.5), 0.025, 0.025, 1.56, 6, "#2B2F36");
    b.push_box(park_center + Vec3::new(0.0, 1.48, 0.0), Vec3::new(0.03, 0.05, 3.0), "#F4F6F8");
    push_street_person(&mut b, park_center + Vec3::new(-2.1, 0.0, 0.5), 0.0, "#2A9D8F", "#2F3A4C", None);
    push_street_person(&mut b, park_center + Vec3::new(2.1, 0.0, 0.5), PI, "#E8742A", "#2F3A4C", None);

    let shuttle_mesh = ba.add_mesh(meshes, Mesh::from(Cone::new(0.075, 0.13)));
    let shuttle_mat = ba.mat_emissive(materials, "#FFFFFF", 0.7);
    commands.spawn((
        WorldEntity,
        BackdropEntity,
        ParkShuttle {
            a: park_center + Vec3::new(-1.85, 1.25, 0.5),
            b: park_center + Vec3::new(1.85, 1.25, 0.5),
            peak: 1.55,
            speed: 1.35,
        },
        Mesh3d(shuttle_mesh),
        MeshMaterial3d(shuttle_mat),
        Transform::from_translation(park_center + Vec3::new(0.0, 2.2, 0.5)),
    ));

    // Cờ tướng (Chinese chess) low table + 2 seated elders + trà đá sign [256, 384, 256, 128]
    let chess = park_center + Vec3::new(2.2, 0.0, 2.2);
    b.push_box(chess + Vec3::new(0.0, 0.44, 0.0), Vec3::new(0.68, 0.05, 0.62), "#2F6FB5");
    b.push_box(chess + Vec3::new(0.0, 0.475, 0.0), Vec3::new(0.46, 0.012, 0.42), "#E8C88A");
    push_street_person(&mut b, chess + Vec3::new(-0.55, 0.0, 0.0), 0.0, "#E8D9C4", "#4A4A52", None);
    push_street_person(&mut b, chess + Vec3::new(0.55, 0.0, 0.0), PI, "#DCE6EE", "#363D4C", None);
    spawn_backdrop_uv_quad(
        commands, meshes, ba, signs_mat.clone(), 512.0, 512.0, [256.0, 384.0, 256.0, 128.0],
        chess + Vec3::new(0.0, 0.45, 0.68), Vec2::new(0.68, 0.34), Quat::IDENTITY,
    );
    commands.spawn((WorldEntity, BackdropEntity, StreetCultureScene::ParkActivities, Transform::from_translation(park_center)));

    // ==================== 7. EVENING QUÁN NHẬU (`street-nhau.js`) ====================
    if let Some(f) = finder
        .find(5.4, 2.2, 2.4, true, 56.0)
        .or_else(|| finder.find(5.4, 2.2, 2.4, true, 84.0))
    {
        let rot = f.face_out_rot();
        // 3 low blue plastic tables + red/blue stools + ice bucket & green beer crates + charcoal grill
        for tx in [-1.5_f32, 0.0, 1.5] {
            b.push_box_rot(f.at(tx, 0.15, 0.58), Vec3::new(1.05, 0.04, 0.65), rot, "#2F6FB5");
            b.push_cyl(f.at(tx, 0.15, 0.72), 0.11, 0.09, 0.22, 10, "#2E86DE"); // ice bucket
            b.push_cyl(f.at(tx + 0.25, 0.18, 0.68), 0.14, 0.12, 0.12, 10, "#C9CED6"); // steaming hotpot
            for (dx, dz, col) in [(-0.28_f32, -0.52_f32, "#D62F2F"), (0.28, -0.52, "#2E6FD1"), (-0.28, 0.52, "#2E6FD1"), (0.28, 0.52, "#D62F2F")] {
                b.push_cyl(f.at(tx + dx, 0.15 + dz, 0.20), 0.15, 0.17, 0.40, 8, col);
            }
        }
        // Beer crates + charcoal grill smoking at end
        b.push_box_rot(f.at(-2.45, -0.25, 0.18), Vec3::new(0.45, 0.34, 0.34), rot, "#1E9E4A");
        b.push_box_rot(f.at(-2.45, -0.25, 0.52), Vec3::new(0.45, 0.34, 0.34), rot, "#C8102E");
        b.push_box_rot(f.at(2.45, 0.45, 0.65), Vec3::new(0.75, 0.22, 0.38), rot, "#3B3F45");
        b.push_box_rot(f.at(2.45, 0.45, 0.78), Vec3::new(0.65, 0.04, 0.30), rot, "#FF6A24");
        // UV-cropped "QUÁN NHẬU · BIA HƠI" sign [0, 128, 256, 128]
        b.push_box_rot(f.at(-1.75, 1.15, 0.75), Vec3::new(1.24, 0.62, 0.06), rot, "#1F4FA0");
        spawn_backdrop_uv_quad(
            commands, meshes, ba, signs_mat.clone(), 512.0, 512.0, [0.0, 128.0, 256.0, 128.0],
            f.at(-1.75, 1.19, 0.75), Vec2::new(1.18, 0.56), rot,
        );
        // Overhead warm string bulbs (dedicated non-cached material so night toggle updates live)
        let bulb_mesh = ba.add_mesh(meshes, Mesh::from(Sphere::new(0.055)));
        let bulb_lin = crate::palette::srgb_hex("#FFD47A").to_linear();
        let bulb_str = if night { 2.6 } else { 0.4 };
        let bulb_mat = ba.add_material(
            materials,
            StandardMaterial {
                base_color: crate::palette::srgb_hex("#FFD47A"),
                emissive: LinearRgba::new(
                    bulb_lin.red * bulb_str,
                    bulb_lin.green * bulb_str,
                    bulb_lin.blue * bulb_str,
                    1.0,
                ),
                perceptual_roughness: 0.3,
                ..default()
            },
        );
        for k in 0..6 {
            let lx = -2.1 + k as f32 * 0.84;
            commands.spawn((
                WorldEntity,
                BackdropEntity,
                NhauBulbNight,
                Mesh3d(bulb_mesh.clone()),
                MeshMaterial3d(bulb_mat.clone()),
                Transform::from_translation(f.at(lx, 0.15, 2.25)),
            ));
        }
        commands.spawn((WorldEntity, BackdropEntity, StreetCultureScene::QuanNhau, Transform::from_translation(f.origin)));
    }

    // ==================== 8. WEEKEND RẠP CƯỚI WEDDING TENT (`street-wedding.js`) ====================
    let wed_pos = if let Some(f) = finder.find(6.0, 2.2, 2.2, true, 64.0) {
        f.origin
    } else {
        Vec3::new(grid.sb.x0 - STREET_W - 6.0, PAVE_Y, grid.sb.z1 - 4.0)
    };
    let eave = 2.28;
    // Tent poles + pink-and-white pleated roof + flower arch with "LỄ VU QUY" sign + 4 banquet tables
    for sx in [-2.8_f32, -1.4, 1.4, 2.8] {
        for sz in [-0.95_f32, 0.95] {
            b.push_cyl(wed_pos + Vec3::new(sx, eave * 0.5, sz), 0.035, 0.035, eave, 8, "#F4F1EA");
        }
    }
    for k in 0..8 {
        let px = -2.62 + k as f32 * 0.75;
        let col = if k % 2 == 0 { "#F7B6C8" } else { "#FFFFFF" };
        b.push_box(wed_pos + Vec3::new(px, eave + 0.18, 0.0), Vec3::new(0.75, 0.28, 2.05), col);
    }
    // Floral arch at entrance
    for sx in [-1.15_f32, 1.15] {
        b.push_cyl(wed_pos + Vec3::new(sx, 1.15, 1.05), 0.14, 0.16, 2.30, 8, "#F7C6D3");
    }
    b.push_box(wed_pos + Vec3::new(0.0, 2.35, 1.05), Vec3::new(2.55, 0.34, 0.24), "#E8476A");
    for k in -4..=4 {
        let fc = if k % 2 == 0 { "#FFFFFF" } else { "#FFE3EA" };
        b.push_sphere(wed_pos + Vec3::new(k as f32 * 0.28, 2.50, 1.14), Vec3::splat(0.14), fc);
    }
    // UV-cropped "LỄ VU QUY" arch banner [256, 128, 256, 128]
    spawn_backdrop_uv_quad(
        commands, meshes, ba, signs_mat, 512.0, 512.0, [256.0, 128.0, 256.0, 128.0],
        wed_pos + Vec3::new(0.0, 2.35, 1.19), Vec2::new(1.75, 0.56), Quat::IDENTITY,
    );
    // 4 round banquet tables with red/pink tablecloths + white bridal car with hood flower bouquet
    for tx in [-2.0_f32, -0.7, 0.7, 2.0] {
        b.push_cyl(wed_pos + Vec3::new(tx, 0.38, 0.0), 0.46, 0.48, 0.76, 12, "#E8476A");
        b.push_cyl(wed_pos + Vec3::new(tx, 0.77, 0.0), 0.47, 0.47, 0.02, 12, "#FFF4F7");
    }
    // Bridal car parked outside arch
    b.push_box(wed_pos + Vec3::new(0.0, 0.58, 2.25), Vec3::new(3.2, 0.56, 1.48), "#F7F8FA");
    b.push_box(wed_pos + Vec3::new(-0.2, 1.08, 2.25), Vec3::new(1.75, 0.46, 1.36), "#23303E");
    b.push_sphere(wed_pos + Vec3::new(1.15, 0.94, 2.25), Vec3::new(0.28, 0.16, 0.34), "#E8476A");
    commands.spawn((WorldEntity, BackdropEntity, StreetCultureScene::WeddingTent, Transform::from_translation(wed_pos)));

    let mat = ba.mat(materials, "#FFFFFF", 0.72, 0.08);
    b.spawn(commands, meshes, ba, mat);
}

/// Zero-allocation Update system animating lunar votive flames, swaying hammock, and park shuttlecock.
pub fn animate_street_scenes_b_system(
    time: Res<Time>,
    mut flames: Query<(&LunarFlame, &mut Transform), (Without<HammockSway>, Without<ParkShuttle>)>,
    mut hammocks: Query<(&HammockSway, &mut Transform), (Without<LunarFlame>, Without<ParkShuttle>)>,
    mut shuttles: Query<(&ParkShuttle, &mut Transform), (Without<LunarFlame>, Without<HammockSway>)>,
) {
    let t = time.elapsed_secs();
    for (fl, mut tf) in &mut flames {
        let s = (0.75 + 0.35 * (t * 9.5 + fl.phase).sin() + 0.15 * (t * 21.0 + fl.phase * 2.0).sin()).max(0.35);
        tf.scale = Vec3::new(fl.base_scale.x, fl.base_scale.y * s, fl.base_scale.z);
        tf.rotation = Quat::from_rotation_z((t * 5.2 + fl.phase).sin() * 0.22);
    }
    for (h, mut tf) in &mut hammocks {
        tf.rotation = h.base_rot * Quat::from_rotation_x((t * 1.4).sin() * 0.16);
    }
    for (sh, mut tf) in &mut shuttles {
        let u = (t * sh.speed).sin() * 0.5 + 0.5;
        let p = sh.a.lerp(sh.b, u);
        let arc = 4.0 * u * (1.0 - u) * sh.peak;
        tf.translation = Vec3::new(p.x, p.y + arc, p.z);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::backdrops::BackdropEntity;
    use crate::layout::{LayoutSpec, OfficeLayout};
    use crate::street::city_grid::spawn_city_grid;

    #[test]
    fn scenes_b_spawns_all_8_cultural_street_modules_and_purges_zero_leaks() {
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
                        let mut finder = SpotFinder::new(grid.office_mid, grid.parks.clone(), grid.plazas.clone(), grid.trees.clone());
                        let signs_mat = ba.mat(&mut mats, "#FFFFFF", 0.5, 0.0);
                        spawn_street_scenes_b(&mut cmds, &mut meshes, &mut mats, &mut ba, &grid, &mut finder, signs_mat, false);
                    });
                });
            });
        });
        world.flush();

        let kinds: Vec<StreetCultureScene> = world.query::<&StreetCultureScene>().iter(&world).copied().collect();
        for expected in [
            StreetCultureScene::CsgtCheckpoint,
            StreetCultureScene::MorningVendors,
            StreetCultureScene::XeOmHammock,
            StreetCultureScene::BarberRepair,
            StreetCultureScene::LunarVotive,
            StreetCultureScene::ParkActivities,
            StreetCultureScene::QuanNhau,
            StreetCultureScene::WeddingTent,
        ] {
            assert!(kinds.contains(&expected), "missing street culture scene {expected:?}");
        }
        assert!(world.query::<&LunarFlame>().iter(&world).count() >= 3);
        assert!(world.query::<&HammockSway>().iter(&world).count() >= 1);
        assert!(world.query::<&ParkShuttle>().iter(&world).count() >= 1);

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
