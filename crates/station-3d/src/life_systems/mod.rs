//! Complete 15-module office `life/` subsystem ported 1:1 from `my-station/src/office/life/*.js`:
//! - `altar.rs`: *Bàn thờ Thần Tài – Thổ Địa* + Mùng 1 / Rằm offerings & animated incense smoke
//! - `drill_lift.rs`: Quarterly *Diễn tập PCCC* fire drill + post-update lift maintenance
//! - `seasons_weather.rs`: Halloween / Christmas / Tết / Normal decorations + Open-Meteo pooled rain + `"SÀN ƯỚT"`
//! - `workflow_life.rs`: Postbox + postman, Incidents red strobes, Quota break sign, Tests lamp board, Trophies cabinet, Photostrips
//! - `mod.rs`: Midday Nap (`nap.js`), Vietnamese Day Swaps (`day-swaps.js`), Your Turn (`your-turn.js`), unified spawn & reset

pub mod altar;
pub mod drill_lift;
pub mod seasons_weather;
pub mod workflow_life;

use crate::cast::CastClock;
use crate::layout::LayoutSpec;
use crate::life::OfficeLifeState;
use crate::palette::WorldPalette;
use crate::replay::ReplayState;
use crate::rooms::WindowBlind;
use bevy::prelude::*;
use chrono::{Datelike, NaiveDate, Weekday};
use station_canvas2d::FontEngine;

pub use altar::{AltarState, is_offering_day, spawn_than_tai_altar, update_altar_system, vietnamese_lunar_day};
pub use drill_lift::{
    DrillPhase, FireDrillState, LiftServicePhase, LiftServiceState, compute_evacuation_path,
    spawn_drill_and_lift_service, update_drill_and_lift_system,
};
pub use seasons_weather::{
    RAIN_POOL_CAP, WeatherVisualState, spawn_seasons_and_weather, update_seasons_and_weather_system,
};
pub use workflow_life::{
    IncidentState, MAX_PHOTOSTRIPS, MAX_TEST_ROWS, MAX_TROPHY_SLOTS, PhotostripSlot,
    PhotostripsState, PostboxState, PostmanStage, QuotaBoardState, TestRunStatus, TestsBoardState,
    TrophiesShelfState, TrophyEntry, spawn_workflow_life, sync_photostrips_from_shots_dir,
    update_workflow_life_system,
};

/// Vietnamese office calendar day classification (`src/shared/workdays.js` + `day-swaps.js`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WorkdayKind {
    Work,
    Weekend,
    Holiday,
    SwapOff,
    MakeupSaturday,
}

const SWAPS_OFF: &[(i32, u32, u32)] = &[(2026, 1, 2), (2026, 8, 31)];
const SWAPS_WORK: &[(i32, u32, u32)] = &[(2026, 1, 10), (2026, 8, 22)];

fn is_vn_fixed_holiday(year: i32, month: u32, day: u32) -> bool {
    matches!(
        (month, day),
        (1, 1) | (4, 30) | (5, 1) | (9, 1) | (9, 2)
    ) || (year >= 2026 && month == 11 && day == 24)
        || (year == 2026 && month == 2 && (16..=20).contains(&day))
        || (year == 2027 && month == 2 && (4..=8).contains(&day))
}

/// Classifies a calendar date per Vietnamese Labor Code & Government swap notices (`workdays.js:78`).
pub fn day_kind(year: i32, month: u32, day: u32) -> WorkdayKind {
    if SWAPS_OFF.contains(&(year, month, day)) {
        return WorkdayKind::SwapOff;
    }
    if SWAPS_WORK.contains(&(year, month, day)) {
        return WorkdayKind::MakeupSaturday;
    }
    if is_vn_fixed_holiday(year, month, day) {
        return WorkdayKind::Holiday;
    }
    let Some(date) = NaiveDate::from_ymd_opt(year, month, day) else {
        return WorkdayKind::Work;
    };
    if matches!(date.weekday(), Weekday::Sat | Weekday::Sun) {
        WorkdayKind::Weekend
    } else {
        WorkdayKind::Work
    }
}

/// True on working days (Mon–Fri excluding holidays/swaps, plus Government make-up Saturdays).
pub fn is_workday(year: i32, month: u32, day: u32) -> bool {
    matches!(
        day_kind(year, month, day),
        WorkdayKind::Work | WorkdayKind::MakeupSaturday
    )
}

/// True when `(year, month, day)` is the first working day of its quarter (quarterly PCCC drill day).
pub fn is_quarter_first_workday(year: i32, month: u32, day: u32) -> bool {
    if !is_workday(year, month, day) {
        return false;
    }
    let q_month = ((month.saturating_sub(1)) / 3) * 3 + 1;
    let Some(mut cur) = NaiveDate::from_ymd_opt(year, q_month, 1) else {
        return false;
    };
    for _ in 0..31 {
        if is_workday(cur.year(), cur.month(), cur.day()) {
            return cur.year() == year && cur.month() == month && cur.day() == day;
        }
        let Some(next) = cur.succ_opt() else {
            break;
        };
        cur = next;
    }
    false
}

