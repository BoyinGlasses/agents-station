use crate::auth::{AuthStore, constant_time_eq};
use axum::extract::{ConnectInfo, FromRequestParts};
use axum::http::request::Parts;
use axum::http::{HeaderMap, StatusCode};
use std::convert::Infallible;
use std::net::{IpAddr, SocketAddr};

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

/// Server-side marker inserted by a dedicated tunnel listener / router extension.
/// Never derived from client-controllable HTTP headers.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct ViaTunnel(pub bool);

impl<S: Send + Sync> FromRequestParts<S> for ViaTunnel {
    type Rejection = Infallible;

    async fn from_request_parts(parts: &mut Parts, _state: &S) -> Result<Self, Self::Rejection> {
        Ok(parts
            .extensions
            .get::<ViaTunnel>()
            .copied()
            .unwrap_or_default())
    }
}

fn parse_host_port(raw: &str) -> Option<(String, Option<u16>)> {
    let s = raw.trim();
    if s.is_empty()
        || s.bytes()
            .any(|b| b.is_ascii_whitespace() || b.is_ascii_control() || matches!(b, b'/' | b'?' | b'#' | b'@'))
    {
        return None;
    }
    if let Some(rest) = s.strip_prefix('[') {
        let end = rest.find(']')?;
        if end == 0 {
            return None;
        }
        let inner = &rest[..end];
        inner.parse::<std::net::Ipv6Addr>().ok()?;
        let tail = &rest[end + 1..];
        let port = if tail.is_empty() {
            None
        } else if let Some(p) = tail.strip_prefix(':') {
            Some(p.parse::<u16>().ok()?)
        } else {
            return None;
        };
        return Some((format!("[{}]", inner.to_ascii_lowercase()), port));
    }
    if s.contains('[') || s.contains(']') {
        return None;
    }
    if s.matches(':').count() > 1 {
        s.parse::<std::net::Ipv6Addr>().ok()?;
        return Some((s.to_ascii_lowercase(), None));
    }
    match s.rsplit_once(':') {
        Some((h, p)) => {
            if h.is_empty() {
                return None;
            }
            let port = p.parse::<u16>().ok()?;
            Some((h.to_ascii_lowercase(), Some(port)))
        }
        None => Some((s.to_ascii_lowercase(), None)),
    }
}

fn parse_origin(raw: &str) -> Option<(&'static str, String, Option<u16>)> {
    let s = raw.trim();
    let (scheme, rest) = if s.len() > 7 && s[..7].eq_ignore_ascii_case("http://") {
        ("http", &s[7..])
    } else if s.len() > 8 && s[..8].eq_ignore_ascii_case("https://") {
        ("https", &s[8..])
    } else {
        return None;
    };
    let (host, port) = parse_host_port(rest)?;
    Some((scheme, host, port))
}

fn is_local_ip(ip: IpAddr) -> bool {
    match ip {
        IpAddr::V4(v4) => v4.is_loopback() || v4.is_private() || v4.is_link_local(),
        IpAddr::V6(v6) => {
            v6.is_loopback()
                || (v6.segments()[0] & 0xfe00) == 0xfc00
                || (v6.segments()[0] & 0xffc0) == 0xfe80
                || v6.to_ipv4_mapped().is_some_and(|v4| {
                    v4.is_loopback() || v4.is_private() || v4.is_link_local()
                })
        }
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
        Some((h, port)) if !port.is_empty() && port.bytes().all(|b| b.is_ascii_digit()) => h,
        _ => trimmed,
    }
}

pub fn is_loopback_host(raw: &str) -> bool {
    let Some((h, _)) = parse_host_port(raw) else {
        return false;
    };
    matches!(h.as_str(), "localhost" | "127.0.0.1" | "[::1]" | "::1")
}

