//! JSON endpoints behind the click-to-edit page script. Every field name maps to a fixed column
//! here; nothing from the request reaches SQL unparameterised.

use super::handlers::{api_csrf_ok, api_err};
use super::{store, AuthApi};
use crate::app::{AppError, AppResult, State};
use crate::security::CsrfCookie;
use axum::extract::{Multipart, Path, State as AxState};
use axum::http::{HeaderMap, StatusCode};
use axum::response::IntoResponse;
use axum::{Extension, Json};
use serde::Deserialize;
use store::ProductValue;

#[derive(Deserialize)]
pub struct EditBody {
    pub field: String,
    pub value: String,
}

fn escape(s: &str) -> String {
    html_escape::encode_text(s).to_string()
}

/// Plain text typed by Nathaniel becomes paragraphs; blank lines separate them. No HTML is accepted.
fn paragraphs_html(text: &str) -> String {
    text.replace("\r\n", "\n")
        .split("\n\n")
        .map(|p| p.trim())
        .filter(|p| !p.is_empty())
        .map(|p| format!("<p>{}</p>", escape(p).replace('\n', "<br>")))
        .collect::<Vec<_>>()
        .join("\n")
}

async fn refresh_catalog(state: &State) {
    if let Ok(c) = crate::catalog::load(&state.pool).await {
        if let Ok(mut w) = state.catalog.write() {
            *w = std::sync::Arc::new(c);
        }
    }
}

fn needs_step_up(field: &str, before: &store::ProductEdit, value: &str) -> bool {
    match field {
        "status" => value == "sold" || value == "archived",
        "price" => match super::auth::parse_dollars(value) {
            Ok(cents) if before.price_cents > 0 => {
                let pct = (cents - before.price_cents).abs() as f64 / before.price_cents as f64;
                pct > 0.5
            }
            _ => false,
        },
        _ => false,
    }
}

fn step_up_required() -> axum::response::Response {
    (StatusCode::PRECONDITION_REQUIRED, Json(serde_json::json!({"ok": false, "step_up": true, "error": "Please re-enter your password for this change."}))).into_response()
}

