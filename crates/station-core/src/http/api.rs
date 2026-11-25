use crate::http::AppState;
use crate::http::guard::{
    OptionalPeer, ViaTunnel, enforce_loopback_strict, enforce_same_origin_post_with_transport,
    is_authorized, is_loopback_peer,
};
use crate::shared::timeline::build_timeline;
use crate::state::loops::LoopConfig;
use crate::state::models::{default_claude_models, models_of, with_models};
use crate::state::sessions::now_ms;
use crate::store::prompt_report::build_prompt_report;
use crate::store::push::{PushKeys, PushSubscription};
use crate::store::wrapped::week_summary;
use axum::Json;
use axum::extract::{Path as AxumPath, Query, State};
use axum::http::{HeaderMap, StatusCode, header};
use axum::response::{IntoResponse, Response};
use serde_json::{Value, json};
use std::collections::{HashMap, HashSet};
use std::net::{IpAddr, Ipv4Addr, SocketAddr};
use std::sync::Arc;

fn require_auth(
    state: &AppState,
    peer: Option<SocketAddr>,
    via_tunnel: bool,
    headers: &HeaderMap,
    q: &HashMap<String, String>,
) -> Result<bool, StatusCode> {
    if !is_authorized(
        peer,
        via_tunnel,
        headers,
        q.get("k").map(String::as_str),
        &state.auth,
    ) {
        return Err(StatusCode::UNAUTHORIZED);
    }
    Ok(is_loopback_peer(peer, via_tunnel, headers))
}

fn require_local_post(
    peer: Option<SocketAddr>,
    via_tunnel: bool,
    headers: &HeaderMap,
) -> Result<(), StatusCode> {
    enforce_loopback_strict(peer, via_tunnel, headers)
}

pub async fn get_live(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    OptionalPeer(peer): OptionalPeer,
    ViaTunnel(via_tunnel): ViaTunnel,
    Query(q): Query<HashMap<String, String>>,
) -> Result<Json<Value>, StatusCode> {
    let local = require_auth(&state, peer, via_tunnel, &headers, &q)?;
    let lang = state.prefs.lock().unwrap_or_else(|e| e.into_inner()).lang();
    let office = state.office.lock().unwrap_or_else(|e| e.into_inner());
    let mut snap = office.snapshot_json(local, lang);
    if let Some(obj) = snap.as_object_mut() {
        obj.insert("trophies".to_owned(), state.trophies.lock().unwrap_or_else(|e| e.into_inner()).pub_view());
        obj.insert("lift".to_owned(), json!(state.lift.pub_view(now_ms())));
        if local {
            obj.insert("pin".to_owned(), Value::String(state.auth.current_pin()));
        }
    }
    Ok(Json(snap))
}

pub async fn get_doctor(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    OptionalPeer(peer): OptionalPeer,
    ViaTunnel(via_tunnel): ViaTunnel,
    Query(q): Query<HashMap<String, String>>,
) -> Result<Json<Value>, StatusCode> {
    let _ = require_auth(&state, peer, via_tunnel, &headers, &q)?;
    Ok(Json(json!({
        "ok": true,
        "platform": state.sys.platform().name(),
        "dataDir": state.data_dir,
        "otel": state.otel.lock().unwrap_or_else(|e| e.into_inner()).info(),
        "sessions": state.office.lock().unwrap_or_else(|e| e.into_inner()).sessions.len(),
    })))
}

pub async fn get_log(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    OptionalPeer(peer): OptionalPeer,
    ViaTunnel(via_tunnel): ViaTunnel,
    Query(q): Query<HashMap<String, String>>,
) -> Result<Json<Value>, StatusCode> {
    let _ = require_auth(&state, peer, via_tunnel, &headers, &q)?;
    let office = state.office.lock().unwrap_or_else(|e| e.into_inner());
    Ok(Json(json!({ "ok": true, "feed": office.feed })))
}

pub async fn get_timeline(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    OptionalPeer(peer): OptionalPeer,
    ViaTunnel(via_tunnel): ViaTunnel,
    Query(q): Query<HashMap<String, String>>,
) -> Result<Json<Value>, StatusCode> {
    let _ = require_auth(&state, peer, via_tunnel, &headers, &q)?;
    let now = now_ms();
    let from = q
        .get("from")
        .and_then(|s| s.parse::<u64>().ok())
        .unwrap_or_else(|| now.saturating_sub(86_400_000));
    let to = q.get("to").and_then(|s| s.parse::<u64>().ok()).unwrap_or(now);
    let tl = build_timeline(&[], from, to, now);
    Ok(Json(json!(tl)))
}

pub async fn get_wrapped(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    OptionalPeer(peer): OptionalPeer,
    ViaTunnel(via_tunnel): ViaTunnel,
    Query(q): Query<HashMap<String, String>>,
) -> Result<Json<Value>, StatusCode> {
    let _ = require_auth(&state, peer, via_tunnel, &headers, &q)?;
    let summary = week_summary(&state.data_dir.join("replay"), 7, now_ms());
    Ok(Json(summary))
}

pub fn is_valid_day_key(day: &str) -> bool {
    let b = day.as_bytes();
    b.len() == 10
        && b[0..4].iter().all(u8::is_ascii_digit)
        && b[4] == b'-'
        && b[5..7].iter().all(u8::is_ascii_digit)
        && b[7] == b'-'
        && b[8..10].iter().all(u8::is_ascii_digit)
}

