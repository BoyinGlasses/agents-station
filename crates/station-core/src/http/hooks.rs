use crate::http::AppState;
use crate::http::guard::{OptionalPeer, enforce_loopback_strict};
use crate::shared::cache_clock::cache_from_status;
use crate::state::overlap::{file_of, is_edit_tool};
use crate::state::sessions::now_ms;
use crate::store::otel::day_key;
use axum::Json;
use axum::extract::{Query, State};
use axum::http::{HeaderMap, StatusCode};
use serde_json::{Value, json};
use std::collections::HashMap;
use std::path::Path;
use std::sync::Arc;

pub async fn post_event(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    OptionalPeer(peer): OptionalPeer,
    Json(body): Json<Value>,
) -> Result<Json<Value>, StatusCode> {
    enforce_loopback_strict(peer, &headers)?;
    let ts = now_ms();
    let today = day_key(ts);

    let ev_opt = {
        let mut office = state.office.lock().expect("office lock poisoned");
        office.ingest_raw_event(body)
    };

    if let Some(ev) = ev_opt {
        let mut trophies = state.trophies.lock().expect("trophies lock poisoned");
        trophies.session(&ev.session_id, &today, ts);
        match ev.hook_event_name.as_str() {
            "UserPromptSubmit" => {
                trophies.bump("prompts", 1, &today, ts);
                if let Some(prompt) = &ev.prompt {
                    let mut vault = state.vault.lock().expect("vault lock poisoned");
                    let cwd = ev.cwd.as_deref().unwrap_or("");
                    let title = Path::new(cwd)
                        .file_name()
                        .and_then(|n| n.to_str())
                        .unwrap_or(&ev.agent);
                    vault.record_prompt(&ev.session_id, &ev.agent, title, cwd, prompt, 0, ts);
                }
            }
            "PostToolUse" => {
                let tn = ev.tool_name.as_deref().unwrap_or("");
                if is_edit_tool(tn) {
                    trophies.bump("edit", 1, &today, ts);
                } else if tn == "Bash" {
                    trophies.bump("cmd", 1, &today, ts);
                } else if matches!(tn, "Read" | "Grep" | "Glob") {
                    trophies.bump("read", 1, &today, ts);
                } else if matches!(tn, "WebSearch" | "WebFetch") {
                    trophies.bump("web", 1, &today, ts);
                }
            }
            "Stop" => {
                if let Some(cwd) = &ev.cwd
                    && let Some(name) = Path::new(cwd).file_name().and_then(|n| n.to_str())
                {
                    let mut tenure = state.tenure.lock().expect("tenure lock poisoned");
                    tenure.touch(name, ts, true);
                }
            }
            _ => {}
        }
        Ok(Json(json!({ "ok": true, "session_id": ev.session_id })))
    } else {
        Ok(Json(json!({ "ok": true, "ignored": true })))
    }
}

pub async fn post_hook_loop(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    OptionalPeer(peer): OptionalPeer,
    Json(body): Json<Value>,
) -> Result<Json<Value>, StatusCode> {
    enforce_loopback_strict(peer, &headers)?;
    let ts = now_ms();
    let mut office = state.office.lock().expect("office lock poisoned");

    if let Some(sid) = body.get("session_id").and_then(Value::as_str) {
        let tool = body.get("tool_name").and_then(Value::as_str).unwrap_or("");
        let input = body.get("tool_input").unwrap_or(&Value::Null);
        let cwd = body.get("cwd").and_then(Value::as_str).unwrap_or("");
        if is_edit_tool(tool) {
            let f = file_of(input, cwd);
            if !f.is_empty() {
                office.undo.before(sid, Path::new(&f));
                if let Some(warn) = office.overlap.before(sid, &f, ts) {
                    office.overlap.note(
                        sid,
                        &format!("Note: another session ({}) recently edited {f}.", warn.other),
                    );
                }
            }
        }
    }

    let res = office.loops.check(&body, ts);
    if res.stop {
        return Ok(Json(json!({ "continue": false, "stopReason": "Stopped from Agents Station" })));
    }
    if let Some(deny) = res.deny {
        return Ok(Json(json!({
            "hookSpecificOutput": {
                "hookEventName": "PreToolUse",
                "permissionDecision": "deny",
                "permissionDecisionReason": format!("Blocked repeated failing call ({} x{}): try a different approach.", deny.label, deny.n)
            }
        })));
    }
    let sid = body.get("session_id").and_then(Value::as_str).unwrap_or("");
    let note = office.overlap.take(sid);
    let mut ctx_parts = Vec::new();
    if let Some(h) = res.hint {
        ctx_parts.push(h);
    }
    if !note.is_empty() {
        ctx_parts.push(note);
    }
    if !ctx_parts.is_empty() {
        return Ok(Json(json!({
            "hookSpecificOutput": {
                "hookEventName": "PreToolUse",
                "additionalContext": ctx_parts.join("\n\n")
            }
        })));
    }
    Ok(Json(json!({})))
}

