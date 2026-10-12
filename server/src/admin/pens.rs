//! Hidden and deleted pens (Nathaniel, meeting of 2026-10-05). A hidden pen (status 'draft') is one
//! he is still writing or took down; a deleted pen (status 'archived') is off the website for good but
//! its row stays for his accounts, restorable, until he erases it. Erasing needs his password again,
//! and leaves a redirect behind so an address that was once public never turns into a dead end.

use super::handlers::{api_csrf_ok, api_err};
use super::{store, Auth, AuthApi};
use crate::app::{html, AppError, AppResult, PageMeta, State};
use crate::security::{self, CsrfCookie};
use askama::Template;
use axum::extract::{ConnectInfo, Form, Path, Query, State as AxState};
use axum::http::{HeaderMap, StatusCode};
use axum::response::{IntoResponse, Redirect};
use axum::{Extension, Json};
use chrono::{DateTime, Utc};
use serde::Deserialize;
use sqlx::FromRow;
use std::net::SocketAddr;

#[derive(FromRow)]
pub struct PenRow {
    pub id: i32,
    pub slug: String,
    pub short_title: String,
    pub sku: String,
    pub price_cents: i64,
    pub status: String,
    pub updated_at: DateTime<Utc>,
    pub deleted_at: Option<DateTime<Utc>>,
    pub sold_at: Option<DateTime<Utc>>,
    pub photos: i64,
}

impl PenRow {
    pub fn price(&self) -> String {
        if self.price_cents > 0 {
            crate::money::fmt_cents(self.price_cents)
        } else {
            "no price".into()
        }
    }
    pub fn when(&self) -> String {
        self.deleted_at.unwrap_or(self.updated_at).format("%b %-d, %Y").to_string()
    }
    pub fn was_sold(&self) -> bool {
        self.sold_at.is_some()
    }
}

#[derive(Template)]
#[template(path = "admin/pens.html")]
pub struct PensTpl {
    pub page: PageMeta,
    pub hidden: Vec<PenRow>,
    pub deleted: Vec<PenRow>,
    pub notice: String,
    pub error: String,
}

#[derive(Deserialize, Default)]
pub struct PensQuery {
    #[serde(default)]
    pub done: String,
}

const SELECT: &str = "SELECT p.id, p.slug, p.short_title, p.sku, p.price_cents, p.status, p.updated_at, p.deleted_at, p.sold_at,
    (SELECT count(*) FROM product_image i WHERE i.product_id = p.id AND i.archived_at IS NULL) AS photos FROM product p";

async fn render(state: &State, cookie: &CsrfCookie, notice: &str, error: &str) -> AppResult {
    let hidden: Vec<PenRow> = sqlx::query_as(&format!("{SELECT} WHERE p.status = 'draft' ORDER BY p.updated_at DESC")).fetch_all(&state.pool).await?;
    let deleted: Vec<PenRow> = sqlx::query_as(&format!("{SELECT} WHERE p.status = 'archived' ORDER BY p.deleted_at DESC NULLS LAST, p.updated_at DESC")).fetch_all(&state.pool).await?;
    let mut page = PageMeta::new(state, "Hidden and deleted pens | ThePenMarket.com admin", "Admin area for ThePenMarket.com.", "/admin/pens/");
    page.noindex = true;
    page.section = "admin".into();
    page.csrf = security::csrf_token(&state.cfg.csrf_secret, &cookie.0);
    html(&PensTpl { page, hidden, deleted, notice: notice.into(), error: error.into() })
}

pub async fn page(AxState(state): AxState<State>, Extension(cookie): Extension<CsrfCookie>, Auth(_admin): Auth, Query(q): Query<PensQuery>) -> AppResult {
    let notice = match q.done.as_str() {
        "deleted" => "Deleted. It is off the website; it stays below until you erase it.",
        "restored" => "Restored as hidden. Open it and set it to Live when it should be for sale again.",
        "erased" => "Erased for good.",
        _ => "",
    };
    render(&state, &cookie, notice, "").await
}

async fn refresh_catalog(state: &State) {
    match crate::catalog::load(&state.pool).await {
        Ok(c) => {
            if let Ok(mut w) = state.catalog.write() {
                *w = std::sync::Arc::new(c);
            }
        }
        Err(e) => tracing::warn!("catalog refresh failed: {e:#}"),
    }
}

