use crate::http::{AppState, start_tunnel_http_server};
use regex::Regex;
use serde::Serialize;
use std::path::{Path, PathBuf};
use std::process::Stdio;
use std::sync::{Arc, Mutex, OnceLock};
use tokio::io::{AsyncBufReadExt, BufReader};
use tokio::process::{Child, Command};
use tokio::sync::watch;

#[cfg(windows)]
pub const CREATE_NO_WINDOW: u32 = 0x0800_0000;

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct TunnelSnapshot {
    pub state: String,
    pub url: Option<String>,
    pub host: Option<String>,
    pub port: Option<u16>,
    pub error: Option<String>,
}

struct TunnelInner {
    generation: u64,
    state: String,
    url: Option<String>,
    host: Option<String>,
    port: Option<u16>,
    error: Option<String>,
    server_shutdown: Option<watch::Sender<bool>>,
    child: Option<Child>,
}

impl TunnelInner {
    fn new() -> Self {
        Self {
            generation: 0,
            state: "off".to_owned(),
            url: None,
            host: None,
            port: None,
            error: None,
            server_shutdown: None,
            child: None,
        }
    }

    fn snapshot(&self) -> TunnelSnapshot {
        TunnelSnapshot {
            state: self.state.clone(),
            url: self.url.clone(),
            host: self.host.clone(),
            port: self.port,
            error: self.error.clone(),
        }
    }

    fn teardown_locked(&mut self) {
        if let Some(mut child) = self.child.take() {
            let _ = child.start_kill();
        }
        if let Some(tx) = self.server_shutdown.take() {
            let _ = tx.send(true);
        }
    }
}

/// Extracts a Cloudflare Quick Tunnel URL (`https://<slug>.trycloudflare.com`) from `cloudflared` output.
pub fn parse_trycloudflare_url(line: &str) -> Option<String> {
    static RE: OnceLock<Regex> = OnceLock::new();
    let re = RE.get_or_init(|| {
        Regex::new(r"https://[a-z0-9-]+\.trycloudflare\.com").expect("valid trycloudflare regex")
    });
    re.find(line).map(|m| m.as_str().to_owned())
}

pub fn extract_https_host(url: &str) -> Option<String> {
    let parsed = reqwest::Url::parse(url).ok()?;
    if parsed.scheme() != "https" {
        return None;
    }
    parsed.host_str().map(|h| h.to_ascii_lowercase())
}

fn resolve_cloudflared_bin(data_dir: &Path) -> PathBuf {
    if let Ok(cmd) = std::env::var("OFFICE_TUNNEL_CMD") {
        let trimmed = cmd.trim();
        if !trimmed.is_empty() {
            return PathBuf::from(trimmed);
        }
    }
    let bundled = data_dir
        .join("bin")
        .join(if cfg!(windows) { "cloudflared.exe" } else { "cloudflared" });
    if bundled.is_file() {
        return bundled;
    }
    PathBuf::from(if cfg!(windows) { "cloudflared.exe" } else { "cloudflared" })
}

pub struct TunnelManager {
    data_dir: PathBuf,
    inner: Arc<Mutex<TunnelInner>>,
}

impl TunnelManager {
    pub fn new(data_dir: PathBuf) -> Self {
        Self {
            data_dir,
            inner: Arc::new(Mutex::new(TunnelInner::new())),
        }
    }

    pub fn active_host(&self) -> Option<String> {
        self.inner
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .host
            .clone()
    }

    pub fn snapshot(&self) -> TunnelSnapshot {
        self.inner
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .snapshot()
    }

    pub async fn start(&self, state: Arc<AppState>) -> TunnelSnapshot {
        self.start_with_command(state, None).await
    }

