//! The Trading Post: his classifieds, imported from the live site, and the listing form. With Stripe
//! keys set, a listing waits unpublished until its fee is paid at Stripe Checkout (or goes live at
//! once for an Unlimited Posts member); without keys the form stays the labelled demo it was.

use crate::app::{breadcrumb_node, html, org_node, AppError, AppResult, Entity, PageMeta, State};
use crate::db::{self, Listing};
use crate::engine;
use crate::money::fmt_dollars;
use crate::routes::forms::{self, FormDoneTpl};
use crate::security::CsrfCookie;
use askama::Template;
use axum::extract::{ConnectInfo, Form, Multipart, Path, Query, State as AxState};
use axum::http::HeaderMap;
use axum::response::{IntoResponse, Redirect};
use axum::Extension;
use serde::Deserialize;
use thepenmarket::money::fmt_cents;
use thepenmarket::stripe;
use thepenmarket::tp_pay::{self, NewListing, TpListing};

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
            {"@type": "Question", "name": "Does ThePenMarket.com guarantee Trading Post items?", "acceptedAnswer": {"@type": "Answer", "text": "No. The merchandise is not guaranteed by ThePenMarket.com, nor does the transaction pass through our hands. We do our best to vet out the bad dealers, and ask that you report any problems immediately to info@thepenmarket.com."}}
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
    let mut l = db::listing(&state.pool, &slug).await?.ok_or(AppError::NotFound)?;
    l.description_html = crate::links::unlink(&l.description_html, &state.site().dead_links);
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
    // A listing with no photo is not marked up as a product: search engines require an image for one.
    let mut nodes = vec![org_node(&state)];
    if !l.image_large.is_empty() {
        nodes.push(serde_json::json!({
            "@type": "Product",
            "@id": state.abs(&format!("/trading-post/{slug}/#listing")),
            "name": l.title,
            "description": desc_text,
            "image": if l.image_large.is_empty() { serde_json::Value::Null } else { serde_json::json!([state.abs(&l.image_large)]) },
            "url": state.abs(&format!("/trading-post/{slug}/")),
            "offers": {"@type": "Offer", "priceCurrency": "USD", "price": l.price.trim_start_matches('$').replace(',', ""), "availability": "https://schema.org/InStock", "itemCondition": "https://schema.org/UsedCondition", "description": "Private classified listing on the ThePenMarket.com Trading Post; the sale is between buyer and seller."}
        }));
    }
    nodes.push(breadcrumb_node(&state, &[("Home", "/"), ("Trading Post", "/trading-post/"), (l.title.as_str(), &format!("/trading-post/{slug}/"))]));
    page = page.with_jsonld(nodes);
    html(&TpListingTpl { page, l, more, tp_email: Entity::TP_EMAIL.to_string() })
}

#[derive(Template)]
#[template(path = "post_your_product.html")]
pub struct PostFormTpl {
    pub page: PageMeta,
    pub csrf: String,
    pub listing_price: String,
    pub unlimited_price: String,
    /// Stripe keys are set: the fee is taken at Stripe Checkout, and the form asks for an e-mail.
    pub checkout_on: bool,
}

pub async fn post_form(AxState(state): AxState<State>, Extension(cookie): Extension<CsrfCookie>) -> AppResult {
    let mut page = PageMeta::new(&state, "Post Your Product on the Trading Post ($5 a Listing) | ThePenMarket.com", "List a pen, pencil or writing ephemera on the ThePenMarket.com Trading Post for $5. Era, price, contact and a photo; you keep the pen until someone pays you.", "/post-your-product/");
    page.section = "trading-post".into();
    page = page.with_jsonld(vec![org_node(&state), breadcrumb_node(&state, &[("Home", "/"), ("Trading Post", "/trading-post/"), ("Post Your Product", "/post-your-product/")])]);
    let csrf = crate::security::csrf_token(&state.cfg.csrf_secret, &cookie.0);
    html(&PostFormTpl { page, csrf, listing_price: fmt_dollars(engine::TRADING_POST_LISTING_CENTS), unlimited_price: fmt_dollars(engine::UNLIMITED_POSTS_YEAR_CENTS), checkout_on: state.cfg.checkout_enabled() })
}

