use crate::auth::{AuthStore, constant_time_eq};
use axum::extract::{ConnectInfo, FromRequestParts};
use axum::http::request::Parts;
use axum::http::{HeaderMap, StatusCode};
use std::convert::Infallible;
use std::net::SocketAddr;

/// Axum 0.8 compatible optional peer extractor.
/// Works both under `into_make_service_with_connect_info::<SocketAddr>()` and in `oneshot` tests.
#[derive(Debug, Clone, Copy)]
pub struct OptionalPeer(pub Option<SocketAddr>);

impl<S: Send + Sync> FromRequestParts<S> for OptionalPeer {
    type Rejection = Infallible;

    async fn from_request_parts(parts: &mut Parts, _state: &S) -> Result<Self, Self::Rejection> {
        let addr = parts.extensions.get::<ConnectInfo<SocketAddr>>().map(|c| c.0);
        Ok(Self(addr))
    }
}

pub fn host_without_port(raw: &str) -> &str {
    let trimmed = raw.trim();
    if let Some(rest) = trimmed.strip_prefix('[') {
        if let Some(end) = rest.find(']') {
            return &trimmed[..=end + 1];
        }
    }
    match trimmed.rsplit_once(':') {
        Some((h, port)) if port.bytes().all(|b| b.is_ascii_digit()) => h,
        _ => trimmed,
    }
}

pub fn is_loopback_host(raw: &str) -> bool {
    let h = host_without_port(raw).to_ascii_lowercase();
    matches!(h.as_str(), "localhost" | "127.0.0.1" | "[::1]" | "::1")
}

pub fn origin_is_loopback(origin: &str) -> bool {
    let low = origin.trim().to_ascii_lowercase();
    let Some(rest) = low
        .strip_prefix("http://")
        .or_else(|| low.strip_prefix("https://"))
    else {
        return false;
    };
    let authority = rest.split('/').next().unwrap_or("");
    is_loopback_host(authority)
}

pub fn is_loopback_peer(peer: Option<SocketAddr>, headers: &HeaderMap) -> bool {
    if headers.contains_key("x-office-via-tunnel") {
        return false;
    }
    match peer {
        Some(addr) => addr.ip().is_loopback(),
        None => {
            // Unit tests via oneshot without ConnectInfo: honor Host/X-Forwarded-For if explicitly remote.
            if headers.contains_key("x-office-remote-test") {
                return false;
            }
            headers
                .get("host")
                .and_then(|v| v.to_str().ok())
                .is_none_or(is_loopback_host)
        }
    }
}

/// Strict guard for loopback-only routes (`/event`, `/hook/*`, `/v1/*`, `/api/harnesses/*`).
/// Rejects non-loopback peers, spoofed Host headers (DNS rebinding), and cross-origin browser requests.
pub fn enforce_loopback_strict(peer: Option<SocketAddr>, headers: &HeaderMap) -> Result<(), StatusCode> {
    if !is_loopback_peer(peer, headers) {
        return Err(StatusCode::FORBIDDEN);
    }
    if let Some(host) = headers.get("host").and_then(|v| v.to_str().ok())
        && !is_loopback_host(host)
    {
        return Err(StatusCode::FORBIDDEN);
    }
    if let Some(origin) = headers.get("origin").and_then(|v| v.to_str().ok())
        && !origin_is_loopback(origin)
    {
        return Err(StatusCode::FORBIDDEN);
    }
    Ok(())
}

pub fn is_authorized(
    peer: Option<SocketAddr>,
    headers: &HeaderMap,
    query_k: Option<&str>,
    auth: &AuthStore,
) -> bool {
    if is_loopback_peer(peer, headers) {
        return true;
    }
    let via_tunnel = headers.contains_key("x-office-via-tunnel");
    if !via_tunnel
        && let Some(k) = query_k
        && constant_time_eq(k, auth.token())
    {
        return true;
    }
    if let Some(cookie) = headers.get("cookie").and_then(|v| v.to_str().ok())
        && auth.verify_cookie(cookie)
    {
        return true;
    }
    false
}
