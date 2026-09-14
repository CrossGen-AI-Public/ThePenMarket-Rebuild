//! Shared application state, the error type every handler returns, and page-level helpers
//! (rendering, canonical URLs, the entity constants every page repeats byte-for-byte).

use crate::config::Config;
use crate::engine::Catalog;
use askama::Template;
use axum::http::{header, StatusCode};
use axum::response::{Html, IntoResponse, Response};
use sqlx::PgPool;
use std::collections::HashMap;
use std::net::IpAddr;
use std::sync::{Arc, Mutex, RwLock};
use std::time::Instant;

/// The one entity, repeated identically in the footer, the contact page and the JSON-LD.
pub struct Entity;
impl Entity {
    pub const NAME: &'static str = "ThePenMarket.com";
    pub const OWNER: &'static str = "Nathaniel Cerf";
    pub const PO_BOX: &'static str = "P.O. Box 1086";
    pub const CITY: &'static str = "Norwich";
    pub const STATE: &'static str = "CT";
    pub const ZIP: &'static str = "06360-1086";
    pub const EMAIL: &'static str = "info@thepenmarket.com";
    pub const PHONE: &'static str = "(847) 708-5062";
    pub const PHONE_TEL: &'static str = "+18477085062";
    pub const TP_EMAIL: &'static str = "tradingpost@thepenmarket.com";
    pub const FACEBOOK: &'static str = "https://www.facebook.com/pages/Thepenmarketcom/427549874033020";
    pub const INSTAGRAM: &'static str = "https://www.instagram.com/thepenmarketdotcom";
    pub const TAGLINE: &'static str = "Sell, trade and buy pens the way you want.";
    pub const LEGAL: &'static str = "© 2026, ThePenMarket.com. All Rights Reserved.";
    pub const GUARANTEE_NAME: &'static str = "Our Guarantee";
    pub const BLOG_NAME: &'static str = "Drippy Musings";
}

pub struct RateLimiter {
    per_min: u32,
    hits: Mutex<HashMap<IpAddr, (u32, Instant)>>,
}
impl RateLimiter {
    pub fn new(per_min: u32) -> Self {
        Self { per_min, hits: Mutex::new(HashMap::new()) }
    }
    /// True when the caller is still within its budget for the current minute.
    pub fn allow(&self, ip: IpAddr) -> bool {
        let now = Instant::now();
        let Ok(mut map) = self.hits.lock() else { return true };
        if map.len() > 5000 {
            map.retain(|_, (_, t)| now.duration_since(*t).as_secs() < 60);
        }
        let entry = map.entry(ip).or_insert((0, now));
        if now.duration_since(entry.1).as_secs() >= 60 {
            *entry = (0, now);
        }
        entry.0 += 1;
        entry.0 <= self.per_min
    }
}

pub struct AppState {
    pub cfg: Config,
    pub pool: PgPool,
    pub catalog: RwLock<Arc<Catalog>>,
    pub guide_limiter: RateLimiter,
    pub form_limiter: RateLimiter,
    pub http: reqwest::Client,
    /// Short hash of the CSS and JS files, appended to their URLs so a redeploy never pairs new HTML
    /// with a stylesheet the browser cached from the previous build (static files are cached 7 days).
    pub asset_v: String,
    pub mailer: Arc<dyn crate::admin::mail::Mailer>,
    pub admin_limiter: RateLimiter,
}
pub type State = Arc<AppState>;

/// FNV-1a over the bytes of the site's own CSS and JS, as 8 hex characters.
pub fn asset_version(static_dir: &std::path::Path) -> String {
    let mut h: u64 = 0xcbf29ce484222325;
    for rel in ["css/site.css", "fonts/fonts.css", "js/site.js", "js/engine.js", "js/guide.js"] {
        if let Ok(bytes) = std::fs::read(static_dir.join(rel)) {
            for b in bytes {
                h ^= b as u64;
                h = h.wrapping_mul(0x100000001b3);
            }
        }
    }
    format!("{:08x}", (h >> 32) as u32 ^ h as u32)
}

impl AppState {
    pub fn catalog(&self) -> Arc<Catalog> {
        self.catalog.read().map(|c| c.clone()).unwrap_or_default()
    }
    pub fn origin(&self) -> &str {
        &self.cfg.site_origin
    }
    pub fn abs(&self, path: &str) -> String {
        format!("{}{}", self.cfg.site_origin, path)
    }
}

#[derive(Debug)]
pub enum AppError {
    NotFound,
    Forbidden(String),
    BadRequest(String),
    Internal(anyhow::Error),
}
impl<E: Into<anyhow::Error>> From<E> for AppError {
    fn from(e: E) -> Self {
        AppError::Internal(e.into())
    }
}
impl IntoResponse for AppError {
    fn into_response(self) -> Response {
        match self {
            AppError::NotFound => (StatusCode::NOT_FOUND, "not found").into_response(),
            AppError::Forbidden(m) => (StatusCode::FORBIDDEN, m).into_response(),
            AppError::BadRequest(m) => (StatusCode::BAD_REQUEST, m).into_response(),
            AppError::Internal(e) => {
                tracing::error!("internal: {e:#}");
                (StatusCode::INTERNAL_SERVER_ERROR, "something went wrong on our end").into_response()
            }
        }
    }
}
pub type AppResult = Result<Response, AppError>;