fn done(state: &State, title: &str, heading: &str, lines: Vec<String>, back_url: &str, back_label: &str) -> AppResult {
    let mut page = PageMeta::new(state, &format!("{title} | ThePenMarket.com"), "Your Trading Post listing at ThePenMarket.com.", "/post-your-product/");
    page.noindex = true;
    html(&FormDoneTpl { page, heading: heading.into(), lines, back_url: back_url.into(), back_label: back_label.into() })
}

fn contact_line() -> String {
    format!("Questions: call {} or e-mail {}.", Entity::PHONE, Entity::TP_EMAIL)
}

/// A private upload key as `read_multipart` makes them (hex + .jpg); anything else is not a path.
fn valid_photo_key(k: &str) -> bool {
    k.len() <= 40 && k.ends_with(".jpg") && k.trim_end_matches(".jpg").chars().all(|c| c.is_ascii_hexdigit())
}

/// A listing just went live (fee paid, or a member's): publish its photo from the private uploads
/// into the served media folder (only now, never while it is unpaid) and e-mail him the listing as the
/// form always has, with how the fee was covered.
pub async fn go_live(state: &State, l: &TpListing, fee_line: &str) {
    let mut has_photo = false;
    if let Some(key) = l.photo_key.as_deref().filter(|k| valid_photo_key(k)) {
        match tokio::fs::read(state.cfg.uploads_dir.join(key)).await {
            Ok(bytes) => {
                let rel = format!("uploads/tradingpost/{}/{}-{}.jpg", chrono::Utc::now().format("%Y/%m"), l.id, key.trim_end_matches(".jpg"));
                match thepenmarket::media::reencode_upload_with_variants_async(bytes, state.cfg.media_dir.join(&rel)).await {
                    Ok(_) => {
                        has_photo = true;
                        if let Err(e) = tp_pay::set_image(&state.pool, l.id, &rel).await {
                            tracing::warn!("listing #{}: photo saved but not attached: {e:#}", l.id);
                        }
                    }
                    Err(e) => tracing::warn!("listing #{}: photo could not be published: {e:#}", l.id),
                }
            }
            Err(e) => tracing::warn!("listing #{}: uploaded photo missing: {e}", l.id),
        }
    }
    let Some(sub) = l.form_submission_id else { return };
    let fields: serde_json::Value = match sqlx::query_scalar("SELECT fields FROM form_submission WHERE id = $1").bind(sub).fetch_optional(&state.pool).await {
        Ok(Some(f)) => f,
        _ => serde_json::json!({}),
    };
    let mut fields = fields.as_object().cloned().unwrap_or_default();
    fields.insert("listing_fee".into(), serde_json::Value::String(fee_line.to_string()));
    fields.insert("live_at".into(), serde_json::Value::String(state.abs(&format!("/trading-post/{}/", l.slug))));
    forms::notify(state, "trading_post", sub, &serde_json::Value::Object(fields), has_photo || l.photo_key.is_some());
}