pub async fn get_replay(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    OptionalPeer(peer): OptionalPeer,
    ViaTunnel(via_tunnel): ViaTunnel,
    Query(q): Query<HashMap<String, String>>,
) -> Result<Json<Value>, StatusCode> {
    let _ = require_auth(&state, peer, via_tunnel, &headers, &q)?;
    let day = q.get("day").map(String::as_str).unwrap_or("");
    if !is_valid_day_key(day) {
        return Err(StatusCode::BAD_REQUEST);
    }
    let file = state.data_dir.join("replay").join(format!("{day}.jsonl"));
    let items: Vec<Value> = std::fs::read_to_string(file)
        .ok()
        .map(|t| {
            t.lines()
                .filter_map(|l| serde_json::from_str::<Value>(l).ok())
                .collect()
        })
        .unwrap_or_default();
    Ok(Json(json!({ "ok": true, "day": day, "items": items })))
}

pub async fn get_replay_days(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    OptionalPeer(peer): OptionalPeer,
    ViaTunnel(via_tunnel): ViaTunnel,
    Query(q): Query<HashMap<String, String>>,
) -> Result<Json<Value>, StatusCode> {
    let _ = require_auth(&state, peer, via_tunnel, &headers, &q)?;
    let mut days: Vec<String> = std::fs::read_dir(state.data_dir.join("replay"))
        .ok()
        .into_iter()
        .flatten()
        .flatten()
        .filter_map(|e| {
            let n = e.file_name().to_string_lossy().into_owned();
            n.strip_suffix(".jsonl").map(str::to_owned)
        })
        .collect();
    days.sort();
    Ok(Json(json!({ "ok": true, "days": days })))
}

pub async fn get_stranded(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    OptionalPeer(peer): OptionalPeer,
    ViaTunnel(via_tunnel): ViaTunnel,
    Query(q): Query<HashMap<String, String>>,
) -> Result<Json<Value>, StatusCode> {
    let _ = require_auth(&state, peer, via_tunnel, &headers, &q)?;
    Ok(Json(json!({ "ok": true, "at": now_ms(), "items": [], "repos": 0 })))
}

pub async fn get_pins(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    OptionalPeer(peer): OptionalPeer,
    ViaTunnel(via_tunnel): ViaTunnel,
    Query(q): Query<HashMap<String, String>>,
) -> Result<Json<Value>, StatusCode> {
    let _ = require_auth(&state, peer, via_tunnel, &headers, &q)?;
    let office = state.office.lock().unwrap_or_else(|e| e.into_inner());
    Ok(Json(json!({ "ok": true, "pins": office.pins })))
}

pub async fn post_pins(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    OptionalPeer(peer): OptionalPeer,
    ViaTunnel(via_tunnel): ViaTunnel,
    Json(body): Json<Value>,
) -> Result<Json<Value>, StatusCode> {
    require_local_post(peer, via_tunnel, &headers)?;
    let sid = body.get("sid").and_then(Value::as_str).unwrap_or("");
    let mut office = state.office.lock().unwrap_or_else(|e| e.into_inner());
    if let Some(idx) = body.get("remove").and_then(Value::as_u64) {
        let ok = office.remove_pin(sid, idx as usize).unwrap_or(false);
        return Ok(Json(json!({ "ok": ok, "pins": office.pins })));
    }
    let text = body.get("text").and_then(Value::as_str).unwrap_or("");
    let ok = office.add_pin(sid, text).unwrap_or(false);
    Ok(Json(json!({ "ok": ok, "pins": office.pins })))
}

pub async fn get_vault(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    OptionalPeer(peer): OptionalPeer,
    ViaTunnel(via_tunnel): ViaTunnel,
    Query(q): Query<HashMap<String, String>>,
) -> Result<Json<Value>, StatusCode> {
    let _ = require_auth(&state, peer, via_tunnel, &headers, &q)?;
    let query = q.get("q").map(String::as_str).unwrap_or("");
    let vault = state.vault.lock().unwrap_or_else(|e| e.into_inner());
    Ok(Json(json!({ "ok": true, "items": vault.search(query, 50) })))
}

pub async fn get_vault_report(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    OptionalPeer(peer): OptionalPeer,
    ViaTunnel(via_tunnel): ViaTunnel,
    Query(q): Query<HashMap<String, String>>,
) -> Result<Json<Value>, StatusCode> {
    let _ = require_auth(&state, peer, via_tunnel, &headers, &q)?;
    let since = now_ms().saturating_sub(7 * 86_400_000);
    let prompts = state.vault.lock().unwrap_or_else(|e| e.into_inner()).recent_prompts(since);
    Ok(Json(json!(build_prompt_report(&prompts, 3))))
}

pub async fn get_undo(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    OptionalPeer(peer): OptionalPeer,
    ViaTunnel(via_tunnel): ViaTunnel,
    Query(q): Query<HashMap<String, String>>,
) -> Result<Json<Value>, StatusCode> {
    let _ = require_auth(&state, peer, via_tunnel, &headers, &q)?;
    let sid = q.get("sid").map(String::as_str).unwrap_or("");
    let office = state.office.lock().unwrap_or_else(|e| e.into_inner());
    Ok(Json(json!({ "ok": true, "plan": office.undo.plan(sid) })))
}

pub async fn post_undo(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    OptionalPeer(peer): OptionalPeer,
    ViaTunnel(via_tunnel): ViaTunnel,
    Json(body): Json<Value>,
) -> Result<Json<Value>, StatusCode> {
    require_local_post(peer, via_tunnel, &headers)?;
    let sid = body.get("sid").and_then(Value::as_str).unwrap_or("");
    let mut office = state.office.lock().unwrap_or_else(|e| e.into_inner());
    let res = office.undo.apply(sid, now_ms());
    Ok(Json(json!({ "ok": true, "result": res })))
}

pub async fn get_burn(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    OptionalPeer(peer): OptionalPeer,
    ViaTunnel(via_tunnel): ViaTunnel,
    Query(q): Query<HashMap<String, String>>,
) -> Result<Json<Value>, StatusCode> {
    let _ = require_auth(&state, peer, via_tunnel, &headers, &q)?;
    let range = q.get("range").map(String::as_str).unwrap_or("today");
    let summary = state.otel.lock().unwrap_or_else(|e| e.into_inner()).burn_summary(range, now_ms());
    Ok(Json(json!(summary)))
}