    pub async fn start_with_command(
        &self,
        state: Arc<AppState>,
        cmd_override: Option<(PathBuf, Vec<String>)>,
    ) -> TunnelSnapshot {
        {
            let guard = self.inner.lock().unwrap_or_else(|e| e.into_inner());
            if matches!(guard.state.as_str(), "starting" | "on") && guard.child.is_some() {
                return guard.snapshot();
            }
        }

        let handle = match start_tunnel_http_server(state, 0).await {
            Ok(h) => h,
            Err(err) => {
                let mut guard = self.inner.lock().unwrap_or_else(|e| e.into_inner());
                guard.teardown_locked();
                guard.state = "error".to_owned();
                guard.url = None;
                guard.host = None;
                guard.port = None;
                guard.error = Some(format!("failed to bind tunnel listener: {err}"));
                return guard.snapshot();
            }
        };

        let port = handle.addr.port();
        let origin_url = format!("http://127.0.0.1:{port}");
        let (bin, args) = match cmd_override {
            Some((b, a)) => (
                b,
                a.into_iter()
                    .map(|arg| arg.replace("{url}", &origin_url))
                    .collect::<Vec<_>>(),
            ),
            None => (
                resolve_cloudflared_bin(&self.data_dir),
                vec![
                    "tunnel".to_owned(),
                    "--no-autoupdate".to_owned(),
                    "--url".to_owned(),
                    origin_url,
                ],
            ),
        };

        let mut cmd = Command::new(&bin);
        cmd.args(&args)
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .kill_on_drop(true);
        #[cfg(windows)]
        {
            cmd.creation_flags(CREATE_NO_WINDOW);
        }

        let mut child = match cmd.spawn() {
            Ok(c) => c,
            Err(err) => {
                let _ = handle.shutdown.send(true);
                let mut guard = self.inner.lock().unwrap_or_else(|e| e.into_inner());
                guard.teardown_locked();
                guard.state = "error".to_owned();
                guard.url = None;
                guard.host = None;
                guard.port = None;
                guard.error = Some(format!("failed to spawn cloudflared: {err}"));
                return guard.snapshot();
            }
        };

        let stderr = child.stderr.take();
        let stdout = child.stdout.take();

        let (generation, snap) = {
            let mut guard = self.inner.lock().unwrap_or_else(|e| e.into_inner());
            guard.teardown_locked();
            guard.generation = guard.generation.wrapping_add(1);
            guard.state = "starting".to_owned();
            guard.url = None;
            guard.host = None;
            guard.port = Some(port);
            guard.error = None;
            guard.server_shutdown = Some(handle.shutdown);
            guard.child = Some(child);
            (guard.generation, guard.snapshot())
        };

        if let Some(err_pipe) = stderr {
            let inner = Arc::clone(&self.inner);
            tokio::spawn(async move {
                let mut lines = BufReader::new(err_pipe).lines();
                while let Ok(Some(line)) = lines.next_line().await {
                    if let Some(url) = parse_trycloudflare_url(&line)
                        && let Some(host) = extract_https_host(&url)
                    {
                        let mut guard = inner.lock().unwrap_or_else(|e| e.into_inner());
                        if guard.generation == generation {
                            guard.url = Some(url);
                            guard.host = Some(host);
                            guard.state = "on".to_owned();
                            guard.error = None;
                        }
                    }
                }
                let mut guard = inner.lock().unwrap_or_else(|e| e.into_inner());
                if guard.generation == generation && matches!(guard.state.as_str(), "starting" | "on") {
                    let msg = if guard.url.is_none() {
                        "cloudflared exited before publishing tunnel URL"
                    } else {
                        "cloudflared exited"
                    };
                    guard.teardown_locked();
                    guard.state = "error".to_owned();
                    guard.url = None;
                    guard.host = None;
                    guard.port = None;
                    guard.error = Some(msg.to_owned());
                }
            });
        }

        if let Some(out_pipe) = stdout {
            let inner = Arc::clone(&self.inner);
            tokio::spawn(async move {
                let mut lines = BufReader::new(out_pipe).lines();
                while let Ok(Some(line)) = lines.next_line().await {
                    if let Some(url) = parse_trycloudflare_url(&line)
                        && let Some(host) = extract_https_host(&url)
                    {
                        let mut guard = inner.lock().unwrap_or_else(|e| e.into_inner());
                        if guard.generation == generation {
                            guard.url = Some(url);
                            guard.host = Some(host);
                            guard.state = "on".to_owned();
                            guard.error = None;
                        }
                    }
                }
            });
        }

        snap
    }

