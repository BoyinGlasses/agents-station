use crate::auth::constant_time_eq;
use crate::http::AppState;
use crate::http::guard::{
    OptionalPeer, ViaTunnel, enforce_same_origin_post_with_transport, is_authorized,
    is_loopback_peer,
};
use crate::state::sessions::now_ms;
use axum::extract::{Query, State};
use axum::http::{HeaderMap, HeaderValue, StatusCode, header};
use axum::response::{Html, IntoResponse, Redirect, Response};
use serde_json::{Value, json};
use std::collections::HashMap;
use std::net::{IpAddr, Ipv4Addr};
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
    OptionalPeer(peer): OptionalPeer,
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
    let matched = constant_time_eq(pin, &expected);
    let ip = peer
        .map(|a| a.ip())
        .unwrap_or(IpAddr::V4(Ipv4Addr::LOCALHOST));
    {
        let mut limiter = state
            .pin_limiter
            .lock()
            .unwrap_or_else(|e| e.into_inner());
        if let Err(code) = limiter.check_and_record(ip, now_ms(), matched) {
            return (
                code,
                axum::Json(json!({ "ok": false, "error": "too many attempts" })),
            )
                .into_response();
        }
    }
    if !matched {
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

#[cfg(test)]
mod tests {
    use crate::http::{AppState, build_router};
    use axum::body::Body;
    use axum::extract::ConnectInfo;
    use axum::http::{Request, StatusCode};
    use serde_json::json;
    use std::net::{IpAddr, Ipv4Addr, SocketAddr};
    use std::sync::Arc;
    use tower::ServiceExt;

    #[tokio::test]
    async fn post_pin_locks_out_on_6th_failure_per_ip_and_21st_global() {
        // 1. Per-IP lockout: 5 bad attempts -> 401, 6th -> 429 (even with valid PIN)
        let dir = tempfile::tempdir().unwrap();
        let state = Arc::new(AppState::new(dir.path().to_path_buf(), None).unwrap());
        state.auth.set_pin("492817").unwrap();
        let app = build_router(state.clone());
        let ip_a = SocketAddr::new(IpAddr::V4(Ipv4Addr::new(192, 168, 1, 10)), 50000);

        for i in 0..5 {
            let mut req = Request::builder()
                .method("POST")
                .uri("/pin")
                .header("host", "127.0.0.1:4310")
                .header("content-type", "application/json")
                .body(Body::from(json!({"pin": "000000"}).to_string()))
                .unwrap();
            req.extensions_mut().insert(ConnectInfo(ip_a));
            let resp = app.clone().oneshot(req).await.unwrap();
            assert_eq!(resp.status(), StatusCode::UNAUTHORIZED, "attempt {}", i + 1);
        }

        let mut sixth = Request::builder()
            .method("POST")
            .uri("/pin")
            .header("host", "127.0.0.1:4310")
            .header("content-type", "application/json")
            .body(Body::from(json!({"pin": "000000"}).to_string()))
            .unwrap();
        sixth.extensions_mut().insert(ConnectInfo(ip_a));
        let resp = app.clone().oneshot(sixth).await.unwrap();
        assert_eq!(resp.status(), StatusCode::TOO_MANY_REQUESTS);

        // Even the correct PIN is blocked while locked out
        let mut seventh_good = Request::builder()
            .method("POST")
            .uri("/pin")
            .header("host", "127.0.0.1:4310")
            .header("content-type", "application/json")
            .body(Body::from(json!({"pin": state.auth.current_pin()}).to_string()))
            .unwrap();
        seventh_good.extensions_mut().insert(ConnectInfo(ip_a));
        let resp = app.oneshot(seventh_good).await.unwrap();
        assert_eq!(resp.status(), StatusCode::TOO_MANY_REQUESTS);

        // 2. Global lockout: 20 failures across 5 IPs (4 each) -> 401; 21st from 6th IP -> 429
        let dir2 = tempfile::tempdir().unwrap();
        let state2 = Arc::new(AppState::new(dir2.path().to_path_buf(), None).unwrap());
        state2.auth.set_pin("492817").unwrap();
        let app2 = build_router(state2);

        for ip_idx in 1u8..=5 {
            let addr = SocketAddr::new(IpAddr::V4(Ipv4Addr::new(10, 0, 0, ip_idx)), 50000);
            for attempt in 0..4 {
                let mut req = Request::builder()
                    .method("POST")
                    .uri("/pin")
                    .header("host", "127.0.0.1:4310")
                    .header("content-type", "application/json")
                    .body(Body::from(json!({"pin": "000000"}).to_string()))
                    .unwrap();
                req.extensions_mut().insert(ConnectInfo(addr));
                let resp = app2.clone().oneshot(req).await.unwrap();
                assert_eq!(
                    resp.status(),
                    StatusCode::UNAUTHORIZED,
                    "ip {ip_idx} attempt {}",
                    attempt + 1
                );
            }
        }

        let ip6 = SocketAddr::new(IpAddr::V4(Ipv4Addr::new(10, 0, 0, 6)), 50000);
        let mut twenty_first = Request::builder()
            .method("POST")
            .uri("/pin")
            .header("host", "127.0.0.1:4310")
            .header("content-type", "application/json")
            .body(Body::from(json!({"pin": "000000"}).to_string()))
            .unwrap();
        twenty_first.extensions_mut().insert(ConnectInfo(ip6));
        let resp = app2.oneshot(twenty_first).await.unwrap();
        assert_eq!(resp.status(), StatusCode::TOO_MANY_REQUESTS);
    }

    #[tokio::test]
    async fn post_api_pin_enforces_rate_limiter_on_6th_bad_attempt() {
        let dir = tempfile::tempdir().unwrap();
        let state = Arc::new(AppState::new(dir.path().to_path_buf(), None).unwrap());
        let app = build_router(state);
        let loopback = SocketAddr::new(IpAddr::V4(Ipv4Addr::LOCALHOST), 51000);

        for i in 0..5 {
            let mut req = Request::builder()
                .method("POST")
                .uri("/api/pin")
                .header("host", "127.0.0.1:4310")
                .header("content-type", "application/json")
                .body(Body::from(json!({"pin": "123456"}).to_string()))
                .unwrap();
            req.extensions_mut().insert(ConnectInfo(loopback));
            let resp = app.clone().oneshot(req).await.unwrap();
            assert_eq!(resp.status(), StatusCode::OK, "attempt {}", i + 1);
        }

        let mut sixth = Request::builder()
            .method("POST")
            .uri("/api/pin")
            .header("host", "127.0.0.1:4310")
            .header("content-type", "application/json")
            .body(Body::from(json!({"pin": "492817"}).to_string()))
            .unwrap();
        sixth.extensions_mut().insert(ConnectInfo(loopback));
        let resp = app.oneshot(sixth).await.unwrap();
        assert_eq!(resp.status(), StatusCode::TOO_MANY_REQUESTS);
    }
}
