pub mod api;
pub mod guard;
pub mod hooks;
pub mod otel_routes;
pub mod phone;
pub mod sse;

use crate::auth::{AuthStore, PinRateLimiter};
use crate::config::{home_dir, resolve_claude_dir};
use crate::platform::default_platform;
use crate::prefs::PrefsStore;
use crate::shared::duty::{DutyState, fresh as fresh_duty};
use crate::state::auto_resume::AutoResumeGuard;
use crate::state::claude_version::ClaudeVersionWatcher;
use crate::state::git::GitTracker;
use crate::state::lift_service::LiftService;
use crate::state::sessions::{OfficeState, ReapReport, now_ms};
use crate::state::steer::SteerQueue;
use crate::state::stranded::StrandedScanner;
use crate::state::tasks::TaskManager;
use crate::state::tenure::TenureStore;
use crate::store::agent_history::AgentHistory;
use crate::store::antigravity_accounts::AntigravityAccounts;
use crate::store::antigravity_runs::AntigravityWatcher;
use crate::store::claude_accounts::ClaudeAccounts;
use crate::store::codex_accounts::CodexAccounts;
use crate::store::images::ImageStore;
use crate::store::otel::OtelStore;
use crate::store::push::PushStore;
use crate::store::rewrite::Rewriter;
use crate::store::trophies::TrophyStore;
use crate::store::usage::UsageScanner;
use crate::store::vault::VaultStore;
use crate::system::SystemMonitor;
use anyhow::Result;
use axum::Router;
use axum::extract::{Request, State};
use axum::http::StatusCode;
use axum::middleware::{self, Next};
use axum::response::Response;
use axum::routing::{get, post};
use serde_json::Value;
use std::net::SocketAddr;
use std::path::PathBuf;
use std::sync::{Arc, Mutex};
use std::time::Duration;
use tokio::sync::watch;

pub const OTEL_FLUSH_INTERVAL: Duration = Duration::from_secs(15);
pub const REAP_INTERVAL: Duration = Duration::from_secs(60);

pub trait HarnessApi: Send + Sync {
    fn list_json(&self) -> Value;
    fn launch(&self, id: &str) -> Result<Value>;
    fn mutate_custom(&self, body: Value) -> Result<Value>;
}

pub struct AppState {
    pub data_dir: PathBuf,
    pub claude_dir: PathBuf,
    pub auth: AuthStore,
    pub pin_limiter: Mutex<PinRateLimiter>,
    pub prefs: Mutex<PrefsStore>,
    pub office: Mutex<OfficeState>,
    pub sys: SystemMonitor,
    pub otel: Mutex<OtelStore>,
    pub trophies: Mutex<TrophyStore>,
    pub vault: Arc<Mutex<VaultStore>>,
    pub tenure: Mutex<TenureStore>,
    pub tasks: Arc<Mutex<TaskManager>>,
    pub steer_queue: Mutex<SteerQueue>,
    pub push: Mutex<PushStore>,
    pub rewriter: Arc<Mutex<Rewriter>>,
    pub claude_accounts: Mutex<ClaudeAccounts>,
    pub codex_accounts: Mutex<CodexAccounts>,
    pub ag_accounts: Mutex<AntigravityAccounts>,
    pub claude_version: Arc<Mutex<ClaudeVersionWatcher>>,
    pub images: Mutex<ImageStore>,
    pub git: Arc<Mutex<GitTracker>>,
    pub usage: Arc<Mutex<UsageScanner>>,
    pub stranded: Arc<Mutex<StrandedScanner>>,
    pub agent_history: Arc<Mutex<AgentHistory>>,
    pub ag_watcher: Arc<Mutex<AntigravityWatcher>>,
    pub duty: Mutex<DutyState>,
    pub auto_resume: Mutex<AutoResumeGuard>,
    pub lift: LiftService,
    pub harnesses: Option<Arc<dyn HarnessApi>>,
}

impl AppState {
    pub fn new(data_dir: PathBuf, harnesses: Option<Arc<dyn HarnessApi>>) -> Result<Self> {
        let claude_dir = resolve_claude_dir(std::env::var("CLAUDE_CONFIG_DIR").ok().as_deref());
        Self::new_with_claude_dir(data_dir, claude_dir, harnesses)
    }

