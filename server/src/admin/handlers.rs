//! Admin HTML pages: sign-in, new-device code, forgot / reset, devices, history, preview toggle.

use super::{auth, mail, store, Auth, AdminCtx, DEVICE_COOKIE, PENDING_COOKIE, SESSION_COOKIE};
use crate::app::{html, AppError, AppResult, PageMeta, State};
use crate::security::{self, CsrfCookie};
use askama::Template;
use axum::extract::{ConnectInfo, Form, Path, State as AxState};
use axum::http::{header, HeaderMap, StatusCode};
use axum::response::{IntoResponse, Redirect, Response};
use axum::Extension;
use chrono::Utc;
use hmac::{Hmac, Mac};
use serde::Deserialize;
use sha2::Sha256;
use std::net::SocketAddr;

#[derive(Template)]
#[template(path = "admin/login.html")]
pub struct LoginTpl {
    pub page: PageMeta,
    pub error: String,
    pub notice: String,
}

#[derive(Template)]
#[template(path = "admin/verify.html")]
pub struct VerifyTpl {
    pub page: PageMeta,
    pub error: String,
    pub email_hint: String,
}

#[derive(Template)]
#[template(path = "admin/forgot.html")]
pub struct ForgotTpl {
    pub page: PageMeta,
    pub sent: bool,
}

#[derive(Template)]
#[template(path = "admin/reset.html")]
pub struct ResetTpl {
    pub page: PageMeta,
    pub token: String,
    pub error: String,
    pub invalid: bool,
}

#[derive(Template)]
#[template(path = "admin/devices.html")]
pub struct DevicesTpl {
    pub page: PageMeta,
    pub admin: AdminCtx,
    pub devices: Vec<DeviceView>,
}

pub struct DeviceView {
    pub id: i32,
    pub name: String,
    pub ip: String,
    pub first_seen: String,
    pub last_seen: String,
    pub expires: String,
}

#[derive(Template)]
#[template(path = "admin/history.html")]
pub struct HistoryTpl {
    pub page: PageMeta,
    pub admin: AdminCtx,
    pub rows: Vec<HistoryView>,
}

pub struct HistoryView {
    pub when: String,
    pub what: String,
    pub detail: String,
    pub from: String,
}

fn meta(state: &State, title: &str, path: &str, cookie: &CsrfCookie) -> PageMeta {
    let mut page = PageMeta::new(state, title, "Admin area for ThePenMarket.com.", path);
    page.noindex = true;
    page.section = "admin".into();
    page.csrf = security::csrf_token(&state.cfg.csrf_secret, &cookie.0);
    page
}

fn ip_of(headers: &HeaderMap, peer: SocketAddr) -> String {
    security::client_ip(headers, peer.ip()).to_string()
}

fn ua_of(headers: &HeaderMap) -> String {
    headers.get(header::USER_AGENT).and_then(|v| v.to_str().ok()).unwrap_or("").to_string()
}

/// Per-address throttle on every sign-in surface. The page is the sign-in form with a plain sentence,
/// not a bare error, because the person reading it is usually just typing too fast.
fn rate_limited(state: &State, headers: &HeaderMap, peer: SocketAddr, cookie: &CsrfCookie) -> Option<Response> {
    let ip = security::client_ip(headers, peer.ip());
    if state.admin_limiter.allow(ip) {
        return None;
    }
    let tpl = LoginTpl { page: meta(state, "Sign in | ThePenMarket.com", "/admin/login/", cookie), error: "Too many tries in a row. Wait a minute, then try again.".into(), notice: String::new() };
    Some(crate::app::html_status(&tpl, StatusCode::TOO_MANY_REQUESTS).unwrap_or_else(|e| e.into_response()))
}

fn hide_email(email: &str) -> String {
    let mut it = email.splitn(2, '@');
    let user = it.next().unwrap_or("");
    let dom = it.next().unwrap_or("");
    let shown: String = user.chars().take(3).collect();
    format!("{shown}•••@{dom}")
}

/// The pending-verification cookie carries the code row id, signed with the CSRF secret.
fn sign_pending(secret: &str, code_id: i32, account_id: i32) -> String {
    let payload = format!("{code_id}.{account_id}");
    let mut mac = Hmac::<Sha256>::new_from_slice(secret.as_bytes()).unwrap_or_else(|_| Hmac::<Sha256>::new_from_slice(b"fallback").unwrap_or_else(|_| unreachable!()));
    mac.update(payload.as_bytes());
    format!("{payload}.{}", hex::encode(mac.finalize().into_bytes()))
}

