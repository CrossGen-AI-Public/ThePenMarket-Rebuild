//! The glossary in the admin. Entries found in his writing arrive here as hidden drafts with his own
//! sentences beside the box; he writes (or pastes) the definition and publishes. The make / nib /
//! filling / era definitions the build drafted are listed for him to rewrite and tick as his own.
//! Nothing on this page writes a word for him.

use super::content::{check, meta};
use super::{store, Auth};
use crate::app::{html, AppError, AppResult, PageMeta, State};
use crate::glossary::{self, TermRow, TAXONOMIES};
use crate::security::CsrfCookie;
use askama::Template;
use axum::extract::{Form, Path, Query, State as AxState};
use axum::response::{IntoResponse, Redirect};
use axum::Extension;
use serde::Deserialize;
use sqlx::Row;

pub struct Miss {
    pub word: String,
    pub hits: i32,
}
pub struct TaxDef {
    pub tbl: String,
    pub label: String,
    pub slug: String,
    pub name: String,
    pub definition: String,
    pub approved: bool,
    pub url: String,
}

#[derive(Template)]
#[template(path = "admin/glossary.html")]
pub struct ListTpl {
    pub page: PageMeta,
    pub live: Vec<TermRow>,
    pub found: Vec<TermRow>,
    pub mine: Vec<TermRow>,
    pub misses: Vec<Miss>,
    pub defs: Vec<TaxDef>,
    pub defs_approved: usize,
    pub note: String,
}

#[derive(Deserialize, Default)]
pub struct NoteQuery {
    #[serde(default)]
    pub saved: String,
    #[serde(default)]
    pub found: String,
    #[serde(default)]
    pub deleted: String,
}

fn topics_of(rows: &[TermRow]) -> Vec<String> {
    let mut t: Vec<String> = rows.iter().map(|r| r.topic.clone()).filter(|t| !t.is_empty()).collect();
    t.sort();
    t.dedup();
    t
}

pub async fn list(AxState(state): AxState<State>, Extension(cookie): Extension<CsrfCookie>, Auth(_admin): Auth, Query(q): Query<NoteQuery>) -> AppResult {
    let rows = glossary::all_terms(&state.pool).await?;
    let (live, drafts): (Vec<TermRow>, Vec<TermRow>) = rows.into_iter().partition(|r| r.status == "published");
    let (mut found, mine): (Vec<TermRow>, Vec<TermRow>) = drafts.into_iter().partition(|r| r.origin == "mined" && r.definition.is_empty());
    found.sort_by(|a, b| b.mentions.cmp(&a.mentions).then(a.name.cmp(&b.name)));
    let misses = sqlx::query("SELECT word, hits FROM glossary_miss ORDER BY hits DESC, last_seen DESC LIMIT 30").fetch_all(&state.pool).await?.iter().map(|r| Miss { word: r.get("word"), hits: r.get("hits") }).collect();
    let mut defs = vec![];
    for (table, _, _, label, base) in TAXONOMIES {
        let sql = format!("SELECT t.slug, t.name, t.definition, (a.slug IS NOT NULL) AS approved FROM {table} t LEFT JOIN glossary_approved_def a ON a.tbl = $1 AND a.slug = t.slug ORDER BY t.name");
        for r in sqlx::query(&sql).bind(table).fetch_all(&state.pool).await? {
            let slug: String = r.get("slug");
            defs.push(TaxDef { tbl: table.to_string(), label: label.to_string(), url: format!("{base}{slug}/"), slug, name: crate::text::decode(&r.get::<String, _>("name")), definition: r.get("definition"), approved: r.get("approved") });
        }
    }
    let defs_approved = defs.iter().filter(|d| d.approved).count();
    let note = if q.saved == "1" { "Saved.".to_string() } else if q.deleted == "1" { "Deleted.".to_string() } else if !q.found.is_empty() { format!("Looked through your posts and listings: {} new draft{} proposed.", q.found, if q.found == "1" { "" } else { "s" }) } else { String::new() };
    html(&ListTpl { page: meta(&state, "Glossary", "/admin/glossary/", &cookie), live, found, mine, misses, defs, defs_approved, note })
}

pub struct QuoteRow {
    pub quote: String,
    pub source_url: String,
    pub source_title: String,
}

#[derive(Template)]
#[template(path = "admin/glossary_edit.html")]
pub struct EditTpl {
    pub page: PageMeta,
    pub t: TermRow,
    pub quotes: Vec<QuoteRow>,
    pub brands: Vec<(String, String)>,
    pub topics: Vec<String>,
    pub saved: bool,
    pub problem: String,
}

#[derive(Deserialize, Default)]
pub struct EditQuery {
    #[serde(default)]
    pub saved: String,
    #[serde(default)]
    pub problem: String,
}

