//! `/admin/links/`: the outside links in his content and what each answered (ADR-0001). Dead ones are
//! shown on the site as plain words; this is where he sees which, and can ask for a fresh check.

use super::Auth;
use crate::app::{html, AppError, AppResult, PageMeta, State};
use crate::security::{self, CsrfCookie};
use askama::Template;
use axum::extract::{Form, Query, State as AxState};
use axum::response::{IntoResponse, Redirect};
use axum::Extension;
use chrono::{DateTime, Utc};
use serde::Deserialize;
use sqlx::FromRow;

#[derive(FromRow)]
pub struct LinkRow {
    pub url: String,
    pub last_result: String,
    pub dead: bool,
    pub dead_since: Option<DateTime<Utc>>,
    pub last_checked_at: Option<DateTime<Utc>>,
    pub found_on: String,
}

impl LinkRow {
    pub fn since(&self) -> String {
        self.dead_since.map(|t| t.format("%b %-d, %Y").to_string()).unwrap_or_default()
    }
    pub fn checked(&self) -> String {
        self.last_checked_at.map(|t| t.format("%b %-d, %Y").to_string()).unwrap_or_default()
    }
    pub fn watching(&self) -> bool {
        !self.dead && self.last_result.starts_with("gone")
    }
}

#[derive(Template)]
#[template(path = "admin/links.html")]
pub struct LinksTpl {
    pub page: PageMeta,
    pub dead: Vec<LinkRow>,
    pub watching: Vec<LinkRow>,
    pub unsure: Vec<LinkRow>,
    pub total: i64,
    pub started: bool,
}

#[derive(Deserialize, Default)]
pub struct LinksQuery {
    #[serde(default)]
    pub started: String,
}

pub async fn page(AxState(state): AxState<State>, Extension(cookie): Extension<CsrfCookie>, Auth(_admin): Auth, Query(q): Query<LinksQuery>) -> AppResult {
    let rows: Vec<LinkRow> = sqlx::query_as("SELECT url, last_result, dead, dead_since, last_checked_at, found_on FROM external_link ORDER BY dead DESC, dead_since DESC NULLS LAST, url").fetch_all(&state.pool).await?;
    let total = rows.len() as i64;
    let (dead, rest): (Vec<LinkRow>, Vec<LinkRow>) = rows.into_iter().partition(|r| r.dead);
    let (watching, rest): (Vec<LinkRow>, Vec<LinkRow>) = rest.into_iter().partition(|r| r.watching());
    let unsure: Vec<LinkRow> = rest.into_iter().filter(|r| r.last_result.starts_with("unsure") || r.last_result.starts_with("skipped")).collect();
    let mut page = PageMeta::new(&state, "Links to other websites | ThePenMarket.com admin", "Admin area for ThePenMarket.com.", "/admin/links/");
    page.noindex = true;
    page.section = "admin".into();
    page.csrf = security::csrf_token(&state.cfg.csrf_secret, &cookie.0);
    html(&LinksTpl { page, dead, watching, unsure, total, started: q.started == "1" })
}

#[derive(Deserialize)]
pub struct CsrfForm {
    pub csrf: String,
}

/// `POST /admin/links/check/`: run the daily check now, in the background.
pub async fn check_now(AxState(state): AxState<State>, Extension(cookie): Extension<CsrfCookie>, Auth(admin): Auth, Form(form): Form<CsrfForm>) -> AppResult {
    if admin.preview || !security::csrf_ok(&state.cfg.csrf_secret, &cookie.0, &form.csrf) {
        return Err(AppError::Forbidden("The form expired. Reload the page and try again.".into()));
    }
    static RUNNING: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);
    if !RUNNING.swap(true, std::sync::atomic::Ordering::SeqCst) {
        let st = state.clone();
        tokio::spawn(async move {
            match crate::links::check_all(&st).await {
                Ok((n, dead)) => {
                    tracing::info!(checked = n, dead, "outside links checked (asked for in the admin)");
                    crate::app::refresh_site_cache(&st).await;
                }
                Err(e) => tracing::warn!("outside link check failed: {e:#}"),
            }
            RUNNING.store(false, std::sync::atomic::Ordering::SeqCst);
        });
    }
    Ok(Redirect::to("/admin/links/?started=1").into_response())
}