fn read_pending(secret: &str, value: &str) -> Option<(i32, i32)> {
    let mut parts = value.rsplitn(2, '.');
    let sig = parts.next()?;
    let payload = parts.next()?;
    let mut mac = Hmac::<Sha256>::new_from_slice(secret.as_bytes()).ok()?;
    mac.update(payload.as_bytes());
    let expect = hex::encode(mac.finalize().into_bytes());
    if expect.len() != sig.len() || expect.bytes().zip(sig.bytes()).fold(0u8, |a, (x, y)| a | (x ^ y)) != 0 {
        return None;
    }
    let mut p = payload.splitn(2, '.');
    Some((p.next()?.parse().ok()?, p.next()?.parse().ok()?))
}

fn with_cookies(mut res: Response, cookies: &[String]) -> Response {
    for c in cookies {
        if let Ok(v) = header::HeaderValue::from_str(c) {
            res.headers_mut().append(header::SET_COOKIE, v);
        }
    }
    res
}

pub async fn home(AxState(_state): AxState<State>, admin: Option<Extension<AdminCtx>>) -> AppResult {
    Ok(match admin {
        Some(_) => Redirect::to("/shop/").into_response(),
        None => Redirect::to("/admin/login/").into_response(),
    })
}

pub async fn login_form(AxState(state): AxState<State>, Extension(cookie): Extension<CsrfCookie>, admin: Option<Extension<AdminCtx>>, axum::extract::Query(q): axum::extract::Query<std::collections::HashMap<String, String>>) -> AppResult {
    if admin.is_some() {
        return Ok(Redirect::to("/shop/").into_response());
    }
    let notice = match q.get("notice").map(String::as_str) {
        Some("reset") => "Your password was changed. Sign in with the new one.".to_string(),
        Some("out") => "You are signed out.".to_string(),
        Some("expired") => "Your session ended. Sign in again.".to_string(),
        _ => String::new(),
    };
    html(&LoginTpl { page: meta(&state, "Sign in | ThePenMarket.com", "/admin/login/", &cookie), error: String::new(), notice })
}

#[derive(Deserialize)]
pub struct LoginForm {
    pub email: String,
    pub password: String,
    pub csrf: String,
}

const WRONG: &str = "That e-mail or password is wrong.";