pub async fn edit_product(AxState(state): AxState<State>, Extension(cookie): Extension<CsrfCookie>, AuthApi(admin): AuthApi, headers: HeaderMap, Path(id): Path<i32>, Json(body): Json<EditBody>) -> AppResult {
    if admin.preview {
        return Ok(api_err(StatusCode::FORBIDDEN, "Turn off Preview to edit."));
    }
    if !api_csrf_ok(&state, &cookie, &headers) {
        return Ok(api_err(StatusCode::FORBIDDEN, "The page expired. Reload and try again."));
    }
    let Some(before) = store::product_for_edit(&state.pool, id).await? else {
        return Err(AppError::NotFound);
    };
    let field = body.field.as_str();
    let value = body.value.trim();
    if needs_step_up(field, &before, value) && !admin.step_up_fresh {
        return Ok(step_up_required());
    }
    let (label, was, now): (&str, String, String) = match field {
        "title" => {
            if value.is_empty() || value.chars().count() > 200 {
                return Ok(api_err(StatusCode::BAD_REQUEST, "The title needs 1 to 200 characters."));
            }
            store::update_product_field(&state.pool, id, "title", ProductValue::Text(value.to_string())).await?;
            ("Title", before.title.clone(), value.to_string())
        }
        "short_title" => {
            if value.is_empty() || value.chars().count() > 120 {
                return Ok(api_err(StatusCode::BAD_REQUEST, "The short name needs 1 to 120 characters."));
            }
            store::update_product_field(&state.pool, id, "short_title", ProductValue::Text(value.to_string())).await?;
            ("Short name", before.short_title.clone(), value.to_string())
        }
        "sku" => {
            if value.chars().count() > 40 {
                return Ok(api_err(StatusCode::BAD_REQUEST, "The SKU is too long."));
            }
            store::update_product_field(&state.pool, id, "sku", ProductValue::Text(value.to_string())).await?;
            ("SKU", before.sku.clone(), value.to_string())
        }
        "price" => {
            let cents = match super::auth::parse_dollars(value) {
                Ok(c) => c,
                Err(e) => return Ok(api_err(StatusCode::BAD_REQUEST, &e)),
            };
            store::update_product_field(&state.pool, id, "price_cents", ProductValue::Big(cents)).await?;
            ("Price", crate::money::fmt_cents(before.price_cents), crate::money::fmt_cents(cents))
        }
        "sale_price" => {
            let cents = if value.is_empty() {
                None
            } else {
                match super::auth::parse_dollars(value) {
                    Ok(c) => Some(c),
                    Err(e) => return Ok(api_err(StatusCode::BAD_REQUEST, &e)),
                }
            };
            store::update_product_field(&state.pool, id, "sale_price_cents", ProductValue::OptBig(cents)).await?;
            ("Sale price", before.sale_price_cents.map(crate::money::fmt_cents).unwrap_or_else(|| "none".into()), cents.map(crate::money::fmt_cents).unwrap_or_else(|| "none".into()))
        }
        "status" => {
            if !matches!(value, "live" | "draft" | "sold") {
                return Ok(api_err(StatusCode::BAD_REQUEST, "Status must be live, hidden or sold."));
            }
            store::update_product_field(&state.pool, id, "status", ProductValue::Text(value.to_string())).await?;
            ("Status", before.status.clone(), value.to_string())
        }
        "length_cm" => {
            let mm: Option<f64> = if value.is_empty() {
                None
            } else {
                match value.trim_end_matches("cm").trim().parse::<f64>() {
                    Ok(cm) if (1.0..=60.0).contains(&cm) => Some((cm * 10.0 * 10.0).round() / 10.0),
                    _ => return Ok(api_err(StatusCode::BAD_REQUEST, "Capped length in centimetres, like 13.4.")),
                }
            };
            store::update_product_field(&state.pool, id, "length_mm", ProductValue::OptNum(mm)).await?;
            ("Capped length", before.length_mm.map(|m| format!("{:.1} cm", m / 10.0)).unwrap_or_else(|| "none".into()), mm.map(|m| format!("{:.1} cm", m / 10.0)).unwrap_or_else(|| "none".into()))
        }
        "description" => {
            if value.chars().count() > 20000 {
                return Ok(api_err(StatusCode::BAD_REQUEST, "The description is too long."));
            }
            let html = paragraphs_html(value);
            store::update_description(&state.pool, id, &html, value).await?;
            ("Description", crate::text::summary(&before.description_text, 60), crate::text::summary(value, 60))
        }
        "brand" | "era" | "nib" | "filling_mechanism" | "category" => {
            let (table, column, label) = match field {
                "brand" => ("brand", "brand_id", "Brand"),
                "era" => ("era", "era_id", "Era"),
                "nib" => ("nib", "nib_id", "Nib"),
                "filling_mechanism" => ("filling_mechanism", "filling_mechanism_id", "Filling mechanism"),
                _ => ("category", "category_id", "Category"),
            };
            if value.is_empty() {
                if field == "category" {
                    return Ok(api_err(StatusCode::BAD_REQUEST, "A pen needs a category."));
                }
                store::update_product_field(&state.pool, id, column, ProductValue::OptInt(None)).await?;
                (label, "set".into(), "none".into())
            } else {
                let Some(tid) = store::term_id(&state.pool, table, value).await? else {
                    return Ok(api_err(StatusCode::BAD_REQUEST, "That choice isn't in the list."));
                };
                if field == "category" {
                    store::update_product_field(&state.pool, id, column, ProductValue::Int(tid)).await?;
                } else {
                    store::update_product_field(&state.pool, id, column, ProductValue::OptInt(Some(tid))).await?;
                }
                (label, "previous".into(), value.to_string())
            }
        }
        _ => return Ok(api_err(StatusCode::BAD_REQUEST, "That field can't be edited here.")),
    };
    store::audit(&state.pool, "admin.edit", serde_json::json!({"email": admin.email, "ip": admin.ip, "pen": before.short_title, "product_id": id, "field": label, "before": was, "after": now})).await?;
    refresh_catalog(&state).await;
    let after = store::product_for_edit(&state.pool, id).await?.ok_or(AppError::NotFound)?;
    Ok(Json(serde_json::json!({
        "ok": true,
        "field": field,
        "display": display_value(field, &after),
        "html": if field == "description" { after.description_html.clone() } else { String::new() },
        "status": after.status,
    })).into_response())
}