/// Universal DNS-rebinding guard for all HTTP routes.
/// Allows loopback hosts, raw local private/link-local IPv4/IPv6 literals (for LAN `/phone`),
/// and an explicitly configured tunnel hostname. Rejects missing or unknown hosts with `421 Misdirected Request`.
pub fn enforce_host_allowed(
    headers: &HeaderMap,
    allowed_tunnel_host: Option<&str>,
) -> Result<(), StatusCode> {
    let mut hosts = headers.get_all("host").iter();
    let Some(first) = hosts.next() else {
        return Err(StatusCode::MISDIRECTED_REQUEST);
    };
    if hosts.next().is_some() {
        return Err(StatusCode::MISDIRECTED_REQUEST);
    }
    let Ok(raw) = first.to_str() else {
        return Err(StatusCode::MISDIRECTED_REQUEST);
    };
    let Some((host, _)) = parse_host_port(raw) else {
        return Err(StatusCode::MISDIRECTED_REQUEST);
    };
    if host == "localhost" {
        return Ok(());
    }
    let ip_candidate = host
        .strip_prefix('[')
        .and_then(|s| s.strip_suffix(']'))
        .unwrap_or(&host);
    if let Ok(ip) = ip_candidate.parse::<IpAddr>() {
        return if is_local_ip(ip) {
            Ok(())
        } else {
            Err(StatusCode::MISDIRECTED_REQUEST)
        };
    }
    if let Some(allowed) = allowed_tunnel_host
        && let Some((allowed_host, _)) = parse_host_port(allowed)
        && host == allowed_host
    {
        return Ok(());
    }
    Err(StatusCode::MISDIRECTED_REQUEST)
}

/// Exact-port CSRF guard for state-mutating browser POST requests over local plain HTTP.
/// When an `Origin` header is present, its scheme (`http`), authority, and port must match `Host` exactly.
pub fn enforce_same_origin_post(
    headers: &HeaderMap,
    allowed_tunnel_host: Option<&str>,
) -> Result<(), StatusCode> {
    enforce_same_origin_post_with_transport(headers, allowed_tunnel_host, false)
}

/// Exact-scheme and exact-port CSRF guard for state-mutating browser POST requests.
/// Expects `https` (default port 443) when `via_tunnel` is true, and `http` (default port 80) otherwise.
pub fn enforce_same_origin_post_with_transport(
    headers: &HeaderMap,
    allowed_tunnel_host: Option<&str>,
    via_tunnel: bool,
) -> Result<(), StatusCode> {
    enforce_host_allowed(headers, allowed_tunnel_host)?;
    let mut origins = headers.get_all("origin").iter();
    let Some(first_origin) = origins.next() else {
        return Ok(());
    };
    if origins.next().is_some() {
        return Err(StatusCode::FORBIDDEN);
    }
    let Ok(origin_raw) = first_origin.to_str() else {
        return Err(StatusCode::FORBIDDEN);
    };
    let Some((origin_scheme, origin_host, origin_port)) = parse_origin(origin_raw) else {
        return Err(StatusCode::FORBIDDEN);
    };
    let (expected_scheme, expected_default_port) = if via_tunnel {
        ("https", 443u16)
    } else {
        ("http", 80u16)
    };
    if origin_scheme != expected_scheme {
        return Err(StatusCode::FORBIDDEN);
    }
    let Some(host_raw) = headers.get("host").and_then(|v| v.to_str().ok()) else {
        return Err(StatusCode::FORBIDDEN);
    };
    let Some((host, host_port)) = parse_host_port(host_raw) else {
        return Err(StatusCode::FORBIDDEN);
    };
    if origin_host != host {
        return Err(StatusCode::FORBIDDEN);
    }
    if origin_port.unwrap_or(expected_default_port) != host_port.unwrap_or(expected_default_port) {
        return Err(StatusCode::FORBIDDEN);
    }
    Ok(())
}

pub fn is_loopback_peer(peer: Option<SocketAddr>, via_tunnel: bool, headers: &HeaderMap) -> bool {
    if via_tunnel {
        return false;
    }
    match peer {
        Some(addr) => addr.ip().is_loopback(),
        None => {
            // Unit tests via oneshot without ConnectInfo: honor explicit remote test marker.
            if headers.contains_key("x-office-remote-test") {
                return false;
            }
            headers
                .get("host")
                .and_then(|v| v.to_str().ok())
                .is_some_and(is_loopback_host)
        }
    }
}

