//! First-party visit analytics and A/B experiments. The browser script (`static/js/track.js`) posts
//! small batches to `/api/t`; nothing leaves this server. A visitor is a random cookie value: no
//! name, e-mail or IP address is stored, and browsers that send Do Not Track or Global Privacy
//! Control get no cookie and are never recorded. All SQL for both features is here, parameterised.

use crate::app::{AppState, State};
use axum::body::Bytes;
use axum::extract::{ConnectInfo, Request, State as AxState};
use axum::http::{header, HeaderMap, HeaderValue, StatusCode};
use axum::middleware::Next;
use axum::response::{IntoResponse, Response};
use rand::RngCore;
use serde::Deserialize;
use sqlx::{PgPool, Row};
use std::collections::HashMap;
use std::net::SocketAddr;

pub const VISITOR_COOKIE: &str = "pm_v";
const MAX_EVENTS: usize = 40;
const KEEP_DAYS: i32 = 400;

tokio::task_local! {
    pub static VISITOR: Option<String>;
}

pub fn current_visitor() -> Option<String> {
    VISITOR.try_with(|v| v.clone()).ok().flatten()
}

fn valid_visitor(v: &str) -> bool {
    v.len() == 24 && v.chars().all(|c| c.is_ascii_hexdigit())
}

/// The visitor cookie is `<id>.<signature>`: only ids this site handed out are counted, so nobody can
/// invent visitors to push a phrase into "popular searches" or tilt an experiment.
fn visitor_sig(secret: &str, id: &str) -> String {
    crate::security::csrf_token(secret, &format!("visitor:{id}"))[..16].to_string()
}

fn read_visitor(secret: &str, cookie: &str) -> Option<String> {
    let (id, sig) = cookie.split_once('.')?;
    (valid_visitor(id) && crate::admin::auth::eq_ct(sig, &visitor_sig(secret, id))).then(|| id.to_string())
}

/// Events one visitor may record in a day; a person reading the site never comes near it.
const EVENTS_PER_VISITOR_DAY: u32 = 600;
static SPENT: std::sync::Mutex<Option<(chrono::NaiveDate, std::collections::HashMap<String, u32>)>> = std::sync::Mutex::new(None);

fn take_events(visitor: &str, n: u32) -> u32 {
    let today = chrono::Utc::now().date_naive();
    let Ok(mut g) = SPENT.lock() else { return 0 };
    let (day, map) = g.get_or_insert_with(|| (today, Default::default()));
    if *day != today {
        *day = today;
        map.clear();
    }
    let used = map.entry(visitor.to_string()).or_insert(0);
    let allowed = n.min(EVENTS_PER_VISITOR_DAY.saturating_sub(*used));
    *used += allowed;
    allowed
}

fn opted_out(headers: &HeaderMap) -> bool {
    let is_one = |k: &str| headers.get(k).and_then(|v| v.to_str().ok()).map(|v| v.trim() == "1").unwrap_or(false);
    is_one("dnt") || is_one("sec-gpc")
}

/// Give every page request a visitor id (unless the browser opted out) and expose it on the task.
pub async fn visitor(AxState(state): AxState<State>, req: Request, next: Next) -> Response {
    let path = req.uri().path();
    if path.starts_with("/static/") || path.starts_with("/media/") || path.starts_with("/admin/") || path == "/healthz" {
        return VISITOR.scope(None, next.run(req)).await;
    }
    let existing = crate::admin::cookie(req.headers(), VISITOR_COOKIE).and_then(|v| read_visitor(&state.cfg.csrf_secret, &v));
    let out = opted_out(req.headers());
    let https = crate::admin::is_https(req.headers());
    let (id, fresh) = match (existing, out) {
        (_, true) => (None, false),
        (Some(v), false) => (Some(v), false),
        (None, false) => {
            let mut b = [0u8; 12];
            rand::rng().fill_bytes(&mut b);
            (Some(hex::encode(b)), true)
        }
    };
    let mut res = VISITOR.scope(id.clone(), next.run(req)).await;
    if let (Some(v), true) = (id, fresh) {
        let c = format!("{VISITOR_COOKIE}={v}.{}; Path=/; SameSite=Lax; Max-Age=31536000{}", visitor_sig(&state.cfg.csrf_secret, &v), if https { "; Secure" } else { "" });
        if let Ok(hv) = HeaderValue::from_str(&c) {
            res.headers_mut().append(header::SET_COOKIE, hv);
        }
    }
    res
}

