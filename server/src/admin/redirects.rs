//! Old addresses. Every URL the old site had keeps working: the router serves the old shapes and the
//! redirect table 301s the rest. Here Nathaniel sees that table, downloads it, and pastes the lists he
//! keeps (the really old site's URLs): each line is checked against the new site, matched to a page
//! by its name when he gives no destination, and added as a 301. Nothing he adds is guessed silently:
//! the result says, line by line, what happened.

use super::{store, Auth};
use crate::app::{html, AppError, AppResult, PageMeta, State};
use crate::security::{self, CsrfCookie};
use askama::Template;
use axum::extract::{Form, Path, Query, State as AxState};
use axum::http::header;
use axum::response::{IntoResponse, Redirect};
use axum::Extension;
use serde::Deserialize;
use sqlx::{FromRow, Row};
use std::time::Duration;

/// One paste is checked line by line against the running site; this keeps a paste to a few seconds.
const MAX_LINES: usize = 2000;
const LIST_LIMIT: i64 = 300;
/// Hosts whose full URLs are this site's own addresses (old or new).
const OWN_HOSTS: [&str; 2] = ["thepenmarket.com", "www.thepenmarket.com"];

fn meta(state: &State, cookie: &CsrfCookie) -> PageMeta {
    let mut page = PageMeta::new(state, "Old addresses | ThePenMarket.com admin", "Admin area for ThePenMarket.com.", "/admin/redirects/");
    page.noindex = true;
    page.section = "admin".into();
    page.csrf = security::csrf_token(&state.cfg.csrf_secret, &cookie.0);
    page
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

#[derive(FromRow)]
pub struct RedirectRow {
    pub id: i32,
    pub old_path: String,
    pub new_path: String,
    pub hits: i64,
    pub reason: String,
    pub added_by: String,
}

/// What happened to one pasted line.
pub struct Outcome {
    pub old: String,
    pub to: String,
    pub kind: &'static str,
    pub note: String,
}

impl Outcome {
    pub fn css(&self) -> &'static str {
        match self.kind {
            "added" | "updated" => "ok",
            "works" => "same",
            _ => "todo",
        }
    }
}

#[derive(Template)]
#[template(path = "admin/redirects.html")]
pub struct RedirectsTpl {
    pub page: PageMeta,
    pub total: i64,
    pub his: i64,
    pub rows: Vec<RedirectRow>,
    pub q: String,
    pub mine: bool,
    pub shown: usize,
    pub report: Vec<Outcome>,
    pub added: usize,
    /// Lines that still need a destination, ready to fill in and paste back.
    pub retry: String,
    pub deleted: bool,
}

#[derive(Deserialize, Default)]
pub struct ListQuery {
    #[serde(default)]
    pub q: String,
    #[serde(default)]
    pub show: String,
    #[serde(default)]
    pub deleted: String,
}

async fn render(state: &State, cookie: &CsrfCookie, q: &str, mine: bool, report: Vec<Outcome>, deleted: bool) -> AppResult {
    let counts = sqlx::query("SELECT count(*) AS total, count(*) FILTER (WHERE added_by = 'admin') AS his FROM redirect").fetch_one(&state.pool).await?;
    let like = format!("%{}%", crate::text::like_escape(q.trim()));
    let rows: Vec<RedirectRow> = sqlx::query_as(
        "SELECT id, old_path, new_path, hits, reason, added_by FROM redirect
         WHERE ($1 = '%%' OR old_path ILIKE $1 OR new_path ILIKE $1) AND (NOT $2 OR added_by = 'admin')
         ORDER BY (added_by = 'admin') DESC, created_at DESC, hits DESC, old_path LIMIT $3",
    )
    .bind(&like)
    .bind(mine)
    .bind(LIST_LIMIT)
    .fetch_all(&state.pool)
    .await?;
    let added = report.iter().filter(|o| o.kind == "added" || o.kind == "updated").count();
    let retry = report.iter().filter(|o| o.kind == "nomatch").map(|o| format!("{}  {}", o.old, o.to)).collect::<Vec<_>>().join("\n");
    html(&RedirectsTpl { page: meta(state, cookie), total: counts.get("total"), his: counts.get("his"), shown: rows.len(), rows, q: q.trim().to_string(), mine, report, added, retry, deleted })
}