pub async fn post_submit(AxState(state): AxState<State>, Extension(cookie): Extension<CsrfCookie>, ConnectInfo(peer): ConnectInfo<std::net::SocketAddr>, headers: HeaderMap, mp: Multipart) -> AppResult {
    let checkout_on = state.cfg.checkout_enabled();
    let required: &[&str] = if checkout_on { &["item", "era", "price", "contact", "email"] } else { &["item", "era", "price", "contact"] };
    let f = forms::read_multipart(&state, &cookie, peer, &headers, mp, required).await?;
    let id = db::save_form(&state.pool, "trading_post", f.fields.clone(), f.photo_key.clone()).await?;
    if !checkout_on {
        forms::notify(&state, "trading_post", id, &f.fields, f.photo_key.is_some());
        let page = PageMeta::new(&state, "Listing received (demo) | ThePenMarket.com", "Demo submission stored; no payment was taken and nothing was published.", "/post-your-product/");
        return html(&FormDoneTpl {
            page,
            heading: "Listing received, as a demo".into(),
            lines: vec![
                format!("Reference #{id}. This build stores the listing but does not take the $5 payment or publish it; on the live site, Gravity Forms charged the fee and the listing went to review."),
                format!("Item: {}. Era: {}. Price: {}. Contact: {}.", f.get("item"), f.get("era"), f.get("price"), f.get("contact")),
                if f.photo_key.is_some() { "Your photo was received and stored (re-encoded, metadata stripped).".into() } else { "No photo was attached.".into() },
            ],
            back_url: "/trading-post/".into(),
            back_label: "Back to the Trading Post".into(),
        });
    }
    let email = f.get("email");
    let new = NewListing { submission_id: id, item: f.get("item"), era: f.get("era"), price: f.get("price"), contact: f.get("contact"), description: f.get("description"), email: email.clone(), photo_key: f.photo_key.clone() };
    if let Some(until) = tp_pay::member_until(&state.pool, &email).await? {
        let l = tp_pay::create_listing(&state.pool, &new, true, 0).await?;
        let through = until.format("%B %-d, %Y").to_string();
        crate::admin::store::audit(&state.pool, "tp.listing_member", serde_json::json!({"listing_id": l.id, "title": l.title, "email": email, "paid_through": through})).await?;
        go_live(&state, &l, &format!("none: Unlimited Posts member ({email}), paid through {through}")).await;
        let url = format!("/trading-post/{}/", l.slug);
        let notice = crate::admin::mail::member_listing_message(&email, &l.title, &state.abs(&url), &through, &state.cfg.trading_post_to);
        let st = state.clone();
        tokio::spawn(async move {
            if let Err(e) = st.mailer.send(notice).await {
                tracing::warn!("member listing notice could not be e-mailed: {e:#}");
            }
        });
        return done(&state, "Your listing is live", "Your listing is live", vec![
            format!("Reference #{id}. No fee: your Unlimited Posts membership is paid through {through}."),
            format!("Item: {}. Era: {}. Price: {}. Contact: {}.", f.get("item"), f.get("era"), f.get("price"), f.get("contact")),
            contact_line(),
        ], &url, "See your listing");
    }
    let fee = engine::TRADING_POST_LISTING_CENTS;
    let l = tp_pay::create_listing(&state.pool, &new, false, fee).await?;
    let form = stripe::listing_form(l.id, &l.title, &email, fee, state.origin());
    match stripe::create_session(&state.http, &state.cfg.stripe_secret_key, &form, &format!("pm-tp-{}", l.id)).await {
        Ok(s) if s.url.as_deref().is_some_and(|u| u.starts_with("https://")) => {
            tp_pay::attach_session(&state.pool, l.id, &s.id).await?;
            Ok(Redirect::to(&s.url.unwrap_or_default()).into_response())
        }
        other => {
            let why = match other {
                Err(e) => format!("{e:#}"),
                Ok(_) => "Stripe returned a session with no checkout address".into(),
            };
            tracing::warn!(listing = l.id, "listing checkout could not start: {why}");
            tp_pay::note(&state.pool, l.id, &format!("Checkout could not start: {why}")).await?;
            done(&state, "Checkout could not start", "Checkout could not start", vec![
                format!("Reference #{id}. Nothing was charged. Your listing is saved but not published."),
                "Please try again in a few minutes.".into(),
                contact_line(),
            ], "/post-your-product/", "Back to the form")
        }
    }
}

/// `POST /trading-post/membership/`: start an Unlimited Posts membership at Stripe Checkout.
pub async fn membership_start(AxState(state): AxState<State>, Extension(cookie): Extension<CsrfCookie>, ConnectInfo(peer): ConnectInfo<std::net::SocketAddr>, headers: HeaderMap, Form(form): Form<super::checkout::BuyForm>) -> AppResult {
    if !state.form_limiter.allow(crate::security::client_ip(&headers, peer.ip())) {
        return Err(AppError::Forbidden("too many tries from this address; wait a minute and try again".into()));
    }
    if !crate::security::csrf_ok(&state.cfg.csrf_secret, &cookie.0, &form.csrf) {
        return Err(AppError::Forbidden("the page expired; go back, reload it and try again".into()));
    }
    let back = "/product/unlimited-posts/";
    if !state.cfg.checkout_enabled() {
        return Ok(Redirect::to(back).into_response());
    }
    let form = stripe::membership_form(engine::UNLIMITED_POSTS_YEAR_CENTS, state.origin());
    let key = format!("pm-member-{}", crate::admin::auth::random_token(12));
    match stripe::create_session(&state.http, &state.cfg.stripe_secret_key, &form, &key).await {
        Ok(s) if s.url.as_deref().is_some_and(|u| u.starts_with("https://")) => Ok(Redirect::to(&s.url.unwrap_or_default()).into_response()),
        other => {
            if let Err(e) = other {
                tracing::warn!("membership checkout could not start: {e:#}");
            }
            done(&state, "Checkout could not start", "Checkout could not start", vec!["Nothing was charged.".into(), "Please try again in a few minutes.".into(), contact_line()], back, "Back")
        }
    }
}