pub async fn login(AxState(state): AxState<State>, Extension(cookie): Extension<CsrfCookie>, ConnectInfo(peer): ConnectInfo<SocketAddr>, headers: HeaderMap, Form(form): Form<LoginForm>) -> AppResult {
    if let Some(r) = rate_limited(&state, &headers, peer, &cookie) {
        return Ok(r);
    }
    if !security::csrf_ok(&state.cfg.csrf_secret, &cookie.0, &form.csrf) {
        return Err(AppError::Forbidden("The form expired. Reload the page and try again.".into()));
    }
    let https = super::is_https(&headers);
    let ip = ip_of(&headers, peer);
    let ua = ua_of(&headers);
    let page = meta(&state, "Sign in | ThePenMarket.com", "/admin/login/", &cookie);
    let acc = store::account_by_email(&state.pool, &form.email).await?;
    let (ok, acc) = match acc {
        Some(a) => {
            if a.locked_until.map(|t| t > Utc::now()).unwrap_or(false) {
                return html(&LoginTpl { page, error: "This sign-in is locked for an hour after too many wrong passwords. You can use Forgot password.".into(), notice: String::new() });
            }
            (auth::verify_password(&form.password, &a.password_hash), Some(a))
        }
        None => {
            auth::verify_password(&form.password, &auth::dummy_hash());
            (false, None)
        }
    };
    let Some(acc) = acc else {
        return html(&LoginTpl { page, error: WRONG.into(), notice: String::new() });
    };
    if !ok {
        let locked = store::record_failure(&state.pool, acc.id).await?;
        store::audit(&state.pool, "admin.login_failed", serde_json::json!({"email": acc.email, "ip": ip, "locked": locked})).await?;
        if locked {
            let _ = state.mailer.send(mail::lockout_message(&acc.email, &acc.email, &ip)).await;
            if !state.cfg.admin_alert_email.is_empty() {
                let _ = state.mailer.send(mail::lockout_message(&state.cfg.admin_alert_email, &acc.email, &ip)).await;
            }
        }
        return html(&LoginTpl { page, error: WRONG.into(), notice: String::new() });
    }
    store::clear_failures(&state.pool, acc.id).await?;

    // Known device: straight in. New device: e-mail a code and hold a signed pending cookie.
    let dev_name = auth::cookie_name(https, DEVICE_COOKIE);
    let device = match super::cookie(&headers, &dev_name) {
        Some(t) if t.len() == 64 => store::device_by_token(&state.pool, acc.id, &auth::sha256_hex(&t)).await?,
        _ => None,
    };
    if let Some(d) = device {
        let token = auth::random_token(32);
        store::create_session(&state.pool, acc.id, &auth::sha256_hex(&token), Some(d.id), &ip, &ua, true).await?;
        store::audit(&state.pool, "admin.login", serde_json::json!({"email": acc.email, "ip": ip, "device": d.name})).await?;
        let c = auth::set_cookie(&auth::cookie_name(https, SESSION_COOKIE), &token, https, auth::SESSION_ABSOLUTE_HOURS * 3600);
        return Ok(with_cookies(Redirect::to("/shop/").into_response(), &[c]));
    }
    let code = auth::six_digit_code();
    let code_id = store::create_code(&state.pool, acc.id, "device", &auth::sha256_hex(&code), auth::CODE_MIN, &ip).await?;
    state.mailer.send(mail::code_message(&acc.email, &code, &auth::device_name(&ua))).await?;
    store::audit(&state.pool, "admin.code_sent", serde_json::json!({"email": acc.email, "ip": ip, "device": auth::device_name(&ua)})).await?;
    let pending = auth::set_cookie(&auth::cookie_name(https, PENDING_COOKIE), &sign_pending(&state.cfg.csrf_secret, code_id, acc.id), https, auth::CODE_MIN * 60);
    Ok(with_cookies(Redirect::to("/admin/verify/").into_response(), &[pending]))
}

pub async fn verify_form(AxState(state): AxState<State>, Extension(cookie): Extension<CsrfCookie>, headers: HeaderMap) -> AppResult {
    let https = super::is_https(&headers);
    let Some((_, account_id)) = super::cookie(&headers, &auth::cookie_name(https, PENDING_COOKIE)).and_then(|v| read_pending(&state.cfg.csrf_secret, &v)) else {
        return Ok(Redirect::to("/admin/login/").into_response());
    };
    let email = store::account_by_id(&state.pool, account_id).await?.map(|a| a.email).unwrap_or_default();
    html(&VerifyTpl { page: meta(&state, "Is this you? | ThePenMarket.com", "/admin/verify/", &cookie), error: String::new(), email_hint: hide_email(&email) })
}

#[derive(Deserialize)]
pub struct VerifyForm {
    pub code: String,
    pub csrf: String,
    #[serde(default)]
    pub remember: Option<String>,
}