pub async fn page(AxState(state): AxState<State>, Extension(cookie): Extension<CsrfCookie>, Auth(_admin): Auth, Query(q): Query<ListQuery>) -> AppResult {
    render(&state, &cookie, &q.q, q.show == "mine", vec![], q.deleted == "1").await
}

#[derive(Deserialize)]
pub struct AddForm {
    pub csrf: String,
    #[serde(default)]
    pub lines: String,
}

pub async fn add(AxState(state): AxState<State>, Extension(cookie): Extension<CsrfCookie>, Auth(admin): Auth, Form(form): Form<AddForm>) -> AppResult {
    check(&state, &cookie, &admin, &form.csrf)?;
    let probe = Probe::new(&state)?;
    let mut report = vec![];
    for line in form.lines.lines().map(str::trim).filter(|l| !l.is_empty() && !l.starts_with('#')).take(MAX_LINES) {
        report.push(add_line(&state, &probe, line).await?);
    }
    let added = report.iter().filter(|o| o.kind == "added" || o.kind == "updated").count();
    store::audit(&state.pool, "admin.redirects_added", serde_json::json!({"email": admin.email, "ip": admin.ip, "lines": report.len(), "added": added})).await?;
    render(&state, &cookie, "", added > 0, report, false).await
}

#[derive(Deserialize)]
pub struct CsrfForm {
    pub csrf: String,
}

/// Only addresses from his own lists can be removed; the import's keep old URLs from ever 404ing.
pub async fn delete(AxState(state): AxState<State>, Extension(cookie): Extension<CsrfCookie>, Auth(admin): Auth, Path(id): Path<i32>, Form(form): Form<CsrfForm>) -> AppResult {
    check(&state, &cookie, &admin, &form.csrf)?;
    let row = sqlx::query("DELETE FROM redirect WHERE id = $1 AND added_by = 'admin' RETURNING old_path, new_path").bind(id).fetch_optional(&state.pool).await?.ok_or(AppError::NotFound)?;
    store::audit(&state.pool, "admin.redirect_deleted", serde_json::json!({"email": admin.email, "ip": admin.ip, "old": row.get::<String, _>("old_path"), "new": row.get::<String, _>("new_path")})).await?;
    Ok(Redirect::to("/admin/redirects/?deleted=1").into_response())
}

/// `GET /admin/redirects.csv`: every old address and where it goes now.
pub async fn csv(AxState(state): AxState<State>, Auth(_admin): Auth) -> AppResult {
    let rows: Vec<RedirectRow> = sqlx::query_as("SELECT id, old_path, new_path, hits, reason, added_by FROM redirect ORDER BY old_path").fetch_all(&state.pool).await?;
    let mut out = String::from("old_address,goes_to,visits_since_launch,added_by,why\n");
    for r in rows {
        out.push_str(&[cell(&r.old_path), cell(&r.new_path), r.hits.to_string(), cell(if r.added_by == "admin" { "you" } else { "import" }), cell(&r.reason)].join(","));
        out.push('\n');
    }
    Ok(([(header::CONTENT_TYPE, "text/csv; charset=utf-8"), (header::CONTENT_DISPOSITION, "attachment; filename=\"thepenmarket-old-addresses.csv\"")], out).into_response())
}

fn cell(s: &str) -> String {
    // A leading = + - @ would be read as a formula by a spreadsheet.
    let s = if s.starts_with(['=', '+', '-', '@']) { format!("'{s}") } else { s.to_string() };
    if s.contains([',', '"', '\n']) {
        format!("\"{}\"", s.replace('"', "\"\""))
    } else {
        s
    }
}

// ---- one pasted line ---------------------------------------------------------------------------------

/// "old", "old new", "old -> new", "old, new" (a spreadsheet's two columns pasted in work too).
pub fn split_line(line: &str) -> (String, Option<String>) {
    let cleaned = line.replace("->", " ").replace('→', " ").replace(['\t', ','], " ");
    let mut parts = cleaned.split_whitespace();
    let old = parts.next().unwrap_or("").to_string();
    (old, parts.next().map(str::to_string))
}

