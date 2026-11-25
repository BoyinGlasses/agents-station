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
    let lang = state.prefs.lock().expect("prefs lock").lang();
    let (snap_str, rx) = {
        let office = state.office.lock().expect("office lock");
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
