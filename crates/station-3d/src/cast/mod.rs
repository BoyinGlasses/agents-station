pub mod meals_breaks;
pub mod office_games;
pub mod staff_roles;

use crate::chibi_meshes::ChibiMeshCache;
use crate::chibi_props::{ActivePropEntity, PropKind, set_active_prop};
use crate::palette::WorldPalette;
use crate::people::LiveSessionWorker;
use bevy::prelude::*;

pub use meals_breaks::{
    BREAK_COFFEE_DIALOGUE, COM_NHA_WAIT_SPOTS, COM_TRUA_FRIDAY_DIALOGUE,
    COM_TRUA_WEEKDAY_DIALOGUE, DutyChore, ErrandKind, MealsBreaksState, TEATIME_DIALOGUE,
    step_meals_breaks_system,
};
pub use office_games::{
    BilliardsStage, KARAOKE_SONGS, KaraokeSong, KaraokeStage, OfficeGamesState, SongMood,
    karaoke_score, step_office_games_system,
};
pub use staff_roles::{
    FarewellStage, InternTenure, OfficeStaffNpc, StaffCastState, StaffRole,
    spawn_office_staff_cast, step_staff_roles_system,
};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum OutingShopKind {
    CircleK,
    Gs25,
    Mixue,
    SevenEleven,
    Highlands,
    Cong,
    PhucLong,
    Katinat,
    WinMart,
}

impl OutingShopKind {
    pub fn prop_kind(self) -> PropKind {
        match self {
            Self::CircleK => PropKind::Noodle,
            Self::Gs25 => PropKind::Tokbokki,
            Self::Mixue => PropKind::IceCream,
            Self::SevenEleven => PropKind::Onigiri,
            Self::Highlands | Self::Cong => PropKind::Coffee,
            Self::PhucLong | Self::Katinat => PropKind::Boba,
            Self::WinMart => PropKind::CarrierBag,
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            Self::CircleK => "Circle K",
            Self::Gs25 => "GS25",
            Self::Mixue => "Mixue",
            Self::SevenEleven => "7-Eleven",
            Self::Highlands => "Highlands Coffee",
            Self::Cong => "Cộng Cà Phê",
            Self::PhucLong => "Phúc Long",
            Self::Katinat => "Katinat",
            Self::WinMart => "WinMart+",
        }
    }
}

/// Shared deterministic office wall-clock for scheduled cultural & leisure routines.
#[derive(Resource, Debug, Clone)]
pub struct CastClock {
    /// Minutes since midnight (`0..1440`). E.g. `12 * 60 = 720` (noon), `16 * 60 = 960` (16:00).
    pub minutes: u32,
    /// Day of week (`0 = Sun`, `1 = Mon`, ..., `5 = Fri`, `6 = Sat`).
    pub weekday: u8,
    /// Day of month (`1..=31`).
    pub day_of_month: u8,
    /// True when today is the last weekday of the month (payday).
    pub is_last_weekday_of_month: bool,
    /// True on working days (Mon-Fri or make-up Saturday, non-holiday).
    pub is_workday: bool,
    /// Monotonic day counter used for once-per-day guards without `String` allocations.
    pub day_index: u32,
    /// Accumulated simulation seconds.
    pub sim_secs: f32,
}

impl Default for CastClock {
    fn default() -> Self {
        Self {
            minutes: 9 * 60 + 30,
            weekday: 3, // Wednesday
            day_of_month: 15,
            is_last_weekday_of_month: false,
            is_workday: true,
            day_index: 1,
            sim_secs: 0.0,
        }
    }
}

impl CastClock {
    pub fn is_monday(&self) -> bool {
        self.weekday == 1
    }

    pub fn is_friday(&self) -> bool {
        self.weekday == 5
    }

    /// Payday on the 5th, 10th, 25th, or last weekday of the month (`payday.js`).
    pub fn is_payday(&self) -> bool {
        matches!(self.day_of_month, 5 | 10 | 25) || self.is_last_weekday_of_month
    }
}

/// Event & trigger bus feeding scheduled and reactive cultural routines.
#[derive(Resource, Debug, Clone)]
pub struct CastEvents {
    /// New git release tag (e.g. `"v1.2.0"`) triggering Korean photobooth strip outing.
    pub release_tag: Option<String>,
    /// True when CI has a green build today and hasn't turned red (`pickle.js`).
    pub ci_green: bool,
    /// True if any red CI occurred today (blocks pickleball unless forced).
    pub ci_red_today: bool,
    /// Host laptop battery percentage (`< 20` triggers V-Green scooter `EvSwap`).
    pub battery_pct: u8,
    /// True when host laptop is plugged in / charging.
    pub battery_charging: bool,
    /// Meeting table bell ring trigger (`gatherings.js`).
    pub bell_rung: bool,
    /// Prop errand triggers (`errands.js`).
    pub printer_job: bool,
    pub shredder_job: bool,
    pub water_cooler_job: bool,
    pub plant_thirsty: bool,
    /// Ambient outdoor temperature in Celsius (`>= 32.0` triggers Mixue ice-cream run).
    pub ambient_temp_c: f32,
    /// Whether City street backdrop is active (enables street outings).
    pub city_street_available: bool,
    /// Explicit debug/manual triggers (consumed when started).
    pub force_billiards: bool,
    pub force_foosball: bool,
    pub force_gaming: bool,
    pub force_karaoke: bool,
    pub force_comnha: bool,
    pub force_comtrua: bool,
    pub force_breaks: bool,
    pub force_teatime: bool,
    pub force_duty: bool,
    pub force_outing: Option<OutingShopKind>,
    pub force_nhau: bool,
    pub force_payday: bool,
    pub force_pickle: bool,
    pub force_evswap: bool,
}

