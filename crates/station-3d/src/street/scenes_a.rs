//! UV-mapped 3D street landmark modules: Vietcombank ATM (`street-atm`), V-Green EV swap
//! cabinet + e-scooter row with 5-bar battery gauges (`street-ev`), Korean Photo Studio
//! (`street-photo`), and 20×44ft Pickleball court + doubles rally (`street-pickle`).

use crate::backdrops::{BackdropAssets, BackdropEntity};
use crate::palette::WorldEntity;
use crate::street::city_grid::{CityGridData, STREET_W};
use crate::street::{GY, PAVE_Y, SpotFinder, StreetMeshBuilder, spawn_backdrop_uv_quad};
use bevy::prelude::*;
use station_canvas2d::FontEngine;
use station_canvas2d::street_surfaces::{
    draw_atm_atlas, draw_ev_cabinet_atlas, draw_photo_studio_atlas, draw_pickleball_atlas,
};
use std::f32::consts::{FRAC_PI_2, PI};

// USA Pickleball official court dimensions in metres (`street-pickle.js:30`)
const PB_W: f32 = 6.096;
const PB_L: f32 = 13.411;
const PB_K: f32 = 2.134;
const PB_LINE: f32 = 0.055;
const PB_TOP: f32 = 0.22;

#[derive(Component)]
pub struct StreetAtmBooth;

#[derive(Component)]
pub struct StreetEvStation;

#[derive(Component)]
pub struct StreetEvScooterRow {
    pub center: Vec3,
    pub half_extents: Vec2,
}

#[derive(Component)]
pub struct StreetPhotoStudio;

#[derive(Component)]
pub struct StreetPhotoFlash {
    pub phase: f32,
}

#[derive(Component)]
pub struct StreetPickleCourt;

#[derive(Component)]
pub struct PickleRallyBall {
    pub origin: Vec3,
    pub axis_x: bool,
    pub court_y: f32,
}

fn push_figure(b: &mut StreetMeshBuilder, pos: Vec3, yaw: f32, shirt: &str, pants: &str, hair: &str) {
    let rot = Quat::from_rotation_y(yaw);
    b.push_box_rot(pos + rot * Vec3::new(0.0, 0.36, -0.09), Vec3::new(0.13, 0.72, 0.13), rot, pants);
    b.push_box_rot(pos + rot * Vec3::new(0.0, 0.36, 0.09), Vec3::new(0.13, 0.72, 0.13), rot, pants);
    b.push_box_rot(pos + rot * Vec3::new(0.0, 0.98, 0.0), Vec3::new(0.24, 0.54, 0.36), rot, shirt);
    b.push_sphere(pos + Vec3::new(0.0, 1.40, 0.0), Vec3::splat(0.125), "#E8C2A0");
    b.push_sphere(pos + Vec3::new(0.0, 1.45, 0.0), Vec3::new(0.13, 0.09, 0.13), hair);
}