pub async fn get_sys(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    OptionalPeer(peer): OptionalPeer,
    ViaTunnel(via_tunnel): ViaTunnel,
    Query(q): Query<HashMap<String, String>>,
) -> Result<Json<Value>, StatusCode> {
    let _ = require_auth(&state, peer, via_tunnel, &headers, &q)?;
    Ok(Json(json!(state.sys.snapshot())))
}

pub async fn post_sys_action(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    OptionalPeer(peer): OptionalPeer,
    ViaTunnel(via_tunnel): ViaTunnel,
    Json(body): Json<Value>,
) -> Result<Json<Value>, StatusCode> {
    require_local_post(peer, via_tunnel, &headers)?;
    let action = body.get("action").and_then(Value::as_str).unwrap_or("");
    let force = body.get("force").and_then(Value::as_bool).unwrap_or(false);
    let start = body.get("start").and_then(Value::as_u64);
    Ok(Json(state.sys.action(action, body.get("target"), force, start)))
}

pub async fn post_sys_auto(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    OptionalPeer(peer): OptionalPeer,
    ViaTunnel(via_tunnel): ViaTunnel,
    Json(body): Json<Value>,
) -> Result<Json<Value>, StatusCode> {
    require_local_post(peer, via_tunnel, &headers)?;
    let hours = body.get("hours").and_then(Value::as_u64).unwrap_or(0) as u8;
    let ok = state.prefs.lock().unwrap_or_else(|e| e.into_inner()).set_auto_close(hours).unwrap_or(false);
    Ok(Json(json!({ "ok": ok, "hours": hours })))
}

pub async fn get_panel(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    OptionalPeer(peer): OptionalPeer,
    ViaTunnel(via_tunnel): ViaTunnel,
    Query(q): Query<HashMap<String, String>>,
) -> Result<Json<Value>, StatusCode> {
    let _ = require_auth(&state, peer, via_tunnel, &headers, &q)?;
    let mut office = state.office.lock().unwrap_or_else(|e| e.into_inner());
    office.bar_last_seen_ms = now_ms();
    Ok(Json(json!({
        "ok": true,
        "sessions": office.sessions.values().collect::<Vec<_>>(),
        "perms": office.pending_perms.values().collect::<Vec<_>>(),
    })))
}

pub async fn get_chat(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    OptionalPeer(peer): OptionalPeer,
    ViaTunnel(via_tunnel): ViaTunnel,
    Query(q): Query<HashMap<String, String>>,
) -> Result<Json<Value>, StatusCode> {
    let _ = require_auth(&state, peer, via_tunnel, &headers, &q)?;
    let office = state.office.lock().unwrap_or_else(|e| e.into_inner());
    Ok(Json(json!({
        "ok": true,
        "msgs": office.chat.msgs,
        "pinned": office.chat.pinned,
        "typing": office.chat.typing,
    })))
}

pub async fn post_chat(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    OptionalPeer(peer): OptionalPeer,
    ViaTunnel(via_tunnel): ViaTunnel,
    Query(q): Query<HashMap<String, String>>,
    Json(body): Json<Value>,
) -> Result<Json<Value>, StatusCode> {
    enforce_same_origin_post_with_transport(
        &headers,
        state.allowed_tunnel_host().as_deref(),
        via_tunnel,
    )?;
    let _ = require_auth(&state, peer, via_tunnel, &headers, &q)?;
    let ts = now_ms();
    let mut office = state.office.lock().unwrap_or_else(|e| e.into_inner());
    let op = body.get("op").and_then(Value::as_str).unwrap_or("send");
    match op {
        "react" => {
            let id = body.get("id").and_then(Value::as_str).unwrap_or("");
            let emoji = body.get("emoji").and_then(Value::as_str).unwrap_or("");
            let ok = office.chat.react(id, "me", emoji);
            let _ = office.save_chat();
            Ok(Json(json!({ "ok": ok })))
        }
        "vote" => {
            let id = body.get("id").and_then(Value::as_str).unwrap_or("");
            let idx = body.get("i").and_then(Value::as_u64).unwrap_or(0) as usize;
            let ok = office.chat.vote(id, "me", idx);
            let _ = office.save_chat();
            Ok(Json(json!({ "ok": ok })))
        }
        "recall" => {
            let id = body.get("id").and_then(Value::as_str).unwrap_or("");
            let ok = office.chat.recall(id);
            let _ = office.save_chat();
            Ok(Json(json!({ "ok": ok })))
        }
        "pin" => {
            let id = body.get("id").and_then(Value::as_str).unwrap_or("");
            let ok = office.chat.pin(id);
            let _ = office.save_chat();
            Ok(Json(json!({ "ok": ok })))
        }
        _ => {
            let raw_text = body.get("text").and_then(Value::as_str).unwrap_or("");
            let text = crate::redact::redact(raw_text);
            let msg = office.chat.say("me", "Boss", &text, ts, false, None);
            let _ = office.save_chat();
            Ok(Json(json!({ "ok": true, "msg": msg })))
        }
    }
}

pub async fn get_loop(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    OptionalPeer(peer): OptionalPeer,
    ViaTunnel(via_tunnel): ViaTunnel,
    Query(q): Query<HashMap<String, String>>,
) -> Result<Json<Value>, StatusCode> {
    let _ = require_auth(&state, peer, via_tunnel, &headers, &q)?;
    let cfg = state.office.lock().unwrap_or_else(|e| e.into_inner()).loops.config();
    Ok(Json(json!({ "ok": true, "config": cfg })))
}