pub async fn post_hook_status(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    OptionalPeer(peer): OptionalPeer,
    Json(body): Json<Value>,
) -> Result<Json<Value>, StatusCode> {
    enforce_loopback_strict(peer, &headers)?;
    let ts = now_ms();
    let sid = body.get("session_id").and_then(Value::as_str).unwrap_or("");
    if !sid.is_empty() {
        let mut office = state.office.lock().expect("office lock poisoned");
        let ctx_tokens = body
            .get("context_window")
            .and_then(|c| c.get("total_input_tokens"))
            .and_then(Value::as_u64);
        if let Some(cache) = cache_from_status(body.get("prompt_cache"), ctx_tokens)
            && let Some(s) = office.sessions.get_mut(sid)
        {
            s.cache = Some(cache);
        }
        if let Some(cost) = body.get("cost")
            && let Some(usd) = cost.get("total_cost_usd").and_then(Value::as_f64)
        {
            let added = cost.get("total_lines_added").and_then(Value::as_i64);
            let removed = cost.get("total_lines_removed").and_then(Value::as_i64);
            let working = office
                .sessions
                .get(sid)
                .is_some_and(|s| s.status == "work");
            let _ = office.loops.on_status(sid, usd, added, removed, working, ts);
        }
    }
    Ok(Json(json!({})))
}

pub async fn post_hook_bind(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    OptionalPeer(peer): OptionalPeer,
    Query(q): Query<HashMap<String, String>>,
    Json(body): Json<Value>,
) -> Result<Json<Value>, StatusCode> {
    enforce_loopback_strict(peer, &headers)?;
    let job = headers
        .get("x-office-job")
        .and_then(|v| v.to_str().ok())
        .or_else(|| q.get("task").map(String::as_str))
        .unwrap_or("");
    let inbox = headers
        .get("x-office-inbox")
        .and_then(|v| v.to_str().ok())
        .unwrap_or("");
    let sid = body.get("session_id").and_then(Value::as_str).unwrap_or("");
    if !job.is_empty() {
        let mut tasks = state.tasks.lock().expect("tasks lock poisoned");
        tasks.bind(job, sid, inbox);
    }
    Ok(Json(json!({ "ok": true })))
}

pub async fn post_hook_start(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    OptionalPeer(peer): OptionalPeer,
    Json(body): Json<Value>,
) -> Result<Json<Value>, StatusCode> {
    enforce_loopback_strict(peer, &headers)?;
    let sid = body.get("session_id").and_then(Value::as_str).unwrap_or("");
    let office = state.office.lock().expect("office lock poisoned");
    if let Some(list) = office.pins.get(sid)
        && !list.is_empty()
    {
        return Ok(Json(json!({
            "hookSpecificOutput": {
                "hookEventName": "SessionStart",
                "additionalContext": list.join("\n")
            }
        })));
    }
    Ok(Json(json!({})))
}

pub async fn post_hook_permission(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    OptionalPeer(peer): OptionalPeer,
    Json(mut body): Json<Value>,
) -> Result<Json<Value>, StatusCode> {
    enforce_loopback_strict(peer, &headers)?;
    if let Some(obj) = body.as_object_mut() {
        obj.entry("hook_event_name")
            .or_insert_with(|| Value::String("PermissionRequest".to_owned()));
    }
    let mut office = state.office.lock().expect("office lock poisoned");
    office.ingest_raw_event(body);
    // Non-blocking fast return when no interactive hold is active so Claude Code falls back safely.
    Ok(Json(json!({})))
}

pub async fn post_hook_question(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    OptionalPeer(peer): OptionalPeer,
    Json(body): Json<Value>,
) -> Result<Json<Value>, StatusCode> {
    enforce_loopback_strict(peer, &headers)?;
    let mut office = state.office.lock().expect("office lock poisoned");
    office.ingest_raw_event(body);
    Ok(Json(json!({})))
}

pub async fn post_hook_denied(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    OptionalPeer(peer): OptionalPeer,
    Json(body): Json<Value>,
) -> Result<Json<Value>, StatusCode> {
    enforce_loopback_strict(peer, &headers)?;
    let ts = now_ms();
    let mut office = state.office.lock().expect("office lock poisoned");
    let sid = body
        .get("session_id")
        .and_then(Value::as_str)
        .unwrap_or("claude")
        .to_owned();
    office.chat.deny_card(&sid, &sid, body, ts);
    Ok(Json(json!({ "ok": true })))
}