/// A pasted address as the path (and query) the site sees: the host is dropped, a fragment too.
/// `None` for something that is not an address, or for the site's own working areas.
pub fn normalize(raw: &str) -> Option<String> {
    let raw = raw.trim().trim_matches(|c| matches!(c, '<' | '>' | '"' | '\'')).trim();
    if raw.is_empty() || raw.len() > 500 {
        return None;
    }
    let with_scheme = if raw.starts_with("//") {
        format!("http:{raw}")
    } else if raw.starts_with("http://") || raw.starts_with("https://") {
        raw.to_string()
    } else if !raw.starts_with('/') && raw.split('/').next().map(|h| h.contains('.') && !h.contains('?')).unwrap_or(false) && raw.contains('/') {
        format!("http://{raw}")
    } else {
        format!("http://x{}{}", if raw.starts_with('/') { "" } else { "/" }, raw)
    };
    let url = reqwest::Url::parse(&with_scheme).ok()?;
    let mut out = url.path().to_string();
    if let Some(q) = url.query().filter(|q| !q.is_empty()) {
        out.push('?');
        out.push_str(q);
    }
    let blocked = ["/admin", "/static/", "/media/", "/api/"];
    // "//host" (what "/\\host" parses to) is another site to a browser, never a path here.
    if out == "/" || out.starts_with("//") || out.contains('\\') || blocked.iter().any(|b| out.starts_with(b)) {
        return None;
    }
    Some(out)
}

/// A destination must be on this site: a path, or a full URL on his domain or this server's origin.
fn normalize_target(raw: &str, site_origin: &str) -> Option<String> {
    let raw = raw.trim();
    if raw.contains('\\') {
        return None;
    }
    if raw.starts_with("http://") || raw.starts_with("https://") || raw.starts_with("//") {
        let full = if raw.starts_with("//") { format!("http:{raw}") } else { raw.to_string() };
        let url = reqwest::Url::parse(&full).ok()?;
        let host = url.host_str().unwrap_or("").to_lowercase();
        let own_origin = reqwest::Url::parse(site_origin).ok().and_then(|u| u.host_str().map(str::to_lowercase)).unwrap_or_default();
        if !OWN_HOSTS.contains(&host.as_str()) && host != own_origin {
            return None;
        }
    }
    let p = normalize(raw).or_else(|| (raw == "/").then(|| "/".to_string()))?;
    Some(p)
}

fn same_address(a: &str, b: &str) -> bool {
    a.trim_end_matches('/') == b.trim_end_matches('/')
}

/// The old address's last part as a slug: "/store/Parker-51-Vacumatic.html" -> "parker-51-vacumatic".
pub fn last_slug(path: &str) -> String {
    let p = path.split('?').next().unwrap_or("");
    let last = p.trim_end_matches('/').rsplit('/').next().unwrap_or("");
    let stem = [".html", ".htm", ".php", ".aspx", ".asp", ".shtml"].iter().find_map(|ext| last.strip_suffix(ext)).unwrap_or(last);
    crate::text::slugify(&stem.replace(['_', '+'], "-"))
}

/// Find a page on the new site with the old address's name.
async fn match_by_name(state: &State, path: &str) -> anyhow::Result<Option<(String, String)>> {
    let slug = last_slug(path);
    if slug.is_empty() || slug == "index" {
        return Ok(None);
    }
    let pool = &state.pool;
    let found: Option<String> = sqlx::query_scalar(
        "SELECT url FROM (
            SELECT 1 AS rank, '/product/' || slug || '/' AS url FROM product WHERE slug = $1 OR slug = 'vintage-pens-' || $1 OR slug = 'pre-owned-pens-' || $1
            UNION ALL SELECT 2, '/' || slug || '/' FROM post WHERE slug = $1 AND status = 'published'
            UNION ALL SELECT 3, '/' || slug || '/' FROM page WHERE slug = $1 AND status = 'published' AND custom
            UNION ALL SELECT 4, '/brand/' || slug || '/' FROM brand WHERE slug = $1
            UNION ALL SELECT 5, '/product-category/' || slug || '/' FROM category WHERE slug = $1 AND slug <> 'memberships'
            UNION ALL SELECT 6, '/glossary/' || slug || '/' FROM glossary_term WHERE slug = $1 AND status = 'published'
            UNION ALL SELECT 7, '/trading-post/' || slug || '/' FROM tp_listing WHERE slug = $1 AND status = 'live'
         ) m ORDER BY rank LIMIT 1",
    )
    .bind(&slug)
    .fetch_optional(pool)
    .await?;
    Ok(found.map(|u| (u, format!("matched by name (“{slug}”)"))))
}