pub async fn post_loop(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    OptionalPeer(peer): OptionalPeer,
    ViaTunnel(via_tunnel): ViaTunnel,
    Json(body): Json<Value>,
) -> Result<Json<Value>, StatusCode> {
    require_local_post(peer, via_tunnel, &headers)?;
    let mut office = state.office.lock().unwrap_or_else(|e| e.into_inner());
    if let Some(sid) = body.get("stop").and_then(Value::as_str) {
        let ok = office.loops.stop(sid, now_ms());
        return Ok(Json(json!({ "ok": ok })));
    }
    if let (Some(sid), Some(hint)) = (
        body.get("sid").and_then(Value::as_str),
        body.get("hint").and_then(Value::as_str),
    ) {
        let ok = office.loops.hint(sid, hint);
        return Ok(Json(json!({ "ok": ok })));
    }
    let cur = office.loops.config();
    let next = LoopConfig {
        on: body.get("on").and_then(Value::as_bool).unwrap_or(cur.on),
        n: body.get("n").and_then(Value::as_u64).map(|v| v as u32).unwrap_or(cur.n),
        min: body.get("min").and_then(Value::as_u64).map(|v| v as u32).unwrap_or(cur.min),
    };
    office.loops.set_config(next);
    Ok(Json(json!({ "ok": true, "config": office.loops.config() })))
}

pub async fn get_tenure(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    OptionalPeer(peer): OptionalPeer,
    ViaTunnel(via_tunnel): ViaTunnel,
    Query(q): Query<HashMap<String, String>>,
) -> Result<Json<Value>, StatusCode> {
    let _ = require_auth(&state, peer, via_tunnel, &headers, &q)?;
    let live: HashSet<String> = state
        .office
        .lock()
        .unwrap_or_else(|e| e.into_inner())
        .sessions
        .values()
        .map(|s| s.name.clone())
        .collect();
    let list = state.tenure.lock().unwrap_or_else(|e| e.into_inner()).list(30, now_ms(), &live);
    Ok(Json(json!({ "ok": true, "items": list })))
}

pub async fn post_tenure(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    OptionalPeer(peer): OptionalPeer,
    ViaTunnel(via_tunnel): ViaTunnel,
    Json(body): Json<Value>,
) -> Result<Json<Value>, StatusCode> {
    require_local_post(peer, via_tunnel, &headers)?;
    let name = body.get("name").and_then(Value::as_str).unwrap_or("");
    let op = body.get("op").and_then(Value::as_str).unwrap_or("");
    let mut tenure = state.tenure.lock().unwrap_or_else(|e| e.into_inner());
    let ok = match op {
        "official" => tenure.mark_official(name, now_ms()),
        "farewell" => tenure.mark_farewell(name, 30, now_ms(), false),
        _ => false,
    };
    Ok(Json(json!({ "ok": ok })))
}

pub async fn get_tasks(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    OptionalPeer(peer): OptionalPeer,
    ViaTunnel(via_tunnel): ViaTunnel,
    Query(q): Query<HashMap<String, String>>,
) -> Result<Json<Value>, StatusCode> {
    let _ = require_auth(&state, peer, via_tunnel, &headers, &q)?;
    let tasks = state.tasks.lock().unwrap_or_else(|e| e.into_inner());
    Ok(Json(json!({ "ok": true, "tasks": tasks.list() })))
}

pub async fn post_tasks(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    OptionalPeer(peer): OptionalPeer,
    ViaTunnel(via_tunnel): ViaTunnel,
    Json(body): Json<Value>,
) -> Result<Json<Value>, StatusCode> {
    require_local_post(peer, via_tunnel, &headers)?;
    let req = crate::state::tasks::TaskStartRequest {
        dir: body.get("dir").and_then(Value::as_str).unwrap_or("").to_owned(),
        text: body.get("text").and_then(Value::as_str).unwrap_or("").to_owned(),
        agent: body.get("agent").and_then(Value::as_str).unwrap_or("claude").to_owned(),
        model: body.get("model").and_then(Value::as_str).unwrap_or("").to_owned(),
        fork: body.get("fork").and_then(Value::as_bool).unwrap_or(false),
    };
    let mut tasks = state.tasks.lock().unwrap_or_else(|e| e.into_inner());
    Ok(Json(tasks.validate_and_register(req, now_ms())))
}

pub async fn post_tasks_stop(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    OptionalPeer(peer): OptionalPeer,
    ViaTunnel(via_tunnel): ViaTunnel,
    Json(body): Json<Value>,
) -> Result<Json<Value>, StatusCode> {
    require_local_post(peer, via_tunnel, &headers)?;
    let id = body.get("id").and_then(Value::as_str).unwrap_or("");
    let ok = state.tasks.lock().unwrap_or_else(|e| e.into_inner()).finish(id).is_some();
    Ok(Json(json!({ "ok": ok })))
}

pub async fn get_models(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    OptionalPeer(peer): OptionalPeer,
    ViaTunnel(via_tunnel): ViaTunnel,
    Query(q): Query<HashMap<String, String>>,
) -> Result<Json<Value>, StatusCode> {
    let _ = require_auth(&state, peer, via_tunnel, &headers, &q)?;
    let settings = state
        .prefs
        .lock()
        .unwrap_or_else(|e| e.into_inner())
        .get("claudeModels")
        .cloned()
        .unwrap_or_else(|| json!({}));
    Ok(Json(json!({
        "ok": true,
        "models": default_claude_models(),
        "config": models_of(&settings),
    })))
}

pub async fn post_models(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    OptionalPeer(peer): OptionalPeer,
    ViaTunnel(via_tunnel): ViaTunnel,
    Json(body): Json<Value>,
) -> Result<Json<Value>, StatusCode> {
    require_local_post(peer, via_tunnel, &headers)?;
    let mut prefs = state.prefs.lock().unwrap_or_else(|e| e.into_inner());
    let cur = prefs.get("claudeModels").cloned().unwrap_or_else(|| json!({}));
    let next = with_models(
        &cur,
        body.get("main").and_then(Value::as_str),
        body.get("sub").and_then(Value::as_str),
        body.get("force").and_then(Value::as_bool),
    );
    let _ = prefs.set("claudeModels", Some(next.clone()));
    Ok(Json(json!({ "ok": true, "config": models_of(&next) })))
}