#[derive(Deserialize)]
pub struct ThanksQuery {
    #[serde(default)]
    pub session_id: String,
}

/// `GET /trading-post/thanks/?session_id=…`: what happened to a listing fee or a membership.
pub async fn thanks(AxState(state): AxState<State>, Query(q): Query<ThanksQuery>) -> AppResult {
    if !stripe::valid_session_id(&q.session_id) {
        return Err(AppError::NotFound);
    }
    let session = || async {
        if state.cfg.stripe_secret_key.is_empty() {
            None
        } else {
            stripe::retrieve_session(&state.http, &state.cfg.stripe_secret_key, &q.session_id).await.map_err(|e| tracing::warn!("trading post thanks could not read the session: {e:#}")).ok()
        }
    };
    if let Some(l) = tp_pay::listing_by_session(&state.pool, &q.session_id).await? {
        let what = format!("Listing #{}: {}.", l.id, l.title);
        let url = format!("/trading-post/{}/", l.slug);
        if l.status == "live" {
            return done(&state, "Your listing is live", "Thank you. Your listing is live", vec![what, l.amount_total_cents.map(|c| format!("Paid: {}.", fmt_cents(c))).unwrap_or_default(), contact_line()], &url, "See your listing");
        }
        if l.note.contains("refunded") {
            return done(&state, "Payment refunded", "Your payment was refunded", vec![what, "The listing had been taken down before your payment arrived, so the fee was refunded in full.".into(), contact_line()], "/trading-post/", "Back to the Trading Post");
        }
        let s = session().await;
        if s.as_ref().is_some_and(|s| s.paid()) {
            return done(&state, "Payment received", "Thank you. Your payment went through", vec![what, "Your listing goes live as soon as Stripe confirms the payment, usually within a minute.".into(), contact_line()], &url, "See your listing");
        }
        if l.note.starts_with("Bank payment") || s.as_ref().is_some_and(|s| s.status.as_deref() == Some("complete")) {
            return done(&state, "Payment processing", "Your bank payment is processing", vec![what, "Your listing goes live when the payment clears.".into(), contact_line()], "/trading-post/", "Back to the Trading Post");
        }
        return done(&state, "Not completed", "This checkout was not completed", vec![what, "Nothing was charged, and the listing is not published.".into(), contact_line()], "/post-your-product/", "Back to the form");
    }
    let member = tp_pay::member_by_session(&state.pool, &q.session_id).await?;
    let s = session().await;
    let is_membership = member.is_some() || s.as_ref().is_some_and(|s| s.kind() == stripe::KIND_MEMBERSHIP);
    if !is_membership {
        return Err(AppError::NotFound);
    }
    if let Some(m) = member {
        return done(&state, "Unlimited Posts", "Thank you. Your Unlimited Posts membership is active", vec![
            format!("Paid through {}, for {}.", m.paid_through.format("%B %-d, %Y"), m.email),
            "Post with that e-mail address and your listings go live without the listing fee.".into(),
            contact_line(),
        ], "/post-your-product/", "Post your product");
    }
    if let Some(s) = s.filter(|s| s.paid()) {
        return done(&state, "Unlimited Posts", "Thank you. Your payment went through", vec![
            format!("Your Unlimited Posts membership is being recorded for {}.", s.email()),
            "Post with that e-mail address and your listings go live without the listing fee.".into(),
            contact_line(),
        ], "/post-your-product/", "Post your product");
    }
    done(&state, "Not completed", "This checkout was not completed", vec!["Nothing was charged.".into(), contact_line()], "/product/unlimited-posts/", "Back")
}