#[allow(clippy::too_many_arguments)]
pub fn spawn_street_scenes_a(
    commands: &mut Commands,
    meshes: &mut Assets<Mesh>,
    materials: &mut Assets<StandardMaterial>,
    images: &mut Assets<Image>,
    ba: &mut BackdropAssets,
    fonts: &mut FontEngine,
    grid: &CityGridData,
    finder: &mut SpotFinder,
    night: bool,
) {
    let mut solid = StreetMeshBuilder::default();
    let mut glass = StreetMeshBuilder::default();

    // ==================== 1. VIETCOMBANK ATM BOOTH (`street-atm.js`) ====================
    if let Some(f) = finder
        .find(4.2, 1.3, 2.0, true, 42.0)
        .or_else(|| finder.find(4.2, 1.3, 2.0, true, 64.0))
    {
        let rot = f.face_out_rot();
        let bx = -1.25;
        let h0 = 0.06;
        let h = 2.50;
        let wd = 1.50;
        let dp = 1.30;

        // Concrete step, floor, back & side walls, light-green stripes, roof, green fascia box, AC unit
        solid.push_box_rot(f.at(bx, 0.0, 0.0), Vec3::new(wd + 0.2, 0.12, dp + 0.2), rot, "#A9AEB4");
        solid.push_box_rot(f.at(bx, 0.0, h0 + 0.02), Vec3::new(wd, 0.04, dp), rot, "#D6D9DC");
        solid.push_box_rot(f.at(bx, -dp * 0.5 + 0.03, h0 + (h - 0.42) * 0.5), Vec3::new(wd, h - 0.42, 0.06), rot, "#F2F4F3");
        for s in [-1.0_f32, 1.0] {
            solid.push_box_rot(f.at(bx + s * (wd * 0.5 - 0.03), 0.0, h0 + (h - 0.42) * 0.5), Vec3::new(0.06, h - 0.42, dp), rot, "#F2F4F3");
            solid.push_box_rot(f.at(bx + s * (wd * 0.5 - 0.03), dp * 0.5 - 0.20, h0 + 0.95), Vec3::new(0.07, 1.30, 0.10), rot, "#8DC63F");
            solid.push_box_rot(f.at(bx + s * (wd * 0.5 - 0.04), dp * 0.5 - 0.04, h0 + (h - 0.42) * 0.5), Vec3::new(0.08, h - 0.42, 0.08), rot, "#C9CED6");
        }
        solid.push_box_rot(f.at(bx, 0.0, h0 + h), Vec3::new(wd + 0.06, 0.06, dp + 0.06), rot, "#E3E6E9");
        solid.push_box_rot(f.at(bx, 0.0, h0 + h - 0.21), Vec3::new(wd + 0.06, 0.42, dp + 0.06), rot, "#00703C");
        solid.push_box_rot(f.at(bx + wd * 0.5 + 0.14, -0.25, h0 + 1.65), Vec3::new(0.36, 0.50, 0.22), rot, "#EEF0F2");
        // Glass front pane
        glass.push_box_rot(f.at(bx, dp * 0.5 - 0.02, h0 + (h - 0.42) * 0.5), Vec3::new(wd - 0.16, h - 0.46, 0.02), rot, "#DDF2E8");

        // Interior ATM machine + slanted keypad + receipt bin
        solid.push_box_rot(f.at(bx, -0.36, h0 + 0.71), Vec3::new(0.68, 1.42, 0.50), rot, "#5E6670");
        solid.push_box_rot(f.at(bx, -0.06, h0 + 0.98), Vec3::new(0.56, 0.04, 0.24), rot * Quat::from_rotation_x(0.32), "#3A3F47");
        solid.push_cyl(f.at(bx + 0.55, -0.40, h0 + 0.18), 0.11, 0.10, 0.36, 10, "#2B2F36");

        // Payday queue figures waiting along the sidewalk
        for (k, qx) in [0.15_f32, 0.95, 1.75].iter().enumerate() {
            let shirts = ["#2E6FB5", "#1F9E4A", "#B85C7A"];
            push_figure(&mut solid, f.at(*qx, 0.85, -0.06), f.yaw, shirts[k], "#2B2F36", "#241E1A");
        }

        // UV-cropped Vietcombank 512×512 ATM atlas panels (ZERO uncropped sprite sheet!)
        let atm_mat = ba.textured(images, materials, draw_atm_atlas(fonts), night, 0.35);
        // Fascia front [0, 0, 512, 144] & back
        spawn_backdrop_uv_quad(
            commands, meshes, ba, atm_mat.clone(), 512.0, 512.0, [0.0, 0.0, 512.0, 144.0],
            f.at(bx, dp * 0.5 + 0.04, h0 + h - 0.21), Vec2::new(wd + 0.04, 0.40), rot,
        );
        spawn_backdrop_uv_quad(
            commands, meshes, ba, atm_mat.clone(), 512.0, 512.0, [0.0, 0.0, 512.0, 144.0],
            f.at(bx, -dp * 0.5 - 0.04, h0 + h - 0.21), Vec2::new(wd + 0.04, 0.40), rot * Quat::from_rotation_y(PI),
        );
        // Fascia sides [0, 144, 512, 144]
        for s in [-1.0_f32, 1.0] {
            spawn_backdrop_uv_quad(
                commands, meshes, ba, atm_mat.clone(), 512.0, 512.0, [0.0, 144.0, 512.0, 144.0],
                f.at(bx + s * (wd * 0.5 + 0.04), 0.0, h0 + h - 0.21), Vec2::new(dp + 0.04, 0.40),
                rot * Quat::from_rotation_y(s * FRAC_PI_2),
            );
        }
        // Frosted glass band [0, 288, 512, 64] & QR notice [192, 432, 320, 80]
        spawn_backdrop_uv_quad(
            commands, meshes, ba, atm_mat.clone(), 512.0, 512.0, [0.0, 288.0, 512.0, 64.0],
            f.at(bx, dp * 0.5 + 0.01, h0 + 1.15), Vec2::new(wd - 0.14, 0.22), rot,
        );
        spawn_backdrop_uv_quad(
            commands, meshes, ba, atm_mat.clone(), 512.0, 512.0, [192.0, 432.0, 320.0, 80.0],
            f.at(bx + 0.36, dp * 0.5 + 0.012, h0 + 1.56), Vec2::new(0.46, 0.14), rot,
        );
        // Interior machine screen [0, 352, 192, 144] & header [192, 352, 320, 80]
        spawn_backdrop_uv_quad(
            commands, meshes, ba, atm_mat.clone(), 512.0, 512.0, [0.0, 352.0, 192.0, 144.0],
            f.at(bx, -0.10, h0 + 1.24), Vec2::new(0.44, 0.34), rot,
        );
        spawn_backdrop_uv_quad(
            commands, meshes, ba, atm_mat, 512.0, 512.0, [192.0, 352.0, 320.0, 80.0],
            f.at(bx, -0.10, h0 + 1.52), Vec2::new(0.64, 0.18), rot,
        );
        commands.spawn((WorldEntity, BackdropEntity, StreetAtmBooth, Transform::from_translation(f.origin)));
    }

    // ==================== 2. V-GREEN EV SWAP CABINET + E-SCOOTER ROW (`street-ev.js`) ====================
    if let Some(f) = finder
        .find(3.2, 1.4, 1.9, true, 46.0)
        .or_else(|| finder.find(3.2, 1.4, 1.9, true, 66.0))
    {
        let rot = f.face_out_rot();
        let (cw, ch, cd) = (1.76, 2.08, 0.68);
        // White cabinet body + green canopy roof + concrete plinth
        solid.push_box_rot(f.at(0.0, 0.0, 0.04), Vec3::new(cw + 0.24, 0.08, cd + 0.24), rot, "#A9AEB4");
        solid.push_box_rot(f.at(0.0, 0.0, 0.08 + ch * 0.5), Vec3::new(cw, ch, cd), rot, "#F4F6F5");
        solid.push_box_rot(f.at(0.0, 0.12, 0.08 + ch + 0.04), Vec3::new(cw + 0.22, 0.08, cd + 0.36), rot, "#00A651");
        // Hanoi Ring Road 1 LEZ signpost beside cabinet
        solid.push_cyl(f.at(1.32, 0.25, 1.10), 0.03, 0.03, 2.20, 8, "#5E6670");
        solid.push_box_rot(f.at(1.32, 0.25, 1.88), Vec3::new(0.54, 0.68, 0.04), rot, "#1F5FB4");

        // UV-cropped 1024×640 V-Green EV cabinet atlas (Front [0,0,512,640], Sides [512,0,256,640], LEZ [768,0,256,320])
        let ev_mat = ba.textured(images, materials, draw_ev_cabinet_atlas(fonts), night, 0.38);
        spawn_backdrop_uv_quad(
            commands, meshes, ba, ev_mat.clone(), 1024.0, 640.0, [0.0, 0.0, 512.0, 640.0],
            f.at(0.0, cd * 0.5 + 0.012, 0.08 + ch * 0.5), Vec2::new(cw - 0.04, ch - 0.04), rot,
        );
        for s in [-1.0_f32, 1.0] {
            spawn_backdrop_uv_quad(
                commands, meshes, ba, ev_mat.clone(), 1024.0, 640.0, [512.0, 0.0, 256.0, 640.0],
                f.at(s * (cw * 0.5 + 0.012), 0.0, 0.08 + ch * 0.5), Vec2::new(cd - 0.02, ch - 0.04),
                rot * Quat::from_rotation_y(s * FRAC_PI_2),
            );
        }
        spawn_backdrop_uv_quad(
            commands, meshes, ba, ev_mat, 1024.0, 640.0, [768.0, 0.0, 256.0, 320.0],
            f.at(1.32, 0.28, 1.88), Vec2::new(0.50, 0.64), rot,
        );

        // Row of 5 parked electric scooters on unobstructed sidewalk (far-side kerb in HQ to clear terrace)
        let ev_cols = ["#F2F4F7", "#22B8BE", "#3B7AD9", "#E85D4A", "#2B2F36"];
        let bar_cols = ["#3DDC84", "#3DDC84", "#F2B544", "#3DDC84", "#FF5F5F"];
        let row_z = if grid.layout.is_hq() {
            grid.sb.z1 + STREET_W + 1.05
        } else {
            grid.sb.z1 - 0.85
        };
        for (i, col) in ev_cols.iter().enumerate() {
            let bx = grid.office_mid.x - 4.0 + i as f32 * 0.92;
            let base = Vec3::new(bx, PAVE_Y, row_z);
            solid.push_box(base + Vec3::new(0.0, 0.52, 0.0), Vec3::new(0.36, 0.30, 1.30), col);
            solid.push_box(base + Vec3::new(0.0, 0.78, -0.44), Vec3::new(0.42, 0.46, 0.16), col);
            solid.push_box(base + Vec3::new(0.0, 0.72, 0.18), Vec3::new(0.32, 0.10, 0.68), "#1C1E22");
            solid.push_box(base + Vec3::new(0.0, 0.90, -0.53), Vec3::new(0.34, 0.05, 0.03), "#EAF6FF");
            // 5-bar battery gauge along the side body
            let bars_lit = if i == 4 { 1 } else if i == 2 { 3 } else { 5 };
            for b_idx in 0..5 {
                let c = if b_idx < bars_lit { bar_cols[i] } else { "#2A2F36" };
                solid.push_box(
                    base + Vec3::new(0.19, 0.56, -0.10 + b_idx as f32 * 0.08),
                    Vec3::new(0.02, 0.08, 0.05),
                    c,
                );
            }
        }
        let row_cx = grid.office_mid.x - 4.0 + 2.0 * 0.92;
        commands.spawn((
            WorldEntity,
            BackdropEntity,
            StreetEvScooterRow {
                center: Vec3::new(row_cx, PAVE_Y, row_z),
                half_extents: Vec2::new(2.05, 0.65),
            },
            Transform::from_translation(Vec3::new(row_cx, PAVE_Y, row_z)),
        ));
        commands.spawn((WorldEntity, BackdropEntity, StreetEvStation, Transform::from_translation(f.origin)));
    }

    // ==================== 3. KOREAN SELF PHOTO STUDIO (`street-photo.js`) ====================
    if let Some(f) = finder
        .find(5.6, 2.7, 2.75, true, 48.0)
        .or_else(|| finder.find(5.6, 2.7, 2.75, true, 68.0))
    {
        let rot = f.face_out_rot();
        let (x0, x1, z0, z1, fl, h) = (-2.45_f32, 2.45_f32, -1.20_f32, 1.15_f32, 0.15_f32, 2.75_f32);
        let w = x1 - x0;
        let d = z1 - z0;
        // Pink corrugated shipping container + roof + floor + corner blocks
        solid.push_box_rot(f.at(0.0, 0.0, fl + 0.02), Vec3::new(w, 0.06, d), rot, "#F4EFF1");
        solid.push_box_rot(f.at(0.0, 0.0, h - 0.04), Vec3::new(w, 0.08, d), rot, "#E8A9BE");
        solid.push_box_rot(f.at(0.0, z0 + 0.03, (h + fl) * 0.5), Vec3::new(w, h - fl, 0.06), rot, "#F5B8CC");
        for sx in [x0 + 0.03, x1 - 0.03] {
            solid.push_box_rot(f.at(sx, 0.0, (h + fl) * 0.5), Vec3::new(0.06, h - fl, d), rot, "#F5B8CC");
        }
        // Container ribs on back
        let mut rx = x0 + 0.25;
        while rx < x1 - 0.15 {
            solid.push_box_rot(f.at(rx, z0 - 0.015, (h + fl) * 0.5), Vec3::new(0.10, h - fl - 0.2, 0.03), rot, "#E9A3BB");
            rx += 0.42;
        }
        // Dark neon sign backing + front mullions + step
        solid.push_box_rot(f.at(0.0, z1 - 0.03, h - 0.33), Vec3::new(w, 0.52, 0.08), rot, "#2A1420");
        for mx in [x0 + 0.05, -0.85, 0.45, 1.25, x1 - 0.05] {
            solid.push_box_rot(f.at(mx, z1 - 0.03, fl + 1.02), Vec3::new(0.06, 2.02, 0.07), rot, "#F4F4F2");
        }
        glass.push_box_rot(f.at(-0.6, z1 - 0.02, fl + 1.02), Vec3::new(3.5, 2.0, 0.02), rot, "#FFEAF2");

        // Two interior photo booths behind pink & black pleated curtains + ring lights
        let flash_mesh = ba.add_mesh(meshes, Mesh::from(Sphere::new(0.14)));
        let flash_mat = ba.mat_emissive(materials, "#FFF6FB", 3.4);
        for (idx, (bx0, bx1, curt_col)) in [(-2.30_f32, -0.95_f32, "#F28AB2"), (-0.85, 0.35, "#2B2F36")].iter().enumerate() {
            let bcx = (bx0 + bx1) * 0.5;
            let bw = bx1 - bx0;
            solid.push_box_rot(f.at(*bx0, -0.52, fl + 1.10), Vec3::new(0.05, 2.15, 1.32), rot, "#2B2F36");
            solid.push_box_rot(f.at(*bx1, -0.52, fl + 1.10), Vec3::new(0.05, 2.15, 1.32), rot, "#2B2F36");
            // Curtain hanging with 0.7m gap underneath so feet show
            solid.push_box_rot(f.at(bcx, 0.16, fl + 1.44), Vec3::new(bw - 0.06, 1.38, 0.04), rot, curt_col);
            // Camera console + ring light inside
            solid.push_box_rot(f.at(bcx, z0 + 0.14, fl + 1.50), Vec3::new(0.46, 0.38, 0.08), rot, "#1E2125");
            // Animated flash bulb inside booth (shared mesh & emissive material across both booths)
            commands.spawn((
                WorldEntity,
                BackdropEntity,
                StreetPhotoFlash { phase: idx as f32 * 2.7 },
                Mesh3d(flash_mesh.clone()),
                MeshMaterial3d(flash_mat.clone()),
                Transform::from_translation(f.at(bcx, z0 + 0.24, fl + 1.72)),
                Visibility::Hidden,
            ));
        }
        // Props table by door (headband basket + mirror) + sidewalk A-frame board
        solid.push_box_rot(f.at(1.25, -0.75, fl + 0.78), Vec3::new(0.70, 0.05, 0.45), rot, "#F4F4F2");
        solid.push_cyl(f.at(1.25, -0.75, fl + 0.88), 0.20, 0.16, 0.16, 10, "#C9A86A");
        solid.push_box_rot(f.at(2.15, 1.55, 0.56), Vec3::new(0.76, 1.06, 0.06), rot * Quat::from_rotation_x(-0.15), "#E8327A");

        // UV-cropped 1024×512 Korean Photo Studio atlas!
        let photo_mat = ba.textured(images, materials, draw_photo_studio_atlas(fonts), true, 0.35);
        // Neon sign [0, 0, 1024, 192]
        spawn_backdrop_uv_quad(
            commands, meshes, ba, photo_mat.clone(), 1024.0, 512.0, [0.0, 0.0, 1024.0, 192.0],
            f.at(0.0, z1 + 0.02, h - 0.33), Vec2::new(w - 0.06, 0.50), rot,
        );
        // Price board A-frame [0, 192, 384, 320]
        spawn_backdrop_uv_quad(
            commands, meshes, ba, photo_mat.clone(), 1024.0, 512.0, [0.0, 192.0, 384.0, 320.0],
            f.at(2.15, 1.59, 0.56), Vec2::new(0.72, 1.00), rot * Quat::from_rotation_x(-0.15),
        );
        // Sample photo strips wall [384, 192, 400, 200]
        spawn_backdrop_uv_quad(
            commands, meshes, ba, photo_mat.clone(), 1024.0, 512.0, [384.0, 192.0, 400.0, 200.0],
            f.at(1.30, z0 + 0.08, fl + 1.52), Vec2::new(1.35, 0.68), rot,
        );
        // Booth countdown screens [784, 192, 240, 160]
        for bcx in [-1.62_f32, -0.25] {
            spawn_backdrop_uv_quad(
                commands, meshes, ba, photo_mat.clone(), 1024.0, 512.0, [784.0, 192.0, 240.0, 160.0],
                f.at(bcx, z0 + 0.19, fl + 1.50), Vec2::new(0.42, 0.30), rot,
            );
        }
        commands.spawn((WorldEntity, BackdropEntity, StreetPhotoStudio, Transform::from_translation(f.origin)));
    }

    // ==================== 4. 20×44FT PICKLEBALL COURT + DOUBLES RALLY (`street-pickle.js`) ====================
    if let Some(site) = grid.pickle {
        let cx = (site.x0 + site.x1) * 0.5;
        let cz = (site.z0 + site.z1) * 0.5;
        let lf = if site.axis_x { site.x1 - site.x0 } else { site.z1 - site.z0 };
        let wf = if site.axis_x { site.z1 - site.z0 } else { site.x1 - site.x0 };
        let court_rot = if site.axis_x { Quat::IDENTITY } else { Quat::from_rotation_y(FRAC_PI_2) };
        let pt = |lx: f32, ly: f32, lz: f32| Vec3::new(cx, GY, cz) + court_rot * Vec3::new(lx, ly, lz);

        // Concrete slab + green surround + 20×44ft blue acrylic court + crisp 5.5cm white markings
        solid.push_box_rot(pt(0.0, PB_TOP * 0.5, 0.0), Vec3::new(lf, PB_TOP, wf), court_rot, "#A3A7A6");
        solid.push_box_rot(pt(0.0, PB_TOP + 0.008, 0.0), Vec3::new(lf - 0.12, 0.016, wf - 0.12), court_rot, "#2F7356");
        let cl = PB_L.min(lf - 0.8);
        let cw = PB_W.min(wf - 0.8);
        let hl = cl * 0.5;
        let hw = cw * 0.5;
        let kl = PB_K.min(hl * 0.45);
        solid.push_box_rot(pt(0.0, PB_TOP + 0.018, 0.0), Vec3::new(cl, 0.012, cw), court_rot, "#2C5C9C");

        let ly = PB_TOP + 0.027;
        for s in [-1.0_f32, 1.0] {
            // Baselines, sidelines, non-volley kitchen lines, center service lines
            solid.push_box_rot(pt(s * (hl - PB_LINE * 0.5), ly, 0.0), Vec3::new(PB_LINE, 0.006, cw), court_rot, "#F4F6F2");
            solid.push_box_rot(pt(0.0, ly, s * (hw - PB_LINE * 0.5)), Vec3::new(cl, 0.006, PB_LINE), court_rot, "#F4F6F2");
            solid.push_box_rot(pt(s * kl, ly, 0.0), Vec3::new(PB_LINE, 0.006, cw), court_rot, "#F4F6F2");
            let svc_len = (hl - kl).max(0.5);
            solid.push_box_rot(pt(s * (kl + svc_len * 0.5), ly, 0.0), Vec3::new(svc_len, 0.006, PB_LINE), court_rot, "#F4F6F2");
        }
        // Net posts + top tape + net body
        for s in [-1.0_f32, 1.0] {
            solid.push_cyl(pt(0.0, PB_TOP + 0.46, s * (hw + 0.25)), 0.035, 0.035, 0.92, 8, "#1E2125");
        }
        solid.push_box_rot(pt(0.0, PB_TOP + 0.88, 0.0), Vec3::new(0.04, 0.06, cw + 0.44), court_rot, "#F6F8FA");
        glass.push_box_rot(pt(0.0, PB_TOP + 0.48, 0.0), Vec3::new(0.02, 0.74, cw + 0.40), court_rot, "#2A3038");

        // Fence posts + 4 corner LED floodlight poles + 2 benches
        let hf = lf * 0.5 - 0.08;
        let hwf = wf * 0.5 - 0.08;
        for sx in [-1.0_f32, 1.0] {
            for sz in [-1.0_f32, 1.0] {
                solid.push_cyl(pt(sx * hf, PB_TOP + 2.1, sz * hwf), 0.045, 0.055, 4.2, 8, "#24463A");
                solid.push_box_rot(pt(sx * (hf - 0.18), PB_TOP + 4.18, sz * (hwf - 0.18)), Vec3::new(0.42, 0.16, 0.22), court_rot, "#EEF6FF");
            }
        }
        for bx in [-2.6_f32, 2.6] {
            solid.push_box_rot(pt(bx, PB_TOP + 0.42, hwf - 0.42), Vec3::new(1.5, 0.06, 0.36), court_rot, "#3B6B8A");
        }

        // 4 doubles players on court (2 per side, visors + paddles!)
        let players = [
            (-kl - 1.3, -1.25, 0.0_f32, "#F2C230", "#2B2F36"),
            (-kl - 0.3, 1.25, 0.0, "#FF6B9A", "#F4F4F2"),
            (kl + 1.3, 1.25, PI, "#3F7CC4", "#2B2F36"),
            (kl + 0.3, -1.25, PI, "#3DDC84", "#F4F4F2"),
        ];
        for (px, pz, pyaw, sh, pa) in players {
            let py = if site.axis_x { pyaw } else { pyaw + FRAC_PI_2 };
            push_figure(&mut solid, pt(px, PB_TOP + 0.02, pz), py, sh, pa, "#241E1A");
        }

        // UV-cropped 1024×512 Pickleball windscreen banners [0,0,1024,128] & gate board [0,128,512,384]
        let pb_mat = ba.textured(images, materials, draw_pickleball_atlas(fonts), false, 0.55);
        for s in [-1.0_f32, 1.0] {
            spawn_backdrop_uv_quad(
                commands, meshes, ba, pb_mat.clone(), 1024.0, 512.0, [0.0, 0.0, 1024.0, 128.0],
                pt(s * (hf - 0.02), PB_TOP + 1.35, 0.0), Vec2::new(wf - 0.4, 1.25),
                court_rot * Quat::from_rotation_y(if s > 0.0 { -FRAC_PI_2 } else { FRAC_PI_2 }),
            );
        }
        spawn_backdrop_uv_quad(
            commands, meshes, ba, pb_mat, 1024.0, 512.0, [0.0, 128.0, 512.0, 384.0],
            pt(-hf + 1.4, PB_TOP + 1.25, hwf + 0.03), Vec2::new(1.55, 1.15), court_rot,
        );

        // Animated yellow-green rallying pickleball
        let ball_mesh = ba.add_mesh(meshes, Mesh::from(Sphere::new(0.065)));
        let ball_mat = ba.mat_emissive(materials, "#D9EA4E", 1.1);
        commands.spawn((
            WorldEntity,
            BackdropEntity,
            PickleRallyBall {
                origin: Vec3::new(cx, GY, cz),
                axis_x: site.axis_x,
                court_y: GY + PB_TOP + 0.95,
            },
            Mesh3d(ball_mesh),
            MeshMaterial3d(ball_mat),
            Transform::from_translation(pt(0.0, PB_TOP + 1.35, 0.0)),
        ));
        commands.spawn((WorldEntity, BackdropEntity, StreetPickleCourt, Transform::from_translation(Vec3::new(cx, GY, cz))));
    }

    let solid_mat = ba.mat(materials, "#FFFFFF", 0.62, 0.12);
    solid.spawn(commands, meshes, ba, solid_mat);
    let glass_mat = ba.mat_blend(materials, "#D6ECF7", 0.28, 0.12, false);
    glass.spawn(commands, meshes, ba, glass_mat);
}