pub async fn get_prefs(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    OptionalPeer(peer): OptionalPeer,
    ViaTunnel(via_tunnel): ViaTunnel,
    Query(q): Query<HashMap<String, String>>,
) -> Result<Json<Value>, StatusCode> {
    let _ = require_auth(&state, peer, via_tunnel, &headers, &q)?;
    let prefs = state.prefs.lock().unwrap_or_else(|e| e.into_inner());
    Ok(Json(json!({
        "ok": true,
        "lang": prefs.lang(),
        "autoCloseHours": prefs.auto_close(),
        "prefs": prefs.snapshot(),
    })))
}

pub async fn post_prefs(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    OptionalPeer(peer): OptionalPeer,
    ViaTunnel(via_tunnel): ViaTunnel,
    Json(body): Json<Value>,
) -> Result<Json<Value>, StatusCode> {
    require_local_post(peer, via_tunnel, &headers)?;
    let mut prefs = state.prefs.lock().unwrap_or_else(|e| e.into_inner());
    if let Some(l) = body.get("lang").and_then(Value::as_str) {
        let _ = prefs.set_lang(l);
    }
    if let Some(h) = body.get("autoCloseHours").and_then(Value::as_u64) {
        let _ = prefs.set_auto_close(h as u8);
    }
    if let Some(obj) = body.as_object() {
        for (k, v) in obj {
            if k != "lang" && k != "autoCloseHours" {
                let _ = prefs.set(k, Some(v.clone()));
            }
        }
    }
    Ok(Json(json!({ "ok": true, "lang": prefs.lang(), "prefs": prefs.snapshot() })))
}

pub async fn post_pin_update(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    OptionalPeer(peer): OptionalPeer,
    ViaTunnel(via_tunnel): ViaTunnel,
    Json(body): Json<Value>,
) -> Result<Json<Value>, StatusCode> {
    require_local_post(peer, via_tunnel, &headers)?;
    let pin = body.get("pin").and_then(Value::as_str).unwrap_or("");
    let valid = !crate::auth::pin_weak(pin);
    let ip = if via_tunnel {
        IpAddr::V4(Ipv4Addr::UNSPECIFIED)
    } else {
        peer.map(|a| a.ip())
            .unwrap_or(IpAddr::V4(Ipv4Addr::LOCALHOST))
    };
    {
        let mut limiter = state
            .pin_limiter
            .lock()
            .unwrap_or_else(|e| e.into_inner());
        limiter.check_and_record(ip, now_ms(), valid)?;
    }
    let ok = if valid {
        state.auth.set_pin(pin).unwrap_or(false)
    } else {
        false
    };
    Ok(Json(json!({ "ok": ok, "pin": state.auth.current_pin() })))
}

pub async fn post_presence(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    OptionalPeer(peer): OptionalPeer,
    ViaTunnel(via_tunnel): ViaTunnel,
    Query(q): Query<HashMap<String, String>>,
    Json(body): Json<Value>,
) -> Result<Json<Value>, StatusCode> {
    enforce_same_origin_post_with_transport(
        &headers,
        state.allowed_tunnel_host().as_deref(),
        via_tunnel,
    )?;
    let _ = require_auth(&state, peer, via_tunnel, &headers, &q)?;
    let focused = body.get("focused").and_then(Value::as_bool).unwrap_or(false);
    state.office.lock().unwrap_or_else(|e| e.into_inner()).presence_focused = focused;
    Ok(Json(json!({ "ok": true, "focused": focused })))
}

pub async fn post_perm(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    OptionalPeer(peer): OptionalPeer,
    ViaTunnel(via_tunnel): ViaTunnel,
    Query(q): Query<HashMap<String, String>>,
    Json(body): Json<Value>,
) -> Result<Json<Value>, StatusCode> {
    enforce_same_origin_post_with_transport(
        &headers,
        state.allowed_tunnel_host().as_deref(),
        via_tunnel,
    )?;
    let local = require_auth(&state, peer, via_tunnel, &headers, &q)?;
    if !local {
        let phone_approve = state
            .prefs
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .get("phoneApprove")
            .and_then(Value::as_bool)
            .unwrap_or(false);
        if !phone_approve {
            return Err(StatusCode::FORBIDDEN);
        }
    }
    let id = body.get("id").and_then(Value::as_str).unwrap_or("");
    let mut office = state.office.lock().unwrap_or_else(|e| e.into_inner());
    if !local
        && office
            .pending_perms
            .get(id)
            .is_some_and(|p| p.danger.is_some())
    {
        return Err(StatusCode::FORBIDDEN);
    }
    let removed = office.pending_perms.remove(id).is_some();
    Ok(Json(json!({ "ok": removed })))
}

pub async fn get_push(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    OptionalPeer(peer): OptionalPeer,
    ViaTunnel(via_tunnel): ViaTunnel,
    Query(q): Query<HashMap<String, String>>,
) -> Result<Json<Value>, StatusCode> {
    let _ = require_auth(&state, peer, via_tunnel, &headers, &q)?;
    let push = state.push.lock().unwrap_or_else(|e| e.into_inner());
    Ok(Json(json!({ "ok": true, "publicKey": push.public_key, "count": push.count() })))
}

