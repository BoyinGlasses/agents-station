use crate::chibi::{ChibiDims, ChibiRig};
use crate::chibi_meshes::ChibiMeshCache;
use crate::palette::{WorldEntity, WorldPalette};
use bevy::prelude::*;
use std::f32::consts::{FRAC_PI_2, PI};

/// All 15+ held & carried props ported 1:1 from `chibi-rig.js`.
/// Spawned lazily on demand via `set_active_prop` — never pre-spawned hidden on every character.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum PropKind {
    Mug,
    Cup,
    Noodle,
    Tokbokki,
    Onigiri,
    BanhBao,
    IceCream,
    Chicken,
    Paddle,
    BatteryPack,
    Phone,
    Letter,
    Cake,
    HomeBox,
    Mic,
    Gamepad,
    Book,
    Folder,
    ParcelBox,
    Cue,
    Beer,
    Boba,
    BanhMi,
    Coffee,
    Paper,
    Umbrella,
    LunchBag,
    CarrierBag,
    GroceryBag,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PropMount {
    RightHand,
    LeftHand,
    SpineBook,
    SpineGamepad,
    SpineParcel,
}

impl PropKind {
    pub fn mount(self) -> PropMount {
        match self {
            PropKind::LunchBag | PropKind::CarrierBag | PropKind::GroceryBag => PropMount::LeftHand,
            PropKind::Book => PropMount::SpineBook,
            PropKind::Gamepad => PropMount::SpineGamepad,
            PropKind::ParcelBox => PropMount::SpineParcel,
            _ => PropMount::RightHand,
        }
    }
}

/// Stored on a chibi root entity to track its single lazily spawned active prop subtree.
#[derive(Component, Debug, Clone, Copy, PartialEq, Eq)]
pub struct ActivePropEntity {
    pub kind: PropKind,
    pub entity: Entity,
}

/// Marker placed on the root entity of a lazily spawned held prop subtree.
#[derive(Component, Debug, Clone, Copy, PartialEq, Eq)]
pub struct HeldPropRoot {
    pub kind: PropKind,
}

struct PropPart {
    mesh: Handle<Mesh>,
    mat: Handle<StandardMaterial>,
    transform: Transform,
}

fn part(
    mesh: Handle<Mesh>,
    mat: Handle<StandardMaterial>,
    pos: Vec3,
    rot: Quat,
    scale: Vec3,
) -> PropPart {
    PropPart {
        mesh,
        mat,
        transform: Transform {
            translation: pos,
            rotation: rot,
            scale,
        },
    }
}

