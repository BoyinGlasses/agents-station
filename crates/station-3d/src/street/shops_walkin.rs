//! All 10 city-selectable walk-in 3D ground-floor Vietnamese shops (`shops.js`, `shop-cong.js`,
//! `shop-highlands.js`, `shop-phuclong.js`, `shop-mixue.js`, `shop-tgdd.js`, `shop-dmx.js`,
//! `shop-katinat.js`, `shop-longchau.js`).

use crate::backdrops::{BackdropAssets, BackdropEntity};
use crate::chibi_props::PropKind;
use crate::palette::WorldEntity;
use crate::street::city_grid::{SHOP_CEIL, SHOP_W, ShopDir, ShopFront, shop_depth};
use crate::street::{GY, StreetMeshBuilder, spawn_backdrop_uv_quad};
use bevy::prelude::*;
use station_canvas2d::street_surfaces::{draw_generic_shop_strip, draw_shop_cell};
use station_canvas2d::{BrandRegistry, FontEngine};
use std::collections::HashMap;
use std::f32::consts::FRAC_PI_2;

/// Walkable ground-floor room & open doorway cutout for a walk-in shop so first-person players
/// can walk right off the pavement through the open doorway into the shop interior.
#[derive(Debug, Clone, Copy)]
pub struct WalkInRoomBounds {
    pub chain: &'static str,
    pub room_min: Vec2,
    pub room_max: Vec2,
    pub door_min: Vec2,
    pub door_max: Vec2,
}