// ---- ingest ------------------------------------------------------------------------------------

#[derive(Deserialize)]
struct Batch {
    #[serde(default)]
    p: String,
    #[serde(default)]
    d: String,
    #[serde(default)]
    pid: Option<i32>,
    #[serde(default)]
    e: Vec<Event>,
}

#[derive(Deserialize)]
struct Event {
    k: String,
    #[serde(default)]
    d: serde_json::Map<String, serde_json::Value>,
}

/// Keep only the keys each kind is allowed to carry, strings bounded, numbers finite.
fn clean_detail(kind: &str, d: &serde_json::Map<String, serde_json::Value>) -> serde_json::Value {
    let allowed: &[&str] = match kind {
        "view" => &["ref", "dh", "vw"],
        "click" => &["sel", "tx", "href", "rx", "ry", "px", "py", "dead", "vw"],
        "scroll" => &["max"],
        "search" => &["q", "n", "source"],
        "exposure" => &["exp", "var"],
        "conversion" => &["type"],
        _ => &[],
    };
    let mut out = serde_json::Map::new();
    for k in allowed {
        match d.get(*k) {
            Some(serde_json::Value::String(s)) => {
                out.insert(k.to_string(), serde_json::Value::String(s.chars().take(if *k == "sel" { 300 } else { 120 }).collect()));
            }
            Some(serde_json::Value::Number(n)) if n.as_f64().map(f64::is_finite).unwrap_or(false) => {
                out.insert(k.to_string(), serde_json::Value::Number(n.clone()));
            }
            Some(serde_json::Value::Bool(b)) => {
                out.insert(k.to_string(), serde_json::Value::Bool(*b));
            }
            _ => {}
        }
    }
    serde_json::Value::Object(out)
}

/// `POST /api/t`: the tracker's batches. Always answers 204 so the page never waits on it.
pub async fn ingest(AxState(state): AxState<State>, ConnectInfo(peer): ConnectInfo<SocketAddr>, headers: HeaderMap, admin: Option<axum::Extension<crate::admin::AdminCtx>>, body: Bytes) -> Response {
    let done = StatusCode::NO_CONTENT.into_response();
    // Browsers send Sec-Fetch-Site with every fetch; a batch without it came from a script, not a page.
    let cross_site = headers.get("sec-fetch-site").and_then(|v| v.to_str().ok()).map(|v| v != "same-origin").unwrap_or(true);
    if admin.is_some() || cross_site || opted_out(&headers) || !state.track_limiter.allow(crate::security::client_ip(&headers, peer.ip())) {
        return done;
    }
    let Some(visitor) = crate::admin::cookie(&headers, VISITOR_COOKIE).and_then(|v| read_visitor(&state.cfg.csrf_secret, &v)) else { return done };
    let Ok(batch) = serde_json::from_slice::<Batch>(&body) else { return done };
    let path: String = batch.p.split('?').next().unwrap_or("").chars().take(200).collect();
    if !path.starts_with('/') {
        return done;
    }
    let device = match batch.d.as_str() {
        "phone" | "tablet" | "desktop" => batch.d.as_str(),
        _ => "",
    };
    let allowed = take_events(&visitor, batch.e.len().min(MAX_EVENTS) as u32) as usize;
    for ev in batch.e.iter().take(allowed) {
        if !matches!(ev.k.as_str(), "view" | "click" | "scroll" | "search" | "exposure" | "conversion") {
            continue;
        }
        let detail = clean_detail(&ev.k, &ev.d);
        if let Err(e) = sqlx::query("INSERT INTO visit_event (visitor, kind, path, device, product_id, detail) VALUES ($1, $2, $3, $4, $5, $6)").bind(&visitor).bind(&ev.k).bind(&path).bind(device).bind(batch.pid).bind(detail).execute(&state.pool).await {
            tracing::warn!("visit_event insert failed: {e:#}");
            break;
        }
    }
    done
}

