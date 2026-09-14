//! Drippy Musings: the blog index, category archives and posts at their original root-level slugs.

use crate::app::{breadcrumb_node, html, org_node, AppError, AppResult, Entity, PageMeta, State};
use crate::db::{self, BlogCategory, PostCard};
use askama::Template;
use axum::extract::{OriginalUri, Path, Query, State as AxState};
use serde::Deserialize;

const PER_PAGE: i64 = 12;

#[derive(Deserialize, Default)]
pub struct BlogQuery {
    pub q: Option<String>,
    pub page: Option<i64>,
}

pub struct PageLink {
    pub n: i64,
    pub url: String,
    pub current: bool,
}

#[derive(Template)]
#[template(path = "blog.html")]
pub struct BlogTpl {
    pub page: PageMeta,
    pub heading: String,
    pub intro: String,
    pub posts: Vec<PostCard>,
    pub total: i64,
    pub categories: Vec<BlogCategory>,
    pub current_category: String,
    pub q: String,
    pub pages: Vec<PageLink>,
    pub prev_url: String,
    pub next_url: String,
    pub base_path: String,
    pub series: Vec<PostCard>,
}

fn page_url(base: &str, q: &str, n: i64) -> String {
    let mut parts = vec![];
    if !q.is_empty() {
        parts.push(format!("q={}", q.replace(' ', "+")));
    }
    if n > 1 {
        parts.push(format!("page={n}"));
    }
    if parts.is_empty() { base.to_string() } else { format!("{base}?{}", parts.join("&")) }
}

async fn render(state: &State, base: &str, category: Option<BlogCategory>, q: BlogQuery) -> AppResult {
    let page_n = q.page.unwrap_or(1).max(1);
    let term = q.q.clone().unwrap_or_default().trim().to_string();
    let cat_slug = category.as_ref().map(|c| c.slug.clone());
    let (posts, total) = db::posts(&state.pool, cat_slug.as_deref(), if term.is_empty() { None } else { Some(&term) }, page_n, PER_PAGE).await?;
    let categories = db::blog_categories(&state.pool).await?;
    let total_pages = ((total + PER_PAGE - 1) / PER_PAGE).max(1);
    let pages: Vec<PageLink> = (1..=total_pages).filter(|n| total_pages <= 9 || (*n - page_n).abs() <= 2 || *n == 1 || *n == total_pages).map(|n| PageLink { n, url: page_url(base, &term, n), current: n == page_n }).collect();
    let prev_url = if page_n > 1 { page_url(base, &term, page_n - 1) } else { String::new() };
    let next_url = if page_n < total_pages { page_url(base, &term, page_n + 1) } else { String::new() };
    let series = if category.is_none() && term.is_empty() && page_n == 1 {
        db::posts_by_slugs(&state.pool, &["how-do-i-start-collecting-pens-know-thy-obsession", "dont-get-fooled-by-fake-mont-blancs-vermeil-solitaire", "summers-sunburned-inks", "how-do-i-restore-a-parker-vacumatic"]).await?
    } else {
        vec![]
    };
    let (heading, intro, title, description) = match (&category, term.is_empty()) {
        (Some(c), _) => (
            c.name.clone(),
            if c.description.is_empty() { format!("{} posts in {}.", c.count, c.name) } else { format!("{} {} posts. {}", c.count, c.name, c.description) },
            format!("{}: {} Posts from Drippy Musings | ThePenMarket.com", c.name, c.count),
            if c.description.is_empty() { format!("{} posts in {} on the ThePenMarket.com blog.", c.count, c.name) } else { crate::text::summary(&c.description, 155) },
        ),
        (None, false) => (format!("Search: {term}"), format!("{total} posts mention “{term}”."), format!("“{term}” in Drippy Musings | ThePenMarket.com"), format!("Blog posts on ThePenMarket.com that mention {term}.")),
        (None, true) => (
            Entity::BLOG_NAME.to_string(),
            format!("{} posts since September 2013 by Nathaniel Cerf: how to start collecting, what happens at the repair bench, inks under the sun, fake Montblancs, and the pens of presidents and writers. \"It is a space dedicated to pen lovers old and new.\"", total),
            format!("Drippy Musings: {} Posts on Vintage Pens, Repairs and Ink | ThePenMarket.com", total),
            format!("Drippy Musings is the ThePenMarket.com blog: {} posts by Nathaniel Cerf on collecting vintage pens, restoring them, testing inks and spotting fakes.", total),
        ),
    };
    let mut page = PageMeta::new(state, &title, &description, &page_url(base, &term, page_n));
    page.section = "blog".into();
    page.noindex = !term.is_empty() || page_n > 1;
    if let Some(p) = posts.iter().find(|p| !p.image.is_empty()) {
        page.og_image = state.abs(&p.image);
    }
    let list: Vec<serde_json::Value> = posts.iter().enumerate().map(|(i, p)| serde_json::json!({"@type": "ListItem", "position": i + 1, "url": state.abs(&p.url), "name": p.title})).collect();
    let mut crumbs = vec![("Home", "/"), (Entity::BLOG_NAME, "/blog/")];
    let cat_path = category.as_ref().map(|c| format!("/category/{}/", c.slug)).unwrap_or_default();
    if let Some(c) = &category {
        crumbs.push((c.name.as_str(), cat_path.as_str()));
    }
    page = page.with_jsonld(vec![
        org_node(state),
        serde_json::json!({"@type": "Blog", "@id": state.abs("/blog/#blog"), "url": state.abs("/blog/"), "name": Entity::BLOG_NAME, "description": "The ThePenMarket.com blog by Nathaniel Cerf.", "publisher": {"@id": state.abs("/#organization")}}),
        serde_json::json!({"@type": "CollectionPage", "url": state.abs(base), "name": heading, "isPartOf": {"@id": state.abs("/#website")}, "mainEntity": {"@type": "ItemList", "numberOfItems": total, "itemListElement": list}}),
        breadcrumb_node(state, &crumbs),
    ]);
    html(&BlogTpl { page, heading, intro, posts, total, categories, current_category: cat_slug.unwrap_or_default(), q: term, pages, prev_url, next_url, base_path: base.to_string(), series })
}

