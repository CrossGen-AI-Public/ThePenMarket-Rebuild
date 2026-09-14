//! All SQL for the admin feature. Parameterised only.

use chrono::{DateTime, Duration, Utc};
use sqlx::{PgPool, Row};

#[derive(Clone, Debug, sqlx::FromRow)]
pub struct Account {
    pub id: i32,
    pub email: String,
    pub display_name: String,
    pub password_hash: String,
    pub failed_count: i32,
    pub failed_since: Option<DateTime<Utc>>,
    pub locked_until: Option<DateTime<Utc>>,
}

#[derive(Clone, Debug, sqlx::FromRow)]
pub struct Session {
    pub id: i32,
    pub account_id: i32,
    pub device_id: Option<i32>,
    pub created_at: DateTime<Utc>,
    pub last_seen_at: DateTime<Utc>,
    pub step_up_at: Option<DateTime<Utc>>,
    pub preview: bool,
    pub revoked_at: Option<DateTime<Utc>>,
}

#[derive(Clone, Debug, sqlx::FromRow)]
pub struct Device {
    pub id: i32,
    pub name: String,
    pub ip: String,
    pub first_seen_at: DateTime<Utc>,
    pub last_seen_at: DateTime<Utc>,
    pub expires_at: DateTime<Utc>,
    pub revoked_at: Option<DateTime<Utc>>,
}

#[derive(Clone, Debug, sqlx::FromRow)]
pub struct Code {
    pub id: i32,
    pub account_id: i32,
    pub code_hash: String,
    pub expires_at: DateTime<Utc>,
    pub used_at: Option<DateTime<Utc>>,
    pub attempts: i32,
}

pub async fn account_by_email(pool: &PgPool, email: &str) -> anyhow::Result<Option<Account>> {
    Ok(sqlx::query_as("SELECT id, email, display_name, password_hash, failed_count, failed_since, locked_until FROM admin_account WHERE email = $1").bind(email.trim().to_lowercase()).fetch_optional(pool).await?)
}

pub async fn account_by_id(pool: &PgPool, id: i32) -> anyhow::Result<Option<Account>> {
    Ok(sqlx::query_as("SELECT id, email, display_name, password_hash, failed_count, failed_since, locked_until FROM admin_account WHERE id = $1").bind(id).fetch_optional(pool).await?)
}

pub async fn upsert_account(pool: &PgPool, email: &str, display_name: &str, password_hash: &str) -> anyhow::Result<i32> {
    let row = sqlx::query("INSERT INTO admin_account (email, display_name, password_hash) VALUES ($1, $2, $3) ON CONFLICT (email) DO UPDATE SET password_hash = EXCLUDED.password_hash, display_name = EXCLUDED.display_name, password_set_at = now(), failed_count = 0, locked_until = NULL RETURNING id")
        .bind(email.trim().to_lowercase()).bind(display_name).bind(password_hash).fetch_one(pool).await?;
    Ok(row.get("id"))
}

pub async fn set_password(pool: &PgPool, account_id: i32, password_hash: &str) -> anyhow::Result<()> {
    sqlx::query("UPDATE admin_account SET password_hash = $2, password_set_at = now(), failed_count = 0, failed_since = NULL, locked_until = NULL WHERE id = $1").bind(account_id).bind(password_hash).execute(pool).await?;
    Ok(())
}

pub async fn unlock(pool: &PgPool, account_id: i32) -> anyhow::Result<()> {
    sqlx::query("UPDATE admin_account SET failed_count = 0, failed_since = NULL, locked_until = NULL WHERE id = $1").bind(account_id).execute(pool).await?;
    Ok(())
}

