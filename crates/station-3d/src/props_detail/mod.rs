pub mod cat;
pub mod leisure;
pub mod microwave;
pub mod shredder_cups;

use bevy::prelude::*;

#[derive(Debug, Clone)]
pub struct DeskPropTelemetry {
    pub desk_index: usize,
    pub occupied: bool,
    pub resting: bool,
    pub ctx_pct: f32,
    /// `< 0.0`: no cup, `0.0`: cold cup, `0.01..=1.0`: warm steaming cup.
    pub cache_warmth: f32,
}

#[derive(Debug, Clone)]
pub struct DevServerSpec {
    pub port: Option<u16>,
    pub desk_index: Option<usize>,
    pub alive: bool,
    pub cpu_pct: f32,
    pub label: String,
}

#[derive(Resource, Debug, Default, Clone)]
pub struct PropsLiveTelemetry {
    pub live_synced: bool,
    pub desks: Vec<DeskPropTelemetry>,
    pub dev_servers: Vec<DevServerSpec>,
    pub shred_requested: bool,
    pub print_requested: u32,
}
