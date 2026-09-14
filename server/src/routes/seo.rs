//! robots.txt, llms.txt and the sitemaps, split by section with an index at /sitemap.xml.

use crate::app::{AppResult, Entity, State};
use crate::db;
use crate::engine;
use crate::money::fmt_dollars;
use axum::extract::State as AxState;
use axum::http::header;
use axum::response::IntoResponse;

fn xml(body: String) -> AppResult {
    Ok(([(header::CONTENT_TYPE, "application/xml; charset=utf-8"), (header::CACHE_CONTROL, "public, max-age=3600")], body).into_response())
}

fn text(body: String) -> AppResult {
    Ok(([(header::CONTENT_TYPE, "text/plain; charset=utf-8"), (header::CACHE_CONTROL, "public, max-age=3600")], body).into_response())
}

fn esc(s: &str) -> String {
    s.replace('&', "&amp;").replace('<', "&lt;").replace('>', "&gt;").replace('"', "&quot;")
}

fn urlset(state: &State, entries: &[(String, String)]) -> String {
    let mut out = String::from("<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n<urlset xmlns=\"http://www.sitemaps.org/schemas/sitemap/0.9\">\n");
    for (path, lastmod) in entries {
        out.push_str(&format!("  <url><loc>{}</loc><lastmod>{}</lastmod></url>\n", esc(&state.abs(path)), lastmod));
    }
    out.push_str("</urlset>\n");
    out
}

fn today() -> String {
    chrono::Utc::now().format("%Y-%m-%d").to_string()
}

pub async fn robots(AxState(state): AxState<State>) -> AppResult {
    let o = state.origin();
    text(format!(
        "User-agent: *\nAllow: /\nAllow: /llms.txt\nDisallow: /admin/\nDisallow: /api/\nDisallow: /healthz\nDisallow: /uploads/\nDisallow: /post-your-product/\nDisallow: /mailing-list/\nDisallow: /*?*sort=\nDisallow: /*?*page=\n\nUser-agent: GPTBot\nAllow: /\n\nUser-agent: ClaudeBot\nAllow: /\n\nUser-agent: PerplexityBot\nAllow: /\n\nUser-agent: Google-Extended\nAllow: /\n\nSitemap: {o}/sitemap.xml\nSitemap: {o}/sitemap-products.xml\nSitemap: {o}/sitemap-categories.xml\nSitemap: {o}/sitemap-terms.xml\nSitemap: {o}/sitemap-posts.xml\nSitemap: {o}/sitemap-pages.xml\nSitemap: {o}/sitemap-trading-post.xml\n"
    ))
}

pub async fn llms(AxState(state): AxState<State>) -> AppResult {
    let cat = state.catalog();
    let s = engine::stats(&cat);
    let (_, posts) = db::posts(&state.pool, None, None, 1, 1).await?;
    let (_, listings) = db::listings(&state.pool, 1, 1).await?;
    let o = state.origin();
    let cats: Vec<String> = s.by_category.iter().map(|c| format!("- {} ({}): {o}/product-category/{}/", c.name, c.count, c.slug)).collect();
    text(format!(
        "# {name}\n\n> Buy, sell and trade pens the way you want. {name} sells restored vintage fountain pens and pre-owned luxury pens, one of each, repairs vintage pens, buys and consigns collections, and runs the Trading Post classifieds. Owner and restorer: {owner}. Online since 2007. {po}, {city}, {st} {zip}. {email}. {phone}.\n\n## The catalog ({live} items in stock, {min} to {max})\n\n{cats}\n\nEvery product page lists Filling Mechanism, Era, Nib Size, SKU and capped length, with his description and the flaws stated plainly. Facet pages: {o}/brand/{{slug}}/, {o}/era/{{slug}}/, {o}/nib/{{slug}}/, {o}/pw-filling-mechanism/{{slug}}/ (each opens with a definition).\n\n## {gname}\n\n{terms}\n\n## Repairs\n\nWhat can we repair: {scope}. {excl}. Free estimate; discounts are available for shipments of {disc} or more pens. {o}/pen-repairs/\n\n## Selling pens\n\nCash to consignment for vintage pens and pre-owned luxury pens. {sell} {o}/sell-my-pens/\n\n## Trading Post ({listings} live listings)\n\nClassified ads for fountain pens, pencils and writing ephemera: $5 per listing, or Unlimited Posts for $125 a year. The merchandise is not guaranteed by {name}, nor does the transaction pass through our hands. {o}/trading-post/\n\n## Drippy Musings ({posts} posts)\n\nThe blog since 2013: How Do I Start Collecting Pens?, Notes from the Work Bench, Ink Reviews, Famous People & Pens, fake Montblanc identification. {o}/blog/\n\n## Contact\n\n{name}, {po}, {city}, {st} {zip}. {email}. {phone}. Facebook: {fb}. Instagram: {ig}.\n\n## Sitemaps\n\n{o}/sitemap.xml\n",
        name = Entity::NAME, owner = Entity::OWNER, po = Entity::PO_BOX, city = Entity::CITY, st = Entity::STATE, zip = Entity::ZIP, email = Entity::EMAIL, phone = Entity::PHONE,
        live = s.live_total, min = fmt_dollars(s.price_min_cents), max = fmt_dollars(s.price_max_cents), cats = cats.join("\n"),
        gname = Entity::GUARANTEE_NAME, terms = engine::GUARANTEE_TERMS, scope = engine::REPAIR_SCOPE.join(", "), excl = engine::REPAIR_EXCLUSION, disc = engine::REPAIR_DISCOUNT_MIN_PENS,
        sell = engine::SELL_EXCLUSION_LINE, listings = listings, posts = posts, fb = Entity::FACEBOOK, ig = Entity::INSTAGRAM
    ))
}

