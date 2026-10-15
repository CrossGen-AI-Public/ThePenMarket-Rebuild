//! The admin's view of `insights.rs`: the Insights page (ranked tables, no charts to decode), the
//! data behind the on-page heat map, and the experiments he can start, stop and settle.

use super::handlers::api_err;
use super::{store, Auth, AuthApi};
use crate::app::{html, AppError, AppResult, PageMeta, State};
use crate::insights::{self, Counted};
use crate::security::{self, CsrfCookie};
use askama::Template;
use axum::extract::{Form, Path, Query, State as AxState};
use axum::http::StatusCode;
use axum::response::{IntoResponse, Redirect};
use axum::{Extension, Json};
use serde::Deserialize;

fn meta(state: &State, title: &str, path: &str, cookie: &CsrfCookie) -> PageMeta {
    let mut page = PageMeta::new(state, &format!("{title} | ThePenMarket.com admin"), "Admin area for ThePenMarket.com.", path);
    page.noindex = true;
    page.section = "admin".into();
    page.csrf = security::csrf_token(&state.cfg.csrf_secret, &cookie.0);
    page
}

/// A ranked list with a bar per row; `pct` is the row's share of the largest value in its list.
pub struct Ranked {
    pub title: &'static str,
    pub note: &'static str,
    pub unit: &'static str,
    pub rows: Vec<RankedRow>,
}
pub struct RankedRow {
    pub label: String,
    pub url: String,
    pub n: i64,
    pub extra: String,
    pub pct: i64,
}

fn ranked(title: &'static str, note: &'static str, unit: &'static str, rows: Vec<Counted>) -> Ranked {
    let max = rows.iter().map(|r| r.n).max().unwrap_or(0).max(1);
    Ranked { title, note, unit, rows: rows.into_iter().map(|r| RankedRow { pct: (r.n * 100 / max).clamp(2, 100), label: r.label, url: r.url, n: r.n, extra: r.extra }).collect() }
}

#[derive(Template)]
#[template(path = "admin/insights.html")]
pub struct InsightsTpl {
    pub page: PageMeta,
    pub days: i32,
    pub totals: insights::Totals,
    pub lists: Vec<Ranked>,
}

#[derive(Deserialize, Default)]
pub struct RangeQuery {
    pub days: Option<i32>,
}

pub async fn insights_page(AxState(state): AxState<State>, Extension(cookie): Extension<CsrfCookie>, Auth(_admin): Auth, Query(q): Query<RangeQuery>) -> AppResult {
    let days = match q.days.unwrap_or(30) {
        7 => 7,
        90 => 90,
        _ => 30,
    };
    let pool = &state.pool;
    let mut conv = insights::conversions(pool, days).await?;
    for c in &mut conv {
        c.label = match c.label.as_str() {
            "buy" => "Pressed “Buy this pen”".into(),
            "call" => "Tapped the phone number".into(),
            "email" => "Clicked the e-mail address".into(),
            other => format!("Sent a form ({})", other.trim_start_matches("form:")),
        };
    }
    let lists = vec![
        ranked("Pens people opened", "Different visitors who opened each pen's page.", "visitors", insights::top_pens(pool, days, 15).await?),
        ranked("What people searched for", "Typed into the shop's search box.", "visitors", insights::top_searches(pool, days, 15, "shop", false).await?),
        ranked("Searches that found nothing", "Pens people want that are not in the case. Worth a look when you are buying; add one to Sourcing.", "visitors", insights::top_searches(pool, days, 15, "shop", true).await?),
        ranked("What people asked Ask the Pen Market", "Questions asked more than once are good FAQs.", "visitors", insights::top_searches(pool, days, 15, "guide", false).await?),
        ranked("Pages visited", "With how far down the page people scrolled, on average.", "visitors", insights::top_pages(pool, days, 15).await?),
        ranked("Clicks that went nowhere", "Things people clicked that are not links or buttons: they expected something to happen. Open the page and turn on the heat map to see where.", "clicks", insights::dead_clicks(pool, days, 15).await?),
        ranked("What visitors did next", "The steps that lead to a sale or a job.", "times", conv),
        ranked("Phone, tablet or computer", "", "visitors", insights::devices(pool, days).await?),
    ];
    html(&InsightsTpl { page: meta(&state, "Insights", "/admin/insights/", &cookie), days, totals: insights::totals(pool, days).await?, lists })
}

#[derive(Deserialize)]
pub struct HeatQuery {
    pub path: String,
    #[serde(default)]
    pub device: String,
    pub days: Option<i32>,
}