pub async fn prune(pool: &PgPool) -> anyhow::Result<u64> {
    Ok(sqlx::query("DELETE FROM visit_event WHERE at < now() - make_interval(days => $1)").bind(KEEP_DAYS).execute(pool).await?.rows_affected())
}

// ---- experiments -------------------------------------------------------------------------------

pub struct ExperimentDef {
    pub key: &'static str,
    pub name: &'static str,
    pub description: &'static str,
    /// (variant key, what that visitor sees). The first one is what the site does today.
    pub variants: &'static [(&'static str, &'static str)],
}

/// Every experiment the code knows how to run. Adding one means adding the branch that reads it.
pub const REGISTRY: &[ExperimentDef] = &[
    ExperimentDef {
        key: "hero-stage",
        name: "Homepage pen: 3D model or photograph",
        description: "Half of visitors see the interactive 3D pen at the top of the homepage, half see Nathaniel's photograph of the same pen. Tells you whether the interactive pen helps or gets in the way.",
        variants: &[("3d", "Interactive 3D pen"), ("photo", "His photograph of the pen")],
    },
    ExperimentDef {
        key: "home-popular",
        name: "Homepage: Most Popular section",
        description: "Half of visitors get a Most Popular row (the pens looked at most this month) above New Arrivals, half do not.",
        variants: &[("off", "No Most Popular row"), ("on", "Most Popular row shown")],
    },
    ExperimentDef {
        key: "brand-order",
        name: "Brand filter: by stock or by popularity",
        description: "Half of visitors see brands listed by how many pens are in stock, half by how much each brand is looked at. Pinned brands lead for everyone.",
        variants: &[("stock", "Most pens in stock first"), ("popular", "Most looked-at first")],
    },
];

#[derive(Clone, Debug)]
pub struct ExpState {
    pub status: String,
    pub winner: Option<String>,
    pub variants: Vec<String>,
}

pub struct Assignment {
    pub variant: String,
    /// True while the experiment is collecting data (the page reports the exposure).
    pub running: bool,
}

/// Make sure every registry entry has a row; never touches status or winner of existing rows.
pub async fn sync_registry(pool: &PgPool) -> anyhow::Result<()> {
    for d in REGISTRY {
        let variants: Vec<serde_json::Value> = d.variants.iter().map(|(k, l)| serde_json::json!({"key": k, "label": l})).collect();
        sqlx::query("INSERT INTO experiment (key, name, description, variants) VALUES ($1, $2, $3, $4) ON CONFLICT (key) DO UPDATE SET name = EXCLUDED.name, description = EXCLUDED.description, variants = EXCLUDED.variants")
            .bind(d.key).bind(d.name).bind(d.description).bind(serde_json::Value::Array(variants)).execute(pool).await?;
    }
    Ok(())
}

pub async fn load_states(pool: &PgPool) -> anyhow::Result<HashMap<String, ExpState>> {
    let rows = sqlx::query("SELECT key, status, winner, variants FROM experiment").fetch_all(pool).await?;
    Ok(rows
        .iter()
        .map(|r| {
            let variants: serde_json::Value = r.get("variants");
            let keys = variants.as_array().map(|a| a.iter().filter_map(|v| v.get("key").and_then(|k| k.as_str()).map(str::to_string)).collect()).unwrap_or_default();
            (r.get::<String, _>("key"), ExpState { status: r.get("status"), winner: r.get("winner"), variants: keys })
        })
        .collect())
}