impl Default for CastEvents {
    fn default() -> Self {
        Self {
            release_tag: None,
            ci_green: false,
            ci_red_today: false,
            battery_pct: 85,
            battery_charging: false,
            bell_rung: false,
            printer_job: false,
            shredder_job: false,
            water_cooler_job: false,
            plant_thirsty: false,
            ambient_temp_c: 28.0,
            city_street_available: true,
            force_billiards: false,
            force_foosball: false,
            force_gaming: false,
            force_karaoke: false,
            force_comnha: false,
            force_comtrua: false,
            force_breaks: false,
            force_teatime: false,
            force_duty: false,
            force_outing: None,
            force_nhau: false,
            force_payday: false,
            force_pickle: false,
            force_evswap: false,
        }
    }
}

/// All 17 cultural & leisure routines ported from `my-station/src/office/people/*.js`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum RoutineKind {
    // Office games (4)
    Billiards,
    Foosball,
    Gaming,
    Karaoke,
    // Meals & breaks (7)
    ComNha,
    ComTrua,
    Breaks,
    Teatime,
    Duty,
    Errands,
    Gatherings,
    // Street outings (6)
    Outings,
    Nhau,
    Payday,
    Photobooth,
    Pickleball,
    EvSwap,
}

/// Single-routine membership marker attached to a participating worker so no worker can join
/// two routines at once.
#[derive(Component, Debug, Clone, Copy)]
pub struct ActiveRoutine {
    pub kind: RoutineKind,
    pub slot: u8,
    pub stage: u8,
    pub dwell_secs: f32,
    pub home_pos: Vec3,
    pub home_yaw: f32,
    pub home_sit: bool,
    pub desired_prop: Option<PropKind>,
    pub returning: bool,
}

impl ActiveRoutine {
    pub fn new(
        kind: RoutineKind,
        slot: u8,
        dwell_secs: f32,
        home_pos: Vec3,
        home_yaw: f32,
        home_sit: bool,
        desired_prop: Option<PropKind>,
    ) -> Self {
        Self {
            kind,
            slot,
            stage: 0,
            dwell_secs,
            home_pos,
            home_yaw,
            home_sit,
            desired_prop,
            returning: false,
        }
    }
}

/// Zero-allocation per-frame claim tracker preventing two routines from claiming the same worker
/// within the same ECS frame before deferred `Commands` flush.
#[derive(Resource, Debug)]
pub struct RoutineCoordinator {
    claimed: [Option<Entity>; 64],
    count: usize,
}

impl Default for RoutineCoordinator {
    fn default() -> Self {
        Self {
            claimed: [None; 64],
            count: 0,
        }
    }
}

impl RoutineCoordinator {
    pub fn reset(&mut self) {
        self.count = 0;
    }

    pub fn is_claimed(&self, entity: Entity) -> bool {
        self.claimed[..self.count].contains(&Some(entity))
    }

    pub fn claim(&mut self, entity: Entity) -> bool {
        if self.is_claimed(entity) || self.count >= self.claimed.len() {
            return false;
        }
        self.claimed[self.count] = Some(entity);
        self.count += 1;
        true
    }

    pub fn claimed_count(&self) -> usize {
        self.count
    }
}

#[inline]
pub(crate) fn is_worker_resting(live: &LiveSessionWorker) -> bool {
    matches!(live.status.as_str(), "rest" | "idle" | "sleep")
}

#[inline]
pub(crate) fn is_worker_recalled(live: &LiveSessionWorker) -> bool {
    !is_worker_resting(live)
}

#[allow(clippy::too_many_arguments)]
pub(crate) fn sync_lazy_prop(
    commands: &mut Commands,
    chibi_cache: &ChibiMeshCache,
    palette: &mut WorldPalette,
    materials: &mut Assets<StandardMaterial>,
    ent: Entity,
    active_prop: Option<&ActivePropEntity>,
    desired: Option<PropKind>,
) {
    if active_prop.map(|p| p.kind) != desired {
        set_active_prop(commands, chibi_cache, palette, materials, ent, desired);
    }
}

/// Advances `CastClock::sim_secs` and seeds `RoutineCoordinator` with every currently active
/// routine participant before the three cultural routine systems run.
pub fn begin_cast_tick_system(
    time: Res<Time>,
    mut clock: Option<ResMut<CastClock>>,
    mut coord: Option<ResMut<RoutineCoordinator>>,
    active_q: Query<Entity, With<ActiveRoutine>>,
) {
    let dt = time.delta_secs().clamp(0.0, 0.1);
    if let Some(c) = clock.as_deref_mut() {
        c.sim_secs += dt;
    }
    if let Some(co) = coord.as_deref_mut() {
        co.reset();
        for ent in &active_q {
            co.claim(ent);
        }
    }
}