pub async fn index(AxState(state): AxState<State>, Query(q): Query<BlogQuery>) -> AppResult {
    render(&state, "/blog/", None, q).await
}

pub async fn category(AxState(state): AxState<State>, Path(slug): Path<String>, Query(q): Query<BlogQuery>) -> AppResult {
    let c = db::blog_category(&state.pool, &slug).await?.ok_or(AppError::NotFound)?;
    render(&state, &format!("/category/{slug}/"), Some(c), q).await
}

#[derive(Template)]
#[template(path = "post.html")]
pub struct PostTpl {
    pub page: PageMeta,
    pub post: PostCard,
    pub body_html: String,
    pub modified: String,
    pub author: String,
    pub more: Vec<PostCard>,
    pub related_pens: Vec<db::ProductCard>,
}

/// Root-level slugs: a post, or an old URL the redirect table knows, or a real 404.
pub async fn post(AxState(state): AxState<State>, Path(slug): Path<String>, OriginalUri(uri): OriginalUri) -> AppResult {
    let Some((row, body)) = db::post_by_slug(&state.pool, &slug).await? else {
        if let Some(target) = db::redirect_for(&state.pool, uri.path()).await? {
            return Ok(crate::app::moved(&target));
        }
        return crate::routes::render_404(&state, uri.path());
    };
    let card = PostCard::from(row.clone());
    let more = if let Some(c) = &row.category_slug { db::posts(&state.pool, Some(c), None, 1, 4).await?.0.into_iter().filter(|p| p.slug != slug).take(3).collect() } else { vec![] };
    // Pens the post mentions by brand: the first brand name in the title, if any.
    let brands = ["Parker", "Sheaffer", "Waterman", "Montblanc", "Mont Blanc", "Esterbrook", "Conklin", "Pelikan", "Wahl", "Eversharp", "Namiki", "Pilot", "Omas", "Aurora", "Visconti", "Lamy", "TWSBI", "Cross"];
    let mentioned = brands.iter().find(|b| row.title.to_lowercase().contains(&b.to_lowercase())).map(|b| b.to_string());
    let related_pens = match mentioned {
        Some(b) => {
            let f = db::ShopFilter { q: Some(b), sort: "newest".into(), page: 1, per_page: 4, ..Default::default() };
            let ranges = vec![];
            db::shop(&state.pool, &f, &ranges).await?.items
        }
        None => vec![],
    };
    let title = format!("{} | Drippy Musings | ThePenMarket.com", row.title);
    let mut page = PageMeta::new(&state, &title, &row.meta_description, &format!("/{slug}/"));
    page.section = "blog".into();
    if !card.image.is_empty() {
        page.og_image = state.abs(&card.image);
    }
    let mut crumbs = vec![("Home".to_string(), "/".to_string()), (Entity::BLOG_NAME.to_string(), "/blog/".to_string())];
    if let (Some(n), Some(s)) = (&row.category_name, &row.category_slug) {
        crumbs.push((n.clone(), format!("/category/{s}/")));
    }
    crumbs.push((row.title.clone(), format!("/{slug}/")));
    page = page.with_jsonld(vec![
        org_node(&state),
        serde_json::json!({
            "@type": "Article",
            "@id": state.abs(&format!("/{slug}/#article")),
            "headline": row.title,
            "description": row.meta_description,
            "image": if card.image.is_empty() { serde_json::Value::Null } else { serde_json::json!([state.abs(&card.image)]) },
            "datePublished": row.published_at.to_rfc3339(),
            "dateModified": row.modified_at.to_rfc3339(),
            "author": {"@type": "Person", "@id": state.abs("/about-us/#nathaniel-cerf"), "name": Entity::OWNER, "url": state.abs("/about-us/")},
            "publisher": {"@id": state.abs("/#organization")},
            "isPartOf": {"@id": state.abs("/blog/#blog")},
            "articleSection": row.category_name,
            "wordCount": row.word_count,
            "mainEntityOfPage": state.abs(&format!("/{slug}/"))
        }),
        breadcrumb_node(&state, &crumbs.iter().map(|(a, b)| (a.as_str(), b.as_str())).collect::<Vec<_>>()),
    ]);
    html(&PostTpl { page, post: card, body_html: body, modified: row.modified_at.format("%B %-d, %Y").to_string(), author: row.author_name, more, related_pens })
}