fn build_prop_parts(
    cache: &ChibiMeshCache,
    palette: &mut WorldPalette,
    materials: &mut Assets<StandardMaterial>,
    kind: PropKind,
) -> Vec<PropPart> {
    let mut parts = Vec::with_capacity(8);
    let mut m = |hex: &str, rough: f32, metal: f32| palette.mat(materials, hex, rough, metal);

    match kind {
        PropKind::Mug => {
            let white = m("#FFFFFF", 0.55, 0.0);
            let coffee = m("#4B2E1E", 0.3, 0.0);
            parts.push(part(
                cache.cyl.clone(),
                white.clone(),
                Vec3::ZERO,
                Quat::IDENTITY,
                Vec3::new(0.052, 0.11, 0.052),
            ));
            parts.push(part(
                cache.cyl.clone(),
                coffee,
                Vec3::new(0.0, 0.048, 0.0),
                Quat::IDENTITY,
                Vec3::new(0.044, 0.016, 0.044),
            ));
            parts.push(part(
                cache.torus_thick.clone(),
                white,
                Vec3::new(0.064, 0.0, 0.0),
                Quat::IDENTITY,
                Vec3::splat(0.034),
            ));
        }
        PropKind::Cup | PropKind::Coffee => {
            let drink = m(
                if kind == PropKind::Coffee { "#6F4326" } else { "#B98A5E" },
                0.35,
                0.0,
            );
            let ice = m("#E6EEF2", 0.2, 0.0);
            let lid = m("#F2F6F8", 0.2, 0.0);
            let straw = m("#2E8B57", 0.6, 0.0);
            let band = m("#E21F26", 0.85, 0.0);
            parts.push(part(
                cache.cyl.clone(),
                drink,
                Vec3::new(0.0, -0.015, 0.0),
                Quat::IDENTITY,
                Vec3::new(0.050, 0.10, 0.050),
            ));
            parts.push(part(
                cache.cyl.clone(),
                ice,
                Vec3::new(0.0, 0.06, 0.0),
                Quat::IDENTITY,
                Vec3::new(0.058, 0.05, 0.058),
            ));
            parts.push(part(
                cache.hemi_dome.clone(),
                lid,
                Vec3::new(0.0, 0.085, 0.0),
                Quat::IDENTITY,
                Vec3::new(0.062, 0.037, 0.062),
            ));
            parts.push(part(
                cache.cyl.clone(),
                straw,
                Vec3::new(0.012, 0.15, 0.0),
                Quat::from_rotation_z(-0.15),
                Vec3::new(0.007, 0.13, 0.007),
            ));
            parts.push(part(
                cache.cyl.clone(),
                band,
                Vec3::ZERO,
                Quat::IDENTITY,
                Vec3::new(0.053, 0.035, 0.053),
            ));
        }
        PropKind::Boba => {
            let tea = m("#D8A677", 0.35, 0.0);
            let pearl = m("#241B18", 0.25, 0.0);
            let lid = m("#F4F7FA", 0.2, 0.0);
            let straw = m("#1F2329", 0.5, 0.0);
            parts.push(part(
                cache.cyl.clone(),
                tea,
                Vec3::new(0.0, 0.01, 0.0),
                Quat::IDENTITY,
                Vec3::new(0.054, 0.13, 0.054),
            ));
            parts.push(part(
                cache.cyl.clone(),
                pearl,
                Vec3::new(0.0, -0.04, 0.0),
                Quat::IDENTITY,
                Vec3::new(0.050, 0.03, 0.050),
            ));
            parts.push(part(
                cache.hemi_dome.clone(),
                lid,
                Vec3::new(0.0, 0.075, 0.0),
                Quat::IDENTITY,
                Vec3::new(0.058, 0.032, 0.058),
            ));
            parts.push(part(
                cache.cyl.clone(),
                straw,
                Vec3::new(0.01, 0.145, 0.0),
                Quat::from_rotation_z(-0.12),
                Vec3::new(0.011, 0.14, 0.011),
            ));
        }
        PropKind::Noodle => {
            let paper = m("#FBF8F1", 0.75, 0.0);
            let red = m("#E3412B", 0.7, 0.0);
            let noodles = m("#E8C27A", 0.8, 0.0);
            let lid = m("#F2D24B", 0.5, 0.0);
            let stick = m("#D9B98A", 0.8, 0.0);
            parts.push(part(
                cache.cyl.clone(),
                paper,
                Vec3::new(0.0, -0.01, 0.0),
                Quat::IDENTITY,
                Vec3::new(0.050, 0.10, 0.050),
            ));
            parts.push(part(
                cache.cyl.clone(),
                red,
                Vec3::new(0.0, 0.004, 0.0),
                Quat::IDENTITY,
                Vec3::new(0.054, 0.04, 0.054),
            ));
            parts.push(part(
                cache.cyl.clone(),
                noodles,
                Vec3::new(0.0, 0.037, 0.0),
                Quat::IDENTITY,
                Vec3::new(0.05, 0.006, 0.05),
            ));
            parts.push(part(
                cache.half_brim.clone(),
                lid,
                Vec3::new(0.0, 0.041, 0.0),
                Quat::from_rotation_x(1.15),
                Vec3::new(0.057, 0.004, 0.057),
            ));
            for x in [-0.009_f32, 0.009] {
                parts.push(part(
                    cache.cyl.clone(),
                    stick.clone(),
                    Vec3::new(x, 0.09, 0.012),
                    Quat::from_rotation_z(x * 5.0),
                    Vec3::new(0.0035, 0.19, 0.0035),
                ));
            }
            parts.push(part(
                cache.plane.clone(),
                cache.steam_mat.clone(),
                Vec3::new(0.0, 0.13, 0.0),
                Quat::IDENTITY,
                Vec3::splat(0.11),
            ));
        }
        PropKind::Tokbokki => {
            let paper = m("#FBF8F1", 0.75, 0.0);
            let blue = m("#0068B7", 0.8, 0.0);
            let sauce = m("#C8321F", 0.4, 0.0);
            let cake = m("#E0503A", 0.4, 0.0);
            let stick = m("#D9B98A", 0.8, 0.0);
            let oden = m("#D9A65B", 0.8, 0.0);
            parts.push(part(
                cache.cyl.clone(),
                paper,
                Vec3::new(0.0, -0.012, 0.0),
                Quat::IDENTITY,
                Vec3::new(0.052, 0.095, 0.052),
            ));
            parts.push(part(
                cache.cyl.clone(),
                blue,
                Vec3::new(0.0, 0.018, 0.0),
                Quat::IDENTITY,
                Vec3::new(0.056, 0.03, 0.056),
            ));
            parts.push(part(
                cache.cyl.clone(),
                sauce,
                Vec3::new(0.0, 0.03, 0.0),
                Quat::IDENTITY,
                Vec3::new(0.052, 0.012, 0.052),
            ));
            for (x, z, r) in [(-0.02, 0.012, 0.3), (0.014, -0.014, -0.6), (0.022, 0.018, 1.2)] {
                parts.push(part(
                    cache.cyl.clone(),
                    cake.clone(),
                    Vec3::new(x, 0.04, z),
                    Quat::from_euler(EulerRot::XYZ, FRAC_PI_2, r, 0.0),
                    Vec3::new(0.009, 0.042, 0.009),
                ));
            }
            parts.push(part(
                cache.cyl.clone(),
                stick,
                Vec3::new(-0.012, 0.10, 0.0),
                Quat::from_rotation_z(0.12),
                Vec3::new(0.0025, 0.20, 0.0025),
            ));
            parts.push(part(
                cache.box_mesh.clone(),
                oden,
                Vec3::new(-0.019, 0.155, 0.0),
                Quat::from_rotation_z(0.12),
                Vec3::new(0.05, 0.06, 0.008),
            ));
        }
        PropKind::Onigiri => {
            let rice = m("#F7F6F0", 0.5, 0.0);
            let nori = m("#1F2A22", 0.7, 0.0);
            let strip = m("#E2241B", 0.6, 0.0);
            parts.push(part(
                cache.tri_prism.clone(),
                rice,
                Vec3::new(0.0, 0.035, 0.0),
                Quat::from_rotation_z(PI) * Quat::from_rotation_x(FRAC_PI_2),
                Vec3::new(0.058, 0.034, 0.058),
            ));
            parts.push(part(
                cache.box_mesh.clone(),
                nori,
                Vec3::new(0.0, 0.016, 0.0),
                Quat::IDENTITY,
                Vec3::new(0.05, 0.034, 0.037),
            ));
            parts.push(part(
                cache.box_mesh.clone(),
                strip,
                Vec3::new(0.0, 0.04, 0.0),
                Quat::IDENTITY,
                Vec3::new(0.008, 0.07, 0.038),
            ));
        }
        PropKind::BanhBao => {
            let base = m("#E3D5B5", 0.9, 0.0);
            let dough = m("#FBF5E8", 0.9, 0.0);
            parts.push(part(
                cache.cyl.clone(),
                base,
                Vec3::new(0.0, -0.01, 0.0),
                Quat::IDENTITY,
                Vec3::new(0.05, 0.004, 0.05),
            ));
            parts.push(part(
                cache.sphere.clone(),
                dough.clone(),
                Vec3::new(0.0, 0.026, 0.0),
                Quat::IDENTITY,
                Vec3::new(0.054, 0.039, 0.054),
            ));
            parts.push(part(
                cache.cone.clone(),
                dough,
                Vec3::new(0.0, 0.068, 0.0),
                Quat::IDENTITY,
                Vec3::new(0.034, 0.02, 0.034),
            ));
        }
        PropKind::IceCream => {
            let waffle = m("#D49A55", 0.8, 0.0);
            let cream = m("#FFF8EC", 0.55, 0.0);
            parts.push(part(
                cache.cone.clone(),
                waffle,
                Vec3::new(0.0, -0.02, 0.0),
                Quat::from_rotation_x(PI),
                Vec3::new(0.068, 0.11, 0.068),
            ));
            parts.push(part(
                cache.sphere.clone(),
                cream.clone(),
                Vec3::new(0.0, 0.045, 0.0),
                Quat::IDENTITY,
                Vec3::new(0.038, 0.030, 0.038),
            ));
            parts.push(part(
                cache.cone.clone(),
                cream,
                Vec3::new(0.0, 0.088, 0.0),
                Quat::IDENTITY,
                Vec3::new(0.06, 0.05, 0.06),
            ));
        }
        PropKind::Chicken => {
            let paper = m("#FBF8F1", 0.75, 0.0);
            let blue = m("#003894", 0.8, 0.0);
            let meat = m("#B8702E", 0.6, 0.0);
            let bone = m("#F4EEE2", 0.6, 0.0);
            parts.push(part(
                cache.box_mesh.clone(),
                paper,
                Vec3::ZERO,
                Quat::IDENTITY,
                Vec3::new(0.08, 0.09, 0.05),
            ));
            parts.push(part(
                cache.box_mesh.clone(),
                blue,
                Vec3::new(0.0, 0.022, 0.0),
                Quat::IDENTITY,
                Vec3::new(0.081, 0.024, 0.051),
            ));
            parts.push(part(
                cache.capsule.clone(),
                meat,
                Vec3::new(0.008, 0.07, 0.0),
                Quat::from_rotation_z(0.3),
                Vec3::new(0.044, 0.04, 0.044),
            ));
            parts.push(part(
                cache.cyl.clone(),
                bone,
                Vec3::new(-0.006, 0.112, 0.0),
                Quat::from_rotation_z(0.3),
                Vec3::new(0.006, 0.03, 0.006),
            ));
        }
        PropKind::Paddle => {
            let face = m("#F2C230", 0.45, 0.0);
            let edge = m("#1E2125", 0.5, 0.0);
            parts.push(part(
                cache.cyl.clone(),
                edge.clone(),
                Vec3::new(0.0, 0.03, 0.0),
                Quat::IDENTITY,
                Vec3::new(0.015, 0.10, 0.015),
            ));
            parts.push(part(
                cache.box_mesh.clone(),
                face,
                Vec3::new(0.0, 0.17, 0.0),
                Quat::IDENTITY,
                Vec3::new(0.15, 0.19, 0.012),
            ));
            for x in [-0.077_f32, 0.077] {
                parts.push(part(
                    cache.box_mesh.clone(),
                    edge.clone(),
                    Vec3::new(x, 0.17, 0.0),
                    Quat::IDENTITY,
                    Vec3::new(0.008, 0.19, 0.016),
                ));
            }
            parts.push(part(
                cache.box_mesh.clone(),
                edge,
                Vec3::new(0.0, 0.266, 0.0),
                Quat::IDENTITY,
                Vec3::new(0.16, 0.008, 0.016),
            ));
        }
        PropKind::BatteryPack => {
            let case = m("#2B2F36", 0.5, 0.1);
            let stripe = m("#00A651", 0.5, 0.0);
            let handle = m("#1C1E22", 0.5, 0.0);
            parts.push(part(
                cache.box_mesh.clone(),
                case,
                Vec3::new(0.0, -0.08, 0.0),
                Quat::IDENTITY,
                Vec3::new(0.15, 0.17, 0.09),
            ));
            parts.push(part(
                cache.box_mesh.clone(),
                stripe,
                Vec3::new(0.0, -0.04, 0.0),
                Quat::IDENTITY,
                Vec3::new(0.152, 0.02, 0.092),
            ));
            parts.push(part(
                cache.arc_pi_med.clone(),
                handle,
                Vec3::new(0.0, 0.005, 0.0),
                Quat::IDENTITY,
                Vec3::splat(0.035),
            ));
        }
        PropKind::Phone => {
            let body = m("#1C1F25", 0.35, 0.15);
            let screen = palette.emissive(materials, "#9ED8FF", 0.85);
            parts.push(part(
                cache.box_mesh.clone(),
                body,
                Vec3::new(0.0, 0.05, 0.0),
                Quat::IDENTITY,
                Vec3::new(0.058, 0.11, 0.009),
            ));
            parts.push(part(
                cache.plane.clone(),
                screen,
                Vec3::new(0.0, 0.05, -0.0052),
                Quat::from_rotation_y(PI),
                Vec3::new(0.048, 0.096, 1.0),
            ));
        }
        PropKind::Letter => {
            let env = m("#FBFAF5", 0.8, 0.0);
            let red = m("#D9342B", 0.6, 0.0);
            let blue = m("#2E5BBA", 0.6, 0.0);
            let stamp = m("#E8742A", 0.6, 0.0);
            parts.push(part(
                cache.box_mesh.clone(),
                env,
                Vec3::new(0.0, 0.03, 0.0),
                Quat::IDENTITY,
                Vec3::new(0.12, 0.08, 0.006),
            ));
            parts.push(part(
                cache.box_mesh.clone(),
                red,
                Vec3::new(0.0, 0.068, 0.0),
                Quat::IDENTITY,
                Vec3::new(0.12, 0.006, 0.007),
            ));
            parts.push(part(
                cache.box_mesh.clone(),
                blue,
                Vec3::new(0.0, -0.008, 0.0),
                Quat::IDENTITY,
                Vec3::new(0.12, 0.006, 0.007),
            ));
            parts.push(part(
                cache.box_mesh.clone(),
                stamp,
                Vec3::new(0.042, 0.052, 0.001),
                Quat::IDENTITY,
                Vec3::new(0.022, 0.026, 0.007),
            ));
        }
        PropKind::Cake => {
            let plate = m("#FBFAF5", 0.8, 0.0);
            let sponge = m("#F3B6C4", 0.7, 0.0);
            let cream = m("#FFF6EC", 0.6, 0.0);
            let cherry = m("#D8322F", 0.4, 0.0);
            parts.push(part(
                cache.cyl.clone(),
                plate,
                Vec3::new(0.0, -0.012, 0.0),
                Quat::IDENTITY,
                Vec3::new(0.075, 0.006, 0.075),
            ));
            parts.push(part(
                cache.cake_wedge.clone(),
                sponge,
                Vec3::new(-0.012, 0.014, -0.012),
                Quat::IDENTITY,
                Vec3::new(0.055, 0.045, 0.055),
            ));
            parts.push(part(
                cache.cake_wedge.clone(),
                cream,
                Vec3::new(-0.012, 0.041, -0.012),
                Quat::IDENTITY,
                Vec3::new(0.056, 0.01, 0.056),
            ));
            parts.push(part(
                cache.sphere.clone(),
                cherry,
                Vec3::new(0.004, 0.05, 0.004),
                Quat::IDENTITY,
                Vec3::splat(0.01),
            ));
        }
        PropKind::HomeBox => {
            let tub = m("#EEF1F2", 0.4, 0.0);
            let rice = m("#F4EFE2", 0.8, 0.0);
            let lid = m("#2F7FD0", 0.45, 0.0);
            parts.push(part(
                cache.box_mesh.clone(),
                tub,
                Vec3::new(0.0, 0.02, 0.0),
                Quat::IDENTITY,
                Vec3::new(0.15, 0.05, 0.105),
            ));
            parts.push(part(
                cache.box_mesh.clone(),
                rice,
                Vec3::new(0.0, 0.025, 0.0),
                Quat::IDENTITY,
                Vec3::new(0.13, 0.03, 0.085),
            ));
            parts.push(part(
                cache.box_mesh.clone(),
                lid,
                Vec3::new(0.0, 0.052, 0.0),
                Quat::IDENTITY,
                Vec3::new(0.156, 0.014, 0.11),
            ));
            parts.push(part(
                cache.plane.clone(),
                cache.steam_mat.clone(),
                Vec3::new(0.0, 0.11, 0.0),
                Quat::IDENTITY,
                Vec3::splat(0.09),
            ));
        }
        PropKind::Mic => {
            let grip = m("#1C1D21", 0.5, 0.1);
            let collar = m("#D7DBE0", 0.25, 0.7);
            let head = m("#AEB4BC", 0.45, 0.6);
            parts.push(part(
                cache.cyl.clone(),
                grip,
                Vec3::new(0.0, 0.02, 0.0),
                Quat::IDENTITY,
                Vec3::new(0.015, 0.13, 0.015),
            ));
            parts.push(part(
                cache.cyl.clone(),
                collar,
                Vec3::new(0.0, 0.095, 0.0),
                Quat::IDENTITY,
                Vec3::new(0.020, 0.025, 0.020),
            ));
            parts.push(part(
                cache.sphere.clone(),
                head,
                Vec3::new(0.0, 0.125, 0.0),
                Quat::IDENTITY,
                Vec3::splat(0.031),
            ));
        }
        PropKind::Gamepad => {
            let body = m("#F2F3F5", 0.45, 0.0);
            let trim = m("#1C1F25", 0.55, 0.0);
            let glow = palette.emissive(materials, "#5FA8FF", 1.2);
            parts.push(part(
                cache.box_mesh.clone(),
                body.clone(),
                Vec3::ZERO,
                Quat::IDENTITY,
                Vec3::new(0.15, 0.03, 0.065),
            ));
            for sx in [-1.0_f32, 1.0] {
                parts.push(part(
                    cache.capsule.clone(),
                    body.clone(),
                    Vec3::new(sx * 0.062, -0.006, 0.035),
                    Quat::from_euler(EulerRot::XYZ, FRAC_PI_2 + 0.35, 0.0, -sx * 0.35),
                    Vec3::new(0.044, 0.04, 0.044),
                ));
                parts.push(part(
                    cache.box_mesh.clone(),
                    trim.clone(),
                    Vec3::new(sx * 0.03, 0.02, 0.012),
                    Quat::IDENTITY,
                    Vec3::new(0.018, 0.012, 0.018),
                ));
            }
            parts.push(part(
                cache.box_mesh.clone(),
                trim,
                Vec3::new(0.0, 0.016, -0.014),
                Quat::IDENTITY,
                Vec3::new(0.05, 0.004, 0.028),
            ));
            parts.push(part(
                cache.box_mesh.clone(),
                glow,
                Vec3::new(0.0, 0.012, -0.034),
                Quat::IDENTITY,
                Vec3::new(0.05, 0.004, 0.006),
            ));
        }
        PropKind::Book => {
            let cover = m("#C8553D", 0.65, 0.0);
            let page = m("#FBF8F0", 0.85, 0.0);
            for sx in [-1.0_f32, 1.0] {
                parts.push(part(
                    cache.box_mesh.clone(),
                    cover.clone(),
                    Vec3::new(sx * 0.09, 0.0, 0.0),
                    Quat::from_rotation_z(-sx * 0.18),
                    Vec3::new(0.18, 0.018, 0.25),
                ));
                parts.push(part(
                    cache.box_mesh.clone(),
                    page.clone(),
                    Vec3::new(sx * 0.09, 0.015, 0.0),
                    Quat::from_rotation_z(-sx * 0.18),
                    Vec3::new(0.16, 0.018, 0.23),
                ));
            }
        }
        PropKind::Folder => {
            let folder = m("#D8A657", 0.75, 0.0);
            let sheet = m("#FAF8F2", 0.85, 0.0);
            parts.push(part(
                cache.box_mesh.clone(),
                folder,
                Vec3::new(0.0, 0.06, 0.0),
                Quat::IDENTITY,
                Vec3::new(0.18, 0.24, 0.016),
            ));
            parts.push(part(
                cache.box_mesh.clone(),
                sheet,
                Vec3::new(0.0, 0.068, 0.003),
                Quat::IDENTITY,
                Vec3::new(0.16, 0.22, 0.012),
            ));
        }
        PropKind::Paper => {
            let sheet = m("#FBFAF4", 0.82, 0.0);
            let ink = m("#4B5563", 0.8, 0.0);
            parts.push(part(
                cache.box_mesh.clone(),
                sheet,
                Vec3::new(0.0, 0.06, 0.0),
                Quat::IDENTITY,
                Vec3::new(0.15, 0.20, 0.005),
            ));
            parts.push(part(
                cache.box_mesh.clone(),
                ink,
                Vec3::new(0.0, 0.10, 0.0035),
                Quat::IDENTITY,
                Vec3::new(0.10, 0.012, 0.002),
            ));
        }
        PropKind::ParcelBox => {
            let card = m("#C99A6B", 0.85, 0.0);
            let tape = m("#E5652E", 0.65, 0.0);
            parts.push(part(
                cache.box_mesh.clone(),
                card,
                Vec3::ZERO,
                Quat::IDENTITY,
                Vec3::new(0.32, 0.24, 0.26),
            ));
            parts.push(part(
                cache.box_mesh.clone(),
                tape,
                Vec3::new(0.0, 0.121, 0.0),
                Quat::IDENTITY,
                Vec3::new(0.322, 0.006, 0.045),
            ));
        }
        PropKind::Cue => {
            let shaft = m("#E8CFA3", 0.45, 0.0);
            let butt = m("#2E2420", 0.5, 0.0);
            let tip = m("#4B9CD3", 0.6, 0.0);
            parts.push(part(
                cache.cyl.clone(),
                butt,
                Vec3::new(0.0, 0.0, -0.18),
                Quat::from_rotation_x(FRAC_PI_2),
                Vec3::new(0.013, 0.42, 0.013),
            ));
            parts.push(part(
                cache.cyl.clone(),
                shaft,
                Vec3::new(0.0, 0.0, 0.34),
                Quat::from_rotation_x(FRAC_PI_2),
                Vec3::new(0.009, 0.64, 0.009),
            ));
            parts.push(part(
                cache.cyl.clone(),
                tip,
                Vec3::new(0.0, 0.0, 0.67),
                Quat::from_rotation_x(FRAC_PI_2),
                Vec3::new(0.008, 0.02, 0.008),
            ));
        }
        PropKind::Beer => {
            let glass = m("#E7A826", 0.25, 0.0);
            let foam = m("#FFFDF6", 0.75, 0.0);
            parts.push(part(
                cache.cyl.clone(),
                glass,
                Vec3::new(0.0, 0.02, 0.0),
                Quat::IDENTITY,
                Vec3::new(0.048, 0.12, 0.048),
            ));
            parts.push(part(
                cache.cyl.clone(),
                foam.clone(),
                Vec3::new(0.0, 0.09, 0.0),
                Quat::IDENTITY,
                Vec3::new(0.050, 0.03, 0.050),
            ));
            parts.push(part(
                cache.torus_thick.clone(),
                foam,
                Vec3::new(0.056, 0.02, 0.0),
                Quat::IDENTITY,
                Vec3::splat(0.03),
            ));
        }
        PropKind::BanhMi => {
            let bread = m("#D89A4B", 0.75, 0.0);
            let sleeve = m("#FBF6EC", 0.85, 0.0);
            let herb = m("#4A9B43", 0.8, 0.0);
            parts.push(part(
                cache.capsule.clone(),
                bread,
                Vec3::new(0.0, 0.05, 0.0),
                Quat::from_rotation_z(0.18),
                Vec3::new(0.068, 0.12, 0.058),
            ));
            parts.push(part(
                cache.cyl.clone(),
                sleeve,
                Vec3::new(0.0, 0.0, 0.0),
                Quat::from_rotation_z(0.18),
                Vec3::new(0.037, 0.10, 0.032),
            ));
            parts.push(part(
                cache.sphere.clone(),
                herb,
                Vec3::new(-0.01, 0.105, 0.018),
                Quat::IDENTITY,
                Vec3::new(0.022, 0.018, 0.016),
            ));
        }
        PropKind::Umbrella => {
            let pole = m("#2B2F36", 0.4, 0.5);
            let canopy = m("#3B78C4", 0.6, 0.0);
            parts.push(part(
                cache.cyl.clone(),
                pole,
                Vec3::new(0.0, 0.36, 0.0),
                Quat::IDENTITY,
                Vec3::new(0.010, 0.74, 0.010),
            ));
            parts.push(part(
                cache.hemi_dome.clone(),
                canopy,
                Vec3::new(0.0, 0.58, 0.0),
                Quat::IDENTITY,
                Vec3::new(0.46, 0.19, 0.46),
            ));
        }
        PropKind::LunchBag => {
            let bag = m("#2D4A73", 0.85, 0.0);
            let zip = m("#C9CED6", 0.35, 0.4);
            parts.push(part(
                cache.box_mesh.clone(),
                bag.clone(),
                Vec3::new(0.0, -0.15, 0.0),
                Quat::IDENTITY,
                Vec3::new(0.21, 0.16, 0.13),
            ));
            parts.push(part(
                cache.box_mesh.clone(),
                zip,
                Vec3::new(0.0, -0.072, 0.0),
                Quat::IDENTITY,
                Vec3::new(0.214, 0.01, 0.134),
            ));
            parts.push(part(
                cache.arc_pi_med.clone(),
                bag,
                Vec3::new(0.0, -0.068, 0.0),
                Quat::IDENTITY,
                Vec3::splat(0.045),
            ));
        }
        PropKind::CarrierBag => {
            let bag = m("#F4F3EE", 0.75, 0.0);
            let lid = m("#FFFFFF", 0.5, 0.0);
            parts.push(part(
                cache.box_mesh.clone(),
                bag.clone(),
                Vec3::new(0.0, -0.19, 0.0),
                Quat::IDENTITY,
                Vec3::new(0.27, 0.22, 0.15),
            ));
            parts.push(part(
                cache.box_mesh.clone(),
                lid,
                Vec3::new(0.0, -0.07, 0.0),
                Quat::IDENTITY,
                Vec3::new(0.24, 0.025, 0.13),
            ));
            for x in [-0.07_f32, 0.07] {
                parts.push(part(
                    cache.arc_pi_thin.clone(),
                    bag.clone(),
                    Vec3::new(x, -0.075, 0.0),
                    Quat::IDENTITY,
                    Vec3::splat(0.045),
                ));
            }
        }
        PropKind::GroceryBag => {
            let plastic = m("#F6F6F2", 0.45, 0.0);
            let greens_a = m("#5BAA3A", 0.8, 0.0);
            let greens_b = m("#3E8A2C", 0.8, 0.0);
            let carrot = m("#E8742A", 0.6, 0.0);
            let band = m("#E21B2A", 0.8, 0.0);
            parts.push(part(
                cache.box_mesh.clone(),
                plastic.clone(),
                Vec3::new(0.0, -0.18, 0.0),
                Quat::IDENTITY,
                Vec3::new(0.22, 0.19, 0.13),
            ));
            for x in [-0.06_f32, 0.06] {
                parts.push(part(
                    cache.arc_pi_thin.clone(),
                    plastic.clone(),
                    Vec3::new(x, -0.08, 0.0),
                    Quat::IDENTITY,
                    Vec3::splat(0.04),
                ));
            }
            parts.push(part(
                cache.sphere.clone(),
                greens_a,
                Vec3::new(-0.04, -0.07, 0.0),
                Quat::IDENTITY,
                Vec3::new(0.06, 0.075, 0.045),
            ));
            parts.push(part(
                cache.sphere.clone(),
                greens_b,
                Vec3::new(0.035, -0.08, 0.01),
                Quat::IDENTITY,
                Vec3::new(0.045, 0.054, 0.036),
            ));
            parts.push(part(
                cache.cone.clone(),
                carrot,
                Vec3::new(0.07, -0.06, -0.02),
                Quat::from_rotation_z(-0.4) * Quat::from_rotation_x(PI),
                Vec3::new(0.028, 0.10, 0.028),
            ));
            parts.push(part(
                cache.box_mesh.clone(),
                band,
                Vec3::new(0.0, -0.20, 0.0),
                Quat::IDENTITY,
                Vec3::new(0.222, 0.035, 0.132),
            ));
        }
    }

    parts
}

