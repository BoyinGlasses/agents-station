use bevy::prelude::*;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ReplayEvent {
    pub ts: u64,
    pub session_id: String,
    pub kind: String,
    pub summary: String,
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
    }

    pub fn step(&mut self, dt_secs: f32) {
        if !self.active || !self.playing || self.end_ms <= self.start_ms {
            return;
        }
        let advance = (dt_secs.max(0.0) * self.speed.max(0.25) * 1000.0) as u64;
        self.cursor_ms = self.cursor_ms.saturating_add(advance).min(self.end_ms);
        if self.cursor_ms >= self.end_ms {
            self.playing = false;
        }
    }

    pub fn scrub(&mut self, fraction: f32) {
        let f = fraction.clamp(0.0, 1.0);
        let span = self.end_ms.saturating_sub(self.start_ms);
        self.cursor_ms = self.start_ms + (span as f32 * f).round() as u64;
    }

    pub fn visible_count(&self) -> usize {
        self.events.iter().filter(|e| e.ts <= self.cursor_ms).count()
    }
}