pub async fn post_push(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    OptionalPeer(peer): OptionalPeer,
    ViaTunnel(via_tunnel): ViaTunnel,
    Query(q): Query<HashMap<String, String>>,
    Json(body): Json<Value>,
) -> Result<Json<Value>, StatusCode> {
    enforce_same_origin_post_with_transport(
        &headers,
        state.allowed_tunnel_host().as_deref(),
        via_tunnel,
    )?;
    let _ = require_auth(&state, peer, via_tunnel, &headers, &q)?;
    let mut push = state.push.lock().unwrap_or_else(|e| e.into_inner());
    if let Some(ep) = body.get("remove").and_then(Value::as_str) {
        let ok = push.remove(ep);
        return Ok(Json(json!({ "ok": ok, "count": push.count() })));
    }
    let endpoint = body.get("endpoint").and_then(Value::as_str).unwrap_or("").to_owned();
    let p256dh = body
        .get("keys")
        .and_then(|k| k.get("p256dh"))
        .and_then(Value::as_str)
        .unwrap_or("")
        .to_owned();
    let auth = body
        .get("keys")
        .and_then(|k| k.get("auth"))
        .and_then(Value::as_str)
        .unwrap_or("")
        .to_owned();
    let ok = push.add(
        PushSubscription {
            endpoint,
            keys: PushKeys { p256dh, auth },
            ua: headers
                .get("user-agent")
                .and_then(|v| v.to_str().ok())
                .unwrap_or("")
                .to_owned(),
            at: 0,
        },
        now_ms(),
    );
    Ok(Json(json!({ "ok": ok, "count": push.count() })))
}

pub async fn post_prompt_rewrite(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    OptionalPeer(peer): OptionalPeer,
    ViaTunnel(via_tunnel): ViaTunnel,
    Json(body): Json<Value>,
) -> Result<Json<Value>, StatusCode> {
    require_local_post(peer, via_tunnel, &headers)?;
    let text = body.get("text").and_then(Value::as_str).unwrap_or("");
    let lang = body.get("lang").and_then(Value::as_str).unwrap_or("en");
    let photos = body.get("photos").and_then(Value::as_u64).unwrap_or(0) as usize;
    let mut rw = state.rewriter.lock().unwrap_or_else(|e| e.into_inner());
    let res = rw.rewrite_with(text, lang, photos, now_ms(), |redacted, _sys, _ph| {
        Ok(format!("Goal: {redacted}"))
    });
    Ok(Json(json!(res)))
}

pub async fn get_image(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    OptionalPeer(peer): OptionalPeer,
    ViaTunnel(via_tunnel): ViaTunnel,
    Query(q): Query<HashMap<String, String>>,
) -> Response {
    if require_auth(&state, peer, via_tunnel, &headers, &q).is_err() {
        return StatusCode::UNAUTHORIZED.into_response();
    }
    (
        StatusCode::NOT_FOUND,
        [
            (header::CONTENT_SECURITY_POLICY, "sandbox"),
            (header::X_CONTENT_TYPE_OPTIONS, "nosniff"),
        ],
        "not found",
    )
        .into_response()
}

pub async fn get_claude_version(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    OptionalPeer(peer): OptionalPeer,
    ViaTunnel(via_tunnel): ViaTunnel,
    Query(q): Query<HashMap<String, String>>,
) -> Result<Json<Value>, StatusCode> {
    let _ = require_auth(&state, peer, via_tunnel, &headers, &q)?;
    let v = state.claude_version.lock().unwrap_or_else(|e| e.into_inner()).current().clone();
    Ok(Json(json!({ "ok": true, "version": v })))
}

pub async fn post_todos(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    OptionalPeer(peer): OptionalPeer,
    ViaTunnel(via_tunnel): ViaTunnel,
    Json(body): Json<Value>,
) -> Result<Json<Value>, StatusCode> {
    require_local_post(peer, via_tunnel, &headers)?;
    if let Some(arr) = body.get("todos").and_then(Value::as_array) {
        state.office.lock().unwrap_or_else(|e| e.into_inner()).todos = arr.clone();
    }
    Ok(Json(json!({ "ok": true })))
}

pub async fn post_codex_ai(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    OptionalPeer(peer): OptionalPeer,
    ViaTunnel(via_tunnel): ViaTunnel,
    AxumPath(action): AxumPath<String>,
    Json(body): Json<Value>,
) -> Result<Json<Value>, StatusCode> {
    require_local_post(peer, via_tunnel, &headers)?;
    let res = state.codex_accounts.lock().unwrap_or_else(|e| e.into_inner()).mutate(&action, &body);
    Ok(Json(res))
}

pub async fn post_ag_ai(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    OptionalPeer(peer): OptionalPeer,
    ViaTunnel(via_tunnel): ViaTunnel,
    AxumPath(action): AxumPath<String>,
    Json(body): Json<Value>,
) -> Result<Json<Value>, StatusCode> {
    require_local_post(peer, via_tunnel, &headers)?;
    let res = state.ag_accounts.lock().unwrap_or_else(|e| e.into_inner()).mutate(&action, &body);
    Ok(Json(res))
}

pub async fn post_accounts_any(
    headers: HeaderMap,
    OptionalPeer(peer): OptionalPeer,
    ViaTunnel(via_tunnel): ViaTunnel,
    Json(_body): Json<Value>,
) -> Result<Json<Value>, StatusCode> {
    require_local_post(peer, via_tunnel, &headers)?;
    Ok(Json(json!({ "ok": true })))
}

pub async fn get_generic_ok(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    OptionalPeer(peer): OptionalPeer,
    ViaTunnel(via_tunnel): ViaTunnel,
    Query(q): Query<HashMap<String, String>>,
) -> Result<Json<Value>, StatusCode> {
    let _ = require_auth(&state, peer, via_tunnel, &headers, &q)?;
    Ok(Json(json!({ "ok": true })))
}

pub async fn post_local_ok(
    headers: HeaderMap,
    OptionalPeer(peer): OptionalPeer,
    ViaTunnel(via_tunnel): ViaTunnel,
    Json(_body): Json<Value>,
) -> Result<Json<Value>, StatusCode> {
    require_local_post(peer, via_tunnel, &headers)?;
    Ok(Json(json!({ "ok": true })))
}

