//! Street & after-hours cultural routines ported from Golden Oracle `src/office/people/`:
//! - `outings.js`: Convenience-store runs (GS25/Circle K/Mixue/7-Eleven/Highlands/Cộng)
//! - `nhau.js`: Evening *quán nhậu* (`"1, 2, 3 dô!"` with `Beer` + Grab home, scooter left parked)
//! - `payday.js`: Payday ATM queue (5th, 10th, 25th, or last weekday of month)
//! - `photobooth.js`: Release-tag Korean photobooth with 4-frame flash countdown
//! - `pickle.js`: Green-CI after-hours (`>= 17:30`) pickleball doubles rally (`Paddle`)
//! - `evswap.js`: `< 20%` battery scooter battery swap (`BatteryPack`)

pub use crate::cast::OutingShopKind;
use crate::cast::{
    ActiveRoutine, CastClock, CastEvents, RoutineCoordinator, RoutineKind, is_worker_recalled,
    is_worker_resting, return_arrive_anim, sync_home_lifecycle, sync_lazy_prop,
};
use crate::chibi::{ChibiWorker, WorkerAnim};
use crate::chibi_meshes::ChibiMeshCache;
use crate::chibi_props::{ActivePropEntity, PropKind};
use crate::nav::NavGraph;
use crate::palette::{WorldEntity, WorldPalette};
use crate::people::{LiveSessionWorker, NavWalker, SubagentWorker, WorkerLifecycle};
use bevy::prelude::*;

pub const NHAU_CHEERS_LINE: &str = "1, 2, 3 dô!";
pub const NHAU_NO_DRIVE_LINE: &str = "Đã uống rượu bia thì không lái xe — gọi Grab về!";

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum PhotoboothStage {
    #[default]
    Idle,
    Going,
    Shooting,
    Printed,
    Returning,
}

/// Optional `WorldEntity` flash light spawned on demand during Korean photobooth shoots.
#[derive(Component, Debug, Clone, Copy)]
pub struct PhotoboothFlashLight;

#[derive(Resource, Debug, Default, Clone)]
pub struct StreetOutingsState {
    pub outing_active: bool,
    pub outing_shop: Option<OutingShopKind>,
    pub outings_completed: u32,
    pub next_auto_outing_secs: f32,
    pub nhau_active: bool,
    pub nhau_day: Option<u32>,
    pub nhau_toasts: u32,
    pub nhau_toast_timer: f32,
    pub nhau_grab_home_dispatched: bool,
    pub nhau_last_chant: &'static str,
    pub payday_active: bool,
    pub payday_day: Option<u32>,
    pub payday_trips_today: u8,
    pub photobooth_stage: PhotoboothStage,
    pub photobooth_frame: u8,
    pub photobooth_flashes_fired: u8,
    pub photobooth_flash_intensity: f32,
    pub photobooth_strips_printed: u32,
    pub photobooth_tag: String,
    pub photobooth_frame_timer: f32,
    pub pickle_active: bool,
    pub pickle_day: Option<u32>,
    pub pickle_rally_points: u32,
    pub pickle_point_timer: f32,
    pub evswap_active: bool,
    pub evswap_day: Option<u32>,
    pub evswap_completed: u32,
}

fn is_street_routine(kind: RoutineKind) -> bool {
    matches!(
        kind,
        RoutineKind::Outings
            | RoutineKind::Nhau
            | RoutineKind::Payday
            | RoutineKind::Photobooth
            | RoutineKind::Pickleball
            | RoutineKind::EvSwap
    )
}

#[allow(clippy::too_many_arguments)]
fn send_home(
    nav: &NavGraph,
    live: &LiveSessionWorker,
    chibi: &ChibiWorker,
    walker: &mut NavWalker,
    lifecycle: Option<Mut<'_, WorkerLifecycle>>,
    vis: &mut Visibility,
    routine: &mut ActiveRoutine,
    recalled: bool,
) {
    routine.returning = true;
    routine.desired_prop = None;
    *vis = Visibility::Visible;
    let anim = return_arrive_anim(live, recalled);
    sync_home_lifecycle(lifecycle, anim);
    walker.set_destination(
        nav,
        chibi.base_pos,
        routine.home_pos,
        routine.home_yaw,
        routine.home_sit,
        anim,
    );
}