    pub fn stop(&self) -> TunnelSnapshot {
        let mut guard = self.inner.lock().unwrap_or_else(|e| e.into_inner());
        guard.generation = guard.generation.wrapping_add(1);
        guard.teardown_locked();
        guard.state = "off".to_owned();
        guard.url = None;
        guard.host = None;
        guard.port = None;
        guard.error = None;
        guard.snapshot()
    }
}

impl Drop for TunnelManager {
    fn drop(&mut self) {
        let mut guard = self.inner.lock().unwrap_or_else(|e| e.into_inner());
        guard.generation = guard.generation.wrapping_add(1);
        guard.teardown_locked();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::http::guard::enforce_host_allowed;
    use axum::http::{HeaderMap, StatusCode};
    use std::time::Duration;

    #[test]
    fn parse_trycloudflare_url_extracts_valid_quick_tunnel_origin() {
        let sample = "2026-11-25T10:00:00Z INF |  https://quiet-river-42.trycloudflare.com  |";
        let url = parse_trycloudflare_url(sample).expect("must extract url");
        assert_eq!(url, "https://quiet-river-42.trycloudflare.com");
        assert_eq!(
            extract_https_host(&url).as_deref(),
            Some("quiet-river-42.trycloudflare.com")
        );
        assert_eq!(parse_trycloudflare_url("http://evil.trycloudflare.com"), None);
        assert_eq!(parse_trycloudflare_url("https://evil.example.com"), None);
    }

    #[tokio::test]
    async fn dedicated_tunnel_listener_enforces_pin_auth_from_127_0_0_1() {
        let dir = tempfile::tempdir().unwrap();
        let state = Arc::new(AppState::new(dir.path().to_path_buf(), None).unwrap());
        let handle = start_tunnel_http_server(Arc::clone(&state), 0)
            .await
            .expect("tunnel listener must bind 127.0.0.1:0");

        let client = reqwest::Client::builder()
            .timeout(Duration::from_secs(5))
            .redirect(reqwest::redirect::Policy::none())
            .build()
            .unwrap();

        // 1. Unauthenticated GET /api/live from 127.0.0.1 over tunnel listener -> 401 Unauthorized
        let url_live = format!("http://{}/api/live", handle.addr);
        let resp = client.get(&url_live).send().await.unwrap();
        assert_eq!(
            resp.status().as_u16(),
            StatusCode::UNAUTHORIZED.as_u16(),
            "tunnel listener must reject unauthenticated 127.0.0.1 /api/live with 401"
        );

        // 2. Unauthenticated GET /phone over tunnel listener -> 303 redirect to /pin
        let url_phone = format!("http://{}/phone", handle.addr);
        let resp_phone = client.get(&url_phone).send().await.unwrap();
        assert_eq!(resp_phone.status().as_u16(), StatusCode::SEE_OTHER.as_u16());

        // 3. POST /event from 127.0.0.1 over tunnel listener -> 403 Forbidden (hooks never trust tunnel)
        let url_ev = format!("http://{}/event", handle.addr);
        let resp_ev = client
            .post(&url_ev)
            .json(&serde_json::json!({"session_id":"s1","hook_event_name":"SessionStart"}))
            .send()
            .await
            .unwrap();
        assert_eq!(resp_ev.status().as_u16(), StatusCode::FORBIDDEN.as_u16());

        // 4. Token query ?k=<token> is also rejected on ViaTunnel(true) -> 401
        let url_tok = format!("http://{}/api/live?k={}", handle.addr, state.auth.token());
        let resp_tok = client.get(&url_tok).send().await.unwrap();
        assert_eq!(resp_tok.status().as_u16(), StatusCode::UNAUTHORIZED.as_u16());

        // 5. Valid PIN unlock sets Secure cookie and allows /api/live with that cookie
        let url_pin = format!("http://{}/pin", handle.addr);
        let pin_resp = client
            .post(&url_pin)
            .json(&serde_json::json!({"pin": state.auth.current_pin(), "keep": true}))
            .send()
            .await
            .unwrap();
        assert_eq!(pin_resp.status().as_u16(), StatusCode::OK.as_u16());
        let set_cookie = pin_resp
            .headers()
            .get("set-cookie")
            .unwrap()
            .to_str()
            .unwrap()
            .to_owned();
        assert!(set_cookie.contains("; Secure"));
        let cookie_pair = set_cookie.split(';').next().unwrap();

        let authed_resp = client
            .get(&url_live)
            .header("cookie", cookie_pair)
            .send()
            .await
            .unwrap();
        assert_eq!(authed_resp.status().as_u16(), StatusCode::OK.as_u16());

        let _ = handle.shutdown.send(true);
    }

    #[tokio::test]
    async fn tunnel_manager_parses_stderr_url_updates_allowed_host_and_stops_cleanly() {
        let dir = tempfile::tempdir().unwrap();
        let state = Arc::new(AppState::new(dir.path().to_path_buf(), None).unwrap());

        // Use a tiny platform command that prints a trycloudflare URL to stderr and stays alive briefly
        #[cfg(windows)]
        let cmd = (
            PathBuf::from("cmd.exe"),
            vec![
                "/C".to_owned(),
                "echo INF https://amber-comet-77.trycloudflare.com 1>&2 & ping -n 10 127.0.0.1 >nul"
                    .to_owned(),
            ],
        );
        #[cfg(not(windows))]
        let cmd = (
            PathBuf::from("sh"),
            vec![
                "-c".to_owned(),
                "echo 'INF https://amber-comet-77.trycloudflare.com' >&2; sleep 10".to_owned(),
            ],
        );

        let initial = state
            .tunnel
            .start_with_command(Arc::clone(&state), Some(cmd))
            .await;
        assert!(matches!(initial.state.as_str(), "starting" | "on"));
        let port = initial.port.expect("ephemeral listener port must be set");

        // Wait briefly for stderr line to be parsed
        let mut ready = false;
        for _ in 0..40 {
            let snap = state.tunnel.snapshot();
            if snap.state == "on" && snap.url.as_deref() == Some("https://amber-comet-77.trycloudflare.com") {
                ready = true;
                break;
            }
            tokio::time::sleep(Duration::from_millis(50)).await;
        }
        assert!(ready, "tunnel manager must parse trycloudflare URL from stderr");
        assert_eq!(
            state.allowed_tunnel_host().as_deref(),
            Some("amber-comet-77.trycloudflare.com")
        );

        // Host header check permits the active quick tunnel hostname
        let mut h = HeaderMap::new();
        h.insert("host", "amber-comet-77.trycloudflare.com".parse().unwrap());
        assert_eq!(
            enforce_host_allowed(&h, state.allowed_tunnel_host().as_deref()),
            Ok(())
        );

        // Live ephemeral tunnel listener enforces PIN (401) even with the active tunnel Host header
        let client = reqwest::Client::builder()
            .timeout(Duration::from_secs(3))
            .build()
            .unwrap();
        let resp = client
            .get(format!("http://127.0.0.1:{port}/api/live"))
            .header("host", "amber-comet-77.trycloudflare.com")
            .send()
            .await
            .unwrap();
        assert_eq!(resp.status().as_u16(), StatusCode::UNAUTHORIZED.as_u16());

        // Stop kills child and shuts down listener + clears allowed host
        let stopped = state.tunnel.stop();
        assert_eq!(stopped.state, "off");
        assert_eq!(stopped.url, None);
        assert_eq!(stopped.port, None);
        assert_eq!(state.allowed_tunnel_host(), None);
    }
}
