//! Sourcing: the pens Nathaniel is hunting for. Each "want" carries prepared searches on the
//! marketplaces he buys from and what comparable pens are priced at in his own shop (computed from
//! the catalog, never guessed). With eBay Browse API keys in the environment the server also checks
//! eBay once a day and lists what it found under the price he set; without keys everything else
//! still works. Suggestions come from what visitors searched for and did not find.

use super::{store, Auth};
use crate::app::{html, AppError, AppResult, PageMeta, State};
use crate::money::fmt_dollars;
use crate::security::{self, CsrfCookie};
use askama::Template;
use axum::extract::{Form, Path, Query, State as AxState};
use axum::response::{IntoResponse, Redirect};
use axum::Extension;
use serde::Deserialize;
use sqlx::{PgPool, Row};

fn meta(state: &State, title: &str, path: &str, cookie: &CsrfCookie) -> PageMeta {
    let mut page = PageMeta::new(state, &format!("{title} | ThePenMarket.com admin"), "Admin area for ThePenMarket.com.", path);
    page.noindex = true;
    page.section = "admin".into();
    page.csrf = security::csrf_token(&state.cfg.csrf_secret, &cookie.0);
    page
}

pub struct FindView {
    pub id: i32,
    pub title: String,
    pub price: String,
    pub url: String,
    pub status: String,
    pub found: String,
}

pub struct WantView {
    pub id: i32,
    pub label: String,
    pub keywords: String,
    pub max_price: String,
    pub notes: String,
    pub status: String,
    pub source: String,
    pub checked: String,
    /// "7 like it in your shop, priced $85 to $240, middle $140" from his own listings.
    pub shop_prices: String,
    pub links: Vec<(String, String)>,
    pub finds: Vec<FindView>,
}

#[derive(Template)]
#[template(path = "admin/sourcing.html")]
pub struct SourcingTpl {
    pub page: PageMeta,
    pub wants: Vec<WantView>,
    pub suggestions: Vec<(String, i64)>,
    pub ebay_connected: bool,
    pub checked_now: bool,
}

fn tokens(s: &str) -> Vec<String> {
    s.to_lowercase().split(|c: char| !c.is_alphanumeric()).filter(|t| t.len() > 1).map(str::to_string).collect()
}

/// His own asking prices (in stock and sold) for pens whose name holds every keyword.
pub fn shop_prices(cat: &crate::engine::Catalog, keywords: &str) -> String {
    let want = tokens(keywords);
    if want.is_empty() {
        return String::new();
    }
    let mut prices: Vec<i64> = cat.pens.iter().filter(|p| { let hay = format!("{} {}", p.title, p.brand).to_lowercase(); want.iter().all(|t| hay.contains(t.as_str())) }).map(|p| p.price_cents).filter(|c| *c > 0).collect();
    if prices.is_empty() {
        return "None like it in your shop yet, so there is no price of yours to compare with.".into();
    }
    prices.sort_unstable();
    let mid = prices[prices.len() / 2];
    format!("{} like it in your shop (in stock or sold), priced {} to {}, middle {}.", prices.len(), fmt_dollars(prices[0]), fmt_dollars(prices[prices.len() - 1]), fmt_dollars(mid))
}

fn market_links(keywords: &str, max_cents: Option<i64>) -> Vec<(String, String)> {
    let q = urlencoding::encode(keywords.trim()).to_string();
    let cap = max_cents.map(|c| format!("&_udhi={}", c / 100)).unwrap_or_default();
    vec![
        ("eBay, for sale now".into(), format!("https://www.ebay.com/sch/i.html?_nkw={q}&_sacat=0&_sop=10{cap}")),
        ("eBay, what they sold for".into(), format!("https://www.ebay.com/sch/i.html?_nkw={q}&_sacat=0&LH_Sold=1&LH_Complete=1")),
        ("Etsy".into(), format!("https://www.etsy.com/search?q={q}")),
        ("LiveAuctioneers".into(), format!("https://www.liveauctioneers.com/search/?keyword={q}")),
    ]
}

#[derive(Deserialize, Default)]
pub struct PageQuery {
    #[serde(default)]
    pub checked: String,
}