/// Steps `Outings`, `Nhau`, `Payday`, `Photobooth`, `Pickleball`, and `EvSwap` over `NavGraph`
/// with lazy `set_active_prop`, tracked `WorldEntity` photobooth flash light, and zero per-frame heap churn.
#[allow(clippy::too_many_arguments, clippy::type_complexity)]
pub fn step_street_outings_system(
    time: Res<Time>,
    mut commands: Commands,
    nav: Option<Res<NavGraph>>,
    clock: Option<Res<CastClock>>,
    mut events: Option<ResMut<CastEvents>>,
    mut coord: Option<ResMut<RoutineCoordinator>>,
    mut state: Option<ResMut<StreetOutingsState>>,
    chibi_cache: Option<Res<ChibiMeshCache>>,
    mut palette: Option<ResMut<WorldPalette>>,
    mut materials: Option<ResMut<Assets<StandardMaterial>>>,
    mut workers_q: Query<
        (
            Entity,
            &LiveSessionWorker,
            &mut ChibiWorker,
            &mut NavWalker,
            Option<&mut WorkerLifecycle>,
            &mut Visibility,
            Option<&mut ActiveRoutine>,
            Option<&ActivePropEntity>,
        ),
        (Without<crate::cast::OfficeStaffNpc>, Without<SubagentWorker>),
    >,
    mut flash_q: Query<(Entity, &mut PointLight), With<PhotoboothFlashLight>>,
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

    if state.payday_day != Some(clock.day_index) {
        state.payday_day = Some(clock.day_index);
        state.payday_trips_today = 0;
    }

    // If the backdrop switched away from City, immediately abort all active street outings,
    // restore visibility, stow props, despawn photobooth flash light, and walk participants home.
    if !events.city_street_available {
        let mut photo_returning = 0usize;
        for (ent, live, chibi, mut walker, lifecycle, mut vis, routine_opt, active_prop) in
            &mut workers_q
        {
            let Some(mut routine) = routine_opt else {
                continue;
            };
            if !is_street_routine(routine.kind) {
                continue;
            }
            if !routine.returning {
                send_home(
                    nav,
                    live,
                    &chibi,
                    &mut walker,
                    lifecycle,
                    &mut vis,
                    &mut routine,
                    is_worker_recalled(live),
                );
            }
            sync_lazy_prop(&mut commands, cache, pal, mats, ent, active_prop, None);
            if walker.is_walking() {
                if routine.kind == RoutineKind::Photobooth {
                    photo_returning += 1;
                }
            } else {
                *vis = Visibility::Visible;
                commands.entity(ent).remove::<ActiveRoutine>();
            }
        }
        for (fe, _) in &flash_q {
            commands.entity(fe).try_despawn();
        }
        state.photobooth_flash_intensity = 0.0;
        state.photobooth_stage = if photo_returning > 0 {
            PhotoboothStage::Returning
        } else {
            PhotoboothStage::Idle
        };
        state.outing_active = false;
        state.outing_shop = None;
        state.nhau_active = false;
        state.payday_active = false;
        state.pickle_active = false;
        state.evswap_active = false;
        return;
    }

    // Decay photobooth flash intensity smoothly
    if state.photobooth_flash_intensity > 0.0 {
        state.photobooth_flash_intensity = (state.photobooth_flash_intensity - dt * 3.5).max(0.0);
    }

    // Pass 1: Step active street outing participants.
    let mut outing_active = false;
    let mut nhau_total = 0usize;
    let mut nhau_seated = 0usize;
    let mut nhau_done = false;
    let mut payday_active = false;
    let mut photo_total = 0usize;
    let mut photo_returning = 0usize;
    let mut photo_arrived = 0usize;
    let mut pickle_total = 0usize;
    let mut pickle_playing = 0usize;
    let mut evswap_active = false;

    for (ent, live, chibi, mut walker, lifecycle, mut vis, routine_opt, active_prop) in
        &mut workers_q
    {
        let Some(mut routine) = routine_opt else {
            continue;
        };
        if !is_street_routine(routine.kind) {
            continue;
        }

        if routine.returning {
            sync_lazy_prop(&mut commands, cache, pal, mats, ent, active_prop, None);
            if walker.is_walking() {
                if routine.kind == RoutineKind::Photobooth {
                    photo_returning += 1;
                }
            } else {
                *vis = Visibility::Visible;
                commands.entity(ent).remove::<ActiveRoutine>();
            }
            continue;
        }

        if is_worker_recalled(live) {
            let was_photo = routine.kind == RoutineKind::Photobooth;
            send_home(
                nav,
                live,
                &chibi,
                &mut walker,
                lifecycle,
                &mut vis,
                &mut routine,
                true,
            );
            sync_lazy_prop(&mut commands, cache, pal, mats, ent, active_prop, None);
            if was_photo && walker.is_walking() {
                photo_returning += 1;
            }
            continue;
        }

        // Dwell ONLY counts down after arrival (!walker.is_walking())
        if !walker.is_walking() && routine.dwell_secs > 0.0 {
            routine.dwell_secs = (routine.dwell_secs - dt).max(0.0);
        }

        match routine.kind {
            RoutineKind::Outings => {
                outing_active = true;
                if !walker.is_walking() && routine.dwell_secs <= 0.0 {
                    if routine.stage == 0 {
                        *vis = Visibility::Hidden;
                        routine.stage = 1;
                        routine.dwell_secs = 4.0;
                    } else {
                        state.outings_completed += 1;
                        send_home(
                            nav,
                            live,
                            &chibi,
                            &mut walker,
                            lifecycle,
                            &mut vis,
                            &mut routine,
                            false,
                        );
                    }
                }
            }
            RoutineKind::Nhau => {
                nhau_total += 1;
                if !walker.is_walking() {
                    if routine.stage == 0 {
                        *vis = Visibility::Hidden;
                        routine.stage = 1;
                        routine.dwell_secs = 6.0;
                    } else if routine.stage == 1 {
                        nhau_seated += 1;
                        if routine.dwell_secs <= 0.0 {
                            nhau_done = true;
                        }
                    }
                }
            }
            RoutineKind::Payday => {
                payday_active = true;
                if !walker.is_walking() && routine.dwell_secs <= 0.0 {
                    if routine.stage == 0 {
                        *vis = Visibility::Hidden;
                        routine.stage = 1;
                        routine.dwell_secs = 4.5;
                    } else {
                        send_home(
                            nav,
                            live,
                            &chibi,
                            &mut walker,
                            lifecycle,
                            &mut vis,
                            &mut routine,
                            false,
                        );
                    }
                }
            }
            RoutineKind::Photobooth => {
                photo_total += 1;
                if !walker.is_walking() {
                    photo_arrived += 1;
                    if state.photobooth_stage == PhotoboothStage::Shooting {
                        *vis = Visibility::Hidden;
                    }
                }
            }
            RoutineKind::Pickleball => {
                pickle_total += 1;
                if !walker.is_walking() {
                    if routine.stage == 0 {
                        *vis = Visibility::Hidden;
                        routine.stage = 1;
                        routine.dwell_secs = 7.0;
                    } else if routine.stage == 1 {
                        pickle_playing += 1;
                        if routine.dwell_secs <= 0.0 {
                            send_home(
                                nav,
                                live,
                                &chibi,
                                &mut walker,
                                lifecycle,
                                &mut vis,
                                &mut routine,
                                false,
                            );
                        }
                    }
                }
            }
            RoutineKind::EvSwap => {
                evswap_active = true;
                if !walker.is_walking() && routine.dwell_secs <= 0.0 {
                    if routine.stage == 0 {
                        *vis = Visibility::Hidden;
                        routine.stage = 1;
                        routine.dwell_secs = 4.0;
                    } else {
                        state.evswap_completed += 1;
                        send_home(
                            nav,
                            live,
                            &chibi,
                            &mut walker,
                            lifecycle,
                            &mut vis,
                            &mut routine,
                            false,
                        );
                    }
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

    // Advance Nhau cheers ("1, 2, 3 dô!") & Grab home wrap-up
    if nhau_total == 0 {
        state.nhau_active = false;
    } else if nhau_seated > 0 {
        state.nhau_toast_timer += dt;
        if state.nhau_toast_timer >= 2.0 {
            state.nhau_toast_timer = 0.0;
            state.nhau_toasts += 1;
            state.nhau_last_chant = NHAU_CHEERS_LINE;
        }
        if nhau_done {
            if state.nhau_toasts == 0 {
                state.nhau_toasts = 1;
                state.nhau_last_chant = NHAU_CHEERS_LINE;
            }
            state.nhau_grab_home_dispatched = true;
            state.nhau_active = false;
            for (ent, live, chibi, mut walker, lifecycle, mut vis, routine_opt, active_prop) in
                &mut workers_q
            {
                if let Some(mut r) = routine_opt
                    && r.kind == RoutineKind::Nhau
                    && !r.returning
                {
                    send_home(
                        nav,
                        live,
                        &chibi,
                        &mut walker,
                        lifecycle,
                        &mut vis,
                        &mut r,
                        false,
                    );
                    sync_lazy_prop(&mut commands, cache, pal, mats, ent, active_prop, None);
                }
            }
        }
    }

    // Advance Pickleball rally points
    if pickle_total == 0 {
        state.pickle_active = false;
    } else if pickle_playing > 0 {
        state.pickle_point_timer += dt;
        if state.pickle_point_timer >= 1.5 {
            state.pickle_point_timer = 0.0;
            state.pickle_rally_points += 1;
        }
    }

    // Advance Korean Photobooth 4-frame flash countdown (keep `Returning` alive while walking back)
    let mut finish_photobooth = false;
    if photo_total == 0 {
        for (fe, _) in &flash_q {
            commands.entity(fe).try_despawn();
        }
        state.photobooth_flash_intensity = 0.0;
        if photo_returning > 0 {
            state.photobooth_stage = PhotoboothStage::Returning;
        } else if state.photobooth_stage != PhotoboothStage::Idle {
            state.photobooth_stage = PhotoboothStage::Idle;
        }
    } else {
        match state.photobooth_stage {
            PhotoboothStage::Going if photo_arrived == photo_total => {
                state.photobooth_stage = PhotoboothStage::Shooting;
                state.photobooth_frame = 0;
                state.photobooth_flashes_fired = 0;
                state.photobooth_frame_timer = 0.0;
                if flash_q.is_empty() {
                    commands.spawn((
                        WorldEntity,
                        PhotoboothFlashLight,
                        PointLight {
                            color: Color::srgb(1.0, 0.96, 1.0),
                            intensity: 0.0,
                            range: 9.0,
                            shadows_enabled: false,
                            ..default()
                        },
                        Transform::from_xyz(-9.5, 2.0, 3.3),
                    ));
                }
            }
            PhotoboothStage::Shooting => {
                state.photobooth_frame_timer += dt;
                if state.photobooth_frame_timer >= 1.1 {
                    state.photobooth_frame_timer = 0.0;
                    state.photobooth_frame = state.photobooth_frame.saturating_add(1);
                    state.photobooth_flashes_fired =
                        state.photobooth_flashes_fired.saturating_add(1);
                    state.photobooth_flash_intensity = 1.0;
                    if state.photobooth_flashes_fired >= 4 {
                        state.photobooth_stage = PhotoboothStage::Printed;
                        state.photobooth_strips_printed += 1;
                        finish_photobooth = true;
                    }
                }
            }
            PhotoboothStage::Printed => {
                finish_photobooth = true;
            }
            _ => {}
        }
        for (_, mut pl) in &mut flash_q {
            pl.intensity = state.photobooth_flash_intensity * 90_000.0;
        }
    }

    if finish_photobooth {
        state.photobooth_stage = PhotoboothStage::Returning;
        for (fe, _) in &flash_q {
            commands.entity(fe).try_despawn();
        }
        state.photobooth_flash_intensity = 0.0;
        for (ent, live, chibi, mut walker, lifecycle, mut vis, routine_opt, active_prop) in
            &mut workers_q
        {
            if let Some(mut r) = routine_opt
                && r.kind == RoutineKind::Photobooth
                && !r.returning
            {
                send_home(
                    nav,
                    live,
                    &chibi,
                    &mut walker,
                    lifecycle,
                    &mut vis,
                    &mut r,
                    false,
                );
                sync_lazy_prop(&mut commands, cache, pal, mats, ent, active_prop, None);
            }
        }
    }

    state.outing_active = outing_active;
    if !outing_active {
        state.outing_shop = None;
    }
    state.payday_active = payday_active;
    state.evswap_active = evswap_active;

    // Pass 2: Collect free resting workers into a fixed stack buffer (zero heap allocations).
    let mut free_buf: [Option<(Entity, Vec3, f32, bool)>; 16] = [None; 16];
    let mut free_len = 0usize;
    for (ent, live, chibi, walker, _, _, routine_opt, _) in &workers_q {
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
    let door = nav.door_in();

    // 1. Korean Photobooth on release_tag event (`photobooth.js`)
    if state.photobooth_stage == PhotoboothStage::Idle
        && events.release_tag.is_some()
        && free_len.saturating_sub(next_free) >= 1
    {
        if let Some(tag) = events.release_tag.take() {
            state.photobooth_tag = tag;
            state.photobooth_stage = PhotoboothStage::Going;
            state.photobooth_frame = 0;
            state.photobooth_flashes_fired = 0;
            state.photobooth_frame_timer = 0.0;
            let count = free_len.saturating_sub(next_free).min(4);
            for slot in 0..count {
                let Some((ent, home_pos, home_yaw, home_sit)) = free_buf[next_free] else {
                    break;
                };
                next_free += 1;
                if !coord.claim(ent) {
                    continue;
                }
                if let Ok((_, _, chibi, mut walker, _, _, _, _)) = workers_q.get_mut(ent) {
                    walker.set_destination(
                        nav,
                        chibi.base_pos,
                        door.pos,
                        door.face,
                        false,
                        WorkerAnim::Rest,
                    );
                    commands.entity(ent).insert(ActiveRoutine::new(
                        RoutineKind::Photobooth,
                        slot as u8,
                        0.2,
                        home_pos,
                        home_yaw,
                        home_sit,
                        None,
                    ));
                }
            }
        }
    }

    // 2. Green-CI after-hours (`>= 17:30`) Pickleball rally holding `PropKind::Paddle` (`pickle.js`)
    let pickle_window = events.ci_green
        && !events.ci_red_today
        && (17 * 60 + 30..20 * 60).contains(&m)
        && state.pickle_day != Some(clock.day_index);
    let want_pickle = events.force_pickle || pickle_window;
    if !state.pickle_active && want_pickle && free_len.saturating_sub(next_free) >= 2 {
        events.force_pickle = false;
        state.pickle_day = Some(clock.day_index);
        state.pickle_active = true;
        state.pickle_point_timer = 0.0;
        let count = free_len.saturating_sub(next_free).min(5);
        for slot in 0..count {
            let Some((ent, home_pos, home_yaw, home_sit)) = free_buf[next_free] else {
                break;
            };
            next_free += 1;
            if !coord.claim(ent) {
                continue;
            }
            let prop = if slot < 4 {
                PropKind::Paddle
            } else {
                PropKind::Phone
            };
            if let Ok((_, _, chibi, mut walker, _, _, _, active_prop)) = workers_q.get_mut(ent) {
                walker.set_destination(
                    nav,
                    chibi.base_pos,
                    door.pos,
                    door.face,
                    false,
                    WorkerAnim::Rest,
                );
                commands.entity(ent).insert(ActiveRoutine::new(
                    RoutineKind::Pickleball,
                    slot as u8,
                    0.5,
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
            }
        }
    }

    // 3. `< 20%` battery scooter battery swap holding `PropKind::BatteryPack` (`evswap.js`)
    let ev_due = events.battery_pct < 20
        && !events.battery_charging
        && state.evswap_day != Some(clock.day_index);
    let want_ev = events.force_evswap || ev_due;
    if !state.evswap_active && want_ev && free_len.saturating_sub(next_free) >= 1 {
        events.force_evswap = false;
        state.evswap_day = Some(clock.day_index);
        if let Some((ent, home_pos, home_yaw, home_sit)) = free_buf[next_free] {
            next_free += 1;
            if coord.claim(ent) {
                if let Ok((_, _, chibi, mut walker, _, _, _, active_prop)) = workers_q.get_mut(ent)
                {
                    walker.set_destination(
                        nav,
                        chibi.base_pos,
                        door.pos,
                        door.face,
                        false,
                        WorkerAnim::Rest,
                    );
                    commands.entity(ent).insert(ActiveRoutine::new(
                        RoutineKind::EvSwap,
                        0,
                        0.5,
                        home_pos,
                        home_yaw,
                        home_sit,
                        Some(PropKind::BatteryPack),
                    ));
                    sync_lazy_prop(
                        &mut commands,
                        cache,
                        pal,
                        mats,
                        ent,
                        active_prop,
                        Some(PropKind::BatteryPack),
                    );
                    state.evswap_active = true;
                }
            }
        }
    }

    // 4. Convenience-store snack run (`outings.js`: consume `force_outing` ONLY on dispatch; cooldown auto runs)
    let shop_candidate = if let Some(shop) = events.force_outing {
        Some((shop, true))
    } else if clock.sim_secs >= state.next_auto_outing_secs {
        if events.ambient_temp_c >= 32.0 && (750..1050).contains(&m) {
            Some((OutingShopKind::Mixue, false))
        } else if m >= 1320 || m < 120 {
            Some((OutingShopKind::CircleK, false))
        } else {
            None
        }
    } else {
        None
    };
    if !state.outing_active
        && let Some((shop, is_forced)) = shop_candidate
        && free_len.saturating_sub(next_free) >= 1
    {
        if let Some((ent, home_pos, home_yaw, home_sit)) = free_buf[next_free] {
            next_free += 1;
            if coord.claim(ent) {
                if is_forced {
                    events.force_outing = None;
                } else {
                    state.next_auto_outing_secs = clock.sim_secs + 360.0;
                }
                let prop = shop.prop_kind();
                if let Ok((_, _, chibi, mut walker, _, _, _, active_prop)) = workers_q.get_mut(ent)
                {
                    walker.set_destination(
                        nav,
                        chibi.base_pos,
                        door.pos,
                        door.face,
                        false,
                        WorkerAnim::Rest,
                    );
                    commands.entity(ent).insert(ActiveRoutine::new(
                        RoutineKind::Outings,
                        0,
                        0.5,
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
                    state.outing_active = true;
                    state.outing_shop = Some(shop);
                }
            }
        }
    }

    // 5. Payday ATM queue (5th, 10th, 25th, or last weekday of month; `payday.js`)
    let payday_window = clock.is_payday()
        && ((705..=780).contains(&m) || (1005..=1110).contains(&m))
        && state.payday_trips_today < 2;
    let want_payday = events.force_payday || payday_window;
    if !state.payday_active && want_payday && free_len.saturating_sub(next_free) >= 1 {
        events.force_payday = false;
        if let Some((ent, home_pos, home_yaw, home_sit)) = free_buf[next_free] {
            next_free += 1;
            if coord.claim(ent) {
                if let Ok((_, _, chibi, mut walker, _, _, _, active_prop)) = workers_q.get_mut(ent)
                {
                    walker.set_destination(
                        nav,
                        chibi.base_pos,
                        door.pos,
                        door.face,
                        false,
                        WorkerAnim::Rest,
                    );
                    commands.entity(ent).insert(ActiveRoutine::new(
                        RoutineKind::Payday,
                        0,
                        0.5,
                        home_pos,
                        home_yaw,
                        home_sit,
                        Some(PropKind::Phone),
                    ));
                    sync_lazy_prop(
                        &mut commands,
                        cache,
                        pal,
                        mats,
                        ent,
                        active_prop,
                        Some(PropKind::Phone),
                    );
                    state.payday_active = true;
                    state.payday_trips_today = state.payday_trips_today.saturating_add(1);
                }
            }
        }
    }

    // 6. Evening quán nhậu (`nhau.js`: "1, 2, 3 dô!" with Beer + Grab home)
    let nhau_window = clock.is_workday
        && state.nhau_day != Some(clock.day_index)
        && ((clock.is_friday() && (1110..=1170).contains(&m))
            || (!clock.is_friday() && clock.day_index % 4 == 0 && (1080..=1200).contains(&m)));
    let want_nhau = events.force_nhau || nhau_window;
    if !state.nhau_active && want_nhau && free_len.saturating_sub(next_free) >= 2 {
        events.force_nhau = false;
        state.nhau_day = Some(clock.day_index);
        state.nhau_active = true;
        state.nhau_toasts = 0;
        state.nhau_toast_timer = 0.0;
        state.nhau_grab_home_dispatched = false;
        state.nhau_last_chant = "";
        let count = free_len.saturating_sub(next_free).min(4);
        for slot in 0..count {
            let Some((ent, home_pos, home_yaw, home_sit)) = free_buf[next_free] else {
                break;
            };
            next_free += 1;
            if !coord.claim(ent) {
                continue;
            }
            if let Ok((_, _, chibi, mut walker, _, _, _, active_prop)) = workers_q.get_mut(ent) {
                walker.set_destination(
                    nav,
                    chibi.base_pos,
                    door.pos,
                    door.face,
                    false,
                    WorkerAnim::Rest,
                );
                commands.entity(ent).insert(ActiveRoutine::new(
                    RoutineKind::Nhau,
                    slot as u8,
                    0.5,
                    home_pos,
                    home_yaw,
                    home_sit,
                    Some(PropKind::Beer),
                ));
                sync_lazy_prop(
                    &mut commands,
                    cache,
                    pal,
                    mats,
                    ent,
                    active_prop,
                    Some(PropKind::Beer),
                );
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
    use crate::layout::OfficeLayout;
    use crate::palette::WorldEntity;
    use crate::people::{LiveWorkerSpec, step_nav_walkers_system, sync_live_workers_in_world};
    use crate::{
        BackdropAssets, OfficeIbl, SharedFontEngine, spawn_full_office_world,
        unload_full_office_world,
    };
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
        world.insert_resource(StreetOutingsState::default());

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
                session_id: format!("street-worker-{i}"),
                agent: "claude".to_owned(),
                name: format!("Street {i}"),
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
    fn release_tag_event_triggers_korean_photobooth_with_4_frame_flash_progression() {
        let mut world = build_pro_world(4);
        let mut sched = Schedule::default();
        sched.add_systems(
            (
                begin_cast_tick_system,
                step_street_outings_system,
                step_nav_walkers_system,
            )
                .chain(),
        );

        world.resource_mut::<CastClock>().minutes = 14 * 60;
        world.resource_mut::<CastEvents>().release_tag = Some("v2.3.0".to_owned());

        tick(&mut world, &mut sched, 1, 50);
        assert_eq!(
            world.resource::<StreetOutingsState>().photobooth_stage,
            PhotoboothStage::Going
        );
        assert_eq!(world.resource::<StreetOutingsState>().photobooth_tag, "v2.3.0");

        // Advance through walk-to-lift + 4-frame flash countdown + strip print
        tick(&mut world, &mut sched, 260, 100);
        let st = world.resource::<StreetOutingsState>();
        assert_eq!(
            st.photobooth_flashes_fired, 4,
            "Korean photobooth must fire all 4 countdown flashes; got {}",
            st.photobooth_flashes_fired
        );
        assert!(
            st.photobooth_strips_printed >= 1,
            "Korean photobooth must print a release strip after 4 frames"
        );
    }

    #[test]
    fn photobooth_recall_mid_shooting_restores_visibility_and_despawns_flash_light() {
        let mut world = build_pro_world(2);
        let mut sched = Schedule::default();
        sched.add_systems(
            (
                begin_cast_tick_system,
                step_street_outings_system,
                step_nav_walkers_system,
            )
                .chain(),
        );

        world.resource_mut::<CastClock>().minutes = 14 * 60;
        world.resource_mut::<CastEvents>().release_tag = Some("v3.0.0".to_owned());

        // Advance until Shooting stage is active and workers are Hidden inside the street booth
        let mut reached_shooting = false;
        for _ in 0..160 {
            tick(&mut world, &mut sched, 1, 100);
            if world.resource::<StreetOutingsState>().photobooth_stage == PhotoboothStage::Shooting
            {
                tick(&mut world, &mut sched, 1, 100);
                reached_shooting = true;
                break;
            }
        }
        assert!(reached_shooting, "Photobooth must reach Shooting stage");
        assert_eq!(
            world
                .query_filtered::<Entity, With<PhotoboothFlashLight>>()
                .iter(&world)
                .count(),
            1,
            "PhotoboothFlashLight must exist during Shooting"
        );
        assert!(
            world
                .query_filtered::<&Visibility, With<LiveSessionWorker>>()
                .iter(&world)
                .all(|v| *v == Visibility::Hidden),
            "Photobooth participants should be hidden while shooting downstairs"
        );

        // Recall both workers back to work ("wait" status) mid-Shooting!
        for mut live in world.query::<&mut LiveSessionWorker>().iter_mut(&mut world) {
            live.status = "wait".to_owned();
        }
        tick(&mut world, &mut sched, 1, 100);

        assert_eq!(
            world
                .query_filtered::<Entity, With<PhotoboothFlashLight>>()
                .iter(&world)
                .count(),
            0,
            "Recalling Photobooth workers mid-Shooting must despawn PhotoboothFlashLight"
        );
        assert!(
            world
                .query_filtered::<&Visibility, With<LiveSessionWorker>>()
                .iter(&world)
                .all(|v| *v == Visibility::Visible),
            "Recalled Photobooth workers must immediately restore Visibility::Visible"
        );
        assert_eq!(
            world.resource::<StreetOutingsState>().photobooth_stage,
            PhotoboothStage::Returning,
            "Photobooth stage must stay Returning while recalled workers walk back"
        );

        // Finish return walk -> stage transitions cleanly to Idle and workers enter Wait anim
        tick(&mut world, &mut sched, 160, 100);
        assert_eq!(
            world.resource::<StreetOutingsState>().photobooth_stage,
            PhotoboothStage::Idle
        );
        assert!(
            world
                .query_filtered::<&ChibiWorker, With<LiveSessionWorker>>()
                .iter(&world)
                .all(|c| c.anim == WorkerAnim::Wait),
            "Recalled worker with status='wait' must arrive in WorkerAnim::Wait"
        );
    }

    #[test]
    fn green_ci_after_hours_triggers_pickleball_rally_holding_paddle() {
        let mut world = build_pro_world(4);
        let mut sched = Schedule::default();
        sched.add_systems(
            (
                begin_cast_tick_system,
                step_street_outings_system,
                step_nav_walkers_system,
            )
                .chain(),
        );

        {
            let mut clk = world.resource_mut::<CastClock>();
            clk.minutes = 17 * 60 + 35;
            clk.weekday = 3;
        }
        world.resource_mut::<CastEvents>().ci_green = true;

        tick(&mut world, &mut sched, 1, 50);
        assert!(
            world.resource::<StreetOutingsState>().pickle_active,
            "Green CI at >= 17:30 must trigger Pickleball rally"
        );
        let paddle_holders = world
            .query::<(&ActiveRoutine, Option<&ActivePropEntity>)>()
            .iter(&world)
            .filter(|(r, p)| {
                r.kind == RoutineKind::Pickleball && p.map(|x| x.kind) == Some(PropKind::Paddle)
            })
            .count();
        assert!(
            paddle_holders >= 2,
            "Pickleball players must hold lazy PropKind::Paddle; got {paddle_holders}"
        );
    }

    #[test]
    fn low_battery_under_20_triggers_evswap_holding_battery_pack() {
        let mut world = build_pro_world(2);
        let mut sched = Schedule::default();
        sched.add_systems(
            (
                begin_cast_tick_system,
                step_street_outings_system,
                step_nav_walkers_system,
            )
                .chain(),
        );

        world.resource_mut::<CastClock>().minutes = 15 * 60;
        {
            let mut ev = world.resource_mut::<CastEvents>();
            ev.battery_pct = 15;
            ev.battery_charging = false;
        }

        tick(&mut world, &mut sched, 1, 50);
        assert!(
            world.resource::<StreetOutingsState>().evswap_active,
            "battery_pct < 20 must trigger EvSwap routine"
        );
        let pack_holders = world
            .query::<(&ActiveRoutine, Option<&ActivePropEntity>)>()
            .iter(&world)
            .filter(|(r, p)| {
                r.kind == RoutineKind::EvSwap && p.map(|x| x.kind) == Some(PropKind::BatteryPack)
            })
            .count();
        assert_eq!(
            pack_holders, 1,
            "EvSwap worker must hold lazy PropKind::BatteryPack"
        );
    }

    #[test]
    fn payday_nhau_and_convenience_outings_trigger_and_unload_with_zero_leaks() {
        let mut world = build_pro_world(8);
        let mut sched = Schedule::default();
        sched.add_systems(
            (
                begin_cast_tick_system,
                step_street_outings_system,
                step_nav_walkers_system,
            )
                .chain(),
        );

        {
            let mut clk = world.resource_mut::<CastClock>();
            clk.minutes = 18 * 60 + 35;
            clk.weekday = 5;
            clk.day_of_month = 25;
        }
        {
            let mut ev = world.resource_mut::<CastEvents>();
            ev.force_outing = Some(OutingShopKind::Gs25);
            ev.force_payday = true;
            ev.force_nhau = true;
        }

        tick(&mut world, &mut sched, 1, 50);

        let st = world.resource::<StreetOutingsState>();
        assert!(st.outing_active, "Convenience store outing must start");
        assert!(st.payday_active, "Payday ATM queue must start");
        assert!(st.nhau_active, "Evening quán nhậu must start");

        let mut saw_tokbokki = false;
        let mut saw_beer = false;
        for (r, p) in world
            .query::<(&ActiveRoutine, Option<&ActivePropEntity>)>()
            .iter(&world)
        {
            if r.kind == RoutineKind::Outings && p.map(|x| x.kind) == Some(PropKind::Tokbokki) {
                saw_tokbokki = true;
            }
            if r.kind == RoutineKind::Nhau && p.map(|x| x.kind) == Some(PropKind::Beer) {
                saw_beer = true;
            }
        }
        assert!(saw_tokbokki, "GS25 outing must hold lazy PropKind::Tokbokki");
        assert!(saw_beer, "Quán nhậu worker must hold lazy PropKind::Beer");

        // Advance so quán nhậu cheers ("1, 2, 3 dô!") and Grab home dispatch fire
        tick(&mut world, &mut sched, 260, 100);
        let st = world.resource::<StreetOutingsState>();
        assert!(st.nhau_toasts >= 1, "Quán nhậu must trigger toast rounds");
        assert_eq!(st.nhau_last_chant, NHAU_CHEERS_LINE);
        assert!(
            st.nhau_grab_home_dispatched,
            "Quán nhậu must dispatch Grab home at wrap-up"
        );

        // Full unload must leave 0 WorldEntity and 0 leaked assets
        let world_ents: Vec<Entity> = world
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
                        world_ents,
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

        assert_eq!(
            world
                .query_filtered::<Entity, With<WorldEntity>>()
                .iter(&world)
                .count(),
            0
        );
        assert_eq!(world.resource::<Assets<Mesh>>().len(), 0);
        assert_eq!(world.resource::<Assets<StandardMaterial>>().len(), 0);
        assert_eq!(world.resource::<Assets<Image>>().len(), 0);
    }
}
