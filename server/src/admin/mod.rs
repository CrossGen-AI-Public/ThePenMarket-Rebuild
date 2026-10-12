//! Nathaniel's admin: sign-in, remembered devices, sessions, click-to-edit, preview, history.
//! SPEC-0001. The session context rides a task-local so every template can render the admin bar
//! without touching the handlers that render customer pages.

pub mod api;
pub mod content;
pub mod glossary;
pub mod insight_pages;
pub mod links_page;
pub mod manage;
pub mod pens;
pub mod redirects;
pub mod search;
pub mod sourcing;
pub use thepenmarket::admin_core::auth;
pub mod handlers;
pub use thepenmarket::admin_core::mail;
pub use thepenmarket::admin_core::store;

use crate::app::State;
use axum::extract::{FromRequestParts, Request, State as AxState};
use axum::http::request::Parts;
use axum::http::{header, StatusCode};
use axum::middleware::Next;
use axum::response::{IntoResponse, Redirect, Response};
use axum::routing::{get, post};
use axum::Router;
use chrono::Utc;

pub const SESSION_COOKIE: &str = "pm_admin";
pub const DEVICE_COOKIE: &str = "pm_device";
pub const PENDING_COOKIE: &str = "pm_pending";

/// What the templates and the API know about the signed-in admin.
#[derive(Clone, Debug)]
pub struct AdminCtx {
    pub account_id: i32,
    pub session_id: i32,
    pub name: String,
    pub email: String,
    pub preview: bool,
    pub step_up_fresh: bool,
    pub https: bool,
    pub csrf: String,
    pub ip: String,
}

tokio::task_local! {
    pub static ADMIN: Option<AdminCtx>;
}

/// The admin context for the current request, if signed in (readable anywhere on the request task).
pub fn current() -> Option<AdminCtx> {
    ADMIN.try_with(|a| a.clone()).ok().flatten()
}

/// Signed in and NOT previewing: the pages show edit affordances.
pub fn editing() -> bool {
    current().map(|a| !a.preview).unwrap_or(false)
}

pub fn is_https(headers: &axum::http::HeaderMap) -> bool {
    headers.get("x-forwarded-proto").and_then(|v| v.to_str().ok()).map(|v| v.eq_ignore_ascii_case("https")).unwrap_or(false)
}

pub fn cookie(headers: &axum::http::HeaderMap, name: &str) -> Option<String> {
    let raw = headers.get(header::COOKIE)?.to_str().ok()?;
    raw.split(';').map(|c| c.trim()).find_map(|c| c.strip_prefix(&format!("{name}=")).map(|v| v.to_string()))
}

/// Resolve the session cookie once per request and expose it to templates and handlers.
pub async fn context(AxState(state): AxState<State>, mut req: Request, next: Next) -> Response {
    let https = is_https(req.headers());
    let peer = req.extensions().get::<axum::extract::ConnectInfo<std::net::SocketAddr>>().map(|c| c.0.ip()).unwrap_or(std::net::IpAddr::V4(std::net::Ipv4Addr::UNSPECIFIED));
    let ip = crate::security::client_ip(req.headers(), peer).to_string();
    let name = auth::cookie_name(https, SESSION_COOKIE);
    let mut ctx: Option<AdminCtx> = None;
    let csrf = req.extensions().get::<crate::security::CsrfCookie>().map(|c| crate::security::csrf_token(&state.cfg.csrf_secret, &c.0)).unwrap_or_default();
    if let Some(token) = cookie(req.headers(), &name).filter(|t| t.len() == 64) {
        if let Ok(Some(s)) = store::session_by_token(&state.pool, &auth::sha256_hex(&token)).await {
            if let Ok(Some(acc)) = store::account_by_id(&state.pool, s.account_id).await {
                let fresh = s.step_up_at.map(|t| Utc::now() - t < chrono::Duration::minutes(auth::STEP_UP_MIN)).unwrap_or(false);
                ctx = Some(AdminCtx { account_id: acc.id, session_id: s.id, name: acc.display_name, email: acc.email, preview: s.preview, step_up_fresh: fresh, https, csrf: csrf.clone(), ip: ip.clone() });
            }
        }
    }
    if let Some(c) = &ctx {
        req.extensions_mut().insert(c.clone());
    }
    let mut res = ADMIN.scope(ctx.clone(), next.run(req)).await;
    if ctx.is_some() {
        // Never let a shared cache or a proxy keep a page that was rendered for the admin.
        if let Ok(v) = header::HeaderValue::from_str("no-store") {
            res.headers_mut().insert(header::CACHE_CONTROL, v);
        }
    }
    res
}

/// Extractor for admin pages: redirects to sign-in when there is no session.
pub struct Auth(pub AdminCtx);

impl<S: Send + Sync> FromRequestParts<S> for Auth {
    type Rejection = Response;
    async fn from_request_parts(parts: &mut Parts, _s: &S) -> Result<Self, Self::Rejection> {
        match parts.extensions.get::<AdminCtx>() {
            Some(c) => Ok(Auth(c.clone())),
            None => Err(Redirect::to("/admin/login/").into_response()),
        }
    }
}

/// Extractor for the JSON API: 401 with a JSON body when there is no session.
pub struct AuthApi(pub AdminCtx);

