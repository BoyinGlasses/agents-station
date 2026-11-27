//! Workflow-driven office life systems:
//! - Commit `Postbox` + postman arrival on `git push` (`post.js`)
//! - `Incidents` red warning strobes on 3 failures / 5 min (`incidents.js`)
//! - `Quota` break countdown board (`"HẾT QUOTA"`, `quota.js`)
//! - Server-room `Tests` lamp board (`tests.js`)
//! - Live `Trophies` cabinet reading `data/totals.json` (`trophies.js`)
//! - Whiteboard `Photostrips` reading up to 6 thumbnails from `data/shots/` (`photostrips.js`)

use crate::layout::LayoutSpec;
use crate::life::OfficeLifeState;
use crate::palette::{
    WorldEntity, WorldPalette, spawn_box_rot, spawn_cyl, spawn_cyl_rot, spawn_quad, spawn_sphere,
};
use bevy::prelude::*;
use station_canvas2d::boards_and_social::{PhotostripInput, StripPerson, draw_photostrip};
use station_canvas2d::{Canvas2d, FontEngine, RgbaSurface, TextAlign, hex};
use std::collections::HashMap;
use std::f32::consts::{FRAC_PI_2, PI};
use std::path::Path;

/// Mirrors `station_core::store::trophies::MILESTONES` without forcing `station-3d` to mmap `station-core`.
const TROPHY_MILESTONES: [(&str, &str, &str, u8); 21] = [
    ("p100", "prompts", "cup", 1),
    ("p1k", "prompts", "cup", 2),
    ("p10k", "prompts", "cup", 3),
    ("e1k", "edit", "cup", 1),
    ("e10k", "edit", "cup", 2),
    ("e100k", "edit", "cup", 3),
    ("c1k", "cmd", "star", 1),
    ("c10k", "cmd", "star", 2),
    ("c100k", "cmd", "star", 3),
    ("r1k", "read", "book", 1),
    ("r10k", "read", "book", 2),
    ("r100k", "read", "book", 3),
    ("d1", "deploy", "rocket", 1),
    ("d10", "deploy", "rocket", 2),
    ("d100", "deploy", "rocket", 3),
    ("s10", "sessions", "medal", 1),
    ("s100", "sessions", "medal", 2),
    ("s1k", "sessions", "medal", 3),
    ("days7", "days", "medal", 1),
    ("days30", "days", "medal", 2),
    ("days100", "days", "medal", 3),
];

pub const MAX_PHOTOSTRIPS: usize = 6;
pub const MAX_TEST_ROWS: usize = 8;
pub const MAX_TROPHY_SLOTS: usize = 9;
pub const PHOTOSTRIP_THUMB_MAX_W: u32 = 128;
pub const PHOTOSTRIP_THUMB_MAX_H: u32 = 192;
const INCIDENT_WINDOW_SECS: f32 = 300.0;
const INCIDENT_DURATION_SECS: f32 = 25.0;
const TEST_LAMP_FADE_SECS: f32 = 180.0;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum PostmanStage {
    #[default]
    Offstage,
    Arriving,
    Collecting,
    Departing,
}

#[derive(Resource, Debug, Clone, Default)]
pub struct PostboxState {
    pub in_box: usize,
    pub today_commits: usize,
    pub push_due_secs: f32,
    pub postman_stage: PostmanStage,
    pub stage_elapsed: f32,
    pub door_open: f32,
}

impl PostboxState {
    pub fn on_commit(&mut self) {
        self.in_box = self.in_box.saturating_add(1);
        self.today_commits = self.today_commits.saturating_add(1);
    }

    pub fn on_push(&mut self) {
        if self.postman_stage == PostmanStage::Offstage && self.push_due_secs <= 0.0 {
            self.push_due_secs = 2.0;
        }
    }
}

#[derive(Resource, Debug, Clone)]
pub struct IncidentState {
    pub fail_ring: [f32; 8],
    pub fail_count: usize,
    pub incident_until: f32,
    pub sim_secs: f32,
}

impl Default for IncidentState {
    fn default() -> Self {
        Self {
            fail_ring: [0.0; 8],
            fail_count: 0,
            incident_until: 0.0,
            sim_secs: 0.0,
        }
    }
}

impl IncidentState {
    pub fn record_failure(&mut self) {
        let now = self.sim_secs;
        // Compact expired timestamps in-place (zero allocations)
        let mut keep = 0usize;
        for i in 0..self.fail_count.min(self.fail_ring.len()) {
            if now - self.fail_ring[i] <= INCIDENT_WINDOW_SECS {
                self.fail_ring[keep] = self.fail_ring[i];
                keep += 1;
            }
        }
        if keep < self.fail_ring.len() {
            self.fail_ring[keep] = now;
            keep += 1;
        } else {
            self.fail_ring.rotate_left(1);
            self.fail_ring[self.fail_ring.len() - 1] = now;
        }
        self.fail_count = keep;

        if self.fail_count >= 3 && now >= self.incident_until {
            self.fail_count = 0;
            self.incident_until = now + INCIDENT_DURATION_SECS;
        }
    }

    pub fn is_active(&self) -> bool {
        self.incident_until > self.sim_secs
    }
}

#[derive(Resource, Debug, Clone, Default)]
pub struct QuotaBoardState {
    pub exhausted: bool,
    pub remaining_secs: f32,
    pub fold: f32,
    pub blink_on: bool,
}