/// Stable bucket for a visitor in one experiment (FNV-1a over both, so experiments are independent).
pub fn bucket(visitor: &str, key: &str, n: usize) -> usize {
    let mut h: u64 = 0xcbf29ce484222325;
    for b in visitor.bytes().chain([b'|']).chain(key.bytes()) {
        h ^= b as u64;
        h = h.wrapping_mul(0x100000001b3);
    }
    if n == 0 { 0 } else { (h % n as u64) as usize }
}

/// What this request should see for `key`: the winner once decided, a stable bucket while running,
/// and `None` when the experiment is idle (the caller falls back to the site's setting).
pub fn variant(state: &AppState, key: &str) -> Option<Assignment> {
    let site = state.site();
    let s = site.experiments.get(key)?;
    if let Some(w) = s.winner.as_ref().filter(|w| s.variants.contains(w)) {
        return Some(Assignment { variant: w.clone(), running: false });
    }
    if s.status != "running" || s.variants.is_empty() {
        return None;
    }
    match current_visitor() {
        Some(v) => Some(Assignment { variant: s.variants[bucket(&v, key, s.variants.len())].clone(), running: true }),
        // Opted-out browsers get today's behaviour and are not counted.
        None => Some(Assignment { variant: s.variants[0].clone(), running: false }),
    }
}

/// `body[data-exp]`: "hero-stage:photo,home-popular:on" for the experiments this page is exposing.
pub fn exposure_attr(pairs: &[(&str, &Option<Assignment>)]) -> String {
    pairs.iter().filter_map(|(k, a)| a.as_ref().filter(|a| a.running).map(|a| format!("{k}:{}", a.variant))).collect::<Vec<_>>().join(",")
}

pub struct ExperimentRow {
    pub id: i32,
    pub key: String,
    pub name: String,
    pub description: String,
    pub status: String,
    pub winner: Option<String>,
    pub variants: Vec<(String, String)>,
    pub started_at: Option<chrono::DateTime<chrono::Utc>>,
}

pub async fn experiments(pool: &PgPool) -> anyhow::Result<Vec<ExperimentRow>> {
    let rows = sqlx::query("SELECT id, key, name, description, status, winner, variants, started_at FROM experiment ORDER BY id").fetch_all(pool).await?;
    Ok(rows
        .iter()
        .map(|r| {
            let v: serde_json::Value = r.get("variants");
            let variants = v.as_array().map(|a| a.iter().map(|x| (x.get("key").and_then(|k| k.as_str()).unwrap_or("").to_string(), x.get("label").and_then(|k| k.as_str()).unwrap_or("").to_string())).collect()).unwrap_or_default();
            ExperimentRow { id: r.get("id"), key: r.get("key"), name: r.get("name"), description: r.get("description"), status: r.get("status"), winner: r.get("winner"), variants, started_at: r.get("started_at") }
        })
        .collect())
}

pub async fn set_experiment_status(pool: &PgPool, key: &str, status: &str, winner: Option<&str>) -> anyhow::Result<()> {
    match status {
        "running" => sqlx::query("UPDATE experiment SET status = 'running', winner = NULL, started_at = now(), stopped_at = NULL WHERE key = $1").bind(key).execute(pool).await?,
        "stopped" => sqlx::query("UPDATE experiment SET status = 'stopped', winner = $2, stopped_at = now() WHERE key = $1").bind(key).bind(winner).execute(pool).await?,
        _ => sqlx::query("UPDATE experiment SET status = 'draft', winner = NULL, started_at = NULL, stopped_at = NULL WHERE key = $1").bind(key).execute(pool).await?,
    };
    Ok(())
}

pub struct VariantResult {
    pub variant: String,
    pub visitors: i64,
    pub pen_views: i64,
    pub converters: i64,
}