/// Lazily spawns ONLY the single active prop subtree parented to the chibi's mount joint
/// (despawning any previous active prop first) and despawns cleanly when `prop` is `None`.
pub fn set_active_prop(
    commands: &mut Commands,
    cache: &ChibiMeshCache,
    palette: &mut WorldPalette,
    materials: &mut Assets<StandardMaterial>,
    chibi: Entity,
    prop: Option<PropKind>,
) {
    let built_parts = prop.map(|k| build_prop_parts(cache, palette, materials, k));

    commands.queue(move |world: &mut World| {
        if world.get_entity(chibi).is_err() {
            return;
        }
        if let Some(active) = world.get::<ActivePropEntity>(chibi).copied() {
            if Some(active.kind) == prop && world.get_entity(active.entity).is_ok() {
                return;
            }
            if world.get_entity(active.entity).is_ok() {
                world.despawn(active.entity);
            }
            if let Ok(mut e) = world.get_entity_mut(chibi) {
                e.remove::<ActivePropEntity>();
            }
        }

        let (Some(kind), Some(parts)) = (prop, built_parts) else {
            return;
        };

        let (mount_entity, root_tf) = if let Some(rig) = world.get::<ChibiRig>(chibi).copied() {
            let d = ChibiDims::for_look(rig.look);
            match kind.mount() {
                PropMount::RightHand => (
                    rig.hand_r,
                    Transform::from_xyz(0.0, -0.02, d.hand_r * 0.9),
                ),
                PropMount::LeftHand => (rig.hand_l, Transform::from_xyz(0.0, -0.02, 0.0)),
                PropMount::SpineBook => (
                    rig.spine,
                    Transform::from_xyz(0.0, d.sh_y - 0.20, 0.27)
                        .with_rotation(Quat::from_rotation_x(-1.0)),
                ),
                PropMount::SpineGamepad => (
                    rig.spine,
                    Transform::from_xyz(0.0, d.sh_y - 0.30, 0.24)
                        .with_rotation(Quat::from_rotation_x(-0.55)),
                ),
                PropMount::SpineParcel => (
                    rig.spine,
                    Transform::from_xyz(0.0, d.sh_y - 0.34, 0.30),
                ),
            }
        } else {
            (chibi, Transform::IDENTITY)
        };

        let prop_root = world
            .spawn((
                WorldEntity,
                HeldPropRoot { kind },
                root_tf,
                Visibility::Visible,
            ))
            .id();

        for p in parts {
            let child = world
                .spawn((
                    WorldEntity,
                    Mesh3d(p.mesh),
                    MeshMaterial3d(p.mat),
                    p.transform,
                ))
                .id();
            world.entity_mut(prop_root).add_child(child);
        }

        if world.get_entity(mount_entity).is_ok() {
            world.entity_mut(mount_entity).add_child(prop_root);
        }
        if let Ok(mut e) = world.get_entity_mut(chibi) {
            e.insert(ActivePropEntity {
                kind,
                entity: prop_root,
            });
        }
    });
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::palette::WorldEntity;

    #[test]
    fn chibi_set_active_prop_lazily_spawns_switches_and_despawns_cleanly() {
        let mut world = World::new();
        world.insert_resource(Assets::<Mesh>::default());
        world.insert_resource(Assets::<StandardMaterial>::default());
        world.insert_resource(Assets::<Image>::default());

        let (cache, mut palette) = world.resource_scope(|world, mut meshes: Mut<Assets<Mesh>>| {
            world.resource_scope(|world, mut mats: Mut<Assets<StandardMaterial>>| {
                world.resource_scope(|_world, mut imgs: Mut<Assets<Image>>| {
                    let cache = ChibiMeshCache::build(&mut meshes, &mut mats, &mut imgs);
                    let palette = WorldPalette::new(&mut meshes);
                    (cache, palette)
                })
            })
        });

        let chibi = world
            .spawn((WorldEntity, Transform::default(), Visibility::Visible))
            .id();
        let baseline_entities = world
            .query_filtered::<Entity, With<WorldEntity>>()
            .iter(&world)
            .count();

        // 1. Attach Mug -> spawns exactly 1 HeldPropRoot and ActivePropEntity on chibi
        world.resource_scope(|world, mut mats: Mut<Assets<StandardMaterial>>| {
            let mut cmds = world.commands();
            set_active_prop(
                &mut cmds,
                &cache,
                &mut palette,
                &mut mats,
                chibi,
                Some(PropKind::Mug),
            );
        });
        world.flush();

        assert_eq!(
            world.query::<&HeldPropRoot>().iter(&world).count(),
            1,
            "exactly 1 prop root must be spawned"
        );
        assert_eq!(
            world.get::<ActivePropEntity>(chibi).map(|a| a.kind),
            Some(PropKind::Mug)
        );

        // 2. Switch to Paddle -> old Mug despawned, exactly 1 HeldPropRoot (Paddle) remains
        world.resource_scope(|world, mut mats: Mut<Assets<StandardMaterial>>| {
            let mut cmds = world.commands();
            set_active_prop(
                &mut cmds,
                &cache,
                &mut palette,
                &mut mats,
                chibi,
                Some(PropKind::Paddle),
            );
        });
        world.flush();

        let roots: Vec<PropKind> = world
            .query::<&HeldPropRoot>()
            .iter(&world)
            .map(|r| r.kind)
            .collect();
        assert_eq!(roots, vec![PropKind::Paddle]);
        assert_eq!(
            world.get::<ActivePropEntity>(chibi).map(|a| a.kind),
            Some(PropKind::Paddle)
        );

        // 3. Clear to None -> despawns prop + children back to exact baseline entity count
        world.resource_scope(|world, mut mats: Mut<Assets<StandardMaterial>>| {
            let mut cmds = world.commands();
            set_active_prop(&mut cmds, &cache, &mut palette, &mut mats, chibi, None);
        });
        world.flush();

        assert_eq!(world.query::<&HeldPropRoot>().iter(&world).count(), 0);
        assert!(world.get::<ActivePropEntity>(chibi).is_none());
        assert_eq!(
            world
                .query_filtered::<Entity, With<WorldEntity>>()
                .iter(&world)
                .count(),
            baseline_entities,
            "clearing prop to None must leave 0 orphan entities"
        );
    }
}