/// Record a wrong password; returns true when this attempt crossed the lockout threshold.
pub async fn record_failure(pool: &PgPool, account_id: i32) -> anyhow::Result<bool> {
    let now = Utc::now();
    let window = Duration::minutes(super::auth::LOCK_WINDOW_MIN);
    let acc = account_by_id(pool, account_id).await?.ok_or_else(|| anyhow::anyhow!("no account"))?;
    let (count, since) = match acc.failed_since {
        Some(s) if now - s < window => (acc.failed_count + 1, s),
        _ => (1, now),
    };
    let lock = count >= super::auth::LOCK_AFTER;
    let until = if lock { Some(now + Duration::hours(super::auth::LOCK_HOURS)) } else { None };
    sqlx::query("UPDATE admin_account SET failed_count = $2, failed_since = $3, locked_until = COALESCE($4, locked_until) WHERE id = $1").bind(account_id).bind(count).bind(since).bind(until).execute(pool).await?;
    Ok(lock)
}

pub async fn clear_failures(pool: &PgPool, account_id: i32) -> anyhow::Result<()> {
    sqlx::query("UPDATE admin_account SET failed_count = 0, failed_since = NULL WHERE id = $1").bind(account_id).execute(pool).await?;
    Ok(())
}

pub async fn create_session(pool: &PgPool, account_id: i32, token_hash: &str, device_id: Option<i32>, ip: &str, ua: &str, step_up: bool) -> anyhow::Result<i32> {
    let row = sqlx::query("INSERT INTO admin_session (account_id, token_hash, device_id, ip, user_agent, step_up_at) VALUES ($1, $2, $3, $4, $5, CASE WHEN $6 THEN now() ELSE NULL END) RETURNING id")
        .bind(account_id).bind(token_hash).bind(device_id).bind(ip).bind(ua.chars().take(300).collect::<String>()).bind(step_up).fetch_one(pool).await?;
    Ok(row.get("id"))
}

/// Live session for a cookie: not revoked, not idle past the limit, not older than the absolute limit.
pub async fn session_by_token(pool: &PgPool, token_hash: &str) -> anyhow::Result<Option<Session>> {
    let s: Option<Session> = sqlx::query_as("SELECT id, account_id, device_id, created_at, last_seen_at, step_up_at, preview, revoked_at FROM admin_session WHERE token_hash = $1").bind(token_hash).fetch_optional(pool).await?;
    let Some(s) = s else { return Ok(None) };
    let now = Utc::now();
    if s.revoked_at.is_some() || now - s.last_seen_at > Duration::minutes(super::auth::SESSION_IDLE_MIN) || now - s.created_at > Duration::hours(super::auth::SESSION_ABSOLUTE_HOURS) {
        return Ok(None);
    }
    sqlx::query("UPDATE admin_session SET last_seen_at = now() WHERE id = $1").bind(s.id).execute(pool).await?;
    Ok(Some(s))
}

pub async fn revoke_session(pool: &PgPool, id: i32) -> anyhow::Result<()> {
    sqlx::query("UPDATE admin_session SET revoked_at = now() WHERE id = $1 AND revoked_at IS NULL").bind(id).execute(pool).await?;
    Ok(())
}

pub async fn revoke_all_sessions(pool: &PgPool, account_id: i32) -> anyhow::Result<()> {
    sqlx::query("UPDATE admin_session SET revoked_at = now() WHERE account_id = $1 AND revoked_at IS NULL").bind(account_id).execute(pool).await?;
    Ok(())
}

pub async fn set_preview(pool: &PgPool, session_id: i32, on: bool) -> anyhow::Result<()> {
    sqlx::query("UPDATE admin_session SET preview = $2 WHERE id = $1").bind(session_id).bind(on).execute(pool).await?;
    Ok(())
}

pub async fn touch_step_up(pool: &PgPool, session_id: i32) -> anyhow::Result<()> {
    sqlx::query("UPDATE admin_session SET step_up_at = now() WHERE id = $1").bind(session_id).execute(pool).await?;
    Ok(())
}

