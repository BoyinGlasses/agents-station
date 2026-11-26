use crate::fps::AdaptiveFpsState;
use crate::tray::{TrayState, notify_parent_hold_alert, notify_parent_in_tray};
use bevy::app::AppExit;
use bevy::ecs::system::SystemParam;
use bevy::prelude::*;
use bevy::render::camera::{RenderTarget, Viewport};
use bevy::render::view::screenshot::{Screenshot, save_to_disk};
use bevy::window::PrimaryWindow;
use bevy_egui::egui;
use chrono::{Local, TimeZone};
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
    BurnSnapshot, ChatMessage, FeedEntry, HarnessDockItem, PendingPermCard, PlaceCardAction,
    PlaceCardInfo, PlaceCardRow, ReplayRange, SessionCard, SysSnapshot, UiAction,
    normalize_feed_kind, normalize_status,
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
}

struct RemoteActionCmd {
    path: &'static str,
    body: Value,
    refresh: RefreshKind,
    launch_toast_label: Option<String>,
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
    burn: Option<BurnSnapshot>,
    has_chat: bool,
    chat: Vec<ChatMessage>,
    has_vault: bool,
    vault_results: Vec<(String, String)>,
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
            Some(SessionCard {
                id,
                agent,
                project,
                status,
                model,
                last_prompt,
                tool,
                tool_kind,
                desk,
                cost_usd,
                tokens,
                pinned: false,
                age_label: String::new(),
                ctx_pct,
                subagents: Vec::new(),
            })
        })
        .collect()
}

fn parse_perms_from_json(val: &Value) -> Vec<PendingPermCard> {
    let Some(arr) = val.get("perms").and_then(Value::as_array) else {
        return Vec::new();
    };
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
            })
        })
        .collect()
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
    let mut snap = snap_lock.lock().unwrap_or_else(|e| e.into_inner());
    snap.has_snapshot = true;
    snap.sessions = sessions;
    snap.pending_perms = pending_perms;
    snap.feed = feed;
    if let Some(p) = pin {
        snap.pairing_pin = Some(p);
    }
}

