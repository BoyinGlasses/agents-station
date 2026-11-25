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
        let mut otel = state.otel.lock().unwrap_or_else(|e| e.into_inner());
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
        let mut otel = state.otel.lock().unwrap_or_else(|e| e.into_inner());
        otel.ingest_logs(&body, now_ms())
    };
    Ok(Json(json!({ "accepted": accepted })))
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
    async fn otel_routes_recover_from_poisoned_mutex() {
        let dir = tempfile::tempdir().unwrap();
        let state = Arc::new(AppState::new(dir.path().to_path_buf(), None).unwrap());
        let _ = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            let _guard = state.otel.lock().unwrap();
            panic!("poison otel");
        }));

        let app = build_router(state);
        let req_m = Request::builder()
            .method("POST")
            .uri("/v1/metrics")
            .header("host", "127.0.0.1:4310")
            .header("content-type", "application/json")
            .body(Body::from(json!({ "resourceMetrics": [] }).to_string()))
            .unwrap();
        assert_eq!(app.clone().oneshot(req_m).await.unwrap().status(), StatusCode::OK);

        let req_l = Request::builder()
            .method("POST")
            .uri("/v1/logs")
            .header("host", "127.0.0.1:4310")
            .header("content-type", "application/json")
            .body(Body::from(json!({ "resourceLogs": [] }).to_string()))
            .unwrap();
        assert_eq!(app.oneshot(req_l).await.unwrap().status(), StatusCode::OK);
    }
}