pub async fn device_by_token(pool: &PgPool, account_id: i32, token_hash: &str) -> anyhow::Result<Option<Device>> {
    let d: Option<Device> = sqlx::query_as("SELECT id, name, ip, first_seen_at, last_seen_at, expires_at, revoked_at FROM admin_device WHERE account_id = $1 AND token_hash = $2").bind(account_id).bind(token_hash).fetch_optional(pool).await?;
    let Some(d) = d else { return Ok(None) };
    if d.revoked_at.is_some() || d.expires_at < Utc::now() {
        return Ok(None);
    }
    sqlx::query("UPDATE admin_device SET last_seen_at = now() WHERE id = $1").bind(d.id).execute(pool).await?;
    Ok(Some(d))
}

pub async fn create_device(pool: &PgPool, account_id: i32, token_hash: &str, name: &str, ip: &str) -> anyhow::Result<i32> {
    let row = sqlx::query("INSERT INTO admin_device (account_id, token_hash, name, ip, expires_at) VALUES ($1, $2, $3, $4, now() + make_interval(days => $5)) RETURNING id")
        .bind(account_id).bind(token_hash).bind(name).bind(ip).bind(super::auth::DEVICE_DAYS as i32).fetch_one(pool).await?;
    Ok(row.get("id"))
}

pub async fn devices(pool: &PgPool, account_id: i32) -> anyhow::Result<Vec<Device>> {
    Ok(sqlx::query_as("SELECT id, name, ip, first_seen_at, last_seen_at, expires_at, revoked_at FROM admin_device WHERE account_id = $1 AND revoked_at IS NULL AND expires_at > now() ORDER BY last_seen_at DESC").bind(account_id).fetch_all(pool).await?)
}

pub async fn revoke_device(pool: &PgPool, account_id: i32, device_id: i32) -> anyhow::Result<()> {
    sqlx::query("UPDATE admin_device SET revoked_at = now() WHERE id = $1 AND account_id = $2").bind(device_id).bind(account_id).execute(pool).await?;
    sqlx::query("UPDATE admin_session SET revoked_at = now() WHERE device_id = $1 AND revoked_at IS NULL").bind(device_id).execute(pool).await?;
    Ok(())
}

pub async fn create_code(pool: &PgPool, account_id: i32, kind: &str, code_hash: &str, minutes: i64, ip: &str) -> anyhow::Result<i32> {
    // Only one live code of each kind per account: older ones stop working when a new one is sent.
    sqlx::query("UPDATE admin_code SET used_at = now() WHERE account_id = $1 AND kind = $2 AND used_at IS NULL").bind(account_id).bind(kind).execute(pool).await?;
    let row = sqlx::query("INSERT INTO admin_code (account_id, kind, code_hash, expires_at, ip) VALUES ($1, $2, $3, now() + make_interval(mins => $4), $5) RETURNING id")
        .bind(account_id).bind(kind).bind(code_hash).bind(minutes as i32).bind(ip).fetch_one(pool).await?;
    Ok(row.get("id"))
}

pub async fn code_by_id(pool: &PgPool, id: i32, kind: &str) -> anyhow::Result<Option<Code>> {
    Ok(sqlx::query_as("SELECT id, account_id, code_hash, expires_at, used_at, attempts FROM admin_code WHERE id = $1 AND kind = $2").bind(id).bind(kind).fetch_optional(pool).await?)
}

pub async fn code_by_hash(pool: &PgPool, kind: &str, code_hash: &str) -> anyhow::Result<Option<Code>> {
    Ok(sqlx::query_as("SELECT id, account_id, code_hash, expires_at, used_at, attempts FROM admin_code WHERE kind = $1 AND code_hash = $2").bind(kind).bind(code_hash).fetch_optional(pool).await?)
}

pub async fn bump_code_attempts(pool: &PgPool, id: i32) -> anyhow::Result<i32> {
    let row = sqlx::query("UPDATE admin_code SET attempts = attempts + 1 WHERE id = $1 RETURNING attempts").bind(id).fetch_one(pool).await?;
    Ok(row.get("attempts"))
}

pub async fn use_code(pool: &PgPool, id: i32) -> anyhow::Result<()> {
    sqlx::query("UPDATE admin_code SET used_at = now() WHERE id = $1").bind(id).execute(pool).await?;
    Ok(())
}

