use crate::interact::InteractiveProp;
use crate::layout::LayoutSpec;
use crate::palette::{
    WorldEntity, WorldPalette, spawn_box, spawn_box_rot, spawn_cyl, spawn_quad, spawn_sphere,
};
use crate::rooms::spawn_pot_plant;
use bevy::asset::RenderAssetUsages;
use bevy::prelude::*;
use bevy::render::render_resource::{Extent3d, TextureDimension, TextureFormat};
use station_canvas2d::boards_and_social::{
    WhiteboardInput, WhiteboardTask, draw_sticky_note, draw_whiteboard,
};
use station_canvas2d::{FontEngine, hex};
use std::f32::consts::PI;

#[derive(Component)]
pub struct ServerFan {
    pub speed: f32,
}

#[derive(Component)]
pub struct WhiteboardSurface;

/// Redraws the 3D whiteboard texture in-place on its existing `Image` asset (zero leaked assets).
pub fn redraw_whiteboard_surface(
    text: &str,
    streamer: bool,
    wb_mat_handle: &Handle<StandardMaterial>,
    materials: &mut Assets<StandardMaterial>,
    images: &mut Assets<Image>,
    fonts: &mut FontEngine,
) -> bool {
    let Some(mat) = materials.get(wb_mat_handle) else {
        return false;
    };
    let Some(tex_handle) = mat.base_color_texture.clone() else {
        return false;
    };

    let mut tasks = Vec::new();
    let mut open_todos = 0usize;
    let mut total_todos = 0usize;
    for raw_line in text.lines() {
        let trimmed = raw_line.trim();
        if trimmed.is_empty() {
            continue;
        }
        let body = trimmed
            .trim_start_matches("- ")
            .trim_start_matches("* ")
            .trim();
        let done = body.starts_with("[x]") || body.starts_with("[X]");
        let clean = body
            .trim_start_matches("[x]")
            .trim_start_matches("[X]")
            .trim_start_matches("[ ]")
            .trim();
        if clean.is_empty() {
            continue;
        }
        total_todos += 1;
        if !done {
            open_todos += 1;
        }
        if tasks.len() < 4 {
            tasks.push(WhiteboardTask {
                name: format!("Note {}", tasks.len() + 1),
                color: if done { hex("#3DDC84") } else { hex("#D97757") },
                state: if done { "done".into() } else { "doing".into() },
                prompt: clean.to_owned(),
                todos_done: usize::from(done),
                todos_total: 1,
            });
        }
    }

    let surf = draw_whiteboard(
        &WhiteboardInput {
            title: "Whiteboard".into(),
            yours_title: "Yours".into(),
            streamer,
            tasks,
            open_todos,
            total_todos,
        },
        fonts,
    );
    let new_img = Image::new(
        Extent3d {
            width: surf.width.max(1),
            height: surf.height.max(1),
            depth_or_array_layers: 1,
        },
        TextureDimension::D2,
        surf.rgba,
        TextureFormat::Rgba8UnormSrgb,
        RenderAssetUsages::RENDER_WORLD,
    );
    // Always re-insert under the existing Handle<Image> id: `RenderAssetUsages::RENDER_WORLD`
    // drops the CPU copy from `Assets<Image>` after frame-1 GPU extraction, so `get_mut` would
    // return `None` at runtime while `insert` re-queues GPU upload under the same handle.
    images.insert(&tex_handle, new_img);
    true
}