pub async fn page(AxState(state): AxState<State>, Extension(cookie): Extension<CsrfCookie>, Auth(_admin): Auth, Query(q): Query<PageQuery>) -> AppResult {
    let cat = state.catalog();
    let rows = sqlx::query("SELECT id, label, keywords, max_cents, notes, status, source, checked_at FROM sourcing_want WHERE status <> 'archived' ORDER BY (status = 'active') DESC, created_at DESC").fetch_all(&state.pool).await?;
    let mut wants = vec![];
    for r in &rows {
        let id: i32 = r.get("id");
        let keywords: String = r.get("keywords");
        let max_cents: Option<i64> = r.get("max_cents");
        let finds = sqlx::query("SELECT id, title, price_cents, url, status, found_at FROM sourcing_find WHERE want_id = $1 AND status <> 'dismissed' ORDER BY (status = 'saved') DESC, found_at DESC LIMIT 12").bind(id).fetch_all(&state.pool).await?;
        wants.push(WantView {
            id,
            label: r.get("label"),
            shop_prices: shop_prices(&cat, &keywords),
            links: market_links(&keywords, max_cents),
            keywords,
            max_price: max_cents.map(|c| format!("{}", c / 100)).unwrap_or_default(),
            notes: r.get("notes"),
            status: r.get("status"),
            source: r.get("source"),
            checked: r.get::<Option<chrono::DateTime<chrono::Utc>>, _>("checked_at").map(super::handlers::fmt_time).unwrap_or_default(),
            finds: finds.iter().map(|f| FindView { id: f.get("id"), title: f.get("title"), price: f.get::<Option<i64>, _>("price_cents").map(crate::money::fmt_cents).unwrap_or_default(), url: f.get("url"), status: f.get("status"), found: super::handlers::fmt_time(f.get("found_at")) }).collect(),
        });
    }
    // Demand he could not fill: searches that found nothing, not already on the list.
    let have: Vec<String> = wants.iter().map(|w| w.keywords.to_lowercase()).collect();
    let suggestions = crate::insights::top_searches(&state.pool, 90, 12, "shop", true).await?.into_iter().filter(|s| !have.contains(&s.label)).map(|s| (s.label, s.n)).take(8).collect();
    html(&SourcingTpl { page: meta(&state, "Sourcing", "/admin/sourcing/", &cookie), wants, suggestions, ebay_connected: ebay_ready(&state), checked_now: q.checked == "1" })
}

#[derive(Deserialize)]
pub struct WantForm {
    pub csrf: String,
    #[serde(default)]
    pub label: String,
    #[serde(default)]
    pub keywords: String,
    #[serde(default)]
    pub max_price: String,
    #[serde(default)]
    pub notes: String,
    #[serde(default)]
    pub status: String,
    #[serde(default)]
    pub source: String,
}

fn guard(state: &State, cookie: &CsrfCookie, admin: &super::AdminCtx, csrf: &str) -> Result<(), AppError> {
    if admin.preview {
        return Err(AppError::Forbidden("Turn off Preview to make changes.".into()));
    }
    if !security::csrf_ok(&state.cfg.csrf_secret, &cookie.0, csrf) {
        return Err(AppError::Forbidden("The form expired. Reload the page and try again.".into()));
    }
    Ok(())
}

pub async fn want_save(AxState(state): AxState<State>, Extension(cookie): Extension<CsrfCookie>, Auth(admin): Auth, id: Option<Path<i32>>, Form(form): Form<WantForm>) -> AppResult {
    guard(&state, &cookie, &admin, &form.csrf)?;
    let keywords: String = form.keywords.trim().chars().take(120).collect();
    if keywords.is_empty() {
        return Err(AppError::BadRequest("Say what to search for. Go back; nothing was lost.".into()));
    }
    let label: String = if form.label.trim().is_empty() { keywords.clone() } else { form.label.trim().chars().take(120).collect() };
    let max_cents = if form.max_price.trim().is_empty() { None } else { super::auth::parse_dollars(&form.max_price).ok() };
    let notes: String = form.notes.trim().chars().take(1000).collect();
    match id {
        Some(Path(id)) => {
            let status = match form.status.as_str() {
                "paused" => "paused",
                "archived" => "archived",
                _ => "active",
            };
            sqlx::query("UPDATE sourcing_want SET label = $2, keywords = $3, max_cents = $4, notes = $5, status = $6 WHERE id = $1").bind(id).bind(&label).bind(&keywords).bind(max_cents).bind(&notes).bind(status).execute(&state.pool).await?;
        }
        None => {
            let source = if form.source == "search-demand" { "search-demand" } else { "manual" };
            sqlx::query("INSERT INTO sourcing_want (label, keywords, max_cents, notes, source) VALUES ($1,$2,$3,$4,$5)").bind(&label).bind(&keywords).bind(max_cents).bind(&notes).bind(source).execute(&state.pool).await?;
        }
    }
    store::audit(&state.pool, "admin.sourcing_saved", serde_json::json!({"email": admin.email, "ip": admin.ip, "want": label})).await?;
    Ok(Redirect::to("/admin/sourcing/").into_response())
}

#[derive(Deserialize)]
pub struct FindForm {
    pub csrf: String,
    pub status: String,
}

pub async fn find_status(AxState(state): AxState<State>, Extension(cookie): Extension<CsrfCookie>, Auth(admin): Auth, Path(id): Path<i32>, Form(form): Form<FindForm>) -> AppResult {
    guard(&state, &cookie, &admin, &form.csrf)?;
    let status = match form.status.as_str() {
        "saved" => "saved",
        "dismissed" => "dismissed",
        _ => "new",
    };
    sqlx::query("UPDATE sourcing_find SET status = $2 WHERE id = $1").bind(id).bind(status).execute(&state.pool).await?;
    Ok(Redirect::to("/admin/sourcing/").into_response())
}