/// Append-only audit row. `detail` carries who, what, before and after.
pub async fn audit(pool: &PgPool, kind: &str, detail: serde_json::Value) -> anyhow::Result<()> {
    sqlx::query("INSERT INTO event_log (kind, detail) VALUES ($1, $2)").bind(kind).bind(detail).execute(pool).await?;
    Ok(())
}

#[derive(Clone, Debug)]
pub struct AuditRow {
    pub at: DateTime<Utc>,
    pub kind: String,
    pub detail: serde_json::Value,
}

pub async fn history(pool: &PgPool, limit: i64) -> anyhow::Result<Vec<AuditRow>> {
    let rows = sqlx::query("SELECT at, kind, detail FROM event_log WHERE kind LIKE 'admin.%' ORDER BY at DESC LIMIT $1").bind(limit).fetch_all(pool).await?;
    Ok(rows.into_iter().map(|r| AuditRow { at: r.get("at"), kind: r.get("kind"), detail: r.get("detail") }).collect())
}

// ---- product editing -------------------------------------------------------------------------

#[derive(Clone, Debug, sqlx::FromRow)]
pub struct ProductEdit {
    pub id: i32,
    pub slug: String,
    pub title: String,
    pub short_title: String,
    pub sku: String,
    pub category_id: i32,
    pub brand_id: Option<i32>,
    pub era_id: Option<i32>,
    pub nib_id: Option<i32>,
    pub filling_mechanism_id: Option<i32>,
    pub price_cents: i64,
    pub sale_price_cents: Option<i64>,
    pub length_mm: Option<f64>,
    pub status: String,
    pub description_html: String,
    pub description_text: String,
}

pub async fn product_for_edit(pool: &PgPool, id: i32) -> anyhow::Result<Option<ProductEdit>> {
    Ok(sqlx::query_as("SELECT id, slug, title, short_title, sku, category_id, brand_id, era_id, nib_id, filling_mechanism_id, price_cents, sale_price_cents, length_mm::float8 AS length_mm, status, description_html, description_text FROM product WHERE id = $1").bind(id).fetch_optional(pool).await?)
}

pub async fn term_id(pool: &PgPool, table: &str, slug: &str) -> anyhow::Result<Option<i32>> {
    let sql = match table {
        "brand" => "SELECT id FROM brand WHERE slug = $1",
        "era" => "SELECT id FROM era WHERE slug = $1",
        "nib" => "SELECT id FROM nib WHERE slug = $1",
        "filling_mechanism" => "SELECT id FROM filling_mechanism WHERE slug = $1",
        "category" => "SELECT id FROM category WHERE slug = $1",
        _ => return Ok(None),
    };
    let row = sqlx::query(sql).bind(slug).fetch_optional(pool).await?;
    Ok(row.map(|r| r.get::<i32, _>("id")))
}

pub async fn update_product_field(pool: &PgPool, id: i32, column: &str, value: ProductValue) -> anyhow::Result<()> {
    // Column names come from a fixed match in api.rs, never from the request.
    let sql = format!("UPDATE product SET {column} = $2, updated_at = now() WHERE id = $1");
    let q = sqlx::query(&sql).bind(id);
    let q = match value {
        ProductValue::Text(s) => q.bind(s),
        ProductValue::Int(i) => q.bind(i),
        ProductValue::OptInt(i) => q.bind(i),
        ProductValue::Big(i) => q.bind(i),
        ProductValue::OptBig(i) => q.bind(i),
        ProductValue::OptNum(f) => q.bind(f),
    };
    q.execute(pool).await?;
    Ok(())
}

pub enum ProductValue {
    Text(String),
    Int(i32),
    OptInt(Option<i32>),
    Big(i64),
    OptBig(Option<i64>),
    OptNum(Option<f64>),
}