pub async fn heatmap(AxState(state): AxState<State>, AuthApi(_admin): AuthApi, Query(q): Query<HeatQuery>) -> AppResult {
    if !q.path.starts_with('/') || q.path.len() > 200 {
        return Ok(api_err(StatusCode::BAD_REQUEST, "Bad path."));
    }
    let days = q.days.unwrap_or(30).clamp(1, 365);
    Ok(Json(insights::heatmap(&state.pool, &q.path, &q.device, days).await?).into_response())
}

pub struct VariantView {
    pub key: String,
    pub label: String,
    pub visitors: i64,
    pub pen_views_each: String,
    pub converters: i64,
    pub rate: String,
    pub is_winner: bool,
}
pub struct ExperimentView {
    pub key: String,
    pub name: String,
    pub description: String,
    pub status: String,
    pub started: String,
    pub variants: Vec<VariantView>,
    pub enough: bool,
}

#[derive(Template)]
#[template(path = "admin/experiments.html")]
pub struct ExperimentsTpl {
    pub page: PageMeta,
    pub rows: Vec<ExperimentView>,
}

/// Below this many visitors per version the difference is mostly luck; the page says so.
const ENOUGH_VISITORS: i64 = 100;

pub async fn experiments(AxState(state): AxState<State>, Extension(cookie): Extension<CsrfCookie>, Auth(_admin): Auth) -> AppResult {
    let mut rows = vec![];
    for e in insights::experiments(&state.pool).await? {
        let results = insights::experiment_results(&state.pool, &e.key, e.started_at).await?;
        let variants: Vec<VariantView> = e
            .variants
            .iter()
            .map(|(k, label)| {
                let r = results.iter().find(|r| &r.variant == k);
                let (visitors, views, conv) = r.map(|r| (r.visitors, r.pen_views, r.converters)).unwrap_or((0, 0, 0));
                VariantView {
                    key: k.clone(),
                    label: label.clone(),
                    visitors,
                    pen_views_each: if visitors > 0 { format!("{:.1}", views as f64 / visitors as f64) } else { "–".into() },
                    converters: conv,
                    rate: if visitors > 0 { format!("{:.1}%", conv as f64 * 100.0 / visitors as f64) } else { "–".into() },
                    is_winner: e.winner.as_deref() == Some(k.as_str()),
                }
            })
            .collect();
        let enough = variants.iter().all(|v| v.visitors >= ENOUGH_VISITORS);
        rows.push(ExperimentView { key: e.key, name: e.name, description: e.description, status: e.status, started: e.started_at.map(super::handlers::fmt_time).unwrap_or_default(), variants, enough });
    }
    html(&ExperimentsTpl { page: meta(&state, "Experiments", "/admin/experiments/", &cookie), rows })
}

#[derive(Deserialize)]
pub struct ExperimentForm {
    pub csrf: String,
    pub action: String,
    #[serde(default)]
    pub winner: String,
}

pub async fn experiment_action(AxState(state): AxState<State>, Extension(cookie): Extension<CsrfCookie>, Auth(admin): Auth, Path(key): Path<String>, Form(form): Form<ExperimentForm>) -> AppResult {
    if admin.preview {
        return Err(AppError::Forbidden("Turn off Preview to make changes.".into()));
    }
    if !security::csrf_ok(&state.cfg.csrf_secret, &cookie.0, &form.csrf) {
        return Err(AppError::Forbidden("The form expired. Reload the page and try again.".into()));
    }
    let Some(def) = insights::REGISTRY.iter().find(|d| d.key == key) else { return Err(AppError::NotFound) };
    match form.action.as_str() {
        "start" => insights::set_experiment_status(&state.pool, &key, "running", None).await?,
        "stop" => {
            let winner = def.variants.iter().map(|(k, _)| *k).find(|k| *k == form.winner);
            insights::set_experiment_status(&state.pool, &key, "stopped", winner).await?
        }
        "reset" => insights::set_experiment_status(&state.pool, &key, "draft", None).await?,
        _ => return Err(AppError::BadRequest("Unknown action.".into())),
    }
    store::audit(&state.pool, "admin.experiment", serde_json::json!({"email": admin.email, "ip": admin.ip, "experiment": def.name, "action": form.action, "winner": form.winner})).await?;
    crate::app::refresh_site_cache(&state).await;
    Ok(Redirect::to("/admin/experiments/").into_response())
}