    pub fn new_with_claude_dir(
        data_dir: PathBuf,
        claude_dir: PathBuf,
        harnesses: Option<Arc<dyn HarnessApi>>,
    ) -> Result<Self> {
        let now = now_ms();
        let auth = AuthStore::load_or_init(&data_dir)?;
        let prefs = PrefsStore::load(&data_dir);
        let office = OfficeState::new(data_dir.clone());
        let sys = SystemMonitor::new(default_platform(), claude_dir.clone());
        let otel = OtelStore::load(data_dir.join("otel"), 30, now);
        let trophies = TrophyStore::load(&data_dir, now);
        let vault = Arc::new(Mutex::new(VaultStore::load(&data_dir)));
        let tenure = TenureStore::load(&data_dir, now);
        let tasks = Arc::new(Mutex::new(TaskManager::new()));
        let steer_queue = Mutex::new(SteerQueue::default());
        let push = PushStore::load(&data_dir);
        let rewriter = Arc::new(Mutex::new(Rewriter::default()));
        let claude_accounts = ClaudeAccounts::load(&data_dir, &claude_dir);
        let codex_accounts = CodexAccounts::load(&data_dir);
        let ag_accounts = AntigravityAccounts::load(&data_dir);
        let claude_version = Arc::new(Mutex::new(ClaudeVersionWatcher::load(&data_dir, home_dir())));
        let images = ImageStore::load(&data_dir);
        let git = Arc::new(Mutex::new(GitTracker::new()));
        let usage = Arc::new(Mutex::new(UsageScanner::new()));
        let stranded = Arc::new(Mutex::new(StrandedScanner::new()));
        let agent_history = Arc::new(Mutex::new(AgentHistory::default()));
        let ag_watcher = Arc::new(Mutex::new(AntigravityWatcher::new(
            home_dir().join(".gemini").join("antigravity").join("brain"),
        )));
        let duty = Mutex::new(fresh_duty());
        let auto_resume = Mutex::new(AutoResumeGuard::default());
        let lift = LiftService::init(&data_dir, env!("CARGO_PKG_VERSION"), env!("CARGO_PKG_VERSION"), now);

        Ok(Self {
            data_dir,
            claude_dir,
            auth,
            pin_limiter: Mutex::new(PinRateLimiter::new()),
            prefs: Mutex::new(prefs),
            office: Mutex::new(office),
            sys,
            otel: Mutex::new(otel),
            trophies: Mutex::new(trophies),
            vault,
            tenure: Mutex::new(tenure),
            tasks,
            steer_queue,
            push: Mutex::new(push),
            rewriter,
            claude_accounts: Mutex::new(claude_accounts),
            codex_accounts: Mutex::new(codex_accounts),
            ag_accounts: Mutex::new(ag_accounts),
            claude_version,
            images: Mutex::new(images),
            git,
            usage,
            stranded,
            agent_history,
            ag_watcher,
            duty,
            auto_resume,
            lift,
            harnesses,
        })
    }

    pub fn allowed_tunnel_host(&self) -> Option<String> {
        self.prefs
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .get("tunnelDomain")
            .and_then(Value::as_str)
            .map(str::trim)
            .filter(|s| !s.is_empty())
            .map(str::to_owned)
    }

    pub fn flush_all(&self) -> Result<()> {
        let now = now_ms();
        self.prefs.lock().unwrap_or_else(|e| e.into_inner()).save()?;
        self.office.lock().unwrap_or_else(|e| e.into_inner()).flush_all()?;
        self.otel.lock().unwrap_or_else(|e| e.into_inner()).flush(now)?;
        self.trophies.lock().unwrap_or_else(|e| e.into_inner()).flush()?;
        self.vault.lock().unwrap_or_else(|e| e.into_inner()).flush()?;
        self.tenure.lock().unwrap_or_else(|e| e.into_inner()).save()?;
        Ok(())
    }

    pub fn flush_dirty_otel(&self, now: u64) -> Result<bool> {
        let mut otel = self.otel.lock().unwrap_or_else(|e| e.into_inner());
        if otel.is_dirty() {
            otel.flush(now)?;
            Ok(true)
        } else {
            Ok(false)
        }
    }

    pub fn reap_once(&self, now: u64) -> ReapReport {
        let auto_close_hours = self
            .prefs
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .auto_close();
        let mut office = self.office.lock().unwrap_or_else(|e| e.into_inner());
        office.reap_stale(now, auto_close_hours)
    }
}

async fn enforce_host_middleware(
    State(state): State<Arc<AppState>>,
    req: Request,
    next: Next,
) -> Result<Response, StatusCode> {
    let allowed_tunnel = state.allowed_tunnel_host();
    guard::enforce_host_allowed(req.headers(), allowed_tunnel.as_deref())?;
    Ok(next.run(req).await)
}