pub async fn update_description(pool: &PgPool, id: i32, html: &str, text: &str) -> anyhow::Result<()> {
    sqlx::query("UPDATE product SET description_html = $2, description_text = $3, search_text = title || ' ' || $3, updated_at = now() WHERE id = $1").bind(id).bind(html).bind(text).execute(pool).await?;
    Ok(())
}

pub async fn create_draft_product(pool: &PgPool, slug: &str, title: &str, short_title: &str, category_id: i32) -> anyhow::Result<i32> {
    let row = sqlx::query("INSERT INTO product (sku, slug, title, short_title, category_id, price_cents, status, listed_year, listed_month, search_text) VALUES ('', $1, $2, $3, $4, 0, 'draft', EXTRACT(YEAR FROM now())::int, EXTRACT(MONTH FROM now())::int, $2) RETURNING id")
        .bind(slug).bind(title).bind(short_title).bind(category_id).fetch_one(pool).await?;
    Ok(row.get("id"))
}

pub async fn slug_taken(pool: &PgPool, slug: &str) -> anyhow::Result<bool> {
    let row = sqlx::query("SELECT count(*) AS n FROM product WHERE slug = $1").bind(slug).fetch_one(pool).await?;
    Ok(row.get::<i64, _>("n") > 0)
}

pub async fn insert_image(pool: &PgPool, product_id: i32, path: &str, alt: &str, w: u32, h: u32, has_480: bool, has_960: bool) -> anyhow::Result<i32> {
    let row = sqlx::query("INSERT INTO product_image (product_id, position, path, alt, width, height, has_480, has_960) VALUES ($1, (SELECT COALESCE(MAX(position), -1) + 1 FROM product_image WHERE product_id = $1), $2, $3, $4, $5, $6, $7) RETURNING id")
        .bind(product_id).bind(path).bind(alt).bind(w as i32).bind(h as i32).bind(has_480).bind(has_960).fetch_one(pool).await?;
    Ok(row.get("id"))
}

pub async fn image_owner(pool: &PgPool, image_id: i32) -> anyhow::Result<Option<(i32, String)>> {
    let row = sqlx::query("SELECT product_id, path FROM product_image WHERE id = $1 AND archived_at IS NULL").bind(image_id).fetch_optional(pool).await?;
    Ok(row.map(|r| (r.get("product_id"), r.get("path"))))
}

pub async fn archive_image(pool: &PgPool, image_id: i32) -> anyhow::Result<()> {
    sqlx::query("UPDATE product_image SET archived_at = now() WHERE id = $1").bind(image_id).execute(pool).await?;
    Ok(())
}

pub async fn reorder_images(pool: &PgPool, product_id: i32, ids: &[i32]) -> anyhow::Result<()> {
    let mut tx = pool.begin().await?;
    for (pos, id) in ids.iter().enumerate() {
        sqlx::query("UPDATE product_image SET position = $3 WHERE id = $1 AND product_id = $2").bind(id).bind(product_id).bind(pos as i32).execute(&mut *tx).await?;
    }
    tx.commit().await?;
    Ok(())
}

#[derive(Clone, Debug, serde::Serialize)]
pub struct TermOption {
    pub slug: String,
    pub name: String,
}

pub async fn term_options(pool: &PgPool, table: &str) -> anyhow::Result<Vec<TermOption>> {
    let sql = match table {
        "brand" => "SELECT slug, name FROM brand ORDER BY name",
        "era" => "SELECT slug, name FROM era ORDER BY sort_order, name",
        "nib" => "SELECT slug, name FROM nib ORDER BY sort_order, name",
        "filling_mechanism" => "SELECT slug, name FROM filling_mechanism ORDER BY sort_order, name",
        "category" => "SELECT slug, name FROM category WHERE slug <> 'memberships' ORDER BY sort_order, name",
        _ => return Ok(vec![]),
    };
    let rows = sqlx::query(sql).fetch_all(pool).await?;
    Ok(rows.into_iter().map(|r| TermOption { slug: r.get("slug"), name: r.get("name") }).collect())
}
