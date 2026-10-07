//! Search spellings: his list of the ways people misspell a word and the way he spells it. A search
//! for any of them finds all of them (`spelling::expand`). Beside the list: what visitors searched
//! for lately that found nothing, which is where new misspellings show up.

use super::{store, Auth};
use crate::app::{html, AppError, AppResult, PageMeta, State};
use crate::security::{self, CsrfCookie};
use crate::settings;
use crate::spelling;
use askama::Template;
use axum::extract::{Form, Query, State as AxState};
use axum::response::{IntoResponse, Redirect};
use axum::Extension;
use serde::Deserialize;

#[derive(Template)]
#[template(path = "admin/search.html")]
pub struct SearchTpl {
    pub page: PageMeta,
    pub saved: bool,
    pub list: String,
    pub words: usize,
    pub misses: Vec<crate::insights::Counted>,
}

#[derive(Deserialize, Default)]
pub struct SavedQuery {
    #[serde(default)]
    pub saved: String,
}

pub async fn page(AxState(state): AxState<State>, Extension(cookie): Extension<CsrfCookie>, Auth(_admin): Auth, Query(q): Query<SavedQuery>) -> AppResult {
    let list = settings::search_spellings(&state.pool).await?;
    let mut page = PageMeta::new(&state, "Search spellings | ThePenMarket.com admin", "Admin area for ThePenMarket.com.", "/admin/search/");
    page.noindex = true;
    page.section = "admin".into();
    page.csrf = security::csrf_token(&state.cfg.csrf_secret, &cookie.0);
    html(&SearchTpl { page, saved: q.saved == "1", words: list.len(), list: spelling::render(&list), misses: crate::insights::top_searches(&state.pool, 90, 30, "shop", true).await? })
}

#[derive(Deserialize)]
pub struct SaveForm {
    pub csrf: String,
    #[serde(default)]
    pub list: String,
}

pub async fn save(AxState(state): AxState<State>, Extension(cookie): Extension<CsrfCookie>, Auth(admin): Auth, Form(form): Form<SaveForm>) -> AppResult {
    if admin.preview {
        return Err(AppError::Forbidden("Turn off Preview to make changes.".into()));
    }
    if !security::csrf_ok(&state.cfg.csrf_secret, &cookie.0, &form.csrf) {
        return Err(AppError::Forbidden("The form expired. Reload the page and try again.".into()));
    }
    let list = spelling::parse(&form.list);
    // Saved even when empty: an empty list means he wants no spelling help, not the e-mail's examples.
    settings::set(&state.pool, settings::SEARCH_SPELLINGS, serde_json::to_value(&list)?).await?;
    crate::app::refresh_site_cache(&state).await;
    store::audit(&state.pool, "admin.search_spellings_saved", serde_json::json!({"email": admin.email, "ip": admin.ip, "words": list.len()})).await?;
    Ok(Redirect::to("/admin/search/?saved=1").into_response())
}
