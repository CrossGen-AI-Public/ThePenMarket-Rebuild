//! Security middleware: response headers, CSRF (double-submit cookie signed with HMAC), and
//! the trailing-slash canonicaliser. The router defines every public path explicitly.

use crate::app::State;
use axum::body::Body;
use axum::extract::Request;
use axum::http::{header, HeaderValue, StatusCode, Uri};
use axum::middleware::Next;
use axum::response::{IntoResponse, Response};
use hmac::{Hmac, Mac};
use rand::RngCore;
use sha2::Sha256;

pub const CSRF_COOKIE: &str = "pm_csrf";
/// On HTTPS the CSRF cookie carries the `__Host-` prefix, so a sibling subdomain cannot plant one.
pub const CSRF_COOKIE_HOST: &str = "__Host-pm_csrf";

tokio::task_local! {
    /// This request's form token, so every page can render forms without each handler asking.
    static FORM_TOKEN: String;
}

/// The CSRF form token for the current request ("" outside a request).
pub fn form_token() -> String {
    FORM_TOKEN.try_with(|t| t.clone()).unwrap_or_default()
}

// ---- trusted proxies ------------------------------------------------------------------------------

static TRUSTED: std::sync::OnceLock<Vec<(std::net::IpAddr, u8)>> = std::sync::OnceLock::new();

/// `TRUSTED_PROXIES`: comma-separated addresses or CIDR ranges (e.g. `172.16.0.0/12` for Caddy on a
/// Docker network). Only requests arriving from these may set X-Forwarded-For / -Proto. Unset: the
/// loopback addresses only.
pub fn init_trusted_proxies(spec: &str) {
    let mut v = vec![];
    for part in spec.split(',').map(str::trim).filter(|p| !p.is_empty()) {
        let (ip, bits) = part.split_once('/').map(|(a, b)| (a, b.parse::<u8>().ok())).unwrap_or((part, None));
        if let Ok(ip) = ip.parse::<std::net::IpAddr>() {
            let max = if ip.is_ipv4() { 32 } else { 128 };
            v.push((ip, bits.unwrap_or(max).min(max)));
        } else {
            tracing::warn!("TRUSTED_PROXIES: ignoring {part:?}");
        }
    }
    if v.is_empty() {
        v = vec![(std::net::IpAddr::V4(std::net::Ipv4Addr::LOCALHOST), 32), (std::net::IpAddr::V6(std::net::Ipv6Addr::LOCALHOST), 128)];
    }
    let _ = TRUSTED.set(v);
}

fn in_range(ip: std::net::IpAddr, net: std::net::IpAddr, bits: u8) -> bool {
    use std::net::IpAddr::{V4, V6};
    match (ip, net) {
        (V4(a), V4(b)) => {
            let m = if bits == 0 { 0 } else { u32::MAX << (32 - bits as u32) };
            (u32::from(a) & m) == (u32::from(b) & m)
        }
        (V6(a), V6(b)) => {
            let m = if bits == 0 { 0 } else { u128::MAX << (128 - bits as u32) };
            (u128::from(a) & m) == (u128::from(b) & m)
        }
        (V6(a), V4(_)) => a.to_ipv4_mapped().map(|a4| in_range(V4(a4), net, bits)).unwrap_or(false),
        _ => false,
    }
}

pub fn is_trusted_proxy(peer: std::net::IpAddr) -> bool {
    let list = TRUSTED.get_or_init(|| vec![(std::net::IpAddr::V4(std::net::Ipv4Addr::LOCALHOST), 32), (std::net::IpAddr::V6(std::net::Ipv6Addr::LOCALHOST), 128)]);
    list.iter().any(|(net, bits)| in_range(peer, *net, *bits))
}

/// Outermost layer: X-Forwarded-For / -Proto / -Host and X-Real-IP are believed only from a trusted
/// proxy. From anyone else they are removed, so a client talking to the app port directly cannot pick
/// its own IP for the rate limiters or claim HTTPS.
pub async fn proxy_headers(mut req: Request, next: Next) -> Response {
    let peer = req.extensions().get::<axum::extract::ConnectInfo<std::net::SocketAddr>>().map(|c| c.0.ip());
    if !peer.map(is_trusted_proxy).unwrap_or(false) {
        let h = req.headers_mut();
        for k in ["x-forwarded-for", "x-forwarded-proto", "x-forwarded-host", "x-real-ip", "forwarded"] {
            h.remove(k);
        }
    }
    next.run(req).await
}

/// The visitor's CSRF cookie value (set by the middleware if absent).
#[derive(Clone, Debug)]
pub struct CsrfCookie(pub String);

fn cookie_value(req: &Request, name: &str) -> Option<String> {
    let raw = req.headers().get(header::COOKIE)?.to_str().ok()?;
    raw.split(';').map(|c| c.trim()).find_map(|c| c.strip_prefix(&format!("{name}=")).map(|v| v.to_string()))
}