#[derive(Deserialize)]
pub struct CsrfOnly {
    pub csrf: String,
}

pub async fn check_now(AxState(state): AxState<State>, Extension(cookie): Extension<CsrfCookie>, Auth(admin): Auth, Form(form): Form<CsrfOnly>) -> AppResult {
    guard(&state, &cookie, &admin, &form.csrf)?;
    if let Err(e) = check_all(&state).await {
        tracing::warn!("sourcing check failed: {e:#}");
    }
    Ok(Redirect::to("/admin/sourcing/?checked=1").into_response())
}

// ---- eBay Browse API (only with EBAY_APP_ID and EBAY_CERT_ID set) -------------------------------------

pub fn ebay_ready(state: &State) -> bool {
    !state.cfg.ebay_app_id.is_empty() && !state.cfg.ebay_cert_id.is_empty()
}

async fn ebay_token(state: &State) -> anyhow::Result<String> {
    let res = state
        .http
        .post("https://api.ebay.com/identity/v1/oauth2/token")
        .basic_auth(&state.cfg.ebay_app_id, Some(&state.cfg.ebay_cert_id))
        .header(reqwest::header::CONTENT_TYPE, "application/x-www-form-urlencoded")
        .body("grant_type=client_credentials&scope=https%3A%2F%2Fapi.ebay.com%2Foauth%2Fapi_scope")
        .send()
        .await?
        .error_for_status()?;
    let j: serde_json::Value = res.json().await?;
    j.get("access_token").and_then(|t| t.as_str()).map(str::to_string).ok_or_else(|| anyhow::anyhow!("eBay returned no access token"))
}

/// One Browse search per active want; new listings become `sourcing_find` rows (deduplicated by item id).
pub async fn check_all(state: &State) -> anyhow::Result<usize> {
    if !ebay_ready(state) {
        return Ok(0);
    }
    let token = ebay_token(state).await?;
    let wants = sqlx::query("SELECT id, keywords, max_cents FROM sourcing_want WHERE status = 'active'").fetch_all(&state.pool).await?;
    let mut added = 0;
    for w in &wants {
        let id: i32 = w.get("id");
        let keywords: String = w.get("keywords");
        let max_cents: Option<i64> = w.get("max_cents");
        let mut url = format!("https://api.ebay.com/buy/browse/v1/item_summary/search?q={}&limit=20&sort=newlyListed", urlencoding::encode(&keywords));
        if let Some(c) = max_cents {
            url.push_str(&format!("&filter={}", urlencoding::encode(&format!("price:[..{}],priceCurrency:USD", c / 100))));
        }
        let res = state.http.get(&url).bearer_auth(&token).header("X-EBAY-C-MARKETPLACE-ID", "EBAY_US").send().await?;
        if !res.status().is_success() {
            tracing::warn!(status = %res.status(), "eBay search failed for want {id}");
            continue;
        }
        let j: serde_json::Value = res.json().await?;
        for item in j.get("itemSummaries").and_then(|a| a.as_array()).map(|a| a.as_slice()).unwrap_or(&[]) {
            let (Some(ext), Some(title), Some(link)) = (item.get("itemId").and_then(|v| v.as_str()), item.get("title").and_then(|v| v.as_str()), item.get("itemWebUrl").and_then(|v| v.as_str())) else { continue };
            if !link.starts_with("https://www.ebay.com/") {
                continue;
            }
            let cents = item.get("price").and_then(|p| p.get("value")).and_then(|v| v.as_str()).and_then(|v| v.parse::<f64>().ok()).map(|d| (d * 100.0).round() as i64);
            added += insert_find(&state.pool, id, "ebay", ext, title, cents, link).await?;
        }
        sqlx::query("UPDATE sourcing_want SET checked_at = now() WHERE id = $1").bind(id).execute(&state.pool).await?;
    }
    Ok(added)
}

async fn insert_find(pool: &PgPool, want_id: i32, provider: &str, external_id: &str, title: &str, price_cents: Option<i64>, url: &str) -> anyhow::Result<usize> {
    let n = sqlx::query("INSERT INTO sourcing_find (want_id, provider, external_id, title, price_cents, url) VALUES ($1,$2,$3,$4,$5,$6) ON CONFLICT (provider, external_id) DO NOTHING")
        .bind(want_id).bind(provider).bind(external_id).bind(title.chars().take(200).collect::<String>()).bind(price_cents).bind(url).execute(pool).await?.rows_affected();
    Ok(n as usize)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn links_carry_the_words_and_the_cap() {
        let l = market_links("parker 51 aerometric", Some(12_000));
        assert!(l[0].1.contains("_nkw=parker%2051%20aerometric") && l[0].1.contains("_udhi=120"));
        assert!(l[1].1.contains("LH_Sold=1"));
        assert_eq!(tokens("Mont-Blanc 146, a"), vec!["mont", "blanc", "146"]);
    }
}