pub async fn get_harnesses(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    OptionalPeer(peer): OptionalPeer,
    ViaTunnel(via_tunnel): ViaTunnel,
) -> Result<Json<Value>, StatusCode> {
    enforce_loopback_strict(peer, via_tunnel, &headers)?;
    if let Some(api) = &state.harnesses {
        Ok(Json(api.list_json()))
    } else {
        Ok(Json(json!({ "ok": true, "harnesses": [] })))
    }
}

pub async fn post_harnesses_launch(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    OptionalPeer(peer): OptionalPeer,
    ViaTunnel(via_tunnel): ViaTunnel,
    Json(body): Json<Value>,
) -> Result<Json<Value>, StatusCode> {
    enforce_loopback_strict(peer, via_tunnel, &headers)?;
    let id = body.get("id").and_then(Value::as_str).unwrap_or("");
    if let Some(api) = &state.harnesses {
        match api.launch(id) {
            Ok(v) => Ok(Json(v)),
            Err(e) => Ok(Json(json!({ "ok": false, "error": e.to_string() }))),
        }
    } else {
        Ok(Json(json!({ "ok": false, "error": "launcher unavailable" })))
    }
}

pub async fn post_harnesses_custom(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    OptionalPeer(peer): OptionalPeer,
    ViaTunnel(via_tunnel): ViaTunnel,
    Json(body): Json<Value>,
) -> Result<Json<Value>, StatusCode> {
    enforce_loopback_strict(peer, via_tunnel, &headers)?;
    if let Some(api) = &state.harnesses {
        match api.mutate_custom(body) {
            Ok(v) => Ok(Json(v)),
            Err(e) => Ok(Json(json!({ "ok": false, "error": e.to_string() }))),
        }
    } else {
        Ok(Json(json!({ "ok": false, "error": "launcher unavailable" })))
    }
}

#[cfg(test)]
mod tests {
    use super::is_valid_day_key;
    use crate::http::{AppState, build_router};
    use axum::body::Body;
    use axum::http::{Request, StatusCode};
    use std::sync::Arc;
    use tower::ServiceExt;

    #[test]
    fn is_valid_day_key_matches_strict_yyyy_mm_dd() {
        assert!(is_valid_day_key("2026-11-25"));
        assert!(!is_valid_day_key(""));
        assert!(!is_valid_day_key("../../secret"));
        assert!(!is_valid_day_key("2026-1-5"));
        assert!(!is_valid_day_key("2026-11-25a"));
        assert!(!is_valid_day_key("2026/11/25"));
    }

    #[tokio::test]
    async fn replay_rejects_path_traversal_and_invalid_day_keys() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(dir.path().join("secret.jsonl"), "{\"leaked\":true}\n").unwrap();
        let replay_dir = dir.path().join("replay");
        std::fs::create_dir_all(&replay_dir).unwrap();
        std::fs::write(replay_dir.join("2026-11-25.jsonl"), "{\"ok\":true}\n").unwrap();

        let state = Arc::new(AppState::new(dir.path().to_path_buf(), None).unwrap());
        let app = build_router(state);

        for bad_uri in [
            "/api/replay?day=../../secret",
            "/api/replay?day=../secret",
            "/api/replay?day=2026-11-25/../../secret",
            "/api/replay?day=2026-1-5",
            "/api/replay?day=not-a-date",
            "/api/replay",
        ] {
            let req = Request::builder()
                .uri(bad_uri)
                .header("host", "127.0.0.1:4310")
                .body(Body::empty())
                .unwrap();
            let resp = app.clone().oneshot(req).await.unwrap();
            assert_eq!(
                resp.status(),
                StatusCode::BAD_REQUEST,
                "expected 400 for {bad_uri}"
            );
        }