pub async fn edit(AxState(state): AxState<State>, Extension(cookie): Extension<CsrfCookie>, Auth(_admin): Auth, Path(id): Path<i32>, Query(q): Query<EditQuery>) -> AppResult {
    let t = glossary::term_by_id(&state.pool, id).await?.ok_or(AppError::NotFound)?;
    let quotes = sqlx::query("SELECT quote, source_url, source_title FROM glossary_quote WHERE term_id = $1 ORDER BY score DESC, id").bind(id).fetch_all(&state.pool).await?.iter().map(|r| QuoteRow { quote: r.get("quote"), source_url: r.get("source_url"), source_title: r.get("source_title") }).collect();
    let brands = sqlx::query("SELECT slug, name FROM brand ORDER BY name").fetch_all(&state.pool).await?.iter().map(|r| (r.get::<String, _>("slug"), crate::text::decode(&r.get::<String, _>("name")))).collect();
    let topics = topics_of(&glossary::all_terms(&state.pool).await?);
    let problem = if q.problem == "empty" { "An entry needs a definition before it can be published. It was saved as a hidden draft.".to_string() } else { String::new() };
    html(&EditTpl { page: meta(&state, &format!("Glossary: {}", t.name), &format!("/admin/glossary/{id}/"), &cookie), t, quotes, brands, topics, saved: q.saved == "1", problem })
}

#[derive(Deserialize)]
pub struct NewForm {
    pub csrf: String,
    pub name: String,
    #[serde(default)]
    pub kind: String,
}

pub async fn create(AxState(state): AxState<State>, Extension(cookie): Extension<CsrfCookie>, Auth(admin): Auth, Form(form): Form<NewForm>) -> AppResult {
    check(&state, &cookie, &admin, &form.csrf)?;
    let name: String = form.name.trim().chars().take(80).collect();
    let base = crate::text::slugify(&name);
    if base.is_empty() {
        return Err(AppError::BadRequest("An entry needs a name. Go back; nothing was lost.".into()));
    }
    let kind = if form.kind == "model" { "model" } else { "term" };
    let mut slug = base.clone();
    let mut n = 2;
    while sqlx::query_scalar::<_, i64>("SELECT count(*) FROM glossary_term WHERE slug = $1").bind(&slug).fetch_one(&state.pool).await? > 0 {
        slug = format!("{base}-{n}");
        n += 1;
    }
    let id: i32 = sqlx::query_scalar("INSERT INTO glossary_term (slug, name, kind, topic) VALUES ($1, $2, $3, $4) RETURNING id").bind(&slug).bind(&name).bind(kind).bind(if kind == "model" { "Models" } else { "" }).fetch_one(&state.pool).await?;
    // The word is no longer one visitors are waiting for.
    sqlx::query("DELETE FROM glossary_miss WHERE word = $1").bind(crate::gloss::fold(&name)).execute(&state.pool).await?;
    store::audit(&state.pool, "admin.glossary_created", serde_json::json!({"email": admin.email, "ip": admin.ip, "name": name})).await?;
    Ok(Redirect::to(&format!("/admin/glossary/{id}/")).into_response())
}

#[derive(Deserialize)]
pub struct TermForm {
    pub csrf: String,
    pub name: String,
    #[serde(default)]
    pub kind: String,
    #[serde(default)]
    pub topic: String,
    #[serde(default)]
    pub aliases: String,
    #[serde(default)]
    pub definition: String,
    #[serde(default)]
    pub brand_slug: String,
    #[serde(default)]
    pub image_path: String,
    #[serde(default)]
    pub image_alt: String,
    #[serde(default)]
    pub see_also: String,
    #[serde(default)]
    pub source_url: String,
    #[serde(default)]
    pub source_title: String,
    #[serde(default)]
    pub status: String,
    #[serde(default)]
    pub in_guide: String,
    #[serde(default)]
    pub link_in_listings: String,
}

fn cut(s: &str, max: usize) -> String {
    s.trim().chars().take(max).collect()
}