impl WalkInRoomBounds {
    pub fn contains_walkable(&self, x: f32, z: f32) -> bool {
        let in_room = x >= self.room_min.x
            && x <= self.room_max.x
            && z >= self.room_min.y
            && z <= self.room_max.y;
        let in_door = x >= self.door_min.x
            && x <= self.door_max.x
            && z >= self.door_min.y
            && z <= self.door_max.y;
        in_room || in_door
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum ShopSpotKind {
    Take {
        item_id: &'static str,
        price_k: u32,
        carry: PropKind,
    },
    Pay {
        default_item_id: &'static str,
        default_price_k: u32,
        default_carry: PropKind,
    },
    Sit {
        seat_yaw: f32,
        is_stool: bool,
    },
}

#[derive(Debug, Clone, Copy)]
pub struct StreetShopSpot {
    pub chain: &'static str,
    pub shop_name: &'static str,
    pub pos: Vec2,
    pub kind: ShopSpotKind,
}

fn chain_catalog(
    chain: &str,
) -> (&'static str, (&'static str, u32, PropKind), (&'static str, u32, PropKind), bool) {
    match chain {
        "cong" => ("Cộng Cà Phê", ("cotDuaCaPhe", 55, PropKind::Coffee), ("suaChuaCaPhe", 45, PropKind::Coffee), true),
        "highlands" => ("Highlands Coffee", ("phinSuaDa", 29, PropKind::Coffee), ("freezeTraXanh", 55, PropKind::Boba), true),
        "phuclong" => ("Phúc Long", ("traSuaPhucLong", 45, PropKind::Boba), ("traDao", 45, PropKind::Cup), true),
        "mixue" => ("Mixue", ("cone", 10, PropKind::IceCream), ("bubbleTea", 25, PropKind::Boba), true),
        "tgdd" => ("Thế Giới Di Động", ("phoneCase", 90, PropKind::CarrierBag), ("charger", 150, PropKind::CarrierBag), false),
        "dmx" => ("Điện Máy Xanh", ("kettle", 290, PropKind::CarrierBag), ("batteries", 35, PropKind::CarrierBag), false),
        "katinat" => ("Katinat", ("oolongNuong", 55, PropKind::Boba), ("boGiaDuaNon", 60, PropKind::Boba), true),
        "longchau" => ("Long Châu", ("panadol", 22, PropKind::CarrierBag), ("hapacol", 18, PropKind::CarrierBag), false),
        "bhx" => ("Bách Hóa Xanh", ("eggs", 30, PropKind::GroceryBag), ("meat", 65, PropKind::GroceryBag), false),
        "winmart" => ("WinMart+", ("milk", 32, PropKind::GroceryBag), ("veg", 12, PropKind::GroceryBag), false),
        "gs25" => ("GS25", ("bananaMilk", 28, PropKind::Cup), ("tok", 35, PropKind::Tokbokki), true),
        "seven" => ("7-Eleven", ("onigiri", 17, PropKind::Onigiri), ("bun", 19, PropKind::BanhBao), true),
        _ => ("Circle K", ("peachTea", 20, PropKind::Cup), ("noodle", 12, PropKind::Noodle), true),
    }
}

/// Builds exact walkable room cutouts and interactive Take/Pay/Sit spots for all walk-in shops.
pub fn build_shop_nav_zones_and_spots(
    shop_fronts: &[ShopFront],
    city: &str,
) -> (Vec<WalkInRoomBounds>, Vec<StreetShopSpot>) {
    let (set, walk_in_count) = city_shop_set(city);
    let mut rooms = Vec::new();
    let mut spots = Vec::new();

    for (idx, front) in shop_fronts.iter().enumerate() {
        if idx >= walk_in_count || idx >= set.len() {
            break;
        }
        let chain = set[idx];
        let f = ShopFrame::from_front(front);
        let depth = shop_depth(front.deep);
        let hw = SHOP_W * 0.5 - 0.35;

        // Compute world AABB of the interior walkable room and open doorway corridor
        let p_a = f.pt(-hw, 0.0, -depth + 0.55);
        let p_b = f.pt(hw, 0.0, -0.15);
        let room_min = Vec2::new(p_a.x.min(p_b.x), p_a.z.min(p_b.z));
        let room_max = Vec2::new(p_a.x.max(p_b.x), p_a.z.max(p_b.z));

        let d_a = f.pt(-DOOR_HALF, 0.0, -0.65);
        let d_b = f.pt(DOOR_HALF, 0.0, 0.85);
        let door_min = Vec2::new(d_a.x.min(d_b.x), d_a.z.min(d_b.z));
        let door_max = Vec2::new(d_a.x.max(d_b.x), d_a.z.max(d_b.z));

        rooms.push(WalkInRoomBounds {
            chain,
            room_min,
            room_max,
            door_min,
            door_max,
        });

        let (shop_name, item_a, item_b, has_seats) = chain_catalog(chain);
        // Take item spot (fridge / shelf / display)
        let take_w = f.pt(0.65, 0.0, -depth * 0.62);
        spots.push(StreetShopSpot {
            chain,
            shop_name,
            pos: Vec2::new(take_w.x, take_w.z),
            kind: ShopSpotKind::Take {
                item_id: item_a.0,
                price_k: item_a.1,
                carry: item_a.2,
            },
        });
        // Pay / order counter spot
        let pay_w = f.pt(-1.15, 0.0, -depth * 0.55);
        spots.push(StreetShopSpot {
            chain,
            shop_name,
            pos: Vec2::new(pay_w.x, pay_w.z),
            kind: ShopSpotKind::Pay {
                default_item_id: item_b.0,
                default_price_k: item_b.1,
                default_carry: item_b.2,
            },
        });
        // Window counter / low stool seat spot
        if has_seats {
            let sit_w = f.pt(1.65, 0.0, -1.25);
            spots.push(StreetShopSpot {
                chain,
                shop_name,
                pos: Vec2::new(sit_w.x, sit_w.z),
                kind: ShopSpotKind::Sit {
                    seat_yaw: f.yaw,
                    is_stool: chain == "cong",
                },
            });
        }
    }

    (rooms, spots)
}

const FL: f32 = 0.12;
const SIGN_H: f32 = 0.86;
const DOOR_HALF: f32 = 0.82;

const SET_HCM: [&str; 12] = ["gs25", "mixue", "bhx", "dmx", "highlands", "tgdd", "cong", "longchau", "phuclong", "katinat", "circlek", "seven"];
const SET_HN: [&str; 12] = ["circlek", "mixue", "highlands", "cong", "tgdd", "longchau", "phuclong", "katinat", "gs25", "pho", "dmx", "buncha"];
const SET_HP: [&str; 12] = ["winmart", "mixue", "highlands", "tgdd", "longchau", "phuclong", "banhdacua", "circlek", "dmx", "cong", "haisan", "longchau"];
const SET_DN: [&str; 12] = ["winmart", "mixue", "highlands", "tgdd", "longchau", "phuclong", "miquang", "haisan", "dmx", "cong", "banhxeo", "longchau"];
const SET_HUE: [&str; 12] = ["winmart", "mixue", "highlands", "tgdd", "longchau", "bunbo", "comhen", "dmx", "winmart", "banhxeo", "longchau", "cong"];
const SET_NT: [&str; 12] = ["winmart", "mixue", "highlands", "tgdd", "longchau", "phuclong", "bunchaca", "haisan", "dmx", "cong", "banhxeo", "longchau"];
const SET_DL: [&str; 12] = ["winmart", "mixue", "highlands", "tgdd", "longchau", "phuclong", "katinat", "banhcan", "suadau", "dmx", "cong", "banhxeo"];
const SET_CT: [&str; 12] = ["bhx", "mixue", "highlands", "tgdd", "longchau", "phuclong", "katinat", "winmart", "hutieu", "laumam", "dmx", "bhx"];

#[derive(Component)]
pub struct WalkInShopRoom {
    pub chain: &'static str,
    pub doorway: Vec3,
    pub dir_out: Vec3,
}

#[derive(Component)]
pub struct MixueSnowKing {
    pub base_y: f32,
    pub base_yaw: f32,
}

#[derive(Component)]
pub struct TgddTubeDancer {
    pub base_pos: Vec3,
    pub seg_idx: usize,
}

#[derive(Component)]
pub struct ShopNightShutter {
    pub closed_at_night: bool,
    pub open_y: f32,
    pub closed_y: f32,
}

/// Marker on shop fascia quads so Day↔Night toggling updates shop lightbox emissive live.
#[derive(Component)]
pub struct ShopLightboxNight;

/// Returns `(chain_roster, walk_in_count)` matching `shops.js:52,70-79` (`hcm` = 10 walk-ins).
pub fn city_shop_set(city: &str) -> (&'static [&'static str; 12], usize) {
    match city.to_ascii_lowercase().as_str() {
        "hn" => (&SET_HN, 8),
        "hp" => (&SET_HP, 8),
        "dn" => (&SET_DN, 8),
        "hue" => (&SET_HUE, 8),
        "nt" => (&SET_NT, 8),
        "dl" => (&SET_DL, 8),
        "ct" => (&SET_CT, 8),
        _ => (&SET_HCM, 10),
    }
}

struct ShopFrame {
    origin: Vec3,
    rot: Quat,
    yaw: f32,
    dir_out: Vec3,
}

impl ShopFrame {
    fn from_front(front: &ShopFront) -> Self {
        let (yaw, dir_out) = match front.dir {
            ShopDir::PosX => (FRAC_PI_2, Vec3::X),
            ShopDir::PosZ => (0.0, Vec3::Z),
        };
        Self {
            origin: Vec3::new(front.x, GY, front.z),
            rot: Quat::from_rotation_y(yaw),
            yaw,
            dir_out,
        }
    }