/// A true 301 (axum's `Redirect::permanent` is a 308; the old URLs must answer 301).
pub fn moved(target: &str) -> Response {
    let loc = header::HeaderValue::from_str(target).unwrap_or_else(|_| header::HeaderValue::from_static("/"));
    (StatusCode::MOVED_PERMANENTLY, [(header::LOCATION, loc)]).into_response()
}

/// Render an Askama template as a complete HTML response.
pub fn html<T: Template>(t: &T) -> AppResult {
    let body = t.render().map_err(|e| AppError::Internal(e.into()))?;
    Ok(([(header::CONTENT_TYPE, "text/html; charset=utf-8"), (header::CACHE_CONTROL, "no-cache")], Html(body)).into_response())
}

pub fn html_status<T: Template>(t: &T, status: StatusCode) -> AppResult {
    let body = t.render().map_err(|e| AppError::Internal(e.into()))?;
    Ok((status, [(header::CONTENT_TYPE, "text/html; charset=utf-8")], Html(body)).into_response())
}

/// What every page's `<head>` needs. Title and h1 do different jobs and are never the same string.
#[derive(Clone, Debug, Default)]
pub struct PageMeta {
    pub title: String,
    pub description: String,
    pub canonical: String,
    pub og_image: String,
    pub jsonld: String,
    pub noindex: bool,
    pub csrf: String,
    pub nav_counts: NavCounts,
    pub section: String,
    pub asset_v: String,
    /// Signed-in admin, if any (from the request's task-local; None for customers).
    pub admin: Option<crate::admin::AdminCtx>,
    pub admin_csrf: String,
    pub current_path: String,
}

#[derive(Clone, Debug, Default)]
pub struct NavCounts {
    pub vintage: usize,
    pub pre_owned: usize,
    pub pencils: usize,
    pub inkwells: usize,
    pub live_total: usize,
}

impl PageMeta {
    pub fn new(state: &AppState, title: &str, description: &str, path: &str) -> Self {
        let cat = state.catalog();
        let s = crate::engine::stats(&cat);
        let pick = |slug: &str| s.by_category.iter().find(|c| c.slug == slug).map(|c| c.count).unwrap_or(0);
        Self {
            title: title.to_string(),
            description: description.to_string(),
            canonical: state.abs(path),
            og_image: state.abs("/static/img/og-default.jpg"),
            jsonld: String::new(),
            noindex: false,
            csrf: String::new(),
            nav_counts: NavCounts { vintage: pick("vintage-pens"), pre_owned: pick("pre-owned-pens"), pencils: pick("pencils"), inkwells: pick("inkwells-blotters"), live_total: s.live_total },
            section: String::new(),
            asset_v: state.asset_v.clone(),
            admin_csrf: crate::admin::current().map(|a| a.csrf).unwrap_or_default(),
            admin: crate::admin::current(),
            current_path: path.to_string(),
        }
    }
    pub fn with_jsonld(mut self, blocks: Vec<serde_json::Value>) -> Self {
        let graph = serde_json::json!({ "@context": "https://schema.org", "@graph": blocks });
        self.jsonld = serde_json::to_string(&graph).unwrap_or_default().replace("</", "<\\/");
        self
    }
}

/// Organization + LocalBusiness (Store) node, with @id so the rest of the graph links to it.
pub fn org_node(state: &AppState) -> serde_json::Value {
    serde_json::json!({
        "@type": ["Organization", "Store"],
        "@id": state.abs("/#organization"),
        "name": Entity::NAME,
        "url": state.abs("/"),
        "logo": state.abs("/static/img/logo.png"),
        "image": state.abs("/static/img/og-default.jpg"),
        "email": Entity::EMAIL,
        "telephone": Entity::PHONE,
        "founder": { "@type": "Person", "@id": state.abs("/about-us/#nathaniel-cerf"), "name": Entity::OWNER },
        "foundingDate": "2007",
        "address": { "@type": "PostalAddress", "postOfficeBoxNumber": "1086", "addressLocality": Entity::CITY, "addressRegion": Entity::STATE, "postalCode": Entity::ZIP, "addressCountry": "US" },
        "sameAs": [Entity::FACEBOOK, Entity::INSTAGRAM],
        "priceRange": "$9.99 - $16,499.99",
        "slogan": Entity::TAGLINE
    })
}

pub fn website_node(state: &AppState) -> serde_json::Value {
    serde_json::json!({
        "@type": "WebSite",
        "@id": state.abs("/#website"),
        "url": state.abs("/"),
        "name": Entity::NAME,
        "publisher": { "@id": state.abs("/#organization") },
        "potentialAction": {
            "@type": "SearchAction",
            "target": { "@type": "EntryPoint", "urlTemplate": state.abs("/shop/?q={search_term_string}") },
            "query-input": "required name=search_term_string"
        }
    })
}

pub fn breadcrumb_node(state: &AppState, items: &[(&str, &str)]) -> serde_json::Value {
    let list: Vec<serde_json::Value> = items
        .iter()
        .enumerate()
        .map(|(i, (name, path))| serde_json::json!({ "@type": "ListItem", "position": i + 1, "name": name, "item": state.abs(path) }))
        .collect();
    serde_json::json!({ "@type": "BreadcrumbList", "itemListElement": list })
}
