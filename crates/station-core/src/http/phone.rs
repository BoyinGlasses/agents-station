use crate::auth::constant_time_eq;
use crate::http::AppState;
use crate::http::guard::{
    OptionalPeer, ViaTunnel, enforce_same_origin_post_with_transport, is_authorized,
    is_loopback_peer,
};
use axum::extract::{Query, State};
use axum::http::{HeaderMap, HeaderValue, StatusCode, header};
use axum::response::{Html, IntoResponse, Redirect, Response};
use serde_json::{Value, json};
use std::collections::HashMap;
use std::sync::Arc;

pub async fn get_root(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    OptionalPeer(peer): OptionalPeer,
    ViaTunnel(via_tunnel): ViaTunnel,
    Query(q): Query<HashMap<String, String>>,
) -> Response {
    if !is_authorized(
        peer,
        via_tunnel,
        &headers,
        q.get("k").map(String::as_str),
        &state.auth,
    ) {
        return Redirect::to("/pin").into_response();
    }
    Html(PHONE_HTML).into_response()
}

pub async fn get_phone(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    OptionalPeer(peer): OptionalPeer,
    ViaTunnel(via_tunnel): ViaTunnel,
    Query(q): Query<HashMap<String, String>>,
) -> Response {
    if !is_authorized(
        peer,
        via_tunnel,
        &headers,
        q.get("k").map(String::as_str),
        &state.auth,
    ) {
        return Redirect::to("/pin").into_response();
    }
    Html(PHONE_HTML).into_response()
}

pub async fn get_pin() -> Html<&'static str> {
    Html(PIN_HTML)
}

pub async fn post_pin(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    ViaTunnel(via_tunnel): ViaTunnel,
    axum::Json(body): axum::Json<Value>,
) -> Response {
    if let Err(code) = enforce_same_origin_post_with_transport(
        &headers,
        state.allowed_tunnel_host().as_deref(),
        via_tunnel,
    ) {
        return code.into_response();
    }
    let pin = body.get("pin").and_then(Value::as_str).unwrap_or("").trim();
    let keep = body.get("keep").and_then(Value::as_bool).unwrap_or(true);
    let expected = state.auth.current_pin();
    if !constant_time_eq(pin, &expected) {
        return (StatusCode::UNAUTHORIZED, axum::Json(json!({ "ok": false }))).into_response();
    }
    let cookie = state.auth.set_cookie_header(keep, via_tunnel);
    let mut resp = axum::Json(json!({ "ok": true })).into_response();
    if let Ok(hv) = HeaderValue::from_str(&cookie) {
        resp.headers_mut().insert(header::SET_COOKIE, hv);
    }
    resp
}

pub async fn get_manifest() -> Response {
    let body = json!({
        "name": "Agents Station",
        "short_name": "Station",
        "start_url": "/phone",
        "display": "standalone",
        "background_color": "#14161e",
        "theme_color": "#d66440"
    });
    (
        [(header::CONTENT_TYPE, "application/manifest+json")],
        body.to_string(),
    )
        .into_response()
}

pub async fn get_sw() -> Response {
    (
        [(header::CONTENT_TYPE, "application/javascript")],
        "self.addEventListener('fetch', () => {});\n",
    )
        .into_response()
}

pub async fn get_health(
    headers: HeaderMap,
    OptionalPeer(peer): OptionalPeer,
    ViaTunnel(via_tunnel): ViaTunnel,
) -> (StatusCode, &'static str) {
    let _ = is_loopback_peer(peer, via_tunnel, &headers);
    (StatusCode::OK, "ok")
}

const PIN_HTML: &str = r#"<!doctype html>
<html lang="en"><head><meta charset="utf-8"><meta name="viewport" content="width=device-width,initial-scale=1">
<title>Agents Station — PIN</title>
<style>body{background:#14161e;color:#f5f0e8;font-family:system-ui,sans-serif;display:grid;place-items:center;min-height:100vh;margin:0}
form{background:#1e222e;padding:24px;border-radius:12px;border:1px solid #d66440;width:min(320px,90vw)}
input,button{width:100%;padding:10px;margin-top:10px;border-radius:8px;border:1px solid #3a4052;background:#14161e;color:#fff;font-size:16px}
button{background:#d66440;border:none;font-weight:600;cursor:pointer}</style></head>
<body><form id="f"><h2>Agents Station</h2><input id="p" inputmode="numeric" placeholder="Enter PIN" required>
<label style="display:flex;gap:8px;margin-top:10px"><input id="k" type="checkbox" checked style="width:auto">Remember device</label>
<button type="submit">Unlock</button></form>
<script>document.getElementById('f').onsubmit=async e=>{e.preventDefault();const r=await fetch('/pin',{method:'POST',headers:{'content-type':'application/json'},body:JSON.stringify({pin:p.value,keep:k.checked})});if(r.ok)location.href='/phone';else alert('Wrong PIN');};</script>
</body></html>"#;

const PHONE_HTML: &str = r#"<!doctype html>
<html lang="en"><head><meta charset="utf-8"><meta name="viewport" content="width=device-width,initial-scale=1">
<link rel="manifest" href="/manifest.webmanifest"><title>Agents Station Remote</title>
<style>body{background:#14161e;color:#f5f0e8;font-family:system-ui,sans-serif;margin:0;padding:16px}
.card{background:#1e222e;border:1px solid #2f3547;border-radius:10px;padding:12px;margin-bottom:10px}</style></head>
<body><h2>Agents Station · Remote</h2><div id="app">Loading…</div>
<script>const es=new EventSource('/stream');es.onmessage=e=>{const d=JSON.parse(e.data);if(d.k==='snap'){app.innerHTML=(d.sessions||[]).map(s=>`<div class="card"><b>${s.name}</b> · ${s.status}</div>`).join('')||'<div class="card">No active sessions</div>';}};</script>
</body></html>"#;
