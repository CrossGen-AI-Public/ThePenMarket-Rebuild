//! The Trading Post: his classifieds, imported read-only from the live site, plus the $5 listing
//! form as a labelled demo.

use crate::app::{breadcrumb_node, html, org_node, AppError, AppResult, Entity, PageMeta, State};
use crate::db::{self, Listing};
use crate::engine;
use crate::money::fmt_dollars;
use crate::routes::forms::{self, FormDoneTpl};
use crate::security::CsrfCookie;
use askama::Template;
use axum::extract::{ConnectInfo, Multipart, Path, Query, State as AxState};
use axum::http::HeaderMap;
use axum::Extension;
use serde::Deserialize;

const PER_PAGE: i64 = 12;

#[derive(Deserialize, Default)]
pub struct TpQuery {
    pub page: Option<i64>,
}

pub struct PageLink {
    pub n: i64,
    pub url: String,
    pub current: bool,
}

#[derive(Template)]
#[template(path = "trading_post.html")]
pub struct TpIndexTpl {
    pub page: PageMeta,
    pub listings: Vec<Listing>,
    pub total: i64,
    pub pages: Vec<PageLink>,
    pub prev_url: String,
    pub next_url: String,
    pub listing_price: String,
    pub unlimited_price: String,
    pub tp_email: String,
}

pub async fn index(AxState(state): AxState<State>, Query(q): Query<TpQuery>) -> AppResult {
    let page_n = q.page.unwrap_or(1).max(1);
    let (listings, total) = db::listings(&state.pool, page_n, PER_PAGE).await?;
    let total_pages = ((total + PER_PAGE - 1) / PER_PAGE).max(1);
    let url = |n: i64| if n > 1 { format!("/trading-post/?page={n}") } else { "/trading-post/".to_string() };
    let pages: Vec<PageLink> = (1..=total_pages).map(|n| PageLink { n, url: url(n), current: n == page_n }).collect();
    let prev_url = if page_n > 1 { url(page_n - 1) } else { String::new() };
    let next_url = if page_n < total_pages { url(page_n + 1) } else { String::new() };
    let title = format!("Trading Post: {} Pen Classifieds, $5 a Listing | ThePenMarket.com", total);
    let description = format!("The Trading Post is ThePenMarket.com's classified ads for fountain pens, pencils and writing ephemera: {} live listings. Sell your pens directly to our customers for $5 a listing, and you keep your pen until someone pays you.", total);
    let mut page = PageMeta::new(&state, &title, &description, &url(page_n));
    page.section = "trading-post".into();
    page.noindex = page_n > 1;
    if let Some(l) = listings.iter().find(|l| !l.image.is_empty()) {
        page.og_image = state.abs(&l.image_large);
    }
    let list: Vec<serde_json::Value> = listings.iter().enumerate().map(|(i, l)| serde_json::json!({"@type": "ListItem", "position": i + 1, "url": state.abs(&l.url), "name": l.title})).collect();
    page = page.with_jsonld(vec![
        org_node(&state),
        serde_json::json!({"@type": "CollectionPage", "@id": state.abs("/trading-post/#collection"), "url": state.abs("/trading-post/"), "name": "Trading Post", "description": description, "isPartOf": {"@id": state.abs("/#website")}, "mainEntity": {"@type": "ItemList", "numberOfItems": total, "itemListElement": list}}),
        serde_json::json!({"@type": "FAQPage", "mainEntity": [
            {"@type": "Question", "name": "How much does a Trading Post listing cost?", "acceptedAnswer": {"@type": "Answer", "text": "It only costs $5 per listing, and you keep your pen until someone pays you. An Unlimited Posts membership is $125 for one calendar year of unlimited listings."}},
            {"@type": "Question", "name": "Does ThePenMarket.com guarantee Trading Post items?", "acceptedAnswer": {"@type": "Answer", "text": "No. The merchandise is not guaranteed by ThePenMarket.com, nor does the transaction pass through our hands. We do our best to vet out the bad dealers, and ask that you report any problems immediately to tradingpost@thepenmarket.com."}}
        ]}),
        breadcrumb_node(&state, &[("Home", "/"), ("Trading Post", "/trading-post/")]),
    ]);
    html(&TpIndexTpl { page, listings, total, pages, prev_url, next_url, listing_price: fmt_dollars(engine::TRADING_POST_LISTING_CENTS), unlimited_price: fmt_dollars(engine::UNLIMITED_POSTS_YEAR_CENTS), tp_email: Entity::TP_EMAIL.to_string() })
}

#[derive(Template)]
#[template(path = "tp_listing.html")]
pub struct TpListingTpl {
    pub page: PageMeta,
    pub l: Listing,
    pub more: Vec<Listing>,
    pub tp_email: String,
}

