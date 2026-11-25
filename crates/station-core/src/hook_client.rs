use std::collections::HashMap;
use std::path::Path;
use std::process::Stdio;
use std::time::Duration;
use tokio::io::{AsyncReadExt, AsyncWriteExt};

#[cfg(windows)]
const CREATE_NO_WINDOW: u32 = 0x0800_0000;

const STATUSLINE_CHAIN_ENV: &str = "AGENTS_STATION_STATUSLINE_CHAIN";
const MAX_STATUSLINE_STDOUT: u64 = 64 * 1024;

#[derive(Debug, Clone, Default)]
pub struct HookArgs {
    pub url: String,
    pub r#async: bool,
    pub fast: bool,
    pub wait: Option<u64>,
    pub job: bool,
    pub agent: Option<String>,
    pub event: Option<String>,
    pub response_json: bool,
}

fn prepare_payload(args: &HookArgs, stdin_bytes: &[u8]) -> Vec<u8> {
    let Some(agent) = &args.agent else {
        return stdin_bytes.to_vec();
    };
    let mut val = serde_json::from_slice::<serde_json::Value>(stdin_bytes)
        .ok()
        .filter(|v| v.is_object())
        .unwrap_or_else(|| serde_json::json!({}));
    if let Some(obj) = val.as_object_mut() {
        obj.insert("agent".to_owned(), serde_json::Value::String(agent.clone()));
        if let Some(ev) = &args.event {
            obj.insert(
                "hook_event_name".to_owned(),
                serde_json::Value::String(ev.clone()),
            );
        }
    }
    serde_json::to_vec(&val).unwrap_or_else(|_| stdin_bytes.to_vec())
}

/// Native Rust replacement for `hooks/send.mjs`.
/// Never fails the calling agent hook: unreachable/timeout errors return gracefully.
pub async fn run_hook_client(
    args: &HookArgs,
    stdin_bytes: &[u8],
    env: &HashMap<String, String>,
) -> String {
    let wait_s = args.wait.unwrap_or(55).max(1);
    let timeout = if args.r#async {
        Duration::from_millis(2_000)
    } else if args.fast {
        Duration::from_millis(300)
    } else {
        Duration::from_secs(wait_s)
    };

    let mut out = String::new();
    let payload = prepare_payload(args, stdin_bytes);

    if let Ok(client) = reqwest::Client::builder().timeout(timeout).build() {
        let mut req = client
            .post(&args.url)
            .header("Content-Type", "application/json");
        if !args.r#async && !args.fast {
            req = req.header("X-Office-Wait", wait_s.to_string());
        }
        if args.job {
            if let Some(job_dir) = env.get("CLAUDE_JOB_DIR")
                && let Some(base) = Path::new(job_dir).file_name().and_then(|n| n.to_str())
            {
                req = req.header("X-Office-Job", base);
            }
            if let Some(inbox) = env.get("CLAUDE_CODE_MESSAGING_SOCKET")
                && !inbox.is_empty()
            {
                req = req.header("X-Office-Inbox", inbox);
            }
        }
        if let Ok(resp) = req.body(payload).send().await
            && !args.r#async
            && resp.status().is_success()
            && let Ok(text) = resp.text().await
        {
            out.push_str(&text);
        }
    }

    if args.response_json {
        match args.event.as_deref() {
            Some("Stop") => out.push_str(r#"{"decision":""}"#),
            Some("PreToolUse") => {}
            _ => out.push_str("{}"),
        }
    }

    out
}

fn build_status_hook_body(doc: &serde_json::Value) -> serde_json::Value {
    let mut out = serde_json::Map::new();
    for key in ["session_id", "rate_limits", "context_window", "cost"] {
        if let Some(v) = doc.get(key)
            && !v.is_null()
        {
            out.insert(key.to_owned(), v.clone());
        }
    }
    if let Some(pc) = doc.get("prompt_cache").and_then(serde_json::Value::as_object) {
        let mut cache = serde_json::Map::new();
        for key in [
            "warm",
            "caching_observed",
            "ttl",
            "expires_at",
            "recache_tokens_if_cold",
        ] {
            if let Some(v) = pc.get(key) {
                cache.insert(key.to_owned(), v.clone());
            }
        }
        out.insert("prompt_cache".to_owned(), serde_json::Value::Object(cache));
    }
    serde_json::Value::Object(out)
}