/// Midday Vietnamese office nap state (`nap.js`, 12:30–13:30: lowers roller blinds smoothly).
#[derive(Resource, Debug, Clone, Default)]
pub struct NapState {
    pub active: bool,
    pub blind_level: f32,
}

/// Tracks sessions that finished while the user was unfocused (`your-turn.js`).
#[derive(Resource, Debug, Clone, Default)]
pub struct YourTurnState {
    pub unseen_done: Vec<String>,
}

impl YourTurnState {
    pub fn mark_done(&mut self, session_id: &str, focused: bool) {
        if focused || session_id.is_empty() {
            return;
        }
        if !self.unseen_done.iter().any(|s| s == session_id) {
            self.unseen_done.push(session_id.to_owned());
        }
    }

    pub fn clear_seen(&mut self, session_id: &str) {
        self.unseen_done.retain(|s| s != session_id);
    }

    pub fn clear_all(&mut self) {
        self.unseen_done.clear();
    }

    /// Selects the next session needing attention: waiting sessions take priority over unseen done.
    pub fn next_needing_you(&mut self, waiting_ids: &[&str]) -> Option<String> {
        if let Some(&w) = waiting_ids.first() {
            self.clear_seen(w);
            return Some(w.to_owned());
        }
        if self.unseen_done.is_empty() {
            None
        } else {
            Some(self.unseen_done.remove(0))
        }
    }
}

/// Lowers `WindowBlind`s smoothly during 12:30–13:30 midday nap (`nap.js:36-46`).
pub fn update_nap_system(
    time: Res<Time>,
    clock: Option<Res<CastClock>>,
    replay: Option<Res<ReplayState>>,
    mut nap: Option<ResMut<NapState>>,
    mut life: Option<ResMut<OfficeLifeState>>,
    mut q_blinds: Query<&mut WindowBlind>,
    mut q_cloth: Query<(&mut Transform, &mut Visibility), Without<WindowBlind>>,
) {
    let Some(nap) = nap.as_deref_mut() else {
        return;
    };
    let dt = time.delta_secs().clamp(0.0, 0.25);
    let in_replay = replay.as_deref().is_some_and(|r| r.active);
    let mins = clock.as_deref().map(|c| c.minutes).unwrap_or(9 * 60 + 30);
    let on = !in_replay && (750..810).contains(&mins); // 12:30..13:30
    nap.active = on;
    if let Some(l) = life.as_deref_mut() {
        l.nap_active = on;
    }

    let goal = if on { 1.0_f32 } else { 0.0_f32 };
    let changed = (nap.blind_level - goal).abs() > 0.001;
    if changed {
        let dir = (goal - nap.blind_level).signum();
        nap.blind_level = (nap.blind_level + dir * dt / 3.0).clamp(0.0, 1.0);
        if (nap.blind_level - goal).abs() <= 0.001 {
            nap.blind_level = goal;
        }
    }

    for mut b in &mut q_blinds {
        if changed || (b.lowered - nap.blind_level).abs() > 1e-4 {
            b.lowered = nap.blind_level;
            if let Ok((mut tf, mut vis)) = q_cloth.get_mut(b.cloth_entity) {
                let h = (b.full_height * b.lowered).max(0.001);
                tf.scale.y = h;
                tf.translation.y = b.top_y - h * 0.5;
                let want_vis = if b.lowered <= 0.001 {
                    Visibility::Hidden
                } else {
                    Visibility::Visible
                };
                if *vis != want_vis {
                    *vis = want_vis;
                }
            }
        }
    }
}

/// Spawns all 15 `life/` world elements (every entity carries `WorldEntity`, every asset tracked in `WorldPalette`).
pub fn spawn_all_life_systems(
    commands: &mut Commands,
    pal: &mut WorldPalette,
    materials: &mut Assets<StandardMaterial>,
    images: &mut Assets<Image>,
    fonts: &mut FontEngine,
    spec: &LayoutSpec,
    life: &OfficeLifeState,
) {
    spawn_than_tai_altar(commands, pal, materials, images, fonts, spec);
    spawn_drill_and_lift_service(commands, pal, materials, images, fonts, spec);
    spawn_seasons_and_weather(commands, pal, materials, images, fonts, spec, life);
    spawn_workflow_life(commands, pal, materials, images, fonts, spec);
}