/// Asks the running site what it answers for an address, without following redirects.
pub struct Probe {
    client: reqwest::Client,
    base: String,
}

impl Probe {
    fn new(state: &State) -> anyhow::Result<Self> {
        let host = if state.cfg.host == "0.0.0.0" || state.cfg.host.is_empty() { "127.0.0.1".to_string() } else { state.cfg.host.clone() };
        let client = reqwest::Client::builder().redirect(reqwest::redirect::Policy::none()).timeout(Duration::from_secs(10)).build()?;
        Ok(Probe { client, base: format!("http://{host}:{}", state.cfg.port) })
    }

    /// Where an address ends up after at most four hops, and the final status.
    async fn land(&self, path: &str) -> (u16, String) {
        let mut at = path.to_string();
        for _ in 0..4 {
            let Ok(res) = self.client.get(format!("{}{}", self.base, at)).send().await else { return (0, at) };
            let code = res.status().as_u16();
            match res.headers().get(header::LOCATION).and_then(|v| v.to_str().ok()) {
                Some(loc) if (300..400).contains(&code) => {
                    at = match reqwest::Url::parse(loc) {
                        Ok(u) => format!("{}{}", u.path(), u.query().map(|q| format!("?{q}")).unwrap_or_default()),
                        Err(_) => loc.to_string(),
                    };
                }
                _ => return (code, at),
            }
        }
        (310, at)
    }
}

async fn existing(state: &State, old: &str) -> anyhow::Result<Option<String>> {
    let alt = match old.split_once('?') {
        Some((p, q)) => format!("{}?{q}", if p.ends_with('/') { p.trim_end_matches('/').to_string() } else { format!("{p}/") }),
        None => if old.ends_with('/') { old.trim_end_matches('/').to_string() } else { format!("{old}/") },
    };
    Ok(sqlx::query_scalar("SELECT new_path FROM redirect WHERE old_path = $1 OR old_path = $2 LIMIT 1").bind(old).bind(alt).fetch_optional(&state.pool).await?)
}

async fn save(state: &State, old: &str, new: &str, why: &str) -> anyhow::Result<()> {
    sqlx::query("INSERT INTO redirect (old_path, new_path, status, reason, added_by) VALUES ($1, $2, 301, $3, 'admin') ON CONFLICT (old_path) DO UPDATE SET new_path = EXCLUDED.new_path, reason = EXCLUDED.reason, added_by = 'admin'")
        .bind(old)
        .bind(new)
        .bind(why)
        .execute(&state.pool)
        .await?;
    Ok(())
}