impl QuotaBoardState {
    pub fn set_exhausted(&mut self, exhausted: bool, remaining_secs: f32) {
        self.exhausted = exhausted;
        self.remaining_secs = remaining_secs.max(0.0);
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum TestRunStatus {
    #[default]
    Idle,
    Running,
    Pass,
    Fail,
}

#[derive(Debug, Clone, Copy, Default)]
pub struct TestLampRow {
    pub status: TestRunStatus,
    pub updated_at: f32,
}

#[derive(Resource, Debug, Clone)]
pub struct TestsBoardState {
    pub rows: [TestLampRow; MAX_TEST_ROWS],
    pub sim_secs: f32,
}

impl Default for TestsBoardState {
    fn default() -> Self {
        Self {
            rows: [TestLampRow::default(); MAX_TEST_ROWS],
            sim_secs: 0.0,
        }
    }
}

impl TestsBoardState {
    pub fn set_row(&mut self, row: usize, status: TestRunStatus) {
        if row < MAX_TEST_ROWS {
            self.rows[row] = TestLampRow {
                status,
                updated_at: self.sim_secs,
            };
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct TrophyEntry {
    pub kind: &'static str,
    pub tier: u8,
}

#[derive(Resource, Debug, Clone)]
pub struct TrophiesShelfState {
    pub entries: Vec<TrophyEntry>,
    pub has_plaque: bool,
    pub dirty: bool,
    pub last_sig: Option<(u64, u64)>,
    pub needs_sync: bool,
}

impl Default for TrophiesShelfState {
    fn default() -> Self {
        Self {
            entries: Vec::new(),
            has_plaque: false,
            dirty: false,
            last_sig: None,
            needs_sync: true,
        }
    }
}

fn file_mtime_ms_and_len(meta: &std::fs::Metadata) -> (u64, u64) {
    let mtime_ms = meta
        .modified()
        .ok()
        .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
        .map(|d| d.as_millis() as u64)
        .unwrap_or(0);
    (mtime_ms, meta.len())
}

impl TrophiesShelfState {
    /// Parses `data/totals.json` (`TrophyStore` schema) and selects the highest reached tier
    /// (`1=Bronze`, `2=Silver`, `3=Gold`) per milestone category, capped at `MAX_TROPHY_SLOTS`.
    /// Skips disk read + JSON parse when `(mtime_ms, len)` is unchanged and `needs_sync == false`.
    pub fn load_from_totals_json(&mut self, path: &Path) -> usize {
        let sig = std::fs::metadata(path).ok().map(|m| file_mtime_ms_and_len(&m));
        if !self.needs_sync && sig == self.last_sig {
            return self.entries.len();
        }
        let Some(sig) = sig else {
            self.last_sig = None;
            self.needs_sync = false;
            return self.entries.len();
        };
        let Ok(raw) = std::fs::read_to_string(path) else {
            return self.entries.len();
        };
        let Ok(val) = serde_json::from_str::<serde_json::Value>(&raw) else {
            return self.entries.len();
        };
        let Some(earned) = val.get("earned").and_then(|v| v.as_object()) else {
            return self.entries.len();
        };
        self.has_plaque = earned.contains_key("open");

        let mut best_by_cat: HashMap<&'static str, TrophyEntry> = HashMap::new();
        for &(id, cat, kind, tier) in &TROPHY_MILESTONES {
            if !earned.contains_key(id) {
                continue;
            }
            best_by_cat
                .entry(cat)
                .and_modify(|cur| {
                    if tier > cur.tier {
                        *cur = TrophyEntry { kind, tier };
                    }
                })
                .or_insert(TrophyEntry { kind, tier });
        }

        // Deterministic slot order matching milestone categories
        let cat_order = ["prompts", "edit", "cmd", "read", "deploy", "sessions", "days"];
        self.entries.clear();
        for cat in cat_order {
            if let Some(entry) = best_by_cat.get(cat) {
                if self.entries.len() < MAX_TROPHY_SLOTS {
                    self.entries.push(*entry);
                }
            }
        }
        self.last_sig = Some(sig);
        self.needs_sync = false;
        self.dirty = true;
        self.entries.len()
    }
}

#[derive(Resource, Debug, Clone)]
pub struct PhotostripsState {
    pub loaded_paths: Vec<String>,
    pub dirty: bool,
    pub last_sig: Vec<(String, u64, u64)>,
    pub slot_images: [Option<Handle<Image>>; MAX_PHOTOSTRIPS],
    pub needs_sync: bool,
}

impl Default for PhotostripsState {
    fn default() -> Self {
        Self {
            loaded_paths: Vec::new(),
            dirty: false,
            last_sig: Vec::new(),
            slot_images: std::array::from_fn(|_| None),
            needs_sync: true,
        }
    }
}

fn compute_shots_signature(shots_dir: &Path) -> Vec<(String, u64, u64)> {
    let Ok(entries) = std::fs::read_dir(shots_dir) else {
        return Vec::new();
    };
    let mut files: Vec<(String, u64, u64)> = entries
        .flatten()
        .filter_map(|e| {
            let path = e.path();
            if !path.is_file() {
                return None;
            }
            let ext = path.extension()?.to_str()?.to_ascii_lowercase();
            if matches!(ext.as_str(), "png" | "jpg" | "jpeg") {
                let (mtime_ms, len) = e
                    .metadata()
                    .ok()
                    .map(|m| file_mtime_ms_and_len(&m))
                    .unwrap_or((0, 0));
                Some((path.to_string_lossy().into_owned(), mtime_ms, len))
            } else {
                None
            }
        })
        .collect();
    files.sort_by(|a, b| a.0.cmp(&b.0));
    if files.len() > MAX_PHOTOSTRIPS {
        let drain = files.len() - MAX_PHOTOSTRIPS;
        files.drain(..drain);
    }
    files
}

impl PhotostripsState {
    /// Marks state dirty after a 3D world rebuild so handles are re-uploaded cleanly into the new palette.
    pub fn mark_world_rebuilt(&mut self) {
        self.slot_images = std::array::from_fn(|_| None);
        self.needs_sync = true;
    }

    /// Scans `data/shots/` safely (returns 0 if missing), keeping up to `MAX_PHOTOSTRIPS` (6)
    /// newest PNG/JPEG paths sorted lexicographically newest-last.
    pub fn scan_shots_dir(&mut self, shots_dir: &Path) -> usize {
        let sig = compute_shots_signature(shots_dir);
        if !self.needs_sync && sig == self.last_sig {
            return self.loaded_paths.len();
        }
        self.loaded_paths = sig.iter().map(|(p, _, _)| p.clone()).collect();
        self.last_sig = sig;
        self.dirty = true;
        self.needs_sync = false;
        self.loaded_paths.len()
    }
}

#[derive(Component, Debug, Clone, Copy)]
pub struct PostboxDoor;

#[derive(Component, Debug, Clone, Copy)]
pub struct PostboxLetterTip(pub usize);

#[derive(Component, Debug, Clone, Copy)]
pub struct PostmanActor {
    pub home_pos: Vec3,
}

#[derive(Component, Debug, Clone, Copy)]
pub struct IncidentAlarmLight;

#[derive(Component, Debug, Clone, Copy)]
pub struct QuotaBoardRoot;

#[derive(Component, Debug, Clone)]
pub struct TestLampLed {
    pub row: usize,
    pub mat: Handle<StandardMaterial>,
}

#[derive(Component, Debug, Clone, Copy)]
pub struct TrophyCabinetSlot {
    pub index: usize,
}

#[derive(Component, Debug, Clone, Copy)]
pub struct TrophyPlaque;

#[derive(Component, Debug, Clone)]
pub struct PhotostripSlot {
    pub index: usize,
    pub mat: Handle<StandardMaterial>,
}

fn draw_post_sign(fonts: &mut FontEngine) -> RgbaSurface {
    let mut c = Canvas2d::new(128, 48);
    c.fill(hex("#FFFFFF"));
    fonts.draw_text(
        &mut c,
        "POST",
        64.0,
        26.0,
        28.0,
        hex("#C8261B"),
        TextAlign::Center,
        Some(112.0),
        true,
        false,
    );
    c.into_surface()
}

fn draw_quota_board(fonts: &mut FontEngine) -> RgbaSurface {
    let mut c = Canvas2d::new(384, 150);
    c.fill(hex("#121419"));
    c.fill_circle(28.0, 30.0, 8.0, hex("#FF5A4E"));
    fonts.draw_text(
        &mut c,
        "HẾT QUOTA · NGHỈ GIẢI LAO",
        46.0,
        30.0,
        20.0,
        hex("#FF8A7E"),
        TextAlign::Left,
        Some(324.0),
        true,
        false,
    );
    fonts.draw_text(
        &mut c,
        "COOLDOWN",
        192.0,
        84.0,
        46.0,
        hex("#FFB23F"),
        TextAlign::Center,
        Some(350.0),
        true,
        true,
    );
    fonts.draw_text(
        &mut c,
        "Đang chờ giới hạn đặt lại...",
        192.0,
        128.0,
        16.0,
        hex("#A3ABB9"),
        TextAlign::Center,
        Some(350.0),
        false,
        false,
    );
    c.into_surface()
}

fn draw_tests_board(fonts: &mut FontEngine) -> RgbaSurface {
    let mut c = Canvas2d::new(384, 288);
    c.fill(hex("#12161D"));
    c.fill_rect(0.0, 0.0, 384.0, 44.0, hex("#1C2330"));
    fonts.draw_text(
        &mut c,
        "TEST BOARD",
        18.0,
        24.0,
        20.0,
        hex("#E8ECF4"),
        TextAlign::Left,
        Some(300.0),
        true,
        false,
    );
    c.fill_circle(358.0, 22.0, 6.0, hex("#3DDC84"));
    for i in 0..MAX_TEST_ROWS {
        let y = 48.0 + i as f32 * 30.0;
        if i % 2 == 1 {
            c.fill_rect(0.0, y, 384.0, 30.0, hex("#171C26"));
        }
        c.fill_rect(48.0, y + 7.0, 5.0, 16.0, hex("#5F8FD8"));
        let label = format!("Slot {}", i + 1);
        fonts.draw_text(
            &mut c,
            &label,
            64.0,
            y + 16.0,
            15.0,
            hex("#9AA3B2"),
            TextAlign::Left,
            Some(220.0),
            true,
            false,
        );
    }
    c.into_surface()
}

fn draw_opening_plaque(fonts: &mut FontEngine) -> RgbaSurface {
    let mut c = Canvas2d::new(320, 192);
    c.linear_gradient_rect(
        0.0,
        0.0,
        320.0,
        192.0,
        (0.0, 0.0),
        (320.0, 192.0),
        &[(0.0, hex("#E9C46A")), (1.0, hex("#B98A2E"))],
    );
    c.stroke_rect(10.0, 10.0, 300.0, 172.0, 4.0, hex("#6E4F16"));
    fonts.draw_text(
        &mut c,
        "KHAI TRƯƠNG VĂN PHÒNG",
        160.0,
        58.0,
        18.0,
        hex("#4A3410"),
        TextAlign::Center,
        Some(280.0),
        true,
        false,
    );
    fonts.draw_text(
        &mut c,
        "AGENTS STATION",
        160.0,
        110.0,
        30.0,
        hex("#3B280A"),
        TextAlign::Center,
        Some(280.0),
        true,
        false,
    );
    fonts.draw_text(
        &mut c,
        "My Office",
        160.0,
        152.0,
        16.0,
        hex("#4A3410"),
        TextAlign::Center,
        Some(280.0),
        false,
        false,
    );
    c.into_surface()
}

fn decode_capped_thumbnail(path: &Path) -> Option<RgbaSurface> {
    let bytes = std::fs::read(path).ok()?;
    if bytes.is_empty() || bytes.len() > 8 * 1024 * 1024 {
        return None;
    }
    let mut reader = image::ImageReader::new(std::io::Cursor::new(bytes))
        .with_guessed_format()
        .ok()?;
    let mut limits = image::Limits::default();
    limits.max_image_width = Some(4096);
    limits.max_image_height = Some(4096);
    limits.max_alloc = Some(32 * 1024 * 1024);
    reader.limits(limits);
    let img = reader.decode().ok()?;
    let thumb = img
        .thumbnail(PHOTOSTRIP_THUMB_MAX_W, PHOTOSTRIP_THUMB_MAX_H)
        .to_rgba8();
    Some(RgbaSurface {
        width: thumb.width().max(1),
        height: thumb.height().max(1),
        rgba: thumb.into_raw(),
    })
}

/// Spawns all 6 workflow-life props: Postbox + Postman, Incident alarm light, Quota board,
/// Server-room Tests lamp board, Trophy cabinet + plaque, and up to 6 Whiteboard Photostrips.
pub fn spawn_workflow_life(
    commands: &mut Commands,
    pal: &mut WorldPalette,
    materials: &mut Assets<StandardMaterial>,
    images: &mut Assets<Image>,
    fonts: &mut FontEngine,
    spec: &LayoutSpec,
) {
    let is_pro = spec.layout.is_pro();

    // 1. Commit Postbox + Postman (`post.js`)
    let (px, py, pz, pry) = if is_pro {
        (-3.53_f32, -0.15_f32, spec.door_z + 1.30, FRAC_PI_2)
    } else {
        (-7.90, 0.0, 0.25, FRAC_PI_2)
    };
    let pb_rot = Quat::from_rotation_y(pry);
    let pb_at = |lx: f32, ly: f32, lz: f32| Vec3::new(px, py, pz) + pb_rot * Vec3::new(lx, ly, lz);
    let post_red = pal.mat(materials, "#C8261B", 0.45, 0.15);
    let post_dark = pal.mat(materials, "#8E1A13", 0.5, 0.1);
    let slot_m = pal.matte(materials, "#1A1A1E");
    let brass_m = pal.mat(materials, "#B8912F", 0.35, 0.6);
    let letter_m = pal.matte(materials, "#FBFAF5");
    let post_sign_m = pal.textured_lit(images, materials, draw_post_sign(fonts), 0.5);

    spawn_box_rot(commands, pal, post_red.clone(), pb_at(0.0, 1.10, 0.10), Vec3::new(0.36, 0.44, 0.20), pb_rot);
    spawn_cyl_rot(commands, pal, post_dark.clone(), pb_at(0.0, 1.32, 0.10), 0.20, 0.36, pb_rot * Quat::from_rotation_z(FRAC_PI_2));
    spawn_box_rot(commands, pal, slot_m, pb_at(0.0, 1.24, 0.205), Vec3::new(0.22, 0.026, 0.02), pb_rot);
    spawn_box_rot(commands, pal, brass_m, pb_at(0.0, 0.80, 0.01), Vec3::new(0.38, 0.09, 0.015), pb_rot);

    // Hinged front door (`PostboxDoor`)
    let door_ent = commands
        .spawn((
            WorldEntity,
            PostboxDoor,
            Transform::from_translation(pb_at(-0.14, 1.00, 0.205)).with_rotation(pb_rot),
        ))
        .with_children(|p| {
            p.spawn((
                WorldEntity,
                Mesh3d(pal.unit_box.clone()),
                MeshMaterial3d(post_red),
                Transform::from_xyz(0.14, 0.0, 0.004).with_scale(Vec3::new(0.28, 0.22, 0.016)),
            ));
            p.spawn((
                WorldEntity,
                Mesh3d(pal.unit_plane.clone()),
                MeshMaterial3d(post_sign_m),
                Transform::from_xyz(0.14, 0.05, 0.014).with_scale(Vec3::new(0.15, 0.056, 1.0)),
            ));
        })
        .id();
    let _ = door_ent;

    // 3 letter tips sticking out of the slot (`PostboxLetterTip`)
    for (k, lx) in [-0.06_f32, 0.0, 0.06].into_iter().enumerate() {
        let tip = spawn_box_rot(
            commands,
            pal,
            letter_m.clone(),
            pb_at(lx, 1.262 + (k % 2) as f32 * 0.008, 0.205),
            Vec3::new(0.07, 0.05, 0.006),
            pb_rot * Quat::from_rotation_z((k as f32 - 1.0) * 0.12),
        );
        commands.entity(tip).insert((PostboxLetterTip(k), Visibility::Hidden));
    }

    // Postman actor in dark-green uniform + cap + sack (hidden until `on_push()`)
    let postman_pos = pb_at(0.0, 0.0, 0.62);
    let pm_shirt = pal.mat(materials, "#2F5D3A", 0.65, 0.0);
    let pm_pants = pal.matte(materials, "#2B2F36");
    let pm_skin = pal.mat(materials, "#EBC3A0", 0.55, 0.0);
    let pm_bag = pal.matte(materials, "#EFECE3");
    commands
        .spawn((
            WorldEntity,
            PostmanActor {
                home_pos: Vec3::new(postman_pos.x, 0.0, postman_pos.z),
            },
            Transform::from_xyz(postman_pos.x, 0.0, postman_pos.z)
                .with_rotation(Quat::from_rotation_y(pry + PI)),
            Visibility::Hidden,
        ))
        .with_children(|p| {
            p.spawn((
                WorldEntity,
                Mesh3d(pal.unit_cyl.clone()),
                MeshMaterial3d(pm_pants),
                Transform::from_xyz(0.0, 0.28, 0.0).with_scale(Vec3::new(0.24, 0.56, 0.24)),
            ));
            p.spawn((
                WorldEntity,
                Mesh3d(pal.unit_cyl.clone()),
                MeshMaterial3d(pm_shirt.clone()),
                Transform::from_xyz(0.0, 0.78, 0.0).with_scale(Vec3::new(0.32, 0.46, 0.26)),
            ));
            p.spawn((
                WorldEntity,
                Mesh3d(pal.unit_sphere.clone()),
                MeshMaterial3d(pm_skin),
                Transform::from_xyz(0.0, 1.18, 0.0).with_scale(Vec3::splat(0.32)),
            ));
            p.spawn((
                WorldEntity,
                Mesh3d(pal.unit_cyl.clone()),
                MeshMaterial3d(pm_shirt),
                Transform::from_xyz(0.0, 1.32, 0.0).with_scale(Vec3::new(0.34, 0.08, 0.36)),
            ));
            p.spawn((
                WorldEntity,
                Mesh3d(pal.unit_sphere.clone()),
                MeshMaterial3d(pm_bag),
                Transform::from_xyz(0.24, 0.62, 0.06).with_scale(Vec3::new(0.22, 0.28, 0.20)),
            ));
        });

    // 2. Incident Red Warning Strobe PointLight (`incidents.js:29,93`)
    commands.spawn((
        WorldEntity,
        IncidentAlarmLight,
        PointLight {
            color: Color::srgb(1.0, 0.16, 0.16),
            intensity: 0.0,
            range: 18.0,
            shadows_enabled: false,
            ..default()
        },
        Transform::from_xyz(spec.center.x, 3.6, spec.center.y),
    ));

    // 3. Quota Break Countdown Board ("HẾT QUOTA", `quota.js:49-57`)
    let (qx, qy, qz, qry) = if is_pro {
        (11.15_f32, 3.00_f32, -7.91_f32, 0.0_f32)
    } else {
        (spec.clock[0] + 0.30, spec.clock[1] - 0.86, spec.clock[2] + 0.02, spec.clock[3])
    };
    let q_frame = pal.mat(materials, "#23262D", 0.45, 0.2);
    let q_mat = pal.textured_unlit(images, materials, draw_quota_board(fonts));
    commands
        .spawn((
            WorldEntity,
            QuotaBoardRoot,
            Transform::from_xyz(qx, qy, qz)
                .with_rotation(Quat::from_rotation_y(qry))
                .with_scale(Vec3::new(1.0, 0.001, 1.0)),
            Visibility::Hidden,
        ))
        .with_children(|p| {
            p.spawn((
                WorldEntity,
                Mesh3d(pal.unit_box.clone()),
                MeshMaterial3d(q_frame),
                Transform::IDENTITY.with_scale(Vec3::new(1.32, 0.58, 0.05)),
            ));
            p.spawn((
                WorldEntity,
                Mesh3d(pal.unit_plane.clone()),
                MeshMaterial3d(q_mat),
                Transform::from_xyz(0.0, 0.0, 0.03).with_scale(Vec3::new(1.22, 0.48, 1.0)),
            ));
        });

    // 4. Server-Room Tests Lamp Board (`tests.js:91-110`)
    let (tx, ty, tz, try_ang, tw, has_stand) = if is_pro {
        (-5.05_f32, 1.55_f32, -2.62_f32, 0.35_f32, 1.50_f32, true)
    } else {
        (7.42, 1.75, -5.93, 0.0, 1.00, false)
    };
    let th = tw * (288.0 / 384.0);
    let tb_rot = Quat::from_rotation_y(try_ang);
    let tb_at = |lx: f32, ly: f32, lz: f32| Vec3::new(tx, 0.0, tz) + tb_rot * Vec3::new(lx, ly, lz);
    let frame_m = pal.mat(materials, "#1B1F26", 0.45, 0.3);
    let steel_m = pal.mat(materials, "#8E949C", 0.35, 0.6);
    let tests_mat = pal.textured_unlit(images, materials, draw_tests_board(fonts));

    spawn_box_rot(commands, pal, frame_m.clone(), tb_at(0.0, ty, 0.0), Vec3::new(tw + 0.08, th + 0.08, 0.05), tb_rot);
    if has_stand {
        for sx in [-1.0_f32, 1.0] {
            let post_h = ty - th * 0.5;
            spawn_cyl(commands, pal, steel_m.clone(), tb_at(sx * tw * 0.32, post_h * 0.5, -0.02), 0.05, post_h);
            spawn_box_rot(commands, pal, frame_m.clone(), tb_at(sx * tw * 0.32, 0.02, 0.02), Vec3::new(0.08, 0.04, 0.50), tb_rot);
        }
    }
    spawn_quad(commands, pal, tests_mat, tb_at(0.0, ty, 0.027), Vec2::new(tw, th), tb_rot);

    // 8 independent per-row LED discs (`TestLampLed`)
    let u = tw / 384.0;
    for row in 0..MAX_TEST_ROWS {
        let led_mat = pal.owned_emissive(materials, "#2E343D", 0.25);
        let lx = (26.0 - 192.0) * u;
        let ly = ty + (144.0 - (48.0 + row as f32 * 30.0 + 15.0)) * u;
        let led = spawn_sphere(commands, pal, led_mat.clone(), tb_at(lx, ly, 0.032), Vec3::splat(16.0 * u));
        commands.entity(led).insert(TestLampLed { row, mat: led_mat });
    }

    // 5. Live Trophies Cabinet + Opening Plaque (`trophies.js:56-120`)
    let gold_m = pal.mat(materials, "#E9B949", 0.26, 0.82);
    let silver_m = pal.mat(materials, "#D3D8DF", 0.25, 0.82);
    let bronze_m = pal.mat(materials, "#C9885A", 0.28, 0.78);
    let base_wood = pal.mat(materials, "#2B2118", 0.5, 0.05);

    if is_pro {
        let cab_pos = Vec3::new(-7.74, 0.0, -7.30);
        let cab_rot = Quat::from_rotation_y(FRAC_PI_2);
        let cab_at = |lx: f32, ly: f32, lz: f32| cab_pos + cab_rot * Vec3::new(lx, ly, lz);
        let walnut = pal.mat(materials, "#4A3628", 0.5, 0.0);
        let back_m = pal.matte(materials, "#E9E2D6");
        let glass_m = pal.glass(materials, "#D8ECF7", 0.14);

        spawn_box_rot(commands, pal, walnut.clone(), cab_at(0.0, 0.25, 0.0), Vec3::new(1.02, 0.50, 0.44), cab_rot);
        for sx in [-1.0_f32, 1.0] {
            spawn_box_rot(commands, pal, walnut.clone(), cab_at(sx * 0.49, 1.25, 0.0), Vec3::new(0.04, 1.50, 0.44), cab_rot);
        }
        spawn_box_rot(commands, pal, walnut.clone(), cab_at(0.0, 2.02, 0.0), Vec3::new(1.02, 0.06, 0.46), cab_rot);
        spawn_box_rot(commands, pal, back_m, cab_at(0.0, 1.25, -0.20), Vec3::new(0.96, 1.48, 0.02), cab_rot);
        for sy in [1.41_f32, 0.97] {
            spawn_box_rot(commands, pal, glass_m.clone(), cab_at(0.0, sy, 0.0), Vec3::new(0.94, 0.02, 0.38), cab_rot);
        }
        spawn_box_rot(commands, pal, glass_m, cab_at(0.0, 1.25, 0.215), Vec3::new(0.94, 1.48, 0.01), cab_rot);

        // 9 trophy figurine slots across 3 shelves (3 per shelf)
        let shelf_ys = [1.42_f32, 0.98, 0.54];
        for slot in 0..MAX_TROPHY_SLOTS {
            let lx = ((slot % 3) as f32 - 1.0) * 0.30;
            let sy = shelf_ys[slot / 3];
            let tm = match slot % 3 {
                0 => gold_m.clone(),
                1 => silver_m.clone(),
                _ => bronze_m.clone(),
            };
            commands
                .spawn((
                    WorldEntity,
                    TrophyCabinetSlot { index: slot },
                    Transform::from_translation(cab_at(lx, sy, 0.01)).with_rotation(cab_rot),
                    Visibility::Hidden,
                ))
                .with_children(|p| {
                    p.spawn((
                        WorldEntity,
                        Mesh3d(pal.unit_box.clone()),
                        MeshMaterial3d(base_wood.clone()),
                        Transform::from_xyz(0.0, 0.03, 0.0).with_scale(Vec3::new(0.14, 0.06, 0.11)),
                    ));
                    p.spawn((
                        WorldEntity,
                        Mesh3d(pal.unit_cyl.clone()),
                        MeshMaterial3d(tm.clone()),
                        Transform::from_xyz(0.0, 0.14, 0.0).with_scale(Vec3::new(0.11, 0.16, 0.11)),
                    ));
                    p.spawn((
                        WorldEntity,
                        Mesh3d(pal.unit_sphere.clone()),
                        MeshMaterial3d(tm),
                        Transform::from_xyz(0.0, 0.25, 0.0).with_scale(Vec3::splat(0.11)),
                    ));
                });
        }

        // Brass opening plaque over the director's sofa
        let plaque_m = pal.textured_lit(images, materials, draw_opening_plaque(fonts), 0.35);
        commands
            .spawn((
                WorldEntity,
                TrophyPlaque,
                Transform::from_xyz(-7.94, 1.62, -4.10).with_rotation(Quat::from_rotation_y(FRAC_PI_2)),
                Visibility::Visible,
            ))
            .with_children(|p| {
                p.spawn((
                    WorldEntity,
                    Mesh3d(pal.unit_box.clone()),
                    MeshMaterial3d(walnut),
                    Transform::from_xyz(0.0, 0.0, 0.018).with_scale(Vec3::new(0.62, 0.42, 0.035)),
                ));
                p.spawn((
                    WorldEntity,
                    Mesh3d(pal.unit_plane.clone()),
                    MeshMaterial3d(plaque_m),
                    Transform::from_xyz(0.0, 0.0, 0.038).with_scale(Vec3::new(0.50, 0.30, 1.0)),
                ));
            });
    } else {
        // Cozy: shelf trophies along `spec.shelf`
        let [sx, sz, _] = spec.shelf;
        for slot in 0..MAX_TROPHY_SLOTS.min(5) {
            let tm = match slot % 3 {
                0 => gold_m.clone(),
                1 => silver_m.clone(),
                _ => bronze_m.clone(),
            };
            let t = spawn_cyl(
                commands,
                pal,
                tm,
                Vec3::new(sx - 0.7 + slot as f32 * 0.34, 2.42, sz),
                0.14,
                0.22,
            );
            commands.entity(t).insert((TrophyCabinetSlot { index: slot }, Visibility::Hidden));
        }
    }

    // 6. Whiteboard Photostrips (up to 6 slots along lower-right of `spec.board`, `photostrips.js`)
    let [bx, by, bz, bry] = spec.board;
    let board_rot = Quat::from_rotation_y(bry);
    let default_strip = draw_photostrip(
        &PhotostripInput {
            tag: "v1.0.0".into(),
            day: "25/11".into(),
            seed: 2,
            people: vec![
                StripPerson { hair: hex("#3B2618"), skin: hex("#F5D0B5"), shirt: hex("#D97757") },
                StripPerson { hair: hex("#1E1E24"), skin: hex("#EBC3A0"), shirt: hex("#5F8FD8") },
                StripPerson { hair: hex("#5A3A2A"), skin: hex("#F7DAC4"), shirt: hex("#3E9B74") },
            ],
        },
        fonts,
    );
    let default_strip_mat = pal.textured_lit(images, materials, default_strip, 0.55);

    for slot in 0..MAX_PHOTOSTRIPS {
        let tilt = (((slot as i32 * 37) % 7) - 3) as f32 * 0.02;
        let local = Vec3::new(0.075 + slot as f32 * 0.0005, -0.575, -0.28 - slot as f32 * 0.205);
        let world_pos = Vec3::new(bx, by, bz) + board_rot * local;
        let rot = board_rot * Quat::from_rotation_y(FRAC_PI_2) * Quat::from_rotation_z(tilt);
        let slot_mat = if slot == 0 {
            default_strip_mat.clone()
        } else {
            // Dedicated owned material per dynamic slot so `sync_photostrips_from_shots_dir`
            // can swap base_color_texture independently and purge cleanly on unload.
            pal.owned_emissive(materials, "#FFFFFF", 0.0)
        };
        let ent = spawn_quad(commands, pal, slot_mat.clone(), world_pos, Vec2::new(0.14, 0.39), rot);
        let vis = if slot == 0 {
            Visibility::Visible
        } else {
            Visibility::Hidden
        };
        commands.entity(ent).insert((PhotostripSlot { index: slot, mat: slot_mat }, vis));
    }
}

/// Syncs up to `MAX_PHOTOSTRIPS` (6) thumbnails from `data/shots/` onto the whiteboard photostrip
/// slots using capped `RENDER_WORLD` images tracked in `WorldPalette`. Skips disk decode & GPU
/// upload completely when the directory signature `(path, mtime_ms, len)` is unchanged, and replaces
/// prior slot handles in-place so `Assets<Image>::len()` stays strictly bounded.
pub fn sync_photostrips_from_shots_dir(
    shots_dir: &Path,
    state: &mut PhotostripsState,
    pal: &mut WorldPalette,
    images: &mut Assets<Image>,
    materials: &mut Assets<StandardMaterial>,
    slots: &mut Query<(&PhotostripSlot, &mut Visibility)>,
) -> usize {
    let sig = compute_shots_signature(shots_dir);
    if !state.needs_sync && sig == state.last_sig {
        return state.slot_images.iter().filter(|h| h.is_some()).count();
    }

    state.loaded_paths = sig.iter().map(|(p, _, _)| p.clone()).collect();
    state.last_sig = sig;
    state.needs_sync = false;
    state.dirty = false;

    let mut loaded = 0usize;
    for i in 0..MAX_PHOTOSTRIPS {
        if let Some(path_str) = state.loaded_paths.get(i)
            && let Some(surf) = decode_capped_thumbnail(Path::new(path_str))
        {
            let handle = pal.replace_owned_image(images, state.slot_images[i].as_ref(), surf);
            state.slot_images[i] = Some(handle);
            loaded += 1;
        } else if let Some(old_h) = state.slot_images[i].take() {
            pal.remove_owned_image(images, &old_h);
        }
    }

    if loaded == 0 {
        return 0;
    }

    for (slot, mut vis) in slots.iter_mut() {
        if let Some(tex) = state.slot_images.get(slot.index).and_then(|h| h.as_ref()) {
            if let Some(mat) = materials.get_mut(&slot.mat) {
                mat.base_color = Color::WHITE;
                mat.base_color_texture = Some(tex.clone());
                mat.emissive = LinearRgba::BLACK;
                mat.perceptual_roughness = 0.55;
            }
            *vis = Visibility::Visible;
        } else {
            *vis = Visibility::Hidden;
        }
    }
    loaded
}

#[allow(clippy::too_many_arguments, clippy::type_complexity)]
pub fn update_workflow_life_system(
    time: Res<Time>,
    mut postbox: Option<ResMut<PostboxState>>,
    mut incidents: Option<ResMut<IncidentState>>,
    mut quota: Option<ResMut<QuotaBoardState>>,
    mut tests: Option<ResMut<TestsBoardState>>,
    mut trophies: Option<ResMut<TrophiesShelfState>>,
    mut life: Option<ResMut<OfficeLifeState>>,
    mut materials: Option<ResMut<Assets<StandardMaterial>>>,
    mut q_door: Query<
        &mut Transform,
        (
            With<PostboxDoor>,
            Without<PostmanActor>,
            Without<QuotaBoardRoot>,
        ),
    >,
    mut q_tips: Query<
        (&PostboxLetterTip, &mut Visibility),
        (
            Without<PostmanActor>,
            Without<QuotaBoardRoot>,
            Without<TrophyCabinetSlot>,
            Without<TrophyPlaque>,
        ),
    >,
    mut q_postman: Query<
        (&PostmanActor, &mut Visibility, &mut Transform),
        (
            Without<PostboxDoor>,
            Without<QuotaBoardRoot>,
            Without<PostboxLetterTip>,
            Without<TrophyCabinetSlot>,
            Without<TrophyPlaque>,
        ),
    >,
    mut q_alarm: Query<&mut PointLight, With<IncidentAlarmLight>>,
    mut q_quota: Query<
        (&mut Visibility, &mut Transform),
        (
            With<QuotaBoardRoot>,
            Without<PostboxDoor>,
            Without<PostmanActor>,
            Without<PostboxLetterTip>,
            Without<TrophyCabinetSlot>,
            Without<TrophyPlaque>,
        ),
    >,
    q_lamps: Query<&TestLampLed>,
    mut q_trophies: Query<
        (&TrophyCabinetSlot, &mut Visibility),
        (
            Without<PostboxLetterTip>,
            Without<PostmanActor>,
            Without<QuotaBoardRoot>,
            Without<TrophyPlaque>,
        ),
    >,
    mut q_plaque: Query<
        &mut Visibility,
        (
            With<TrophyPlaque>,
            Without<PostboxLetterTip>,
            Without<PostmanActor>,
            Without<QuotaBoardRoot>,
            Without<TrophyCabinetSlot>,
        ),
    >,
) {
    let dt = time.delta_secs().clamp(0.0, 0.25);

    // 1. Postbox + Postman arrival on git push
    if let Some(pb) = postbox.as_deref_mut() {
        match pb.postman_stage {
            PostmanStage::Offstage => {
                if pb.push_due_secs > 0.0 {
                    pb.push_due_secs = (pb.push_due_secs - dt).max(0.0);
                    if pb.push_due_secs <= 0.0 {
                        if pb.in_box > 0 {
                            pb.postman_stage = PostmanStage::Arriving;
                            pb.stage_elapsed = 0.0;
                        }
                    }
                }
            }
            PostmanStage::Arriving => {
                pb.stage_elapsed += dt;
                if pb.stage_elapsed >= 2.0 {
                    pb.postman_stage = PostmanStage::Collecting;
                    pb.stage_elapsed = 0.0;
                }
            }
            PostmanStage::Collecting => {
                pb.stage_elapsed += dt;
                if pb.stage_elapsed >= 2.4 {
                    pb.in_box = 0;
                    pb.postman_stage = PostmanStage::Departing;
                    pb.stage_elapsed = 0.0;
                }
            }
            PostmanStage::Departing => {
                pb.stage_elapsed += dt;
                if pb.stage_elapsed >= 1.8 {
                    pb.postman_stage = PostmanStage::Offstage;
                    pb.stage_elapsed = 0.0;
                }
            }
        }

        let target_open = if pb.postman_stage == PostmanStage::Collecting {
            1.0
        } else {
            0.0
        };
        pb.door_open += (target_open - pb.door_open) * (dt * 6.0).min(1.0);
        for mut tf in &mut q_door {
            tf.rotation = Quat::from_rotation_y(FRAC_PI_2 - pb.door_open * 1.8);
        }

        let shown_tips = pb.in_box.min(3);
        for (tip, mut vis) in &mut q_tips {
            let want = if tip.0 < shown_tips {
                Visibility::Visible
            } else {
                Visibility::Hidden
            };
            if *vis != want {
                *vis = want;
            }
        }

        let postman_vis = if pb.postman_stage == PostmanStage::Offstage {
            Visibility::Hidden
        } else {
            Visibility::Visible
        };
        for (pm, mut vis, mut tf) in &mut q_postman {
            if *vis != postman_vis {
                *vis = postman_vis;
            }
            let offset_x = match pb.postman_stage {
                PostmanStage::Arriving => (1.0 - (pb.stage_elapsed / 2.0).clamp(0.0, 1.0)) * 1.4,
                PostmanStage::Departing => (pb.stage_elapsed / 1.8).clamp(0.0, 1.0) * 1.4,
                _ => 0.0,
            };
            tf.translation = pm.home_pos + Vec3::new(offset_x, 0.0, 0.0);
        }
    }

    // 2. Incidents red warning strobe
    if let Some(inc) = incidents.as_deref_mut() {
        inc.sim_secs += dt;
        let active = inc.is_active();
        if let Some(l) = life.as_deref_mut() {
            l.incident_active = active;
        }
        let intensity = if active {
            (0.5 + 0.5 * (inc.sim_secs * 9.0).sin()) * 22_000.0
        } else {
            0.0
        };
        for mut pl in &mut q_alarm {
            pl.intensity = intensity;
        }
    }

    // 3. Quota break sign fold animation
    if let Some(q) = quota.as_deref_mut() {
        if q.exhausted && q.remaining_secs > 0.0 {
            q.remaining_secs = (q.remaining_secs - dt).max(0.0);
            if q.remaining_secs <= 0.0 {
                q.exhausted = false;
            }
        }
        let dir = if q.exhausted { 1.0 } else { -1.0 };
        q.fold = (q.fold + dir * dt * 3.0).clamp(0.0, 1.0);
        q.blink_on = ((time.elapsed_secs() * 1.6) as u32) % 2 == 0;
        let ease = 1.0 - (1.0 - q.fold).powi(3);
        let want_vis = if q.fold > 0.01 {
            Visibility::Visible
        } else {
            Visibility::Hidden
        };
        for (mut vis, mut tf) in &mut q_quota {
            if *vis != want_vis {
                *vis = want_vis;
            }
            tf.scale.y = ease.max(0.001);
        }
    }

    // 4. Server-room Tests lamp board LEDs (amber blink / green / red / fade to grey)
    if let Some(tb) = tests.as_deref_mut() {
        tb.sim_secs += dt;
        let t = tb.sim_secs;
        if let Some(mats) = materials.as_deref_mut() {
            for led in &q_lamps {
                let row = tb.rows[led.row.min(MAX_TEST_ROWS - 1)];
                let col = match row.status {
                    TestRunStatus::Idle => LinearRgba::new(0.08, 0.09, 0.11, 1.0),
                    TestRunStatus::Running => {
                        if (t * 7.0).sin() > 0.0 {
                            LinearRgba::new(2.2, 1.5, 0.35, 1.0)
                        } else {
                            LinearRgba::new(0.45, 0.30, 0.08, 1.0)
                        }
                    }
                    TestRunStatus::Pass => {
                        let fade = ((t - row.updated_at - (TEST_LAMP_FADE_SECS - 30.0)) / 30.0).clamp(0.0, 1.0);
                        LinearRgba::new(
                            0.25 * (1.0 - fade) + 0.08 * fade,
                            2.1 * (1.0 - fade) + 0.09 * fade,
                            0.65 * (1.0 - fade) + 0.11 * fade,
                            1.0,
                        )
                    }
                    TestRunStatus::Fail => {
                        let fade = ((t - row.updated_at - (TEST_LAMP_FADE_SECS - 30.0)) / 30.0).clamp(0.0, 1.0);
                        LinearRgba::new(
                            2.3 * (1.0 - fade) + 0.08 * fade,
                            0.32 * (1.0 - fade) + 0.09 * fade,
                            0.32 * (1.0 - fade) + 0.11 * fade,
                            1.0,
                        )
                    }
                };
                if let Some(m) = mats.get_mut(&led.mat) {
                    m.emissive = col;
                }
            }
        }
    }

    // 5. Live Trophies shelf visibility sync
    if let Some(tr) = trophies.as_deref_mut() {
        let count = tr.entries.len();
        if let Some(l) = life.as_deref_mut() {
            l.earned_trophies = count;
        }
        if tr.dirty {
            tr.dirty = false;
            for (slot, mut vis) in &mut q_trophies {
                let want = if slot.index < count {
                    Visibility::Visible
                } else {
                    Visibility::Hidden
                };
                if *vis != want {
                    *vis = want;
                }
            }
            let plaque_vis = if tr.has_plaque || count > 0 {
                Visibility::Visible
            } else {
                Visibility::Hidden
            };
            for mut vis in &mut q_plaque {
                if *vis != plaque_vis {
                    *vis = plaque_vis;
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::layout::OfficeLayout;
    use bevy::asset::RenderAssetUsages;
    use std::path::PathBuf;

    #[test]
    fn workflow_life_postbox_incidents_quota_tests_trophies_and_photostrips_cap_work_with_zero_leaks() {
        let assets_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../assets");
        let mut fonts = FontEngine::from_assets_dir(&assets_dir);
        let tmp = tempfile::tempdir().unwrap();

        // 1. Trophies from data/totals.json
        let totals_path = tmp.path().join("totals.json");
        std::fs::write(
            &totals_path,
            r#"{"since":1700000000000,"live":{"prompts":1200,"deploy":12},"earned":{"open":1700000000000,"p100":1,"p1k":2,"d1":3,"d10":4}}"#,
        )
        .unwrap();
        let mut trophies = TrophiesShelfState::default();
        let loaded_trophies = trophies.load_from_totals_json(&totals_path);
        assert_eq!(loaded_trophies, 2, "highest tier per milestone kind: prompts(silver) + deploy(silver)");
        assert!(trophies.has_plaque);

        // 2. Photostrips from data/shots/ (safe when missing, capped at <= 6)
        let missing_dir = tmp.path().join("no_shots");
        let mut strips = PhotostripsState::default();
        assert_eq!(strips.scan_shots_dir(&missing_dir), 0);

        let shots_dir = tmp.path().join("shots");
        std::fs::create_dir_all(&shots_dir).unwrap();
        let img = image::RgbaImage::from_pixel(320, 480, image::Rgba([230, 80, 120, 255]));
        for i in 0..9 {
            img.save(shots_dir.join(format!("shot-{i:02}.png"))).unwrap();
        }
        assert_eq!(strips.scan_shots_dir(&shots_dir), MAX_PHOTOSTRIPS);
        assert!(strips.loaded_paths.len() <= MAX_PHOTOSTRIPS);

        // 3. ECS systems: Postbox, Incidents, Quota, Tests Board, Trophies, Photostrips
        let mut app = App::new();
        app.insert_resource(Time::<()>::default())
            .insert_resource(Assets::<Mesh>::default())
            .insert_resource(Assets::<StandardMaterial>::default())
            .insert_resource(Assets::<Image>::default())
            .insert_resource(PostboxState::default())
            .insert_resource(IncidentState::default())
            .insert_resource(QuotaBoardState::default())
            .insert_resource(TestsBoardState::default())
            .insert_resource(trophies)
            .insert_resource(strips)
            .insert_resource(OfficeLifeState::default())
            .add_systems(Update, update_workflow_life_system);

        let spec = LayoutSpec::for_layout(OfficeLayout::Pro);
        app.world_mut().resource_scope(|world, mut meshes: Mut<Assets<Mesh>>| {
            world.resource_scope(|world, mut mats: Mut<Assets<StandardMaterial>>| {
                world.resource_scope(|world, mut imgs: Mut<Assets<Image>>| {
                    let mut pal = WorldPalette::new(&mut meshes);
                    let mut cmds = world.commands();
                    spawn_workflow_life(&mut cmds, &mut pal, &mut mats, &mut imgs, &mut fonts, &spec);
                    world.insert_resource(pal);
                });
            });
        });
        app.world_mut().flush();

        // Sync photostrips from shots dir into 3D slots
        let synced_strips = app.world_mut().resource_scope(|world, mut pal: Mut<WorldPalette>| {
            world.resource_scope(|world, mut imgs: Mut<Assets<Image>>| {
                world.resource_scope(|world, mut mats: Mut<Assets<StandardMaterial>>| {
                    world.resource_scope(|world, mut st: Mut<PhotostripsState>| {
                        let mut q = world.query::<(&PhotostripSlot, &mut Visibility)>();
                        // Use a one-shot system via direct world query iteration
                        let mut loaded = 0usize;
                        let count = st.scan_shots_dir(&shots_dir);
                        st.dirty = false;
                        if count > 0 {
                            let mut handles = Vec::new();
                            for p in &st.loaded_paths {
                                if let Some(surf) = decode_capped_thumbnail(Path::new(p)) {
                                    assert!(surf.width <= PHOTOSTRIP_THUMB_MAX_W);
                                    assert!(surf.height <= PHOTOSTRIP_THUMB_MAX_H);
                                    handles.push(pal.upload_surface(&mut imgs, surf));
                                }
                            }
                            loaded = handles.len().min(MAX_PHOTOSTRIPS);
                            for (slot, mut vis) in q.iter_mut(world) {
                                if slot.index < loaded {
                                    if let Some(m) = mats.get_mut(&slot.mat) {
                                        m.base_color_texture = Some(handles[slot.index].clone());
                                    }
                                    *vis = Visibility::Visible;
                                } else {
                                    *vis = Visibility::Hidden;
                                }
                            }
                        }
                        loaded
                    })
                })
            })
        });
        assert_eq!(synced_strips, MAX_PHOTOSTRIPS);

        // Postbox commit + push
        {
            let mut pb = app.world_mut().resource_mut::<PostboxState>();
            pb.on_commit();
            pb.on_commit();
            pb.on_push();
            assert_eq!(pb.in_box, 2);
        }
        // Incidents: 3 failures trigger red warning strobe
        {
            let mut inc = app.world_mut().resource_mut::<IncidentState>();
            inc.record_failure();
            inc.record_failure();
            assert!(!inc.is_active());
            inc.record_failure();
            assert!(inc.is_active());
        }
        // Quota break sign + Tests lamp board
        app.world_mut().resource_mut::<QuotaBoardState>().set_exhausted(true, 1800.0);
        {
            let mut tb = app.world_mut().resource_mut::<TestsBoardState>();
            tb.set_row(0, TestRunStatus::Running);
            tb.set_row(1, TestRunStatus::Pass);
            tb.set_row(2, TestRunStatus::Fail);
        }

        // Advance simulation -> alarm pulses, quota unfolds, postman collects & empties postbox
        for _ in 0..50 {
            app.world_mut()
                .resource_mut::<Time<()>>()
                .advance_by(std::time::Duration::from_millis(250));
            app.update();
        }
        assert!(app.world().resource::<OfficeLifeState>().incident_active);
        assert!(app.world().resource::<QuotaBoardState>().fold > 0.9);
        assert_eq!(app.world().resource::<PostboxState>().in_box, 0);

        // Verify all uploaded images use RENDER_WORLD
        for (_, img) in app.world().resource::<Assets<Image>>().iter() {
            assert_eq!(img.asset_usage, RenderAssetUsages::RENDER_WORLD);
        }

        // Zero-leak purge check
        let ents: Vec<Entity> = app.world_mut().query_filtered::<Entity, With<WorldEntity>>().iter(app.world()).collect();
        for e in ents {
            app.world_mut().despawn(e);
        }
        let mut pal = app.world_mut().remove_resource::<WorldPalette>().unwrap();
        app.world_mut().resource_scope(|world, mut meshes: Mut<Assets<Mesh>>| {
            world.resource_scope(|world, mut mats: Mut<Assets<StandardMaterial>>| {
                world.resource_scope(|_world, mut imgs: Mut<Assets<Image>>| {
                    pal.purge_all(&mut meshes, &mut mats, &mut imgs);
                });
            });
        });
        assert_eq!(app.world().resource::<Assets<Mesh>>().len(), 0);
        assert_eq!(app.world().resource::<Assets<StandardMaterial>>().len(), 0);
        assert_eq!(app.world().resource::<Assets<Image>>().len(), 0);
    }

    #[test]
    fn sync_photostrips_from_shots_dir_skips_unchanged_replaces_in_place_and_unloads_zero_leaks() {
        use crate::{BackdropAssets, ChibiMeshCache, OfficeIbl, unload_full_office_world};
        use bevy::ecs::system::SystemState;

        let assets_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../assets");
        let mut fonts = FontEngine::from_assets_dir(&assets_dir);
        let tmp = tempfile::tempdir().unwrap();
        let shots_dir = tmp.path().join("shots");
        std::fs::create_dir_all(&shots_dir).unwrap();

        let img_a = image::RgbaImage::from_pixel(160, 240, image::Rgba([200, 90, 90, 255]));
        for i in 0..4 {
            img_a.save(shots_dir.join(format!("shot-{i:02}.png"))).unwrap();
        }

        let mut world = World::new();
        world.insert_resource(Assets::<Mesh>::default());
        world.insert_resource(Assets::<StandardMaterial>::default());
        world.insert_resource(Assets::<Image>::default());
        world.insert_resource(PhotostripsState::default());

        let spec = LayoutSpec::for_layout(OfficeLayout::Pro);
        world.resource_scope(|world, mut meshes: Mut<Assets<Mesh>>| {
            world.resource_scope(|world, mut mats: Mut<Assets<StandardMaterial>>| {
                world.resource_scope(|world, mut imgs: Mut<Assets<Image>>| {
                    let mut pal = WorldPalette::new(&mut meshes);
                    let mut cmds = world.commands();
                    spawn_workflow_life(&mut cmds, &mut pal, &mut mats, &mut imgs, &mut fonts, &spec);
                    world.insert_resource(pal);
                });
            });
        });
        world.flush();

        let base_imgs = world.resource::<Assets<Image>>().len();

        let run_sync = |world: &mut World, dir: &Path| -> usize {
            let mut ss: SystemState<(
                ResMut<PhotostripsState>,
                ResMut<WorldPalette>,
                ResMut<Assets<Image>>,
                ResMut<Assets<StandardMaterial>>,
                Query<(&PhotostripSlot, &mut Visibility)>,
            )> = SystemState::new(world);
            let (mut st, mut pal, mut imgs, mut mats, mut q) = ss.get_mut(world);
            sync_photostrips_from_shots_dir(dir, &mut st, &mut pal, &mut imgs, &mut mats, &mut q)
        };

        // 1. First sync loads 4 thumbnails
        assert_eq!(run_sync(&mut world, &shots_dir), 4);
        let after_first = world.resource::<Assets<Image>>().len();
        assert_eq!(after_first, base_imgs + 4);

        // 2. Repeated sync with unchanged files must NOT allocate new Images
        for _ in 0..5 {
            assert_eq!(run_sync(&mut world, &shots_dir), 4);
        }
        assert_eq!(
            world.resource::<Assets<Image>>().len(),
            after_first,
            "unchanged repeated syncs must not leak or grow Assets<Image>"
        );

        // 3. Modify existing PNG (different dimensions/bytes -> different len) + add 2 more (total 6)
        let img_b = image::RgbaImage::from_pixel(180, 260, image::Rgba([40, 180, 120, 255]));
        img_b.save(shots_dir.join("shot-00.png")).unwrap();
        img_b.save(shots_dir.join("shot-04.png")).unwrap();
        img_b.save(shots_dir.join("shot-05.png")).unwrap();
        assert_eq!(run_sync(&mut world, &shots_dir), 6);
        let after_six = world.resource::<Assets<Image>>().len();
        assert_eq!(
            after_six,
            base_imgs + 6,
            "modifying & expanding photostrips must replace old handles in-place without leaking"
        );

        // 4. Modify all 6 again -> Assets<Image>::len() MUST stay strictly bounded at base_imgs + 6
        let img_c = image::RgbaImage::from_pixel(200, 280, image::Rgba([60, 90, 220, 255]));
        for i in 0..6 {
            img_c.save(shots_dir.join(format!("shot-{i:02}.png"))).unwrap();
        }
        assert_eq!(run_sync(&mut world, &shots_dir), 6);
        assert_eq!(
            world.resource::<Assets<Image>>().len(),
            base_imgs + 6,
            "Assets<Image> must stay strictly bounded across repeated modified refreshes"
        );

        // 5. Full unload via unload_full_office_world must reach 0 Images / Meshes / Materials
        let ents: Vec<Entity> = world
            .query_filtered::<Entity, With<WorldEntity>>()
            .iter(&world)
            .collect();
        world.resource_scope(|world, mut meshes: Mut<Assets<Mesh>>| {
            world.resource_scope(|world, mut mats: Mut<Assets<StandardMaterial>>| {
                world.resource_scope(|world, mut imgs: Mut<Assets<Image>>| {
                    let mut pal = world.remove_resource::<WorldPalette>();
                    let mut ba = world.remove_resource::<BackdropAssets>();
                    let ibl = world.remove_resource::<OfficeIbl>();
                    let cm = world.remove_resource::<ChibiMeshCache>();
                    let mut cmds = world.commands();
                    unload_full_office_world(
                        &mut cmds,
                        ents,
                        pal.as_mut(),
                        ba.as_mut(),
                        ibl.as_ref(),
                        cm.as_ref(),
                        &mut meshes,
                        &mut mats,
                        &mut imgs,
                    );
                });
            });
        });
        world.flush();
        assert_eq!(world.resource::<Assets<Mesh>>().len(), 0);
        assert_eq!(world.resource::<Assets<StandardMaterial>>().len(), 0);
        assert_eq!(world.resource::<Assets<Image>>().len(), 0);
    }
}
