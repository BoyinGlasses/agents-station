use crate::http::AppState;
use crate::http::guard::{OptionalPeer, ViaTunnel, enforce_hook_loopback};
use crate::state::sessions::now_ms;
use axum::Json;
use axum::extract::State;
use axum::http::{HeaderMap, StatusCode};
use serde_json::{Value, json};
use std::sync::Arc;

pub async fn post_metrics(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    OptionalPeer(peer): OptionalPeer,
    ViaTunnel(via_tunnel): ViaTunnel,
    Json(body): Json<Value>,
) -> Result<Json<Value>, StatusCode> {
    enforce_hook_loopback(peer, via_tunnel, &headers)?;
    if body.get("resourceMetrics").is_none() {
        return Err(StatusCode::BAD_REQUEST);
    }
    let accepted = {
        let mut otel = state.otel.lock().expect("otel lock poisoned");
        otel.ingest_metrics(&body, now_ms())
    };
    Ok(Json(json!({ "accepted": accepted })))
}

pub async fn post_logs(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    OptionalPeer(peer): OptionalPeer,
    ViaTunnel(via_tunnel): ViaTunnel,
    Json(body): Json<Value>,
) -> Result<Json<Value>, StatusCode> {
    enforce_hook_loopback(peer, via_tunnel, &headers)?;
    if body.get("resourceLogs").is_none() {
        return Err(StatusCode::BAD_REQUEST);
    }
    let accepted = {
        let mut otel = state.otel.lock().expect("otel lock poisoned");
        otel.ingest_logs(&body, now_ms())
    };
    Ok(Json(json!({ "accepted": accepted })))
}
