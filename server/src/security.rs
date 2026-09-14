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

/// The visitor's CSRF cookie value (set by the middleware if absent).
#[derive(Clone, Debug)]
pub struct CsrfCookie(pub String);

fn cookie_value(req: &Request, name: &str) -> Option<String> {
    let raw = req.headers().get(header::COOKIE)?.to_str().ok()?;
    raw.split(';').map(|c| c.trim()).find_map(|c| c.strip_prefix(&format!("{name}=")).map(|v| v.to_string()))
}

fn is_https(req: &Request) -> bool {
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

/// The hidden field must be either the HMAC token (rendered server-side on form pages) or the raw
/// cookie value (copied in by the page script for the footer form): both prove the sender can read
/// this site's cookie, which a cross-site form cannot.
pub fn csrf_ok(secret: &str, cookie: &str, field: &str) -> bool {
    if cookie.len() < 32 {
        return false;
    }
    eq_ct(&csrf_token(secret, cookie), field) || eq_ct(cookie, field)
}

/// Ensures every visitor has a CSRF cookie and makes it available to handlers as an extension.
pub async fn csrf_cookie(mut req: Request, next: Next) -> Response {
    let (value, fresh) = match cookie_value(&req, CSRF_COOKIE).filter(|v| v.len() >= 32 && v.chars().all(|c| c.is_ascii_hexdigit())) {
        Some(v) => (v, false),
        None => {
            let mut bytes = [0u8; 24];
            rand::rng().fill_bytes(&mut bytes);
            (hex::encode(bytes), true)
        }
    };
    let secure = is_https(&req);
    req.extensions_mut().insert(CsrfCookie(value.clone()));
    let mut res = next.run(req).await;
    if fresh {
        let cookie = format!("{CSRF_COOKIE}={value}; Path=/; SameSite=Lax; Max-Age=31536000{}", if secure { "; Secure" } else { "" });
        if let Ok(hv) = HeaderValue::from_str(&cookie) {
            res.headers_mut().append(header::SET_COOKIE, hv);
        }
    }
    res
}

/// CSP, HSTS, nosniff, frame-deny, referrer and permissions policies on every response.
pub async fn security_headers(req: Request, next: Next) -> Response {
    let mut res = next.run(req).await;
    let h = res.headers_mut();
    let csp = "default-src 'self'; img-src 'self' data: https://thepenmarket.com; style-src 'self' 'unsafe-inline'; script-src 'self'; font-src 'self'; connect-src 'self'; frame-src https://www.youtube.com https://www.youtube-nocookie.com; frame-ancestors 'none'; base-uri 'self'; form-action 'self'; object-src 'none'";
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
    set(h, "permissions-policy", "camera=(), microphone=(), geolocation=(), payment=()");
    res
}

/// Old WordPress URLs end in a slash, and the sitemaps keep that shape. Anything without one that is
/// not a file, an API route or a health check gets a 301 to the slashed form.
pub async fn trailing_slash(req: Request, next: Next) -> Response {
    let uri = req.uri().clone();
    let path = uri.path();
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

pub fn client_ip(req_headers: &axum::http::HeaderMap, peer: std::net::IpAddr) -> std::net::IpAddr {
    req_headers
        .get("x-forwarded-for")
        .and_then(|v| v.to_str().ok())
        .and_then(|v| v.split(',').next())
        .and_then(|v| v.trim().parse().ok())
        .unwrap_or(peer)
}

#[allow(dead_code)]
pub fn empty_body() -> Body {
    Body::empty()
}

#[allow(dead_code)]
pub fn state_marker(_s: &State) {}