pub fn build_router(state: Arc<AppState>) -> Router {
    Router::new()
        .route("/", get(phone::get_root))
        .route("/phone", get(phone::get_phone))
        .route("/pin", get(phone::get_pin).post(phone::post_pin))
        .route("/manifest.webmanifest", get(phone::get_manifest))
        .route("/sw.js", get(phone::get_sw))
        .route("/health", get(phone::get_health))
        .route("/stream", get(sse::get_stream))
        // Hooks & OTLP (strictly loopback)
        .route("/event", post(hooks::post_event))
        .route("/v1/metrics", post(otel_routes::post_metrics))
        .route("/v1/logs", post(otel_routes::post_logs))
        .route("/hook/loop", post(hooks::post_hook_loop))
        .route("/hook/status", post(hooks::post_hook_status))
        .route("/hook/bind", post(hooks::post_hook_bind))
        .route("/hook/start", post(hooks::post_hook_start))
        .route("/hook/permission", post(hooks::post_hook_permission))
        .route("/hook/question", post(hooks::post_hook_question))
        .route("/hook/denied", post(hooks::post_hook_denied))
        // Core GET/POST APIs
        .route("/api/live", get(api::get_live))
        .route("/api/git", get(api::get_git))
        .route("/api/doctor", get(api::get_doctor))
        .route("/api/log", get(api::get_log))
        .route("/api/timeline", get(api::get_timeline))
        .route("/api/wrapped", get(api::get_wrapped))
        .route("/api/replay", get(api::get_replay))
        .route("/api/replay/days", get(api::get_replay_days))
        .route("/api/stranded", get(api::get_stranded))
        .route("/api/stranded/open", post(api::post_stranded_open))
        .route("/api/stranded/remove", post(api::post_stranded_remove))
        .route("/api/pins", get(api::get_pins).post(api::post_pins))
        .route("/api/vault", get(api::get_vault))
        .route("/api/vault/report", get(api::get_vault_report))
        .route("/api/vault/transcript", get(api::get_vault_transcript))
        .route("/api/undo", get(api::get_undo).post(api::post_undo))
        .route("/api/burn", get(api::get_burn))
        .route("/api/sys", get(api::get_sys))
        .route("/api/sys/action", post(api::post_sys_action))
        .route("/api/sys/auto", post(api::post_sys_auto))
        .route("/api/panel", get(api::get_panel))
        .route("/api/panel/answer", post(api::post_panel_answer))
        .route("/api/panel/open", post(api::post_panel_open))
        .route("/api/chat", get(api::get_chat).post(api::post_chat))
        .route("/api/loop", get(api::get_loop).post(api::post_loop))
        .route("/api/tenure", get(api::get_tenure).post(api::post_tenure))
        .route("/api/cache", get(api::get_cache).post(api::post_cache))
        .route("/api/tasks", get(api::get_tasks).post(api::post_tasks))
        .route("/api/tasks/allow", post(api::post_tasks_allow))
        .route("/api/tasks/bg", post(api::post_tasks_bg))
        .route("/api/tasks/stop", post(api::post_tasks_stop))
        .route("/api/tasks/steer", post(api::post_tasks_steer))
        .route("/api/steer", post(api::post_tasks_steer))
        .route("/api/models", get(api::get_models).post(api::post_models))
        .route("/api/quick", get(api::get_quick).post(api::post_quick))
        .route("/api/claude-version", get(api::get_claude_version))
        .route("/api/push", get(api::get_push).post(api::post_push))
        .route("/api/image", get(api::get_image))
        .route("/api/macapprove", get(api::get_macapprove).post(api::post_macapprove))
        .route("/api/tunnel", get(api::get_tunnel_status).post(api::post_tunnel_toggle))
        .route("/api/tunnel/domain", post(api::post_tunnel_domain))
        .route("/api/presence", post(api::post_presence))
        .route("/api/perm", post(api::post_perm))
        .route("/api/pin", post(api::post_pin_update))
        .route("/api/lan", post(api::post_lan))
        .route("/api/phone", post(api::post_phone))
        .route("/api/rc", post(api::post_rc))
        .route("/api/prompt/rewrite", post(api::post_prompt_rewrite))
        .route("/api/open", post(api::post_open))
        .route("/api/denied", post(api::post_denied))
        .route("/api/ci", post(api::post_ci))
        .route("/api/prefs", get(api::get_prefs).post(api::post_prefs))
        .route("/api/todos", post(api::post_todos))
        // Accounts & multi-AI
        .route("/api/accounts", get(api::get_accounts).post(api::post_accounts))
        .route("/api/accounts/{action}", post(api::post_accounts_action))
        .route("/api/ai/codex/{action}", post(api::post_codex_ai))
        .route("/api/ai/ag/{action}", post(api::post_ag_ai))
        // Native GUI Harness Launcher (strictly loopback)
        .route("/api/harnesses", get(api::get_harnesses))
        .route("/api/harnesses/launch", post(api::post_harnesses_launch))
        .route("/api/harnesses/custom", post(api::post_harnesses_custom))
        .layer(middleware::from_fn_with_state(
            state.clone(),
            enforce_host_middleware,
        ))
        .with_state(state)
}