pub async fn save(AxState(state): AxState<State>, Extension(cookie): Extension<CsrfCookie>, Auth(admin): Auth, Path(id): Path<i32>, Form(form): Form<TermForm>) -> AppResult {
    check(&state, &cookie, &admin, &form.csrf)?;
    let name = cut(&form.name, 80);
    if name.is_empty() {
        return Err(AppError::BadRequest("An entry needs a name. Go back; nothing was lost.".into()));
    }
    let definition = cut(&form.definition, 6000);
    let kind = if form.kind == "model" { "model" } else { "term" };
    // Only his own site's photos and pages: a path on this server, never an outside address.
    let local = |s: &str, max: usize| -> String { let v = cut(s, max); if v.starts_with('/') && !v.starts_with("//") && !v.contains("..") { v } else { String::new() } };
    let image_path = local(&form.image_path, 300);
    let source_url = local(&form.source_url, 300);
    let wants_live = form.status == "published";
    let status = if wants_live && !definition.is_empty() { "published" } else { "draft" };
    sqlx::query("UPDATE glossary_term SET name = $2, kind = $3, topic = $4, aliases = $5, definition = $6, brand_slug = $7, image_path = $8, image_alt = $9, see_also = $10, source_url = $11, source_title = $12, status = $13, in_guide = $14, link_in_listings = $15, updated_at = now() WHERE id = $1")
        .bind(id).bind(&name).bind(kind).bind(cut(&form.topic, 60)).bind(cut(&form.aliases, 400)).bind(&definition).bind(cut(&form.brand_slug, 80)).bind(&image_path).bind(cut(&form.image_alt, 200)).bind(cut(&form.see_also, 400))
        .bind(&source_url).bind(if source_url.is_empty() { String::new() } else { cut(&form.source_title, 200) }).bind(status).bind(form.in_guide == "1").bind(form.link_in_listings == "1")
        .execute(&state.pool).await?;
    store::audit(&state.pool, "admin.glossary_edited", serde_json::json!({"email": admin.email, "ip": admin.ip, "name": name, "status": status})).await?;
    crate::app::refresh_site_cache(&state).await;
    let tail = if wants_live && status == "draft" { "problem=empty" } else { "saved=1" };
    Ok(Redirect::to(&format!("/admin/glossary/{id}/?{tail}")).into_response())
}

#[derive(Deserialize)]
pub struct CsrfOnly {
    pub csrf: String,
}

pub async fn delete(AxState(state): AxState<State>, Extension(cookie): Extension<CsrfCookie>, Auth(admin): Auth, Path(id): Path<i32>, Form(form): Form<CsrfOnly>) -> AppResult {
    check(&state, &cookie, &admin, &form.csrf)?;
    if let Some(t) = glossary::term_by_id(&state.pool, id).await? {
        sqlx::query("DELETE FROM glossary_term WHERE id = $1").bind(id).execute(&state.pool).await?;
        store::audit(&state.pool, "admin.glossary_deleted", serde_json::json!({"email": admin.email, "ip": admin.ip, "name": t.name})).await?;
        crate::app::refresh_site_cache(&state).await;
    }
    Ok(Redirect::to("/admin/glossary/?deleted=1").into_response())
}

/// "Look through my writing again": new posts and listings since the last pass.
pub async fn mine(AxState(state): AxState<State>, Extension(cookie): Extension<CsrfCookie>, Auth(admin): Auth, Form(form): Form<CsrfOnly>) -> AppResult {
    check(&state, &cookie, &admin, &form.csrf)?;
    let report = glossary::mine(&state.pool).await?;
    store::audit(&state.pool, "admin.glossary_mined", serde_json::json!({"email": admin.email, "ip": admin.ip, "proposed": report.proposed, "quotes": report.quotes})).await?;
    Ok(Redirect::to(&format!("/admin/glossary/?found={}", report.proposed)).into_response())
}

#[derive(Deserialize)]
pub struct DefForm {
    pub csrf: String,
    pub tbl: String,
    pub slug: String,
    #[serde(default)]
    pub definition: String,
    #[serde(default)]
    pub approved: String,
}

/// His wording for a make, nib, filling mechanism or era, and whether it is his to show.
pub async fn def_save(AxState(state): AxState<State>, Extension(cookie): Extension<CsrfCookie>, Auth(admin): Auth, Form(form): Form<DefForm>) -> AppResult {
    check(&state, &cookie, &admin, &form.csrf)?;
    let Some((table, ..)) = TAXONOMIES.iter().find(|(t, ..)| *t == form.tbl) else { return Err(AppError::BadRequest("Unknown list.".into())) };
    let definition = cut(&form.definition, 4000);
    let approved = form.approved == "1" && !definition.is_empty();
    let changed = sqlx::query(&format!("UPDATE {table} SET definition = $2 WHERE slug = $1")).bind(&form.slug).bind(&definition).execute(&state.pool).await?;
    if changed.rows_affected() == 0 {
        return Err(AppError::NotFound);
    }
    if approved {
        sqlx::query("INSERT INTO glossary_approved_def (tbl, slug) VALUES ($1, $2) ON CONFLICT DO NOTHING").bind(table).bind(&form.slug).execute(&state.pool).await?;
    } else {
        sqlx::query("DELETE FROM glossary_approved_def WHERE tbl = $1 AND slug = $2").bind(table).bind(&form.slug).execute(&state.pool).await?;
    }
    store::audit(&state.pool, "admin.glossary_def_edited", serde_json::json!({"email": admin.email, "ip": admin.ip, "list": table, "slug": form.slug, "approved": approved})).await?;
    crate::app::refresh_site_cache(&state).await;
    Ok(Redirect::to("/admin/glossary/?saved=1#defs").into_response())
}
