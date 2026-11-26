//! Meals, breaks, chores, errands & gatherings ported from Golden Oracle `src/office/people/`:
//! - `comnha.js`: Noon `12:00` pantry microwave lunch queue (`HomeBox` / `Noodle`)
//! - `comtrua.js`: Noon `12:00` street *cơm bình dân* & Friday *bún đậu mắm tôm*
//! - `breaks.js`: Afternoon `15:00` coffee machine queue (`Coffee` / `Cup`)
//! - `teatime.js`: Friday `16:00` bubble tea & *bánh tráng trộn* (`Boba`)
//! - `duty.js`: Daily *trực nhật* chores (cups, trash, plants, tables) synced to duty board
//! - `errands.js`: Reactive prop errands (printer, shredder, water cooler, thirsty plant)
//! - `gatherings.js`: Bell stand-up & celebration gatherings at the meeting table

use crate::cast::{
    ActiveRoutine, CastClock, CastEvents, RoutineCoordinator, RoutineKind, is_worker_recalled,
    is_worker_resting, sync_lazy_prop,
};
use crate::chibi::{ChibiWorker, WorkerAnim};
use crate::chibi_meshes::ChibiMeshCache;
use crate::chibi_props::{ActivePropEntity, PropKind};
use crate::interact::PropActivated;
use crate::nav::{NavGraph, StationKind};
use crate::palette::WorldPalette;
use crate::people::{LiveSessionWorker, NavWalker, SubagentWorker};
use crate::props_detail::microwave::{MicrowaveOven, MicrowaveStage};
use crate::props_detail::shredder_cups::PaperShredder;
use bevy::prelude::*;
use std::f32::consts::{FRAC_PI_2, PI};

pub const COM_NHA_WAIT_SPOTS: [[f32; 2]; 5] = [
    [9.55, -5.42],
    [9.55, -5.98],
    [9.75, -6.45],
    [10.35, -6.48],
    [10.95, -6.48],
];

const COFFEE_QUEUE_SPOTS: [[f32; 2]; 5] = [
    [11.45, -6.72],
    [11.55, -6.00],
    [11.55, -5.30],
    [11.55, -4.60],
    [11.55, -3.90],
];