fn require_loopback_host(headers: &HeaderMap) -> Result<(), StatusCode> {
    let mut hosts = headers.get_all("host").iter();
    let Some(first_host) = hosts.next() else {
        return Err(StatusCode::FORBIDDEN);
    };
    if hosts.next().is_some() {
        return Err(StatusCode::FORBIDDEN);
    }
    let Ok(host) = first_host.to_str() else {
        return Err(StatusCode::FORBIDDEN);
    };
    if !is_loopback_host(host) {
        return Err(StatusCode::FORBIDDEN);
    }
    Ok(())
}

/// Strict guard for non-browser local hook & OTLP routes (`/event`, `/hook/*`, `/v1/*`).
/// Rejects non-loopback peers, tunnel connections, missing/non-loopback `Host`, and ANY `Origin` header.
pub fn enforce_hook_loopback(
    peer: Option<SocketAddr>,
    via_tunnel: bool,
    headers: &HeaderMap,
) -> Result<(), StatusCode> {
    if !is_loopback_peer(peer, via_tunnel, headers) {
        return Err(StatusCode::FORBIDDEN);
    }
    require_loopback_host(headers)?;
    if headers.contains_key("origin") {
        return Err(StatusCode::FORBIDDEN);
    }
    Ok(())
}

/// Strict guard for loopback-only local UI routes (`/api/harnesses/*`, local mutations).
/// Rejects non-loopback peers, tunnel connections, missing/non-loopback `Host`, and cross-port/cross-origin browser requests.
pub fn enforce_loopback_strict(
    peer: Option<SocketAddr>,
    via_tunnel: bool,
    headers: &HeaderMap,
) -> Result<(), StatusCode> {
    if !is_loopback_peer(peer, via_tunnel, headers) {
        return Err(StatusCode::FORBIDDEN);
    }
    require_loopback_host(headers)?;
    enforce_same_origin_post(headers, None)
}

