use std::collections::HashMap;
use std::path::Path;
use std::time::Duration;

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
}
