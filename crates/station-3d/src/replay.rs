//! Replay timeline scrubber & world-state seeker (`replay.js` + `timeline.js`):
//! Fast-forwards or rewinds deterministic world snapshots (`ReplayWorldSnapshot`) at any
//! timestamp, skips quiet stretches longer than `speed * 3000 ms`, builds per-session timeline
//! spans, and restores live state cleanly on exit.

use bevy::prelude::*;
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ReplayEvent {
    pub ts: u64,
    pub session_id: String,
    pub kind: String,
    pub summary: String,
    #[serde(default)]
    pub agent: String,
    #[serde(default)]
    pub project: String,
    #[serde(default)]
    pub status: String,
    #[serde(default)]
    pub tool: String,
    #[serde(default)]
    pub desk: Option<usize>,
    #[serde(default)]
    pub event_type: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ReplaySessionSnap {
    pub id: String,
    pub agent: String,
    pub project: String,
    pub status: String,
    pub tool: Option<String>,
    pub last_summary: String,
    pub desk: usize,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ReplayWorldSnapshot {
    pub cursor_ms: u64,
    pub visible_events: usize,
    pub sessions: Vec<ReplaySessionSnap>,
    pub commits: usize,
    pub pushes: usize,
    pub fails: usize,
    pub incident_active: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TimelineSpan {
    pub session_id: String,
    pub start_ms: u64,
    pub end_ms: u64,
    pub kind: String,
    pub label: String,
}

#[derive(Resource, Debug, Clone)]
pub struct ReplayState {
    pub active: bool,
    pub playing: bool,
    pub speed: f32,
    pub start_ms: u64,
    pub end_ms: u64,
    pub cursor_ms: u64,
    pub events: Vec<ReplayEvent>,
    pub snapshot: ReplayWorldSnapshot,
    pub needs_live_restore: bool,
}

impl Default for ReplayState {
    fn default() -> Self {
        Self {
            active: false,
            playing: false,
            speed: 4.0,
            start_ms: 0,
            end_ms: 0,
            cursor_ms: 0,
            events: Vec::new(),
            snapshot: ReplayWorldSnapshot::default(),
            needs_live_restore: false,
        }
    }
}

impl ReplayState {
    pub fn load_events(&mut self, mut events: Vec<ReplayEvent>) {
        events.sort_by_key(|e| e.ts);
        self.start_ms = events.first().map(|e| e.ts).unwrap_or(0);
        self.end_ms = events.last().map(|e| e.ts).unwrap_or(self.start_ms);
        self.cursor_ms = self.start_ms;
        self.events = events;
        self.active = !self.events.is_empty();
        self.needs_live_restore = false;
        self.snapshot = self.compute_snapshot_at(self.cursor_ms);
    }

    pub fn step(&mut self, dt_secs: f32) {
        if !self.active || !self.playing || self.end_ms <= self.start_ms {
            return;
        }
        let speed = self.speed.max(0.25);
        // Skip quiet stretches longer than `speed * 3000 ms` (`replay.js:81`)
        if let Some(next_ev) = self.events.iter().find(|e| e.ts > self.cursor_ms) {
            let quiet_threshold = (speed * 3000.0) as u64;
            if next_ev.ts.saturating_sub(self.cursor_ms) > quiet_threshold {
                let lead = (speed * 500.0) as u64;
                self.cursor_ms = next_ev.ts.saturating_sub(lead).max(self.cursor_ms);
            }
        }
        let advance = (dt_secs.max(0.0) * speed * 1000.0) as u64;
        self.cursor_ms = self.cursor_ms.saturating_add(advance).min(self.end_ms);
        if self.cursor_ms >= self.end_ms {
            self.playing = false;
        }
        self.snapshot = self.compute_snapshot_at(self.cursor_ms);
    }

    pub fn scrub(&mut self, fraction: f32) {
        let f = fraction.clamp(0.0, 1.0);
        let span = self.end_ms.saturating_sub(self.start_ms);
        let target = self.start_ms + (span as f32 * f).round() as u64;
        self.seek_to(target);
    }

    pub fn visible_count(&self) -> usize {
        self.events.iter().filter(|e| e.ts <= self.cursor_ms).count()
    }

    /// Deterministically rebuilds the office world state at `target_ms` (forward or rewind).
    pub fn seek_to(&mut self, target_ms: u64) -> ReplayWorldSnapshot {
        self.cursor_ms = target_ms.clamp(self.start_ms, self.end_ms.max(self.start_ms));
        self.snapshot = self.compute_snapshot_at(self.cursor_ms);
        self.snapshot.clone()
    }

    /// Exits replay mode cleanly and flags live-state restoration for the bridge.
    pub fn restore_live(&mut self) {
        self.active = false;
        self.playing = false;
        self.snapshot = ReplayWorldSnapshot::default();
        self.needs_live_restore = true;
    }

    /// Builds per-session activity spans across the loaded replay window (`timeline.js`).
    pub fn build_timeline_spans(&self) -> Vec<TimelineSpan> {
        let mut by_sess: BTreeMap<&str, Vec<&ReplayEvent>> = BTreeMap::new();
        for ev in &self.events {
            if !ev.session_id.is_empty() {
                by_sess.entry(ev.session_id.as_str()).or_default().push(ev);
            }
        }
        let mut spans = Vec::new();
        for (sid, evs) in by_sess {
            for (idx, ev) in evs.iter().enumerate() {
                let end_ms = evs
                    .get(idx + 1)
                    .map(|n| n.ts)
                    .unwrap_or(self.end_ms.max(ev.ts + 1_000));
                let kind = if !ev.event_type.is_empty() {
                    ev.event_type.clone()
                } else {
                    ev.kind.clone()
                };
                spans.push(TimelineSpan {
                    session_id: sid.to_owned(),
                    start_ms: ev.ts,
                    end_ms: end_ms.max(ev.ts + 1),
                    kind,
                    label: ev.summary.clone(),
                });
            }
        }
        spans
    }

    fn compute_snapshot_at(&self, target_ms: u64) -> ReplayWorldSnapshot {
        let mut sessions_map: BTreeMap<String, ReplaySessionSnap> = BTreeMap::new();
        let mut visible_events = 0usize;
        let mut commits = 0usize;
        let mut pushes = 0usize;
        let mut fails = 0usize;
        let mut recent_fails: Vec<u64> = Vec::new();
        let mut incident_active = false;

        for ev in &self.events {
            if ev.ts > target_ms {
                break;
            }
            visible_events += 1;
            let ev_type = if ev.event_type.is_empty() {
                ev.kind.as_str()
            } else {
                ev.event_type.as_str()
            };

            match ev_type {
                "commit" => commits += 1,
                "push" => pushes += 1,
                "fail" => {
                    fails += 1;
                    recent_fails.push(ev.ts);
                    recent_fails.retain(|&t| ev.ts.saturating_sub(t) <= 300_000);
                    if recent_fails.len() >= 3 {
                        incident_active = target_ms.saturating_sub(ev.ts) <= 25_000;
                    }
                }
                _ => {}
            }

            if ev.session_id.is_empty() {
                continue;
            }
            if ev.kind == "gone" || ev_type == "end" {
                sessions_map.remove(&ev.session_id);
                continue;
            }

            let next_desk = sessions_map.len();
            let entry = sessions_map
                .entry(ev.session_id.clone())
                .or_insert_with(|| ReplaySessionSnap {
                    id: ev.session_id.clone(),
                    agent: if ev.agent.is_empty() {
                        "claude".to_owned()
                    } else {
                        ev.agent.clone()
                    },
                    project: if ev.project.is_empty() {
                        ev.session_id.clone()
                    } else {
                        ev.project.clone()
                    },
                    status: "work".to_owned(),
                    tool: None,
                    last_summary: String::new(),
                    desk: ev.desk.unwrap_or(next_desk),
                });

            if !ev.agent.is_empty() {
                entry.agent = ev.agent.clone();
            }
            if !ev.project.is_empty() {
                entry.project = ev.project.clone();
            }
            if let Some(d) = ev.desk {
                entry.desk = d;
            }
            if !ev.status.is_empty() {
                entry.status = ev.status.clone();
            } else {
                entry.status = match ev_type {
                    "stop" | "rest" => "rest".to_owned(),
                    "wait" | "ask" => "wait".to_owned(),
                    _ => "work".to_owned(),
                };
            }
            if !ev.tool.is_empty() {
                entry.tool = Some(ev.tool.clone());
            } else if !ev_type.is_empty() {
                entry.tool = Some(ev_type.to_owned());
            }
            if !ev.summary.is_empty() {
                entry.last_summary = ev.summary.clone();
            }
        }

        ReplayWorldSnapshot {
            cursor_ms: target_ms,
            visible_events,
            sessions: sessions_map.into_values().collect(),
            commits,
            pushes,
            fails,
            incident_active,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn mk_ev(ts: u64, sid: &str, kind: &str, ev_type: &str, status: &str, summary: &str) -> ReplayEvent {
        ReplayEvent {
            ts,
            session_id: sid.into(),
            kind: kind.into(),
            summary: summary.into(),
            agent: "claude".into(),
            project: "shop-api".into(),
            status: status.into(),
            tool: ev_type.into(),
            desk: Some(0),
            event_type: ev_type.into(),
        }
    }

    #[test]
    fn replay_seek_scrub_quiet_skip_snapshot_and_restore_work_authentically() {
        let mut rep = ReplayState::default();
        rep.load_events(vec![
            mk_ev(1_000, "s1", "ev", "prompt", "work", "Fix cart pagination"),
            mk_ev(2_000, "s1", "ev", "edit", "work", "src/cart.ts"),
            mk_ev(3_000, "s1", "ev", "commit", "work", "commit cart fix"),
            mk_ev(60_000, "s1", "ev", "push", "work", "main"),
            mk_ev(61_000, "s1", "ev", "stop", "rest", "Done"),
        ]);
        assert!(rep.active);
        assert_eq!(rep.visible_count(), 1);

        // Seek forward to 3_500ms -> sees 3 events, 1 commit, working session
        let snap1 = rep.seek_to(3_500);
        assert_eq!(snap1.visible_events, 3);
        assert_eq!(snap1.commits, 1);
        assert_eq!(snap1.pushes, 0);
        assert_eq!(snap1.sessions.len(), 1);
        assert_eq!(snap1.sessions[0].status, "work");

        // Quiet stretch skip: speed=4x -> threshold 12s, next event at 60_000ms (> 12s gap)
        rep.playing = true;
        rep.speed = 4.0;
        rep.step(0.1);
        assert!(rep.cursor_ms >= 58_000, "expected quiet stretch skip near 60_000ms, got {}", rep.cursor_ms);

        // Seek to end -> sees push & resting session
        let snap_end = rep.seek_to(65_000);
        assert_eq!(snap_end.visible_events, 5);
        assert_eq!(snap_end.pushes, 1);
        assert_eq!(snap_end.sessions[0].status, "rest");

        // Seek backward to 1_500ms -> rewinds cleanly (0 commits, status = work)
        let snap_back = rep.seek_to(1_500);
        assert_eq!(snap_back.visible_events, 1);
        assert_eq!(snap_back.commits, 0);
        assert_eq!(snap_back.sessions[0].status, "work");

        // Timeline spans built from events
        let spans = rep.build_timeline_spans();
        assert_eq!(spans.len(), 5);

        // Restore live clears replay & flags live restore
        rep.restore_live();
        assert!(!rep.active);
        assert!(!rep.playing);
        assert!(rep.needs_live_restore);
    }
}
