use crate::fps::AdaptiveFpsState;
use crate::tray::{TrayState, notify_parent_hold_alert, notify_parent_in_tray};
use bevy::app::AppExit;
use bevy::ecs::system::SystemParam;
use bevy::prelude::*;
use serde_json::{Value, json};
use station_3d::{BackdropAssets, BackdropEntity, WorldEntity, WorldPalette};
use station_canvas2d::BrandRegistry;
use station_core::http::{AppState, HarnessApi, start_http_server};
use station_core::state::sessions::PermDecision;
use station_launcher::focus_launch::{Win32WindowLauncher, WindowLauncher};
use station_launcher::{HarnessService, HarnessStatus, LaunchOutcome};
use station_ui::state::{FeedEntry, HarnessDockItem, PendingPermCard, SessionCard, UiAction};
use station_ui::{I18nStore, UiState, ViewMode};
use std::collections::HashSet;
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

#[derive(Default, Clone)]
struct RemoteSnapshot {
    has_snapshot: bool,
    sessions: Vec<SessionCard>,
    pending_perms: Vec<PendingPermCard>,
    feed: Vec<FeedEntry>,
    harnesses: Vec<HarnessDockItem>,
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

fn parse_sessions_from_json(val: &Value) -> Vec<SessionCard> {
    let Some(arr) = val.get("sessions").and_then(Value::as_array) else {
        return Vec::new();
    };
    arr.iter()
        .filter_map(|s| {
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
            let status = s
                .get("status")
                .and_then(Value::as_str)
                .unwrap_or("idle")
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
            let tool_calls = s.get("tool_calls").and_then(Value::as_u64).unwrap_or(0);
            Some(SessionCard {
                id,
                agent,
                project,
                status,
                model,
                last_prompt,
                tool,
                cost_usd: 0.0,
                tokens: tool_calls * 420,
                pinned: false,
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
                ts_label: format!("{}", (ts / 1000) % 86400),
                agent: f.get("name").and_then(Value::as_str)?.to_owned(),
                message: f.get("label").and_then(Value::as_str)?.to_owned(),
                level: if kind == "error" {
                    "err".to_owned()
                } else {
                    "info".to_owned()
                },
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
    let mut snap = snap_lock.lock().unwrap_or_else(|e| e.into_inner());
    snap.has_snapshot = true;
    snap.sessions = sessions;
    snap.pending_perms = pending_perms;
    snap.feed = feed;
}

fn apply_remote_snapshot_to_ui(snap: &RemoteSnapshot, ui: &mut UiState) {
    if !snap.harnesses.is_empty() {
        ui.harnesses = snap.harnesses.clone();
    }
    if snap.has_snapshot {
        ui.sessions = snap.sessions.clone();
        ui.pending_perms = snap.pending_perms.clone();
        if !snap.feed.is_empty() {
            ui.feed = snap.feed.clone();
        }
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
                                            if let Ok(val) = r.json::<Value>().await
                                                && let Some(outcome) = parse_launch_outcome_from_json(&val)
                                            {
                                                let _ = toast_a.try_send(launch_outcome_toast(label, &outcome));
                                            }
                                        }
                                        true
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

                // Periodic 1 Hz harness refresh alongside live SSE stream.
                let client_h = client.clone();
                let base_h = base_url.clone();
                let host_h = host_header.clone();
                let snap_h = snap_lock.clone();
                let mut rx_h = rx.clone();
                tokio::spawn(async move {
                    let mut tick = tokio::time::interval(HARNESS_POLL_INTERVAL);
                    tick.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);
                    loop {
                        tokio::select! {
                            _ = tick.tick() => {
                                fetch_remote_harnesses(&client_h, &base_h, &host_h, &snap_h).await;
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
                                                if val.get("k").and_then(Value::as_str)
                                                    == Some("snap")
                                                {
                                                    apply_snapshot_to_remote(&snap_lock, &val);
                                                } else {
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
    pub assets_dir: PathBuf,
    port: u16,
    sync_timer: Timer,
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
                assets_dir,
                port,
                sync_timer: Timer::from_seconds(0.5, TimerMode::Repeating),
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

        let app_state = match AppState::new(data_dir, Some(harness_api)) {
            Ok(st) => Arc::new(st),
            Err(err) => {
                let harness_poller = HarnessPoller::spawn(harnesses.clone(), HARNESS_POLL_INTERVAL);
                return Self {
                    backend: BridgeBackend::BindError {
                        message: format!("Failed to initialize station store: {err}"),
                        harnesses,
                        harness_poller,
                    },
                    assets_dir,
                    port,
                    sync_timer: Timer::from_seconds(1.0, TimerMode::Repeating),
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
                    _shutdown: None,
                },
                assets_dir,
                port,
                sync_timer: Timer::from_seconds(1.0, TimerMode::Repeating),
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
        let (bind_tx, bind_rx) = std::sync::mpsc::sync_channel::<Result<(), String>>(1);

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
                rt.block_on(async move {
                    match start_http_server(state_bg.clone(), &host_bg, port).await {
                        Ok(handle) => {
                            let _ = bind_tx.send(Ok(()));
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
            Ok(()) => {
                let harness_poller =
                    HarnessPoller::spawn(harnesses.clone(), HARNESS_POLL_INTERVAL);
                Self {
                    backend: BridgeBackend::InProcess {
                        app_state,
                        harnesses,
                        harness_poller,
                        _shutdown: Some(tx),
                    },
                    assets_dir,
                    port,
                    sync_timer: Timer::from_seconds(1.0, TimerMode::Repeating),
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
                        assets_dir,
                        port,
                        sync_timer: Timer::from_seconds(0.5, TimerMode::Repeating),
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
                        assets_dir,
                        port,
                        sync_timer: Timer::from_seconds(1.0, TimerMode::Repeating),
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
            .add_systems(Update, (sync_live_state_to_ui, process_ui_actions));
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

fn sync_live_state_to_ui(
    time: Res<Time>,
    mut bridge: ResMut<LiveServerBridge>,
    mut ui: ResMut<UiState>,
    windows: Query<&Window>,
    mut tray: Option<ResMut<TrayState>>,
) {
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
        BridgeBackend::AttachedHttp { .. } => {}
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

    bridge.sync_timer.tick(time.delta());
    if !bridge.sync_timer.just_finished() {
        return;
    }

    let mut synced_live_perms = false;
    match &bridge.backend {
        BridgeBackend::AttachedHttp { snapshot, .. } => {
            let snap = snapshot.lock().unwrap_or_else(|e| e.into_inner()).clone();
            synced_live_perms = snap.has_snapshot;
            apply_remote_snapshot_to_ui(&snap, &mut ui);
        }
        BridgeBackend::InProcess { app_state, .. } => {
            if let Ok(office) = app_state.office.lock() {
                if !office.sessions.is_empty() {
                    ui.sessions = office
                        .sessions
                        .values()
                        .map(|s| SessionCard {
                            id: s.id.clone(),
                            agent: s.agent.clone(),
                            project: s.name.clone(),
                            status: s.status.clone(),
                            model: s.model.clone().unwrap_or_else(|| "default".to_owned()),
                            last_prompt: s.last_prompt.clone().unwrap_or_default(),
                            tool: s.last.as_ref().map(|t| t.label.clone()),
                            cost_usd: 0.0,
                            tokens: s.tool_calls * 420,
                            pinned: office.pins.contains_key(&s.id),
                        })
                        .collect();
                }
                if !office.pending_perms.is_empty() {
                    synced_live_perms = true;
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
                } else if !bridge.seen_perm_ids.is_empty() {
                    synced_live_perms = true;
                    ui.pending_perms.clear();
                }
                if !office.feed.is_empty() {
                    ui.feed = office
                        .feed
                        .iter()
                        .take(12)
                        .map(|f| FeedEntry {
                            ts_label: format!("{}", (f.ts / 1000) % 86400),
                            agent: f.name.clone(),
                            message: f.label.clone(),
                            level: if f.kind == "error" {
                                "err".to_owned()
                            } else {
                                "info".to_owned()
                            },
                        })
                        .collect();
                }
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
                if let BridgeBackend::AttachedHttp { action_tx, .. } = &bridge.backend {
                    let _ = action_tx.try_send(RemoteActionCmd {
                        path: "/api/prefs",
                        body: json!({ "lang": code }),
                        refresh: RefreshKind::None,
                        launch_toast_label: None,
                    });
                }
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
                    BridgeBackend::BindError { .. } => {}
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
            UiAction::SetWeatherCity(_)
            | UiAction::SetSound(_)
            | UiAction::ToggleCameraFp
            | UiAction::TriggerPhoto
            | UiAction::ToggleDemo
            | UiAction::ToggleReplay
            | UiAction::SendPrompt { .. }
            | UiAction::PostGroupChat(_)
            | UiAction::SaveWhiteboard(_) => {}
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
        let mut ui = UiState::default();
        assert!(!ui.sessions.is_empty());
        assert!(!ui.pending_perms.is_empty());

        let populated = RemoteSnapshot {
            has_snapshot: true,
            sessions: vec![SessionCard {
                id: "s-1".to_owned(),
                agent: "claude".to_owned(),
                project: "proj".to_owned(),
                status: "working".to_owned(),
                model: "opus".to_owned(),
                last_prompt: "hi".to_owned(),
                tool: None,
                cost_usd: 0.0,
                tokens: 0,
                pinned: false,
            }],
            pending_perms: vec![PendingPermCard {
                id: "p-1".to_owned(),
                session_id: "s-1".to_owned(),
                agent: "claude".to_owned(),
                tool: "Bash".to_owned(),
                summary: "echo hi".to_owned(),
                kind: "permission".to_owned(),
            }],
            feed: Vec::new(),
            harnesses: Vec::new(),
        };
        apply_remote_snapshot_to_ui(&populated, &mut ui);
        assert_eq!(ui.sessions.len(), 1);
        assert_eq!(ui.pending_perms.len(), 1);

        let cleared = RemoteSnapshot {
            has_snapshot: true,
            sessions: Vec::new(),
            pending_perms: Vec::new(),
            feed: Vec::new(),
            harnesses: Vec::new(),
        };
        apply_remote_snapshot_to_ui(&cleared, &mut ui);
        assert!(ui.sessions.is_empty(), "ended sessions must clear from UI");
        assert!(ui.pending_perms.is_empty(), "resolved perms must clear from UI");
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
            assets_dir: PathBuf::from("assets"),
            port: 4310,
            sync_timer: Timer::from_seconds(1.0, TimerMode::Repeating),
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
}