pub async fn verify(AxState(state): AxState<State>, Extension(cookie): Extension<CsrfCookie>, ConnectInfo(peer): ConnectInfo<SocketAddr>, headers: HeaderMap, Form(form): Form<VerifyForm>) -> AppResult {
    if let Some(r) = rate_limited(&state, &headers, peer, &cookie) {
        return Ok(r);
    }
    if !security::csrf_ok(&state.cfg.csrf_secret, &cookie.0, &form.csrf) {
        return Err(AppError::Forbidden("The form expired. Reload the page and try again.".into()));
    }
    let https = super::is_https(&headers);
    let ip = ip_of(&headers, peer);
    let ua = ua_of(&headers);
    let Some((code_id, account_id)) = super::cookie(&headers, &auth::cookie_name(https, PENDING_COOKIE)).and_then(|v| read_pending(&state.cfg.csrf_secret, &v)) else {
        return Ok(Redirect::to("/admin/login/").into_response());
    };
    let page = meta(&state, "Is this you? | ThePenMarket.com", "/admin/verify/", &cookie);
    let acc = store::account_by_id(&state.pool, account_id).await?.ok_or(AppError::NotFound)?;
    let hint = hide_email(&acc.email);
    let Some(code) = store::code_by_id(&state.pool, code_id, "device").await? else {
        return html(&VerifyTpl { page, error: "That code is no longer valid. Sign in again to get a new one.".into(), email_hint: hint });
    };
    let typed: String = form.code.chars().filter(|c| c.is_ascii_digit()).collect();
    let good = code.account_id == account_id && code.used_at.is_none() && code.expires_at > Utc::now() && code.attempts < auth::CODE_ATTEMPTS && auth::sha256_hex(&typed) == code.code_hash;
    if !good {
        let n = store::bump_code_attempts(&state.pool, code.id).await?;
        store::audit(&state.pool, "admin.code_failed", serde_json::json!({"email": acc.email, "ip": ip, "attempts": n})).await?;
        let msg = if n >= auth::CODE_ATTEMPTS || code.expires_at <= Utc::now() { "That code is no longer valid. Sign in again to get a new one." } else { "That code isn't right. Check the e-mail and try again." };
        return html(&VerifyTpl { page, error: msg.into(), email_hint: hint });
    }
    store::use_code(&state.pool, code.id).await?;
    let mut cookies = vec![auth::clear_cookie(&auth::cookie_name(https, PENDING_COOKIE), https)];
    let mut device_id = None;
    if form.remember.is_some() {
        let dtoken = auth::random_token(32);
        let id = store::create_device(&state.pool, acc.id, &auth::sha256_hex(&dtoken), &auth::device_name(&ua), &ip).await?;
        cookies.push(auth::set_cookie(&auth::cookie_name(https, DEVICE_COOKIE), &dtoken, https, auth::DEVICE_DAYS * 86400));
        device_id = Some(id);
    }
    let token = auth::random_token(32);
    store::create_session(&state.pool, acc.id, &auth::sha256_hex(&token), device_id, &ip, &ua, true).await?;
    cookies.push(auth::set_cookie(&auth::cookie_name(https, SESSION_COOKIE), &token, https, auth::SESSION_ABSOLUTE_HOURS * 3600));
    store::audit(&state.pool, "admin.login", serde_json::json!({"email": acc.email, "ip": ip, "device": auth::device_name(&ua), "new_device": true, "remembered": device_id.is_some()})).await?;
    Ok(with_cookies(Redirect::to("/shop/").into_response(), &cookies))
}

#[derive(Deserialize)]
pub struct CsrfOnly {
    pub csrf: String,
}

pub async fn logout(AxState(state): AxState<State>, Extension(cookie): Extension<CsrfCookie>, admin: Option<Extension<AdminCtx>>, headers: HeaderMap, Form(form): Form<CsrfOnly>) -> AppResult {
    if !security::csrf_ok(&state.cfg.csrf_secret, &cookie.0, &form.csrf) {
        return Err(AppError::Forbidden("The form expired.".into()));
    }
    let https = super::is_https(&headers);
    if let Some(Extension(a)) = admin {
        store::revoke_session(&state.pool, a.session_id).await?;
        store::audit(&state.pool, "admin.logout", serde_json::json!({"email": a.email})).await?;
    }
    Ok(with_cookies(Redirect::to("/admin/login/?notice=out").into_response(), &[auth::clear_cookie(&auth::cookie_name(https, SESSION_COOKIE), https)]))
}

pub async fn forgot_form(AxState(state): AxState<State>, Extension(cookie): Extension<CsrfCookie>) -> AppResult {
    html(&ForgotTpl { page: meta(&state, "Forgot password | ThePenMarket.com", "/admin/forgot/", &cookie), sent: false })
}

#[derive(Deserialize)]
pub struct ForgotForm {
    pub email: String,
    pub csrf: String,
}