pub async fn sitemap_index(AxState(state): AxState<State>) -> AppResult {
    let t = today();
    let mut out = String::from("<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n<sitemapindex xmlns=\"http://www.sitemaps.org/schemas/sitemap/0.9\">\n");
    for n in ["products", "categories", "terms", "posts", "pages", "trading-post"] {
        out.push_str(&format!("  <sitemap><loc>{}</loc><lastmod>{t}</lastmod></sitemap>\n", esc(&state.abs(&format!("/sitemap-{n}.xml")))));
    }
    out.push_str("</sitemapindex>\n");
    xml(out)
}

pub async fn sitemap_products(AxState(state): AxState<State>) -> AppResult {
    xml(urlset(&state, &db::sitemap_products(&state.pool).await?))
}

pub async fn sitemap_categories(AxState(state): AxState<State>) -> AppResult {
    let t = today();
    let mut entries = vec![("/shop/".to_string(), t.clone()), ("/on-sale-pens/".to_string(), t.clone())];
    for c in db::categories(&state.pool).await? {
        entries.push((format!("/product-category/{}/", c.slug), t.clone()));
    }
    for c in db::blog_categories(&state.pool).await? {
        entries.push((format!("/category/{}/", c.slug), t.clone()));
    }
    xml(urlset(&state, &entries))
}

pub async fn sitemap_terms(AxState(state): AxState<State>) -> AppResult {
    let t = today();
    let mut entries = vec![];
    for (table, fk, prefix) in [("brand", "brand_id", "/brand/"), ("era", "era_id", "/era/"), ("nib", "nib_id", "/nib/"), ("filling_mechanism", "filling_mechanism_id", "/pw-filling-mechanism/")] {
        for slug in db::term_slugs_with_products(&state.pool, table, fk).await? {
            entries.push((format!("{prefix}{slug}/"), t.clone()));
        }
    }
    for r in db::price_ranges(&state.pool).await? {
        entries.push((format!("/pw-price-range/{}/", r.slug), t.clone()));
    }
    xml(urlset(&state, &entries))
}

pub async fn sitemap_posts(AxState(state): AxState<State>) -> AppResult {
    let mut entries = vec![("/blog/".to_string(), today())];
    entries.extend(db::sitemap_posts(&state.pool).await?);
    xml(urlset(&state, &entries))
}

pub async fn sitemap_pages(AxState(state): AxState<State>) -> AppResult {
    let t = today();
    let entries: Vec<(String, String)> = ["/", "/about-us/", "/contact/", "/guarantee/", "/pen-repairs/", "/sell-my-pens/", "/privacy/", "/post-your-product/"].iter().map(|p| (p.to_string(), t.clone())).collect();
    xml(urlset(&state, &entries))
}

pub async fn sitemap_trading_post(AxState(state): AxState<State>) -> AppResult {
    let mut entries = vec![("/trading-post/".to_string(), today())];
    entries.extend(db::sitemap_listings(&state.pool).await?);
    xml(urlset(&state, &entries))
}