/// Router for a dedicated loopback tunnel listener: marks every request server-side with `ViaTunnel(true)`.
pub fn build_tunnel_router(state: Arc<AppState>) -> Router {
    build_router(state).layer(axum::Extension(guard::ViaTunnel(true)))
}

pub struct ServerHandle {
    pub addr: SocketAddr,
    pub shutdown: watch::Sender<bool>,
}

fn spawn_maintenance_loop_with_intervals(
    state: Arc<AppState>,
    mut shutdown: watch::Receiver<bool>,
    otel_every: Duration,
    reap_every: Duration,
) -> tokio::task::JoinHandle<()> {
    tokio::spawn(async move {
        if *shutdown.borrow() {
            let _ = state.flush_dirty_otel(now_ms());
            return;
        }
        let start = tokio::time::Instant::now();
        let mut otel_tick = tokio::time::interval_at(start + otel_every, otel_every);
        otel_tick.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);
        let mut reap_tick = tokio::time::interval_at(start + reap_every, reap_every);
        reap_tick.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);

        loop {
            tokio::select! {
                _ = otel_tick.tick() => {
                    let _ = state.flush_dirty_otel(now_ms());
                }
                _ = reap_tick.tick() => {
                    let now = now_ms();
                    let _ = state.reap_once(now);
                    let cv = Arc::clone(&state.claude_version);
                    let ag = Arc::clone(&state.ag_watcher);
                    let _ = tokio::task::spawn_blocking(move || {
                        cv.lock().unwrap_or_else(|e| e.into_inner()).check(now);
                        let _ = ag.lock().unwrap_or_else(|e| e.into_inner()).scan_changed();
                    })
                    .await;
                }
                changed = shutdown.changed() => {
                    if changed.is_err() || *shutdown.borrow() {
                        let _ = state.flush_dirty_otel(now_ms());
                        break;
                    }
                }
            }
        }
    })
}

pub fn spawn_maintenance_loop(
    state: Arc<AppState>,
    shutdown: watch::Receiver<bool>,
) -> tokio::task::JoinHandle<()> {
    spawn_maintenance_loop_with_intervals(state, shutdown, OTEL_FLUSH_INTERVAL, REAP_INTERVAL)
}

pub async fn start_http_server(
    state: Arc<AppState>,
    host: &str,
    port: u16,
) -> Result<ServerHandle> {
    let listener = tokio::net::TcpListener::bind((host, port)).await?;
    let addr = listener.local_addr()?;
    let (tx, mut rx) = watch::channel(false);
    spawn_maintenance_loop(state.clone(), tx.subscribe());
    let router = build_router(state.clone());

    tokio::spawn(async move {
        let svc = router.into_make_service_with_connect_info::<SocketAddr>();
        let _ = axum::serve(listener, svc)
            .with_graceful_shutdown(async move {
                let _ = rx.changed().await;
            })
            .await;
        let _ = state.flush_all();
    });

    Ok(ServerHandle { addr, shutdown: tx })
}

/// Dedicated loopback-only listener for tunnel ingress (e.g. cloudflared).
/// Injects `ViaTunnel(true)` server-side so loopback auto-trust is never granted.
pub async fn start_tunnel_http_server(
    state: Arc<AppState>,
    port: u16,
) -> Result<ServerHandle> {
    let listener = tokio::net::TcpListener::bind(("127.0.0.1", port)).await?;
    let addr = listener.local_addr()?;
    let (tx, mut rx) = watch::channel(false);
    let router = build_tunnel_router(state.clone());

    tokio::spawn(async move {
        let svc = router.into_make_service_with_connect_info::<SocketAddr>();
        let _ = axum::serve(listener, svc)
            .with_graceful_shutdown(async move {
                let _ = rx.changed().await;
            })
            .await;
    });

    Ok(ServerHandle { addr, shutdown: tx })
}