/// Per variant since the experiment started: visitors exposed, pen pages they opened afterwards,
/// and how many of them went on to press Buy, call, e-mail or send a form.
pub async fn experiment_results(pool: &PgPool, key: &str, since: Option<chrono::DateTime<chrono::Utc>>) -> anyhow::Result<Vec<VariantResult>> {
    let since = since.unwrap_or_else(|| chrono::Utc::now() - chrono::Duration::days(3650));
    let rows = sqlx::query(
        "WITH exposed AS (
            SELECT visitor, min(detail->>'var') AS variant, min(at) AS first_at
            FROM visit_event WHERE kind = 'exposure' AND detail->>'exp' = $1 AND at >= $2 GROUP BY visitor
         )
         SELECT x.variant,
                count(*) AS visitors,
                COALESCE(sum((SELECT count(*) FROM visit_event v WHERE v.visitor = x.visitor AND v.kind = 'view' AND v.product_id IS NOT NULL AND v.at >= x.first_at)), 0)::bigint AS pen_views,
                count(*) FILTER (WHERE EXISTS (SELECT 1 FROM visit_event c WHERE c.visitor = x.visitor AND c.kind = 'conversion' AND c.at >= x.first_at)) AS converters
         FROM exposed x GROUP BY x.variant ORDER BY x.variant",
    )
    .bind(key)
    .bind(since)
    .fetch_all(pool)
    .await?;
    Ok(rows.iter().map(|r| VariantResult { variant: r.get::<Option<String>, _>("variant").unwrap_or_default(), visitors: r.get("visitors"), pen_views: r.get("pen_views"), converters: r.get("converters") }).collect())
}

// ---- what the site itself reads back ------------------------------------------------------------

/// Live pens ranked by how many different visitors opened them in the window.
pub async fn popular_product_ids(pool: &PgPool, days: i32, limit: i64, min_visitors: i64) -> anyhow::Result<Vec<i32>> {
    let rows = sqlx::query(
        "SELECT v.product_id AS id FROM visit_event v JOIN product p ON p.id = v.product_id AND p.status = 'live'
         WHERE v.kind = 'view' AND v.product_id IS NOT NULL AND v.at > now() - make_interval(days => $1)
         GROUP BY v.product_id HAVING count(DISTINCT v.visitor) >= $3 ORDER BY count(DISTINCT v.visitor) DESC, max(v.at) DESC LIMIT $2",
    )
    .bind(days)
    .bind(limit)
    .bind(min_visitors)
    .fetch_all(pool)
    .await?;
    Ok(rows.iter().map(|r| r.get("id")).collect())
}

/// Interest per brand slug: pen pages opened plus brand-page visits in the window.
pub async fn brand_interest(pool: &PgPool, days: i32) -> anyhow::Result<HashMap<String, i64>> {
    let rows = sqlx::query(
        "SELECT slug, sum(n)::bigint AS n FROM (
            SELECT b.slug, count(DISTINCT v.visitor) AS n FROM visit_event v JOIN product p ON p.id = v.product_id JOIN brand b ON b.id = p.brand_id
              WHERE v.kind = 'view' AND v.at > now() - make_interval(days => $1) GROUP BY b.slug
            UNION ALL
            SELECT b.slug, count(DISTINCT v.visitor) AS n FROM visit_event v JOIN brand b ON v.path = '/brand/' || b.slug || '/'
              WHERE v.kind = 'view' AND v.at > now() - make_interval(days => $1) GROUP BY b.slug
            UNION ALL
            SELECT b.slug, count(DISTINCT v.visitor) AS n FROM visit_event v JOIN brand b ON lower(v.detail->>'q') LIKE '%' || lower(b.name) || '%'
              WHERE v.kind = 'search' AND v.at > now() - make_interval(days => $1) GROUP BY b.slug
         ) t GROUP BY slug",
    )
    .bind(days)
    .fetch_all(pool)
    .await?;
    Ok(rows.iter().map(|r| (r.get::<String, _>("slug"), r.get::<i64, _>("n"))).collect())
}