/// Always answers the same way, so the form never reveals whether an address is ours.
pub async fn forgot(AxState(state): AxState<State>, Extension(cookie): Extension<CsrfCookie>, ConnectInfo(peer): ConnectInfo<SocketAddr>, headers: HeaderMap, Form(form): Form<ForgotForm>) -> AppResult {
    if let Some(r) = rate_limited(&state, &headers, peer, &cookie) {
        return Ok(r);
    }
    if !security::csrf_ok(&state.cfg.csrf_secret, &cookie.0, &form.csrf) {
        return Err(AppError::Forbidden("The form expired. Reload the page and try again.".into()));
    }
    let ip = ip_of(&headers, peer);
    if let Some(acc) = store::account_by_email(&state.pool, &form.email).await? {
        let token = auth::random_token(32);
        store::create_code(&state.pool, acc.id, "reset", &auth::sha256_hex(&token), auth::RESET_MIN, &ip).await?;
        let link = state.abs(&format!("/admin/reset/{token}/"));
        state.mailer.send(mail::reset_message(&acc.email, &link)).await?;
        store::audit(&state.pool, "admin.reset_requested", serde_json::json!({"email": acc.email, "ip": ip})).await?;
    }
    html(&ForgotTpl { page: meta(&state, "Forgot password | ThePenMarket.com", "/admin/forgot/", &cookie), sent: true })
}

async fn reset_code(state: &State, token: &str) -> anyhow::Result<Option<store::Code>> {
    if token.len() != 64 || !token.chars().all(|c| c.is_ascii_hexdigit()) {
        return Ok(None);
    }
    let c = store::code_by_hash(&state.pool, "reset", &auth::sha256_hex(token)).await?;
    Ok(c.filter(|c| c.used_at.is_none() && c.expires_at > Utc::now()))
}

pub async fn reset_form(AxState(state): AxState<State>, Extension(cookie): Extension<CsrfCookie>, Path(token): Path<String>) -> AppResult {
    let valid = reset_code(&state, &token).await?.is_some();
    html(&ResetTpl { page: meta(&state, "Choose a new password | ThePenMarket.com", "/admin/forgot/", &cookie), token: if valid { token } else { String::new() }, error: String::new(), invalid: !valid })
}

#[derive(Deserialize)]
pub struct ResetForm {
    pub password: String,
    pub password2: String,
    pub csrf: String,
}

pub async fn reset(AxState(state): AxState<State>, Extension(cookie): Extension<CsrfCookie>, ConnectInfo(peer): ConnectInfo<SocketAddr>, headers: HeaderMap, Path(token): Path<String>, Form(form): Form<ResetForm>) -> AppResult {
    if let Some(r) = rate_limited(&state, &headers, peer, &cookie) {
        return Ok(r);
    }
    if !security::csrf_ok(&state.cfg.csrf_secret, &cookie.0, &form.csrf) {
        return Err(AppError::Forbidden("The form expired. Reload the page and try again.".into()));
    }
    let page = meta(&state, "Choose a new password | ThePenMarket.com", "/admin/forgot/", &cookie);
    let Some(code) = reset_code(&state, &token).await? else {
        return html(&ResetTpl { page, token: String::new(), error: String::new(), invalid: true });
    };
    let mut error = String::new();
    if form.password != form.password2 {
        error = "The two passwords don't match.".into();
    } else if let Err(e) = auth::password_policy(&form.password) {
        error = e;
    } else if let Some(n) = auth::breached_count(&state.http, &form.password).await {
        if n > 0 {
            error = "That password shows up in lists criminals already have. Pick a different one.".into();
        }
    }
    if !error.is_empty() {
        return html(&ResetTpl { page, token, error, invalid: false });
    }
    let hash = auth::hash_password(&form.password)?;
    store::set_password(&state.pool, code.account_id, &hash).await?;
    store::use_code(&state.pool, code.id).await?;
    store::revoke_all_sessions(&state.pool, code.account_id).await?;
    let email = store::account_by_id(&state.pool, code.account_id).await?.map(|a| a.email).unwrap_or_default();
    store::audit(&state.pool, "admin.password_reset", serde_json::json!({"email": email, "ip": ip_of(&headers, peer)})).await?;
    Ok(Redirect::to("/admin/login/?notice=reset").into_response())
}

fn fmt_time(t: chrono::DateTime<Utc>) -> String {
    t.with_timezone(&chrono_tz_offset()).format("%b %-d, %Y, %-I:%M %p").to_string()
}

fn chrono_tz_offset() -> chrono::FixedOffset {
    // Eastern time for Norwich, CT (DST handled roughly: April–October).
    let m = Utc::now().format("%m").to_string().parse::<u32>().unwrap_or(1);
    let hours = if (4..=10).contains(&m) { -4 } else { -5 };
    chrono::FixedOffset::east_opt(hours * 3600).unwrap_or_else(|| chrono::FixedOffset::east_opt(0).unwrap_or_else(|| unreachable!("zero offset is valid")))
}