#[cfg(test)]
mod tests {
    use super::*;
    use axum::body::Body;
    use axum::http::{Request, StatusCode};
    use serde_json::json;
    use tower::ServiceExt;

    #[tokio::test]
    async fn security_guards_reject_spoofed_host_origin_and_remote_unauth() {
        let dir = tempfile::tempdir().unwrap();
        let state = Arc::new(AppState::new(dir.path().to_path_buf(), None).unwrap());
        let app = build_router(state.clone());

        // 1. Spoofed Host header -> 421 Misdirected Request and zero session mutations
        let bad_host = Request::builder()
            .method("POST")
            .uri("/event")
            .header("host", "evil.com")
            .header("content-type", "application/json")
            .body(Body::from(json!({"session_id":"s1","hook_event_name":"SessionStart"}).to_string()))
            .unwrap();
        let resp = app.clone().oneshot(bad_host).await.unwrap();
        assert_eq!(resp.status(), StatusCode::MISDIRECTED_REQUEST);
        assert_eq!(state.office.lock().unwrap().sessions.len(), 0);

        // 2. Cross-origin browser request -> 403
        let bad_origin = Request::builder()
            .method("POST")
            .uri("/api/harnesses/launch")
            .header("host", "127.0.0.1:4310")
            .header("origin", "https://evil.com")
            .header("content-type", "application/json")
            .body(Body::from(json!({"id":"claude"}).to_string()))
            .unwrap();
        let resp = app.clone().oneshot(bad_origin).await.unwrap();
        assert_eq!(resp.status(), StatusCode::FORBIDDEN);

        // 3. Valid loopback SessionStart -> 200 and visible in /api/live
        let good = Request::builder()
            .method("POST")
            .uri("/event")
            .header("host", "127.0.0.1:4310")
            .header("content-type", "application/json")
            .body(Body::from(json!({"session_id":"s1","hook_event_name":"SessionStart","cwd":"D:/repo"}).to_string()))
            .unwrap();
        let resp = app.clone().oneshot(good).await.unwrap();
        assert_eq!(resp.status(), StatusCode::OK);
        assert_eq!(state.office.lock().unwrap().sessions.len(), 1);

        // 4. Health check -> 200 ok
        let health = Request::builder()
            .uri("/health")
            .header("host", "127.0.0.1:4310")
            .body(Body::empty())
            .unwrap();
        let resp = app.clone().oneshot(health).await.unwrap();
        assert_eq!(resp.status(), StatusCode::OK);

        // 5. OTLP bad schema -> 400
        let bad_otel = Request::builder()
            .method("POST")
            .uri("/v1/metrics")
            .header("host", "127.0.0.1:4310")
            .header("content-type", "application/json")
            .body(Body::from("{}"))
            .unwrap();
        let resp = app.clone().oneshot(bad_otel).await.unwrap();
        assert_eq!(resp.status(), StatusCode::BAD_REQUEST);

        // 6. Remote unauthenticated request -> 303 on / and 401 on /api/live
        let remote_root = Request::builder()
            .uri("/")
            .header("host", "127.0.0.1:4310")
            .header("x-office-remote-test", "1")
            .body(Body::empty())
            .unwrap();
        let resp = app.clone().oneshot(remote_root).await.unwrap();
        assert_eq!(resp.status(), StatusCode::SEE_OTHER);

        let remote_api = Request::builder()
            .uri("/api/live")
            .header("host", "127.0.0.1:4310")
            .header("x-office-remote-test", "1")
            .body(Body::empty())
            .unwrap();
        let resp = app.clone().oneshot(remote_api).await.unwrap();
        assert_eq!(resp.status(), StatusCode::UNAUTHORIZED);

        // 7. /api/image includes sandbox CSP + nosniff
        let img = Request::builder()
            .uri("/api/image")
            .header("host", "127.0.0.1:4310")
            .body(Body::empty())
            .unwrap();
        let resp = app.clone().oneshot(img).await.unwrap();
        assert_eq!(resp.headers().get("content-security-policy").unwrap(), "sandbox");
        assert_eq!(resp.headers().get("x-content-type-options").unwrap(), "nosniff");
    }

