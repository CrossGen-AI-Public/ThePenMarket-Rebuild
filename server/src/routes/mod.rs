//! The router: every public path, explicitly. Nothing else is reachable.

pub mod blog;
pub mod forms;
pub mod home;
pub mod pages;
pub mod seo;
pub mod shop;
pub mod trading_post;

use crate::app::{html_status, moved, AppError, AppResult, PageMeta, State};
use axum::body::Body;
use axum::extract::Request;
use axum::middleware::Next;
use axum::response::Response;
use crate::security;
use askama::Template;
use axum::extract::{OriginalUri, Path, State as AxState};
use axum::http::{header, StatusCode};
use axum::middleware;
use axum::response::IntoResponse;
use axum::routing::{get, post};
use axum::Router;
use tower_http::compression::CompressionLayer;
use tower_http::services::ServeDir;
use tower_http::set_header::SetResponseHeaderLayer;

pub fn router(state: State) -> Router {
    let static_files = Router::new()
        .nest_service("/static", ServeDir::new(state.cfg.static_dir.clone()))
        .nest_service("/media", ServeDir::new(state.cfg.media_dir.clone()))
        .layer(SetResponseHeaderLayer::if_not_present(header::CACHE_CONTROL, header::HeaderValue::from_static("public, max-age=604800")));

    Router::new()
        .route("/", get(home::home))
        .route("/shop/", get(shop::shop))
        .route("/on-sale-pens/", get(shop::on_sale))
        .route("/product-category/{slug}/", get(shop::category))
        .route("/brand/{slug}/", get(shop::brand))
        .route("/era/{slug}/", get(shop::era))
        .route("/nib/{slug}/", get(shop::nib))
        .route("/pw-filling-mechanism/{slug}/", get(shop::mechanism))
        .route("/pw-price-range/{slug}/", get(shop::price_range))
        .route("/product/{slug}/", get(shop::product))
        .route("/blog/", get(blog::index))
        .route("/category/{slug}/", get(blog::category))
        .route("/trading-post/", get(trading_post::index))
        .route("/trading-post/{slug}/", get(trading_post::listing))
        .route("/post-your-product/", get(trading_post::post_form).post(trading_post::post_submit))
        .route("/pen-repairs/", get(pages::repairs).post(forms::repair))
        .route("/sell-my-pens/", get(pages::sell).post(forms::sell))
        .route("/contact/", get(pages::contact).post(forms::contact))
        .route("/mailing-list/", post(forms::mailing_list))
        .route("/guarantee/", get(pages::guarantee))
        .route("/about-us/", get(pages::about))
        .route("/privacy/", get(pages::privacy))
        .route("/api/guide", post(crate::guide::chat))
        .route("/api/guide/health", get(crate::guide::health))
        .route("/api/guide/catalog.json", get(crate::guide::catalog_json))
        .route("/healthz", get(healthz))
        .route("/robots.txt", get(seo::robots))
        .route("/llms.txt", get(seo::llms))
        .route("/sitemap.xml", get(seo::sitemap_index))
        .route("/sitemap-products.xml", get(seo::sitemap_products))
        .route("/sitemap-categories.xml", get(seo::sitemap_categories))
        .route("/sitemap-terms.xml", get(seo::sitemap_terms))
        .route("/sitemap-posts.xml", get(seo::sitemap_posts))
        .route("/sitemap-pages.xml", get(seo::sitemap_pages))
        .route("/sitemap-trading-post.xml", get(seo::sitemap_trading_post))
        .route("/wp-content/uploads/{*path}", get(old_upload))
        .route("/{slug}/", get(blog::post))
        .fallback(fallback)
        .merge(static_files)
        .layer(middleware::from_fn_with_state(state.clone(), not_found_page))
        .layer(middleware::from_fn(security::security_headers))
        .layer(middleware::from_fn(security::csrf_cookie))
        .layer(middleware::from_fn(security::trailing_slash))
        .layer(CompressionLayer::new())
        .layer(axum::extract::DefaultBodyLimit::max(64 * 1024))
        .with_state(state)
}

async fn healthz() -> &'static str {
    "ok"
}

/// Old image URLs keep working: /wp-content/uploads/... -> /media/uploads/...
async fn old_upload(AxState(state): AxState<State>, Path(path): Path<String>) -> AppResult {
    let clean = path.split('?').next().unwrap_or("").trim_start_matches('/');
    if clean.contains("..") {
        return Err(AppError::NotFound);
    }
    if state.cfg.media_dir.join("uploads").join(clean).exists() {
        return Ok(moved(&format!("/media/uploads/{clean}")));
    }
    Err(AppError::NotFound)
}

#[derive(Template)]
#[template(path = "404.html")]
pub struct NotFoundTpl {
    pub page: PageMeta,
    pub path: String,
}

/// Anything not routed: consult the redirect table (2,500 old URLs), else a real 404 page.
async fn fallback(AxState(state): AxState<State>, OriginalUri(uri): OriginalUri) -> AppResult {
    let path = uri.path().to_string();
    if let Some(target) = crate::db::redirect_for(&state.pool, &path).await? {
        return Ok(moved(&target));
    }
    render_404(&state, &path)
}

pub fn render_404(state: &State, path: &str) -> AppResult {
    let mut page = PageMeta::new(state, "Page not found | ThePenMarket.com", "That page is not here. Search the pens, or start at the shop.", path);
    page.noindex = true;
    html_status(&NotFoundTpl { page, path: path.to_string() }, StatusCode::NOT_FOUND)
}

/// Any handler's bare 404 (a product or post that does not exist) becomes the real 404 page.
async fn not_found_page(AxState(state): AxState<State>, req: Request, next: Next) -> Response {
    let path = req.uri().path().to_string();
    let res = next.run(req).await;
    let is_plain = res.headers().get(header::CONTENT_TYPE).map(|v| v.to_str().unwrap_or("").starts_with("text/plain")).unwrap_or(true);
    if res.status() == StatusCode::NOT_FOUND && is_plain && !path.starts_with("/api/") && !path.starts_with("/static/") && !path.starts_with("/media/") {
        // A URL the router shapes but the data no longer has (a sold product from the old sitemap): the redirect table first.
        if let Ok(Some(target)) = crate::db::redirect_for(&state.pool, &path).await {
            return moved(&target);
        }
        if let Ok(page) = render_404(&state, &path) {
            return page;
        }
    }
    let _ = Body::empty();
    res
}