/// Zero-allocation animation for Pickleball doubles rally arc & Korean Photo Studio booth flash.
pub fn animate_street_scenes_a_system(
    time: Res<Time>,
    mut balls: Query<(&PickleRallyBall, &mut Transform)>,
    mut flashes: Query<(&StreetPhotoFlash, &mut Visibility)>,
) {
    let t = time.elapsed_secs();
    for (b, mut tf) in &mut balls {
        let u = (t * 0.95).sin();
        let along = u * 4.4;
        let across = (t * 0.48).cos() * 1.35;
        let arc_y = (1.0 - u * u) * 1.15;
        let offset = if b.axis_x {
            Vec3::new(along, 0.0, across)
        } else {
            Vec3::new(across, 0.0, -along)
        };
        tf.translation = Vec3::new(b.origin.x + offset.x, b.court_y + arc_y, b.origin.z + offset.z);
    }
    for (f, mut vis) in &mut flashes {
        let cyc = (t + f.phase).rem_euclid(5.2);
        *vis = if cyc < 0.14 {
            Visibility::Inherited
        } else {
            Visibility::Hidden
        };
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::backdrops::BackdropEntity;
    use crate::layout::{LayoutSpec, OfficeLayout};
    use crate::street::city_grid::spawn_city_grid;
    use bevy::render::mesh::VertexAttributeValues;
    use std::path::PathBuf;

    #[test]
    fn scenes_a_spawns_uv_cropped_atm_ev_photo_pickle_and_purges_zero_leaks() {
        let assets_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../assets");
        let spec = LayoutSpec::for_layout(OfficeLayout::Pro);
        let mut fonts = FontEngine::from_assets_dir(&assets_dir);

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
                        spawn_street_scenes_a(&mut cmds, &mut meshes, &mut mats, &mut imgs, &mut ba, &mut fonts, &grid, &mut finder, false);
                    });
                });
            });
        });
        world.flush();

        assert_eq!(world.query::<&StreetAtmBooth>().iter(&world).count(), 1);
        assert_eq!(world.query::<&StreetEvStation>().iter(&world).count(), 1);
        assert_eq!(world.query::<&StreetPhotoStudio>().iter(&world).count(), 1);
        assert_eq!(world.query::<&StreetPickleCourt>().iter(&world).count(), 1);
        assert_eq!(world.query::<&PickleRallyBall>().iter(&world).count(), 1);

        let meshes = world.resource::<Assets<Mesh>>();
        let cropped_quads = world
            .resource::<BackdropAssets>()
            .owned_meshes
            .iter()
            .filter_map(|h| meshes.get(h))
            .filter(|m| {
                if m.count_vertices() != 4 {
                    return false;
                }
                if let Some(VertexAttributeValues::Float32x2(uvs)) = m.attribute(Mesh::ATTRIBUTE_UV_0) {
                    let max_u = uvs.iter().map(|u| u[0]).fold(0.0_f32, f32::max);
                    let max_v = uvs.iter().map(|u| u[1]).fold(0.0_f32, f32::max);
                    let min_u = uvs.iter().map(|u| u[0]).fold(1.0_f32, f32::min);
                    let min_v = uvs.iter().map(|u| u[1]).fold(1.0_f32, f32::min);
                    (max_u - min_u) < 0.99 || (max_v - min_v) < 0.99
                } else {
                    false
                }
            })
            .count();
        assert!(cropped_quads >= 10, "expected >= 10 UV-cropped atlas quads, got {cropped_quads}");

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
