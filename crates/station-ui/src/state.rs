use crate::icons::IconKind;
use crate::labels3d::ProjectedLabel;
use bevy::prelude::*;
use bevy_egui::egui;
use serde::{Deserialize, Serialize};
use station_3d::{BackdropKind, CameraPreset, ChibiLook, OfficeLayout};
use station_launcher::HarnessStatus;

/// Logical `egui::Rect` of the transparent central 3D stage area, updated each frame by the shell.
#[derive(Resource, Debug, Clone, Copy, Default, PartialEq)]
pub struct StageViewportRect(pub Option<egui::Rect>);

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
pub enum ViewMode {
    #[default]
    Mode3D,
    Lite2D,
    PanelCompact,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ActiveDrawer {
    None,
    Session,
    System,
    Burn,
    Vault,
    GroupChat,
    Settings,
    Timeline,
    Wrapped,
    PhoneQr,
    Whiteboard,
    AddHarness,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum GpuPrefUi {
    #[default]
    Auto,
    Integrated,
    Discrete,
}

impl GpuPrefUi {
    pub fn label(self) -> &'static str {
        match self {
            Self::Auto => "Auto",
            Self::Integrated => "Integrated",
            Self::Discrete => "Discrete",
        }
    }

    pub fn as_pref_str(self) -> &'static str {
        match self {
            Self::Auto => "auto",
            Self::Integrated => "integrated",
            Self::Discrete => "discrete",
        }
    }

    pub fn from_pref_str(raw: &str) -> Self {
        match raw.trim().to_ascii_lowercase().as_str() {
            "integrated" | "low" | "low_power" => Self::Integrated,
            "discrete" | "high" | "high_performance" => Self::Discrete,
            _ => Self::Auto,
        }
    }
}

