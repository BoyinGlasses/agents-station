use crate::http::AppState;
use crate::http::guard::{
    OptionalPeer, ViaTunnel, enforce_loopback_strict, enforce_same_origin_post_with_transport,
    is_authorized, is_loopback_peer,
};
use crate::shared::timeline::build_timeline;
use crate::state::loops::LoopConfig;
use crate::state::models::{
    DEFAULT_SPLIT_NOTE, MAIN_MODELS, SUB_MODELS, default_claude_models, has_split, models_of,
    with_models, with_split,
};
use crate::state::sessions::{PermDecision, now_ms, one_line, read_replay_range};
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
    let today_date = chrono::Local::now().date_naive();
    let today_str = today_date.format("%Y-%m-%d").to_string();
    let prev_str = today_date
        .pred_opt()
        .map(|d| d.format("%Y-%m-%d").to_string())
        .unwrap_or_default();
    let people: Vec<crate::shared::duty::DutyPerson> = office
        .sessions
        .values()
        .map(|s| crate::shared::duty::DutyPerson {
            key: s.id.clone(),
            name: s.name.clone(),
        })
        .collect();
    let duty_view = {
        let mut d = state.duty.lock().unwrap_or_else(|e| e.into_inner());
        let rolled = crate::shared::duty::roll(&d, &today_str, &prev_str);
        *d = crate::shared::duty::assign_exact(&rolled, &people);
        d.clone()
    };
    let lunches: Vec<Value> = office
        .sessions
        .values()
        .filter(|s| crate::shared::comnha::brings_lunch(&s.id, today_date, false))
        .map(|s| {
            let b = crate::shared::comnha::lunch_of(&s.id, today_date);
            json!({
                "id": s.id,
                "box": b,
                "warmAt": crate::shared::comnha::warm_at(&s.id, today_date),
            })
        })
        .collect();
    if let Some(obj) = snap.as_object_mut() {
        obj.insert(
            "trophies".to_owned(),
            state.trophies.lock().unwrap_or_else(|e| e.into_inner()).pub_view(),
        );
        obj.insert("lift".to_owned(), json!(state.lift.pub_view(now_ms())));
        obj.insert("duty".to_owned(), json!(duty_view));
        obj.insert("lunch".to_owned(), Value::Array(lunches));
        if local {
            obj.insert("pin".to_owned(), Value::String(state.auth.current_pin()));
        }
    }
    Ok(Json(snap))
}

pub async fn get_git(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    OptionalPeer(peer): OptionalPeer,
    ViaTunnel(via_tunnel): ViaTunnel,
    Query(q): Query<HashMap<String, String>>,
) -> Result<Json<Value>, StatusCode> {
    let _ = require_auth(&state, peer, via_tunnel, &headers, &q)?;
    let repos = state
        .git
        .lock()
        .unwrap_or_else(|e| e.into_inner())
        .all_repos();
    Ok(Json(json!({ "ok": true, "repos": repos })))
}