/// Spawns whiteboard + stickies, coffee corner, meeting table, 3 server racks + fans,
/// bookshelf, lounge sofa, rocket sculpture, globe, and floor lamps.
pub fn spawn_furniture(
    commands: &mut Commands,
    palette: &mut WorldPalette,
    materials: &mut Assets<StandardMaterial>,
    images: &mut Assets<Image>,
    fonts: &mut FontEngine,
    spec: &LayoutSpec,
) {
    let metal = palette.mat(materials, "#2B2F36", 0.35, 0.65);
    let wood_dark = palette.mat(materials, "#6E4B32", 0.6, 0.0);
    let wood_mid = palette.mat(materials, "#9A6E48", 0.55, 0.0);

    // 1. Whiteboard + live station-canvas2d surface + 3 sticky notes
    {
        let [bx, by, bz, bry] = spec.board;
        let rot = Quat::from_rotation_y(bry);
        let frame_m = palette.mat(materials, "#9AA3AD", 0.45, 0.4);
        spawn_box_rot(commands, palette, frame_m.clone(), Vec3::new(bx, by, bz), Vec3::new(0.08, 1.75, 3.25), rot);
        spawn_box_rot(commands, palette, frame_m, Vec3::new(bx + 0.1, by - 0.86, bz + 0.6), Vec3::new(0.14, 0.05, 1.2), rot);

        let wb_surf = draw_whiteboard(
            &WhiteboardInput {
                title: "Whiteboard".into(),
                yours_title: "Yours".into(),
                streamer: false,
                tasks: vec![
                    WhiteboardTask {
                        name: "Claude".into(),
                        color: hex("#D97757"),
                        state: "doing".into(),
                        prompt: "Build native Rust Agents Station 3D office".into(),
                        todos_done: 4,
                        todos_total: 6,
                    },
                    WhiteboardTask {
                        name: "Antigravity".into(),
                        color: hex("#5E9BF0"),
                        state: "done".into(),
                        prompt: "Verify G2 procedural 2D texture parity".into(),
                        todos_done: 5,
                        todos_total: 5,
                    },
                ],
                open_todos: 2,
                total_todos: 3,
            },
            fonts,
        );
        let wb_mat = palette.textured_lit(images, materials, wb_surf, 0.35);
        let wb_ent = spawn_quad(
            commands,
            palette,
            wb_mat,
            Vec3::new(bx + 0.05, by, bz),
            Vec2::new(3.1, 1.55),
            rot * Quat::from_rotation_y(PI * 0.5),
        );
        commands.entity(wb_ent).insert((
            WhiteboardSurface,
            InteractiveProp::new("whiteboard", "office", "Office Whiteboard"),
        ));

        let stickies = [
            ("Ship P3 3D world", false, hex("#FFE787"), 0.25_f32, -0.47_f32),
            ("Verify RAM ≤ 300MB", true, hex("#BDE7C8"), 0.25, -0.89),
            ("Mua cà phê sữa đá", false, hex("#FFB5C2"), -0.18, -0.47),
        ];
        for (txt, done, col, sy, sz) in stickies {
            let st_surf = draw_sticky_note(txt, done, col, false, fonts);
            let st_mat = palette.textured_lit(images, materials, st_surf, 0.85);
            spawn_quad(
                commands,
                palette,
                st_mat,
                Vec3::new(bx + 0.075, by + sy, bz + sz),
                Vec2::new(0.36, 0.36),
                rot * Quat::from_rotation_y(PI * 0.5),
            );
        }
    }

    // 2. Coffee corner (counter, espresso machine, LED, mugs, donuts)
    {
        let [cx, cz, cry] = spec.coffee;
        let rot = Quat::from_rotation_y(cry);
        let at = |lx: f32, ly: f32, lz: f32| Vec3::new(cx, 0.0, cz) + rot * Vec3::new(lx, ly, lz);
        let cab_col = if spec.layout.is_pro() { "#E6E1D8" } else { "#3F5A46" };
        let top_col = if spec.layout.is_pro() { "#3B3F46" } else { "#ECE6DA" };
        let cab_m = palette.mat(materials, cab_col, 0.7, 0.0);
        let top_m = palette.mat(materials, top_col, 0.35, 0.05);
        let mach_m = palette.mat(materials, "#24282F", 0.35, 0.4);
        let led_m = palette.emissive(materials, "#FF6B4A", 1.5);

        spawn_box_rot(commands, palette, cab_m, at(0.0, 0.46, 0.0), Vec3::new(2.9, 0.92, 0.75), rot);
        spawn_box_rot(commands, palette, top_m, at(0.0, 0.955, 0.0), Vec3::new(3.0, 0.07, 0.84), rot);
        spawn_box_rot(commands, palette, mach_m, at(-0.85, 1.3, 0.08), Vec3::new(0.52, 0.62, 0.46), rot);
        spawn_sphere(commands, palette, led_m, at(-0.69, 1.5, -0.15), Vec3::splat(0.06));
        for (k, col) in ["#FFFFFF", "#FFE787", "#BFDDF7"].iter().enumerate() {
            let mm = palette.mat(materials, col, 0.35, 0.0);
            spawn_cyl(commands, palette, mm, at(-0.05 + k as f32 * 0.22, 1.06, 0.12), 0.12, 0.13);
        }
        let plate_m = palette.mat(materials, "#FFFFFF", 0.3, 0.0);
        spawn_cyl(commands, palette, plate_m, at(0.95, 1.005, 0.0), 0.5, 0.03);
        for (k, col) in ["#E8A0B4", "#8B5A3C", "#FFD166"].iter().enumerate() {
            let dm = palette.matte(materials, col);
            spawn_cyl(commands, palette, dm, at(0.89 + k as f32 * 0.11, 1.05, -0.06 + (k % 2) as f32 * 0.12), 0.14, 0.05);
        }
        let plant_p = at(1.4, 0.99, 0.05);
        spawn_pot_plant(commands, palette, materials, plant_p.x, plant_p.y, plant_p.z, 0.35, false);
    }

    // 3. Meeting table & chairs
    {
        let [mx, mz] = spec.meet;
        let chair_m = palette.mat(materials, if spec.layout.is_pro() { "#3C4656" } else { "#4E7D5B" }, 0.9, 0.0);
        if spec.layout.is_pro() {
            let tbl_m = palette.mat(materials, "#F2EFEA", 0.4, 0.0);
            spawn_box(commands, palette, tbl_m, Vec3::new(mx, 0.76, mz), Vec3::new(3.6, 0.07, 1.3));
            for sx in [-1.35_f32, 1.35] {
                spawn_box(commands, palette, metal.clone(), Vec3::new(mx + sx, 0.37, mz), Vec3::new(0.1, 0.72, 0.9));
            }
            let phone_m = palette.mat(materials, "#2B2F36", 0.4, 0.2);
            spawn_box(commands, palette, phone_m, Vec3::new(mx, 0.82, mz), Vec3::new(0.28, 0.05, 0.28));
            let seats = [
                (-0.95_f32, -4.35_f32), (0.2, -4.35), (1.35, -4.35),
                (-0.95, -6.85), (0.2, -6.85), (1.35, -6.85), (-2.15, -5.6),
            ];
            for (sx, sz) in seats {
                spawn_box(commands, palette, chair_m.clone(), Vec3::new(sx, 0.45, sz), Vec3::new(0.5, 0.1, 0.48));
            }
        } else {
            let tbl_m = palette.mat(materials, "#A87B52", 0.45, 0.0);
            spawn_cyl(commands, palette, tbl_m, Vec3::new(mx, 0.76, mz), 2.04, 0.08);
            spawn_cyl(commands, palette, wood_dark.clone(), Vec3::new(mx, 0.36, mz), 0.3, 0.7);
            for a in [0.0_f32, PI * 0.5, PI, PI * 1.5] {
                let sx = mx + a.sin() * 1.35;
                let sz = mz + a.cos() * 1.35;
                spawn_box(commands, palette, chair_m.clone(), Vec3::new(sx, 0.44, sz), Vec3::new(0.5, 0.12, 0.48));
            }
        }
    }

    // 4. Server racks (3 racks, LEDs, spinning top exhaust fans)
    {
        let [rx, rz, rry] = spec.racks;
        let rot = Quat::from_rotation_y(rry);
        let at = |lx: f32, ly: f32, lz: f32| Vec3::new(rx, 0.0, rz) + rot * Vec3::new(lx, ly, lz);
        let rack_m = palette.mat(materials, "#1E2229", 0.55, 0.3);
        let blade_m = palette.mat(materials, "#2C323B", 0.5, 0.35);
        let led_g = palette.emissive(materials, "#5BE37D", 1.8);
        let led_a = palette.emissive(materials, "#FFB347", 1.8);
        let led_b = palette.emissive(materials, "#4FB6FF", 1.8);
        let fan_m = palette.mat(materials, "#9AA3AD", 0.4, 0.5);

        for i in 0..3 {
            let x = -1.05 + i as f32 * 1.05;
            spawn_box_rot(commands, palette, rack_m.clone(), at(x, 1.2, 0.0), Vec3::new(0.95, 2.4, 0.8), rot);
            for r in 0..7 {
                let y = 0.34 + r as f32 * 0.29;
                spawn_box_rot(commands, palette, blade_m.clone(), at(x, y, 0.41), Vec3::new(0.8, 0.22, 0.03), rot);
                let lmat = match (i + r) % 3 {
                    0 => led_g.clone(),
                    1 => led_a.clone(),
                    _ => led_b.clone(),
                };
                spawn_box_rot(commands, palette, lmat, at(x + 0.24, y, 0.435), Vec3::new(0.18, 0.045, 0.02), rot);
            }
            for fx in [-0.22_f32, 0.22] {
                let fan_ent = commands
                    .spawn((
                        WorldEntity,
                        ServerFan { speed: 4.5 + i as f32 * 0.7 },
                        Mesh3d(palette.unit_box.clone()),
                        MeshMaterial3d(fan_m.clone()),
                        Transform::from_translation(at(x + fx, 2.43, 0.2))
                            .with_scale(Vec3::new(0.24, 0.02, 0.06)),
                    ))
                    .id();
                let _ = fan_ent;
            }
        }
    }

    // 5. Bookshelf + colorful books
    {
        let [sx, sz, sry] = spec.shelf;
        let rot = Quat::from_rotation_y(sry);
        let at = |lx: f32, ly: f32, lz: f32| Vec3::new(sx, 0.0, sz) + rot * Vec3::new(lx, ly, lz);
        spawn_box_rot(commands, palette, wood_mid.clone(), at(0.0, 1.15, 0.0), Vec3::new(2.2, 2.3, 0.42), rot);
        let book_cols = ["#D97757", "#3F7CC4", "#2E9E62", "#E2B04A", "#8C4FA8"];
        for row in 0..4 {
            for b in 0..7 {
                let bm = palette.matte(materials, book_cols[(row + b) % book_cols.len()]);
                spawn_box_rot(
                    commands,
                    palette,
                    bm,
                    at(-0.82 + b as f32 * 0.26, 0.42 + row as f32 * 0.5, 0.14),
                    Vec3::new(0.18, 0.34, 0.22),
                    rot,
                );
            }
        }
    }

    // 6. Lounge sofa + cushions
    {
        let [sx, sz, sw, sry] = spec.sofa;
        let rot = Quat::from_rotation_y(sry);
        let at = |lx: f32, ly: f32, lz: f32| Vec3::new(sx, 0.0, sz) + rot * Vec3::new(lx, ly, lz);
        let sofa_m = palette.mat(materials, "#5B748C", 0.9, 0.0);
        let cush_a = palette.mat(materials, "#E8A05E", 0.9, 0.0);
        let cush_b = palette.mat(materials, "#D97757", 0.9, 0.0);
        spawn_box_rot(commands, palette, sofa_m.clone(), at(0.0, 0.26, 0.0), Vec3::new(sw, 0.4, 0.86), rot);
        spawn_box_rot(commands, palette, sofa_m.clone(), at(0.0, 0.62, -0.34), Vec3::new(sw, 0.52, 0.18), rot);
        spawn_box_rot(commands, palette, sofa_m.clone(), at(-sw * 0.5 + 0.1, 0.45, 0.0), Vec3::new(0.2, 0.46, 0.86), rot);
        spawn_box_rot(commands, palette, sofa_m, at(sw * 0.5 - 0.1, 0.45, 0.0), Vec3::new(0.2, 0.46, 0.86), rot);
        spawn_box_rot(commands, palette, cush_a, at(-0.55, 0.56, -0.1), Vec3::new(0.36, 0.32, 0.14), rot);
        spawn_box_rot(commands, palette, cush_b, at(0.55, 0.56, -0.1), Vec3::new(0.36, 0.32, 0.14), rot);
    }

    // 7. Rocket sculpture + Globe
    {
        let [rx, rz] = spec.rocket;
        let body_m = palette.mat(materials, "#F2EFEA", 0.35, 0.15);
        let tip_m = palette.mat(materials, "#D66440", 0.4, 0.1);
        let win_m = palette.emissive(materials, "#6AAEFF", 1.1);
        spawn_cyl(commands, palette, body_m, Vec3::new(rx, 0.9, rz), 0.38, 1.3);
        spawn_sphere(commands, palette, tip_m.clone(), Vec3::new(rx, 1.62, rz), Vec3::new(0.36, 0.45, 0.36));
        spawn_sphere(commands, palette, win_m, Vec3::new(rx - 0.17, 1.05, rz), Vec3::splat(0.14));
        spawn_box(commands, palette, tip_m, Vec3::new(rx, 0.25, rz), Vec3::new(0.65, 0.35, 0.06));

        let [gx, gz] = spec.globe;
        let ocean_m = palette.mat(materials, "#3B7CB8", 0.35, 0.1);
        spawn_cyl(commands, palette, wood_dark, Vec3::new(gx, 0.36, gz), 0.08, 0.72);
        spawn_sphere(commands, palette, ocean_m, Vec3::new(gx, 0.92, gz), Vec3::splat(0.42));
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    #[test]
    fn redraw_whiteboard_surface_succeeds_after_render_world_extraction() {
        let assets_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../assets");
        let mut fonts = FontEngine::from_assets_dir(&assets_dir);
        let mut meshes = Assets::<Mesh>::default();
        let mut materials = Assets::<StandardMaterial>::default();
        let mut images = Assets::<Image>::default();
        let mut pal = WorldPalette::new(&mut meshes);

        let initial_surf = draw_whiteboard(
            &WhiteboardInput {
                title: "Whiteboard".into(),
                yours_title: "Yours".into(),
                streamer: false,
                tasks: Vec::new(),
                open_todos: 0,
                total_todos: 0,
            },
            &mut fonts,
        );
        let wb_mat = pal.textured_lit(&mut images, &mut materials, initial_surf, 0.35);
        let tex_handle = materials
            .get(&wb_mat)
            .and_then(|m| m.base_color_texture.clone())
            .expect("whiteboard material must have base_color_texture");

        // Simulate Bevy frame-1 RenderAssetUsages::RENDER_WORLD extraction removing the CPU Image.
        images.remove(&tex_handle);
        assert!(images.get(&tex_handle).is_none());

        let ok = redraw_whiteboard_surface(
            "- Ship Fix 1\n- [x] Verify redraw",
            false,
            &wb_mat,
            &mut materials,
            &mut images,
            &mut fonts,
        );
        assert!(ok, "redraw_whiteboard_surface must succeed after RENDER_WORLD extraction");
        let reinserted = images
            .get(&tex_handle)
            .expect("redraw must reinsert updated Image under the same Handle<Image> id");
        assert!(reinserted.width() > 0 && reinserted.height() > 0);
        assert_eq!(images.len(), 1, "redraw must not leak extra Image entries");
        assert_eq!(materials.len(), 1, "redraw must not leak extra StandardMaterial entries");
    }
}
