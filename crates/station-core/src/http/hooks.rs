use crate::http::AppState;
use crate::http::guard::{OptionalPeer, ViaTunnel, enforce_hook_loopback};
use crate::redact::redact;
use crate::shared::cache_clock::cache_from_status;
use crate::shared::danger::danger_key;
use crate::state::mac_hold::{
    BAR_ALIVE_MS, DENY_HOLD_MS, HoldDecision, HoldInput, PHONE_HOLD_MS, effective_bar_hold_ms,
    effective_hold_ms, hold_plan, wait_header_ms,
};
use crate::state::overlap::{file_of, is_edit_tool};
use crate::state::sessions::{PendingPerm, classify_tool, clip, now_ms, one_line};
use crate::store::otel::day_key;
use crate::store::push::{PushMessage, deliver_prepared};
use axum::Json;
use axum::extract::{Query, State};
use axum::http::{HeaderMap, StatusCode};
use serde_json::{Value, json};
use std::collections::HashMap;
use std::path::Path;
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::oneshot;

pub async fn post_event(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    OptionalPeer(peer): OptionalPeer,
    ViaTunnel(via_tunnel): ViaTunnel,
    Json(body): Json<Value>,
) -> Result<Json<Value>, StatusCode> {
    enforce_hook_loopback(peer, via_tunnel, &headers)?;
    let ts = now_ms();
    let today = day_key(ts);

    let ev_opt = {
        let mut office = state.office.lock().unwrap_or_else(|e| e.into_inner());
        office.ingest_raw_event(body)
    };

    if let Some(ev) = ev_opt {
        {
            let mut trophies = state.trophies.lock().unwrap_or_else(|e| e.into_inner());
            trophies.session(&ev.session_id, &today, ts);
            match ev.hook_event_name.as_str() {
                "UserPromptSubmit" => {
                    trophies.bump("prompts", 1, &today, ts);
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
                _ => {}
            }
            let _ = trophies.flush();
        }
        match ev.hook_event_name.as_str() {
            "PreToolUse" | "PostToolUse" => {
                let tn = ev.tool_name.as_deref().unwrap_or("");
                if matches!(tn, "Read" | "Write" | "Edit" | "MultiEdit") {
                    let cwd = ev.cwd.as_deref().unwrap_or("");
                    let fp = file_of(&ev.tool_input, cwd);
                    if !fp.is_empty() {
                        let img_id = state
                            .images
                            .lock()
                            .unwrap_or_else(|e| e.into_inner())
                            .register(Path::new(&fp));
                        if let Some(id) = img_id {
                            let mut office = state.office.lock().unwrap_or_else(|e| e.into_inner());
                            if let Some(last) = office
                                .feed
                                .back_mut()
                                .filter(|f| f.sess == ev.session_id)
                            {
                                last.image = Some(id);
                            }
                        }
                    }
                }
            }
            "UserPromptSubmit" => {
                if let Some(prompt) = &ev.prompt {
                    let mut vault = state.vault.lock().unwrap_or_else(|e| e.into_inner());
                    let cwd = ev.cwd.as_deref().unwrap_or("");
                    let title = Path::new(cwd)
                        .file_name()
                        .and_then(|n| n.to_str())
                        .unwrap_or(&ev.agent);
                    vault.record_prompt(&ev.session_id, &ev.agent, title, cwd, prompt, 0, ts);
                    let _ = vault.flush();
                }
            }
            "Stop" | "StopFailure" => {
                if let Some(cwd) = &ev.cwd
                    && let Some(name) = Path::new(cwd).file_name().and_then(|n| n.to_str())
                {
                    let mut tenure = state.tenure.lock().unwrap_or_else(|e| e.into_inner());
                    tenure.touch(name, ts, true);
                    let _ = tenure.save();
                }
                // Drain any queued steer note for this session when its turn finishes.
                if ev.hook_event_name == "Stop" {
                    let queued = {
                        let mut sq = state.steer_queue.lock().unwrap_or_else(|e| e.into_inner());
                        sq.take(&ev.session_id).or_else(|| {
                            ev.cwd
                                .as_deref()
                                .and_then(|c| sq.take(&format!("dir:{c}")))
                        })
                    };
                    if let Some(q) = queued {
                        state
                            .office
                            .lock()
                            .unwrap_or_else(|e| e.into_inner())
                            .broadcast(json!({
                                "k": "steerDrain",
                                "sid": ev.session_id,
                                "name": q.name,
                            }));
                        let target_dir = q
                            .dir
                            .or_else(|| ev.cwd.clone())
                            .unwrap_or_default();
                        if !target_dir.is_empty() && Path::new(&target_dir).is_dir() {
                            let _ = crate::http::api::spawn_task_with_card(
                                &state,
                                crate::state::tasks::TaskStartRequest {
                                    dir: target_dir,
                                    text: q.text,
                                    agent: q.agent,
                                    model: String::new(),
                                    fork: false,
                                },
                            );
                        }
                    }
                }
                if ev.hook_event_name == "StopFailure" {
                    let enabled = state
                        .prefs
                        .lock()
                        .unwrap_or_else(|e| e.into_inner())
                        .get("autoResume")
                        .and_then(Value::as_bool)
                        .unwrap_or(false);
                    if enabled {
                        let has_override = state
                            .tasks
                            .lock()
                            .unwrap_or_else(|e| e.into_inner())
                            .has_command_override();
                        let has_bin = if has_override {
                            true
                        } else {
                            tokio::task::spawn_blocking(|| {
                                crate::state::tasks::resolve_agent_cli("claude").is_some()
                            })
                            .await
                            .unwrap_or(false)
                        };
                        let decision = state
                            .auto_resume
                            .lock()
                            .unwrap_or_else(|e| e.into_inner())
                            .check_and_record(enabled, &ev.session_id, has_bin, ts);
                        if decision.ok {
                            {
                                let mut office =
                                    state.office.lock().unwrap_or_else(|e| e.into_inner());
                                let note = office.chat.system(
                                    &format!(
                                        "Auto-resuming session {} (attempt {})",
                                        ev.session_id,
                                        decision.attempts.unwrap_or(1)
                                    ),
                                    ts,
                                );
                                let _ = office.save_chat();
                                office.broadcast(json!({
                                    "k": "autoResume",
                                    "sid": ev.session_id,
                                    "attempts": decision.attempts,
                                    "msg": note,
                                }));
                            }
                            if let Some(cwd) = ev.cwd.as_deref()
                                && Path::new(cwd).is_dir()
                            {
                                let _ = crate::http::api::spawn_task_with_card(
                                    &state,
                                    crate::state::tasks::TaskStartRequest {
                                        dir: cwd.to_owned(),
                                        text: format!("Continue session {}", ev.session_id),
                                        agent: "claude".to_owned(),
                                        model: String::new(),
                                        fork: false,
                                    },
                                );
                            }
                        }
                    }
                }
            }
            _ => {}
        }

        // Refresh git + usage off the async runtime thread via spawn_blocking.
        let (sess_cwd, sess_tp) = {
            let office = state.office.lock().unwrap_or_else(|e| e.into_inner());
            office
                .sessions
                .get(&ev.session_id)
                .map(|s| (s.cwd.clone(), s.transcript_path.clone()))
                .unwrap_or_default()
        };
        if !sess_cwd.is_empty() && Path::new(&sess_cwd).is_dir() {
            let git = Arc::clone(&state.git);
            let cwd_clone = sess_cwd.clone();
            if let Ok(Some(info)) = tokio::task::spawn_blocking(move || {
                git.lock().unwrap_or_else(|e| e.into_inner()).poll_cwd(&cwd_clone)
            })
            .await
            {
                state
                    .office
                    .lock()
                    .unwrap_or_else(|e| e.into_inner())
                    .apply_git(&ev.session_id, info);
            }
        }
        if sess_tp.is_some()
            || matches!(
                ev.hook_event_name.as_str(),
                "UserPromptSubmit" | "PostToolUse" | "Stop"
            )
        {
            let usage = Arc::clone(&state.usage);
            let sid_clone = ev.session_id.clone();
            let claude_dir = state.claude_dir.clone();
            if let Ok(Some(u)) = tokio::task::spawn_blocking(move || {
                usage
                    .lock()
                    .unwrap_or_else(|e| e.into_inner())
                    .scan_session(&sid_clone, sess_tp.as_deref(), &claude_dir)
            })
            .await
            {
                state
                    .office
                    .lock()
                    .unwrap_or_else(|e| e.into_inner())
                    .apply_usage(&ev.session_id, &u);
            }
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
    ViaTunnel(via_tunnel): ViaTunnel,
    Json(body): Json<Value>,
) -> Result<Json<Value>, StatusCode> {
    enforce_hook_loopback(peer, via_tunnel, &headers)?;
    let ts = now_ms();
    let mut office = state.office.lock().unwrap_or_else(|e| e.into_inner());

    if let Some(sid) = body.get("session_id").and_then(Value::as_str) {
        let tool = body.get("tool_name").and_then(Value::as_str).unwrap_or("");
        let input = body.get("tool_input").unwrap_or(&Value::Null);
        let cwd = office
            .sessions
            .get(sid)
            .map(|s| s.cwd.clone())
            .filter(|s| !s.is_empty())
            .or_else(|| {
                body.get("cwd")
                    .and_then(Value::as_str)
                    .filter(|s| !s.is_empty())
                    .map(str::to_owned)
            })
            .unwrap_or_default();
        if is_edit_tool(tool) {
            let f = file_of(input, &cwd);
            if !f.is_empty() {
                let cwd_opt = (!cwd.is_empty()).then(|| Path::new(cwd.as_str()));
                office.undo.before(sid, Path::new(&f), cwd_opt);
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
    ViaTunnel(via_tunnel): ViaTunnel,
    Json(body): Json<Value>,
) -> Result<Json<Value>, StatusCode> {
    enforce_hook_loopback(peer, via_tunnel, &headers)?;
    let ts = now_ms();
    let sid = body.get("session_id").and_then(Value::as_str).unwrap_or("");
    if !sid.is_empty() {
        let mut office = state.office.lock().unwrap_or_else(|e| e.into_inner());
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
    ViaTunnel(via_tunnel): ViaTunnel,
    Query(q): Query<HashMap<String, String>>,
    Json(body): Json<Value>,
) -> Result<Json<Value>, StatusCode> {
    enforce_hook_loopback(peer, via_tunnel, &headers)?;
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
        let mut tasks = state.tasks.lock().unwrap_or_else(|e| e.into_inner());
        tasks.bind(job, sid, inbox);
    }
    Ok(Json(json!({ "ok": true })))
}

pub async fn post_hook_start(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    OptionalPeer(peer): OptionalPeer,
    ViaTunnel(via_tunnel): ViaTunnel,
    Json(body): Json<Value>,
) -> Result<Json<Value>, StatusCode> {
    enforce_hook_loopback(peer, via_tunnel, &headers)?;
    let sid = body.get("session_id").and_then(Value::as_str).unwrap_or("");
    let office = state.office.lock().unwrap_or_else(|e| e.into_inner());
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

fn compute_hold_plan(state: &AppState, headers: &HeaderMap, normal_ms: u64) -> HoldDecision {
    let now = now_ms();
    let (focused, bar_last_seen_ms) = {
        let office = state.office.lock().unwrap_or_else(|e| e.into_inner());
        (office.presence_focused, office.bar_last_seen_ms)
    };
    let (mac_approve, phone_approve) = {
        let prefs = state.prefs.lock().unwrap_or_else(|e| e.into_inner());
        let mac = prefs
            .get("macApprove")
            .and_then(Value::as_bool)
            .unwrap_or(true);
        let phone = prefs
            .get("phoneApprove")
            .and_then(Value::as_bool)
            .unwrap_or(false);
        (mac, phone)
    };
    let push_count = state.push.lock().unwrap_or_else(|e| e.into_inner()).count();
    let bar_alive = bar_last_seen_ms > 0 && now.saturating_sub(bar_last_seen_ms) < BAR_ALIVE_MS;
    let phone = phone_approve && push_count > 0;
    let idle_sec = if phone || bar_alive {
        state.sys.platform().idle_seconds().unwrap_or(f64::INFINITY)
    } else {
        f64::INFINITY
    };

    let header_ms = wait_header_ms(headers.get("x-office-wait").and_then(|v| v.to_str().ok()));
    let phone_ms = header_ms.unwrap_or(normal_ms).min(PHONE_HOLD_MS);
    let plan = hold_plan(HoldInput {
        focused,
        bar_alive,
        idle_sec,
        pref: mac_approve,
        normal_ms,
        bar_ms: effective_bar_hold_ms(),
        phone,
        phone_ms,
    });
    let ms = match header_ms {
        Some(w) => plan.ms.min(w),
        None => plan.ms,
    };
    HoldDecision { ms, by: plan.by }
}

fn spawn_phone_push(state: &Arc<AppState>, title: String, body: String, tag: String) {
    let msg = PushMessage {
        title,
        body: clip(&body, 180),
        tag: Some(tag),
        url: "/phone".to_owned(),
        urgent: true,
    };
    let deliveries = state
        .push
        .lock()
        .unwrap_or_else(|e| e.into_inner())
        .prepare_deliveries(&msg, now_ms() / 1000);
    if deliveries.is_empty() {
        return;
    }
    let st = Arc::clone(state);
    tokio::spawn(async move {
        let dead = deliver_prepared(deliveries).await;
        if !dead.is_empty() {
            let mut push = st.push.lock().unwrap_or_else(|e| e.into_inner());
            for ep in dead {
                push.remove(&ep);
            }
        }
    });
}

struct HoldCleanupGuard {
    state: Arc<AppState>,
    id: String,
    active: bool,
}

impl Drop for HoldCleanupGuard {
    fn drop(&mut self) {
        if self.active {
            let mut office = self.state.office.lock().unwrap_or_else(|e| e.into_inner());
            office.cancel_hold(&self.id);
        }
    }
}

async fn await_hold(
    state: Arc<AppState>,
    perm_id: String,
    hold_ms: u64,
    rx: oneshot::Receiver<Value>,
) -> Value {
    let mut guard = HoldCleanupGuard {
        state,
        id: perm_id,
        active: true,
    };
    match tokio::time::timeout(Duration::from_millis(hold_ms), rx).await {
        Ok(Ok(val)) => {
            guard.active = false;
            val
        }
        _ => json!({}),
    }
}

fn raw_perm_detail(tool_input: &Value) -> String {
    tool_input
        .get("command")
        .or_else(|| tool_input.get("file_path"))
        .or_else(|| tool_input.get("url"))
        .or_else(|| tool_input.get("pattern"))
        .or_else(|| tool_input.get("query"))
        .or_else(|| tool_input.get("prompt"))
        .and_then(Value::as_str)
        .map(str::to_owned)
        .unwrap_or_else(|| tool_input.to_string())
}

fn perm_detail_clipped(raw: &str) -> String {
    clip(&redact(raw), 400)
}

fn is_no_verdict_reason(reason: &str) -> bool {
    let lower = reason.trim().to_ascii_lowercase();
    lower.starts_with("auto mode could not evaluate")
        || lower.starts_with("classifier unavailable")
}

pub async fn post_hook_permission(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    OptionalPeer(peer): OptionalPeer,
    ViaTunnel(via_tunnel): ViaTunnel,
    Json(body): Json<Value>,
) -> Result<Json<Value>, StatusCode> {
    enforce_hook_loopback(peer, via_tunnel, &headers)?;
    let sid = body.get("session_id").and_then(Value::as_str).unwrap_or("");
    let tool_name = body.get("tool_name").and_then(Value::as_str).unwrap_or("tool");
    if sid.is_empty() || matches!(tool_name, "AskUserQuestion" | "ExitPlanMode") {
        return Ok(Json(json!({})));
    }

    let hold = compute_hold_plan(&state, &headers, effective_hold_ms());
    let hold_ms = hold.ms;
    if hold_ms == 0 {
        return Ok(Json(json!({})));
    }

    let ts = now_ms();
    let tool_input = body.get("tool_input").unwrap_or(&Value::Null);
    let summary = classify_tool(tool_name, tool_input);
    let raw_detail = raw_perm_detail(tool_input);
    let dk = danger_key(Some("perm"), Some(&summary.kind), Some(&raw_detail)).map(str::to_owned);
    let detail = perm_detail_clipped(&raw_detail);
    let cwd_name = body
        .get("cwd")
        .and_then(Value::as_str)
        .and_then(|c| Path::new(c).file_name())
        .and_then(|n| n.to_str())
        .map(str::to_owned);
    let agent = body
        .get("agent")
        .and_then(Value::as_str)
        .unwrap_or("claude")
        .to_owned();

    let (tx, rx) = oneshot::channel();
    let (perm_id, push_title, push_body) = {
        let mut office = state.office.lock().unwrap_or_else(|e| e.into_inner());
        let perm_id = format!("p_{}_{}", ts, office.next_seq());
        let name = office
            .sessions
            .get(sid)
            .map(|s| s.name.clone())
            .or(cwd_name)
            .unwrap_or(agent);
        let push_title = format!("{name} · {}", summary.label);
        let push_body = detail.clone();
        let perm = PendingPerm {
            id: perm_id.clone(),
            sess: sid.to_owned(),
            name,
            kind: "perm".to_owned(),
            tool_type: summary.kind,
            label: summary.label,
            detail,
            danger: dk,
            created_at: ts,
        };
        office.register_hold(perm, tx);
        (perm_id, push_title, push_body)
    };
    if hold.by == Some("phone") {
        spawn_phone_push(&state, push_title, push_body, format!("perm-{perm_id}"));
    }

    let response = await_hold(state, perm_id, hold_ms, rx).await;
    Ok(Json(response))
}

pub async fn post_hook_question(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    OptionalPeer(peer): OptionalPeer,
    ViaTunnel(via_tunnel): ViaTunnel,
    Json(body): Json<Value>,
) -> Result<Json<Value>, StatusCode> {
    enforce_hook_loopback(peer, via_tunnel, &headers)?;
    let sid = body.get("session_id").and_then(Value::as_str).unwrap_or("");
    let tool_name = body.get("tool_name").and_then(Value::as_str).unwrap_or("");
    if sid.is_empty() || !matches!(tool_name, "AskUserQuestion" | "ExitPlanMode") {
        return Ok(Json(json!({})));
    }

    let tool_input = body
        .get("tool_input")
        .cloned()
        .unwrap_or_else(|| json!({}));
    let (kind, tool_type, label, detail) = if tool_name == "AskUserQuestion" {
        let Some(qs) = tool_input.get("questions").and_then(Value::as_array) else {
            return Ok(Json(json!({})));
        };
        if qs.is_empty() {
            return Ok(Json(json!({})));
        }
        let detail = qs
            .iter()
            .take(4)
            .filter_map(|q| q.get("question").and_then(Value::as_str))
            .map(|q| clip(&redact(&one_line(q)), 400))
            .collect::<Vec<_>>()
            .join("\n");
        ("ask", "ask", "Question", detail)
    } else {
        let plan = tool_input
            .get("plan")
            .and_then(Value::as_str)
            .unwrap_or("");
        ("plan", "plan", "Plan", clip(&redact(&one_line(plan)), 200))
    };

    let hold = compute_hold_plan(&state, &headers, effective_hold_ms());
    let hold_ms = hold.ms;
    if hold_ms == 0 {
        return Ok(Json(json!({})));
    }

    let ts = now_ms();
    let cwd_name = body
        .get("cwd")
        .and_then(Value::as_str)
        .and_then(|c| Path::new(c).file_name())
        .and_then(|n| n.to_str())
        .map(str::to_owned);

    let (tx, rx) = oneshot::channel();
    let (perm_id, push_title, push_body) = {
        let mut office = state.office.lock().unwrap_or_else(|e| e.into_inner());
        let perm_id = format!("q_{}_{}", ts, office.next_seq());
        let name = office
            .sessions
            .get(sid)
            .map(|s| s.name.clone())
            .or(cwd_name)
            .unwrap_or_else(|| "claude".to_owned());
        let push_title = format!("{name} · {label}");
        let push_body = detail.clone();
        let perm = PendingPerm {
            id: perm_id.clone(),
            sess: sid.to_owned(),
            name,
            kind: kind.to_owned(),
            tool_type: tool_type.to_owned(),
            label: label.to_owned(),
            detail,
            danger: None,
            created_at: ts,
        };
        office.register_hold_with_input(perm, tx, Some(tool_input));
        (perm_id, push_title, push_body)
    };
    if hold.by == Some("phone") {
        spawn_phone_push(&state, push_title, push_body, format!("perm-{perm_id}"));
    }

    let response = await_hold(state, perm_id, hold_ms, rx).await;
    Ok(Json(response))
}

pub async fn post_hook_denied(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    OptionalPeer(peer): OptionalPeer,
    ViaTunnel(via_tunnel): ViaTunnel,
    Json(body): Json<Value>,
) -> Result<Json<Value>, StatusCode> {
    enforce_hook_loopback(peer, via_tunnel, &headers)?;
    let ts = now_ms();
    let sid = body
        .get("session_id")
        .and_then(Value::as_str)
        .unwrap_or("claude")
        .to_owned();
    let reason = body.get("reason").and_then(Value::as_str).unwrap_or("");
    let can_retry = !is_no_verdict_reason(reason);
    let hold = if can_retry {
        compute_hold_plan(&state, &headers, DENY_HOLD_MS.min(effective_hold_ms()))
    } else {
        HoldDecision { ms: 0, by: None }
    };
    let hold_ms = hold.ms;

    let mut redacted = crate::redact::redact_json_value(&body);
    if let Some(obj) = redacted.as_object_mut() {
        obj.insert(
            "state".to_owned(),
            Value::String(if hold_ms > 0 { "held" } else { "kept" }.to_owned()),
        );
        obj.insert("retry".to_owned(), Value::Bool(can_retry));
        obj.insert(
            "until".to_owned(),
            Value::from(if hold_ms > 0 { ts.saturating_add(hold_ms) } else { 0 }),
        );
    }

    let tool_name = body.get("tool_name").and_then(Value::as_str).unwrap_or("tool");
    let tool_input = body.get("tool_input").unwrap_or(&Value::Null);
    let summary = classify_tool(tool_name, tool_input);
    let raw_detail = raw_perm_detail(tool_input);
    let dk = danger_key(Some("perm"), Some(&summary.kind), Some(&raw_detail)).map(str::to_owned);
    let detail = perm_detail_clipped(&raw_detail);

    let hold_rx = {
        let mut office = state.office.lock().unwrap_or_else(|e| e.into_inner());
        let sname = office
            .sessions
            .get(&sid)
            .map(|s| s.name.clone())
            .unwrap_or_else(|| sid.clone());
        let card = office.chat.deny_card(&sid, &sname, redacted, ts);
        let _ = office.save_chat();
        if hold_ms > 0 {
            let (tx, rx) = oneshot::channel();
            let card_id = card.id.clone();
            let push_title = format!("{sname} · Denied: {}", summary.label);
            let push_body = detail.clone();
            let perm = PendingPerm {
                id: card_id.clone(),
                sess: sid,
                name: sname,
                kind: "denied".to_owned(),
                tool_type: summary.kind,
                label: summary.label,
                detail,
                danger: dk,
                created_at: ts,
            };
            office.register_hold(perm, tx);
            Some((card_id, rx, push_title, push_body))
        } else {
            None
        }
    };

    if let Some((card_id, rx, push_title, push_body)) = hold_rx {
        if hold.by == Some("phone") {
            spawn_phone_push(&state, push_title, push_body, format!("perm-{card_id}"));
        }
        let response = await_hold(state, card_id, hold_ms, rx).await;
        Ok(Json(response))
    } else {
        Ok(Json(json!({})))
    }
}

#[cfg(test)]
mod tests {
    use crate::http::{AppState, build_router};
    use axum::body::Body;
    use axum::http::{Request, StatusCode};
    use serde_json::json;
    use std::sync::Arc;
    use tower::ServiceExt;

    #[tokio::test]
    async fn post_hook_denied_recursively_redacts_secrets_before_deny_card() {
        let dir = tempfile::tempdir().unwrap();
        let state = Arc::new(AppState::new(dir.path().to_path_buf(), None).unwrap());
        let app = build_router(state.clone());

        let payload = json!({
            "session_id": "s1",
            "tool_name": "Bash",
            "reason": "leaked sk-ant-api03-secret1234",
            "tool_input": {
                "command": "curl -H 'Authorization: Bearer mytok' https://api.example.com",
                "nested": {
                    "gh": "token ghp_1234567890abcdef",
                    "aws": ["AKIAIOSFODNN7EXAMPLE", "safe-value"]
                }
            }
        });
        let req = Request::builder()
            .method("POST")
            .uri("/hook/denied")
            .header("host", "127.0.0.1:4310")
            .header("content-type", "application/json")
            .body(Body::from(payload.to_string()))
            .unwrap();
        let resp = app.oneshot(req).await.unwrap();
        assert_eq!(resp.status(), StatusCode::OK);

        let deny_json = state
            .office
            .lock()
            .unwrap()
            .chat
            .msgs
            .last()
            .unwrap()
            .deny
            .as_ref()
            .unwrap()
            .to_string();
        for secret in [
            "sk-ant-api03-secret1234",
            "ghp_1234567890abcdef",
            "AKIAIOSFODNN7EXAMPLE",
            "mytok",
        ] {
            assert!(!deny_json.contains(secret), "deny_card leaked {secret}: {deny_json}");
        }
        assert!(deny_json.contains("safe-value"));
    }

    #[tokio::test]
    async fn post_hook_loop_prefers_registered_session_cwd_over_payload_cwd() {
        let dir = tempfile::tempdir().unwrap();
        let workspace = dir.path().join("workspace");
        std::fs::create_dir_all(&workspace).unwrap();
        let outside = dir.path().join("outside.txt");
        std::fs::write(&outside, "outside-before").unwrap();

        let state = Arc::new(AppState::new(dir.path().join("data"), None).unwrap());
        {
            let mut office = state.office.lock().unwrap();
            office.ingest_raw_event(json!({
                "session_id": "sess-cwd-lock",
                "hook_event_name": "UserPromptSubmit",
                "cwd": workspace.to_string_lossy(),
                "prompt": "edit something"
            }));
        }

        let app = build_router(state.clone());
        // Payload attempts to widen cwd to parent directory and target outside.txt.
        let payload = json!({
            "session_id": "sess-cwd-lock",
            "tool_name": "Edit",
            "cwd": dir.path().to_string_lossy(),
            "tool_input": { "file_path": outside.to_string_lossy() }
        });
        let req = Request::builder()
            .method("POST")
            .uri("/hook/loop")
            .header("host", "127.0.0.1:4310")
            .header("content-type", "application/json")
            .body(Body::from(payload.to_string()))
            .unwrap();
        let resp = app.oneshot(req).await.unwrap();
        assert_eq!(resp.status(), StatusCode::OK);

        let office = state.office.lock().unwrap();
        assert!(
            office.undo.plan("sess-cwd-lock").is_none(),
            "hook payload cwd must not widen registered session.cwd boundary"
        );
    }

    #[tokio::test]
    async fn post_event_persists_vault_trophies_tenure_and_replay_before_shutdown() {
        use crate::state::sessions::now_ms;
        use crate::state::tenure::TenureStore;
        use crate::store::otel::day_key;
        use crate::store::trophies::TrophyStore;
        use crate::store::vault::VaultStore;
        use std::collections::HashSet;

        let dir = tempfile::tempdir().unwrap();
        let data_dir = dir.path().to_path_buf();
        let before = now_ms();

        {
            let state = Arc::new(AppState::new(data_dir.clone(), None).unwrap());
            let app = build_router(state);
            for payload in [
                json!({
                    "session_id": "s-kill",
                    "hook_event_name": "UserPromptSubmit",
                    "cwd": "D:/repo/persist-me",
                    "prompt": "persist before SIGKILL"
                }),
                json!({
                    "session_id": "s-kill",
                    "hook_event_name": "PostToolUse",
                    "tool_name": "Edit",
                    "tool_input": { "file_path": "src/lib.rs" }
                }),
                json!({
                    "session_id": "s-kill",
                    "hook_event_name": "Stop",
                    "cwd": "D:/repo/persist-me"
                }),
            ] {
                let req = Request::builder()
                    .method("POST")
                    .uri("/event")
                    .header("host", "127.0.0.1:4310")
                    .header("content-type", "application/json")
                    .body(Body::from(payload.to_string()))
                    .unwrap();
                assert_eq!(app.clone().oneshot(req).await.unwrap().status(), StatusCode::OK);
            }
            // Drop without calling flush_all() to simulate sudden process termination.
        }

        assert!(data_dir.join("vault.json").exists(), "vault.json must persist before shutdown");
        assert!(data_dir.join("totals.json").exists(), "totals.json must persist before shutdown");
        assert!(data_dir.join("tenure.json").exists(), "tenure.json must persist before shutdown");
        let replay_path = data_dir
            .join("replay")
            .join(format!("{}.jsonl", day_key(before)));
        assert!(replay_path.exists(), "replay JSONL must persist before shutdown");

        let vault = VaultStore::load(&data_dir);
        assert_eq!(vault.search("SIGKILL", 10).len(), 1);

        let trophies = TrophyStore::load(&data_dir, now_ms());
        assert_eq!(trophies.total("prompts"), 1);
        assert_eq!(trophies.total("edit"), 1);
        assert_eq!(trophies.total("sessions"), 1);

        let tenure = TenureStore::load(&data_dir, now_ms());
        let list = tenure.list(30, now_ms(), &HashSet::new());
        assert!(
            list.iter().any(|p| p.name == "persist-me" && p.turns == 1),
            "tenure.json must record Stop turn before shutdown: {list:?}"
        );
    }

    async fn read_json_body(resp: axum::response::Response) -> serde_json::Value {
        let bytes = axum::body::to_bytes(resp.into_body(), 256 * 1024)
            .await
            .unwrap();
        serde_json::from_slice(&bytes).unwrap()
    }

    async fn wait_for_pending_id(state: &Arc<AppState>) -> String {
        let deadline = tokio::time::Instant::now() + std::time::Duration::from_secs(2);
        loop {
            if let Some(id) = state.office.lock().unwrap().pending_perms.keys().next().cloned() {
                return id;
            }
            assert!(
                tokio::time::Instant::now() < deadline,
                "timed out waiting for pending_perms hold registration"
            );
            tokio::time::sleep(std::time::Duration::from_millis(10)).await;
        }
    }

    #[tokio::test]
    async fn post_hook_permission_unfocused_returns_empty_immediately() {
        let dir = tempfile::tempdir().unwrap();
        let state = Arc::new(AppState::new(dir.path().to_path_buf(), None).unwrap());
        let app = build_router(state.clone());

        let payload = json!({
            "session_id": "s-unfocused",
            "tool_name": "Bash",
            "tool_input": { "command": "echo hello" }
        });
        let req = Request::builder()
            .method("POST")
            .uri("/hook/permission")
            .header("host", "127.0.0.1:4310")
            .header("content-type", "application/json")
            .header("x-office-wait", "30")
            .body(Body::from(payload.to_string()))
            .unwrap();

        let started = std::time::Instant::now();
        let resp = app.oneshot(req).await.unwrap();
        let elapsed = started.elapsed();

        assert_eq!(resp.status(), StatusCode::OK);
        assert!(
            elapsed < std::time::Duration::from_millis(50),
            "unfocused /hook/permission must return immediately (< 50 ms), took {elapsed:?}"
        );
        let body = read_json_body(resp).await;
        assert_eq!(body, json!({}));
        assert!(
            state.office.lock().unwrap().pending_perms.is_empty(),
            "unfocused /hook/permission must not leave a dangling pending_perm"
        );
    }

    #[tokio::test]
    async fn post_hook_permission_focused_holds_until_api_perm_resolves_and_caps_wait() {
        let dir = tempfile::tempdir().unwrap();
        let state = Arc::new(AppState::new(dir.path().to_path_buf(), None).unwrap());
        state.office.lock().unwrap().presence_focused = true;
        let app = build_router(state.clone());

        // 1. Focused hook holds until POST /api/perm resolves it with choice: "allow".
        let payload = json!({
            "session_id": "s-focused",
            "tool_name": "Bash",
            "tool_input": { "command": "cargo test" }
        });
        let app_hook = app.clone();
        let hook_task = tokio::spawn(async move {
            let req = Request::builder()
                .method("POST")
                .uri("/hook/permission")
                .header("host", "127.0.0.1:4310")
                .header("content-type", "application/json")
                .header("x-office-wait", "5")
                .body(Body::from(payload.to_string()))
                .unwrap();
            app_hook.oneshot(req).await.unwrap()
        });

        let perm_id = wait_for_pending_id(&state).await;
        assert!(
            !hook_task.is_finished(),
            "/hook/permission must remain blocked while waiting for user decision"
        );

        let decide_req = Request::builder()
            .method("POST")
            .uri("/api/perm")
            .header("host", "127.0.0.1:4310")
            .header("content-type", "application/json")
            .body(Body::from(json!({ "id": perm_id, "choice": "allow" }).to_string()))
            .unwrap();
        let decide_resp = app.clone().oneshot(decide_req).await.unwrap();
        assert_eq!(decide_resp.status(), StatusCode::OK);

        let hook_resp = tokio::time::timeout(std::time::Duration::from_secs(2), hook_task)
            .await
            .expect("hook must unblock promptly after /api/perm")
            .unwrap();
        assert_eq!(hook_resp.status(), StatusCode::OK);
        let hook_json = read_json_body(hook_resp).await;
        assert_eq!(
            hook_json["hookSpecificOutput"]["permissionDecision"],
            "allow",
            "expected Claude hookSpecificOutput.permissionDecision == allow, got {hook_json}"
        );
        assert_eq!(
            hook_json["hookSpecificOutput"]["hookEventName"],
            "PermissionRequest"
        );
        assert_eq!(
            hook_json["hookSpecificOutput"]["decision"]["behavior"],
            "allow"
        );
        assert!(state.office.lock().unwrap().pending_perms.is_empty());

        // 2. X-Office-Wait caps the hold window below normal_ms (45_000 ms) and cleans up on timeout.
        let cap_req = Request::builder()
            .method("POST")
            .uri("/hook/permission")
            .header("host", "127.0.0.1:4310")
            .header("content-type", "application/json")
            .header("x-office-wait", "0.15")
            .body(Body::from(
                json!({
                    "session_id": "s-focused",
                    "tool_name": "Bash",
                    "tool_input": { "command": "echo timeout" }
                })
                .to_string(),
            ))
            .unwrap();
        let started = std::time::Instant::now();
        let cap_resp = app.oneshot(cap_req).await.unwrap();
        let elapsed = started.elapsed();
        assert_eq!(cap_resp.status(), StatusCode::OK);
        assert_eq!(read_json_body(cap_resp).await, json!({}));
        assert!(
            elapsed >= std::time::Duration::from_millis(100)
                && elapsed < std::time::Duration::from_millis(1500),
            "expected hold capped near 150ms by X-Office-Wait, elapsed={elapsed:?}"
        );
        assert!(
            state.office.lock().unwrap().pending_perms.is_empty(),
            "expired hold must be cleaned from pending_perms"
        );
    }

    #[tokio::test]
    async fn post_hook_question_holds_until_answered_via_perm_or_panel_answer() {
        let dir = tempfile::tempdir().unwrap();
        let state = Arc::new(AppState::new(dir.path().to_path_buf(), None).unwrap());
        state.office.lock().unwrap().presence_focused = true;
        let app = build_router(state.clone());

        let q_payload = json!({
            "session_id": "s-ask",
            "tool_name": "AskUserQuestion",
            "tool_input": {
                "questions": [{
                    "question": "Which database?",
                    "header": "DB",
                    "options": [
                        { "label": "Postgres", "description": "Relational" },
                        { "label": "SQLite", "description": "Embedded" }
                    ]
                }]
            }
        });

        // 1. Answer via POST /api/perm
        let app_q1 = app.clone();
        let p1 = q_payload.clone();
        let q1_task = tokio::spawn(async move {
            let req = Request::builder()
                .method("POST")
                .uri("/hook/question")
                .header("host", "127.0.0.1:4310")
                .header("content-type", "application/json")
                .header("x-office-wait", "5")
                .body(Body::from(p1.to_string()))
                .unwrap();
            app_q1.oneshot(req).await.unwrap()
        });

        let q1_id = wait_for_pending_id(&state).await;
        assert!(!q1_task.is_finished());

        let ans1_req = Request::builder()
            .method("POST")
            .uri("/api/perm")
            .header("host", "127.0.0.1:4310")
            .header("content-type", "application/json")
            .body(Body::from(
                json!({
                    "id": q1_id,
                    "choice": "answer",
                    "answers": { "Which database?": "Postgres" }
                })
                .to_string(),
            ))
            .unwrap();
        assert_eq!(app.clone().oneshot(ans1_req).await.unwrap().status(), StatusCode::OK);

        let q1_resp = tokio::time::timeout(std::time::Duration::from_secs(2), q1_task)
            .await
            .unwrap()
            .unwrap();
        assert_eq!(q1_resp.status(), StatusCode::OK);
        let q1_json = read_json_body(q1_resp).await;
        assert_eq!(q1_json["hookSpecificOutput"]["hookEventName"], "PreToolUse");
        assert_eq!(q1_json["hookSpecificOutput"]["permissionDecision"], "allow");
        assert_eq!(
            q1_json["hookSpecificOutput"]["updatedInput"]["answers"]["Which database?"],
            "Postgres"
        );
        assert!(state.office.lock().unwrap().pending_perms.is_empty());

        // 2. Answer via POST /api/panel/answer
        let app_q2 = app.clone();
        let p2 = q_payload;
        let q2_task = tokio::spawn(async move {
            let req = Request::builder()
                .method("POST")
                .uri("/hook/question")
                .header("host", "127.0.0.1:4310")
                .header("content-type", "application/json")
                .header("x-office-wait", "5")
                .body(Body::from(p2.to_string()))
                .unwrap();
            app_q2.oneshot(req).await.unwrap()
        });

        let q2_id = wait_for_pending_id(&state).await;
        assert!(!q2_task.is_finished());

        let ans2_req = Request::builder()
            .method("POST")
            .uri("/api/panel/answer")
            .header("host", "127.0.0.1:4310")
            .header("content-type", "application/json")
            .body(Body::from(
                json!({
                    "id": q2_id,
                    "answers": { "Which database?": "SQLite" }
                })
                .to_string(),
            ))
            .unwrap();
        assert_eq!(app.clone().oneshot(ans2_req).await.unwrap().status(), StatusCode::OK);

        let q2_resp = tokio::time::timeout(std::time::Duration::from_secs(2), q2_task)
            .await
            .unwrap()
            .unwrap();
        assert_eq!(q2_resp.status(), StatusCode::OK);
        let q2_json = read_json_body(q2_resp).await;
        assert_eq!(q2_json["hookSpecificOutput"]["hookEventName"], "PreToolUse");
        assert_eq!(q2_json["hookSpecificOutput"]["permissionDecision"], "allow");
        assert_eq!(
            q2_json["hookSpecificOutput"]["updatedInput"]["answers"]["Which database?"],
            "SQLite"
        );
        assert!(state.office.lock().unwrap().pending_perms.is_empty());
    }

    #[tokio::test]
    async fn post_hook_denied_holds_for_retry_and_resolves_via_api_denied() {
        let dir = tempfile::tempdir().unwrap();
        let state = Arc::new(AppState::new(dir.path().to_path_buf(), None).unwrap());
        state.office.lock().unwrap().presence_focused = true;
        let app = build_router(state.clone());

        let deny_payload = json!({
            "session_id": "s-deny",
            "tool_name": "Bash",
            "reason": "Auto mode blocked potentially destructive command",
            "tool_input": { "command": "rm -rf /tmp/scratch" }
        });

        // 1. Resolve with op: "retry" -> returns hookSpecificOutput { hookEventName: "PermissionDenied", retry: true }
        let app_d1 = app.clone();
        let p1 = deny_payload.clone();
        let d1_task = tokio::spawn(async move {
            let req = Request::builder()
                .method("POST")
                .uri("/hook/denied")
                .header("host", "127.0.0.1:4310")
                .header("content-type", "application/json")
                .header("x-office-wait", "5")
                .body(Body::from(p1.to_string()))
                .unwrap();
            app_d1.oneshot(req).await.unwrap()
        });

        let d1_id = wait_for_pending_id(&state).await;
        assert!(!d1_task.is_finished());

        let retry_req = Request::builder()
            .method("POST")
            .uri("/api/denied")
            .header("host", "127.0.0.1:4310")
            .header("content-type", "application/json")
            .body(Body::from(json!({ "id": d1_id, "op": "retry" }).to_string()))
            .unwrap();
        assert_eq!(app.clone().oneshot(retry_req).await.unwrap().status(), StatusCode::OK);

        let d1_resp = tokio::time::timeout(std::time::Duration::from_secs(2), d1_task)
            .await
            .unwrap()
            .unwrap();
        assert_eq!(d1_resp.status(), StatusCode::OK);
        let d1_json = read_json_body(d1_resp).await;
        assert_eq!(d1_json["hookSpecificOutput"]["hookEventName"], "PermissionDenied");
        assert_eq!(d1_json["hookSpecificOutput"]["retry"], true);
        assert!(state.office.lock().unwrap().pending_perms.is_empty());

        // 2. Resolve with op: "keep" -> returns {}
        let app_d2 = app.clone();
        let p2 = deny_payload;
        let d2_task = tokio::spawn(async move {
            let req = Request::builder()
                .method("POST")
                .uri("/hook/denied")
                .header("host", "127.0.0.1:4310")
                .header("content-type", "application/json")
                .header("x-office-wait", "5")
                .body(Body::from(p2.to_string()))
                .unwrap();
            app_d2.oneshot(req).await.unwrap()
        });

        let d2_id = wait_for_pending_id(&state).await;
        assert!(!d2_task.is_finished());

        let keep_req = Request::builder()
            .method("POST")
            .uri("/api/denied")
            .header("host", "127.0.0.1:4310")
            .header("content-type", "application/json")
            .body(Body::from(json!({ "id": d2_id, "op": "keep" }).to_string()))
            .unwrap();
        assert_eq!(app.clone().oneshot(keep_req).await.unwrap().status(), StatusCode::OK);

        let d2_resp = tokio::time::timeout(std::time::Duration::from_secs(2), d2_task)
            .await
            .unwrap()
            .unwrap();
        assert_eq!(d2_resp.status(), StatusCode::OK);
        let d2_json = read_json_body(d2_resp).await;
        assert_eq!(d2_json, json!({}));
        assert!(state.office.lock().unwrap().pending_perms.is_empty());
    }

    #[tokio::test]
    async fn post_hook_permission_detects_danger_past_400_char_prefix() {
        let dir = tempfile::tempdir().unwrap();
        let state = Arc::new(AppState::new(dir.path().to_path_buf(), None).unwrap());
        state.office.lock().unwrap().presence_focused = true;
        let app = build_router(state.clone());

        let long_cmd = format!("{}rm -rf /", "echo hello world; ".repeat(30));
        let payload = json!({
            "session_id": "s-long-danger",
            "tool_name": "Bash",
            "tool_input": { "command": long_cmd }
        });

        let app_hook = app.clone();
        let hook_task = tokio::spawn(async move {
            let req = Request::builder()
                .method("POST")
                .uri("/hook/permission")
                .header("host", "127.0.0.1:4310")
                .header("content-type", "application/json")
                .header("x-office-wait", "5")
                .body(Body::from(payload.to_string()))
                .unwrap();
            app_hook.oneshot(req).await.unwrap()
        });

        let perm_id = wait_for_pending_id(&state).await;
        {
            let office = state.office.lock().unwrap();
            let perm = office.pending_perms.get(&perm_id).unwrap();
            assert!(
                perm.danger.is_some(),
                "danger_key must run on full unclipped command before 400-char truncation"
            );
            assert!(perm.detail.chars().count() <= 400);
        }

        // Resolve so background hook finishes immediately.
        let _ = state
            .office
            .lock()
            .unwrap()
            .resolve_hold(&perm_id, crate::state::sessions::PermDecision::Deny {
                reason: "blocked".to_owned(),
            });
        let _ = tokio::time::timeout(std::time::Duration::from_secs(2), hook_task).await;
    }

    #[tokio::test]
    async fn post_denied_enforces_denied_kind_and_blocks_remote_dangerous_retry() {
        use crate::auth::COOKIE_NAME;
        use axum::extract::ConnectInfo;
        use std::net::{IpAddr, Ipv4Addr, SocketAddr};

        let dir = tempfile::tempdir().unwrap();
        let state = Arc::new(AppState::new(dir.path().to_path_buf(), None).unwrap());
        state.office.lock().unwrap().presence_focused = true;
        state
            .prefs
            .lock()
            .unwrap()
            .set("phoneApprove", Some(serde_json::Value::Bool(true)))
            .unwrap();
        let cookie = format!("{COOKIE_NAME}={}", state.auth.sign_cookie(false));
        let app = build_router(state.clone());

        // 1. Spawn a dangerous /hook/denied hold (rm -rf after 500-char prefix too).
        let deny_payload = json!({
            "session_id": "s-deny-danger",
            "tool_name": "Bash",
            "reason": "Blocked destructive command",
            "tool_input": { "command": format!("{}rm -rf /tmp/foo", "echo safe step; ".repeat(30)) }
        });
        let app_d = app.clone();
        let deny_task = tokio::spawn(async move {
            let req = Request::builder()
                .method("POST")
                .uri("/hook/denied")
                .header("host", "127.0.0.1:4310")
                .header("content-type", "application/json")
                .header("x-office-wait", "5")
                .body(Body::from(deny_payload.to_string()))
                .unwrap();
            app_d.oneshot(req).await.unwrap()
        });
        let deny_id = wait_for_pending_id(&state).await;
        assert!(
            state
                .office
                .lock()
                .unwrap()
                .pending_perms
                .get(&deny_id)
                .unwrap()
                .danger
                .is_some(),
            "destructive /hook/denied command must populate PendingPerm.danger"
        );

        // 2. Also register a normal "perm" hold to test cross-kind isolation.
        let (perm_tx, mut perm_rx) = tokio::sync::oneshot::channel();
        {
            let mut office = state.office.lock().unwrap();
            office.register_hold(
                crate::state::sessions::PendingPerm {
                    id: "perm-cross".to_owned(),
                    sess: "s-cross".to_owned(),
                    name: "proj".to_owned(),
                    kind: "perm".to_owned(),
                    tool_type: "bash".to_owned(),
                    label: "echo".to_owned(),
                    detail: "echo hi".to_owned(),
                    danger: None,
                    created_at: 1,
                },
                perm_tx,
            );
        }

        let lan_peer = SocketAddr::new(IpAddr::V4(Ipv4Addr::new(192, 168, 1, 50)), 52100);

        // 3. Remote POST /api/denied targeting a "perm" hold -> 400 and perm preserved.
        let mut cross_req = Request::builder()
            .method("POST")
            .uri("/api/denied")
            .header("host", "192.168.1.20:4310")
            .header("cookie", &cookie)
            .header("content-type", "application/json")
            .body(Body::from(json!({ "id": "perm-cross", "op": "retry" }).to_string()))
            .unwrap();
        cross_req.extensions_mut().insert(ConnectInfo(lan_peer));
        let cross_resp = app.clone().oneshot(cross_req).await.unwrap();
        assert_eq!(cross_resp.status(), StatusCode::BAD_REQUEST);
        assert!(state.office.lock().unwrap().pending_perms.contains_key("perm-cross"));
        assert!(perm_rx.try_recv().is_err());

        // 4. Remote POST /api/denied targeting dangerous "denied" hold -> 403 and hold preserved.
        let mut remote_danger_req = Request::builder()
            .method("POST")
            .uri("/api/denied")
            .header("host", "192.168.1.20:4310")
            .header("cookie", &cookie)
            .header("content-type", "application/json")
            .body(Body::from(json!({ "id": deny_id, "op": "retry" }).to_string()))
            .unwrap();
        remote_danger_req.extensions_mut().insert(ConnectInfo(lan_peer));
        let rd_resp = app.clone().oneshot(remote_danger_req).await.unwrap();
        assert_eq!(rd_resp.status(), StatusCode::FORBIDDEN);
        assert!(state.office.lock().unwrap().pending_perms.contains_key(&deny_id));

        // 5. Loopback POST /api/denied resolves dangerous "denied" hold with retry: true.
        let local_req = Request::builder()
            .method("POST")
            .uri("/api/denied")
            .header("host", "127.0.0.1:4310")
            .header("content-type", "application/json")
            .body(Body::from(json!({ "id": deny_id, "op": "retry" }).to_string()))
            .unwrap();
        assert_eq!(app.clone().oneshot(local_req).await.unwrap().status(), StatusCode::OK);
        let d_resp = tokio::time::timeout(std::time::Duration::from_secs(2), deny_task)
            .await
            .unwrap()
            .unwrap();
        let d_json = read_json_body(d_resp).await;
        assert_eq!(d_json["hookSpecificOutput"]["retry"], true);

        // Clean up perm-cross.
        state.office.lock().unwrap().cancel_hold("perm-cross");
    }
}