pub async fn listing(AxState(state): AxState<State>, Path(slug): Path<String>) -> AppResult {
    let l = db::listing(&state.pool, &slug).await?.ok_or(AppError::NotFound)?;
    let (more, _) = db::listings(&state.pool, 1, 5).await?;
    let more: Vec<Listing> = more.into_iter().filter(|m| m.slug != slug).take(4).collect();
    let desc_text = crate::text::strip_tags(&l.description_html);
    let title = format!("{} | Trading Post | ThePenMarket.com", l.title);
    let description = crate::text::summary(&format!("Trading Post listing, {}: {} {}", l.price, l.title, desc_text), 155);
    let mut page = PageMeta::new(&state, &title, &description, &format!("/trading-post/{slug}/"));
    page.section = "trading-post".into();
    if !l.image_large.is_empty() {
        page.og_image = state.abs(&l.image_large);
    }
    page = page.with_jsonld(vec![
        org_node(&state),
        serde_json::json!({
            "@type": "Product",
            "@id": state.abs(&format!("/trading-post/{slug}/#listing")),
            "name": l.title,
            "description": desc_text,
            "image": if l.image_large.is_empty() { serde_json::Value::Null } else { serde_json::json!([state.abs(&l.image_large)]) },
            "url": state.abs(&format!("/trading-post/{slug}/")),
            "offers": {"@type": "Offer", "priceCurrency": "USD", "price": l.price.trim_start_matches('$').replace(',', ""), "availability": "https://schema.org/InStock", "itemCondition": "https://schema.org/UsedCondition", "description": "Private classified listing on the ThePenMarket.com Trading Post; the sale is between buyer and seller."}
        }),
        breadcrumb_node(&state, &[("Home", "/"), ("Trading Post", "/trading-post/"), (l.title.as_str(), &format!("/trading-post/{slug}/"))]),
    ]);
    html(&TpListingTpl { page, l, more, tp_email: Entity::TP_EMAIL.to_string() })
}

#[derive(Template)]
#[template(path = "post_your_product.html")]
pub struct PostFormTpl {
    pub page: PageMeta,
    pub csrf: String,
    pub listing_price: String,
    pub unlimited_price: String,
}

pub async fn post_form(AxState(state): AxState<State>, Extension(cookie): Extension<CsrfCookie>) -> AppResult {
    let mut page = PageMeta::new(&state, "Post Your Product on the Trading Post ($5 a Listing) | ThePenMarket.com", "List a pen, pencil or writing ephemera on the ThePenMarket.com Trading Post for $5. Era, price, contact and a photo; you keep the pen until someone pays you.", "/post-your-product/");
    page.section = "trading-post".into();
    page = page.with_jsonld(vec![org_node(&state), breadcrumb_node(&state, &[("Home", "/"), ("Trading Post", "/trading-post/"), ("Post Your Product", "/post-your-product/")])]);
    let csrf = crate::security::csrf_token(&state.cfg.csrf_secret, &cookie.0);
    html(&PostFormTpl { page, csrf, listing_price: fmt_dollars(engine::TRADING_POST_LISTING_CENTS), unlimited_price: fmt_dollars(engine::UNLIMITED_POSTS_YEAR_CENTS) })
}

pub async fn post_submit(AxState(state): AxState<State>, Extension(cookie): Extension<CsrfCookie>, ConnectInfo(peer): ConnectInfo<std::net::SocketAddr>, headers: HeaderMap, mp: Multipart) -> AppResult {
    let f = forms::read_multipart(&state, &cookie, peer, &headers, mp, &["item", "era", "price", "contact"]).await?;
    let id = db::save_form(&state.pool, "trading_post", f.fields.clone(), f.photo_key.clone()).await?;
    let page = PageMeta::new(&state, "Listing received (demo) | ThePenMarket.com", "Demo submission stored; no payment was taken and nothing was published.", "/post-your-product/");
    html(&FormDoneTpl {
        page,
        heading: "Listing received, as a demo".into(),
        lines: vec![
            format!("Reference #{id}. This build stores the listing but does not take the $5 payment or publish it; on the live site, Gravity Forms charged the fee and the listing went to review."),
            format!("Item: {}. Era: {}. Price: {}. Contact: {}.", f.get("item"), f.get("era"), f.get("price"), f.get("contact")),
            if f.photo_key.is_some() { "Your photo was received and stored (re-encoded, metadata stripped).".into() } else { "No photo was attached.".into() },
        ],
        back_url: "/trading-post/".into(),
        back_label: "Back to the Trading Post".into(),
    })
}
