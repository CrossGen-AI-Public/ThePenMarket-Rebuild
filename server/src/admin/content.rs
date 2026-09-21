//! Writing in the admin: blog posts (the migrated ones and new ones), custom pages (grading system,
//! glossary, anything else he wants a permanent page for) and the site-wide FAQs. The editor sends
//! HTML; `clean_html` is the only way it reaches the database. SQL for these three lives here.

use super::handlers::{api_csrf_ok, api_err};
use super::{store, Auth, AuthApi};
use crate::app::{html, AppError, AppResult, PageMeta, State};
use crate::security::{self, CsrfCookie};
use askama::Template;
use axum::extract::{Form, Multipart, Path, Query, State as AxState};
use axum::http::{HeaderMap, StatusCode};
use axum::response::{IntoResponse, Redirect};
use axum::{Extension, Json};
use serde::Deserialize;
use sqlx::{PgPool, Row};

fn meta(state: &State, title: &str, path: &str, cookie: &CsrfCookie) -> PageMeta {
    let mut page = PageMeta::new(state, &format!("{title} | ThePenMarket.com admin"), "Admin area for ThePenMarket.com.", path);
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

/// The only HTML that reaches a post or a page: text structure, links, images, YouTube embeds.
/// Scripts, styles, event handlers and any other iframe are dropped.
pub fn clean_html(input: &str) -> String {
    let mut b = ammonia::Builder::default();
    b.add_tags(["figure", "figcaption", "iframe"])
        .add_tag_attributes("img", ["loading", "decoding"])
        .add_tag_attributes("iframe", ["src", "width", "height", "title", "allowfullscreen"])
        .add_generic_attributes(["class", "id"])
        .url_relative(ammonia::UrlRelative::PassThrough)
        .attribute_filter(|element, attribute, value| match (element, attribute) {
            ("iframe", "src") => (value.starts_with("https://www.youtube.com/embed/") || value.starts_with("https://www.youtube-nocookie.com/embed/")).then(|| value.into()),
            _ => Some(value.into()),
        });
    b.clean(input).to_string()
}

/// Paths the router or the import already own; a new post or page may not take one.
const RESERVED: [&str; 28] = [
    "admin", "api", "static", "media", "uploads", "shop", "blog", "category", "product", "product-category", "brand", "era", "nib", "pw-filling-mechanism", "pw-price-range", "on-sale-pens", "trading-post", "post-your-product", "pen-repairs", "sell-my-pens", "contact", "mailing-list", "guarantee", "about-us", "privacy", "faq", "site-map",
    "wp-content",
];

async fn slug_free(pool: &PgPool, slug: &str, own_post: Option<i32>, own_page: Option<i32>) -> anyhow::Result<bool> {
    if slug.is_empty() || RESERVED.contains(&slug) {
        return Ok(false);
    }
    let n: i64 = sqlx::query_scalar("SELECT (SELECT count(*) FROM post WHERE slug = $1 AND id IS DISTINCT FROM $2) + (SELECT count(*) FROM page WHERE slug = $1 AND id IS DISTINCT FROM $3) + (SELECT count(*) FROM redirect WHERE old_path = '/' || $1 || '/')")
        .bind(slug).bind(own_post).bind(own_page).fetch_one(pool).await?;
    Ok(n == 0)
}

async fn unique_slug(pool: &PgPool, wanted: &str, own_post: Option<i32>, own_page: Option<i32>) -> anyhow::Result<String> {
    let base: String = crate::text::slugify(wanted).chars().take(90).collect();
    let base = if base.is_empty() { "untitled".to_string() } else { base };
    let mut slug = base.clone();
    let mut n = 2;
    while !slug_free(pool, &slug, own_post, own_page).await? {
        slug = format!("{base}-{n}");
        n += 1;
    }
    Ok(slug)
}

/// `/media/...` images in the body whose file is not on disk, plus how many images there are at all.
/// GoDaddy's rebuild dropped most photos from the older posts; this is how he finds them.
fn image_report(state: &State, body_html: &str) -> (usize, usize) {
    let mut total = 0;
    let mut missing = 0;
    for part in body_html.split("<img").skip(1) {
        total += 1;
        let src = part.split("src=\"").nth(1).and_then(|s| s.split('"').next()).unwrap_or("");
        if let Some(rel) = src.strip_prefix("/media/") {
            let rel = rel.split('?').next().unwrap_or("");
            if rel.contains("..") || !state.cfg.media_dir.join(rel).exists() {
                missing += 1;
            }
        }
    }
    (total, missing)
}

// ---- photo upload for the editor -----------------------------------------------------------------

/// `POST /admin/api/media/`: one photo, re-encoded (metadata stripped) with the usual 480/960 variants.
pub async fn upload_media(AxState(state): AxState<State>, Extension(cookie): Extension<CsrfCookie>, AuthApi(admin): AuthApi, headers: HeaderMap, mut mp: Multipart) -> AppResult {
    if admin.preview || !api_csrf_ok(&state, &cookie, &headers) {
        return Ok(api_err(StatusCode::FORBIDDEN, "The page expired. Reload and try again."));
    }
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
    let Some(bytes) = bytes else { return Ok(api_err(StatusCode::BAD_REQUEST, "No photo was attached.")) };
    let key = format!("uploads/admin/blog/{}/{}.jpg", chrono::Utc::now().format("%Y/%m"), super::auth::random_token(8));
    let imported = match crate::media::reencode_upload_with_variants(&bytes, &state.cfg.media_dir.join(&key)) {
        Ok(i) => i,
        Err(_) => return Ok(api_err(StatusCode::BAD_REQUEST, "That file could not be read as a photo (JPEG, PNG or WebP).")),
    };
    store::audit(&state.pool, "admin.media_added", serde_json::json!({"email": admin.email, "ip": admin.ip, "file": key})).await?;
    let show = if imported.has_960 { crate::media::variant_rel(&key, 960) } else { key.clone() };
    Ok(Json(serde_json::json!({"ok": true, "path": key, "url": format!("/media/{show}"), "full": format!("/media/{key}"), "width": imported.width, "height": imported.height})).into_response())
}

/// `GET /admin/api/posts/?q=`: published posts by title, for the editor's "Link to a blog post".
pub async fn post_lookup(AxState(state): AxState<State>, AuthApi(_admin): AuthApi, Query(q): Query<ListQuery>) -> AppResult {
    let rows = sqlx::query("SELECT title, slug FROM post WHERE status = 'published' AND ($1 = '' OR title ILIKE '%' || $1 || '%') ORDER BY published_at DESC LIMIT 25").bind(q.q.trim()).fetch_all(&state.pool).await?;
    let out: Vec<serde_json::Value> = rows.iter().map(|r| serde_json::json!({"title": r.get::<String, _>("title"), "url": format!("/{}/", r.get::<String, _>("slug"))})).collect();
    Ok(Json(serde_json::json!({"ok": true, "posts": out})).into_response())
}

// ---- posts ---------------------------------------------------------------------------------------

pub struct PostListRow {
    pub id: i32,
    pub title: String,
    pub url: String,
    pub category: String,
    pub date: String,
    pub status: String,
    pub images: usize,
    pub missing: usize,
}

#[derive(Template)]
#[template(path = "admin/posts.html")]
pub struct PostsTpl {
    pub page: PageMeta,
    pub rows: Vec<PostListRow>,
    pub q: String,
    pub only: String,
    pub total: usize,
    pub without_photos: usize,
    pub with_missing: usize,
}

#[derive(Deserialize, Default)]
pub struct ListQuery {
    #[serde(default)]
    pub q: String,
    #[serde(default)]
    pub only: String,
    #[serde(default)]
    pub saved: String,
}

pub async fn posts(AxState(state): AxState<State>, Extension(cookie): Extension<CsrfCookie>, Auth(_admin): Auth, Query(q): Query<ListQuery>) -> AppResult {
    let rows = sqlx::query("SELECT p.id, p.slug, p.title, p.status, p.published_at, p.body_html, COALESCE(bc.name, '') AS category FROM post p LEFT JOIN blog_category bc ON bc.id = p.category_id WHERE ($1 = '' OR p.title ILIKE '%' || $1 || '%') ORDER BY (p.status = 'draft') DESC, p.published_at DESC")
        .bind(q.q.trim()).fetch_all(&state.pool).await?;
    let mut all: Vec<PostListRow> = rows
        .iter()
        .map(|r| {
            let body: String = r.get("body_html");
            let (images, missing) = image_report(&state, &body);
            PostListRow { id: r.get("id"), title: r.get("title"), url: format!("/{}/", r.get::<String, _>("slug")), category: r.get("category"), date: r.get::<chrono::DateTime<chrono::Utc>, _>("published_at").format("%b %-d, %Y").to_string(), status: r.get("status"), images, missing }
        })
        .collect();
    let (total, without_photos, with_missing) = (all.len(), all.iter().filter(|r| r.images == 0).count(), all.iter().filter(|r| r.missing > 0).count());
    match q.only.as_str() {
        "no-photos" => all.retain(|r| r.images == 0),
        "missing" => all.retain(|r| r.missing > 0),
        "drafts" => all.retain(|r| r.status == "draft"),
        _ => {}
    }
    html(&PostsTpl { page: meta(&state, "Blog posts", "/admin/posts/", &cookie), rows: all, q: q.q, only: q.only, total, without_photos, with_missing })
}

pub struct Choice {
    pub value: String,
    pub label: String,
    pub selected: bool,
}

#[derive(Template)]
#[template(path = "admin/post_edit.html")]
pub struct PostEditTpl {
    pub page: PageMeta,
    pub id: i32,
    pub is_new: bool,
    pub title: String,
    pub slug: String,
    pub url: String,
    pub status: String,
    pub categories: Vec<Choice>,
    pub excerpt: String,
    pub meta_description: String,
    pub body_html: String,
    pub featured_path: String,
    pub featured_url: String,
    pub featured_alt: String,
    pub error: String,
    pub saved: bool,
    pub missing: usize,
}

async fn category_choices(pool: &PgPool, selected: Option<i32>) -> anyhow::Result<Vec<Choice>> {
    let rows = sqlx::query("SELECT id, name FROM blog_category ORDER BY name").fetch_all(pool).await?;
    Ok(rows.iter().map(|r| { let id: i32 = r.get("id"); Choice { value: id.to_string(), label: r.get("name"), selected: Some(id) == selected } }).collect())
}

pub async fn post_new(AxState(state): AxState<State>, Extension(cookie): Extension<CsrfCookie>, Auth(_admin): Auth) -> AppResult {
    html(&PostEditTpl { page: meta(&state, "New blog post", "/admin/posts/new/", &cookie), id: 0, is_new: true, title: String::new(), slug: String::new(), url: String::new(), status: "draft".into(), categories: category_choices(&state.pool, None).await?, excerpt: String::new(), meta_description: String::new(), body_html: String::new(), featured_path: String::new(), featured_url: String::new(), featured_alt: String::new(), error: String::new(), saved: false, missing: 0 })
}

pub async fn post_edit(AxState(state): AxState<State>, Extension(cookie): Extension<CsrfCookie>, Auth(_admin): Auth, Path(id): Path<i32>, Query(q): Query<ListQuery>) -> AppResult {
    let r = sqlx::query("SELECT id, slug, title, status, category_id, excerpt, meta_description, body_html, featured_path, featured_alt FROM post WHERE id = $1").bind(id).fetch_optional(&state.pool).await?.ok_or(AppError::NotFound)?;
    let body: String = r.get("body_html");
    let featured: Option<String> = r.get("featured_path");
    let slug: String = r.get("slug");
    let (_, missing) = image_report(&state, &body);
    html(&PostEditTpl {
        page: meta(&state, "Edit blog post", &format!("/admin/posts/{id}/"), &cookie),
        id,
        is_new: false,
        title: r.get("title"),
        url: format!("/{slug}/"),
        slug,
        status: r.get("status"),
        categories: category_choices(&state.pool, r.get("category_id")).await?,
        excerpt: r.get("excerpt"),
        meta_description: r.get("meta_description"),
        body_html: body,
        featured_url: featured.as_ref().map(|p| format!("/media/{p}")).unwrap_or_default(),
        featured_path: featured.unwrap_or_default(),
        featured_alt: r.get("featured_alt"),
        error: String::new(),
        saved: q.saved == "1",
        missing,
    })
}

#[derive(Deserialize)]
pub struct PostForm {
    pub csrf: String,
    pub title: String,
    #[serde(default)]
    pub category: String,
    #[serde(default)]
    pub new_category: String,
    #[serde(default)]
    pub excerpt: String,
    #[serde(default)]
    pub meta_description: String,
    #[serde(default)]
    pub body_html: String,
    #[serde(default)]
    pub featured_path: String,
    #[serde(default)]
    pub featured_alt: String,
    #[serde(default)]
    pub status: String,
}

fn safe_media_path(state: &State, p: &str) -> Option<String> {
    let p = p.trim().trim_start_matches("/media/");
    (!p.is_empty() && p.starts_with("uploads/") && !p.contains("..") && state.cfg.media_dir.join(p).exists()).then(|| p.to_string())
}

async fn resolve_category(pool: &PgPool, form: &PostForm) -> anyhow::Result<Option<i32>> {
    let new = form.new_category.trim();
    if !new.is_empty() {
        let name: String = new.chars().take(80).collect();
        let slug = crate::text::slugify(&name);
        let row = sqlx::query("INSERT INTO blog_category (slug, name) VALUES ($1, $2) ON CONFLICT (slug) DO UPDATE SET name = blog_category.name RETURNING id").bind(slug).bind(name).fetch_one(pool).await?;
        return Ok(Some(row.get("id")));
    }
    Ok(form.category.parse::<i32>().ok())
}

pub async fn post_save(AxState(state): AxState<State>, Extension(cookie): Extension<CsrfCookie>, Auth(admin): Auth, id: Option<Path<i32>>, Form(form): Form<PostForm>) -> AppResult {
    check(&state, &cookie, &admin, &form.csrf)?;
    let id = id.map(|Path(i)| i);
    let title: String = form.title.trim().chars().take(200).collect();
    if title.is_empty() {
        return Err(AppError::BadRequest("A post needs a title. Go back and add one; nothing was lost.".into()));
    }
    let body = clean_html(&form.body_html);
    let text = crate::text::strip_tags(&body);
    let excerpt = if form.excerpt.trim().is_empty() { crate::text::summary(&text, 200) } else { form.excerpt.trim().chars().take(400).collect() };
    let meta_description = if form.meta_description.trim().is_empty() { crate::text::summary(&text, 155) } else { form.meta_description.trim().chars().take(300).collect() };
    let status = if form.status == "published" { "published" } else { "draft" };
    let category_id = resolve_category(&state.pool, &form).await?;
    let featured = safe_media_path(&state, &form.featured_path);
    let featured_alt: String = form.featured_alt.trim().chars().take(200).collect();
    let words = crate::text::word_count(&text);
    let search = format!("{title} {text}");
    let saved_id = match id {
        Some(id) => {
            let before = sqlx::query("SELECT status FROM post WHERE id = $1").bind(id).fetch_optional(&state.pool).await?.ok_or(AppError::NotFound)?;
            let was: String = before.get("status");
            // A draft going live for the first time is dated today; an old post keeps its date.
            sqlx::query("UPDATE post SET title = $2, body_html = $3, excerpt = $4, meta_description = $5, category_id = $6, featured_path = $7, featured_alt = $8, status = $9, word_count = $10, search_text = $11, modified_at = now(), published_at = CASE WHEN $12 THEN now() ELSE published_at END WHERE id = $1")
                .bind(id).bind(&title).bind(&body).bind(&excerpt).bind(&meta_description).bind(category_id).bind(&featured).bind(&featured_alt).bind(status).bind(words).bind(&search).bind(was == "draft" && status == "published")
                .execute(&state.pool).await?;
            store::audit(&state.pool, "admin.post_edited", serde_json::json!({"email": admin.email, "ip": admin.ip, "post": title, "post_id": id, "status": status})).await?;
            id
        }
        None => {
            let slug = unique_slug(&state.pool, &title, None, None).await?;
            let row = sqlx::query("INSERT INTO post (slug, title, body_html, excerpt, meta_description, category_id, featured_path, featured_alt, status, word_count, search_text, published_at, modified_at) VALUES ($1,$2,$3,$4,$5,$6,$7,$8,$9,$10,$11, now(), now()) RETURNING id")
                .bind(&slug).bind(&title).bind(&body).bind(&excerpt).bind(&meta_description).bind(category_id).bind(&featured).bind(&featured_alt).bind(status).bind(words).bind(&search)
                .fetch_one(&state.pool).await?;
            let new_id: i32 = row.get("id");
            store::audit(&state.pool, "admin.post_created", serde_json::json!({"email": admin.email, "ip": admin.ip, "post": title, "post_id": new_id, "status": status})).await?;
            new_id
        }
    };
    Ok(Redirect::to(&format!("/admin/posts/{saved_id}/?saved=1")).into_response())
}

// ---- custom pages ----------------------------------------------------------------------------------

pub struct PageListRow {
    pub id: i32,
    pub title: String,
    pub url: String,
    pub status: String,
    pub in_footer: bool,
    pub modified: String,
}

#[derive(Template)]
#[template(path = "admin/pages.html")]
pub struct PagesTpl {
    pub page: PageMeta,
    pub rows: Vec<PageListRow>,
}

pub async fn pages(AxState(state): AxState<State>, Extension(cookie): Extension<CsrfCookie>, Auth(_admin): Auth) -> AppResult {
    let rows = sqlx::query("SELECT id, slug, title, status, show_in_footer, modified_at FROM page WHERE custom ORDER BY title").fetch_all(&state.pool).await?;
    let rows = rows.iter().map(|r| PageListRow { id: r.get("id"), title: r.get("title"), url: format!("/{}/", r.get::<String, _>("slug")), status: r.get("status"), in_footer: r.get("show_in_footer"), modified: r.get::<chrono::DateTime<chrono::Utc>, _>("modified_at").format("%b %-d, %Y").to_string() }).collect();
    html(&PagesTpl { page: meta(&state, "Pages", "/admin/pages/", &cookie), rows })
}

#[derive(Template)]
#[template(path = "admin/page_edit.html")]
pub struct PageEditTpl {
    pub page: PageMeta,
    pub id: i32,
    pub is_new: bool,
    pub title: String,
    pub slug: String,
    pub url: String,
    pub status: String,
    pub in_footer: bool,
    pub meta_description: String,
    pub body_html: String,
    pub saved: bool,
}

pub async fn page_new(AxState(state): AxState<State>, Extension(cookie): Extension<CsrfCookie>, Auth(_admin): Auth) -> AppResult {
    html(&PageEditTpl { page: meta(&state, "New page", "/admin/pages/new/", &cookie), id: 0, is_new: true, title: String::new(), slug: String::new(), url: String::new(), status: "draft".into(), in_footer: true, meta_description: String::new(), body_html: String::new(), saved: false })
}

pub async fn page_edit(AxState(state): AxState<State>, Extension(cookie): Extension<CsrfCookie>, Auth(_admin): Auth, Path(id): Path<i32>, Query(q): Query<ListQuery>) -> AppResult {
    let r = sqlx::query("SELECT id, slug, title, status, show_in_footer, meta_description, body_html FROM page WHERE id = $1 AND custom").bind(id).fetch_optional(&state.pool).await?.ok_or(AppError::NotFound)?;
    let slug: String = r.get("slug");
    html(&PageEditTpl { page: meta(&state, "Edit page", &format!("/admin/pages/{id}/"), &cookie), id, is_new: false, title: r.get("title"), url: format!("/{slug}/"), slug, status: r.get("status"), in_footer: r.get("show_in_footer"), meta_description: r.get("meta_description"), body_html: r.get("body_html"), saved: q.saved == "1" })
}

#[derive(Deserialize)]
pub struct PageForm {
    pub csrf: String,
    pub title: String,
    #[serde(default)]
    pub slug: String,
    #[serde(default)]
    pub meta_description: String,
    #[serde(default)]
    pub body_html: String,
    #[serde(default)]
    pub status: String,
    #[serde(default)]
    pub in_footer: String,
}

pub async fn page_save(AxState(state): AxState<State>, Extension(cookie): Extension<CsrfCookie>, Auth(admin): Auth, id: Option<Path<i32>>, Form(form): Form<PageForm>) -> AppResult {
    check(&state, &cookie, &admin, &form.csrf)?;
    let id = id.map(|Path(i)| i);
    let title: String = form.title.trim().chars().take(160).collect();
    if title.is_empty() {
        return Err(AppError::BadRequest("A page needs a title. Go back and add one; nothing was lost.".into()));
    }
    let body = clean_html(&form.body_html);
    let status = if form.status == "published" { "published" } else { "draft" };
    let in_footer = form.in_footer == "1";
    let meta_description: String = form.meta_description.trim().chars().take(300).collect();
    let saved_id = match id {
        Some(id) => {
            let n = sqlx::query("UPDATE page SET title = $2, body_html = $3, meta_description = $4, status = $5, show_in_footer = $6, modified_at = now() WHERE id = $1 AND custom")
                .bind(id).bind(&title).bind(&body).bind(&meta_description).bind(status).bind(in_footer).execute(&state.pool).await?.rows_affected();
            if n == 0 {
                return Err(AppError::NotFound);
            }
            store::audit(&state.pool, "admin.page_edited", serde_json::json!({"email": admin.email, "ip": admin.ip, "page": title, "page_id": id, "status": status})).await?;
            id
        }
        None => {
            // The address is chosen once, when the page is made, so links to it never break.
            let wanted = if form.slug.trim().is_empty() { title.clone() } else { form.slug.clone() };
            let slug = unique_slug(&state.pool, &wanted, None, None).await?;
            let row = sqlx::query("INSERT INTO page (slug, title, body_html, meta_description, status, show_in_footer, custom, published_at, modified_at) VALUES ($1,$2,$3,$4,$5,$6, TRUE, now(), now()) RETURNING id")
                .bind(&slug).bind(&title).bind(&body).bind(&meta_description).bind(status).bind(in_footer).fetch_one(&state.pool).await?;
            let new_id: i32 = row.get("id");
            store::audit(&state.pool, "admin.page_created", serde_json::json!({"email": admin.email, "ip": admin.ip, "page": title, "page_id": new_id, "status": status})).await?;
            new_id
        }
    };
    crate::app::refresh_site_cache(&state).await;
    Ok(Redirect::to(&format!("/admin/pages/{saved_id}/?saved=1")).into_response())
}

// ---- FAQs ------------------------------------------------------------------------------------------

#[derive(Template)]
#[template(path = "admin/faqs.html")]
pub struct FaqsTpl {
    pub page: PageMeta,
    pub rows: Vec<crate::db::FaqRow>,
    pub topics: Vec<String>,
    pub saved: bool,
}

pub async fn faqs(AxState(state): AxState<State>, Extension(cookie): Extension<CsrfCookie>, Auth(_admin): Auth, Query(q): Query<ListQuery>) -> AppResult {
    let rows = crate::db::faqs(&state.pool, false).await?;
    let mut topics: Vec<String> = rows.iter().map(|r| r.topic.clone()).collect();
    topics.dedup();
    html(&FaqsTpl { page: meta(&state, "FAQs", "/admin/faqs/", &cookie), rows, topics, saved: q.saved == "1" })
}

#[derive(Deserialize)]
pub struct FaqForm {
    pub csrf: String,
    pub topic: String,
    pub question: String,
    pub answer: String,
    #[serde(default)]
    pub sort_order: String,
    #[serde(default)]
    pub status: String,
    #[serde(default)]
    pub in_guide: String,
}

pub async fn faq_save(AxState(state): AxState<State>, Extension(cookie): Extension<CsrfCookie>, Auth(admin): Auth, id: Option<Path<i32>>, Form(form): Form<FaqForm>) -> AppResult {
    check(&state, &cookie, &admin, &form.csrf)?;
    let (topic, question, answer): (String, String, String) = (form.topic.trim().chars().take(80).collect(), form.question.trim().chars().take(300).collect(), form.answer.trim().chars().take(4000).collect());
    if topic.is_empty() || question.is_empty() || answer.is_empty() {
        return Err(AppError::BadRequest("A question needs a topic, the question and an answer. Go back; nothing was lost.".into()));
    }
    let status = if form.status == "draft" { "draft" } else { "published" };
    let sort: i32 = form.sort_order.trim().parse().unwrap_or(100);
    let in_guide = form.in_guide == "1";
    match id {
        Some(Path(id)) => {
            sqlx::query("UPDATE faq SET topic = $2, question = $3, answer = $4, sort_order = $5, status = $6, in_guide = $7, updated_at = now() WHERE id = $1").bind(id).bind(&topic).bind(&question).bind(&answer).bind(sort).bind(status).bind(in_guide).execute(&state.pool).await?;
            store::audit(&state.pool, "admin.faq_edited", serde_json::json!({"email": admin.email, "ip": admin.ip, "question": question, "status": status})).await?;
        }
        None => {
            sqlx::query("INSERT INTO faq (topic, question, answer, sort_order, status, in_guide) VALUES ($1,$2,$3,$4,$5,$6)").bind(&topic).bind(&question).bind(&answer).bind(sort).bind(status).bind(in_guide).execute(&state.pool).await?;
            store::audit(&state.pool, "admin.faq_created", serde_json::json!({"email": admin.email, "ip": admin.ip, "question": question})).await?;
        }
    }
    crate::app::refresh_site_cache(&state).await;
    Ok(Redirect::to("/admin/faqs/?saved=1").into_response())
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn clean_html_keeps_writing_and_drops_scripts() {
        let dirty = r#"<h2 onclick="x()">Nibs</h2><p style="color:red">A <strong>flex</strong> nib <a href="/nib/flexible/" target="_blank">here</a>.</p><script>alert(1)</script><figure><img src="/media/uploads/a.jpg" alt="Nib" onerror="x()"><figcaption>Cap</figcaption></figure><iframe src="https://evil.example/x"></iframe><iframe src="https://www.youtube.com/embed/abc"></iframe>"#;
        let c = clean_html(dirty);
        assert!(c.contains("<h2>Nibs</h2>") && c.contains("<strong>flex</strong>") && c.contains("href=\"/nib/flexible/\""));
        assert!(c.contains("<figure><img src=\"/media/uploads/a.jpg\" alt=\"Nib\"><figcaption>Cap</figcaption></figure>"));
        assert!(!c.contains("script") && !c.contains("onclick") && !c.contains("onerror") && !c.contains("style=") && !c.contains("evil.example"));
        assert!(c.contains("https://www.youtube.com/embed/abc"));
    }
    #[test]
    fn reserved_slugs_cover_the_router() {
        for s in ["shop", "admin", "faq", "site-map", "blog"] {
            assert!(RESERVED.contains(&s));
        }
    }
}