pub fn is_authorized(
    peer: Option<SocketAddr>,
    via_tunnel: bool,
    headers: &HeaderMap,
    query_k: Option<&str>,
    auth: &AuthStore,
) -> bool {
    if is_loopback_peer(peer, via_tunnel, headers) {
        return true;
    }
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::http::{AppState, build_router};
    use axum::body::Body;
    use axum::http::Request;
    use serde_json::json;
    use std::net::{IpAddr, Ipv4Addr};
    use std::sync::Arc;
    use tower::ServiceExt;

    fn loopback_addr() -> SocketAddr {
        SocketAddr::new(IpAddr::V4(Ipv4Addr::LOCALHOST), 54321)
    }

    #[tokio::test]
    async fn hooks_and_otlp_reject_any_origin_header() {
        let dir = tempfile::tempdir().unwrap();
        let state = Arc::new(AppState::new(dir.path().to_path_buf(), None).unwrap());
        let app = build_router(state);

        let paths = [
            ("/event", json!({"session_id":"s1","hook_event_name":"SessionStart"})),
            ("/hook/loop", json!({"session_id":"s1"})),
            ("/hook/status", json!({"session_id":"s1"})),
            ("/hook/start", json!({"session_id":"s1"})),
            ("/v1/metrics", json!({"resourceMetrics":[]})),
            ("/v1/logs", json!({"resourceLogs":[]})),
        ];

        for (uri, body) in paths {
            let mut req = Request::builder()
                .method("POST")
                .uri(uri)
                .header("host", "127.0.0.1:4310")
                .header("origin", "http://127.0.0.1:4310")
                .header("content-type", "application/json")
                .body(Body::from(body.to_string()))
                .unwrap();
            req.extensions_mut().insert(ConnectInfo(loopback_addr()));
            let resp = app.clone().oneshot(req).await.unwrap();
            assert_eq!(
                resp.status(),
                StatusCode::FORBIDDEN,
                "expected 403 for {uri} with Origin header"
            );
        }
    }

    #[tokio::test]
    async fn custom_harness_post_rejects_cross_port_localhost_origin() {
        let dir = tempfile::tempdir().unwrap();
        let state = Arc::new(AppState::new(dir.path().to_path_buf(), None).unwrap());
        let app = build_router(state);

        let mut req = Request::builder()
            .method("POST")
            .uri("/api/harnesses/custom")
            .header("host", "127.0.0.1:4310")
            .header("origin", "http://localhost:5173")
            .header("content-type", "application/json")
            .body(Body::from(json!({"op":"upsert","id":"x","path":"C:/x.exe"}).to_string()))
            .unwrap();
        req.extensions_mut().insert(ConnectInfo(loopback_addr()));
        let resp = app.oneshot(req).await.unwrap();
        assert_eq!(resp.status(), StatusCode::FORBIDDEN);
    }

    #[tokio::test]
    async fn live_from_loopback_with_evil_host_returns_421() {
        let dir = tempfile::tempdir().unwrap();
        let state = Arc::new(AppState::new(dir.path().to_path_buf(), None).unwrap());
        let app = build_router(state);

        let mut req = Request::builder()
            .method("GET")
            .uri("/api/live")
            .header("host", "evil.com:4310")
            .body(Body::empty())
            .unwrap();
        req.extensions_mut().insert(ConnectInfo(loopback_addr()));
        let resp = app.oneshot(req).await.unwrap();
        assert_eq!(resp.status(), StatusCode::MISDIRECTED_REQUEST);
    }

    #[tokio::test]
    async fn x_office_via_tunnel_header_alone_is_not_trusted() {
        let dir = tempfile::tempdir().unwrap();
        let state = Arc::new(AppState::new(dir.path().to_path_buf(), None).unwrap());
        let app = build_router(state);

        let mut req = Request::builder()
            .method("GET")
            .uri("/api/live")
            .header("host", "127.0.0.1:4310")
            .header("x-office-via-tunnel", "1")
            .body(Body::empty())
            .unwrap();
        req.extensions_mut().insert(ConnectInfo(loopback_addr()));
        let resp = app.oneshot(req).await.unwrap();
        assert_eq!(resp.status(), StatusCode::OK);
    }

    #[tokio::test]
    async fn via_tunnel_extension_from_loopback_requires_pin_cookie() {
        let dir = tempfile::tempdir().unwrap();
        let state = Arc::new(AppState::new(dir.path().to_path_buf(), None).unwrap());
        let app = build_router(state.clone());

        // ViaTunnel(true) extension without PIN cookie and without x-office-via-tunnel header -> 401
        let mut req = Request::builder()
            .method("GET")
            .uri("/api/live")
            .header("host", "127.0.0.1:4310")
            .body(Body::empty())
            .unwrap();
        req.extensions_mut().insert(ConnectInfo(loopback_addr()));
        req.extensions_mut().insert(ViaTunnel(true));
        let resp = app.clone().oneshot(req).await.unwrap();
        assert_eq!(resp.status(), StatusCode::UNAUTHORIZED);

        // Dedicated tunnel router automatically enforces ViaTunnel(true)
        let tunnel_app = crate::http::build_tunnel_router(state.clone());
        let mut req2 = Request::builder()
            .method("GET")
            .uri("/api/live")
            .header("host", "127.0.0.1:4310")
            .body(Body::empty())
            .unwrap();
        req2.extensions_mut().insert(ConnectInfo(loopback_addr()));
        let resp2 = tunnel_app.clone().oneshot(req2).await.unwrap();
        assert_eq!(resp2.status(), StatusCode::UNAUTHORIZED);

        // Tunnel router also rejects /event even from loopback -> 403
        let mut hook_req = Request::builder()
            .method("POST")
            .uri("/event")
            .header("host", "127.0.0.1:4310")
            .header("content-type", "application/json")
            .body(Body::from(json!({"session_id":"s1","hook_event_name":"SessionStart"}).to_string()))
            .unwrap();
        hook_req.extensions_mut().insert(ConnectInfo(loopback_addr()));
        let hook_resp = tunnel_app.oneshot(hook_req).await.unwrap();
        assert_eq!(hook_resp.status(), StatusCode::FORBIDDEN);

        // POST /pin sets Secure cookie only on ViaTunnel(true), not on spoofed header
        let pin = state.auth.current_pin();
        let mut pin_header_only = Request::builder()
            .method("POST")
            .uri("/pin")
            .header("host", "127.0.0.1:4310")
            .header("x-office-via-tunnel", "1")
            .header("content-type", "application/json")
            .body(Body::from(json!({"pin": pin, "keep": true}).to_string()))
            .unwrap();
        pin_header_only.extensions_mut().insert(ConnectInfo(loopback_addr()));
        let resp_hdr = app.clone().oneshot(pin_header_only).await.unwrap();
        assert_eq!(resp_hdr.status(), StatusCode::OK);
        let cookie_hdr = resp_hdr.headers().get("set-cookie").unwrap().to_str().unwrap();
        assert!(!cookie_hdr.contains("; Secure"));

        let mut pin_via_ext = Request::builder()
            .method("POST")
            .uri("/pin")
            .header("host", "127.0.0.1:4310")
            .header("content-type", "application/json")
            .body(Body::from(json!({"pin": pin, "keep": true}).to_string()))
            .unwrap();
        pin_via_ext.extensions_mut().insert(ConnectInfo(loopback_addr()));
        pin_via_ext.extensions_mut().insert(ViaTunnel(true));
        let resp_ext = app.oneshot(pin_via_ext).await.unwrap();
        assert_eq!(resp_ext.status(), StatusCode::OK);
        let cookie_ext = resp_ext.headers().get("set-cookie").unwrap().to_str().unwrap();
        assert!(cookie_ext.contains("; Secure"));
    }

    #[test]
    fn enforce_host_allowed_covers_loopback_lan_tunnel_and_rejects_evil() {
        let mk = |host: &str| {
            let mut h = HeaderMap::new();
            h.insert("host", host.parse().unwrap());
            h
        };

        for ok_host in [
            "127.0.0.1:4310",
            "127.0.0.1",
            "localhost:4310",
            "LOCALHOST",
            "[::1]:4310",
            "[::1]",
            "192.168.1.42:4310",
            "10.0.0.5:4310",
            "172.16.8.1:4310",
            "169.254.1.2:4310",
            "[fd12::1]:4310",
            "[fe80::1]:4310",
        ] {
            assert_eq!(
                enforce_host_allowed(&mk(ok_host), None),
                Ok(()),
                "expected allowed host: {ok_host}"
            );
        }

        assert_eq!(
            enforce_host_allowed(&mk("station.example.com"), Some("station.example.com")),
            Ok(())
        );
        assert_eq!(
            enforce_host_allowed(&mk("station.example.com:443"), Some("station.example.com")),
            Ok(())
        );

        for bad_host in [
            "evil.com:4310",
            "evil.com",
            "localhost.evil.com:4310",
            "8.8.8.8:4310",
            "[2001:4860:4860::8888]:4310",
            "127.0.0.1:abc",
            "127.0.0.1:",
            "localhost@evil.com",
            "",
        ] {
            assert_eq!(
                enforce_host_allowed(&mk(bad_host), Some("station.example.com")),
                Err(StatusCode::MISDIRECTED_REQUEST),
                "expected 421 for host: {bad_host}"
            );
        }
    }

    #[test]
    fn enforce_same_origin_post_requires_exact_host_and_port_match() {
        let mk = |host: &str, origin: Option<&str>| {
            let mut h = HeaderMap::new();
            h.insert("host", host.parse().unwrap());
            if let Some(o) = origin {
                h.insert("origin", o.parse().unwrap());
            }
            h
        };

        assert_eq!(enforce_same_origin_post(&mk("127.0.0.1:4310", None), None), Ok(()));
        assert_eq!(
            enforce_same_origin_post(&mk("127.0.0.1:4310", Some("http://127.0.0.1:4310")), None),
            Ok(())
        );
        assert_eq!(
            enforce_same_origin_post(&mk("localhost", Some("http://localhost:80")), None),
            Ok(())
        );
        assert_eq!(
            enforce_same_origin_post(&mk("[::1]:4310", Some("http://[::1]:4310")), None),
            Ok(())
        );

        for (host, origin) in [
            ("127.0.0.1:4310", "http://localhost:5173"),
            ("localhost:4310", "http://localhost:5173"),
            ("127.0.0.1:4310", "http://localhost:4310"),
            ("127.0.0.1:4310", "http://127.0.0.1"),
            ("[::1]:4310", "http://[::1]:5173"),
            ("127.0.0.1:4310", "null"),
            ("127.0.0.1:4310", "http://127.0.0.1:4310/extra"),
            // Scheme confusion against local plain-HTTP server
            ("localhost", "https://localhost:443"),
            ("127.0.0.1:4310", "https://127.0.0.1:4310"),
        ] {
            assert_eq!(
                enforce_same_origin_post(&mk(host, Some(origin)), None),
                Err(StatusCode::FORBIDDEN),
                "expected 403 for Host={host} Origin={origin}"
            );
        }

        // HTTPS tunnel transport (via_tunnel = true): requires https + effective port 443
        assert_eq!(
            enforce_same_origin_post_with_transport(
                &mk("station.example.com", Some("https://station.example.com")),
                Some("station.example.com"),
                true,
            ),
            Ok(())
        );
        assert_eq!(
            enforce_same_origin_post_with_transport(
                &mk("station.example.com", Some("https://station.example.com:443")),
                Some("station.example.com"),
                true,
            ),
            Ok(())
        );
        for (host, origin) in [
            ("station.example.com", "http://station.example.com:80"),
            ("station.example.com", "http://station.example.com"),
            ("station.example.com", "https://station.example.com:8443"),
        ] {
            assert_eq!(
                enforce_same_origin_post_with_transport(
                    &mk(host, Some(origin)),
                    Some("station.example.com"),
                    true,
                ),
                Err(StatusCode::FORBIDDEN),
                "expected 403 over tunnel for Host={host} Origin={origin}"
            );
        }
    }

    #[tokio::test]
    async fn missing_host_header_fails_closed_with_421_and_403_on_hooks() {
        let empty = HeaderMap::new();
        assert_eq!(
            enforce_host_allowed(&empty, None),
            Err(StatusCode::MISDIRECTED_REQUEST)
        );
        assert_eq!(
            enforce_hook_loopback(Some(loopback_addr()), false, &empty),
            Err(StatusCode::FORBIDDEN)
        );
        assert_eq!(
            enforce_loopback_strict(Some(loopback_addr()), false, &empty),
            Err(StatusCode::FORBIDDEN)
        );

        let dir = tempfile::tempdir().unwrap();
        let state = Arc::new(AppState::new(dir.path().to_path_buf(), None).unwrap());
        let app = build_router(state);

        let mut req = Request::builder()
            .method("GET")
            .uri("/api/live")
            .body(Body::empty())
            .unwrap();
        req.extensions_mut().insert(ConnectInfo(loopback_addr()));
        let resp = app.oneshot(req).await.unwrap();
        assert_eq!(resp.status(), StatusCode::MISDIRECTED_REQUEST);
    }

    #[test]
    fn enforce_hook_loopback_rejects_origin_tunnel_and_non_loopback() {
        let mut ok_headers = HeaderMap::new();
        ok_headers.insert("host", "127.0.0.1:4310".parse().unwrap());
        assert_eq!(enforce_hook_loopback(Some(loopback_addr()), false, &ok_headers), Ok(()));

        // ViaTunnel rejected
        assert_eq!(
            enforce_hook_loopback(Some(loopback_addr()), true, &ok_headers),
            Err(StatusCode::FORBIDDEN)
        );

        // Non-loopback peer rejected
        let lan_peer = SocketAddr::new(IpAddr::V4(Ipv4Addr::new(192, 168, 1, 20)), 54321);
        assert_eq!(
            enforce_hook_loopback(Some(lan_peer), false, &ok_headers),
            Err(StatusCode::FORBIDDEN)
        );

        // Non-loopback Host (even private LAN IP) rejected on hooks
        let mut lan_host = HeaderMap::new();
        lan_host.insert("host", "192.168.1.20:4310".parse().unwrap());
        assert_eq!(
            enforce_hook_loopback(Some(loopback_addr()), false, &lan_host),
            Err(StatusCode::FORBIDDEN)
        );

        // Any Origin header rejected
        let mut with_origin = ok_headers.clone();
        with_origin.insert("origin", "http://127.0.0.1:4310".parse().unwrap());
        assert_eq!(
            enforce_hook_loopback(Some(loopback_addr()), false, &with_origin),
            Err(StatusCode::FORBIDDEN)
        );
    }
}