pub async fn devices(AxState(state): AxState<State>, Extension(cookie): Extension<CsrfCookie>, Auth(admin): Auth) -> AppResult {
    let rows = store::devices(&state.pool, admin.account_id).await?;
    let devices = rows.into_iter().map(|d| DeviceView { id: d.id, name: d.name, ip: d.ip, first_seen: fmt_time(d.first_seen_at), last_seen: fmt_time(d.last_seen_at), expires: fmt_time(d.expires_at) }).collect();
    html(&DevicesTpl { page: meta(&state, "Devices | ThePenMarket.com", "/admin/devices/", &cookie), admin, devices })
}

pub async fn revoke_device(AxState(state): AxState<State>, Extension(cookie): Extension<CsrfCookie>, Auth(admin): Auth, Path(id): Path<i32>, Form(form): Form<CsrfOnly>) -> AppResult {
    if !security::csrf_ok(&state.cfg.csrf_secret, &cookie.0, &form.csrf) {
        return Err(AppError::Forbidden("The form expired.".into()));
    }
    store::revoke_device(&state.pool, admin.account_id, id).await?;
    store::audit(&state.pool, "admin.device_revoked", serde_json::json!({"email": admin.email, "device_id": id})).await?;
    Ok(Redirect::to("/admin/devices/").into_response())
}

fn describe(kind: &str, d: &serde_json::Value) -> (String, String) {
    let s = |k: &str| d.get(k).and_then(|v| v.as_str()).unwrap_or("").to_string();
    match kind {
        "admin.login" => ("Signed in".into(), if d.get("new_device").and_then(|v| v.as_bool()).unwrap_or(false) { format!("New device ({})", s("device")) } else { s("device") }),
        "admin.login_failed" => ("Wrong password".into(), if d.get("locked").and_then(|v| v.as_bool()).unwrap_or(false) { "Locked for an hour; you and CrossGen were e-mailed".into() } else { String::new() }),
        "admin.code_sent" => ("Sign-in code e-mailed".into(), s("device")),
        "admin.code_failed" => ("Wrong sign-in code".into(), format!("attempt {}", d.get("attempts").and_then(|v| v.as_i64()).unwrap_or(0))),
        "admin.logout" => ("Signed out".into(), String::new()),
        "admin.reset_requested" => ("Password reset link e-mailed".into(), String::new()),
        "admin.password_reset" => ("Password changed".into(), "All other sessions signed out".into()),
        "admin.device_revoked" => ("Device forgotten".into(), String::new()),
        "admin.step_up" => ("Password re-entered".into(), String::new()),
        "admin.preview" => (if d.get("on").and_then(|v| v.as_bool()).unwrap_or(false) { "Preview as customer: on" } else { "Preview as customer: off" }.into(), String::new()),
        "admin.edit" => (format!("{} edited: {}", s("pen"), s("field")), format!("{} → {}", s("before"), s("after"))),
        "admin.create" => (format!("New pen added: {}", s("pen")), String::new()),
        "admin.photo_added" => (format!("Photo added: {}", s("pen")), s("file")),
        "admin.photo_archived" => (format!("Photo removed: {}", s("pen")), s("file")),
        "admin.photos_reordered" => (format!("Photos reordered: {}", s("pen")), String::new()),
        _ => (kind.to_string(), d.to_string()),
    }
}

pub async fn history(AxState(state): AxState<State>, Extension(cookie): Extension<CsrfCookie>, Auth(admin): Auth) -> AppResult {
    let rows = store::history(&state.pool, 300).await?;
    let rows = rows.into_iter().map(|r| { let (what, detail) = describe(&r.kind, &r.detail); HistoryView { when: fmt_time(r.at), what, detail, from: r.detail.get("ip").and_then(|v| v.as_str()).unwrap_or("").to_string() } }).collect();
    html(&HistoryTpl { page: meta(&state, "History | ThePenMarket.com", "/admin/history/", &cookie), admin, rows })
}

