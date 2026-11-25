use crate::layout::LayoutSpec;
use crate::palette::{
    WorldEntity, WorldPalette, spawn_box_rot, spawn_cyl, spawn_quad, spawn_sphere,
};
use bevy::prelude::*;
use station_canvas2d::FontEngine;
use station_canvas2d::office_surfaces::{ALL_SCREEN_KINDS, ScreenKind, draw_desk_screen};
use std::collections::HashMap;
use std::f32::consts::PI;

#[derive(Component, Debug, Clone, Copy)]
pub struct DeskSlot {
    pub index: usize,
}

#[derive(Component, Debug, Clone, Copy)]
pub struct DeskScreen {
    pub desk_index: usize,
}

#[derive(Resource, Default)]
pub struct ScreenMaterials {
    pub by_kind: HashMap<ScreenKind, Handle<StandardMaterial>>,
}

/// Maps a live session's canonical status (`"work"`/`"wait"`/`"rest"`) and active tool
/// to the corresponding desk monitor `ScreenKind`.
pub fn screen_kind_for_session(
    status: &str,
    tool_kind: Option<&str>,
    tool_label: Option<&str>,
) -> ScreenKind {
    match status {
        "wait" | "waiting" => return ScreenKind::Wait,
        "rest" | "idle" | "sleep" => return ScreenKind::Sleep,
        _ => {}
    }
    let kind_l = tool_kind.unwrap_or("").to_ascii_lowercase();
    let label_l = tool_label.unwrap_or("").to_ascii_lowercase();
    match kind_l.as_str() {
        "bash" | "term" | "terminal" | "shell" | "cmd" | "exec" => ScreenKind::Term,
        "read" | "grep" | "glob" | "search" => ScreenKind::Read,
        "web" | "fetch" | "webfetch" | "websearch" => ScreenKind::Web,
        "chat" | "plan" | "ask" | "agent" | "task" => ScreenKind::Chat,
        "err" | "error" | "fail" => ScreenKind::Wait,
        "edit" | "write" | "multiedit" | "code" => ScreenKind::Code,
        _ => {
            if label_l.contains("bash")
                || label_l.contains("cargo")
                || label_l.contains("npm")
                || label_l.contains("term")
            {
                ScreenKind::Term
            } else if label_l.contains("read")
                || label_l.contains("grep")
                || label_l.contains("search")
                || label_l.contains("inspect")
            {
                ScreenKind::Read
            } else if label_l.contains("web") || label_l.contains("http") || label_l.contains("fetch")
            {
                ScreenKind::Web
            } else if label_l.contains("plan") || label_l.contains("ask") || label_l.contains("chat")
            {
                ScreenKind::Chat
            } else if label_l.contains("err") || label_l.contains("fail") {
                ScreenKind::Wait
            } else {
                ScreenKind::Code
            }
        }
    }
}