/// Normalizes session status strings to canonical `"work"` / `"wait"` / `"rest"`.
pub fn normalize_status(raw: &str) -> &'static str {
    match raw.trim().to_ascii_lowercase().as_str() {
        "work" | "working" | "active" => "work",
        "wait" | "waiting" | "perm" => "wait",
        _ => "rest",
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TopBarPopover {
    Apps,
    Plan,
    Settings,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SubagentItem {
    pub id: String,
    pub label: String,
    pub doing: String,
}

/// Normalizes feed item kinds to canonical `"cmd" | "edit" | "read" | "web" | "agent" | "info"`.
pub fn normalize_feed_kind(raw: &str) -> &'static str {
    match raw.trim().to_ascii_lowercase().as_str() {
        "cmd" | "bash" | "shell" | "run" | "test" | "command" => "cmd",
        "edit" | "write" | "patch" => "edit",
        "read" | "grep" | "glob" | "search" => "read",
        "web" | "fetch" | "browse" => "web",
        "agent" | "subagent" | "task" => "agent",
        _ => "info",
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum CacheWarmth {
    #[default]
    Unknown,
    Warm(u32),
    Cold,
}

#[derive(Debug, Clone, Default)]
pub struct SessionCard {
    pub id: String,
    pub agent: String,
    pub project: String,
    pub dir: String,
    pub git_branch: Option<String>,
    pub git_dirty: usize,
    pub cache_warmth: CacheWarmth,
    pub todos_done: usize,
    pub todos_total: usize,
    pub status: String,
    pub model: String,
    pub last_prompt: String,
    pub last_reply: String,
    pub tool: Option<String>,
    pub tool_kind: Option<String>,
    pub desk: usize,
    pub cost_usd: f64,
    pub tokens: u64,
    pub pinned: bool,
    pub age_label: String,
    pub ctx_pct: f32,
    pub subagents: Vec<SubagentItem>,
}

#[derive(Debug, Clone)]
pub struct PendingPermCard {
    pub id: String,
    pub session_id: String,
    pub agent: String,
    pub tool: String,
    pub summary: String,
    pub kind: String, // "permission" | "denied" | "ci"
}

#[derive(Debug, Clone)]
pub struct HarnessDockItem {
    pub id: String,
    pub label: String,
    pub status: HarnessStatus,
    pub custom: bool,
    pub hook_agent: Option<String>,
}

#[derive(Debug, Clone, Default)]
pub struct SysSnapshot {
    pub cpu_pct: f32,
    pub mem_used_gb: f32,
    pub mem_total_gb: f32,
    pub disk_pct: f32,
    pub ports: Vec<(u16, String)>,
    pub private_mb: f32,
}

#[derive(Debug, Clone, Default)]
pub struct BurnSnapshot {
    pub today_usd: f64,
    pub month_usd: f64,
    pub input_tokens: u64,
    pub output_tokens: u64,
    pub cache_read_tokens: u64,
    pub per_model: Vec<(String, f64)>,
}

#[derive(Debug, Clone, Default)]
pub struct FeedEntry {
    pub ts_label: String,
    pub agent: String,
    pub message: String,
    pub level: String, // "ok" | "warn" | "err" | "info"
    pub kind: String,  // "cmd" | "edit" | "read" | "web" | "agent" | "info"
    pub attention: bool,
}

#[derive(Debug, Clone)]
pub struct ChatMessage {
    pub sender: String,
    pub role: String,
    pub text: String,
    pub ts: String,
}

#[derive(Debug, Clone)]
pub struct ToastNotice {
    pub id: u64,
    pub title: String,
    pub body: String,
    pub ttl_sec: f32,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum PhotoSize {
    Screen,
    #[default]
    Fhd,
    Uhd4k,
}

impl PhotoSize {
    pub fn dimensions_tag(self) -> &'static str {
        match self {
            Self::Screen => "screen",
            Self::Fhd => "1920x1080",
            Self::Uhd4k => "3840x2160",
        }
    }

    /// Computes output `(width, height)` preserving source aspect ratio matching `photo.js:83`.
    pub fn target_dimensions(self, src_w: u32, src_h: u32) -> (u32, u32) {
        let sw = src_w.max(1);
        let sh = src_h.max(1);
        match self {
            Self::Screen => (sw, sh),
            Self::Fhd => {
                let w = 1920_u32;
                let h = ((w as f64 * sh as f64) / sw as f64).round().max(1.0) as u32;
                (w, h)
            }
            Self::Uhd4k => {
                let w = 3840_u32;
                let h = ((w as f64 * sh as f64) / sw as f64).round().max(1.0) as u32;
                (w, h)
            }
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum PhotoTimeMode {
    #[default]
    Auto,
    Day,
    Night,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum ClipShape {
    #[default]
    Square,
    Tall,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum ReplayRange {
    #[default]
    H1,
    H3,
    Today,
    Yesterday,
}

#[derive(Debug, Clone, Default)]
pub struct BilliardsHudState {
    pub score_label: String,
    pub turn_label: String,
    /// Power meter in `0.0..=1.0`.
    pub power: f32,
}

#[derive(Debug, Clone)]
pub struct PlaceCardRow {
    pub title: String,
    pub subtitle: String,
    pub session_id: Option<String>,
    pub accent_tone: Option<String>,
}

#[derive(Debug, Clone)]
pub struct PlaceCardAction {
    pub id: String,
    pub label: String,
    pub primary: bool,
}

#[derive(Debug, Clone)]
pub struct PlaceCardInfo {
    pub place_id: String,
    pub icon: IconKind,
    pub title: String,
    pub subtitle: String,
    pub rows: Vec<PlaceCardRow>,
    pub empty_text: String,
    pub footer_note: Option<String>,
    pub actions: Vec<PlaceCardAction>,
}

#[derive(Debug, Clone)]
pub enum UiAction {
    LaunchHarness(String),
    AddCustomHarness {
        id: String,
        label: String,
        path: String,
        hook_agent: Option<String>,
    },
    RemoveCustomHarness(String),
    SetViewMode(ViewMode),
    SwitchLayout(OfficeLayout),
    SwitchBackdrop(BackdropKind),
    ToggleNight(bool),
    SetChibiLook(ChibiLook),
    SetLang(String),
    SetWeatherCity(String),
    SetSound(bool),
    SetGpuPref(GpuPrefUi),
    SelectCameraPreset(CameraPreset),
    ZoomCamera(f32),
    ResetCameraZoom,
    ToggleFullscreenStage,
    SetPhotoMode(bool),
    SetPhotoTimeAuto,
    CapturePhoto {
        size: PhotoSize,
        show_names: bool,
        show_mark: bool,
    },
    TriggerTimelapseClip {
        shape: ClipShape,
        music: bool,
    },
    ToggleCameraFp,
    TriggerFirstPersonInteract,
    TriggerPhoto,
    ToggleDemo,
    ToggleReplay,
    ReplayTogglePlay,
    ReplayScrub(f32),
    ReplaySetSpeed(f32),
    ReplaySetRange(ReplayRange),
    ReplayClose,
    OpenPlaceCard(String),
    ClosePlaceCard,
    TriggerPlaceAction {
        place_id: String,
        action_id: String,
    },
    DecidePermission {
        id: String,
        decision: String,
    },
    SendPrompt {
        session_id: String,
        text: String,
    },
    PostGroupChat(String),
    SaveWhiteboard(String),
    RefreshTelemetry,
    MinimizeToTray,
}

#[derive(Resource, Debug, Clone)]
pub struct UiState {
    pub view_mode: ViewMode,
    pub active_drawer: ActiveDrawer,
    pub selected_session: Option<String>,
    pub layout: OfficeLayout,
    pub backdrop: BackdropKind,
    pub chibi_look: ChibiLook,
    pub night: bool,
    pub streamer_mode: bool,
    pub sound_on: bool,
    pub auto_close_tray: bool,
    pub gpu_pref: GpuPrefUi,
    pub weather_city: String,
    pub weather_temp_c: i32,
    pub weather_desc: String,
    pub clock_label: String,
    pub first_person: bool,
    pub demo_running: bool,
    pub replay_playing: bool,
    pub server_port: u16,
    pub server_banner: Option<String>,
    pub pairing_pin: String,
    pub target_fps: u32,
    pub sessions: Vec<SessionCard>,
    pub pending_perms: Vec<PendingPermCard>,
    pub harnesses: Vec<HarnessDockItem>,
    pub sys: SysSnapshot,
    pub burn: BurnSnapshot,
    pub feed: Vec<FeedEntry>,
    pub chat: Vec<ChatMessage>,
    pub toasts: Vec<ToastNotice>,
    pub whiteboard_text: String,
    pub vault_query: String,
    pub vault_results: Vec<(String, String)>,
    pub compose_draft: String,
    pub chat_draft: String,
    pub new_harness_id: String,
    pub new_harness_label: String,
    pub new_harness_path: String,
    pub fullscreen_stage: bool,
    pub photo_mode: bool,
    pub camera_preset: CameraPreset,
    pub camera_zoom_pct: u32,
    pub projected_labels: Vec<ProjectedLabel>,
    pub show_zone_chips: bool,
    pub hovered_label_id: Option<String>,
    pub photo_size: PhotoSize,
    pub photo_show_names: bool,
    pub photo_show_mark: bool,
    pub photo_time_mode: PhotoTimeMode,
    pub photo_capture_hide_frames: u8,
    pub clip_shape: ClipShape,
    pub clip_music: bool,
    pub replay_active: bool,
    pub replay_progress: f32,
    pub replay_speed: f32,
    pub replay_range: ReplayRange,
    pub replay_time_label: String,
    pub replay_span_label: String,
    pub replay_heatmap: Vec<f32>,
    pub fp_near_label: Option<String>,
    pub fp_billiards: Option<BilliardsHudState>,
    pub active_place_card: Option<PlaceCardInfo>,
    pub session_filter_status: Option<String>,
    pub session_search: String,
    pub lite_filter: Option<String>,
    pub lite_search: String,
    pub lite_focus_search_req: bool,
    pub input_lost_focus_this_frame: bool,
    pub panel_quota_open: bool,
    pub panel_mac_open: bool,
    pub panel_steer_session: Option<String>,
    pub panel_steer_draft: String,
    pub activity_attention_only: bool,
    pub activity_type_filter: Option<String>,
    pub activity_sheet_open: bool,
    pub open_popover: Option<TopBarPopover>,
    pub settings_tab: u8,
    pub plan_tab: u8,
    pub unread_chat: usize,
    pub connected: bool,
    pub kpi_tasks: u64,
    pub kpi_reads: u64,
    pub kpi_edits: u64,
    pub kpi_cmds: u64,
    pub pending_actions: Vec<UiAction>,
    next_toast_id: u64,
}

impl Default for UiState {
    fn default() -> Self {
        Self::new_live(OfficeLayout::Pro, BackdropKind::City, false)
    }
}

impl UiState {
    /// Clean live startup state with zero fake sessions, perms, feed, chat, or vault entries.
    pub fn new_live(layout: OfficeLayout, backdrop: BackdropKind, night: bool) -> Self {
        Self {
            view_mode: ViewMode::Mode3D,
            active_drawer: ActiveDrawer::None,
            selected_session: None,
            layout,
            backdrop,
            chibi_look: ChibiLook::Pixar,
            night,
            streamer_mode: false,
            sound_on: true,
            auto_close_tray: true,
            gpu_pref: GpuPrefUi::Auto,
            weather_city: station_3d::life::weather_city_display("hcm").1,
            weather_temp_c: 31,
            weather_desc: String::new(),
            clock_label: "--:--".to_owned(),
            first_person: false,
            demo_running: false,
            replay_playing: false,
            server_port: 4310,
            server_banner: None,
            pairing_pin: String::new(),
            target_fps: 60,
            sessions: Vec::new(),
            pending_perms: Vec::new(),
            harnesses: Vec::new(),
            sys: SysSnapshot::default(),
            burn: BurnSnapshot::default(),
            feed: Vec::new(),
            chat: Vec::new(),
            toasts: Vec::new(),
            whiteboard_text: String::new(),
            vault_query: String::new(),
            vault_results: Vec::new(),
            compose_draft: String::new(),
            chat_draft: String::new(),
            new_harness_id: String::new(),
            new_harness_label: String::new(),
            new_harness_path: String::new(),
            fullscreen_stage: false,
            photo_mode: false,
            camera_preset: CameraPreset::Home,
            camera_zoom_pct: 100,
            projected_labels: Vec::new(),
            show_zone_chips: true,
            hovered_label_id: None,
            photo_size: PhotoSize::Fhd,
            photo_show_names: true,
            photo_show_mark: true,
            photo_time_mode: PhotoTimeMode::Auto,
            photo_capture_hide_frames: 0,
            clip_shape: ClipShape::Square,
            clip_music: true,
            replay_active: false,
            replay_progress: 0.0,
            replay_speed: 1.0,
            replay_range: ReplayRange::H1,
            replay_time_label: "--:--".to_owned(),
            replay_span_label: String::new(),
            replay_heatmap: Vec::new(),
            fp_near_label: None,
            fp_billiards: None,
            active_place_card: None,
            session_filter_status: None,
            session_search: String::new(),
            lite_filter: None,
            lite_search: String::new(),
            lite_focus_search_req: false,
            input_lost_focus_this_frame: false,
            panel_quota_open: false,
            panel_mac_open: false,
            panel_steer_session: None,
            panel_steer_draft: String::new(),
            activity_attention_only: false,
            activity_type_filter: None,
            activity_sheet_open: false,
            open_popover: None,
            settings_tab: 0,
            plan_tab: 0,
            unread_chat: 0,
            connected: true,
            kpi_tasks: 0,
            kpi_reads: 0,
            kpi_edits: 0,
            kpi_cmds: 0,
            pending_actions: Vec::new(),
            next_toast_id: 1,
        }
    }

    pub fn with_demo_seed(layout: OfficeLayout, backdrop: BackdropKind, night: bool) -> Self {
        Self {
            view_mode: ViewMode::Mode3D,
            active_drawer: ActiveDrawer::None,
            selected_session: Some("sess-claude-1".to_owned()),
            layout,
            backdrop,
            chibi_look: ChibiLook::Pixar,
            night,
            streamer_mode: false,
            sound_on: true,
            auto_close_tray: true,
            gpu_pref: GpuPrefUi::Auto,
            weather_city: "HCMC".to_owned(),
            weather_temp_c: 31,
            weather_desc: "Sunny".to_owned(),
            clock_label: "16:45".to_owned(),
            first_person: false,
            demo_running: true,
            replay_playing: false,
            server_port: 4310,
            server_banner: None,
            pairing_pin: "431088".to_owned(),
            target_fps: 60,
            sessions: vec![
                SessionCard {
                    id: "sess-claude-1".to_owned(),
                    agent: "claude".to_owned(),
                    project: "agents-station".to_owned(),
                    dir: "D:\\mystation\\agents-station".to_owned(),
                    git_branch: Some("main".to_owned()),
                    git_dirty: 2,
                    cache_warmth: CacheWarmth::Warm(18),
                    todos_done: 3,
                    todos_total: 5,
                    status: "work".to_owned(),
                    model: "claude-opus-4-6".to_owned(),
                    last_prompt: "Build native Rust 3-column control-room shell".to_owned(),
                    last_reply: String::new(),
                    tool: Some("Edit crates/station-ui/src/shell/topbar.rs".to_owned()),
                    tool_kind: Some("edit".to_owned()),
                    desk: 0,
                    cost_usd: 1.42,
                    tokens: 84_200,
                    pinned: true,
                    age_label: "2m".to_owned(),
                    ctx_pct: 0.42,
                    subagents: vec![SubagentItem {
                        id: "sub-ui-1".to_owned(),
                        label: "shell-inspector".to_owned(),
                        doing: "cargo test -p station-ui".to_owned(),
                    }],
                },
                SessionCard {
                    id: "sess-codex-2".to_owned(),
                    agent: "codex".to_owned(),
                    project: "station-core".to_owned(),
                    dir: "D:\\mystation\\agents-station".to_owned(),
                    git_branch: Some("feat/otlp".to_owned()),
                    git_dirty: 1,
                    cache_warmth: CacheWarmth::Warm(4),
                    todos_done: 2,
                    todos_total: 3,
                    status: "wait".to_owned(),
                    model: "gpt-5.3-codex".to_owned(),
                    last_prompt: "Verify OTLP /v1/metrics parity".to_owned(),
                    last_reply: String::new(),
                    tool: Some("cargo test -p station-core".to_owned()),
                    tool_kind: Some("bash".to_owned()),
                    desk: 1,
                    cost_usd: 0.68,
                    tokens: 41_900,
                    pinned: false,
                    age_label: "1m".to_owned(),
                    ctx_pct: 0.66,
                    subagents: Vec::new(),
                },
                SessionCard {
                    id: "sess-agy-3".to_owned(),
                    agent: "antigravity".to_owned(),
                    project: "station-3d".to_owned(),
                    dir: "D:\\mystation\\agents-station".to_owned(),
                    git_branch: Some("perf/vram".to_owned()),
                    git_dirty: 0,
                    cache_warmth: CacheWarmth::Warm(24),
                    todos_done: 4,
                    todos_total: 4,
                    status: "work".to_owned(),
                    model: "gemini-3-pro".to_owned(),
                    last_prompt: "Optimize 44-desk HQ VRAM budget".to_owned(),
                    last_reply: String::new(),
                    tool: Some("Inspect WorldPalette".to_owned()),
                    tool_kind: Some("read".to_owned()),
                    desk: 2,
                    cost_usd: 0.34,
                    tokens: 52_100,
                    pinned: false,
                    age_label: "4m".to_owned(),
                    ctx_pct: 0.28,
                    subagents: Vec::new(),
                },
                SessionCard {
                    id: "sess-oc-4".to_owned(),
                    agent: "opencode".to_owned(),
                    project: "station-launcher".to_owned(),
                    dir: "D:\\mystation\\launcher".to_owned(),
                    git_branch: Some("win32-gui".to_owned()),
                    git_dirty: 0,
                    cache_warmth: CacheWarmth::Cold,
                    todos_done: 2,
                    todos_total: 2,
                    status: "rest".to_owned(),
                    model: "claude-sonnet-4-6".to_owned(),
                    last_prompt: "Win32 GUI harness detection".to_owned(),
                    last_reply: "Verified ShellExecuteW focus/launch across 5 harnesses.".to_owned(),
                    tool: None,
                    tool_kind: None,
                    desk: 3,
                    cost_usd: 0.19,
                    tokens: 18_400,
                    pinned: false,
                    age_label: "9m".to_owned(),
                    ctx_pct: 0.14,
                    subagents: Vec::new(),
                },
            ],
            pending_perms: vec![PendingPermCard {
                id: "perm-1".to_owned(),
                session_id: "sess-codex-2".to_owned(),
                agent: "codex".to_owned(),
                tool: "Bash".to_owned(),
                summary: "cargo test -p station-core --no-fail-fast".to_owned(),
                kind: "permission".to_owned(),
            }],
            harnesses: vec![
                HarnessDockItem {
                    id: "claude".to_owned(),
                    label: "Claude".to_owned(),
                    status: HarnessStatus::RunningHooked,
                    custom: false,
                    hook_agent: Some("claude".to_owned()),
                },
                HarnessDockItem {
                    id: "codex".to_owned(),
                    label: "Codex".to_owned(),
                    status: HarnessStatus::InstalledHooked,
                    custom: false,
                    hook_agent: Some("codex".to_owned()),
                },
                HarnessDockItem {
                    id: "opencode".to_owned(),
                    label: "OpenCode".to_owned(),
                    status: HarnessStatus::InstalledHooked,
                    custom: false,
                    hook_agent: Some("opencode".to_owned()),
                },
                HarnessDockItem {
                    id: "zcode".to_owned(),
                    label: "ZCode".to_owned(),
                    status: HarnessStatus::NeedsHook,
                    custom: false,
                    hook_agent: Some("zcode".to_owned()),
                },
                HarnessDockItem {
                    id: "antigravity".to_owned(),
                    label: "Antigravity".to_owned(),
                    status: HarnessStatus::RunningHooked,
                    custom: false,
                    hook_agent: Some("antigravity".to_owned()),
                },
            ],
            sys: SysSnapshot {
                cpu_pct: 14.5,
                mem_used_gb: 4.8,
                mem_total_gb: 8.0,
                disk_pct: 61.0,
                ports: vec![(4310, "agents-station".to_owned())],
                private_mb: 142.0,
            },
            burn: BurnSnapshot {
                today_usd: 2.63,
                month_usd: 41.80,
                input_tokens: 138_400,
                output_tokens: 58_200,
                cache_read_tokens: 612_000,
                per_model: vec![
                    ("claude-opus-4-6".to_owned(), 1.42),
                    ("gpt-5.3-codex".to_owned(), 0.68),
                    ("gemini-3-pro".to_owned(), 0.34),
                    ("claude-sonnet-4-6".to_owned(), 0.19),
                ],
            },
            feed: vec![
                FeedEntry {
                    ts_label: "16:44".to_owned(),
                    agent: "claude".to_owned(),
                    message: "Edit crates/station-ui/src/shell/topbar.rs".to_owned(),
                    level: "ok".to_owned(),
                    kind: "edit".to_owned(),
                    attention: false,
                },
                FeedEntry {
                    ts_label: "16:43".to_owned(),
                    agent: "codex".to_owned(),
                    message: "cargo test -p station-core --no-fail-fast".to_owned(),
                    level: "warn".to_owned(),
                    kind: "cmd".to_owned(),
                    attention: true,
                },
                FeedEntry {
                    ts_label: "16:42".to_owned(),
                    agent: "claude".to_owned(),
                    message: "shell-inspector · cargo test -p station-ui".to_owned(),
                    level: "info".to_owned(),
                    kind: "agent".to_owned(),
                    attention: false,
                },
                FeedEntry {
                    ts_label: "16:41".to_owned(),
                    agent: "antigravity".to_owned(),
                    message: "Read crates/station-3d/src/palette.rs".to_owned(),
                    level: "info".to_owned(),
                    kind: "read".to_owned(),
                    attention: false,
                },
            ],
            chat: vec![
                ChatMessage {
                    sender: "Claude".to_owned(),
                    role: "Architect".to_owned(),
                    text: "Gate P4 passed at 148.5 MB HQ Private WS.".to_owned(),
                    ts: "16:40".to_owned(),
                },
                ChatMessage {
                    sender: "Antigravity".to_owned(),
                    role: "Builder".to_owned(),
                    text: "Starting native HUD, 2D Lite & Tray bridge now.".to_owned(),
                    ts: "16:42".to_owned(),
                },
            ],
            toasts: Vec::new(),
            whiteboard_text: "Sprint Goals:\n- Zero PowerShell Win32 tray\n- 2D Lite <= 110 MB\n- 5 Native GUI Harnesses".to_owned(),
            vault_query: String::new(),
            vault_results: vec![
                ("agents-station".to_owned(), "Native Rust rebuild with 100% parity".to_owned()),
                ("station-launcher".to_owned(), "Direct GUI focus/launch via ShellExecuteW".to_owned()),
            ],
            compose_draft: String::new(),
            chat_draft: String::new(),
            new_harness_id: String::new(),
            new_harness_label: String::new(),
            new_harness_path: String::new(),
            fullscreen_stage: false,
            photo_mode: false,
            camera_preset: CameraPreset::Home,
            camera_zoom_pct: 100,
            projected_labels: Vec::new(),
            show_zone_chips: true,
            hovered_label_id: None,
            photo_size: PhotoSize::Fhd,
            photo_show_names: true,
            photo_show_mark: true,
            photo_time_mode: PhotoTimeMode::Auto,
            photo_capture_hide_frames: 0,
            clip_shape: ClipShape::Square,
            clip_music: true,
            replay_active: false,
            replay_progress: 0.35,
            replay_speed: 2.0,
            replay_range: ReplayRange::H1,
            replay_time_label: "16:22".to_owned(),
            replay_span_label: "15:45 – 16:45".to_owned(),
            replay_heatmap: vec![
                0.15, 0.28, 0.45, 0.80, 0.62, 0.35, 0.90, 0.72, 0.50, 0.25, 0.40, 0.85, 0.95, 0.60,
                0.30, 0.55,
            ],
            fp_near_label: None,
            fp_billiards: None,
            active_place_card: None,
            session_filter_status: None,
            session_search: String::new(),
            lite_filter: None,
            lite_search: String::new(),
            lite_focus_search_req: false,
            input_lost_focus_this_frame: false,
            panel_quota_open: false,
            panel_mac_open: false,
            panel_steer_session: None,
            panel_steer_draft: String::new(),
            activity_attention_only: false,
            activity_type_filter: None,
            activity_sheet_open: false,
            open_popover: None,
            settings_tab: 0,
            plan_tab: 0,
            unread_chat: 1,
            connected: true,
            kpi_tasks: 24,
            kpi_reads: 42,
            kpi_edits: 18,
            kpi_cmds: 11,
            pending_actions: Vec::new(),
            next_toast_id: 1,
        }
    }

    pub fn push_action(&mut self, action: UiAction) {
        self.pending_actions.push(action);
    }

    pub fn drain_actions(&mut self) -> Vec<UiAction> {
        std::mem::take(&mut self.pending_actions)
    }

    pub fn push_toast(&mut self, title: impl Into<String>, body: impl Into<String>) {
        let id = self.next_toast_id;
        self.next_toast_id = self.next_toast_id.saturating_add(1);
        self.toasts.push(ToastNotice {
            id,
            title: title.into(),
            body: body.into(),
            ttl_sec: 5.0,
        });
        if self.toasts.len() > 4 {
            self.toasts.remove(0);
        }
    }

    pub fn tick_toasts(&mut self, dt: f32) {
        for t in &mut self.toasts {
            t.ttl_sec -= dt;
        }
        self.toasts.retain(|t| t.ttl_sec > 0.0);
    }

    pub fn toggle_drawer(&mut self, target: ActiveDrawer) {
        self.active_drawer = if self.active_drawer == target {
            ActiveDrawer::None
        } else {
            target
        };
    }

    pub fn working_count(&self) -> usize {
        self.sessions.iter().filter(|s| s.status == "work").count()
    }

    pub fn waiting_count(&self) -> usize {
        let wait_sessions = self.sessions.iter().filter(|s| s.status == "wait").count();
        let orphan_perms = self
            .pending_perms
            .iter()
            .filter(|p| {
                !self
                    .sessions
                    .iter()
                    .any(|s| s.id == p.session_id && s.status == "wait")
            })
            .count();
        wait_sessions + orphan_perms
    }

    pub fn attention_feed_count(&self) -> usize {
        self.feed
            .iter()
            .filter(|e| e.attention || e.level == "err" || e.level == "warn")
            .count()
    }

    pub fn resting_count(&self) -> usize {
        self.sessions.iter().filter(|s| s.status == "rest").count()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ui_state_toggles_drawers_queues_actions_and_expires_toasts() {
        let mut st = UiState::default();
        assert_eq!(st.active_drawer, ActiveDrawer::None);
        st.toggle_drawer(ActiveDrawer::Settings);
        assert_eq!(st.active_drawer, ActiveDrawer::Settings);
        st.toggle_drawer(ActiveDrawer::Settings);
        assert_eq!(st.active_drawer, ActiveDrawer::None);

        st.push_action(UiAction::LaunchHarness("claude".to_owned()));
        assert_eq!(st.drain_actions().len(), 1);
        assert!(st.drain_actions().is_empty());

        st.push_toast("Harness", "Focused Claude");
        assert_eq!(st.toasts.len(), 1);
        st.tick_toasts(6.0);
        assert!(st.toasts.is_empty());
    }

    #[test]
    fn new_live_starts_empty_and_counts_work_wait_rest_accurately() {
        let mut st = UiState::new_live(OfficeLayout::Pro, BackdropKind::City, false);
        assert!(st.sessions.is_empty(), "live mode must start with 0 fake sessions");
        assert!(st.pending_perms.is_empty(), "live mode must start with 0 fake perms");
        assert!(st.feed.is_empty(), "live mode must start with 0 fake feed entries");
        assert!(st.chat.is_empty(), "live mode must start with 0 fake chat messages");
        assert_eq!(st.working_count(), 0);
        assert_eq!(st.waiting_count(), 0);
        assert_eq!(st.resting_count(), 0);

        st.sessions = vec![
            SessionCard {
                id: "s-work".to_owned(),
                agent: "claude".to_owned(),
                project: "proj".to_owned(),
                status: "work".to_owned(),
                model: "opus".to_owned(),
                last_prompt: "code".to_owned(),
                tool: Some("Edit".to_owned()),
                tool_kind: Some("edit".to_owned()),
                desk: 0,
                cost_usd: 0.1,
                tokens: 100,
                pinned: false,
                ..Default::default()
            },
            SessionCard {
                id: "s-wait".to_owned(),
                agent: "codex".to_owned(),
                project: "proj".to_owned(),
                status: "wait".to_owned(),
                model: "gpt".to_owned(),
                last_prompt: "approve".to_owned(),
                tool: Some("Bash".to_owned()),
                tool_kind: Some("bash".to_owned()),
                desk: 1,
                cost_usd: 0.0,
                tokens: 50,
                pinned: false,
                ..Default::default()
            },
            SessionCard {
                id: "s-rest".to_owned(),
                agent: "opencode".to_owned(),
                project: "proj".to_owned(),
                status: "rest".to_owned(),
                model: "sonnet".to_owned(),
                last_prompt: "done".to_owned(),
                tool: None,
                tool_kind: None,
                desk: 2,
                cost_usd: 0.0,
                tokens: 20,
                pinned: false,
                ..Default::default()
            },
        ];
        assert_eq!(st.working_count(), 1);
        assert_eq!(st.waiting_count(), 1);
        assert_eq!(st.resting_count(), 1);

        let mut demo = UiState::with_demo_seed(OfficeLayout::Pro, BackdropKind::City, false);
        assert_eq!(demo.working_count(), 2);
        assert_eq!(
            demo.waiting_count(),
            1,
            "pending perm for an already-waiting session must not double-count"
        );
        assert_eq!(demo.resting_count(), 1);
        assert_eq!(
            demo.working_count() + demo.waiting_count() + demo.resting_count(),
            demo.sessions.len()
        );
        assert_eq!(demo.attention_feed_count(), 1);

        // Orphaned pending perm (for a non-wait or unknown session) still increments waiting_count.
        demo.pending_perms.push(PendingPermCard {
            id: "perm-orphan".to_owned(),
            session_id: "sess-claude-1".to_owned(),
            agent: "claude".to_owned(),
            tool: "Bash".to_owned(),
            summary: "echo test".to_owned(),
            kind: "permission".to_owned(),
        });
        assert_eq!(demo.waiting_count(), 2);

        assert_eq!(GpuPrefUi::from_pref_str("integrated"), GpuPrefUi::Integrated);
        assert_eq!(GpuPrefUi::Discrete.as_pref_str(), "discrete");
        st.push_action(UiAction::SetGpuPref(GpuPrefUi::Discrete));
        assert_eq!(st.drain_actions().len(), 1);
    }
}
