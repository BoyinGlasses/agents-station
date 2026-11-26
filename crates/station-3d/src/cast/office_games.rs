//! Office leisure game routines ported from Golden Oracle `src/office/people/`:
//! - `billiards.js`: 2-player pool match with `PropKind::Cue` and ball break shots
//! - `foosball.js`: 2-player foosball rally on opposite sides of the lounge table
//! - `gaming.js`: Lounge sofa console gaming with `PropKind::Gamepad`
//! - `karaoke.js` + `karaoke-songs.js`: Authentic 4-track Vietnamese/English karaoke booth + 0–100 score

use crate::cast::{
    ActiveRoutine, CastClock, CastEvents, RoutineCoordinator, RoutineKind, is_worker_recalled,
    is_worker_resting, sync_lazy_prop,
};
use crate::chibi::{ChibiWorker, WorkerAnim};
use crate::chibi_meshes::ChibiMeshCache;
use crate::chibi_props::{ActivePropEntity, PropKind};
use crate::nav::{NavGraph, face_to};
use crate::palette::WorldPalette;
use crate::people::{LiveSessionWorker, NavWalker, SubagentWorker};
use crate::props_detail::leisure::{BilliardsBall, BilliardsTable, KaraokeBooth};
use bevy::prelude::*;
use std::f32::consts::{FRAC_PI_2, PI};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SongMood {
    Major,
    Minor,
}