async fn add_line(state: &State, probe: &Probe, line: &str) -> anyhow::Result<Outcome> {
    let (old_raw, new_raw) = split_line(line);
    let mut out = Outcome { old: old_raw.clone(), to: String::new(), kind: "invalid", note: String::new() };
    let Some(old) = normalize(&old_raw) else {
        out.note = "Not an old address this page can redirect (the home page and the admin are left alone).".into();
        return Ok(out);
    };
    out.old = old.clone();
    let target = match &new_raw {
        Some(n) => match normalize_target(n, &state.cfg.site_origin) {
            Some(t) => Some(t),
            None => {
                out.note = format!("“{n}” is not an address on this site. Use a path like /brand/sheaffer/.");
                return Ok(out);
            }
        },
        None => None,
    };
    if let Some(t) = &target {
        if same_address(t, &old) {
            out.note = "The old and new addresses are the same.".into();
            return Ok(out);
        }
    }
    let before = existing(state, &old).await?;
    if before.is_none() {
        // Already a page (or already redirected by the router's own rules): nothing to add.
        let (code, landed) = probe.land(&old).await;
        if code == 200 {
            out.kind = "works";
            out.to = landed.clone();
            out.note = if same_address(&landed, &old) { "Already a page on the new site.".into() } else { "Already opens a page on the new site.".into() };
            return Ok(out);
        }
    }
    let (dest, why) = match (target, &before) {
        (Some(t), _) => (t, "from your list".to_string()),
        (None, Some(b)) => {
            out.kind = "works";
            out.to = b.clone();
            out.note = "Already redirects.".into();
            return Ok(out);
        }
        (None, None) => match match_by_name(state, &old).await? {
            Some((u, why)) => (u, why),
            None => {
                out.kind = "nomatch";
                let words = last_slug(&old).replace('-', "+");
                out.to = if words.is_empty() { String::new() } else { format!("/shop/?q={words}") };
                out.note = "No page with this name on the new site. Add where it should go, or keep the shop search suggested here.".into();
                return Ok(out);
            }
        },
    };
    let (code, _) = probe.land(&dest).await;
    if code != 200 {
        out.kind = "invalid";
        out.to = dest.clone();
        out.note = format!("{dest} does not open a page on the new site{}. Nothing was added.", if code == 0 { String::new() } else { format!(" (it answers {code})") });
        return Ok(out);
    }
    save(state, &old, &dest, &why).await?;
    out.kind = if before.is_some() { "updated" } else { "added" };
    out.note = match &before {
        Some(b) if !same_address(b, &dest) => format!("Changed: it went to {b}."),
        Some(_) => "Already went there; now marked as yours.".into(),
        None => why,
    };
    out.to = dest;
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn never_another_site() {
        assert_eq!(normalize("/\\evil.com/x"), None);
        assert_eq!(normalize_target("/\\evil.com", "https://thepenmarket.com"), None);
        assert_eq!(normalize_target("https://evil.com/x", "https://thepenmarket.com"), None);
        assert_eq!(normalize_target("//evil.com/x", "https://thepenmarket.com"), None);
        assert_eq!(normalize_target("https://thepenmarket.com/shop/", "https://thepenmarket.com").as_deref(), Some("/shop/"));
    }

    #[test]
    fn pasted_addresses() {
        assert_eq!(normalize("https://www.thepenmarket.com/product/vintage-pens-montblanc-644/").as_deref(), Some("/product/vintage-pens-montblanc-644/"));
        assert_eq!(normalize("thepenmarket.com/store/index.php?main_page=product_info&products_id=12#top").as_deref(), Some("/store/index.php?main_page=product_info&products_id=12"));
        assert_eq!(normalize("/old-page.html").as_deref(), Some("/old-page.html"));
        assert_eq!(normalize("old-page").as_deref(), Some("/old-page"));
        assert_eq!(normalize("https://thepenmarket.com/"), None);
        assert_eq!(normalize("/admin/settings/"), None);
        assert_eq!(normalize(""), None);
    }

    #[test]
    fn lines_with_and_without_a_destination() {
        assert_eq!(split_line("/a.html"), ("/a.html".into(), None));
        assert_eq!(split_line("/a.html -> /brand/sheaffer/"), ("/a.html".into(), Some("/brand/sheaffer/".into())));
        assert_eq!(split_line("/a.html\t/b/"), ("/a.html".into(), Some("/b/".into())));
        assert_eq!(split_line("/a.html, /b/"), ("/a.html".into(), Some("/b/".into())));
    }

    #[test]
    fn destinations_stay_on_the_site() {
        assert_eq!(normalize_target("https://thepenmarket.com/brand/sheaffer/", "http://127.0.0.1:8140").as_deref(), Some("/brand/sheaffer/"));
        assert_eq!(normalize_target("/shop/?q=parker+51", "http://127.0.0.1:8140").as_deref(), Some("/shop/?q=parker+51"));
        assert_eq!(normalize_target("https://penmarket.crossgen-ai.com/faq/", "https://penmarket.crossgen-ai.com").as_deref(), Some("/faq/"));
        assert_eq!(normalize_target("https://evil.example/x", "http://127.0.0.1:8140"), None);
        assert_eq!(normalize_target("/", "http://127.0.0.1:8140").as_deref(), Some("/"));
    }

    #[test]
    fn names_from_old_addresses() {
        assert_eq!(last_slug("/store/Parker_51_Vacumatic.html"), "parker-51-vacumatic");
        assert_eq!(last_slug("/product/vintage-pens-montblanc-644/"), "vintage-pens-montblanc-644");
        assert_eq!(last_slug("/index.php?x=1"), "index");
    }

    #[test]
    fn csv_cells() {
        assert_eq!(cell("/a,b"), "\"/a,b\"");
        assert_eq!(cell("=HYPERLINK(1)"), "'=HYPERLINK(1)");
        assert_eq!(cell("/plain/"), "/plain/");
    }
}