fn format_fallback_status_line(doc: &serde_json::Value) -> String {
    let pct = |v: Option<&serde_json::Value>| -> Option<String> {
        v.and_then(serde_json::Value::as_f64)
            .map(|n| format!("{}%", n.round() as i64))
    };
    let model = doc
        .get("model")
        .and_then(|m| m.get("display_name"))
        .and_then(serde_json::Value::as_str)
        .filter(|s| !s.trim().is_empty())
        .unwrap_or("Claude");
    let mut parts = vec![model.to_owned()];
    if let Some(ctx) = pct(doc.get("context_window").and_then(|c| c.get("used_percentage"))) {
        parts.push(format!("ctx {ctx}"));
    }
    if let Some(five) = pct(doc
        .get("rate_limits")
        .and_then(|r| r.get("five_hour"))
        .and_then(|f| f.get("used_percentage")))
    {
        parts.push(format!("5h {five}"));
    }
    if let Some(seven) = pct(doc
        .get("rate_limits")
        .and_then(|r| r.get("seven_day"))
        .and_then(|s| s.get("used_percentage")))
    {
        parts.push(format!("7d {seven}"));
    }
    format!("{}\n", parts.join(" · "))
}

fn is_recursive_status_cmd(cmd: &str) -> bool {
    cmd.to_ascii_lowercase().contains("status-line")
}

async fn run_chained_status_cmd(cmd: &str, stdin_bytes: &[u8]) -> String {
    #[cfg(windows)]
    let mut command = {
        let mut c = tokio::process::Command::new("cmd");
        c.raw_arg(format!("/D /S /C \"{cmd}\""));
        c.creation_flags(CREATE_NO_WINDOW);
        c
    };
    #[cfg(not(windows))]
    let mut command = {
        let mut c = tokio::process::Command::new("sh");
        c.args(["-c", cmd]);
        c
    };

    command
        .env(STATUSLINE_CHAIN_ENV, "1")
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::null());

    let Ok(mut child) = command.spawn() else {
        return String::new();
    };
    let mut stdin = child.stdin.take();
    let mut stdout = child.stdout.take();
    let input = stdin_bytes.to_vec();

    let run = async {
        let write_fut = async {
            if let Some(mut s) = stdin.take() {
                let _ = s.write_all(&input).await;
                let _ = s.shutdown().await;
            }
        };
        let read_fut = async {
            let mut buf = Vec::new();
            if let Some(o) = stdout.take() {
                let _ = o.take(MAX_STATUSLINE_STDOUT).read_to_end(&mut buf).await;
            }
            buf
        };
        let (_, buf) = tokio::join!(write_fut, read_fut);
        let _ = child.wait().await;
        String::from_utf8_lossy(&buf).into_owned()
    };

    match tokio::time::timeout(Duration::from_millis(3_000), run).await {
        Ok(out) => out,
        Err(_) => {
            let _ = child.kill().await;
            String::new()
        }
    }
}