pub async fn get_doctor(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    OptionalPeer(peer): OptionalPeer,
    ViaTunnel(via_tunnel): ViaTunnel,
    Query(q): Query<HashMap<String, String>>,
) -> Result<Json<Value>, StatusCode> {
    let _ = require_auth(&state, peer, via_tunnel, &headers, &q)?;
    let ag_watcher = Arc::clone(&state.ag_watcher);
    let agent_hist = Arc::clone(&state.agent_history);
    let claude_projects = state.claude_dir.join("projects");
    let (vsix_count, ag_changed, history_count) = tokio::task::spawn_blocking(move || {
        let vsix = crate::store::vsix::find_installed_claude_vsix(&crate::config::home_dir()).len();
        let ag = ag_watcher
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .scan_changed()
            .len();
        let hist = {
            let mut h = agent_hist.lock().unwrap_or_else(|e| e.into_inner());
            let _ = h.scan_jsonl_dir("claude", &claude_projects);
            h.list(None, 50).len()
        };
        (vsix, ag, hist)
    })
    .await
    .unwrap_or((0, 0, 0));
    Ok(Json(json!({
        "ok": true,
        "platform": state.sys.platform().name(),
        "dataDir": state.data_dir,
        "otel": state.otel.lock().unwrap_or_else(|e| e.into_inner()).info(),
        "sessions": state.office.lock().unwrap_or_else(|e| e.into_inner()).sessions.len(),
        "vsix": vsix_count,
        "antigravityRuns": ag_changed,
        "agentHistory": history_count,
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
    if let Some(sid) = q.get("id").filter(|s| !s.is_empty()) {
        let tp = {
            let office = state.office.lock().unwrap_or_else(|e| e.into_inner());
            office
                .sessions
                .get(sid)
                .and_then(|s| s.transcript_path.clone())
        };
        if let Some(path) = tp {
            let items = tokio::task::spawn_blocking(move || {
                crate::store::codex_history::read_codex_history(&path, 120)
            })
            .await
            .unwrap_or_default();
            return Ok(Json(json!({ "ok": true, "items": items })));
        }
    }
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
    let items = read_replay_range(&state.data_dir, from, to);
    let tl = build_timeline(&items, from, to, now);
    let mut val = json!(tl);
    if let Some(obj) = val.as_object_mut() {
        obj.insert("truncated".to_owned(), Value::Bool(false));
    }
    Ok(Json(val))
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
    let from = q.get("from").and_then(|s| s.parse::<u64>().ok()).unwrap_or(0);
    let to = q
        .get("to")
        .and_then(|s| s.parse::<u64>().ok())
        .unwrap_or(u64::MAX);
    let file = state.data_dir.join("replay").join(format!("{day}.jsonl"));
    let items: Vec<Value> = std::fs::read_to_string(file)
        .ok()
        .map(|t| {
            t.lines()
                .filter_map(|l| serde_json::from_str::<Value>(l).ok())
                .filter(|v| {
                    let ts = v.get("t").and_then(Value::as_u64).unwrap_or(0);
                    ts >= from && ts <= to
                })
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
    let today = crate::store::otel::day_key(now_ms());
    Ok(Json(json!({ "ok": true, "days": days, "today": today })))
}

fn collect_stranded_dirs(state: &AppState) -> (Vec<String>, Vec<String>) {
    let mut dirs = Vec::new();
    let mut in_use = Vec::new();
    {
        let office = state.office.lock().unwrap_or_else(|e| e.into_inner());
        dirs.extend(office.known_dirs.iter().cloned());
        for s in office.sessions.values() {
            if !s.cwd.is_empty() {
                dirs.push(s.cwd.clone());
                in_use.push(s.cwd.clone());
            }
        }
    }
    {
        let tasks = state.tasks.lock().unwrap_or_else(|e| e.into_inner());
        for t in tasks.list() {
            if !t.dir.is_empty() {
                dirs.push(t.dir.clone());
                in_use.push(t.dir.clone());
            }
        }
    }
    (dirs, in_use)
}

pub async fn get_stranded(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    OptionalPeer(peer): OptionalPeer,
    ViaTunnel(via_tunnel): ViaTunnel,
    Query(q): Query<HashMap<String, String>>,
) -> Result<Json<Value>, StatusCode> {
    let local = require_auth(&state, peer, via_tunnel, &headers, &q)?;
    let force = q.contains_key("fresh");
    let (dirs, in_use) = collect_stranded_dirs(&state);
    let scanner = Arc::clone(&state.stranded);
    let now = now_ms();
    let snap = tokio::task::spawn_blocking(move || {
        scanner
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .scan(&dirs, &in_use, now, force)
    })
    .await
    .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    Ok(Json(json!({
        "ok": snap.ok,
        "at": snap.at,
        "repos": snap.repos,
        "items": snap.items,
        "local": local,
        "partial": false,
        "scanning": false,
        "sizing": false,
    })))
}

pub async fn post_stranded_open(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    OptionalPeer(peer): OptionalPeer,
    ViaTunnel(via_tunnel): ViaTunnel,
    Json(body): Json<Value>,
) -> Result<Json<Value>, StatusCode> {
    require_local_post(peer, via_tunnel, &headers)?;
    let id = body.get("id").and_then(Value::as_str).unwrap_or("");
    let entry = state
        .stranded
        .lock()
        .unwrap_or_else(|e| e.into_inner())
        .entry(id);
    let Some(entry) = entry else {
        return Ok(Json(json!({ "ok": false, "code": "gone" })));
    };
    let path = std::path::PathBuf::from(&entry.item.path);
    let res = tokio::task::spawn_blocking(move || crate::state::stranded::open_folder(&path))
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    match res {
        Ok(()) => Ok(Json(json!({ "ok": true, "path": entry.item.path }))),
        Err(msg) => Ok(Json(json!({ "ok": false, "message": msg }))),
    }
}

pub async fn post_stranded_remove(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    OptionalPeer(peer): OptionalPeer,
    ViaTunnel(via_tunnel): ViaTunnel,
    Json(body): Json<Value>,
) -> Result<Json<Value>, StatusCode> {
    require_local_post(peer, via_tunnel, &headers)?;
    let id = body.get("id").and_then(Value::as_str).unwrap_or("").to_owned();
    let (_, in_use) = collect_stranded_dirs(&state);
    let scanner = Arc::clone(&state.stranded);
    let res = tokio::task::spawn_blocking(move || {
        scanner
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .remove(&id, &in_use)
    })
    .await
    .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    Ok(Json(json!(res)))
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
    let sid = body
        .get("id")
        .or_else(|| body.get("sid"))
        .and_then(Value::as_str)
        .unwrap_or("");
    let mut office = state.office.lock().unwrap_or_else(|e| e.into_inner());
    let op = body.get("op").and_then(Value::as_str);
    if let Some(idx) = body.get("remove").and_then(Value::as_u64) {
        let ok = office.remove_pin(sid, idx as usize).unwrap_or(false);
        return Ok(Json(json!({ "ok": ok, "pins": office.pins })));
    }
    if op == Some("del") {
        let ok = if let Some(idx) = body.get("pin").and_then(Value::as_u64) {
            office.remove_pin(sid, idx as usize).unwrap_or(false)
        } else if let Some(pin_str) = body.get("pin").and_then(Value::as_str) {
            if let Ok(idx) = pin_str.parse::<usize>() {
                office.remove_pin(sid, idx).unwrap_or(false)
            } else if let Some(pos) = office
                .pins
                .get(sid)
                .and_then(|v| v.iter().position(|x| x == pin_str))
            {
                office.remove_pin(sid, pos).unwrap_or(false)
            } else {
                false
            }
        } else {
            false
        };
        return Ok(Json(json!({ "ok": ok, "pins": office.pins })));
    }
    let text = body.get("text").and_then(Value::as_str).unwrap_or("");
    let ok = office.add_pin(sid, text).unwrap_or(false);
    Ok(Json(json!({ "ok": ok, "pins": office.pins })))
}

async fn ensure_vault_scanned(state: &AppState, force: bool) -> Result<(), StatusCode> {
    let needs_scan = force
        || state
            .vault
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .scanned_at()
            == 0;
    if !needs_scan {
        return Ok(());
    }
    let vault = Arc::clone(&state.vault);
    let claude_dir = state.claude_dir.clone();
    tokio::task::spawn_blocking(move || {
        let mut g = vault.lock().unwrap_or_else(|e| e.into_inner());
        let _ = g.scan_claude_projects(&claude_dir);
    })
    .await
    .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    Ok(())
}

pub async fn get_vault(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    OptionalPeer(peer): OptionalPeer,
    ViaTunnel(via_tunnel): ViaTunnel,
    Query(q): Query<HashMap<String, String>>,
) -> Result<Json<Value>, StatusCode> {
    let local = require_auth(&state, peer, via_tunnel, &headers, &q)?;
    if let Some(sid) = q.get("sid").filter(|s| !s.is_empty()) {
        if !crate::store::vault::is_valid_sid(sid) {
            return Err(StatusCode::BAD_REQUEST);
        }
        let vault = Arc::clone(&state.vault);
        let sid_owned = sid.clone();
        let rec = tokio::task::spawn_blocking(move || {
            vault
                .lock()
                .unwrap_or_else(|e| e.into_inner())
                .get_transcript(&sid_owned)
        })
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
        return match rec {
            Some(r) => Ok(Json(json!({ "ok": true, "rec": r }))),
            None => Ok(Json(json!({ "ok": false }))),
        };
    }

    let force = local && q.contains_key("fresh");
    ensure_vault_scanned(&state, force).await?;

    let query: String = q
        .get("q")
        .map(String::as_str)
        .unwrap_or("")
        .chars()
        .take(200)
        .collect();
    let limit = q
        .get("limit")
        .and_then(|s| s.parse::<usize>().ok())
        .unwrap_or(60)
        .clamp(1, 200);
    let vault = state.vault.lock().unwrap_or_else(|e| e.into_inner());
    let stats = vault.stats();
    let items: Vec<Value> = vault
        .search(&query, limit)
        .into_iter()
        .map(|e| json!(e))
        .collect();
    Ok(Json(json!({ "ok": true, "stats": stats, "items": items })))
}

pub async fn get_vault_transcript(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    OptionalPeer(peer): OptionalPeer,
    ViaTunnel(via_tunnel): ViaTunnel,
    Query(q): Query<HashMap<String, String>>,
) -> Result<Json<Value>, StatusCode> {
    let _ = require_auth(&state, peer, via_tunnel, &headers, &q)?;
    let sid = q
        .get("sid")
        .or_else(|| q.get("id"))
        .map(String::as_str)
        .unwrap_or("");
    if !crate::store::vault::is_valid_sid(sid) {
        return Err(StatusCode::BAD_REQUEST);
    }
    ensure_vault_scanned(&state, false).await?;
    let vault = Arc::clone(&state.vault);
    let sid_owned = sid.to_owned();
    let rec = tokio::task::spawn_blocking(move || {
        vault
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .get_transcript(&sid_owned)
    })
    .await
    .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    match rec {
        Some(r) => Ok(Json(json!({ "ok": true, "rec": r }))),
        None => Ok(Json(json!({ "ok": false }))),
    }
}

pub async fn get_vault_report(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    OptionalPeer(peer): OptionalPeer,
    ViaTunnel(via_tunnel): ViaTunnel,
    Query(q): Query<HashMap<String, String>>,
) -> Result<Json<Value>, StatusCode> {
    let _ = require_auth(&state, peer, via_tunnel, &headers, &q)?;
    ensure_vault_scanned(&state, false).await?;
    let since = now_ms().saturating_sub(7 * 86_400_000);
    let prompts = state
        .vault
        .lock()
        .unwrap_or_else(|e| e.into_inner())
        .recent_prompts(since);
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
    let sid = q
        .get("id")
        .or_else(|| q.get("sid"))
        .map(String::as_str)
        .unwrap_or("");
    let office = state.office.lock().unwrap_or_else(|e| e.into_inner());
    Ok(Json(json!({ "ok": true, "plan": office.undo.plan(sid) })))
}

pub async fn post_undo(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    OptionalPeer(peer): OptionalPeer,
    ViaTunnel(via_tunnel): ViaTunnel,
    Query(q): Query<HashMap<String, String>>,
    Json(body): Json<Value>,
) -> Result<(StatusCode, Json<Value>), StatusCode> {
    enforce_same_origin_post_with_transport(
        &headers,
        state.allowed_tunnel_host().as_deref(),
        via_tunnel,
    )?;
    let local = require_auth(&state, peer, via_tunnel, &headers, &q)?;
    require_task_permission(&state, local)?;
    let sid = body
        .get("id")
        .or_else(|| body.get("sid"))
        .and_then(Value::as_str)
        .unwrap_or("");
    if sid.is_empty() {
        return Ok((StatusCode::NOT_FOUND, Json(json!({ "ok": false, "code": "gone" }))));
    }

    if body.get("op").and_then(Value::as_str) == Some("tell") {
        let files_str = match body.get("files") {
            Some(Value::String(s)) => s.chars().take(2000).collect::<String>(),
            Some(Value::Array(arr)) => arr
                .iter()
                .filter_map(Value::as_str)
                .collect::<Vec<_>>()
                .join(", ")
                .chars()
                .take(2000)
                .collect(),
            _ => String::new(),
        };
        let note = format!("Heads up: reverted files from your last turn: {files_str}");
        let (sess_busy, sess_cwd) = {
            let office = state.office.lock().unwrap_or_else(|e| e.into_inner());
            office
                .sessions
                .get(sid)
                .map(|s| (s.status != "rest", s.cwd.clone()))
                .unwrap_or((false, String::new()))
        };
        if sess_busy {
            let (st, val) = steer_inner(&state, &json!({ "sid": sid, "text": note })).await;
            return Ok((st, Json(val)));
        }
        let res = spawn_task_with_card(
            &state,
            crate::state::tasks::TaskStartRequest {
                dir: sess_cwd,
                text: note,
                agent: "claude".to_owned(),
                model: String::new(),
                fork: false,
            },
        );
        let st = if res.get("ok").and_then(Value::as_bool) == Some(true) {
            StatusCode::OK
        } else {
            StatusCode::CONFLICT
        };
        return Ok((st, Json(res)));
    }

    let only_files: Option<Vec<String>> = body
        .get("files")
        .and_then(Value::as_array)
        .map(|arr| {
            arr.iter()
                .filter_map(Value::as_str)
                .map(str::to_owned)
                .collect()
        });
    let mut office = state.office.lock().unwrap_or_else(|e| e.into_inner());
    if !office.sessions.contains_key(sid) && office.undo.plan(sid).is_none() {
        return Ok((StatusCode::NOT_FOUND, Json(json!({ "ok": false, "code": "gone" }))));
    }
    if office
        .sessions
        .get(sid)
        .is_some_and(|s| s.status != "rest")
    {
        return Ok((StatusCode::CONFLICT, Json(json!({ "ok": false, "code": "busy" }))));
    }
    let ts = now_ms();
    let res = office
        .undo
        .apply_filtered(sid, ts, only_files.as_deref());
    Ok((
        StatusCode::OK,
        Json(json!({
            "ok": true,
            "done": res.done,
            "after": res.after,
            "result": res,
        })),
    ))
}

pub async fn get_burn(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    OptionalPeer(peer): OptionalPeer,
    ViaTunnel(via_tunnel): ViaTunnel,
    Query(q): Query<HashMap<String, String>>,
) -> Result<Json<Value>, StatusCode> {
    let _ = require_auth(&state, peer, via_tunnel, &headers, &q)?;
    let range = match q.get("range").map(String::as_str) {
        Some("7d") => "7d",
        Some("30d") => "30d",
        _ => "today",
    };
    let (summary, info) = {
        let otel = state.otel.lock().unwrap_or_else(|e| e.into_inner());
        (otel.burn_summary(range, now_ms()), otel.info())
    };
    let mut val = json!(summary);
    if let Some(obj) = val.as_object_mut() {
        obj.insert("ok".to_owned(), Value::Bool(true));
        obj.insert("range".to_owned(), Value::String(range.to_owned()));
        obj.insert("days".to_owned(), info.get("days").cloned().unwrap_or(json!(0)));
        obj.insert(
            "lastLogsAt".to_owned(),
            info.get("lastLogsAt").cloned().unwrap_or(json!(0)),
        );
        obj.insert(
            "lastMetricsAt".to_owned(),
            info.get("lastMetricsAt").cloned().unwrap_or(json!(0)),
        );
        obj.insert("rest".to_owned(), json!({ "n": 0, "usd": 0.0, "share": 0.0 }));
        let settings_path = state.claude_dir.join("settings.json");
        let setup = std::fs::read_to_string(&settings_path)
            .ok()
            .and_then(|s| serde_json::from_str::<Value>(&s).ok())
            .and_then(|v| v.get("env").and_then(Value::as_object).cloned())
            .map(|env| {
                if !crate::store::otel_setup::marked_keys(&env).is_empty() {
                    "ours"
                } else if env.get("CLAUDE_CODE_ENABLE_TELEMETRY").and_then(Value::as_str) == Some("1") {
                    "other"
                } else {
                    "none"
                }
            })
            .unwrap_or("none");
        obj.insert("setup".to_owned(), Value::String(setup.to_owned()));
        obj.insert("plan".to_owned(), Value::Null);
    }
    Ok(Json(val))
}

pub async fn get_sys(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    OptionalPeer(peer): OptionalPeer,
    ViaTunnel(via_tunnel): ViaTunnel,
    Query(q): Query<HashMap<String, String>>,
) -> Result<Json<Value>, StatusCode> {
    let _ = require_auth(&state, peer, via_tunnel, &headers, &q)?;
    let auto_hours = state
        .prefs
        .lock()
        .unwrap_or_else(|e| e.into_inner())
        .auto_close();
    let mut val = json!(state.sys.snapshot());
    if let Some(obj) = val.as_object_mut() {
        obj.insert("auto".to_owned(), json!({ "hours": auto_hours, "last": Value::Null }));
        obj.entry("ide".to_owned()).or_insert_with(|| json!([]));
        obj.entry("open".to_owned()).or_insert_with(|| json!([]));
        obj.entry("rules".to_owned()).or_insert_with(|| json!([]));
    }
    Ok(Json(val))
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

fn is_valid_chat_photo(data: &str) -> bool {
    if data.len() >= 500_000 {
        return false;
    }
    let Some(rest) = data
        .strip_prefix("data:image/jpeg;base64,")
        .or_else(|| data.strip_prefix("data:image/png;base64,"))
        .or_else(|| data.strip_prefix("data:image/webp;base64,"))
    else {
        return false;
    };
    !rest.is_empty()
        && rest
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b == b'+' || b == b'/' || b == b'=')
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
    let local = require_auth(&state, peer, via_tunnel, &headers, &q)?;
    let ts = now_ms();
    let mut office = state.office.lock().unwrap_or_else(|e| e.into_inner());
    let op = body.get("op").and_then(Value::as_str).unwrap_or("send");
    match op {
        "send" => {
            let raw_text = body.get("text").and_then(Value::as_str).unwrap_or("");
            let text = crate::redact::redact(raw_text);
            let sticker = body.get("sticker").and_then(Value::as_u64).map(|v| v as u32);
            let reply = body
                .get("replyTo")
                .and_then(Value::as_str)
                .filter(|s| !s.is_empty())
                .and_then(|rid| office.chat.reply_ref(rid));
            let msg = office
                .chat
                .send_rich("me", "Boss", &text, ts, false, sticker, None, reply);
            let _ = office.save_chat();
            office.broadcast(json!({ "k": "chat", "op": "send", "m": msg }));
            Ok(Json(json!({ "ok": true, "msg": msg })))
        }
        "react" => {
            let id = body.get("id").and_then(Value::as_str).unwrap_or("");
            let emoji = body.get("emoji").and_then(Value::as_str).unwrap_or("");
            let ok = office.chat.react(id, "me", emoji);
            let _ = office.save_chat();
            if ok {
                let m = office.chat.by_id(id).cloned();
                office.broadcast(json!({ "k": "chat", "op": "react", "id": id, "m": m }));
            }
            Ok(Json(json!({ "ok": ok })))
        }
        "vote" => {
            let id = body.get("id").and_then(Value::as_str).unwrap_or("");
            let idx = body.get("i").and_then(Value::as_u64).unwrap_or(0) as usize;
            let ok = office.chat.vote(id, "me", idx);
            let _ = office.save_chat();
            if ok {
                let m = office.chat.by_id(id).cloned();
                office.broadcast(json!({ "k": "chat", "op": "vote", "id": id, "m": m }));
            }
            Ok(Json(json!({ "ok": ok })))
        }
        "rsvp" => {
            let id = body.get("id").and_then(Value::as_str).unwrap_or("");
            let yes = body.get("yes").and_then(Value::as_bool).unwrap_or(false);
            let ok = office.chat.rsvp(id, "me", yes);
            let _ = office.save_chat();
            if ok {
                let m = office.chat.by_id(id).cloned();
                office.broadcast(json!({ "k": "chat", "op": "rsvp", "id": id, "m": m }));
            }
            Ok(Json(json!({ "ok": ok })))
        }
        "recall" => {
            let id = body.get("id").and_then(Value::as_str).unwrap_or("");
            let ok = office.chat.recall(id);
            let _ = office.save_chat();
            if ok {
                office.broadcast(json!({ "k": "chat", "op": "recall", "id": id }));
            }
            Ok(Json(json!({ "ok": ok })))
        }
        "pin" => {
            let id = body.get("id").and_then(Value::as_str).unwrap_or("");
            let ok = office.chat.pin(id);
            let _ = office.save_chat();
            if ok {
                office.broadcast(json!({ "k": "chat", "op": "pin", "id": id }));
            }
            Ok(Json(json!({ "ok": ok })))
        }
        "poll" => {
            let q_text = crate::redact::redact(body.get("q").and_then(Value::as_str).unwrap_or(""));
            let opts: Vec<String> = body
                .get("opts")
                .and_then(Value::as_array)
                .map(|arr| {
                    arr.iter()
                        .take(6)
                        .filter_map(Value::as_str)
                        .map(crate::redact::redact)
                        .collect()
                })
                .unwrap_or_default();
            let Some(msg) = office.chat.poll("me", "Boss", &q_text, &opts, ts) else {
                return Err(StatusCode::BAD_REQUEST);
            };
            let _ = office.save_chat();
            office.broadcast(json!({ "k": "chat", "op": "poll", "m": msg }));
            Ok(Json(json!({ "ok": true, "msg": msg })))
        }
        "photo" => {
            if !local {
                return Err(StatusCode::FORBIDDEN);
            }
            let data = body.get("data").and_then(Value::as_str).unwrap_or("");
            if !is_valid_chat_photo(data) {
                return Err(StatusCode::BAD_REQUEST);
            }
            let msg = office.chat.photo("me", "Boss", data.to_owned(), ts);
            let _ = office.save_chat();
            office.broadcast(json!({ "k": "chat", "op": "photo", "m": msg }));
            Ok(Json(json!({ "ok": true, "msg": msg })))
        }
        "say" => {
            if !local {
                return Err(StatusCode::FORBIDDEN);
            }
            let key = body.get("key").and_then(Value::as_str);
            if !office.chat.fresh(key) {
                return Ok(Json(json!({ "ok": true, "deduped": true })));
            }
            let from: String = body
                .get("from")
                .and_then(Value::as_str)
                .unwrap_or("reception")
                .chars()
                .take(60)
                .collect();
            let name = body
                .get("name")
                .and_then(Value::as_str)
                .unwrap_or(&from)
                .to_owned();
            let text = crate::redact::redact(body.get("text").and_then(Value::as_str).unwrap_or(""));
            let sticker = body.get("sticker").and_then(Value::as_u64).map(|v| v as u32);
            let photo = body
                .get("photo")
                .and_then(Value::as_str)
                .filter(|d| is_valid_chat_photo(d))
                .map(str::to_owned);
            let msg = office
                .chat
                .send_rich(&from, &name, &text, ts, false, sticker, photo, None);
            if body.get("pin").and_then(Value::as_bool) == Some(true) {
                office.chat.pin(&msg.id);
            }
            let _ = office.save_chat();
            office.broadcast(json!({ "k": "chat", "op": "say", "m": msg }));
            Ok(Json(json!({ "ok": true, "msg": msg })))
        }
        _ => Err(StatusCode::BAD_REQUEST),
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

fn tasks_out(state: &AppState, local: bool) -> Value {
    let (phone_tasks, phone_bg) = {
        let prefs = state.prefs.lock().unwrap_or_else(|e| e.into_inner());
        (
            prefs.get("phoneTasks").and_then(Value::as_bool).unwrap_or(false),
            prefs.get("phoneTasksBg").and_then(Value::as_bool).unwrap_or(false),
        )
    };
    let (running, completed, bg_probe) = {
        let tasks = state.tasks.lock().unwrap_or_else(|e| e.into_inner());
        (
            tasks.list().into_iter().cloned().collect::<Vec<_>>(),
            tasks.recent_completed().into_iter().cloned().collect::<Vec<_>>(),
            tasks.bg_probe().clone(),
        )
    };
    let (projects, sessions) = {
        let office = state.office.lock().unwrap_or_else(|e| e.into_inner());
        let projects: Vec<Value> = office
            .known_dirs
            .iter()
            .filter(|d| std::path::Path::new(d).exists())
            .map(|d| {
                let name = std::path::Path::new(d)
                    .file_name()
                    .and_then(|n| n.to_str())
                    .unwrap_or(d);
                json!({ "dir": d, "name": name })
            })
            .collect();
        let sessions: Vec<Value> = office
            .sessions
            .values()
            .filter(|s| !s.cwd.is_empty())
            .map(|s| {
                json!({
                    "id": s.id,
                    "sid": s.id,
                    "agent": s.agent,
                    "name": s.name,
                    "dir": s.cwd,
                    "status": s.status,
                    "cache": s.cache,
                })
            })
            .collect();
        (projects, sessions)
    };
    json!({
        "ok": true,
        "allowed": local || phone_tasks,
        "tasks": running,
        "running": running,
        "completed": completed,
        "bg": {
            "on": phone_bg,
            "supported": bg_probe.supported,
            "version": bg_probe.version,
        },
        "agents": crate::state::tasks::available_agents(),
        "projects": projects,
        "sessions": sessions,
    })
}

fn require_task_permission(state: &AppState, local: bool) -> Result<(), StatusCode> {
    if local {
        return Ok(());
    }
    let allowed = state
        .prefs
        .lock()
        .unwrap_or_else(|e| e.into_inner())
        .get("phoneTasks")
        .and_then(Value::as_bool)
        .unwrap_or(false);
    if allowed {
        Ok(())
    } else {
        Err(StatusCode::FORBIDDEN)
    }
}

fn is_dir_allowed_for_remote(state: &AppState, dir: &str) -> bool {
    let Ok(target) = std::fs::canonicalize(dir) else {
        return false;
    };
    if !target.is_dir() {
        return false;
    }
    let matches_or_inside = |root: &str| -> bool {
        if root.is_empty() {
            return false;
        }
        std::fs::canonicalize(root).is_ok_and(|r| target == r || target.starts_with(&r))
    };
    {
        let office = state.office.lock().unwrap_or_else(|e| e.into_inner());
        if office.known_dirs.iter().any(|d| matches_or_inside(d))
            || office.sessions.values().any(|s| matches_or_inside(&s.cwd))
        {
            return true;
        }
    }
    {
        let git = state.git.lock().unwrap_or_else(|e| e.into_inner());
        if git.all_repos().iter().any(|r| matches_or_inside(&r.root)) {
            return true;
        }
    }
    {
        let stranded = state.stranded.lock().unwrap_or_else(|e| e.into_inner());
        if stranded.dir_of(dir).is_some() {
            return true;
        }
    }
    false
}

pub(crate) fn spawn_task_with_card(
    state: &Arc<AppState>,
    req: crate::state::tasks::TaskStartRequest,
) -> Value {
    let ts = now_ms();
    let work_agent = req.agent.clone();
    let dir_name = std::path::Path::new(&req.dir)
        .file_name()
        .and_then(|n| n.to_str())
        .unwrap_or(&work_agent)
        .to_owned();
    // Create and broadcast the work card BEFORE spawning so fast-exiting children
    // can never race ahead of `card_id` registration.
    let card_id = {
        let mut office = state.office.lock().unwrap_or_else(|e| e.into_inner());
        let card = office.chat.work_card(
            &work_agent,
            &dir_name,
            json!({ "state": "work", "steps": [], "n": 0, "at": ts, "task": "" }),
            ts,
        );
        let id = card.id.clone();
        let _ = office.save_chat();
        office.broadcast(json!({ "k": "chat", "op": "work", "m": card }));
        id
    };

    let state_done = Arc::clone(state);
    let cid_done = card_id.clone();
    let on_done: Arc<dyn Fn(crate::state::tasks::TaskRecord) + Send + Sync> =
        Arc::new(move |rec| {
            let mut updated_card = None;
            {
                let mut office = state_done.office.lock().unwrap_or_else(|e| e.into_inner());
                let st = if rec.status == "done" { "done" } else { &rec.status };
                if let Some(m) = office.chat.set_work(
                    &cid_done,
                    json!({
                        "state": st,
                        "task": rec.id,
                        "result": rec.result,
                        "error": rec.error,
                        "shots": rec.shots,
                        "choices": rec.choices,
                    }),
                ) {
                    updated_card = Some(m.clone());
                }
                let _ = office.save_chat();
                if let Some(m) = &updated_card {
                    office.broadcast(json!({ "k": "chat", "op": "work", "id": cid_done, "m": m }));
                }
            }
            // Drain any queued steer note targeting this task or its session.
            let queued = {
                let mut sq = state_done
                    .steer_queue
                    .lock()
                    .unwrap_or_else(|e| e.into_inner());
                sq.take(&rec.id)
                    .or_else(|| rec.sid.as_deref().and_then(|sid| sq.take(sid)))
                    .or_else(|| sq.take(&format!("dir:{}", rec.dir)))
            };
            if let Some(q) = queued
                && let Some(q_dir) = q.dir.filter(|d| std::path::Path::new(d).is_dir())
            {
                let _ = spawn_task_with_card(
                    &state_done,
                    crate::state::tasks::TaskStartRequest {
                        dir: q_dir,
                        text: q.text,
                        agent: q.agent,
                        model: String::new(),
                        fork: false,
                    },
                );
            }
        });

    let res = crate::state::tasks::start_and_spawn(
        Arc::clone(&state.tasks),
        req,
        ts,
        Some(on_done),
    );
    if res.get("ok").and_then(Value::as_bool) == Some(true) {
        if let Some(tid) = res
            .get("task")
            .and_then(|t| t.get("id"))
            .and_then(Value::as_str)
        {
            let mut office = state.office.lock().unwrap_or_else(|e| e.into_inner());
            if let Some(m) = office.chat.set_work(&card_id, json!({ "task": tid })) {
                let m_clone = m.clone();
                let _ = office.save_chat();
                office.broadcast(json!({ "k": "chat", "op": "work", "id": card_id, "m": m_clone }));
            }
        }
    } else {
        // Spawn failed: remove the pre-created work card cleanly.
        let mut office = state.office.lock().unwrap_or_else(|e| e.into_inner());
        office.chat.msgs.retain(|m| m.id != card_id);
        let _ = office.save_chat();
    }
    res
}

pub(crate) async fn steer_inner(
    state: &Arc<AppState>,
    body: &Value,
) -> (StatusCode, Value) {
    let text: String = body
        .get("text")
        .and_then(Value::as_str)
        .unwrap_or("")
        .trim()
        .chars()
        .take(2000)
        .collect();
    if text.is_empty() {
        return (StatusCode::BAD_REQUEST, json!({ "ok": false, "code": "empty" }));
    }
    let task_id = body.get("task").and_then(Value::as_str);
    let sid_req = body.get("sid").and_then(Value::as_str);
    let dir_req = body.get("dir").and_then(Value::as_str);
    let agent_req = body.get("agent").and_then(Value::as_str).unwrap_or("claude");

    let (task_rec, task_inbox) = {
        let tasks = state.tasks.lock().unwrap_or_else(|e| e.into_inner());
        let rec = task_id
            .and_then(|id| tasks.get(id))
            .or_else(|| sid_req.and_then(|sid| tasks.of_sid(sid)));
        let inbox = rec.as_ref().and_then(|r| tasks.inbox_of(&r.id));
        (rec, inbox)
    };

    let sess_rec = {
        let office = state.office.lock().unwrap_or_else(|e| e.into_inner());
        let target_sid = task_rec
            .as_ref()
            .and_then(|t| t.sid.as_deref())
            .or(sid_req);
        target_sid.and_then(|sid| office.sessions.get(sid).cloned())
    };

    let sess_busy = sess_rec.as_ref().is_some_and(|s| s.status != "rest");
    if task_rec.is_none() && !sess_busy {
        return (StatusCode::CONFLICT, json!({ "ok": false, "code": "notBusy" }));
    }

    let name = sess_rec
        .as_ref()
        .map(|s| s.name.clone())
        .or_else(|| {
            task_rec.as_ref().and_then(|t| {
                std::path::Path::new(&t.dir)
                    .file_name()
                    .and_then(|n| n.to_str())
                    .map(str::to_owned)
            })
        })
        .unwrap_or_else(|| "Claude".to_owned());
    let agent = task_rec
        .as_ref()
        .map(|t| t.agent.as_str())
        .or_else(|| sess_rec.as_ref().map(|s| s.agent.as_str()))
        .unwrap_or(agent_req)
        .to_owned();
    let inbox = task_inbox.or_else(|| sess_rec.as_ref().and_then(|s| s.inbox.clone()));
    let cwd = sess_rec
        .as_ref()
        .map(|s| s.cwd.clone())
        .or_else(|| task_rec.as_ref().map(|t| t.dir.clone()))
        .or_else(|| dir_req.map(str::to_owned));

    let check = if agent == "claude" {
        let inbound = crate::state::steer::inbound_from(
            Some(state.claude_dir.as_path()),
            cwd.as_deref().map(std::path::Path::new),
        );
        let sock_ok = inbox
            .as_deref()
            .is_some_and(crate::state::steer::is_socket_path);
        crate::state::steer::steerable(inbox.as_deref(), "", inbound, sock_ok)
    } else {
        crate::state::steer::SteerCheck {
            ok: false,
            why: Some("noInbox"),
        }
    };

    let ts = now_ms();
    let clean_text = crate::redact::redact(&text);
    {
        let mut office = state.office.lock().unwrap_or_else(|e| e.into_inner());
        let msg = office
            .chat
            .say("me", "Boss", &format!("💬 @{name} {clean_text}"), ts, true, None);
        let _ = office.save_chat();
        office.broadcast(json!({ "k": "chat", "op": "send", "m": msg }));
    }

    if check.ok
        && let Some(ib) = inbox.as_deref()
        && crate::state::steer::deliver(ib, &text, 3000).await
    {
        return (StatusCode::OK, json!({ "ok": true, "how": "now" }));
    }

    let why = if check.ok {
        "failed"
    } else {
        check.why.unwrap_or("noInbox")
    };
    let key = sid_req
        .map(str::to_owned)
        .or_else(|| sess_rec.as_ref().map(|s| s.id.clone()))
        .or_else(|| task_rec.as_ref().map(|t| t.id.clone()))
        .or_else(|| cwd.as_ref().map(|d| format!("dir:{d}")));
    let Some(key) = key else {
        return (StatusCode::CONFLICT, json!({ "ok": false, "code": "notBusy" }));
    };

    state
        .steer_queue
        .lock()
        .unwrap_or_else(|e| e.into_inner())
        .enqueue(
            crate::state::steer::QueuedSteer {
                key,
                sid: sid_req
                    .map(str::to_owned)
                    .or_else(|| sess_rec.as_ref().map(|s| s.id.clone())),
                dir: cwd,
                agent,
                text,
                name,
                at: ts,
            },
            ts,
        );
    (StatusCode::OK, json!({ "ok": true, "how": "later", "why": why }))
}

pub async fn get_tasks(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    OptionalPeer(peer): OptionalPeer,
    ViaTunnel(via_tunnel): ViaTunnel,
    Query(q): Query<HashMap<String, String>>,
) -> Result<Json<Value>, StatusCode> {
    let local = require_auth(&state, peer, via_tunnel, &headers, &q)?;
    Ok(Json(tasks_out(&state, local)))
}

pub async fn post_tasks(
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
    require_task_permission(&state, local)?;
    let sid = body.get("sid").and_then(Value::as_str);
    let mut dir = body
        .get("dir")
        .and_then(Value::as_str)
        .unwrap_or("")
        .to_owned();
    if dir.is_empty()
        && let Some(sid) = sid
    {
        let office = state.office.lock().unwrap_or_else(|e| e.into_inner());
        if let Some(s) = office.sessions.get(sid) {
            dir = s.cwd.clone();
        }
    }
    if !local && !is_dir_allowed_for_remote(&state, &dir) {
        return Err(StatusCode::FORBIDDEN);
    }
    let req = crate::state::tasks::TaskStartRequest {
        dir,
        text: body.get("text").and_then(Value::as_str).unwrap_or("").to_owned(),
        agent: body
            .get("agent")
            .and_then(Value::as_str)
            .unwrap_or("claude")
            .to_owned(),
        model: body.get("model").and_then(Value::as_str).unwrap_or("").to_owned(),
        fork: body.get("fork").and_then(Value::as_bool).unwrap_or(false),
    };
    Ok(Json(spawn_task_with_card(&state, req)))
}

pub async fn post_tasks_allow(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    OptionalPeer(peer): OptionalPeer,
    ViaTunnel(via_tunnel): ViaTunnel,
    Json(body): Json<Value>,
) -> Result<Json<Value>, StatusCode> {
    require_local_post(peer, via_tunnel, &headers)?;
    let on = body.get("on").and_then(Value::as_bool).unwrap_or(false);
    let approve = {
        let mut prefs = state.prefs.lock().unwrap_or_else(|e| e.into_inner());
        let _ = prefs.set("phoneTasks", if on { Some(Value::Bool(true)) } else { None });
        prefs
            .get("phoneApprove")
            .and_then(Value::as_bool)
            .unwrap_or(false)
    };
    state
        .office
        .lock()
        .unwrap_or_else(|e| e.into_inner())
        .broadcast(json!({ "k": "phone", "approve": approve, "tasks": on }));
    Ok(Json(tasks_out(&state, true)))
}

pub async fn post_tasks_bg(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    OptionalPeer(peer): OptionalPeer,
    ViaTunnel(via_tunnel): ViaTunnel,
    Json(body): Json<Value>,
) -> Result<Json<Value>, StatusCode> {
    require_local_post(peer, via_tunnel, &headers)?;
    let on = body.get("on").and_then(Value::as_bool).unwrap_or(false);
    {
        let mut prefs = state.prefs.lock().unwrap_or_else(|e| e.into_inner());
        let _ = prefs.set("phoneTasksBg", if on { Some(Value::Bool(true)) } else { None });
    }
    let probe = tokio::task::spawn_blocking(|| {
        if let Some(cli) = crate::state::tasks::resolve_agent_cli("claude") {
            crate::state::bg_sessions::probe_claude_bg(&cli.file)
        } else {
            crate::state::bg_sessions::BgProbeResult::default()
        }
    })
    .await
    .unwrap_or_default();
    state
        .tasks
        .lock()
        .unwrap_or_else(|e| e.into_inner())
        .set_bg_probe(probe);
    Ok(Json(tasks_out(&state, true)))
}

pub async fn post_tasks_stop(
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
    require_task_permission(&state, local)?;
    let id = body.get("id").and_then(Value::as_str).unwrap_or("");
    let ok = {
        let mut tasks = state.tasks.lock().unwrap_or_else(|e| e.into_inner());
        let target = tasks
            .of_sid(id)
            .map(|r| r.id.clone())
            .unwrap_or_else(|| id.to_owned());
        tasks.stop(&target)
    };
    Ok(Json(json!({ "ok": ok })))
}

pub async fn post_tasks_steer(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    OptionalPeer(peer): OptionalPeer,
    ViaTunnel(via_tunnel): ViaTunnel,
    Query(q): Query<HashMap<String, String>>,
    Json(body): Json<Value>,
) -> Result<(StatusCode, Json<Value>), StatusCode> {
    enforce_same_origin_post_with_transport(
        &headers,
        state.allowed_tunnel_host().as_deref(),
        via_tunnel,
    )?;
    let local = require_auth(&state, peer, via_tunnel, &headers, &q)?;
    require_task_permission(&state, local)?;
    let (status, val) = steer_inner(&state, &body).await;
    Ok((status, Json(val)))
}

pub async fn post_ci(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    OptionalPeer(peer): OptionalPeer,
    ViaTunnel(via_tunnel): ViaTunnel,
    Query(q): Query<HashMap<String, String>>,
    Json(body): Json<Value>,
) -> Result<(StatusCode, Json<Value>), StatusCode> {
    enforce_same_origin_post_with_transport(
        &headers,
        state.allowed_tunnel_host().as_deref(),
        via_tunnel,
    )?;
    let local = require_auth(&state, peer, via_tunnel, &headers, &q)?;
    require_task_permission(&state, local)?;
    let id = body.get("id").and_then(Value::as_str).unwrap_or("");
    let ci_data = {
        let office = state.office.lock().unwrap_or_else(|e| e.into_inner());
        office
            .chat
            .by_id(id)
            .filter(|m| m.kind == "ci")
            .and_then(|m| m.ci.clone())
    };
    let Some(c) = ci_data else {
        return Ok((StatusCode::NOT_FOUND, Json(json!({ "ok": false, "code": "gone" }))));
    };
    if c.get("state").and_then(Value::as_str) != Some("red") {
        return Ok((StatusCode::CONFLICT, Json(json!({ "ok": false, "code": "notRed" }))));
    }
    let wf = c.get("wf").and_then(Value::as_str).unwrap_or("CI");
    let branch = c.get("branch").and_then(Value::as_str).unwrap_or("");
    let url = c.get("url").and_then(Value::as_str).unwrap_or("");
    let sid = c
        .get("sid")
        .or_else(|| c.get("sess"))
        .and_then(Value::as_str)
        .unwrap_or("");
    let dir = c.get("dir").and_then(Value::as_str).unwrap_or("");
    let prompt = format!("Fix failing {wf} on branch {branch} ({url})");

    let sess_busy = {
        let office = state.office.lock().unwrap_or_else(|e| e.into_inner());
        !sid.is_empty() && office.sessions.get(sid).is_some_and(|s| s.status != "rest")
    };
    if sess_busy {
        let (st, out) = steer_inner(&state, &json!({ "sid": sid, "text": prompt })).await;
        if st == StatusCode::OK {
            let mut office = state.office.lock().unwrap_or_else(|e| e.into_inner());
            office.chat.set_ci(id, json!({ "fix": "note" }));
            let _ = office.save_chat();
            return Ok((StatusCode::OK, Json(json!({ "ok": true, "how": "note" }))));
        }
        return Ok((st, Json(out)));
    }
    let res = spawn_task_with_card(
        &state,
        crate::state::tasks::TaskStartRequest {
            dir: dir.to_owned(),
            text: prompt,
            agent: "claude".to_owned(),
            model: String::new(),
            fork: false,
        },
    );
    if res.get("ok").and_then(Value::as_bool) == Some(true) {
        let mut office = state.office.lock().unwrap_or_else(|e| e.into_inner());
        office.chat.set_ci(id, json!({ "fix": "sent" }));
        let _ = office.save_chat();
        Ok((StatusCode::OK, Json(json!({ "ok": true, "how": "sent" }))))
    } else {
        Ok((StatusCode::CONFLICT, Json(res)))
    }
}

fn read_real_text(path: &std::path::Path) -> (std::path::PathBuf, Option<String>) {
    let real = std::fs::canonicalize(path).unwrap_or_else(|_| path.to_path_buf());
    match std::fs::read_to_string(&real) {
        Ok(text) => (real, Some(text)),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => (real, Some(String::new())),
        Err(_) => (real, None),
    }
}

fn models_out(claude_dir: &std::path::Path) -> Value {
    let (_, set_text) = read_real_text(&claude_dir.join("settings.json"));
    let (_, mem_text) = read_real_text(&claude_dir.join("CLAUDE.md"));
    let parsed: Option<Value> = set_text.as_deref().and_then(|t| {
        if t.trim().is_empty() {
            Some(json!({}))
        } else {
            serde_json::from_str::<Value>(t).ok()
        }
    });
    let read_only = set_text.is_none() || parsed.is_none();
    let cfg = parsed.unwrap_or_else(|| json!({}));
    let view = models_of(&cfg);
    let split = mem_text.as_deref().is_some_and(has_split);
    json!({
        "ok": true,
        "main": view.main,
        "sub": view.sub,
        "force": view.force,
        "split": split,
        "main_": MAIN_MODELS,
        "sub_": SUB_MODELS,
        "readOnly": read_only,
        "models": default_claude_models(),
        "config": view,
    })
}

pub async fn get_models(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    OptionalPeer(peer): OptionalPeer,
    ViaTunnel(via_tunnel): ViaTunnel,
    Query(q): Query<HashMap<String, String>>,
) -> Result<Json<Value>, StatusCode> {
    let local = require_auth(&state, peer, via_tunnel, &headers, &q)?;
    if q.contains_key("agent") || q.contains_key("dir") || q.contains_key("sid") {
        require_task_permission(&state, local)?;
        let agent = q.get("agent").map(String::as_str).unwrap_or("claude");
        if !matches!(agent, "claude" | "codex" | "opencode" | "antigravity" | "zcode") {
            return Err(StatusCode::BAD_REQUEST);
        }
        return Ok(Json(json!({
            "ok": true,
            "models": if agent == "claude" { default_claude_models() } else { Vec::new() },
            "source": if agent == "claude" { "cli-aliases" } else { "cli" },
        })));
    }
    if !local {
        return Err(StatusCode::FORBIDDEN);
    }
    Ok(Json(models_out(&state.claude_dir)))
}

pub async fn post_models(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    OptionalPeer(peer): OptionalPeer,
    ViaTunnel(via_tunnel): ViaTunnel,
    Json(body): Json<Value>,
) -> Result<Json<Value>, StatusCode> {
    require_local_post(peer, via_tunnel, &headers)?;
    let main = match body.get("main") {
        None => None,
        Some(v) => {
            let s = v.as_str().ok_or(StatusCode::BAD_REQUEST)?;
            if !MAIN_MODELS.contains(&s) {
                return Err(StatusCode::BAD_REQUEST);
            }
            Some(s)
        }
    };
    let sub = match body.get("sub") {
        None => None,
        Some(v) => {
            let s = v.as_str().ok_or(StatusCode::BAD_REQUEST)?;
            if !SUB_MODELS.contains(&s) {
                return Err(StatusCode::BAD_REQUEST);
            }
            Some(s)
        }
    };
    let force = body.get("force").and_then(Value::as_bool);
    let split = body.get("split").and_then(Value::as_bool);

    if main.is_some() || sub.is_some() || force.is_some() {
        let (real_settings, set_text) = read_real_text(&state.claude_dir.join("settings.json"));
        let Some(raw) = set_text else {
            return Ok(Json(json!({ "ok": false, "message": "failed to read settings.json" })));
        };
        let cfg: Value = if raw.trim().is_empty() {
            json!({})
        } else {
            match serde_json::from_str(&raw) {
                Ok(v) => v,
                Err(_) => {
                    return Ok(Json(json!({ "ok": false, "message": "settings.json is not valid JSON" })));
                }
            }
        };
        let next = with_models(&cfg, main, sub, force);
        if crate::atomic::write_json_atomic(&real_settings, &next).is_err() {
            return Ok(Json(json!({ "ok": false, "message": "failed to write settings.json" })));
        }
    }

    if let Some(on) = split {
        let (real_md, mem_text) = read_real_text(&state.claude_dir.join("CLAUDE.md"));
        let Some(raw) = mem_text else {
            return Ok(Json(json!({ "ok": false, "message": "failed to read CLAUDE.md" })));
        };
        let next = with_split(&raw, on, DEFAULT_SPLIT_NOTE);
        if next != raw
            && crate::atomic::write_bytes_atomic(&real_md, next.as_bytes()).is_err()
        {
            return Ok(Json(json!({ "ok": false, "message": "failed to write CLAUDE.md" })));
        }
    }

    Ok(Json(models_out(&state.claude_dir)))
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
    let raw_pin = body
        .get("pin")
        .and_then(Value::as_str)
        .map(str::trim)
        .unwrap_or("");
    if raw_pin.is_empty() && body.get("pin").is_none() {
        return match state.auth.regenerate_pin() {
            Ok(pin) => Ok(Json(json!({ "ok": true, "pin": pin }))),
            Err(_) => Ok(Json(json!({ "ok": false, "pin": state.auth.current_pin() }))),
        };
    }
    let valid = !crate::auth::pin_weak(raw_pin);
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
        state.auth.set_pin(raw_pin).unwrap_or(false)
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

fn parse_answers_map(
    body: &Value,
    tool_input: Option<&Value>,
) -> Option<serde_json::Map<String, Value>> {
    let raw = body.get("answers")?;
    if let Some(obj) = raw.as_object() {
        return (!obj.is_empty()).then(|| obj.clone());
    }
    let arr = raw.as_array()?;
    let qs = tool_input
        .and_then(|i| i.get("questions"))
        .and_then(Value::as_array)?;
    let mut out = serde_json::Map::new();
    for item in arr {
        let Some(q_idx) = item.get("q").and_then(Value::as_u64).map(|n| n as usize) else {
            continue;
        };
        let Some(q_obj) = qs.get(q_idx) else {
            continue;
        };
        let Some(q_text) = q_obj.get("question").and_then(Value::as_str) else {
            continue;
        };
        let multi = q_obj
            .get("multiSelect")
            .and_then(Value::as_bool)
            .unwrap_or(false);
        let opts = q_obj.get("options").and_then(Value::as_array);
        let mut parts: Vec<String> = item
            .get("pick")
            .and_then(Value::as_array)
            .map(|picks| {
                picks
                    .iter()
                    .filter_map(Value::as_u64)
                    .filter_map(|idx| opts.and_then(|o| o.get(idx as usize)))
                    .filter_map(|o| o.get("label").and_then(Value::as_str))
                    .map(str::to_owned)
                    .collect()
            })
            .unwrap_or_default();
        if !multi && parts.len() > 1 {
            parts.truncate(1);
        }
        if let Some(other) = item.get("other").and_then(Value::as_str) {
            let clean: String = one_line(other).chars().take(500).collect();
            if !clean.is_empty() {
                parts.push(clean);
            }
        }
        let val = parts.join(", ");
        if !val.is_empty() {
            out.insert(q_text.to_owned(), Value::String(val));
        }
    }
    (!out.is_empty()).then_some(out)
}

enum ParsedPermAction {
    Skip,
    Decide(PermDecision),
}

fn parse_perm_action(
    body: &Value,
    perm_kind: &str,
    tool_input: Option<&Value>,
) -> Result<ParsedPermAction, StatusCode> {
    let choice = body
        .get("choice")
        .or_else(|| body.get("decision"))
        .and_then(Value::as_str);
    if choice == Some("skip") {
        return Ok(ParsedPermAction::Skip);
    }
    let has_answers = body.get("answers").is_some();
    let deny_reason = || {
        body.get("reason")
            .or_else(|| body.get("note"))
            .and_then(Value::as_str)
            .unwrap_or("")
            .to_owned()
    };

    match perm_kind {
        "perm" | "permission" => {
            if has_answers {
                return Err(StatusCode::BAD_REQUEST);
            }
            match choice {
                Some("allow" | "always" | "approve") | None => {
                    Ok(ParsedPermAction::Decide(PermDecision::Allow))
                }
                Some("deny" | "revise") => {
                    Ok(ParsedPermAction::Decide(PermDecision::Deny {
                        reason: deny_reason(),
                    }))
                }
                _ => Err(StatusCode::BAD_REQUEST),
            }
        }
        "ask" => {
            if has_answers || choice == Some("answer") {
                let Some(map) = parse_answers_map(body, tool_input) else {
                    return Err(StatusCode::BAD_REQUEST);
                };
                return Ok(ParsedPermAction::Decide(PermDecision::Answers(map)));
            }
            match choice {
                Some("deny" | "revise") => {
                    Ok(ParsedPermAction::Decide(PermDecision::Deny {
                        reason: deny_reason(),
                    }))
                }
                _ => Err(StatusCode::BAD_REQUEST),
            }
        }
        "plan" => {
            if has_answers {
                return Err(StatusCode::BAD_REQUEST);
            }
            match choice {
                Some("allow" | "approve") | None => {
                    Ok(ParsedPermAction::Decide(PermDecision::Allow))
                }
                Some("deny" | "revise") => {
                    Ok(ParsedPermAction::Decide(PermDecision::Deny {
                        reason: deny_reason(),
                    }))
                }
                _ => Err(StatusCode::BAD_REQUEST),
            }
        }
        "denied" => {
            if has_answers {
                return Err(StatusCode::BAD_REQUEST);
            }
            match choice {
                Some("retry") => Ok(ParsedPermAction::Decide(PermDecision::Retry)),
                Some("keep") => Ok(ParsedPermAction::Decide(PermDecision::Keep)),
                _ => Err(StatusCode::BAD_REQUEST),
            }
        }
        _ => Err(StatusCode::BAD_REQUEST),
    }
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
    let perm_kind = office
        .pending_perms
        .get(id)
        .map(|p| p.kind.clone())
        .unwrap_or_else(|| "perm".to_owned());
    let action = parse_perm_action(&body, &perm_kind, office.hold_tool_input(id))?;
    let resolved = match action {
        ParsedPermAction::Skip => office.cancel_hold(id),
        ParsedPermAction::Decide(decision) => office.resolve_hold(id, decision).is_some(),
    };
    Ok(Json(json!({ "ok": resolved })))
}

pub async fn post_panel_answer(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    OptionalPeer(peer): OptionalPeer,
    ViaTunnel(via_tunnel): ViaTunnel,
    Json(body): Json<Value>,
) -> Result<Json<Value>, StatusCode> {
    require_local_post(peer, via_tunnel, &headers)?;
    let id = body.get("id").and_then(Value::as_str).unwrap_or("");
    let mut office = state.office.lock().unwrap_or_else(|e| e.into_inner());
    let perm_kind = office
        .pending_perms
        .get(id)
        .map(|p| p.kind.clone())
        .unwrap_or_else(|| "perm".to_owned());
    let action = parse_perm_action(&body, &perm_kind, office.hold_tool_input(id))?;
    if matches!(action, ParsedPermAction::Decide(PermDecision::Allow))
        && office
            .pending_perms
            .get(id)
            .is_some_and(|p| p.danger.is_some())
    {
        return Err(StatusCode::FORBIDDEN);
    }
    let resolved = match action {
        ParsedPermAction::Skip => office.cancel_hold(id),
        ParsedPermAction::Decide(decision) => office.resolve_hold(id, decision).is_some(),
    };
    Ok(Json(json!({ "ok": resolved })))
}

pub async fn post_denied(
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
    let op = body
        .get("op")
        .or_else(|| body.get("choice"))
        .and_then(Value::as_str)
        .unwrap_or("keep");
    let decision = match op {
        "retry" => PermDecision::Retry,
        "keep" => PermDecision::Keep,
        _ => return Err(StatusCode::BAD_REQUEST),
    };
    let mut office = state.office.lock().unwrap_or_else(|e| e.into_inner());
    let Some(perm) = office.pending_perms.get(id) else {
        return Ok(Json(json!({ "ok": false })));
    };
    if perm.kind != "denied" {
        return Err(StatusCode::BAD_REQUEST);
    }
    if !local && perm.danger.is_some() {
        return Err(StatusCode::FORBIDDEN);
    }
    let resolved = office.resolve_hold(id, decision).is_some();
    Ok(Json(json!({ "ok": resolved })))
}

pub async fn get_push(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    OptionalPeer(peer): OptionalPeer,
    ViaTunnel(via_tunnel): ViaTunnel,
    Query(q): Query<HashMap<String, String>>,
) -> Result<Json<Value>, StatusCode> {
    let _ = require_auth(&state, peer, via_tunnel, &headers, &q)?;
    let endpoint = q.get("endpoint").map(String::as_str).unwrap_or("");
    let push = state.push.lock().unwrap_or_else(|e| e.into_inner());
    Ok(Json(json!({
        "ok": true,
        "key": push.public_key,
        "publicKey": push.public_key,
        "count": push.count(),
        "on": push.has(endpoint),
    })))
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

pub async fn post_push_test(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    OptionalPeer(peer): OptionalPeer,
    ViaTunnel(via_tunnel): ViaTunnel,
    Query(q): Query<HashMap<String, String>>,
    body: Option<Json<Value>>,
) -> Result<Json<Value>, StatusCode> {
    enforce_same_origin_post_with_transport(
        &headers,
        state.allowed_tunnel_host().as_deref(),
        via_tunnel,
    )?;
    let _ = require_auth(&state, peer, via_tunnel, &headers, &q)?;
    let payload = body.map(|Json(v)| v);
    let title: String = payload
        .as_ref()
        .and_then(|v| v.get("title"))
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .map(|s| crate::redact::redact(s).chars().take(120).collect())
        .unwrap_or_else(|| "Agents Station".to_owned());
    let body_text: String = payload
        .as_ref()
        .and_then(|v| v.get("body"))
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .map(|s| crate::redact::redact(s).chars().take(240).collect())
        .unwrap_or_else(|| "Test notification".to_owned());
    let msg = crate::store::push::PushMessage {
        title,
        body: body_text,
        tag: Some("station-test".to_owned()),
        url: "/phone".to_owned(),
        urgent: false,
    };
    let deliveries = state
        .push
        .lock()
        .unwrap_or_else(|e| e.into_inner())
        .prepare_deliveries(&msg, now_ms() / 1000);
    let sent = deliveries.len();
    if !deliveries.is_empty() {
        let state_bg = Arc::clone(&state);
        tokio::spawn(async move {
            let dead = crate::store::push::deliver_prepared(deliveries).await;
            if !dead.is_empty()
                && let Ok(mut push) = state_bg.push.lock()
            {
                for ep in dead {
                    push.remove(&ep);
                }
            }
        });
    }
    Ok(Json(json!({ "ok": true, "sent": sent })))
}

pub async fn post_prompt_rewrite(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    OptionalPeer(peer): OptionalPeer,
    ViaTunnel(via_tunnel): ViaTunnel,
    Query(q): Query<HashMap<String, String>>,
    Json(body): Json<Value>,
) -> Result<(StatusCode, Json<Value>), StatusCode> {
    enforce_same_origin_post_with_transport(
        &headers,
        state.allowed_tunnel_host().as_deref(),
        via_tunnel,
    )?;
    let local = require_auth(&state, peer, via_tunnel, &headers, &q)?;
    require_task_permission(&state, local)?;
    let text = body.get("text").and_then(Value::as_str).unwrap_or("").to_owned();
    let lang = body
        .get("lang")
        .and_then(Value::as_str)
        .unwrap_or("en")
        .to_owned();
    let photos = body.get("photos").and_then(Value::as_u64).unwrap_or(0) as usize;
    let rewriter = Arc::clone(&state.rewriter);
    let work_dir = state.data_dir.join("rewrite");
    let now = now_ms();
    let res = tokio::task::spawn_blocking(move || {
        crate::store::rewrite::Rewriter::execute_blocking(rewriter, work_dir, text, lang, photos, now)
    })
    .await
    .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    let status = if res.ok {
        StatusCode::OK
    } else if matches!(res.code, Some("busy" | "tooMany")) {
        StatusCode::TOO_MANY_REQUESTS
    } else {
        StatusCode::CONFLICT
    };
    Ok((status, Json(json!(res))))
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
    let sec_headers = [
        (header::CONTENT_SECURITY_POLICY, "sandbox"),
        (header::X_CONTENT_TYPE_OPTIONS, "nosniff"),
    ];
    if q.contains_key("file") || q.contains_key("path") {
        return (StatusCode::BAD_REQUEST, sec_headers, "bad request").into_response();
    }
    let Some(id) = q.get("id").map(String::as_str) else {
        return (StatusCode::NOT_FOUND, sec_headers, "not found").into_response();
    };
    if id.is_empty() {
        return (StatusCode::BAD_REQUEST, sec_headers, "bad request").into_response();
    }
    let resolved = {
        let store = state.images.lock().unwrap_or_else(|e| e.into_inner());
        store.lookup(id)
    };
    match resolved {
        Err(crate::store::images::ImageResolveError::InvalidId) => {
            (StatusCode::BAD_REQUEST, sec_headers, "invalid id").into_response()
        }
        Err(_) => (StatusCode::NOT_FOUND, sec_headers, "not found").into_response(),
        Ok((path, mime)) => {
            let read_res = tokio::task::spawn_blocking(move || {
                crate::store::images::read_image_bytes_bounded(
                    &path,
                    crate::store::images::MAX_IMAGE_BYTES,
                )
            })
            .await;
            match read_res {
                Ok(Ok(bytes)) => (
                    StatusCode::OK,
                    [
                        (header::CONTENT_TYPE, mime),
                        (header::CONTENT_SECURITY_POLICY, "sandbox"),
                        (header::X_CONTENT_TYPE_OPTIONS, "nosniff"),
                        (header::CACHE_CONTROL, "private, max-age=30"),
                    ],
                    bytes,
                )
                    .into_response(),
                _ => (StatusCode::NOT_FOUND, sec_headers, "not found").into_response(),
            }
        }
    }
}

pub async fn get_claude_version(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    OptionalPeer(peer): OptionalPeer,
    ViaTunnel(via_tunnel): ViaTunnel,
    Query(q): Query<HashMap<String, String>>,
) -> Result<Json<Value>, StatusCode> {
    let _ = require_auth(&state, peer, via_tunnel, &headers, &q)?;
    let cv = Arc::clone(&state.claude_version);
    let now = now_ms();
    let v = tokio::task::spawn_blocking(move || {
        let mut g = cv.lock().unwrap_or_else(|e| e.into_inner());
        if g.current().version.is_none() {
            g.check(now).clone()
        } else {
            g.current().clone()
        }
    })
    .await
    .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    Ok(Json(json!({
        "ok": true,
        "version": v,
        "prev": v.prev,
        "at": v.at,
    })))
}

pub async fn get_cache(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    OptionalPeer(peer): OptionalPeer,
    ViaTunnel(via_tunnel): ViaTunnel,
    Query(q): Query<HashMap<String, String>>,
) -> Result<Json<Value>, StatusCode> {
    let _ = require_auth(&state, peer, via_tunnel, &headers, &q)?;
    let nudge = state
        .prefs
        .lock()
        .unwrap_or_else(|e| e.into_inner())
        .get("cacheNudge")
        .and_then(Value::as_bool)
        .unwrap_or(false);
    Ok(Json(json!({ "ok": true, "nudge": nudge })))
}

pub async fn post_cache(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    OptionalPeer(peer): OptionalPeer,
    ViaTunnel(via_tunnel): ViaTunnel,
    Json(body): Json<Value>,
) -> Result<Json<Value>, StatusCode> {
    require_local_post(peer, via_tunnel, &headers)?;
    let nudge = body.get("nudge").and_then(Value::as_bool).unwrap_or(false);
    let _ = state
        .prefs
        .lock()
        .unwrap_or_else(|e| e.into_inner())
        .set("cacheNudge", if nudge { Some(Value::Bool(true)) } else { None });
    Ok(Json(json!({ "ok": true, "nudge": nudge })))
}

pub async fn get_quick(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    OptionalPeer(peer): OptionalPeer,
    ViaTunnel(via_tunnel): ViaTunnel,
    Query(q): Query<HashMap<String, String>>,
) -> Result<Json<Value>, StatusCode> {
    let _ = require_auth(&state, peer, via_tunnel, &headers, &q)?;
    let list: Option<Value> = std::fs::read_to_string(state.data_dir.join("quick.json"))
        .ok()
        .and_then(|s| serde_json::from_str(&s).ok());
    Ok(Json(json!({ "ok": true, "list": list })))
}

pub async fn post_quick(
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
    let list: Vec<Value> = body
        .get("list")
        .and_then(Value::as_array)
        .map(|arr| {
            arr.iter()
                .take(12)
                .filter_map(|item| {
                    let label: String = one_line(item.get("label").and_then(Value::as_str).unwrap_or(""))
                        .chars()
                        .take(24)
                        .collect();
                    let prompt: String = item
                        .get("prompt")
                        .and_then(Value::as_str)
                        .unwrap_or("")
                        .trim()
                        .chars()
                        .take(2000)
                        .collect();
                    if label.is_empty() || prompt.is_empty() {
                        None
                    } else {
                        Some(json!({
                            "label": crate::redact::redact(&label),
                            "prompt": crate::redact::redact(&prompt),
                        }))
                    }
                })
                .collect()
        })
        .unwrap_or_default();
    crate::atomic::write_json_atomic(&state.data_dir.join("quick.json"), &list)
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    Ok(Json(json!({ "ok": true, "list": list })))
}

pub async fn get_macapprove(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    OptionalPeer(peer): OptionalPeer,
    ViaTunnel(via_tunnel): ViaTunnel,
    Query(q): Query<HashMap<String, String>>,
) -> Result<Json<Value>, StatusCode> {
    let local = require_auth(&state, peer, via_tunnel, &headers, &q)?;
    if !local {
        return Err(StatusCode::FORBIDDEN);
    }
    let on = state
        .prefs
        .lock()
        .unwrap_or_else(|e| e.into_inner())
        .get("macApprove")
        .and_then(Value::as_bool)
        .unwrap_or(true);
    Ok(Json(json!({ "ok": true, "on": on })))
}

pub async fn post_macapprove(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    OptionalPeer(peer): OptionalPeer,
    ViaTunnel(via_tunnel): ViaTunnel,
    Json(body): Json<Value>,
) -> Result<Json<Value>, StatusCode> {
    require_local_post(peer, via_tunnel, &headers)?;
    let on = body.get("on").and_then(Value::as_bool).unwrap_or(true);
    let _ = state
        .prefs
        .lock()
        .unwrap_or_else(|e| e.into_inner())
        .set("macApprove", if !on { Some(Value::Bool(false)) } else { None });
    Ok(Json(json!({ "ok": true, "on": on })))
}

pub async fn post_phone(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    OptionalPeer(peer): OptionalPeer,
    ViaTunnel(via_tunnel): ViaTunnel,
    Json(body): Json<Value>,
) -> Result<Json<Value>, StatusCode> {
    require_local_post(peer, via_tunnel, &headers)?;
    let approve = body.get("approve").and_then(Value::as_bool).unwrap_or(false);
    let tasks_on = {
        let mut prefs = state.prefs.lock().unwrap_or_else(|e| e.into_inner());
        let _ = prefs.set("phoneApprove", if approve { Some(Value::Bool(true)) } else { None });
        prefs
            .get("phoneTasks")
            .and_then(Value::as_bool)
            .unwrap_or(false)
    };
    state
        .office
        .lock()
        .unwrap_or_else(|e| e.into_inner())
        .broadcast(json!({ "k": "phone", "approve": approve, "tasks": tasks_on }));
    Ok(Json(json!({ "ok": true, "approve": approve })))
}

pub async fn post_lan(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    OptionalPeer(peer): OptionalPeer,
    ViaTunnel(via_tunnel): ViaTunnel,
    Json(body): Json<Value>,
) -> Result<Json<Value>, StatusCode> {
    require_local_post(peer, via_tunnel, &headers)?;
    let on = body.get("on").and_then(Value::as_bool).unwrap_or(false);
    let _ = state
        .prefs
        .lock()
        .unwrap_or_else(|e| e.into_inner())
        .set("lan", if on { Some(Value::Bool(true)) } else { None });
    Ok(Json(json!({ "ok": true, "on": on, "pin": state.auth.current_pin() })))
}

fn rc_at_startup(claude_dir: &std::path::Path) -> bool {
    let settings_path = claude_dir.join("settings.json");
    let real = std::fs::canonicalize(&settings_path).unwrap_or(settings_path);
    std::fs::read_to_string(&real)
        .ok()
        .and_then(|s| serde_json::from_str::<Value>(&s).ok())
        .and_then(|v| v.get("remoteControlAtStartup").and_then(Value::as_bool))
        .unwrap_or(false)
}

fn set_rc_at_startup(claude_dir: &std::path::Path, on: bool) -> bool {
    let settings_path = claude_dir.join("settings.json");
    let (real, raw) = match std::fs::canonicalize(&settings_path) {
        Ok(canonical) => match std::fs::read_to_string(&canonical) {
            Ok(text) => (canonical, text),
            Err(_) => return false,
        },
        Err(err) if err.kind() == std::io::ErrorKind::NotFound => {
            (settings_path, String::new())
        }
        Err(_) => return false,
    };

    let mut doc = if raw.trim().is_empty() {
        json!({})
    } else {
        match serde_json::from_str::<Value>(&raw) {
            Ok(v) if v.is_object() => v,
            // Never overwrite an existing file with comments (JSONC) or invalid syntax.
            _ => return false,
        }
    };

    if let Some(obj) = doc.as_object_mut() {
        if on {
            obj.insert("remoteControlAtStartup".to_owned(), Value::Bool(true));
        } else {
            obj.remove("remoteControlAtStartup");
        }
    }

    let Ok(mut bytes) = serde_json::to_vec_pretty(&doc) else {
        return false;
    };
    bytes.push(b'\n');
    crate::atomic::write_bytes_atomic(&real, &bytes).is_ok()
}

pub async fn post_rc(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    OptionalPeer(peer): OptionalPeer,
    ViaTunnel(via_tunnel): ViaTunnel,
    Json(body): Json<Value>,
) -> Result<(StatusCode, Json<Value>), StatusCode> {
    require_local_post(peer, via_tunnel, &headers)?;
    let on = body.get("on").and_then(Value::as_bool).unwrap_or(false);
    let wrote = set_rc_at_startup(&state.claude_dir, on);
    let actual_rc = rc_at_startup(&state.claude_dir);
    if wrote {
        let _ = state
            .prefs
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .set(
                "rcAtStartup",
                if actual_rc { Some(Value::Bool(true)) } else { None },
            );
        Ok((StatusCode::OK, Json(json!({ "ok": true, "rc": actual_rc }))))
    } else {
        Ok((
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(json!({ "ok": false, "rc": actual_rc })),
        ))
    }
}

pub async fn post_open(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    OptionalPeer(peer): OptionalPeer,
    ViaTunnel(via_tunnel): ViaTunnel,
    Json(body): Json<Value>,
) -> Result<Json<Value>, StatusCode> {
    require_local_post(peer, via_tunnel, &headers)?;
    let id = body.get("id").and_then(Value::as_str).unwrap_or("");
    let cwd = {
        let office = state.office.lock().unwrap_or_else(|e| e.into_inner());
        office.sessions.get(id).map(|s| s.cwd.clone())
    };
    let Some(dir) = cwd.filter(|s| !s.is_empty()) else {
        return Ok(Json(json!({ "ok": false, "code": "gone" })));
    };
    let p = std::path::PathBuf::from(&dir);
    let res = tokio::task::spawn_blocking(move || crate::state::stranded::open_folder(&p))
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    match res {
        Ok(()) => Ok(Json(json!({ "ok": true, "path": dir }))),
        Err(msg) => Ok(Json(json!({ "ok": false, "message": msg }))),
    }
}

pub async fn post_panel_open(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    OptionalPeer(peer): OptionalPeer,
    ViaTunnel(via_tunnel): ViaTunnel,
    Json(body): Json<Value>,
) -> Result<Json<Value>, StatusCode> {
    require_local_post(peer, via_tunnel, &headers)?;
    let what = body.get("what").and_then(Value::as_str).unwrap_or("");
    let url = if what == "2d" {
        format!("http://127.0.0.1:{}/?view=2d", crate::DEFAULT_PORT)
    } else {
        format!("http://127.0.0.1:{}/", crate::DEFAULT_PORT)
    };
    state
        .office
        .lock()
        .unwrap_or_else(|e| e.into_inner())
        .broadcast(json!({ "k": "panel_open", "what": what, "url": url }));
    Ok(Json(json!({ "ok": true, "url": url })))
}

pub async fn post_todos(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    OptionalPeer(peer): OptionalPeer,
    ViaTunnel(via_tunnel): ViaTunnel,
    Json(body): Json<Value>,
) -> Result<Json<Value>, StatusCode> {
    require_local_post(peer, via_tunnel, &headers)?;
    let mut office = state.office.lock().unwrap_or_else(|e| e.into_inner());
    if let Some(arr) = body.get("todos").and_then(Value::as_array) {
        let _ = office.set_todos(arr.clone());
    }
    Ok(Json(json!({ "ok": true, "todos": office.todos })))
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
    let ag = Arc::clone(&state.ag_accounts);
    let res = tokio::task::spawn_blocking(move || {
        ag.lock()
            .unwrap_or_else(|e| e.into_inner())
            .mutate(&action, &body)
    })
    .await
    .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    Ok(Json(res))
}

pub async fn get_accounts(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    OptionalPeer(peer): OptionalPeer,
    ViaTunnel(via_tunnel): ViaTunnel,
    Query(q): Query<HashMap<String, String>>,
) -> Result<Json<Value>, StatusCode> {
    let local = require_auth(&state, peer, via_tunnel, &headers, &q)?;
    if !local {
        return Err(StatusCode::FORBIDDEN);
    }
    let mut view = state
        .claude_accounts
        .lock()
        .unwrap_or_else(|e| e.into_inner())
        .view();
    let codex_view = state
        .codex_accounts
        .lock()
        .unwrap_or_else(|e| e.into_inner())
        .view();
    let ag_view = state
        .ag_accounts
        .lock()
        .unwrap_or_else(|e| e.into_inner())
        .view();
    if let Some(obj) = view.as_object_mut() {
        obj.insert("codex".to_owned(), codex_view);
        obj.insert("ag".to_owned(), ag_view);
    }
    Ok(Json(view))
}

pub async fn post_accounts(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    OptionalPeer(peer): OptionalPeer,
    ViaTunnel(via_tunnel): ViaTunnel,
    Json(body): Json<Value>,
) -> Result<Json<Value>, StatusCode> {
    require_local_post(peer, via_tunnel, &headers)?;
    let res = state
        .claude_accounts
        .lock()
        .unwrap_or_else(|e| e.into_inner())
        .mutate("prepare", &body);
    Ok(Json(res))
}

pub async fn post_accounts_action(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    OptionalPeer(peer): OptionalPeer,
    ViaTunnel(via_tunnel): ViaTunnel,
    AxumPath(action): AxumPath<String>,
    Json(body): Json<Value>,
) -> Result<Json<Value>, StatusCode> {
    require_local_post(peer, via_tunnel, &headers)?;
    let res = state
        .claude_accounts
        .lock()
        .unwrap_or_else(|e| e.into_inner())
        .mutate(&action, &body);
    Ok(Json(res))
}

fn is_valid_tunnel_hostname(host: &str) -> bool {
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

/// Best-effort private RFC-1918 LAN IPv4 detection via OS routing table (zero network packets sent).
pub fn detect_private_lan_ipv4() -> Option<String> {
    let sock = std::net::UdpSocket::bind("0.0.0.0:0").ok()?;
    sock.connect("192.0.2.1:80").ok()?;
    match sock.local_addr().ok()?.ip() {
        IpAddr::V4(v4) if v4.is_private() => Some(v4.to_string()),
        _ => None,
    }
}

fn tunnel_status_json(state: &AppState) -> Value {
    let (on, domain, tasks, rc, lan_on, phone_approve) = {
        let prefs = state.prefs.lock().unwrap_or_else(|e| e.into_inner());
        let on = prefs
            .get("tunnel")
            .and_then(Value::as_bool)
            .unwrap_or(false);
        let domain = prefs
            .get("tunnelDomain")
            .and_then(Value::as_str)
            .unwrap_or("")
            .to_owned();
        let tasks = prefs
            .get("phoneTasks")
            .and_then(Value::as_bool)
            .unwrap_or(false);
        let rc = rc_at_startup(&state.claude_dir);
        let lan_on = prefs
            .get("lan")
            .and_then(Value::as_bool)
            .unwrap_or(false);
        let phone_approve = prefs
            .get("phoneApprove")
            .and_then(Value::as_bool)
            .unwrap_or(false);
        (on, domain, tasks, rc, lan_on, phone_approve)
    };
    let bg = state
        .tasks
        .lock()
        .unwrap_or_else(|e| e.into_inner())
        .bg_probe()
        .clone();
    let snap = state.tunnel.snapshot();
    let lan_ip = detect_private_lan_ipv4();
    json!({
        "ok": true,
        "on": on,
        "state": snap.state,
        "domain": domain,
        "fixed": domain,
        "url": snap.url,
        "host": snap.host,
        "port": snap.port,
        "error": snap.error,
        "pin": state.auth.current_pin(),
        "awake": Value::Null,
        "rc": rc,
        "tasks": tasks,
        "bg": bg,
        "lanOn": lan_on,
        "phoneApprove": phone_approve,
        "lanIp": lan_ip,
    })
}

pub async fn get_tunnel_status(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    OptionalPeer(peer): OptionalPeer,
    ViaTunnel(via_tunnel): ViaTunnel,
    Query(q): Query<HashMap<String, String>>,
) -> Result<Json<Value>, StatusCode> {
    let local = require_auth(&state, peer, via_tunnel, &headers, &q)?;
    if !local {
        return Err(StatusCode::FORBIDDEN);
    }
    Ok(Json(tunnel_status_json(&state)))
}

pub async fn post_tunnel_toggle(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    OptionalPeer(peer): OptionalPeer,
    ViaTunnel(via_tunnel): ViaTunnel,
    Json(body): Json<Value>,
) -> Result<Json<Value>, StatusCode> {
    require_local_post(peer, via_tunnel, &headers)?;
    let on = body.get("on").and_then(Value::as_bool).unwrap_or(false);
    {
        let mut prefs = state.prefs.lock().unwrap_or_else(|e| e.into_inner());
        let _ = prefs.set("tunnel", if on { Some(Value::Bool(true)) } else { None });
    }
    if on {
        let _ = state.tunnel.start(Arc::clone(&state)).await;
    } else {
        let _ = state.tunnel.stop();
    }
    Ok(Json(tunnel_status_json(&state)))
}

pub async fn post_tunnel_start(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    OptionalPeer(peer): OptionalPeer,
    ViaTunnel(via_tunnel): ViaTunnel,
) -> Result<Json<Value>, StatusCode> {
    require_local_post(peer, via_tunnel, &headers)?;
    {
        let mut prefs = state.prefs.lock().unwrap_or_else(|e| e.into_inner());
        let _ = prefs.set("tunnel", Some(Value::Bool(true)));
    }
    let _ = state.tunnel.start(Arc::clone(&state)).await;
    Ok(Json(tunnel_status_json(&state)))
}

pub async fn post_tunnel_stop(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    OptionalPeer(peer): OptionalPeer,
    ViaTunnel(via_tunnel): ViaTunnel,
) -> Result<Json<Value>, StatusCode> {
    require_local_post(peer, via_tunnel, &headers)?;
    {
        let mut prefs = state.prefs.lock().unwrap_or_else(|e| e.into_inner());
        let _ = prefs.set("tunnel", None);
    }
    let _ = state.tunnel.stop();
    Ok(Json(tunnel_status_json(&state)))
}

pub async fn post_tunnel_domain(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    OptionalPeer(peer): OptionalPeer,
    ViaTunnel(via_tunnel): ViaTunnel,
    Json(body): Json<Value>,
) -> Result<Json<Value>, StatusCode> {
    require_local_post(peer, via_tunnel, &headers)?;
    let raw = body
        .get("host")
        .or_else(|| body.get("domain"))
        .and_then(Value::as_str)
        .unwrap_or("")
        .trim()
        .to_ascii_lowercase();
    if !raw.is_empty() && !is_valid_tunnel_hostname(&raw) {
        return Err(StatusCode::BAD_REQUEST);
    }
    {
        let mut prefs = state.prefs.lock().unwrap_or_else(|e| e.into_inner());
        let _ = prefs.set(
            "tunnelDomain",
            if raw.is_empty() {
                None
            } else {
                Some(Value::String(raw))
            },
        );
    }
    Ok(Json(tunnel_status_json(&state)))
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

    #[tokio::test]
    async fn replay_timeline_and_wrapped_read_live_persisted_events() {
        use crate::state::sessions::now_ms;
        use axum::body::to_bytes;
        use serde_json::{Value, json};

        let dir = tempfile::tempdir().unwrap();
        let state = Arc::new(AppState::new(dir.path().to_path_buf(), None).unwrap());
        let app = build_router(state);

        let before = now_ms();
        for ev in [
            json!({
                "session_id": "s-hist",
                "hook_event_name": "UserPromptSubmit",
                "cwd": "D:/repo/hist-proj",
                "prompt": "ship feature"
            }),
            json!({
                "session_id": "s-hist",
                "hook_event_name": "PreToolUse",
                "tool_name": "Edit",
                "tool_input": { "file_path": "src/main.rs" }
            }),
            json!({
                "session_id": "s-hist",
                "hook_event_name": "Stop",
                "cwd": "D:/repo/hist-proj"
            }),
        ] {
            let req = Request::builder()
                .method("POST")
                .uri("/event")
                .header("host", "127.0.0.1:4310")
                .header("content-type", "application/json")
                .body(Body::from(ev.to_string()))
                .unwrap();
            assert_eq!(app.clone().oneshot(req).await.unwrap().status(), StatusCode::OK);
        }

        let today = crate::store::otel::day_key(before);
        let replay_req = Request::builder()
            .uri(format!("/api/replay?day={today}"))
            .header("host", "127.0.0.1:4310")
            .body(Body::empty())
            .unwrap();
        let replay_resp = app.clone().oneshot(replay_req).await.unwrap();
        assert_eq!(replay_resp.status(), StatusCode::OK);
        let replay_body: Value =
            serde_json::from_slice(&to_bytes(replay_resp.into_body(), 64 * 1024).await.unwrap())
                .unwrap();
        assert_eq!(replay_body["items"].as_array().unwrap().len(), 3);

        let tl_req = Request::builder()
            .uri("/api/timeline")
            .header("host", "127.0.0.1:4310")
            .body(Body::empty())
            .unwrap();
        let tl_resp = app.clone().oneshot(tl_req).await.unwrap();
        assert_eq!(tl_resp.status(), StatusCode::OK);
        let tl_body: Value =
            serde_json::from_slice(&to_bytes(tl_resp.into_body(), 64 * 1024).await.unwrap())
                .unwrap();
        assert!(
            !tl_body["rows"].as_array().unwrap().is_empty(),
            "/api/timeline must read replay history rows: {tl_body}"
        );
        assert_eq!(tl_body["totals"]["prompts"], 1);

        let wrapped_req = Request::builder()
            .uri("/api/wrapped")
            .header("host", "127.0.0.1:4310")
            .body(Body::empty())
            .unwrap();
        let wrapped_resp = app.oneshot(wrapped_req).await.unwrap();
        assert_eq!(wrapped_resp.status(), StatusCode::OK);
        let wrapped_body: Value =
            serde_json::from_slice(&to_bytes(wrapped_resp.into_body(), 64 * 1024).await.unwrap())
                .unwrap();
        assert_eq!(wrapped_body["days"], 1);
        assert_eq!(wrapped_body["perDay"][0]["prompts"], 1);
    }

    #[tokio::test]
    async fn post_perm_validates_decision_matches_hold_kind() {
        use crate::state::sessions::PendingPerm;
        use serde_json::json;

        let dir = tempfile::tempdir().unwrap();
        let state = Arc::new(AppState::new(dir.path().to_path_buf(), None).unwrap());
        let mk = |id: &str, kind: &str| PendingPerm {
            id: id.to_owned(),
            sess: "s1".to_owned(),
            name: "proj".to_owned(),
            kind: kind.to_owned(),
            tool_type: "bash".to_owned(),
            label: "cmd".to_owned(),
            detail: "echo hi".to_owned(),
            danger: None,
            created_at: 1,
        };
        {
            let mut office = state.office.lock().unwrap();
            office.pending_perms.insert("h-perm".into(), mk("h-perm", "perm"));
            office.pending_perms.insert("h-ask".into(), mk("h-ask", "ask"));
            office.pending_perms.insert("h-plan".into(), mk("h-plan", "plan"));
            office.pending_perms.insert("h-denied".into(), mk("h-denied", "denied"));
        }

        let app = build_router(state.clone());
        let mismatches = [
            ("h-perm", json!({ "id": "h-perm", "choice": "retry" })),
            ("h-perm", json!({ "id": "h-perm", "choice": "keep" })),
            ("h-perm", json!({ "id": "h-perm", "answers": { "Q": "A" } })),
            ("h-ask", json!({ "id": "h-ask", "choice": "allow" })),
            ("h-ask", json!({ "id": "h-ask", "choice": "retry" })),
            ("h-plan", json!({ "id": "h-plan", "answers": { "Q": "A" } })),
            ("h-plan", json!({ "id": "h-plan", "choice": "retry" })),
            ("h-denied", json!({ "id": "h-denied", "choice": "allow" })),
            ("h-denied", json!({ "id": "h-denied", "choice": "deny" })),
            ("h-denied", json!({ "id": "h-denied" })),
        ];
        for (id, body) in mismatches {
            let req = Request::builder()
                .method("POST")
                .uri("/api/perm")
                .header("host", "127.0.0.1:4310")
                .header("content-type", "application/json")
                .body(Body::from(body.to_string()))
                .unwrap();
            let resp = app.clone().oneshot(req).await.unwrap();
            assert_eq!(
                resp.status(),
                StatusCode::BAD_REQUEST,
                "expected 400 for mismatched decision {body}"
            );
            assert!(
                state.office.lock().unwrap().pending_perms.contains_key(id),
                "mismatched decision must not remove hold {id}"
            );
        }

        // Valid per-kind decisions succeed.
        for body in [
            json!({ "id": "h-denied", "choice": "retry" }),
            json!({ "id": "h-plan", "choice": "allow" }),
            json!({ "id": "h-ask", "choice": "deny", "reason": "skip" }),
            json!({ "id": "h-perm", "choice": "allow" }),
        ] {
            let req = Request::builder()
                .method("POST")
                .uri("/api/perm")
                .header("host", "127.0.0.1:4310")
                .header("content-type", "application/json")
                .body(Body::from(body.to_string()))
                .unwrap();
            assert_eq!(app.clone().oneshot(req).await.unwrap().status(), StatusCode::OK);
        }
        assert!(state.office.lock().unwrap().pending_perms.is_empty());
    }

    #[tokio::test]
    async fn get_image_serves_registered_image_with_sandbox_headers_and_rejects_traversal() {
        use axum::body::to_bytes;
        use serde_json::json;

        let dir = tempfile::tempdir().unwrap();
        let data_dir = dir.path().join("data");
        let work_dir = dir.path().join("work");
        std::fs::create_dir_all(&work_dir).unwrap();
        let png_path = work_dir.join("shot.png");
        let png_bytes: &[u8] = &[0x89, b'P', b'N', b'G', 0x0d, 0x0a, 0x1a, 0x0a, 1, 2, 3];
        std::fs::write(&png_path, png_bytes).unwrap();
        let secret_path = dir.path().join("secret.txt");
        std::fs::write(&secret_path, "top-secret").unwrap();

        let state = Arc::new(AppState::new(data_dir, None).unwrap());
        let app = build_router(state.clone());

        // Trigger Read event on an image file so ImageStore registers it and FeedItem exposes its id.
        let ev = json!({
            "session_id": "s-img",
            "hook_event_name": "PreToolUse",
            "cwd": work_dir.to_string_lossy(),
            "tool_name": "Read",
            "tool_input": { "file_path": png_path.to_string_lossy() }
        });
        let ev_req = Request::builder()
            .method("POST")
            .uri("/event")
            .header("host", "127.0.0.1:4310")
            .header("content-type", "application/json")
            .body(Body::from(ev.to_string()))
            .unwrap();
        assert_eq!(app.clone().oneshot(ev_req).await.unwrap().status(), StatusCode::OK);

        // Fetch /api/log to obtain the registered image id from the feed item.
        let log_req = Request::builder()
            .uri("/api/log")
            .header("host", "127.0.0.1:4310")
            .body(Body::empty())
            .unwrap();
        let log_resp = app.clone().oneshot(log_req).await.unwrap();
        let log_json: serde_json::Value =
            serde_json::from_slice(&to_bytes(log_resp.into_body(), 64 * 1024).await.unwrap())
                .unwrap();
        let img_id = log_json["feed"]
            .as_array()
            .unwrap()
            .iter()
            .find_map(|f| f.get("image").and_then(serde_json::Value::as_str))
            .expect("feed item for Read on .png must include registered image id")
            .to_owned();

        // Valid registered id -> 200 + sandbox CSP + nosniff + image/png + exact bytes
        let ok_req = Request::builder()
            .uri(format!("/api/image?id={img_id}"))
            .header("host", "127.0.0.1:4310")
            .body(Body::empty())
            .unwrap();
        let ok_resp = app.clone().oneshot(ok_req).await.unwrap();
        assert_eq!(ok_resp.status(), StatusCode::OK);
        assert_eq!(ok_resp.headers().get("content-security-policy").unwrap(), "sandbox");
        assert_eq!(ok_resp.headers().get("x-content-type-options").unwrap(), "nosniff");
        assert_eq!(ok_resp.headers().get("content-type").unwrap(), "image/png");
        let got_bytes = to_bytes(ok_resp.into_body(), 64 * 1024).await.unwrap();
        assert_eq!(got_bytes.as_ref(), png_bytes);

        // Path traversal / invalid ids -> 400 or 403 with sandbox + nosniff headers
        for bad_uri in [
            "/api/image?id=../../secret.txt",
            "/api/image?id=../shot.png",
            "/api/image?id=sub/shot.png",
            "/api/image?id=sub\\shot.png",
            "/api/image?id=..%2fsecret.txt",
        ] {
            let req = Request::builder()
                .uri(bad_uri)
                .header("host", "127.0.0.1:4310")
                .body(Body::empty())
                .unwrap();
            let resp = app.clone().oneshot(req).await.unwrap();
            assert!(
                matches!(resp.status(), StatusCode::BAD_REQUEST | StatusCode::FORBIDDEN),
                "expected 400/403 for {bad_uri}, got {}",
                resp.status()
            );
            assert_eq!(resp.headers().get("content-security-policy").unwrap(), "sandbox");
            assert_eq!(resp.headers().get("x-content-type-options").unwrap(), "nosniff");
        }
    }

    #[tokio::test]
    async fn post_event_populates_session_git_usage_cost_tokens_and_test_counts_in_api_live() {
        use axum::body::to_bytes;
        use serde_json::{Value, json};
        use std::process::Command;

        let dir = tempfile::tempdir().unwrap();
        let repo = dir.path().join("repo");
        std::fs::create_dir_all(&repo).unwrap();

        let git = |args: &[&str]| {
            let mut cmd = Command::new("git");
            cmd.args(args).current_dir(&repo);
            #[cfg(windows)]
            {
                use std::os::windows::process::CommandExt;
                cmd.creation_flags(0x0800_0000);
            }
            let out = cmd.output().expect("git must run");
            assert!(out.status.success(), "git {args:?} failed: {:?}", out);
        };
        git(&["init"]);
        git(&["config", "user.email", "test@example.com"]);
        git(&["config", "user.name", "Test"]);
        std::fs::write(repo.join("a.txt"), "v1\n").unwrap();
        git(&["add", "a.txt"]);
        git(&["commit", "-m", "init"]);
        // Dirty working tree
        std::fs::write(repo.join("a.txt"), "v2\n").unwrap();

        let transcript = dir.path().join("s-usage.jsonl");
        std::fs::write(
            &transcript,
            concat!(
                r#"{"type":"assistant","message":{"id":"m1","model":"claude-sonnet-5-5","usage":{"input_tokens":1000000,"output_tokens":200000,"cache_read_input_tokens":0}}}"#,
                "\n"
            ),
        )
        .unwrap();

        let state = Arc::new(AppState::new(dir.path().join("data"), None).unwrap());
        let app = build_router(state.clone());

        let ev1 = json!({
            "session_id": "s-usage",
            "hook_event_name": "UserPromptSubmit",
            "cwd": repo.to_string_lossy(),
            "transcript_path": transcript.to_string_lossy(),
            "prompt": "run tests"
        });
        let req1 = Request::builder()
            .method("POST")
            .uri("/event")
            .header("host", "127.0.0.1:4310")
            .header("content-type", "application/json")
            .body(Body::from(ev1.to_string()))
            .unwrap();
        assert_eq!(app.clone().oneshot(req1).await.unwrap().status(), StatusCode::OK);

        let ev2 = json!({
            "session_id": "s-usage",
            "hook_event_name": "PostToolUse",
            "cwd": repo.to_string_lossy(),
            "transcript_path": transcript.to_string_lossy(),
            "tool_name": "Bash",
            "tool_input": { "command": "cargo test -p station-core" },
            "tool_response": { "stdout": "test result: ok. 12 passed; 0 failed;" }
        });
        let req2 = Request::builder()
            .method("POST")
            .uri("/event")
            .header("host", "127.0.0.1:4310")
            .header("content-type", "application/json")
            .body(Body::from(ev2.to_string()))
            .unwrap();
        assert_eq!(app.clone().oneshot(req2).await.unwrap().status(), StatusCode::OK);

        let live_req = Request::builder()
            .uri("/api/live")
            .header("host", "127.0.0.1:4310")
            .body(Body::empty())
            .unwrap();
        let live_resp = app.clone().oneshot(live_req).await.unwrap();
        assert_eq!(live_resp.status(), StatusCode::OK);
        let live: Value =
            serde_json::from_slice(&to_bytes(live_resp.into_body(), 128 * 1024).await.unwrap())
                .unwrap();
        let sess = live["sessions"]
            .as_array()
            .unwrap()
            .iter()
            .find(|s| s["id"] == "s-usage")
            .expect("session s-usage must exist");

        assert!(
            !sess["git"]["branch"].as_str().unwrap_or("").is_empty(),
            "git branch must be populated: {sess}"
        );
        assert!(
            sess["git"]["dirty"].as_u64().unwrap_or(0) >= 1,
            "git dirty count must reflect modified file: {sess}"
        );
        let cost = sess["cost_usd"].as_f64().unwrap_or(0.0);
        assert!((cost - 4.0).abs() < 1e-4, "expected cost_usd ~ 4.0, got {cost} in {sess}");
        assert_eq!(sess["input_tokens"], 1_000_000);
        assert_eq!(sess["output_tokens"], 200_000);
        assert_eq!(sess["model"], "claude-sonnet-5-5");
        assert_eq!(sess["model_label"], "Sonnet 5.5");
        assert_eq!(sess["last_test"]["runner"], "cargo test");
        assert_eq!(sess["last_test"]["pass"], 12);
        assert_eq!(sess["last_test"]["fail"], 0);

        let git_req = Request::builder()
            .uri("/api/git")
            .header("host", "127.0.0.1:4310")
            .body(Body::empty())
            .unwrap();
        let git_resp = app.oneshot(git_req).await.unwrap();
        assert_eq!(git_resp.status(), StatusCode::OK);
    }

    #[tokio::test]
    async fn stranded_lists_worktrees_refuses_dirty_or_unknown_and_removes_clean_linked_worktree() {
        use axum::body::to_bytes;
        use serde_json::{Value, json};
        use std::process::Command;

        let dir = tempfile::tempdir().unwrap();
        let main_repo = dir.path().join("main-repo");
        let wt_clean = dir.path().join("wt-clean");
        let wt_dirty = dir.path().join("wt-dirty");
        std::fs::create_dir_all(&main_repo).unwrap();

        let git = |cwd: &std::path::Path, args: &[&str]| {
            let mut cmd = Command::new("git");
            cmd.args(args).current_dir(cwd);
            #[cfg(windows)]
            {
                use std::os::windows::process::CommandExt;
                cmd.creation_flags(0x0800_0000);
            }
            let out = cmd.output().expect("git must run");
            assert!(out.status.success(), "git {args:?} failed: {:?}", out);
        };
        git(&main_repo, &["init"]);
        git(&main_repo, &["config", "user.email", "test@example.com"]);
        git(&main_repo, &["config", "user.name", "Test"]);
        std::fs::write(main_repo.join("readme.md"), "hello\n").unwrap();
        git(&main_repo, &["add", "readme.md"]);
        git(&main_repo, &["commit", "-m", "init"]);
        git(&main_repo, &["worktree", "add", "-b", "clean-br", wt_clean.to_str().unwrap()]);
        git(&main_repo, &["worktree", "add", "-b", "dirty-br", wt_dirty.to_str().unwrap()]);
        std::fs::write(wt_dirty.join("uncommitted.txt"), "dirty\n").unwrap();

        let state = Arc::new(AppState::new(dir.path().join("data"), None).unwrap());
        state
            .office
            .lock()
            .unwrap()
            .note_dir(&main_repo.to_string_lossy());
        let app = build_router(state.clone());

        // 1. GET /api/stranded?fresh=1 lists both linked worktrees
        let req = Request::builder()
            .uri("/api/stranded?fresh=1")
            .header("host", "127.0.0.1:4310")
            .body(Body::empty())
            .unwrap();
        let resp = app.clone().oneshot(req).await.unwrap();
        assert_eq!(resp.status(), StatusCode::OK);
        let body: Value =
            serde_json::from_slice(&to_bytes(resp.into_body(), 128 * 1024).await.unwrap()).unwrap();
        assert_eq!(body["ok"], true);
        assert!(body["repos"].as_u64().unwrap_or(0) >= 1, "expected repos >= 1: {body}");
        let items = body["items"].as_array().unwrap();
        let clean_item = items
            .iter()
            .find(|i| i["branch"] == "clean-br")
            .expect("clean-br worktree must appear in /api/stranded");
        let dirty_item = items
            .iter()
            .find(|i| i["branch"] == "dirty-br")
            .expect("dirty-br worktree must appear in /api/stranded");
        assert_eq!(clean_item["removable"], true);
        assert_eq!(dirty_item["removable"], false);
        assert_eq!(dirty_item["work"], true);

        let clean_id = clean_item["id"].as_str().unwrap().to_owned();
        let dirty_id = dirty_item["id"].as_str().unwrap().to_owned();

        // 2. POST /api/stranded/open rejects unknown id (never stubs ok:true)
        let bad_open = Request::builder()
            .method("POST")
            .uri("/api/stranded/open")
            .header("host", "127.0.0.1:4310")
            .header("content-type", "application/json")
            .body(Body::from(json!({ "id": "unknown-id" }).to_string()))
            .unwrap();
        let open_resp = app.clone().oneshot(bad_open).await.unwrap();
        let open_body: Value =
            serde_json::from_slice(&to_bytes(open_resp.into_body(), 64 * 1024).await.unwrap())
                .unwrap();
        assert_eq!(open_body["ok"], false);

        // 3. POST /api/stranded/remove refuses dirty worktree
        let rm_dirty = Request::builder()
            .method("POST")
            .uri("/api/stranded/remove")
            .header("host", "127.0.0.1:4310")
            .header("content-type", "application/json")
            .body(Body::from(json!({ "id": dirty_id }).to_string()))
            .unwrap();
        let rm_dirty_resp = app.clone().oneshot(rm_dirty).await.unwrap();
        let rm_dirty_body: Value =
            serde_json::from_slice(&to_bytes(rm_dirty_resp.into_body(), 64 * 1024).await.unwrap())
                .unwrap();
        assert_eq!(rm_dirty_body["ok"], false);
        assert_eq!(rm_dirty_body["code"], "notClean");
        assert!(wt_dirty.exists(), "dirty worktree must not be deleted");

        // 4. POST /api/stranded/remove deletes clean linked worktree
        let rm_clean = Request::builder()
            .method("POST")
            .uri("/api/stranded/remove")
            .header("host", "127.0.0.1:4310")
            .header("content-type", "application/json")
            .body(Body::from(json!({ "id": clean_id }).to_string()))
            .unwrap();
        let rm_clean_resp = app.oneshot(rm_clean).await.unwrap();
        let rm_clean_body: Value =
            serde_json::from_slice(&to_bytes(rm_clean_resp.into_body(), 64 * 1024).await.unwrap())
                .unwrap();
        assert_eq!(rm_clean_body["ok"], true, "clean worktree must remove cleanly: {rm_clean_body}");
        assert!(!wt_clean.exists(), "clean worktree directory must be removed");
    }

    #[tokio::test]
    async fn tasks_spawn_never_wedges_at_three_and_wires_allow_bg_stop_steer_and_ci() {
        use crate::state::sessions::now_ms;
        use axum::body::to_bytes;
        use serde_json::{Value, json};
        use std::time::Duration;

        let dir = tempfile::tempdir().unwrap();
        let work_dir = dir.path().join("proj");
        std::fs::create_dir_all(&work_dir).unwrap();

        let state = Arc::new(AppState::new(dir.path().join("data"), None).unwrap());
        let app = build_router(state.clone());

        // 1. POST /api/tasks/allow toggles phoneTasks pref (a no-op stub would not persist it!)
        let allow_req = Request::builder()
            .method("POST")
            .uri("/api/tasks/allow")
            .header("host", "127.0.0.1:4310")
            .header("content-type", "application/json")
            .body(Body::from(json!({ "on": true }).to_string()))
            .unwrap();
        let allow_resp = app.clone().oneshot(allow_req).await.unwrap();
        assert_eq!(allow_resp.status(), StatusCode::OK);
        assert_eq!(
            state.prefs.lock().unwrap().get("phoneTasks").and_then(Value::as_bool),
            Some(true),
            "/api/tasks/allow must persist phoneTasks pref"
        );

        // 2. POST /api/tasks/bg toggles phoneTasksBg pref
        let bg_req = Request::builder()
            .method("POST")
            .uri("/api/tasks/bg")
            .header("host", "127.0.0.1:4310")
            .header("content-type", "application/json")
            .body(Body::from(json!({ "on": true }).to_string()))
            .unwrap();
        let bg_resp = app.clone().oneshot(bg_req).await.unwrap();
        assert_eq!(bg_resp.status(), StatusCode::OK);
        assert_eq!(
            state.prefs.lock().unwrap().get("phoneTasksBg").and_then(Value::as_bool),
            Some(true),
            "/api/tasks/bg must persist phoneTasksBg pref"
        );

        // 3. Configure fast exiting CLI command override and run 4 sequential tasks (must NOT wedge at 3!)
        #[cfg(windows)]
        let fast_cmd = || crate::state::tasks::TaskCommandSpec {
            program: std::path::PathBuf::from("cmd.exe"),
            args: vec!["/C".into(), "echo".into(), "done".into()],
            stdin_payload: None,
        };
        #[cfg(not(windows))]
        let fast_cmd = || crate::state::tasks::TaskCommandSpec {
            program: std::path::PathBuf::from("sh"),
            args: vec!["-c".into(), "echo done".into()],
            stdin_payload: None,
        };
        state
            .tasks
            .lock()
            .unwrap()
            .set_command_override(Arc::new(move |_req, _id| Some(fast_cmd())));

        for i in 0..4 {
            let req = Request::builder()
                .method("POST")
                .uri("/api/tasks")
                .header("host", "127.0.0.1:4310")
                .header("content-type", "application/json")
                .body(Body::from(
                    json!({
                        "dir": work_dir.to_string_lossy(),
                        "text": format!("task #{i}"),
                        "agent": "claude"
                    })
                    .to_string(),
                ))
                .unwrap();
            let resp = app.clone().oneshot(req).await.unwrap();
            assert_eq!(resp.status(), StatusCode::OK);
            let body: Value =
                serde_json::from_slice(&to_bytes(resp.into_body(), 64 * 1024).await.unwrap())
                    .unwrap();
            assert_eq!(body["ok"], true, "task #{i} must start without wedging: {body}");
            // Wait up to 2s for child command to exit and free slot
            let deadline = tokio::time::Instant::now() + Duration::from_secs(2);
            while !state.tasks.lock().unwrap().list().is_empty() {
                assert!(
                    tokio::time::Instant::now() < deadline,
                    "task #{i} never freed its running slot"
                );
                tokio::time::sleep(Duration::from_millis(15)).await;
            }
        }

        // 4. POST /api/steer rejects empty text (400), rejects idle session (409), queues busy session without inbox (200 how="later")
        let empty_steer = Request::builder()
            .method("POST")
            .uri("/api/steer")
            .header("host", "127.0.0.1:4310")
            .header("content-type", "application/json")
            .body(Body::from(json!({ "sid": "s-steer", "text": "   " }).to_string()))
            .unwrap();
        assert_eq!(app.clone().oneshot(empty_steer).await.unwrap().status(), StatusCode::BAD_REQUEST);

        // Start a working session s-steer
        let start_ev = Request::builder()
            .method("POST")
            .uri("/event")
            .header("host", "127.0.0.1:4310")
            .header("content-type", "application/json")
            .body(Body::from(
                json!({
                    "session_id": "s-steer",
                    "hook_event_name": "UserPromptSubmit",
                    "cwd": work_dir.to_string_lossy(),
                    "prompt": "working"
                })
                .to_string(),
            ))
            .unwrap();
        assert_eq!(app.clone().oneshot(start_ev).await.unwrap().status(), StatusCode::OK);

        let steer_req = Request::builder()
            .method("POST")
            .uri("/api/steer")
            .header("host", "127.0.0.1:4310")
            .header("content-type", "application/json")
            .body(Body::from(json!({ "sid": "s-steer", "text": "fix tests too" }).to_string()))
            .unwrap();
        let steer_resp = app.clone().oneshot(steer_req).await.unwrap();
        assert_eq!(steer_resp.status(), StatusCode::OK);
        let steer_body: Value =
            serde_json::from_slice(&to_bytes(steer_resp.into_body(), 64 * 1024).await.unwrap())
                .unwrap();
        assert_eq!(steer_body["ok"], true);
        assert_eq!(steer_body["how"], "later");
        assert_eq!(steer_body["why"], "noInbox");

        // 5. POST /api/ci returns 404 for unknown card, and 200 how="sent"|"note" for a red CI card
        let bad_ci = Request::builder()
            .method("POST")
            .uri("/api/ci")
            .header("host", "127.0.0.1:4310")
            .header("content-type", "application/json")
            .body(Body::from(json!({ "id": "missing-ci" }).to_string()))
            .unwrap();
        assert_eq!(app.clone().oneshot(bad_ci).await.unwrap().status(), StatusCode::NOT_FOUND);

        let ci_card_id = {
            let mut office = state.office.lock().unwrap();
            office
                .chat
                .ci_card(
                    "s-steer",
                    "proj",
                    json!({
                        "repo": "proj",
                        "branch": "main",
                        "wf": "CI",
                        "url": "https://example.test/runs/1",
                        "sess": "s-steer",
                        "sid": "s-steer",
                        "dir": work_dir.to_string_lossy(),
                        "state": "red",
                        "fix": null
                    }),
                    now_ms(),
                )
                .id
        };
        let ok_ci = Request::builder()
            .method("POST")
            .uri("/api/ci")
            .header("host", "127.0.0.1:4310")
            .header("content-type", "application/json")
            .body(Body::from(json!({ "id": ci_card_id }).to_string()))
            .unwrap();
        let ok_ci_resp = app.oneshot(ok_ci).await.unwrap();
        assert_eq!(ok_ci_resp.status(), StatusCode::OK);
        let ok_ci_body: Value =
            serde_json::from_slice(&to_bytes(ok_ci_resp.into_body(), 64 * 1024).await.unwrap())
                .unwrap();
        assert_eq!(ok_ci_body["ok"], true);
        assert!(matches!(ok_ci_body["how"].as_str(), Some("note" | "sent")));
    }

    #[tokio::test]
    async fn post_prompt_rewrite_uses_real_transport_redacts_secrets_and_enforces_codes() {
        use axum::body::to_bytes;
        use serde_json::{Value, json};
        use std::sync::Mutex;

        let dir = tempfile::tempdir().unwrap();
        let state = Arc::new(AppState::new(dir.path().to_path_buf(), None).unwrap());
        let seen: Arc<Mutex<Vec<String>>> = Arc::new(Mutex::new(Vec::new()));
        let seen_cb = Arc::clone(&seen);
        state
            .rewriter
            .lock()
            .unwrap()
            .set_transport(Arc::new(move |redacted, _sys, _photos| {
                seen_cb.lock().unwrap().push(redacted.to_owned());
                Ok(format!(
                    "Goal: Rewrite via transport ({redacted})\nContext: repo\nDone when: tests pass"
                ))
            }));
        let app = build_router(state.clone());

        // 1. Valid rewrite invokes real transport with secret already redacted
        let req = Request::builder()
            .method("POST")
            .uri("/api/prompt/rewrite")
            .header("host", "127.0.0.1:4310")
            .header("content-type", "application/json")
            .body(Body::from(
                json!({
                    "text": "fix bug using sk-ant-api03-secret9999",
                    "lang": "en",
                    "photos": 1
                })
                .to_string(),
            ))
            .unwrap();
        let resp = app.clone().oneshot(req).await.unwrap();
        assert_eq!(resp.status(), StatusCode::OK);
        let body: Value =
            serde_json::from_slice(&to_bytes(resp.into_body(), 64 * 1024).await.unwrap()).unwrap();
        assert_eq!(body["ok"], true);
        let text = body["text"].as_str().unwrap();
        assert!(text.contains("Rewrite via transport"), "must use real transport, got: {text}");
        assert!(!text.contains("sk-ant-api03-secret9999"));
        let captured = seen.lock().unwrap();
        assert_eq!(captured.len(), 1);
        assert!(!captured[0].contains("sk-ant-api03-secret9999"));

        // 2. Empty prompt -> 409 with code="empty"
        let empty_req = Request::builder()
            .method("POST")
            .uri("/api/prompt/rewrite")
            .header("host", "127.0.0.1:4310")
            .header("content-type", "application/json")
            .body(Body::from(json!({ "text": "   " }).to_string()))
            .unwrap();
        let empty_resp = app.oneshot(empty_req).await.unwrap();
        assert_eq!(empty_resp.status(), StatusCode::CONFLICT);
        let empty_body: Value =
            serde_json::from_slice(&to_bytes(empty_resp.into_body(), 64 * 1024).await.unwrap())
                .unwrap();
        assert_eq!(empty_body["ok"], false);
        assert_eq!(empty_body["code"], "empty");
    }

    #[tokio::test]
    async fn post_chat_supports_all_ops_broadcasts_sse_and_rejects_unknown_op() {
        use axum::body::to_bytes;
        use serde_json::{Value, json};

        let dir = tempfile::tempdir().unwrap();
        let state = Arc::new(AppState::new(dir.path().to_path_buf(), None).unwrap());
        let mut rx = state.office.lock().unwrap().subscribe();
        let app = build_router(state.clone());

        let post = |body: Value| {
            Request::builder()
                .method("POST")
                .uri("/api/chat")
                .header("host", "127.0.0.1:4310")
                .header("content-type", "application/json")
                .body(Body::from(body.to_string()))
                .unwrap()
        };

        // 1. Unknown op -> 400
        let bad = app.clone().oneshot(post(json!({ "op": "bogus" }))).await.unwrap();
        assert_eq!(bad.status(), StatusCode::BAD_REQUEST);

        // 2. send + SSE broadcast { k: "chat" }
        let r1 = app
            .clone()
            .oneshot(post(json!({ "op": "send", "text": "hello world" })))
            .await
            .unwrap();
        assert_eq!(r1.status(), StatusCode::OK);
        let b1: Value =
            serde_json::from_slice(&to_bytes(r1.into_body(), 64 * 1024).await.unwrap()).unwrap();
        let msg_id = b1["msg"]["id"].as_str().unwrap().to_owned();
        let sse_ev = rx.try_recv().expect("POST /api/chat must broadcast SSE event");
        assert_eq!(sse_ev["k"], "chat");

        // 3. send with replyTo + sticker
        let r2 = app
            .clone()
            .oneshot(post(json!({ "op": "send", "text": "replying", "replyTo": msg_id, "sticker": 3 })))
            .await
            .unwrap();
        assert_eq!(r2.status(), StatusCode::OK);
        let b2: Value =
            serde_json::from_slice(&to_bytes(r2.into_body(), 64 * 1024).await.unwrap()).unwrap();
        assert_eq!(b2["msg"]["sticker"], 3);
        assert_eq!(b2["msg"]["reply"]["id"], msg_id);

        // 4. poll + vote + pin + react + photo + say + rsvp + recall
        let r_poll = app
            .clone()
            .oneshot(post(json!({ "op": "poll", "q": "Ship?", "opts": ["Yes", "No"] })))
            .await
            .unwrap();
        assert_eq!(r_poll.status(), StatusCode::OK);
        let b_poll: Value =
            serde_json::from_slice(&to_bytes(r_poll.into_body(), 64 * 1024).await.unwrap()).unwrap();
        let poll_id = b_poll["msg"]["id"].as_str().unwrap().to_owned();
        assert_eq!(
            app.clone()
                .oneshot(post(json!({ "op": "vote", "id": poll_id, "i": 0 })))
                .await
                .unwrap()
                .status(),
            StatusCode::OK
        );
        assert_eq!(
            app.clone()
                .oneshot(post(json!({ "op": "photo", "data": "data:image/jpeg;base64,/9j/4AAQ" })))
                .await
                .unwrap()
                .status(),
            StatusCode::OK
        );
        assert_eq!(
            app.clone()
                .oneshot(post(json!({ "op": "say", "from": "reception", "text": "Welcome!" })))
                .await
                .unwrap()
                .status(),
            StatusCode::OK
        );
        assert_eq!(
            app.clone()
                .oneshot(post(json!({ "op": "rsvp", "id": msg_id, "yes": true })))
                .await
                .unwrap()
                .status(),
            StatusCode::OK
        );
    }

    #[tokio::test]
    async fn post_undo_supports_selective_files_and_tell_op() {
        use axum::body::to_bytes;
        use serde_json::{Value, json};

        let dir = tempfile::tempdir().unwrap();
        let ws = dir.path().join("ws");
        std::fs::create_dir_all(&ws).unwrap();
        let fa = ws.join("a.txt");
        let fb = ws.join("b.txt");
        std::fs::write(&fa, "orig-a").unwrap();
        std::fs::write(&fb, "orig-b").unwrap();

        let state = Arc::new(AppState::new(dir.path().join("data"), None).unwrap());
        #[cfg(windows)]
        let fast_cmd = || crate::state::tasks::TaskCommandSpec {
            program: std::path::PathBuf::from("cmd.exe"),
            args: vec!["/C".into(), "echo".into(), "ok".into()],
            stdin_payload: None,
        };
        #[cfg(not(windows))]
        let fast_cmd = || crate::state::tasks::TaskCommandSpec {
            program: std::path::PathBuf::from("sh"),
            args: vec!["-c".into(), "echo ok".into()],
            stdin_payload: None,
        };
        state
            .tasks
            .lock()
            .unwrap()
            .set_command_override(Arc::new(move |_req, _id| Some(fast_cmd())));
        let app = build_router(state.clone());

        for ev in [
            json!({ "session_id": "s-undo", "hook_event_name": "UserPromptSubmit", "cwd": ws.to_string_lossy(), "prompt": "edit two files" }),
            json!({ "session_id": "s-undo", "hook_event_name": "PreToolUse", "cwd": ws.to_string_lossy(), "tool_name": "Edit", "tool_input": { "file_path": fa.to_string_lossy() } }),
            json!({ "session_id": "s-undo", "hook_event_name": "PreToolUse", "cwd": ws.to_string_lossy(), "tool_name": "Edit", "tool_input": { "file_path": fb.to_string_lossy() } }),
        ] {
            let req = Request::builder()
                .method("POST")
                .uri("/event")
                .header("host", "127.0.0.1:4310")
                .header("content-type", "application/json")
                .body(Body::from(ev.to_string()))
                .unwrap();
            assert_eq!(app.clone().oneshot(req).await.unwrap().status(), StatusCode::OK);
        }
        std::fs::write(&fa, "new-a").unwrap();
        std::fs::write(&fb, "new-b").unwrap();
        let stop_req = Request::builder()
            .method("POST")
            .uri("/event")
            .header("host", "127.0.0.1:4310")
            .header("content-type", "application/json")
            .body(Body::from(
                json!({ "session_id": "s-undo", "hook_event_name": "Stop", "cwd": ws.to_string_lossy() }).to_string(),
            ))
            .unwrap();
        assert_eq!(app.clone().oneshot(stop_req).await.unwrap().status(), StatusCode::OK);

        // Selective undo: restore ONLY a.txt, leave b.txt modified!
        let canon_a = std::fs::canonicalize(&fa).unwrap();
        let undo_req = Request::builder()
            .method("POST")
            .uri("/api/undo")
            .header("host", "127.0.0.1:4310")
            .header("content-type", "application/json")
            .body(Body::from(
                json!({ "id": "s-undo", "files": [canon_a.to_string_lossy()] }).to_string(),
            ))
            .unwrap();
        let undo_resp = app.clone().oneshot(undo_req).await.unwrap();
        assert_eq!(undo_resp.status(), StatusCode::OK);
        let undo_body: Value =
            serde_json::from_slice(&to_bytes(undo_resp.into_body(), 64 * 1024).await.unwrap())
                .unwrap();
        assert_eq!(undo_body["ok"], true);
        assert_eq!(std::fs::read_to_string(&fa).unwrap(), "orig-a");
        assert_eq!(std::fs::read_to_string(&fb).unwrap(), "new-b");

        // op: "tell" notifies session
        let tell_req = Request::builder()
            .method("POST")
            .uri("/api/undo")
            .header("host", "127.0.0.1:4310")
            .header("content-type", "application/json")
            .body(Body::from(
                json!({ "id": "s-undo", "op": "tell", "files": "a.txt" }).to_string(),
            ))
            .unwrap();
        let tell_resp = app.oneshot(tell_req).await.unwrap();
        assert_eq!(tell_resp.status(), StatusCode::OK);
    }

    #[tokio::test]
    async fn cache_quick_macapprove_phone_lan_rc_and_replay_range_work() {
        use axum::body::to_bytes;
        use serde_json::{Value, json};

        let dir = tempfile::tempdir().unwrap();
        let state = Arc::new(AppState::new(dir.path().to_path_buf(), None).unwrap());
        let app = build_router(state.clone());

        // /api/cache
        let c_post = Request::builder()
            .method("POST")
            .uri("/api/cache")
            .header("host", "127.0.0.1:4310")
            .header("content-type", "application/json")
            .body(Body::from(json!({ "nudge": true }).to_string()))
            .unwrap();
        assert_eq!(app.clone().oneshot(c_post).await.unwrap().status(), StatusCode::OK);
        let c_get = Request::builder()
            .uri("/api/cache")
            .header("host", "127.0.0.1:4310")
            .body(Body::empty())
            .unwrap();
        let c_body: Value = serde_json::from_slice(
            &to_bytes(app.clone().oneshot(c_get).await.unwrap().into_body(), 64 * 1024)
                .await
                .unwrap(),
        )
        .unwrap();
        assert_eq!(c_body["nudge"], true);

        // /api/quick
        let q_post = Request::builder()
            .method("POST")
            .uri("/api/quick")
            .header("host", "127.0.0.1:4310")
            .header("content-type", "application/json")
            .body(Body::from(
                json!({ "list": [{ "label": "Test", "prompt": "run cargo test" }] }).to_string(),
            ))
            .unwrap();
        assert_eq!(app.clone().oneshot(q_post).await.unwrap().status(), StatusCode::OK);
        let q_get = Request::builder()
            .uri("/api/quick")
            .header("host", "127.0.0.1:4310")
            .body(Body::empty())
            .unwrap();
        let q_body: Value = serde_json::from_slice(
            &to_bytes(app.clone().oneshot(q_get).await.unwrap().into_body(), 64 * 1024)
                .await
                .unwrap(),
        )
        .unwrap();
        assert_eq!(q_body["list"].as_array().unwrap().len(), 1);
        assert_eq!(q_body["list"][0]["label"], "Test");

        // /api/macapprove
        let m_post = Request::builder()
            .method("POST")
            .uri("/api/macapprove")
            .header("host", "127.0.0.1:4310")
            .header("content-type", "application/json")
            .body(Body::from(json!({ "on": false }).to_string()))
            .unwrap();
        assert_eq!(app.clone().oneshot(m_post).await.unwrap().status(), StatusCode::OK);
        let m_get = Request::builder()
            .uri("/api/macapprove")
            .header("host", "127.0.0.1:4310")
            .body(Body::empty())
            .unwrap();
        let m_body: Value = serde_json::from_slice(
            &to_bytes(app.clone().oneshot(m_get).await.unwrap().into_body(), 64 * 1024)
                .await
                .unwrap(),
        )
        .unwrap();
        assert_eq!(m_body["on"], false);

        // /api/phone, /api/lan, /api/rc persist prefs
        for (uri, payload, key) in [
            ("/api/phone", json!({ "approve": true }), "phoneApprove"),
            ("/api/lan", json!({ "on": true }), "lan"),
            ("/api/rc", json!({ "on": true }), "rcAtStartup"),
        ] {
            let req = Request::builder()
                .method("POST")
                .uri(uri)
                .header("host", "127.0.0.1:4310")
                .header("content-type", "application/json")
                .body(Body::from(payload.to_string()))
                .unwrap();
            assert_eq!(app.clone().oneshot(req).await.unwrap().status(), StatusCode::OK);
            assert_eq!(
                state.prefs.lock().unwrap().get(key).and_then(Value::as_bool),
                Some(true),
                "{uri} must persist pref {key}"
            );
        }

        // /api/replay?day=...&from=...&to=... filters by timestamp
        let replay_dir = dir.path().join("replay");
        std::fs::create_dir_all(&replay_dir).unwrap();
        std::fs::write(
            replay_dir.join("2026-11-25.jsonl"),
            "{\"t\":1000,\"m\":{\"k\":\"ev\"}}\n{\"t\":5000,\"m\":{\"k\":\"ev\"}}\n",
        )
        .unwrap();
        let rep_req = Request::builder()
            .uri("/api/replay?day=2026-11-25&from=2000&to=6000")
            .header("host", "127.0.0.1:4310")
            .body(Body::empty())
            .unwrap();
        let rep_body: Value = serde_json::from_slice(
            &to_bytes(app.clone().oneshot(rep_req).await.unwrap().into_body(), 64 * 1024)
                .await
                .unwrap(),
        )
        .unwrap();
        assert_eq!(rep_body["items"].as_array().unwrap().len(), 1);
        assert_eq!(rep_body["items"][0]["t"], 5000);

        let days_req = Request::builder()
            .uri("/api/replay/days")
            .header("host", "127.0.0.1:4310")
            .body(Body::empty())
            .unwrap();
        let days_body: Value = serde_json::from_slice(
            &to_bytes(app.oneshot(days_req).await.unwrap().into_body(), 64 * 1024)
                .await
                .unwrap(),
        )
        .unwrap();
        assert!(days_body["today"].as_str().is_some_and(|s| s.len() == 10));
    }

    #[tokio::test]
    async fn post_tasks_enforces_remote_dir_allowlist_stops_inflight_child_and_broadcasts_done() {
        use crate::auth::COOKIE_NAME;
        use axum::body::to_bytes;
        use axum::extract::ConnectInfo;
        use serde_json::{Value, json};
        use std::net::{IpAddr, Ipv4Addr, SocketAddr};
        use std::time::Duration;

        let dir = tempfile::tempdir().unwrap();
        let allowed_dir = dir.path().join("allowed");
        let secret_dir = dir.path().join("secret");
        std::fs::create_dir_all(&allowed_dir).unwrap();
        std::fs::create_dir_all(&secret_dir).unwrap();

        let state = Arc::new(AppState::new(dir.path().join("data"), None).unwrap());
        state
            .prefs
            .lock()
            .unwrap()
            .set("phoneTasks", Some(Value::Bool(true)))
            .unwrap();
        state
            .office
            .lock()
            .unwrap()
            .note_dir(&allowed_dir.to_string_lossy());
        let cookie = format!("{COOKIE_NAME}={}", state.auth.sign_cookie(false));
        let lan_peer = SocketAddr::new(IpAddr::V4(Ipv4Addr::new(192, 168, 1, 50)), 52100);

        #[cfg(windows)]
        let slow_cmd = || crate::state::tasks::TaskCommandSpec {
            program: std::path::PathBuf::from("ping.exe"),
            args: vec!["-n".into(), "20".into(), "127.0.0.1".into()],
            stdin_payload: None,
        };
        #[cfg(not(windows))]
        let slow_cmd = || crate::state::tasks::TaskCommandSpec {
            program: std::path::PathBuf::from("sleep"),
            args: vec!["20".into()],
            stdin_payload: None,
        };
        state
            .tasks
            .lock()
            .unwrap()
            .set_command_override(Arc::new(move |_req, _id| Some(slow_cmd())));

        let app = build_router(state.clone());

        // 1. Remote caller targeting unknown directory -> 403 Forbidden
        let mut bad_remote = Request::builder()
            .method("POST")
            .uri("/api/tasks")
            .header("host", "192.168.1.20:4310")
            .header("cookie", &cookie)
            .header("content-type", "application/json")
            .body(Body::from(
                json!({ "dir": secret_dir.to_string_lossy(), "text": "pwn", "agent": "claude" })
                    .to_string(),
            ))
            .unwrap();
        bad_remote.extensions_mut().insert(ConnectInfo(lan_peer));
        assert_eq!(
            app.clone().oneshot(bad_remote).await.unwrap().status(),
            StatusCode::FORBIDDEN
        );

        // 2. Remote caller targeting known project dir -> 200 OK, then POST /api/tasks/stop kills & frees slot
        let mut ok_remote = Request::builder()
            .method("POST")
            .uri("/api/tasks")
            .header("host", "192.168.1.20:4310")
            .header("cookie", &cookie)
            .header("content-type", "application/json")
            .body(Body::from(
                json!({ "dir": allowed_dir.to_string_lossy(), "text": "run", "agent": "claude" })
                    .to_string(),
            ))
            .unwrap();
        ok_remote.extensions_mut().insert(ConnectInfo(lan_peer));
        let start_resp = app.clone().oneshot(ok_remote).await.unwrap();
        assert_eq!(start_resp.status(), StatusCode::OK);
        let start_body: Value =
            serde_json::from_slice(&to_bytes(start_resp.into_body(), 64 * 1024).await.unwrap())
                .unwrap();
        let tid = start_body["task"]["id"].as_str().unwrap().to_owned();
        assert_eq!(state.tasks.lock().unwrap().list().len(), 1);

        let stop_req = Request::builder()
            .method("POST")
            .uri("/api/tasks/stop")
            .header("host", "127.0.0.1:4310")
            .header("content-type", "application/json")
            .body(Body::from(json!({ "id": tid }).to_string()))
            .unwrap();
        let stop_resp = app.clone().oneshot(stop_req).await.unwrap();
        assert_eq!(stop_resp.status(), StatusCode::OK);
        let stop_body: Value =
            serde_json::from_slice(&to_bytes(stop_resp.into_body(), 64 * 1024).await.unwrap())
                .unwrap();
        assert_eq!(stop_body["ok"], true);
        assert!(
            state.tasks.lock().unwrap().list().is_empty(),
            "/api/tasks/stop must immediately release running slot"
        );

        // 3. Fast task broadcasts SSE completion update with state="done"
        #[cfg(windows)]
        let fast_cmd = || crate::state::tasks::TaskCommandSpec {
            program: std::path::PathBuf::from("cmd.exe"),
            args: vec!["/C".into(), "echo".into(), "finished-ok".into()],
            stdin_payload: None,
        };
        #[cfg(not(windows))]
        let fast_cmd = || crate::state::tasks::TaskCommandSpec {
            program: std::path::PathBuf::from("sh"),
            args: vec!["-c".into(), "echo finished-ok".into()],
            stdin_payload: None,
        };
        state
            .tasks
            .lock()
            .unwrap()
            .set_command_override(Arc::new(move |_req, _id| Some(fast_cmd())));
        let mut rx = state.office.lock().unwrap().subscribe();
        let run_req = Request::builder()
            .method("POST")
            .uri("/api/tasks")
            .header("host", "127.0.0.1:4310")
            .header("content-type", "application/json")
            .body(Body::from(
                json!({ "dir": allowed_dir.to_string_lossy(), "text": "quick", "agent": "claude" })
                    .to_string(),
            ))
            .unwrap();
        assert_eq!(app.oneshot(run_req).await.unwrap().status(), StatusCode::OK);

        let mut saw_done = false;
        let deadline = tokio::time::Instant::now() + Duration::from_secs(3);
        while tokio::time::Instant::now() < deadline && !saw_done {
            if let Ok(Ok(ev)) = tokio::time::timeout(Duration::from_millis(200), rx.recv()).await {
                if ev["k"] == "chat"
                    && ev["op"] == "work"
                    && ev["m"]["work"]["state"] == "done"
                {
                    saw_done = true;
                }
            }
        }
        assert!(saw_done, "task completion must broadcast SSE work card update with state=done");
    }

    #[tokio::test]
    async fn stop_drains_queued_steer_and_stop_failure_triggers_auto_resume() {
        use serde_json::{Value, json};
        use std::time::Duration;

        let dir = tempfile::tempdir().unwrap();
        let ws = dir.path().join("ws");
        std::fs::create_dir_all(&ws).unwrap();
        let sid = "12345678-1234-1234-1234-1234567890ab";

        let state = Arc::new(AppState::new(dir.path().join("data"), None).unwrap());
        #[cfg(windows)]
        let fast_cmd = || crate::state::tasks::TaskCommandSpec {
            program: std::path::PathBuf::from("cmd.exe"),
            args: vec!["/C".into(), "echo".into(), "resumed".into()],
            stdin_payload: None,
        };
        #[cfg(not(windows))]
        let fast_cmd = || crate::state::tasks::TaskCommandSpec {
            program: std::path::PathBuf::from("sh"),
            args: vec!["-c".into(), "echo resumed".into()],
            stdin_payload: None,
        };
        state
            .tasks
            .lock()
            .unwrap()
            .set_command_override(Arc::new(move |_req, _id| Some(fast_cmd())));
        state
            .prefs
            .lock()
            .unwrap()
            .set("autoResume", Some(Value::Bool(true)))
            .unwrap();

        let app = build_router(state.clone());

        // 1. Start working session and queue a steer note
        let start = Request::builder()
            .method("POST")
            .uri("/event")
            .header("host", "127.0.0.1:4310")
            .header("content-type", "application/json")
            .body(Body::from(
                json!({
                    "session_id": sid,
                    "hook_event_name": "UserPromptSubmit",
                    "cwd": ws.to_string_lossy(),
                    "prompt": "do work"
                })
                .to_string(),
            ))
            .unwrap();
        assert_eq!(app.clone().oneshot(start).await.unwrap().status(), StatusCode::OK);

        let steer = Request::builder()
            .method("POST")
            .uri("/api/steer")
            .header("host", "127.0.0.1:4310")
            .header("content-type", "application/json")
            .body(Body::from(json!({ "sid": sid, "text": "also lint" }).to_string()))
            .unwrap();
        assert_eq!(app.clone().oneshot(steer).await.unwrap().status(), StatusCode::OK);
        assert_eq!(state.steer_queue.lock().unwrap().len(), 1);

        // 2. Stop drains SteerQueue and broadcasts steerDrain
        let mut rx = state.office.lock().unwrap().subscribe();
        let stop = Request::builder()
            .method("POST")
            .uri("/event")
            .header("host", "127.0.0.1:4310")
            .header("content-type", "application/json")
            .body(Body::from(
                json!({
                    "session_id": sid,
                    "hook_event_name": "Stop",
                    "cwd": ws.to_string_lossy()
                })
                .to_string(),
            ))
            .unwrap();
        assert_eq!(app.clone().oneshot(stop).await.unwrap().status(), StatusCode::OK);
        assert!(
            state.steer_queue.lock().unwrap().is_empty(),
            "Stop event must drain queued steer for the session"
        );
        let mut saw_steer_drain = false;
        while let Ok(ev) = rx.try_recv() {
            if ev["k"] == "steerDrain" && ev["sid"] == sid {
                saw_steer_drain = true;
            }
        }
        assert!(saw_steer_drain, "Stop event must broadcast steerDrain SSE event");

        // Wait for queued task to finish before testing StopFailure
        let deadline = tokio::time::Instant::now() + Duration::from_secs(2);
        while !state.tasks.lock().unwrap().list().is_empty()
            && tokio::time::Instant::now() < deadline
        {
            tokio::time::sleep(Duration::from_millis(10)).await;
        }

        // 3. StopFailure triggers AutoResumeGuard and broadcasts autoResume event
        let fail = Request::builder()
            .method("POST")
            .uri("/event")
            .header("host", "127.0.0.1:4310")
            .header("content-type", "application/json")
            .body(Body::from(
                json!({
                    "session_id": sid,
                    "hook_event_name": "StopFailure",
                    "cwd": ws.to_string_lossy()
                })
                .to_string(),
            ))
            .unwrap();
        assert_eq!(app.oneshot(fail).await.unwrap().status(), StatusCode::OK);
        let mut saw_auto_resume = false;
        while let Ok(ev) = rx.try_recv() {
            if ev["k"] == "autoResume" && ev["sid"] == sid && ev["attempts"] == 1 {
                saw_auto_resume = true;
            }
        }
        assert!(saw_auto_resume, "StopFailure with autoResume=true must trigger autoResume");
    }

    #[tokio::test]
    async fn post_undo_selective_files_keeps_remaining_file_undoable_and_returns_404_when_missing() {
        use axum::body::to_bytes;
        use serde_json::{Value, json};

        let dir = tempfile::tempdir().unwrap();
        let ws = dir.path().join("ws");
        std::fs::create_dir_all(&ws).unwrap();
        let fa = ws.join("a.txt");
        let fb = ws.join("b.txt");
        std::fs::write(&fa, "orig-a").unwrap();
        std::fs::write(&fb, "orig-b").unwrap();

        let state = Arc::new(AppState::new(dir.path().join("data"), None).unwrap());
        let app = build_router(state.clone());

        // Missing session & no turns -> 404
        let missing = Request::builder()
            .method("POST")
            .uri("/api/undo")
            .header("host", "127.0.0.1:4310")
            .header("content-type", "application/json")
            .body(Body::from(json!({ "id": "nonexistent-sid" }).to_string()))
            .unwrap();
        assert_eq!(app.clone().oneshot(missing).await.unwrap().status(), StatusCode::NOT_FOUND);

        for ev in [
            json!({ "session_id": "s-sel2", "hook_event_name": "UserPromptSubmit", "cwd": ws.to_string_lossy(), "prompt": "edit two" }),
            json!({ "session_id": "s-sel2", "hook_event_name": "PreToolUse", "cwd": ws.to_string_lossy(), "tool_name": "Edit", "tool_input": { "file_path": fa.to_string_lossy() } }),
            json!({ "session_id": "s-sel2", "hook_event_name": "PreToolUse", "cwd": ws.to_string_lossy(), "tool_name": "Edit", "tool_input": { "file_path": fb.to_string_lossy() } }),
        ] {
            let req = Request::builder()
                .method("POST")
                .uri("/event")
                .header("host", "127.0.0.1:4310")
                .header("content-type", "application/json")
                .body(Body::from(ev.to_string()))
                .unwrap();
            assert_eq!(app.clone().oneshot(req).await.unwrap().status(), StatusCode::OK);
        }
        std::fs::write(&fa, "mod-a").unwrap();
        std::fs::write(&fb, "mod-b").unwrap();
        let stop = Request::builder()
            .method("POST")
            .uri("/event")
            .header("host", "127.0.0.1:4310")
            .header("content-type", "application/json")
            .body(Body::from(
                json!({ "session_id": "s-sel2", "hook_event_name": "Stop", "cwd": ws.to_string_lossy() }).to_string(),
            ))
            .unwrap();
        assert_eq!(app.clone().oneshot(stop).await.unwrap().status(), StatusCode::OK);

        // First selective undo: only a.txt
        let canon_a = std::fs::canonicalize(&fa).unwrap();
        let req_a = Request::builder()
            .method("POST")
            .uri("/api/undo")
            .header("host", "127.0.0.1:4310")
            .header("content-type", "application/json")
            .body(Body::from(json!({ "id": "s-sel2", "files": [canon_a.to_string_lossy()] }).to_string()))
            .unwrap();
        assert_eq!(app.clone().oneshot(req_a).await.unwrap().status(), StatusCode::OK);
        assert_eq!(std::fs::read_to_string(&fa).unwrap(), "orig-a");
        assert_eq!(std::fs::read_to_string(&fb).unwrap(), "mod-b");

        // Second selective undo: b.txt MUST STILL be undoable (not locked out by turn.undone!)
        let canon_b = std::fs::canonicalize(&fb).unwrap();
        let req_b = Request::builder()
            .method("POST")
            .uri("/api/undo")
            .header("host", "127.0.0.1:4310")
            .header("content-type", "application/json")
            .body(Body::from(json!({ "id": "s-sel2", "files": [canon_b.to_string_lossy()] }).to_string()))
            .unwrap();
        let resp_b = app.oneshot(req_b).await.unwrap();
        assert_eq!(resp_b.status(), StatusCode::OK);
        let body_b: Value =
            serde_json::from_slice(&to_bytes(resp_b.into_body(), 64 * 1024).await.unwrap()).unwrap();
        assert_eq!(body_b["done"].as_array().unwrap().len(), 1);
        assert_eq!(std::fs::read_to_string(&fb).unwrap(), "orig-b");
    }

    #[tokio::test]
    async fn remote_auth_boundaries_block_chat_photo_say_and_undo_without_phone_tasks() {
        use crate::auth::COOKIE_NAME;
        use crate::http::guard::ViaTunnel;
        use axum::extract::ConnectInfo;
        use serde_json::json;
        use std::net::{IpAddr, Ipv4Addr, SocketAddr};

        let dir = tempfile::tempdir().unwrap();
        let state = Arc::new(AppState::new(dir.path().to_path_buf(), None).unwrap());
        let cookie = format!("{COOKIE_NAME}={}", state.auth.sign_cookie(false));
        let app = build_router(state);
        let lan_peer = SocketAddr::new(IpAddr::V4(Ipv4Addr::new(192, 168, 1, 50)), 52200);
        let loopback_peer = SocketAddr::new(IpAddr::V4(Ipv4Addr::LOCALHOST), 52201);

        let cases = [
            ("/api/chat", json!({ "op": "photo", "data": "data:image/jpeg;base64,/9j/4AAQ" })),
            ("/api/chat", json!({ "op": "say", "from": "reception", "text": "spoof" })),
            ("/api/undo", json!({ "id": "s1" })),
        ];
        for (uri, payload) in cases {
            let mut lan_req = Request::builder()
                .method("POST")
                .uri(uri)
                .header("host", "192.168.1.20:4310")
                .header("cookie", &cookie)
                .header("content-type", "application/json")
                .body(Body::from(payload.to_string()))
                .unwrap();
            lan_req.extensions_mut().insert(ConnectInfo(lan_peer));
            assert_eq!(
                app.clone().oneshot(lan_req).await.unwrap().status(),
                StatusCode::FORBIDDEN,
                "LAN caller must get 403 on {uri} {payload}"
            );

            let mut tun_req = Request::builder()
                .method("POST")
                .uri(uri)
                .header("host", "127.0.0.1:4310")
                .header("cookie", &cookie)
                .header("content-type", "application/json")
                .body(Body::from(payload.to_string()))
                .unwrap();
            tun_req.extensions_mut().insert(ConnectInfo(loopback_peer));
            tun_req.extensions_mut().insert(ViaTunnel(true));
            assert_eq!(
                app.clone().oneshot(tun_req).await.unwrap().status(),
                StatusCode::FORBIDDEN,
                "ViaTunnel caller must get 403 on {uri} {payload}"
            );
        }
    }

    #[tokio::test]
    async fn models_api_reads_and_writes_claude_settings_and_claude_md() {
        use crate::auth::COOKIE_NAME;
        use crate::http::guard::ViaTunnel;
        use axum::body::to_bytes;
        use axum::extract::ConnectInfo;
        use serde_json::{Value, json};
        use std::net::{IpAddr, Ipv4Addr, SocketAddr};

        let dir = tempfile::tempdir().unwrap();
        let data_dir = dir.path().join("data");
        let claude_dir = dir.path().join("claude");
        std::fs::create_dir_all(&claude_dir).unwrap();
        std::fs::write(
            claude_dir.join("settings.json"),
            r#"{"theme":"dark","model":"sonnet"}"#,
        )
        .unwrap();
        std::fs::write(claude_dir.join("CLAUDE.md"), "# Team Rules\n").unwrap();

        let state = Arc::new(AppState::new_with_claude_dir(data_dir, claude_dir.clone(), None).unwrap());
        let cookie = format!("{COOKIE_NAME}={}", state.auth.sign_cookie(false));
        let app = build_router(state);

        // GET /api/models reads settings.json + CLAUDE.md
        let req = Request::builder()
            .uri("/api/models")
            .header("host", "127.0.0.1:4310")
            .body(Body::empty())
            .unwrap();
        let resp = app.clone().oneshot(req).await.unwrap();
        assert_eq!(resp.status(), StatusCode::OK);
        let body: Value =
            serde_json::from_slice(&to_bytes(resp.into_body(), 64 * 1024).await.unwrap()).unwrap();
        assert_eq!(body["main"], "sonnet");
        assert_eq!(body["sub"], "");
        assert_eq!(body["force"], false);
        assert_eq!(body["split"], false);
        assert_eq!(body["readOnly"], false);

        // POST /api/models updates settings.json + CLAUDE.md atomically
        let post_req = Request::builder()
            .method("POST")
            .uri("/api/models")
            .header("host", "127.0.0.1:4310")
            .header("content-type", "application/json")
            .body(Body::from(
                json!({ "main": "opus", "sub": "haiku", "force": true, "split": true }).to_string(),
            ))
            .unwrap();
        let resp = app.clone().oneshot(post_req).await.unwrap();
        assert_eq!(resp.status(), StatusCode::OK);
        let body: Value =
            serde_json::from_slice(&to_bytes(resp.into_body(), 64 * 1024).await.unwrap()).unwrap();
        assert_eq!(body["main"], "opus");
        assert_eq!(body["sub"], "haiku");
        assert_eq!(body["force"], true);
        assert_eq!(body["split"], true);

        let disk_settings: Value =
            serde_json::from_str(&std::fs::read_to_string(claude_dir.join("settings.json")).unwrap())
                .unwrap();
        assert_eq!(disk_settings["theme"], "dark");
        assert_eq!(disk_settings["model"], "opus");
        assert_eq!(disk_settings["env"]["CLAUDE_CODE_SUBAGENT_MODEL"], "haiku");
        assert_eq!(disk_settings["env"]["CLAUDE_CODE_SUBAGENT_MODEL_FORCE"], "1");

        let disk_md = std::fs::read_to_string(claude_dir.join("CLAUDE.md")).unwrap();
        assert!(disk_md.contains("# Team Rules"));
        assert!(disk_md.contains("<!-- my-office:subagents -->"));

        // Clear back to defaults + split=false
        let clear_req = Request::builder()
            .method("POST")
            .uri("/api/models")
            .header("host", "127.0.0.1:4310")
            .header("content-type", "application/json")
            .body(Body::from(
                json!({ "main": "", "sub": "", "split": false }).to_string(),
            ))
            .unwrap();
        let resp = app.clone().oneshot(clear_req).await.unwrap();
        assert_eq!(resp.status(), StatusCode::OK);
        let cleared_settings: Value =
            serde_json::from_str(&std::fs::read_to_string(claude_dir.join("settings.json")).unwrap())
                .unwrap();
        assert!(cleared_settings.get("model").is_none());
        assert!(cleared_settings.get("env").is_none());
        assert_eq!(cleared_settings["theme"], "dark");
        let cleared_md = std::fs::read_to_string(claude_dir.join("CLAUDE.md")).unwrap();
        assert!(!cleared_md.contains("<!-- my-office:subagents -->"));
        assert_eq!(cleared_md, "# Team Rules\n");

        // Invalid model -> 400
        let bad_req = Request::builder()
            .method("POST")
            .uri("/api/models")
            .header("host", "127.0.0.1:4310")
            .header("content-type", "application/json")
            .body(Body::from(json!({ "main": "gpt-4o" }).to_string()))
            .unwrap();
        assert_eq!(
            app.clone().oneshot(bad_req).await.unwrap().status(),
            StatusCode::BAD_REQUEST
        );

        // Remote tunnel caller rejected on GET & POST /api/models
        let loopback_peer = SocketAddr::new(IpAddr::V4(Ipv4Addr::LOCALHOST), 52250);
        let mut tun_get = Request::builder()
            .uri("/api/models")
            .header("host", "127.0.0.1:4310")
            .header("cookie", &cookie)
            .body(Body::empty())
            .unwrap();
        tun_get.extensions_mut().insert(ConnectInfo(loopback_peer));
        tun_get.extensions_mut().insert(ViaTunnel(true));
        assert_eq!(
            app.oneshot(tun_get).await.unwrap().status(),
            StatusCode::FORBIDDEN
        );
    }

    #[tokio::test]
    async fn vault_http_routes_scan_projects_search_vietnamese_and_read_transcript() {
        use axum::body::to_bytes;
        use serde_json::Value;

        let dir = tempfile::tempdir().unwrap();
        let data_dir = dir.path().join("data");
        let claude_dir = dir.path().join("claude");
        let proj = claude_dir.join("projects").join("demo");
        std::fs::create_dir_all(&proj).unwrap();
        std::fs::write(
            proj.join("sess-http-1.jsonl"),
            concat!(
                r#"{"type":"custom-title","customTitle":"Tối ưu RAM"}"#,
                "\n",
                r#"{"type":"user","cwd":"/repo/demo","timestamp":"2026-11-25T10:00:00Z","message":{"content":"<system-reminder>skip</system-reminder>Kiểm tra bộ nhớ định kỳ"}}"#,
                "\n",
                r#"{"type":"assistant","timestamp":"2026-11-25T10:00:05Z","message":{"content":[{"type":"text","text":"Xong rồi"}]}}"#,
                "\n"
            ),
        )
        .unwrap();

        let state = Arc::new(AppState::new_with_claude_dir(data_dir, claude_dir, None).unwrap());
        let app = build_router(state);

        // GET /api/vault?fresh=1&q=toi+uu+ram scans off-thread and matches Vietnamese accents
        let req = Request::builder()
            .uri("/api/vault?fresh=1&q=toi%20uu%20ram")
            .header("host", "127.0.0.1:4310")
            .body(Body::empty())
            .unwrap();
        let resp = app.clone().oneshot(req).await.unwrap();
        assert_eq!(resp.status(), StatusCode::OK);
        let body: Value =
            serde_json::from_slice(&to_bytes(resp.into_body(), 128 * 1024).await.unwrap()).unwrap();
        assert_eq!(body["ok"], true);
        assert_eq!(body["items"].as_array().unwrap().len(), 1);
        assert_eq!(body["items"][0]["sid"], "sess-http-1");
        assert!(body["stats"]["n"].as_u64().unwrap() >= 1);

        // GET /api/vault?sid=sess-http-1 returns full cleaned transcript
        let rec_req = Request::builder()
            .uri("/api/vault?sid=sess-http-1")
            .header("host", "127.0.0.1:4310")
            .body(Body::empty())
            .unwrap();
        let resp = app.clone().oneshot(rec_req).await.unwrap();
        assert_eq!(resp.status(), StatusCode::OK);
        let rec_body: Value =
            serde_json::from_slice(&to_bytes(resp.into_body(), 128 * 1024).await.unwrap()).unwrap();
        assert_eq!(rec_body["ok"], true);
        assert_eq!(rec_body["rec"]["msgs"].as_array().unwrap().len(), 2);
        assert!(!rec_body["rec"]["msgs"][0]["t"].as_str().unwrap().contains("skip"));

        // GET /api/vault/transcript?sid=sess-http-1 returns archived transcript
        let tr_req = Request::builder()
            .uri("/api/vault/transcript?sid=sess-http-1")
            .header("host", "127.0.0.1:4310")
            .body(Body::empty())
            .unwrap();
        let resp = app.clone().oneshot(tr_req).await.unwrap();
        assert_eq!(resp.status(), StatusCode::OK);
        let tr_body: Value =
            serde_json::from_slice(&to_bytes(resp.into_body(), 128 * 1024).await.unwrap()).unwrap();
        assert_eq!(tr_body["ok"], true);
        assert_eq!(tr_body["rec"]["sid"], "sess-http-1");

        // Invalid sid rejected cleanly
        let bad_req = Request::builder()
            .uri("/api/vault/transcript?sid=../../secret")
            .header("host", "127.0.0.1:4310")
            .body(Body::empty())
            .unwrap();
        let resp = app.oneshot(bad_req).await.unwrap();
        assert_eq!(resp.status(), StatusCode::BAD_REQUEST);
    }

    #[tokio::test]
    async fn accounts_and_tunnel_http_routes_persist_real_state_and_enforce_loopback() {
        use crate::auth::COOKIE_NAME;
        use crate::http::guard::ViaTunnel;
        use axum::body::to_bytes;
        use axum::extract::ConnectInfo;
        use serde_json::{Value, json};
        use std::net::{IpAddr, Ipv4Addr, SocketAddr};

        let dir = tempfile::tempdir().unwrap();
        let data_dir = dir.path().join("data");
        let claude_dir = dir.path().join(".claude");
        std::fs::create_dir_all(&claude_dir).unwrap();
        std::fs::write(
            claude_dir.join(".credentials.json"),
            r#"{"claudeAiOauth":{"accessToken":"tok-primary"}}"#,
        )
        .unwrap();

        let state = Arc::new(AppState::new_with_claude_dir(data_dir, claude_dir, None).unwrap());
        let cookie = format!("{COOKIE_NAME}={}", state.auth.sign_cookie(false));
        let app = build_router(state.clone());

        // 1. Save main, prepare secondary "side", rename, switch, and GET /api/accounts
        let req = Request::builder()
            .method("POST")
            .uri("/api/accounts/save")
            .header("host", "127.0.0.1:4310")
            .header("content-type", "application/json")
            .body(Body::from(json!({ "name": "", "label": "Main" }).to_string()))
            .unwrap();
        assert_eq!(app.clone().oneshot(req).await.unwrap().status(), StatusCode::OK);

        let prep = Request::builder()
            .method("POST")
            .uri("/api/accounts")
            .header("host", "127.0.0.1:4310")
            .header("content-type", "application/json")
            .body(Body::from(json!({ "name": "side" }).to_string()))
            .unwrap();
        let resp = app.clone().oneshot(prep).await.unwrap();
        assert_eq!(resp.status(), StatusCode::OK);
        let body: Value =
            serde_json::from_slice(&to_bytes(resp.into_body(), 64 * 1024).await.unwrap()).unwrap();
        assert_eq!(body["ok"], true);
        assert_eq!(body["name"], "side");
        assert!(dir.path().join(".claude-side").is_dir());

        // Switching to an unsnapshotted slot fails closed with noCopy
        let sw_empty = Request::builder()
            .method("POST")
            .uri("/api/accounts/switch")
            .header("host", "127.0.0.1:4310")
            .header("content-type", "application/json")
            .body(Body::from(json!({ "name": "side" }).to_string()))
            .unwrap();
        let resp = app.clone().oneshot(sw_empty).await.unwrap();
        assert_eq!(resp.status(), StatusCode::OK);
        let sw_empty_body: Value =
            serde_json::from_slice(&to_bytes(resp.into_body(), 64 * 1024).await.unwrap()).unwrap();
        assert_eq!(sw_empty_body["ok"], false);
        assert_eq!(sw_empty_body["code"], "noCopy");

        // Save side credentials, switch back to main, then switch to side cleanly
        let save_side = Request::builder()
            .method("POST")
            .uri("/api/accounts/save")
            .header("host", "127.0.0.1:4310")
            .header("content-type", "application/json")
            .body(Body::from(json!({ "name": "side", "label": "Side" }).to_string()))
            .unwrap();
        assert_eq!(app.clone().oneshot(save_side).await.unwrap().status(), StatusCode::OK);

        let sw_main = Request::builder()
            .method("POST")
            .uri("/api/accounts/switch")
            .header("host", "127.0.0.1:4310")
            .header("content-type", "application/json")
            .body(Body::from(json!({ "name": "" }).to_string()))
            .unwrap();
        assert_eq!(app.clone().oneshot(sw_main).await.unwrap().status(), StatusCode::OK);

        let sw = Request::builder()
            .method("POST")
            .uri("/api/accounts/switch")
            .header("host", "127.0.0.1:4310")
            .header("content-type", "application/json")
            .body(Body::from(json!({ "name": "side" }).to_string()))
            .unwrap();
        assert_eq!(app.clone().oneshot(sw).await.unwrap().status(), StatusCode::OK);

        let list = Request::builder()
            .uri("/api/accounts")
            .header("host", "127.0.0.1:4310")
            .body(Body::empty())
            .unwrap();
        let resp = app.clone().oneshot(list).await.unwrap();
        assert_eq!(resp.status(), StatusCode::OK);
        let body: Value =
            serde_json::from_slice(&to_bytes(resp.into_body(), 64 * 1024).await.unwrap()).unwrap();
        assert_eq!(body["active"], "side");

        // 2. Tunnel domain + toggle routes persist into prefs and validate hostname
        let set_dom = Request::builder()
            .method("POST")
            .uri("/api/tunnel/domain")
            .header("host", "127.0.0.1:4310")
            .header("content-type", "application/json")
            .body(Body::from(json!({ "host": "office.example.com" }).to_string()))
            .unwrap();
        let resp = app.clone().oneshot(set_dom).await.unwrap();
        assert_eq!(resp.status(), StatusCode::OK);
        let dom_body: Value =
            serde_json::from_slice(&to_bytes(resp.into_body(), 64 * 1024).await.unwrap()).unwrap();
        assert_eq!(dom_body["domain"], "office.example.com");
        assert_eq!(
            state.allowed_tunnel_host().as_deref(),
            Some("office.example.com")
        );

        let bad_dom = Request::builder()
            .method("POST")
            .uri("/api/tunnel/domain")
            .header("host", "127.0.0.1:4310")
            .header("content-type", "application/json")
            .body(Body::from(json!({ "host": "https://evil.com/path" }).to_string()))
            .unwrap();
        assert_eq!(
            app.clone().oneshot(bad_dom).await.unwrap().status(),
            StatusCode::BAD_REQUEST
        );

        let start_tun = Request::builder()
            .method("POST")
            .uri("/api/tunnel")
            .header("host", "127.0.0.1:4310")
            .header("content-type", "application/json")
            .body(Body::from(json!({ "on": true }).to_string()))
            .unwrap();
        assert_eq!(
            app.clone().oneshot(start_tun).await.unwrap().status(),
            StatusCode::OK
        );

        let get_tun = Request::builder()
            .uri("/api/tunnel")
            .header("host", "127.0.0.1:4310")
            .body(Body::empty())
            .unwrap();
        let resp = app.clone().oneshot(get_tun).await.unwrap();
        assert_eq!(resp.status(), StatusCode::OK);
        let tun_body: Value =
            serde_json::from_slice(&to_bytes(resp.into_body(), 64 * 1024).await.unwrap()).unwrap();
        assert_eq!(tun_body["on"], true);
        assert_eq!(tun_body["domain"], "office.example.com");

        // 2b. /api/tunnel/stop disables tunnel cleanly
        let stop_tun = Request::builder()
            .method("POST")
            .uri("/api/tunnel/stop")
            .header("host", "127.0.0.1:4310")
            .body(Body::empty())
            .unwrap();
        let stop_resp = app.clone().oneshot(stop_tun).await.unwrap();
        assert_eq!(stop_resp.status(), StatusCode::OK);
        let stop_body: Value =
            serde_json::from_slice(&to_bytes(stop_resp.into_body(), 64 * 1024).await.unwrap())
                .unwrap();
        assert_eq!(stop_body["on"], false);
        assert_eq!(stop_body["state"], "off");

        // 3. Remote ViaTunnel rejected on GET /api/accounts, GET /api/tunnel, POST /api/tunnel/start, POST /api/tunnel/stop
        let loopback_peer = SocketAddr::new(IpAddr::V4(Ipv4Addr::LOCALHOST), 52280);
        for (method, uri) in [
            ("GET", "/api/accounts"),
            ("GET", "/api/tunnel"),
            ("POST", "/api/tunnel/start"),
            ("POST", "/api/tunnel/stop"),
        ] {
            let mut tun_req = Request::builder()
                .method(method)
                .uri(uri)
                .header("host", "127.0.0.1:4310")
                .header("cookie", &cookie)
                .body(Body::empty())
                .unwrap();
            tun_req.extensions_mut().insert(ConnectInfo(loopback_peer));
            tun_req.extensions_mut().insert(ViaTunnel(true));
            assert_eq!(
                app.clone().oneshot(tun_req).await.unwrap().status(),
                StatusCode::FORBIDDEN,
                "ViaTunnel must be 403 on {method} {uri}"
            );
        }
    }

    #[tokio::test]
    async fn oracle_contract_shapes_and_pins_id_op_add_match_golden_oracle() {
        use axum::body::to_bytes;
        use serde_json::{Value, json};

        let dir = tempfile::tempdir().unwrap();
        let claude_dir = dir.path().join("claude");
        std::fs::create_dir_all(&claude_dir).unwrap();
        let state = Arc::new(
            AppState::new_with_claude_dir(dir.path().to_path_buf(), claude_dir, None).unwrap(),
        );
        let app = build_router(state);

        // 1. POST /api/pins with Oracle `{ id, op: "add", text }` succeeds and persists
        let add_pin = Request::builder()
            .method("POST")
            .uri("/api/pins")
            .header("host", "127.0.0.1:4310")
            .header("content-type", "application/json")
            .body(Body::from(
                json!({ "id": "s-oracle", "op": "add", "text": "keep parity" }).to_string(),
            ))
            .unwrap();
        let resp = app.clone().oneshot(add_pin).await.unwrap();
        assert_eq!(resp.status(), StatusCode::OK);
        let pin_body: Value =
            serde_json::from_slice(&to_bytes(resp.into_body(), 64 * 1024).await.unwrap()).unwrap();
        assert_eq!(pin_body["ok"], true);
        assert_eq!(pin_body["pins"]["s-oracle"][0], "keep parity");

        // 2. GET /api/burn includes Oracle envelope fields
        let burn_resp = app
            .clone()
            .oneshot(
                Request::builder()
                    .uri("/api/burn")
                    .header("host", "127.0.0.1:4310")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        let burn: Value =
            serde_json::from_slice(&to_bytes(burn_resp.into_body(), 64 * 1024).await.unwrap())
                .unwrap();
        for key in [
            "ok",
            "range",
            "days",
            "lastLogsAt",
            "lastMetricsAt",
            "rest",
            "setup",
            "plan",
        ] {
            assert!(burn.get(key).is_some(), "/api/burn missing `{key}`");
        }

        // 3. GET /api/timeline includes `truncated`
        let tl_resp = app
            .clone()
            .oneshot(
                Request::builder()
                    .uri("/api/timeline")
                    .header("host", "127.0.0.1:4310")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        let tl: Value =
            serde_json::from_slice(&to_bytes(tl_resp.into_body(), 64 * 1024).await.unwrap())
                .unwrap();
        assert!(tl.get("truncated").is_some(), "/api/timeline missing `truncated`");

        // 4. GET /api/push includes `key` and `on` alongside `publicKey`
        let push_resp = app
            .clone()
            .oneshot(
                Request::builder()
                    .uri("/api/push")
                    .header("host", "127.0.0.1:4310")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        let push: Value =
            serde_json::from_slice(&to_bytes(push_resp.into_body(), 64 * 1024).await.unwrap())
                .unwrap();
        assert!(push.get("key").is_some(), "/api/push missing `key`");
        assert!(push.get("on").is_some(), "/api/push missing `on`");

        // 5. GET /api/stranded includes `partial`, `scanning`, `sizing`
        let str_resp = app
            .clone()
            .oneshot(
                Request::builder()
                    .uri("/api/stranded")
                    .header("host", "127.0.0.1:4310")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        let str_val: Value =
            serde_json::from_slice(&to_bytes(str_resp.into_body(), 64 * 1024).await.unwrap())
                .unwrap();
        for key in ["partial", "scanning", "sizing"] {
            assert!(str_val.get(key).is_some(), "/api/stranded missing `{key}`");
        }

        // 6. GET /api/tunnel includes `awake`, `bg`, `fixed`, `pin`, `rc`, `tasks`
        let tun_resp = app
            .clone()
            .oneshot(
                Request::builder()
                    .uri("/api/tunnel")
                    .header("host", "127.0.0.1:4310")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        let tun: Value =
            serde_json::from_slice(&to_bytes(tun_resp.into_body(), 64 * 1024).await.unwrap())
                .unwrap();
        for key in ["awake", "bg", "fixed", "pin", "rc", "tasks"] {
            assert!(tun.get(key).is_some(), "/api/tunnel missing `{key}`");
        }

        // 7. GET /api/sys includes `auto`, `ide`, `open`, `rules`
        let sys_resp = app
            .clone()
            .oneshot(
                Request::builder()
                    .uri("/api/sys")
                    .header("host", "127.0.0.1:4310")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        let sys: Value =
            serde_json::from_slice(&to_bytes(sys_resp.into_body(), 64 * 1024).await.unwrap())
                .unwrap();
        for key in ["auto", "ide", "open", "rules"] {
            assert!(sys.get(key).is_some(), "/api/sys missing `{key}`");
        }
    }

    #[tokio::test]
    async fn post_tasks_stop_resolves_running_task_by_session_id() {
        use axum::body::to_bytes;
        use serde_json::{Value, json};

        let dir = tempfile::tempdir().unwrap();
        let state = Arc::new(AppState::new(dir.path().to_path_buf(), None).unwrap());
        let task_id = {
            let mut tasks = state.tasks.lock().unwrap();
            let started = tasks.validate_and_register(
                crate::state::tasks::TaskStartRequest {
                    dir: dir.path().to_string_lossy().into_owned(),
                    text: "run task".to_owned(),
                    agent: "claude".to_owned(),
                    model: String::new(),
                    fork: false,
                },
                crate::state::sessions::now_ms(),
            );
            let tid = started["task"]["id"].as_str().unwrap().to_owned();
            tasks.bind(&tid, "sess-stop-via-sid", "");
            tid
        };

        let app = build_router(state.clone());
        let resp = app
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri("/api/tasks/stop")
                    .header("host", "127.0.0.1:4310")
                    .header("content-type", "application/json")
                    .body(Body::from(json!({ "id": "sess-stop-via-sid" }).to_string()))
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(resp.status(), StatusCode::OK);
        let body: Value =
            serde_json::from_slice(&to_bytes(resp.into_body(), 16 * 1024).await.unwrap()).unwrap();
        assert_eq!(body["ok"], true);
        assert!(
            state.tasks.lock().unwrap().get(&task_id).is_none(),
            "task must be stopped when targeted by session_id"
        );
    }

    #[tokio::test]
    async fn phone_accounts_and_push_test_endpoints_support_full_drawer_contracts() {
        use axum::body::to_bytes;
        use serde_json::{Value, json};

        let dir = tempfile::tempdir().unwrap();
        let state = Arc::new(AppState::new(dir.path().to_path_buf(), None).unwrap());
        let app = build_router(state.clone());

        // 1. POST /api/pin {} regenerates a fresh valid non-weak PIN
        let pin_resp = app
            .clone()
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri("/api/pin")
                    .header("host", "127.0.0.1:4310")
                    .header("content-type", "application/json")
                    .body(Body::from(json!({}).to_string()))
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(pin_resp.status(), StatusCode::OK);
        let pin_body: Value =
            serde_json::from_slice(&to_bytes(pin_resp.into_body(), 16 * 1024).await.unwrap())
                .unwrap();
        assert_eq!(pin_body["ok"], true, "empty POST /api/pin must regenerate PIN");
        let new_pin = pin_body["pin"].as_str().unwrap_or("");
        assert!(!crate::auth::pin_weak(new_pin), "regenerated PIN must not be weak: {new_pin}");

        // 2. POST /api/push/test returns 200 OK with { ok: true, sent: 0 }
        let push_resp = app
            .clone()
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri("/api/push/test")
                    .header("host", "127.0.0.1:4310")
                    .header("content-type", "application/json")
                    .body(Body::from("{}"))
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(push_resp.status(), StatusCode::OK);
        let push_body: Value =
            serde_json::from_slice(&to_bytes(push_resp.into_body(), 16 * 1024).await.unwrap())
                .unwrap();
        assert_eq!(push_body["ok"], true);
        assert_eq!(push_body["sent"], 0);

        // 3. GET /api/accounts includes codex & ag views alongside Claude accounts
        let acct_resp = app
            .clone()
            .oneshot(
                Request::builder()
                    .uri("/api/accounts")
                    .header("host", "127.0.0.1:4310")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(acct_resp.status(), StatusCode::OK);
        let acct_body: Value =
            serde_json::from_slice(&to_bytes(acct_resp.into_body(), 64 * 1024).await.unwrap())
                .unwrap();
        assert!(acct_body.get("accounts").is_some());
        assert!(acct_body.get("codex").is_some(), "/api/accounts missing `codex`");
        assert!(acct_body.get("ag").is_some(), "/api/accounts missing `ag`");

        // 4. GET /api/tunnel includes lanOn, phoneApprove, lanIp
        let tun_resp = app
            .oneshot(
                Request::builder()
                    .uri("/api/tunnel")
                    .header("host", "127.0.0.1:4310")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(tun_resp.status(), StatusCode::OK);
        let tun_body: Value =
            serde_json::from_slice(&to_bytes(tun_resp.into_body(), 64 * 1024).await.unwrap())
                .unwrap();
        assert!(tun_body.get("lanOn").is_some(), "/api/tunnel missing `lanOn`");
        assert!(tun_body.get("phoneApprove").is_some(), "/api/tunnel missing `phoneApprove`");
        assert!(tun_body.get("lanIp").is_some(), "/api/tunnel missing `lanIp`");
    }

    #[tokio::test]
    async fn post_rc_aborts_on_jsonc_without_overwriting_and_preserves_valid_settings() {
        use axum::body::to_bytes;
        use serde_json::{Value, json};

        let dir = tempfile::tempdir().unwrap();
        let data_dir = dir.path().join("data");
        let claude_dir = dir.path().join(".claude");
        std::fs::create_dir_all(&claude_dir).unwrap();

        // 1. Existing JSONC file with comments must NEVER be overwritten or diverge prefs.
        let jsonc = "{\n  // user comment\n  \"model\": \"opus\",\n  \"hooks\": {\"Stop\": []}\n}\n";
        let settings_path = claude_dir.join("settings.json");
        std::fs::write(&settings_path, jsonc).unwrap();

        let state = Arc::new(
            AppState::new_with_claude_dir(data_dir, claude_dir.clone(), None).unwrap(),
        );
        let app = build_router(state.clone());

        let fail_resp = app
            .clone()
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri("/api/rc")
                    .header("host", "127.0.0.1:4310")
                    .header("content-type", "application/json")
                    .body(Body::from(json!({ "on": true }).to_string()))
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(fail_resp.status(), StatusCode::INTERNAL_SERVER_ERROR);
        let fail_body: Value =
            serde_json::from_slice(&to_bytes(fail_resp.into_body(), 16 * 1024).await.unwrap())
                .unwrap();
        assert_eq!(fail_body["ok"], false);
        assert_eq!(fail_body["rc"], false);
        assert_eq!(
            std::fs::read_to_string(&settings_path).unwrap(),
            jsonc,
            "JSONC settings.json must remain byte-for-byte intact"
        );
        assert!(
            state.prefs.lock().unwrap().get("rcAtStartup").is_none(),
            "prefs must not mutate when settings.json write aborts"
        );

        // 2. Valid JSON preserves hooks/env and toggles remoteControlAtStartup cleanly.
        std::fs::write(
            &settings_path,
            r#"{"model":"opus","hooks":{"Stop":[]},"env":{"FOO":"bar"}}"#,
        )
        .unwrap();
        let ok_resp = app
            .clone()
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri("/api/rc")
                    .header("host", "127.0.0.1:4310")
                    .header("content-type", "application/json")
                    .body(Body::from(json!({ "on": true }).to_string()))
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(ok_resp.status(), StatusCode::OK);
        let saved: Value =
            serde_json::from_str(&std::fs::read_to_string(&settings_path).unwrap()).unwrap();
        assert_eq!(saved["remoteControlAtStartup"], true);
        assert_eq!(saved["model"], "opus");
        assert_eq!(saved["env"]["FOO"], "bar");

        let off_resp = app
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri("/api/rc")
                    .header("host", "127.0.0.1:4310")
                    .header("content-type", "application/json")
                    .body(Body::from(json!({ "on": false }).to_string()))
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(off_resp.status(), StatusCode::OK);
        let saved_off: Value =
            serde_json::from_str(&std::fs::read_to_string(&settings_path).unwrap()).unwrap();
        assert!(saved_off.get("remoteControlAtStartup").is_none());
        assert_eq!(saved_off["model"], "opus");
    }
}