pub const COM_TRUA_WEEKDAY_DIALOGUE: &str = "Đi ăn cơm bình dân không cả nhà?";
pub const COM_TRUA_FRIDAY_DIALOGUE: &str = "Thứ Sáu bún đậu mắm tôm thôi!";
pub const BREAK_COFFEE_DIALOGUE: &str = "Chiều làm ly cà phê sữa đá cho tỉnh nào!";
pub const TEATIME_DIALOGUE: &str = "Trà sữa bánh tráng trộn về rồi cả phòng ơi!";

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum DutyChore {
    Plants = 0,
    Cups = 1,
    Trash = 2,
    Tables = 3,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ErrandKind {
    Printer,
    Shredder,
    WaterCooler,
    PlantWater,
}

#[derive(Resource, Debug, Default, Clone)]
pub struct MealsBreaksState {
    pub comnha_day: Option<u32>,
    pub comnha_served: u32,
    pub comnha_active_count: usize,
    pub comtrua_day: Option<u32>,
    pub comtrua_active: bool,
    pub comtrua_is_friday_bun_dau: bool,
    pub comtrua_dialogue: &'static str,
    pub breaks_day: Option<u32>,
    pub breaks_served: u32,
    pub teatime_day: Option<u32>,
    pub teatime_active: bool,
    pub teatime_participants: usize,
    pub duty_day: Option<u32>,
    pub duty_completed: [bool; 4],
    pub duty_active_chore: Option<DutyChore>,
    pub duty_cups_pile: u8,
    pub errands_completed: u32,
    pub last_errand: Option<ErrandKind>,
    pub gathering_active: bool,
    pub monday_standup_day: Option<u32>,
    pub gatherings_held: u32,
}

fn send_home(
    nav: &NavGraph,
    chibi: &ChibiWorker,
    walker: &mut NavWalker,
    vis: &mut Visibility,
    routine: &mut ActiveRoutine,
    recalled: bool,
) {
    routine.returning = true;
    routine.desired_prop = None;
    *vis = Visibility::Visible;
    let anim = if recalled {
        WorkerAnim::Work
    } else {
        WorkerAnim::Sleep
    };
    walker.set_destination(
        nav,
        chibi.base_pos,
        routine.home_pos,
        routine.home_yaw,
        routine.home_sit,
        anim,
    );
}

fn comnha_oven_spot(nav: &NavGraph, slot: usize) -> (Vec3, f32) {
    if nav.layout().is_pro() {
        let z = if slot % 2 == 0 { -4.63 } else { -3.99 };
        (Vec3::new(9.35, 0.0, z), -FRAC_PI_2)
    } else {
        let sp = nav.station_spot_for(StationKind::Coffee, slot);
        (sp.pos, sp.face)
    }
}

fn duty_chore_target(nav: &NavGraph, chore: DutyChore) -> (Vec3, f32, PropKind) {
    let pro = nav.layout().is_pro();
    match chore {
        DutyChore::Plants => {
            let sp = nav.station_spot_for(StationKind::Library, 0);
            (sp.pos, sp.face, PropKind::Cup)
        }
        DutyChore::Cups => {
            if pro {
                (Vec3::new(12.25, 0.0, -6.75), PI, PropKind::Mug)
            } else {
                let sp = nav.station_spot_for(StationKind::Coffee, 0);
                (sp.pos, sp.face, PropKind::Mug)
            }
        }
        DutyChore::Trash => {
            if pro {
                (Vec3::new(9.55, 0.0, -5.0), -FRAC_PI_2, PropKind::CarrierBag)
            } else {
                let sp = nav.door_in();
                (sp.pos, sp.face, PropKind::CarrierBag)
            }
        }
        DutyChore::Tables => {
            let kind = if pro {
                StationKind::Dining
            } else {
                StationKind::Meeting
            };
            let sp = nav.station_spot_for(kind, 0);
            (sp.pos, sp.face, PropKind::Paper)
        }
    }
}

fn errand_target(nav: &NavGraph, errand: ErrandKind) -> (Vec3, f32, PropKind) {
    let pro = nav.layout().is_pro();
    match errand {
        ErrandKind::Printer => {
            if pro {
                (Vec3::new(-2.4, 0.0, 0.55), -FRAC_PI_2, PropKind::Paper)
            } else {
                (Vec3::new(-3.15, 0.0, 4.4), 0.0, PropKind::Paper)
            }
        }
        ErrandKind::Shredder => {
            if pro {
                (Vec3::new(-2.4, 0.0, 1.45), -FRAC_PI_2, PropKind::Paper)
            } else {
                (Vec3::new(-2.3, 0.0, 4.4), 0.0, PropKind::Paper)
            }
        }
        ErrandKind::WaterCooler => {
            if pro {
                (Vec3::new(9.3, 0.0, -6.15), -FRAC_PI_2, PropKind::Cup)
            } else {
                let sp = nav.station_spot_for(StationKind::Coffee, 1);
                (sp.pos, sp.face, PropKind::Cup)
            }
        }
        ErrandKind::PlantWater => {
            if pro {
                (Vec3::new(-3.95, 0.0, 4.4), 0.0, PropKind::Cup)
            } else {
                (Vec3::new(-5.2, 0.0, -3.3), PI, PropKind::Cup)
            }
        }
    }
}

/// Steps `ComNha`, `ComTrua`, `Breaks`, `Teatime`, `Duty`, `Errands`, and `Gatherings`
/// over `NavGraph` with lazy `set_active_prop` and zero per-frame heap allocations.
#[allow(clippy::too_many_arguments, clippy::type_complexity)]
pub fn step_meals_breaks_system(
    time: Res<Time>,
    mut commands: Commands,
    nav: Option<Res<NavGraph>>,
    clock: Option<Res<CastClock>>,
    mut events: Option<ResMut<CastEvents>>,
    mut coord: Option<ResMut<RoutineCoordinator>>,
    mut state: Option<ResMut<MealsBreaksState>>,
    chibi_cache: Option<Res<ChibiMeshCache>>,
    mut palette: Option<ResMut<WorldPalette>>,
    mut materials: Option<ResMut<Assets<StandardMaterial>>>,
    mut prop_ev: EventReader<PropActivated>,
    mut workers_q: Query<
        (
            Entity,
            &LiveSessionWorker,
            &mut ChibiWorker,
            &mut NavWalker,
            &mut Visibility,
            Option<&mut ActiveRoutine>,
            Option<&ActivePropEntity>,
        ),
        (Without<crate::cast::OfficeStaffNpc>, Without<SubagentWorker>),
    >,
    mut ovens_q: Query<&mut MicrowaveOven>,
    mut shredders_q: Query<&mut PaperShredder>,
) {
    let (
        Some(nav),
        Some(clock),
        Some(events),
        Some(coord),
        Some(state),
        Some(cache),
        Some(pal),
        Some(mats),
    ) = (
        nav.as_deref(),
        clock.as_deref(),
        events.as_deref_mut(),
        coord.as_deref_mut(),
        state.as_deref_mut(),
        chibi_cache.as_deref(),
        palette.as_deref_mut(),
        materials.as_deref_mut(),
    )
    else {
        return;
    };

    let dt = time.delta_secs().clamp(0.0, 0.1);
    let m = clock.minutes;
    let pro = nav.layout().is_pro();

    // Roll daily duty board on new day
    if state.duty_day != Some(clock.day_index) {
        state.duty_day = Some(clock.day_index);
        state.duty_completed = [false; 4];
        state.duty_cups_pile = 4;
    }

    // Read interactive prop activations (`bell`, `printer`, `shredder`)
    for ev in prop_ev.read() {
        match ev.id {
            "bell" => events.bell_rung = true,
            "printer" => events.printer_job = true,
            "shredder" => events.shredder_job = true,
            _ => {}
        }
    }

    // Pass 1: Step active meal/break/chore/errand/gathering participants.
    let mut comnha_active = 0usize;
    let mut comtrua_active = 0usize;
    let mut teatime_active = 0usize;
    let mut gathering_active = 0usize;
    let mut duty_active = false;
    let mut trigger_oven_slot: Option<usize> = None;
    let mut trigger_shredder = false;

    for (ent, live, chibi, mut walker, mut vis, routine_opt, active_prop) in &mut workers_q {
        let Some(mut routine) = routine_opt else {
            continue;
        };
        if !matches!(
            routine.kind,
            RoutineKind::ComNha
                | RoutineKind::ComTrua
                | RoutineKind::Breaks
                | RoutineKind::Teatime
                | RoutineKind::Duty
                | RoutineKind::Errands
                | RoutineKind::Gatherings
        ) {
            continue;
        }

        if routine.returning {
            sync_lazy_prop(&mut commands, cache, pal, mats, ent, active_prop, None);
            if !walker.is_walking() {
                *vis = Visibility::Visible;
                commands.entity(ent).remove::<ActiveRoutine>();
            }
            continue;
        }

        // Recall back to desk immediately if work arrives
        if is_worker_recalled(live) {
            send_home(nav, &chibi, &mut walker, &mut vis, &mut routine, true);
            sync_lazy_prop(&mut commands, cache, pal, mats, ent, active_prop, None);
            continue;
        }

        // Dwell ONLY counts down after arrival (!walker.is_walking())
        if !walker.is_walking() && routine.dwell_secs > 0.0 {
            routine.dwell_secs = (routine.dwell_secs - dt).max(0.0);
        }

        match routine.kind {
            RoutineKind::ComNha => {
                comnha_active += 1;
                if !walker.is_walking() && routine.dwell_secs <= 0.0 {
                    if routine.stage == 0 {
                        // Stage 0 -> 1: step up to microwave & start heating
                        let (oven_pos, oven_face) = comnha_oven_spot(nav, routine.slot as usize);
                        walker.set_destination(
                            nav,
                            chibi.base_pos,
                            oven_pos,
                            oven_face,
                            false,
                            WorkerAnim::Wait,
                        );
                        routine.stage = 1;
                        routine.dwell_secs = 2.0;
                        routine.desired_prop = Some(PropKind::HomeBox);
                        trigger_oven_slot = Some((routine.slot as usize) % 2);
                    } else if routine.stage == 1 {
                        // Stage 1 -> 2: take hot HomeBox / Noodle to dining seat to eat
                        let eat_kind = if pro {
                            StationKind::Dining
                        } else {
                            StationKind::Meeting
                        };
                        let seat = nav.station_spot_for(eat_kind, routine.slot as usize);
                        let food_prop = if routine.slot % 2 == 0 {
                            PropKind::HomeBox
                        } else {
                            PropKind::Noodle
                        };
                        walker.set_destination(
                            nav,
                            chibi.base_pos,
                            seat.pos,
                            seat.face,
                            true,
                            WorkerAnim::Rest,
                        );
                        routine.stage = 2;
                        routine.dwell_secs = 6.0;
                        routine.desired_prop = Some(food_prop);
                    } else {
                        // Done eating -> return to desk
                        state.comnha_served += 1;
                        send_home(nav, &chibi, &mut walker, &mut vis, &mut routine, false);
                    }
                }
            }
            RoutineKind::ComTrua => {
                comtrua_active += 1;
                if !walker.is_walking() && routine.dwell_secs <= 0.0 {
                    if routine.stage == 0 {
                        // Reached lift -> down to street quán cơm / bún đậu mắm tôm
                        *vis = Visibility::Hidden;
                        routine.stage = 1;
                        routine.dwell_secs = 8.0;
                    } else {
                        // Paid "Cô ơi, tính tiền!" -> back up lift to desk
                        send_home(nav, &chibi, &mut walker, &mut vis, &mut routine, false);
                    }
                }
            }
            RoutineKind::Breaks => {
                if !walker.is_walking() && routine.dwell_secs <= 0.0 {
                    if routine.stage == 0 {
                        // Brewed coffee -> carry Cup/Coffee to dining/desk to sip
                        let seat = if pro {
                            nav.station_spot_for(StationKind::Dining, routine.slot as usize + 4)
                        } else {
                            nav.station_spot_for(StationKind::Meeting, routine.slot as usize)
                        };
                        let prop = if routine.slot % 2 == 0 {
                            PropKind::Coffee
                        } else {
                            PropKind::Cup
                        };
                        walker.set_destination(
                            nav,
                            chibi.base_pos,
                            seat.pos,
                            seat.face,
                            true,
                            WorkerAnim::Rest,
                        );
                        routine.stage = 1;
                        routine.dwell_secs = 5.0;
                        routine.desired_prop = Some(prop);
                    } else {
                        state.breaks_served += 1;
                        send_home(nav, &chibi, &mut walker, &mut vis, &mut routine, false);
                    }
                }
            }
            RoutineKind::Teatime => {
                teatime_active += 1;
                if !walker.is_walking() && routine.dwell_secs <= 0.0 {
                    send_home(nav, &chibi, &mut walker, &mut vis, &mut routine, false);
                }
            }
            RoutineKind::Duty => {
                duty_active = true;
                if !walker.is_walking() && routine.dwell_secs <= 0.0 {
                    let idx = (routine.slot as usize).min(3);
                    state.duty_completed[idx] = true;
                    if idx == DutyChore::Cups as usize {
                        state.duty_cups_pile = 0;
                    }
                    state.duty_active_chore = None;
                    send_home(nav, &chibi, &mut walker, &mut vis, &mut routine, false);
                }
            }
            RoutineKind::Errands => {
                if !walker.is_walking() && routine.dwell_secs <= 0.0 {
                    if routine.slot == ErrandKind::Shredder as u8 {
                        trigger_shredder = true;
                    }
                    state.errands_completed += 1;
                    send_home(nav, &chibi, &mut walker, &mut vis, &mut routine, false);
                }
            }
            RoutineKind::Gatherings => {
                gathering_active += 1;
                if !walker.is_walking() && routine.dwell_secs <= 0.0 {
                    send_home(nav, &chibi, &mut walker, &mut vis, &mut routine, false);
                }
            }
            _ => {}
        }

        sync_lazy_prop(
            &mut commands,
            cache,
            pal,
            mats,
            ent,
            active_prop,
            routine.desired_prop,
        );
    }

    if let Some(oven_idx) = trigger_oven_slot {
        for mut oven in &mut ovens_q {
            if oven.index == oven_idx {
                oven.stage = MicrowaveStage::Run;
                oven.time_left = 45.0;
                oven.open_goal = 0.0;
                break;
            }
        }
    }
    if trigger_shredder {
        for mut sh in &mut shredders_q {
            if sh.run_left <= 0.0 {
                sh.run_left = sh.run_total;
            }
        }
    }

    state.comnha_active_count = comnha_active;
    state.comtrua_active = comtrua_active > 0;
    state.teatime_active = teatime_active > 0;
    state.teatime_participants = teatime_active;
    state.gathering_active = gathering_active > 0;
    if !duty_active {
        state.duty_active_chore = None;
    }

    // Pass 2: Collect free resting workers into a fixed stack buffer (zero heap allocations).
    let mut free_buf: [Option<(Entity, Vec3, f32, bool)>; 16] = [None; 16];
    let mut free_len = 0usize;
    for (ent, live, chibi, walker, _, routine_opt, _) in &workers_q {
        if free_len >= free_buf.len() {
            break;
        }
        if routine_opt.is_none()
            && is_worker_resting(live)
            && !walker.is_walking()
            && !coord.is_claimed(ent)
        {
            let (hp, hy, hs) = nav
                .desk_spot(live.desk_index)
                .map(|d| (d.pos, d.face, d.sit))
                .unwrap_or((chibi.base_pos, chibi.base_yaw, chibi.sitting));
            free_buf[free_len] = Some((ent, hp, hy, hs));
            free_len += 1;
        }
    }
    let mut next_free = 0usize;

    // 1. Gatherings: Bell ring OR Monday 09:00 stand-up
    let monday_standup = clock.is_monday()
        && clock.is_workday
        && (540..=550).contains(&m)
        && state.monday_standup_day != Some(clock.day_index);
    let want_gathering = events.bell_rung || monday_standup;
    if !state.gathering_active && want_gathering && free_len.saturating_sub(next_free) >= 1 {
        events.bell_rung = false;
        if monday_standup {
            state.monday_standup_day = Some(clock.day_index);
        }
        let count = free_len.saturating_sub(next_free).min(10);
        for slot in 0..count {
            let Some((ent, home_pos, home_yaw, home_sit)) = free_buf[next_free] else {
                break;
            };
            next_free += 1;
            if !coord.claim(ent) {
                continue;
            }
            let sp = nav.station_spot_for(StationKind::Meeting, slot);
            if let Ok((_, _, chibi, mut walker, _, _, _)) = workers_q.get_mut(ent) {
                walker.set_destination(nav, chibi.base_pos, sp.pos, sp.face, sp.sit, WorkerAnim::Wait);
                commands.entity(ent).insert(ActiveRoutine::new(
                    RoutineKind::Gatherings,
                    slot as u8,
                    8.0,
                    home_pos,
                    home_yaw,
                    home_sit,
                    None,
                ));
            }
        }
        state.gathering_active = true;
        state.gatherings_held += 1;
    }

    // 2. ComNha (Noon 11:45..13:00 microwave lunch queue)
    let comnha_window = clock.is_workday
        && (705..=780).contains(&m)
        && state.comnha_day != Some(clock.day_index);
    let want_comnha = events.force_comnha || comnha_window;
    if comnha_active == 0 && want_comnha && free_len.saturating_sub(next_free) >= 1 {
        events.force_comnha = false;
        state.comnha_day = Some(clock.day_index);
        let avail = free_len.saturating_sub(next_free);
        // Leave half of resting workers for street ComTrua if noon street lunch is also planned today
        let comtrua_planned =
            clock.is_workday && (clock.is_friday() || clock.day_index % 3 == 0);
        let count = if comtrua_planned && avail >= 2 {
            (avail / 2).min(3)
        } else {
            avail.min(3)
        };
        for slot in 0..count {
            let Some((ent, home_pos, home_yaw, home_sit)) = free_buf[next_free] else {
                break;
            };
            next_free += 1;
            if !coord.claim(ent) {
                continue;
            }
            let (wait_pos, wait_face) = if pro {
                let [wx, wz] = COM_NHA_WAIT_SPOTS[slot.min(COM_NHA_WAIT_SPOTS.len() - 1)];
                (Vec3::new(wx, 0.0, wz), 0.0)
            } else {
                let sp = nav.station_spot_for(StationKind::Coffee, slot);
                (sp.pos, sp.face)
            };
            if let Ok((_, _, chibi, mut walker, _, _, active_prop)) = workers_q.get_mut(ent) {
                walker.set_destination(
                    nav,
                    chibi.base_pos,
                    wait_pos,
                    wait_face,
                    false,
                    WorkerAnim::Wait,
                );
                commands.entity(ent).insert(ActiveRoutine::new(
                    RoutineKind::ComNha,
                    slot as u8,
                    1.0 + slot as f32 * 0.6,
                    home_pos,
                    home_yaw,
                    home_sit,
                    Some(PropKind::LunchBag),
                ));
                sync_lazy_prop(
                    &mut commands,
                    cache,
                    pal,
                    mats,
                    ent,
                    active_prop,
                    Some(PropKind::LunchBag),
                );
            }
        }
        state.comnha_active_count = count;
    }

    // 3. ComTrua (Noon 11:20..12:30 street cơm bình dân / Friday bún đậu mắm tôm)
    let comtrua_planned = clock.is_workday && (clock.is_friday() || clock.day_index % 3 == 0);
    let comtrua_window = events.city_street_available
        && comtrua_planned
        && (680..=750).contains(&m)
        && state.comtrua_day != Some(clock.day_index);
    let want_comtrua = events.force_comtrua || comtrua_window;
    if !state.comtrua_active && want_comtrua && free_len.saturating_sub(next_free) >= 1 {
        events.force_comtrua = false;
        state.comtrua_day = Some(clock.day_index);
        state.comtrua_is_friday_bun_dau = clock.is_friday();
        state.comtrua_dialogue = if clock.is_friday() {
            COM_TRUA_FRIDAY_DIALOGUE
        } else {
            COM_TRUA_WEEKDAY_DIALOGUE
        };
        let count = free_len.saturating_sub(next_free).min(4);
        let door = nav.door_in();
        for slot in 0..count {
            let Some((ent, home_pos, home_yaw, home_sit)) = free_buf[next_free] else {
                break;
            };
            next_free += 1;
            if !coord.claim(ent) {
                continue;
            }
            if let Ok((_, _, chibi, mut walker, _, _, _)) = workers_q.get_mut(ent) {
                walker.set_destination(
                    nav,
                    chibi.base_pos,
                    door.pos,
                    door.face,
                    false,
                    WorkerAnim::Rest,
                );
                commands.entity(ent).insert(ActiveRoutine::new(
                    RoutineKind::ComTrua,
                    slot as u8,
                    0.5,
                    home_pos,
                    home_yaw,
                    home_sit,
                    None,
                ));
            }
        }
        state.comtrua_active = true;
    }

    // 4. Breaks (Afternoon 15:00..15:45 coffee machine queue)
    let breaks_window = clock.is_workday
        && (900..=945).contains(&m)
        && state.breaks_day != Some(clock.day_index);
    let want_breaks = events.force_breaks || breaks_window;
    if want_breaks && free_len.saturating_sub(next_free) >= 1 {
        events.force_breaks = false;
        state.breaks_day = Some(clock.day_index);
        let count = free_len.saturating_sub(next_free).min(3);
        for slot in 0..count {
            let Some((ent, home_pos, home_yaw, home_sit)) = free_buf[next_free] else {
                break;
            };
            next_free += 1;
            if !coord.claim(ent) {
                continue;
            }
            let (q_pos, q_face) = if pro {
                let [qx, qz] = COFFEE_QUEUE_SPOTS[slot.min(COFFEE_QUEUE_SPOTS.len() - 1)];
                (Vec3::new(qx, 0.0, qz), PI)
            } else {
                let sp = nav.station_spot_for(StationKind::Coffee, slot);
                (sp.pos, sp.face)
            };
            let cup_prop = if slot % 2 == 0 {
                PropKind::Coffee
            } else {
                PropKind::Cup
            };
            if let Ok((_, _, chibi, mut walker, _, _, active_prop)) = workers_q.get_mut(ent) {
                walker.set_destination(nav, chibi.base_pos, q_pos, q_face, false, WorkerAnim::Wait);
                commands.entity(ent).insert(ActiveRoutine::new(
                    RoutineKind::Breaks,
                    slot as u8,
                    2.0,
                    home_pos,
                    home_yaw,
                    home_sit,
                    Some(cup_prop),
                ));
                sync_lazy_prop(
                    &mut commands,
                    cache,
                    pal,
                    mats,
                    ent,
                    active_prop,
                    Some(cup_prop),
                );
            }
        }
    }

    // 5. Teatime (Friday 16:00..16:20 bubble tea & bánh tráng trộn holding PropKind::Boba)
    let teatime_window = clock.is_workday
        && clock.is_friday()
        && (960..=980).contains(&m)
        && state.teatime_day != Some(clock.day_index);
    let want_teatime = events.force_teatime || teatime_window;
    if !state.teatime_active && want_teatime && free_len.saturating_sub(next_free) >= 1 {
        events.force_teatime = false;
        state.teatime_day = Some(clock.day_index);
        let seat_kind = if pro {
            StationKind::Dining
        } else {
            StationKind::Meeting
        };
        let count = free_len.saturating_sub(next_free).min(6);
        for slot in 0..count {
            let Some((ent, home_pos, home_yaw, home_sit)) = free_buf[next_free] else {
                break;
            };
            next_free += 1;
            if !coord.claim(ent) {
                continue;
            }
            let seat = nav.station_spot_for(seat_kind, slot);
            if let Ok((_, _, chibi, mut walker, _, _, active_prop)) = workers_q.get_mut(ent) {
                walker.set_destination(
                    nav,
                    chibi.base_pos,
                    seat.pos,
                    seat.face,
                    true,
                    WorkerAnim::Rest,
                );
                commands.entity(ent).insert(ActiveRoutine::new(
                    RoutineKind::Teatime,
                    slot as u8,
                    12.0,
                    home_pos,
                    home_yaw,
                    home_sit,
                    Some(PropKind::Boba),
                ));
                sync_lazy_prop(
                    &mut commands,
                    cache,
                    pal,
                    mats,
                    ent,
                    active_prop,
                    Some(PropKind::Boba),
                );
            }
        }
        state.teatime_active = true;
        state.teatime_participants = count;
    }

    // 6. Duty (*trực nhật* daily chores: Plants, Cups, Trash, Tables)
    if state.duty_active_chore.is_none() && free_len.saturating_sub(next_free) >= 1 {
        let scheduled_chore = if events.force_duty {
            [
                DutyChore::Cups,
                DutyChore::Trash,
                DutyChore::Plants,
                DutyChore::Tables,
            ]
            .into_iter()
            .find(|c| !state.duty_completed[*c as usize])
        } else if clock.is_workday {
            if !state.duty_completed[DutyChore::Plants as usize] && (540..=690).contains(&m) {
                Some(DutyChore::Plants)
            } else if !state.duty_completed[DutyChore::Cups as usize]
                && state.duty_cups_pile > 0
                && (810..=990).contains(&m)
            {
                Some(DutyChore::Cups)
            } else if !state.duty_completed[DutyChore::Trash as usize] && (1005..=1080).contains(&m)
            {
                Some(DutyChore::Trash)
            } else if !state.duty_completed[DutyChore::Tables as usize]
                && (1005..=1080).contains(&m)
            {
                Some(DutyChore::Tables)
            } else {
                None
            }
        } else {
            None
        };

        if let Some(chore) = scheduled_chore {
            events.force_duty = false;
            if let Some((ent, home_pos, home_yaw, home_sit)) = free_buf[next_free] {
                next_free += 1;
                if coord.claim(ent) {
                    let (target, face, prop) = duty_chore_target(nav, chore);
                    if let Ok((_, _, chibi, mut walker, _, _, active_prop)) = workers_q.get_mut(ent)
                    {
                        walker.set_destination(
                            nav,
                            chibi.base_pos,
                            target,
                            face,
                            false,
                            WorkerAnim::Work,
                        );
                        commands.entity(ent).insert(ActiveRoutine::new(
                            RoutineKind::Duty,
                            chore as u8,
                            3.0,
                            home_pos,
                            home_yaw,
                            home_sit,
                            Some(prop),
                        ));
                        sync_lazy_prop(
                            &mut commands,
                            cache,
                            pal,
                            mats,
                            ent,
                            active_prop,
                            Some(prop),
                        );
                        state.duty_active_chore = Some(chore);
                    }
                }
            }
        }
    }

    // 7. Errands (printer, shredder, water cooler, thirsty plant)
    let pending_errand = if events.printer_job {
        events.printer_job = false;
        Some(ErrandKind::Printer)
    } else if events.shredder_job {
        events.shredder_job = false;
        Some(ErrandKind::Shredder)
    } else if events.water_cooler_job {
        events.water_cooler_job = false;
        Some(ErrandKind::WaterCooler)
    } else if events.plant_thirsty {
        events.plant_thirsty = false;
        Some(ErrandKind::PlantWater)
    } else {
        None
    };

    if let Some(errand) = pending_errand
        && free_len.saturating_sub(next_free) >= 1
    {
        if let Some((ent, home_pos, home_yaw, home_sit)) = free_buf[next_free] {
            if coord.claim(ent) {
                let (target, face, prop) = errand_target(nav, errand);
                if let Ok((_, _, chibi, mut walker, _, _, active_prop)) = workers_q.get_mut(ent) {
                    walker.set_destination(
                        nav,
                        chibi.base_pos,
                        target,
                        face,
                        false,
                        WorkerAnim::Wait,
                    );
                    commands.entity(ent).insert(ActiveRoutine::new(
                        RoutineKind::Errands,
                        errand as u8,
                        2.0,
                        home_pos,
                        home_yaw,
                        home_sit,
                        Some(prop),
                    ));
                    sync_lazy_prop(
                        &mut commands,
                        cache,
                        pal,
                        mats,
                        ent,
                        active_prop,
                        Some(prop),
                    );
                    state.last_errand = Some(errand);
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::backdrops::BackdropKind;
    use crate::cast::{RoutineKind, begin_cast_tick_system};
    use crate::chibi::{ChibiLook, WorkerAnim};
    use crate::chibi_props::PropKind;
    use crate::layout::OfficeLayout;
    use crate::people::{LiveWorkerSpec, step_nav_walkers_system, sync_live_workers_in_world};
    use crate::{SharedFontEngine, spawn_full_office_world};
    use std::path::PathBuf;
    use std::time::Duration;

    fn build_pro_world(worker_count: usize) -> World {
        let assets_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../assets");
        let mut world = World::new();
        world.insert_resource(Assets::<Mesh>::default());
        world.insert_resource(Assets::<StandardMaterial>::default());
        world.insert_resource(Assets::<Image>::default());
        world.insert_resource(SharedFontEngine::new(assets_dir.clone()));
        world.insert_resource(Time::<()>::default());
        world.insert_resource(CastClock::default());
        world.insert_resource(CastEvents::default());
        world.insert_resource(RoutineCoordinator::default());
        world.insert_resource(MealsBreaksState::default());
        world.init_resource::<Events<PropActivated>>();

        world.resource_scope(|world, mut meshes: Mut<Assets<Mesh>>| {
            world.resource_scope(|world, mut mats: Mut<Assets<StandardMaterial>>| {
                world.resource_scope(|world, mut imgs: Mut<Assets<Image>>| {
                    world.resource_scope(|world, mut shared_fonts: Mut<SharedFontEngine>| {
                        let fonts = shared_fonts.get_or_init();
                        let mut cmds = world.commands();
                        spawn_full_office_world(
                            &mut cmds,
                            &mut meshes,
                            &mut mats,
                            &mut imgs,
                            fonts,
                            &assets_dir,
                            OfficeLayout::Pro,
                            BackdropKind::City,
                            ChibiLook::Pixar,
                            false,
                        );
                    });
                });
            });
        });
        world.flush();

        let specs: Vec<LiveWorkerSpec> = (0..worker_count)
            .map(|i| LiveWorkerSpec {
                session_id: format!("meal-worker-{i}"),
                agent: "claude".to_owned(),
                name: format!("Worker {i}"),
                status: "rest".to_owned(),
                desk_hint: i,
                tool_kind: None,
                tool_label: None,
                git_dirty: 0,
                subagents: Vec::new(),
            })
            .collect();
        sync_live_workers_in_world(&mut world, &specs, ChibiLook::Pixar);

        let nav = world.resource::<NavGraph>().clone();
        for (live, mut chibi, mut walker) in world
            .query::<(&LiveSessionWorker, &mut ChibiWorker, &mut NavWalker)>()
            .iter_mut(&mut world)
        {
            if let Some(ds) = nav.desk_spot(live.desk_index) {
                chibi.base_pos = ds.pos;
                chibi.base_yaw = ds.face;
                chibi.sitting = true;
                chibi.anim = WorkerAnim::Sleep;
                walker.waypoints.clear();
                walker.cursor = 0;
            }
        }
        world
    }

    fn tick(world: &mut World, sched: &mut Schedule, steps: usize, dt_ms: u64) {
        for _ in 0..steps {
            world
                .resource_mut::<Time>()
                .advance_by(Duration::from_millis(dt_ms));
            sched.run(world);
            world.flush();
        }
    }

    #[test]
    fn noon_1200_triggers_comnha_and_comtrua_with_nav_paths_and_lazy_lunchbox() {
        let mut world = build_pro_world(6);
        let mut sched = Schedule::default();
        sched.add_systems(
            (
                begin_cast_tick_system,
                step_meals_breaks_system,
                step_nav_walkers_system,
            )
                .chain(),
        );

        // Friday 12:00 noon -> triggers both ComNha microwave lunch queue & Friday Bún đậu mắm tôm ComTrua
        {
            let mut clk = world.resource_mut::<CastClock>();
            clk.minutes = 12 * 60;
            clk.weekday = 5;
            clk.is_workday = true;
        }
        tick(&mut world, &mut sched, 1, 50);

        let nav = world.resource::<NavGraph>().clone();
        let mut comnha_workers = 0;
        let mut comtrua_workers = 0;
        let mut saw_lunch_prop = false;

        for (chibi, walker, routine, prop) in world
            .query::<(
                &ChibiWorker,
                &NavWalker,
                &ActiveRoutine,
                Option<&ActivePropEntity>,
            )>()
            .iter(&world)
        {
            if walker.is_walking() {
                let mut prev = chibi.base_pos;
                for &wp in &walker.waypoints[walker.cursor..] {
                    assert!(
                        !nav.crosses_wall(prev, wp),
                        "{:?} walk crossed a wall: {prev:?} -> {wp:?}",
                        routine.kind
                    );
                    prev = wp;
                }
            }
            match routine.kind {
                RoutineKind::ComNha => {
                    comnha_workers += 1;
                    if matches!(
                        prop.map(|p| p.kind),
                        Some(PropKind::HomeBox | PropKind::Noodle | PropKind::LunchBag)
                    ) {
                        saw_lunch_prop = true;
                    }
                }
                RoutineKind::ComTrua => comtrua_workers += 1,
                _ => {}
            }
        }

        assert!(
            comnha_workers >= 1,
            "Noon 12:00 must trigger ComNha microwave lunch queue"
        );
        assert!(
            saw_lunch_prop,
            "ComNha worker must carry lazy lunchbox/noodle prop"
        );
        assert!(
            comtrua_workers >= 1,
            "Friday noon 12:00 must trigger ComTrua street bún đậu mắm tôm outing"
        );
        assert!(
            world.resource::<MealsBreaksState>().comtrua_is_friday_bun_dau,
            "Friday ComTrua must be marked as bún đậu mắm tôm"
        );

        // Step until ComNha heats & holds HomeBox/Noodle while eating
        let mut saw_homebox_or_noodle = false;
        for _ in 0..260 {
            tick(&mut world, &mut sched, 1, 100);
            for (r, p) in world
                .query::<(&ActiveRoutine, Option<&ActivePropEntity>)>()
                .iter(&world)
            {
                if r.kind == RoutineKind::ComNha
                    && matches!(p.map(|x| x.kind), Some(PropKind::HomeBox | PropKind::Noodle))
                {
                    saw_homebox_or_noodle = true;
                }
            }
            if saw_homebox_or_noodle {
                break;
            }
        }
        assert!(
            saw_homebox_or_noodle,
            "ComNha worker must transition to holding HomeBox or Noodle"
        );
    }

    #[test]
    fn friday_1600_triggers_teatime_holding_boba_prop() {
        let mut world = build_pro_world(4);
        let mut sched = Schedule::default();
        sched.add_systems(
            (
                begin_cast_tick_system,
                step_meals_breaks_system,
                step_nav_walkers_system,
            )
                .chain(),
        );

        {
            let mut clk = world.resource_mut::<CastClock>();
            clk.minutes = 16 * 60;
            clk.weekday = 5;
            clk.is_workday = true;
        }
        tick(&mut world, &mut sched, 1, 50);

        let boba_count = world
            .query::<(&ActiveRoutine, Option<&ActivePropEntity>)>()
            .iter(&world)
            .filter(|(r, p)| r.kind == RoutineKind::Teatime && p.map(|x| x.kind) == Some(PropKind::Boba))
            .count();

        assert!(
            world.resource::<MealsBreaksState>().teatime_active,
            "16:00 Friday must activate Teatime"
        );
        assert!(
            boba_count >= 1,
            "Teatime participants must hold lazy PropKind::Boba; got {boba_count}"
        );
    }

    #[test]
    fn afternoon_breaks_duty_errands_and_bell_gatherings_trigger_cleanly() {
        let mut world = build_pro_world(6);
        let mut sched = Schedule::default();
        sched.add_systems(
            (
                begin_cast_tick_system,
                step_meals_breaks_system,
                step_nav_walkers_system,
            )
                .chain(),
        );

        // 1. Bell gathering
        world.resource_mut::<CastEvents>().bell_rung = true;
        tick(&mut world, &mut sched, 1, 50);
        assert!(
            world.resource::<MealsBreaksState>().gathering_active,
            "Bell ring must trigger Gathering at meeting table"
        );
        let gatherers = world
            .query::<&ActiveRoutine>()
            .iter(&world)
            .filter(|r| r.kind == RoutineKind::Gatherings)
            .count();
        assert!(gatherers >= 1);

        // Clear gathering and test 15:00 Breaks + Duty + Printer Errand
        for (_, mut w) in world
            .query_filtered::<(Entity, &mut NavWalker), With<ActiveRoutine>>()
            .iter_mut(&mut world)
        {
            w.waypoints.clear();
            w.cursor = 0;
        }
        let ents: Vec<Entity> = world
            .query_filtered::<Entity, With<ActiveRoutine>>()
            .iter(&world)
            .collect();
        for e in ents {
            world.entity_mut(e).remove::<ActiveRoutine>();
        }
        world.resource_mut::<MealsBreaksState>().gathering_active = false;

        {
            let mut clk = world.resource_mut::<CastClock>();
            clk.minutes = 15 * 60 + 5;
            clk.weekday = 2;
        }
        {
            let mut ev = world.resource_mut::<CastEvents>();
            ev.force_duty = true;
            ev.printer_job = true;
        }
        tick(&mut world, &mut sched, 1, 50);

        let mut saw_break = false;
        let mut saw_duty = false;
        let mut saw_errand = false;
        for (r, p) in world
            .query::<(&ActiveRoutine, Option<&ActivePropEntity>)>()
            .iter(&world)
        {
            match r.kind {
                RoutineKind::Breaks => {
                    saw_break = matches!(p.map(|x| x.kind), Some(PropKind::Coffee | PropKind::Cup));
                }
                RoutineKind::Duty => saw_duty = true,
                RoutineKind::Errands => saw_errand = true,
                _ => {}
            }
        }
        assert!(saw_break, "15:00 must trigger Breaks holding Coffee/Cup");
        assert!(saw_duty, "Duty chore must be claimed");
        assert!(saw_errand, "Printer job must trigger Errands");
    }
}
