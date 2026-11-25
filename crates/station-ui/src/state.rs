use bevy::prelude::*;
use serde::{Deserialize, Serialize};
use station_3d::{BackdropKind, ChibiLook, OfficeLayout};
use station_launcher::HarnessStatus;

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
}

#[derive(Debug, Clone)]
pub struct SessionCard {
    pub id: String,
    pub agent: String,
    pub project: String,
    pub status: String,
    pub model: String,
    pub last_prompt: String,
    pub tool: Option<String>,
    pub cost_usd: f64,
    pub tokens: u64,
    pub pinned: bool,
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

#[derive(Debug, Clone)]
pub struct FeedEntry {
    pub ts_label: String,
    pub agent: String,
    pub message: String,
    pub level: String, // "ok" | "warn" | "err" | "info"
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
    ToggleCameraFp,
    TriggerPhoto,
    ToggleDemo,
    ToggleReplay,
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
    pub pending_actions: Vec<UiAction>,
    next_toast_id: u64,
}

impl Default for UiState {
    fn default() -> Self {
        Self::with_demo_seed(OfficeLayout::Pro, BackdropKind::City, false)
    }
}

impl UiState {
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
            demo_running: false,
            replay_playing: false,
            server_port: 4310,
            pairing_pin: "431088".to_owned(),
            target_fps: 60,
            sessions: vec![
                SessionCard {
                    id: "sess-claude-1".to_owned(),
                    agent: "claude".to_owned(),
                    project: "agents-station".to_owned(),
                    status: "working".to_owned(),
                    model: "claude-opus-4-6".to_owned(),
                    last_prompt: "Build native Rust HUD & Harness Dock".to_owned(),
                    tool: Some("Edit crates/station-ui/src/hud.rs".to_owned()),
                    cost_usd: 1.42,
                    tokens: 84_200,
                    pinned: true,
                },
                SessionCard {
                    id: "sess-codex-2".to_owned(),
                    agent: "codex".to_owned(),
                    project: "station-core".to_owned(),
                    status: "waiting".to_owned(),
                    model: "gpt-5.3-codex".to_owned(),
                    last_prompt: "Verify OTLP /v1/metrics parity".to_owned(),
                    tool: Some("cargo test -p station-core".to_owned()),
                    cost_usd: 0.68,
                    tokens: 41_900,
                    pinned: false,
                },
                SessionCard {
                    id: "sess-agy-3".to_owned(),
                    agent: "antigravity".to_owned(),
                    project: "station-3d".to_owned(),
                    status: "working".to_owned(),
                    model: "gemini-3-pro".to_owned(),
                    last_prompt: "Optimize 44-desk HQ VRAM budget".to_owned(),
                    tool: Some("Inspect WorldPalette".to_owned()),
                    cost_usd: 0.34,
                    tokens: 52_100,
                    pinned: false,
                },
                SessionCard {
                    id: "sess-oc-4".to_owned(),
                    agent: "opencode".to_owned(),
                    project: "station-launcher".to_owned(),
                    status: "idle".to_owned(),
                    model: "claude-sonnet-4-6".to_owned(),
                    last_prompt: "Win32 GUI harness detection".to_owned(),
                    tool: None,
                    cost_usd: 0.19,
                    tokens: 18_400,
                    pinned: false,
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
                    message: "Compiled station-ui i18n bundles (en/vi/th)".to_owned(),
                    level: "ok".to_owned(),
                },
                FeedEntry {
                    ts_label: "16:43".to_owned(),
                    agent: "codex".to_owned(),
                    message: "Awaiting permission: cargo test -p station-core".to_owned(),
                    level: "warn".to_owned(),
                },
                FeedEntry {
                    ts_label: "16:41".to_owned(),
                    agent: "antigravity".to_owned(),
                    message: "HQ 44-desk RAM verified: 148.5 MB Private".to_owned(),
                    level: "info".to_owned(),
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
        self.sessions.iter().filter(|s| s.status == "working").count()
    }

    pub fn waiting_count(&self) -> usize {
        self.sessions.iter().filter(|s| s.status == "waiting").count()
            + self.pending_perms.len()
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
}