fn is_https(req: &Request) -> bool {
    // Only a trusted proxy's header survives `proxy_headers`.
    req.headers().get("x-forwarded-proto").and_then(|v| v.to_str().ok()).map(|v| v.eq_ignore_ascii_case("https")).unwrap_or(false)
}

/// Sign the cookie value so the hidden form field cannot be forged without the cookie.
pub fn csrf_token(secret: &str, cookie: &str) -> String {
    let mut mac = Hmac::<Sha256>::new_from_slice(secret.as_bytes()).unwrap_or_else(|_| Hmac::<Sha256>::new_from_slice(b"fallback-secret").unwrap_or_else(|_| unreachable!("hmac accepts any key length")));
    mac.update(cookie.as_bytes());
    hex::encode(mac.finalize().into_bytes())
}

fn eq_ct(a: &str, b: &str) -> bool {
    a.len() == b.len() && a.bytes().zip(b.bytes()).fold(0u8, |acc, (x, y)| acc | (x ^ y)) == 0
}

/// The hidden field (or X-CSRF header) must be the HMAC of this visitor's cookie, rendered by the
/// server into the page. The raw cookie value is not accepted: a cookie planted by another site on the
/// same domain must not be enough to forge a form.
pub fn csrf_ok(secret: &str, cookie: &str, field: &str) -> bool {
    if cookie.len() < 32 {
        return false;
    }
    eq_ct(&csrf_token(secret, cookie), field)
}

/// Ensures every visitor has a CSRF cookie and makes it available to handlers as an extension, and
/// this request's form token to every template (`PageMeta.csrf`).
pub async fn csrf_cookie(axum::extract::State(state): axum::extract::State<State>, mut req: Request, next: Next) -> Response {
    let secure = is_https(&req);
    let name = if secure { CSRF_COOKIE_HOST } else { CSRF_COOKIE };
    let (value, fresh) = match cookie_value(&req, name).filter(|v| v.len() >= 32 && v.chars().all(|c| c.is_ascii_hexdigit())) {
        Some(v) => (v, false),
        None => {
            let mut bytes = [0u8; 24];
            rand::rng().fill_bytes(&mut bytes);
            (hex::encode(bytes), true)
        }
    };
    req.extensions_mut().insert(CsrfCookie(value.clone()));
    let token = csrf_token(&state.cfg.csrf_secret, &value);
    let mut res = FORM_TOKEN.scope(token, next.run(req)).await;
    if fresh {
        let cookie = format!("{name}={value}; Path=/; SameSite=Lax; HttpOnly; Max-Age=31536000{}", if secure { "; Secure" } else { "" });
        if let Ok(hv) = HeaderValue::from_str(&cookie) {
            res.headers_mut().append(header::SET_COOKIE, hv);
        }
    }
    res
}

/// A form or API call from another site is refused before any handler sees it. Browsers say where a
/// request came from (Sec-Fetch-Site, else Origin); a request that says neither (curl, Stripe's
/// webhook, very old browsers) still needs its CSRF token or signature further in.
pub async fn same_origin(axum::extract::State(state): axum::extract::State<State>, req: Request, next: Next) -> Response {
    let m = req.method();
    if !(m == axum::http::Method::GET || m == axum::http::Method::HEAD || m == axum::http::Method::OPTIONS) && req.uri().path() != "/api/stripe/webhook" {
        let h = req.headers();
        let site = h.get("sec-fetch-site").and_then(|v| v.to_str().ok());
        let refused = match site {
            Some(s) => !s.eq_ignore_ascii_case("same-origin"),
            None => h.get(header::ORIGIN).and_then(|v| v.to_str().ok()).map(|o| o != "null" && o.trim_end_matches('/') != state.cfg.site_origin.trim_end_matches('/')).unwrap_or(false),
        };
        if refused {
            return (StatusCode::FORBIDDEN, "This form was sent from another website, so it was refused.").into_response();
        }
    }
    next.run(req).await
}

