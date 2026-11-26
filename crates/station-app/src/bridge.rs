use crate::fps::AdaptiveFpsState;
use crate::tray::{TrayState, notify_parent_hold_alert, notify_parent_in_tray};
use base64::Engine as _;
use bevy::app::AppExit;
use bevy::ecs::system::SystemParam;
use bevy::prelude::*;
use bevy::render::camera::{RenderTarget, Viewport};
use bevy::render::view::screenshot::{Screenshot, ScreenshotCaptured, save_to_disk};
use bevy::window::PrimaryWindow;
use bevy_egui::egui;
use chrono::{Local, TimeZone, Timelike};
use serde_json::{Value, json};
use station_3d::{BackdropAssets, BackdropEntity, LayoutSpec, SfxKind, WorldEntity, WorldPalette};
use station_canvas2d::BrandRegistry;
use station_core::http::{AppState, HarnessApi, start_http_server};
use station_core::state::sessions::{PermDecision, now_ms};
use station_launcher::focus_launch::{Win32WindowLauncher, WindowLauncher};
use station_launcher::{HarnessService, HarnessStatus, LaunchOutcome};
use station_ui::icons::IconKind;
use station_ui::labels3d::{
    LabelPriority, ProjectedLabel, ProjectedLabelKind, accent_for_status, estimate_label_size,
    project_world_to_stage,
};
use station_ui::state::{
    AccountOption, AiAccountEntry, AiProviderTab, BurnDayBucketUi, BurnDetailRow, BurnDrawerState,
    BurnProjectRowUi, BurnRange, BurnRoleRowUi, BurnSessionRowUi, BurnSnapshot, CacheWarmth,
    ChatImagePreview, ChatMessage, ChatPollOptUi, ChatPollUi, ChatRemindUi, ChatReplyRefUi,
    CleanupRowUi, ClipShape, DevServerUi, FeedEntry, HarnessDockItem, MacDrawerState,
    PendingPermCard, PhotoSize, PlaceCardAction, PlaceCardInfo, PlaceCardRow,
    PromptReportExampleUi, PromptReportUi, QuotaWindowUi, ReplayRange, SessionCard, SessionLogItem,
    SessionUndoView, StrandedItemUi, SysProcRow, SysSnapshot, TodoItem, UiAction, UndoFileEntry,
    VaultListItem, VaultMsgUi, VaultTranscriptUi, WrappedSummaryUi, normalize_feed_kind,
    normalize_status,
};
use station_ui::theme::{ACCENT, AMBER, BLUE, GREEN, RED};
use station_ui::{ActiveDrawer, I18nStore, StageViewportRect, UiState, ViewMode};
use std::collections::{HashMap, HashSet};
use std::io::{Read, Write};
use std::net::{SocketAddr, TcpStream};
use std::path::PathBuf;
use std::sync::{Arc, Mutex};
use std::time::Duration;
use tokio::sync::{mpsc, watch};

const MAX_SSE_LINE_BYTES: usize = 2 * 1024 * 1024;
const MAX_CHAT_IMAGE_BYTES: usize = 8 * 1024 * 1024;
const MAX_CHAT_DATA_URL_LEN: usize = 500_000;
const REMOTE_ACTION_QUEUE_CAP: usize = 64;
const TOAST_QUEUE_CAP: usize = 32;
const HARNESS_POLL_INTERVAL: Duration = Duration::from_secs(1);
const TELEMETRY_POLL_INTERVAL: Duration = Duration::from_secs(3);

/// Marks a Bevy GUI app running as the ephemeral `--gui-worker` child of `TrayDaemon`.
#[derive(Resource, Debug, Clone, Copy, Default)]
pub struct GuiWorkerMode(pub bool);

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BridgeMode {
    InProcess,
    AttachedHttp,
    BindError,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum RefreshKind {
    None,
    Live,
    Harnesses,
    Chat,
    All,
}

struct RemoteActionCmd {
    path: &'static str,
    body: Value,
    refresh: RefreshKind,
    launch_toast_label: Option<String>,
}

enum RemoteQueryCmd {
    FetchLog { session_id: String },
    FetchUndo { session_id: String },
    ApplyUndo { session_id: String, files: Vec<String> },
    RewritePrompt { text: String, lang: String },
    FetchWrapped,
    FetchImage { id: String },
    FetchBurn(BurnRange),
    FetchVaultList { query: String, fresh: bool, seq: u64 },
    FetchVaultTranscript { sid: String },
    FetchVaultReport,
    FetchStranded { fresh: bool },
    StrandedMutate { path: &'static str, id: String, toast_title: String, toast_ok: String, toast_err: String },
    SysMutate { path: &'static str, body: Value, toast_title: String, toast_ok: String, toast_err: String },
    FetchAiAccounts,
    AiAccountMutate {
        provider: AiProviderTab,
        action: String,
        body: Value,
        toast_title: String,
        toast_ok: String,
        toast_err: String,
    },
    FetchPhoneStatus,
    PhoneMutate {
        path: &'static str,
        body: Value,
        toast_title: String,
        toast_ok: String,
        toast_empty: Option<String>,
        toast_err: String,
    },
}

enum BridgeAsyncResult {
    SessionLog {
        session_id: String,
        items: Vec<SessionLogItem>,
    },
    SessionUndo(SessionUndoView),
    RewriteResult(Result<String, String>),
    WrappedSummary(Result<WrappedSummaryUi, String>),
    WrappedExported {
        rel_path: Option<String>,
        toast_title: String,
        toast_body: String,
    },
    ChatImage(Result<ChatImagePreview, String>),
    BurnData(Result<BurnDrawerState, String>),
    VaultList {
        seq: u64,
        items: Vec<VaultListItem>,
        total_count: usize,
        total_bytes: u64,
    },
    VaultTranscript(Option<VaultTranscriptUi>),
    VaultReport(Result<PromptReportUi, String>),
    StrandedData {
        items: Vec<StrandedItemUi>,
        repos: usize,
    },
    AiAccountsData {
        claude: Vec<AiAccountEntry>,
        claude_auto: bool,
        claude_move: bool,
        claude_resume: bool,
        codex: Vec<AiAccountEntry>,
        codex_auto: bool,
        codex_seat_note: String,
        ag: Vec<AiAccountEntry>,
        ag_auto: bool,
        ag_away: bool,
        ag_pending: Option<String>,
        ag_note: String,
    },
    AiAccountNotice {
        title: String,
        ok: bool,
        code: Option<String>,
        ok_msg: String,
        err_msg: String,
    },
    PhoneStatus {
        lan_on: bool,
        lan_ip: Option<String>,
        tunnel_on: bool,
        tunnel_state: String,
        tunnel_url: Option<String>,
        tunnel_domain: String,
        tunnel_error: Option<String>,
        phone_approve: bool,
        push_count: usize,
        pin: Option<String>,
    },
}

#[derive(Clone)]
struct PollerWakeup(Arc<Mutex<Option<std::sync::mpsc::Sender<()>>>>);

impl PollerWakeup {
    fn trigger(&self) {
        if let Ok(guard) = self.0.lock()
            && let Some(tx) = guard.as_ref()
        {
            let _ = tx.send(());
        }
    }
}

/// Runs `HarnessService::list()` on a dedicated 1 Hz background thread so Win32 process/window
/// enumeration never stalls the Bevy render thread. Drops cleanly by closing the wakeup channel.
struct HarnessPoller {
    latest_rx: watch::Receiver<Option<Vec<HarnessDockItem>>>,
    wakeup: PollerWakeup,
    join_handle: Option<std::thread::JoinHandle<()>>,
}

impl HarnessPoller {
    fn spawn<L: WindowLauncher + 'static>(
        service: Arc<HarnessService<L>>,
        interval: Duration,
    ) -> Self {
        let (snap_tx, snap_rx) = watch::channel::<Option<Vec<HarnessDockItem>>>(None);
        let (wake_tx, wake_rx) = std::sync::mpsc::channel::<()>();
        let wakeup = PollerWakeup(Arc::new(Mutex::new(Some(wake_tx))));
        let join_handle = std::thread::Builder::new()
            .name("station-harness-scanner".to_owned())
            .spawn(move || {
                loop {
                    let items: Vec<HarnessDockItem> = service
                        .list()
                        .into_iter()
                        .map(|h| HarnessDockItem {
                            id: h.id,
                            label: h.label,
                            status: h.status,
                            custom: h.custom,
                            hook_agent: h.hook_agent,
                        })
                        .collect();
                    snap_tx.send_replace(Some(items));
                    match wake_rx.recv_timeout(interval) {
                        Ok(()) | Err(std::sync::mpsc::RecvTimeoutError::Timeout) => {}
                        Err(std::sync::mpsc::RecvTimeoutError::Disconnected) => break,
                    }
                }
            })
            .ok();
        Self {
            latest_rx: snap_rx,
            wakeup,
            join_handle,
        }
    }

    fn take_latest(&mut self) -> Option<Vec<HarnessDockItem>> {
        if self.latest_rx.has_changed().unwrap_or(false) {
            self.latest_rx.borrow_and_update().clone()
        } else {
            None
        }
    }

    fn trigger_refresh(&self) {
        self.wakeup.trigger();
    }

    fn wakeup_handle(&self) -> PollerWakeup {
        self.wakeup.clone()
    }
}

impl Drop for HarnessPoller {
    fn drop(&mut self) {
        // Close the sole Sender so recv_timeout wakes immediately even if a launch thread holds PollerWakeup.
        if let Ok(mut guard) = self.wakeup.0.lock() {
            guard.take();
        }
        if let Some(handle) = self.join_handle.take() {
            let _ = handle.join();
        }
    }
}

fn launch_outcome_toast(label: &str, outcome: &LaunchOutcome) -> (String, String) {
    match outcome {
        LaunchOutcome::FocusedExisting { pid } => (
            format!("Focused {label}"),
            format!("Switched to running {label} (PID {pid})"),
        ),
        LaunchOutcome::LaunchedNew => (
            format!("Launched {label}"),
            format!("Started {label} desktop app"),
        ),
        LaunchOutcome::NotInstalled => (
            format!("{label} not installed"),
            format!("Install {label} or add its .exe/.lnk via + Add"),
        ),
    }
}

fn launch_error_toast(label: &str, err: &str) -> (String, String) {
    (format!("Failed to open {label}"), err.to_owned())
}

fn parse_launch_outcome_from_json(val: &Value) -> Option<LaunchOutcome> {
    val.get("result")
        .and_then(|r| serde_json::from_value::<LaunchOutcome>(r.clone()).ok())
}

fn interpret_remote_launch_response(label: &str, val: &Value) -> (bool, (String, String)) {
    if val.get("ok").and_then(Value::as_bool) == Some(false) || val.get("error").is_some() {
        let err = val
            .get("error")
            .and_then(Value::as_str)
            .filter(|s| !s.is_empty())
            .unwrap_or("launch failed");
        return (false, launch_error_toast(label, err));
    }
    match parse_launch_outcome_from_json(val) {
        Some(outcome) => (true, launch_outcome_toast(label, &outcome)),
        None => (false, launch_error_toast(label, "invalid launch response")),
    }
}

#[derive(Default, Clone)]
struct RemoteSnapshot {
    has_snapshot: bool,
    sessions: Vec<SessionCard>,
    pending_perms: Vec<PendingPermCard>,
    feed: Vec<FeedEntry>,
    harnesses: Vec<HarnessDockItem>,
    pairing_pin: Option<String>,
    sys: Option<SysSnapshot>,
    mac_drawer: Option<MacDrawerState>,
    burn: Option<BurnSnapshot>,
    has_chat: bool,
    chat: Vec<ChatMessage>,
    chat_pinned_id: Option<String>,
    chat_typing: Vec<String>,
    ci_alerts: Vec<PendingPermCard>,
    has_vault: bool,
    vault_results: Vec<(String, String)>,
    has_pins: bool,
    pins: HashMap<String, Vec<String>>,
    has_todos: bool,
    todos: Vec<TodoItem>,
    accounts: Vec<AccountOption>,
}

/// Byte-buffered SSE line decoder that preserves multi-byte UTF-8 sequences split across
/// TCP chunks and bounds the maximum unterminated line length to prevent unbounded growth.
struct SseByteDecoder {
    buf: Vec<u8>,
    max_line_bytes: usize,
}

impl SseByteDecoder {
    fn new(max_line_bytes: usize) -> Self {
        Self {
            buf: Vec::new(),
            max_line_bytes,
        }
    }

    fn push_chunk(&mut self, chunk: &[u8]) -> Result<Vec<String>, ()> {
        let mut lines = Vec::new();
        let mut start = 0usize;

        while let Some(rel_nl) = chunk[start..].iter().position(|&b| b == b'\n') {
            let nl_idx = start + rel_nl;
            let seg = &chunk[start..nl_idx];
            if self.buf.len().saturating_add(seg.len()) > self.max_line_bytes {
                self.buf.clear();
                return Err(());
            }
            let line_bytes: &[u8] = if self.buf.is_empty() {
                seg
            } else {
                self.buf.extend_from_slice(seg);
                &self.buf
            };
            let trimmed = line_bytes.strip_suffix(b"\r").unwrap_or(line_bytes);
            lines.push(String::from_utf8_lossy(trimmed).into_owned());
            self.buf.clear();
            start = nl_idx + 1;
        }

        let tail = &chunk[start..];
        if !tail.is_empty() {
            if self.buf.len().saturating_add(tail.len()) > self.max_line_bytes {
                self.buf.clear();
                return Err(());
            }
            self.buf.extend_from_slice(tail);
        }

        Ok(lines)
    }
}

enum BridgeBackend {
    InProcess {
        app_state: Arc<AppState>,
        harnesses: Arc<HarnessService<Win32WindowLauncher>>,
        harness_poller: HarnessPoller,
        runtime_handle: Option<tokio::runtime::Handle>,
        _shutdown: Option<watch::Sender<bool>>,
    },
    AttachedHttp {
        snapshot: Arc<Mutex<RemoteSnapshot>>,
        action_tx: mpsc::Sender<RemoteActionCmd>,
        query_tx: mpsc::Sender<RemoteQueryCmd>,
        _shutdown: watch::Sender<bool>,
    },
    BindError {
        message: String,
        harnesses: Arc<HarnessService<Win32WindowLauncher>>,
        harness_poller: HarnessPoller,
    },
}

/// Fast synchronous loopback probe checking `GET /health` with explicit `Host: 127.0.0.1:{port}`
/// header and expecting HTTP `200` with body `"ok"`. Uses raw `TcpStream` so parent process
/// incurs zero TLS/reqwest overhead.
pub fn probe_local_health(port: u16) -> bool {
    let addr = SocketAddr::from(([127, 0, 0, 1], port));
    let Ok(mut stream) = TcpStream::connect_timeout(&addr, Duration::from_millis(250)) else {
        return false;
    };
    let _ = stream.set_read_timeout(Some(Duration::from_millis(350)));
    let _ = stream.set_write_timeout(Some(Duration::from_millis(350)));
    let req = format!(
        "GET /health HTTP/1.1\r\nHost: 127.0.0.1:{port}\r\nConnection: close\r\n\r\n"
    );
    if stream.write_all(req.as_bytes()).is_err() {
        return false;
    }
    let mut buf = [0u8; 1024];
    let mut total = 0usize;
    loop {
        match stream.read(&mut buf[total..]) {
            Ok(0) => break,
            Ok(n) => {
                total += n;
                if total >= buf.len() {
                    break;
                }
            }
            Err(_) => break,
        }
    }
    let Ok(resp) = std::str::from_utf8(&buf[..total]) else {
        return false;
    };
    let Some((head, body)) = resp.split_once("\r\n\r\n") else {
        return false;
    };
    let status_ok = head.starts_with("HTTP/1.1 200") || head.starts_with("HTTP/1.0 200");
    status_ok && body.trim() == "ok"
}

/// Fast synchronous loopback `POST /api/presence { "focused": false }` used before exiting
/// `--gui-worker` so an attached HTTP daemon immediately releases focus without waiting for async queues.
pub fn post_presence_unfocused_sync(port: u16) -> bool {
    if port == 0 {
        return false;
    }
    let addr = SocketAddr::from(([127, 0, 0, 1], port));
    let Ok(mut stream) = TcpStream::connect_timeout(&addr, Duration::from_millis(200)) else {
        return false;
    };
    let _ = stream.set_read_timeout(Some(Duration::from_millis(250)));
    let _ = stream.set_write_timeout(Some(Duration::from_millis(250)));
    let body = r#"{"focused":false}"#;
    let req = format!(
        "POST /api/presence HTTP/1.1\r\nHost: 127.0.0.1:{port}\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
        body.len()
    );
    if stream.write_all(req.as_bytes()).is_err() {
        return false;
    }
    let mut buf = [0u8; 256];
    let Ok(n) = stream.read(&mut buf) else {
        return false;
    };
    let resp = std::str::from_utf8(&buf[..n]).unwrap_or("");
    resp.starts_with("HTTP/1.1 200") || resp.starts_with("HTTP/1.0 200")
}

fn format_hhmm_ms(ms: u64) -> String {
    if ms == 0 {
        return Local::now().format("%H:%M").to_string();
    }
    Local
        .timestamp_millis_opt(ms as i64)
        .single()
        .map(|dt| dt.format("%H:%M").to_string())
        .unwrap_or_else(|| format!("{}", (ms / 1000) % 86400))
}

fn parse_sessions_from_json(val: &Value) -> Vec<SessionCard> {
    let Some(arr) = val.get("sessions").and_then(Value::as_array) else {
        return Vec::new();
    };
    arr.iter()
        .enumerate()
        .filter_map(|(idx, s)| {
            let id = s.get("id").and_then(Value::as_str)?.to_owned();
            let agent = s
                .get("agent")
                .and_then(Value::as_str)
                .unwrap_or("claude")
                .to_owned();
            let project = s
                .get("name")
                .and_then(Value::as_str)
                .unwrap_or(&agent)
                .to_owned();
            let status = normalize_status(
                s.get("status").and_then(Value::as_str).unwrap_or("rest"),
            )
            .to_owned();
            let model = s
                .get("model")
                .and_then(Value::as_str)
                .unwrap_or("default")
                .to_owned();
            let last_prompt = s
                .get("last_prompt")
                .and_then(Value::as_str)
                .unwrap_or_default()
                .to_owned();
            let tool = s
                .get("last")
                .and_then(|l| l.get("label"))
                .and_then(Value::as_str)
                .map(str::to_owned);
            let tool_kind = s
                .get("last")
                .and_then(|l| l.get("type"))
                .and_then(Value::as_str)
                .map(str::to_owned);
            let desk = s
                .get("desk")
                .and_then(Value::as_u64)
                .map(|d| d as usize)
                .unwrap_or(idx);
            let cost_usd = s.get("cost_usd").and_then(Value::as_f64).unwrap_or(0.0);
            let in_tok = s.get("input_tokens").and_then(Value::as_u64).unwrap_or(0);
            let out_tok = s.get("output_tokens").and_then(Value::as_u64).unwrap_or(0);
            let tool_calls = s.get("tool_calls").and_then(Value::as_u64).unwrap_or(0);
            let tokens = if in_tok + out_tok > 0 {
                in_tok + out_tok
            } else {
                tool_calls * 420
            };
            let ctx_tokens = s.get("ctx_tokens").and_then(Value::as_u64).unwrap_or(0);
            let ctx_max = s.get("ctx_max").and_then(Value::as_u64).unwrap_or(0);
            let ctx_pct = if ctx_max > 0 {
                (ctx_tokens as f32 / ctx_max as f32).clamp(0.0, 1.0)
            } else {
                0.0
            };
            let dir = s
                .get("cwd")
                .or_else(|| s.get("dir"))
                .and_then(Value::as_str)
                .unwrap_or_default()
                .to_owned();
            let git_branch = s
                .get("git")
                .and_then(|g| g.get("branch"))
                .and_then(Value::as_str)
                .filter(|b| !b.is_empty())
                .map(str::to_owned);
            let git_dirty = s
                .get("git")
                .and_then(|g| g.get("dirty"))
                .and_then(Value::as_u64)
                .unwrap_or(0) as usize;
            let cache_warmth = s
                .get("cache")
                .and_then(|c| serde_json::from_value::<station_core::shared::cache_clock::CacheState>(c.clone()).ok())
                .map(|c| {
                    let view = station_core::shared::cache_clock::cache_clock(Some(&c), now_ms());
                    match view.state {
                        "warm" => CacheWarmth::Warm(((view.left as f64 / 60_000.0).round() as u32).max(1)),
                        "cold" => CacheWarmth::Cold,
                        _ => CacheWarmth::Unknown,
                    }
                })
                .unwrap_or(CacheWarmth::Unknown);
            let subagents = s
                .get("subs")
                .or_else(|| s.get("subagents"))
                .and_then(Value::as_array)
                .map(|arr| {
                    arr.iter()
                        .filter_map(|sub| {
                            let id = sub.get("id").and_then(Value::as_str)?.to_owned();
                            let label = sub
                                .get("type")
                                .or_else(|| sub.get("label"))
                                .and_then(Value::as_str)
                                .unwrap_or("agent")
                                .to_owned();
                            let doing = sub
                                .get("desc")
                                .or_else(|| sub.get("doing"))
                                .and_then(Value::as_str)
                                .unwrap_or_default()
                                .to_owned();
                            Some(station_ui::state::SubagentItem { id, label, doing })
                        })
                        .collect()
                })
                .unwrap_or_default();
            Some(SessionCard {
                id,
                agent,
                project,
                dir,
                git_branch,
                git_dirty,
                cache_warmth,
                todos_done: 0,
                todos_total: 0,
                status,
                model,
                last_prompt,
                last_reply: String::new(),
                tool,
                tool_kind,
                desk,
                cost_usd,
                tokens,
                pinned: false,
                age_label: String::new(),
                ctx_pct,
                subagents,
            })
        })
        .collect()
}

fn parse_perms_from_json(val: &Value) -> Vec<PendingPermCard> {
    let mut out: Vec<PendingPermCard> = val
        .get("perms")
        .and_then(Value::as_array)
        .map(|arr| {
            arr.iter()
                .filter_map(|p| {
                    let id = p.get("id").and_then(Value::as_str)?.to_owned();
                    Some(PendingPermCard {
                        id,
                        session_id: p
                            .get("sess")
                            .and_then(Value::as_str)
                            .unwrap_or_default()
                            .to_owned(),
                        agent: p
                            .get("name")
                            .and_then(Value::as_str)
                            .unwrap_or_default()
                            .to_owned(),
                        // `PendingPerm::tool_type` serializes as `"type"` via `#[serde(rename = "type")]`.
                        tool: p
                            .get("type")
                            .or_else(|| p.get("tool_type"))
                            .and_then(Value::as_str)
                            .unwrap_or_default()
                            .to_owned(),
                        summary: p
                            .get("detail")
                            .and_then(Value::as_str)
                            .unwrap_or_default()
                            .to_owned(),
                        kind: p
                            .get("kind")
                            .and_then(Value::as_str)
                            .unwrap_or("permission")
                            .to_owned(),
                        danger: p
                            .get("danger")
                            .and_then(Value::as_str)
                            .filter(|s| !s.is_empty())
                            .map(str::to_owned),
                        tool_input: p.get("tool_input").cloned(),
                        meta: p.get("meta").cloned(),
                    })
                })
                .collect()
        })
        .unwrap_or_default();

    if let Some(stuck_arr) = val.get("stuck").and_then(Value::as_array) {
        for st in stuck_arr {
            let sess = st
                .get("sess")
                .and_then(Value::as_str)
                .unwrap_or_default()
                .to_owned();
            if sess.is_empty() {
                continue;
            }
            let id = st
                .get("id")
                .and_then(Value::as_str)
                .filter(|s| !s.is_empty())
                .map(str::to_owned)
                .unwrap_or_else(|| format!("stuck:{sess}"));
            if out.iter().any(|p| p.id == id) {
                continue;
            }
            let n = st.get("n").and_then(Value::as_u64).unwrap_or(0);
            let mins = st.get("mins").and_then(Value::as_u64).unwrap_or(0);
            let kind = st.get("kind").and_then(Value::as_str).unwrap_or("loop");
            out.push(PendingPermCard {
                id,
                session_id: sess,
                agent: st
                    .get("name")
                    .and_then(Value::as_str)
                    .unwrap_or_default()
                    .to_owned(),
                tool: "loop".to_owned(),
                summary: st
                    .get("label")
                    .and_then(Value::as_str)
                    .unwrap_or_default()
                    .to_owned(),
                kind: "stuck".to_owned(),
                danger: None,
                tool_input: None,
                meta: Some(json!({
                    "stuck_kind": kind,
                    "stuck_n": n,
                    "stuck_next": n + 1,
                    "stuck_mins": mins,
                    "stuck_usd": st.get("usd").cloned().unwrap_or(Value::Null),
                })),
            });
        }
    }
    out
}

fn parse_ci_alerts_from_chat_json(val: &Value) -> Vec<PendingPermCard> {
    let Some(arr) = val.get("msgs").and_then(Value::as_array) else {
        return Vec::new();
    };
    arr.iter()
        .filter_map(|m| {
            if m.get("kind").and_then(Value::as_str) != Some("ci") {
                return None;
            }
            let ci = m.get("ci")?;
            if ci.get("state").and_then(Value::as_str) != Some("red")
                || ci.get("fix").and_then(Value::as_str).is_some()
            {
                return None;
            }
            let id = m.get("id").and_then(Value::as_str)?.to_owned();
            let wf = ci.get("wf").and_then(Value::as_str).unwrap_or("CI");
            let repo = ci.get("repo").and_then(Value::as_str).unwrap_or("");
            let branch = ci.get("branch").and_then(Value::as_str).unwrap_or("");
            let sess = ci
                .get("sid")
                .or_else(|| ci.get("sess"))
                .and_then(Value::as_str)
                .unwrap_or_default();
            Some(PendingPermCard {
                id,
                session_id: sess.to_owned(),
                agent: repo.to_owned(),
                tool: wf.to_owned(),
                summary: format!("{wf} · {repo} ({branch})"),
                kind: "ci".to_owned(),
                danger: None,
                tool_input: None,
                meta: Some(ci.clone()),
            })
        })
        .collect()
}

fn parse_todos_from_values(arr: &[Value]) -> Vec<TodoItem> {
    arr.iter()
        .enumerate()
        .filter_map(|(idx, t)| {
            let text = t.get("text").and_then(Value::as_str)?.trim();
            if text.is_empty() {
                return None;
            }
            let id = t
                .get("id")
                .and_then(Value::as_str)
                .filter(|s| !s.is_empty())
                .map(str::to_owned)
                .unwrap_or_else(|| format!("td-{idx}"));
            let done = t.get("done").and_then(Value::as_bool).unwrap_or(false);
            let done_at = t.get("doneAt").and_then(Value::as_u64).unwrap_or(0);
            Some(TodoItem {
                id,
                text: text.to_owned(),
                done,
                done_at,
            })
        })
        .collect()
}

fn todos_to_json_values(todos: &[TodoItem]) -> Vec<Value> {
    todos
        .iter()
        .map(|t| {
            json!({
                "id": t.id,
                "text": t.text,
                "done": t.done,
                "doneAt": t.done_at,
            })
        })
        .collect()
}

fn parse_wrapped_from_json(val: &Value) -> WrappedSummaryUi {
    let stats = val.get("stats");
    let top_project = val
        .get("projects")
        .and_then(Value::as_array)
        .and_then(|arr| arr.first())
        .and_then(|p| {
            let name = p.get("name").and_then(Value::as_str)?.to_owned();
            let hrs = p.get("hours").and_then(Value::as_f64).unwrap_or(0.0);
            Some((name, hrs))
        });
    let busiest_day = val
        .get("busiest")
        .filter(|v| v.is_object())
        .and_then(|b| {
            let day = b.get("day").and_then(Value::as_str)?.to_owned();
            let wd = b.get("weekday").and_then(Value::as_u64).unwrap_or(0) as u32;
            let hrs = b.get("hours").and_then(Value::as_f64).unwrap_or(0.0);
            Some((day, wd, hrs))
        });
    WrappedSummaryUi {
        loading: false,
        loaded: true,
        error: None,
        from: val.get("from").and_then(Value::as_str).unwrap_or_default().to_owned(),
        to: val.get("to").and_then(Value::as_str).unwrap_or_default().to_owned(),
        days: val.get("days").and_then(Value::as_u64).unwrap_or(0) as usize,
        hours_sum: val.get("hoursSum").and_then(Value::as_f64).unwrap_or(0.0),
        hours_wall: val.get("hoursWall").and_then(Value::as_f64).unwrap_or(0.0),
        wait_hours: val.get("waitHours").and_then(Value::as_f64).unwrap_or(0.0),
        prompts: stats.and_then(|s| s.get("prompts")).and_then(Value::as_u64).unwrap_or(0),
        edits: stats.and_then(|s| s.get("edit")).and_then(Value::as_u64).unwrap_or(0),
        reads: stats.and_then(|s| s.get("read")).and_then(Value::as_u64).unwrap_or(0),
        cmds: stats.and_then(|s| s.get("cmd")).and_then(Value::as_u64).unwrap_or(0),
        deploys: stats.and_then(|s| s.get("deploy")).and_then(Value::as_u64).unwrap_or(0),
        commits: val.get("commits").and_then(Value::as_u64).unwrap_or(0),
        pushes: val.get("pushes").and_then(Value::as_u64).unwrap_or(0),
        longest_min: val.get("longestMin").and_then(Value::as_u64).unwrap_or(0),
        cost: val.get("cost").and_then(Value::as_f64).unwrap_or(0.0),
        sessions: val.get("sessions").and_then(Value::as_u64).unwrap_or(0) as usize,
        top_project,
        busiest_day,
        last_saved_path: None,
    }
}

fn sanitize_wrapped_date_tag(raw: &str) -> String {
    let trimmed = raw.trim();
    if station_core::http::api::is_valid_day_key(trimmed) {
        return trimmed.to_owned();
    }
    let clean: String = trimmed
        .chars()
        .filter(|c| c.is_ascii_alphanumeric() || *c == '-' || *c == '_')
        .take(32)
        .collect();
    if clean.is_empty() {
        "latest".to_owned()
    } else {
        clean
    }
}

fn decode_bounded_chat_preview(id: String, bytes: &[u8]) -> Result<ChatImagePreview, String> {
    if bytes.is_empty() || bytes.len() > MAX_CHAT_IMAGE_BYTES {
        return Err("image size limit exceeded".to_owned());
    }
    let mut reader = image::ImageReader::new(std::io::Cursor::new(bytes))
        .with_guessed_format()
        .map_err(|e| e.to_string())?;
    let mut limits = image::Limits::default();
    limits.max_image_width = Some(4096);
    limits.max_image_height = Some(4096);
    limits.max_alloc = Some(32 * 1024 * 1024);
    reader.limits(limits);
    let img = reader.decode().map_err(|e| e.to_string())?;
    let thumb = img.thumbnail(360, 270).to_rgba8();
    let (w, h) = (thumb.width() as usize, thumb.height() as usize);
    Ok(ChatImagePreview {
        id,
        width: w,
        height: h,
        rgba: thumb.into_raw(),
    })
}

fn decode_chat_data_url(id: String, data_url: &str) -> Result<ChatImagePreview, String> {
    if data_url.len() >= MAX_CHAT_DATA_URL_LEN {
        return Err("image size limit exceeded".to_owned());
    }
    let rest = data_url
        .strip_prefix("data:image/jpeg;base64,")
        .or_else(|| data_url.strip_prefix("data:image/png;base64,"))
        .ok_or_else(|| "unsupported image format".to_owned())?;
    let bytes = base64::engine::general_purpose::STANDARD
        .decode(rest)
        .map_err(|e| e.to_string())?;
    decode_bounded_chat_preview(id, &bytes)
}

async fn read_response_bytes_bounded(
    mut resp: reqwest::Response,
    max_bytes: usize,
) -> Result<Vec<u8>, String> {
    if resp.content_length().is_some_and(|n| n > max_bytes as u64) {
        return Err("image size limit exceeded".to_owned());
    }
    let mut buf = Vec::new();
    while let Some(chunk) = resp.chunk().await.map_err(|e| e.to_string())? {
        if buf.len().saturating_add(chunk.len()) > max_bytes {
            return Err("image size limit exceeded".to_owned());
        }
        buf.extend_from_slice(&chunk);
    }
    Ok(buf)
}

fn parse_accounts_from_view(val: &Value) -> Vec<AccountOption> {
    let active_slug = val.get("active").and_then(Value::as_str).unwrap_or("");
    let labels = val.get("labels").and_then(Value::as_object);
    let mut out = Vec::new();
    let mut has_main = false;

    if let Some(arr) = val.get("accounts").and_then(Value::as_array) {
        for a in arr {
            let Some(name) = a.get("name").and_then(Value::as_str) else {
                continue;
            };
            if name.is_empty() {
                has_main = true;
            }
            let fallback = if name.is_empty() { "Main" } else { name };
            let lbl = a
                .get("label")
                .and_then(Value::as_str)
                .filter(|s| !s.is_empty())
                .or_else(|| {
                    if name.is_empty() {
                        labels
                            .and_then(|m| m.get("_main").or_else(|| m.get("")))
                            .and_then(Value::as_str)
                    } else {
                        labels.and_then(|m| m.get(name)).and_then(Value::as_str)
                    }
                })
                .filter(|s| !s.is_empty())
                .unwrap_or(fallback)
                .to_owned();
            let active = a
                .get("active")
                .and_then(Value::as_bool)
                .unwrap_or(active_slug == name);
            out.push(AccountOption {
                slug: name.to_owned(),
                label: lbl,
                active,
                pct: None,
            });
        }
    }

    if !has_main {
        let main_lbl = labels
            .and_then(|m| m.get("_main").or_else(|| m.get("")))
            .and_then(Value::as_str)
            .filter(|s| !s.is_empty())
            .unwrap_or("Main")
            .to_owned();
        out.insert(
            0,
            AccountOption {
                slug: String::new(),
                label: main_lbl,
                active: active_slug.is_empty(),
                pct: None,
            },
        );
    }
    out
}

fn parse_log_items_from_json(session_id: &str, val: &Value) -> Vec<SessionLogItem> {
    if let Some(arr) = val.get("items").and_then(Value::as_array) {
        return arr
            .iter()
            .filter_map(|it| {
                let text = it.get("text").and_then(Value::as_str)?.trim();
                if text.is_empty() {
                    return None;
                }
                let role = it
                    .get("role")
                    .and_then(Value::as_str)
                    .unwrap_or("assistant")
                    .to_owned();
                let ts = it
                    .get("ts")
                    .and_then(Value::as_u64)
                    .map(format_hhmm_ms)
                    .unwrap_or_default();
                let tool_type = it
                    .get("type")
                    .and_then(Value::as_str)
                    .map(str::to_owned);
                Some(SessionLogItem {
                    role,
                    text: text.to_owned(),
                    ts,
                    tool_type,
                })
            })
            .collect();
    }
    if let Some(arr) = val.get("feed").and_then(Value::as_array) {
        return arr
            .iter()
            .filter(|f| f.get("sess").and_then(Value::as_str) == Some(session_id))
            .filter_map(|f| {
                let label = f.get("label").and_then(Value::as_str)?.trim();
                if label.is_empty() {
                    return None;
                }
                let kind = f.get("type").and_then(Value::as_str).unwrap_or("tool");
                let role = if kind == "prompt" { "user" } else { "tool" }.to_owned();
                let ts = format_hhmm_ms(f.get("ts").and_then(Value::as_u64).unwrap_or(0));
                Some(SessionLogItem {
                    role,
                    text: label.to_owned(),
                    ts,
                    tool_type: Some(kind.to_owned()),
                })
            })
            .collect();
    }
    Vec::new()
}

fn undo_view_from_plan(
    session_id: &str,
    plan: Option<station_core::state::undo::UndoPlan>,
) -> SessionUndoView {
    match plan {
        Some(p) => SessionUndoView {
            session_id: session_id.to_owned(),
            loading: false,
            prompt: p.prompt,
            time_label: format_hhmm_ms(p.at),
            files: p
                .files
                .into_iter()
                .map(|f| {
                    let selected = matches!(f.action.as_str(), "restore" | "delete");
                    UndoFileEntry {
                        file: f.file,
                        action: f.action,
                        selected,
                    }
                })
                .collect(),
            ..Default::default()
        },
        None => SessionUndoView {
            session_id: session_id.to_owned(),
            loading: false,
            ..Default::default()
        },
    }
}

fn parse_undo_view_from_json(session_id: &str, val: &Value) -> SessionUndoView {
    let plan = val
        .get("plan")
        .filter(|v| !v.is_null())
        .and_then(|v| serde_json::from_value::<station_core::state::undo::UndoPlan>(v.clone()).ok());
    undo_view_from_plan(session_id, plan)
}

fn parse_undo_apply_from_json(session_id: &str, val: &Value) -> SessionUndoView {
    let done_files = val
        .get("done")
        .and_then(Value::as_array)
        .map(|arr| {
            arr.iter()
                .filter_map(|x| {
                    let f = x.get("file").and_then(Value::as_str)?.to_owned();
                    let a = x.get("action").and_then(Value::as_str).unwrap_or("restore").to_owned();
                    Some((f, a))
                })
                .collect()
        })
        .unwrap_or_default();
    let backup_dir = val
        .get("after")
        .and_then(Value::as_str)
        .map(str::to_owned);
    let error = if val.get("ok").and_then(Value::as_bool) == Some(false) {
        Some(
            val.get("message")
                .or_else(|| val.get("code"))
                .and_then(Value::as_str)
                .unwrap_or("undo failed")
                .to_owned(),
        )
    } else {
        None
    };
    SessionUndoView {
        session_id: session_id.to_owned(),
        loading: false,
        done_files,
        backup_dir,
        error,
        ..Default::default()
    }
}

fn parse_feed_from_json(val: &Value) -> Vec<FeedEntry> {
    let Some(arr) = val.get("feed").and_then(Value::as_array) else {
        return Vec::new();
    };
    arr.iter()
        .take(12)
        .filter_map(|f| {
            let ts = f.get("ts").and_then(Value::as_u64).unwrap_or(0);
            // `FeedItem::kind` serializes as `"type"` via `#[serde(rename = "type")]`.
            let kind = f
                .get("type")
                .or_else(|| f.get("kind"))
                .and_then(Value::as_str)
                .unwrap_or("info");
            Some(FeedEntry {
                ts_label: format_hhmm_ms(ts),
                agent: f.get("name").and_then(Value::as_str)?.to_owned(),
                message: f.get("label").and_then(Value::as_str)?.to_owned(),
                level: if kind == "error" || kind == "fail" {
                    "err".to_owned()
                } else if kind == "wait" {
                    "warn".to_owned()
                } else {
                    "info".to_owned()
                },
                kind: normalize_feed_kind(kind).to_owned(),
                attention: matches!(kind, "error" | "fail" | "wait"),
            })
        })
        .collect()
}

fn parse_harnesses_from_json(val: &Value) -> Vec<HarnessDockItem> {
    let Some(arr) = val.get("harnesses").and_then(Value::as_array) else {
        return Vec::new();
    };
    arr.iter()
        .filter_map(|h| {
            let id = h.get("id").and_then(Value::as_str)?.to_owned();
            let label = h.get("label").and_then(Value::as_str)?.to_owned();
            let status = match h.get("status").and_then(Value::as_str).unwrap_or("") {
                "running_hooked" => HarnessStatus::RunningHooked,
                "installed_hooked" => HarnessStatus::InstalledHooked,
                "needs_hook" => HarnessStatus::NeedsHook,
                _ => HarnessStatus::NotInstalled,
            };
            let custom = h.get("custom").and_then(Value::as_bool).unwrap_or(false);
            let hook_agent = h
                .get("hook_agent")
                .and_then(Value::as_str)
                .map(str::to_owned);
            Some(HarnessDockItem {
                id,
                label,
                status,
                custom,
                hook_agent,
            })
        })
        .collect()
}

fn apply_snapshot_to_remote(snap_lock: &Arc<Mutex<RemoteSnapshot>>, val: &Value) {
    let sessions = parse_sessions_from_json(val);
    let pending_perms = parse_perms_from_json(val);
    let feed = parse_feed_from_json(val);
    let pin = val
        .get("pin")
        .and_then(Value::as_str)
        .filter(|s| !s.is_empty())
        .map(str::to_owned);
    let pins: Option<HashMap<String, Vec<String>>> = val
        .get("pins")
        .and_then(|p| serde_json::from_value(p.clone()).ok());
    let todos: Option<Vec<TodoItem>> = val
        .get("todos")
        .and_then(Value::as_array)
        .map(|a| parse_todos_from_values(a));
    let mut snap = snap_lock.lock().unwrap_or_else(|e| e.into_inner());
    snap.has_snapshot = true;
    snap.sessions = sessions;
    snap.pending_perms = pending_perms;
    snap.feed = feed;
    if let Some(p) = pin {
        snap.pairing_pin = Some(p);
    }
    if let Some(pm) = pins {
        snap.has_pins = true;
        snap.pins = pm;
    }
    if let Some(t) = todos {
        snap.has_todos = true;
        snap.todos = t;
    }
}

fn parse_chat_from_json(val: &Value) -> (Vec<ChatMessage>, Option<String>, Vec<String>) {
    let pinned = val
        .get("pinned")
        .and_then(Value::as_str)
        .filter(|s| !s.is_empty())
        .map(str::to_owned);
    let typing = val
        .get("typing")
        .and_then(Value::as_array)
        .map(|arr| {
            arr.iter()
                .filter_map(Value::as_str)
                .map(str::to_owned)
                .collect()
        })
        .unwrap_or_default();
    let Some(arr) = val.get("msgs").and_then(Value::as_array) else {
        return (Vec::new(), pinned, typing);
    };
    let msgs = arr
        .iter()
        .rev()
        .take(80)
        .collect::<Vec<_>>()
        .into_iter()
        .rev()
        .filter_map(|m| {
            let id = m.get("id").and_then(Value::as_str).unwrap_or_default().to_owned();
            let role = m
                .get("kind")
                .and_then(Value::as_str)
                .unwrap_or("text")
                .to_owned();
            let text = m.get("text").and_then(Value::as_str).unwrap_or("").to_owned();
            let recalled = m.get("recalled").and_then(Value::as_bool).unwrap_or(false);
            let sticker = m.get("sticker").and_then(Value::as_u64).map(|v| v as u32);
            let photo = m
                .get("photo")
                .and_then(Value::as_str)
                .filter(|s| !s.is_empty())
                .map(str::to_owned);
            let poll = m.get("poll").and_then(|p| {
                let q = p.get("q").and_then(Value::as_str)?.to_owned();
                let opts = p
                    .get("opts")
                    .and_then(Value::as_array)?
                    .iter()
                    .filter_map(|o| {
                        let t = o.get("t").and_then(Value::as_str)?.to_owned();
                        let by = o
                            .get("by")
                            .and_then(Value::as_array)
                            .map(|a| a.iter().filter_map(Value::as_str).map(str::to_owned).collect())
                            .unwrap_or_default();
                        Some(ChatPollOptUi { t, by })
                    })
                    .collect();
                Some(ChatPollUi { q, opts })
            });
            let remind = m.get("remind").and_then(|r| {
                let title = r.get("title").and_then(Value::as_str)?.to_owned();
                let at_ms = r.get("at").and_then(Value::as_u64).unwrap_or(0);
                let repeat = r.get("repeat").and_then(Value::as_str).unwrap_or("").to_owned();
                let yes = r
                    .get("yes")
                    .and_then(Value::as_array)
                    .map(|a| a.iter().filter_map(Value::as_str).map(str::to_owned).collect())
                    .unwrap_or_default();
                let no = r
                    .get("no")
                    .and_then(Value::as_array)
                    .map(|a| a.iter().filter_map(Value::as_str).map(str::to_owned).collect())
                    .unwrap_or_default();
                let done = r.get("done").and_then(Value::as_bool).unwrap_or(false);
                Some(ChatRemindUi {
                    title,
                    at_ms,
                    repeat,
                    yes,
                    no,
                    done,
                })
            });
            let reply = m.get("reply").and_then(|r| {
                Some(ChatReplyRefUi {
                    id: r.get("id").and_then(Value::as_str)?.to_owned(),
                    name: r.get("name").and_then(Value::as_str).unwrap_or("").to_owned(),
                    text: r.get("text").and_then(Value::as_str).unwrap_or("").to_owned(),
                })
            });
            let mut reacts = std::collections::BTreeMap::new();
            if let Some(obj) = m.get("reacts").and_then(Value::as_object) {
                for (k, v) in obj {
                    if let Some(list) = v.as_array() {
                        let who: Vec<String> = list
                            .iter()
                            .filter_map(Value::as_str)
                            .map(str::to_owned)
                            .collect();
                        if !who.is_empty() {
                            reacts.insert(k.clone(), who);
                        }
                    }
                }
            }
            if text.is_empty()
                && !recalled
                && sticker.is_none()
                && photo.is_none()
                && poll.is_none()
                && remind.is_none()
            {
                return None;
            }
            let from = m
                .get("from")
                .and_then(Value::as_str)
                .unwrap_or("office")
                .to_owned();
            let sender = m
                .get("name")
                .and_then(Value::as_str)
                .or_else(|| m.get("from").and_then(Value::as_str))
                .unwrap_or("Office")
                .to_owned();
            let at_ms = m.get("at").and_then(Value::as_u64).unwrap_or(0);
            let ts = format_hhmm_ms(at_ms);
            let tag = m.get("tag").and_then(Value::as_str).map(str::to_owned);
            Some(ChatMessage {
                id,
                from,
                sender,
                role,
                text,
                ts,
                at_ms,
                tag,
                recalled,
                sticker,
                photo,
                reply,
                reacts,
                poll,
                remind,
            })
        })
        .collect();
    (msgs, pinned, typing)
}

fn parse_sys_from_json(val: &Value) -> SysSnapshot {
    let gb = 1024.0 * 1024.0 * 1024.0;
    let mem_used_gb = val
        .pointer("/mem/used")
        .and_then(Value::as_u64)
        .map(|b| (b as f64 / gb) as f32)
        .unwrap_or(0.0);
    let mem_total_gb = val
        .pointer("/mem/total")
        .and_then(Value::as_u64)
        .map(|b| (b as f64 / gb) as f32)
        .unwrap_or(0.0);
    let disk_total = val.pointer("/disk/total").and_then(Value::as_u64).unwrap_or(0);
    let disk_used = val.pointer("/disk/used").and_then(Value::as_u64).unwrap_or(0);
    let disk_pct = if disk_total > 0 {
        ((disk_used as f64 / disk_total as f64) * 100.0) as f32
    } else {
        0.0
    };
    let cpu_pct = val
        .pointer("/cpu/pct")
        .or_else(|| val.pointer("/cpu/used"))
        .and_then(Value::as_f64)
        .unwrap_or(0.0) as f32;
    let mut ports = Vec::new();
    if let Some(arr) = val.get("devServers").and_then(Value::as_array) {
        for srv in arr {
            let label = srv
                .get("label")
                .and_then(Value::as_str)
                .unwrap_or("dev")
                .to_owned();
            if let Some(ps) = srv.get("ports").and_then(Value::as_array) {
                for p in ps {
                    if let Some(port) = p.as_u64().and_then(|v| u16::try_from(v).ok()) {
                        ports.push((port, label.clone()));
                    }
                }
            }
        }
    }
    SysSnapshot {
        cpu_pct,
        mem_used_gb,
        mem_total_gb,
        disk_pct,
        ports,
        private_mb: current_process_private_mb(),
    }
}

fn parse_burn_from_json(today_val: &Value, month_val: Option<&Value>) -> BurnSnapshot {
    let today_usd = today_val
        .pointer("/total/usd")
        .and_then(Value::as_f64)
        .unwrap_or(0.0);
    let month_usd = month_val
        .and_then(|v| v.pointer("/total/usd").and_then(Value::as_f64))
        .unwrap_or(today_usd);
    let input_tokens = today_val
        .pointer("/total/tok/in")
        .and_then(Value::as_f64)
        .unwrap_or(0.0) as u64;
    let output_tokens = today_val
        .pointer("/total/tok/out")
        .and_then(Value::as_f64)
        .unwrap_or(0.0) as u64;
    let cache_read_tokens = today_val
        .pointer("/total/tok/cacheRead")
        .and_then(Value::as_f64)
        .unwrap_or(0.0) as u64;
    let per_model = today_val
        .pointer("/by/model")
        .and_then(Value::as_array)
        .map(|arr| {
            arr.iter()
                .filter_map(|row| {
                    let name = row.get("name").and_then(Value::as_str)?.to_owned();
                    let usd = row.get("usd").and_then(Value::as_f64).unwrap_or(0.0);
                    Some((name, usd))
                })
                .collect()
        })
        .unwrap_or_default();
    BurnSnapshot {
        today_usd,
        month_usd,
        input_tokens,
        output_tokens,
        cache_read_tokens,
        per_model,
    }
}

fn parse_vault_from_json(val: &Value) -> Vec<(String, String)> {
    let Some(arr) = val.get("items").and_then(Value::as_array) else {
        return Vec::new();
    };
    arr.iter()
        .take(30)
        .filter_map(|e| {
            let title = e
                .get("title")
                .and_then(Value::as_str)
                .filter(|s| !s.is_empty())
                .or_else(|| e.get("sid").and_then(Value::as_str))?
                .to_owned();
            let body = e
                .get("first")
                .and_then(Value::as_str)
                .filter(|s| !s.is_empty())
                .or_else(|| e.get("cwd").and_then(Value::as_str))
                .unwrap_or("")
                .to_owned();
            Some((title, body))
        })
        .collect()
}

fn parse_burn_detail_rows(arr: Option<&Vec<Value>>) -> Vec<BurnDetailRow> {
    arr.map(|list| {
        list.iter()
            .filter_map(|v| {
                Some(BurnDetailRow {
                    name: v.get("name").and_then(Value::as_str)?.to_owned(),
                    kind: v
                        .get("kind")
                        .and_then(Value::as_str)
                        .unwrap_or_default()
                        .to_owned(),
                    usd: v.get("usd").and_then(Value::as_f64).unwrap_or(0.0),
                    tokens: v.get("tokens").and_then(Value::as_f64).unwrap_or(0.0),
                    share: v.get("share").and_then(Value::as_f64).unwrap_or(0.0),
                })
            })
            .collect()
    })
    .unwrap_or_default()
}

fn parse_burn_drawer_from_json(range: BurnRange, val: &Value) -> BurnDrawerState {
    let total = val.get("total");
    let plain = val.get("plain");
    let by = val.get("by");
    let ranked = parse_burn_detail_rows(val.get("ranked").and_then(Value::as_array));
    let by_model = parse_burn_detail_rows(
        by.and_then(|b| b.get("model"))
            .and_then(Value::as_array),
    );
    let by_source = parse_burn_detail_rows(
        by.and_then(|b| b.get("source"))
            .and_then(Value::as_array),
    );
    let roles = val
        .get("roles")
        .and_then(Value::as_array)
        .map(|arr| {
            arr.iter()
                .filter_map(|v| {
                    Some(BurnRoleRowUi {
                        role: v.get("role").and_then(Value::as_str)?.to_owned(),
                        model: v.get("model").and_then(Value::as_str).unwrap_or("").to_owned(),
                        usd: v.get("usd").and_then(Value::as_f64).unwrap_or(0.0),
                        tokens: v.get("tokens").and_then(Value::as_f64).unwrap_or(0.0),
                        share: v.get("share").and_then(Value::as_f64).unwrap_or(0.0),
                    })
                })
                .collect()
        })
        .unwrap_or_default();
    let projects = val
        .get("projects")
        .and_then(Value::as_array)
        .map(|arr| {
            arr.iter()
                .filter_map(|v| {
                    Some(BurnProjectRowUi {
                        name: v.get("name").and_then(Value::as_str).unwrap_or("").to_owned(),
                        cwd: v.get("cwd").and_then(Value::as_str).unwrap_or("").to_owned(),
                        usd: v.get("usd").and_then(Value::as_f64).unwrap_or(0.0),
                        share: v.get("share").and_then(Value::as_f64).unwrap_or(0.0),
                        sessions: v.get("sessions").and_then(Value::as_u64).unwrap_or(0) as usize,
                    })
                })
                .collect()
        })
        .unwrap_or_default();
    let sessions = val
        .get("sessions")
        .and_then(Value::as_array)
        .map(|arr| {
            arr.iter()
                .filter_map(|v| {
                    Some(BurnSessionRowUi {
                        sid: v.get("sid").and_then(Value::as_str)?.to_owned(),
                        name: v.get("name").and_then(Value::as_str).unwrap_or("").to_owned(),
                        project: v.get("project").and_then(Value::as_str).unwrap_or("").to_owned(),
                        cwd: v.get("cwd").and_then(Value::as_str).unwrap_or("").to_owned(),
                        usd: v.get("usd").and_then(Value::as_f64).unwrap_or(0.0),
                        tokens: v.get("tokens").and_then(Value::as_f64).unwrap_or(0.0),
                        req: v.get("req").and_then(Value::as_u64).unwrap_or(0),
                        share: v.get("share").and_then(Value::as_f64).unwrap_or(0.0),
                        models: v
                            .get("models")
                            .and_then(Value::as_array)
                            .map(|m| m.iter().filter_map(Value::as_str).map(str::to_owned).collect())
                            .unwrap_or_default(),
                    })
                })
                .collect()
        })
        .unwrap_or_default();
    let per_day = val
        .get("perDay")
        .and_then(Value::as_array)
        .map(|arr| {
            arr.iter()
                .filter_map(|d| {
                    let day = d.get("day").and_then(Value::as_str)?.to_owned();
                    let usd = d.get("usd").and_then(Value::as_f64).unwrap_or(0.0);
                    let mut models: Vec<(String, f64)> = d
                        .get("models")
                        .and_then(Value::as_object)
                        .map(|m| {
                            m.iter()
                                .filter_map(|(k, v)| Some((k.clone(), v.as_f64()?)))
                                .collect()
                        })
                        .unwrap_or_default();
                    models.sort_by(|a, b| a.0.cmp(&b.0));
                    Some(BurnDayBucketUi { day, usd, models })
                })
                .collect()
        })
        .unwrap_or_default();

    BurnDrawerState {
        range,
        loading: false,
        loaded_range: Some(range),
        error: None,
        setup: val
            .get("setup")
            .and_then(Value::as_str)
            .unwrap_or("none")
            .to_owned(),
        money: val.get("money").and_then(Value::as_bool).unwrap_or(true),
        total_usd: total.and_then(|t| t.get("usd")).and_then(Value::as_f64).unwrap_or(0.0),
        total_tokens: total.and_then(|t| t.get("tokens")).and_then(Value::as_f64).unwrap_or(0.0),
        req: total.and_then(|t| t.get("req")).and_then(Value::as_u64).unwrap_or(0),
        err: total.and_then(|t| t.get("err")).and_then(Value::as_u64).unwrap_or(0),
        prompts: total.and_then(|t| t.get("prompts")).and_then(Value::as_u64).unwrap_or(0),
        plain_usd: plain.and_then(|p| p.get("usd")).and_then(Value::as_f64).unwrap_or(0.0),
        plain_share: plain.and_then(|p| p.get("share")).and_then(Value::as_f64).unwrap_or(0.0),
        ranked,
        by_model,
        by_source,
        roles,
        projects,
        sessions,
        per_day,
    }
}

fn parse_vault_list_from_json(val: &Value) -> (Vec<VaultListItem>, usize, u64) {
    let stats = val.get("stats");
    let total_count = stats
        .and_then(|s| s.get("n"))
        .and_then(Value::as_u64)
        .unwrap_or(0) as usize;
    let total_bytes = stats
        .and_then(|s| s.get("bytes"))
        .and_then(Value::as_u64)
        .unwrap_or(0);
    let items = val
        .get("items")
        .and_then(Value::as_array)
        .map(|arr| {
            arr.iter()
                .filter_map(|e| {
                    let sid = e.get("sid").and_then(Value::as_str)?.to_owned();
                    Some(VaultListItem {
                        sid,
                        title: e.get("title").and_then(Value::as_str).unwrap_or("").to_owned(),
                        cwd: e.get("cwd").and_then(Value::as_str).unwrap_or("").to_owned(),
                        first: e.get("first").and_then(Value::as_str).unwrap_or("").to_owned(),
                        end_ms: e.get("end").and_then(Value::as_u64).unwrap_or(0),
                        msg_count: e.get("n").and_then(Value::as_u64).unwrap_or(0) as usize,
                        gone_ms: e.get("gone").and_then(Value::as_u64).unwrap_or(0),
                    })
                })
                .collect()
        })
        .unwrap_or_default();
    (items, total_count, total_bytes)
}

fn parse_vault_transcript_from_json(val: &Value) -> Option<VaultTranscriptUi> {
    let rec = val.get("rec")?;
    let sid = rec.get("sid").and_then(Value::as_str)?.to_owned();
    let msgs = rec
        .get("msgs")
        .and_then(Value::as_array)
        .map(|arr| {
            arr.iter()
                .filter_map(|m| {
                    Some(VaultMsgUi {
                        role: m.get("r").and_then(Value::as_str).unwrap_or("c").to_owned(),
                        text: m.get("t").and_then(Value::as_str)?.to_owned(),
                        at_ms: m.get("at").and_then(Value::as_u64).unwrap_or(0),
                    })
                })
                .collect()
        })
        .unwrap_or_default();
    Some(VaultTranscriptUi {
        sid,
        title: rec.get("title").and_then(Value::as_str).unwrap_or("").to_owned(),
        cwd: rec.get("cwd").and_then(Value::as_str).unwrap_or("").to_owned(),
        start_ms: rec.get("start").and_then(Value::as_u64).unwrap_or(0),
        end_ms: rec.get("end").and_then(Value::as_u64).unwrap_or(0),
        gone_ms: rec.get("gone").and_then(Value::as_u64).unwrap_or(0),
        msgs,
    })
}

fn parse_vault_report_from_json(val: &Value) -> PromptReportUi {
    let mut by: Vec<(String, usize)> = val
        .get("by")
        .and_then(Value::as_object)
        .map(|m| {
            m.iter()
                .map(|(k, v)| (k.clone(), v.as_u64().unwrap_or(0) as usize))
                .collect()
        })
        .unwrap_or_default();
    by.sort_by(|a, b| b.1.cmp(&a.1).then_with(|| a.0.cmp(&b.0)));
    let examples = val
        .get("examples")
        .and_then(Value::as_array)
        .map(|arr| {
            arr.iter()
                .filter_map(|e| {
                    Some(PromptReportExampleUi {
                        text: e.get("t").and_then(Value::as_str)?.to_owned(),
                        ph: e.get("ph").and_then(Value::as_u64).unwrap_or(0) as usize,
                        hint: e.get("hint").and_then(Value::as_str).unwrap_or("vague").to_owned(),
                        phrase: e.get("phrase").and_then(Value::as_str).unwrap_or("").to_owned(),
                        n: e.get("n").and_then(Value::as_u64).unwrap_or(0) as usize,
                        at_ms: e.get("at").and_then(Value::as_u64).unwrap_or(0),
                        title: e.get("title").and_then(Value::as_str).unwrap_or("").to_owned(),
                    })
                })
                .collect()
        })
        .unwrap_or_default();
    PromptReportUi {
        loaded: true,
        loading: false,
        error: None,
        n: val.get("n").and_then(Value::as_u64).unwrap_or(0) as usize,
        clear: val.get("clear").and_then(Value::as_u64).unwrap_or(0) as usize,
        pct: val.get("pct").and_then(Value::as_u64).map(|v| v as u32),
        convs: val.get("convs").and_then(Value::as_u64).unwrap_or(0) as usize,
        by,
        examples,
    }
}

fn parse_mac_drawer_from_json(val: &Value) -> MacDrawerState {
    let gb = 1024.0 * 1024.0 * 1024.0;
    let swap_used_gb = (val
        .get("mem")
        .and_then(|m| m.get("swapUsed").or_else(|| m.get("swap_used")))
        .and_then(Value::as_u64)
        .unwrap_or(0) as f64
        / gb) as f32;
    let swap_total_gb = (val
        .get("mem")
        .and_then(|m| m.get("swapTotal").or_else(|| m.get("swap_total")))
        .and_then(Value::as_u64)
        .unwrap_or(0) as f64
        / gb) as f32;
    let mem_pressure = val
        .get("mem")
        .and_then(|m| m.get("pressure"))
        .and_then(Value::as_str)
        .unwrap_or("normal")
        .to_owned();
    let disk_free_gb = (val
        .get("disk")
        .and_then(|d| d.get("free"))
        .and_then(Value::as_u64)
        .unwrap_or(0) as f64
        / gb) as f32;
    let disk_total_gb = (val
        .get("disk")
        .and_then(|d| d.get("total"))
        .and_then(Value::as_u64)
        .unwrap_or(0) as f64
        / gb) as f32;
    let cpu_cores = val
        .get("cpu")
        .and_then(|c| c.get("cores"))
        .and_then(Value::as_u64)
        .unwrap_or(1) as usize;
    let cpu_load = val
        .get("cpu")
        .and_then(|c| c.get("load"))
        .and_then(Value::as_f64)
        .unwrap_or(0.0) as f32;
    let auto_close_hours = val
        .get("auto")
        .and_then(|a| a.get("hours"))
        .and_then(Value::as_u64)
        .unwrap_or(0) as u8;
    let claude_procs = val
        .get("claude")
        .and_then(Value::as_array)
        .map(|arr| {
            arr.iter()
                .filter_map(|p| {
                    Some(SysProcRow {
                        name: p
                            .get("title")
                            .or_else(|| p.get("name"))
                            .and_then(Value::as_str)?
                            .to_owned(),
                        pid: p.get("pid").and_then(Value::as_u64).unwrap_or(0) as u32,
                        start: p.get("start").and_then(Value::as_u64).unwrap_or(0),
                        count: 1,
                        cpu: p.get("cpu").and_then(Value::as_f64).unwrap_or(0.0) as f32,
                        mem_bytes: p.get("mem").and_then(Value::as_u64).unwrap_or(0),
                        can_quit: true,
                        busy: p.get("busy").and_then(Value::as_bool).unwrap_or(false),
                        idle_sec: p.get("idle").and_then(Value::as_u64).unwrap_or(0),
                        dir: p.get("dir").and_then(Value::as_str).unwrap_or("").to_owned(),
                    })
                })
                .collect()
        })
        .unwrap_or_default();
    let apps = val
        .get("apps")
        .and_then(Value::as_array)
        .map(|arr| {
            arr.iter()
                .filter_map(|a| {
                    Some(SysProcRow {
                        name: a.get("name").and_then(Value::as_str)?.to_owned(),
                        pid: 0,
                        start: 0,
                        count: a.get("count").and_then(Value::as_u64).unwrap_or(1) as usize,
                        cpu: a.get("cpu").and_then(Value::as_f64).unwrap_or(0.0) as f32,
                        mem_bytes: a.get("mem").and_then(Value::as_u64).unwrap_or(0),
                        can_quit: a.get("quit").and_then(Value::as_bool).unwrap_or(false),
                        busy: false,
                        idle_sec: 0,
                        dir: String::new(),
                    })
                })
                .collect()
        })
        .unwrap_or_default();
    let dev_servers = val
        .get("devServers")
        .and_then(Value::as_array)
        .map(|arr| {
            arr.iter()
                .filter_map(|d| {
                    Some(DevServerUi {
                        pid: d.get("pid").and_then(Value::as_u64).unwrap_or(0) as u32,
                        start: d.get("start").and_then(Value::as_u64).unwrap_or(0),
                        label: d.get("label").and_then(Value::as_str)?.to_owned(),
                        ports: d
                            .get("ports")
                            .and_then(Value::as_array)
                            .map(|ps| ps.iter().filter_map(Value::as_u64).map(|p| p as u16).collect())
                            .unwrap_or_default(),
                        dir: d.get("dir").and_then(Value::as_str).unwrap_or("").to_owned(),
                        up_sec: d.get("up").and_then(Value::as_u64).unwrap_or(0),
                        cpu: d.get("cpu").and_then(Value::as_i64).unwrap_or(0),
                        mem_bytes: d.get("mem").and_then(Value::as_u64).unwrap_or(0),
                        conns: d.get("conns").and_then(Value::as_u64).unwrap_or(0) as u32,
                        idle_sec: d.get("idle").and_then(Value::as_u64).unwrap_or(0),
                    })
                })
                .collect()
        })
        .unwrap_or_default();
    let cleanups = val
        .get("cleanups")
        .and_then(Value::as_array)
        .map(|arr| {
            arr.iter()
                .filter_map(|c| {
                    Some(CleanupRowUi {
                        id: c.get("id").and_then(Value::as_str)?.to_owned(),
                        title: c.get("title").and_then(Value::as_str).unwrap_or("").to_owned(),
                        note: c.get("note").and_then(Value::as_str).unwrap_or("").to_owned(),
                        size_bytes: c.get("size").and_then(Value::as_u64).unwrap_or(0),
                    })
                })
                .collect()
        })
        .unwrap_or_default();

    MacDrawerState {
        loading: false,
        swap_used_gb,
        swap_total_gb,
        mem_pressure,
        disk_free_gb,
        disk_total_gb,
        cpu_cores,
        cpu_load,
        auto_close_hours,
        claude_procs,
        apps,
        dev_servers,
        cleanups,
        ..Default::default()
    }
}

fn parse_stranded_from_json(val: &Value) -> (Vec<StrandedItemUi>, usize) {
    let repos = val.get("repos").and_then(Value::as_u64).unwrap_or(0) as usize;
    let items = val
        .get("items")
        .and_then(Value::as_array)
        .map(|arr| {
            arr.iter()
                .filter_map(|it| {
                    Some(StrandedItemUi {
                        id: it.get("id").and_then(Value::as_str)?.to_owned(),
                        path: it
                            .get("path")
                            .or_else(|| it.get("dir"))
                            .and_then(Value::as_str)
                            .unwrap_or("")
                            .to_owned(),
                        repo: it.get("repo").and_then(Value::as_str).unwrap_or("").to_owned(),
                        branch: it.get("branch").and_then(Value::as_str).unwrap_or("").to_owned(),
                        main: it.get("main").and_then(Value::as_bool).unwrap_or(false),
                        locked: it.get("locked").and_then(Value::as_bool).unwrap_or(false),
                        dirty: it.get("dirty").and_then(Value::as_u64).unwrap_or(0) as usize,
                        ahead: it.get("ahead").and_then(Value::as_u64).unwrap_or(0) as u32,
                        stashes: it.get("stashes").and_then(Value::as_u64).unwrap_or(0) as usize,
                        ok: it.get("ok").and_then(Value::as_bool).unwrap_or(true),
                        in_use: it.get("inUse").and_then(Value::as_bool).unwrap_or(false),
                        removable: it.get("removable").and_then(Value::as_bool).unwrap_or(false),
                        work: it.get("work").and_then(Value::as_bool).unwrap_or(false),
                    })
                })
                .collect()
        })
        .unwrap_or_default();
    (items, repos)
}

fn parse_quota_windows(a: &Value) -> Vec<QuotaWindowUi> {
    let mut out = Vec::new();
    if let Some(arr) = a
        .get("windows")
        .or_else(|| a.get("rows"))
        .and_then(Value::as_array)
    {
        for item in arr {
            let Some(pct) = item
                .get("pct")
                .or_else(|| item.get("used"))
                .or_else(|| item.get("utilization"))
                .and_then(Value::as_f64)
            else {
                continue;
            };
            let key = item
                .get("key")
                .or_else(|| item.get("id"))
                .and_then(Value::as_str)
                .unwrap_or("")
                .to_owned();
            let label = item
                .get("label")
                .or_else(|| item.get("name"))
                .and_then(Value::as_str)
                .unwrap_or(key.as_str())
                .to_owned();
            let resets_at_ms = item
                .get("resetsAt")
                .or_else(|| item.get("resets_at"))
                .or_else(|| item.get("resetAt"))
                .and_then(Value::as_u64)
                .unwrap_or(0);
            out.push(QuotaWindowUi {
                key,
                label,
                pct: pct.clamp(0.0, 100.0) as f32,
                resets_at_ms,
            });
        }
        return out;
    }

    // Fallback for object-shaped usage payloads (`usage.fiveHour` / `usage.weekly` / `pct`).
    let src = a.get("usage").or_else(|| a.get("limits")).unwrap_or(a);
    for (field, key) in [
        ("fiveHour", "session"),
        ("session", "session"),
        ("weekly", "weekly"),
        ("week", "weekly"),
    ] {
        if out.iter().any(|w| w.key == key) {
            continue;
        }
        if let Some(w) = src.get(field)
            && let Some(pct) = w
                .as_f64()
                .or_else(|| w.get("pct").or_else(|| w.get("used")).and_then(Value::as_f64))
        {
            out.push(QuotaWindowUi {
                key: key.to_owned(),
                label: key.to_owned(),
                pct: pct.clamp(0.0, 100.0) as f32,
                resets_at_ms: w
                    .get("resetsAt")
                    .or_else(|| w.get("resetAt"))
                    .and_then(Value::as_u64)
                    .unwrap_or(0),
            });
        }
    }
    if out.is_empty()
        && let Some(pct) = a.get("pct").and_then(Value::as_f64)
    {
        out.push(QuotaWindowUi {
            key: "session".to_owned(),
            label: "5h".to_owned(),
            pct: pct.clamp(0.0, 100.0) as f32,
            resets_at_ms: 0,
        });
    }
    out
}

fn parse_ai_slots(arr: Option<&Vec<Value>>) -> Vec<AiAccountEntry> {
    arr.map(|list| {
        list.iter()
            .filter_map(|a| {
                let slug = a
                    .get("slug")
                    .or_else(|| a.get("name"))
                    .and_then(Value::as_str)?
                    .to_owned();
                Some(AiAccountEntry {
                    slug,
                    label: a.get("label").and_then(Value::as_str).unwrap_or("").to_owned(),
                    email: a.get("email").and_then(Value::as_str).unwrap_or("").to_owned(),
                    plan: a.get("plan").and_then(Value::as_str).unwrap_or("").to_owned(),
                    active: a.get("active").and_then(Value::as_bool).unwrap_or(false),
                    has_credentials: a
                        .get("hasCredentials")
                        .and_then(Value::as_bool)
                        .unwrap_or(true),
                    note: a.get("note").and_then(Value::as_str).unwrap_or("").to_owned(),
                    windows: parse_quota_windows(a),
                })
            })
            .collect()
    })
    .unwrap_or_default()
}

fn resolve_ai_error_code(i18n: &I18nStore, code: &str) -> Option<String> {
    if code.is_empty() {
        return None;
    }
    let clean: String = code
        .chars()
        .filter(|c| c.is_ascii_alphanumeric())
        .collect();
    if clean.is_empty() {
        return None;
    }
    for prefix in ["server.ai", "server.account"] {
        let key = format!("{prefix}.{clean}");
        let val = i18n.tr_vars(&key, &[("app", "Antigravity"), ("n", "1")]);
        if val != key && !val.trim().is_empty() {
            return Some(val);
        }
    }
    None
}

fn parse_ai_accounts_from_json(val: &Value) -> BridgeAsyncResult {
    let claude = parse_ai_slots(val.get("accounts").and_then(Value::as_array));
    let claude_auto = val.get("auto").and_then(Value::as_bool).unwrap_or(false);
    let claude_move = val.get("move").and_then(Value::as_bool).unwrap_or(false);
    let claude_resume = val.get("resume").and_then(Value::as_bool).unwrap_or(false);

    let codex_v = val.get("codex");
    let codex = parse_ai_slots(codex_v.and_then(|c| c.get("accounts")).and_then(Value::as_array));
    let codex_auto = codex_v.and_then(|c| c.get("auto")).and_then(Value::as_bool).unwrap_or(false);
    let codex_seat_note = codex_v
        .and_then(|c| c.get("seat"))
        .and_then(Value::as_str)
        .unwrap_or("")
        .to_owned();

    let ag_v = val.get("ag");
    let ag = parse_ai_slots(ag_v.and_then(|g| g.get("accounts")).and_then(Value::as_array));
    let ag_auto = ag_v.and_then(|g| g.get("auto")).and_then(Value::as_bool).unwrap_or(false);
    let ag_away = ag_v.and_then(|g| g.get("away")).and_then(Value::as_bool).unwrap_or(false);
    let ag_pending = ag_v
        .and_then(|g| g.get("pending"))
        .and_then(Value::as_str)
        .map(str::to_owned);
    let ag_note = ag_v
        .and_then(|g| g.get("note"))
        .and_then(Value::as_str)
        .unwrap_or("")
        .to_owned();

    BridgeAsyncResult::AiAccountsData {
        claude,
        claude_auto,
        claude_move,
        claude_resume,
        codex,
        codex_auto,
        codex_seat_note,
        ag,
        ag_auto,
        ag_away,
        ag_pending,
        ag_note,
    }
}

fn parse_phone_status_from_json(tun: &Value, push: Option<&Value>) -> BridgeAsyncResult {
    BridgeAsyncResult::PhoneStatus {
        lan_on: tun.get("lanOn").and_then(Value::as_bool).unwrap_or(false),
        lan_ip: tun
            .get("lanIp")
            .and_then(Value::as_str)
            .filter(|s| !s.is_empty())
            .map(str::to_owned),
        tunnel_on: tun.get("on").and_then(Value::as_bool).unwrap_or(false),
        tunnel_state: tun
            .get("state")
            .and_then(Value::as_str)
            .unwrap_or("off")
            .to_owned(),
        tunnel_url: tun
            .get("url")
            .and_then(Value::as_str)
            .filter(|s| !s.is_empty())
            .map(str::to_owned),
        tunnel_domain: tun
            .get("domain")
            .and_then(Value::as_str)
            .unwrap_or("")
            .to_owned(),
        tunnel_error: tun
            .get("error")
            .and_then(Value::as_str)
            .filter(|s| !s.is_empty())
            .map(str::to_owned),
        phone_approve: tun
            .get("phoneApprove")
            .and_then(Value::as_bool)
            .unwrap_or(false),
        push_count: push
            .and_then(|p| p.get("count"))
            .and_then(Value::as_u64)
            .unwrap_or(0) as usize,
        pin: tun
            .get("pin")
            .and_then(Value::as_str)
            .filter(|s| !s.is_empty())
            .map(str::to_owned),
    }
}

fn build_in_process_ai_accounts(app_state: &AppState) -> BridgeAsyncResult {
    let mut view = app_state
        .claude_accounts
        .lock()
        .unwrap_or_else(|e| e.into_inner())
        .view();
    let codex_view = app_state
        .codex_accounts
        .lock()
        .unwrap_or_else(|e| e.into_inner())
        .view();
    let ag_view = app_state
        .ag_accounts
        .lock()
        .unwrap_or_else(|e| e.into_inner())
        .view();
    if let Some(obj) = view.as_object_mut() {
        obj.insert("codex".to_owned(), codex_view);
        obj.insert("ag".to_owned(), ag_view);
    }
    parse_ai_accounts_from_json(&view)
}

fn build_in_process_phone_status(app_state: &AppState) -> BridgeAsyncResult {
    let (lan_on, tunnel_on, tunnel_domain, phone_approve) = {
        let prefs = app_state.prefs.lock().unwrap_or_else(|e| e.into_inner());
        (
            prefs.get("lan").and_then(Value::as_bool).unwrap_or(false),
            prefs.get("tunnel").and_then(Value::as_bool).unwrap_or(false),
            prefs
                .get("tunnelDomain")
                .and_then(Value::as_str)
                .unwrap_or("")
                .to_owned(),
            prefs
                .get("phoneApprove")
                .and_then(Value::as_bool)
                .unwrap_or(false),
        )
    };
    let snap = app_state.tunnel.snapshot();
    let push_count = app_state
        .push
        .lock()
        .unwrap_or_else(|e| e.into_inner())
        .count();
    BridgeAsyncResult::PhoneStatus {
        lan_on,
        lan_ip: station_core::http::api::detect_private_lan_ipv4(),
        tunnel_on,
        tunnel_state: snap.state,
        tunnel_url: snap.url,
        tunnel_domain,
        tunnel_error: snap.error,
        phone_approve,
        push_count,
        pin: Some(app_state.auth.current_pin()),
    }
}

fn merge_mac_drawer_telemetry(dst: &mut MacDrawerState, src: MacDrawerState) {
    dst.loading = false;
    dst.swap_used_gb = src.swap_used_gb;
    dst.swap_total_gb = src.swap_total_gb;
    dst.mem_pressure = src.mem_pressure;
    dst.disk_free_gb = src.disk_free_gb;
    dst.disk_total_gb = src.disk_total_gb;
    dst.cpu_cores = src.cpu_cores;
    dst.cpu_load = src.cpu_load;
    dst.auto_close_hours = src.auto_close_hours;
    dst.claude_procs = src.claude_procs;
    dst.apps = src.apps;
    dst.dev_servers = src.dev_servers;
    dst.cleanups = src.cleanups;
}

fn collect_in_process_stranded_dirs(app_state: &AppState) -> (Vec<String>, Vec<String>) {
    let mut dirs = Vec::new();
    let mut in_use = Vec::new();
    {
        let office = app_state.office.lock().unwrap_or_else(|e| e.into_inner());
        dirs.extend(office.known_dirs.iter().cloned());
        for s in office.sessions.values() {
            if !s.cwd.is_empty() {
                dirs.push(s.cwd.clone());
                in_use.push(s.cwd.clone());
            }
        }
    }
    {
        let tasks = app_state.tasks.lock().unwrap_or_else(|e| e.into_inner());
        for t in tasks.list() {
            if !t.dir.is_empty() {
                dirs.push(t.dir.clone());
                in_use.push(t.dir.clone());
            }
        }
    }
    (dirs, in_use)
}

fn redact_ai_mutate_body(mut body: Value) -> Value {
    if let Some(obj) = body.as_object_mut() {
        for key in ["label", "name"] {
            if let Some(val) = obj.get_mut(key)
                && let Some(s) = val.as_str()
            {
                *val = Value::String(station_core::redact::redact(s));
            }
        }
    }
    body
}

fn is_valid_tunnel_host_input(host: &str) -> bool {
    if host.is_empty() || host.len() > 253 || !host.contains('.') {
        return false;
    }
    host.split('.').all(|label| {
        !label.is_empty()
            && label.len() <= 63
            && !label.starts_with('-')
            && !label.ends_with('-')
            && label
                .bytes()
                .all(|b| b.is_ascii_alphanumeric() || b == b'-')
    })
}

fn current_process_private_mb() -> f32 {
    0.0
}

fn apply_remote_snapshot_to_ui(snap: &RemoteSnapshot, ui: &mut UiState) {
    if !snap.harnesses.is_empty() {
        ui.harnesses = snap.harnesses.clone();
    }
    if snap.has_snapshot {
        ui.sessions = snap.sessions.clone();
        let mut perms = snap.pending_perms.clone();
        for ci in &snap.ci_alerts {
            if !perms.iter().any(|p| p.id == ci.id) {
                perms.push(ci.clone());
            }
        }
        ui.pending_perms = perms;
        ui.feed = snap.feed.clone();
        if let Some(sel) = ui.selected_session.as_deref()
            && !ui.sessions.iter().any(|s| s.id == sel)
        {
            ui.selected_session = None;
        }
    }
    if snap.has_pins {
        ui.session_pins = snap.pins.clone();
    }
    if snap.has_todos {
        ui.session_todos = snap.todos.clone();
    }
    if !snap.accounts.is_empty() {
        ui.accounts = snap.accounts.clone();
    }
    if let Some(pin) = &snap.pairing_pin {
        ui.pairing_pin = pin.clone();
    }
    if let Some(sys) = &snap.sys {
        ui.sys = sys.clone();
    }
    if let Some(mac) = &snap.mac_drawer {
        merge_mac_drawer_telemetry(&mut ui.mac_drawer, mac.clone());
    }
    if let Some(burn) = &snap.burn {
        ui.burn = burn.clone();
    }
    if snap.has_chat {
        ui.chat = snap.chat.clone();
        ui.chat_pinned_id = snap.chat_pinned_id.clone();
        ui.chat_typing = snap.chat_typing.clone();
    }
    if snap.has_vault {
        ui.vault_results = snap.vault_results.clone();
    }
}

async fn fetch_remote_live(
    client: &reqwest::Client,
    base_url: &str,
    host_header: &str,
    snap_lock: &Arc<Mutex<RemoteSnapshot>>,
) {
    if let Ok(resp) = client
        .get(format!("{base_url}/api/live"))
        .header("host", host_header)
        .timeout(Duration::from_secs(2))
        .send()
        .await
        && resp.status().is_success()
        && let Ok(val) = resp.json::<Value>().await
    {
        apply_snapshot_to_remote(snap_lock, &val);
    }
}

async fn fetch_remote_harnesses(
    client: &reqwest::Client,
    base_url: &str,
    host_header: &str,
    snap_lock: &Arc<Mutex<RemoteSnapshot>>,
) {
    if let Ok(resp) = client
        .get(format!("{base_url}/api/harnesses"))
        .header("host", host_header)
        .timeout(Duration::from_secs(2))
        .send()
        .await
        && resp.status().is_success()
        && let Ok(val) = resp.json::<Value>().await
    {
        let harnesses = parse_harnesses_from_json(&val);
        let mut snap = snap_lock.lock().unwrap_or_else(|e| e.into_inner());
        snap.harnesses = harnesses;
    }
}

async fn fetch_remote_chat(
    client: &reqwest::Client,
    base_url: &str,
    host_header: &str,
    snap_lock: &Arc<Mutex<RemoteSnapshot>>,
) {
    if let Ok(resp) = client
        .get(format!("{base_url}/api/chat"))
        .header("host", host_header)
        .timeout(Duration::from_secs(2))
        .send()
        .await
        && resp.status().is_success()
        && let Ok(val) = resp.json::<Value>().await
    {
        let (chat, pinned, typing) = parse_chat_from_json(&val);
        let ci_alerts = parse_ci_alerts_from_chat_json(&val);
        let mut snap = snap_lock.lock().unwrap_or_else(|e| e.into_inner());
        snap.chat = chat;
        snap.chat_pinned_id = pinned;
        snap.chat_typing = typing;
        snap.ci_alerts = ci_alerts;
        snap.has_chat = true;
    }
}

async fn fetch_remote_telemetry(
    client: &reqwest::Client,
    base_url: &str,
    host_header: &str,
    snap_lock: &Arc<Mutex<RemoteSnapshot>>,
) {
    let get_json = |path: &str| {
        let url = format!("{base_url}{path}");
        async move {
            let resp = client
                .get(url)
                .header("host", host_header)
                .timeout(Duration::from_secs(2))
                .send()
                .await
                .ok()?;
            if !resp.status().is_success() {
                return None;
            }
            resp.json::<Value>().await.ok()
        }
    };

    let (sys_v, burn_today, burn_30d, chat_v, vault_v, accts_v) = tokio::join!(
        get_json("/api/sys"),
        get_json("/api/burn?range=today"),
        get_json("/api/burn?range=30d"),
        get_json("/api/chat"),
        get_json("/api/vault?limit=30"),
        get_json("/api/accounts"),
    );

    let mut snap = snap_lock.lock().unwrap_or_else(|e| e.into_inner());
    if let Some(v) = sys_v {
        snap.sys = Some(parse_sys_from_json(&v));
        snap.mac_drawer = Some(parse_mac_drawer_from_json(&v));
    }
    if let Some(today) = burn_today {
        snap.burn = Some(parse_burn_from_json(&today, burn_30d.as_ref()));
    }
    if let Some(v) = chat_v {
        let (chat, pinned, typing) = parse_chat_from_json(&v);
        snap.chat = chat;
        snap.chat_pinned_id = pinned;
        snap.chat_typing = typing;
        snap.ci_alerts = parse_ci_alerts_from_chat_json(&v);
        snap.has_chat = true;
    }
    if let Some(v) = vault_v {
        snap.vault_results = parse_vault_from_json(&v);
        snap.has_vault = true;
    }
    if let Some(v) = accts_v {
        snap.accounts = parse_accounts_from_view(&v);
    }
}

fn spawn_attached_sse_client(
    base_url: String,
    host_header: String,
    snap_lock: Arc<Mutex<RemoteSnapshot>>,
    toast_tx: mpsc::Sender<(String, String)>,
    async_tx: mpsc::Sender<BridgeAsyncResult>,
) -> (
    mpsc::Sender<RemoteActionCmd>,
    mpsc::Sender<RemoteQueryCmd>,
    watch::Sender<bool>,
) {
    let (action_tx, mut action_rx) = mpsc::channel::<RemoteActionCmd>(REMOTE_ACTION_QUEUE_CAP);
    let (query_tx, mut query_rx) = mpsc::channel::<RemoteQueryCmd>(REMOTE_ACTION_QUEUE_CAP);
    let (tx, mut rx) = watch::channel(false);
    let _ = std::thread::Builder::new()
        .name("station-sse-client".to_owned())
        .spawn(move || {
            let Ok(rt) = tokio::runtime::Builder::new_multi_thread()
                .worker_threads(1)
                .enable_all()
                .build()
            else {
                return;
            };
            rt.block_on(async move {
                let Ok(client) = reqwest::Client::builder().no_proxy().build() else {
                    return;
                };
                fetch_remote_harnesses(&client, &base_url, &host_header, &snap_lock).await;

                // Dedicated async query worker for /api/log, /api/undo, /api/prompt/rewrite, and Task 7 drawers.
                let client_q = client.clone();
                let base_q = base_url.clone();
                let host_q = host_header.clone();
                let async_q = async_tx.clone();
                let toast_q = toast_tx.clone();
                let snap_q = snap_lock.clone();
                let mut rx_q = rx.clone();
                tokio::spawn(async move {
                    loop {
                        tokio::select! {
                            maybe_q = query_rx.recv() => {
                                let Some(q) = maybe_q else { break; };
                                match q {
                                    RemoteQueryCmd::FetchLog { session_id } => {
                                        let url = format!("{base_q}/api/log");
                                        let items = match client_q
                                            .get(url)
                                            .query(&[("id", session_id.as_str())])
                                            .header("host", &host_q)
                                            .timeout(Duration::from_secs(3))
                                            .send()
                                            .await
                                        {
                                            Ok(r) if r.status().is_success() => r
                                                .json::<Value>()
                                                .await
                                                .ok()
                                                .map(|v| parse_log_items_from_json(&session_id, &v))
                                                .unwrap_or_default(),
                                            _ => Vec::new(),
                                        };
                                        let _ = async_q
                                            .try_send(BridgeAsyncResult::SessionLog { session_id, items });
                                    }
                                    RemoteQueryCmd::FetchUndo { session_id } => {
                                        let url = format!("{base_q}/api/undo");
                                        let view = match client_q
                                            .get(url)
                                            .query(&[("id", session_id.as_str())])
                                            .header("host", &host_q)
                                            .timeout(Duration::from_secs(3))
                                            .send()
                                            .await
                                        {
                                            Ok(r) if r.status().is_success() => r
                                                .json::<Value>()
                                                .await
                                                .ok()
                                                .map(|v| parse_undo_view_from_json(&session_id, &v))
                                                .unwrap_or_else(|| SessionUndoView {
                                                    session_id: session_id.clone(),
                                                    loading: false,
                                                    ..Default::default()
                                                }),
                                            _ => SessionUndoView {
                                                session_id: session_id.clone(),
                                                loading: false,
                                                ..Default::default()
                                            },
                                        };
                                        let _ = async_q.try_send(BridgeAsyncResult::SessionUndo(view));
                                    }
                                    RemoteQueryCmd::ApplyUndo { session_id, files } => {
                                        let url = format!("{base_q}/api/undo");
                                        let view = match client_q
                                            .post(url)
                                            .header("host", &host_q)
                                            .timeout(Duration::from_secs(6))
                                            .json(&json!({ "id": session_id, "files": files }))
                                            .send()
                                            .await
                                        {
                                            Ok(r) => r
                                                .json::<Value>()
                                                .await
                                                .ok()
                                                .map(|v| parse_undo_apply_from_json(&session_id, &v))
                                                .unwrap_or_else(|| SessionUndoView {
                                                    session_id: session_id.clone(),
                                                    loading: false,
                                                    error: Some("invalid undo response".to_owned()),
                                                    ..Default::default()
                                                }),
                                            Err(err) => SessionUndoView {
                                                session_id: session_id.clone(),
                                                loading: false,
                                                error: Some(err.to_string()),
                                                ..Default::default()
                                            },
                                        };
                                        let _ = async_q.try_send(BridgeAsyncResult::SessionUndo(view));
                                    }
                                    RemoteQueryCmd::RewritePrompt { text, lang } => {
                                        let url = format!("{base_q}/api/prompt/rewrite");
                                        let res = match client_q
                                            .post(url)
                                            .header("host", &host_q)
                                            .timeout(Duration::from_secs(20))
                                            .json(&json!({ "text": text, "lang": lang }))
                                            .send()
                                            .await
                                        {
                                            Ok(r) => match r.json::<Value>().await {
                                                Ok(v) if v.get("ok").and_then(Value::as_bool) == Some(true) => {
                                                    Ok(v.get("text")
                                                        .and_then(Value::as_str)
                                                        .unwrap_or("")
                                                        .to_owned())
                                                }
                                                Ok(v) => Err(v
                                                    .get("message")
                                                    .or_else(|| v.get("code"))
                                                    .and_then(Value::as_str)
                                                    .unwrap_or("rewrite failed")
                                                    .to_owned()),
                                                Err(e) => Err(e.to_string()),
                                            },
                                            Err(e) => Err(e.to_string()),
                                        };
                                        let _ = async_q.try_send(BridgeAsyncResult::RewriteResult(res));
                                    }
                                    RemoteQueryCmd::FetchWrapped => {
                                        let url = format!("{base_q}/api/wrapped");
                                        let res = match client_q
                                            .get(url)
                                            .header("host", &host_q)
                                            .timeout(Duration::from_secs(5))
                                            .send()
                                            .await
                                        {
                                            Ok(r) if r.status().is_success() => r
                                                .json::<Value>()
                                                .await
                                                .map(|v| parse_wrapped_from_json(&v))
                                                .map_err(|e| e.to_string()),
                                            Ok(r) => Err(format!("HTTP {}", r.status().as_u16())),
                                            Err(e) => Err(e.to_string()),
                                        };
                                        let _ = async_q.try_send(BridgeAsyncResult::WrappedSummary(res));
                                    }
                                    RemoteQueryCmd::FetchImage { id } => {
                                        let url = format!("{base_q}/api/image");
                                        let res = match client_q
                                            .get(url)
                                            .query(&[("id", id.as_str())])
                                            .header("host", &host_q)
                                            .timeout(Duration::from_secs(4))
                                            .send()
                                            .await
                                        {
                                            Ok(r) if r.status().is_success() => {
                                                match read_response_bytes_bounded(r, MAX_CHAT_IMAGE_BYTES).await {
                                                    Ok(bytes) => {
                                                        tokio::task::spawn_blocking(move || {
                                                            decode_bounded_chat_preview(id, &bytes)
                                                        })
                                                        .await
                                                        .unwrap_or_else(|e| Err(e.to_string()))
                                                    }
                                                    Err(e) => Err(e),
                                                }
                                            }
                                            Ok(r) => Err(format!("HTTP {}", r.status().as_u16())),
                                            Err(e) => Err(e.to_string()),
                                        };
                                        let _ = async_q.try_send(BridgeAsyncResult::ChatImage(res));
                                    }
                                    RemoteQueryCmd::FetchBurn(range) => {
                                        let url = format!("{base_q}/api/burn");
                                        let res = match client_q
                                            .get(url)
                                            .query(&[("range", range.api_str())])
                                            .header("host", &host_q)
                                            .timeout(Duration::from_secs(6))
                                            .send()
                                            .await
                                        {
                                            Ok(r) if r.status().is_success() => r
                                                .json::<Value>()
                                                .await
                                                .map(|v| parse_burn_drawer_from_json(range, &v))
                                                .map_err(|e| e.to_string()),
                                            Ok(r) => Err(format!("HTTP {}", r.status().as_u16())),
                                            Err(e) => Err(e.to_string()),
                                        };
                                        let _ = async_q.try_send(BridgeAsyncResult::BurnData(res));
                                    }
                                    RemoteQueryCmd::FetchVaultList { query, fresh, seq } => {
                                        let url = format!("{base_q}/api/vault");
                                        let mut params: Vec<(&str, &str)> = vec![("limit", "60")];
                                        if !query.is_empty() {
                                            params.push(("q", query.as_str()));
                                        }
                                        if fresh {
                                            params.push(("fresh", "1"));
                                        }
                                        let (items, total_count, total_bytes) = match client_q
                                            .get(url)
                                            .query(&params)
                                            .header("host", &host_q)
                                            .timeout(Duration::from_secs(10))
                                            .send()
                                            .await
                                        {
                                            Ok(r) if r.status().is_success() => r
                                                .json::<Value>()
                                                .await
                                                .ok()
                                                .map(|v| parse_vault_list_from_json(&v))
                                                .unwrap_or_default(),
                                            _ => (Vec::new(), 0, 0),
                                        };
                                        let _ = async_q.try_send(BridgeAsyncResult::VaultList {
                                            seq,
                                            items,
                                            total_count,
                                            total_bytes,
                                        });
                                    }
                                    RemoteQueryCmd::FetchVaultTranscript { sid } => {
                                        let url = format!("{base_q}/api/vault/transcript");
                                        let rec = match client_q
                                            .get(url)
                                            .query(&[("sid", sid.as_str())])
                                            .header("host", &host_q)
                                            .timeout(Duration::from_secs(8))
                                            .send()
                                            .await
                                        {
                                            Ok(r) if r.status().is_success() => r
                                                .json::<Value>()
                                                .await
                                                .ok()
                                                .and_then(|v| parse_vault_transcript_from_json(&v)),
                                            _ => None,
                                        };
                                        let _ = async_q.try_send(BridgeAsyncResult::VaultTranscript(rec));
                                    }
                                    RemoteQueryCmd::FetchVaultReport => {
                                        let url = format!("{base_q}/api/vault/report");
                                        let res = match client_q
                                            .get(url)
                                            .header("host", &host_q)
                                            .timeout(Duration::from_secs(10))
                                            .send()
                                            .await
                                        {
                                            Ok(r) if r.status().is_success() => r
                                                .json::<Value>()
                                                .await
                                                .map(|v| parse_vault_report_from_json(&v))
                                                .map_err(|e| e.to_string()),
                                            Ok(r) => Err(format!("HTTP {}", r.status().as_u16())),
                                            Err(e) => Err(e.to_string()),
                                        };
                                        let _ = async_q.try_send(BridgeAsyncResult::VaultReport(res));
                                    }
                                    RemoteQueryCmd::FetchStranded { fresh } => {
                                        let url = format!("{base_q}/api/stranded");
                                        let req = if fresh {
                                            client_q.get(url).query(&[("fresh", "1")])
                                        } else {
                                            client_q.get(url)
                                        };
                                        let (items, repos) = match req
                                            .header("host", &host_q)
                                            .timeout(Duration::from_secs(12))
                                            .send()
                                            .await
                                        {
                                            Ok(r) if r.status().is_success() => r
                                                .json::<Value>()
                                                .await
                                                .ok()
                                                .map(|v| parse_stranded_from_json(&v))
                                                .unwrap_or_default(),
                                            _ => (Vec::new(), 0),
                                        };
                                        let _ = async_q.try_send(BridgeAsyncResult::StrandedData { items, repos });
                                    }
                                    RemoteQueryCmd::StrandedMutate {
                                        path,
                                        id,
                                        toast_title,
                                        toast_ok,
                                        toast_err,
                                    } => {
                                        let url = format!("{base_q}{path}");
                                        let ok = match client_q
                                            .post(url)
                                            .header("host", &host_q)
                                            .timeout(Duration::from_secs(12))
                                            .json(&json!({ "id": id }))
                                            .send()
                                            .await
                                        {
                                            Ok(r) if r.status().is_success() => r
                                                .json::<Value>()
                                                .await
                                                .ok()
                                                .and_then(|v| v.get("ok").and_then(Value::as_bool))
                                                .unwrap_or(false),
                                            _ => false,
                                        };
                                        let _ = toast_q.try_send((
                                            toast_title,
                                            if ok { toast_ok } else { toast_err },
                                        ));
                                        if ok && path.ends_with("/remove") {
                                            let scan_url = format!("{base_q}/api/stranded");
                                            if let Ok(r) = client_q
                                                .get(scan_url)
                                                .query(&[("fresh", "1")])
                                                .header("host", &host_q)
                                                .timeout(Duration::from_secs(12))
                                                .send()
                                                .await
                                                && r.status().is_success()
                                                && let Ok(v) = r.json::<Value>().await
                                            {
                                                let (items, repos) = parse_stranded_from_json(&v);
                                                let _ = async_q.try_send(BridgeAsyncResult::StrandedData { items, repos });
                                            }
                                        }
                                    }
                                    RemoteQueryCmd::SysMutate {
                                        path,
                                        body,
                                        toast_title,
                                        toast_ok,
                                        toast_err,
                                    } => {
                                        let url = format!("{base_q}{path}");
                                        let ok = match client_q
                                            .post(url)
                                            .header("host", &host_q)
                                            .timeout(Duration::from_secs(10))
                                            .json(&body)
                                            .send()
                                            .await
                                        {
                                            Ok(r) if r.status().is_success() => r
                                                .json::<Value>()
                                                .await
                                                .ok()
                                                .and_then(|v| v.get("ok").and_then(Value::as_bool))
                                                .unwrap_or(true),
                                            _ => false,
                                        };
                                        let _ = toast_q.try_send((
                                            toast_title,
                                            if ok { toast_ok } else { toast_err },
                                        ));
                                        fetch_remote_telemetry(&client_q, &base_q, &host_q, &snap_q).await;
                                    }
                                    RemoteQueryCmd::FetchAiAccounts => {
                                        let url = format!("{base_q}/api/accounts");
                                        if let Ok(r) = client_q
                                            .get(url)
                                            .header("host", &host_q)
                                            .timeout(Duration::from_secs(6))
                                            .send()
                                            .await
                                            && r.status().is_success()
                                            && let Ok(v) = r.json::<Value>().await
                                        {
                                            let _ = async_q.try_send(parse_ai_accounts_from_json(&v));
                                        }
                                    }
                                    RemoteQueryCmd::AiAccountMutate {
                                        provider,
                                        action,
                                        body,
                                        toast_title,
                                        toast_ok,
                                        toast_err,
                                    } => {
                                        let valid_action = !action.is_empty()
                                            && action
                                                .bytes()
                                                .all(|b| b.is_ascii_alphanumeric() || b == b'-' || b == b'_');
                                        let url = if provider == AiProviderTab::Claude && action == "prepare" {
                                            format!("{base_q}/api/accounts")
                                        } else if valid_action {
                                            format!("{base_q}{}/{action}", provider.api_prefix())
                                        } else {
                                            String::new()
                                        };
                                        let (ok, code) = if url.is_empty() {
                                            (false, None)
                                        } else {
                                            match client_q
                                                .post(url)
                                                .header("host", &host_q)
                                                .timeout(Duration::from_secs(10))
                                                .json(&body)
                                                .send()
                                                .await
                                            {
                                                Ok(r) if r.status().is_success() => match r.json::<Value>().await {
                                                    Ok(v) => {
                                                        let ok = v.get("ok").and_then(Value::as_bool).unwrap_or(true);
                                                        let code = v
                                                            .get("code")
                                                            .and_then(Value::as_str)
                                                            .map(str::to_owned);
                                                        (ok, code)
                                                    }
                                                    Err(_) => (true, None),
                                                },
                                                _ => (false, None),
                                            }
                                        };
                                        let _ = async_q.try_send(BridgeAsyncResult::AiAccountNotice {
                                            title: toast_title,
                                            ok,
                                            code,
                                            ok_msg: toast_ok,
                                            err_msg: toast_err,
                                        });
                                        if let Ok(r) = client_q
                                            .get(format!("{base_q}/api/accounts"))
                                            .header("host", &host_q)
                                            .timeout(Duration::from_secs(6))
                                            .send()
                                            .await
                                            && r.status().is_success()
                                            && let Ok(v) = r.json::<Value>().await
                                        {
                                            let _ = async_q.try_send(parse_ai_accounts_from_json(&v));
                                        }
                                    }
                                    RemoteQueryCmd::FetchPhoneStatus => {
                                        let tun = match client_q
                                            .get(format!("{base_q}/api/tunnel"))
                                            .header("host", &host_q)
                                            .timeout(Duration::from_secs(5))
                                            .send()
                                            .await
                                        {
                                            Ok(r) if r.status().is_success() => r.json::<Value>().await.ok(),
                                            _ => None,
                                        };
                                        let push = match client_q
                                            .get(format!("{base_q}/api/push"))
                                            .header("host", &host_q)
                                            .timeout(Duration::from_secs(4))
                                            .send()
                                            .await
                                        {
                                            Ok(r) if r.status().is_success() => r.json::<Value>().await.ok(),
                                            _ => None,
                                        };
                                        if let Some(t) = tun {
                                            if let Some(pin) = t
                                                .get("pin")
                                                .and_then(Value::as_str)
                                                .filter(|s| !s.is_empty())
                                                && let Ok(mut snap) = snap_q.lock()
                                            {
                                                snap.pairing_pin = Some(pin.to_owned());
                                            }
                                            let _ = async_q.try_send(parse_phone_status_from_json(&t, push.as_ref()));
                                        }
                                    }
                                    RemoteQueryCmd::PhoneMutate {
                                        path,
                                        body,
                                        toast_title,
                                        toast_ok,
                                        toast_empty,
                                        toast_err,
                                    } => {
                                        let url = format!("{base_q}{path}");
                                        let resp_val = match client_q
                                            .post(url)
                                            .header("host", &host_q)
                                            .timeout(Duration::from_secs(12))
                                            .json(&body)
                                            .send()
                                            .await
                                        {
                                            Ok(r) if r.status().is_success() => {
                                                Some(r.json::<Value>().await.unwrap_or_else(|_| json!({ "ok": true })))
                                            }
                                            _ => None,
                                        };
                                        let ok = resp_val
                                            .as_ref()
                                            .and_then(|v| v.get("ok").and_then(Value::as_bool))
                                            .unwrap_or(resp_val.is_some());
                                        if let Some(pin) = resp_val
                                            .as_ref()
                                            .and_then(|v| v.get("pin"))
                                            .and_then(Value::as_str)
                                            .filter(|s| !s.is_empty())
                                            && let Ok(mut snap) = snap_q.lock()
                                        {
                                            snap.pairing_pin = Some(pin.to_owned());
                                        }
                                        let sent_zero = resp_val
                                            .as_ref()
                                            .and_then(|v| v.get("sent"))
                                            .and_then(Value::as_u64)
                                            == Some(0);
                                        let toast_body = if !ok {
                                            toast_err
                                        } else if sent_zero && let Some(empty_msg) = toast_empty {
                                            empty_msg
                                        } else {
                                            toast_ok
                                        };
                                        let _ = toast_q.try_send((toast_title, toast_body));
                                        let tun = match client_q
                                            .get(format!("{base_q}/api/tunnel"))
                                            .header("host", &host_q)
                                            .timeout(Duration::from_secs(5))
                                            .send()
                                            .await
                                        {
                                            Ok(r) if r.status().is_success() => r.json::<Value>().await.ok(),
                                            _ => None,
                                        };
                                        let push = match client_q
                                            .get(format!("{base_q}/api/push"))
                                            .header("host", &host_q)
                                            .timeout(Duration::from_secs(4))
                                            .send()
                                            .await
                                        {
                                            Ok(r) if r.status().is_success() => r.json::<Value>().await.ok(),
                                            _ => None,
                                        };
                                        if let Some(t) = tun {
                                            if let Some(pin) = t
                                                .get("pin")
                                                .and_then(Value::as_str)
                                                .filter(|s| !s.is_empty())
                                                && let Ok(mut snap) = snap_q.lock()
                                            {
                                                snap.pairing_pin = Some(pin.to_owned());
                                            }
                                            let _ = async_q.try_send(parse_phone_status_from_json(&t, push.as_ref()));
                                        }
                                    }
                                }
                            }
                            changed = rx_q.changed() => {
                                if changed.is_err() || *rx_q.borrow() {
                                    break;
                                }
                            }
                        }
                    }
                });

                // Single pooled outbound POST worker reusing the existing client & runtime.
                let client_a = client.clone();
                let base_a = base_url.clone();
                let host_a = host_header.clone();
                let snap_a = snap_lock.clone();
                let toast_a = toast_tx.clone();
                let mut rx_a = rx.clone();
                tokio::spawn(async move {
                    loop {
                        tokio::select! {
                            maybe_cmd = action_rx.recv() => {
                                let Some(cmd) = maybe_cmd else {
                                    break;
                                };
                                let ok = if cmd.path.is_empty() {
                                    true
                                } else {
                                    let resp = client_a
                                        .post(format!("{base_a}{}", cmd.path))
                                        .header("host", &host_a)
                                        .timeout(Duration::from_secs(3))
                                        .json(&cmd.body)
                                        .send()
                                        .await;
                                    match resp {
                                        Ok(r) if r.status().is_success() => {
                                            if let Some(label) = cmd.launch_toast_label.as_deref() {
                                                match r.json::<Value>().await {
                                                    Ok(val) => {
                                                        let (launch_ok, notice) =
                                                            interpret_remote_launch_response(label, &val);
                                                        let _ = toast_a.try_send(notice);
                                                        launch_ok
                                                    }
                                                    Err(err) => {
                                                        let _ = toast_a.try_send(launch_error_toast(
                                                            label,
                                                            &err.to_string(),
                                                        ));
                                                        false
                                                    }
                                                }
                                            } else {
                                                true
                                            }
                                        }
                                        Ok(r) => {
                                            if let Some(label) = cmd.launch_toast_label.as_deref() {
                                                let _ = toast_a.try_send(launch_error_toast(
                                                    label,
                                                    &format!("HTTP {}", r.status().as_u16()),
                                                ));
                                            }
                                            false
                                        }
                                        Err(err) => {
                                            if let Some(label) = cmd.launch_toast_label.as_deref() {
                                                let _ = toast_a.try_send(launch_error_toast(label, &err.to_string()));
                                            }
                                            false
                                        }
                                    }
                                };
                                if ok {
                                    match cmd.refresh {
                                        RefreshKind::Live => {
                                            fetch_remote_live(&client_a, &base_a, &host_a, &snap_a).await;
                                        }
                                        RefreshKind::Harnesses => {
                                            fetch_remote_harnesses(&client_a, &base_a, &host_a, &snap_a).await;
                                        }
                                        RefreshKind::Chat => {
                                            fetch_remote_chat(&client_a, &base_a, &host_a, &snap_a).await;
                                        }
                                        RefreshKind::All => {
                                            fetch_remote_live(&client_a, &base_a, &host_a, &snap_a).await;
                                            fetch_remote_telemetry(&client_a, &base_a, &host_a, &snap_a).await;
                                            fetch_remote_harnesses(&client_a, &base_a, &host_a, &snap_a).await;
                                        }
                                        RefreshKind::None => {}
                                    }
                                }
                            }
                            changed = rx_a.changed() => {
                                if changed.is_err() || *rx_a.borrow() {
                                    break;
                                }
                            }
                        }
                    }
                });

                // Periodic 1 Hz harness refresh + 3 Hz telemetry (sys/burn/chat/vault) alongside live SSE stream.
                let client_h = client.clone();
                let base_h = base_url.clone();
                let host_h = host_header.clone();
                let snap_h = snap_lock.clone();
                let mut rx_h = rx.clone();
                tokio::spawn(async move {
                    let mut tick = tokio::time::interval(HARNESS_POLL_INTERVAL);
                    tick.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);
                    let mut tele_tick = tokio::time::interval(TELEMETRY_POLL_INTERVAL);
                    tele_tick.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);
                    loop {
                        tokio::select! {
                            _ = tick.tick() => {
                                fetch_remote_harnesses(&client_h, &base_h, &host_h, &snap_h).await;
                            }
                            _ = tele_tick.tick() => {
                                fetch_remote_telemetry(&client_h, &base_h, &host_h, &snap_h).await;
                            }
                            changed = rx_h.changed() => {
                                if changed.is_err() || *rx_h.borrow() {
                                    break;
                                }
                            }
                        }
                    }
                });

                while !*rx.borrow() {
                    let stream_req = client
                        .get(format!("{base_url}/stream"))
                        .header("host", &host_header)
                        .header("accept", "text/event-stream")
                        .send();

                    let resp = tokio::select! {
                        r = stream_req => r,
                        changed = rx.changed() => {
                            if changed.is_err() || *rx.borrow() {
                                break;
                            }
                            continue;
                        }
                    };

                    if let Ok(mut resp) = resp
                        && resp.status().is_success()
                    {
                        let mut decoder = SseByteDecoder::new(MAX_SSE_LINE_BYTES);
                        loop {
                            let chunk = tokio::select! {
                                c = resp.chunk() => c,
                                changed = rx.changed() => {
                                    if changed.is_err() || *rx.borrow() {
                                        return;
                                    }
                                    continue;
                                }
                            };
                            match chunk {
                                Ok(Some(bytes)) => {
                                    let Ok(lines) = decoder.push_chunk(&bytes) else {
                                        // Line exceeded MAX_SSE_LINE_BYTES: drop and reconnect cleanly.
                                        break;
                                    };
                                    for line in lines {
                                        if let Some(data) = line.strip_prefix("data:") {
                                            let trimmed = data.trim();
                                            if let Ok(val) = serde_json::from_str::<Value>(trimmed) {
                                                match val.get("k").and_then(Value::as_str) {
                                                    Some("snap") => {
                                                        apply_snapshot_to_remote(&snap_lock, &val);
                                                    }
                                                    Some("chat") => {
                                                        fetch_remote_chat(
                                                            &client,
                                                            &base_url,
                                                            &host_header,
                                                            &snap_lock,
                                                        )
                                                        .await;
                                                    }
                                                    _ => {
                                                        fetch_remote_live(
                                                            &client,
                                                            &base_url,
                                                            &host_header,
                                                            &snap_lock,
                                                        )
                                                        .await;
                                                    }
                                                }
                                            }
                                        }
                                    }
                                }
                                Ok(None) | Err(_) => break,
                            }
                        }
                    }

                    tokio::select! {
                        _ = tokio::time::sleep(Duration::from_millis(750)) => {}
                        changed = rx.changed() => {
                            if changed.is_err() || *rx.borrow() {
                                break;
                            }
                        }
                    }
                }
            });
        });
    (action_tx, query_tx, tx)
}

#[derive(Resource)]
pub struct LiveServerBridge {
    backend: BridgeBackend,
    pub data_dir: PathBuf,
    pub assets_dir: PathBuf,
    port: u16,
    sync_timer: Timer,
    sys_sync_timer: Timer,
    sys_refresh_due: bool,
    banner_pushed: bool,
    seen_perm_ids: HashSet<String>,
    dismissed_ci_ids: HashSet<String>,
    last_window_focused: Option<bool>,
    toast_tx: mpsc::Sender<(String, String)>,
    toast_rx: mpsc::Receiver<(String, String)>,
    async_tx: mpsc::Sender<BridgeAsyncResult>,
    async_rx: mpsc::Receiver<BridgeAsyncResult>,
}

impl LiveServerBridge {
    pub fn connect_or_start(
        data_dir: PathBuf,
        assets_dir: PathBuf,
        host: String,
        port: u16,
        start_http: bool,
        prefer_attach: bool,
    ) -> Self {
        let (toast_tx, toast_rx) = mpsc::channel(TOAST_QUEUE_CAP);
        let (async_tx, async_rx) = mpsc::channel(TOAST_QUEUE_CAP);

        if prefer_attach && probe_local_health(port) {
            let base_url = format!("http://127.0.0.1:{port}");
            let host_header = format!("127.0.0.1:{port}");
            let snapshot = Arc::new(Mutex::new(RemoteSnapshot::default()));
            let (action_tx, query_tx, shutdown) = spawn_attached_sse_client(
                base_url,
                host_header,
                snapshot.clone(),
                toast_tx.clone(),
                async_tx.clone(),
            );
            return Self {
                backend: BridgeBackend::AttachedHttp {
                    snapshot,
                    action_tx,
                    query_tx,
                    _shutdown: shutdown,
                },
                data_dir,
                assets_dir,
                port,
                sync_timer: Timer::from_seconds(0.5, TimerMode::Repeating),
                sys_sync_timer: Timer::from_seconds(4.0, TimerMode::Repeating),
                sys_refresh_due: true,
                banner_pushed: false,
                seen_perm_ids: HashSet::new(),
                dismissed_ci_ids: HashSet::new(),
                last_window_focused: None,
                toast_tx,
                toast_rx,
                async_tx,
                async_rx,
            };
        }

        let bin_path =
            std::env::current_exe().unwrap_or_else(|_| PathBuf::from("agents-station.exe"));
        let harnesses = Arc::new(HarnessService::new_win32(
            data_dir.clone(),
            bin_path,
            port,
        ));
        let harness_api: Arc<dyn HarnessApi> = harnesses.clone();

        let app_state = match AppState::new(data_dir.clone(), Some(harness_api)) {
            Ok(st) => Arc::new(st),
            Err(err) => {
                let harness_poller = HarnessPoller::spawn(harnesses.clone(), HARNESS_POLL_INTERVAL);
                return Self {
                    backend: BridgeBackend::BindError {
                        message: format!("Failed to initialize station store: {err}"),
                        harnesses,
                        harness_poller,
                    },
                    data_dir,
                    assets_dir,
                    port,
                    sync_timer: Timer::from_seconds(1.0, TimerMode::Repeating),
                    sys_sync_timer: Timer::from_seconds(4.0, TimerMode::Repeating),
                    sys_refresh_due: true,
                    banner_pushed: false,
                    seen_perm_ids: HashSet::new(),
                    dismissed_ci_ids: HashSet::new(),
                    last_window_focused: None,
                    toast_tx,
                    toast_rx,
                    async_tx,
                    async_rx,
                };
            }
        };

        if !start_http {
            let harness_poller = HarnessPoller::spawn(harnesses.clone(), HARNESS_POLL_INTERVAL);
            return Self {
                backend: BridgeBackend::InProcess {
                    app_state,
                    harnesses,
                    harness_poller,
                    runtime_handle: None,
                    _shutdown: None,
                },
                data_dir,
                assets_dir,
                port,
                sync_timer: Timer::from_seconds(1.0, TimerMode::Repeating),
                sys_sync_timer: Timer::from_seconds(4.0, TimerMode::Repeating),
                sys_refresh_due: true,
                banner_pushed: false,
                seen_perm_ids: HashSet::new(),
                dismissed_ci_ids: HashSet::new(),
                last_window_focused: None,
                toast_tx,
                toast_rx,
                async_tx,
                async_rx,
            };
        }

        let state_bg = app_state.clone();
        let host_bg = host.clone();
        let (tx, rx) = watch::channel(false);
        let (bind_tx, bind_rx) =
            std::sync::mpsc::sync_channel::<Result<tokio::runtime::Handle, String>>(1);

        let spawned = std::thread::Builder::new()
            .name("station-http".to_owned())
            .spawn(move || {
                let rt = match tokio::runtime::Builder::new_multi_thread()
                    .worker_threads(2)
                    .enable_all()
                    .build()
                {
                    Ok(rt) => rt,
                    Err(e) => {
                        let _ = bind_tx.send(Err(e.to_string()));
                        return;
                    }
                };
                let rt_handle = rt.handle().clone();
                rt.block_on(async move {
                    match start_http_server(state_bg.clone(), &host_bg, port).await {
                        Ok(handle) => {
                            let _ = bind_tx.send(Ok(rt_handle));
                            let mut rx = rx;
                            while !*rx.borrow() {
                                if rx.changed().await.is_err() {
                                    break;
                                }
                            }
                            let _ = handle.shutdown.send(true);
                            let _ = state_bg.flush_all();
                        }
                        Err(err) => {
                            let _ = bind_tx.send(Err(err.to_string()));
                        }
                    }
                });
            });

        let bind_res = match spawned {
            Ok(_) => bind_rx
                .recv_timeout(Duration::from_secs(3))
                .unwrap_or_else(|_| Err("Timed out waiting for HTTP server bind".to_owned())),
            Err(e) => Err(e.to_string()),
        };

        match bind_res {
            Ok(rt_handle) => {
                let harness_poller =
                    HarnessPoller::spawn(harnesses.clone(), HARNESS_POLL_INTERVAL);
                Self {
                    backend: BridgeBackend::InProcess {
                        app_state,
                        harnesses,
                        harness_poller,
                        runtime_handle: Some(rt_handle),
                        _shutdown: Some(tx),
                    },
                    data_dir,
                    assets_dir,
                    port,
                    sync_timer: Timer::from_seconds(1.0, TimerMode::Repeating),
                    sys_sync_timer: Timer::from_seconds(4.0, TimerMode::Repeating),
                    sys_refresh_due: true,
                    banner_pushed: false,
                    seen_perm_ids: HashSet::new(),
                    dismissed_ci_ids: HashSet::new(),
                    last_window_focused: None,
                    toast_tx,
                    toast_rx,
                    async_tx,
                    async_rx,
                }
            }
            Err(err) => {
                // Re-check once in case another Agents Station daemon just finished binding :4310.
                if prefer_attach && probe_local_health(port) {
                    let base_url = format!("http://127.0.0.1:{port}");
                    let host_header = format!("127.0.0.1:{port}");
                    let snapshot = Arc::new(Mutex::new(RemoteSnapshot::default()));
                    let (action_tx, query_tx, shutdown) = spawn_attached_sse_client(
                        base_url,
                        host_header,
                        snapshot.clone(),
                        toast_tx.clone(),
                        async_tx.clone(),
                    );
                    Self {
                        backend: BridgeBackend::AttachedHttp {
                            snapshot,
                            action_tx,
                            query_tx,
                            _shutdown: shutdown,
                        },
                        data_dir,
                        assets_dir,
                        port,
                        sync_timer: Timer::from_seconds(0.5, TimerMode::Repeating),
                        sys_sync_timer: Timer::from_seconds(4.0, TimerMode::Repeating),
                        sys_refresh_due: true,
                        banner_pushed: false,
                        seen_perm_ids: HashSet::new(),
                        dismissed_ci_ids: HashSet::new(),
                        last_window_focused: None,
                        toast_tx,
                        toast_rx,
                        async_tx,
                        async_rx,
                    }
                } else {
                    let harness_poller =
                        HarnessPoller::spawn(harnesses.clone(), HARNESS_POLL_INTERVAL);
                    Self {
                        backend: BridgeBackend::BindError {
                            message: format!(
                                "Cannot bind http://{host}:{port} ({err}). Stop the conflicting process or change --port."
                            ),
                            harnesses,
                            harness_poller,
                        },
                        data_dir,
                        assets_dir,
                        port,
                        sync_timer: Timer::from_seconds(1.0, TimerMode::Repeating),
                        sys_sync_timer: Timer::from_seconds(4.0, TimerMode::Repeating),
                        sys_refresh_due: true,
                        banner_pushed: false,
                        seen_perm_ids: HashSet::new(),
                        dismissed_ci_ids: HashSet::new(),
                        last_window_focused: None,
                        toast_tx,
                        toast_rx,
                        async_tx,
                        async_rx,
                    }
                }
            }
        }
    }

    pub fn mode(&self) -> BridgeMode {
        match &self.backend {
            BridgeBackend::InProcess { .. } => BridgeMode::InProcess,
            BridgeBackend::AttachedHttp { .. } => BridgeMode::AttachedHttp,
            BridgeBackend::BindError { .. } => BridgeMode::BindError,
        }
    }

    pub fn bind_error_message(&self) -> Option<&str> {
        match &self.backend {
            BridgeBackend::BindError { message, .. } => Some(message.as_str()),
            _ => None,
        }
    }

    /// Immediately clears `presence_focused = false` on the active daemon (in-process or attached HTTP)
    /// so closing or minimizing the GUI never leaves hooks holding for 45s.
    pub fn notify_presence_unfocused(&self) {
        match &self.backend {
            BridgeBackend::InProcess { app_state, .. } => {
                if let Ok(mut office) = app_state.office.lock() {
                    office.presence_focused = false;
                }
            }
            BridgeBackend::AttachedHttp { .. } => {
                let _ = post_presence_unfocused_sync(self.port);
            }
            BridgeBackend::BindError { .. } => {}
        }
    }

    #[cfg(test)]
    pub fn remote_sessions(&self) -> Vec<SessionCard> {
        match &self.backend {
            BridgeBackend::AttachedHttp { snapshot, .. } => snapshot
                .lock()
                .unwrap_or_else(|e| e.into_inner())
                .sessions
                .clone(),
            _ => Vec::new(),
        }
    }

    #[cfg(test)]
    pub fn remote_perms(&self) -> Vec<PendingPermCard> {
        match &self.backend {
            BridgeBackend::AttachedHttp { snapshot, .. } => snapshot
                .lock()
                .unwrap_or_else(|e| e.into_inner())
                .pending_perms
                .clone(),
            _ => Vec::new(),
        }
    }

    #[cfg(test)]
    fn in_process_state(&self) -> Option<Arc<AppState>> {
        match &self.backend {
            BridgeBackend::InProcess { app_state, .. } => Some(app_state.clone()),
            _ => None,
        }
    }

    #[cfg(test)]
    fn enqueue_remote_action(&self, path: &'static str, body: Value, refresh: RefreshKind) -> bool {
        match &self.backend {
            BridgeBackend::AttachedHttp { action_tx, .. } => {
                action_tx
                    .try_send(RemoteActionCmd {
                        path,
                        body,
                        refresh,
                        launch_toast_label: None,
                    })
                    .is_ok()
            }
            _ => false,
        }
    }
}

pub struct LiveServerBridgePlugin {
    pub data_dir: PathBuf,
    pub assets_dir: PathBuf,
    pub host: String,
    pub port: u16,
    pub start_http: bool,
}

/// Converts a logical `egui::Rect` into a physical `Viewport` safely clamped within `target_phys`.
/// Returns `None` when the target or resulting stage rect is smaller than 4x4 physical pixels.
pub fn compute_camera_viewport(
    logical: egui::Rect,
    scale_factor: f32,
    target_phys: UVec2,
) -> Option<Viewport> {
    if target_phys.x < 4 || target_phys.y < 4 {
        return None;
    }
    if !scale_factor.is_finite()
        || scale_factor <= 0.0
        || !logical.min.x.is_finite()
        || !logical.min.y.is_finite()
        || !logical.max.x.is_finite()
        || !logical.max.y.is_finite()
    {
        return None;
    }
    let max_x = target_phys.x as f32;
    let max_y = target_phys.y as f32;
    let x0 = (logical.min.x * scale_factor).floor().clamp(0.0, max_x) as u32;
    let y0 = (logical.min.y * scale_factor).floor().clamp(0.0, max_y) as u32;
    let x1 = (logical.max.x * scale_factor).ceil().clamp(0.0, max_x) as u32;
    let y1 = (logical.max.y * scale_factor).ceil().clamp(0.0, max_y) as u32;
    let w = x1.saturating_sub(x0);
    let h = y1.saturating_sub(y0);
    if w < 4 || h < 4 {
        return None;
    }
    Some(Viewport {
        physical_position: UVec2::new(x0, y0),
        physical_size: UVec2::new(w, h),
        ..default()
    })
}

#[derive(Debug, Clone)]
struct PendingPhotoJob {
    size: PhotoSize,
    shots_dir: PathBuf,
    assets_dir: PathBuf,
    title: String,
    show_names: bool,
    show_mark: bool,
    tags: Vec<station_canvas2d::PhotoNameTag>,
    frames_until_shoot: u8,
}

#[derive(Resource, Default)]
struct PendingPhotoCapture(Option<PendingPhotoJob>);

struct PendingTimelapseJob {
    total_frames: u32,
    captured_frames: u32,
    frame_interval_sec: f32,
    elapsed_since_last: f32,
    frame_tx: Option<std::sync::mpsc::SyncSender<Image>>,
    cancel_flag: Arc<std::sync::atomic::AtomicBool>,
}

#[derive(Resource, Default)]
struct PendingTimelapseCapture(Option<PendingTimelapseJob>);

pub(crate) fn is_night_for_hour(hour: u32) -> bool {
    !(6..18).contains(&hour)
}

pub(crate) fn fp_prop_i18n_key(prop_id: &str, prop_place: &str) -> &'static str {
    match prop_id {
        "cat" => "fp.petCat",
        "whiteboard" => "places.chip.board",
        "printer" => "places.chip.printer",
        "tv" | "dev-servers" => "places.chip.server",
        "duty-board" => "places.chip.dining",
        _ => match prop_place {
            "pantry" | "coffee" => "places.chip.dining",
            "server" => "places.chip.server",
            "lounge" => "places.chip.lounge",
            "meeting" => "places.chip.meeting",
            "library" => "places.chip.library",
            "director" => "places.chip.director",
            "deploy" => "places.chip.deploy",
            "board" => "places.chip.board",
            "printer" => "places.chip.printer",
            "door" => "places.chip.lobby",
            "wc" => "places.chip.wc",
            "window" => "places.chip.window",
            _ => "places.chip.desks",
        },
    }
}

fn place_title_i18n_key(place_id: &str) -> &'static str {
    match place_id {
        "meeting" => "places.meeting.title",
        "coffee" | "pantry" => "places.coffee.title",
        "lounge" => "places.lounge.title",
        "deploy" => "places.deploy.title",
        "library" => "places.library.title",
        "director" => "places.director.title",
        "printer" => "places.printer.title",
        "door" => "places.door.title",
        "wc" => "places.wc.title",
        "window" => "places.window.title",
        _ => "places.desks.title",
    }
}

fn zone_specs_for_layout(spec: &LayoutSpec) -> Vec<(&'static str, IconKind, &'static str, Vec3)> {
    if spec.layout.is_pro() {
        vec![
            ("director", IconKind::Home, "places.chip.director", Vec3::new(-5.4, 2.7, -6.2)),
            ("meeting", IconKind::Users, "places.chip.meeting", Vec3::new(0.2, 2.7, -5.6)),
            ("library", IconKind::Book, "places.chip.libraryPro", Vec3::new(5.8, 2.85, -6.6)),
            ("coffee", IconKind::Coffee, "places.chip.dining", Vec3::new(11.3, 1.4, -3.0)),
            ("lounge", IconKind::Sparkle, "places.chip.lounge", Vec3::new(11.2, 1.3, 6.2)),
            (
                "deploy",
                IconKind::Rocket,
                "places.chip.deploy",
                Vec3::new(spec.rocket[0], 0.9, spec.rocket[1] + 0.7),
            ),
            ("server", IconKind::Server, "places.chip.serverPro", Vec3::new(-5.8, 2.75, -1.0)),
            (
                "board",
                IconKind::List,
                "places.chip.board",
                Vec3::new(-3.4, 3.15, spec.board[2]),
            ),
        ]
    } else {
        vec![
            ("library", IconKind::Book, "places.chip.library", Vec3::new(-5.2, 2.8, -5.3)),
            ("server", IconKind::Server, "places.chip.server", Vec3::new(5.5, 2.58, -5.08)),
            ("deploy", IconKind::Rocket, "places.chip.deploy", Vec3::new(7.25, 0.9, -2.9)),
            ("board", IconKind::List, "places.chip.board", Vec3::new(-7.86, 3.4, -2.6)),
            ("coffee", IconKind::Coffee, "places.chip.coffee", Vec3::new(-5.55, 1.02, 5.74)),
            ("meeting", IconKind::Users, "places.chip.meeting", Vec3::new(5.2, 0.9, 2.4)),
        ]
    }
}

fn zone_live_subtitle(place_id: &str, ui: &UiState, i18n: &I18nStore) -> Option<String> {
    match place_id {
        "coffee" | "lounge" => {
            let n = ui.resting_count();
            (n > 0).then(|| i18n.tr_count("places.live.resting", n))
        }
        "meeting" => {
            let n: usize = ui.sessions.iter().map(|s| s.subagents.len()).sum();
            (n > 0).then(|| i18n.tr_count("places.live.subagents", n))
        }
        "server" => {
            if ui.sys.mem_total_gb > 0.0 {
                let pct = ((ui.sys.mem_used_gb / ui.sys.mem_total_gb) * 100.0).round() as u32;
                Some(format!("RAM {pct}%"))
            } else {
                None
            }
        }
        "library" => {
            let n = ui
                .sessions
                .iter()
                .filter(|s| s.status == "work" && matches!(s.tool_kind.as_deref(), Some("read" | "grep" | "glob")))
                .count();
            (n > 0).then(|| i18n.tr_count("places.live.reading", n))
        }
        "board" => {
            let n = ui.sessions.iter().filter(|s| s.status != "rest").count();
            (n > 0).then(|| i18n.tr_count("places.live.tasks", n))
        }
        _ => None,
    }
}

fn tool_icon_kind(tool_kind: Option<&str>) -> IconKind {
    match tool_kind.unwrap_or("") {
        "edit" | "write" | "patch" => IconKind::Pencil,
        "read" | "grep" | "glob" => IconKind::File,
        "web" | "fetch" | "browse" => IconKind::Globe,
        "agent" | "subagent" => IconKind::Users,
        _ => IconKind::Terminal,
    }
}

fn build_place_card_info(place_id: &str, ui: &UiState, i18n: &I18nStore) -> PlaceCardInfo {
    match place_id {
        "meeting" => {
            let mut rows = Vec::new();
            for s in &ui.sessions {
                for sub in &s.subagents {
                    rows.push(PlaceCardRow {
                        title: sub.label.clone(),
                        subtitle: sub.doing.clone(),
                        session_id: Some(s.id.clone()),
                        accent_tone: Some("blue".to_owned()),
                    });
                }
            }
            PlaceCardInfo {
                place_id: place_id.to_owned(),
                icon: IconKind::Users,
                title: i18n.tr("places.meeting.title"),
                subtitle: i18n.tr("places.meeting.sub"),
                rows,
                empty_text: i18n.tr("places.meeting.empty"),
                footer_note: None,
                actions: vec![PlaceCardAction {
                    id: "focus_meeting".to_owned(),
                    label: i18n.tr("ui.cameraToolbar.meeting"),
                    primary: true,
                }],
            }
        }
        "coffee" => {
            let rows = ui
                .sessions
                .iter()
                .filter(|s| s.status == "rest")
                .map(|s| PlaceCardRow {
                    title: s.project.clone(),
                    subtitle: if s.last_prompt.is_empty() {
                        i18n.tr("places.coffee.noPrompt")
                    } else {
                        s.last_prompt.clone()
                    },
                    session_id: Some(s.id.clone()),
                    accent_tone: Some("amber".to_owned()),
                })
                .collect();
            PlaceCardInfo {
                place_id: place_id.to_owned(),
                icon: IconKind::Coffee,
                title: i18n.tr("places.coffee.title"),
                subtitle: i18n.tr("places.coffee.sub"),
                rows,
                empty_text: i18n.tr("places.coffee.empty"),
                footer_note: Some(i18n.tr("places.coffee.hint")),
                actions: vec![PlaceCardAction {
                    id: "brew".to_owned(),
                    label: i18n.tr("places.coffee.brew"),
                    primary: true,
                }],
            }
        }
        "lounge" => {
            let rows = ui
                .sessions
                .iter()
                .filter(|s| s.status == "rest")
                .map(|s| PlaceCardRow {
                    title: s.project.clone(),
                    subtitle: s.model.clone(),
                    session_id: Some(s.id.clone()),
                    accent_tone: Some("green".to_owned()),
                })
                .collect();
            PlaceCardInfo {
                place_id: place_id.to_owned(),
                icon: IconKind::Sparkle,
                title: i18n.tr("places.lounge.title"),
                subtitle: i18n.tr("places.lounge.sub"),
                rows,
                empty_text: i18n.tr("places.lounge.empty"),
                footer_note: None,
                actions: vec![
                    PlaceCardAction {
                        id: "cat".to_owned(),
                        label: i18n.tr("places.lounge.cat"),
                        primary: false,
                    },
                    PlaceCardAction {
                        id: "rocket".to_owned(),
                        label: i18n.tr("places.lounge.rocket"),
                        primary: true,
                    },
                ],
            }
        }
        "deploy" => PlaceCardInfo {
            place_id: place_id.to_owned(),
            icon: IconKind::Rocket,
            title: i18n.tr("places.deploy.title"),
            subtitle: i18n.tr("places.deploy.sub"),
            rows: ui
                .sessions
                .iter()
                .map(|s| PlaceCardRow {
                    title: s.project.clone(),
                    subtitle: s.tool.clone().unwrap_or_else(|| s.model.clone()),
                    session_id: Some(s.id.clone()),
                    accent_tone: Some(if s.status == "work" { "green" } else { "amber" }.to_owned()),
                })
                .collect(),
            empty_text: i18n.tr("places.deploy.empty"),
            footer_note: None,
            actions: vec![PlaceCardAction {
                id: "test_launch".to_owned(),
                label: i18n.tr("places.deploy.test"),
                primary: true,
            }],
        },
        "library" => PlaceCardInfo {
            place_id: place_id.to_owned(),
            icon: IconKind::Book,
            title: i18n.tr("places.library.title"),
            subtitle: i18n.tr("places.library.sub"),
            rows: ui
                .sessions
                .iter()
                .filter(|s| matches!(s.tool_kind.as_deref(), Some("read" | "grep" | "glob")))
                .map(|s| PlaceCardRow {
                    title: s.project.clone(),
                    subtitle: s.tool.clone().unwrap_or_default(),
                    session_id: Some(s.id.clone()),
                    accent_tone: Some("blue".to_owned()),
                })
                .collect(),
            empty_text: i18n.tr("places.library.empty"),
            footer_note: Some(i18n.tr_count("places.library.today", ui.kpi_reads as usize)),
            actions: Vec::new(),
        },
        "director" => PlaceCardInfo {
            place_id: place_id.to_owned(),
            icon: IconKind::Home,
            title: i18n.tr("places.director.title"),
            subtitle: i18n.tr("places.director.sub"),
            rows: ui
                .sessions
                .iter()
                .filter(|s| !s.last_prompt.is_empty())
                .map(|s| PlaceCardRow {
                    title: s.project.clone(),
                    subtitle: s.last_prompt.clone(),
                    session_id: Some(s.id.clone()),
                    accent_tone: Some("green".to_owned()),
                })
                .collect(),
            empty_text: i18n.tr("places.director.empty"),
            footer_note: None,
            actions: Vec::new(),
        },
        _ => PlaceCardInfo {
            place_id: place_id.to_owned(),
            icon: IconKind::Monitor,
            title: i18n.tr("places.desks.title"),
            subtitle: i18n.tr("places.desks.sub"),
            rows: ui
                .sessions
                .iter()
                .map(|s| PlaceCardRow {
                    title: i18n.tr_vars(
                        "places.desks.row",
                        &[("n", &(s.desk + 1).to_string()), ("name", &s.project)],
                    ),
                    subtitle: s.tool.clone().unwrap_or_else(|| s.model.clone()),
                    session_id: Some(s.id.clone()),
                    accent_tone: Some(
                        match s.status.as_str() {
                            "work" => "green",
                            "wait" => "amber",
                            _ => "blue",
                        }
                        .to_owned(),
                    ),
                })
                .collect(),
            empty_text: i18n.tr("places.desks.empty"),
            footer_note: Some(i18n.tr_count("places.desks.edits", ui.kpi_edits as usize)),
            actions: vec![PlaceCardAction {
                id: "focus_desks".to_owned(),
                label: i18n.tr("ui.cameraToolbar.desks"),
                primary: true,
            }],
        },
    }
}

/// Projects 3D worker tags, speech bubbles, permission alerts, subagents, and zone chips through
/// `Camera::world_to_viewport` into `UiState::projected_labels`, culling behind-camera points.
#[allow(clippy::too_many_arguments)]
pub fn project_stage_labels_to_ui(
    stage_viewport: Res<StageViewportRect>,
    i18n: Res<I18nStore>,
    mut ui: ResMut<UiState>,
    rig: Option<Res<station_3d::camera::CameraRig>>,
    cams: Query<(&Camera, &GlobalTransform), With<station_3d::camera::MainOfficeCamera>>,
    live_workers: Query<(&station_3d::people::LiveSessionWorker, &GlobalTransform)>,
    props_q: Query<(&station_3d::interact::InteractiveProp, &GlobalTransform)>,
) {
    if let Some(rig) = rig.as_deref() {
        ui.camera_zoom_pct = rig.zoom_pct();
        ui.first_person = rig.first_person;
        ui.camera_preset = rig.active_preset;

        if rig.first_person {
            let mut nearest: Option<(f32, String)> = None;
            for (prop, gtf) in &props_q {
                let d = rig.fp_pos.distance(gtf.translation());
                if d < 3.2 && nearest.as_ref().is_none_or(|(best, _)| d < *best) {
                    let key = fp_prop_i18n_key(prop.id, prop.place);
                    nearest = Some((d, i18n.tr(key)));
                }
            }
            for (w, gtf) in &live_workers {
                let d = rig.fp_pos.distance(gtf.translation());
                if d < 3.0 && nearest.as_ref().is_none_or(|(best, _)| d < *best) {
                    let key = if w.status == "rest" { "fp.wake" } else { "fp.watch" };
                    nearest = Some((d, i18n.tr_vars(key, &[("name", &w.name)])));
                }
            }
            ui.fp_near_label = nearest.map(|(_, label)| label);
        } else {
            ui.fp_near_label = None;
            ui.fp_billiards = None;
        }
    }

    let Some(stage_rect) = stage_viewport.0 else {
        ui.projected_labels.clear();
        return;
    };
    let Ok((cam, cam_tf)) = cams.single() else {
        ui.projected_labels.clear();
        return;
    };

    let spec = LayoutSpec::for_layout(ui.layout);
    let mut live_positions: HashMap<&str, Vec3> = HashMap::new();
    for (w, gtf) in &live_workers {
        live_positions.insert(w.session_id.as_str(), gtf.translation());
    }

    let mut labels = Vec::with_capacity(ui.sessions.len() * 2 + 12);

    // 1. Workers, speech bubbles, permission badges, and meeting-room subagents
    for s in &ui.sessions {
        let base_pos = live_positions.get(s.id.as_str()).copied().or_else(|| {
            spec.desks.get(s.desk).map(|desk| {
                let rot = Quat::from_rotation_y(desk.ry);
                Vec3::new(desk.x, 0.0, desk.z) + rot * Vec3::new(0.0, 0.0, -0.62)
            })
        });
        let Some(anchor) = base_pos else {
            continue;
        };

        let head_world = anchor + Vec3::Y * 1.95;
        if let Some((screen_pos, depth)) = project_world_to_stage(head_world, cam, cam_tf, stage_rect)
        {
            let tag_id = format!("worker:{}", s.id);
            let is_hot = ui.hovered_label_id.as_deref() == Some(tag_id.as_str())
                || ui.selected_session.as_deref() == Some(s.id.as_str());
            let perm = ui.pending_perms.iter().find(|p| p.session_id == s.id);

            let tag_text = s.project.clone();
            let tag_sub = Some(s.agent.clone());
            let tag_size =
                estimate_label_size(ProjectedLabelKind::WorkerTag, &tag_text, tag_sub.as_deref());

            labels.push(ProjectedLabel {
                id: tag_id,
                kind: ProjectedLabelKind::WorkerTag,
                priority: if is_hot {
                    LabelPriority::Hovered
                } else {
                    LabelPriority::WorkingSession
                },
                screen_pos,
                depth,
                behind_camera: false,
                size: tag_size,
                text: tag_text,
                subtitle: tag_sub,
                accent: accent_for_status(&s.status),
                status: Some(s.status.clone()),
                session_id: Some(s.id.clone()),
                place_id: None,
                icon: None,
            });

            if let Some(p) = perm {
                let p_text = format!("{} · {}", i18n.tr("station.approval"), p.tool);
                let p_size =
                    estimate_label_size(ProjectedLabelKind::PermissionBadge, &p_text, None);
                let p_accent = if p.kind == "denied" { RED } else { AMBER };
                let min_badge_y = stage_rect.top() + 22.0;
                let max_badge_y = (stage_rect.bottom() - 82.0).max(min_badge_y);
                let badge_y = (screen_pos.y - 32.0).clamp(min_badge_y, max_badge_y);
                labels.push(ProjectedLabel {
                    id: format!("perm:{}", p.id),
                    kind: ProjectedLabelKind::PermissionBadge,
                    priority: LabelPriority::Permission,
                    screen_pos: egui::pos2(screen_pos.x, badge_y),
                    depth: (depth - 0.1).max(0.0),
                    behind_camera: false,
                    size: p_size,
                    text: p_text,
                    subtitle: None,
                    accent: p_accent,
                    status: Some("wait".to_owned()),
                    session_id: Some(s.id.clone()),
                    place_id: None,
                    icon: Some(IconKind::Alert),
                });
            } else if s.status == "work"
                && let Some(tool) = s.tool.as_deref().filter(|t| !t.is_empty())
            {
                let short: String = tool.chars().take(30).collect();
                let b_size = estimate_label_size(ProjectedLabelKind::SpeechBubble, &short, None);
                labels.push(ProjectedLabel {
                    id: format!("bubble:{}", s.id),
                    kind: ProjectedLabelKind::SpeechBubble,
                    priority: LabelPriority::WorkingSession,
                    screen_pos: egui::pos2(screen_pos.x, screen_pos.y - 34.0),
                    depth,
                    behind_camera: false,
                    size: b_size,
                    text: short,
                    subtitle: None,
                    accent: GREEN,
                    status: Some("work".to_owned()),
                    session_id: Some(s.id.clone()),
                    place_id: None,
                    icon: Some(tool_icon_kind(s.tool_kind.as_deref())),
                });
            }
        }

        // Subagents seated around the meeting room table
        for (sub_idx, sub) in s.subagents.iter().enumerate() {
            let sub_world = Vec3::new(
                spec.meet[0] + (sub_idx as f32 - 0.5) * 0.9,
                1.65,
                spec.meet[1] + 0.4,
            );
            if let Some((screen_pos, depth)) =
                project_world_to_stage(sub_world, cam, cam_tf, stage_rect)
            {
                let sz = estimate_label_size(ProjectedLabelKind::SubagentTag, &sub.label, None);
                labels.push(ProjectedLabel {
                    id: format!("sub:{}", sub.id),
                    kind: ProjectedLabelKind::SubagentTag,
                    priority: LabelPriority::Subagent,
                    screen_pos,
                    depth,
                    behind_camera: false,
                    size: sz,
                    text: sub.label.clone(),
                    subtitle: None,
                    accent: BLUE,
                    status: Some("sub".to_owned()),
                    session_id: Some(s.id.clone()),
                    place_id: None,
                    icon: None,
                });
            }
        }
    }

    // 2. Office zone chips
    for (place_id, icon, i18n_key, world_pos) in zone_specs_for_layout(&spec) {
        let Some((screen_pos, depth)) = project_world_to_stage(world_pos, cam, cam_tf, stage_rect)
        else {
            continue;
        };
        let label_id = format!("zone:{place_id}");
        let is_open = ui
            .active_place_card
            .as_ref()
            .is_some_and(|c| c.place_id == place_id);
        let is_hot = is_open || ui.hovered_label_id.as_deref() == Some(label_id.as_str());
        let text = i18n.tr(i18n_key);
        let live = zone_live_subtitle(place_id, &ui, &i18n);
        let sz = estimate_label_size(ProjectedLabelKind::ZoneChip, &text, live.as_deref());
        labels.push(ProjectedLabel {
            id: label_id,
            kind: ProjectedLabelKind::ZoneChip,
            priority: if is_hot {
                LabelPriority::Hovered
            } else {
                LabelPriority::ZoneChip
            },
            screen_pos,
            depth,
            behind_camera: false,
            size: sz,
            text,
            subtitle: live,
            accent: ACCENT,
            status: None,
            session_id: None,
            place_id: Some(place_id.to_owned()),
            icon: Some(icon),
        });
    }

    ui.projected_labels = labels;
}

/// Synchronizes the main 3D office camera viewport to the central stage rect reported by `station-ui`.
pub fn sync_camera_viewport_to_stage(
    stage_rect: Res<StageViewportRect>,
    windows: Query<&Window>,
    mut cams: Query<&mut Camera, With<station_3d::camera::MainOfficeCamera>>,
) {
    let win = windows.iter().next();
    for mut cam in &mut cams {
        let (scale, target_phys) = match &cam.target {
            RenderTarget::Image(_) => match cam.physical_target_size() {
                Some(sz) => (1.0_f32, sz),
                None => {
                    cam.is_active = false;
                    continue;
                }
            },
            _ => match win {
                Some(w) => (
                    w.scale_factor(),
                    UVec2::new(w.physical_width(), w.physical_height()),
                ),
                None => match cam.physical_target_size() {
                    Some(sz) => (1.0_f32, sz),
                    None => {
                        cam.is_active = false;
                        continue;
                    }
                },
            },
        };

        if target_phys.x < 4 || target_phys.y < 4 {
            cam.is_active = false;
            continue;
        }

        match stage_rect.0 {
            Some(rect) => match compute_camera_viewport(rect, scale, target_phys) {
                Some(vp) => {
                    cam.viewport = Some(vp);
                    cam.is_active = true;
                }
                None => {
                    cam.is_active = false;
                }
            },
            None => {
                cam.viewport = None;
                cam.is_active = true;
            }
        }
    }
}

impl Plugin for LiveServerBridgePlugin {
    fn build(&self, app: &mut App) {
        let bridge = LiveServerBridge::connect_or_start(
            self.data_dir.clone(),
            self.assets_dir.clone(),
            self.host.clone(),
            self.port,
            self.start_http,
            true,
        );
        app.insert_resource(bridge)
            .init_resource::<PendingPhotoCapture>()
            .init_resource::<PendingTimelapseCapture>()
            .add_systems(
                Update,
                (
                    sync_live_state_to_ui,
                    drive_pending_photo_capture,
                    drive_pending_timelapse_capture,
                    process_ui_actions,
                    sync_live_sessions_to_3d,
                    handle_picking_events,
                    step_replay_system,
                )
                    .chain()
                    .before(station_3d::LightingSyncSet),
            );
    }
}

fn parse_ui_perm_decision(decision: &str) -> Option<PermDecision> {
    match decision {
        "allow" | "always" | "approve" => Some(PermDecision::Allow),
        "deny" | "revise" => Some(PermDecision::Deny {
            reason: String::new(),
        }),
        "retry" => Some(PermDecision::Retry),
        "keep" => Some(PermDecision::Keep),
        _ => None,
    }
}

fn sync_office_chat_to_ui(office: &station_core::state::sessions::OfficeState, ui: &mut UiState) {
    ui.chat_pinned_id = office.chat.pinned.clone();
    ui.chat_typing = office.chat.typing.clone();
    ui.chat = office
        .chat
        .msgs
        .iter()
        .rev()
        .take(80)
        .collect::<Vec<_>>()
        .into_iter()
        .rev()
        .filter(|m| {
            !m.text.is_empty()
                || m.recalled
                || m.sticker.is_some()
                || m.photo.is_some()
                || m.poll.is_some()
                || m.remind.is_some()
        })
        .map(|m| ChatMessage {
            id: m.id.clone(),
            from: m.from.clone().unwrap_or_else(|| "office".to_owned()),
            sender: m
                .name
                .clone()
                .or_else(|| m.from.clone())
                .unwrap_or_else(|| "Office".to_owned()),
            role: m.kind.clone(),
            text: m.text.clone(),
            ts: format_hhmm_ms(m.at),
            at_ms: m.at,
            tag: m.tag.clone(),
            recalled: m.recalled,
            sticker: m.sticker,
            photo: m.photo.clone(),
            reply: m.reply.as_ref().map(|r| ChatReplyRefUi {
                id: r.id.clone(),
                name: r.name.clone(),
                text: r.text.clone(),
            }),
            reacts: m.reacts.clone(),
            poll: m.poll.as_ref().map(|p| ChatPollUi {
                q: p.q.clone(),
                opts: p
                    .opts
                    .iter()
                    .map(|o| ChatPollOptUi {
                        t: o.t.clone(),
                        by: o.by.clone(),
                    })
                    .collect(),
            }),
            remind: m.remind.as_ref().map(|r| ChatRemindUi {
                title: r.title.clone(),
                at_ms: r.at,
                repeat: r.repeat.clone(),
                yes: r.yes.clone(),
                no: r.no.clone(),
                done: r.done,
            }),
        })
        .collect();
}

fn sync_office_core_to_ui(
    app_state: &AppState,
    dismissed_ci_ids: &HashSet<String>,
    ui: &mut UiState,
) {
    if let Ok(office) = app_state.office.lock() {
        let mut sessions: Vec<SessionCard> = office
            .sessions
            .values()
            .map(|s| {
                let tok = s.input_tokens.saturating_add(s.output_tokens);
                let ctx_pct = if s.ctx_max > 0 {
                    (s.ctx_tokens as f32 / s.ctx_max as f32).clamp(0.0, 1.0)
                } else {
                    0.0
                };
                let cache_warmth = match station_core::shared::cache_clock::cache_clock(
                    s.cache.as_ref(),
                    now_ms(),
                ) {
                    view if view.state == "warm" => {
                        CacheWarmth::Warm(((view.left as f64 / 60_000.0).round() as u32).max(1))
                    }
                    view if view.state == "cold" => CacheWarmth::Cold,
                    _ => CacheWarmth::Unknown,
                };
                let (git_branch, git_dirty) = match &s.git {
                    Some(g) => (
                        (!g.branch.is_empty()).then(|| g.branch.clone()),
                        g.dirty,
                    ),
                    None => (None, 0),
                };
                SessionCard {
                    id: s.id.clone(),
                    agent: s.agent.clone(),
                    project: s.name.clone(),
                    dir: s.cwd.clone(),
                    git_branch,
                    git_dirty,
                    cache_warmth,
                    todos_done: 0,
                    todos_total: 0,
                    status: normalize_status(&s.status).to_owned(),
                    model: s.model.clone().unwrap_or_else(|| "default".to_owned()),
                    last_prompt: s.last_prompt.clone().unwrap_or_default(),
                    last_reply: String::new(),
                    tool: s.last.as_ref().map(|t| t.label.clone()),
                    tool_kind: s.last.as_ref().map(|t| t.kind.clone()),
                    desk: s.desk,
                    cost_usd: s.cost_usd,
                    tokens: if tok > 0 { tok } else { s.tool_calls * 420 },
                    pinned: office.pins.contains_key(&s.id),
                    age_label: String::new(),
                    ctx_pct,
                    subagents: Vec::new(),
                }
            })
            .collect();
        sessions.sort_by(|a, b| a.desk.cmp(&b.desk).then_with(|| a.id.cmp(&b.id)));
        ui.sessions = sessions;
        if let Some(sel) = ui.selected_session.as_deref()
            && !ui.sessions.iter().any(|s| s.id == sel)
        {
            ui.selected_session = None;
        }

        let mut perms: Vec<PendingPermCard> = office
            .pending_perms
            .values()
            .map(|p| {
                let meta = if p.kind == "denied" {
                    office.chat.by_id(&p.id).and_then(|m| m.deny.as_ref()).map(|v| {
                        station_core::redact::redact_and_clip_json_value(
                            v,
                            station_core::redact::TOOL_INPUT_WIRE_MAX_STR_CHARS,
                        )
                    })
                } else {
                    None
                };
                PendingPermCard {
                    id: p.id.clone(),
                    session_id: p.sess.clone(),
                    agent: p.name.clone(),
                    tool: p.tool_type.clone(),
                    summary: p.detail.clone(),
                    kind: p.kind.clone(),
                    danger: p.danger.clone(),
                    tool_input: office.hold_tool_input(&p.id).map(|v| {
                        station_core::redact::redact_and_clip_json_value(
                            v,
                            station_core::redact::TOOL_INPUT_WIRE_MAX_STR_CHARS,
                        )
                    }),
                    meta,
                }
            })
            .collect();

        // Surface active stuck-loop alerts into `perm-stack`.
        for s in office.sessions.values() {
            if let Some(st) = office.loops.stuck_of(&s.id) {
                let stuck_id = format!("stuck:{}", s.id);
                if !perms.iter().any(|p| p.id == stuck_id) {
                    perms.push(PendingPermCard {
                        id: stuck_id,
                        session_id: s.id.clone(),
                        agent: s.name.clone(),
                        tool: "loop".to_owned(),
                        summary: st.label.clone().unwrap_or_default(),
                        kind: "stuck".to_owned(),
                        danger: None,
                        tool_input: None,
                        meta: Some(json!({
                            "stuck_kind": st.kind,
                            "stuck_n": st.n.unwrap_or(0),
                            "stuck_next": st.n.unwrap_or(0) + 1,
                            "stuck_mins": st.mins.unwrap_or(0),
                            "stuck_usd": st.usd,
                        })),
                    });
                }
            }
        }

        // Surface active red, unfixed CI failure alerts into `perm-stack`.
        for m in &office.chat.msgs {
            if m.kind == "ci"
                && !dismissed_ci_ids.contains(&m.id)
                && let Some(ci) = &m.ci
                && ci.get("state").and_then(Value::as_str) == Some("red")
                && ci.get("fix").and_then(Value::as_str).is_none()
                && !perms.iter().any(|p| p.id == m.id)
            {
                let wf = ci.get("wf").and_then(Value::as_str).unwrap_or("CI");
                let repo = ci.get("repo").and_then(Value::as_str).unwrap_or("");
                let branch = ci.get("branch").and_then(Value::as_str).unwrap_or("");
                let sess = ci
                    .get("sid")
                    .or_else(|| ci.get("sess"))
                    .and_then(Value::as_str)
                    .unwrap_or_default();
                perms.push(PendingPermCard {
                    id: m.id.clone(),
                    session_id: sess.to_owned(),
                    agent: repo.to_owned(),
                    tool: wf.to_owned(),
                    summary: format!("{wf} · {repo} ({branch})"),
                    kind: "ci".to_owned(),
                    danger: None,
                    tool_input: None,
                    meta: Some(ci.clone()),
                });
            }
        }
        ui.pending_perms = perms;
        ui.session_pins = office.pins.clone();
        ui.session_todos = parse_todos_from_values(&office.todos);

        ui.feed = office
            .feed
            .iter()
            .rev()
            .take(30)
            .map(|f| FeedEntry {
                ts_label: format_hhmm_ms(f.ts),
                agent: f.name.clone(),
                message: f.label.clone(),
                level: if f.kind == "error" || f.kind == "fail" {
                    "err".to_owned()
                } else if f.kind == "wait" {
                    "warn".to_owned()
                } else {
                    "info".to_owned()
                },
                kind: normalize_feed_kind(&f.kind).to_owned(),
                attention: matches!(f.kind.as_str(), "error" | "fail" | "wait"),
            })
            .collect();
        ui.kpi_tasks = office.sessions.values().map(|s| s.tool_calls).sum();
        ui.kpi_edits = office.sessions.values().map(|s| s.edits).sum();
        ui.kpi_cmds = office
            .feed
            .iter()
            .filter(|f| normalize_feed_kind(&f.kind) == "cmd")
            .count() as u64;
        ui.kpi_reads = office
            .feed
            .iter()
            .filter(|f| normalize_feed_kind(&f.kind) == "read")
            .count() as u64;

        sync_office_chat_to_ui(&office, ui);
    }
    if let Ok(accts) = app_state.claude_accounts.lock() {
        ui.accounts = parse_accounts_from_view(&accts.view());
    }
}

fn sync_in_process_telemetry(app_state: &AppState, ui: &mut UiState, refresh_sys: bool) {
    ui.pairing_pin = app_state.auth.current_pin();

    let now = now_ms();
    if let Ok(otel) = app_state.otel.lock() {
        let today = otel.burn_summary("today", now);
        let month = otel.burn_summary("30d", now);
        let per_model = today
            .by
            .get("model")
            .map(|rows| rows.iter().map(|r| (r.name.clone(), r.usd)).collect())
            .unwrap_or_default();
        ui.burn = BurnSnapshot {
            today_usd: today.total.row.usd,
            month_usd: month.total.row.usd,
            input_tokens: today.total.row.tok.input as u64,
            output_tokens: today.total.row.tok.out as u64,
            cache_read_tokens: today.total.row.tok.cache_read as u64,
            per_model,
        };
    }

    if let Ok(vault) = app_state.vault.lock() {
        ui.vault_results = vault
            .search(&ui.vault_query, 30)
            .into_iter()
            .map(|e| {
                let title = if e.title.is_empty() {
                    e.sid.clone()
                } else {
                    e.title.clone()
                };
                let body = if !e.first.is_empty() {
                    e.first.clone()
                } else {
                    e.cwd.clone()
                };
                (title, body)
            })
            .collect();
    }

    if refresh_sys {
        let snap = app_state.sys.snapshot();
        let gb = 1024.0 * 1024.0 * 1024.0;
        let disk_pct = if snap.disk.total > 0 {
            ((snap.disk.used as f64 / snap.disk.total as f64) * 100.0) as f32
        } else {
            0.0
        };
        let cpu_pct = snap
            .cpu
            .get("pct")
            .or_else(|| snap.cpu.get("used"))
            .and_then(Value::as_f64)
            .unwrap_or(0.0) as f32;
        let mut ports = Vec::new();
        for srv in &snap.dev_servers {
            for &p in &srv.ports {
                ports.push((p, srv.label.clone()));
            }
        }
        let my_pid = std::process::id();
        let private_mb = app_state
            .sys
            .platform()
            .processes()
            .procs
            .get(&my_pid)
            .map(|p| (p.mem as f64 / (1024.0 * 1024.0)) as f32)
            .unwrap_or(ui.sys.private_mb);
        ui.sys = SysSnapshot {
            cpu_pct,
            mem_used_gb: (snap.mem.used as f64 / gb) as f32,
            mem_total_gb: (snap.mem.total as f64 / gb) as f32,
            disk_pct,
            ports,
            private_mb,
        };
        let auto_hours = app_state
            .prefs
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .auto_close();
        let mut sys_val = json!(snap);
        if let Some(obj) = sys_val.as_object_mut() {
            obj.insert("auto".to_owned(), json!({ "hours": auto_hours }));
        }
        merge_mac_drawer_telemetry(&mut ui.mac_drawer, parse_mac_drawer_from_json(&sys_val));
    }
}

fn sync_live_state_to_ui(
    time: Res<Time>,
    mut bridge: ResMut<LiveServerBridge>,
    mut ui: ResMut<UiState>,
    i18n: Option<Res<I18nStore>>,
    windows: Query<&Window>,
    mut tray: Option<ResMut<TrayState>>,
) {
    ui.clock_label = Local::now().format("%H:%M").to_string();

    if !bridge.banner_pushed {
        if let Some(err) = bridge.bind_error_message() {
            ui.server_banner = Some(err.to_owned());
        }
        bridge.banner_pushed = true;
    }

    while let Ok((title, body)) = bridge.toast_rx.try_recv() {
        ui.push_toast(title, body);
    }

    while let Ok(res) = bridge.async_rx.try_recv() {
        match res {
            BridgeAsyncResult::SessionLog { session_id, items } => {
                ui.session_logs.insert(session_id, items);
            }
            BridgeAsyncResult::SessionUndo(view) => {
                ui.session_undo = Some(view);
            }
            BridgeAsyncResult::RewriteResult(res) => {
                ui.compose_rewriting = false;
                let title = i18n
                    .as_deref()
                    .map(|t| t.tr("chat.coach.label"))
                    .unwrap_or_else(|| "chat.coach.label".to_owned());
                match res {
                    Ok(rewritten) if !rewritten.trim().is_empty() => {
                        ui.compose_draft = rewritten;
                        let body = i18n
                            .as_deref()
                            .map(|t| t.tr("chat.coach.rewritten"))
                            .unwrap_or_else(|| "chat.coach.rewritten".to_owned());
                        ui.push_toast(title, body);
                    }
                    Ok(_) => {
                        let body = i18n
                            .as_deref()
                            .map(|t| t.tr("chat.coach.why.empty"))
                            .unwrap_or_else(|| "chat.coach.why.empty".to_owned());
                        ui.push_toast(title, body);
                    }
                    Err(err) => {
                        ui.push_toast(title, err);
                    }
                }
            }
            BridgeAsyncResult::WrappedSummary(res) => match res {
                Ok(mut summary) => {
                    summary.last_saved_path = ui.wrapped.last_saved_path.clone();
                    ui.wrapped = summary;
                }
                Err(err) => {
                    ui.wrapped.loading = false;
                    ui.wrapped.loaded = true;
                    ui.wrapped.error = Some(err);
                }
            },
            BridgeAsyncResult::WrappedExported {
                rel_path,
                toast_title,
                toast_body,
            } => {
                if let Some(path) = rel_path {
                    ui.wrapped.last_saved_path = Some(path);
                }
                ui.push_toast(toast_title, toast_body);
            }
            BridgeAsyncResult::ChatImage(res) => match res {
                Ok(preview) => {
                    ui.chat_preview_image = Some(preview);
                }
                Err(err) => {
                    let title = i18n
                        .as_deref()
                        .map(|t| t.tr("chat.photo"))
                        .unwrap_or_else(|| "chat.photo".to_owned());
                    ui.push_toast(title, err);
                }
            },
            BridgeAsyncResult::BurnData(res) => match res {
                Ok(data) => {
                    ui.burn_drawer = data;
                }
                Err(err) => {
                    ui.burn_drawer.loading = false;
                    ui.burn_drawer.loaded_range = Some(ui.burn_drawer.range);
                    ui.burn_drawer.error = Some(err);
                }
            },
            BridgeAsyncResult::VaultList {
                seq,
                items,
                total_count,
                total_bytes,
            } => {
                if seq >= ui.vault_drawer.req_seq {
                    ui.vault_drawer.loading = false;
                    ui.vault_drawer.loaded = true;
                    ui.vault_drawer.items = items;
                    ui.vault_drawer.total_count = total_count;
                    ui.vault_drawer.total_bytes = total_bytes;
                }
            }
            BridgeAsyncResult::VaultTranscript(rec) => {
                ui.vault_drawer.transcript_loading = None;
                ui.vault_drawer.open_transcript = rec;
            }
            BridgeAsyncResult::VaultReport(res) => match res {
                Ok(report) => {
                    ui.vault_drawer.report = report;
                }
                Err(err) => {
                    ui.vault_drawer.report.loading = false;
                    ui.vault_drawer.report.loaded = true;
                    ui.vault_drawer.report.error = Some(err);
                }
            },
            BridgeAsyncResult::StrandedData { items, repos } => {
                ui.mac_drawer.stranded_loading = false;
                ui.mac_drawer.stranded_loaded = true;
                ui.mac_drawer.stranded = items;
                ui.mac_drawer.stranded_repos = repos;
            }
            BridgeAsyncResult::AiAccountsData {
                claude,
                claude_auto,
                claude_move,
                claude_resume,
                codex,
                codex_auto,
                codex_seat_note,
                ag,
                ag_auto,
                ag_away,
                ag_pending,
                ag_note,
            } => {
                ui.ai_accounts.loaded = true;
                ui.ai_accounts.claude_auto = claude_auto;
                ui.ai_accounts.claude_move = claude_move;
                ui.ai_accounts.claude_resume = claude_resume;
                ui.ai_accounts.codex = codex;
                ui.ai_accounts.codex_auto = codex_auto;
                ui.ai_accounts.codex_seat_note = codex_seat_note;
                ui.ai_accounts.ag = ag;
                ui.ai_accounts.ag_auto = ag_auto;
                ui.ai_accounts.ag_away = ag_away;
                ui.ai_accounts.ag_pending = ag_pending;
                ui.ai_accounts.ag_note = ag_note;
                ui.accounts = claude
                    .iter()
                    .map(|a| AccountOption {
                        slug: a.slug.clone(),
                        label: a.label.clone(),
                        active: a.active,
                        pct: a.windows.first().map(|w| w.pct.round().clamp(0.0, 100.0) as u32),
                    })
                    .collect();
                ui.ai_accounts.claude = claude;
            }
            BridgeAsyncResult::AiAccountNotice {
                title,
                ok,
                code,
                ok_msg,
                err_msg,
            } => {
                let body = if ok {
                    ok_msg
                } else {
                    code.as_deref()
                        .and_then(|c| i18n.as_deref().and_then(|tr| resolve_ai_error_code(tr, c)))
                        .unwrap_or(err_msg)
                };
                ui.push_toast(title, body);
            }
            BridgeAsyncResult::PhoneStatus {
                lan_on,
                lan_ip,
                tunnel_on,
                tunnel_state,
                tunnel_url,
                tunnel_domain,
                tunnel_error,
                phone_approve,
                push_count,
                pin,
            } => {
                ui.phone_modal.loaded = true;
                ui.phone_modal.lan_on = lan_on;
                ui.phone_modal.lan_ip = lan_ip;
                ui.phone_modal.tunnel_on = tunnel_on;
                ui.phone_modal.tunnel_state = tunnel_state;
                ui.phone_modal.tunnel_url = tunnel_url;
                ui.phone_modal.tunnel_domain = tunnel_domain.clone();
                if ui.phone_modal.tunnel_domain_draft.is_empty() && !tunnel_domain.is_empty() {
                    ui.phone_modal.tunnel_domain_draft = tunnel_domain;
                }
                ui.phone_modal.tunnel_error = tunnel_error;
                ui.phone_modal.phone_approve = phone_approve;
                ui.phone_modal.push_count = push_count;
                if let Some(p) = pin {
                    ui.pairing_pin = p;
                }
            }
        }
    }

    match &mut bridge.backend {
        BridgeBackend::InProcess { harness_poller, .. }
        | BridgeBackend::BindError { harness_poller, .. } => {
            if let Some(items) = harness_poller.take_latest()
                && !items.is_empty()
            {
                ui.harnesses = items;
            }
        }
        BridgeBackend::AttachedHttp { snapshot, .. } => {
            if let Ok(snap) = snapshot.try_lock()
                && !snap.harnesses.is_empty()
                && ui.harnesses.is_empty()
            {
                ui.harnesses = snap.harnesses.clone();
            }
        }
    }

    let has_window = windows.iter().next().is_some();
    let window_focused = windows.iter().any(|w| w.visible && w.focused);
    let in_tray = tray.as_deref().is_some_and(|t| t.in_tray);
    let unfocused_or_tray = in_tray || !window_focused;

    if has_window && bridge.last_window_focused != Some(window_focused) {
        let sent = match &bridge.backend {
            BridgeBackend::InProcess { app_state, .. } => {
                if let Ok(mut office) = app_state.office.lock() {
                    office.presence_focused = window_focused;
                    true
                } else {
                    false
                }
            }
            BridgeBackend::AttachedHttp { action_tx, .. } => action_tx
                .try_send(RemoteActionCmd {
                    path: "/api/presence",
                    body: json!({ "focused": window_focused }),
                    refresh: RefreshKind::None,
                    launch_toast_label: None,
                })
                .is_ok(),
            BridgeBackend::BindError { .. } => true,
        };
        if sent {
            bridge.last_window_focused = Some(window_focused);
        }
    }

    bridge.sys_sync_timer.tick(time.delta());
    if bridge.sys_sync_timer.just_finished() {
        bridge.sys_refresh_due = true;
    }
    bridge.sync_timer.tick(time.delta());
    if !bridge.sync_timer.just_finished() {
        return;
    }

    let refresh_sys = bridge.sys_refresh_due;
    let mut synced_live_perms = false;
    match &bridge.backend {
        BridgeBackend::AttachedHttp { snapshot, .. } => {
            let snap = snapshot.lock().unwrap_or_else(|e| e.into_inner()).clone();
            if !ui.demo_running {
                synced_live_perms = snap.has_snapshot;
                apply_remote_snapshot_to_ui(&snap, &mut ui);
                ui.pending_perms
                    .retain(|p| !(p.kind == "ci" && bridge.dismissed_ci_ids.contains(&p.id)));
            } else if let Some(pin) = &snap.pairing_pin {
                ui.pairing_pin = pin.clone();
            }
        }
        BridgeBackend::InProcess { app_state, .. } => {
            if !ui.demo_running {
                synced_live_perms = true;
                sync_office_core_to_ui(app_state, &bridge.dismissed_ci_ids, &mut ui);
            }
            sync_in_process_telemetry(app_state, &mut ui, refresh_sys);
            if refresh_sys {
                bridge.sys_refresh_due = false;
            }
        }
        BridgeBackend::BindError { .. } => {}
    }

    if synced_live_perms {
        let mut current_ids = HashSet::with_capacity(ui.pending_perms.len());
        for perm in &ui.pending_perms {
            current_ids.insert(perm.id.clone());
            if bridge.seen_perm_ids.insert(perm.id.clone()) && unfocused_or_tray {
                let _ = notify_parent_hold_alert(&perm.kind);
                if let Some(t) = tray.as_deref_mut() {
                    t.notifications_sent = t.notifications_sent.saturating_add(1);
                }
            }
        }
        bridge.seen_perm_ids.retain(|id| current_ids.contains(id));
    }
}

#[derive(SystemParam)]
#[allow(clippy::type_complexity)]
struct LiveSync3dParams<'w, 's> {
    chibi_cache: Option<Res<'w, station_3d::ChibiMeshCache>>,
    pal: Option<ResMut<'w, WorldPalette>>,
    materials: Option<ResMut<'w, Assets<StandardMaterial>>>,
    screen_mats: Option<Res<'w, station_3d::desks::ScreenMaterials>>,
    nav: Option<Res<'w, station_3d::NavGraph>>,
    layout: Option<Res<'w, station_3d::OfficeLayout>>,
    scratch: bevy::prelude::Local<'s, station_3d::LiveSyncScratch>,
    specs_buf: bevy::prelude::Local<'s, Vec<station_3d::LiveWorkerSpec>>,
    last_sync_sig: bevy::prelude::Local<'s, Option<(u64, station_3d::OfficeLayout, station_3d::ChibiLook)>>,
    preview_q: Query<'w, 's, Entity, With<station_3d::people::PreviewCastWorker>>,
    workers_q: Query<
        'w,
        's,
        (
            Entity,
            &'static mut station_3d::people::LiveSessionWorker,
            &'static mut station_3d::chibi::ChibiWorker,
            Option<&'static mut station_3d::NavWalker>,
            Option<&'static mut station_3d::WorkerLifecycle>,
            Option<&'static station_3d::ActivePropEntity>,
            Option<&'static Children>,
        ),
    >,
    subagents_q: Query<
        'w,
        's,
        (
            Entity,
            &'static mut station_3d::SubagentWorker,
            &'static station_3d::chibi::ChibiWorker,
        ),
        Without<station_3d::people::LiveSessionWorker>,
    >,
    dashes_q: Query<'w, 's, (Entity, &'static station_3d::TeamLineDash)>,
    halo_q: Query<
        'w,
        's,
        &'static mut MeshMaterial3d<StandardMaterial>,
        (
            With<station_3d::chibi::StatusHalo>,
            Without<station_3d::desks::DeskScreen>,
        ),
    >,
    screens_q: Query<
        'w,
        's,
        (
            &'static station_3d::desks::DeskScreen,
            &'static mut MeshMaterial3d<StandardMaterial>,
        ),
        Without<station_3d::chibi::StatusHalo>,
    >,
}

fn fingerprint_sessions_for_3d(sessions: &[SessionCard]) -> u64 {
    let mut h: u64 = 0xcbf29ce484222325;
    let mut mix = |bytes: &[u8]| {
        for &b in bytes {
            h ^= b as u64;
            h = h.wrapping_mul(0x100000001b3);
        }
        h ^= 0xff;
        h = h.wrapping_mul(0x100000001b3);
    };
    mix(&(sessions.len() as u64).to_le_bytes());
    for s in sessions {
        mix(s.id.as_bytes());
        mix(s.agent.as_bytes());
        mix(s.project.as_bytes());
        mix(s.status.as_bytes());
        mix(&(s.desk as u64).to_le_bytes());
        if let Some(k) = s.tool_kind.as_deref() {
            mix(k.as_bytes());
        } else {
            mix(&[]);
        }
        if let Some(t) = s.tool.as_deref() {
            mix(t.as_bytes());
        } else {
            mix(&[]);
        }
        mix(&(s.git_dirty as u64).to_le_bytes());
        mix(&(s.subagents.len() as u64).to_le_bytes());
        for sub in &s.subagents {
            mix(sub.id.as_bytes());
            mix(sub.label.as_bytes());
            mix(sub.doing.as_bytes());
        }
    }
    h
}

fn sync_live_sessions_to_3d(
    mut commands: Commands,
    ui: Res<UiState>,
    mut rack_telemetry: Option<ResMut<station_3d::ServerRackTelemetry>>,
    mut props_telemetry: Option<ResMut<station_3d::PropsLiveTelemetry>>,
    mut sync3d: LiveSync3dParams,
) {
    if let Some(ref mut rack) = rack_telemetry {
        rack.cpu_load = (ui.sys.cpu_pct / 100.0).clamp(0.0, 1.0);
    }
    if let Some(ref mut ptel) = props_telemetry {
        ptel.live_synced = true;
        ptel.desks.clear();
        for s in &ui.sessions {
            let warmth = match s.cache_warmth {
                CacheWarmth::Warm(mins) => (mins as f32 / 5.0).clamp(0.15, 1.0),
                CacheWarmth::Cold => 0.0,
                CacheWarmth::Unknown => -1.0,
            };
            ptel.desks.push(station_3d::DeskPropTelemetry {
                desk_index: s.desk,
                occupied: true,
                resting: s.status == "rest",
                ctx_pct: s.ctx_pct.clamp(0.0, 1.0),
                cache_warmth: warmth,
            });
        }
        ptel.dev_servers.clear();
        for srv in &ui.mac_drawer.dev_servers {
            let owner_desk = if srv.dir.is_empty() {
                None
            } else {
                ui.sessions
                    .iter()
                    .find(|s| !s.dir.is_empty() && s.dir.eq_ignore_ascii_case(&srv.dir))
                    .map(|s| s.desk)
            };
            ptel.dev_servers.push(station_3d::DevServerSpec {
                port: srv.ports.first().copied(),
                desk_index: owner_desk,
                alive: owner_desk.is_some(),
                cpu_pct: srv.cpu.max(0) as f32,
                label: srv.label.clone(),
            });
        }
    }
    let (Some(chibi_cache), Some(mut pal), Some(mut mats), Some(scr_mats)) = (
        sync3d.chibi_cache,
        sync3d.pal,
        sync3d.materials,
        sync3d.screen_mats,
    ) else {
        return;
    };
    let lay = sync3d.layout.map(|l| *l).unwrap_or(ui.layout);
    let sig = (fingerprint_sessions_for_3d(&ui.sessions), lay, ui.chibi_look);
    let world_fresh = chibi_cache.is_added() || !sync3d.preview_q.is_empty();
    if !world_fresh && *sync3d.last_sync_sig == Some(sig) {
        return;
    }
    *sync3d.last_sync_sig = Some(sig);

    let fallback_nav;
    let nav_ref: &station_3d::NavGraph = match sync3d.nav.as_deref() {
        Some(n) => n,
        None => {
            fallback_nav = station_3d::NavGraph::for_layout(lay);
            &fallback_nav
        }
    };
    sync3d.specs_buf.clear();
    sync3d
        .specs_buf
        .extend(ui.sessions.iter().map(|s| station_3d::LiveWorkerSpec {
            session_id: s.id.clone(),
            agent: s.agent.clone(),
            name: s.project.clone(),
            status: s.status.clone(),
            desk_hint: s.desk,
            tool_kind: s.tool_kind.clone(),
            tool_label: s.tool.clone(),
            git_dirty: s.git_dirty,
            subagents: s
                .subagents
                .iter()
                .map(|sub| station_3d::SubagentSpec {
                    id: sub.id.clone(),
                    label: sub.label.clone(),
                    doing: sub.doing.clone(),
                })
                .collect(),
        }));
    station_3d::people::sync_live_workers_and_screens(
        &mut commands,
        &chibi_cache,
        &mut pal,
        &mut mats,
        &scr_mats,
        nav_ref,
        lay,
        ui.chibi_look,
        &sync3d.specs_buf,
        &mut sync3d.scratch,
        &sync3d.preview_q,
        &mut sync3d.workers_q,
        &mut sync3d.subagents_q,
        &sync3d.dashes_q,
        &mut sync3d.halo_q,
        &mut sync3d.screens_q,
    );
}

fn handle_picking_events(
    mut ui: ResMut<UiState>,
    i18n: Res<I18nStore>,
    mut worker_ev: EventReader<station_3d::WorkerSelected>,
    mut prop_ev: EventReader<station_3d::interact::PropActivated>,
) {
    for ev in worker_ev.read() {
        ui.selected_session = Some(ev.session_id.clone());
        ui.active_drawer = ActiveDrawer::Session;
    }
    for ev in prop_ev.read() {
        match ev.id {
            "whiteboard" => ui.active_drawer = ActiveDrawer::Whiteboard,
            "tv" | "dev-servers" => ui.active_drawer = ActiveDrawer::System,
            _ => {
                ui.active_place_card = Some(build_place_card_info(ev.place, &ui, &i18n));
            }
        }
    }
}

fn load_replay_range_into_state(
    data_dir: &std::path::Path,
    range: ReplayRange,
    rep: &mut station_3d::replay::ReplayState,
) {
    let now = now_ms();
    let (from, to) = match range {
        ReplayRange::H1 => (now.saturating_sub(3_600_000), now),
        ReplayRange::H3 => (now.saturating_sub(10_800_000), now),
        ReplayRange::Today => (now.saturating_sub(86_400_000), now),
        ReplayRange::Yesterday => (
            now.saturating_sub(172_800_000),
            now.saturating_sub(86_400_000),
        ),
    };
    let items = station_core::state::sessions::read_replay_range(data_dir, from, to);
    let events: Vec<station_3d::replay::ReplayEvent> = items
        .into_iter()
        .map(|it| station_3d::replay::ReplayEvent {
            ts: it.t,
            session_id: it
                .m
                .get("s")
                .and_then(|s| s.get("id"))
                .or_else(|| it.m.get("id"))
                .and_then(Value::as_str)
                .unwrap_or_default()
                .to_owned(),
            kind: it
                .m
                .get("k")
                .and_then(Value::as_str)
                .unwrap_or("ev")
                .to_owned(),
            summary: it
                .m
                .get("e")
                .and_then(|e| e.get("label"))
                .and_then(Value::as_str)
                .unwrap_or_default()
                .to_owned(),
        })
        .collect();
    rep.load_events(events);
}

fn build_replay_heatmap(rep: &station_3d::replay::ReplayState, bins: usize) -> Vec<f32> {
    if rep.events.is_empty() || rep.end_ms <= rep.start_ms || bins == 0 {
        return vec![0.12; bins.max(16)];
    }
    let span = (rep.end_ms - rep.start_ms).max(1) as f64;
    let mut counts = vec![0u32; bins];
    for ev in &rep.events {
        let frac = (ev.ts.saturating_sub(rep.start_ms) as f64 / span).clamp(0.0, 0.9999);
        let idx = (frac * bins as f64) as usize;
        counts[idx.min(bins - 1)] += 1;
    }
    let peak = counts.iter().copied().max().unwrap_or(1).max(1) as f32;
    counts
        .into_iter()
        .map(|c| (c as f32 / peak).max(0.08))
        .collect()
}

fn sync_replay_ui_readout(rep: &station_3d::replay::ReplayState, ui: &mut UiState) {
    ui.replay_active = rep.active;
    ui.replay_playing = rep.playing;
    if rep.end_ms > rep.start_ms {
        let span = (rep.end_ms - rep.start_ms) as f32;
        ui.replay_progress =
            (rep.cursor_ms.saturating_sub(rep.start_ms) as f32 / span).clamp(0.0, 1.0);
        ui.replay_time_label = format_hhmm_ms(rep.cursor_ms);
        ui.replay_span_label = format!(
            "{} – {}",
            format_hhmm_ms(rep.start_ms),
            format_hhmm_ms(rep.end_ms)
        );
    }
}

fn step_replay_system(
    time: Res<Time>,
    mut ui: ResMut<UiState>,
    mut replay: Option<ResMut<station_3d::replay::ReplayState>>,
) {
    let Some(rep) = replay.as_deref_mut() else {
        return;
    };
    if rep.active && rep.playing {
        rep.step(time.delta_secs());
        sync_replay_ui_readout(rep, &mut ui);
    }
}

#[derive(SystemParam)]
struct WorldActionParams<'w, 's> {
    world_q: Query<'w, 's, Entity, With<WorldEntity>>,
    backdrop_q: Query<'w, 's, Entity, With<BackdropEntity>>,
    lite_cam_q: Query<'w, 's, Entity, With<station_3d::Lite2dCamera>>,
    pal: Option<ResMut<'w, WorldPalette>>,
    backdrop_assets: Option<ResMut<'w, BackdropAssets>>,
    ibl: Option<Res<'w, station_3d::OfficeIbl>>,
    chibi_meshes: Option<Res<'w, station_3d::ChibiMeshCache>>,
    meshes: Option<ResMut<'w, Assets<Mesh>>>,
    materials: Option<ResMut<'w, Assets<StandardMaterial>>>,
    images: Option<ResMut<'w, Assets<Image>>>,
    shared_fonts: Option<ResMut<'w, station_3d::SharedFontEngine>>,
    clear_color: ResMut<'w, ClearColor>,
    lighting: Option<ResMut<'w, station_3d::lights::LightingState>>,
}

#[derive(SystemParam)]
struct TrayActionParams<'w, 's> {
    worker_mode: Option<Res<'w, GuiWorkerMode>>,
    app_exit: EventWriter<'w, AppExit>,
    windows: Query<'w, 's, &'static mut Window>,
    fps: Option<ResMut<'w, AdaptiveFpsState>>,
    tray: Option<ResMut<'w, TrayState>>,
}

fn drive_pending_photo_capture(
    mut commands: Commands,
    bridge: Res<LiveServerBridge>,
    mut ui: ResMut<UiState>,
    mut pending: ResMut<PendingPhotoCapture>,
) {
    if let Some(job) = pending.0.as_mut() {
        if job.frames_until_shoot > 0 {
            job.frames_until_shoot -= 1;
            return;
        }
        let job = pending.0.take().expect("checked Some");
        let toast_tx = bridge.toast_tx.clone();
        commands
            .spawn(Screenshot::primary_window())
            .observe(move |trigger: Trigger<ScreenshotCaptured>| {
                let raw_img = trigger.event().0.clone();
                let job = job.clone();
                let toast_tx = toast_tx.clone();
                std::thread::spawn(move || {
                    if let Ok(dyn_img) = raw_img.try_into_dynamic() {
                        let rgba = dyn_img.to_rgba8();
                        let (target_w, target_h) =
                            job.size.target_dimensions(rgba.width(), rgba.height());
                        let mut out_rgba = if (target_w, target_h) == (rgba.width(), rgba.height()) {
                            rgba
                        } else {
                            image::imageops::resize(
                                &rgba,
                                target_w,
                                target_h,
                                image::imageops::FilterType::Lanczos3,
                            )
                        };
                        if job.show_names || job.show_mark {
                            let mut fonts =
                                station_canvas2d::FontEngine::from_assets_dir(&job.assets_dir);
                            let tags = if job.show_names { job.tags.as_slice() } else { &[] };
                            let mark = job.show_mark.then_some("Agents Station");
                            station_canvas2d::paint_photo_overlays(
                                out_rgba.as_mut(),
                                target_w,
                                target_h,
                                tags,
                                mark,
                                &mut fonts,
                            );
                        }
                        let _ = std::fs::create_dir_all(&job.shots_dir);
                        let fname = format!("shot-{target_w}x{target_h}-{}.png", now_ms());
                        let shot_path = job.shots_dir.join(&fname);
                        let saved = out_rgba.save(&shot_path).is_ok();
                        drop(out_rgba);
                        if saved {
                            let _ = toast_tx.try_send((
                                job.title,
                                format!("shots/{fname} ({target_w}x{target_h})"),
                            ));
                        }
                    }
                });
            });
    }
    if ui.photo_capture_hide_frames > 0 {
        ui.photo_capture_hide_frames -= 1;
    }
}

fn timelapse_dimensions(shape: ClipShape) -> (u32, u32) {
    match shape {
        ClipShape::Square => (360, 360),
        ClipShape::Tall => (360, 640),
    }
}

fn write_timelapse_wav_if_enabled(path: &std::path::Path) -> bool {
    let wav = crate::clip_music::generate_lofi_wav(15.0, 22_050);
    let ok = std::fs::write(path, &wav).is_ok();
    drop(wav);
    ok
}

fn write_headless_timelapse_gif(
    gif_path: &std::path::Path,
    shape: ClipShape,
    ui: &UiState,
    shared_fonts: Option<&mut station_3d::SharedFontEngine>,
    assets_dir: &std::path::Path,
) -> bool {
    let (w, h) = timelapse_dimensions(shape);
    let Ok(file) = std::fs::File::create(gif_path) else {
        return false;
    };
    let mut encoder = image::codecs::gif::GifEncoder::new_with_speed(file, 20);
    let _ = encoder.set_repeat(image::codecs::gif::Repeat::Infinite);
    let mut fallback_fonts = if shared_fonts.is_none() {
        Some(station_canvas2d::FontEngine::from_assets_dir(assets_dir))
    } else {
        None
    };
    let fonts = match shared_fonts {
        Some(sf) => sf.get_or_init(),
        None => fallback_fonts.as_mut().expect("fallback fonts"),
    };

    const FRAMES: u32 = 15;
    for f in 0..FRAMES {
        let mut c = station_canvas2d::Canvas2d::new(w, h);
        let dusk = f >= 10;
        let top = if dusk {
            station_canvas2d::hex("#101628")
        } else {
            station_canvas2d::hex("#1C2740")
        };
        let bot = if dusk {
            station_canvas2d::hex("#0A0E19")
        } else {
            station_canvas2d::hex("#121A2C")
        };
        c.linear_gradient_rect(
            0.0,
            0.0,
            w as f32,
            h as f32,
            (0.0, 0.0),
            (0.0, h as f32),
            &[(0.0, top), (1.0, bot)],
        );
        let prog = f as f32 / (FRAMES - 1) as f32;
        let hour = 8 + (prog * 15.0).round() as u32;
        let clock = format!("{hour:02}:00");
        c.round_rect(14.0, 14.0, 88.0, 30.0, 15.0, [14, 19, 27, 200]);
        fonts.draw_text(
            &mut c,
            &clock,
            58.0,
            19.0,
            16.0,
            station_canvas2d::hex("#FFD166"),
            station_canvas2d::TextAlign::Center,
            None,
            true,
            true,
        );
        let cx = w as f32 * (0.2 + 0.6 * prog);
        let cy = h as f32 * 0.55;
        c.fill_circle(cx, cy, 26.0, station_canvas2d::hex("#EF8A5F"));
        fonts.draw_text(
            &mut c,
            &format!("Agents Station · {}", ui.sessions.len()),
            w as f32 * 0.5,
            h as f32 - 34.0,
            14.0,
            station_canvas2d::hex("#E8ECF4"),
            station_canvas2d::TextAlign::Center,
            Some(w as f32 - 24.0),
            true,
            false,
        );
        let surf = c.into_surface();
        let Some(rgba) = image::RgbaImage::from_raw(surf.width, surf.height, surf.rgba) else {
            return false;
        };
        let frame = image::Frame::from_parts(
            rgba,
            0,
            0,
            image::Delay::from_numer_denom_ms(1000, 1),
        );
        if encoder.encode_frame(frame).is_err() {
            return false;
        }
    }
    true
}

fn spawn_timelapse_encoder_worker(
    shape: ClipShape,
    music: bool,
    gif_path: PathBuf,
    wav_path: Option<PathBuf>,
    title: String,
    toast_tx: mpsc::Sender<(String, String)>,
    cancel_flag: Arc<std::sync::atomic::AtomicBool>,
) -> Option<std::sync::mpsc::SyncSender<Image>> {
    let file = std::fs::File::create(&gif_path).ok()?;
    let (tx, rx) = std::sync::mpsc::sync_channel::<Image>(1);
    let (tw, th) = timelapse_dimensions(shape);
    std::thread::Builder::new()
        .name("station-timelapse-encoder".to_owned())
        .spawn(move || {
            let mut enc = image::codecs::gif::GifEncoder::new_with_speed(file, 20);
            let _ = enc.set_repeat(image::codecs::gif::Repeat::Infinite);
            for raw_img in rx {
                if cancel_flag.load(std::sync::atomic::Ordering::Acquire) {
                    break;
                }
                if let Ok(dyn_img) = raw_img.try_into_dynamic() {
                    let resized =
                        dyn_img.resize_to_fill(tw, th, image::imageops::FilterType::Triangle);
                    let frame = image::Frame::from_parts(
                        resized.to_rgba8(),
                        0,
                        0,
                        image::Delay::from_numer_denom_ms(1000, 1),
                    );
                    let _ = enc.encode_frame(frame);
                }
            }
            drop(enc);
            if cancel_flag.load(std::sync::atomic::Ordering::Acquire) {
                let _ = std::fs::remove_file(&gif_path);
                if let Some(wp) = wav_path {
                    let _ = std::fs::remove_file(wp);
                }
                return;
            }
            let gif_name = gif_path
                .file_name()
                .and_then(|s| s.to_str())
                .unwrap_or("timelapse.gif")
                .to_owned();
            let mut detail = format!("shots/{gif_name}");
            if music
                && let Some(wp) = wav_path
                && write_timelapse_wav_if_enabled(&wp)
                && let Some(wn) = wp.file_name().and_then(|s| s.to_str())
            {
                detail = format!("shots/{gif_name} + shots/{wn}");
            }
            let _ = toast_tx.try_send((title, detail));
        })
        .ok()?;
    Some(tx)
}

fn drive_pending_timelapse_capture(
    mut commands: Commands,
    time: Res<Time>,
    mut ui: ResMut<UiState>,
    mut pending: ResMut<PendingTimelapseCapture>,
) {
    let Some(job) = pending.0.as_mut() else {
        return;
    };

    if job.captured_frames >= job.total_frames {
        // Dropping `frame_tx` closes the bounded channel so the encoder thread flushes GIF + WAV cleanly.
        let _ = pending.0.take();
        ui.timelapse_recording = false;
        ui.timelapse_progress = 1.0;
        ui.timelapse_elapsed_sec = 15;
        return;
    }

    job.elapsed_since_last += time.delta_secs();
    if job.elapsed_since_last < job.frame_interval_sec {
        return;
    }
    let Some(frame_tx) = job.frame_tx.clone() else {
        let _ = pending.0.take();
        ui.timelapse_recording = false;
        return;
    };
    job.elapsed_since_last = 0.0;
    job.captured_frames += 1;
    let frac = (job.captured_frames as f32 / job.total_frames as f32).clamp(0.0, 1.0);
    ui.timelapse_progress = frac;
    ui.timelapse_elapsed_sec = (frac * 15.0).round() as u32;

    commands
        .spawn(Screenshot::primary_window())
        .observe(move |trigger: Trigger<ScreenshotCaptured>| {
            let _ = frame_tx.try_send(trigger.event().0.clone());
        });
}

fn build_wrapped_card_input(
    ui: &UiState,
    i18n: &I18nStore,
) -> station_canvas2d::WrappedCardInput {
    let w = &ui.wrapped;
    let date_range = station_ui::drawers::modals::format_wrapped_date_range(w, i18n);
    let hours_headline =
        station_ui::drawers::modals::format_wrapped_hours_key("wrapped.hours", w.hours_sum, i18n);
    let worked_line =
        station_ui::drawers::modals::format_wrapped_hours_key("wrapped.worked", w.hours_wall, i18n);
    let waited_str =
        station_ui::drawers::modals::format_wrapped_hours_key("wrapped.waited", w.wait_hours, i18n);
    let sess_str = i18n.tr_vars(
        "wrapped.sessions",
        &[
            ("n", &w.sessions.to_string()),
            ("c", &w.sessions.to_string()),
        ],
    );
    let highlights =
        station_ui::drawers::modals::format_wrapped_highlights(w, ui.streamer_mode, i18n);

    station_canvas2d::WrappedCardInput {
        brand_title: "Agents Station".to_owned(),
        date_range,
        heading: i18n.tr("wrapped.title"),
        hours_headline,
        worked_line,
        waited_sessions_line: format!("{waited_str} · {sess_str}"),
        tiles: [
            (w.prompts.to_string(), i18n.tr("wrapped.prompts")),
            (w.edits.to_string(), i18n.tr("wrapped.edits")),
            (w.cmds.to_string(), i18n.tr("wrapped.cmds")),
            (format!("${:.1}", w.cost), i18n.tr("wrapped.cost")),
        ],
        highlights,
        footer: i18n.tr("wrapped.footer"),
        office_shot: None,
    }
}

fn copy_rgba_or_path_to_clipboard(surf: &station_canvas2d::RgbaSurface, path_str: &str) -> bool {
    let Ok(mut cb) = arboard::Clipboard::new() else {
        return false;
    };
    let data = arboard::ImageData {
        width: surf.width as usize,
        height: surf.height as usize,
        bytes: std::borrow::Cow::Borrowed(&surf.rgba),
    };
    if cb.set_image(data).is_ok() {
        return true;
    }
    cb.set_text(path_str.to_owned()).is_ok()
}

fn render_group_chat_photo_data_url(
    ui: &UiState,
    i18n: &I18nStore,
    shared_fonts: Option<&mut station_3d::SharedFontEngine>,
    assets_dir: &std::path::Path,
) -> Option<String> {
    let mut c = station_canvas2d::Canvas2d::new(480, 360);
    c.linear_gradient_rect(
        0.0,
        0.0,
        480.0,
        360.0,
        (0.0, 0.0),
        (480.0, 360.0),
        &[
            (0.0, station_canvas2d::hex("#1C2436")),
            (1.0, station_canvas2d::hex("#0D1219")),
        ],
    );
    let mut fallback = if shared_fonts.is_none() {
        Some(station_canvas2d::FontEngine::from_assets_dir(assets_dir))
    } else {
        None
    };
    let fonts = match shared_fonts {
        Some(sf) => sf.get_or_init(),
        None => fallback.as_mut()?,
    };
    fonts.draw_text(
        &mut c,
        "Agents Station",
        24.0,
        24.0,
        26.0,
        station_canvas2d::hex("#EF8A5F"),
        station_canvas2d::TextAlign::Left,
        Some(430.0),
        true,
        false,
    );
    let live_lbl = i18n.tr("ui.activityPanel.live");
    fonts.draw_text(
        &mut c,
        &format!("{} {} · {}", ui.sessions.len(), live_lbl, ui.clock_label),
        24.0,
        62.0,
        18.0,
        station_canvas2d::hex("#DCE3F0"),
        station_canvas2d::TextAlign::Left,
        Some(430.0),
        false,
        false,
    );
    let redacted = i18n.tr("station.streamerRedacted");
    for (idx, s) in ui.sessions.iter().take(4).enumerate() {
        let y = 112.0 + idx as f32 * 54.0;
        c.round_rect(24.0, y, 432.0, 42.0, 8.0, station_canvas2d::hex("#18202B"));
        let label = if ui.streamer_mode {
            redacted.as_str()
        } else {
            s.project.as_str()
        };
        fonts.draw_text(
            &mut c,
            label,
            38.0,
            y + 11.0,
            16.0,
            station_canvas2d::hex("#E8ECF4"),
            station_canvas2d::TextAlign::Left,
            Some(390.0),
            true,
            false,
        );
    }
    let surf = c.into_surface();
    let rgba = image::RgbaImage::from_raw(surf.width, surf.height, surf.rgba)?;
    let rgb = image::DynamicImage::ImageRgba8(rgba).to_rgb8();
    let mut jpg = Vec::new();
    let mut enc = image::codecs::jpeg::JpegEncoder::new_with_quality(&mut jpg, 80);
    enc.encode_image(&rgb).ok()?;
    let encoded = base64::engine::general_purpose::STANDARD.encode(&jpg);
    Some(format!("data:image/jpeg;base64,{encoded}"))
}

#[derive(SystemParam)]
struct AuxActionParams<'w, 's> {
    sfx: Option<ResMut<'w, station_3d::audio::SfxEngine>>,
    life: Option<ResMut<'w, station_3d::life::OfficeLifeState>>,
    camera_rig: Option<ResMut<'w, station_3d::camera::CameraRig>>,
    cam_q: Query<'w, 's, &'static mut Projection, With<station_3d::camera::MainOfficeCamera>>,
    replay: Option<ResMut<'w, station_3d::replay::ReplayState>>,
    rocket_queue: Option<ResMut<'w, station_3d::RocketLaunchQueue>>,
    pending_photo: Option<ResMut<'w, PendingPhotoCapture>>,
    pending_timelapse: Option<ResMut<'w, PendingTimelapseCapture>>,
    wb_q: Query<
        'w,
        's,
        &'static MeshMaterial3d<StandardMaterial>,
        With<station_3d::furniture::WhiteboardSurface>,
    >,
    primary_win_q: Query<'w, 's, Entity, With<PrimaryWindow>>,
}

fn persist_pref(bridge: &LiveServerBridge, key: &str, value: Value) {
    match &bridge.backend {
        BridgeBackend::InProcess { app_state, .. } => {
            if let Ok(mut prefs) = app_state.prefs.lock() {
                if key == "lang" {
                    if let Some(code) = value.as_str() {
                        let _ = prefs.set_lang(code);
                    }
                } else {
                    let _ = prefs.set(key, Some(value));
                }
            }
        }
        BridgeBackend::AttachedHttp { action_tx, .. } => {
            let mut map = serde_json::Map::new();
            map.insert(key.to_owned(), value);
            let _ = action_tx.try_send(RemoteActionCmd {
                path: "/api/prefs",
                body: Value::Object(map),
                refresh: RefreshKind::None,
                launch_toast_label: None,
            });
        }
        BridgeBackend::BindError { .. } => {
            let mut prefs = station_core::prefs::PrefsStore::load(&bridge.data_dir);
            if key == "lang" {
                if let Some(code) = value.as_str() {
                    let _ = prefs.set_lang(code);
                }
            } else {
                let _ = prefs.set(key, Some(value));
            }
        }
    }
}

fn parse_whiteboard_todos(text: &str) -> Vec<Value> {
    let mut todos = Vec::new();
    for raw in text.lines() {
        let trimmed = raw.trim();
        if trimmed.is_empty() {
            continue;
        }
        let body = trimmed
            .trim_start_matches("- ")
            .trim_start_matches("* ")
            .trim();
        let done = body.starts_with("[x]") || body.starts_with("[X]");
        let clean = body
            .trim_start_matches("[x]")
            .trim_start_matches("[X]")
            .trim_start_matches("[ ]")
            .trim();
        if !clean.is_empty() {
            todos.push(json!({ "text": clean, "done": done }));
        }
    }
    todos
}

fn save_photostrip_png(
    shot_path: &std::path::Path,
    ui: &UiState,
    shared_fonts: Option<&mut station_3d::SharedFontEngine>,
    assets_dir: &std::path::Path,
) -> bool {
    use station_canvas2d::boards_and_social::{PhotostripInput, StripPerson, draw_photostrip};
    use station_canvas2d::{FontEngine, hex};

    if let Some(parent) = shot_path.parent() {
        let _ = std::fs::create_dir_all(parent);
    }
    let people: Vec<StripPerson> = if ui.sessions.is_empty() {
        vec![
            StripPerson {
                hair: hex("#3B2618"),
                skin: hex("#F5D0B5"),
                shirt: hex("#D97757"),
            },
            StripPerson {
                hair: hex("#1E1E24"),
                skin: hex("#EBC3A0"),
                shirt: hex("#5F8FD8"),
            },
        ]
    } else {
        ui.sessions
            .iter()
            .take(4)
            .enumerate()
            .map(|(i, s)| {
                let cfg = station_3d::chibi::ChibiConfig::for_agent(&s.agent, &s.project, i);
                StripPerson {
                    hair: hex(cfg.hair),
                    skin: hex(cfg.skin),
                    shirt: hex(cfg.shirt),
                }
            })
            .collect()
    };
    let input = PhotostripInput {
        tag: format!("Agents Station · {} live", ui.sessions.len()),
        day: Local::now().format("%d/%m").to_string(),
        seed: (now_ms() % 97) as usize,
        people,
    };
    let surf = if let Some(sf) = shared_fonts {
        draw_photostrip(&input, sf.get_or_init())
    } else {
        let mut fonts = FontEngine::from_assets_dir(assets_dir);
        draw_photostrip(&input, &mut fonts)
    };
    image::RgbaImage::from_raw(surf.width, surf.height, surf.rgba)
        .and_then(|img| img.save(shot_path).ok())
        .is_some()
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum PendingWorldOp {
    None,
    UnloadToLite,
    UnloadForTray,
    EnsureMode3d,
    Rebuild3d,
    SwapBackdrop(station_3d::BackdropKind),
}

fn unload_world_if_possible(commands: &mut Commands, wp: &mut WorldActionParams<'_, '_>) {
    if let (Some(meshes), Some(mats), Some(imgs)) = (
        wp.meshes.as_deref_mut(),
        wp.materials.as_deref_mut(),
        wp.images.as_deref_mut(),
    ) {
        station_3d::unload_full_office_world(
            commands,
            wp.world_q.iter(),
            wp.pal.as_deref_mut(),
            wp.backdrop_assets.as_deref_mut(),
            wp.ibl.as_deref(),
            wp.chibi_meshes.as_deref(),
            meshes,
            mats,
            imgs,
        );
    } else {
        for ent in &wp.world_q {
            commands.entity(ent).try_despawn();
        }
    }
}

fn rebuild_3d_office_world(
    commands: &mut Commands,
    wp: &mut WorldActionParams<'_, '_>,
    assets_dir: &std::path::Path,
    ui: &UiState,
    has_world: bool,
) {
    if let (Some(meshes), Some(mats), Some(imgs), Some(shared_fonts)) = (
        wp.meshes.as_deref_mut(),
        wp.materials.as_deref_mut(),
        wp.images.as_deref_mut(),
        wp.shared_fonts.as_deref_mut(),
    ) {
        // Despawn Lite2dCamera only after confirming all resources exist so we never leave 0 cameras.
        for ent in &wp.lite_cam_q {
            commands.entity(ent).try_despawn();
        }
        if has_world {
            station_3d::unload_full_office_world(
                commands,
                wp.world_q.iter(),
                wp.pal.as_deref_mut(),
                wp.backdrop_assets.as_deref_mut(),
                wp.ibl.as_deref(),
                wp.chibi_meshes.as_deref(),
                meshes,
                mats,
                imgs,
            );
        }
        let fonts = shared_fonts.get_or_init();
        station_3d::spawn_full_office_world(
            commands,
            meshes,
            mats,
            imgs,
            fonts,
            assets_dir,
            ui.layout,
            ui.backdrop,
            ui.chibi_look,
            ui.night,
        );
    }
}

fn process_ui_actions(
    mut commands: Commands,
    mut ui: ResMut<UiState>,
    mut i18n: ResMut<I18nStore>,
    mut bridge: ResMut<LiveServerBridge>,
    mut world_params: WorldActionParams,
    mut tray_params: TrayActionParams,
    mut aux: AuxActionParams,
) {
    let has_world = !world_params.world_q.is_empty();
    let has_lite_cam = !world_params.lite_cam_q.is_empty();
    let mut pending_op = PendingWorldOp::None;

    let actions = ui.drain_actions();
    for action in actions {
        match action {
            UiAction::LaunchHarness(id) => {
                let label = ui
                    .harnesses
                    .iter()
                    .find(|h| h.id == id)
                    .map(|h| h.label.clone())
                    .or_else(|| {
                        if let BridgeBackend::AttachedHttp { snapshot, .. } = &bridge.backend {
                            snapshot.lock().ok().and_then(|s| {
                                s.harnesses
                                    .iter()
                                    .find(|h| h.id == id)
                                    .map(|h| h.label.clone())
                            })
                        } else {
                            None
                        }
                    })
                    .unwrap_or_else(|| id.clone());
                match &bridge.backend {
                    BridgeBackend::AttachedHttp { action_tx, .. } => {
                        let _ = action_tx.try_send(RemoteActionCmd {
                            path: "/api/harnesses/launch",
                            body: json!({ "id": id }),
                            refresh: RefreshKind::Harnesses,
                            launch_toast_label: Some(label),
                        });
                    }
                    BridgeBackend::InProcess {
                        harnesses,
                        harness_poller,
                        ..
                    }
                    | BridgeBackend::BindError {
                        harnesses,
                        harness_poller,
                        ..
                    } => {
                        let svc = harnesses.clone();
                        let toast_tx = bridge.toast_tx.clone();
                        let wakeup = harness_poller.wakeup_handle();
                        std::thread::spawn(move || {
                            let notice = match svc.open(&id) {
                                Ok(outcome) => launch_outcome_toast(&label, &outcome),
                                Err(err) => launch_error_toast(&label, &err.to_string()),
                            };
                            let _ = toast_tx.try_send(notice);
                            wakeup.trigger();
                        });
                    }
                }
            }
            UiAction::AddCustomHarness {
                id,
                label,
                path,
                hook_agent,
            } => {
                let payload = json!({
                    "op": "upsert",
                    "id": id,
                    "label": label,
                    "path": path,
                    "hook_agent": hook_agent,
                });
                match &bridge.backend {
                    BridgeBackend::AttachedHttp { action_tx, .. } => {
                        let _ = action_tx.try_send(RemoteActionCmd {
                            path: "/api/harnesses/custom",
                            body: payload,
                            refresh: RefreshKind::Harnesses,
                            launch_toast_label: None,
                        });
                    }
                    BridgeBackend::InProcess {
                        harnesses,
                        harness_poller,
                        ..
                    }
                    | BridgeBackend::BindError {
                        harnesses,
                        harness_poller,
                        ..
                    } => {
                        let _ = harnesses.mutate_custom(payload);
                        harness_poller.trigger_refresh();
                    }
                }
            }
            UiAction::RemoveCustomHarness(id) => {
                let payload = json!({
                    "op": "remove",
                    "id": id,
                });
                match &bridge.backend {
                    BridgeBackend::AttachedHttp { action_tx, .. } => {
                        let _ = action_tx.try_send(RemoteActionCmd {
                            path: "/api/harnesses/custom",
                            body: payload,
                            refresh: RefreshKind::Harnesses,
                            launch_toast_label: None,
                        });
                    }
                    BridgeBackend::InProcess {
                        harnesses,
                        harness_poller,
                        ..
                    }
                    | BridgeBackend::BindError {
                        harnesses,
                        harness_poller,
                        ..
                    } => {
                        let _ = harnesses.mutate_custom(payload);
                        harness_poller.trigger_refresh();
                    }
                }
            }
            UiAction::SetViewMode(mode @ (ViewMode::Lite2D | ViewMode::PanelCompact)) => {
                ui.view_mode = mode;
                pending_op = PendingWorldOp::UnloadToLite;
                for mut win in &mut tray_params.windows {
                    if mode == ViewMode::PanelCompact {
                        win.resolution.set(420.0, 640.0);
                    } else if win.width() < 600.0 {
                        win.resolution.set(1280.0, 720.0);
                    }
                }
            }
            UiAction::SetViewMode(ViewMode::Mode3D) => {
                ui.view_mode = ViewMode::Mode3D;
                for mut win in &mut tray_params.windows {
                    if win.width() < 600.0 {
                        win.resolution.set(1280.0, 720.0);
                    }
                }
                pending_op = match pending_op {
                    PendingWorldOp::Rebuild3d => PendingWorldOp::Rebuild3d,
                    PendingWorldOp::SwapBackdrop(bk) if has_world => {
                        PendingWorldOp::SwapBackdrop(bk)
                    }
                    _ => PendingWorldOp::EnsureMode3d,
                };
            }
            UiAction::SwitchLayout(lay) => {
                ui.layout = lay;
                ui.view_mode = ViewMode::Mode3D;
                pending_op = PendingWorldOp::Rebuild3d;
            }
            UiAction::SetChibiLook(look) => {
                ui.chibi_look = look;
                if ui.view_mode == ViewMode::Mode3D {
                    pending_op = PendingWorldOp::Rebuild3d;
                }
            }
            UiAction::SwitchBackdrop(bk) => {
                ui.backdrop = bk;
                pending_op = match pending_op {
                    PendingWorldOp::UnloadToLite => PendingWorldOp::UnloadToLite,
                    PendingWorldOp::UnloadForTray => PendingWorldOp::UnloadForTray,
                    PendingWorldOp::Rebuild3d => PendingWorldOp::Rebuild3d,
                    PendingWorldOp::EnsureMode3d if !has_world => PendingWorldOp::EnsureMode3d,
                    _ if ui.view_mode == ViewMode::Mode3D && has_world => {
                        PendingWorldOp::SwapBackdrop(bk)
                    }
                    other => other,
                };
            }
            UiAction::ToggleNight(night) => {
                ui.night = night;
                if let Some(l) = world_params.lighting.as_mut() {
                    l.night = night;
                    l.photo_hour = None;
                    l.room_lights_off = false;
                }
            }
            UiAction::SetLang(code) => {
                i18n.set_lang(&code);
                persist_pref(&bridge, "lang", json!(code));
            }
            UiAction::DecidePermission { id, decision } => {
                let Some(parsed) = parse_ui_perm_decision(&decision) else {
                    continue;
                };
                match &bridge.backend {
                    BridgeBackend::AttachedHttp { action_tx, .. } => {
                        let _ = action_tx.try_send(RemoteActionCmd {
                            path: "/api/perm",
                            body: json!({ "id": id, "choice": decision, "decision": decision }),
                            refresh: RefreshKind::Live,
                            launch_toast_label: None,
                        });
                    }
                    BridgeBackend::InProcess { app_state, .. } => {
                        if let Ok(mut office) = app_state.office.lock() {
                            office.resolve_hold(&id, parsed);
                        }
                    }
                    BridgeBackend::BindError { .. } => {
                        ui.push_toast(i18n.tr("station.approval"), i18n.tr("perm.unreachable"));
                        continue;
                    }
                }
                if let Some(sfx) = aux.sfx.as_deref_mut() {
                    sfx.play_sfx(SfxKind::Click);
                }
                let done_key = match decision.as_str() {
                    "allow" => "perm.done.allow",
                    "always" => "perm.done.always",
                    "deny" => "perm.done.deny",
                    _ => "perm.sent",
                };
                ui.push_toast(i18n.tr("station.approval"), i18n.tr(done_key));
            }
            UiAction::SubmitAskAnswers { id, answers } => {
                match &bridge.backend {
                    BridgeBackend::AttachedHttp { action_tx, .. } => {
                        let _ = action_tx.try_send(RemoteActionCmd {
                            path: "/api/perm",
                            body: json!({
                                "id": id,
                                "choice": "answer",
                                "answers": Value::Object(answers),
                            }),
                            refresh: RefreshKind::Live,
                            launch_toast_label: None,
                        });
                    }
                    BridgeBackend::InProcess { app_state, .. } => {
                        if let Ok(mut office) = app_state.office.lock() {
                            office.resolve_hold(&id, PermDecision::Answers(answers));
                        }
                    }
                    BridgeBackend::BindError { .. } => {
                        ui.push_toast(i18n.tr("station.approval"), i18n.tr("perm.unreachable"));
                        continue;
                    }
                }
                if let Some(sfx) = aux.sfx.as_deref_mut() {
                    sfx.play_sfx(SfxKind::Click);
                }
                ui.push_toast(i18n.tr("station.approval"), i18n.tr("perm.done.answer"));
            }
            UiAction::DecidePlan {
                id,
                approve,
                feedback,
            } => {
                let choice = if approve { "approve" } else { "revise" };
                match &bridge.backend {
                    BridgeBackend::AttachedHttp { action_tx, .. } => {
                        let _ = action_tx.try_send(RemoteActionCmd {
                            path: "/api/perm",
                            body: json!({
                                "id": id,
                                "choice": choice,
                                "note": feedback,
                            }),
                            refresh: RefreshKind::Live,
                            launch_toast_label: None,
                        });
                    }
                    BridgeBackend::InProcess { app_state, .. } => {
                        if let Ok(mut office) = app_state.office.lock() {
                            let dec = if approve {
                                PermDecision::Allow
                            } else {
                                PermDecision::Deny { reason: feedback }
                            };
                            office.resolve_hold(&id, dec);
                        }
                    }
                    BridgeBackend::BindError { .. } => {
                        ui.push_toast(i18n.tr("station.approval"), i18n.tr("perm.unreachable"));
                        continue;
                    }
                }
                if let Some(sfx) = aux.sfx.as_deref_mut() {
                    sfx.play_sfx(SfxKind::Click);
                }
                let done_key = if approve {
                    "perm.done.approve"
                } else {
                    "perm.done.revise"
                };
                ui.push_toast(i18n.tr("station.approval"), i18n.tr(done_key));
            }
            UiAction::SkipHold(id) => {
                match &bridge.backend {
                    BridgeBackend::AttachedHttp { action_tx, .. } => {
                        let _ = action_tx.try_send(RemoteActionCmd {
                            path: "/api/perm",
                            body: json!({ "id": id, "choice": "skip" }),
                            refresh: RefreshKind::Live,
                            launch_toast_label: None,
                        });
                    }
                    BridgeBackend::InProcess { app_state, .. } => {
                        if let Ok(mut office) = app_state.office.lock() {
                            office.cancel_hold(&id);
                        }
                    }
                    BridgeBackend::BindError { .. } => {
                        ui.push_toast(i18n.tr("station.approval"), i18n.tr("perm.unreachable"));
                        continue;
                    }
                }
                ui.push_toast(i18n.tr("station.approval"), i18n.tr("perm.done.skip"));
            }
            UiAction::DecideDenied { id, retry } => {
                let op = if retry { "retry" } else { "keep" };
                match &bridge.backend {
                    BridgeBackend::AttachedHttp { action_tx, .. } => {
                        let _ = action_tx.try_send(RemoteActionCmd {
                            path: "/api/denied",
                            body: json!({ "id": id, "op": op }),
                            refresh: RefreshKind::Live,
                            launch_toast_label: None,
                        });
                    }
                    BridgeBackend::InProcess { app_state, .. } => {
                        if let Ok(mut office) = app_state.office.lock() {
                            let dec = if retry {
                                PermDecision::Retry
                            } else {
                                PermDecision::Keep
                            };
                            office.resolve_hold(&id, dec);
                        }
                    }
                    BridgeBackend::BindError { .. } => {
                        ui.push_toast(i18n.tr("chat.deny.title"), i18n.tr("perm.unreachable"));
                        continue;
                    }
                }
                if let Some(sfx) = aux.sfx.as_deref_mut() {
                    sfx.play_sfx(SfxKind::Click);
                }
                let msg = if retry {
                    i18n.tr("chat.deny.retried")
                } else {
                    i18n.tr("chat.deny.kept")
                };
                ui.push_toast(i18n.tr("chat.deny.title"), msg);
            }
            UiAction::LoopStop { session_id } => {
                match &bridge.backend {
                    BridgeBackend::AttachedHttp { action_tx, .. } => {
                        let _ = action_tx.try_send(RemoteActionCmd {
                            path: "/api/loop",
                            body: json!({ "stop": session_id }),
                            refresh: RefreshKind::Live,
                            launch_toast_label: None,
                        });
                    }
                    BridgeBackend::InProcess { app_state, .. } => {
                        if let Ok(mut office) = app_state.office.lock() {
                            office.loops.stop(&session_id, now_ms());
                        }
                    }
                    BridgeBackend::BindError { .. } => {
                        ui.push_toast(i18n.tr("chat.loop.title"), i18n.tr("drawer.unreachable"));
                        continue;
                    }
                }
                ui.push_toast(i18n.tr("chat.loop.title"), i18n.tr("chat.loop.stopping"));
            }
            UiAction::LoopHint { session_id, hint } => {
                let clean = station_core::redact::redact(hint.trim());
                if clean.is_empty() {
                    continue;
                }
                match &bridge.backend {
                    BridgeBackend::AttachedHttp { action_tx, .. } => {
                        let _ = action_tx.try_send(RemoteActionCmd {
                            path: "/api/loop",
                            body: json!({ "sid": session_id, "hint": clean }),
                            refresh: RefreshKind::Live,
                            launch_toast_label: None,
                        });
                    }
                    BridgeBackend::InProcess { app_state, .. } => {
                        if let Ok(mut office) = app_state.office.lock() {
                            office.loops.hint(&session_id, &clean);
                        }
                    }
                    BridgeBackend::BindError { .. } => {
                        ui.push_toast(i18n.tr("chat.loop.title"), i18n.tr("drawer.unreachable"));
                        continue;
                    }
                }
                ui.push_toast(i18n.tr("chat.loop.title"), i18n.tr("chat.loop.hintSent"));
            }
            UiAction::CiFix { id } => {
                match &bridge.backend {
                    BridgeBackend::AttachedHttp { action_tx, .. } => {
                        let _ = action_tx.try_send(RemoteActionCmd {
                            path: "/api/ci",
                            body: json!({ "id": id }),
                            refresh: RefreshKind::Chat,
                            launch_toast_label: None,
                        });
                    }
                    BridgeBackend::InProcess {
                        app_state,
                        runtime_handle,
                        ..
                    } => {
                        let ci_info = app_state.office.lock().ok().and_then(|office| {
                            let ci = office
                                .chat
                                .by_id(&id)
                                .filter(|m| m.kind == "ci")
                                .and_then(|m| m.ci.clone())?;
                            let wf = ci
                                .get("wf")
                                .and_then(Value::as_str)
                                .unwrap_or("CI")
                                .to_owned();
                            let branch = ci
                                .get("branch")
                                .and_then(Value::as_str)
                                .unwrap_or("")
                                .to_owned();
                            let url = ci
                                .get("url")
                                .and_then(Value::as_str)
                                .unwrap_or("")
                                .to_owned();
                            let sid = ci
                                .get("sid")
                                .or_else(|| ci.get("sess"))
                                .and_then(Value::as_str)
                                .unwrap_or("")
                                .to_owned();
                            let dir = ci
                                .get("dir")
                                .and_then(Value::as_str)
                                .unwrap_or("")
                                .to_owned();
                            let busy = !sid.is_empty()
                                && office.sessions.get(&sid).is_some_and(|s| s.status != "rest");
                            Some((wf, branch, url, sid, dir, busy))
                        });
                        if let Some((wf, branch, url, sid, dir, busy)) = ci_info {
                            let prompt = format!("Fix failing {wf} on branch {branch} ({url})");
                            let ts = now_ms();
                            if busy || runtime_handle.is_none() || dir.is_empty() {
                                if let Ok(mut q) = app_state.steer_queue.lock() {
                                    q.enqueue(
                                        station_core::state::steer::QueuedSteer {
                                            key: if sid.is_empty() { id.clone() } else { sid.clone() },
                                            sid: (!sid.is_empty()).then_some(sid),
                                            dir: (!dir.is_empty()).then_some(dir),
                                            agent: "claude".to_owned(),
                                            text: prompt,
                                            name: wf,
                                            at: ts,
                                        },
                                        ts,
                                    );
                                }
                                if let Ok(mut office) = app_state.office.lock() {
                                    office.chat.set_ci(&id, json!({ "fix": "note" }));
                                    let _ = office.save_chat();
                                }
                            } else if let Some(rt) = runtime_handle {
                                let tasks = app_state.tasks.clone();
                                let req = station_core::state::tasks::TaskStartRequest {
                                    dir,
                                    text: prompt,
                                    agent: "claude".to_owned(),
                                    model: String::new(),
                                    fork: false,
                                };
                                rt.spawn(async move {
                                    let _ = station_core::state::tasks::start_and_spawn(
                                        tasks, req, ts, None,
                                    );
                                });
                                if let Ok(mut office) = app_state.office.lock() {
                                    office.chat.set_ci(&id, json!({ "fix": "sent" }));
                                    let _ = office.save_chat();
                                }
                            }
                        }
                    }
                    BridgeBackend::BindError { .. } => {
                        ui.push_toast(i18n.tr("chat.ci.title"), i18n.tr("drawer.unreachable"));
                        continue;
                    }
                }
                ui.push_toast(i18n.tr("chat.ci.title"), i18n.tr("chat.ci.sent"));
            }
            UiAction::CiDismiss { id } => {
                bridge.dismissed_ci_ids.insert(id.clone());
                if let BridgeBackend::InProcess { app_state, .. } = &bridge.backend
                    && let Ok(mut office) = app_state.office.lock()
                {
                    office.chat.set_ci(&id, json!({ "state": "over" }));
                    let _ = office.save_chat();
                }
            }
            UiAction::FetchSessionLog(session_id) => match &bridge.backend {
                BridgeBackend::AttachedHttp { query_tx, .. } => {
                    let _ = query_tx.try_send(RemoteQueryCmd::FetchLog { session_id });
                }
                BridgeBackend::InProcess { app_state, .. } => {
                    let (tp, feed_fallback) = {
                        let office = app_state.office.lock().unwrap_or_else(|e| e.into_inner());
                        let tp = office
                            .sessions
                            .get(&session_id)
                            .and_then(|s| s.transcript_path.clone());
                        let fb: Vec<SessionLogItem> = office
                            .feed
                            .iter()
                            .filter(|f| f.sess == session_id)
                            .map(|f| SessionLogItem {
                                role: if f.kind == "prompt" { "user" } else { "tool" }.to_owned(),
                                text: f.label.clone(),
                                ts: format_hhmm_ms(f.ts),
                                tool_type: Some(f.kind.clone()),
                            })
                            .collect();
                        (tp, fb)
                    };
                    if let Some(path) = tp {
                        let async_tx = bridge.async_tx.clone();
                        let sid = session_id.clone();
                        std::thread::spawn(move || {
                            let hist =
                                station_core::store::codex_history::read_codex_history(&path, 120);
                            let items: Vec<SessionLogItem> = if hist.is_empty() {
                                feed_fallback
                            } else {
                                hist.into_iter()
                                    .map(|m| SessionLogItem {
                                        role: m.role,
                                        text: m.text,
                                        ts: String::new(),
                                        tool_type: None,
                                    })
                                    .collect()
                            };
                            let _ = async_tx.try_send(BridgeAsyncResult::SessionLog {
                                session_id: sid,
                                items,
                            });
                        });
                    } else if !feed_fallback.is_empty() {
                        ui.session_logs.insert(session_id, feed_fallback);
                    }
                }
                BridgeBackend::BindError { .. } => {
                    ui.push_toast(
                        i18n.tr("ui.sessionDrawer.chat"),
                        i18n.tr("drawer.chatUnreadable"),
                    );
                }
            },
            UiAction::AddSessionPin { session_id, text } => match &bridge.backend {
                BridgeBackend::AttachedHttp { action_tx, .. } => {
                    let _ = action_tx.try_send(RemoteActionCmd {
                        path: "/api/pins",
                        body: json!({ "id": session_id, "op": "add", "text": text }),
                        refresh: RefreshKind::Live,
                        launch_toast_label: None,
                    });
                }
                BridgeBackend::InProcess { app_state, .. } => {
                    if let Ok(mut office) = app_state.office.lock() {
                        let _ = office.add_pin(&session_id, &text);
                        ui.session_pins = office.pins.clone();
                    }
                }
                BridgeBackend::BindError { .. } => {
                    ui.push_toast(i18n.tr("drawer.pins.tab"), i18n.tr("drawer.unreachable"));
                }
            },
            UiAction::RemoveSessionPin { session_id, index } => match &bridge.backend {
                BridgeBackend::AttachedHttp { action_tx, .. } => {
                    let _ = action_tx.try_send(RemoteActionCmd {
                        path: "/api/pins",
                        body: json!({ "id": session_id, "op": "del", "pin": index }),
                        refresh: RefreshKind::Live,
                        launch_toast_label: None,
                    });
                }
                BridgeBackend::InProcess { app_state, .. } => {
                    if let Ok(mut office) = app_state.office.lock() {
                        let _ = office.remove_pin(&session_id, index);
                        ui.session_pins = office.pins.clone();
                    }
                }
                BridgeBackend::BindError { .. } => {
                    ui.push_toast(i18n.tr("drawer.pins.tab"), i18n.tr("drawer.unreachable"));
                }
            },
            UiAction::FetchSessionUndo(session_id) => {
                ui.session_undo = Some(SessionUndoView {
                    session_id: session_id.clone(),
                    loading: true,
                    ..Default::default()
                });
                match &bridge.backend {
                    BridgeBackend::AttachedHttp { query_tx, .. } => {
                        let _ = query_tx.try_send(RemoteQueryCmd::FetchUndo { session_id });
                    }
                    BridgeBackend::InProcess { app_state, .. } => {
                        if let Ok(office) = app_state.office.lock() {
                            let plan = office.undo.plan(&session_id);
                            ui.session_undo = Some(undo_view_from_plan(&session_id, plan));
                        }
                    }
                    BridgeBackend::BindError { .. } => {
                        ui.session_undo = Some(SessionUndoView {
                            session_id,
                            loading: false,
                            error: Some(i18n.tr("drawer.unreachable")),
                            ..Default::default()
                        });
                    }
                }
            }
            UiAction::ApplySessionUndo { session_id, files } => match &bridge.backend {
                BridgeBackend::AttachedHttp { query_tx, .. } => {
                    let _ = query_tx.try_send(RemoteQueryCmd::ApplyUndo { session_id, files });
                }
                BridgeBackend::InProcess { app_state, .. } => {
                    if let Ok(mut office) = app_state.office.lock() {
                        if office
                            .sessions
                            .get(&session_id)
                            .is_some_and(|s| s.status != "rest")
                        {
                            ui.session_undo = Some(SessionUndoView {
                                session_id,
                                loading: false,
                                error: Some("busy".to_owned()),
                                ..Default::default()
                            });
                            continue;
                        }
                        let res =
                            office
                                .undo
                                .apply_filtered(&session_id, now_ms(), Some(&files));
                        ui.session_undo = Some(SessionUndoView {
                            session_id,
                            loading: false,
                            done_files: res
                                .done
                                .into_iter()
                                .map(|s| (s.file, s.action))
                                .collect(),
                            backup_dir: res.after,
                            ..Default::default()
                        });
                    }
                }
                BridgeBackend::BindError { .. } => {
                    ui.push_toast(i18n.tr("drawer.undo.btn"), i18n.tr("drawer.unreachable"));
                }
            },
            UiAction::TellClaudeUndo { session_id, files } => {
                let joined = files.join(", ");
                match &bridge.backend {
                    BridgeBackend::AttachedHttp { action_tx, .. } => {
                        let _ = action_tx.try_send(RemoteActionCmd {
                            path: "/api/undo",
                            body: json!({ "id": session_id, "op": "tell", "files": joined }),
                            refresh: RefreshKind::Live,
                            launch_toast_label: None,
                        });
                    }
                    BridgeBackend::InProcess {
                        app_state,
                        runtime_handle,
                        ..
                    } => {
                        let ts = now_ms();
                        let note = format!("Heads up: reverted files from your last turn: {joined}");
                        let (is_busy, cwd, agent, name) = app_state
                            .office
                            .lock()
                            .ok()
                            .and_then(|o| {
                                o.sessions.get(&session_id).map(|s| {
                                    (
                                        s.status != "rest",
                                        s.cwd.clone(),
                                        s.agent.clone(),
                                        s.name.clone(),
                                    )
                                })
                            })
                            .unwrap_or_else(|| {
                                (false, String::new(), "claude".to_owned(), session_id.clone())
                            });
                        if !is_busy
                            && !cwd.is_empty()
                            && let Some(rt) = runtime_handle
                        {
                            let tasks = app_state.tasks.clone();
                            let req = station_core::state::tasks::TaskStartRequest {
                                dir: cwd,
                                text: note,
                                agent: if agent.is_empty() {
                                    "claude".to_owned()
                                } else {
                                    agent
                                },
                                model: String::new(),
                                fork: false,
                            };
                            rt.spawn(async move {
                                let _ = station_core::state::tasks::start_and_spawn(
                                    tasks, req, ts, None,
                                );
                            });
                        } else if let Ok(mut q) = app_state.steer_queue.lock() {
                            q.enqueue(
                                station_core::state::steer::QueuedSteer {
                                    key: session_id.clone(),
                                    sid: Some(session_id),
                                    dir: (!cwd.is_empty()).then_some(cwd),
                                    agent,
                                    text: note,
                                    name,
                                    at: ts,
                                },
                                ts,
                            );
                        }
                    }
                    BridgeBackend::BindError { .. } => {
                        ui.push_toast(i18n.tr("drawer.undo.btn"), i18n.tr("drawer.unreachable"));
                        continue;
                    }
                }
                ui.push_toast(i18n.tr("drawer.undo.btn"), i18n.tr("drawer.undo.told"));
            }
            UiAction::RewritePrompt { text, lang } => match &bridge.backend {
                BridgeBackend::AttachedHttp { query_tx, .. } => {
                    let _ = query_tx.try_send(RemoteQueryCmd::RewritePrompt { text, lang });
                }
                BridgeBackend::InProcess { app_state, .. } => {
                    let rewriter = app_state.rewriter.clone();
                    let work_dir = app_state.data_dir.join("rewrite");
                    let async_tx = bridge.async_tx.clone();
                    std::thread::spawn(move || {
                        let res = station_core::store::rewrite::Rewriter::execute_blocking(
                            rewriter,
                            work_dir,
                            text,
                            lang,
                            0,
                            now_ms(),
                        );
                        let out = if res.ok {
                            Ok(res.text.unwrap_or_default())
                        } else {
                            Err(res.code.unwrap_or("rewrite failed").to_owned())
                        };
                        let _ = async_tx.try_send(BridgeAsyncResult::RewriteResult(out));
                    });
                }
                BridgeBackend::BindError { .. } => {
                    ui.compose_rewriting = false;
                    ui.push_toast(i18n.tr("chat.coach.label"), i18n.tr("drawer.unreachable"));
                }
            },
            UiAction::SteerSession { session_id, text } => {
                let clean: String = station_core::redact::redact(text.trim())
                    .chars()
                    .take(2000)
                    .collect();
                if clean.is_empty() {
                    continue;
                }
                match &bridge.backend {
                    BridgeBackend::AttachedHttp { action_tx, .. } => {
                        let _ = action_tx.try_send(RemoteActionCmd {
                            path: "/api/tasks/steer",
                            body: json!({ "sid": session_id, "text": clean }),
                            refresh: RefreshKind::Live,
                            launch_toast_label: None,
                        });
                    }
                    BridgeBackend::InProcess {
                        app_state,
                        runtime_handle,
                        ..
                    } => {
                        let ts = now_ms();
                        let (cwd, agent, name, inbox) = app_state
                            .office
                            .lock()
                            .ok()
                            .and_then(|o| {
                                o.sessions.get(&session_id).map(|s| {
                                    (
                                        s.cwd.clone(),
                                        s.agent.clone(),
                                        s.name.clone(),
                                        s.inbox.clone(),
                                    )
                                })
                            })
                            .unwrap_or_else(|| {
                                (
                                    String::new(),
                                    "claude".to_owned(),
                                    session_id.clone(),
                                    None,
                                )
                            });
                        let can_deliver_now = if agent == "claude"
                            && let Some(ib) = inbox.as_deref()
                        {
                            let inbound = station_core::state::steer::inbound_from(
                                Some(app_state.claude_dir.as_path()),
                                (!cwd.is_empty()).then(|| std::path::Path::new(cwd.as_str())),
                            );
                            let sock_ok = station_core::state::steer::is_socket_path(ib);
                            station_core::state::steer::steerable(Some(ib), "", inbound, sock_ok).ok
                        } else {
                            false
                        };
                        let queued = station_core::state::steer::QueuedSteer {
                            key: session_id.clone(),
                            sid: Some(session_id),
                            dir: (!cwd.is_empty()).then_some(cwd),
                            agent,
                            text: clean.clone(),
                            name: name.clone(),
                            at: ts,
                        };
                        if can_deliver_now
                            && let (Some(ib), Some(rt)) = (inbox, runtime_handle)
                        {
                            let state_arc = Arc::clone(app_state);
                            rt.spawn(async move {
                                if !station_core::state::steer::deliver(&ib, &clean, 3000).await
                                    && let Ok(mut q) = state_arc.steer_queue.lock()
                                {
                                    q.enqueue(queued, ts);
                                }
                            });
                        } else if let Ok(mut q) = app_state.steer_queue.lock() {
                            q.enqueue(queued, ts);
                        }
                        ui.push_toast(
                            i18n.tr("chat.steer.btn"),
                            i18n.tr_vars("chat.steer.sent", &[("name", &name)]),
                        );
                    }
                    BridgeBackend::BindError { .. } => {
                        ui.push_toast(i18n.tr("chat.steer.btn"), i18n.tr("drawer.unreachable"));
                        continue;
                    }
                }
                if let Some(sfx) = aux.sfx.as_deref_mut() {
                    sfx.play_sfx(SfxKind::Pop);
                }
            }
            UiAction::StopSessionTask { session_id } => {
                match &bridge.backend {
                    BridgeBackend::AttachedHttp { action_tx, .. } => {
                        let _ = action_tx.try_send(RemoteActionCmd {
                            path: "/api/tasks/stop",
                            body: json!({ "id": session_id }),
                            refresh: RefreshKind::Live,
                            launch_toast_label: None,
                        });
                        let _ = action_tx.try_send(RemoteActionCmd {
                            path: "/api/loop",
                            body: json!({ "stop": session_id }),
                            refresh: RefreshKind::Live,
                            launch_toast_label: None,
                        });
                    }
                    BridgeBackend::InProcess { app_state, .. } => {
                        if let Ok(mut tasks) = app_state.tasks.lock() {
                            let tid = tasks
                                .of_sid(&session_id)
                                .map(|r| r.id.clone())
                                .unwrap_or_else(|| session_id.clone());
                            tasks.stop(&tid);
                        }
                        if let Ok(mut office) = app_state.office.lock() {
                            office.loops.stop(&session_id, now_ms());
                        }
                    }
                    BridgeBackend::BindError { .. } => {
                        ui.push_toast(i18n.tr("chat.loop.stop"), i18n.tr("drawer.unreachable"));
                        continue;
                    }
                }
                ui.push_toast(i18n.tr("chat.loop.stop"), i18n.tr("chat.task.stopped"));
            }
            UiAction::OpenSessionWindow(session_id) => match &bridge.backend {
                BridgeBackend::AttachedHttp { action_tx, .. } => {
                    let _ = action_tx.try_send(RemoteActionCmd {
                        path: "/api/open",
                        body: json!({ "id": session_id }),
                        refresh: RefreshKind::None,
                        launch_toast_label: None,
                    });
                }
                BridgeBackend::InProcess {
                    app_state,
                    harnesses,
                    ..
                } => {
                    let (cwd, agent) = app_state
                        .office
                        .lock()
                        .ok()
                        .and_then(|o| {
                            o.sessions
                                .get(&session_id)
                                .map(|s| (s.cwd.clone(), s.agent.clone()))
                        })
                        .or_else(|| {
                            ui.sessions
                                .iter()
                                .find(|s| s.id == session_id)
                                .map(|s| (s.dir.clone(), s.agent.clone()))
                        })
                        .unwrap_or_default();
                    let toast_tx = bridge.toast_tx.clone();
                    let svc = harnesses.clone();
                    let title = i18n.tr("drawer.openWindowIde");
                    std::thread::spawn(move || {
                        let msg = if !cwd.is_empty() && std::path::Path::new(&cwd).exists() {
                            match station_core::state::stranded::open_folder(std::path::Path::new(
                                &cwd,
                            )) {
                                Ok(()) => cwd,
                                Err(e) => e,
                            }
                        } else if !agent.is_empty() {
                            match svc.open(&agent) {
                                Ok(o) => launch_outcome_toast(&agent, &o).1,
                                Err(e) => e.to_string(),
                            }
                        } else {
                            "no folder".to_owned()
                        };
                        let _ = toast_tx.try_send((title, msg));
                    });
                }
                BridgeBackend::BindError { .. } => {
                    ui.push_toast(i18n.tr("drawer.openWindowIde"), i18n.tr("drawer.unreachable"));
                }
            },
            UiAction::SwitchAccount { session_id, target } => match &bridge.backend {
                BridgeBackend::AttachedHttp { action_tx, .. } => {
                    let _ = action_tx.try_send(RemoteActionCmd {
                        path: "/api/accounts/use",
                        body: json!({ "name": target, "id": session_id }),
                        refresh: RefreshKind::All,
                        launch_toast_label: None,
                    });
                }
                BridgeBackend::InProcess { app_state, .. } => {
                    if let Ok(mut accts) = app_state.claude_accounts.lock() {
                        let res = accts.mutate("use", &json!({ "name": target }));
                        ui.accounts = parse_accounts_from_view(&accts.view());
                        let ok = res.get("ok").and_then(Value::as_bool).unwrap_or(false);
                        let msg = if ok {
                            i18n.tr_vars(
                                "drawer.acct.moved",
                                &[("to", if target.is_empty() { "Main" } else { &target })],
                            )
                        } else {
                            i18n.tr("drawer.acct.failed")
                        };
                        ui.push_toast(i18n.tr("drawer.acct.label"), msg);
                    }
                }
                BridgeBackend::BindError { .. } => {
                    ui.push_toast(i18n.tr("drawer.acct.label"), i18n.tr("drawer.unreachable"));
                }
            },
            UiAction::MinimizeToTray => {
                let is_worker = tray_params.worker_mode.as_ref().is_some_and(|m| m.0);
                bridge.notify_presence_unfocused();
                let _ = notify_parent_in_tray(false);
                if is_worker {
                    // Exit only the heavy Bevy `--gui-worker` so resident RAM drops to the
                    // lightweight parent TrayDaemon + :4310 process (<= 45 MB).
                    tray_params.app_exit.write(AppExit::Success);
                } else {
                    pending_op = PendingWorldOp::UnloadForTray;
                    for mut win in &mut tray_params.windows {
                        win.visible = false;
                    }
                    if let Some(f) = tray_params.fps.as_mut() {
                        f.hidden_to_tray = true;
                    }
                    if let Some(t) = tray_params.tray.as_mut() {
                        t.in_tray = true;
                        t.notifications_sent = t.notifications_sent.saturating_add(1);
                    }
                }
            }
            UiAction::SetWeatherCity(city) => {
                let (code, display) = station_3d::life::weather_city_display(&city);
                ui.weather_city = display.clone();
                if let Some(life) = aux.life.as_deref_mut() {
                    life.weather_city = code.clone();
                }
                persist_pref(&bridge, "weatherCity", json!(code));
                ui.push_toast(
                    i18n.tr("ui.topBar.weather"),
                    i18n.tr_vars("station.weatherSet", &[("city", &display)]),
                );
            }
            UiAction::SetSound(enabled) => {
                ui.sound_on = enabled;
                if let Some(sfx) = aux.sfx.as_deref_mut() {
                    sfx.enabled = enabled;
                    if enabled {
                        sfx.play_sfx(SfxKind::Click);
                    }
                }
                persist_pref(&bridge, "soundOn", json!(enabled));
            }
            UiAction::SetGpuPref(pref) => {
                ui.gpu_pref = pref;
                persist_pref(&bridge, "gpuPref", json!(pref.as_pref_str()));
                ui.push_toast(
                    "GPU Preference",
                    format!("Saved {} (applies on next launch)", pref.label()),
                );
            }
            UiAction::SelectCameraPreset(preset) => {
                ui.camera_preset = preset;
                ui.first_person = false;
                if let Some(rig) = aux.camera_rig.as_deref_mut() {
                    rig.apply_preset(preset, ui.layout);
                    ui.camera_zoom_pct = rig.zoom_pct();
                    let next_proj = rig.projection();
                    for mut proj in &mut aux.cam_q {
                        *proj = next_proj.clone();
                    }
                }
                if let Some(sfx) = aux.sfx.as_deref_mut() {
                    sfx.play_sfx(SfxKind::Click);
                }
            }
            UiAction::ZoomCamera(factor) => {
                if let Some(rig) = aux.camera_rig.as_deref_mut() {
                    rig.zoom_by(factor);
                    ui.camera_zoom_pct = rig.zoom_pct();
                    let next_proj = rig.projection();
                    for mut proj in &mut aux.cam_q {
                        *proj = next_proj.clone();
                    }
                }
            }
            UiAction::ResetCameraZoom => {
                ui.camera_zoom_pct = 100;
                if let Some(rig) = aux.camera_rig.as_deref_mut() {
                    rig.reset_zoom();
                    ui.camera_zoom_pct = rig.zoom_pct();
                    let next_proj = rig.projection();
                    for mut proj in &mut aux.cam_q {
                        *proj = next_proj.clone();
                    }
                }
            }
            UiAction::ToggleFullscreenStage => {
                if ui.fullscreen_stage {
                    ui.photo_mode = false;
                }
                if let Some(sfx) = aux.sfx.as_deref_mut() {
                    sfx.play_sfx(SfxKind::Click);
                }
            }
            UiAction::SetPhotoMode(enabled) => {
                ui.photo_mode = enabled;
                if !enabled {
                    ui.photo_capture_hide_frames = 0;
                    if let Some(pending) = aux.pending_photo.as_deref_mut() {
                        pending.0 = None;
                    }
                } else {
                    ui.active_place_card = None;
                    if let Some(rig) = aux.camera_rig.as_deref_mut()
                        && rig.first_person
                    {
                        rig.first_person = false;
                        ui.first_person = false;
                        let next_proj = rig.projection();
                        for mut proj in &mut aux.cam_q {
                            *proj = next_proj.clone();
                        }
                    }
                }
                if let Some(sfx) = aux.sfx.as_deref_mut() {
                    sfx.play_sfx(SfxKind::Click);
                }
            }
            UiAction::SetPhotoTimeAuto => {
                let night = is_night_for_hour(Local::now().hour());
                ui.night = night;
                if let Some(l) = world_params.lighting.as_mut() {
                    l.night = night;
                    l.photo_hour = None;
                }
            }
            UiAction::CapturePhoto {
                size,
                show_names,
                show_mark,
            } => {
                ui.photo_size = size;
                ui.photo_show_names = show_names;
                ui.photo_show_mark = show_mark;
                let shots_dir = bridge.data_dir.join("shots");
                let _ = std::fs::create_dir_all(&shots_dir);
                if let Some(sfx) = aux.sfx.as_deref_mut() {
                    sfx.play_sfx(SfxKind::Shutter);
                }
                if let Some(primary_ent) = aux.primary_win_q.iter().next()
                    && let Some(pending) = aux.pending_photo.as_deref_mut()
                {
                    let (win_w, win_h) = tray_params
                        .windows
                        .get(primary_ent)
                        .map(|w| (w.width().max(1.0), w.height().max(1.0)))
                        .unwrap_or((1280.0, 720.0));
                    let tags: Vec<station_canvas2d::PhotoNameTag> = ui
                        .projected_labels
                        .iter()
                        .filter(|l| l.kind == ProjectedLabelKind::WorkerTag && !l.behind_camera)
                        .map(|l| station_canvas2d::PhotoNameTag {
                            x_norm: (l.screen_pos.x / win_w).clamp(0.05, 0.95),
                            y_norm: (l.screen_pos.y / win_h).clamp(0.05, 0.95),
                            text: l.text.clone(),
                            dot_rgba: Some([l.accent.r(), l.accent.g(), l.accent.b(), 255]),
                        })
                        .collect();
                    // Hide PhotoBar HUD for the capture frame, then screenshot + scale + paint overlays off-thread.
                    ui.photo_capture_hide_frames = 2;
                    pending.0 = Some(PendingPhotoJob {
                        size,
                        shots_dir,
                        assets_dir: bridge.assets_dir.clone(),
                        title: i18n.tr("ui.photo.label"),
                        show_names,
                        show_mark,
                        tags,
                        frames_until_shoot: 1,
                    });
                } else {
                    let fname = format!("photostrip-{}.png", now_ms());
                    let shot_path = shots_dir.join(&fname);
                    let _ = save_photostrip_png(
                        &shot_path,
                        &ui,
                        world_params.shared_fonts.as_deref_mut(),
                        &bridge.assets_dir,
                    );
                    ui.push_toast(i18n.tr("ui.photo.label"), format!("shots/{fname}"));
                }
            }
            UiAction::TriggerTimelapseClip { shape, music } => {
                if ui.timelapse_recording
                    || aux
                        .pending_timelapse
                        .as_deref()
                        .is_some_and(|p| p.0.is_some())
                {
                    continue;
                }
                ui.clip_shape = shape;
                ui.clip_music = music;
                let shots_dir = bridge.data_dir.join("shots");
                let _ = std::fs::create_dir_all(&shots_dir);
                let shape_tag = match shape {
                    ClipShape::Square => "1x1",
                    ClipShape::Tall => "9x16",
                };
                let ts = now_ms();
                let gif_name = format!("timelapse-15s-{shape_tag}-{ts}.gif");
                let gif_path = shots_dir.join(&gif_name);
                let wav_name = format!("timelapse-15s-{shape_tag}-{ts}.wav");
                let wav_path = music.then(|| shots_dir.join(&wav_name));

                if !aux.primary_win_q.is_empty()
                    && let Some(pending_tl) = aux.pending_timelapse.as_deref_mut()
                {
                    let cancel_flag = Arc::new(std::sync::atomic::AtomicBool::new(false));
                    if let Some(frame_tx) = spawn_timelapse_encoder_worker(
                        shape,
                        music,
                        gif_path,
                        wav_path,
                        i18n.tr("ui.clip.button"),
                        bridge.toast_tx.clone(),
                        Arc::clone(&cancel_flag),
                    ) {
                        ui.timelapse_recording = true;
                        ui.timelapse_progress = 0.0;
                        ui.timelapse_elapsed_sec = 0;
                        pending_tl.0 = Some(PendingTimelapseJob {
                            total_frames: 15,
                            captured_frames: 0,
                            frame_interval_sec: 1.0,
                            elapsed_since_last: 1.0,
                            frame_tx: Some(frame_tx),
                            cancel_flag,
                        });
                    }
                } else {
                    let gif_ok = write_headless_timelapse_gif(
                        &gif_path,
                        shape,
                        &ui,
                        world_params.shared_fonts.as_deref_mut(),
                        &bridge.assets_dir,
                    );
                    let wav_ok = wav_path
                        .as_deref()
                        .is_some_and(write_timelapse_wav_if_enabled);
                    let detail = if gif_ok && wav_ok {
                        format!("shots/{gif_name} + shots/{wav_name}")
                    } else {
                        format!("shots/{gif_name}")
                    };
                    ui.push_toast(i18n.tr("ui.clip.button"), detail);
                }
                if let Some(sfx) = aux.sfx.as_deref_mut() {
                    sfx.play_sfx(SfxKind::Ding);
                }
            }
            UiAction::CancelTimelapseClip => {
                ui.timelapse_recording = false;
                ui.timelapse_progress = 0.0;
                ui.timelapse_elapsed_sec = 0;
                if let Some(pending_tl) = aux.pending_timelapse.as_deref_mut()
                    && let Some(mut job) = pending_tl.0.take()
                {
                    job.cancel_flag
                        .store(true, std::sync::atomic::Ordering::Release);
                    job.frame_tx.take();
                }
                ui.push_toast(i18n.tr("ui.clip.button"), i18n.tr("ui.clip.canceled"));
            }
            UiAction::ToggleCameraFp => {
                if let Some(rig) = aux.camera_rig.as_deref_mut() {
                    rig.first_person = !rig.first_person;
                    ui.first_person = rig.first_person;
                    let next_proj = rig.projection();
                    for mut proj in &mut aux.cam_q {
                        *proj = next_proj.clone();
                    }
                } else {
                    ui.first_person = !ui.first_person;
                }
                if let Some(sfx) = aux.sfx.as_deref_mut() {
                    sfx.play_sfx(SfxKind::Click);
                }
            }
            UiAction::TriggerFirstPersonInteract => {
                if let Some(label) = ui.fp_near_label.clone() {
                    if let Some(sfx) = aux.sfx.as_deref_mut() {
                        sfx.play_sfx(SfxKind::Pop);
                    }
                    ui.push_toast(i18n.tr("ui.firstPersonHud.title"), label);
                }
            }
            UiAction::TriggerPhoto => {
                let shots_dir = bridge.data_dir.join("shots");
                let _ = std::fs::create_dir_all(&shots_dir);
                let fname = format!("shot-{}.png", now_ms());
                let shot_path = shots_dir.join(&fname);
                if let Some(sfx) = aux.sfx.as_deref_mut() {
                    sfx.play_sfx(SfxKind::Shutter);
                }
                if !aux.primary_win_q.is_empty() {
                    commands
                        .spawn(Screenshot::primary_window())
                        .observe(save_to_disk(shot_path));
                } else {
                    let _ = save_photostrip_png(
                        &shot_path,
                        &ui,
                        world_params.shared_fonts.as_deref_mut(),
                        &bridge.assets_dir,
                    );
                }
                ui.push_toast(i18n.tr("ui.photo.label"), format!("shots/{fname}"));
            }
            UiAction::ToggleDemo => {
                ui.demo_running = !ui.demo_running;
                if ui.demo_running {
                    let demo = UiState::with_demo_seed(ui.layout, ui.backdrop, ui.night);
                    ui.sessions = demo.sessions;
                    ui.pending_perms = demo.pending_perms;
                    ui.feed = demo.feed;
                    ui.chat = demo.chat;
                    ui.burn = demo.burn;
                    ui.selected_session = demo.selected_session;
                    if let Some(sfx) = aux.sfx.as_deref_mut() {
                        sfx.play_sfx(SfxKind::Ding);
                    }
                    ui.push_toast(i18n.tr("station.demoChip"), i18n.tr("station.demoOn"));
                } else {
                    ui.sessions.clear();
                    ui.pending_perms.clear();
                    ui.feed.clear();
                    ui.selected_session = None;
                    if let BridgeBackend::InProcess { app_state, .. } = &bridge.backend {
                        sync_office_core_to_ui(app_state, &bridge.dismissed_ci_ids, &mut ui);
                    } else if let BridgeBackend::AttachedHttp { snapshot, .. } = &bridge.backend {
                        let snap = snapshot.lock().unwrap_or_else(|e| e.into_inner()).clone();
                        apply_remote_snapshot_to_ui(&snap, &mut ui);
                    }
                    ui.push_toast(i18n.tr("station.demoChip"), i18n.tr("station.demoOff"));
                }
            }
            UiAction::ToggleReplay => {
                let enable = !ui.replay_active;
                ui.replay_active = enable;
                ui.replay_playing = enable;
                if let Some(rep) = aux.replay.as_deref_mut() {
                    if enable {
                        if rep.events.is_empty() {
                            load_replay_range_into_state(&bridge.data_dir, ui.replay_range, rep);
                        }
                        rep.speed = ui.replay_speed.max(1.0);
                        rep.active = true;
                        rep.playing = true;
                        ui.replay_heatmap = build_replay_heatmap(rep, 24);
                        sync_replay_ui_readout(rep, &mut ui);
                        ui.replay_active = true;
                    } else {
                        rep.playing = false;
                        rep.active = false;
                    }
                }
                let replay_msg = if ui.replay_active {
                    i18n.tr("replay.conn")
                } else {
                    i18n.tr("ui.replayBar.backToLive")
                };
                ui.push_toast(i18n.tr("ui.replayBar.label"), replay_msg);
            }
            UiAction::ReplayTogglePlay => {
                if let Some(rep) = aux.replay.as_deref_mut() {
                    rep.playing = ui.replay_playing;
                    rep.active = ui.replay_active;
                }
            }
            UiAction::ReplayScrub(frac) => {
                ui.replay_progress = frac.clamp(0.0, 1.0);
                if let Some(rep) = aux.replay.as_deref_mut() {
                    rep.scrub(ui.replay_progress);
                    sync_replay_ui_readout(rep, &mut ui);
                    ui.replay_active = true;
                }
            }
            UiAction::ReplaySetSpeed(speed) => {
                ui.replay_speed = speed.clamp(1.0, 8.0);
                if let Some(rep) = aux.replay.as_deref_mut() {
                    rep.speed = ui.replay_speed;
                }
            }
            UiAction::ReplaySetRange(range) => {
                ui.replay_range = range;
                if let Some(rep) = aux.replay.as_deref_mut() {
                    load_replay_range_into_state(&bridge.data_dir, range, rep);
                    rep.speed = ui.replay_speed.max(1.0);
                    rep.active = true;
                    ui.replay_heatmap = build_replay_heatmap(rep, 24);
                    sync_replay_ui_readout(rep, &mut ui);
                    ui.replay_active = true;
                }
            }
            UiAction::ReplayClose => {
                ui.replay_active = false;
                ui.replay_playing = false;
                if let Some(rep) = aux.replay.as_deref_mut() {
                    rep.playing = false;
                    rep.active = false;
                }
            }
            UiAction::OpenPlaceCard(place_id) => {
                match place_id.as_str() {
                    "server" => {
                        ui.active_place_card = None;
                        ui.active_drawer = ActiveDrawer::System;
                    }
                    "board" => {
                        ui.active_place_card = None;
                        ui.active_drawer = ActiveDrawer::Whiteboard;
                    }
                    _ => {
                        ui.active_place_card = Some(build_place_card_info(&place_id, &ui, &i18n));
                    }
                }
                if let Some(sfx) = aux.sfx.as_deref_mut() {
                    sfx.play_sfx(SfxKind::Click);
                }
            }
            UiAction::ClosePlaceCard => {
                ui.active_place_card = None;
            }
            UiAction::TriggerPlaceAction {
                place_id,
                action_id,
            } => {
                match action_id.as_str() {
                    "focus_meeting" => {
                        if let Some(rig) = aux.camera_rig.as_deref_mut() {
                            rig.apply_preset(station_3d::CameraPreset::Meeting, ui.layout);
                            ui.camera_preset = station_3d::CameraPreset::Meeting;
                            ui.camera_zoom_pct = rig.zoom_pct();
                            let next_proj = rig.projection();
                            for mut proj in &mut aux.cam_q {
                                *proj = next_proj.clone();
                            }
                        }
                    }
                    "focus_desks" => {
                        if let Some(rig) = aux.camera_rig.as_deref_mut() {
                            rig.apply_preset(station_3d::CameraPreset::Desks, ui.layout);
                            ui.camera_preset = station_3d::CameraPreset::Desks;
                            ui.camera_zoom_pct = rig.zoom_pct();
                            let next_proj = rig.projection();
                            for mut proj in &mut aux.cam_q {
                                *proj = next_proj.clone();
                            }
                        }
                    }
                    "cat" => {
                        if let Some(sfx) = aux.sfx.as_deref_mut() {
                            sfx.play_sfx(SfxKind::Meow);
                        }
                        ui.push_toast(i18n.tr("places.lounge.title"), i18n.tr("places.lounge.cat"));
                    }
                    "rocket" | "test_launch" => {
                        if let Some(rq) = aux.rocket_queue.as_deref_mut() {
                            rq.pending = true;
                        }
                        if let Some(sfx) = aux.sfx.as_deref_mut() {
                            sfx.play_sfx(SfxKind::Deploy);
                        }
                        ui.push_toast(
                            i18n.tr("places.deploy.title"),
                            i18n.tr("places.deploy.test"),
                        );
                    }
                    _ => {
                        if let Some(sfx) = aux.sfx.as_deref_mut() {
                            sfx.play_sfx(SfxKind::Brew);
                        }
                        ui.push_toast(
                            i18n.tr(place_title_i18n_key(&place_id)),
                            i18n.tr("places.coffee.brew"),
                        );
                    }
                }
            }
            UiAction::SendPrompt { session_id, text } => {
                let clean: String = station_core::redact::redact(text.trim())
                    .chars()
                    .take(4000)
                    .collect();
                if clean.is_empty() {
                    continue;
                }
                let ui_sess = ui.sessions.iter().find(|s| s.id == session_id);
                let fallback_agent = ui_sess
                    .map(|s| s.agent.clone())
                    .unwrap_or_else(|| "claude".to_owned());
                match &bridge.backend {
                    BridgeBackend::AttachedHttp {
                        snapshot,
                        action_tx,
                        ..
                    } => {
                        let is_busy = ui_sess
                            .map(|s| s.status != "rest")
                            .or_else(|| {
                                snapshot.lock().ok().and_then(|snap| {
                                    snap.sessions
                                        .iter()
                                        .find(|s| s.id == session_id)
                                        .map(|s| s.status != "rest")
                                })
                            })
                            .unwrap_or(false);
                        let (path, body) = if is_busy {
                            (
                                "/api/tasks/steer",
                                json!({
                                    "sid": session_id,
                                    "text": clean,
                                }),
                            )
                        } else {
                            (
                                "/api/tasks",
                                json!({
                                    "sid": session_id,
                                    "text": clean,
                                    "agent": fallback_agent,
                                }),
                            )
                        };
                        let _ = action_tx.try_send(RemoteActionCmd {
                            path,
                            body,
                            refresh: RefreshKind::Live,
                            launch_toast_label: None,
                        });
                    }
                    BridgeBackend::InProcess {
                        app_state,
                        runtime_handle,
                        ..
                    } => {
                        let ts = now_ms();
                        let (cwd, agent, name, is_busy) = {
                            let office = app_state.office.lock().unwrap_or_else(|e| e.into_inner());
                            office
                                .sessions
                                .get(&session_id)
                                .map(|s| {
                                    (
                                        s.cwd.clone(),
                                        s.agent.clone(),
                                        s.name.clone(),
                                        s.status != "rest",
                                    )
                                })
                                .unwrap_or_else(|| {
                                    (
                                        String::new(),
                                        fallback_agent.clone(),
                                        session_id.clone(),
                                        true,
                                    )
                                })
                        };
                        if !is_busy
                            && !cwd.is_empty()
                            && let Some(rt) = runtime_handle
                        {
                            // Idle session with a live runtime: dispatch immediately via TaskManager
                            // (never double-enqueue into steer_queue).
                            let tasks = app_state.tasks.clone();
                            let toast_tx = bridge.toast_tx.clone();
                            let req = station_core::state::tasks::TaskStartRequest {
                                dir: cwd,
                                text: clean,
                                agent: agent.clone(),
                                model: String::new(),
                                fork: false,
                            };
                            rt.spawn(async move {
                                let res = station_core::state::tasks::start_and_spawn(
                                    tasks, req, ts, None,
                                );
                                if res.get("ok").and_then(Value::as_bool) != Some(true) {
                                    let code = res
                                        .get("code")
                                        .and_then(Value::as_str)
                                        .unwrap_or("queued");
                                    let _ = toast_tx
                                        .try_send(("Prompt".to_owned(), format!("{agent}: {code}")));
                                }
                            });
                        } else if let Ok(mut q) = app_state.steer_queue.lock() {
                            q.enqueue(
                                station_core::state::steer::QueuedSteer {
                                    key: session_id.clone(),
                                    sid: Some(session_id.clone()),
                                    dir: (!cwd.is_empty()).then_some(cwd.clone()),
                                    agent: agent.clone(),
                                    text: clean.clone(),
                                    name,
                                    at: ts,
                                },
                                ts,
                            );
                        }
                    }
                    BridgeBackend::BindError { .. } => {
                        ui.push_toast(i18n.tr("drawer.compose.send"), i18n.tr("drawer.unreachable"));
                    }
                }
                if let Some(sfx) = aux.sfx.as_deref_mut() {
                    sfx.play_sfx(SfxKind::Pop);
                }
            }
            UiAction::PostGroupChat(text) => {
                let clean = station_core::redact::redact(text.trim());
                if clean.is_empty() {
                    continue;
                }
                match &bridge.backend {
                    BridgeBackend::AttachedHttp { action_tx, .. } => {
                        let _ = action_tx.try_send(RemoteActionCmd {
                            path: "/api/chat",
                            body: json!({ "op": "send", "text": clean }),
                            refresh: RefreshKind::Chat,
                            launch_toast_label: None,
                        });
                    }
                    BridgeBackend::InProcess { app_state, .. } => {
                        if let Ok(mut office) = app_state.office.lock() {
                            let ts = now_ms();
                            let msg = office.chat.send_rich(
                                "me", "Boss", &clean, ts, false, None, None, None,
                            );
                            let _ = office.save_chat();
                            office.broadcast(json!({ "k": "chat", "op": "send", "m": msg }));
                            sync_office_chat_to_ui(&office, &mut ui);
                        }
                    }
                    BridgeBackend::BindError { .. } => {
                        ui.push_toast(i18n.tr("chat.group"), i18n.tr("drawer.unreachable"));
                        continue;
                    }
                }
                if let Some(sfx) = aux.sfx.as_deref_mut() {
                    sfx.play_sfx(SfxKind::Chat);
                }
            }
            UiAction::SendGroupChat {
                text,
                reply_to,
                sticker,
            } => {
                let clean = station_core::redact::redact(text.trim());
                if clean.is_empty() && sticker.is_none() {
                    continue;
                }
                match &bridge.backend {
                    BridgeBackend::AttachedHttp { action_tx, .. } => {
                        let _ = action_tx.try_send(RemoteActionCmd {
                            path: "/api/chat",
                            body: json!({
                                "op": "send",
                                "text": clean,
                                "replyTo": reply_to,
                                "sticker": sticker,
                            }),
                            refresh: RefreshKind::Chat,
                            launch_toast_label: None,
                        });
                    }
                    BridgeBackend::InProcess { app_state, .. } => {
                        if let Ok(mut office) = app_state.office.lock() {
                            let ts = now_ms();
                            let reply_ref = reply_to
                                .as_deref()
                                .filter(|s| !s.is_empty())
                                .and_then(|rid| office.chat.reply_ref(rid));
                            let msg = office.chat.send_rich(
                                "me",
                                "Boss",
                                &clean,
                                ts,
                                false,
                                sticker,
                                None,
                                reply_ref,
                            );
                            let _ = office.save_chat();
                            office.broadcast(json!({ "k": "chat", "op": "send", "m": msg }));
                            sync_office_chat_to_ui(&office, &mut ui);
                        }
                    }
                    BridgeBackend::BindError { .. } => {
                        ui.push_toast(i18n.tr("chat.group"), i18n.tr("drawer.unreachable"));
                        continue;
                    }
                }
                if let Some(sfx) = aux.sfx.as_deref_mut() {
                    sfx.play_sfx(SfxKind::Chat);
                }
            }
            UiAction::ReactGroupChat { id, emoji } => {
                match &bridge.backend {
                    BridgeBackend::AttachedHttp { action_tx, .. } => {
                        let _ = action_tx.try_send(RemoteActionCmd {
                            path: "/api/chat",
                            body: json!({ "op": "react", "id": id, "emoji": emoji }),
                            refresh: RefreshKind::Chat,
                            launch_toast_label: None,
                        });
                    }
                    BridgeBackend::InProcess { app_state, .. } => {
                        if let Ok(mut office) = app_state.office.lock() {
                            if office.chat.react(&id, "me", &emoji) {
                                let _ = office.save_chat();
                                let m = office.chat.by_id(&id).cloned();
                                office.broadcast(
                                    json!({ "k": "chat", "op": "react", "id": id, "m": m }),
                                );
                                sync_office_chat_to_ui(&office, &mut ui);
                            }
                        }
                    }
                    BridgeBackend::BindError { .. } => {
                        ui.push_toast(i18n.tr("chat.group"), i18n.tr("drawer.unreachable"));
                        continue;
                    }
                }
                if let Some(sfx) = aux.sfx.as_deref_mut() {
                    sfx.play_sfx(SfxKind::Pop);
                }
            }
            UiAction::RecallGroupChat { id } => {
                match &bridge.backend {
                    BridgeBackend::AttachedHttp { action_tx, .. } => {
                        let _ = action_tx.try_send(RemoteActionCmd {
                            path: "/api/chat",
                            body: json!({ "op": "recall", "id": id }),
                            refresh: RefreshKind::Chat,
                            launch_toast_label: None,
                        });
                    }
                    BridgeBackend::InProcess { app_state, .. } => {
                        if let Ok(mut office) = app_state.office.lock() {
                            if office.chat.recall(&id) {
                                let _ = office.save_chat();
                                office.broadcast(
                                    json!({ "k": "chat", "op": "recall", "id": id }),
                                );
                                sync_office_chat_to_ui(&office, &mut ui);
                            }
                        }
                    }
                    BridgeBackend::BindError { .. } => {
                        ui.push_toast(i18n.tr("chat.group"), i18n.tr("drawer.unreachable"));
                        continue;
                    }
                }
            }
            UiAction::PinGroupChat { id } => {
                match &bridge.backend {
                    BridgeBackend::AttachedHttp { action_tx, .. } => {
                        let _ = action_tx.try_send(RemoteActionCmd {
                            path: "/api/chat",
                            body: json!({ "op": "pin", "id": id }),
                            refresh: RefreshKind::Chat,
                            launch_toast_label: None,
                        });
                    }
                    BridgeBackend::InProcess { app_state, .. } => {
                        if let Ok(mut office) = app_state.office.lock() {
                            if office.chat.pin(&id) {
                                let _ = office.save_chat();
                                office.broadcast(json!({ "k": "chat", "op": "pin", "id": id }));
                                sync_office_chat_to_ui(&office, &mut ui);
                            }
                        }
                    }
                    BridgeBackend::BindError { .. } => {
                        ui.push_toast(i18n.tr("chat.group"), i18n.tr("drawer.unreachable"));
                        continue;
                    }
                }
            }
            UiAction::CreateGroupPoll { q, opts } => {
                let q_clean = station_core::redact::redact(q.trim());
                let opts_clean: Vec<String> = opts
                    .iter()
                    .take(6)
                    .map(|o| station_core::redact::redact(o.trim()))
                    .filter(|o| !o.is_empty())
                    .collect();
                if q_clean.is_empty() || opts_clean.len() < 2 {
                    continue;
                }
                match &bridge.backend {
                    BridgeBackend::AttachedHttp { action_tx, .. } => {
                        let _ = action_tx.try_send(RemoteActionCmd {
                            path: "/api/chat",
                            body: json!({ "op": "poll", "q": q_clean, "opts": opts_clean }),
                            refresh: RefreshKind::Chat,
                            launch_toast_label: None,
                        });
                    }
                    BridgeBackend::InProcess { app_state, .. } => {
                        if let Ok(mut office) = app_state.office.lock() {
                            let ts = now_ms();
                            if let Some(msg) =
                                office.chat.poll("me", "Boss", &q_clean, &opts_clean, ts)
                            {
                                let _ = office.save_chat();
                                office.broadcast(json!({ "k": "chat", "op": "poll", "m": msg }));
                                sync_office_chat_to_ui(&office, &mut ui);
                            }
                        }
                    }
                    BridgeBackend::BindError { .. } => {
                        ui.push_toast(i18n.tr("chat.group"), i18n.tr("drawer.unreachable"));
                        continue;
                    }
                }
                if let Some(sfx) = aux.sfx.as_deref_mut() {
                    sfx.play_sfx(SfxKind::Pop);
                }
            }
            UiAction::VoteGroupPoll { id, option_index } => {
                match &bridge.backend {
                    BridgeBackend::AttachedHttp { action_tx, .. } => {
                        let _ = action_tx.try_send(RemoteActionCmd {
                            path: "/api/chat",
                            body: json!({ "op": "vote", "id": id, "i": option_index }),
                            refresh: RefreshKind::Chat,
                            launch_toast_label: None,
                        });
                    }
                    BridgeBackend::InProcess { app_state, .. } => {
                        if let Ok(mut office) = app_state.office.lock() {
                            if office.chat.vote(&id, "me", option_index) {
                                let _ = office.save_chat();
                                let m = office.chat.by_id(&id).cloned();
                                office.broadcast(
                                    json!({ "k": "chat", "op": "vote", "id": id, "m": m }),
                                );
                                sync_office_chat_to_ui(&office, &mut ui);
                            }
                        }
                    }
                    BridgeBackend::BindError { .. } => {
                        ui.push_toast(i18n.tr("chat.group"), i18n.tr("drawer.unreachable"));
                        continue;
                    }
                }
            }
            UiAction::RsvpGroupRemind { id, yes } => {
                match &bridge.backend {
                    BridgeBackend::AttachedHttp { action_tx, .. } => {
                        let _ = action_tx.try_send(RemoteActionCmd {
                            path: "/api/chat",
                            body: json!({ "op": "rsvp", "id": id, "yes": yes }),
                            refresh: RefreshKind::Chat,
                            launch_toast_label: None,
                        });
                    }
                    BridgeBackend::InProcess { app_state, .. } => {
                        if let Ok(mut office) = app_state.office.lock() {
                            if office.chat.rsvp(&id, "me", yes) {
                                let _ = office.save_chat();
                                let m = office.chat.by_id(&id).cloned();
                                office.broadcast(
                                    json!({ "k": "chat", "op": "rsvp", "id": id, "m": m }),
                                );
                                sync_office_chat_to_ui(&office, &mut ui);
                            }
                        }
                    }
                    BridgeBackend::BindError { .. } => {
                        ui.push_toast(i18n.tr("chat.group"), i18n.tr("drawer.unreachable"));
                        continue;
                    }
                }
            }
            UiAction::SendGroupPhoto => {
                let Some(data_url) = render_group_chat_photo_data_url(
                    &ui,
                    &i18n,
                    world_params.shared_fonts.as_deref_mut(),
                    &bridge.assets_dir,
                ) else {
                    continue;
                };
                match &bridge.backend {
                    BridgeBackend::AttachedHttp { action_tx, .. } => {
                        let _ = action_tx.try_send(RemoteActionCmd {
                            path: "/api/chat",
                            body: json!({ "op": "photo", "data": data_url }),
                            refresh: RefreshKind::Chat,
                            launch_toast_label: None,
                        });
                    }
                    BridgeBackend::InProcess { app_state, .. } => {
                        if let Ok(mut office) = app_state.office.lock() {
                            let ts = now_ms();
                            let msg = office.chat.photo("me", "Boss", data_url, ts);
                            let _ = office.save_chat();
                            office.broadcast(json!({ "k": "chat", "op": "photo", "m": msg }));
                            sync_office_chat_to_ui(&office, &mut ui);
                        }
                    }
                    BridgeBackend::BindError { .. } => {
                        ui.push_toast(i18n.tr("chat.group"), i18n.tr("drawer.unreachable"));
                        continue;
                    }
                }
                if let Some(sfx) = aux.sfx.as_deref_mut() {
                    sfx.play_sfx(SfxKind::Shutter);
                }
            }
            UiAction::FetchChatImage { id } => match &bridge.backend {
                BridgeBackend::AttachedHttp { query_tx, .. } => {
                    let _ = query_tx.try_send(RemoteQueryCmd::FetchImage { id });
                }
                BridgeBackend::InProcess { app_state, .. } => {
                    let resolved = app_state
                        .images
                        .lock()
                        .ok()
                        .and_then(|st| st.lookup(&id).ok());
                    if let Some((path, _mime)) = resolved {
                        let async_tx = bridge.async_tx.clone();
                        let img_id = id.clone();
                        let gone_msg = i18n.tr("activity.image.gone");
                        std::thread::spawn(move || {
                            let res = station_core::store::images::read_image_bytes_bounded(
                                &path,
                                MAX_CHAT_IMAGE_BYTES as u64,
                            )
                            .map_err(|_| gone_msg)
                            .and_then(|bytes| decode_bounded_chat_preview(img_id, &bytes));
                            let _ = async_tx.try_send(BridgeAsyncResult::ChatImage(res));
                        });
                    } else {
                        ui.push_toast(i18n.tr("chat.photo"), i18n.tr("activity.image.gone"));
                    }
                }
                BridgeBackend::BindError { .. } => {
                    ui.push_toast(i18n.tr("chat.photo"), i18n.tr("drawer.unreachable"));
                    continue;
                }
            },
            UiAction::PreviewChatDataUrl { id, data_url } => {
                let async_tx = bridge.async_tx.clone();
                let gone_msg = i18n.tr("activity.image.gone");
                std::thread::spawn(move || {
                    let res = decode_chat_data_url(id, &data_url).map_err(|_| gone_msg);
                    let _ = async_tx.try_send(BridgeAsyncResult::ChatImage(res));
                });
            }
            UiAction::SaveWhiteboard(text) => {
                let todos = parse_whiteboard_todos(&text);
                match &bridge.backend {
                    BridgeBackend::InProcess { app_state, .. } => {
                        if let Ok(mut office) = app_state.office.lock() {
                            let _ = office.set_todos(todos);
                            ui.session_todos = parse_todos_from_values(&office.todos);
                            ui.whiteboard_text =
                                station_ui::drawers::whiteboard::format_todos_markdown(
                                    &ui.session_todos,
                                );
                        }
                    }
                    BridgeBackend::AttachedHttp { action_tx, .. } => {
                        let clean_todos: Vec<Value> = todos
                            .into_iter()
                            .map(|mut v| {
                                if let Some(obj) = v.as_object_mut()
                                    && let Some(raw) = obj.get("text").and_then(Value::as_str)
                                {
                                    let clean = station_core::redact::redact(raw);
                                    obj.insert("text".to_owned(), Value::String(clean));
                                }
                                v
                            })
                            .collect();
                        ui.session_todos = parse_todos_from_values(&clean_todos);
                        let _ = action_tx.try_send(RemoteActionCmd {
                            path: "/api/todos",
                            body: json!({ "todos": clean_todos }),
                            refresh: RefreshKind::Live,
                            launch_toast_label: None,
                        });
                    }
                    BridgeBackend::BindError { .. } => {
                        ui.push_toast(i18n.tr("board.cardTitle"), i18n.tr("board.saveOffline"));
                        continue;
                    }
                }
                let redraw_text = ui.whiteboard_text.clone();
                if let (Some(mats), Some(imgs), Some(shared_fonts)) = (
                    world_params.materials.as_deref_mut(),
                    world_params.images.as_deref_mut(),
                    world_params.shared_fonts.as_deref_mut(),
                ) {
                    let fonts = shared_fonts.get_or_init();
                    for wb_mat in &aux.wb_q {
                        station_3d::furniture::redraw_whiteboard_surface(
                            &redraw_text,
                            ui.streamer_mode,
                            &wb_mat.0,
                            mats,
                            imgs,
                            fonts,
                        );
                    }
                }
                if let Some(sfx) = aux.sfx.as_deref_mut() {
                    sfx.play_sfx(SfxKind::Rustle);
                }
            }
            UiAction::SyncWhiteboardTodos(mut todos) => {
                for t in &mut todos {
                    t.text = station_core::redact::redact(t.text.trim())
                        .chars()
                        .take(140)
                        .collect();
                }
                ui.session_todos = todos.clone();
                let text = station_ui::drawers::whiteboard::format_todos_markdown(&todos);
                ui.whiteboard_text = text.clone();
                let todos_json = todos_to_json_values(&todos);
                match &bridge.backend {
                    BridgeBackend::InProcess { app_state, .. } => {
                        if let Ok(mut office) = app_state.office.lock() {
                            let _ = office.set_todos(todos_json);
                        }
                    }
                    BridgeBackend::AttachedHttp { action_tx, .. } => {
                        let _ = action_tx.try_send(RemoteActionCmd {
                            path: "/api/todos",
                            body: json!({ "todos": todos_json }),
                            refresh: RefreshKind::Live,
                            launch_toast_label: None,
                        });
                    }
                    BridgeBackend::BindError { .. } => {
                        ui.push_toast(i18n.tr("board.cardTitle"), i18n.tr("board.saveOffline"));
                        continue;
                    }
                }
                if let (Some(mats), Some(imgs), Some(shared_fonts)) = (
                    world_params.materials.as_deref_mut(),
                    world_params.images.as_deref_mut(),
                    world_params.shared_fonts.as_deref_mut(),
                ) {
                    let fonts = shared_fonts.get_or_init();
                    for wb_mat in &aux.wb_q {
                        station_3d::furniture::redraw_whiteboard_surface(
                            &text,
                            ui.streamer_mode,
                            &wb_mat.0,
                            mats,
                            imgs,
                            fonts,
                        );
                    }
                }
                if let Some(sfx) = aux.sfx.as_deref_mut() {
                    sfx.play_sfx(SfxKind::Pop);
                }
            }
            UiAction::FetchWrapped => match &bridge.backend {
                BridgeBackend::AttachedHttp { query_tx, .. } => {
                    let _ = query_tx.try_send(RemoteQueryCmd::FetchWrapped);
                }
                BridgeBackend::InProcess { app_state, .. } => {
                    let replay_dir = app_state.data_dir.join("replay");
                    let async_tx = bridge.async_tx.clone();
                    std::thread::spawn(move || {
                        let val =
                            station_core::store::wrapped::week_summary(&replay_dir, 7, now_ms());
                        let _ = async_tx.try_send(BridgeAsyncResult::WrappedSummary(Ok(
                            parse_wrapped_from_json(&val),
                        )));
                    });
                }
                BridgeBackend::BindError { .. } => {
                    ui.wrapped.loading = false;
                    ui.wrapped.loaded = true;
                    ui.wrapped.error = Some(i18n.tr("wrapped.loadFailed"));
                    ui.push_toast(i18n.tr("ui.wrapped.title"), i18n.tr("wrapped.loadFailed"));
                    continue;
                }
            },
            UiAction::ExportWrappedPng { copy_clipboard } => {
                let input = build_wrapped_card_input(&ui, &i18n);
                let shots_dir = bridge.data_dir.join("shots");
                let assets_dir = bridge.assets_dir.clone();
                let date_tag = sanitize_wrapped_date_tag(&ui.wrapped.to);
                let toast_title = i18n.tr("ui.wrapped.title");
                let copied_msg = i18n.tr("wrapped.copied");
                let copy_failed_msg = i18n.tr_vars(
                    "wrapped.copyFailed",
                    &[("button", &i18n.tr("ui.wrapped.save"))],
                );
                let photo_failed_msg = i18n.tr("ui.photo.failed");
                let async_tx = bridge.async_tx.clone();

                if let Some(sfx) = aux.sfx.as_deref_mut() {
                    sfx.play_sfx(SfxKind::Shutter);
                }
                std::thread::spawn(move || {
                    let mut fonts = station_canvas2d::FontEngine::from_assets_dir(&assets_dir);
                    let surf = station_canvas2d::draw_wrapped_card(&input, &mut fonts);
                    let png_res = station_canvas2d::encode_rgba_to_png(&surf);
                    let result = match png_res {
                        Ok(png_bytes) => {
                            let _ = std::fs::create_dir_all(&shots_dir);
                            let fname = format!("wrapped-{date_tag}-{}.png", now_ms());
                            let out_path = shots_dir.join(&fname);
                            if std::fs::write(&out_path, &png_bytes).is_ok() {
                                let rel = format!("shots/{fname}");
                                let toast_body = if copy_clipboard {
                                    if copy_rgba_or_path_to_clipboard(
                                        &surf,
                                        &out_path.to_string_lossy(),
                                    ) {
                                        copied_msg
                                    } else {
                                        copy_failed_msg
                                    }
                                } else {
                                    rel.clone()
                                };
                                BridgeAsyncResult::WrappedExported {
                                    rel_path: Some(rel),
                                    toast_title,
                                    toast_body,
                                }
                            } else {
                                BridgeAsyncResult::WrappedExported {
                                    rel_path: None,
                                    toast_title,
                                    toast_body: photo_failed_msg,
                                }
                            }
                        }
                        Err(_) => BridgeAsyncResult::WrappedExported {
                            rel_path: None,
                            toast_title,
                            toast_body: photo_failed_msg,
                        },
                    };
                    drop(surf);
                    let _ = async_tx.try_send(result);
                });
            }
            UiAction::FetchBurn(range) => {
                ui.burn_drawer.range = range;
                ui.burn_drawer.loading = true;
                ui.burn_drawer.error = None;
                match &bridge.backend {
                    BridgeBackend::AttachedHttp { query_tx, .. } => {
                        let _ = query_tx.try_send(RemoteQueryCmd::FetchBurn(range));
                    }
                    BridgeBackend::InProcess { app_state, .. } => {
                        let state = Arc::clone(app_state);
                        let async_tx = bridge.async_tx.clone();
                        std::thread::spawn(move || {
                            let (summary, info) = {
                                let otel = state.otel.lock().unwrap_or_else(|e| e.into_inner());
                                (otel.burn_summary(range.api_str(), now_ms()), otel.info())
                            };
                            let mut val = json!(summary);
                            if let Some(obj) = val.as_object_mut() {
                                obj.insert("ok".to_owned(), Value::Bool(true));
                                obj.insert("range".to_owned(), Value::String(range.api_str().to_owned()));
                                obj.insert("days".to_owned(), info.get("days").cloned().unwrap_or(json!(0)));
                                let settings_path = state.claude_dir.join("settings.json");
                                let setup = std::fs::read_to_string(&settings_path)
                                    .ok()
                                    .and_then(|s| serde_json::from_str::<Value>(&s).ok())
                                    .and_then(|v| v.get("env").and_then(Value::as_object).cloned())
                                    .map(|env| {
                                        if !station_core::store::otel_setup::marked_keys(&env).is_empty() {
                                            "ours"
                                        } else if env.get("CLAUDE_CODE_ENABLE_TELEMETRY").and_then(Value::as_str) == Some("1") {
                                            "other"
                                        } else {
                                            "none"
                                        }
                                    })
                                    .unwrap_or("none");
                                obj.insert("setup".to_owned(), Value::String(setup.to_owned()));
                            }
                            let _ = async_tx.try_send(BridgeAsyncResult::BurnData(Ok(
                                parse_burn_drawer_from_json(range, &val),
                            )));
                        });
                    }
                    BridgeBackend::BindError { .. } => {
                        ui.burn_drawer.loading = false;
                        ui.burn_drawer.loaded_range = Some(range);
                        ui.burn_drawer.error = Some(i18n.tr("burn.unreachable"));
                        ui.push_toast(i18n.tr("burn.title"), i18n.tr("burn.unreachable"));
                        continue;
                    }
                }
            }
            UiAction::FetchVaultList { query, fresh, seq } => {
                if seq > ui.vault_drawer.req_seq {
                    ui.vault_drawer.req_seq = seq;
                }
                ui.vault_drawer.loading = true;
                match &bridge.backend {
                    BridgeBackend::AttachedHttp { query_tx, .. } => {
                        let _ = query_tx.try_send(RemoteQueryCmd::FetchVaultList { query, fresh, seq });
                    }
                    BridgeBackend::InProcess { app_state, .. } => {
                        let state = Arc::clone(app_state);
                        let async_tx = bridge.async_tx.clone();
                        std::thread::spawn(move || {
                            let mut vault = state.vault.lock().unwrap_or_else(|e| e.into_inner());
                            if fresh || vault.scanned_at() == 0 {
                                let _ = vault.scan_claude_projects(&state.claude_dir);
                            }
                            let stats = vault.stats();
                            let items: Vec<Value> = vault
                                .search(&query, 60)
                                .into_iter()
                                .map(|e| json!(e))
                                .collect();
                            let val = json!({ "stats": stats, "items": items });
                            let (parsed_items, total_count, total_bytes) = parse_vault_list_from_json(&val);
                            let _ = async_tx.try_send(BridgeAsyncResult::VaultList {
                                seq,
                                items: parsed_items,
                                total_count,
                                total_bytes,
                            });
                        });
                    }
                    BridgeBackend::BindError { .. } => {
                        ui.vault_drawer.loading = false;
                        ui.vault_drawer.loaded = true;
                        ui.push_toast(i18n.tr("vault.title"), i18n.tr("drawer.unreachable"));
                        continue;
                    }
                }
            }
            UiAction::FetchVaultTranscript(sid) => {
                if !station_core::store::vault::is_valid_sid(&sid) {
                    ui.vault_drawer.transcript_loading = None;
                    ui.push_toast(i18n.tr("vault.title"), i18n.tr("vault.gotIt"));
                    continue;
                }
                ui.vault_drawer.transcript_loading = Some(sid.clone());
                match &bridge.backend {
                    BridgeBackend::AttachedHttp { query_tx, .. } => {
                        let _ = query_tx.try_send(RemoteQueryCmd::FetchVaultTranscript { sid });
                    }
                    BridgeBackend::InProcess { app_state, .. } => {
                        let state = Arc::clone(app_state);
                        let async_tx = bridge.async_tx.clone();
                        std::thread::spawn(move || {
                            let mut vault = state.vault.lock().unwrap_or_else(|e| e.into_inner());
                            if vault.scanned_at() == 0 {
                                let _ = vault.scan_claude_projects(&state.claude_dir);
                            }
                            let rec = vault
                                .get_transcript(&sid)
                                .and_then(|r| parse_vault_transcript_from_json(&json!({ "rec": r })));
                            let _ = async_tx.try_send(BridgeAsyncResult::VaultTranscript(rec));
                        });
                    }
                    BridgeBackend::BindError { .. } => {
                        ui.vault_drawer.transcript_loading = None;
                        ui.push_toast(i18n.tr("vault.title"), i18n.tr("drawer.unreachable"));
                        continue;
                    }
                }
            }
            UiAction::FetchVaultReport => {
                ui.vault_drawer.report.loading = true;
                ui.vault_drawer.report.error = None;
                match &bridge.backend {
                    BridgeBackend::AttachedHttp { query_tx, .. } => {
                        let _ = query_tx.try_send(RemoteQueryCmd::FetchVaultReport);
                    }
                    BridgeBackend::InProcess { app_state, .. } => {
                        let state = Arc::clone(app_state);
                        let async_tx = bridge.async_tx.clone();
                        std::thread::spawn(move || {
                            let mut vault = state.vault.lock().unwrap_or_else(|e| e.into_inner());
                            if vault.scanned_at() == 0 {
                                let _ = vault.scan_claude_projects(&state.claude_dir);
                            }
                            let since = now_ms().saturating_sub(7 * 86_400_000);
                            let prompts = vault.recent_prompts(since);
                            drop(vault);
                            let rep_val = json!(station_core::store::prompt_report::build_prompt_report(
                                &prompts, 3
                            ));
                            let _ = async_tx.try_send(BridgeAsyncResult::VaultReport(Ok(
                                parse_vault_report_from_json(&rep_val),
                            )));
                        });
                    }
                    BridgeBackend::BindError { .. } => {
                        ui.vault_drawer.report.loading = false;
                        ui.vault_drawer.report.loaded = true;
                        ui.vault_drawer.report.error = Some(i18n.tr("drawer.unreachable"));
                        ui.push_toast(i18n.tr("vault.tabReport"), i18n.tr("drawer.unreachable"));
                        continue;
                    }
                }
            }
            UiAction::CopyToClipboard {
                text,
                toast_title,
                toast_body,
            } => {
                let toast_tx = bridge.toast_tx.clone();
                let fail_msg = i18n.tr("drawer.copyFailed");
                std::thread::spawn(move || {
                    let ok = arboard::Clipboard::new()
                        .and_then(|mut cb| cb.set_text(text))
                        .is_ok();
                    let _ = toast_tx.try_send((toast_title, if ok { toast_body } else { fail_msg }));
                });
            }
            UiAction::SysAction {
                action,
                target,
                force,
                start,
            } => {
                let title = i18n.tr("ui.macDrawer.title");
                let ok_msg = i18n.tr("mac.action.done");
                let err_msg = i18n.tr("mac.action.failed");
                match &bridge.backend {
                    BridgeBackend::AttachedHttp { query_tx, .. } => {
                        let _ = query_tx.try_send(RemoteQueryCmd::SysMutate {
                            path: "/api/sys/action",
                            body: json!({
                                "action": action,
                                "target": target,
                                "force": force,
                                "start": start,
                            }),
                            toast_title: title,
                            toast_ok: ok_msg,
                            toast_err: err_msg,
                        });
                    }
                    BridgeBackend::InProcess { app_state, .. } => {
                        let state = Arc::clone(app_state);
                        let toast_tx = bridge.toast_tx.clone();
                        bridge.sys_refresh_due = true;
                        std::thread::spawn(move || {
                            let res = state.sys.action(&action, Some(&target), force, start);
                            let ok = res.get("ok").and_then(Value::as_bool).unwrap_or(false);
                            let _ = toast_tx.try_send((title, if ok { ok_msg } else { err_msg }));
                        });
                    }
                    BridgeBackend::BindError { .. } => {
                        ui.push_toast(title, i18n.tr("mac.action.unreachable"));
                        continue;
                    }
                }
            }
            UiAction::SetSysAutoClose(hours) => {
                ui.mac_drawer.auto_close_hours = hours;
                let title = i18n.tr("mac.auto.label");
                let ok_msg = if hours == 0 {
                    i18n.tr("mac.auto.savedOff")
                } else {
                    i18n.tr_vars("mac.auto.saved", &[("n", &hours.to_string())])
                };
                let err_msg = i18n.tr("mac.action.failed");
                match &bridge.backend {
                    BridgeBackend::AttachedHttp { query_tx, .. } => {
                        let _ = query_tx.try_send(RemoteQueryCmd::SysMutate {
                            path: "/api/sys/auto",
                            body: json!({ "hours": hours }),
                            toast_title: title,
                            toast_ok: ok_msg,
                            toast_err: err_msg,
                        });
                    }
                    BridgeBackend::InProcess { app_state, .. } => {
                        let ok = app_state
                            .prefs
                            .lock()
                            .unwrap_or_else(|e| e.into_inner())
                            .set_auto_close(hours)
                            .unwrap_or(false);
                        ui.push_toast(title, if ok { ok_msg } else { err_msg });
                    }
                    BridgeBackend::BindError { .. } => {
                        ui.push_toast(title, i18n.tr("mac.action.unreachable"));
                        continue;
                    }
                }
            }
            UiAction::FetchStranded { fresh } => {
                ui.mac_drawer.stranded_loading = true;
                match &bridge.backend {
                    BridgeBackend::AttachedHttp { query_tx, .. } => {
                        let _ = query_tx.try_send(RemoteQueryCmd::FetchStranded { fresh });
                    }
                    BridgeBackend::InProcess { app_state, .. } => {
                        let state = Arc::clone(app_state);
                        let async_tx = bridge.async_tx.clone();
                        std::thread::spawn(move || {
                            let (dirs, in_use) = collect_in_process_stranded_dirs(&state);
                            let snap = state
                                .stranded
                                .lock()
                                .unwrap_or_else(|e| e.into_inner())
                                .scan(&dirs, &in_use, now_ms(), fresh);
                            let val = json!({ "repos": snap.repos, "items": snap.items });
                            let (items, repos) = parse_stranded_from_json(&val);
                            let _ = async_tx.try_send(BridgeAsyncResult::StrandedData { items, repos });
                        });
                    }
                    BridgeBackend::BindError { .. } => {
                        ui.mac_drawer.stranded_loading = false;
                        ui.mac_drawer.stranded_loaded = true;
                        ui.push_toast(
                            i18n.tr("mac.stranded.title"),
                            i18n.tr("mac.action.unreachable"),
                        );
                        continue;
                    }
                }
            }
            UiAction::OpenStranded(id) => {
                let title = i18n.tr("mac.stranded.title");
                let ok_msg = i18n.tr("mac.stranded.open");
                let err_msg = i18n.tr("mac.action.failed");
                match &bridge.backend {
                    BridgeBackend::AttachedHttp { query_tx, .. } => {
                        let _ = query_tx.try_send(RemoteQueryCmd::StrandedMutate {
                            path: "/api/stranded/open",
                            id,
                            toast_title: title,
                            toast_ok: ok_msg,
                            toast_err: err_msg,
                        });
                    }
                    BridgeBackend::InProcess { app_state, .. } => {
                        let entry = app_state
                            .stranded
                            .lock()
                            .unwrap_or_else(|e| e.into_inner())
                            .entry(&id);
                        if let Some(entry) = entry {
                            let path = PathBuf::from(&entry.item.path);
                            let toast_tx = bridge.toast_tx.clone();
                            std::thread::spawn(move || {
                                let ok = station_core::state::stranded::open_folder(&path).is_ok();
                                let _ = toast_tx.try_send((title, if ok { ok_msg } else { err_msg }));
                            });
                        } else {
                            ui.push_toast(title, i18n.tr("mac.stranded.gone"));
                        }
                    }
                    BridgeBackend::BindError { .. } => {
                        ui.push_toast(title, i18n.tr("mac.action.unreachable"));
                        continue;
                    }
                }
            }
            UiAction::RemoveStranded(id) => {
                let title = i18n.tr("mac.stranded.title");
                let ok_msg = i18n.tr("mac.action.done");
                let err_msg = i18n.tr("mac.action.failed");
                match &bridge.backend {
                    BridgeBackend::AttachedHttp { query_tx, .. } => {
                        let _ = query_tx.try_send(RemoteQueryCmd::StrandedMutate {
                            path: "/api/stranded/remove",
                            id,
                            toast_title: title,
                            toast_ok: ok_msg,
                            toast_err: err_msg,
                        });
                    }
                    BridgeBackend::InProcess { app_state, .. } => {
                        let state = Arc::clone(app_state);
                        let async_tx = bridge.async_tx.clone();
                        let toast_tx = bridge.toast_tx.clone();
                        std::thread::spawn(move || {
                            let (dirs, in_use) = collect_in_process_stranded_dirs(&state);
                            let res = state
                                .stranded
                                .lock()
                                .unwrap_or_else(|e| e.into_inner())
                                .remove(&id, &in_use);
                            let _ = toast_tx.try_send((
                                title,
                                if res.ok { ok_msg } else { err_msg },
                            ));
                            let snap = state
                                .stranded
                                .lock()
                                .unwrap_or_else(|e| e.into_inner())
                                .scan(&dirs, &in_use, now_ms(), true);
                            let (items, repos) =
                                parse_stranded_from_json(&json!({ "repos": snap.repos, "items": snap.items }));
                            let _ = async_tx.try_send(BridgeAsyncResult::StrandedData { items, repos });
                        });
                    }
                    BridgeBackend::BindError { .. } => {
                        ui.push_toast(title, i18n.tr("mac.action.unreachable"));
                        continue;
                    }
                }
            }
            UiAction::FetchAiAccounts => match &bridge.backend {
                BridgeBackend::AttachedHttp { query_tx, .. } => {
                    let _ = query_tx.try_send(RemoteQueryCmd::FetchAiAccounts);
                }
                BridgeBackend::InProcess { app_state, .. } => {
                    let state = Arc::clone(app_state);
                    let async_tx = bridge.async_tx.clone();
                    std::thread::spawn(move || {
                        let _ = async_tx.try_send(build_in_process_ai_accounts(&state));
                    });
                }
                BridgeBackend::BindError { .. } => {
                    ui.ai_accounts.loaded = true;
                    ui.push_toast(i18n.tr("ai.title"), i18n.tr("drawer.unreachable"));
                    continue;
                }
            },
            UiAction::AiAccountMutate {
                provider,
                action,
                body,
            } => {
                let clean_body = redact_ai_mutate_body(body);
                let title = i18n.tr("ai.title");
                let ok_msg = i18n.tr("mac.action.done");
                let err_msg = i18n.tr("ai.failed");
                match &bridge.backend {
                    BridgeBackend::AttachedHttp { query_tx, .. } => {
                        let _ = query_tx.try_send(RemoteQueryCmd::AiAccountMutate {
                            provider,
                            action,
                            body: clean_body,
                            toast_title: title,
                            toast_ok: ok_msg,
                            toast_err: err_msg,
                        });
                    }
                    BridgeBackend::InProcess { app_state, .. } => {
                        let state = Arc::clone(app_state);
                        let async_tx = bridge.async_tx.clone();
                        std::thread::spawn(move || {
                            let res = match provider {
                                AiProviderTab::Claude => state
                                    .claude_accounts
                                    .lock()
                                    .unwrap_or_else(|e| e.into_inner())
                                    .mutate(&action, &clean_body),
                                AiProviderTab::Codex => state
                                    .codex_accounts
                                    .lock()
                                    .unwrap_or_else(|e| e.into_inner())
                                    .mutate(&action, &clean_body),
                                AiProviderTab::Antigravity => state
                                    .ag_accounts
                                    .lock()
                                    .unwrap_or_else(|e| e.into_inner())
                                    .mutate(&action, &clean_body),
                            };
                            let ok = res.get("ok").and_then(Value::as_bool).unwrap_or(false);
                            let code = res
                                .get("code")
                                .and_then(Value::as_str)
                                .map(str::to_owned);
                            let _ = async_tx.try_send(BridgeAsyncResult::AiAccountNotice {
                                title,
                                ok,
                                code,
                                ok_msg,
                                err_msg,
                            });
                            let _ = async_tx.try_send(build_in_process_ai_accounts(&state));
                        });
                    }
                    BridgeBackend::BindError { .. } => {
                        ui.push_toast(title, i18n.tr("drawer.unreachable"));
                        continue;
                    }
                }
            }
            UiAction::FetchPhoneStatus => match &bridge.backend {
                BridgeBackend::AttachedHttp { query_tx, .. } => {
                    let _ = query_tx.try_send(RemoteQueryCmd::FetchPhoneStatus);
                }
                BridgeBackend::InProcess { app_state, .. } => {
                    let state = Arc::clone(app_state);
                    let async_tx = bridge.async_tx.clone();
                    std::thread::spawn(move || {
                        let _ = async_tx.try_send(build_in_process_phone_status(&state));
                    });
                }
                BridgeBackend::BindError { .. } => {
                    ui.phone_modal.loaded = true;
                    ui.push_toast(i18n.tr("ui.phoneModal.title"), i18n.tr("drawer.unreachable"));
                    continue;
                }
            },
            UiAction::ToggleLan(on) => {
                let title = i18n.tr("ui.phoneModal.title");
                let ok_msg = i18n.tr(if on {
                    "topbar.phone.isOn"
                } else {
                    "topbar.phone.isOff"
                });
                let err_msg = i18n.tr("drawer.unreachable");
                match &bridge.backend {
                    BridgeBackend::AttachedHttp { query_tx, .. } => {
                        let _ = query_tx.try_send(RemoteQueryCmd::PhoneMutate {
                            path: "/api/lan",
                            body: json!({ "on": on }),
                            toast_title: title,
                            toast_ok: ok_msg,
                            toast_empty: None,
                            toast_err: err_msg,
                        });
                    }
                    BridgeBackend::InProcess { app_state, .. } => {
                        {
                            let mut prefs = app_state.prefs.lock().unwrap_or_else(|e| e.into_inner());
                            let _ = prefs.set("lan", if on { Some(Value::Bool(true)) } else { None });
                        }
                        ui.phone_modal.lan_on = on;
                        let state = Arc::clone(app_state);
                        let async_tx = bridge.async_tx.clone();
                        std::thread::spawn(move || {
                            let _ = async_tx.try_send(build_in_process_phone_status(&state));
                        });
                        ui.push_toast(title, ok_msg);
                    }
                    BridgeBackend::BindError { .. } => {
                        ui.push_toast(title, err_msg);
                        continue;
                    }
                }
            }
            UiAction::ToggleTunnel(on) => {
                let title = i18n.tr("topbar.phone.tabAway");
                let ok_msg = i18n.tr(if on {
                    "topbar.phone.tunStarting"
                } else {
                    "topbar.phone.isOff"
                });
                let err_msg = i18n.tr("drawer.unreachable");
                match &bridge.backend {
                    BridgeBackend::AttachedHttp { query_tx, .. } => {
                        let _ = query_tx.try_send(RemoteQueryCmd::PhoneMutate {
                            path: "/api/tunnel",
                            body: json!({ "on": on }),
                            toast_title: title,
                            toast_ok: ok_msg,
                            toast_empty: None,
                            toast_err: err_msg,
                        });
                    }
                    BridgeBackend::InProcess {
                        app_state,
                        runtime_handle,
                        ..
                    } => {
                        {
                            let mut prefs = app_state.prefs.lock().unwrap_or_else(|e| e.into_inner());
                            let _ = prefs.set("tunnel", if on { Some(Value::Bool(true)) } else { None });
                        }
                        ui.phone_modal.tunnel_on = on;
                        let state = Arc::clone(app_state);
                        let async_tx = bridge.async_tx.clone();
                        let toast_tx = bridge.toast_tx.clone();
                        if on {
                            if let Some(rt) = runtime_handle.clone() {
                                rt.spawn(async move {
                                    let _ = state.tunnel.start(Arc::clone(&state)).await;
                                    let _ = async_tx.try_send(build_in_process_phone_status(&state));
                                    let _ = toast_tx.try_send((title, ok_msg));
                                });
                            } else {
                                ui.push_toast(title, err_msg);
                            }
                        } else {
                            std::thread::spawn(move || {
                                let _ = state.tunnel.stop();
                                let _ = async_tx.try_send(build_in_process_phone_status(&state));
                                let _ = toast_tx.try_send((title, ok_msg));
                            });
                        }
                    }
                    BridgeBackend::BindError { .. } => {
                        ui.push_toast(title, err_msg);
                        continue;
                    }
                }
            }
            UiAction::SetTunnelDomain(host) => {
                let clean = station_core::redact::redact(&host).trim().to_ascii_lowercase();
                let title = i18n.tr("topbar.phone.fixedTitle");
                let err_msg = i18n.tr("mac.action.failed");
                if !clean.is_empty() && !is_valid_tunnel_host_input(&clean) {
                    ui.push_toast(title, err_msg);
                    continue;
                }
                let ok_msg = i18n.tr("mac.action.done");
                match &bridge.backend {
                    BridgeBackend::AttachedHttp { query_tx, .. } => {
                        let _ = query_tx.try_send(RemoteQueryCmd::PhoneMutate {
                            path: "/api/tunnel/domain",
                            body: json!({ "host": clean }),
                            toast_title: title,
                            toast_ok: ok_msg,
                            toast_empty: None,
                            toast_err: err_msg,
                        });
                    }
                    BridgeBackend::InProcess { app_state, .. } => {
                        {
                            let mut prefs = app_state.prefs.lock().unwrap_or_else(|e| e.into_inner());
                            let _ = prefs.set(
                                "tunnelDomain",
                                if clean.is_empty() {
                                    None
                                } else {
                                    Some(Value::String(clean.clone()))
                                },
                            );
                        }
                        ui.phone_modal.tunnel_domain = clean.clone();
                        ui.phone_modal.tunnel_domain_draft = clean;
                        let state = Arc::clone(app_state);
                        let async_tx = bridge.async_tx.clone();
                        std::thread::spawn(move || {
                            let _ = async_tx.try_send(build_in_process_phone_status(&state));
                        });
                        ui.push_toast(title, ok_msg);
                    }
                    BridgeBackend::BindError { .. } => {
                        ui.push_toast(title, i18n.tr("drawer.unreachable"));
                        continue;
                    }
                }
            }
            UiAction::TogglePhoneApprove(approve) => {
                let title = i18n.tr("topbar.phone.approve");
                let ok_msg = i18n.tr(if approve {
                    "topbar.phone.isOn"
                } else {
                    "topbar.phone.isOff"
                });
                let err_msg = i18n.tr("drawer.unreachable");
                match &bridge.backend {
                    BridgeBackend::AttachedHttp { query_tx, .. } => {
                        let _ = query_tx.try_send(RemoteQueryCmd::PhoneMutate {
                            path: "/api/phone",
                            body: json!({ "approve": approve }),
                            toast_title: title,
                            toast_ok: ok_msg,
                            toast_empty: None,
                            toast_err: err_msg,
                        });
                    }
                    BridgeBackend::InProcess { app_state, .. } => {
                        let tasks_on = {
                            let mut prefs = app_state.prefs.lock().unwrap_or_else(|e| e.into_inner());
                            let _ = prefs.set(
                                "phoneApprove",
                                if approve { Some(Value::Bool(true)) } else { None },
                            );
                            prefs
                                .get("phoneTasks")
                                .and_then(Value::as_bool)
                                .unwrap_or(false)
                        };
                        app_state
                            .office
                            .lock()
                            .unwrap_or_else(|e| e.into_inner())
                            .broadcast(json!({ "k": "phone", "approve": approve, "tasks": tasks_on }));
                        ui.phone_modal.phone_approve = approve;
                        ui.push_toast(title, ok_msg);
                    }
                    BridgeBackend::BindError { .. } => {
                        ui.push_toast(title, err_msg);
                        continue;
                    }
                }
            }
            UiAction::UpdatePin { pin } => {
                let title = i18n.tr("topbar.phone.pinTitle");
                let ok_msg = i18n.tr("topbar.phone.pinSaved");
                let weak_msg = i18n.tr("station.pinWeak");
                let err_msg = i18n.tr("drawer.unreachable");
                match &bridge.backend {
                    BridgeBackend::AttachedHttp { query_tx, .. } => {
                        let body = match pin {
                            Some(p) => json!({ "pin": p.trim() }),
                            None => json!({}),
                        };
                        let _ = query_tx.try_send(RemoteQueryCmd::PhoneMutate {
                            path: "/api/pin",
                            body,
                            toast_title: title,
                            toast_ok: ok_msg,
                            toast_empty: None,
                            toast_err: weak_msg,
                        });
                    }
                    BridgeBackend::InProcess { app_state, .. } => match pin {
                        None => match app_state.auth.regenerate_pin() {
                            Ok(new_pin) => {
                                ui.pairing_pin = new_pin;
                                ui.push_toast(title, ok_msg);
                            }
                            Err(_) => ui.push_toast(title, err_msg),
                        },
                        Some(raw) => {
                            let trimmed = raw.trim();
                            if station_core::auth::pin_weak(trimmed) {
                                ui.push_toast(title, weak_msg);
                            } else if app_state.auth.set_pin(trimmed).unwrap_or(false) {
                                ui.pairing_pin = app_state.auth.current_pin();
                                ui.push_toast(title, ok_msg);
                            } else {
                                ui.push_toast(title, err_msg);
                            }
                        }
                    },
                    BridgeBackend::BindError { .. } => {
                        ui.push_toast(title, err_msg);
                        continue;
                    }
                }
            }
            UiAction::TestWebPush => {
                let title = i18n.tr("server.push.testTitle");
                let body_txt = i18n.tr("server.push.testBody");
                let ok_msg = i18n.tr("topbar.phone.notifySent");
                let empty_msg = i18n.tr("topbar.phone.pushNone");
                let err_msg = i18n.tr("drawer.unreachable");
                match &bridge.backend {
                    BridgeBackend::AttachedHttp { query_tx, .. } => {
                        let _ = query_tx.try_send(RemoteQueryCmd::PhoneMutate {
                            path: "/api/push/test",
                            body: json!({ "title": title, "body": body_txt }),
                            toast_title: title,
                            toast_ok: ok_msg,
                            toast_empty: Some(empty_msg),
                            toast_err: err_msg,
                        });
                    }
                    BridgeBackend::InProcess {
                        app_state,
                        runtime_handle,
                        ..
                    } => {
                        let msg = station_core::store::push::PushMessage {
                            title: title.clone(),
                            body: body_txt,
                            tag: Some("station-test".to_owned()),
                            url: "/phone".to_owned(),
                            urgent: false,
                        };
                        let deliveries = app_state
                            .push
                            .lock()
                            .unwrap_or_else(|e| e.into_inner())
                            .prepare_deliveries(&msg, now_ms() / 1000);
                        if deliveries.is_empty() {
                            ui.push_toast(title, empty_msg);
                        } else if let Some(rt) = runtime_handle.clone() {
                            let state = Arc::clone(app_state);
                            let toast_tx = bridge.toast_tx.clone();
                            rt.spawn(async move {
                                let dead = station_core::store::push::deliver_prepared(deliveries).await;
                                if !dead.is_empty()
                                    && let Ok(mut push) = state.push.lock()
                                {
                                    for ep in dead {
                                        push.remove(&ep);
                                    }
                                }
                                let _ = toast_tx.try_send((title, ok_msg));
                            });
                        } else {
                            ui.push_toast(title, ok_msg);
                        }
                    }
                    BridgeBackend::BindError { .. } => {
                        ui.push_toast(title, err_msg);
                        continue;
                    }
                }
            }
            UiAction::RefreshTelemetry => {
                bridge.sys_refresh_due = true;
                let sync_dur = bridge.sync_timer.duration();
                bridge.sync_timer.set_elapsed(sync_dur);
                match &bridge.backend {
                    BridgeBackend::AttachedHttp { action_tx, .. } => {
                        let _ = action_tx.try_send(RemoteActionCmd {
                            path: "",
                            body: Value::Null,
                            refresh: RefreshKind::All,
                            launch_toast_label: None,
                        });
                    }
                    BridgeBackend::InProcess { harness_poller, .. }
                    | BridgeBackend::BindError { harness_poller, .. } => {
                        harness_poller.trigger_refresh();
                    }
                }
            }
        }
    }

    // Execute at most ONE world transition per frame so deferred Commands never collide.
    match pending_op {
        PendingWorldOp::None => {}
        PendingWorldOp::UnloadForTray => {
            if has_world {
                unload_world_if_possible(&mut commands, &mut world_params);
            }
        }
        PendingWorldOp::UnloadToLite => {
            if has_world {
                unload_world_if_possible(&mut commands, &mut world_params);
            }
            if !has_lite_cam {
                station_3d::camera::spawn_lite_2d_camera(&mut commands);
            }
            world_params.clear_color.0 = Color::srgb(0.06, 0.075, 0.105);
        }
        PendingWorldOp::EnsureMode3d => {
            if !has_world {
                rebuild_3d_office_world(
                    &mut commands,
                    &mut world_params,
                    &bridge.assets_dir,
                    &ui,
                    false,
                );
            } else {
                for ent in &world_params.lite_cam_q {
                    commands.entity(ent).try_despawn();
                }
            }
        }
        PendingWorldOp::Rebuild3d => {
            rebuild_3d_office_world(
                &mut commands,
                &mut world_params,
                &bridge.assets_dir,
                &ui,
                has_world,
            );
        }
        PendingWorldOp::SwapBackdrop(bk) => {
            if let (Some(p), Some(ba), Some(mats), Some(imgs), Some(shared_fonts)) = (
                world_params.pal.as_deref_mut(),
                world_params.backdrop_assets.as_deref_mut(),
                world_params.materials.as_deref_mut(),
                world_params.images.as_deref_mut(),
                world_params.shared_fonts.as_deref_mut(),
            ) {
                station_3d::backdrops::unload_backdrop(
                    &mut commands,
                    &world_params.backdrop_q,
                    ba,
                    mats,
                    imgs,
                );
                let spec = station_3d::LayoutSpec::for_layout(ui.layout);
                let mut brands = BrandRegistry::new(bridge.assets_dir.clone());
                let fonts = shared_fonts.get_or_init();
                station_3d::backdrops::spawn_backdrop(
                    &mut commands,
                    p,
                    mats,
                    imgs,
                    ba,
                    &mut brands,
                    fonts,
                    &spec,
                    bk,
                );
                brands.clear();
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use station_core::state::sessions::{FeedItem, OfficeState, PendingPerm};
    use std::net::TcpListener;
    use std::time::{Duration, Instant};

    #[test]
    fn probe_local_health_detects_station_server_and_rejects_other_listeners() {
        let rt = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .unwrap();
        let tmp = tempfile::tempdir().unwrap();
        let state = Arc::new(AppState::new(tmp.path().to_path_buf(), None).unwrap());
        let handle = rt.block_on(start_http_server(state, "127.0.0.1", 0)).unwrap();
        let ok_port = handle.addr.port();

        let (stop_tx, stop_rx) = std::sync::mpsc::channel::<()>();
        let server_thread = std::thread::spawn(move || {
            let _ = rt.block_on(async move {
                let _ = tokio::task::spawn_blocking(move || {
                    let _ = stop_rx.recv();
                })
                .await;
                let _ = handle.shutdown.send(true);
            });
        });

        assert!(probe_local_health(ok_port));

        let raw = TcpListener::bind("127.0.0.1:0").unwrap();
        let raw_port = raw.local_addr().unwrap().port();
        let raw_thread = std::thread::spawn(move || {
            if let Ok((stream, _)) = raw.accept() {
                drop(stream);
            }
        });
        assert!(!probe_local_health(raw_port));

        let _ = raw_thread.join();
        let _ = stop_tx.send(());
        let _ = server_thread.join();
    }

    #[test]
    fn parsers_round_trip_real_office_state_snapshot_with_perm_and_error_feed() {
        let tmp = tempfile::tempdir().unwrap();
        let mut office = OfficeState::new(tmp.path().to_path_buf());
        office.pending_perms.insert(
            "perm-99".to_owned(),
            PendingPerm {
                id: "perm-99".to_owned(),
                sess: "sess-1".to_owned(),
                name: "agents-station".to_owned(),
                kind: "permission".to_owned(),
                tool_type: "Bash".to_owned(),
                label: "cargo test".to_owned(),
                detail: "cargo test -p station-app".to_owned(),
                danger: None,
                created_at: 1_700_000_000_000,
            },
        );
        office.feed.push_front(FeedItem {
            id: 1,
            ts: 1_700_000_000_000,
            sess: "sess-1".to_owned(),
            name: "codex".to_owned(),
            kind: "error".to_owned(),
            label: "command failed".to_owned(),
            desc: None,
            image: None,
        });

        let snap = office.snapshot_json(true, "en");
        let perms = parse_perms_from_json(&snap);
        assert_eq!(perms.len(), 1);
        assert_eq!(perms[0].id, "perm-99");
        assert_eq!(perms[0].tool, "Bash");
        assert_eq!(perms[0].summary, "cargo test -p station-app");

        let feed = parse_feed_from_json(&snap);
        assert_eq!(feed.len(), 1);
        assert_eq!(feed[0].agent, "codex");
        assert_eq!(feed[0].level, "err");
        assert_eq!(feed[0].message, "command failed");
    }

    #[test]
    fn attached_snapshot_clears_resolved_perms_and_ended_sessions_when_empty() {
        let mut ui = UiState::with_demo_seed(
            station_3d::OfficeLayout::Pro,
            station_3d::BackdropKind::City,
            false,
        );
        assert!(!ui.sessions.is_empty());
        assert!(!ui.pending_perms.is_empty());

        let populated = RemoteSnapshot {
            has_snapshot: true,
            sessions: vec![SessionCard {
                id: "s-1".to_owned(),
                agent: "claude".to_owned(),
                project: "proj".to_owned(),
                status: "work".to_owned(),
                model: "opus".to_owned(),
                last_prompt: "hi".to_owned(),
                tool: None,
                tool_kind: None,
                desk: 0,
                cost_usd: 0.0,
                tokens: 0,
                pinned: false,
                ..Default::default()
            }],
            pending_perms: vec![PendingPermCard {
                id: "p-1".to_owned(),
                session_id: "s-1".to_owned(),
                agent: "claude".to_owned(),
                tool: "Bash".to_owned(),
                summary: "echo hi".to_owned(),
                kind: "permission".to_owned(),
                ..Default::default()
            }],
            ..RemoteSnapshot::default()
        };
        apply_remote_snapshot_to_ui(&populated, &mut ui);
        assert_eq!(ui.sessions.len(), 1);
        assert_eq!(ui.pending_perms.len(), 1);

        let cleared = RemoteSnapshot {
            has_snapshot: true,
            ..RemoteSnapshot::default()
        };
        apply_remote_snapshot_to_ui(&cleared, &mut ui);
        assert!(ui.sessions.is_empty(), "ended sessions must clear from UI");
        assert!(ui.pending_perms.is_empty(), "resolved perms must clear from UI");
        assert!(ui.feed.is_empty(), "empty feed must clear from UI");
    }

    #[test]
    fn sse_byte_decoder_handles_split_multibyte_utf8_and_caps_buffer() {
        let full = "data: {\"msg\":\"Tiếng Việt · ภาษาไทย\"}\r\n";
        let raw = full.as_bytes();
        // Split right inside the 3-byte UTF-8 sequence for 'ế' (E1 BA BF).
        let split_at = full.find('ế').unwrap() + 1;
        let mut dec = SseByteDecoder::new(1024);

        let first = dec.push_chunk(&raw[..split_at]).unwrap();
        assert!(first.is_empty());
        let second = dec.push_chunk(&raw[split_at..]).unwrap();
        assert_eq!(second, vec!["data: {\"msg\":\"Tiếng Việt · ภาษาไทย\"}"]);

        // Overflow guard caps oversized unterminated lines and clears buffer.
        let mut tiny = SseByteDecoder::new(16);
        assert!(tiny.push_chunk(b"0123456789abcdefX").is_err());
        assert_eq!(tiny.push_chunk(b"ok\n").unwrap(), vec!["ok"]);
    }

    #[test]
    fn bridge_attaches_via_sse_when_healthy_and_surfaces_bind_error_on_conflict() {
        let rt = tokio::runtime::Builder::new_multi_thread()
            .worker_threads(1)
            .enable_all()
            .build()
            .unwrap();
        let tmp_srv = tempfile::tempdir().unwrap();
        let srv_state = Arc::new(AppState::new(tmp_srv.path().to_path_buf(), None).unwrap());
        {
            let mut office = srv_state.office.lock().unwrap();
            office.ingest_raw_event(json!({
                "session_id": "s-attach",
                "hook_event_name": "SessionStart",
                "cwd": "D:/mystation/agents-station"
            }));
            office.pending_perms.insert(
                "perm-live".to_owned(),
                PendingPerm {
                    id: "perm-live".to_owned(),
                    sess: "s-attach".to_owned(),
                    name: "agents-station".to_owned(),
                    kind: "permission".to_owned(),
                    tool_type: "Bash".to_owned(),
                    label: "test".to_owned(),
                    detail: "cargo test".to_owned(),
                    danger: None,
                    created_at: 1_700_000_000_000,
                },
            );
        }
        let handle = rt
            .block_on(start_http_server(srv_state, "127.0.0.1", 0))
            .unwrap();
        let srv_port = handle.addr.port();

        let tmp_client = tempfile::tempdir().unwrap();
        let attached = LiveServerBridge::connect_or_start(
            tmp_client.path().to_path_buf(),
            PathBuf::from("assets"),
            "127.0.0.1".to_owned(),
            srv_port,
            true,
            true,
        );
        assert_eq!(attached.mode(), BridgeMode::AttachedHttp);
        assert!(attached.bind_error_message().is_none());

        let deadline = Instant::now() + Duration::from_secs(3);
        let mut saw_session_and_perm = false;
        while Instant::now() < deadline {
            if attached.remote_sessions().iter().any(|s| s.id == "s-attach")
                && attached
                    .remote_perms()
                    .iter()
                    .any(|p| p.id == "perm-live" && p.tool == "Bash")
            {
                saw_session_and_perm = true;
                break;
            }
            std::thread::sleep(Duration::from_millis(25));
        }
        assert!(
            saw_session_and_perm,
            "attached SSE bridge must ingest live session & permission snapshot from running daemon"
        );

        // Resolving permission over pooled action channel triggers /api/live refresh & clears perm.
        assert!(attached.enqueue_remote_action(
            "/api/perm",
            json!({ "id": "perm-live", "decision": "allow" }),
            RefreshKind::Live,
        ));
        let clear_deadline = Instant::now() + Duration::from_secs(3);
        let mut perm_cleared = false;
        while Instant::now() < clear_deadline {
            if attached.remote_perms().is_empty() {
                perm_cleared = true;
                break;
            }
            std::thread::sleep(Duration::from_millis(25));
        }
        assert!(
            perm_cleared,
            "post-POST /api/live refresh must clear resolved permission from attached snapshot"
        );

        drop(attached);
        let _ = handle.shutdown.send(true);
        drop(rt);

        let _blocker = TcpListener::bind("127.0.0.1:0").unwrap();
        let blocked_port = _blocker.local_addr().unwrap().port();

        let tmp_err = tempfile::tempdir().unwrap();
        let errored = LiveServerBridge::connect_or_start(
            tmp_err.path().to_path_buf(),
            PathBuf::from("assets"),
            "127.0.0.1".to_owned(),
            blocked_port,
            true,
            true,
        );
        assert_eq!(errored.mode(), BridgeMode::BindError);
        let msg = errored
            .bind_error_message()
            .expect("bind error must expose visible banner message");
        assert!(msg.contains(&blocked_port.to_string()));
        drop(_blocker);
    }

    #[test]
    fn view_mode_3d_lite_3d_layout_hq_chibi_look_and_3x_backdrop_switch() {
        use bevy::input::mouse::{MouseMotion, MouseWheel};
        use station_3d::{BackdropKind, ChibiLook, Lite2dCamera, OfficeLayout, SharedFontEngine};

        let assets_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../assets");
        let tmp = tempfile::tempdir().unwrap();

        let mut app = App::new();
        app.insert_resource(Assets::<Mesh>::default())
            .insert_resource(Assets::<StandardMaterial>::default())
            .insert_resource(Assets::<Image>::default())
            .insert_resource(ClearColor::default())
            .insert_resource(Time::<()>::default())
            .insert_resource(ButtonInput::<KeyCode>::default())
            .insert_resource(ButtonInput::<MouseButton>::default())
            .add_event::<MouseMotion>()
            .add_event::<MouseWheel>()
            .add_event::<AppExit>()
            .add_plugins(station_3d::OfficeWorldPlugin {
                layout: OfficeLayout::Pro,
                backdrop: BackdropKind::City,
                assets_dir: assets_dir.clone(),
                night: false,
            })
            .insert_resource(UiState::with_demo_seed(
                OfficeLayout::Pro,
                BackdropKind::City,
                false,
            ))
            .insert_resource(I18nStore::load_from_assets(&assets_dir, "en"))
            .insert_resource(LiveServerBridge::connect_or_start(
                tmp.path().to_path_buf(),
                assets_dir,
                "127.0.0.1".to_owned(),
                0,
                false,
                false,
            ))
            .add_systems(Update, process_ui_actions);

        // Initial 3D Pro state.
        app.update();
        assert_eq!(
            app.world_mut()
                .query_filtered::<Entity, With<Camera3d>>()
                .iter(app.world())
                .count(),
            1
        );
        assert_eq!(
            app.world_mut()
                .query_filtered::<Entity, With<station_3d::camera::MainOfficeCamera>>()
                .iter(app.world())
                .count(),
            1
        );
        assert_eq!(
            app.world_mut()
                .query_filtered::<Entity, With<Lite2dCamera>>()
                .iter(app.world())
                .count(),
            0
        );
        assert_eq!(station_3d::desks::count_desks_in_world(app.world_mut()), 12);
        assert_eq!(app.world().resource::<SharedFontEngine>().load_count(), 1);

        // 1. 3D -> Lite2D: unload 3D world and spawn Lite2dCamera (never tagged WorldEntity).
        {
            let mut ui = app.world_mut().resource_mut::<UiState>();
            ui.view_mode = ViewMode::Lite2D;
            ui.push_action(UiAction::SetViewMode(ViewMode::Lite2D));
        }
        app.update();

        assert_eq!(
            app.world_mut()
                .query_filtered::<Entity, With<WorldEntity>>()
                .iter(app.world())
                .count(),
            0,
            "Lite2D must despawn all WorldEntity entities"
        );
        assert_eq!(
            app.world_mut()
                .query_filtered::<Entity, With<Camera3d>>()
                .iter(app.world())
                .count(),
            1,
            "Lite2D must keep exactly 1 active Camera3d (Lite2dCamera)"
        );
        assert_eq!(
            app.world_mut()
                .query_filtered::<Entity, With<Lite2dCamera>>()
                .iter(app.world())
                .count(),
            1
        );
        assert_eq!(app.world().resource::<Assets<Mesh>>().len(), 0);
        assert_eq!(app.world().resource::<Assets<StandardMaterial>>().len(), 0);
        assert_eq!(app.world().resource::<Assets<Image>>().len(), 0);

        // 2. Lite2D -> Mode3D: despawn Lite2dCamera and rebuild 3D world.
        {
            let mut ui = app.world_mut().resource_mut::<UiState>();
            ui.view_mode = ViewMode::Mode3D;
            ui.push_action(UiAction::SetViewMode(ViewMode::Mode3D));
        }
        app.update();

        assert_eq!(
            app.world_mut()
                .query_filtered::<Entity, With<Camera3d>>()
                .iter(app.world())
                .count(),
            1
        );
        assert_eq!(
            app.world_mut()
                .query_filtered::<Entity, With<station_3d::camera::MainOfficeCamera>>()
                .iter(app.world())
                .count(),
            1
        );
        assert_eq!(
            app.world_mut()
                .query_filtered::<Entity, With<Lite2dCamera>>()
                .iter(app.world())
                .count(),
            0
        );
        assert!(
            app.world_mut()
                .query_filtered::<Entity, With<WorldEntity>>()
                .iter(app.world())
                .count()
                > 50
        );
        assert_eq!(station_3d::desks::count_desks_in_world(app.world_mut()), 12);

        // 3. SwitchLayout(Hq): rebuild 44-desk HQ layout with exactly 1 active camera.
        {
            let mut ui = app.world_mut().resource_mut::<UiState>();
            ui.layout = OfficeLayout::Hq;
            ui.push_action(UiAction::SwitchLayout(OfficeLayout::Hq));
        }
        app.update();

        assert_eq!(
            app.world_mut()
                .query_filtered::<Entity, With<Camera3d>>()
                .iter(app.world())
                .count(),
            1
        );
        assert_eq!(station_3d::desks::count_desks_in_world(app.world_mut()), 44);

        // 4. SetChibiLook(Toy): rebuild cleanly with 1 active camera.
        {
            let mut ui = app.world_mut().resource_mut::<UiState>();
            ui.chibi_look = ChibiLook::Toy;
            ui.push_action(UiAction::SetChibiLook(ChibiLook::Toy));
        }
        app.update();
        assert_eq!(
            app.world_mut()
                .query_filtered::<Entity, With<Camera3d>>()
                .iter(app.world())
                .count(),
            1
        );
        assert_eq!(station_3d::desks::count_desks_in_world(app.world_mut()), 44);

        // 5. 3x backdrop switch: 0 leaked StandardMaterials, 0 leaked Images, 1 FontEngine load total.
        let city_mats = app.world().resource::<Assets<StandardMaterial>>().len();
        let city_imgs = app.world().resource::<Assets<Image>>().len();
        for _ in 0..3 {
            for bk in [BackdropKind::Countryside, BackdropKind::Beach, BackdropKind::City] {
                {
                    let mut ui = app.world_mut().resource_mut::<UiState>();
                    ui.backdrop = bk;
                    ui.push_action(UiAction::SwitchBackdrop(bk));
                }
                app.update();
            }
            assert_eq!(
                app.world().resource::<Assets<StandardMaterial>>().len(),
                city_mats,
                "3x backdrop switch must leak 0 StandardMaterials"
            );
            assert_eq!(
                app.world().resource::<Assets<Image>>().len(),
                city_imgs,
                "3x backdrop switch must leak 0 Images"
            );
        }
        assert_eq!(
            app.world().resource::<SharedFontEngine>().load_count(),
            1,
            "SharedFontEngine must never reload 24 TTF files on layout/backdrop switches"
        );
    }

    #[test]
    fn lite_startup_switches_to_3d_cleanly() {
        use bevy::input::mouse::{MouseMotion, MouseWheel};
        use station_3d::{BackdropKind, Lite2dCamera, OfficeLayout, SharedFontEngine};

        let assets_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../assets");
        let tmp = tempfile::tempdir().unwrap();

        let mut ui = UiState::with_demo_seed(OfficeLayout::Pro, BackdropKind::City, false);
        ui.view_mode = ViewMode::Lite2D;

        let mut app = App::new();
        app.insert_resource(Assets::<Mesh>::default())
            .insert_resource(Assets::<StandardMaterial>::default())
            .insert_resource(Assets::<Image>::default())
            .insert_resource(ClearColor::default())
            .insert_resource(Time::<()>::default())
            .insert_resource(ButtonInput::<KeyCode>::default())
            .insert_resource(ButtonInput::<MouseButton>::default())
            .add_event::<MouseMotion>()
            .add_event::<MouseWheel>()
            .add_event::<AppExit>()
            .add_plugins(station_3d::OfficeWorldRuntimePlugin {
                assets_dir: assets_dir.clone(),
            })
            .add_systems(Startup, |mut commands: Commands| {
                station_3d::camera::spawn_lite_2d_camera(&mut commands);
            })
            .insert_resource(ui)
            .insert_resource(I18nStore::load_from_assets(&assets_dir, "en"))
            .insert_resource(LiveServerBridge::connect_or_start(
                tmp.path().to_path_buf(),
                assets_dir,
                "127.0.0.1".to_owned(),
                0,
                false,
                false,
            ))
            .add_systems(Update, process_ui_actions);

        app.update();
        assert_eq!(
            app.world_mut()
                .query_filtered::<Entity, With<Lite2dCamera>>()
                .iter(app.world())
                .count(),
            1
        );
        assert_eq!(
            app.world_mut()
                .query_filtered::<Entity, With<WorldEntity>>()
                .iter(app.world())
                .count(),
            0
        );
        assert_eq!(app.world().resource::<SharedFontEngine>().load_count(), 0);

        {
            let mut ui = app.world_mut().resource_mut::<UiState>();
            ui.view_mode = ViewMode::Mode3D;
            ui.push_action(UiAction::SetViewMode(ViewMode::Mode3D));
        }
        app.update();

        assert_eq!(
            app.world_mut()
                .query_filtered::<Entity, With<Lite2dCamera>>()
                .iter(app.world())
                .count(),
            0
        );
        assert_eq!(
            app.world_mut()
                .query_filtered::<Entity, With<Camera3d>>()
                .iter(app.world())
                .count(),
            1
        );
        assert_eq!(station_3d::desks::count_desks_in_world(app.world_mut()), 12);
        assert_eq!(app.world().resource::<SharedFontEngine>().load_count(), 1);
    }

    #[test]
    fn same_frame_multi_action_batch_coalesces_to_single_camera_and_zero_leaks() {
        use bevy::input::mouse::{MouseMotion, MouseWheel};
        use station_3d::{BackdropKind, Lite2dCamera, OfficeLayout, SharedFontEngine};

        let assets_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../assets");
        let tmp = tempfile::tempdir().unwrap();

        let mut app = App::new();
        app.insert_resource(Assets::<Mesh>::default())
            .insert_resource(Assets::<StandardMaterial>::default())
            .insert_resource(Assets::<Image>::default())
            .insert_resource(ClearColor::default())
            .insert_resource(Time::<()>::default())
            .insert_resource(ButtonInput::<KeyCode>::default())
            .insert_resource(ButtonInput::<MouseButton>::default())
            .add_event::<MouseMotion>()
            .add_event::<MouseWheel>()
            .add_event::<AppExit>()
            .add_plugins(station_3d::OfficeWorldPlugin {
                layout: OfficeLayout::Pro,
                backdrop: BackdropKind::City,
                assets_dir: assets_dir.clone(),
                night: false,
            })
            .insert_resource(UiState::with_demo_seed(
                OfficeLayout::Pro,
                BackdropKind::City,
                false,
            ))
            .insert_resource(I18nStore::load_from_assets(&assets_dir, "en"))
            .insert_resource(LiveServerBridge::connect_or_start(
                tmp.path().to_path_buf(),
                assets_dir,
                "127.0.0.1".to_owned(),
                0,
                false,
                false,
            ))
            .add_systems(Update, process_ui_actions);

        app.update();

        // 1. Single frame: Lite2D -> Mode3D -> SwitchLayout(Hq) -> SwitchBackdrop(Beach).
        {
            let mut ui = app.world_mut().resource_mut::<UiState>();
            ui.push_action(UiAction::SetViewMode(ViewMode::Lite2D));
            ui.push_action(UiAction::SetViewMode(ViewMode::Mode3D));
            ui.push_action(UiAction::SwitchLayout(OfficeLayout::Hq));
            ui.push_action(UiAction::SwitchBackdrop(BackdropKind::Beach));
        }
        app.update();

        assert_eq!(
            app.world_mut()
                .query_filtered::<Entity, With<Camera3d>>()
                .iter(app.world())
                .count(),
            1,
            "same-frame multi-action batch must leave exactly 1 active Camera3d"
        );
        assert_eq!(
            app.world_mut()
                .query_filtered::<Entity, With<Lite2dCamera>>()
                .iter(app.world())
                .count(),
            0
        );
        assert!(
            app.world_mut()
                .query_filtered::<Entity, With<WorldEntity>>()
                .iter(app.world())
                .count()
                > 50
        );
        assert_eq!(station_3d::desks::count_desks_in_world(app.world_mut()), 44);
        assert_eq!(
            app.world().resource::<BackdropAssets>().active,
            Some(BackdropKind::Beach)
        );
        assert_eq!(app.world().resource::<SharedFontEngine>().load_count(), 1);

        // 2. Single frame multiple backdrop swaps: leak 0 materials and 0 images.
        {
            let mut ui = app.world_mut().resource_mut::<UiState>();
            ui.push_action(UiAction::SwitchBackdrop(BackdropKind::City));
        }
        app.update();
        let city_mats = app.world().resource::<Assets<StandardMaterial>>().len();
        let city_imgs = app.world().resource::<Assets<Image>>().len();

        {
            let mut ui = app.world_mut().resource_mut::<UiState>();
            ui.push_action(UiAction::SwitchBackdrop(BackdropKind::Countryside));
            ui.push_action(UiAction::SwitchBackdrop(BackdropKind::Beach));
            ui.push_action(UiAction::SwitchBackdrop(BackdropKind::City));
        }
        app.update();
        assert_eq!(
            app.world().resource::<Assets<StandardMaterial>>().len(),
            city_mats,
            "same-frame multi-backdrop batch must leak 0 StandardMaterials"
        );
        assert_eq!(
            app.world().resource::<Assets<Image>>().len(),
            city_imgs,
            "same-frame multi-backdrop batch must leak 0 Images"
        );

        // 3. Single frame Lite2D + SwitchBackdrop must stay unloaded with 1 Lite2dCamera and 0 assets.
        {
            let mut ui = app.world_mut().resource_mut::<UiState>();
            ui.push_action(UiAction::SetViewMode(ViewMode::Lite2D));
            ui.push_action(UiAction::SwitchBackdrop(BackdropKind::Beach));
        }
        app.update();
        assert_eq!(
            app.world_mut()
                .query_filtered::<Entity, With<WorldEntity>>()
                .iter(app.world())
                .count(),
            0
        );
        assert_eq!(
            app.world_mut()
                .query_filtered::<Entity, With<Camera3d>>()
                .iter(app.world())
                .count(),
            1
        );
        assert_eq!(
            app.world_mut()
                .query_filtered::<Entity, With<Lite2dCamera>>()
                .iter(app.world())
                .count(),
            1
        );
        assert_eq!(app.world().resource::<Assets<Mesh>>().len(), 0);
        assert_eq!(app.world().resource::<Assets<StandardMaterial>>().len(), 0);
        assert_eq!(app.world().resource::<Assets<Image>>().len(), 0);
    }

    #[test]
    fn ui_action_decide_permission_resolves_in_process_oneshot_hold() {
        let assets_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../assets");
        let tmp = tempfile::tempdir().unwrap();
        let bridge = LiveServerBridge::connect_or_start(
            tmp.path().to_path_buf(),
            assets_dir.clone(),
            "127.0.0.1".to_owned(),
            0,
            false,
            false,
        );
        let state = bridge.in_process_state().expect("expected InProcess bridge");
        let (tx, mut rx) = tokio::sync::oneshot::channel();
        {
            let mut office = state.office.lock().unwrap();
            office.register_hold(
                PendingPerm {
                    id: "perm-ui-1".to_owned(),
                    sess: "s1".to_owned(),
                    name: "proj".to_owned(),
                    kind: "perm".to_owned(),
                    tool_type: "bash".to_owned(),
                    label: "cargo test".to_owned(),
                    detail: "cargo test".to_owned(),
                    danger: None,
                    created_at: 1,
                },
                tx,
            );
        }

        let mut app = App::new();
        app.insert_resource(ClearColor::default())
            .insert_resource(UiState::default())
            .insert_resource(I18nStore::load_from_assets(&assets_dir, "en"))
            .insert_resource(bridge)
            .add_event::<AppExit>()
            .add_systems(Update, process_ui_actions);

        {
            let mut ui = app.world_mut().resource_mut::<UiState>();
            ui.push_action(UiAction::DecidePermission {
                id: "perm-ui-1".to_owned(),
                decision: "allow".to_owned(),
            });
        }
        app.update();

        let resolved = rx
            .try_recv()
            .expect("UiAction::DecidePermission must resolve the pending oneshot hold");
        assert_eq!(resolved["hookSpecificOutput"]["permissionDecision"], "allow");
        assert_eq!(
            resolved["hookSpecificOutput"]["decision"]["behavior"],
            "allow"
        );
        assert!(state.office.lock().unwrap().pending_perms.is_empty());
    }

    #[test]
    fn unfocused_new_pending_perm_triggers_tray_alert_once_per_id() {
        let assets_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../assets");
        let tmp = tempfile::tempdir().unwrap();
        let bridge = LiveServerBridge::connect_or_start(
            tmp.path().to_path_buf(),
            assets_dir,
            "127.0.0.1".to_owned(),
            0,
            false,
            false,
        );
        let state = bridge.in_process_state().expect("expected InProcess bridge");
        let (tx, _rx) = tokio::sync::oneshot::channel();
        {
            let mut office = state.office.lock().unwrap();
            office.register_hold(
                PendingPerm {
                    id: "perm-alert-1".to_owned(),
                    sess: "s1".to_owned(),
                    name: "proj".to_owned(),
                    kind: "perm".to_owned(),
                    tool_type: "bash".to_owned(),
                    label: "cmd".to_owned(),
                    detail: "echo alert".to_owned(),
                    danger: None,
                    created_at: 1,
                },
                tx,
            );
        }

        let mut app = App::new();
        let mut time = Time::<()>::default();
        time.advance_by(Duration::from_secs(2));
        app.insert_resource(time)
            .insert_resource(UiState::default())
            .insert_resource(TrayState::default())
            .insert_resource(bridge)
            .add_systems(Update, sync_live_state_to_ui);

        // Spawn an unfocused window so window_focused == false.
        let mut win = Window::default();
        win.focused = false;
        app.world_mut().spawn(win);

        // Tick 1: new pending perm arrives while unfocused -> triggers 1 tray alert.
        app.update();
        assert_eq!(app.world().resource::<TrayState>().notifications_sent, 1);

        // Tick 2: same perm still pending -> must NOT spam duplicate alerts.
        app.world_mut()
            .resource_mut::<Time>()
            .advance_by(Duration::from_secs(2));
        app.update();
        assert_eq!(app.world().resource::<TrayState>().notifications_sent, 1);
    }

    #[test]
    fn unknown_ui_perm_decision_fails_closed_without_resolving_hold() {
        let assets_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../assets");
        let tmp = tempfile::tempdir().unwrap();
        let bridge = LiveServerBridge::connect_or_start(
            tmp.path().to_path_buf(),
            assets_dir.clone(),
            "127.0.0.1".to_owned(),
            0,
            false,
            false,
        );
        let state = bridge.in_process_state().expect("expected InProcess bridge");
        let (tx, mut rx) = tokio::sync::oneshot::channel();
        {
            let mut office = state.office.lock().unwrap();
            office.register_hold(
                PendingPerm {
                    id: "perm-failclosed".to_owned(),
                    sess: "s1".to_owned(),
                    name: "proj".to_owned(),
                    kind: "perm".to_owned(),
                    tool_type: "bash".to_owned(),
                    label: "rm".to_owned(),
                    detail: "rm -rf /".to_owned(),
                    danger: Some("rmRf".to_owned()),
                    created_at: 1,
                },
                tx,
            );
        }

        let mut app = App::new();
        app.insert_resource(ClearColor::default())
            .insert_resource(UiState::default())
            .insert_resource(I18nStore::load_from_assets(&assets_dir, "en"))
            .insert_resource(bridge)
            .add_event::<AppExit>()
            .add_systems(Update, process_ui_actions);

        {
            let mut ui = app.world_mut().resource_mut::<UiState>();
            ui.push_action(UiAction::DecidePermission {
                id: "perm-failclosed".to_owned(),
                decision: "unknown_typo".to_owned(),
            });
        }
        app.update();

        assert!(
            rx.try_recv().is_err(),
            "unknown UI decision string must fail closed and NOT resolve the hold"
        );
        assert!(
            state
                .office
                .lock()
                .unwrap()
                .pending_perms
                .contains_key("perm-failclosed")
        );
    }

    #[test]
    fn minimize_to_tray_resets_presence_focused_in_process() {
        let assets_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../assets");
        let tmp = tempfile::tempdir().unwrap();
        let bridge = LiveServerBridge::connect_or_start(
            tmp.path().to_path_buf(),
            assets_dir.clone(),
            "127.0.0.1".to_owned(),
            0,
            false,
            false,
        );
        let state = bridge.in_process_state().expect("expected InProcess bridge");
        state.office.lock().unwrap().presence_focused = true;

        let mut app = App::new();
        app.insert_resource(ClearColor::default())
            .insert_resource(UiState::default())
            .insert_resource(I18nStore::load_from_assets(&assets_dir, "en"))
            .insert_resource(TrayState::default())
            .insert_resource(bridge)
            .add_event::<AppExit>()
            .add_systems(Update, process_ui_actions);

        {
            let mut ui = app.world_mut().resource_mut::<UiState>();
            ui.push_action(UiAction::MinimizeToTray);
        }
        app.update();

        assert!(
            !state.office.lock().unwrap().presence_focused,
            "MinimizeToTray must reset presence_focused = false"
        );
    }

    #[test]
    fn post_presence_unfocused_sync_clears_presence_on_live_http_daemon() {
        let rt = tokio::runtime::Builder::new_multi_thread()
            .worker_threads(1)
            .enable_all()
            .build()
            .unwrap();
        let tmp = tempfile::tempdir().unwrap();
        let srv_state = Arc::new(AppState::new(tmp.path().to_path_buf(), None).unwrap());
        srv_state.office.lock().unwrap().presence_focused = true;
        let handle = rt
            .block_on(start_http_server(srv_state.clone(), "127.0.0.1", 0))
            .unwrap();
        let port = handle.addr.port();

        assert!(post_presence_unfocused_sync(port));
        assert!(
            !srv_state.office.lock().unwrap().presence_focused,
            "sync loopback POST /api/presence must clear presence_focused on running daemon"
        );

        let _ = handle.shutdown.send(true);
        drop(rt);
    }

    #[test]
    fn full_attached_action_queue_does_not_cache_last_window_focused() {
        let (action_tx, _action_rx) = mpsc::channel::<RemoteActionCmd>(1);
        action_tx
            .try_send(RemoteActionCmd {
                path: "/api/prefs",
                body: json!({}),
                refresh: RefreshKind::None,
                launch_toast_label: None,
            })
            .unwrap();
        let (shutdown, _rx) = watch::channel(false);
        let (toast_tx, toast_rx) = mpsc::channel(TOAST_QUEUE_CAP);
        let (async_tx, async_rx) = mpsc::channel(TOAST_QUEUE_CAP);
        let (query_tx, _query_rx) = mpsc::channel(8);
        let bridge = LiveServerBridge {
            backend: BridgeBackend::AttachedHttp {
                snapshot: Arc::new(Mutex::new(RemoteSnapshot::default())),
                action_tx,
                query_tx,
                _shutdown: shutdown,
            },
            data_dir: PathBuf::from("data"),
            assets_dir: PathBuf::from("assets"),
            port: 4310,
            sync_timer: Timer::from_seconds(1.0, TimerMode::Repeating),
            sys_sync_timer: Timer::from_seconds(4.0, TimerMode::Repeating),
            sys_refresh_due: true,
            banner_pushed: true,
            seen_perm_ids: HashSet::new(),
            dismissed_ci_ids: HashSet::new(),
            last_window_focused: None,
            toast_tx,
            toast_rx,
            async_tx,
            async_rx,
        };

        let mut app = App::new();
        app.insert_resource(Time::<()>::default())
            .insert_resource(UiState::default())
            .insert_resource(bridge)
            .add_systems(Update, sync_live_state_to_ui);
        let mut win = Window::default();
        win.focused = true;
        app.world_mut().spawn(win);

        app.update();
        assert_eq!(
            app.world().resource::<LiveServerBridge>().last_window_focused,
            None,
            "failed try_send must NOT update last_window_focused so presence retries next frame"
        );
    }

    #[test]
    fn background_harness_poller_scans_off_main_thread_and_shuts_down_cleanly() {
        use station_launcher::catalog::{HarnessCatalog, HarnessEntry, LaunchTarget};
        use station_launcher::focus_launch::WindowLauncher;
        use std::collections::HashMap;
        use std::sync::atomic::{AtomicUsize, Ordering};
        use std::thread::ThreadId;

        struct BgTestLauncher {
            scan_count: AtomicUsize,
            last_thread: Mutex<Option<ThreadId>>,
            running: Mutex<HashMap<String, (u32, isize)>>,
        }

        impl WindowLauncher for BgTestLauncher {
            fn snapshot_running_windows(&self) -> HashMap<String, (u32, isize)> {
                self.scan_count.fetch_add(1, Ordering::SeqCst);
                *self.last_thread.lock().unwrap() = Some(std::thread::current().id());
                self.running.lock().unwrap().clone()
            }
            fn focus_window(&self, _hwnd: isize) -> anyhow::Result<()> {
                Ok(())
            }
            fn launch_target(&self, _target: &LaunchTarget) -> anyhow::Result<()> {
                Ok(())
            }
        }

        struct ArcBg(Arc<BgTestLauncher>);
        impl WindowLauncher for ArcBg {
            fn snapshot_running_windows(&self) -> HashMap<String, (u32, isize)> {
                self.0.snapshot_running_windows()
            }
            fn focus_window(&self, hwnd: isize) -> anyhow::Result<()> {
                self.0.focus_window(hwnd)
            }
            fn launch_target(&self, t: &LaunchTarget) -> anyhow::Result<()> {
                self.0.launch_target(t)
            }
        }

        let main_tid = std::thread::current().id();
        let dir = tempfile::tempdir().unwrap();
        let launcher = Arc::new(BgTestLauncher {
            scan_count: AtomicUsize::new(0),
            last_thread: Mutex::new(None),
            running: Mutex::new(HashMap::from([("claude.exe".to_owned(), (777, 0x10))])),
        });
        let builtins = vec![
            HarnessEntry {
                id: "claude".to_owned(),
                label: "Claude".to_owned(),
                hook_agent: None,
                target: Some(LaunchTarget::Exe(PathBuf::from("C:/Apps/Claude.exe"))),
                custom: false,
                process_names: vec!["claude.exe".to_owned()],
            },
            HarnessEntry {
                id: "missing".to_owned(),
                label: "Missing".to_owned(),
                hook_agent: None,
                target: None,
                custom: false,
                process_names: vec![],
            },
        ];
        let svc = Arc::new(HarnessService::with_components(
            HarnessCatalog::load_or_default(dir.path()),
            builtins,
            ArcBg(launcher.clone()),
            PathBuf::from("D:/app/agents-station.exe"),
            4310,
            HashMap::new(),
        ));

        let mut poller = HarnessPoller::spawn(svc, Duration::from_secs(5));
        let deadline = Instant::now() + Duration::from_secs(2);
        let mut latest = None;
        while Instant::now() < deadline {
            if let Some(items) = poller.take_latest() {
                latest = Some(items);
                break;
            }
            std::thread::sleep(Duration::from_millis(10));
        }
        let items = latest.expect("background poller must deliver initial harness snapshot");
        assert_eq!(items.len(), 2);
        assert_eq!(items[0].status, HarnessStatus::RunningHooked);
        assert_eq!(items[1].status, HarnessStatus::NotInstalled);
        assert_ne!(
            launcher.last_thread.lock().unwrap().unwrap(),
            main_tid,
            "scanner must execute off the main Bevy thread"
        );

        // Triggering refresh wakes the thread immediately without waiting for the 5s interval.
        launcher.running.lock().unwrap().clear();
        poller.trigger_refresh();
        let wake_deadline = Instant::now() + Duration::from_secs(1);
        let mut refreshed = None;
        while Instant::now() < wake_deadline {
            if let Some(items) = poller.take_latest() {
                refreshed = Some(items);
                break;
            }
            std::thread::sleep(Duration::from_millis(10));
        }
        let refreshed = refreshed.expect("trigger_refresh must wake poller immediately");
        assert_eq!(refreshed[0].status, HarnessStatus::InstalledHooked);

        // Dropping poller while holding a wakeup handle must exit promptly (< 500ms, not 5s).
        let wakeup_clone = poller.wakeup_handle();
        let drop_start = Instant::now();
        drop(poller);
        assert!(
            drop_start.elapsed() < Duration::from_millis(500),
            "HarnessPoller::drop must wake and join cleanly without waiting for poll interval"
        );
        wakeup_clone.trigger();
    }

    #[test]
    fn launch_harness_outcomes_emit_accurate_toasts_and_parse_remote_json() {
        use station_launcher::LaunchOutcome;

        let (t_focus, b_focus) =
            launch_outcome_toast("Claude", &LaunchOutcome::FocusedExisting { pid: 4242 });
        assert!(t_focus.contains("Focused") && t_focus.contains("Claude"));
        assert!(b_focus.contains("4242"));

        let (t_new, b_new) = launch_outcome_toast("Codex", &LaunchOutcome::LaunchedNew);
        assert!(t_new.contains("Launched") && t_new.contains("Codex"));
        assert!(b_new.contains("Codex"));

        let (t_miss, b_miss) = launch_outcome_toast("ZCode", &LaunchOutcome::NotInstalled);
        assert!(t_miss.contains("not installed") && t_miss.contains("ZCode"));
        assert!(!b_miss.is_empty());

        assert_eq!(
            parse_launch_outcome_from_json(&json!({
                "ok": true,
                "result": { "outcome": "focused_existing", "pid": 9001 }
            })),
            Some(LaunchOutcome::FocusedExisting { pid: 9001 })
        );
        assert_eq!(
            parse_launch_outcome_from_json(&json!({
                "ok": true,
                "result": { "outcome": "launched_new" }
            })),
            Some(LaunchOutcome::LaunchedNew)
        );
        assert_eq!(
            parse_launch_outcome_from_json(&json!({
                "ok": true,
                "result": { "outcome": "not_installed" }
            })),
            Some(LaunchOutcome::NotInstalled)
        );

        // End-to-end: UiAction::LaunchHarness("nonexistent") pushes NotInstalled toast into UiState.
        let assets_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../assets");
        let tmp = tempfile::tempdir().unwrap();
        let bridge = LiveServerBridge::connect_or_start(
            tmp.path().to_path_buf(),
            assets_dir.clone(),
            "127.0.0.1".to_owned(),
            0,
            false,
            false,
        );
        let mut app = App::new();
        app.insert_resource(ClearColor::default())
            .insert_resource(Time::<()>::default())
            .insert_resource(UiState::default())
            .insert_resource(I18nStore::load_from_assets(&assets_dir, "en"))
            .insert_resource(bridge)
            .add_event::<AppExit>()
            .add_systems(Update, (process_ui_actions, sync_live_state_to_ui).chain());

        {
            let mut ui = app.world_mut().resource_mut::<UiState>();
            ui.toasts.clear();
            ui.push_action(UiAction::LaunchHarness("definitely_not_installed".to_owned()));
        }
        let deadline = Instant::now() + Duration::from_secs(2);
        let mut saw_not_installed_toast = false;
        while Instant::now() < deadline {
            app.update();
            if app
                .world()
                .resource::<UiState>()
                .toasts
                .iter()
                .any(|t| t.title.contains("not installed"))
            {
                saw_not_installed_toast = true;
                break;
            }
            std::thread::sleep(Duration::from_millis(15));
        }
        assert!(
            saw_not_installed_toast,
            "launching an uninstalled harness must push an accurate NotInstalled toast into UiState"
        );
    }

    #[test]
    fn attached_http_200_launch_error_emits_error_toast_and_skips_extra_refresh() {
        use std::sync::atomic::{AtomicUsize, Ordering};

        // Direct unit check on response interpreter.
        let (ok, (title, body)) = interpret_remote_launch_response(
            "Claude",
            &json!({ "ok": false, "error": "boom" }),
        );
        assert!(!ok);
        assert_eq!(title, "Failed to open Claude");
        assert_eq!(body, "boom");

        let (bad_ok, (bad_title, _)) =
            interpret_remote_launch_response("Claude", &json!({ "ok": true }));
        assert!(!bad_ok);
        assert_eq!(bad_title, "Failed to open Claude");

        // End-to-end over real loopback HTTP server where POST /api/harnesses/launch returns HTTP 200 {"ok": false, "error": "boom"}.
        struct FailingHarnessApi {
            list_calls: AtomicUsize,
        }
        impl HarnessApi for FailingHarnessApi {
            fn list_json(&self) -> Value {
                self.list_calls.fetch_add(1, Ordering::SeqCst);
                json!({
                    "ok": true,
                    "harnesses": [{
                        "id": "claude",
                        "label": "Claude",
                        "hook_agent": "claude",
                        "status": "installed_hooked",
                        "custom": false
                    }]
                })
            }
            fn launch(&self, _id: &str) -> anyhow::Result<Value> {
                anyhow::bail!("boom")
            }
            fn mutate_custom(&self, _body: Value) -> anyhow::Result<Value> {
                Ok(json!({ "ok": true }))
            }
        }

        let rt = tokio::runtime::Builder::new_multi_thread()
            .worker_threads(1)
            .enable_all()
            .build()
            .unwrap();
        let tmp_srv = tempfile::tempdir().unwrap();
        let api = Arc::new(FailingHarnessApi {
            list_calls: AtomicUsize::new(0),
        });
        let srv_state = Arc::new(
            AppState::new(tmp_srv.path().to_path_buf(), Some(api.clone())).unwrap(),
        );
        let handle = rt
            .block_on(start_http_server(srv_state, "127.0.0.1", 0))
            .unwrap();
        let port = handle.addr.port();

        let assets_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../assets");
        let tmp_client = tempfile::tempdir().unwrap();
        let bridge = LiveServerBridge::connect_or_start(
            tmp_client.path().to_path_buf(),
            assets_dir.clone(),
            "127.0.0.1".to_owned(),
            port,
            true,
            true,
        );
        assert_eq!(bridge.mode(), BridgeMode::AttachedHttp);

        let mut app = App::new();
        app.insert_resource(ClearColor::default())
            .insert_resource(Time::<()>::default())
            .insert_resource(UiState::default())
            .insert_resource(I18nStore::load_from_assets(&assets_dir, "en"))
            .insert_resource(bridge)
            .add_event::<AppExit>()
            .add_systems(Update, (process_ui_actions, sync_live_state_to_ui).chain());

        // Wait for initial harness fetch to settle before recording baseline.
        let init_deadline = Instant::now() + Duration::from_secs(2);
        while Instant::now() < init_deadline && api.list_calls.load(Ordering::SeqCst) == 0 {
            std::thread::sleep(Duration::from_millis(10));
        }

        {
            let mut ui = app.world_mut().resource_mut::<UiState>();
            ui.toasts.clear();
            ui.push_action(UiAction::LaunchHarness("claude".to_owned()));
        }

        let deadline = Instant::now() + Duration::from_secs(2);
        let mut saw_error_toast = false;
        while Instant::now() < deadline {
            app.update();
            if app
                .world()
                .resource::<UiState>()
                .toasts
                .iter()
                .any(|t| t.title == "Failed to open Claude" && t.body == "boom")
            {
                saw_error_toast = true;
                break;
            }
            std::thread::sleep(Duration::from_millis(15));
        }
        assert!(
            saw_error_toast,
            "HTTP 200 {{\"ok\": false, \"error\": \"boom\"}} must emit launch_error_toast in AttachedHttp mode"
        );

        drop(app);
        let _ = handle.shutdown.send(true);
        drop(rt);
    }

    #[test]
    fn in_process_clearing_last_session_and_perm_clears_ui_and_despawns_3d_workers() {
        use bevy::input::mouse::{MouseMotion, MouseWheel};
        use station_3d::{BackdropKind, OfficeLayout};

        let assets_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../assets");
        let tmp = tempfile::tempdir().unwrap();
        let bridge = LiveServerBridge::connect_or_start(
            tmp.path().to_path_buf(),
            assets_dir.clone(),
            "127.0.0.1".to_owned(),
            0,
            false,
            false,
        );
        let state = bridge.in_process_state().expect("expected InProcess bridge");

        let mut app = App::new();
        let mut time = Time::<()>::default();
        time.advance_by(Duration::from_secs(2));
        app.insert_resource(Assets::<Mesh>::default())
            .insert_resource(Assets::<StandardMaterial>::default())
            .insert_resource(Assets::<Image>::default())
            .insert_resource(ClearColor::default())
            .insert_resource(time)
            .insert_resource(ButtonInput::<KeyCode>::default())
            .insert_resource(ButtonInput::<MouseButton>::default())
            .add_event::<MouseMotion>()
            .add_event::<MouseWheel>()
            .add_event::<AppExit>()
            .add_plugins(station_3d::OfficeWorldPlugin {
                layout: OfficeLayout::Pro,
                backdrop: BackdropKind::City,
                assets_dir: assets_dir.clone(),
                night: false,
            })
            .insert_resource(UiState::new_live(
                OfficeLayout::Pro,
                BackdropKind::City,
                false,
            ))
            .insert_resource(I18nStore::load_from_assets(&assets_dir, "en"))
            .insert_resource(bridge)
            .add_systems(
                Update,
                (sync_live_state_to_ui, sync_live_sessions_to_3d).chain(),
            );

        // Initial tick with 0 live sessions -> 0 UI sessions, 0 3D LiveSessionWorkers, valid PIN & HH:MM clock.
        app.update();
        {
            let ui = app.world().resource::<UiState>();
            assert!(ui.sessions.is_empty());
            assert!(ui.pending_perms.is_empty());
            assert!(!ui.pairing_pin.is_empty());
            assert_eq!(ui.clock_label.len(), 5);
            assert_eq!(&ui.clock_label[2..3], ":");
        }
        assert_eq!(
            app.world_mut()
                .query::<&station_3d::people::LiveSessionWorker>()
                .iter(app.world())
                .count(),
            0
        );

        // Ingest 1 active working session + 1 pending perm -> UI and 3D reflect both.
        let (tx, _rx) = tokio::sync::oneshot::channel();
        {
            let mut office = state.office.lock().unwrap();
            office.ingest_raw_event(json!({
                "session_id": "sess-live-clear",
                "hook_event_name": "UserPromptSubmit",
                "cwd": "D:/repo/app",
                "prompt": "build task 6"
            }));
            office.register_hold(
                PendingPerm {
                    id: "perm-clear-1".to_owned(),
                    sess: "sess-live-clear".to_owned(),
                    name: "app".to_owned(),
                    kind: "permission".to_owned(),
                    tool_type: "Bash".to_owned(),
                    label: "cargo test".to_owned(),
                    detail: "cargo test".to_owned(),
                    danger: None,
                    created_at: 1,
                },
                tx,
            );
        }
        app.world_mut()
            .resource_mut::<Time>()
            .advance_by(Duration::from_secs(2));
        app.update();
        assert_eq!(app.world().resource::<UiState>().sessions.len(), 1);
        assert_eq!(app.world().resource::<UiState>().pending_perms.len(), 1);
        assert_eq!(
            app.world_mut()
                .query::<&station_3d::people::LiveSessionWorker>()
                .iter(app.world())
                .count(),
            1
        );

        // Resolve perm and end session -> BOTH UI and 3D must clear cleanly to 0.
        {
            let mut office = state.office.lock().unwrap();
            office.resolve_hold("perm-clear-1", PermDecision::Allow);
            office.ingest_raw_event(json!({
                "session_id": "sess-live-clear",
                "hook_event_name": "SessionEnd"
            }));
        }
        app.world_mut()
            .resource_mut::<Time>()
            .advance_by(Duration::from_secs(2));
        app.update();
        assert!(
            app.world().resource::<UiState>().sessions.is_empty(),
            "ending last session must clear UiState.sessions"
        );
        assert!(
            app.world().resource::<UiState>().pending_perms.is_empty(),
            "resolving last perm must clear UiState.pending_perms"
        );
        assert_eq!(
            app.world_mut()
                .query::<&station_3d::people::LiveSessionWorker>()
                .iter(app.world())
                .count(),
            0,
            "ending last session must despawn dynamic 3D workers cleanly"
        );
    }

    #[test]
    fn all_remaining_ui_actions_execute_real_behavior_with_zero_noop_stubs() {
        use bevy::input::mouse::{MouseMotion, MouseWheel};
        use station_3d::{BackdropKind, OfficeLayout};
        use station_ui::state::GpuPrefUi;

        // 1. Structural guard: zero `=> {}` arms on UiAction inside process_ui_actions.
        let src = include_str!("bridge.rs");
        let start = src.find("fn process_ui_actions(").expect("process_ui_actions present");
        let end = src[start..].find("#[cfg(test)]").map(|o| start + o).unwrap_or(src.len());
        let body = &src[start..end];
        let match_start = body.find("match action {").expect("match action present");
        let match_end = body[match_start..]
            .find("// Execute at most ONE world transition")
            .map(|o| match_start + o)
            .unwrap_or(body.len());
        let match_body = &body[match_start..match_end];
        assert!(
            !match_body.contains("=> {}"),
            "process_ui_actions must have ZERO `=> {{}}` no-op UiAction stubs"
        );

        let assets_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../assets");
        let tmp = tempfile::tempdir().unwrap();
        let data_dir = tmp.path().to_path_buf();
        let bridge = LiveServerBridge::connect_or_start(
            data_dir.clone(),
            assets_dir.clone(),
            "127.0.0.1".to_owned(),
            0,
            false,
            false,
        );
        let state = bridge.in_process_state().expect("expected InProcess bridge");

        let mut app = App::new();
        app.insert_resource(Assets::<Mesh>::default())
            .insert_resource(Assets::<StandardMaterial>::default())
            .insert_resource(Assets::<Image>::default())
            .insert_resource(ClearColor::default())
            .insert_resource(Time::<()>::default())
            .insert_resource(ButtonInput::<KeyCode>::default())
            .insert_resource(ButtonInput::<MouseButton>::default())
            .add_event::<MouseMotion>()
            .add_event::<MouseWheel>()
            .add_event::<AppExit>()
            .add_plugins(station_3d::OfficeWorldPlugin {
                layout: OfficeLayout::Pro,
                backdrop: BackdropKind::City,
                assets_dir: assets_dir.clone(),
                night: false,
            })
            .insert_resource(UiState::new_live(
                OfficeLayout::Pro,
                BackdropKind::City,
                false,
            ))
            .insert_resource(I18nStore::load_from_assets(&assets_dir, "en"))
            .insert_resource(bridge)
            .add_systems(Update, process_ui_actions);

        app.update();

        // Queue every remaining UiAction variant and verify real state/disk mutations.
        {
            let mut ui = app.world_mut().resource_mut::<UiState>();
            ui.push_action(UiAction::SetWeatherCity("hn".to_owned()));
            ui.push_action(UiAction::SetSound(false));
            ui.push_action(UiAction::ToggleCameraFp);
            ui.push_action(UiAction::TriggerPhoto);
            ui.push_action(UiAction::ToggleDemo);
            ui.push_action(UiAction::ToggleReplay);
            ui.push_action(UiAction::PostGroupChat("Hello live team!".to_owned()));
            ui.push_action(UiAction::SaveWhiteboard("- Ship Task 6\n- Verify tests".to_owned()));
            ui.push_action(UiAction::SetGpuPref(GpuPrefUi::Integrated));
            ui.push_action(UiAction::SendPrompt {
                session_id: "sess-prompt-1".to_owned(),
                text: "Run cargo test".to_owned(),
            });
        }
        app.update();

        assert_eq!(app.world().resource::<station_3d::life::OfficeLifeState>().weather_city, "hn");
        assert!(!app.world().resource::<station_3d::audio::SfxEngine>().enabled);
        assert!(app.world().resource::<station_3d::camera::CameraRig>().first_person);
        assert!(app.world().resource::<UiState>().first_person);
        assert!(app.world().resource::<UiState>().demo_running);
        assert!(!app.world().resource::<UiState>().sessions.is_empty());
        assert!(app.world().resource::<station_3d::replay::ReplayState>().playing);

        let shots_dir = data_dir.join("shots");
        let shot_count = std::fs::read_dir(&shots_dir)
            .map(|rd| {
                rd.flatten()
                    .filter(|e| e.path().extension().and_then(|s| s.to_str()) == Some("png"))
                    .count()
            })
            .unwrap_or(0);
        assert!(shot_count >= 1, "TriggerPhoto must save a real PNG to data/shots/");

        {
            let office = state.office.lock().unwrap();
            assert!(
                office.chat.msgs.iter().any(|m| m.text.contains("Hello live team!")),
                "PostGroupChat must persist message into OfficeState.chat"
            );
            assert_eq!(office.todos.len(), 2, "SaveWhiteboard must update OfficeState.todos");
        }
        assert!(
            data_dir.join("whiteboard.json").is_file(),
            "SaveWhiteboard must persist whiteboard.json"
        );
        let prefs = station_core::prefs::PrefsStore::load(&data_dir);
        assert_eq!(
            prefs.get("gpuPref").and_then(Value::as_str),
            Some("integrated"),
            "SetGpuPref must persist gpuPref in prefs.json"
        );
        assert_eq!(
            prefs.get("weatherCity").and_then(Value::as_str),
            Some("hn"),
            "SetWeatherCity must persist weatherCity in prefs.json"
        );
        assert_eq!(
            prefs.get("soundOn").and_then(Value::as_bool),
            Some(false),
            "SetSound must persist soundOn in prefs.json"
        );
        assert!(
            state.steer_queue.lock().unwrap().len() >= 1,
            "SendPrompt must route through steer_queue/TaskManager"
        );
    }

    #[test]
    fn send_prompt_routes_busy_to_steer_and_idle_to_tasks_without_double_execution() {
        let assets_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../assets");

        // 1. InProcess mode with live runtime: busy session -> steer_queue; idle session -> TaskManager ONLY (steer_queue stays 0).
        let tmp = tempfile::tempdir().unwrap();
        let cwd_str = tmp.path().to_string_lossy().into_owned();
        let bridge = LiveServerBridge::connect_or_start(
            tmp.path().to_path_buf(),
            assets_dir.clone(),
            "127.0.0.1".to_owned(),
            0,
            true,
            false,
        );
        let state = bridge.in_process_state().expect("expected InProcess bridge");
        {
            let mut office = state.office.lock().unwrap();
            office.ingest_raw_event(json!({
                "session_id": "s-idle",
                "hook_event_name": "SessionStart",
                "cwd": cwd_str,
            }));
            office.ingest_raw_event(json!({
                "session_id": "s-busy",
                "hook_event_name": "UserPromptSubmit",
                "cwd": cwd_str,
                "prompt": "working"
            }));
        }

        let mut app = App::new();
        app.insert_resource(ClearColor::default())
            .insert_resource(UiState::new_live(
                station_3d::OfficeLayout::Pro,
                station_3d::BackdropKind::City,
                false,
            ))
            .insert_resource(I18nStore::load_from_assets(&assets_dir, "en"))
            .insert_resource(bridge)
            .add_event::<AppExit>()
            .add_systems(Update, process_ui_actions);

        // Idle session must NOT enqueue into steer_queue (avoids double execution on Stop).
        app.world_mut()
            .resource_mut::<UiState>()
            .push_action(UiAction::SendPrompt {
                session_id: "s-idle".to_owned(),
                text: "run idle task".to_owned(),
            });
        app.update();
        assert_eq!(
            state.steer_queue.lock().unwrap().len(),
            0,
            "idle InProcess SendPrompt must dispatch via TaskManager without double-enqueuing into steer_queue"
        );

        // Busy session MUST enqueue into steer_queue.
        app.world_mut()
            .resource_mut::<UiState>()
            .push_action(UiAction::SendPrompt {
                session_id: "s-busy".to_owned(),
                text: "steer busy session".to_owned(),
            });
        app.update();
        assert_eq!(
            state.steer_queue.lock().unwrap().len(),
            1,
            "busy InProcess SendPrompt must enqueue into steer_queue"
        );
        drop(app);

        // 2. AttachedHttp mode: busy session -> POST /api/tasks/steer; resting session -> POST /api/tasks.
        let (action_tx, mut action_rx) = mpsc::channel::<RemoteActionCmd>(8);
        let (query_tx, _query_rx) = mpsc::channel::<RemoteQueryCmd>(8);
        let (shutdown, _rx) = watch::channel(false);
        let (toast_tx, toast_rx) = mpsc::channel(TOAST_QUEUE_CAP);
        let (async_tx, async_rx) = mpsc::channel(TOAST_QUEUE_CAP);
        let attached_bridge = LiveServerBridge {
            backend: BridgeBackend::AttachedHttp {
                snapshot: Arc::new(Mutex::new(RemoteSnapshot::default())),
                action_tx,
                query_tx,
                _shutdown: shutdown,
            },
            data_dir: tmp.path().to_path_buf(),
            assets_dir: assets_dir.clone(),
            port: 4310,
            sync_timer: Timer::from_seconds(1.0, TimerMode::Repeating),
            sys_sync_timer: Timer::from_seconds(4.0, TimerMode::Repeating),
            sys_refresh_due: true,
            banner_pushed: true,
            seen_perm_ids: HashSet::new(),
            dismissed_ci_ids: HashSet::new(),
            last_window_focused: None,
            toast_tx,
            toast_rx,
            async_tx,
            async_rx,
        };
        let mut ui = UiState::new_live(
            station_3d::OfficeLayout::Pro,
            station_3d::BackdropKind::City,
            false,
        );
        ui.sessions = vec![
            SessionCard {
                id: "att-busy".to_owned(),
                agent: "claude".to_owned(),
                project: "p".to_owned(),
                status: "work".to_owned(),
                model: "opus".to_owned(),
                last_prompt: String::new(),
                tool: None,
                tool_kind: None,
                desk: 0,
                cost_usd: 0.0,
                tokens: 0,
                pinned: false,
                ..Default::default()
            },
            SessionCard {
                id: "att-rest".to_owned(),
                agent: "codex".to_owned(),
                project: "p".to_owned(),
                status: "rest".to_owned(),
                model: "gpt".to_owned(),
                last_prompt: String::new(),
                tool: None,
                tool_kind: None,
                desk: 1,
                cost_usd: 0.0,
                tokens: 0,
                pinned: false,
                ..Default::default()
            },
        ];
        ui.push_action(UiAction::SendPrompt {
            session_id: "att-busy".to_owned(),
            text: "pivot now".to_owned(),
        });
        ui.push_action(UiAction::SendPrompt {
            session_id: "att-rest".to_owned(),
            text: "start next".to_owned(),
        });

        let mut app2 = App::new();
        app2.insert_resource(ClearColor::default())
            .insert_resource(ui)
            .insert_resource(I18nStore::load_from_assets(&assets_dir, "en"))
            .insert_resource(attached_bridge)
            .add_event::<AppExit>()
            .add_systems(Update, process_ui_actions);
        app2.update();

        let cmd1 = action_rx.try_recv().expect("busy SendPrompt must enqueue remote cmd");
        assert_eq!(cmd1.path, "/api/tasks/steer");
        assert_eq!(cmd1.body["sid"], "att-busy");

        let cmd2 = action_rx.try_recv().expect("rest SendPrompt must enqueue remote cmd");
        assert_eq!(cmd2.path, "/api/tasks");
        assert_eq!(cmd2.body["sid"], "att-rest");
        assert_eq!(cmd2.body["agent"], "codex");
    }

    #[test]
    fn sys_sync_timer_latches_due_flag_across_unaligned_frame_deltas() {
        let assets_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../assets");
        let tmp = tempfile::tempdir().unwrap();
        let mut bridge = LiveServerBridge::connect_or_start(
            tmp.path().to_path_buf(),
            assets_dir,
            "127.0.0.1".to_owned(),
            0,
            false,
            false,
        );
        // Set sync_timer to 1.0s and sys_sync_timer to 0.4s so sys_sync_timer fires on a frame where sync_timer does NOT.
        bridge.sync_timer = Timer::from_seconds(1.0, TimerMode::Repeating);
        bridge.sys_sync_timer = Timer::from_seconds(0.4, TimerMode::Repeating);
        bridge.sys_refresh_due = false;

        let mut app = App::new();
        app.insert_resource(Time::<()>::default())
            .insert_resource(UiState::new_live(
                station_3d::OfficeLayout::Pro,
                station_3d::BackdropKind::City,
                false,
            ))
            .insert_resource(bridge)
            .add_systems(Update, sync_live_state_to_ui);

        // Advance 450ms -> sys_sync_timer finishes while sync_timer is still at 450ms/1000ms.
        app.world_mut()
            .resource_mut::<Time>()
            .advance_by(Duration::from_millis(450));
        app.update();
        assert!(
            app.world().resource::<LiveServerBridge>().sys_refresh_due,
            "sys_refresh_due must latch true when sys_sync_timer fires off-cycle from sync_timer"
        );

        // Advance another 600ms -> sync_timer finishes, consumes sys_refresh_due, and resets it to false.
        app.world_mut()
            .resource_mut::<Time>()
            .advance_by(Duration::from_millis(600));
        app.update();
        assert!(
            !app.world().resource::<LiveServerBridge>().sys_refresh_due,
            "sys_refresh_due must reset to false after sync_in_process_telemetry runs"
        );
    }

    #[test]
    fn attached_http_post_group_chat_refreshes_chat_immediately_without_waiting_for_telemetry_poll() {
        let rt = tokio::runtime::Builder::new_multi_thread()
            .worker_threads(1)
            .enable_all()
            .build()
            .unwrap();
        let tmp_srv = tempfile::tempdir().unwrap();
        let srv_state = Arc::new(AppState::new(tmp_srv.path().to_path_buf(), None).unwrap());
        let handle = rt
            .block_on(start_http_server(srv_state, "127.0.0.1", 0))
            .unwrap();
        let port = handle.addr.port();

        let assets_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../assets");
        let tmp_client = tempfile::tempdir().unwrap();
        let bridge = LiveServerBridge::connect_or_start(
            tmp_client.path().to_path_buf(),
            assets_dir.clone(),
            "127.0.0.1".to_owned(),
            port,
            true,
            true,
        );
        assert_eq!(bridge.mode(), BridgeMode::AttachedHttp);

        let mut app = App::new();
        app.insert_resource(ClearColor::default())
            .insert_resource(Time::<()>::default())
            .insert_resource(UiState::new_live(
                station_3d::OfficeLayout::Pro,
                station_3d::BackdropKind::City,
                false,
            ))
            .insert_resource(I18nStore::load_from_assets(&assets_dir, "en"))
            .insert_resource(bridge)
            .add_event::<AppExit>()
            .add_systems(Update, (process_ui_actions, sync_live_state_to_ui).chain());

        app.world_mut()
            .resource_mut::<UiState>()
            .push_action(UiAction::PostGroupChat("Immediate attached chat!".to_owned()));

        let deadline = Instant::now() + Duration::from_secs(2);
        let mut saw_chat = false;
        while Instant::now() < deadline {
            app.world_mut()
                .resource_mut::<Time>()
                .advance_by(Duration::from_millis(600));
            app.update();
            if app
                .world()
                .resource::<UiState>()
                .chat
                .iter()
                .any(|m| m.text.contains("Immediate attached chat!"))
            {
                saw_chat = true;
                break;
            }
            std::thread::sleep(Duration::from_millis(20));
        }
        assert!(
            saw_chat,
            "AttachedHttp PostGroupChat must immediately refresh /api/chat into UiState.chat"
        );

        drop(app);
        let _ = handle.shutdown.send(true);
        drop(rt);
    }

    #[test]
    fn attached_http_refresh_telemetry_immediately_fetches_live_and_telemetry_snapshot() {
        let rt = tokio::runtime::Builder::new_multi_thread()
            .worker_threads(1)
            .enable_all()
            .build()
            .unwrap();
        let tmp_srv = tempfile::tempdir().unwrap();
        let srv_state = Arc::new(AppState::new(tmp_srv.path().to_path_buf(), None).unwrap());
        let srv_ref = srv_state.clone();
        let handle = rt
            .block_on(start_http_server(srv_state, "127.0.0.1", 0))
            .unwrap();
        let port = handle.addr.port();

        let assets_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../assets");
        let tmp_client = tempfile::tempdir().unwrap();
        let bridge = LiveServerBridge::connect_or_start(
            tmp_client.path().to_path_buf(),
            assets_dir.clone(),
            "127.0.0.1".to_owned(),
            port,
            true,
            true,
        );
        assert_eq!(bridge.mode(), BridgeMode::AttachedHttp);

        let mut app = App::new();
        app.insert_resource(ClearColor::default())
            .insert_resource(Time::<()>::default())
            .insert_resource(UiState::new_live(
                station_3d::OfficeLayout::Pro,
                station_3d::BackdropKind::City,
                false,
            ))
            .insert_resource(I18nStore::load_from_assets(&assets_dir, "en"))
            .insert_resource(bridge)
            .add_event::<AppExit>()
            .add_systems(Update, (process_ui_actions, sync_live_state_to_ui).chain());

        // Mutate daemon state directly (without broadcasting SSE) so only an explicit
        // RefreshTelemetry fetch can pick it up immediately.
        {
            let mut office = srv_ref.office.lock().unwrap();
            office.sessions.insert(
                "sess-refresh-1".to_owned(),
                station_core::state::sessions::SessionRecord {
                    id: "sess-refresh-1".to_owned(),
                    agent: "claude".to_owned(),
                    name: "refresh-proj".to_owned(),
                    cwd: "D:\\refresh".to_owned(),
                    status: "work".to_owned(),
                    ..Default::default()
                },
            );
        }

        app.world_mut()
            .resource_mut::<UiState>()
            .push_action(UiAction::RefreshTelemetry);

        let deadline = Instant::now() + Duration::from_secs(3);
        let mut saw_session = false;
        let mut saw_sys = false;
        while Instant::now() < deadline {
            app.world_mut()
                .resource_mut::<Time>()
                .advance_by(Duration::from_millis(600));
            app.update();
            let ui = app.world().resource::<UiState>();
            if ui.sessions.iter().any(|s| s.id == "sess-refresh-1") {
                saw_session = true;
            }
            if ui.sys.mem_total_gb > 0.0 {
                saw_sys = true;
            }
            if saw_session && saw_sys {
                break;
            }
            std::thread::sleep(Duration::from_millis(20));
        }
        assert!(
            saw_session,
            "AttachedHttp RefreshTelemetry must immediately pull updated /api/live sessions"
        );
        assert!(
            saw_sys,
            "AttachedHttp RefreshTelemetry must immediately pull /api/sys telemetry"
        );

        drop(app);
        let _ = handle.shutdown.send(true);
        drop(rt);
    }

    #[test]
    fn stage_viewport_clamps_normal_1280x720_and_hidpi_1_5x_and_oob_and_minimized_0x0() {
        // 1. Normal 1280x720 at 1.0 scale
        let stage = egui::Rect::from_min_max(egui::pos2(300.0, 56.0), egui::pos2(950.0, 720.0));
        let vp = compute_camera_viewport(stage, 1.0, UVec2::new(1280, 720))
            .expect("normal 1280x720 viewport must be Some");
        assert_eq!(vp.physical_position, UVec2::new(300, 56));
        assert_eq!(vp.physical_size, UVec2::new(650, 664));

        // 2. HiDPI 1.5x (1280x720 logical -> 1920x1080 physical)
        let vp_hi = compute_camera_viewport(stage, 1.5, UVec2::new(1920, 1080))
            .expect("HiDPI 1.5x viewport must be Some");
        assert_eq!(vp_hi.physical_position, UVec2::new(450, 84));
        assert_eq!(vp_hi.physical_size, UVec2::new(975, 996));
        assert!(vp_hi.physical_position.x + vp_hi.physical_size.x <= 1920);
        assert!(vp_hi.physical_position.y + vp_hi.physical_size.y <= 1080);

        // 3. Out-of-bounds logical rect gets strictly clamped inside physical bounds
        let oob = egui::Rect::from_min_max(egui::pos2(-40.0, -12.0), egui::pos2(2000.0, 1500.0));
        let vp_oob = compute_camera_viewport(oob, 1.0, UVec2::new(1280, 720))
            .expect("clamped OOB viewport must be Some");
        assert_eq!(vp_oob.physical_position, UVec2::new(0, 0));
        assert_eq!(vp_oob.physical_size, UVec2::new(1280, 720));

        // 4. Minimized 0x0 window returns None so wgpu never panics
        assert!(compute_camera_viewport(stage, 1.0, UVec2::new(0, 0)).is_none());
        // 5. Degenerate < 4px stage rect returns None
        let tiny = egui::Rect::from_min_max(egui::pos2(100.0, 100.0), egui::pos2(102.0, 102.0));
        assert!(compute_camera_viewport(tiny, 1.0, UVec2::new(1280, 720)).is_none());
    }

    #[test]
    fn sync_camera_viewport_system_updates_camera_and_deactivates_on_minimized_window() {
        let mut app = App::new();
        app.insert_resource(StageViewportRect(Some(egui::Rect::from_min_max(
            egui::pos2(300.0, 56.0),
            egui::pos2(950.0, 720.0),
        ))))
        .add_systems(Update, sync_camera_viewport_to_stage);

        let mut res = bevy::window::WindowResolution::new(1280.0, 720.0)
            .with_scale_factor_override(1.5);
        res.set_physical_resolution(1920, 1080);
        let win_entity = app
            .world_mut()
            .spawn(Window {
                resolution: res,
                ..default()
            })
            .id();
        let cam_entity = app
            .world_mut()
            .spawn((station_3d::camera::MainOfficeCamera, Camera::default()))
            .id();

        app.update();
        let cam = app.world().get::<Camera>(cam_entity).unwrap();
        assert!(cam.is_active);
        let vp = cam.viewport.as_ref().expect("viewport must be synced");
        assert_eq!(vp.physical_position, UVec2::new(450, 84));
        assert_eq!(vp.physical_size, UVec2::new(975, 996));

        // Minimize window to 0x0 -> camera must be deactivated so wgpu never panics
        app.world_mut()
            .get_mut::<Window>(win_entity)
            .unwrap()
            .resolution
            .set_physical_resolution(0, 0);
        app.update();
        let cam_min = app.world().get::<Camera>(cam_entity).unwrap();
        assert!(
            !cam_min.is_active,
            "camera must deactivate when window physical size is 0x0"
        );
    }

    #[test]
    fn sync_camera_viewport_deactivates_render_target_image_when_target_size_unknown() {
        let mut app = App::new();
        app.insert_resource(StageViewportRect(Some(egui::Rect::from_min_max(
            egui::pos2(300.0, 56.0),
            egui::pos2(950.0, 720.0),
        ))))
        .add_systems(Update, sync_camera_viewport_to_stage);

        let cam_entity = app
            .world_mut()
            .spawn((
                station_3d::camera::MainOfficeCamera,
                Camera {
                    target: RenderTarget::Image(Handle::<Image>::default().into()),
                    is_active: true,
                    ..default()
                },
            ))
            .id();

        app.update();
        let cam = app.world().get::<Camera>(cam_entity).unwrap();
        assert!(
            !cam.is_active,
            "RenderTarget::Image camera must deactivate when physical_target_size() is None"
        );
        assert!(
            cam.viewport.is_none(),
            "viewport must not be populated with hardcoded fallback dimensions"
        );
    }

    #[test]
    fn project_stage_labels_projects_in_front_workers_and_culls_behind_camera() {
        use bevy::render::camera::{CameraProjectionPlugin, ManualTextureViews};
        use bevy::render::render_resource::Extent3d;
        use bevy::window::{WindowCreated, WindowResized, WindowScaleFactorChanged};

        let assets = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../assets");
        let i18n = I18nStore::load_from_assets(&assets, "vi");
        let stage_rect = egui::Rect::from_min_max(egui::pos2(300.0, 56.0), egui::pos2(950.0, 720.0));
        let mut ui = UiState::with_demo_seed(
            station_3d::OfficeLayout::Pro,
            station_3d::BackdropKind::City,
            false,
        );
        ui.sessions.truncate(2);

        let mut app = App::new();
        app.insert_resource(Assets::<Image>::default())
            .insert_resource(ManualTextureViews::default())
            .insert_resource(StageViewportRect(Some(stage_rect)))
            .insert_resource(i18n)
            .insert_resource(ui)
            .add_event::<WindowResized>()
            .add_event::<WindowScaleFactorChanged>()
            .add_event::<WindowCreated>()
            .add_event::<AssetEvent<Image>>()
            .add_plugins((TransformPlugin, CameraProjectionPlugin))
            .add_systems(
                PostUpdate,
                project_stage_labels_to_ui.after(bevy::transform::TransformSystem::TransformPropagate),
            );

        let mut img = Image::default();
        img.texture_descriptor.size = Extent3d {
            width: 1280,
            height: 720,
            depth_or_array_layers: 1,
        };
        let handle = app.world_mut().resource_mut::<Assets<Image>>().add(img);

        // Spawn camera looking at origin from (0, 2, 10)
        app.world_mut().spawn((
            station_3d::camera::MainOfficeCamera,
            Camera {
                target: RenderTarget::Image(handle.into()),
                viewport: Some(Viewport {
                    physical_position: UVec2::new(300, 56),
                    physical_size: UVec2::new(650, 664),
                    ..default()
                }),
                ..default()
            },
            Projection::Perspective(PerspectiveProjection {
                fov: 70.0_f32.to_radians(),
                near: 0.1,
                far: 200.0,
                ..default()
            }),
            Transform::from_xyz(0.0, 2.0, 10.0).looking_at(Vec3::new(0.0, 2.0, 0.0), Vec3::Y),
        ));

        // Worker 0 in front of camera; Worker 1 behind camera.
        app.world_mut().spawn((
            station_3d::people::LiveSessionWorker {
                session_id: "sess-claude-1".to_owned(),
                agent: "claude".to_owned(),
                name: "agents-station".to_owned(),
                desk_index: 0,
                status: "work".to_owned(),
                tool_label: Some("Edit".to_owned()),
                git_dirty: 0,
            },
            Transform::from_xyz(0.0, 0.0, 0.0),
        ));
        app.world_mut().spawn((
            station_3d::people::LiveSessionWorker {
                session_id: "sess-codex-2".to_owned(),
                agent: "codex".to_owned(),
                name: "station-core".to_owned(),
                desk_index: 1,
                status: "wait".to_owned(),
                tool_label: Some("Bash".to_owned()),
                git_dirty: 0,
            },
            Transform::from_xyz(0.0, 0.0, 22.0),
        ));

        app.update();
        let projected = &app.world().resource::<UiState>().projected_labels;
        assert!(
            projected.iter().any(|l| l.id == "worker:sess-claude-1"),
            "in-front worker must be projected: {projected:?}"
        );
        assert!(
            !projected.iter().any(|l| l.id == "worker:sess-codex-2"),
            "behind-camera worker must be culled cleanly: {projected:?}"
        );
    }

    #[test]
    fn camera_toolbar_actions_wire_presets_zoom_and_replay_without_stubs() {
        let dir = tempfile::tempdir().unwrap();
        let assets = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../assets");
        let i18n = I18nStore::load_from_assets(&assets, "vi");
        let spec = LayoutSpec::for_layout(station_3d::OfficeLayout::Pro);
        let rig = station_3d::camera::CameraRig::from_layout(&spec);

        let mut app = App::new();
        app.insert_resource(Time::<()>::default())
            .insert_resource(ClearColor::default())
            .insert_resource(i18n)
            .insert_resource(UiState::with_demo_seed(
                station_3d::OfficeLayout::Pro,
                station_3d::BackdropKind::City,
                false,
            ))
            .insert_resource(rig)
            .insert_resource(station_3d::replay::ReplayState::default())
            .add_event::<station_3d::WorkerSelected>()
            .add_event::<station_3d::interact::PropActivated>()
            .add_event::<AppExit>()
            .add_plugins(LiveServerBridgePlugin {
                data_dir: dir.path().to_path_buf(),
                assets_dir: assets,
                host: "127.0.0.1".to_owned(),
                port: 0,
                start_http: false,
            });
        app.world_mut()
            .spawn((station_3d::camera::MainOfficeCamera, Projection::default()));

        {
            let mut ui = app.world_mut().resource_mut::<UiState>();
            ui.push_action(UiAction::SelectCameraPreset(station_3d::CameraPreset::Meeting));
            ui.push_action(UiAction::ZoomCamera(1.2));
            ui.push_action(UiAction::OpenPlaceCard("meeting".to_owned()));
            ui.push_action(UiAction::ReplaySetSpeed(4.0));
        }
        app.update();

        let ui = app.world().resource::<UiState>();
        let rig = app.world().resource::<station_3d::camera::CameraRig>();
        assert_eq!(rig.active_preset, station_3d::CameraPreset::Meeting);
        assert_eq!(ui.camera_preset, station_3d::CameraPreset::Meeting);
        assert!(rig.zoom_pct() > 200);
        assert_eq!(ui.active_place_card.as_ref().map(|c| c.place_id.as_str()), Some("meeting"));
        assert!((app.world().resource::<station_3d::replay::ReplayState>().speed - 4.0).abs() < 0.01);

        {
            let mut ui = app.world_mut().resource_mut::<UiState>();
            ui.push_action(UiAction::ResetCameraZoom);
            ui.push_action(UiAction::ClosePlaceCard);
        }
        app.update();
        assert_eq!(app.world().resource::<station_3d::camera::CameraRig>().zoom_pct(), 100);
        assert!(app.world().resource::<UiState>().active_place_card.is_none());
    }

    #[test]
    fn all_interactive_props_resolve_valid_fp_near_i18n_keys_in_en_vi_th() {
        let assets = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../assets");
        let all_props = [
            ("whiteboard", "office"),
            ("cat", "lounge"),
            ("tv", "server"),
            ("arcade", "lounge"),
            ("aquarium", "lounge"),
            ("game-tv", "lounge"),
            ("billiards", "lounge"),
            ("karaoke", "lounge"),
            ("duty-board", "pantry"),
            ("printer", "office"),
            ("neon", "office"),
            ("dev-servers", "server"),
            ("vacuum", "office"),
        ];
        for lang in ["en", "vi", "th"] {
            let i18n = I18nStore::load_from_assets(&assets, lang);
            for (prop_id, prop_place) in all_props {
                let key = fp_prop_i18n_key(prop_id, prop_place);
                let text = i18n.tr(key);
                assert!(
                    !text.is_empty() && text != key && !text.starts_with("places."),
                    "lang={lang} prop=({prop_id},{prop_place}) leaked raw i18n key '{key}' -> '{text}'"
                );
            }
            for place in ["pantry", "office", "meeting", "coffee", "lounge", "deploy", "library", "director"] {
                let tkey = place_title_i18n_key(place);
                let title = i18n.tr(tkey);
                assert!(
                    !title.is_empty() && title != tkey,
                    "lang={lang} place={place} leaked raw title key '{tkey}'"
                );
            }
        }
    }

    #[test]
    fn photo_time_auto_restores_clock_driven_lighting_and_capture_hides_photobar() {
        assert!(is_night_for_hour(0));
        assert!(is_night_for_hour(5));
        assert!(!is_night_for_hour(6));
        assert!(!is_night_for_hour(12));
        assert!(!is_night_for_hour(17));
        assert!(is_night_for_hour(18));
        assert!(is_night_for_hour(23));

        assert_eq!(PhotoSize::Screen.target_dimensions(1280, 720), (1280, 720));
        assert_eq!(PhotoSize::Fhd.target_dimensions(1280, 720), (1920, 1080));
        assert_eq!(PhotoSize::Uhd4k.target_dimensions(1280, 720), (3840, 2160));

        let dir = tempfile::tempdir().unwrap();
        let assets = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../assets");
        let i18n = I18nStore::load_from_assets(&assets, "en");
        let expected_night = is_night_for_hour(Local::now().hour());

        let lite_bg = Color::srgb(0.06, 0.075, 0.105);
        let mut app = App::new();
        app.insert_resource(Time::<()>::default())
            .insert_resource(ClearColor(lite_bg))
            .insert_resource(station_3d::lights::LightingState {
                night: !expected_night,
                photo_hour: Some(22.0),
                room_lights_off: false,
            })
            .insert_resource(i18n)
            .insert_resource(UiState::with_demo_seed(
                station_3d::OfficeLayout::Pro,
                station_3d::BackdropKind::City,
                !expected_night,
            ))
            .add_event::<station_3d::WorkerSelected>()
            .add_event::<station_3d::interact::PropActivated>()
            .add_event::<AppExit>()
            .add_systems(
                Update,
                station_3d::lights::sync_lighting_system.in_set(station_3d::LightingSyncSet),
            )
            .add_plugins(LiveServerBridgePlugin {
                data_dir: dir.path().to_path_buf(),
                assets_dir: assets,
                host: "127.0.0.1".to_owned(),
                port: 0,
                start_http: false,
            });
        app.world_mut().spawn((Window::default(), PrimaryWindow));

        // 1. SetPhotoTimeAuto restores real clock-driven day/night state, clears photo_hour,
        // and preserves Lite2D ClearColor when no MainOfficeCamera is present.
        app.world_mut()
            .resource_mut::<UiState>()
            .push_action(UiAction::SetPhotoTimeAuto);
        app.update();
        assert_eq!(app.world().resource::<UiState>().night, expected_night);
        let lighting = app.world().resource::<station_3d::lights::LightingState>();
        assert_eq!(lighting.night, expected_night);
        assert_eq!(lighting.photo_hour, None);
        assert_eq!(
            app.world().resource::<ClearColor>().0,
            lite_bg,
            "Lite2D ClearColor must remain untouched when MainOfficeCamera is absent"
        );

        // 1b. With MainOfficeCamera present, ToggleNight syncs ClearColor on the EXACT same frame
        // via process_ui_actions.before(LightingSyncSet).
        let spec = LayoutSpec::for_layout(station_3d::OfficeLayout::Pro);
        let mut cmds = app.world_mut().commands();
        station_3d::camera::spawn_office_camera(&mut cmds, &spec);
        app.world_mut().flush();
        app.world_mut().resource_mut::<station_3d::lights::LightingState>().photo_hour = Some(10.0);
        app.world_mut()
            .resource_mut::<UiState>()
            .push_action(UiAction::ToggleNight(true));
        app.update();
        assert_eq!(app.world().resource::<station_3d::lights::LightingState>().photo_hour, None);
        assert_eq!(
            app.world().resource::<ClearColor>().0,
            station_3d::palette::srgb_hex("#121724"),
            "ToggleNight must sync 3D ClearColor on the same frame via LightingSyncSet ordering"
        );

        // 2. CapturePhoto hides PhotoBar first, then spawns Screenshot on the clean frame.
        app.world_mut()
            .resource_mut::<UiState>()
            .push_action(UiAction::CapturePhoto {
                size: PhotoSize::Fhd,
                show_names: true,
                show_mark: true,
            });
        app.update();
        assert!(
            app.world().resource::<UiState>().photo_capture_hide_frames >= 1,
            "PhotoBar must be hidden on capture frame before screenshot runs"
        );
        assert_eq!(
            app.world_mut().query::<&Screenshot>().iter(app.world()).count(),
            0,
            "Screenshot must wait 1 clean frame so PhotoBar HUD is not captured"
        );

        // Frame 2 decrements wait; Frame 3 spawns Screenshot while PhotoBar is still hidden (hide_frames == 1).
        app.update();
        app.update();
        assert_eq!(
            app.world_mut().query::<&Screenshot>().iter(app.world()).count(),
            1,
            "Screenshot must spawn cleanly once PhotoBar is hidden"
        );
        assert_eq!(
            app.world().resource::<UiState>().photo_capture_hide_frames,
            1,
            "PhotoBar must remain hidden during the screenshot frame itself"
        );

        // Frame 4 restores PhotoBar cleanly after the capture frame completes.
        app.update();
        assert_eq!(
            app.world().resource::<UiState>().photo_capture_hide_frames,
            0,
            "PhotoBar must reappear after capture frame completes"
        );
    }

    #[test]
    fn task5_approval_cards_and_session_drawer_actions_wire_end_to_end_in_process() {
        let dir = tempfile::tempdir().unwrap();
        let assets = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../assets");
        let i18n = I18nStore::load_from_assets(&assets, "en");

        let mut app = App::new();
        app.insert_resource(Time::<()>::default())
            .insert_resource(ClearColor::default())
            .insert_resource(i18n)
            .insert_resource(UiState::default())
            .add_event::<station_3d::WorkerSelected>()
            .add_event::<station_3d::interact::PropActivated>()
            .add_event::<AppExit>()
            .add_plugins(LiveServerBridgePlugin {
                data_dir: dir.path().to_path_buf(),
                assets_dir: assets,
                host: "127.0.0.1".to_owned(),
                port: 0,
                start_http: false,
            });

        let app_state = app
            .world()
            .resource::<LiveServerBridge>()
            .in_process_state()
            .expect("InProcess state");

        // Register AskUserQuestion, ExitPlanMode, and Denied holds + a red CI card
        let (tx_ask, mut rx_ask) = tokio::sync::oneshot::channel();
        let (tx_plan, mut rx_plan) = tokio::sync::oneshot::channel();
        let (tx_den, mut rx_den) = tokio::sync::oneshot::channel();
        let ci_id = {
            let mut office = app_state.office.lock().unwrap();
            office.register_hold_with_input(
                station_core::state::sessions::PendingPerm {
                    id: "ask-1".to_owned(),
                    sess: "s1".to_owned(),
                    name: "claude".to_owned(),
                    kind: "ask".to_owned(),
                    tool_type: "ask".to_owned(),
                    label: "Question".to_owned(),
                    detail: "Which DB?".to_owned(),
                    danger: None,
                    created_at: now_ms(),
                },
                tx_ask,
                Some(json!({ "questions": [{ "question": "Which DB?", "options": [{ "label": "SQLite" }] }] })),
            );
            office.register_hold_with_input(
                station_core::state::sessions::PendingPerm {
                    id: "plan-1".to_owned(),
                    sess: "s1".to_owned(),
                    name: "claude".to_owned(),
                    kind: "plan".to_owned(),
                    tool_type: "plan".to_owned(),
                    label: "Plan".to_owned(),
                    detail: "## Plan".to_owned(),
                    danger: None,
                    created_at: now_ms(),
                },
                tx_plan,
                Some(json!({ "plan": "## Plan" })),
            );
            office.register_hold(
                station_core::state::sessions::PendingPerm {
                    id: "den-1".to_owned(),
                    sess: "s1".to_owned(),
                    name: "claude".to_owned(),
                    kind: "denied".to_owned(),
                    tool_type: "bash".to_owned(),
                    label: "git push".to_owned(),
                    detail: "git push".to_owned(),
                    danger: None,
                    created_at: now_ms(),
                },
                tx_den,
            );
            let card = office.chat.ci_card(
                "s1",
                "claude",
                json!({ "state": "red", "wf": "Rust CI", "repo": "agents-station", "branch": "main", "sid": "s1" }),
                now_ms(),
            );
            card.id
        };

        // Verify snapshot_json exposes tool_input for ask/plan holds
        let snap = app_state.office.lock().unwrap().snapshot_json(true, "en");
        let ask_wire = snap["perms"]
            .as_array()
            .unwrap()
            .iter()
            .find(|p| p["id"] == "ask-1")
            .expect("ask-1 in snapshot");
        assert!(ask_wire.get("tool_input").is_some());

        // Emit all Task 5 UiActions
        {
            let mut ui = app.world_mut().resource_mut::<UiState>();
            let mut ans = serde_json::Map::new();
            ans.insert("Which DB?".to_owned(), Value::String("SQLite".to_owned()));
            ui.push_action(UiAction::SubmitAskAnswers {
                id: "ask-1".to_owned(),
                answers: ans,
            });
            ui.push_action(UiAction::DecidePlan {
                id: "plan-1".to_owned(),
                approve: false,
                feedback: "Add tests".to_owned(),
            });
            ui.push_action(UiAction::DecideDenied {
                id: "den-1".to_owned(),
                retry: true,
            });
            ui.push_action(UiAction::AddSessionPin {
                session_id: "s1".to_owned(),
                text: "Keep zero warnings".to_owned(),
            });
            ui.push_action(UiAction::SteerSession {
                session_id: "s1".to_owned(),
                text: "Also check clippy".to_owned(),
            });
            ui.push_action(UiAction::CiFix { id: ci_id.clone() });
        }
        app.update();

        let ask_resp = rx_ask.try_recv().expect("ask hold resolved");
        assert_eq!(
            ask_resp["hookSpecificOutput"]["updatedInput"]["answers"]["Which DB?"],
            "SQLite"
        );
        let plan_resp = rx_plan.try_recv().expect("plan hold resolved");
        assert_eq!(plan_resp["hookSpecificOutput"]["permissionDecision"], "deny");
        assert_eq!(
            plan_resp["hookSpecificOutput"]["permissionDecisionReason"],
            "Add tests"
        );
        let den_resp = rx_den.try_recv().expect("denied hold resolved");
        assert_eq!(den_resp["hookSpecificOutput"]["retry"], true);

        {
            let office = app_state.office.lock().unwrap();
            assert_eq!(
                office.pins.get("s1").map(|v| v.as_slice()),
                Some(&["Keep zero warnings".to_owned()][..])
            );
            let ci_msg = office.chat.by_id(&ci_id).expect("ci card");
            assert!(ci_msg.ci.as_ref().unwrap().get("fix").is_some());
        }
        assert!(
            app_state.steer_queue.lock().unwrap().take("s1").is_some(),
            "SteerSession must enqueue steer note"
        );

        // Remove pin
        app.world_mut()
            .resource_mut::<UiState>()
            .push_action(UiAction::RemoveSessionPin {
                session_id: "s1".to_owned(),
                index: 0,
            });
        app.update();
        assert!(!app_state.office.lock().unwrap().pins.contains_key("s1"));
    }

    #[test]
    fn in_process_sync_redacts_and_clips_tool_input_and_rejects_busy_undo() {
        let dir = tempfile::tempdir().unwrap();
        let bridge = LiveServerBridge::connect_or_start(
            dir.path().to_path_buf(),
            dir.path().to_path_buf(),
            "127.0.0.1".to_owned(),
            0,
            false,
            false,
        );
        let app_state = match &bridge.backend {
            BridgeBackend::InProcess { app_state, .. } => app_state.clone(),
            _ => panic!("expected InProcess, got {:?}", bridge.mode()),
        };

        let target_file = dir.path().join("kept.txt");
        std::fs::write(&target_file, "v1").unwrap();

        let (tx, _rx) = tokio::sync::oneshot::channel();
        {
            let mut office = app_state.office.lock().unwrap();
            office.ingest_raw_event(json!({
                "session_id": "s-busy",
                "hook_event_name": "SessionStart",
                "cwd": dir.path().to_string_lossy()
            }));
            office.ingest_raw_event(json!({
                "session_id": "s-busy",
                "hook_event_name": "UserPromptSubmit",
                "prompt": "edit kept.txt"
            }));
            office.ingest_raw_event(json!({
                "session_id": "s-busy",
                "hook_event_name": "PreToolUse",
                "tool_name": "Edit",
                "tool_input": { "file_path": target_file.to_string_lossy() }
            }));
            std::fs::write(&target_file, "v2").unwrap();

            let huge = "plan step line\n".repeat(1500);
            office.register_hold_with_input(
                station_core::state::sessions::PendingPerm {
                    id: "plan-secret".to_owned(),
                    sess: "s-busy".to_owned(),
                    name: "claude".to_owned(),
                    kind: "plan".to_owned(),
                    tool_type: "plan".to_owned(),
                    label: "Plan".to_owned(),
                    detail: "Plan".to_owned(),
                    danger: None,
                    created_at: now_ms(),
                },
                tx,
                Some(json!({
                    "plan": format!("Use sk-secret-12345678901234567890 and {huge}")
                })),
            );
        }

        let mut ui = UiState::new_live(
            station_3d::OfficeLayout::Pro,
            station_3d::BackdropKind::City,
            false,
        );
        sync_office_core_to_ui(&app_state, &HashSet::new(), &mut ui);
        let card = ui
            .pending_perms
            .iter()
            .find(|p| p.id == "plan-secret")
            .expect("plan card synced");
        let plan_text = card
            .tool_input
            .as_ref()
            .and_then(|v| v.get("plan"))
            .and_then(Value::as_str)
            .expect("plan string");
        assert!(!plan_text.contains("sk-secret-12345678901234567890"));
        assert!(plan_text.ends_with('…'));
        assert!(
            plan_text.chars().count() <= station_core::redact::TOOL_INPUT_WIRE_MAX_STR_CHARS
        );

        let assets = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../assets");
        let mut app = App::new();
        app.insert_resource(Time::<()>::default())
            .insert_resource(ClearColor::default())
            .insert_resource(bridge)
            .insert_resource(ui)
            .insert_resource(I18nStore::load_from_assets(&assets, "en"))
            .add_event::<AppExit>()
            .add_systems(Update, process_ui_actions);

        app.world_mut()
            .resource_mut::<UiState>()
            .push_action(UiAction::ApplySessionUndo {
                session_id: "s-busy".to_owned(),
                files: vec![target_file.to_string_lossy().into_owned()],
            });
        app.update();

        let undo = app
            .world()
            .resource::<UiState>()
            .session_undo
            .as_ref()
            .expect("undo state populated");
        assert_eq!(undo.error.as_deref(), Some("busy"));
        assert_eq!(std::fs::read_to_string(&target_file).unwrap(), "v2");
    }

    #[test]
    fn remote_snapshot_parses_stuck_entries_and_accounts_preserves_renamed_main() {
        let snap_lock = Arc::new(Mutex::new(RemoteSnapshot::default()));
        apply_snapshot_to_remote(
            &snap_lock,
            &json!({
                "sessions": [],
                "perms": [{
                    "id": "den-remote",
                    "sess": "s1",
                    "name": "claude",
                    "kind": "denied",
                    "type": "bash",
                    "detail": "git push",
                    "meta": { "reason": "blocked by policy" }
                }],
                "stuck": [{
                    "id": "stuck:s1",
                    "sess": "s1",
                    "name": "claude",
                    "kind": "repeat",
                    "label": "cargo test",
                    "n": 4,
                    "mins": 3,
                    "usd": 0.42
                }],
                "feed": []
            }),
        );
        let snap = snap_lock.lock().unwrap();
        let den = snap
            .pending_perms
            .iter()
            .find(|p| p.id == "den-remote")
            .expect("denied card parsed");
        assert_eq!(
            den.meta.as_ref().and_then(|m| m["reason"].as_str()),
            Some("blocked by policy")
        );
        let stuck = snap
            .pending_perms
            .iter()
            .find(|p| p.id == "stuck:s1")
            .expect("stuck card parsed");
        assert_eq!(stuck.kind, "stuck");
        assert_eq!(stuck.summary, "cargo test");
        assert_eq!(
            stuck.meta.as_ref().and_then(|m| m["stuck_n"].as_u64()),
            Some(4)
        );

        let accts = parse_accounts_from_view(&json!({
            "active": "",
            "accounts": [
                { "name": "", "label": "Work Personal", "active": true },
                { "name": "client-b", "label": "Client B", "active": false }
            ]
        }));
        assert_eq!(accts.len(), 2);
        assert_eq!(accts[0].slug, "");
        assert_eq!(accts[0].label, "Work Personal");
        assert!(accts[0].active);
        assert_eq!(accts[1].slug, "client-b");
        assert_eq!(accts[1].label, "Client B");
    }

    #[test]
    fn bind_error_actions_emit_single_offline_toast_without_success_toast() {
        let dir = tempfile::tempdir().unwrap();
        let harnesses = Arc::new(HarnessService::new_win32(
            dir.path().to_path_buf(),
            PathBuf::from("agents-station.exe"),
            4310,
        ));
        let harness_poller = HarnessPoller::spawn(harnesses.clone(), HARNESS_POLL_INTERVAL);
        let (toast_tx, toast_rx) = mpsc::channel(TOAST_QUEUE_CAP);
        let (async_tx, async_rx) = mpsc::channel(TOAST_QUEUE_CAP);
        let bridge = LiveServerBridge {
            backend: BridgeBackend::BindError {
                message: "offline".to_owned(),
                harnesses,
                harness_poller,
            },
            data_dir: dir.path().to_path_buf(),
            assets_dir: dir.path().to_path_buf(),
            port: 4310,
            sync_timer: Timer::from_seconds(1.0, TimerMode::Repeating),
            sys_sync_timer: Timer::from_seconds(4.0, TimerMode::Repeating),
            sys_refresh_due: false,
            banner_pushed: true,
            seen_perm_ids: HashSet::new(),
            dismissed_ci_ids: HashSet::new(),
            last_window_focused: None,
            toast_tx,
            toast_rx,
            async_tx,
            async_rx,
        };

        let assets = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../assets");
        let mut app = App::new();
        app.insert_resource(Time::<()>::default())
            .insert_resource(ClearColor::default())
            .insert_resource(bridge)
            .insert_resource(UiState::new_live(
                station_3d::OfficeLayout::Pro,
                station_3d::BackdropKind::City,
                false,
            ))
            .insert_resource(I18nStore::load_from_assets(&assets, "en"))
            .add_event::<AppExit>()
            .add_systems(Update, process_ui_actions);

        {
            let mut ui = app.world_mut().resource_mut::<UiState>();
            ui.push_action(UiAction::DecidePlan {
                id: "p1".to_owned(),
                approve: true,
                feedback: String::new(),
            });
            ui.push_action(UiAction::StopSessionTask {
                session_id: "s1".to_owned(),
            });
        }
        app.update();

        let ui = app.world().resource::<UiState>();
        assert_eq!(
            ui.toasts.len(),
            2,
            "each offline action must emit exactly 1 offline toast, got {:?}",
            ui.toasts
        );
    }

    #[test]
    fn task6_in_process_group_chat_whiteboard_wrapped_and_timelapse_end_to_end() {
        let dir = tempfile::tempdir().unwrap();
        let data_dir = dir.path().to_path_buf();
        let assets_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../assets");

        let bridge = LiveServerBridge::connect_or_start(
            data_dir.clone(),
            assets_dir.clone(),
            "127.0.0.1".to_owned(),
            0,
            false,
            false,
        );
        let state = bridge.in_process_state().expect("InProcess state");

        let mut app = App::new();
        app.insert_resource(Time::<()>::default())
            .insert_resource(ClearColor::default())
            .insert_resource(UiState::with_demo_seed(
                station_3d::OfficeLayout::Pro,
                station_3d::BackdropKind::City,
                false,
            ))
            .insert_resource(I18nStore::load_from_assets(&assets_dir, "en"))
            .insert_resource(bridge)
            .add_event::<AppExit>()
            .add_systems(Update, process_ui_actions);

        // 1. GroupChat: send message, create poll, send sticker, send photo, sync whiteboard, export wrapped, trigger timelapse
        {
            let mut ui = app.world_mut().resource_mut::<UiState>();
            ui.push_action(UiAction::SendGroupChat {
                text: "Hello @Claude".to_owned(),
                reply_to: None,
                sticker: None,
            });
            ui.push_action(UiAction::SendGroupChat {
                text: String::new(),
                reply_to: None,
                sticker: Some(3),
            });
            ui.push_action(UiAction::CreateGroupPoll {
                q: "Lunch?".to_owned(),
                opts: vec!["Pho".to_owned(), "Com tam".to_owned()],
            });
            ui.push_action(UiAction::SendGroupPhoto);
            ui.push_action(UiAction::SyncWhiteboardTodos(vec![
                TodoItem {
                    id: "td-1".to_owned(),
                    text: "Ship Wrapped PNG".to_owned(),
                    done: false,
                    done_at: 0,
                },
                TodoItem {
                    id: "td-2".to_owned(),
                    text: "Verify Timelapse GIF".to_owned(),
                    done: true,
                    done_at: 2,
                },
            ]));
            ui.push_action(UiAction::ExportWrappedPng {
                copy_clipboard: false,
            });
            ui.push_action(UiAction::TriggerTimelapseClip {
                shape: ClipShape::Square,
                music: true,
            });
        }
        app.update();

        let (text_id, poll_id) = {
            let office = state.office.lock().unwrap();
            let text_id = office
                .chat
                .msgs
                .iter()
                .find(|m| m.text == "Hello @Claude")
                .expect("text message persisted")
                .id
                .clone();
            let poll_id = office
                .chat
                .msgs
                .iter()
                .find(|m| m.kind == "poll")
                .expect("poll message persisted")
                .id
                .clone();
            assert!(
                office.chat.msgs.iter().any(|m| m.kind == "sticker" && m.sticker == Some(3)),
                "sticker message persisted"
            );
            assert!(
                office.chat.msgs.iter().any(|m| m.kind == "photo" && m.photo.is_some()),
                "photo message persisted"
            );
            assert_eq!(office.todos.len(), 2, "SyncWhiteboardTodos updates office.todos");
            (text_id, poll_id)
        };

        // 2. React, vote poll, reply, and recall own message
        {
            let mut ui = app.world_mut().resource_mut::<UiState>();
            ui.push_action(UiAction::ReactGroupChat {
                id: text_id.clone(),
                emoji: "❤️".to_owned(),
            });
            ui.push_action(UiAction::VoteGroupPoll {
                id: poll_id.clone(),
                option_index: 1,
            });
            ui.push_action(UiAction::SendGroupChat {
                text: "Replying now".to_owned(),
                reply_to: Some(text_id.clone()),
                sticker: None,
            });
            ui.push_action(UiAction::RecallGroupChat {
                id: text_id.clone(),
            });
        }
        app.update();

        {
            let office = state.office.lock().unwrap();
            assert!(
                office.chat.by_id(&text_id).expect("msg").recalled,
                "RecallGroupChat must recall own message"
            );
            let poll_msg = office.chat.by_id(&poll_id).expect("poll");
            assert_eq!(
                poll_msg.poll.as_ref().unwrap().opts[1].by,
                vec!["me".to_owned()],
                "VoteGroupPoll must record vote on option 1"
            );
            assert!(
                office
                    .chat
                    .msgs
                    .iter()
                    .any(|m| m.text == "Replying now"
                        && m.reply.as_ref().is_some_and(|r| r.id == text_id)),
                "Reply message must preserve reply_ref"
            );
        }

        // 3. Verify 1080x1350 Wrapped PNG (exported off-thread) + Timelapse GIF + procedural lo-fi WAV on disk
        let shots_dir = data_dir.join("shots");
        let deadline = std::time::Instant::now() + Duration::from_secs(5);
        let mut wrapped_png = None;
        while std::time::Instant::now() < deadline {
            if let Ok(rd) = std::fs::read_dir(&shots_dir) {
                for entry in rd.flatten() {
                    let p = entry.path();
                    if p.file_name()
                        .and_then(|s| s.to_str())
                        .is_some_and(|n| n.starts_with("wrapped-") && n.ends_with(".png"))
                        && p.metadata().is_ok_and(|m| m.len() > 100)
                    {
                        wrapped_png = Some(p);
                        break;
                    }
                }
            }
            if wrapped_png.is_some() {
                break;
            }
            std::thread::sleep(Duration::from_millis(25));
        }
        let wrapped_png = wrapped_png.expect("wrapped PNG must be written to data/shots/");
        let wrapped_bytes = std::fs::read(&wrapped_png).unwrap();
        assert!(wrapped_bytes.starts_with(b"\x89PNG\r\n\x1a\n"));
        let img = image::load_from_memory(&wrapped_bytes).unwrap();
        assert_eq!((img.width(), img.height()), (1080, 1350));

        let files: Vec<PathBuf> = std::fs::read_dir(&shots_dir)
            .unwrap()
            .flatten()
            .map(|e| e.path())
            .collect();
        let gif_file = files
            .iter()
            .find(|p| p.extension().and_then(|s| s.to_str()) == Some("gif"))
            .expect("timelapse GIF must be written to data/shots/");
        let gif_bytes = std::fs::read(gif_file).unwrap();
        assert!(gif_bytes.starts_with(b"GIF89a") || gif_bytes.starts_with(b"GIF87a"));

        let wav_file = files
            .iter()
            .find(|p| p.extension().and_then(|s| s.to_str()) == Some("wav"))
            .expect("timelapse lo-fi WAV must be written to data/shots/");
        let wav_bytes = std::fs::read(wav_file).unwrap();
        assert_eq!(&wav_bytes[0..4], b"RIFF");
        assert_eq!(&wav_bytes[8..12], b"WAVE");
    }

    #[test]
    fn task6_review_guards_bound_image_decode_sanitize_wrapped_date_and_redact_todos() {
        // 1. Oversized / invalid image bytes fail safely
        let oversized = vec![0u8; MAX_CHAT_IMAGE_BYTES + 1];
        assert!(decode_bounded_chat_preview("big".to_owned(), &oversized).is_err());
        assert!(decode_bounded_chat_preview("bad".to_owned(), b"not-an-image").is_err());
        assert!(decode_chat_data_url("bad".to_owned(), "data:text/html;base64,PGI+").is_err());

        // 2. Valid data-URL photo generated by render_group_chat_photo_data_url decodes cleanly
        let assets_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../assets");
        let i18n = I18nStore::load_from_assets(&assets_dir, "en");
        let mut ui_demo = UiState::with_demo_seed(
            station_3d::OfficeLayout::Pro,
            station_3d::BackdropKind::City,
            false,
        );
        ui_demo.streamer_mode = true;
        let data_url = render_group_chat_photo_data_url(&ui_demo, &i18n, None, &assets_dir)
            .expect("valid JPEG data URL");
        assert!(data_url.starts_with("data:image/jpeg;base64,"));
        let prev = decode_chat_data_url("photo-1".to_owned(), &data_url)
            .expect("data URL must decode cleanly through bounded decoder");
        assert!(prev.width > 0 && prev.width <= 360);
        assert!(prev.height > 0 && prev.height <= 270);

        // 3. Wrapped filename date tag sanitization blocks traversal
        assert_eq!(sanitize_wrapped_date_tag("2026-11-26"), "2026-11-26");
        assert_eq!(sanitize_wrapped_date_tag("../../etc/passwd"), "etcpasswd");
        assert_eq!(sanitize_wrapped_date_tag("///..."), "latest");
        assert_eq!(sanitize_wrapped_date_tag(""), "latest");
    }

    #[test]
    fn task6_attached_http_persists_chat_poll_todos_and_fetches_wrapped() {
        let dir = tempfile::tempdir().unwrap();
        let data_dir = dir.path().to_path_buf();
        let assets_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../assets");

        let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        let port = listener.local_addr().unwrap().port();
        drop(listener);

        let daemon = LiveServerBridge::connect_or_start(
            data_dir.clone(),
            assets_dir.clone(),
            "127.0.0.1".to_owned(),
            port,
            true,
            false,
        );
        let daemon_state = daemon.in_process_state().expect("daemon InProcess");

        let attached = LiveServerBridge::connect_or_start(
            data_dir.clone(),
            assets_dir.clone(),
            "127.0.0.1".to_owned(),
            port,
            true,
            true,
        );
        assert_eq!(attached.mode(), BridgeMode::AttachedHttp);

        let mut app = App::new();
        app.insert_resource(Time::<()>::default())
            .insert_resource(ClearColor::default())
            .insert_resource(UiState::new_live(
                station_3d::OfficeLayout::Pro,
                station_3d::BackdropKind::City,
                false,
            ))
            .insert_resource(I18nStore::load_from_assets(&assets_dir, "en"))
            .insert_resource(attached)
            .add_event::<AppExit>()
            .add_systems(Update, (sync_live_state_to_ui, process_ui_actions).chain());

        {
            let mut ui = app.world_mut().resource_mut::<UiState>();
            ui.push_action(UiAction::CreateGroupPoll {
                q: "Attached poll?".to_owned(),
                opts: vec!["Yes".to_owned(), "No".to_owned()],
            });
            ui.push_action(UiAction::SyncWhiteboardTodos(vec![TodoItem {
                id: "td-http-1".to_owned(),
                text: "Sync over HTTP".to_owned(),
                done: false,
                done_at: 0,
            }]));
            ui.push_action(UiAction::FetchWrapped);
        }
        app.update();

        let deadline = std::time::Instant::now() + Duration::from_secs(4);
        let mut all_synced = false;
        while std::time::Instant::now() < deadline {
            app.update();
            let (poll_ok, todo_ok) = {
                let office = daemon_state.office.lock().unwrap();
                let p = office
                    .chat
                    .msgs
                    .iter()
                    .any(|m| m.kind == "poll" && m.text.contains("Attached poll?"));
                let t = office
                    .todos
                    .iter()
                    .any(|v| v.get("text").and_then(Value::as_str) == Some("Sync over HTTP"));
                (p, t)
            };
            let wrapped_ok = app.world().resource::<UiState>().wrapped.loaded;
            if poll_ok && todo_ok && wrapped_ok {
                all_synced = true;
                break;
            }
            std::thread::sleep(Duration::from_millis(50));
        }
        assert!(
            all_synced,
            "AttachedHttp must persist poll (/api/chat), todos (/api/todos), and load /api/wrapped"
        );
        drop(daemon);
    }

    #[test]
    fn task7_attached_http_fetches_burn_vault_accounts_phone_and_updates_pin() {
        let dir = tempfile::tempdir().unwrap();
        let data_dir = dir.path().to_path_buf();
        let assets_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../assets");

        let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        let port = listener.local_addr().unwrap().port();
        drop(listener);

        let daemon = LiveServerBridge::connect_or_start(
            data_dir.clone(),
            assets_dir.clone(),
            "127.0.0.1".to_owned(),
            port,
            true,
            false,
        );
        let daemon_state = daemon.in_process_state().expect("daemon InProcess");
        let old_pin = daemon_state.auth.current_pin();

        let attached = LiveServerBridge::connect_or_start(
            data_dir.clone(),
            assets_dir.clone(),
            "127.0.0.1".to_owned(),
            port,
            true,
            true,
        );
        assert_eq!(attached.mode(), BridgeMode::AttachedHttp);

        let mut app = App::new();
        app.insert_resource(Time::<()>::default())
            .insert_resource(ClearColor::default())
            .insert_resource(UiState::new_live(
                station_3d::OfficeLayout::Pro,
                station_3d::BackdropKind::City,
                false,
            ))
            .insert_resource(I18nStore::load_from_assets(&assets_dir, "en"))
            .insert_resource(attached)
            .add_event::<AppExit>()
            .add_systems(Update, (sync_live_state_to_ui, process_ui_actions).chain());

        {
            let mut ui = app.world_mut().resource_mut::<UiState>();
            ui.push_action(UiAction::FetchBurn(BurnRange::Days30));
            ui.push_action(UiAction::FetchVaultList {
                query: String::new(),
                fresh: false,
                seq: 1,
            });
            ui.push_action(UiAction::FetchVaultReport);
            ui.push_action(UiAction::FetchAiAccounts);
            ui.push_action(UiAction::FetchPhoneStatus);
            ui.push_action(UiAction::ToggleLan(true));
            ui.push_action(UiAction::TogglePhoneApprove(true));
            ui.push_action(UiAction::SetSysAutoClose(4));
            ui.push_action(UiAction::UpdatePin { pin: None });
            ui.push_action(UiAction::TestWebPush);
        }
        app.update();

        let expected_push_none = app
            .world()
            .resource::<I18nStore>()
            .tr("topbar.phone.pushNone");
        let deadline = std::time::Instant::now() + Duration::from_secs(6);
        let mut all_ok = false;
        while std::time::Instant::now() < deadline {
            app.update();
            let ui = app.world().resource::<UiState>();
            let burn_ok = ui.burn_drawer.loaded_range == Some(BurnRange::Days30);
            let vault_ok = ui.vault_drawer.loaded && ui.vault_drawer.report.loaded;
            let accts_ok = ui.ai_accounts.loaded;
            let phone_ok = ui.phone_modal.loaded
                && ui.phone_modal.lan_on
                && ui.phone_modal.phone_approve
                && ui.pairing_pin.len() == 6
                && ui.pairing_pin != old_pin;
            let push_none_ok = ui.toasts.iter().any(|t| t.body == expected_push_none);
            let auto_ok = daemon_state.prefs.lock().unwrap().auto_close() == 4;
            if burn_ok && vault_ok && accts_ok && phone_ok && push_none_ok && auto_ok {
                all_ok = true;
                break;
            }
            std::thread::sleep(Duration::from_millis(50));
        }
        assert!(
            all_ok,
            "AttachedHttp must load Burn 30d, Vault + Report, AI Accounts, Phone status, LAN/Approve/AutoClose, toast pushNone when sent==0, and regenerate PIN"
        );

        // Regression check (#3): next sync tick applying RemoteSnapshot MUST NOT overwrite the new PIN back to old_pin.
        let new_pin = app.world().resource::<UiState>().pairing_pin.clone();
        {
            let snap_clone = match &app.world().resource::<LiveServerBridge>().backend {
                BridgeBackend::AttachedHttp { snapshot, .. } => {
                    snapshot.lock().unwrap().clone()
                }
                _ => panic!("expected AttachedHttp"),
            };
            let mut ui = app.world_mut().resource_mut::<UiState>();
            apply_remote_snapshot_to_ui(&snap_clone, &mut ui);
            assert_eq!(
                ui.pairing_pin, new_pin,
                "RemoteSnapshot must carry the updated PIN so sync_timer never reverts it"
            );
        }
        drop(daemon);
    }

    #[test]
    fn parse_ai_slots_parses_windows_and_rows_and_localizes_error_codes() {
        let assets_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../assets");
        let payload = json!({
            "auto": true,
            "accounts": [
                {
                    "name": "",
                    "label": "Main",
                    "active": true,
                    "windows": [
                        { "key": "session", "label": "5h", "pct": 42.5, "resetsAt": 1700000000000u64 },
                        { "key": "weekly", "label": "7d", "pct": 78.0, "resetsAt": 1700500000000u64 }
                    ]
                }
            ],
            "codex": {
                "auto": false,
                "accounts": [
                    {
                        "slug": "cx1",
                        "label": "Codex",
                        "active": true,
                        "rows": [{ "key": "session", "label": "5h", "used": 61.0 }]
                    }
                ]
            },
            "ag": {
                "auto": true,
                "away": true,
                "pending": "ag-2",
                "accounts": [
                    {
                        "slug": "ag-1",
                        "label": "AG",
                        "active": true,
                        "windows": [{ "key": "model", "label": "Claude", "pct": 88.0 }]
                    }
                ]
            }
        });
        let parsed = parse_ai_accounts_from_json(&payload);
        match parsed {
            BridgeAsyncResult::AiAccountsData { claude, codex, ag, ag_away, ag_pending, .. } => {
                assert_eq!(claude[0].windows.len(), 2);
                assert_eq!(claude[0].windows[0].key, "session");
                assert!((claude[0].windows[0].pct - 42.5).abs() < 0.01);
                assert_eq!(codex[0].windows.len(), 1);
                assert!((codex[0].windows[0].pct - 61.0).abs() < 0.01);
                assert_eq!(ag[0].windows.len(), 1);
                assert!(ag_away);
                assert_eq!(ag_pending.as_deref(), Some("ag-2"));
            }
            _ => panic!("expected AiAccountsData"),
        }

        for lang in ["en", "vi", "th"] {
            let tr = I18nStore::load_from_assets(&assets_dir, lang);
            for code in ["noCopy", "inSeat", "badName", "noAccount", "noDb"] {
                let resolved = resolve_ai_error_code(&tr, code);
                assert!(
                    resolved.is_some(),
                    "account error code '{code}' must resolve in locale '{lang}'"
                );
            }
        }
    }

    #[test]
    fn task7_all_i18n_keys_resolve_cleanly_in_en_vi_th() {
        let assets_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../assets");
        let keys = [
            // Bridge handlers
            "burn.title",
            "burn.unreachable",
            "vault.title",
            "vault.tabReport",
            "vault.gotIt",
            "drawer.unreachable",
            "drawer.copyFailed",
            "ui.macDrawer.title",
            "mac.action.done",
            "mac.action.failed",
            "mac.action.unreachable",
            "mac.auto.label",
            "mac.auto.savedOff",
            "mac.auto.saved",
            "mac.auto.hours",
            "mac.stranded.title",
            "mac.stranded.open",
            "mac.stranded.gone",
            "ai.title",
            "ai.failed",
            "ai.ag.pending",
            "ai.ag.cancelPending",
            "ai.ag.awayOk",
            "ui.phoneModal.title",
            "topbar.phone.isOn",
            "topbar.phone.isOff",
            "topbar.phone.tabAway",
            "topbar.phone.tunStarting",
            "topbar.phone.fixedTitle",
            "topbar.phone.approve",
            "topbar.phone.pinTitle",
            "topbar.phone.pinSaved",
            "topbar.phone.notifySent",
            "topbar.phone.pushNone",
            "server.push.testTitle",
            "server.push.testBody",
            // Newly added station.* keys for Task 7
            "station.burn30d",
            "station.privateWs",
            "station.addAppTitle",
            "station.addAppDesc",
            "station.addAppId",
            "station.addAppLabel",
            "station.addAppPath",
            "station.addAppSave",
            "station.addAppAdded",
            "station.customApps",
            "station.remove",
            "station.saveSeat",
            "station.moveUp",
            "station.moveDown",
            "station.emailLabel",
            "station.accountsOpen",
            "station.pinWeak",
            "station.kb",
            "station.cpu",
            "station.loadEarlier",
            "station.streamerRedacted",
        ];
        for lang in ["en", "vi", "th"] {
            let tr = I18nStore::load_from_assets(&assets_dir, lang);
            for key in keys {
                let val = tr.tr(key);
                assert_ne!(
                    val, key,
                    "i18n key '{key}' failed to resolve in locale '{lang}'"
                );
                assert!(
                    !val.trim().is_empty(),
                    "i18n key '{key}' resolved to empty string in locale '{lang}'"
                );
            }
        }
    }

    #[test]
    fn sync_live_sessions_to_3d_writes_clamped_cpu_load_into_server_rack_telemetry() {
        let mut app = App::new();
        let mut ui = UiState::default();
        ui.sys.cpu_pct = 64.0;
        app.insert_resource(ui);
        app.insert_resource(station_3d::ServerRackTelemetry::default());
        app.add_systems(Update, sync_live_sessions_to_3d);

        app.update();
        let load = app.world().resource::<station_3d::ServerRackTelemetry>().cpu_load;
        assert!(
            (load - 0.64).abs() < 1e-4,
            "expected cpu_load 0.64 from ui.sys.cpu_pct=64.0, got {load}"
        );

        app.world_mut().resource_mut::<UiState>().sys.cpu_pct = 145.0;
        app.update();
        assert_eq!(
            app.world().resource::<station_3d::ServerRackTelemetry>().cpu_load,
            1.0,
            "cpu_load must clamp to 1.0 above 100%"
        );

        app.world_mut().resource_mut::<UiState>().sys.cpu_pct = -5.0;
        app.update();
        assert_eq!(
            app.world().resource::<station_3d::ServerRackTelemetry>().cpu_load,
            0.0,
            "cpu_load must clamp to 0.0 below 0%"
        );
    }

    #[test]
    fn sync_live_sessions_to_3d_populates_props_live_telemetry_for_cups_stacks_and_dev_servers() {
        let mut app = App::new();
        let mut ui = UiState::default();
        ui.sessions.push(SessionCard {
            id: "sess-props-1".into(),
            agent: "claude".into(),
            project: "agents-station".into(),
            dir: r"D:\mystation\agents-station".into(),
            status: "rest".into(),
            desk: 2,
            ctx_pct: 0.91,
            cache_warmth: CacheWarmth::Warm(4),
            ..Default::default()
        });
        ui.mac_drawer.dev_servers.push(DevServerUi {
            pid: 4242,
            start: 0,
            label: "vite".into(),
            ports: vec![5173],
            dir: r"d:\mystation\agents-station".into(),
            up_sec: 120,
            cpu: 28,
            mem_bytes: 0,
            conns: 1,
            idle_sec: 0,
        });
        ui.mac_drawer.dev_servers.push(DevServerUi {
            pid: 9001,
            start: 0,
            label: "orphan-api".into(),
            ports: vec![8080],
            dir: r"d:\other\closed-repo".into(),
            up_sec: 500,
            cpu: 5,
            mem_bytes: 0,
            conns: 0,
            idle_sec: 300,
        });
        app.insert_resource(ui);
        app.insert_resource(station_3d::PropsLiveTelemetry::default());
        app.add_systems(Update, sync_live_sessions_to_3d);

        app.update();
        let tel = app.world().resource::<station_3d::PropsLiveTelemetry>();
        assert!(tel.live_synced);
        assert_eq!(tel.desks.len(), 1);
        assert_eq!(tel.desks[0].desk_index, 2);
        assert!(tel.desks[0].resting);
        assert!((tel.desks[0].ctx_pct - 0.91).abs() < 1e-4);
        assert!((tel.desks[0].cache_warmth - 0.8).abs() < 1e-4);
        assert_eq!(tel.dev_servers.len(), 2);
        assert!(tel.dev_servers[0].alive);
        assert_eq!(tel.dev_servers[0].desk_index, Some(2));
        assert_eq!(tel.dev_servers[0].port, Some(5173));
        assert!(!tel.dev_servers[1].alive);
        assert_eq!(tel.dev_servers[1].desk_index, None);
    }
}