/// Searches worth suggesting under the search box: typed by several different visitors, found
/// something, and made only of letters, digits and spaces (nothing a visitor typed is shown raw).
pub async fn popular_searches(pool: &PgPool, days: i32, limit: i64) -> anyhow::Result<Vec<String>> {
    let rows = sqlx::query(
        "SELECT lower(trim(detail->>'q')) AS q FROM visit_event
         WHERE kind = 'search' AND COALESCE(detail->>'source', 'shop') = 'shop' AND at > now() - make_interval(days => $1) AND COALESCE((detail->>'n')::numeric, 0) > 0
           AND detail->>'q' ~ '^[A-Za-z0-9 ]{3,30}$'
         GROUP BY 1 HAVING count(DISTINCT visitor) >= 3 ORDER BY count(DISTINCT visitor) DESC, 1 LIMIT $2",
    )
    .bind(days)
    .bind(limit)
    .fetch_all(pool)
    .await?;
    Ok(rows.iter().map(|r| r.get("q")).collect())
}

// ---- the admin's Insights page --------------------------------------------------------------------

#[derive(Default)]
pub struct Totals {
    pub visitors: i64,
    pub page_views: i64,
    pub pen_views: i64,
    pub searches: i64,
    pub conversions: i64,
}

pub async fn totals(pool: &PgPool, days: i32) -> anyhow::Result<Totals> {
    let r = sqlx::query(
        "SELECT count(DISTINCT visitor) AS visitors,
                count(*) FILTER (WHERE kind = 'view') AS page_views,
                count(*) FILTER (WHERE kind = 'view' AND product_id IS NOT NULL) AS pen_views,
                count(*) FILTER (WHERE kind = 'search') AS searches,
                count(*) FILTER (WHERE kind = 'conversion') AS conversions
         FROM visit_event WHERE at > now() - make_interval(days => $1)",
    )
    .bind(days)
    .fetch_one(pool)
    .await?;
    Ok(Totals { visitors: r.get("visitors"), page_views: r.get("page_views"), pen_views: r.get("pen_views"), searches: r.get("searches"), conversions: r.get("conversions") })
}

pub struct Counted {
    pub label: String,
    pub url: String,
    pub n: i64,
    pub extra: String,
}

pub async fn top_pages(pool: &PgPool, days: i32, limit: i64) -> anyhow::Result<Vec<Counted>> {
    let rows = sqlx::query(
        "SELECT v.path, count(DISTINCT v.visitor) AS n,
                (SELECT round(avg((s.detail->>'max')::numeric)) FROM visit_event s WHERE s.kind = 'scroll' AND s.path = v.path AND s.at > now() - make_interval(days => $1)) AS depth
         FROM visit_event v WHERE v.kind = 'view' AND v.at > now() - make_interval(days => $1) GROUP BY v.path ORDER BY n DESC LIMIT $2",
    )
    .bind(days)
    .bind(limit)
    .fetch_all(pool)
    .await?;
    Ok(rows
        .iter()
        .map(|r| {
            let path: String = r.get("path");
            let depth: Option<sqlx::types::BigDecimal> = r.get("depth");
            Counted { label: path.clone(), url: path, n: r.get("n"), extra: depth.map(|d| format!("{d}% scrolled")).unwrap_or_default() }
        })
        .collect())
}

pub async fn top_pens(pool: &PgPool, days: i32, limit: i64) -> anyhow::Result<Vec<Counted>> {
    let rows = sqlx::query(
        "SELECT p.short_title, p.slug, p.status, count(DISTINCT v.visitor) AS n FROM visit_event v JOIN product p ON p.id = v.product_id
         WHERE v.kind = 'view' AND v.at > now() - make_interval(days => $1) GROUP BY p.id ORDER BY n DESC LIMIT $2",
    )
    .bind(days)
    .bind(limit)
    .fetch_all(pool)
    .await?;
    Ok(rows.iter().map(|r| Counted { label: r.get("short_title"), url: format!("/product/{}/", r.get::<String, _>("slug")), n: r.get("n"), extra: r.get("status") }).collect())
}