/// `POST /admin/api/product/{id}/delete/` from the pen's own page: off the website, row kept.
/// Like marking a pen sold, it asks for the password again first (a 428 the editor handles).
pub async fn delete(AxState(state): AxState<State>, Extension(cookie): Extension<CsrfCookie>, AuthApi(admin): AuthApi, headers: HeaderMap, Path(id): Path<i32>) -> AppResult {
    if admin.preview || !api_csrf_ok(&state, &cookie, &headers) {
        return Ok(api_err(StatusCode::FORBIDDEN, "The page expired. Reload and try again."));
    }
    if !admin.step_up_fresh {
        return Ok((StatusCode::PRECONDITION_REQUIRED, Json(serde_json::json!({"ok": false, "step_up": true, "error": "Please re-enter your password to delete a pen."}))).into_response());
    }
    // A pen someone is paying for right now cannot be pulled out from under the payment.
    let held: i64 = sqlx::query_scalar("SELECT count(*) FROM shop_order WHERE product_id = $1 AND status IN ('open', 'processing')").bind(id).fetch_one(&state.pool).await?;
    if held > 0 {
        return Ok(api_err(StatusCode::CONFLICT, "Someone is checking out this pen right now. Try again in half an hour."));
    }
    let Some(before) = store::product_for_edit(&state.pool, id).await? else { return Err(AppError::NotFound) };
    if before.status == "archived" {
        return Ok(Json(serde_json::json!({"ok": true, "url": "/admin/pens/?done=deleted"})).into_response());
    }
    sqlx::query("UPDATE product SET status = 'archived', deleted_at = now(), updated_at = now() WHERE id = $1").bind(id).execute(&state.pool).await?;
    store::audit(&state.pool, "admin.pen_deleted", serde_json::json!({"email": admin.email, "ip": admin.ip, "pen": before.short_title, "product_id": id, "was": before.status})).await?;
    refresh_catalog(&state).await;
    Ok(Json(serde_json::json!({"ok": true, "url": "/admin/pens/?done=deleted"})).into_response())
}

#[derive(Deserialize)]
pub struct CsrfForm {
    pub csrf: String,
}

fn check(state: &State, cookie: &CsrfCookie, admin: &super::AdminCtx, csrf: &str) -> Result<(), AppError> {
    if admin.preview {
        return Err(AppError::Forbidden("Turn off Preview to make changes.".into()));
    }
    if !security::csrf_ok(&state.cfg.csrf_secret, &cookie.0, csrf) {
        return Err(AppError::Forbidden("The form expired. Reload the page and try again.".into()));
    }
    Ok(())
}

/// `POST /admin/pens/{id}/restore/`: a deleted pen comes back hidden, never straight onto the site.
pub async fn restore(AxState(state): AxState<State>, Extension(cookie): Extension<CsrfCookie>, Auth(admin): Auth, Path(id): Path<i32>, Form(form): Form<CsrfForm>) -> AppResult {
    check(&state, &cookie, &admin, &form.csrf)?;
    let title: String = sqlx::query_scalar("UPDATE product SET status = 'draft', deleted_at = NULL, updated_at = now() WHERE id = $1 AND status = 'archived' RETURNING short_title").bind(id).fetch_optional(&state.pool).await?.ok_or(AppError::NotFound)?;
    store::audit(&state.pool, "admin.pen_restored", serde_json::json!({"email": admin.email, "ip": admin.ip, "pen": title, "product_id": id})).await?;
    refresh_catalog(&state).await;
    Ok(Redirect::to("/admin/pens/?done=restored").into_response())
}

#[derive(Deserialize)]
pub struct EraseForm {
    pub csrf: String,
    #[serde(default)]
    pub password: String,
}

/// `POST /admin/pens/{id}/erase/`: gone for good, photos' database rows and tags with it (the photo
/// files stay on disk). Only a deleted pen can be erased, and only with his password typed again.
/// Orders keep their own copy of the pen's name, SKU and price, so the accounts are unaffected.
pub async fn erase(AxState(state): AxState<State>, Extension(cookie): Extension<CsrfCookie>, Auth(admin): Auth, ConnectInfo(peer): ConnectInfo<SocketAddr>, headers: HeaderMap, Path(id): Path<i32>, Form(form): Form<EraseForm>) -> AppResult {
    check(&state, &cookie, &admin, &form.csrf)?;
    if !state.admin_limiter.allow(security::client_ip(&headers, peer.ip())) {
        return render(&state, &cookie, "", "Too many tries in a row. Wait a minute, then try again.").await;
    }
    let acc = store::account_by_id(&state.pool, admin.account_id).await?.ok_or(AppError::NotFound)?;
    if !super::auth::verify_password(&form.password, &acc.password_hash) {
        store::record_failure(&state.pool, acc.id).await?;
        return render(&state, &cookie, "", "That password is wrong, so nothing was erased.").await;
    }
    let mut tx = state.pool.begin().await?;
    let row: Option<(String, String, String)> = sqlx::query_as(
        "SELECT p.slug, p.short_title, c.slug FROM product p JOIN category c ON c.id = p.category_id WHERE p.id = $1 AND p.status = 'archived' FOR UPDATE OF p",
    )
    .bind(id)
    .fetch_optional(&mut *tx)
    .await?;
    let Some((slug, title, cat)) = row else { return Err(AppError::NotFound) };
    // Its address may be in Google or someone's bookmarks: send it to the pen's category rather than a 404.
    sqlx::query("INSERT INTO redirect (old_path, new_path, reason, added_by) VALUES ($1, $2, 'pen erased in the admin', 'admin') ON CONFLICT (old_path) DO NOTHING")
        .bind(format!("/product/{slug}/"))
        .bind(format!("/product-category/{cat}/"))
        .execute(&mut *tx)
        .await?;
    sqlx::query("DELETE FROM product WHERE id = $1").bind(id).execute(&mut *tx).await?;
    tx.commit().await?;
    store::audit(&state.pool, "admin.pen_erased", serde_json::json!({"email": admin.email, "ip": admin.ip, "pen": title, "product_id": id, "slug": slug})).await?;
    refresh_catalog(&state).await;
    Ok(Redirect::to("/admin/pens/?done=erased").into_response())
}