/// CSP, HSTS, nosniff, frame-deny, referrer, permissions and cross-origin policies on every response;
/// admin pages are never cached or indexed.
pub async fn security_headers(req: Request, next: Next) -> Response {
    let path = req.uri().path().to_string();
    let mut res = next.run(req).await;
    let h = res.headers_mut();
    if path.starts_with("/admin") {
        h.insert(header::CACHE_CONTROL, HeaderValue::from_static("no-store"));
        h.insert("x-robots-tag", HeaderValue::from_static("noindex, nofollow"));
    }
    // Photos may be shown by other sites and mail programs (the buyer's receipt); nothing else.
    let corp = if path.starts_with("/media/") || path.starts_with("/static/img/") || path.starts_with("/wp-content/") { "cross-origin" } else { "same-origin" };
    if let Ok(v) = HeaderValue::from_str(corp) {
        h.entry("cross-origin-resource-policy").or_insert(v);
    }
    let csp = "default-src 'self'; img-src 'self' data: https://thepenmarket.com; style-src 'self' 'unsafe-inline'; script-src 'self'; font-src 'self'; connect-src 'self'; frame-src https://www.youtube.com https://www.youtube-nocookie.com; frame-ancestors 'none'; base-uri 'self'; form-action 'self' https://checkout.stripe.com; object-src 'none'";
    let set = |h: &mut axum::http::HeaderMap, k: &'static str, v: &'static str| {
        if let Ok(val) = HeaderValue::from_str(v) {
            h.entry(k).or_insert(val);
        }
    };
    set(h, "content-security-policy", csp);
    set(h, "strict-transport-security", "max-age=31536000; includeSubDomains");
    set(h, "x-content-type-options", "nosniff");
    set(h, "x-frame-options", "DENY");
    set(h, "referrer-policy", "strict-origin-when-cross-origin");
    set(h, "permissions-policy", "camera=(), microphone=(), geolocation=(), payment=(), usb=(), browsing-topics=()");
    set(h, "cross-origin-opener-policy", "same-origin");
    res
}

/// Old WordPress URLs end in a slash, and the sitemaps keep that shape. Anything without one that is
/// not a file, an API route or a health check gets a 301 to the slashed form.
pub async fn trailing_slash(req: Request, next: Next) -> Response {
    let uri = req.uri().clone();
    let path = uri.path();
    // "//evil.com/x" would become a redirect to another site; a backslash is never a real address here.
    if path.starts_with("//") || path.contains('\\') {
        return (StatusCode::NOT_FOUND, "Not found").into_response();
    }
    let last = path.rsplit('/').next().unwrap_or("");
    let exempt = path.starts_with("/api/") || path.starts_with("/static/") || path.starts_with("/media/") || path.starts_with("/uploads/") || path == "/healthz" || path.starts_with("/wp-content/");
    if !path.ends_with('/') && !exempt && !last.contains('.') {
        let target = match uri.query() {
            Some(q) => format!("{path}/?{q}"),
            None => format!("{path}/"),
        };
        if target.parse::<Uri>().is_ok() {
            return crate::app::moved(&target);
        }
    }
    next.run(req).await
}

/// Reject bodies larger than the route allows with a plain 413 instead of a hung connection.
pub fn too_large() -> Response {
    (StatusCode::PAYLOAD_TOO_LARGE, "that upload is too large").into_response()
}

/// The visitor's address for rate limits and the audit log. X-Forwarded-For only survives
/// `proxy_headers` when a trusted proxy sent it; its last entry is the one that proxy saw. An IPv6
/// visitor is keyed by their /64, since one household or phone gets a whole /64.
pub fn client_ip(req_headers: &axum::http::HeaderMap, peer: std::net::IpAddr) -> std::net::IpAddr {
    let ip = req_headers
        .get("x-forwarded-for")
        .and_then(|v| v.to_str().ok())
        .and_then(|v| v.rsplit(',').next())
        .and_then(|v| v.trim().parse().ok())
        .unwrap_or(peer);
    match ip {
        std::net::IpAddr::V6(v6) if v6.to_ipv4_mapped().is_none() => std::net::IpAddr::V6(std::net::Ipv6Addr::from(u128::from(v6) & (u128::MAX << 64))),
        std::net::IpAddr::V6(v6) => v6.to_ipv4_mapped().map(std::net::IpAddr::V4).unwrap_or(ip),
        _ => ip,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn ranges_and_ipv6_keys() {
        let n: std::net::IpAddr = "172.16.0.0".parse().unwrap();
        assert!(in_range("172.18.0.5".parse().unwrap(), n, 12));
        assert!(!in_range("172.32.0.1".parse().unwrap(), n, 12));
        let mut h = axum::http::HeaderMap::new();
        h.insert("x-forwarded-for", "9.9.9.9, 1.2.3.4".parse().unwrap());
        assert_eq!(client_ip(&h, "10.0.0.1".parse().unwrap()).to_string(), "1.2.3.4");
        let a = client_ip(&axum::http::HeaderMap::new(), "2001:db8:1:2:aaaa::1".parse().unwrap());
        let b = client_ip(&axum::http::HeaderMap::new(), "2001:db8:1:2:bbbb::9".parse().unwrap());
        assert_eq!(a, b);
    }
    #[test]
    fn raw_cookie_is_not_a_token() {
        let c = "a".repeat(48);
        assert!(!csrf_ok("secret", &c, &c));
        assert!(csrf_ok("secret", &c, &csrf_token("secret", &c)));
    }
}

#[allow(dead_code)]
pub fn empty_body() -> Body {
    Body::empty()
}

#[allow(dead_code)]
pub fn state_marker(_s: &State) {}