fn display_value(field: &str, p: &store::ProductEdit) -> String {
    match field {
        "title" => p.title.clone(),
        "short_title" => p.short_title.clone(),
        "sku" => p.sku.clone(),
        "price" => crate::money::fmt_cents(p.price_cents),
        "sale_price" => p.sale_price_cents.map(crate::money::fmt_cents).unwrap_or_default(),
        "status" => p.status.clone(),
        "length_cm" => p.length_mm.map(|m| format!("{:.1} cm", m / 10.0)).unwrap_or_default(),
        _ => String::new(),
    }
}

#[derive(Deserialize)]
pub struct CreateBody {
    #[serde(default)]
    pub title: String,
}

pub async fn create_product(AxState(state): AxState<State>, Extension(cookie): Extension<CsrfCookie>, AuthApi(admin): AuthApi, headers: HeaderMap, Json(body): Json<CreateBody>) -> AppResult {
    if admin.preview {
        return Ok(api_err(StatusCode::FORBIDDEN, "Turn off Preview to add a pen."));
    }
    if !api_csrf_ok(&state, &cookie, &headers) {
        return Ok(api_err(StatusCode::FORBIDDEN, "The page expired. Reload and try again."));
    }
    let short = if body.title.trim().is_empty() { "New pen".to_string() } else { body.title.trim().chars().take(120).collect() };
    let title = format!("Vintage Pens: {short}");
    let base = crate::text::slugify(&format!("vintage-pens-{short}"));
    let mut slug = base.clone();
    let mut n = 2;
    while store::slug_taken(&state.pool, &slug).await? {
        slug = format!("{base}-{n}");
        n += 1;
    }
    let Some(cat) = store::term_id(&state.pool, "category", "vintage-pens").await? else {
        return Ok(api_err(StatusCode::INTERNAL_SERVER_ERROR, "The category list is missing."));
    };
    let id = store::create_draft_product(&state.pool, &slug, &title, &short, cat).await?;
    store::audit(&state.pool, "admin.create", serde_json::json!({"email": admin.email, "ip": admin.ip, "pen": short, "product_id": id})).await?;
    refresh_catalog(&state).await;
    Ok(Json(serde_json::json!({"ok": true, "url": format!("/product/{slug}/"), "id": id})).into_response())
}

pub async fn upload_photo(AxState(state): AxState<State>, Extension(cookie): Extension<CsrfCookie>, AuthApi(admin): AuthApi, headers: HeaderMap, Path(id): Path<i32>, mut mp: Multipart) -> AppResult {
    if admin.preview {
        return Ok(api_err(StatusCode::FORBIDDEN, "Turn off Preview to add photos."));
    }
    if !api_csrf_ok(&state, &cookie, &headers) {
        return Ok(api_err(StatusCode::FORBIDDEN, "The page expired. Reload and try again."));
    }
    let Some(p) = store::product_for_edit(&state.pool, id).await? else {
        return Err(AppError::NotFound);
    };
    let mut bytes: Option<Vec<u8>> = None;
    while let Some(field) = mp.next_field().await.map_err(|e| AppError::BadRequest(format!("upload: {e}")))? {
        if field.name() == Some("photo") {
            let b = field.bytes().await.map_err(|e| AppError::BadRequest(format!("photo: {e}")))?;
            if b.len() > 10 * 1024 * 1024 {
                return Ok(api_err(StatusCode::PAYLOAD_TOO_LARGE, "Photos must be under 10 MB."));
            }
            bytes = Some(b.to_vec());
        }
    }
    let Some(bytes) = bytes else {
        return Ok(api_err(StatusCode::BAD_REQUEST, "No photo was attached."));
    };
    let key = format!("uploads/admin/{}/{}-{}.jpg", chrono::Utc::now().format("%Y/%m"), p.id, super::auth::random_token(6));
    let dest = state.cfg.media_dir.join(&key);
    let imported = match crate::media::reencode_upload_with_variants(&bytes, &dest) {
        Ok(i) => i,
        Err(_) => return Ok(api_err(StatusCode::BAD_REQUEST, "That file could not be read as a photo (JPEG, PNG or WebP).")),
    };
    let image_id = store::insert_image(&state.pool, id, &key, &p.short_title, imported.width, imported.height, imported.has_480, imported.has_960).await?;
    store::audit(&state.pool, "admin.photo_added", serde_json::json!({"email": admin.email, "ip": admin.ip, "pen": p.short_title, "product_id": id, "file": key})).await?;
    refresh_catalog(&state).await;
    Ok(Json(serde_json::json!({"ok": true, "id": image_id, "full": format!("/media/{key}"), "large": format!("/media/{}", if imported.has_960 { crate::media::variant_rel(&key, 960) } else { key.clone() }), "thumb": format!("/media/{}", if imported.has_480 { crate::media::variant_rel(&key, 480) } else { key.clone() })})).into_response())
}

