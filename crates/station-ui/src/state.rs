use crate::icons::IconKind;
use crate::labels3d::ProjectedLabel;
use bevy::prelude::*;
use bevy_egui::egui;
use serde::{Deserialize, Serialize};
use serde_json::{Map, Value};
use station_3d::{BackdropKind, CameraPreset, ChibiLook, OfficeLayout};
use station_launcher::HarnessStatus;
use std::collections::{BTreeMap, HashMap, HashSet};

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
    Accounts,
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

#[derive(Debug, Clone, Default)]
pub struct PendingPermCard {
    pub id: String,
    pub session_id: String,
    pub agent: String,
    pub tool: String,
    pub summary: String,
    /// `"perm" | "permission" | "ask" | "plan" | "denied" | "stuck" | "ci"`
    pub kind: String,
    /// Optional danger key (e.g. `"rmRf"`, `"gitPush"`) when a risky command is detected.
    pub danger: Option<String>,
    /// Optional structured tool input JSON (e.g. `AskUserQuestion` `questions[]` or `ExitPlanMode` `plan`).
    pub tool_input: Option<Value>,
    /// Optional extra metadata (`rule`, `reason`, `wf`, `repo`, `branch`, `url`, `stuck_kind`, `stuck_n`, `stuck_mins`, `stuck_usd`).
    pub meta: Option<Value>,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct AskOption {
    pub label: String,
    pub description: String,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct AskQuestion {
    pub header: String,
    pub question: String,
    pub options: Vec<AskOption>,
    pub multi_select: bool,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct AskCardDraft {
    pub picks: Vec<Vec<usize>>,
    pub others: Vec<String>,
}

impl AskCardDraft {
    pub fn ensure_len(&mut self, len: usize) {
        if self.picks.len() < len {
            self.picks.resize_with(len, Vec::new);
        }
        if self.others.len() < len {
            self.others.resize_with(len, String::new);
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum SessionDrawerTab {
    #[default]
    Chat,
    Steps,
    Subagents,
    Pins,
    Undo,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct SessionLogItem {
    pub role: String, // "user" | "assistant" | "tool" | "note" | "summary"
    pub text: String,
    pub ts: String,
    pub tool_type: Option<String>,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct UndoFileEntry {
    pub file: String,
    pub action: String, // "restore" | "delete" | "same" | "changedAfter" | "big"
    pub selected: bool,
}

impl UndoFileEntry {
    pub fn is_undoable(&self) -> bool {
        matches!(self.action.as_str(), "restore" | "delete")
    }
}

#[derive(Debug, Clone, Default)]
pub struct SessionUndoView {
    pub session_id: String,
    pub loading: bool,
    pub prompt: String,
    pub time_label: String,
    pub files: Vec<UndoFileEntry>,
    pub done_files: Vec<(String, String)>,
    pub backup_dir: Option<String>,
    pub error: Option<String>,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct AccountOption {
    pub slug: String,
    pub label: String,
    pub active: bool,
    pub pct: Option<u32>,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct TodoItem {
    pub id: String,
    pub text: String,
    pub done: bool,
    pub done_at: u64,
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

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum BurnRange {
    #[default]
    Today,
    Days7,
    Days30,
}

impl BurnRange {
    pub fn api_str(self) -> &'static str {
        match self {
            Self::Today => "today",
            Self::Days7 => "7d",
            Self::Days30 => "30d",
        }
    }
}

#[derive(Debug, Clone, Default)]
pub struct BurnDetailRow {
    pub name: String,
    pub kind: String,
    pub usd: f64,
    pub tokens: f64,
    pub share: f64,
}

#[derive(Debug, Clone, Default)]
pub struct BurnRoleRowUi {
    pub role: String,
    pub model: String,
    pub usd: f64,
    pub tokens: f64,
    pub share: f64,
}

#[derive(Debug, Clone, Default)]
pub struct BurnProjectRowUi {
    pub name: String,
    pub cwd: String,
    pub usd: f64,
    pub share: f64,
    pub sessions: usize,
}

#[derive(Debug, Clone, Default)]
pub struct BurnSessionRowUi {
    pub sid: String,
    pub name: String,
    pub project: String,
    pub cwd: String,
    pub usd: f64,
    pub tokens: f64,
    pub req: u64,
    pub share: f64,
    pub models: Vec<String>,
}

#[derive(Debug, Clone, Default)]
pub struct BurnDayBucketUi {
    pub day: String,
    pub usd: f64,
    pub models: Vec<(String, f64)>,
}

#[derive(Debug, Clone, Default)]
pub struct BurnDrawerState {
    pub range: BurnRange,
    pub loading: bool,
    pub loaded_range: Option<BurnRange>,
    pub error: Option<String>,
    pub setup: String,
    pub money: bool,
    pub total_usd: f64,
    pub total_tokens: f64,
    pub req: u64,
    pub err: u64,
    pub prompts: u64,
    pub plain_usd: f64,
    pub plain_share: f64,
    pub ranked: Vec<BurnDetailRow>,
    pub by_model: Vec<BurnDetailRow>,
    pub by_source: Vec<BurnDetailRow>,
    pub roles: Vec<BurnRoleRowUi>,
    pub projects: Vec<BurnProjectRowUi>,
    pub sessions: Vec<BurnSessionRowUi>,
    pub per_day: Vec<BurnDayBucketUi>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum VaultTab {
    #[default]
    List,
    Report,
}

#[derive(Debug, Clone, Default)]
pub struct VaultListItem {
    pub sid: String,
    pub title: String,
    pub cwd: String,
    pub first: String,
    pub end_ms: u64,
    pub msg_count: usize,
    pub gone_ms: u64,
}

#[derive(Debug, Clone, Default)]
pub struct VaultMsgUi {
    pub role: String,
    pub text: String,
    pub at_ms: u64,
}

#[derive(Debug, Clone, Default)]
pub struct VaultTranscriptUi {
    pub sid: String,
    pub title: String,
    pub cwd: String,
    pub start_ms: u64,
    pub end_ms: u64,
    pub gone_ms: u64,
    pub msgs: Vec<VaultMsgUi>,
}

#[derive(Debug, Clone, Default)]
pub struct PromptReportExampleUi {
    pub text: String,
    pub ph: usize,
    pub hint: String,
    pub phrase: String,
    pub n: usize,
    pub at_ms: u64,
    pub title: String,
}

#[derive(Debug, Clone, Default)]
pub struct PromptReportUi {
    pub loaded: bool,
    pub loading: bool,
    pub error: Option<String>,
    pub n: usize,
    pub clear: usize,
    pub pct: Option<u32>,
    pub convs: usize,
    pub by: Vec<(String, usize)>,
    pub examples: Vec<PromptReportExampleUi>,
}

#[derive(Debug, Clone, Default)]
pub struct VaultDrawerState {
    pub tab: VaultTab,
    pub items: Vec<VaultListItem>,
    pub total_count: usize,
    pub total_bytes: u64,
    pub loading: bool,
    pub loaded: bool,
    pub open_transcript: Option<VaultTranscriptUi>,
    pub transcript_loading: Option<String>,
    pub report: PromptReportUi,
}

#[derive(Debug, Clone, Default)]
pub struct SysProcRow {
    pub name: String,
    pub pid: u32,
    pub start: u64,
    pub count: usize,
    pub cpu: f32,
    pub mem_bytes: u64,
    pub can_quit: bool,
    pub busy: bool,
    pub idle_sec: u64,
    pub dir: String,
}

#[derive(Debug, Clone, Default)]
pub struct DevServerUi {
    pub pid: u32,
    pub start: u64,
    pub label: String,
    pub ports: Vec<u16>,
    pub dir: String,
    pub up_sec: u64,
    pub cpu: i64,
    pub mem_bytes: u64,
    pub conns: u32,
    pub idle_sec: u64,
}

#[derive(Debug, Clone, Default)]
pub struct CleanupRowUi {
    pub id: String,
    pub title: String,
    pub note: String,
    pub size_bytes: u64,
}

#[derive(Debug, Clone, Default)]
pub struct StrandedItemUi {
    pub id: String,
    pub path: String,
    pub repo: String,
    pub branch: String,
    pub main: bool,
    pub locked: bool,
    pub dirty: usize,
    pub ahead: u32,
    pub stashes: usize,
    pub ok: bool,
    pub in_use: bool,
    pub removable: bool,
    pub work: bool,
}

#[derive(Debug, Clone, Default)]
pub struct MacDrawerState {
    pub loading: bool,
    pub swap_used_gb: f32,
    pub swap_total_gb: f32,
    pub mem_pressure: String,
    pub disk_free_gb: f32,
    pub disk_total_gb: f32,
    pub cpu_cores: usize,
    pub cpu_load: f32,
    pub auto_close_hours: u8,
    pub claude_procs: Vec<SysProcRow>,
    pub apps: Vec<SysProcRow>,
    pub dev_servers: Vec<DevServerUi>,
    pub cleanups: Vec<CleanupRowUi>,
    pub stranded: Vec<StrandedItemUi>,
    pub stranded_loaded: bool,
    pub stranded_loading: bool,
    pub stranded_repos: usize,
    pub armed_action: Option<String>,
    pub stranded_confirm_id: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum AiProviderTab {
    #[default]
    Claude,
    Codex,
    Antigravity,
}

impl AiProviderTab {
    pub fn as_u8(self) -> u8 {
        match self {
            Self::Claude => 0,
            Self::Codex => 1,
            Self::Antigravity => 2,
        }
    }

    pub fn from_u8(v: u8) -> Self {
        match v {
            1 => Self::Codex,
            2 => Self::Antigravity,
            _ => Self::Claude,
        }
    }

    pub fn api_prefix(self) -> &'static str {
        match self {
            Self::Claude => "/api/accounts",
            Self::Codex => "/api/ai/codex",
            Self::Antigravity => "/api/ai/ag",
        }
    }
}

#[derive(Debug, Clone, Default, PartialEq)]
pub struct QuotaWindowUi {
    pub key: String,
    pub label: String,
    pub pct: f32,
    pub resets_at_ms: u64,
}

#[derive(Debug, Clone, Default, PartialEq)]
pub struct AiAccountEntry {
    pub slug: String,
    pub label: String,
    pub email: String,
    pub plan: String,
    pub active: bool,
    pub has_credentials: bool,
    pub note: String,
    pub windows: Vec<QuotaWindowUi>,
}

#[derive(Debug, Clone, Default)]
pub struct AiAccountsState {
    pub tab: AiProviderTab,
    pub loaded: bool,
    pub claude: Vec<AiAccountEntry>,
    pub claude_auto: bool,
    pub claude_move: bool,
    pub claude_resume: bool,
    pub codex: Vec<AiAccountEntry>,
    pub codex_auto: bool,
    pub codex_seat_note: String,
    pub ag: Vec<AiAccountEntry>,
    pub ag_auto: bool,
    pub ag_away: bool,
    pub ag_pending: Option<String>,
    pub ag_note: String,
    pub adding_provider: Option<AiProviderTab>,
    pub add_name_draft: String,
    pub add_email_draft: String,
    pub renaming: Option<(AiProviderTab, String)>,
    pub rename_draft: String,
    pub remove_armed: Option<(AiProviderTab, String)>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum PhoneTab {
    #[default]
    Home,
    Away,
}

#[derive(Debug, Clone, Default)]
pub struct PhoneModalState {
    pub tab: PhoneTab,
    pub loaded: bool,
    pub lan_on: bool,
    pub lan_ip: Option<String>,
    pub tunnel_on: bool,
    pub tunnel_state: String,
    pub tunnel_url: Option<String>,
    pub tunnel_domain: String,
    pub tunnel_domain_draft: String,
    pub tunnel_error: Option<String>,
    pub phone_approve: bool,
    pub push_count: usize,
    pub editing_pin: bool,
    pub pin_draft: String,
    pub pin_regen_armed: bool,
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

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ChatReplyRefUi {
    pub id: String,
    pub name: String,
    pub text: String,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ChatPollOptUi {
    pub t: String,
    pub by: Vec<String>,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ChatPollUi {
    pub q: String,
    pub opts: Vec<ChatPollOptUi>,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ChatRemindUi {
    pub title: String,
    pub at_ms: u64,
    pub repeat: String,
    pub yes: Vec<String>,
    pub no: Vec<String>,
    pub done: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ChatImagePreview {
    pub id: String,
    pub width: usize,
    pub height: usize,
    pub rgba: Vec<u8>,
}

#[derive(Debug, Clone, Default)]
pub struct ChatMessage {
    pub id: String,
    pub from: String,
    pub sender: String,
    pub role: String,
    pub text: String,
    pub ts: String,
    pub at_ms: u64,
    pub tag: Option<String>,
    pub recalled: bool,
    pub sticker: Option<u32>,
    pub photo: Option<String>,
    pub reply: Option<ChatReplyRefUi>,
    pub reacts: BTreeMap<String, Vec<String>>,
    pub poll: Option<ChatPollUi>,
    pub remind: Option<ChatRemindUi>,
}

#[derive(Debug, Clone, Default)]
pub struct WrappedSummaryUi {
    pub loading: bool,
    pub loaded: bool,
    pub error: Option<String>,
    pub from: String,
    pub to: String,
    pub days: usize,
    pub hours_sum: f64,
    pub hours_wall: f64,
    pub wait_hours: f64,
    pub prompts: u64,
    pub edits: u64,
    pub reads: u64,
    pub cmds: u64,
    pub deploys: u64,
    pub commits: u64,
    pub pushes: u64,
    pub longest_min: u64,
    pub cost: f64,
    pub sessions: usize,
    pub top_project: Option<(String, f64)>,
    pub busiest_day: Option<(String, u32, f64)>,
    pub last_saved_path: Option<String>,
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
    SubmitAskAnswers {
        id: String,
        answers: Map<String, Value>,
    },
    DecidePlan {
        id: String,
        approve: bool,
        feedback: String,
    },
    SkipHold(String),
    DecideDenied {
        id: String,
        retry: bool,
    },
    LoopStop {
        session_id: String,
    },
    LoopHint {
        session_id: String,
        hint: String,
    },
    CiFix {
        id: String,
    },
    CiDismiss {
        id: String,
    },
    FetchSessionLog(String),
    AddSessionPin {
        session_id: String,
        text: String,
    },
    RemoveSessionPin {
        session_id: String,
        index: usize,
    },
    FetchSessionUndo(String),
    ApplySessionUndo {
        session_id: String,
        files: Vec<String>,
    },
    TellClaudeUndo {
        session_id: String,
        files: Vec<String>,
    },
    RewritePrompt {
        text: String,
        lang: String,
    },
    SteerSession {
        session_id: String,
        text: String,
    },
    StopSessionTask {
        session_id: String,
    },
    OpenSessionWindow(String),
    SwitchAccount {
        session_id: String,
        target: String,
    },
    SendPrompt {
        session_id: String,
        text: String,
    },
    PostGroupChat(String),
    SendGroupChat {
        text: String,
        reply_to: Option<String>,
        sticker: Option<u32>,
    },
    ReactGroupChat {
        id: String,
        emoji: String,
    },
    RecallGroupChat {
        id: String,
    },
    PinGroupChat {
        id: String,
    },
    CreateGroupPoll {
        q: String,
        opts: Vec<String>,
    },
    VoteGroupPoll {
        id: String,
        option_index: usize,
    },
    RsvpGroupRemind {
        id: String,
        yes: bool,
    },
    SendGroupPhoto,
    FetchChatImage {
        id: String,
    },
    PreviewChatDataUrl {
        id: String,
        data_url: String,
    },
    SaveWhiteboard(String),
    SyncWhiteboardTodos(Vec<TodoItem>),
    FetchWrapped,
    ExportWrappedPng {
        copy_clipboard: bool,
    },
    CancelTimelapseClip,
    FetchBurn(BurnRange),
    FetchVaultList {
        query: String,
        fresh: bool,
    },
    FetchVaultTranscript(String),
    FetchVaultReport,
    CopyToClipboard {
        text: String,
        toast_title: String,
        toast_body: String,
    },
    SysAction {
        action: String,
        target: Value,
        force: bool,
        start: Option<u64>,
    },
    SetSysAutoClose(u8),
    FetchStranded {
        fresh: bool,
    },
    OpenStranded(String),
    RemoveStranded(String),
    FetchAiAccounts,
    AiAccountMutate {
        provider: AiProviderTab,
        action: String,
        body: Value,
    },
    FetchPhoneStatus,
    ToggleLan(bool),
    ToggleTunnel(bool),
    SetTunnelDomain(String),
    TogglePhoneApprove(bool),
    UpdatePin {
        pin: Option<String>,
    },
    TestWebPush,
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
    pub session_tab: SessionDrawerTab,
    pub session_logs: HashMap<String, Vec<SessionLogItem>>,
    pub session_log_loaded_for: Option<String>,
    pub session_pins: HashMap<String, Vec<String>>,
    pub pin_draft: String,
    pub session_undo: Option<SessionUndoView>,
    pub undo_armed_session: Option<String>,
    pub session_todos: Vec<TodoItem>,
    pub accounts: Vec<AccountOption>,
    pub account_menu_open: bool,
    pub compose_rewriting: bool,
    pub compose_prev_draft: Option<String>,
    pub perm_ask_drafts: HashMap<String, AskCardDraft>,
    pub perm_plan_notes: HashMap<String, String>,
    pub perm_stuck_hints: HashMap<String, String>,
    pub perm_danger_armed: HashMap<String, bool>,
    pub chat_pinned_id: Option<String>,
    pub chat_typing: Vec<String>,
    pub chat_reply_to: Option<ChatReplyRefUi>,
    pub chat_poll_open: bool,
    pub chat_poll_q: String,
    pub chat_poll_opts: Vec<String>,
    pub chat_sticker_open: bool,
    pub chat_sticker_tab: u8,
    pub chat_mention_open: bool,
    pub chat_preview_image: Option<ChatImagePreview>,
    pub chat_poll_show_voters: HashSet<String>,
    pub whiteboard_note_draft: String,
    pub wrapped: WrappedSummaryUi,
    pub timelapse_recording: bool,
    pub timelapse_progress: f32,
    pub timelapse_elapsed_sec: u32,
    pub burn_drawer: BurnDrawerState,
    pub vault_drawer: VaultDrawerState,
    pub mac_drawer: MacDrawerState,
    pub ai_accounts: AiAccountsState,
    pub phone_modal: PhoneModalState,
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
            session_tab: SessionDrawerTab::Chat,
            session_logs: HashMap::new(),
            session_log_loaded_for: None,
            session_pins: HashMap::new(),
            pin_draft: String::new(),
            session_undo: None,
            undo_armed_session: None,
            session_todos: Vec::new(),
            accounts: Vec::new(),
            account_menu_open: false,
            compose_rewriting: false,
            compose_prev_draft: None,
            perm_ask_drafts: HashMap::new(),
            perm_plan_notes: HashMap::new(),
            perm_stuck_hints: HashMap::new(),
            perm_danger_armed: HashMap::new(),
            chat_pinned_id: None,
            chat_typing: Vec::new(),
            chat_reply_to: None,
            chat_poll_open: false,
            chat_poll_q: String::new(),
            chat_poll_opts: vec![String::new(), String::new()],
            chat_sticker_open: false,
            chat_sticker_tab: 0,
            chat_mention_open: false,
            chat_preview_image: None,
            chat_poll_show_voters: HashSet::new(),
            whiteboard_note_draft: String::new(),
            wrapped: WrappedSummaryUi::default(),
            timelapse_recording: false,
            timelapse_progress: 0.0,
            timelapse_elapsed_sec: 0,
            burn_drawer: BurnDrawerState::default(),
            vault_drawer: VaultDrawerState::default(),
            mac_drawer: MacDrawerState::default(),
            ai_accounts: AiAccountsState::default(),
            phone_modal: PhoneModalState::default(),
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
                ..Default::default()
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
                    id: "demo-chat-1".to_owned(),
                    from: "sess-claude-1".to_owned(),
                    sender: "Claude".to_owned(),
                    role: "text".to_owned(),
                    text: "Gate P4 passed at 148.5 MB HQ Private WS.".to_owned(),
                    ts: "16:40".to_owned(),
                    tag: Some("claude".to_owned()),
                    reacts: BTreeMap::from([("❤️".to_owned(), vec!["sess-agy-3".to_owned()])]),
                    ..Default::default()
                },
                ChatMessage {
                    id: "demo-chat-2".to_owned(),
                    from: "sess-agy-3".to_owned(),
                    sender: "Antigravity".to_owned(),
                    role: "text".to_owned(),
                    text: "Starting native HUD, 2D Lite & Tray bridge now.".to_owned(),
                    ts: "16:42".to_owned(),
                    tag: Some("antigravity".to_owned()),
                    ..Default::default()
                },
                ChatMessage {
                    id: "demo-poll-1".to_owned(),
                    from: "me".to_owned(),
                    sender: "Boss".to_owned(),
                    role: "poll".to_owned(),
                    text: "Afternoon drink?".to_owned(),
                    ts: "16:44".to_owned(),
                    poll: Some(ChatPollUi {
                        q: "Afternoon drink?".to_owned(),
                        opts: vec![
                            ChatPollOptUi {
                                t: "Iced milk coffee".to_owned(),
                                by: vec!["sess-claude-1".to_owned()],
                            },
                            ChatPollOptUi {
                                t: "Peach tea".to_owned(),
                                by: vec!["sess-agy-3".to_owned()],
                            },
                        ],
                    }),
                    ..Default::default()
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
            session_tab: SessionDrawerTab::Chat,
            session_logs: HashMap::from([(
                "sess-claude-1".to_owned(),
                vec![
                    SessionLogItem {
                        role: "user".to_owned(),
                        text: "Build native Rust 3-column control-room shell".to_owned(),
                        ts: "16:40".to_owned(),
                        tool_type: None,
                    },
                    SessionLogItem {
                        role: "assistant".to_owned(),
                        text: "### Shell Progress\n- Wired `TopBar` + `SessionRail` + `ActivityPanel`\n- Next: **SessionDrawer** markdown view\n```rust\npub fn draw_session_drawer() {}\n```".to_owned(),
                        ts: "16:42".to_owned(),
                        tool_type: None,
                    },
                ],
            )]),
            session_log_loaded_for: Some("sess-claude-1".to_owned()),
            session_pins: HashMap::from([(
                "sess-claude-1".to_owned(),
                vec!["Never modify d:\\mystation\\my-station".to_owned()],
            )]),
            pin_draft: String::new(),
            session_undo: None,
            undo_armed_session: None,
            session_todos: vec![
                TodoItem {
                    id: "demo-todo-1".to_owned(),
                    text: "Implement 6 interactive perm cards".to_owned(),
                    done: true,
                    done_at: 1,
                },
                TodoItem {
                    id: "demo-todo-2".to_owned(),
                    text: "Wire SessionDrawer tabs & compose".to_owned(),
                    done: false,
                    done_at: 0,
                },
            ],
            accounts: vec![
                AccountOption {
                    slug: String::new(),
                    label: "Main".to_owned(),
                    active: true,
                    pct: Some(24),
                },
                AccountOption {
                    slug: "work".to_owned(),
                    label: "Work".to_owned(),
                    active: false,
                    pct: Some(12),
                },
            ],
            account_menu_open: false,
            compose_rewriting: false,
            compose_prev_draft: None,
            perm_ask_drafts: HashMap::new(),
            perm_plan_notes: HashMap::new(),
            perm_stuck_hints: HashMap::new(),
            perm_danger_armed: HashMap::new(),
            chat_pinned_id: Some("demo-poll-1".to_owned()),
            chat_typing: Vec::new(),
            chat_reply_to: None,
            chat_poll_open: false,
            chat_poll_q: String::new(),
            chat_poll_opts: vec![String::new(), String::new()],
            chat_sticker_open: false,
            chat_sticker_tab: 0,
            chat_mention_open: false,
            chat_preview_image: None,
            chat_poll_show_voters: HashSet::new(),
            whiteboard_note_draft: String::new(),
            wrapped: WrappedSummaryUi {
                loaded: true,
                from: "2026-11-19".to_owned(),
                to: "2026-11-25".to_owned(),
                days: 7,
                hours_sum: 63.4,
                hours_wall: 38.2,
                wait_hours: 9.6,
                prompts: 312,
                edits: 1840,
                reads: 4210,
                cmds: 2985,
                deploys: 9,
                commits: 46,
                pushes: 21,
                longest_min: 214,
                cost: 412.5,
                sessions: 27,
                top_project: Some(("agents-station".to_owned(), 21.3)),
                busiest_day: Some(("2026-11-23".to_owned(), 4, 14.2)),
                ..Default::default()
            },
            timelapse_recording: false,
            timelapse_progress: 0.0,
            timelapse_elapsed_sec: 0,
            burn_drawer: BurnDrawerState {
                range: BurnRange::Days7,
                loaded_range: Some(BurnRange::Days7),
                money: true,
                total_usd: 42.6,
                total_tokens: 1_840_000.0,
                req: 312,
                prompts: 68,
                by_model: vec![
                    BurnDetailRow {
                        name: "claude-opus-4-6".to_owned(),
                        usd: 29.8,
                        share: 0.70,
                        ..Default::default()
                    },
                    BurnDetailRow {
                        name: "claude-sonnet-4-6".to_owned(),
                        usd: 12.8,
                        share: 0.30,
                        ..Default::default()
                    },
                ],
                per_day: vec![
                    BurnDayBucketUi {
                        day: "2026-11-24".to_owned(),
                        usd: 14.2,
                        models: vec![("claude-opus-4-6".to_owned(), 10.0), ("claude-sonnet-4-6".to_owned(), 4.2)],
                    },
                    BurnDayBucketUi {
                        day: "2026-11-25".to_owned(),
                        usd: 28.4,
                        models: vec![("claude-opus-4-6".to_owned(), 19.8), ("claude-sonnet-4-6".to_owned(), 8.6)],
                    },
                ],
                ..Default::default()
            },
            vault_drawer: VaultDrawerState {
                loaded: true,
                total_count: 1,
                total_bytes: 4096,
                items: vec![VaultListItem {
                    sid: "demo-vault-1".to_owned(),
                    title: "agents-station".to_owned(),
                    cwd: "D:\\mystation\\agents-station".to_owned(),
                    first: "Native Rust rebuild with 100% parity".to_owned(),
                    end_ms: 1_764_000_000_000,
                    msg_count: 14,
                    gone_ms: 0,
                }],
                ..Default::default()
            },
            mac_drawer: MacDrawerState {
                disk_free_gb: 210.0,
                disk_total_gb: 512.0,
                cpu_cores: 8,
                cpu_load: 1.4,
                mem_pressure: "normal".to_owned(),
                stranded_loaded: true,
                stranded_repos: 1,
                ..Default::default()
            },
            ai_accounts: AiAccountsState {
                loaded: true,
                claude: vec![
                    AiAccountEntry {
                        slug: String::new(),
                        label: "Main".to_owned(),
                        active: true,
                        has_credentials: true,
                        windows: vec![
                            QuotaWindowUi {
                                key: "session".to_owned(),
                                label: "5 h".to_owned(),
                                pct: 24.0,
                                resets_at_ms: 0,
                            },
                            QuotaWindowUi {
                                key: "weekly".to_owned(),
                                label: "Week".to_owned(),
                                pct: 41.0,
                                resets_at_ms: 0,
                            },
                        ],
                        ..Default::default()
                    },
                    AiAccountEntry {
                        slug: "work".to_owned(),
                        label: "Work".to_owned(),
                        active: false,
                        has_credentials: true,
                        ..Default::default()
                    },
                ],
                ..Default::default()
            },
            phone_modal: PhoneModalState {
                loaded: true,
                lan_on: true,
                lan_ip: Some("192.168.1.42".to_owned()),
                ..Default::default()
            },
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
            ..Default::default()
        });
        assert_eq!(demo.waiting_count(), 2);

        assert_eq!(GpuPrefUi::from_pref_str("integrated"), GpuPrefUi::Integrated);
        assert_eq!(GpuPrefUi::Discrete.as_pref_str(), "discrete");
        st.push_action(UiAction::SetGpuPref(GpuPrefUi::Discrete));
        assert_eq!(st.drain_actions().len(), 1);
    }
}