        let good_req = Request::builder()
            .uri("/api/replay?day=2026-11-25")
            .header("host", "127.0.0.1:4310")
            .body(Body::empty())
            .unwrap();
        let resp = app.oneshot(good_req).await.unwrap();
        assert_eq!(resp.status(), StatusCode::OK);
    }

    #[tokio::test]
    async fn post_perm_remote_requires_phone_approve_and_blocks_dangerous_items() {
        use crate::auth::COOKIE_NAME;
        use crate::http::guard::ViaTunnel;
        use crate::state::sessions::PendingPerm;
        use axum::extract::ConnectInfo;
        use serde_json::{Value, json};
        use std::net::{IpAddr, Ipv4Addr, SocketAddr};

        let dir = tempfile::tempdir().unwrap();
        let state = Arc::new(AppState::new(dir.path().to_path_buf(), None).unwrap());
        let cookie = format!("{COOKIE_NAME}={}", state.auth.sign_cookie(false));

        let mk_perm = |id: &str, danger: Option<&str>| PendingPerm {
            id: id.to_owned(),
            sess: "s1".to_owned(),
            name: "proj".to_owned(),
            kind: "perm".to_owned(),
            tool_type: "bash".to_owned(),
            label: "cmd".to_owned(),
            detail: "detail".to_owned(),
            danger: danger.map(str::to_owned),
            created_at: 1,
        };
        {
            let mut office = state.office.lock().unwrap();
            office.pending_perms.insert("safe1".into(), mk_perm("safe1", None));
            office.pending_perms.insert("danger1".into(), mk_perm("danger1", Some("rm_rf")));
        }

        let app = build_router(state.clone());
        let lan_peer = SocketAddr::new(IpAddr::V4(Ipv4Addr::new(192, 168, 1, 50)), 52000);
        let loopback_peer = SocketAddr::new(IpAddr::V4(Ipv4Addr::LOCALHOST), 52001);

        // 1. Non-loopback with phoneApprove unset -> 403 and perm preserved
        let mut req = Request::builder()
            .method("POST")
            .uri("/api/perm")
            .header("host", "192.168.1.20:4310")
            .header("cookie", &cookie)
            .header("content-type", "application/json")
            .body(Body::from(json!({"id": "safe1"}).to_string()))
            .unwrap();
        req.extensions_mut().insert(ConnectInfo(lan_peer));
        let resp = app.clone().oneshot(req).await.unwrap();
        assert_eq!(resp.status(), StatusCode::FORBIDDEN);
        assert!(state.office.lock().unwrap().pending_perms.contains_key("safe1"));

        // 2. ViaTunnel(true) even from loopback IP with phoneApprove unset -> 403
        let mut tunnel_req = Request::builder()
            .method("POST")
            .uri("/api/perm")
            .header("host", "127.0.0.1:4310")
            .header("cookie", &cookie)
            .header("content-type", "application/json")
            .body(Body::from(json!({"id": "safe1"}).to_string()))
            .unwrap();
        tunnel_req.extensions_mut().insert(ConnectInfo(loopback_peer));
        tunnel_req.extensions_mut().insert(ViaTunnel(true));
        let resp = app.clone().oneshot(tunnel_req).await.unwrap();
        assert_eq!(resp.status(), StatusCode::FORBIDDEN);
        assert!(state.office.lock().unwrap().pending_perms.contains_key("safe1"));

        // Enable phoneApprove = true
        state
            .prefs
            .lock()
            .unwrap()
            .set("phoneApprove", Some(Value::Bool(true)))
            .unwrap();

        // 3. Non-loopback with phoneApprove=true STILL rejects danger.is_some() -> 403
        let mut danger_req = Request::builder()
            .method("POST")
            .uri("/api/perm")
            .header("host", "192.168.1.20:4310")
            .header("cookie", &cookie)
            .header("content-type", "application/json")
            .body(Body::from(json!({"id": "danger1"}).to_string()))
            .unwrap();
        danger_req.extensions_mut().insert(ConnectInfo(lan_peer));
        let resp = app.clone().oneshot(danger_req).await.unwrap();
        assert_eq!(resp.status(), StatusCode::FORBIDDEN);
        assert!(state.office.lock().unwrap().pending_perms.contains_key("danger1"));

        // 4. ViaTunnel(true) with phoneApprove=true STILL rejects danger.is_some() -> 403
        let mut danger_tunnel = Request::builder()
            .method("POST")
            .uri("/api/perm")
            .header("host", "127.0.0.1:4310")
            .header("cookie", &cookie)
            .header("content-type", "application/json")
            .body(Body::from(json!({"id": "danger1"}).to_string()))
            .unwrap();
        danger_tunnel.extensions_mut().insert(ConnectInfo(loopback_peer));
        danger_tunnel.extensions_mut().insert(ViaTunnel(true));
        let resp = app.clone().oneshot(danger_tunnel).await.unwrap();
        assert_eq!(resp.status(), StatusCode::FORBIDDEN);
        assert!(state.office.lock().unwrap().pending_perms.contains_key("danger1"));

        // 5. Non-loopback with phoneApprove=true allows safe perm -> 200 and removes it
        let mut ok_remote = Request::builder()
            .method("POST")
            .uri("/api/perm")
            .header("host", "192.168.1.20:4310")
            .header("cookie", &cookie)
            .header("content-type", "application/json")
            .body(Body::from(json!({"id": "safe1"}).to_string()))
            .unwrap();
        ok_remote.extensions_mut().insert(ConnectInfo(lan_peer));
        let resp = app.clone().oneshot(ok_remote).await.unwrap();
        assert_eq!(resp.status(), StatusCode::OK);
        assert!(!state.office.lock().unwrap().pending_perms.contains_key("safe1"));

        // 6. Loopback (non-tunnel) allows dangerous perm even when phoneApprove=false
        state
            .prefs
            .lock()
            .unwrap()
            .set("phoneApprove", Some(Value::Bool(false)))
            .unwrap();
        let mut ok_local = Request::builder()
            .method("POST")
            .uri("/api/perm")
            .header("host", "127.0.0.1:4310")
            .header("content-type", "application/json")
            .body(Body::from(json!({"id": "danger1"}).to_string()))
            .unwrap();
        ok_local.extensions_mut().insert(ConnectInfo(loopback_peer));
        let resp = app.oneshot(ok_local).await.unwrap();
        assert_eq!(resp.status(), StatusCode::OK);
        assert!(!state.office.lock().unwrap().pending_perms.contains_key("danger1"));
    }

    #[tokio::test]
    async fn post_chat_redacts_secrets_before_storing_in_chat_json() {
        use serde_json::json;

        let dir = tempfile::tempdir().unwrap();
        let state = Arc::new(AppState::new(dir.path().to_path_buf(), None).unwrap());
        let app = build_router(state.clone());

        let payload = json!({
            "op": "send",
            "text": "keys: sk-ant-api03-secret1234 ghp_1234567890abcdef AKIAIOSFODNN7EXAMPLE"
        });
        let req = Request::builder()
            .method("POST")
            .uri("/api/chat")
            .header("host", "127.0.0.1:4310")
            .header("content-type", "application/json")
            .body(Body::from(payload.to_string()))
            .unwrap();
        let resp = app.oneshot(req).await.unwrap();
        assert_eq!(resp.status(), StatusCode::OK);

        let stored_text = state
            .office
            .lock()
            .unwrap()
            .chat
            .msgs
            .last()
            .unwrap()
            .text
            .clone();
        let on_disk = std::fs::read_to_string(dir.path().join("chat.json")).unwrap();
        for secret in [
            "sk-ant-api03-secret1234",
            "ghp_1234567890abcdef",
            "AKIAIOSFODNN7EXAMPLE",
        ] {
            assert!(!stored_text.contains(secret), "in-memory chat leaked {secret}");
            assert!(!on_disk.contains(secret), "chat.json leaked {secret}");
        }
    }
}