fn parse_chat_from_json(val: &Value) -> Vec<ChatMessage> {
    let Some(arr) = val.get("msgs").and_then(Value::as_array) else {
        return Vec::new();
    };
    arr.iter()
        .rev()
        .take(40)
        .collect::<Vec<_>>()
        .into_iter()
        .rev()
        .filter_map(|m| {
            let text = m.get("text").and_then(Value::as_str).unwrap_or("");
            if text.is_empty() {
                return None;
            }
            let sender = m
                .get("name")
                .and_then(Value::as_str)
                .or_else(|| m.get("from").and_then(Value::as_str))
                .unwrap_or("Office")
                .to_owned();
            let role = m
                .get("kind")
                .and_then(Value::as_str)
                .unwrap_or("chat")
                .to_owned();
            let ts = format_hhmm_ms(m.get("at").and_then(Value::as_u64).unwrap_or(0));
            Some(ChatMessage {
                sender,
                role,
                text: text.to_owned(),
                ts,
            })
        })
        .collect()
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

fn current_process_private_mb() -> f32 {
    0.0
}

fn apply_remote_snapshot_to_ui(snap: &RemoteSnapshot, ui: &mut UiState) {
    if !snap.harnesses.is_empty() {
        ui.harnesses = snap.harnesses.clone();
    }
    if snap.has_snapshot {
        ui.sessions = snap.sessions.clone();
        ui.pending_perms = snap.pending_perms.clone();
        ui.feed = snap.feed.clone();
        if let Some(sel) = ui.selected_session.as_deref()
            && !ui.sessions.iter().any(|s| s.id == sel)
        {
            ui.selected_session = None;
        }
    }
    if let Some(pin) = &snap.pairing_pin {
        ui.pairing_pin = pin.clone();
    }
    if let Some(sys) = &snap.sys {
        ui.sys = sys.clone();
    }
    if let Some(burn) = &snap.burn {
        ui.burn = burn.clone();
    }
    if snap.has_chat {
        ui.chat = snap.chat.clone();
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
        let chat = parse_chat_from_json(&val);
        let mut snap = snap_lock.lock().unwrap_or_else(|e| e.into_inner());
        snap.chat = chat;
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

    let sys_v = get_json("/api/sys").await;
    let burn_today = get_json("/api/burn?range=today").await;
    let burn_30d = get_json("/api/burn?range=30d").await;
    let chat_v = get_json("/api/chat").await;
    let vault_v = get_json("/api/vault?limit=30").await;

    let mut snap = snap_lock.lock().unwrap_or_else(|e| e.into_inner());
    if let Some(v) = sys_v {
        snap.sys = Some(parse_sys_from_json(&v));
    }
    if let Some(today) = burn_today {
        snap.burn = Some(parse_burn_from_json(&today, burn_30d.as_ref()));
    }
    if let Some(v) = chat_v {
        snap.chat = parse_chat_from_json(&v);
        snap.has_chat = true;
    }
    if let Some(v) = vault_v {
        snap.vault_results = parse_vault_from_json(&v);
        snap.has_vault = true;
    }
}

fn spawn_attached_sse_client(
    base_url: String,
    host_header: String,
    snap_lock: Arc<Mutex<RemoteSnapshot>>,
    toast_tx: mpsc::Sender<(String, String)>,
) -> (mpsc::Sender<RemoteActionCmd>, watch::Sender<bool>) {
    let (action_tx, mut action_rx) = mpsc::channel::<RemoteActionCmd>(REMOTE_ACTION_QUEUE_CAP);
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
                                let resp = client_a
                                    .post(format!("{base_a}{}", cmd.path))
                                    .header("host", &host_a)
                                    .timeout(Duration::from_secs(3))
                                    .json(&cmd.body)
                                    .send()
                                    .await;
                                let ok = match resp {
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
    (action_tx, tx)
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
    last_window_focused: Option<bool>,
    toast_tx: mpsc::Sender<(String, String)>,
    toast_rx: mpsc::Receiver<(String, String)>,
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

        if prefer_attach && probe_local_health(port) {
            let base_url = format!("http://127.0.0.1:{port}");
            let host_header = format!("127.0.0.1:{port}");
            let snapshot = Arc::new(Mutex::new(RemoteSnapshot::default()));
            let (action_tx, shutdown) = spawn_attached_sse_client(
                base_url,
                host_header,
                snapshot.clone(),
                toast_tx.clone(),
            );
            return Self {
                backend: BridgeBackend::AttachedHttp {
                    snapshot,
                    action_tx,
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
                last_window_focused: None,
                toast_tx,
                toast_rx,
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
                    last_window_focused: None,
                    toast_tx,
                    toast_rx,
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
                last_window_focused: None,
                toast_tx,
                toast_rx,
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
                    last_window_focused: None,
                    toast_tx,
                    toast_rx,
                }
            }
            Err(err) => {
                // Re-check once in case another Agents Station daemon just finished binding :4310.
                if prefer_attach && probe_local_health(port) {
                    let base_url = format!("http://127.0.0.1:{port}");
                    let host_header = format!("127.0.0.1:{port}");
                    let snapshot = Arc::new(Mutex::new(RemoteSnapshot::default()));
                    let (action_tx, shutdown) = spawn_attached_sse_client(
                        base_url,
                        host_header,
                        snapshot.clone(),
                        toast_tx.clone(),
                    );
                    Self {
                        backend: BridgeBackend::AttachedHttp {
                            snapshot,
                            action_tx,
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
                        last_window_focused: None,
                        toast_tx,
                        toast_rx,
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
                        last_window_focused: None,
                        toast_tx,
                        toast_rx,
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
                    let label = if prop.id == "cat" {
                        i18n.tr("fp.petCat")
                    } else {
                        i18n.tr(&format!("places.chip.{}", prop.place))
                    };
                    nearest = Some((d, label));
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
                labels.push(ProjectedLabel {
                    id: format!("perm:{}", p.id),
                    kind: ProjectedLabelKind::PermissionBadge,
                    priority: LabelPriority::Permission,
                    screen_pos: egui::pos2(screen_pos.x, screen_pos.y - 32.0),
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
        app.insert_resource(bridge).add_systems(
            Update,
            (
                sync_live_state_to_ui,
                process_ui_actions,
                sync_live_sessions_to_3d,
                handle_picking_events,
                step_replay_system,
            )
                .chain(),
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
    ui.chat = office
        .chat
        .msgs
        .iter()
        .rev()
        .take(40)
        .collect::<Vec<_>>()
        .into_iter()
        .rev()
        .filter(|m| !m.text.is_empty())
        .map(|m| ChatMessage {
            sender: m
                .name
                .clone()
                .or_else(|| m.from.clone())
                .unwrap_or_else(|| "Office".to_owned()),
            role: m.kind.clone(),
            text: m.text.clone(),
            ts: format_hhmm_ms(m.at),
        })
        .collect();
}

fn sync_office_core_to_ui(app_state: &AppState, ui: &mut UiState) {
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
                SessionCard {
                    id: s.id.clone(),
                    agent: s.agent.clone(),
                    project: s.name.clone(),
                    status: normalize_status(&s.status).to_owned(),
                    model: s.model.clone().unwrap_or_else(|| "default".to_owned()),
                    last_prompt: s.last_prompt.clone().unwrap_or_default(),
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

        ui.pending_perms = office
            .pending_perms
            .values()
            .map(|p| PendingPermCard {
                id: p.id.clone(),
                session_id: p.sess.clone(),
                agent: p.name.clone(),
                tool: p.tool_type.clone(),
                summary: p.detail.clone(),
                kind: p.kind.clone(),
            })
            .collect();

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
    }
}

fn sync_live_state_to_ui(
    time: Res<Time>,
    mut bridge: ResMut<LiveServerBridge>,
    mut ui: ResMut<UiState>,
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
            } else if let Some(pin) = &snap.pairing_pin {
                ui.pairing_pin = pin.clone();
            }
        }
        BridgeBackend::InProcess { app_state, .. } => {
            if !ui.demo_running {
                synced_live_perms = true;
                sync_office_core_to_ui(app_state, &mut ui);
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

#[allow(clippy::too_many_arguments)]
fn sync_live_sessions_to_3d(
    mut commands: Commands,
    ui: Res<UiState>,
    pal: Option<ResMut<WorldPalette>>,
    materials: Option<ResMut<Assets<StandardMaterial>>>,
    screen_mats: Option<Res<station_3d::desks::ScreenMaterials>>,
    layout: Option<Res<station_3d::OfficeLayout>>,
    preview_q: Query<Entity, With<station_3d::people::PreviewCastWorker>>,
    mut workers_q: Query<(
        Entity,
        &mut station_3d::people::LiveSessionWorker,
        &mut station_3d::chibi::ChibiWorker,
        Option<&Children>,
    )>,
    mut halo_q: Query<
        &mut MeshMaterial3d<StandardMaterial>,
        (
            With<station_3d::chibi::StatusHalo>,
            Without<station_3d::desks::DeskScreen>,
        ),
    >,
    mut screens_q: Query<
        (
            &station_3d::desks::DeskScreen,
            &mut MeshMaterial3d<StandardMaterial>,
        ),
        Without<station_3d::chibi::StatusHalo>,
    >,
) {
    let (Some(mut pal), Some(mut mats), Some(scr_mats)) = (pal, materials, screen_mats) else {
        return;
    };
    let lay = layout.map(|l| *l).unwrap_or(ui.layout);
    let specs: Vec<station_3d::LiveWorkerSpec> = ui
        .sessions
        .iter()
        .map(|s| station_3d::LiveWorkerSpec {
            session_id: s.id.clone(),
            agent: s.agent.clone(),
            name: s.project.clone(),
            status: s.status.clone(),
            desk_hint: s.desk,
            tool_kind: s.tool_kind.clone(),
            tool_label: s.tool.clone(),
        })
        .collect();
    station_3d::people::sync_live_workers_and_screens(
        &mut commands,
        &mut pal,
        &mut mats,
        &scr_mats,
        lay,
        ui.chibi_look,
        &specs,
        &preview_q,
        &mut workers_q,
        &mut halo_q,
        &mut screens_q,
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

#[derive(SystemParam)]
struct AuxActionParams<'w, 's> {
    sfx: Option<ResMut<'w, station_3d::audio::SfxEngine>>,
    life: Option<ResMut<'w, station_3d::life::OfficeLifeState>>,
    camera_rig: Option<ResMut<'w, station_3d::camera::CameraRig>>,
    cam_q: Query<'w, 's, &'static mut Projection, With<station_3d::camera::MainOfficeCamera>>,
    replay: Option<ResMut<'w, station_3d::replay::ReplayState>>,
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
    bridge: Res<LiveServerBridge>,
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
            }
            UiAction::SetViewMode(ViewMode::Mode3D) => {
                ui.view_mode = ViewMode::Mode3D;
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
                }
                world_params.clear_color.0 = if night {
                    Color::srgb(0.06, 0.08, 0.13)
                } else {
                    Color::srgb(0.75, 0.85, 0.94)
                };
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
                        ui.push_toast("Permission", "Server offline — cannot send decision");
                    }
                }
                if let Some(sfx) = aux.sfx.as_deref_mut() {
                    sfx.play_sfx(SfxKind::Click);
                }
                ui.push_toast("Permission", format!("{id} -> {decision}"));
            }
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
                ui.push_toast("Weather", format!("City set to {display}"));
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
                if enabled {
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
                let fname = format!("shot-{}-{}.png", size.dimensions_tag(), now_ms());
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
                ui.push_toast(
                    i18n.tr("ui.photo.label"),
                    format!("shots/{fname} ({})", size.dimensions_tag()),
                );
            }
            UiAction::TriggerTimelapseClip { shape, music } => {
                ui.clip_shape = shape;
                ui.clip_music = music;
                let shots_dir = bridge.data_dir.join("shots");
                let _ = std::fs::create_dir_all(&shots_dir);
                let shape_tag = match shape {
                    station_ui::state::ClipShape::Square => "1x1",
                    station_ui::state::ClipShape::Tall => "9x16",
                };
                let fname = format!("timelapse-15s-{shape_tag}-{}.png", now_ms());
                let clip_path = shots_dir.join(&fname);
                let _ = save_photostrip_png(
                    &clip_path,
                    &ui,
                    world_params.shared_fonts.as_deref_mut(),
                    &bridge.assets_dir,
                );
                if let Some(sfx) = aux.sfx.as_deref_mut() {
                    sfx.play_sfx(SfxKind::Ding);
                }
                ui.push_toast(i18n.tr("ui.clip.button"), format!("shots/{fname}"));
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
                ui.push_toast("Photo Booth", format!("Saved shots/{fname}"));
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
                    ui.push_toast("Demo Mode", "Office demo cast activated");
                } else {
                    ui.sessions.clear();
                    ui.pending_perms.clear();
                    ui.feed.clear();
                    ui.selected_session = None;
                    if let BridgeBackend::InProcess { app_state, .. } = &bridge.backend {
                        sync_office_core_to_ui(app_state, &mut ui);
                    } else if let BridgeBackend::AttachedHttp { snapshot, .. } = &bridge.backend {
                        let snap = snapshot.lock().unwrap_or_else(|e| e.into_inner()).clone();
                        apply_remote_snapshot_to_ui(&snap, &mut ui);
                    }
                    ui.push_toast("Demo Mode", "Returned to live session state");
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
                            i18n.tr(&format!("places.{place_id}.title")),
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
                        ui.push_toast("Prompt", "Server offline — prompt not sent");
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
                        ui.push_toast("Group Chat", "Server offline — message not persisted");
                    }
                }
                if let Some(sfx) = aux.sfx.as_deref_mut() {
                    sfx.play_sfx(SfxKind::Chat);
                }
            }
            UiAction::SaveWhiteboard(text) => {
                let todos = parse_whiteboard_todos(&text);
                let _ = station_core::atomic::write_json_atomic(
                    &bridge.data_dir.join("whiteboard.json"),
                    &json!({ "text": text, "todos": todos }),
                );
                match &bridge.backend {
                    BridgeBackend::InProcess { app_state, .. } => {
                        if let Ok(mut office) = app_state.office.lock() {
                            office.todos = todos;
                        }
                    }
                    BridgeBackend::AttachedHttp { action_tx, .. } => {
                        let _ = action_tx.try_send(RemoteActionCmd {
                            path: "/api/todos",
                            body: json!({ "todos": todos }),
                            refresh: RefreshKind::None,
                            launch_toast_label: None,
                        });
                    }
                    BridgeBackend::BindError { .. } => {
                        ui.push_toast("Whiteboard", "Saved locally to whiteboard.json");
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
                    sfx.play_sfx(SfxKind::Rustle);
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
        let bridge = LiveServerBridge {
            backend: BridgeBackend::AttachedHttp {
                snapshot: Arc::new(Mutex::new(RemoteSnapshot::default())),
                action_tx,
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
            last_window_focused: None,
            toast_tx,
            toast_rx,
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
        let (shutdown, _rx) = watch::channel(false);
        let (toast_tx, toast_rx) = mpsc::channel(TOAST_QUEUE_CAP);
        let attached_bridge = LiveServerBridge {
            backend: BridgeBackend::AttachedHttp {
                snapshot: Arc::new(Mutex::new(RemoteSnapshot::default())),
                action_tx,
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
            last_window_focused: None,
            toast_tx,
            toast_rx,
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
}