/// `source`: "shop" (the search box) or "guide" (questions typed to Ask the Pen Market).
/// `empty_only`: searches that found nothing, which is demand he cannot fill today.
pub async fn top_searches(pool: &PgPool, days: i32, limit: i64, source: &str, empty_only: bool) -> anyhow::Result<Vec<Counted>> {
    let rows = sqlx::query(
        "SELECT lower(trim(detail->>'q')) AS q, count(DISTINCT visitor) AS n, round(avg(COALESCE((detail->>'n')::numeric, 0))) AS found
         FROM visit_event WHERE kind = 'search' AND COALESCE(detail->>'source', 'shop') = $3 AND at > now() - make_interval(days => $1) AND trim(COALESCE(detail->>'q', '')) <> ''
         GROUP BY 1 HAVING ($4 = FALSE OR max(COALESCE((detail->>'n')::numeric, 0)) = 0) ORDER BY n DESC, 1 LIMIT $2",
    )
    .bind(days)
    .bind(limit)
    .bind(source)
    .bind(empty_only)
    .fetch_all(pool)
    .await?;
    Ok(rows
        .iter()
        .map(|r| {
            let q: String = r.get("q");
            let found: Option<sqlx::types::BigDecimal> = r.get("found");
            Counted { url: format!("/shop/?q={}", urlencoding::encode(&q)), label: q, n: r.get("n"), extra: if source == "shop" { format!("{} found", found.map(|f| f.to_string()).unwrap_or_else(|| "0".into())) } else { String::new() } }
        })
        .collect())
}

pub async fn devices(pool: &PgPool, days: i32) -> anyhow::Result<Vec<Counted>> {
    let rows = sqlx::query("SELECT device, count(DISTINCT visitor) AS n FROM visit_event WHERE kind = 'view' AND device <> '' AND at > now() - make_interval(days => $1) GROUP BY device ORDER BY n DESC").bind(days).fetch_all(pool).await?;
    Ok(rows.iter().map(|r| Counted { label: r.get("device"), url: String::new(), n: r.get("n"), extra: String::new() }).collect())
}

/// Clicks on things that are not links or buttons: where visitors expected something to happen.
pub async fn dead_clicks(pool: &PgPool, days: i32, limit: i64) -> anyhow::Result<Vec<Counted>> {
    let rows = sqlx::query(
        "SELECT path, COALESCE(NULLIF(detail->>'tx', ''), detail->>'sel') AS what, count(*) AS n FROM visit_event
         WHERE kind = 'click' AND (detail->>'dead')::boolean IS TRUE AND at > now() - make_interval(days => $1)
         GROUP BY 1, 2 HAVING count(*) >= 2 ORDER BY n DESC LIMIT $2",
    )
    .bind(days)
    .bind(limit)
    .fetch_all(pool)
    .await?;
    Ok(rows.iter().map(|r| Counted { label: r.get::<Option<String>, _>("what").unwrap_or_default(), url: r.get("path"), n: r.get("n"), extra: String::new() }).collect())
}

pub async fn conversions(pool: &PgPool, days: i32) -> anyhow::Result<Vec<Counted>> {
    let rows = sqlx::query("SELECT COALESCE(detail->>'type', '') AS t, count(*) AS n FROM visit_event WHERE kind = 'conversion' AND at > now() - make_interval(days => $1) GROUP BY 1 ORDER BY n DESC").bind(days).fetch_all(pool).await?;
    Ok(rows.iter().map(|r| Counted { label: r.get("t"), url: String::new(), n: r.get("n"), extra: String::new() }).collect())
}