#[derive(Debug, Clone, Copy)]
pub struct KaraokeSong {
    pub id: &'static str,
    pub mood: SongMood,
    pub beat_secs: f32,
    pub base_midi: u8,
    pub title_vi: &'static str,
    pub title_en: &'static str,
    pub lyrics_vi: [&'static str; 6],
    pub lyrics_en: [&'static str; 6],
}

/// Exact 4 original office tracks from `d:\mystation\my-station\src\office\people\karaoke-songs.js`.
pub const KARAOKE_SONGS: [KaraokeSong; 4] = [
    KaraokeSong {
        id: "deploy",
        mood: SongMood::Minor,
        beat_secs: 0.50,
        base_midi: 57,
        title_vi: "Deploy lúc nửa đêm",
        title_en: "Deploying at Midnight",
        lyrics_vi: [
            "Đêm nay phố vắng em ngồi một mình",
            "Màn hình xanh ngắt cà phê đã nguội",
            "Tay em run run bấm nút deploy",
            "Cầu cho mọi thứ chạy ngon chạy êm",
            "Ơi con bug nhỏ đừng về đêm nay",
            "Để em ngủ yên đến sáng mai",
        ],
        lyrics_en: [
            "Late at night the street is empty, here I sit alone",
            "Screen is glowing blue, my coffee has gone cold",
            "Trembling hands, I press deploy",
            "Pray it all runs smooth and right",
            "Little bug, oh please don’t come tonight",
            "Let me sleep until the morning light",
        ],
    },
    KaraokeSong {
        id: "monday",
        mood: SongMood::Major,
        beat_secs: 0.38,
        base_midi: 55,
        title_vi: "Thứ Hai đầu tuần",
        title_en: "Monday Morning",
        lyrics_vi: [
            "Sáng thứ Hai em mở máy lên",
            "Ly cà phê sữa đá trên bàn",
            "Test đỏ rồi xanh xanh rồi lại đỏ",
            "Sửa hoài không xong em vẫn cười",
            "Commit một cái push lên thật nhanh",
            "Tuần này chắc chắn sẽ thật vui",
        ],
        lyrics_en: [
            "Monday morning, I switch on my screen",
            "Iced milk coffee on my desk",
            "Tests go red, then green, then red again",
            "Still not fixed, but still I smile",
            "Commit it now and push it fast",
            "This week is going to be fun",
        ],
    },
    KaraokeSong {
        id: "friday",
        mood: SongMood::Major,
        beat_secs: 0.40,
        base_midi: 57,
        title_vi: "Chiều thứ Sáu",
        title_en: "Friday Afternoon",
        lyrics_vi: [
            "Chiều thứ Sáu nắng vàng ngoài phố",
            "Trà sữa về rồi cả phòng ơi",
            "Bánh tráng trộn cay cay ngon quá",
            "Ngồi bên nhau kể chuyện tuần qua",
            "Mai cuối tuần mình đi chơi nhé",
            "Code để thứ Hai tính sau",
        ],
        lyrics_en: [
            "Friday afternoon, the sun is gold outside",
            "Milk tea is here, come on, everyone",
            "Spicy rice paper salad, so good",
            "Sitting close, we talk about the week",
            "The weekend’s here, let’s go out and play",
            "The code can wait till Monday comes",
        ],
    },
    KaraokeSong {
        id: "limit",
        mood: SongMood::Minor,
        beat_secs: 0.48,
        base_midi: 52,
        title_vi: "Hết lượt Claude",
        title_en: "Out of Claude",
        lyrics_vi: [
            "Năm tiếng trôi qua Claude đã mệt rồi",
            "Em ra pantry ngồi nhìn đồng hồ",
            "Đếm từng phút một mong lượt quay về",
            "Ly trà đã cạn mà vẫn chưa xong",
            "Bao giờ đặt lại mình lại cùng nhau",
            "Viết tiếp những dòng code còn dang dở",
        ],
        lyrics_en: [
            "Five hours gone, and Claude is tired now",
            "I sit in the pantry watching the clock",
            "Counting every minute till it comes back",
            "My tea is gone, the work’s not done",
            "When it resets, we’ll be back together",
            "Writing the lines of code we left behind",
        ],
    },
];

/// Deterministic Vietnamese karaoke machine score (`karaoke.js:111`: 78..=100, weighted to 90s).
pub fn karaoke_score(seed: u32) -> u8 {
    let h = seed.wrapping_mul(2_654_435_761) ^ (seed >> 13);
    let u = (h % 10_000) as f32 / 10_000.0;
    (78.0 + u.sqrt() * 22.0).round().clamp(78.0, 100.0) as u8
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum BilliardsStage {
    #[default]
    Idle,
    Gather,
    Playing,
    Over,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum KaraokeStage {
    #[default]
    Idle,
    WalkIn,
    Singing,
    Scored,
}

#[derive(Resource, Debug, Default, Clone)]
pub struct OfficeGamesState {
    pub billiards_stage: BilliardsStage,
    pub billiards_score: [u8; 2],
    pub billiards_shots: u32,
    pub billiards_turn: u8,
    pub billiards_shot_timer: f32,
    pub foosball_active: bool,
    pub foosball_score: [u8; 2],
    pub foosball_rally_ticks: u32,
    pub foosball_goal_timer: f32,
    pub gaming_active: bool,
    pub gaming_title_index: usize,
    pub gaming_score: [u8; 2],
    pub gaming_timer: f32,
    pub karaoke_stage: KaraokeStage,
    pub karaoke_song_index: usize,
    pub karaoke_last_score: Option<u8>,
    pub karaoke_songs_sung: u32,
    pub karaoke_elapsed: f32,
}

const BIL_X: f32 = 12.85;
const BIL_Z: f32 = 6.20;
const BIL_SPOTS: [[f32; 2]; 2] = [[12.35, 4.92], [11.78, 6.20]];
const FOOS_SPOTS: [(Vec3, f32); 2] = [
    (Vec3::new(10.35, 0.0, 6.28), 0.0),
    (Vec3::new(10.35, 0.0, 7.42), PI),
];
const SOFA_SPOTS: [Vec3; 2] = [Vec3::new(10.5, 0.0, 3.8), Vec3::new(11.5, 0.0, 3.8)];
const KTV_SPOTS: [Vec3; 2] = [Vec3::new(-0.75, 0.0, 7.4), Vec3::new(-0.75, 0.0, 7.12)];
const BIL_TARGET: u8 = 5;
const FOOS_TARGET: u8 = 5;

fn send_home(
    nav: &NavGraph,
    chibi: &ChibiWorker,
    walker: &mut NavWalker,
    routine: &mut ActiveRoutine,
    recalled: bool,
) {
    routine.returning = true;
    routine.desired_prop = None;
    let arrive_anim = if recalled {
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
        arrive_anim,
    );
}

/// Steps Billiards, Foosball, Sofa Console Gaming, and Karaoke Booth routines over `NavGraph`
/// with lazy `set_active_prop` and zero per-frame heap allocations.
#[allow(clippy::too_many_arguments, clippy::type_complexity)]
pub fn step_office_games_system(
    time: Res<Time>,
    mut commands: Commands,
    nav: Option<Res<NavGraph>>,
    clock: Option<Res<CastClock>>,
    mut events: Option<ResMut<CastEvents>>,
    mut coord: Option<ResMut<RoutineCoordinator>>,
    mut state: Option<ResMut<OfficeGamesState>>,
    chibi_cache: Option<Res<ChibiMeshCache>>,
    mut palette: Option<ResMut<WorldPalette>>,
    mut materials: Option<ResMut<Assets<StandardMaterial>>>,
    mut workers_q: Query<
        (
            Entity,
            &LiveSessionWorker,
            &mut ChibiWorker,
            &mut NavWalker,
            Option<&mut ActiveRoutine>,
            Option<&ActivePropEntity>,
        ),
        (Without<crate::cast::OfficeStaffNpc>, Without<SubagentWorker>),
    >,
    mut bil_tbl_q: Query<&mut BilliardsTable>,
    mut bil_balls_q: Query<&mut BilliardsBall>,
    mut ktv_q: Query<&mut KaraokeBooth>,
) {
    let (Some(nav), Some(clock), Some(events), Some(coord), Some(state), Some(cache), Some(pal), Some(mats)) = (
        nav.as_deref(),
        clock.as_deref(),
        events.as_deref_mut(),
        coord.as_deref_mut(),
        state.as_deref_mut(),
        chibi_cache.as_deref(),
        palette.as_deref_mut(),
        materials.as_deref_mut(),
    ) else {
        return;
    };

    if !nav.layout().is_pro() {
        return;
    }

    let dt = time.delta_secs().clamp(0.0, 0.1);
    let m = clock.minutes;

    // Pass 1: Step existing office-game participants & collect per-game readiness on stack.
    let mut bil_total = 0u8;
    let mut bil_arrived = 0u8;
    let mut bil_recalled = false;
    let mut bil_done = false;

    let mut foos_total = 0u8;
    let mut foos_arrived = 0u8;
    let mut foos_recalled = false;
    let mut foos_done = false;

    let mut game_total = 0u8;
    let mut game_arrived = 0u8;
    let mut game_recalled = false;
    let mut game_done = false;

    let mut ktv_total = 0u8;
    let mut ktv_arrived = 0u8;
    let mut ktv_walking = false;
    let mut ktv_recalled = false;
    let mut ktv_done = false;

    for (ent, live, mut chibi, walker, routine_opt, active_prop) in &mut workers_q {
        let Some(mut routine) = routine_opt else {
            continue;
        };
        if !matches!(
            routine.kind,
            RoutineKind::Billiards
                | RoutineKind::Foosball
                | RoutineKind::Gaming
                | RoutineKind::Karaoke
        ) {
            continue;
        }

        if routine.returning {
            sync_lazy_prop(&mut commands, cache, pal, mats, ent, active_prop, None);
            if !walker.is_walking() {
                commands.entity(ent).remove::<ActiveRoutine>();
            }
            continue;
        }

        // Dwell ONLY counts down after arrival (!walker.is_walking()).
        if !walker.is_walking() && routine.dwell_secs > 0.0 {
            routine.dwell_secs = (routine.dwell_secs - dt).max(0.0);
        }

        let recalled = is_worker_recalled(live);
        match routine.kind {
            RoutineKind::Billiards => {
                bil_total += 1;
                if recalled {
                    bil_recalled = true;
                }
                if !walker.is_walking() {
                    bil_arrived += 1;
                    if routine.dwell_secs <= 0.0 {
                        bil_done = true;
                    }
                }
            }
            RoutineKind::Foosball => {
                foos_total += 1;
                if recalled {
                    foos_recalled = true;
                }
                if !walker.is_walking() {
                    foos_arrived += 1;
                    if routine.dwell_secs <= 0.0 {
                        foos_done = true;
                    }
                }
            }
            RoutineKind::Gaming => {
                game_total += 1;
                if recalled {
                    game_recalled = true;
                }
                if !walker.is_walking() {
                    game_arrived += 1;
                    if routine.dwell_secs <= 0.0 {
                        game_done = true;
                    }
                }
            }
            RoutineKind::Karaoke => {
                ktv_total += 1;
                if recalled {
                    ktv_recalled = true;
                }
                if walker.is_walking() {
                    ktv_walking = true;
                } else {
                    ktv_arrived += 1;
                    if state.karaoke_stage == KaraokeStage::Singing {
                        chibi.anim = WorkerAnim::Sing;
                    }
                    if routine.dwell_secs <= 0.0 && state.karaoke_stage == KaraokeStage::Scored {
                        ktv_done = true;
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

    // Advance Billiards match state
    let mut end_bil = false;
    if bil_total == 0 {
        state.billiards_stage = BilliardsStage::Idle;
    } else if bil_recalled || bil_done || bil_total < 2 {
        end_bil = true;
        state.billiards_stage = BilliardsStage::Over;
    } else if state.billiards_stage == BilliardsStage::Gather && bil_arrived == 2 {
        state.billiards_stage = BilliardsStage::Playing;
        state.billiards_shot_timer = 0.0;
    } else if state.billiards_stage == BilliardsStage::Playing && bil_arrived == 2 {
        state.billiards_shot_timer -= dt;
        if state.billiards_shot_timer <= 0.0 {
            state.billiards_shot_timer = 1.6;
            state.billiards_shots += 1;
            let turn = (state.billiards_turn & 1) as usize;
            if state.billiards_shots % 2 == 1 {
                state.billiards_score[turn] = state.billiards_score[turn].saturating_add(1);
            } else {
                state.billiards_turn = 1 - state.billiards_turn;
            }
            // Trigger live cue-ball physics break on the 3D BilliardsTable
            if let Ok(mut tbl) = bil_tbl_q.single_mut() {
                tbl.active = true;
            }
            for mut ball in &mut bil_balls_q {
                if ball.number == 0 {
                    let sign = if state.billiards_turn == 0 { 1.0 } else { -1.0 };
                    ball.vel = Vec2::new(0.12 * sign, -2.3);
                    break;
                }
            }
            if state.billiards_score[0] >= BIL_TARGET || state.billiards_score[1] >= BIL_TARGET {
                end_bil = true;
                state.billiards_stage = BilliardsStage::Over;
            }
        }
    }

    // Advance Foosball rally state
    let mut end_foos = false;
    if foos_total == 0 {
        state.foosball_active = false;
    } else if foos_recalled || foos_done || foos_total < 2 {
        end_foos = true;
        state.foosball_active = false;
    } else if foos_arrived == 2 {
        state.foosball_rally_ticks += 1;
        state.foosball_goal_timer += dt;
        if state.foosball_goal_timer >= 2.2 {
            state.foosball_goal_timer = 0.0;
            let side = (state.foosball_rally_ticks as usize) & 1;
            state.foosball_score[side] = state.foosball_score[side].saturating_add(1);
            if state.foosball_score[0] >= FOOS_TARGET || state.foosball_score[1] >= FOOS_TARGET {
                end_foos = true;
                state.foosball_active = false;
            }
        }
    }

    // Advance Sofa Gaming state
    let mut end_game = false;
    if game_total == 0 {
        state.gaming_active = false;
    } else if game_recalled || game_done {
        end_game = true;
        state.gaming_active = false;
    } else if game_arrived == game_total {
        state.gaming_timer += dt;
        if state.gaming_timer >= 2.5 {
            state.gaming_timer = 0.0;
            let side = (state.gaming_score[0] as usize + state.gaming_score[1] as usize) & 1;
            state.gaming_score[side] = state.gaming_score[side].saturating_add(1);
        }
    }

    // Advance Karaoke Booth state + glass door
    let mut end_ktv = false;
    if ktv_total == 0 {
        state.karaoke_stage = KaraokeStage::Idle;
    } else if ktv_recalled || ktv_done {
        end_ktv = true;
        state.karaoke_stage = KaraokeStage::Idle;
    } else if state.karaoke_stage == KaraokeStage::WalkIn && ktv_arrived == ktv_total {
        state.karaoke_stage = KaraokeStage::Singing;
        state.karaoke_elapsed = 0.0;
    } else if state.karaoke_stage == KaraokeStage::Singing {
        state.karaoke_elapsed += dt;
        if state.karaoke_elapsed >= 4.0 {
            let seed = clock
                .day_index
                .wrapping_mul(97)
                .wrapping_add(state.karaoke_songs_sung.wrapping_mul(31))
                .wrapping_add(state.karaoke_song_index as u32 + 7);
            state.karaoke_last_score = Some(karaoke_score(seed));
            state.karaoke_songs_sung += 1;
            state.karaoke_stage = KaraokeStage::Scored;
            state.karaoke_elapsed = 0.0;
        }
    } else if state.karaoke_stage == KaraokeStage::Scored {
        state.karaoke_elapsed += dt;
        if state.karaoke_elapsed >= 2.0 {
            end_ktv = true;
            state.karaoke_stage = KaraokeStage::Idle;
        }
    }
    for mut booth in &mut ktv_q {
        booth.target_open = if ktv_walking || end_ktv { 1.15 } else { 0.0 };
    }

    // Pass 2: Send finished / recalled participants back to their desks.
    if end_bil || end_foos || end_game || end_ktv {
        for (ent, live, chibi, mut walker, routine_opt, active_prop) in &mut workers_q {
            let Some(mut routine) = routine_opt else {
                continue;
            };
            if routine.returning {
                continue;
            }
            let should_end = match routine.kind {
                RoutineKind::Billiards => end_bil,
                RoutineKind::Foosball => end_foos,
                RoutineKind::Gaming => end_game,
                RoutineKind::Karaoke => end_ktv,
                _ => false,
            };
            if should_end {
                send_home(nav, &chibi, &mut walker, &mut routine, is_worker_recalled(live));
                sync_lazy_prop(&mut commands, cache, pal, mats, ent, active_prop, None);
            }
        }
    }

    // Pass 3: Collect free resting workers into a fixed stack buffer (zero heap allocations).
    let mut free_buf: [Option<(Entity, Vec3, f32, bool)>; 16] = [None; 16];
    let mut free_len = 0usize;
    for (ent, live, chibi, walker, routine_opt, _) in &workers_q {
        if free_len >= free_buf.len() {
            break;
        }
        if routine_opt.is_none()
            && is_worker_resting(live)
            && !walker.is_walking()
            && !coord.is_claimed(ent)
        {
            let (hx, hy, hs) = nav
                .desk_spot(live.desk_index)
                .map(|d| (d.pos, d.face, d.sit))
                .unwrap_or((chibi.base_pos, chibi.base_yaw, chibi.sitting));
            free_buf[free_len] = Some((ent, hx, hy, hs));
            free_len += 1;
        }
    }
    let mut next_free = 0usize;

    // 1. Start Billiards (2 players with lazy PropKind::Cue)
    let want_bil = events.force_billiards;
    if state.billiards_stage == BilliardsStage::Idle
        && want_bil
        && free_len.saturating_sub(next_free) >= 2
    {
        events.force_billiards = false;
        state.billiards_stage = BilliardsStage::Gather;
        state.billiards_score = [0, 0];
        state.billiards_shots = 0;
        state.billiards_turn = 0;
        state.billiards_shot_timer = 0.0;
        for slot in 0..2 {
            let Some((ent, home_pos, home_yaw, home_sit)) = free_buf[next_free] else {
                break;
            };
            next_free += 1;
            if !coord.claim(ent) {
                continue;
            }
            let [tx, tz] = BIL_SPOTS[slot];
            let target = Vec3::new(tx, 0.0, tz);
            let face = face_to(tx, tz, BIL_X, BIL_Z);
            if let Ok((_, _, chibi, mut walker, _, active_prop)) = workers_q.get_mut(ent) {
                walker.set_destination(nav, chibi.base_pos, target, face, false, WorkerAnim::Rest);
                commands.entity(ent).insert(ActiveRoutine::new(
                    RoutineKind::Billiards,
                    slot as u8,
                    30.0,
                    home_pos,
                    home_yaw,
                    home_sit,
                    Some(PropKind::Cue),
                ));
                sync_lazy_prop(
                    &mut commands,
                    cache,
                    pal,
                    mats,
                    ent,
                    active_prop,
                    Some(PropKind::Cue),
                );
            }
        }
    }

    // 2. Start Foosball (2 players on opposite table sides)
    let want_foos = events.force_foosball;
    if !state.foosball_active && want_foos && free_len.saturating_sub(next_free) >= 2 {
        events.force_foosball = false;
        state.foosball_active = true;
        state.foosball_score = [0, 0];
        state.foosball_rally_ticks = 0;
        state.foosball_goal_timer = 0.0;
        for slot in 0..2 {
            let Some((ent, home_pos, home_yaw, home_sit)) = free_buf[next_free] else {
                break;
            };
            next_free += 1;
            if !coord.claim(ent) {
                continue;
            }
            let (target, face) = FOOS_SPOTS[slot];
            if let Ok((_, _, chibi, mut walker, _, _)) = workers_q.get_mut(ent) {
                walker.set_destination(nav, chibi.base_pos, target, face, false, WorkerAnim::Work);
                commands.entity(ent).insert(ActiveRoutine::new(
                    RoutineKind::Foosball,
                    slot as u8,
                    25.0,
                    home_pos,
                    home_yaw,
                    home_sit,
                    None,
                ));
            }
        }
    }

    // 3. Start Sofa Console Gaming (1–2 players holding lazy PropKind::Gamepad)
    let want_game = events.force_gaming;
    if !state.gaming_active && want_game && free_len.saturating_sub(next_free) >= 1 {
        events.force_gaming = false;
        state.gaming_active = true;
        state.gaming_score = [0, 0];
        state.gaming_timer = 0.0;
        state.gaming_title_index = (state.gaming_title_index + 1) % 3;
        let seats = free_len.saturating_sub(next_free).min(2);
        for slot in 0..seats {
            let Some((ent, home_pos, home_yaw, home_sit)) = free_buf[next_free] else {
                break;
            };
            next_free += 1;
            if !coord.claim(ent) {
                continue;
            }
            let target = SOFA_SPOTS[slot];
            if let Ok((_, _, chibi, mut walker, _, active_prop)) = workers_q.get_mut(ent) {
                walker.set_destination(nav, chibi.base_pos, target, PI, true, WorkerAnim::Rest);
                commands.entity(ent).insert(ActiveRoutine::new(
                    RoutineKind::Gaming,
                    slot as u8,
                    24.0,
                    home_pos,
                    home_yaw,
                    home_sit,
                    Some(PropKind::Gamepad),
                ));
                sync_lazy_prop(
                    &mut commands,
                    cache,
                    pal,
                    mats,
                    ent,
                    active_prop,
                    Some(PropKind::Gamepad),
                );
            }
        }
    }

    // 4. Start Karaoke Booth (1–2 singers holding lazy PropKind::Mic + authentic track list)
    let friday_karaoke = clock.is_friday() && m >= 16 * 60 + 30 && m < 21 * 60 && state.karaoke_songs_sung == 0;
    let want_ktv = events.force_karaoke || friday_karaoke;
    if state.karaoke_stage == KaraokeStage::Idle
        && want_ktv
        && free_len.saturating_sub(next_free) >= 1
    {
        events.force_karaoke = false;
        state.karaoke_stage = KaraokeStage::WalkIn;
        state.karaoke_elapsed = 0.0;
        state.karaoke_song_index = if clock.is_friday() {
            2 // "Chiều thứ Sáu"
        } else if clock.is_monday() {
            1 // "Thứ Hai đầu tuần"
        } else {
            (state.karaoke_songs_sung as usize) % KARAOKE_SONGS.len()
        };
        let singers = free_len.saturating_sub(next_free).min(2);
        for slot in 0..singers {
            let Some((ent, home_pos, home_yaw, home_sit)) = free_buf[next_free] else {
                break;
            };
            next_free += 1;
            if !coord.claim(ent) {
                continue;
            }
            let target = KTV_SPOTS[slot];
            if let Ok((_, _, chibi, mut walker, _, active_prop)) = workers_q.get_mut(ent) {
                walker.set_destination(
                    nav,
                    chibi.base_pos,
                    target,
                    -FRAC_PI_2,
                    false,
                    WorkerAnim::Sing,
                );
                commands.entity(ent).insert(ActiveRoutine::new(
                    RoutineKind::Karaoke,
                    slot as u8,
                    18.0,
                    home_pos,
                    home_yaw,
                    home_sit,
                    Some(PropKind::Mic),
                ));
                sync_lazy_prop(
                    &mut commands,
                    cache,
                    pal,
                    mats,
                    ent,
                    active_prop,
                    Some(PropKind::Mic),
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
    use crate::chibi_props::PropKind;
    use crate::layout::OfficeLayout;
    use crate::palette::WorldEntity;
    use crate::people::{LiveWorkerSpec, step_nav_walkers_system, sync_live_workers_in_world};
    use crate::{
        BackdropAssets, OfficeIbl, SharedFontEngine, spawn_full_office_world,
        unload_full_office_world,
    };
    use std::path::PathBuf;
    use std::time::Duration;

    fn build_pro_world_with_resting_workers(count: usize) -> World {
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
        world.insert_resource(OfficeGamesState::default());

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

        let specs: Vec<LiveWorkerSpec> = (0..count)
            .map(|i| LiveWorkerSpec {
                session_id: format!("rest-worker-{i}"),
                agent: if i % 2 == 0 { "claude" } else { "codex" }.to_owned(),
                name: format!("Dev {i}"),
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
    fn billiards_match_walks_two_workers_with_cues_and_triggers_shots() {
        let mut world = build_pro_world_with_resting_workers(4);
        let mut sched = Schedule::default();
        sched.add_systems(
            (
                begin_cast_tick_system,
                step_office_games_system,
                step_nav_walkers_system,
            )
                .chain(),
        );

        world.resource_mut::<CastClock>().minutes = 14 * 60;
        world.resource_mut::<CastEvents>().force_billiards = true;
        tick(&mut world, &mut sched, 1, 50);

        let players: Vec<(Entity, ActiveRoutine, Option<PropKind>)> = world
            .query::<(Entity, &ActiveRoutine, Option<&ActivePropEntity>)>()
            .iter(&world)
            .filter(|(_, r, _)| r.kind == RoutineKind::Billiards)
            .map(|(e, r, p)| (e, *r, p.map(|x| x.kind)))
            .collect();
        assert_eq!(players.len(), 2, "Billiards must claim exactly 2 resting workers");
        assert!(
            players.iter().all(|(_, _, p)| *p == Some(PropKind::Cue)),
            "Both billiards players must hold lazy PropKind::Cue"
        );

        // Advance until they arrive and take at least one shot.
        tick(&mut world, &mut sched, 260, 100);
        let st = world.resource::<OfficeGamesState>();
        assert!(
            st.billiards_shots >= 1,
            "Billiards match must trigger pool shots after players arrive; shots={}",
            st.billiards_shots
        );
    }

    #[test]
    fn foosball_and_gaming_and_karaoke_run_cleanly_without_mutual_conflict_and_zero_leaks() {
        let mut world = build_pro_world_with_resting_workers(8);
        let mut sched = Schedule::default();
        sched.add_systems(
            (
                begin_cast_tick_system,
                step_office_games_system,
                step_nav_walkers_system,
            )
                .chain(),
        );

        {
            let mut ev = world.resource_mut::<CastEvents>();
            ev.force_billiards = true;
            ev.force_foosball = true;
            ev.force_gaming = true;
            ev.force_karaoke = true;
        }
        tick(&mut world, &mut sched, 1, 50);

        // Verify all 4 games started on disjoint workers (zero double-booking).
        let mut bil = 0;
        let mut foos = 0;
        let mut game = 0;
        let mut ktv = 0;
        for (r, prop) in world
            .query::<(&ActiveRoutine, Option<&ActivePropEntity>)>()
            .iter(&world)
        {
            match r.kind {
                RoutineKind::Billiards => {
                    assert_eq!(prop.map(|p| p.kind), Some(PropKind::Cue));
                    bil += 1;
                }
                RoutineKind::Foosball => foos += 1,
                RoutineKind::Gaming => {
                    assert_eq!(prop.map(|p| p.kind), Some(PropKind::Gamepad));
                    game += 1;
                }
                RoutineKind::Karaoke => {
                    assert_eq!(prop.map(|p| p.kind), Some(PropKind::Mic));
                    ktv += 1;
                }
                _ => {}
            }
        }
        assert_eq!(bil, 2, "Billiards should claim 2 workers");
        assert_eq!(foos, 2, "Foosball should claim 2 workers");
        assert!(game >= 1, "Gaming should claim at least 1 worker");
        assert!(ktv >= 1, "Karaoke should claim at least 1 worker");

        // Advance long enough for karaoke to sing & score (78..=100) and foosball rally to tick.
        tick(&mut world, &mut sched, 320, 100);
        let st = world.resource::<OfficeGamesState>();
        assert!(st.foosball_rally_ticks > 0, "Foosball must advance rally ticks");
        let score = st
            .karaoke_last_score
            .expect("Karaoke must produce a 0..=100 score");
        assert!(
            (78..=100).contains(&score),
            "Karaoke score must be in authentic 78..=100 range, got {score}"
        );
        assert_eq!(KARAOKE_SONGS.len(), 4);
        assert_eq!(KARAOKE_SONGS[0].title_vi, "Deploy lúc nửa đêm");

        // Unload full office world -> 0 leaked entities, meshes, materials, or images.
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