    #[tokio::test]
    async fn maintenance_loop_flushes_dirty_otel_reaps_stale_and_flushes_on_shutdown() {
        use crate::state::sessions::PendingPerm;
        use crate::store::otel::{COST_METRIC, OtelStore, day_key};
        use std::time::Duration;

        let dir = tempfile::tempdir().unwrap();
        let data_dir = dir.path().to_path_buf();
        let state = Arc::new(AppState::new(data_dir.clone(), None).unwrap());
        state.prefs.lock().unwrap().set_auto_close(2).unwrap();

        let now = now_ms();
        let metric_at = |val: f64, seq: u64| {
            let t_ns = (now + seq * 1000) * 1_000_000;
            json!({
                "resourceMetrics": [{
                    "scopeMetrics": [{
                        "metrics": [{
                            "name": COST_METRIC,
                            "sum": {
                                "aggregationTemporality": 1,
                                "dataPoints": [{
                                    "asDouble": val,
                                    "startTimeUnixNano": (t_ns - 1_000_000).to_string(),
                                    "timeUnixNano": t_ns.to_string(),
                                    "attributes": [
                                        { "key": "session.id", "value": { "stringValue": "s-maint" } },
                                        { "key": "model", "value": { "stringValue": "claude-sonnet-4" } }
                                    ]
                                }]
                            }
                        }]
                    }]
                }]
            })
        };

        {
            let mut otel = state.otel.lock().unwrap();
            assert_eq!(otel.ingest_metrics(&metric_at(1.5, 1), now), 1);
            assert!(otel.is_dirty());
        }
        assert!(
            !data_dir.join("otel").join("state.json").exists(),
            "otel state should not be flushed before maintenance tick"
        );

        {
            let mut office = state.office.lock().unwrap();
            office
                .ingest_raw_event(json!({
                    "session_id": "s-idle-maint",
                    "hook_event_name": "SessionStart",
                    "cwd": "D:/repo/old"
                }))
                .unwrap();
            office.sessions.get_mut("s-idle-maint").unwrap().updated_at =
                now.saturating_sub(3 * 3_600_000);
            office.pending_perms.insert(
                "p-old".into(),
                PendingPerm {
                    id: "p-old".into(),
                    sess: "s-idle-maint".into(),
                    name: "old".into(),
                    kind: "perm".into(),
                    tool_type: "bash".into(),
                    label: "cmd".into(),
                    detail: "echo".into(),
                    danger: None,
                    created_at: now.saturating_sub(15 * 60_000),
                },
            );
        }

        assert_eq!(OTEL_FLUSH_INTERVAL, Duration::from_secs(15));
        assert_eq!(REAP_INTERVAL, Duration::from_secs(60));

        let (tx, rx) = watch::channel(false);
        let handle = spawn_maintenance_loop_with_intervals(
            state.clone(),
            rx,
            Duration::from_millis(20),
            Duration::from_millis(80),
        );

        // OTel flush tick fires before shutdown and before reaper tick.
        tokio::time::sleep(Duration::from_millis(45)).await;
        let otel_dir = data_dir.join("otel");
        assert!(otel_dir.join("state.json").exists(), "otel state.json must flush on periodic tick");
        assert!(
            otel_dir.join(format!("{}.json", day_key(now))).exists(),
            "otel day file must flush on periodic tick"
        );
        let reloaded = OtelStore::load(otel_dir.clone(), 30, now);
        assert!((reloaded.burn_summary("today", now).total.row.usd - 1.5).abs() < 1e-6);

        // Reaper tick fires and cleans idle session + expired pending_perms.
        tokio::time::sleep(Duration::from_millis(80)).await;
        {
            let office = state.office.lock().unwrap();
            assert!(!office.sessions.contains_key("s-idle-maint"));
            assert!(!office.pending_perms.contains_key("p-old"));
        }

        // Stop short-interval loop cleanly.
        tx.send(true).unwrap();
        handle.await.unwrap();

        // Public spawn_maintenance_loop flushes dirty OTel on shutdown.
        let (tx2, rx2) = watch::channel(false);
        let handle2 = spawn_maintenance_loop(state.clone(), rx2);
        {
            let mut otel = state.otel.lock().unwrap();
            assert_eq!(otel.ingest_metrics(&metric_at(2.0, 2), now), 1);
        }
        tx2.send(true).unwrap();
        handle2.await.unwrap();

        let reloaded2 = OtelStore::load(otel_dir, 30, now);
        assert!((reloaded2.burn_summary("today", now).total.row.usd - 3.5).abs() < 1e-6);
    }
}