pub async fn history_csv(AxState(state): AxState<State>, Auth(_admin): Auth) -> AppResult {
    let rows = store::history(&state.pool, 5000).await?;
    let mut w = csv::Writer::from_writer(vec![]);
    w.write_record(["when", "what", "detail", "from"])?;
    for r in rows {
        let (what, detail) = describe(&r.kind, &r.detail);
        w.write_record([fmt_time(r.at), what, detail, r.detail.get("ip").and_then(|v| v.as_str()).unwrap_or("").to_string()])?;
    }
    let bytes = w.into_inner().map_err(|e| anyhow::anyhow!("csv: {e}"))?;
    Ok(([(header::CONTENT_TYPE, "text/csv; charset=utf-8"), (header::CONTENT_DISPOSITION, "attachment; filename=\"thepenmarket-history.csv\"")], bytes).into_response())
}

#[derive(Deserialize)]
pub struct PreviewForm {
    pub csrf: String,
    pub on: String,
    #[serde(default)]
    pub back: String,
}

pub async fn preview(AxState(state): AxState<State>, Extension(cookie): Extension<CsrfCookie>, Auth(admin): Auth, Form(form): Form<PreviewForm>) -> AppResult {
    if !security::csrf_ok(&state.cfg.csrf_secret, &cookie.0, &form.csrf) {
        return Err(AppError::Forbidden("The form expired.".into()));
    }
    let on = form.on == "1";
    store::set_preview(&state.pool, admin.session_id, on).await?;
    store::audit(&state.pool, "admin.preview", serde_json::json!({"email": admin.email, "on": on})).await?;
    let back = if form.back.starts_with('/') && !form.back.starts_with("//") { form.back } else { "/".to_string() };
    Ok(Redirect::to(&back).into_response())
}

#[derive(Deserialize)]
pub struct StepUpForm {
    pub password: String,
}

/// Re-enter the password before a sensitive change (JSON, called by admin.js).
pub async fn step_up(AxState(state): AxState<State>, Extension(cookie): Extension<CsrfCookie>, super::AuthApi(admin): super::AuthApi, ConnectInfo(peer): ConnectInfo<SocketAddr>, headers: HeaderMap, axum::Json(form): axum::Json<StepUpForm>) -> AppResult {
    if !state.admin_limiter.allow(security::client_ip(&headers, peer.ip())) {
        return Ok(api_err(StatusCode::TOO_MANY_REQUESTS, "Too many tries in a row. Wait a minute, then try again."));
    }
    if !api_csrf_ok(&state, &cookie, &headers) {
        return Ok(api_err(StatusCode::FORBIDDEN, "The page expired. Reload and try again."));
    }
    let acc = store::account_by_id(&state.pool, admin.account_id).await?.ok_or(AppError::NotFound)?;
    if !auth::verify_password(&form.password, &acc.password_hash) {
        store::record_failure(&state.pool, acc.id).await?;
        return Ok(api_err(StatusCode::FORBIDDEN, "That password is wrong."));
    }
    store::touch_step_up(&state.pool, admin.session_id).await?;
    store::audit(&state.pool, "admin.step_up", serde_json::json!({"email": admin.email})).await?;
    Ok(axum::Json(serde_json::json!({"ok": true})).into_response())
}

pub fn api_csrf_ok(state: &State, cookie: &CsrfCookie, headers: &HeaderMap) -> bool {
    let sent = headers.get("x-csrf").and_then(|v| v.to_str().ok()).unwrap_or("");
    security::csrf_ok(&state.cfg.csrf_secret, &cookie.0, sent)
}

pub fn api_err(status: StatusCode, msg: &str) -> Response {
    (status, axum::Json(serde_json::json!({"ok": false, "error": msg}))).into_response()
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn pending_cookie_roundtrip_and_tamper() {
        let v = sign_pending("secret", 42, 7);
        assert_eq!(read_pending("secret", &v), Some((42, 7)));
        assert_eq!(read_pending("other", &v), None);
        let mut t = v.clone();
        t.replace_range(0..1, "9");
        assert!(read_pending("secret", &t).is_none() || read_pending("secret", &t) == Some((42, 7)) && false);
        assert_eq!(hide_email("nathaniel@thepenmarket.com"), "nat•••@thepenmarket.com");
        let (w, d) = describe("admin.edit", &serde_json::json!({"pen": "Waterman 56", "field": "Price", "before": "$749.99", "after": "$699.99"}));
        assert_eq!(w, "Waterman 56 edited: Price");
        assert_eq!(d, "$749.99 → $699.99");
        assert_eq!(describe("admin.logout", &serde_json::json!({})).0, "Signed out");
    }
}