impl<S: Send + Sync> FromRequestParts<S> for AuthApi {
    type Rejection = Response;
    async fn from_request_parts(parts: &mut Parts, _s: &S) -> Result<Self, Self::Rejection> {
        match parts.extensions.get::<AdminCtx>() {
            Some(c) => Ok(AuthApi(c.clone())),
            None => Err((StatusCode::UNAUTHORIZED, [(header::CONTENT_TYPE, "application/json")], "{\"ok\":false,\"error\":\"Please sign in.\"}").into_response()),
        }
    }
}

pub fn router() -> Router<State> {
    Router::new()
        .route("/admin/", get(manage::home))
        .route("/admin/settings/", get(manage::settings_page).post(manage::settings_save))
        .route("/admin/brands/", get(manage::brands).post(manage::brands_save))
        .route("/admin/orders/", get(manage::orders))
        .route("/admin/inbox/", get(manage::inbox))
        .route("/admin/inbox/{id}/", post(manage::inbox_handled))
        .route("/admin/inbox/photo/{key}", get(manage::inbox_photo))
        .route("/admin/api/featured/", post(manage::set_featured))
        .route("/admin/posts/", get(content::posts))
        .route("/admin/posts/new/", get(content::post_new).post(content::post_save).layer(axum::extract::DefaultBodyLimit::max(2 * 1024 * 1024)))
        .route("/admin/posts/{id}/", get(content::post_edit).post(content::post_save).layer(axum::extract::DefaultBodyLimit::max(2 * 1024 * 1024)))
        .route("/admin/pages/", get(content::pages))
        .route("/admin/pages/new/", get(content::page_new).post(content::page_save).layer(axum::extract::DefaultBodyLimit::max(2 * 1024 * 1024)))
        .route("/admin/pages/{id}/", get(content::page_edit).post(content::page_save).layer(axum::extract::DefaultBodyLimit::max(2 * 1024 * 1024)))
        .route("/admin/faqs/", get(content::faqs).post(content::faq_save))
        .route("/admin/redirects/", get(redirects::page).post(redirects::add).layer(axum::extract::DefaultBodyLimit::max(1024 * 1024)))
        .route("/admin/redirects/{id}/delete/", post(redirects::delete))
        .route("/admin/redirects.csv", get(redirects::csv))
        .route("/admin/search/", get(search::page).post(search::save))
        .route("/admin/faqs/{id}/", post(content::faq_save))
        .route("/admin/glossary/", get(glossary::list).post(glossary::create))
        .route("/admin/glossary/mine/", post(glossary::mine))
        .route("/admin/glossary/defs/", post(glossary::def_save))
        .route("/admin/glossary/{id}/", get(glossary::edit).post(glossary::save))
        .route("/admin/glossary/{id}/delete/", post(glossary::delete))
        .route("/admin/api/posts/", get(content::post_lookup))
        .route("/admin/api/media/", post(content::upload_media).layer(axum::extract::DefaultBodyLimit::max(12 * 1024 * 1024)))
        .route("/admin/insights/", get(insight_pages::insights_page))
        .route("/admin/api/heatmap/", get(insight_pages::heatmap))
        .route("/admin/experiments/", get(insight_pages::experiments))
        .route("/admin/experiments/{key}/", post(insight_pages::experiment_action))
        .route("/admin/sourcing/", get(sourcing::page).post(sourcing::want_save))
        .route("/admin/sourcing/check/", post(sourcing::check_now))
        .route("/admin/sourcing/find/{id}/", post(sourcing::find_status))
        .route("/admin/sourcing/{id}/", post(sourcing::want_save))
        .route("/admin/login/", get(handlers::login_form).post(handlers::login))
        .route("/admin/verify/", get(handlers::verify_form).post(handlers::verify))
        .route("/admin/logout/", post(handlers::logout))
        .route("/admin/forgot/", get(handlers::forgot_form).post(handlers::forgot))
        .route("/admin/reset/{token}/", get(handlers::reset_form).post(handlers::reset))
        .route("/admin/devices/", get(handlers::devices))
        .route("/admin/devices/{id}/revoke/", post(handlers::revoke_device))
        .route("/admin/password/", get(handlers::password_form).post(handlers::password))
        .route("/admin/history/", get(handlers::history))
        .route("/admin/history.csv", get(handlers::history_csv))
        .route("/admin/preview/", post(handlers::preview))
        .route("/admin/step-up/", post(handlers::step_up))
        .route("/admin/api/product/", post(api::create_product))
        .route("/admin/api/product/{id}/", axum::routing::patch(api::edit_product))
        .route("/admin/api/product/{id}/delete/", post(pens::delete))
        .route("/admin/pens/", get(pens::page))
        .route("/admin/links/", get(links_page::page))
        .route("/admin/links/check/", post(links_page::check_now))
        .route("/admin/pens/{id}/restore/", post(pens::restore))
        .route("/admin/pens/{id}/erase/", post(pens::erase))
        .route("/admin/api/product/{id}/photo/", post(api::upload_photo).layer(axum::extract::DefaultBodyLimit::max(12 * 1024 * 1024)))
        .route("/admin/api/product/{id}/photos/order/", post(api::reorder_photos))
        .route("/admin/api/photo/{id}/archive/", post(api::archive_photo))
        .route("/admin/api/options/", get(api::options))
}