/// Everything the on-page heat map draws for one path.
pub async fn heatmap(pool: &PgPool, path: &str, device: &str, days: i32) -> anyhow::Result<serde_json::Value> {
    let dev = if matches!(device, "phone" | "tablet" | "desktop") { device } else { "" };
    let clicks = sqlx::query(
        "SELECT detail->>'sel' AS sel, detail->>'tx' AS tx, (detail->>'rx')::float8 AS rx, (detail->>'ry')::float8 AS ry, (detail->>'px')::float8 AS px, (detail->>'py')::float8 AS py, COALESCE((detail->>'dead')::boolean, FALSE) AS dead
         FROM visit_event WHERE kind = 'click' AND path = $1 AND ($2 = '' OR device = $2) AND at > now() - make_interval(days => $3) ORDER BY at DESC LIMIT 4000",
    )
    .bind(path)
    .bind(dev)
    .bind(days)
    .fetch_all(pool)
    .await?;
    let scroll = sqlx::query("SELECT (detail->>'max')::float8 AS max FROM visit_event WHERE kind = 'scroll' AND path = $1 AND ($2 = '' OR device = $2) AND at > now() - make_interval(days => $3) LIMIT 4000").bind(path).bind(dev).bind(days).fetch_all(pool).await?;
    let views: i64 = sqlx::query("SELECT count(DISTINCT visitor) AS n FROM visit_event WHERE kind = 'view' AND path = $1 AND ($2 = '' OR device = $2) AND at > now() - make_interval(days => $3)").bind(path).bind(dev).bind(days).fetch_one(pool).await?.get("n");
    let depths: Vec<f64> = scroll.iter().filter_map(|r| r.get::<Option<f64>, _>("max")).collect();
    // Share of visitors who reached each quarter of the page.
    let reach: Vec<serde_json::Value> = [25.0, 50.0, 75.0, 100.0].iter().map(|q| serde_json::json!({"pct": q, "share": if depths.is_empty() { 0.0 } else { depths.iter().filter(|d| **d >= *q - 0.5).count() as f64 / depths.len() as f64 }})).collect();
    Ok(serde_json::json!({
        "views": views,
        "clicks": clicks.iter().map(|r| serde_json::json!({"sel": r.get::<Option<String>, _>("sel"), "tx": r.get::<Option<String>, _>("tx"), "rx": r.get::<Option<f64>, _>("rx"), "ry": r.get::<Option<f64>, _>("ry"), "px": r.get::<Option<f64>, _>("px"), "py": r.get::<Option<f64>, _>("py"), "dead": r.get::<bool, _>("dead")})).collect::<Vec<_>>(),
        "reach": reach,
    }))
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn buckets_are_stable_and_spread() {
        assert_eq!(bucket("abc", "hero-stage", 2), bucket("abc", "hero-stage", 2));
        let mut counts = [0usize; 2];
        for i in 0..2000 {
            counts[bucket(&format!("{i:024x}"), "hero-stage", 2)] += 1;
        }
        assert!(counts[0] > 800 && counts[1] > 800, "{counts:?}");
        assert_eq!(bucket("abc", "x", 0), 0);
    }
    #[test]
    fn detail_is_whitelisted() {
        let d: serde_json::Map<String, serde_json::Value> = serde_json::from_str(r#"{"q":"parker 51","n":4,"evil":"<script>","source":"shop"}"#).unwrap_or_default();
        let c = clean_detail("search", &d);
        assert_eq!(c, serde_json::json!({"q": "parker 51", "n": 4, "source": "shop"}));
        assert_eq!(clean_detail("nope", &d), serde_json::json!({}));
    }
    #[test]
    fn exposure_attr_only_lists_running() {
        let a = Some(Assignment { variant: "photo".into(), running: true });
        let b = Some(Assignment { variant: "on".into(), running: false });
        assert_eq!(exposure_attr(&[("hero-stage", &a), ("home-popular", &b), ("x", &None)]), "hero-stage:photo");
    }
}
