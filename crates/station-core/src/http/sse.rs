use crate::http::AppState;
use crate::http::guard::{OptionalPeer, ViaTunnel, is_authorized, is_loopback_peer};
use axum::extract::{Query, State};
use axum::http::{HeaderMap, StatusCode};
use axum::response::sse::{Event, KeepAlive, Sse};
use futures_util::stream::{self, Stream};
use std::collections::HashMap;
use std::convert::Infallible;
use std::sync::Arc;
use tokio_stream::StreamExt;
use tokio_stream::wrappers::BroadcastStream;

pub async fn get_stream(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    OptionalPeer(peer): OptionalPeer,
    ViaTunnel(via_tunnel): ViaTunnel,
    Query(q): Query<HashMap<String, String>>,
) -> Result<Sse<impl Stream<Item = Result<Event, Infallible>>>, StatusCode> {
    if !is_authorized(
        peer,
        via_tunnel,
        &headers,
        q.get("k").map(String::as_str),
        &state.auth,
    ) {
        return Err(StatusCode::UNAUTHORIZED);
    }
    let local = is_loopback_peer(peer, via_tunnel, &headers);
    let lang = state.prefs.lock().unwrap_or_else(|e| e.into_inner()).lang();
    let (snap_str, rx) = {
        let office = state.office.lock().unwrap_or_else(|e| e.into_inner());
        let snap = office.snapshot_json(local, lang);
        (snap.to_string(), office.subscribe())
    };

    let init = stream::once(async move { Ok::<Event, Infallible>(Event::default().data(snap_str)) });
    let updates = BroadcastStream::new(rx).filter_map(|res| match res {
        Ok(val) => Some(Ok::<Event, Infallible>(Event::default().data(val.to_string()))),
        Err(_) => None,
    });

    Ok(Sse::new(init.chain(updates)).keep_alive(KeepAlive::default()))
}

#[cfg(test)]
mod tests {
    use crate::http::{AppState, build_router};
    use axum::body::Body;
    use axum::http::{Request, StatusCode};
    use std::sync::Arc;
    use tower::ServiceExt;

    #[tokio::test]
    async fn sse_stream_recovers_from_poisoned_prefs_and_office_locks() {
        let dir = tempfile::tempdir().unwrap();
        let state = Arc::new(AppState::new(dir.path().to_path_buf(), None).unwrap());
        let _ = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            let _g = state.prefs.lock().unwrap();
            panic!("poison prefs");
        }));
        let _ = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            let _g = state.office.lock().unwrap();
            panic!("poison office");
        }));

        let app = build_router(state);
        let req = Request::builder()
            .uri("/stream")
            .header("host", "127.0.0.1:4310")
            .body(Body::empty())
            .unwrap();
        let resp = app.oneshot(req).await.unwrap();
        assert_eq!(resp.status(), StatusCode::OK);
    }
}