/// Claude Code `statusLine` command handler: POSTs status metrics to local
/// `127.0.0.1:{port}/hook/status` (400ms cap) while concurrently chaining any
/// previously installed command saved in `statusline-prev.json`.
pub async fn run_status_line(port: u16, data_dir: &Path, stdin_bytes: &[u8]) -> String {
    let doc = serde_json::from_slice::<serde_json::Value>(stdin_bytes)
        .unwrap_or_else(|_| serde_json::json!({}));
    let body = serde_json::to_vec(&build_status_hook_body(&doc)).unwrap_or_default();
    let url = format!("http://127.0.0.1:{port}/hook/status");
    let host_header = format!("127.0.0.1:{port}");

    let forward = async {
        if let Ok(client) = reqwest::Client::builder()
            .timeout(Duration::from_millis(400))
            .build()
        {
            let _ = client
                .post(&url)
                .header("Host", &host_header)
                .header("Content-Type", "application/json")
                .body(body)
                .send()
                .await;
        }
    };

    let already_chained = std::env::var_os(STATUSLINE_CHAIN_ENV).is_some();
    let prev_cmd = if already_chained {
        None
    } else {
        std::fs::read_to_string(data_dir.join("statusline-prev.json"))
            .ok()
            .and_then(|s| serde_json::from_str::<serde_json::Value>(&s).ok())
            .and_then(|v| {
                v.get("command")
                    .and_then(serde_json::Value::as_str)
                    .map(str::to_owned)
            })
            .filter(|s| !s.trim().is_empty() && !is_recursive_status_cmd(s))
    };

    let chained = async {
        match prev_cmd.as_deref() {
            Some(cmd) => run_chained_status_cmd(cmd, stdin_bytes).await,
            None => String::new(),
        }
    };

    let (_, prev_out) = tokio::join!(forward, chained);
    if !prev_out.trim().is_empty() {
        prev_out
    } else {
        format_fallback_status_line(&doc)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use axum::Router;
    use axum::http::HeaderMap;
    use axum::routing::post;
    use std::sync::{Arc, Mutex};
    use std::time::Instant;

    #[tokio::test]
    async fn closed_port_fast_and_response_json_contracts() {
        // Bind ephemeral port and drop immediately so connection refuses fast.
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let closed_port = listener.local_addr().unwrap().port();
        drop(listener);
        let url = format!("http://127.0.0.1:{closed_port}/event");
        let env = HashMap::new();

        let t0 = Instant::now();
        let fast_out = run_hook_client(
            &HookArgs {
                url: url.clone(),
                fast: true,
                ..Default::default()
            },
            b"{}",
            &env,
        )
        .await;
        assert_eq!(fast_out, "");
        assert!(t0.elapsed() < Duration::from_millis(2_500));

        let stop_out = run_hook_client(
            &HookArgs {
                url: url.clone(),
                r#async: true,
                agent: Some("antigravity".to_owned()),
                event: Some("Stop".to_owned()),
                response_json: true,
                ..Default::default()
            },
            b"{}",
            &env,
        )
        .await;
        assert_eq!(stop_out, r#"{"decision":""}"#);

        let notif_out = run_hook_client(
            &HookArgs {
                url: url.clone(),
                r#async: true,
                event: Some("Notification".to_owned()),
                response_json: true,
                ..Default::default()
            },
            b"{}",
            &env,
        )
        .await;
        assert_eq!(notif_out, "{}");

        let pre_out = run_hook_client(
            &HookArgs {
                url,
                r#async: true,
                event: Some("PreToolUse".to_owned()),
                response_json: true,
                ..Default::default()
            },
            b"{}",
            &env,
        )
        .await;
        assert_eq!(pre_out, "");
    }

    #[tokio::test]
    async fn live_server_receives_headers_injected_fields_and_returns_reply() {
        let captured = Arc::new(Mutex::new(None::<(String, String, String, serde_json::Value)>));
        let cap_clone = captured.clone();

        let app = Router::new().route(
            "/hook/test",
            post(move |headers: HeaderMap, axum::Json(body): axum::Json<serde_json::Value>| {
                let cap = cap_clone.clone();
                async move {
                    let wait = headers
                        .get("x-office-wait")
                        .and_then(|v| v.to_str().ok())
                        .unwrap_or("")
                        .to_owned();
                    let job = headers
                        .get("x-office-job")
                        .and_then(|v| v.to_str().ok())
                        .unwrap_or("")
                        .to_owned();
                    let inbox = headers
                        .get("x-office-inbox")
                        .and_then(|v| v.to_str().ok())
                        .unwrap_or("")
                        .to_owned();
                    *cap.lock().unwrap() = Some((wait, job, inbox, body));
                    r#"{"ok":true,"reply":"allowed"}"#
                }
            }),
        );

        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();
        let server = tokio::spawn(async move {
            let _ = axum::serve(listener, app).await;
        });

        let mut env = HashMap::new();
        env.insert("CLAUDE_JOB_DIR".to_owned(), "C:/tmp/job-42".to_owned());
        env.insert("CLAUDE_CODE_MESSAGING_SOCKET".to_owned(), "pipe-inbox".to_owned());

        let out = run_hook_client(
            &HookArgs {
                url: format!("http://{addr}/hook/test"),
                wait: Some(120),
                job: true,
                agent: Some("codex".to_owned()),
                event: Some("PreToolUse".to_owned()),
                ..Default::default()
            },
            br#"{"session_id":"s-9"}"#,
            &env,
        )
        .await;

        assert_eq!(out, r#"{"ok":true,"reply":"allowed"}"#);
        let (wait, job, inbox, body) = captured.lock().unwrap().clone().unwrap();
        assert_eq!(wait, "120");
        assert_eq!(job, "job-42");
        assert_eq!(inbox, "pipe-inbox");
        assert_eq!(body["session_id"], "s-9");
        assert_eq!(body["agent"], "codex");
        assert_eq!(body["hook_event_name"], "PreToolUse");
        server.abort();
    }

    #[tokio::test]
    async fn run_status_line_chains_prev_command_and_posts_hook_status() {
        let captured = Arc::new(Mutex::new(None::<(String, serde_json::Value)>));
        let cap_clone = captured.clone();

        let app = Router::new().route(
            "/hook/status",
            post(move |headers: HeaderMap, axum::Json(body): axum::Json<serde_json::Value>| {
                let cap = cap_clone.clone();
                async move {
                    let host = headers
                        .get("host")
                        .and_then(|v| v.to_str().ok())
                        .unwrap_or("")
                        .to_owned();
                    *cap.lock().unwrap() = Some((host, body));
                    "{}"
                }
            }),
        );

        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();
        let server = tokio::spawn(async move {
            let _ = axum::serve(listener, app).await;
        });

        let dir = tempfile::tempdir().unwrap();
        std::fs::write(
            dir.path().join("statusline-prev.json"),
            r#"{"type":"command","command":"echo CHAINED_STATUS_OK"}"#,
        )
        .unwrap();

        let payload = serde_json::json!({
            "session_id": "s-status-1",
            "model": { "display_name": "Claude Opus" },
            "context_window": { "used_percentage": 42.4, "total_input_tokens": 12000 },
            "rate_limits": {
                "five_hour": { "used_percentage": 18.2 },
                "seven_day": { "used_percentage": 7.0 }
            },
            "cost": { "total_cost_usd": 1.25 },
            "prompt_cache": { "warm": true, "ttl": 300, "extra_ignored": "nope" }
        })
        .to_string();

        let out = run_status_line(addr.port(), dir.path(), payload.as_bytes()).await;
        assert!(
            out.contains("CHAINED_STATUS_OK"),
            "expected chained stdout, got {out:?}"
        );

        let (host, body) = captured
            .lock()
            .unwrap()
            .clone()
            .expect("/hook/status should have received POST");
        assert_eq!(host, format!("127.0.0.1:{}", addr.port()));
        assert_eq!(body["session_id"], "s-status-1");
        assert_eq!(body["context_window"]["used_percentage"], 42.4);
        assert_eq!(body["cost"]["total_cost_usd"], 1.25);
        assert_eq!(body["prompt_cache"]["warm"], true);
        assert!(body["prompt_cache"].get("extra_ignored").is_none());

        // Without statusline-prev.json, prints formatted fallback line.
        std::fs::remove_file(dir.path().join("statusline-prev.json")).unwrap();
        let fallback = run_status_line(addr.port(), dir.path(), payload.as_bytes()).await;
        assert!(fallback.contains("Claude Opus"), "fallback={fallback:?}");
        assert!(fallback.contains("42%"), "fallback={fallback:?}");
        assert!(fallback.contains("18%"), "fallback={fallback:?}");
        assert!(fallback.contains("7%"), "fallback={fallback:?}");

        server.abort();
    }

    #[tokio::test]
    async fn run_status_line_blocks_recursive_cmd_and_runs_quoted_windows_cmd() {
        let dir = tempfile::tempdir().unwrap();
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let closed_port = listener.local_addr().unwrap().port();
        drop(listener);

        let payload = serde_json::json!({
            "model": { "display_name": "Claude Sonnet" },
            "context_window": { "used_percentage": 15.0 }
        })
        .to_string();

        // 1. Recursive status-line prev command must be skipped (never fork-bomb), returning fallback.
        std::fs::write(
            dir.path().join("statusline-prev.json"),
            r#"{"type":"command","command":"\"D:/app/agents-station.exe\" status-line --port 4310"}"#,
        )
        .unwrap();
        let out = run_status_line(closed_port, dir.path(), payload.as_bytes()).await;
        assert!(
            out.contains("Claude Sonnet") && out.contains("15%"),
            "recursive prev_cmd should fall back cleanly, got {out:?}"
        );

        // 2. Quoted command string executes cleanly on Windows (cmd /D /S /C raw_arg) and Unix.
        #[cfg(windows)]
        let quoted_cmd = r#""cmd.exe" /C echo "QUOTED_ARG_OK""#;
        #[cfg(not(windows))]
        let quoted_cmd = r#"sh -c 'echo "QUOTED_ARG_OK"'"#;
        std::fs::write(
            dir.path().join("statusline-prev.json"),
            serde_json::json!({ "type": "command", "command": quoted_cmd }).to_string(),
        )
        .unwrap();
        let quoted_out = run_status_line(closed_port, dir.path(), payload.as_bytes()).await;
        assert!(
            quoted_out.contains("QUOTED_ARG_OK"),
            "quoted prev_cmd must execute cleanly, got {quoted_out:?}"
        );
    }
}