/// Spawns all 9 (Cozy) / 12 (Pro) / 44 (HQ) desks, dual monitors, chairs, and shared canvas2d screen textures.
pub fn spawn_desks(
    commands: &mut Commands,
    palette: &mut WorldPalette,
    materials: &mut Assets<StandardMaterial>,
    images: &mut Assets<Image>,
    fonts: &mut FontEngine,
    spec: &LayoutSpec,
) -> ScreenMaterials {
    let mut screen_mats = ScreenMaterials::default();
    for kind in ALL_SCREEN_KINDS {
        let surf = draw_desk_screen(kind, fonts);
        let mat = palette.textured_unlit(images, materials, surf);
        screen_mats.by_kind.insert(kind, mat);
    }

    let desk_top = palette.mat(materials, "#F4F0E8", 0.45, 0.0);
    let desk_mat = palette.mat(materials, "#2C323C", 0.8, 0.0);
    let metal = palette.mat(materials, "#2B2F36", 0.35, 0.65);
    let laptop_m = palette.mat(materials, "#C9CED6", 0.35, 0.5);
    let monitor_m = palette.mat(materials, "#1B1F26", 0.4, 0.1);
    let ped_m = palette.mat(materials, "#D8D3CA", 0.6, 0.0);
    let mug_m = palette.mat(materials, "#FFFFFF", 0.35, 0.0);
    let chair_m = palette.mat(materials, "#596170", 0.88, 0.0);
    let lamp_bulb = palette.emissive(materials, "#FFE8BA", 1.1);
    let divider_m = palette.mat(materials, "#6A7C8E", 0.9, 0.0);

    // Pod fabric screens between facing pairs on Pro/HQ
    for &[cx, cz] in &spec.pods {
        spawn_box_rot(
            commands,
            palette,
            divider_m.clone(),
            Vec3::new(cx, 0.98, cz),
            Vec3::new(2.96, 0.42, 0.045),
            Quat::IDENTITY,
        );
    }

    let preview_kinds = [
        ScreenKind::Code,
        ScreenKind::Term,
        ScreenKind::Read,
        ScreenKind::Chat,
        ScreenKind::Web,
        ScreenKind::Wait,
        ScreenKind::Code,
        ScreenKind::Sleep,
    ];

    for (i, d) in spec.desks.iter().enumerate() {
        let rot = Quat::from_rotation_y(d.ry);
        let at = |lx: f32, ly: f32, lz: f32| Vec3::new(d.x, 0.0, d.z) + rot * Vec3::new(lx, ly, lz);

        // Desk top & frame
        let desk_ent = spawn_box_rot(commands, palette, desk_top.clone(), at(0.0, 0.75, 0.0), Vec3::new(1.5, 0.05, 0.74), rot);
        commands.entity(desk_ent).insert(DeskSlot { index: i });

        for sx in [-0.68_f32, 0.68] {
            spawn_box_rot(commands, palette, metal.clone(), at(sx, 0.37, 0.3), Vec3::new(0.05, 0.72, 0.05), rot);
            spawn_box_rot(commands, palette, metal.clone(), at(sx, 0.37, -0.3), Vec3::new(0.05, 0.72, 0.05), rot);
        }
        spawn_box_rot(commands, palette, metal.clone(), at(0.0, 0.56, 0.33), Vec3::new(1.36, 0.3, 0.025), rot);
        spawn_box_rot(commands, palette, desk_mat.clone(), at(0.0, 0.778, -0.08), Vec3::new(0.95, 0.01, 0.42), rot);

        // Laptop base + lid + live screen
        spawn_box_rot(commands, palette, laptop_m.clone(), at(0.0, 0.795, -0.04), Vec3::new(0.56, 0.03, 0.38), rot);
        let lid_rot = rot * Quat::from_rotation_x(0.22);
        spawn_box_rot(commands, palette, laptop_m.clone(), at(0.0, 0.99, 0.16), Vec3::new(0.56, 0.38, 0.025), lid_rot);

        let scr_kind = if i < 8 {
            preview_kinds[i % preview_kinds.len()]
        } else {
            ScreenKind::Off
        };
        let scr_mat = screen_mats.by_kind[&scr_kind].clone();
        let laptop_scr = spawn_quad(
            commands,
            palette,
            scr_mat.clone(),
            at(0.0, 0.99, 0.14),
            Vec2::new(0.5, 0.32),
            rot * Quat::from_rotation_y(PI),
        );
        commands
            .entity(laptop_scr)
            .insert(DeskScreen { desk_index: i });

        // Mug
        spawn_cyl(commands, palette, mug_m.clone(), at(0.52, 0.83, 0.16), 0.11, 0.11);

        // Pro/HQ: secondary monitor & drawer pedestal
        if spec.layout.is_pro() {
            let mon_rot = rot * Quat::from_rotation_y(0.42);
            spawn_box_rot(commands, palette, metal.clone(), at(-0.5, 0.9, 0.2), Vec3::new(0.04, 0.24, 0.03), mon_rot);
            spawn_box_rot(commands, palette, monitor_m.clone(), at(-0.5, 1.1, 0.2), Vec3::new(0.5, 0.31, 0.03), mon_rot);
            let mon_scr = spawn_quad(
                commands,
                palette,
                scr_mat,
                at(-0.5, 1.1, 0.18),
                Vec2::new(0.46, 0.27),
                mon_rot * Quat::from_rotation_y(PI),
            );
            commands
                .entity(mon_scr)
                .insert(DeskScreen { desk_index: i });
            spawn_box_rot(commands, palette, ped_m.clone(), at(0.45, 0.29, 0.04), Vec3::new(0.4, 0.58, 0.55), rot);
        }

        // Desk lamp on every 3rd desk
        if i % 3 == 2 {
            spawn_cyl(commands, palette, metal.clone(), at(0.6, 0.96, -0.2), 0.03, 0.34);
            spawn_sphere(commands, palette, lamp_bulb.clone(), at(0.6, 1.12, -0.2), Vec3::splat(0.11));
        }

        // Rolling office chair
        spawn_cyl(commands, palette, metal.clone(), at(0.0, 0.24, -0.66), 0.06, 0.34);
        spawn_box_rot(commands, palette, chair_m.clone(), at(0.0, 0.445, -0.66), Vec3::new(0.54, 0.09, 0.52), rot);
        spawn_box_rot(commands, palette, chair_m.clone(), at(0.0, 0.78, -0.92), Vec3::new(0.5, 0.56, 0.09), rot);
    }

    screen_mats
}

pub fn count_desks_in_world(world: &mut World) -> usize {
    world.query_filtered::<&DeskSlot, With<WorldEntity>>().iter(world).count()
}