#[derive(Deserialize)]
pub struct OrderBody {
    pub ids: Vec<i32>,
}

pub async fn reorder_photos(AxState(state): AxState<State>, Extension(cookie): Extension<CsrfCookie>, AuthApi(admin): AuthApi, headers: HeaderMap, Path(id): Path<i32>, Json(body): Json<OrderBody>) -> AppResult {
    if admin.preview || !api_csrf_ok(&state, &cookie, &headers) {
        return Ok(api_err(StatusCode::FORBIDDEN, "Not allowed."));
    }
    let Some(p) = store::product_for_edit(&state.pool, id).await? else {
        return Err(AppError::NotFound);
    };
    if body.ids.len() > 50 {
        return Ok(api_err(StatusCode::BAD_REQUEST, "Too many photos."));
    }
    store::reorder_images(&state.pool, id, &body.ids).await?;
    store::audit(&state.pool, "admin.photos_reordered", serde_json::json!({"email": admin.email, "ip": admin.ip, "pen": p.short_title, "product_id": id})).await?;
    refresh_catalog(&state).await;
    Ok(Json(serde_json::json!({"ok": true})).into_response())
}

pub async fn archive_photo(AxState(state): AxState<State>, Extension(cookie): Extension<CsrfCookie>, AuthApi(admin): AuthApi, headers: HeaderMap, Path(image_id): Path<i32>) -> AppResult {
    if admin.preview || !api_csrf_ok(&state, &cookie, &headers) {
        return Ok(api_err(StatusCode::FORBIDDEN, "Not allowed."));
    }
    if !admin.step_up_fresh {
        return Ok(step_up_required());
    }
    let Some((product_id, path)) = store::image_owner(&state.pool, image_id).await? else {
        return Err(AppError::NotFound);
    };
    let pen = store::product_for_edit(&state.pool, product_id).await?.map(|p| p.short_title).unwrap_or_default();
    store::archive_image(&state.pool, image_id).await?;
    store::audit(&state.pool, "admin.photo_archived", serde_json::json!({"email": admin.email, "ip": admin.ip, "pen": pen, "product_id": product_id, "file": path})).await?;
    refresh_catalog(&state).await;
    Ok(Json(serde_json::json!({"ok": true})).into_response())
}

/// The pickers' choices for the editors (brands, eras, nibs, fillers, categories).
pub async fn options(AxState(state): AxState<State>, AuthApi(_admin): AuthApi) -> AppResult {
    let mut out = serde_json::Map::new();
    for t in ["brand", "era", "nib", "filling_mechanism", "category"] {
        out.insert(t.to_string(), serde_json::to_value(store::term_options(&state.pool, t).await?)?);
    }
    Ok(Json(serde_json::Value::Object(out)).into_response())
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn paragraphs_escape_html() {
        assert_eq!(paragraphs_html("Hello <b>there</b>\n\nSecond line\nwith break"), "<p>Hello &lt;b&gt;there&lt;/b&gt;</p>\n<p>Second line<br>with break</p>");
        assert_eq!(paragraphs_html("  \n\n "), "");
    }
    #[test]
    fn step_up_rules() {
        let p = store::ProductEdit { id: 1, slug: "x".into(), title: "t".into(), short_title: "s".into(), sku: "".into(), category_id: 1, brand_id: None, era_id: None, nib_id: None, filling_mechanism_id: None, price_cents: 10000, sale_price_cents: None, length_mm: None, status: "live".into(), description_html: "".into(), description_text: "".into() };
        assert!(needs_step_up("status", &p, "sold"));
        assert!(!needs_step_up("status", &p, "live"));
        assert!(needs_step_up("price", &p, "10"));
        assert!(!needs_step_up("price", &p, "120"));
        assert!(!needs_step_up("title", &p, "anything"));
    }
}