/// Inserts all `life_systems` runtime resources into an `App`.
pub fn register_life_resources(app: &mut App) {
    app.insert_resource(AltarState::default())
        .insert_resource(FireDrillState::default())
        .insert_resource(LiftServiceState::default())
        .insert_resource(WeatherVisualState::default())
        .insert_resource(PostboxState::default())
        .insert_resource(IncidentState::default())
        .insert_resource(QuotaBoardState::default())
        .insert_resource(TestsBoardState::default())
        .insert_resource(TrophiesShelfState::default())
        .insert_resource(PhotostripsState::default())
        .insert_resource(NapState::default())
        .insert_resource(YourTurnState::default());
}

/// Resets all `life_systems` resources cleanly during `unload_full_office_world`.
pub fn reset_life_resources(commands: &mut Commands) {
    commands.insert_resource(AltarState::default());
    commands.insert_resource(FireDrillState::default());
    commands.insert_resource(LiftServiceState::default());
    commands.insert_resource(WeatherVisualState::default());
    commands.insert_resource(PostboxState::default());
    commands.insert_resource(IncidentState::default());
    commands.insert_resource(QuotaBoardState::default());
    commands.insert_resource(TestsBoardState::default());
    commands.insert_resource(TrophiesShelfState::default());
    commands.insert_resource(PhotostripsState::default());
    commands.insert_resource(NapState::default());
    commands.insert_resource(YourTurnState::default());
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn day_swaps_nap_and_your_turn_behave_authentically() {
        // 2026-01-02 is swapped off (nghỉ bù), 2026-01-10 is make-up Saturday (làm bù)
        assert_eq!(day_kind(2026, 1, 2), WorkdayKind::SwapOff);
        assert!(!is_workday(2026, 1, 2));
        assert_eq!(day_kind(2026, 1, 10), WorkdayKind::MakeupSaturday);
        assert!(is_workday(2026, 1, 10));
        // Q1 2026: Jan 1 holiday, Jan 2 swap off, Jan 3-4 weekend -> first workday is Monday Jan 5
        assert!(is_quarter_first_workday(2026, 1, 5));
        assert!(!is_quarter_first_workday(2026, 1, 1));

        // YourTurn prioritizes waiting over unseen done
        let mut yt = YourTurnState::default();
        yt.mark_done("sess-done", false);
        assert_eq!(yt.next_needing_you(&["sess-wait"]).as_deref(), Some("sess-wait"));
        assert_eq!(yt.next_needing_you(&[]).as_deref(), Some("sess-done"));
        assert_eq!(yt.next_needing_you(&[]), None);

        // Midday nap 12:30-13:30 lowers window blinds
        let mut app = App::new();
        app.insert_resource(Time::<()>::default())
            .insert_resource(CastClock {
                minutes: 12 * 60 + 45,
                ..default()
            })
            .insert_resource(ReplayState::default())
            .insert_resource(NapState::default())
            .insert_resource(OfficeLifeState::default())
            .add_systems(Update, update_nap_system);
        let cloth = app
            .world_mut()
            .spawn((
                Transform::from_xyz(0.0, 2.5, 0.0).with_scale(Vec3::new(1.2, 0.001, 1.0)),
                Visibility::Hidden,
            ))
            .id();
        app.world_mut().spawn(WindowBlind {
            cloth_entity: cloth,
            top_y: 2.5,
            full_height: 1.8,
            lowered: 0.0,
        });
        for _ in 0..15 {
            app.world_mut()
                .resource_mut::<Time<()>>()
                .advance_by(std::time::Duration::from_millis(250));
            app.update();
        }
        assert!(app.world().resource::<NapState>().active);
        assert!(app.world().resource::<OfficeLifeState>().nap_active);
        let blind = app.world_mut().query::<&WindowBlind>().single(app.world()).unwrap();
        assert!(blind.lowered > 0.8);
        let cloth_tf = *app.world().get::<Transform>(cloth).unwrap();
        let cloth_vis = *app.world().get::<Visibility>(cloth).unwrap();
        assert_eq!(cloth_vis, Visibility::Visible, "cloth must become visible when lowered");
        let expected_h = (1.8 * blind.lowered).max(0.001);
        assert!(
            (cloth_tf.scale.y - expected_h).abs() < 1e-3,
            "cloth scale.y ({}) must track full_height * lowered ({expected_h})",
            cloth_tf.scale.y
        );
        assert!(
            (cloth_tf.translation.y - (2.5 - expected_h * 0.5)).abs() < 1e-3,
            "cloth translation.y ({}) must hang from top_y",
            cloth_tf.translation.y
        );

        // After nap ends -> blinds roll back up and hide
        app.world_mut().resource_mut::<CastClock>().minutes = 14 * 60;
        for _ in 0..20 {
            app.world_mut()
                .resource_mut::<Time<()>>()
                .advance_by(std::time::Duration::from_millis(250));
            app.update();
        }
        assert!(!app.world().resource::<NapState>().active);
        let cloth_vis_after = *app.world().get::<Visibility>(cloth).unwrap();
        assert_eq!(cloth_vis_after, Visibility::Hidden, "cloth must hide when fully raised");
    }
}