    fn pt(&self, lx: f32, ly: f32, lz: f32) -> Vec3 {
        self.origin + self.rot * Vec3::new(lx, ly, lz)
    }
}

fn push_shop_box(b: &mut StreetMeshBuilder, f: &ShopFrame, lx: f32, ly: f32, lz: f32, w: f32, h: f32, d: f32, hex: &str) {
    b.push_box_rot(f.pt(lx, ly, lz), Vec3::new(w, h, d), f.rot, hex);
}

fn push_shop_cyl(b: &mut StreetMeshBuilder, f: &ShopFrame, lx: f32, ly: f32, lz: f32, r0: f32, r1: f32, h: f32, hex: &str) {
    b.push_cyl_rot(f.pt(lx, ly, lz), r0, r1, h, 10, f.rot, hex);
}

fn build_chain_interior(
    solid: &mut StreetMeshBuilder,
    f: &ShopFrame,
    chain: &str,
    depth: f32,
) {
    let (floor_col, wall_col, counter_col, accent_col) = match chain {
        "cong" => ("#8A6848", "#56663A", "#5A3C24", "#C8202A"),
        "highlands" => ("#C8B39A", "#EFE6D8", "#6B3E24", "#9B1C21"),
        "phuclong" => ("#D4C4AE", "#F2EFE8", "#6E4A2E", "#006F3C"),
        "mixue" => ("#F5F3EE", "#FFFFFF", "#E50914", "#F6C343"),
        "tgdd" => ("#E9ECF0", "#F7F8FA", "#181A1E", "#FFD400"),
        "dmx" => ("#E6ECF2", "#F4F8FC", "#0072CE", "#FFF200"),
        "katinat" => ("#CFC2AE", "#F1ECE4", "#1A2B4C", "#BF9369"),
        "longchau" => ("#F2F5FA", "#FFFFFF", "#1554D1", "#51B848"),
        "bhx" => ("#DFE5E0", "#F2F6F2", "#008C44", "#FFE600"),
        "winmart" => ("#EAE6E0", "#F7F5F2", "#E21B2A", "#FFFFFF"),
        "gs25" => ("#E8EDF2", "#F7F9FB", "#0068B7", "#2BB8E6"),
        _ => ("#ECEBE8", "#F7F6F4", "#E21F26", "#F39C1F"),
    };

    let hw = SHOP_W * 0.5;
    // Floor, ceiling, back wall, side walls
    push_shop_box(solid, f, 0.0, FL * 0.5, -depth * 0.5, SHOP_W, FL, depth, floor_col);
    push_shop_box(solid, f, 0.0, SHOP_CEIL - 0.04, -depth * 0.5, SHOP_W, 0.08, depth, "#F3F2EE");
    push_shop_box(solid, f, 0.0, SHOP_CEIL * 0.5, -depth + 0.06, SHOP_W, SHOP_CEIL, 0.12, wall_col);
    push_shop_box(solid, f, -hw + 0.06, SHOP_CEIL * 0.5, -depth * 0.5, 0.12, SHOP_CEIL, depth, wall_col);
    push_shop_box(solid, f, hw - 0.06, SHOP_CEIL * 0.5, -depth * 0.5, 0.12, SHOP_CEIL, depth, wall_col);

    // Front mullions framing the walk-in open doorway (`[-DOOR_HALF..DOOR_HALF]`)
    let frame_col = if chain == "cong" { "#4E5E2B" } else { "#4A515B" };
    for mx in [-hw + 0.06, -DOOR_HALF, DOOR_HALF, hw - 0.06] {
        push_shop_box(solid, f, mx, FL + 1.15, -0.04, 0.08, 2.30, 0.08, frame_col);
    }
    push_shop_box(solid, f, 0.0, FL + 2.32, -0.04, SHOP_W, 0.08, 0.08, frame_col);

    // Chain-specific 3D interior fittings matching Oracle shop-*.js!
    match chain {
        "cong" => {
            // Terracotta tile entrance awning over door
            push_shop_box(solid, f, 0.0, FL + 2.30, 0.34, 2.4, 0.10, 0.72, "#B5562E");
            // Wooden order counter + enamel mugs + thermos flasks
            push_shop_box(solid, f, -1.35, FL + 0.48, -depth + 1.15, 2.5, 0.96, 0.65, counter_col);
            push_shop_cyl(solid, f, -1.9, FL + 1.12, -depth + 1.1, 0.07, 0.07, 0.28, "#C8202A");
            push_shop_cyl(solid, f, -1.65, FL + 1.10, -depth + 1.1, 0.06, 0.06, 0.24, "#3A7A42");
            // Long bench with iconic red "chăn con công" peacock cushions
            push_shop_box(solid, f, 1.55, FL + 0.24, -depth + 0.55, 2.6, 0.48, 0.56, "#6B462B");
            for k in 0..4 {
                push_shop_box(solid, f, 0.60 + k as f32 * 0.62, FL + 0.56, -depth + 0.42, 0.44, 0.34, 0.14, "#C8202A");
                push_shop_box(solid, f, 0.60 + k as f32 * 0.62, FL + 0.58, -depth + 0.495, 0.16, 0.16, 0.02, "#F2C230");
            }
            // Vintage wooden CRT TV in corner + low wooden coffee tables & stools
            push_shop_box(solid, f, 2.75, FL + 0.72, -depth + 0.75, 0.58, 0.48, 0.42, "#5E3C24");
            push_shop_box(solid, f, 2.75, FL + 0.74, -depth + 0.97, 0.40, 0.32, 0.03, "#2A343B");
            for tx in [1.1_f32, 2.2] {
                push_shop_box(solid, f, tx, FL + 0.38, -1.55, 0.72, 0.05, 0.52, "#7A5232");
                push_shop_box(solid, f, tx - 0.30, FL + 0.19, -1.55, 0.06, 0.38, 0.46, "#4E3422");
                push_shop_box(solid, f, tx + 0.30, FL + 0.19, -1.55, 0.06, 0.38, 0.46, "#4E3422");
            }
            // Sidewalk folding table & vintage bicycle basket out front
            push_shop_box(solid, f, -2.35, FL + 0.42, 0.65, 0.65, 0.04, 0.50, "#4E5E2B");
            push_shop_box(solid, f, 2.45, FL + 0.72, 0.62, 0.34, 0.24, 0.28, "#C9A86A");
        }
        "highlands" | "phuclong" | "katinat" => {
            // Order counter with espresso/tea brewers + pastry/cup display
            push_shop_box(solid, f, -1.1, FL + 0.48, -depth + 1.15, 3.2, 0.96, 0.68, counter_col);
            push_shop_box(solid, f, -1.1, FL + 0.50, -depth + 1.50, 3.1, 0.26, 0.04, accent_col);
            // Espresso / tea machine + glass pastry display
            push_shop_box(solid, f, -2.1, FL + 1.18, -depth + 1.10, 0.68, 0.44, 0.42, "#C9CED6");
            push_shop_box(solid, f, -0.3, FL + 1.16, -depth + 1.12, 0.92, 0.40, 0.48, "#DCEBF2");
            // Back shelving / tea tin display wall (Phúc Long) or globe lamps (Katinat)
            for row in 0..3 {
                push_shop_box(solid, f, -1.1, FL + 0.85 + row as f32 * 0.46, -depth + 0.28, 3.0, 0.05, 0.30, "#6E4A2E");
                for k in 0..5 {
                    push_shop_cyl(solid, f, -2.2 + k as f32 * 0.52, FL + 1.00 + row as f32 * 0.46, -depth + 0.28, 0.07, 0.07, 0.22, accent_col);
                }
            }
            // Cafe seating tables & chairs
            for tx in [1.45_f32, 2.45] {
                push_shop_cyl(solid, f, tx, FL + 0.72, -1.65, 0.36, 0.36, 0.05, "#8A5E3B");
                push_shop_cyl(solid, f, tx, FL + 0.36, -1.65, 0.04, 0.16, 0.72, "#2B2F36");
                for dz in [-0.52_f32, 0.52] {
                    push_shop_box(solid, f, tx, FL + 0.42, -1.65 + dz, 0.36, 0.05, 0.36, counter_col);
                }
            }
            // Sidewalk outdoor cafe table
            push_shop_cyl(solid, f, 2.1, FL + 0.70, 0.75, 0.35, 0.35, 0.05, "#7A4B2E");
            push_shop_cyl(solid, f, 2.1, FL + 0.35, 0.75, 0.04, 0.14, 0.70, "#2B2F36");
        }
        "mixue" => {
            // Red order counter facing street + soft-serve machine + tea shakers
            push_shop_box(solid, f, -0.95, FL + 0.48, -depth + 1.25, 3.4, 0.96, 0.72, "#E50914");
            push_shop_box(solid, f, -0.95, FL + 0.98, -depth + 1.25, 3.46, 0.05, 0.76, "#FFFFFF");
            push_shop_box(solid, f, -2.0, FL + 1.32, -depth + 1.22, 0.56, 0.64, 0.52, "#DCE2E8");
            push_shop_box(solid, f, -0.8, FL + 1.22, -depth + 1.18, 0.42, 0.44, 0.38, "#E50914");
            // White tables & low brown stools
            for tx in [1.6_f32, 2.45] {
                push_shop_box(solid, f, tx, FL + 0.70, -1.7, 0.64, 0.05, 0.64, "#FFFFFF");
                push_shop_cyl(solid, f, tx, FL + 0.35, -1.7, 0.04, 0.14, 0.70, "#B9C0C8");
                push_shop_cyl(solid, f, tx, FL + 0.22, -1.15, 0.16, 0.16, 0.44, "#8A5E3B");
                push_shop_cyl(solid, f, tx, FL + 0.22, -2.25, 0.16, 0.16, 0.44, "#8A5E3B");
            }
        }
        "tgdd" => {
            // Long white smartphone demo tables with phone stands + yellow price tags + back till
            for tx in [-1.55_f32, 1.55] {
                push_shop_box(solid, f, tx, FL + 0.45, -1.85, 1.75, 0.88, 0.74, "#F7F8FA");
                push_shop_box(solid, f, tx, FL + 0.90, -1.85, 1.78, 0.04, 0.78, "#FFD400");
                for k in 0..4 {
                    let px = tx - 0.60 + k as f32 * 0.40;
                    push_shop_box(solid, f, px, FL + 0.99, -1.68, 0.10, 0.14, 0.04, "#1C232B");
                    push_shop_box(solid, f, px, FL + 0.96, -2.02, 0.10, 0.12, 0.04, "#FFD400");
                }
            }
            // Black & yellow cashier till at back
            push_shop_box(solid, f, 0.0, FL + 0.48, -depth + 0.85, 2.6, 0.96, 0.62, "#141414");
            push_shop_box(solid, f, 0.0, FL + 0.14, -depth + 1.18, 2.6, 0.16, 0.04, "#FFD400");
        }
        "dmx" => {
            // 6-screen glowing TV wall at back (2 rows × 3 columns)
            let tv_cols = ["#3C8DDE", "#2E8B3E", "#FF2D55", "#00C7BE", "#E8603C", "#00AEEF"];
            for idx in 0..6 {
                let col = idx % 3;
                let row = idx / 3;
                let tx = -1.6 + col as f32 * 1.6;
                let ty = FL + 1.05 + row as f32 * 0.78;
                push_shop_box(solid, f, tx, ty, -depth + 0.16, 1.24, 0.68, 0.06, "#16181C");
                push_shop_box(solid, f, tx, ty, -depth + 0.20, 1.16, 0.60, 0.02, tv_cols[idx]);
            }
            // Front-loading washing machines & refrigerators on left
            for k in 0..2 {
                let wz = -1.35 - k as f32 * 0.95;
                push_shop_box(solid, f, -2.45, FL + 0.45, wz, 0.68, 0.88, 0.68, "#F2F4F7");
                push_shop_cyl(solid, f, -2.10, FL + 0.48, wz, 0.22, 0.22, 0.04, "#2B2F36");
            }
            // 2 dark leather massage chairs on right
            for k in 0..2 {
                let mz = -1.40 - k as f32 * 1.20;
                push_shop_box(solid, f, 2.25, FL + 0.34, mz, 0.85, 0.56, 0.78, "#2C2420");
                push_shop_box(solid, f, 2.58, FL + 0.78, mz, 0.32, 0.76, 0.76, "#3A2F2A");
                push_shop_box(solid, f, 1.88, FL + 0.22, mz, 0.34, 0.36, 0.66, "#241E1B");
            }
        }
        "longchau" => {
            // Long blue & white prescription counter + illuminated medicine shelves
            push_shop_box(solid, f, 0.0, FL + 0.48, -depth + 1.35, 4.8, 0.96, 0.64, "#1554D1");
            push_shop_box(solid, f, 0.0, FL + 0.98, -depth + 1.35, 4.86, 0.05, 0.68, "#FFFFFF");
            let box_cols = ["#FFFFFF", "#E8F0FB", "#D9534F", "#2E86DE", "#F39C1F", "#2E9E4A"];
            for row in 0..4 {
                let sy = FL + 0.65 + row as f32 * 0.42;
                push_shop_box(solid, f, 0.0, sy, -depth + 0.26, 5.4, 0.04, 0.28, "#E8ECF2");
                for k in 0..10 {
                    push_shop_box(
                        solid,
                        f,
                        -2.3 + k as f32 * 0.51,
                        sy + 0.13,
                        -depth + 0.28,
                        0.32,
                        0.22,
                        0.18,
                        box_cols[(row * 5 + k) % box_cols.len()],
                    );
                }
            }
            // Customer waiting bench
            push_shop_box(solid, f, 2.1, FL + 0.42, -1.15, 1.4, 0.08, 0.42, "#1554D1");
        }
        _ => {
            // Convenience stores (`gs25`, `circlek`, `seven`, `ministop`) & mini-marts (`bhx`, `winmart`)
            // Drinks chiller wall along back + 2 center aisles stocked with colorful goods + checkout till
            push_shop_box(solid, f, 0.5, FL + 1.05, -depth + 0.34, 4.4, 2.05, 0.52, "#DCE8F2");
            let packs = ["#E41E2B", "#1C4FA0", "#F7C600", "#E8742A", "#2E9E4A", "#7E3FA6", "#0B7FC2"];
            for aisle_x in [-0.65_f32, 1.45] {
                push_shop_box(solid, f, aisle_x, FL + 0.72, -1.95, 0.54, 1.42, 2.15, "#E9ECEF");
                for s in 0..3 {
                    for k in 0..4 {
                        push_shop_box(
                            solid,
                            f,
                            aisle_x,
                            FL + 0.35 + s as f32 * 0.40,
                            -1.25 - k as f32 * 0.46,
                            0.58,
                            0.24,
                            0.32,
                            packs[(s * 4 + k + (aisle_x > 0.0) as usize * 3) % packs.len()],
                        );
                    }
                }
            }
            // Checkout counter on left with hot-food / produce corner
            push_shop_box(solid, f, -2.35, FL + 0.48, -1.85, 0.76, 0.96, 2.2, counter_col);
            push_shop_box(solid, f, -2.35, FL + 1.06, -1.25, 0.48, 0.24, 0.56, accent_col);
        }
    }
}

/// Spawns all 12 city-selectable shops (first 8–10 are full walk-in 3D ground-floor rooms with
/// UV-cropped fascias/menus/posters/doors, animated Snow King & TGDĐ tube dancer, and night shutters;
/// remaining fronts are UV-cropped painted storefronts). Returns `(chain, doorway_world_pos)` list.
#[allow(clippy::too_many_arguments)]
pub fn spawn_walkin_shops(
    commands: &mut Commands,
    meshes: &mut Assets<Mesh>,
    materials: &mut Assets<StandardMaterial>,
    images: &mut Assets<Image>,
    ba: &mut BackdropAssets,
    brands: &mut BrandRegistry,
    fonts: &mut FontEngine,
    fronts: &[ShopFront],
    city: &str,
    night: bool,
) -> Vec<(String, Vec3)> {
    let (roster, walk_ins) = city_shop_set(city);
    let mut solid = StreetMeshBuilder::default();
    let mut glass = StreetMeshBuilder::default();
    let mut shop_mats: HashMap<&'static str, Handle<StandardMaterial>> = HashMap::new();
    let mut generic_store_mat: Option<Handle<StandardMaterial>> = None;
    let mut doors = Vec::new();

    for (idx, front) in fronts.iter().take(roster.len()).enumerate() {
        let chain = roster[idx];
        let f = ShopFrame::from_front(front);
        let depth = shop_depth(front.deep);
        let is_walkin = idx < walk_ins;

        let shop_mat = shop_mats
            .entry(chain)
            .or_insert_with(|| {
                let surf = draw_shop_cell(chain, brands, fonts);
                let img = ba.upload_image(images, surf);
                let em = if night { 0.42 } else { 0.08 };
                ba.add_material(
                    materials,
                    StandardMaterial {
                        base_color: Color::WHITE,
                        base_color_texture: Some(img.clone()),
                        emissive: LinearRgba::new(em, em, em, 1.0),
                        emissive_texture: Some(img),
                        perceptual_roughness: 0.42,
                        ..default()
                    },
                )
            })
            .clone();

        // 1. Fascia lightbox sign (UV-cropped [0, 0, 1024, 140])
        push_shop_box(&mut solid, &f, 0.0, SHOP_CEIL + SIGN_H * 0.45, 0.06, SHOP_W + 0.08, SIGN_H + 0.06, 0.24, "#2B2F36");
        let fascia_e = spawn_backdrop_uv_quad(
            commands,
            meshes,
            ba,
            shop_mat.clone(),
            1024.0,
            512.0,
            [0.0, 0.0, 1024.0, 140.0],
            f.pt(0.0, SHOP_CEIL + SIGN_H * 0.45, 0.19),
            Vec2::new(SHOP_W, SIGN_H),
            f.rot,
        );
        commands.entity(fascia_e).insert(ShopLightboxNight);

        if is_walkin {
            build_chain_interior(&mut solid, &f, chain, depth);
            // Storefront glass panes left and right of doorway (doorway itself stays OPEN to walk in!)
            let pane_w = SHOP_W * 0.5 - DOOR_HALF - 0.08;
            let pane_x = DOOR_HALF + pane_w * 0.5 + 0.04;
            push_shop_box(&mut glass, &f, -pane_x, FL + 1.15, -0.02, pane_w, 2.26, 0.02, "#D8EEF7");
            push_shop_box(&mut glass, &f, pane_x, FL + 1.15, -0.02, pane_w, 2.26, 0.02, "#D8EEF7");

            // UV-cropped Poster A [0,140,200,290] & Poster B [200,140,200,290] on left/right windows
            spawn_backdrop_uv_quad(
                commands,
                meshes,
                ba,
                shop_mat.clone(),
                1024.0,
                512.0,
                [0.0, 140.0, 200.0, 290.0],
                f.pt(-pane_x, FL + 1.25, 0.02),
                Vec2::new(0.86, 1.22),
                f.rot,
            );
            spawn_backdrop_uv_quad(
                commands,
                meshes,
                ba,
                shop_mat.clone(),
                1024.0,
                512.0,
                [200.0, 140.0, 200.0, 290.0],
                f.pt(pane_x, FL + 1.25, 0.02),
                Vec2::new(0.86, 1.22),
                f.rot,
            );
            // UV-cropped interior Menu board [400,140,420,200] & Back wall brand band [0,432,1024,80]
            spawn_backdrop_uv_quad(
                commands,
                meshes,
                ba,
                shop_mat.clone(),
                1024.0,
                512.0,
                [400.0, 140.0, 420.0, 200.0],
                f.pt(-0.9, FL + 1.82, -depth + 0.16),
                Vec2::new(2.2, 0.96),
                f.rot,
            );
            spawn_backdrop_uv_quad(
                commands,
                meshes,
                ba,
                shop_mat.clone(),
                1024.0,
                512.0,
                [0.0, 432.0, 1024.0, 80.0],
                f.pt(0.0, SHOP_CEIL - 0.34, -depth + 0.15),
                Vec2::new(SHOP_W - 0.3, 0.42),
                f.rot,
            );
            // UV-cropped Door hours sticker [820,140,204,204]
            spawn_backdrop_uv_quad(
                commands,
                meshes,
                ba,
                shop_mat.clone(),
                1024.0,
                512.0,
                [820.0, 140.0, 204.0, 204.0],
                f.pt(DOOR_HALF + 0.24, FL + 1.48, 0.025),
                Vec2::new(0.36, 0.36),
                f.rot,
            );

            let doorway = f.pt(0.0, FL, 0.55);
            commands.spawn((
                WorldEntity,
                BackdropEntity,
                WalkInShopRoom {
                    chain,
                    doorway,
                    dir_out: f.dir_out,
                },
                Transform::from_translation(doorway),
            ));
            doors.push((chain.to_owned(), doorway));

            // Mixue: animated dancing Snow King mascot out front (`shop-mixue.js`)
            if chain == "mixue" {
                let mut sk = StreetMeshBuilder::default();
                sk.push_sphere(Vec3::new(0.0, 0.46, 0.0), Vec3::new(0.34, 0.36, 0.32), "#FBFBF8");
                sk.push_sphere(Vec3::new(0.0, 0.96, 0.0), Vec3::new(0.27, 0.26, 0.27), "#FBFBF8");
                // Red cape + collar
                sk.push_cyl(Vec3::new(0.0, 0.48, -0.06), 0.30, 0.38, 0.56, 10, "#E50914");
                sk.push_cyl(Vec3::new(0.0, 0.72, 0.0), 0.23, 0.25, 0.08, 10, "#E50914");
                // Golden crown
                sk.push_cyl(Vec3::new(0.0, 1.25, 0.0), 0.14, 0.16, 0.16, 8, "#F6C343");
                // Eyes + pink cheeks + ice-cream cone sceptre
                for sx in [-0.09_f32, 0.09] {
                    sk.push_sphere(Vec3::new(sx, 0.98, 0.24), Vec3::splat(0.036), "#2B6CD8");
                    sk.push_sphere(Vec3::new(sx * 1.6, 0.90, 0.22), Vec3::splat(0.042), "#F7A1B5");
                }
                sk.push_bar(Vec3::new(0.32, 0.25, 0.12), Vec3::new(0.38, 1.18, 0.18), 0.025, 6, "#F6C343");
                sk.push_sphere(Vec3::new(0.38, 1.26, 0.18), Vec3::splat(0.11), "#FBFBF8");

                let sk_mesh = ba.add_mesh(meshes, sk.build());
                let sk_mat = ba.mat(materials, "#FFFFFF", 0.55, 0.05);
                let sk_pos = f.pt(-1.75, FL, 0.68);
                commands.spawn((
                    WorldEntity,
                    BackdropEntity,
                    MixueSnowKing {
                        base_y: sk_pos.y,
                        base_yaw: f.yaw,
                    },
                    Mesh3d(sk_mesh),
                    MeshMaterial3d(sk_mat),
                    Transform::from_translation(sk_pos).with_rotation(f.rot),
                ));
            }

            // Thế Giới Di Động: animated 5-segment inflatable tube dancer ("người hơi") (`shop-tgdd.js`)
            if chain == "tgdd" {
                let seg_mesh = ba.add_mesh(meshes, Mesh::from(Cylinder::new(0.19, 0.54)));
                let yellow_m = ba.mat(materials, "#FFD400", 0.65, 0.0);
                let black_m = ba.mat(materials, "#181A1E", 0.65, 0.0);
                let base_pos = f.pt(2.35, FL + 0.28, 0.72);
                for seg_idx in 0..5 {
                    let mat = if seg_idx % 2 == 0 { yellow_m.clone() } else { black_m.clone() };
                    commands.spawn((
                        WorldEntity,
                        BackdropEntity,
                        TgddTubeDancer { base_pos, seg_idx },
                        Mesh3d(seg_mesh.clone()),
                        MeshMaterial3d(mat),
                        Transform::from_translation(base_pos + Vec3::new(0.0, seg_idx as f32 * 0.48, 0.0)),
                    ));
                }
            }

            // WinMart+ / Bách Hóa Xanh: roll-down security night shutter (`shops.js:9`)
            if matches!(chain, "bhx" | "winmart") {
                let shutter_mesh = ba.add_mesh(meshes, Mesh::from(Cuboid::new(SHOP_W - 0.16, 2.28, 0.05)));
                let shutter_mat = ba.mat(materials, "#A8B0B8", 0.50, 0.45);
                let open_y = FL + 2.38;
                let closed_y = FL + 1.14;
                let cur_y = if night { closed_y } else { open_y };
                commands.spawn((
                    WorldEntity,
                    BackdropEntity,
                    ShopNightShutter {
                        closed_at_night: true,
                        open_y,
                        closed_y,
                    },
                    Mesh3d(shutter_mesh),
                    MeshMaterial3d(shutter_mat),
                    Transform::from_translation(f.pt(0.0, cur_y, 0.015)).with_rotation(f.rot),
                    if night { Visibility::Inherited } else { Visibility::Hidden },
                ));
            }
        } else {
            // Non-walk-in distant shops (#11–12): clean painted storefront glass slice (not packed UI poster strip)
            let store_mat = generic_store_mat
                .get_or_insert_with(|| {
                    let img = ba.upload_image(images, draw_generic_shop_strip());
                    let em = if night { 0.42 } else { 0.08 };
                    ba.add_material(
                        materials,
                        StandardMaterial {
                            base_color: Color::WHITE,
                            base_color_texture: Some(img.clone()),
                            emissive: LinearRgba::new(em, em, em, 1.0),
                            emissive_texture: Some(img),
                            perceptual_roughness: 0.42,
                            ..default()
                        },
                    )
                })
                .clone();
            let slot_x = ((idx % 8) * 64) as f32;
            let store_e = spawn_backdrop_uv_quad(
                commands,
                meshes,
                ba,
                store_mat,
                512.0,
                64.0,
                [slot_x, 22.0, 64.0, 42.0],
                f.pt(0.0, FL + 1.18, 0.04),
                Vec2::new(SHOP_W - 0.2, 2.32),
                f.rot,
            );
            commands.entity(store_e).insert(ShopLightboxNight);
        }
    }

    let solid_mat = ba.mat(materials, "#FFFFFF", 0.72, 0.05);
    solid.spawn(commands, meshes, ba, solid_mat);
    let glass_mat = ba.mat_blend(materials, "#D6ECF7", 0.13, 0.08, false);
    glass.spawn(commands, meshes, ba, glass_mat);

    doors
}

/// Zero-allocation Update system animating Mixue's Snow King, TGDĐ's inflatable tube dancer,
/// and WinMart+/BHX roll-down shutters on night toggle.
pub fn animate_walkin_shops_system(
    time: Res<Time>,
    lighting: Option<Res<crate::lights::LightingState>>,
    mut snow_kings: Query<(&MixueSnowKing, &mut Transform), Without<TgddTubeDancer>>,
    mut dancers: Query<(&TgddTubeDancer, &mut Transform), Without<MixueSnowKing>>,
    mut shutters: Query<(&ShopNightShutter, &mut Transform, &mut Visibility), (Without<MixueSnowKing>, Without<TgddTubeDancer>)>,
) {
    let t = time.elapsed_secs();
    for (sk, mut tf) in &mut snow_kings {
        let bounce = (t * 4.2).sin().abs() * 0.12;
        let sway = (t * 2.1).sin() * 0.22;
        tf.translation.y = sk.base_y + bounce;
        tf.rotation = Quat::from_rotation_y(sk.base_yaw + sway) * Quat::from_rotation_z((t * 4.2).cos() * 0.08);
    }
    for (d, mut tf) in &mut dancers {
        let k = d.seg_idx as f32;
        let whip = (t * 5.6 - k * 0.65).sin() * (0.06 + k * 0.055);
        let sway_z = (t * 4.1 - k * 0.5).cos() * (0.04 + k * 0.04);
        tf.translation = d.base_pos + Vec3::new(whip, k * 0.48, sway_z);
        tf.rotation = Quat::from_rotation_z(-whip * 1.1) * Quat::from_rotation_x(sway_z * 0.9);
    }
    if let Some(l) = lighting {
        let is_night = l.is_night();
        for (sh, mut tf, mut vis) in &mut shutters {
            let shut = sh.closed_at_night && is_night;
            tf.translation.y = if shut { sh.closed_y } else { sh.open_y };
            *vis = if shut { Visibility::Inherited } else { Visibility::Hidden };
        }
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
    fn shops_walkin_spawns_10_hcm_walkin_rooms_animated_mascots_shutters_and_purges_zero_leaks() {
        let assets_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../assets");
        let spec = LayoutSpec::for_layout(OfficeLayout::Pro);
        let mut fonts = FontEngine::from_assets_dir(&assets_dir);
        let mut brands = BrandRegistry::new(assets_dir);

        let mut world = World::new();
        world.insert_resource(Assets::<Mesh>::default());
        world.insert_resource(Assets::<StandardMaterial>::default());
        world.insert_resource(Assets::<Image>::default());
        world.insert_resource(BackdropAssets::default());

        let (set, walk_ins) = city_shop_set("hcm");
        assert_eq!(set.len(), 12);
        assert_eq!(walk_ins, 10);

        let doors = world.resource_scope(|world, mut meshes: Mut<Assets<Mesh>>| {
            world.resource_scope(|world, mut mats: Mut<Assets<StandardMaterial>>| {
                world.resource_scope(|world, mut imgs: Mut<Assets<Image>>| {
                    world.resource_scope(|world, mut ba: Mut<BackdropAssets>| {
                        let mut cmds = world.commands();
                        let grid = spawn_city_grid(&mut cmds, &mut meshes, &mut mats, &mut imgs, &mut ba, &spec, false);
                        spawn_walkin_shops(
                            &mut cmds,
                            &mut meshes,
                            &mut mats,
                            &mut imgs,
                            &mut ba,
                            &mut brands,
                            &mut fonts,
                            &grid.shop_fronts,
                            "hcm",
                            false,
                        )
                    })
                })
            })
        });
        world.flush();

        assert_eq!(doors.len(), 10, "HCM must spawn 10 walk-in shop doorways");
        assert_eq!(world.query::<&WalkInShopRoom>().iter(&world).count(), 10);
        assert!(world.query::<&MixueSnowKing>().iter(&world).count() >= 1);
        assert!(world.query::<&TgddTubeDancer>().iter(&world).count() >= 3);
        assert!(world.query::<&ShopNightShutter>().iter(&world).count() >= 1);

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
