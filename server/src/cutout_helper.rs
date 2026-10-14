//! Automatic cut-outs from Nathaniel's Mac. The helper in `static/helper/` runs on his Mac every 15
//! minutes: it asks which pens for sale have no cut-out (`GET /api/cutouts/needed`), makes each one with
//! Apple's Remove Background (the Vision subject mask, `scripts/cutout.swift`) and sends it back
//! (`POST /api/cutouts/{id}`), or says a photo has no clear subject (`POST /api/cutouts/{id}/none`).
//! It signs in with its own key, made at /admin/settings/ and good for nothing else; the site keeps only
//! the key's SHA-256, so a copy of the database cannot be used to send photos.

use crate::app::State;
use crate::settings;
use axum::extract::{Multipart, Path, State as AxState};
use axum::http::{header, HeaderMap, StatusCode};
use axum::response::{IntoResponse, Response};
use sha2::{Digest, Sha256};
use thepenmarket::media::cutout_rel;

pub const KEY_HASH: &str = "cutout_helper_key_sha256";
pub const LAST_SEEN: &str = "cutout_helper_last_seen";

pub fn hash(key: &str) -> String {
    hex::encode(Sha256::digest(key.as_bytes()))
}

fn same(a: &str, b: &str) -> bool {
    a.len() == b.len() && a.bytes().zip(b.bytes()).fold(0u8, |acc, (x, y)| acc | (x ^ y)) == 0
}

async fn authorized(state: &State, headers: &HeaderMap) -> bool {
    let Some(key) = headers.get(header::AUTHORIZATION).and_then(|v| v.to_str().ok()).and_then(|v| v.strip_prefix("Bearer ")) else {
        return false;
    };
    if key.len() < 32 {
        return false;
    }
    match settings::get(&state.pool, KEY_HASH).await.ok().flatten().and_then(|v| v.as_str().map(str::to_string)) {
        Some(stored) => same(&hash(key.trim()), &stored),
        None => false,
    }
}

fn refuse() -> Response {
    (StatusCode::UNAUTHORIZED, "This Mac's key is not valid any more. Set it up again from Homepage and photos in the admin.\n").into_response()
}

/// Lead photo of every pen for sale, relative to the media folder: (product id, photo, has a 960 copy).
async fn lead_photos(state: &State) -> anyhow::Result<Vec<(i32, String, bool)>> {
    use sqlx::Row;
    let rows = sqlx::query(
        "SELECT DISTINCT ON (i.product_id) i.product_id, i.path, i.has_960 FROM product_image i JOIN product p ON p.id = i.product_id \
         WHERE i.archived_at IS NULL AND p.status = 'live' AND p.sold_at IS NULL AND p.deleted_at IS NULL ORDER BY i.product_id, i.position",
    )
    .fetch_all(&state.pool)
    .await?;
    Ok(rows.iter().map(|r| (r.get("product_id"), r.get("path"), r.get("has_960"))).collect())
}

/// Pens for sale whose lead photo has no cut-out (and was not marked as having no clear subject).
pub async fn waiting(state: &State) -> anyhow::Result<Vec<(i32, String)>> {
    let media = state.cfg.media_dir.clone();
    Ok(lead_photos(state)
        .await?
        .into_iter()
        .filter(|(_, rel, _)| !rel.contains(".."))
        .filter(|(_, rel, _)| {
            let c = cutout_rel(rel);
            !media.join(&c).is_file() && !media.join(format!("{c}.none")).is_file()
        })
        .map(|(id, rel, has960)| (id, if has960 { thepenmarket::media::variant_rel(&rel, 960) } else { rel }))
        .collect())
}

/// `GET /api/cutouts/needed`: one line per pen, "<product id><TAB>/media/<photo>", at most 20.
pub async fn needed(AxState(state): AxState<State>, headers: HeaderMap) -> Response {
    if !authorized(&state, &headers).await {
        return refuse();
    }
    let _ = settings::set(&state.pool, LAST_SEEN, serde_json::json!(chrono::Utc::now().to_rfc3339())).await;
    match waiting(&state).await {
        Ok(list) => {
            let body: String = list.iter().take(20).map(|(id, rel)| format!("{id}\t/media/{rel}\n")).collect();
            ([(header::CONTENT_TYPE, "text/plain; charset=utf-8"), (header::CACHE_CONTROL, "no-store")], body).into_response()
        }
        Err(e) => {
            tracing::warn!("cutout helper list failed: {e:#}");
            (StatusCode::INTERNAL_SERVER_ERROR, "try again later\n").into_response()
        }
    }
}

/// `POST /api/cutouts/{id}` with a `photo` field: the cut-out the Mac made of that pen's lead photo.
pub async fn receive(AxState(state): AxState<State>, headers: HeaderMap, Path(id): Path<i32>, mut mp: Multipart) -> Response {
    if !authorized(&state, &headers).await {
        return refuse();
    }
    let mut bytes = None;
    while let Ok(Some(field)) = mp.next_field().await {
        if field.name() == Some("photo") {
            match field.bytes().await {
                Ok(b) if b.len() <= 10 * 1024 * 1024 => bytes = Some(b.to_vec()),
                _ => return (StatusCode::PAYLOAD_TOO_LARGE, "photo too large\n").into_response(),
            }
        }
    }
    let Some(bytes) = bytes else {
        return (StatusCode::BAD_REQUEST, "no photo\n").into_response();
    };
    match crate::hero_photo::store_cutout(&state, id, bytes).await {
        Ok(crate::hero_photo::Stored::Done) => {
            let _ = crate::admin::store::audit(&state.pool, "admin.hero_cutout_mac", serde_json::json!({"product_id": id})).await;
            (StatusCode::OK, "ok\n").into_response()
        }
        Ok(crate::hero_photo::Stored::NoPhoto) => (StatusCode::NOT_FOUND, "no such pen photo\n").into_response(),
        Ok(crate::hero_photo::Stored::StillHasBackground) => (StatusCode::UNPROCESSABLE_ENTITY, "the background is still there\n").into_response(),
        Ok(crate::hero_photo::Stored::Unreadable) => (StatusCode::UNPROCESSABLE_ENTITY, "not a readable image\n").into_response(),
        Err(e) => {
            tracing::warn!("cutout helper upload failed: {e:#}");
            (StatusCode::INTERNAL_SERVER_ERROR, "try again later\n").into_response()
        }
    }
}

/// `POST /api/cutouts/{id}/none`: the Mac found no clear subject in that photo; stop asking for it. A
/// cut-out he uploads himself later replaces this.
pub async fn none(AxState(state): AxState<State>, headers: HeaderMap, Path(id): Path<i32>) -> Response {
    if !authorized(&state, &headers).await {
        return refuse();
    }
    match crate::hero_photo::lead_photo(&state, id).await {
        Ok(Some(rel)) if !rel.contains("..") => {
            let _ = std::fs::write(state.cfg.media_dir.join(format!("{}.none", cutout_rel(&rel))), b"no clear subject (Mac helper)\n");
            (StatusCode::OK, "ok\n").into_response()
        }
        _ => (StatusCode::NOT_FOUND, "no such pen photo\n").into_response(),
    }
}
