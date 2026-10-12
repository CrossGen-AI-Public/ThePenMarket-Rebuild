//! Buying a pen: "Buy this pen" holds it and sends the buyer to Stripe's own checkout page; the
//! webhook marks it sold; the thanks page reports what Stripe says happened. With no Stripe keys
//! configured none of this is reachable from the pages and the product page keeps its demo notice.

use crate::app::{html, AppError, AppResult, Entity, PageMeta, State};
use crate::security::{self, CsrfCookie};
use askama::Template;
use axum::body::Bytes;
use axum::extract::{ConnectInfo, Form, Path, Query, State as AxState};
use axum::http::{HeaderMap, StatusCode};
use axum::response::{IntoResponse, Redirect, Response};
use axum::Extension;
use serde::Deserialize;
use std::net::SocketAddr;
use std::sync::Arc;
use thepenmarket::money::fmt_cents;
use thepenmarket::orders::{self, Hold, OrderRow, Outcome};
use thepenmarket::shipping::{self, Region};
use thepenmarket::stripe;
use thepenmarket::tp_pay::{self, ListingOutcome, MemberOutcome};

/// "Is this hold mine?" without storing the buyer's cookie itself.
pub fn visitor_key(cookie: &CsrfCookie) -> String {
    crate::admin::auth::sha256_hex(&format!("checkout:{}", cookie.0))
}

#[derive(Template)]
#[template(path = "checkout_done.html")]
pub struct CheckoutDoneTpl {
    pub page: PageMeta,
    pub eyebrow: String,
    pub heading: String,
    pub lines: Vec<String>,
    pub back_url: String,
    pub back_label: String,
}

fn done(state: &State, path: &str, eyebrow: &str, heading: &str, lines: Vec<String>, back_url: &str, back_label: &str) -> AppResult {
    let mut page = PageMeta::new(state, &format!("{heading} | ThePenMarket.com"), "Your order at ThePenMarket.com.", path);
    page.noindex = true;
    html(&CheckoutDoneTpl { page, eyebrow: eyebrow.into(), heading: heading.into(), lines, back_url: back_url.into(), back_label: back_label.into() })
}

fn contact_line(sku: &str) -> String {
    format!("Questions: call {} or e-mail {}{}.", Entity::PHONE, Entity::EMAIL, if sku.is_empty() { String::new() } else { format!(" and mention SKU {sku}") })
}

const MAX_HOLDS: i64 = 2;

#[derive(Deserialize)]
pub struct BuyForm {
    pub csrf: String,
    /// Where the pen ships: "us" (the default) or "intl".
    #[serde(default)]
    pub ship: String,
}

/// `POST /checkout/{slug}/`: hold the pen and open a Stripe Checkout Session for it.
pub async fn buy(AxState(state): AxState<State>, Extension(cookie): Extension<CsrfCookie>, ConnectInfo(peer): ConnectInfo<SocketAddr>, headers: HeaderMap, Path(slug): Path<String>, Form(form): Form<BuyForm>) -> AppResult {
    if !state.form_limiter.allow(security::client_ip(&headers, peer.ip())) {
        return Err(AppError::Forbidden("too many tries from this address; wait a minute and try again".into()));
    }
    if !security::csrf_ok(&state.cfg.csrf_secret, &cookie.0, &form.csrf) {
        return Err(AppError::Forbidden("the page expired; go back, reload it and press Buy again".into()));
    }
    let back = format!("/product/{slug}/");
    let Some(pen) = orders::for_sale(&state.pool, &slug).await? else { return Err(AppError::NotFound) };
    if !state.cfg.checkout_enabled() || !pen.buyable() {
        // The product page explains why (sold, not for sale, or checkout not connected).
        return Ok(Redirect::to(&back).into_response());
    }
    let (ship_list, _) = crate::settings::shipping_options(&state.pool).await?;
    let region = if form.ship == "intl" { Region::Intl } else { Region::Us };
    if region == Region::Intl && !shipping::ships_abroad(&ship_list) {
        // He has no rate for abroad; the page does not offer it.
        return Ok(Redirect::to(&back).into_response());
    }
    let offered = shipping::for_region(&ship_list, region);
    let now = chrono::Utc::now().timestamp();
    let session_expires = now + stripe::SESSION_MINUTES * 60;
    let hold_until = session_expires + stripe::HOLD_GRACE_MINUTES * 60;
    // Two pens at a time per visitor: enough for anyone buying, too few to hold the whole case hostage.
    let ip_key = crate::admin::auth::sha256_hex(&format!("hold-ip:{}", security::client_ip(&headers, peer.ip())));
    if orders::holds_elsewhere(&state.pool, &visitor_key(&cookie), &ip_key, pen.id).await? >= MAX_HOLDS {
        return done(&state, &back, "Checkout", "Finish your other checkout first", vec![
            format!("You are already checking out {MAX_HOLDS} pens. Pay for them, or let one go, and then come back for this one."),
            contact_line(&pen.sku),
        ], &back, "Back to the pen");
    }
    let key = visitor_key(&cookie);
    let mut hold = orders::place_hold(&state.pool, &pen, &key, region, &offered, hold_until).await?;
    if matches!(&hold, Hold::Yours { region: r, .. } if r != region.key()) {
        // The same buyer, now shipping somewhere else: close the old checkout (its rates were for
        // the other place) and open a new one.
        if let Some(sid) = orders::release(&state.pool, pen.id, &key).await? {
            if let Err(e) = stripe::expire_session(&state.http, &state.cfg.stripe_secret_key, &sid).await {
                tracing::warn!("could not close the old checkout session: {e:#}");
            }
        }
        hold = orders::place_hold(&state.pool, &pen, &key, region, &offered, hold_until).await?;
    }
    let order_id = match hold {
        Hold::Taken => return Ok(Redirect::to(&back).into_response()),
        Hold::Yours { url, .. } => return Ok(Redirect::to(&url).into_response()),
        Hold::Placed(id) => id,
    };
    orders::set_holder_ip(&state.pool, order_id, &ip_key).await?;
    let item = stripe::CheckoutItem {
        product_id: pen.id,
        order_id,
        name: pen.title.clone(),
        sku: pen.sku.clone(),
        slug: pen.slug.clone(),
        unit_amount_cents: pen.amount_cents,
        image_url: if pen.image.is_empty() { String::new() } else { state.abs(&format!("/media/{}", pen.image)) },
    };
    let form = stripe::checkout_form(&item, state.origin(), session_expires, region, &offered);
    match stripe::create_session(&state.http, &state.cfg.stripe_secret_key, &form, &format!("pm-order-{order_id}")).await {
        Ok(s) if s.url.as_deref().is_some_and(|u| u.starts_with("https://")) => {
            let url = s.url.unwrap_or_default();
            orders::attach_session(&state.pool, order_id, &s.id, &url).await?;
            Ok(Redirect::to(&url).into_response())
        }
        other => {
            let why = match other {
                Err(e) => format!("{e:#}"),
                Ok(_) => "Stripe returned a session with no checkout address".into(),
            };
            tracing::warn!(order = order_id, "checkout could not start: {why}");
            orders::fail_hold(&state.pool, order_id, &why).await?;
            done(&state, &back, "Checkout", "Checkout could not start", vec![
                "Nothing was charged, and the pen is still for sale.".into(),
                "Please try again in a few minutes.".into(),
                contact_line(&pen.sku),
            ], &back, "Back to the pen")
        }
    }
}

/// `POST /checkout/{slug}/release/`: the buyer lets go of a pen they were checking out.
pub async fn release(AxState(state): AxState<State>, Extension(cookie): Extension<CsrfCookie>, ConnectInfo(peer): ConnectInfo<SocketAddr>, headers: HeaderMap, Path(slug): Path<String>, Form(form): Form<BuyForm>) -> AppResult {
    if !state.form_limiter.allow(security::client_ip(&headers, peer.ip())) {
        return Err(AppError::Forbidden("too many tries from this address; wait a minute and try again".into()));
    }
    if !security::csrf_ok(&state.cfg.csrf_secret, &cookie.0, &form.csrf) {
        return Err(AppError::Forbidden("the page expired; go back, reload it and try again".into()));
    }
    let Some(pen) = orders::for_sale(&state.pool, &slug).await? else { return Err(AppError::NotFound) };
    if let Some(sid) = orders::release(&state.pool, pen.id, &visitor_key(&cookie)).await? {
        if state.cfg.checkout_enabled() {
            // Close it at Stripe too, so the old checkout page cannot take a payment for a pen that is free again.
            if let Err(e) = stripe::expire_session(&state.http, &state.cfg.stripe_secret_key, &sid).await {
                tracing::warn!("could not close checkout session after release: {e:#}");
            }
        }
    }
    Ok(Redirect::to(&format!("/product/{slug}/")).into_response())
}

#[derive(Deserialize)]
pub struct ThanksQuery {
    #[serde(default)]
    pub session_id: String,
}

fn ship_to(o: &OrderRow) -> String {
    let Some(a) = &o.shipping_address else { return String::new() };
    let g = |k: &str| a.get(k).and_then(|v| v.as_str()).unwrap_or("").trim().to_string();
    let country = g("country");
    let parts: Vec<String> = [g("name"), g("line1"), g("line2"), format!("{} {} {}", g("city"), g("state"), g("postal_code")).trim().to_string(), if country == "US" { String::new() } else { country }].into_iter().filter(|s| !s.is_empty()).collect();
    parts.join(", ")
}

/// The shipping on an order: from the order once the webhook stored it, else from the session Stripe
/// returned to the thanks page. "USPS Insured Priority, $15.00", or "" when none was charged.
fn shipping_text(o: &OrderRow, s: Option<&stripe::Session>) -> String {
    let label = o.shipping_label();
    if !label.is_empty() {
        return label;
    }
    let Some(s) = s else { return String::new() };
    let Some(c) = s.shipping_charged().filter(|c| *c > 0) else { return String::new() };
    let name = s.shipping_rate_amount().map(|a| shipping::chosen_name(&o.shipping_offered, a)).unwrap_or_default();
    if name.is_empty() { fmt_cents(c) } else { format!("{name}, {}", fmt_cents(c)) }
}

/// `GET /checkout/thanks/?session_id=…`: what Stripe says happened to this checkout.
pub async fn thanks(AxState(state): AxState<State>, Query(q): Query<ThanksQuery>) -> AppResult {
    if !stripe::valid_session_id(&q.session_id) {
        return Err(AppError::NotFound);
    }
    let Some(order) = orders::by_session(&state.pool, &q.session_id).await? else { return Err(AppError::NotFound) };
    let session = if state.cfg.stripe_secret_key.is_empty() { None } else { stripe::retrieve_session(&state.http, &state.cfg.stripe_secret_key, &q.session_id).await.map_err(|e| tracing::warn!("thanks page could not read the session: {e:#}")).ok() };
    let path = "/checkout/thanks/";
    let product_url = format!("/product/{}/", order.product_slug);
    let total = session.as_ref().and_then(|s| s.amount_total).or(order.amount_total_cents).unwrap_or(order.amount_cents + order.shipping_cents);
    let what = format!("Order #{}: {}{}.", order.id, order.product_title, if order.product_sku.is_empty() { String::new() } else { format!(" (SKU {})", order.product_sku) });
    let ship_text = shipping_text(&order, session.as_ref());
    let paid_line = if ship_text.is_empty() { format!("Total: {}.", fmt_cents(total)) } else { format!("Total: {}, including shipping ({ship_text}).", fmt_cents(total)) };
    let mut ship = ship_to(&order);
    if ship.is_empty() {
        if let Some(s) = session.as_ref().and_then(|s| s.shipping_json()) {
            ship = ship_to(&OrderRow { shipping_address: Some(s), ..order.clone() });
        }
    }
    let ship_line = if ship.is_empty() { String::new() } else { format!("Shipping to: {ship}.") };
    let paid = order.status == "paid" || (order.status != "refunded" && session.as_ref().is_some_and(|s| s.paid()));
    let processing = order.status == "processing" || session.as_ref().is_some_and(|s| s.status.as_deref() == Some("complete") && !s.paid());
    if order.status == "refunded" {
        return done(&state, path, "Order", "This pen was already sold", vec![
            what,
            "Your payment went through after the pen had sold, so it has been refunded in full to the card or account you paid with.".into(),
            contact_line(&order.product_sku),
        ], "/shop/", "Back to the shop");
    }
    if paid {
        return done(&state, path, "Order", "Thank you. Your payment went through", vec![what, paid_line, ship_line, contact_line(&order.product_sku)], &product_url, "Back to the pen");
    }
    if processing {
        return done(&state, path, "Order", "Your bank payment is processing", vec![
            what,
            paid_line,
            "The pen is held for you while your bank payment clears.".into(),
            ship_line,
            contact_line(&order.product_sku),
        ], &product_url, "Back to the pen");
    }
    done(&state, path, "Order", "This checkout was not completed", vec![what, "Nothing was charged.".into(), contact_line(&order.product_sku)], &product_url, "Back to the pen")
}

/// `POST /api/stripe/webhook`: Stripe's signed account events. Every answer other than 2xx makes
/// Stripe retry, so failures roll back and return 500; bad signatures get 400 and are never retried
/// into success.
pub async fn webhook(AxState(state): AxState<State>, headers: HeaderMap, body: Bytes) -> Response {
    if state.cfg.stripe_webhook_secret.is_empty() {
        return (StatusCode::SERVICE_UNAVAILABLE, "checkout is not configured").into_response();
    }
    let sig = headers.get("stripe-signature").and_then(|v| v.to_str().ok());
    if let Err(e) = stripe::verify_signature(sig, &body, &state.cfg.stripe_webhook_secret, chrono::Utc::now().timestamp(), stripe::SIGNATURE_TOLERANCE_SECS) {
        tracing::warn!("stripe webhook rejected: {e}");
        return (StatusCode::BAD_REQUEST, "bad signature").into_response();
    }
    let event: stripe::Event = match serde_json::from_slice(&body) {
        Ok(e) => e,
        Err(_) => return (StatusCode::BAD_REQUEST, "unreadable event").into_response(),
    };
    match handle_event(&state, &event).await {
        Ok(()) => (StatusCode::OK, "ok").into_response(),
        Err(e) => {
            tracing::error!(event = %event.id, kind = %event.kind, "stripe webhook failed: {e:#}");
            (StatusCode::INTERNAL_SERVER_ERROR, "failed").into_response()
        }
    }
}

/// What one webhook event did, by what it was for.
enum Handled {
    Pen(Outcome),
    Listing(ListingOutcome),
    Member(MemberOutcome),
}

async fn handle_event(state: &State, event: &stripe::Event) -> anyhow::Result<()> {
    let mut tx = state.pool.begin().await?;
    if !orders::record_event(&mut tx, &event.id, &event.kind).await? {
        return Ok(()); // already handled
    }
    let obj = &event.data.object;
    let session = if event.kind.starts_with("checkout.session.") { stripe::Session::from_value(obj) } else { None };
    let kind = event.kind.as_str();
    let out = match &session {
        Some(s) if s.kind() == stripe::KIND_LISTING => Handled::Listing(match kind {
            "checkout.session.completed" => tp_pay::on_listing_paid(&mut tx, s, false).await?,
            "checkout.session.async_payment_succeeded" => tp_pay::on_listing_paid(&mut tx, s, true).await?,
            "checkout.session.async_payment_failed" => tp_pay::on_listing_unpaid(&mut tx, s, "The bank payment failed; the listing was not published.").await?,
            "checkout.session.expired" => tp_pay::on_listing_unpaid(&mut tx, s, "Checkout closed without payment; the listing was not published.").await?,
            _ => ListingOutcome::Ignored("not an event the Trading Post uses"),
        }),
        Some(s) if s.kind() == stripe::KIND_MEMBERSHIP => Handled::Member(match kind {
            "checkout.session.completed" | "checkout.session.async_payment_succeeded" => tp_pay::on_membership_completed(&mut tx, s).await?,
            _ => MemberOutcome::Ignored("membership checkout not completed"),
        }),
        Some(s) => Handled::Pen(match kind {
            "checkout.session.completed" => orders::on_completed(&mut tx, s).await?,
            "checkout.session.async_payment_succeeded" => orders::on_async_succeeded(&mut tx, s).await?,
            "checkout.session.async_payment_failed" => orders::on_async_failed(&mut tx, s).await?,
            "checkout.session.expired" => orders::on_expired(&mut tx, s).await?,
            _ => Outcome::Ignored("not an event the shop uses"),
        }),
        None => match kind {
            "invoice.paid" => Handled::Member(tp_pay::on_invoice_paid(&mut tx, &stripe::invoice_paid(obj)).await?),
            "customer.subscription.deleted" => Handled::Member(tp_pay::on_subscription_ended(&mut tx, &stripe::subscription_ended(obj)).await?),
            _ => Handled::Pen(Outcome::Ignored("not an event the shop uses")),
        },
    };
    // Refunds go out before committing: if one fails, the event is not recorded and Stripe retries.
    match &out {
        Handled::Pen(Outcome::Refund { order, payment_intent }) => {
            if payment_intent.is_empty() {
                sqlx::query("UPDATE shop_order SET status = 'failed', note = note || ' Stripe sent no payment id, so nothing was refunded automatically: refund it by hand in the Stripe dashboard.' WHERE id = $1").bind(order.id).execute(&mut *tx).await?;
            } else {
                stripe::refund(&state.http, &state.cfg.stripe_secret_key, payment_intent, &format!("pm-refund-{}", order.id)).await?;
            }
        }
        Handled::Listing(ListingOutcome::Refund { listing, payment_intent }) => {
            if payment_intent.is_empty() {
                sqlx::query("UPDATE tp_listing SET note = note || ' Stripe sent no payment id, so nothing was refunded automatically: refund it by hand in the Stripe dashboard.' WHERE id = $1").bind(listing.id).execute(&mut *tx).await?;
            } else {
                stripe::refund(&state.http, &state.cfg.stripe_secret_key, payment_intent, &format!("pm-tp-refund-{}", listing.id)).await?;
            }
        }
        _ => {}
    }
    tx.commit().await?;
    match out {
        Handled::Pen(o) => after(state, event, o).await,
        Handled::Listing(o) => after_listing(state, event, o).await,
        Handled::Member(o) => after_member(state, event, o).await,
    }
    Ok(())
}

/// A paid listing goes live: its photo is published, the log and his mail hear about it.
async fn after_listing(state: &State, event: &stripe::Event, out: ListingOutcome) {
    match out {
        ListingOutcome::Live(l) => {
            if let Err(e) = crate::admin::store::audit(&state.pool, "tp.listing_paid", serde_json::json!({"listing_id": l.id, "title": l.title, "amount_total_cents": l.amount_total_cents, "event": event.id, "note": l.note})).await {
                tracing::warn!("listing #{} audit failed: {e:#}", l.id);
            }
            let fee = format!("{} paid through Stripe{}", l.amount_total_cents.map(fmt_cents).unwrap_or_default(), if l.note.is_empty() { String::new() } else { format!(" ({})", l.note.trim()) });
            super::trading_post::go_live(state, &l, &fee).await;
        }
        ListingOutcome::Refund { listing, .. } => {
            let _ = crate::admin::store::audit(&state.pool, "tp.listing_refunded", serde_json::json!({"listing_id": listing.id, "title": listing.title, "event": event.id, "note": listing.note})).await;
            let lines = vec![("Listing".to_string(), format!("#{} {}", listing.id, listing.title)), ("Poster e-mail".into(), listing.poster_email.clone()), ("Note".into(), listing.note.clone())];
            let msg = crate::admin::mail::order_message(&state.cfg.trading_post_to, &format!("Refunded: Trading Post fee for {}", listing.title), &lines, &listing.poster_email, &state.abs("/admin/orders/#trading-post"));
            let state = Arc::clone(state);
            tokio::spawn(async move {
                if let Err(e) = state.mailer.send(msg).await {
                    tracing::warn!("listing refund notice could not be e-mailed: {e:#}");
                }
            });
        }
        ListingOutcome::Processing(l) | ListingOutcome::Unpaid(l) => tracing::info!(event = %event.id, listing = l.id, "trading post listing not paid yet: {}", l.note),
        ListingOutcome::Ignored(why) => tracing::info!(event = %event.id, kind = %event.kind, "stripe event ignored: {why}"),
    }
}

/// Memberships: the log always; his mail when someone joins.
async fn after_member(state: &State, event: &stripe::Event, out: MemberOutcome) {
    let (kind, m) = match out {
        MemberOutcome::Joined(m) => ("tp.membership_joined", m),
        MemberOutcome::Renewed(m) => ("tp.membership_renewed", m),
        MemberOutcome::Ended(m) => ("tp.membership_ended", m),
        MemberOutcome::Ignored(why) => {
            tracing::info!(event = %event.id, kind = %event.kind, "stripe event ignored: {why}");
            return;
        }
    };
    let through = m.paid_through.format("%B %-d, %Y").to_string();
    if let Err(e) = crate::admin::store::audit(&state.pool, kind, serde_json::json!({"email": m.email, "paid_through": through, "subscription": m.stripe_subscription, "event": event.id})).await {
        tracing::warn!("membership audit failed: {e:#}");
    }
    if kind == "tp.membership_joined" && event.kind.starts_with("checkout.session.") {
        let lines = vec![("Member e-mail".to_string(), m.email.clone()), ("Paid through".into(), through), ("Stripe subscription".into(), m.stripe_subscription.clone())];
        let msg = crate::admin::mail::order_message(&state.cfg.trading_post_to, &format!("New Unlimited Posts member: {}", m.email), &lines, &m.email, &state.abs("/admin/orders/#members"));
        let state = Arc::clone(state);
        tokio::spawn(async move {
            if let Err(e) = state.mailer.send(msg).await {
                tracing::warn!("membership notice could not be e-mailed: {e:#}");
            }
        });
    }
}

/// What follows a committed event: the catalog forgets a sold pen, the log and his mail hear about it.
async fn after(state: &State, event: &stripe::Event, out: Outcome) {
    let (kind, subject, o) = match out {
        Outcome::Sold(o) => {
            refresh_catalog(state).await;
            ("shop.sold", format!("Sold online: {}", o.product_title), o)
        }
        Outcome::Refund { order, .. } => ("shop.refunded", format!("Refunded: {} was paid for after it sold", order.product_title), order),
        Outcome::Processing(o) => ("shop.payment_processing", format!("Bank payment started: {}", o.product_title), o),
        Outcome::Released => return,
        Outcome::Ignored(why) => {
            tracing::info!(event = %event.id, kind = %event.kind, "stripe event ignored: {why}");
            return;
        }
    };
    let detail = serde_json::json!({"order_id": o.id, "product_id": o.product_id, "pen": o.product_title, "sku": o.product_sku, "amount_total_cents": o.amount_total_cents, "shipping_cents": o.shipping_cents, "shipping_option": o.shipping_option, "ship_region": o.ship_region, "event": event.id, "note": o.note});
    if let Err(e) = crate::admin::store::audit(&state.pool, kind, detail).await {
        tracing::warn!("order #{} audit failed: {e:#}", o.id);
    }
    let shipping = o.shipping_label();
    let lines = vec![
        ("Order".to_string(), format!("#{}", o.id)),
        ("Pen".into(), o.product_title.clone()),
        ("SKU".into(), o.product_sku.clone()),
        ("Total charged".into(), o.amount_total_cents.map(fmt_cents).unwrap_or_default()),
        ("Ships".into(), if o.ship_region == "intl" { "Outside the US".into() } else { "In the US".into() }),
        ("Shipping charged".into(), if shipping.is_empty() { "none".into() } else { shipping }),
        ("Buyer".into(), o.buyer_name.clone()),
        ("Buyer e-mail".into(), o.buyer_email.clone()),
        ("Ship to".into(), ship_to(&o)),
        ("Stripe payment".into(), o.stripe_payment_intent.clone()),
        ("Note".into(), o.note.trim().to_string()),
    ];
    let msg = crate::admin::mail::order_message(&state.cfg.forms_to, &subject, &lines, &o.buyer_email, &state.abs("/admin/orders/"));
    let buyer = buyer_message(state, kind, &o).await;
    let state = Arc::clone(state);
    tokio::spawn(async move {
        if let Err(e) = state.mailer.send(msg).await {
            tracing::warn!("order #{} could not be e-mailed: {e:#}", o.id);
        }
        if let Some(b) = buyer {
            match state.mailer.send(b).await {
                Ok(()) => tracing::info!("order #{}: buyer e-mailed", o.id),
                Err(e) => tracing::warn!("order #{}: buyer could not be e-mailed: {e:#}", o.id),
            }
        }
    });
}

/// The buyer's own e-mail: the receipt with the pen's photo and description once it is sold to them, or
/// word that their payment went back when the pen had already sold. Nothing for a bank payment still
/// clearing (Stripe tells them about that).
async fn buyer_message(state: &State, kind: &str, o: &OrderRow) -> Option<crate::admin::mail::Message> {
    let to = o.buyer_email.trim();
    if !to.contains('@') || to.contains(['\r', '\n', ',', ' ']) {
        return None;
    }
    match kind {
        "shop.sold" => {
            let image = match crate::db::product_images(&state.pool, o.product_id).await {
                Ok(imgs) => imgs.first().map(|i| state.abs(&i.large)).unwrap_or_default(),
                Err(_) => String::new(),
            };
            if !image.is_empty() {
                let _ = sqlx::query("UPDATE shop_order SET product_image = $2 WHERE id = $1").bind(o.id).bind(&image).execute(&state.pool).await;
            }
            let r = crate::admin::mail::Receipt {
                order_id: o.id,
                pen: o.product_title.clone(),
                sku: o.product_sku.clone(),
                total: o.amount_total_cents.map(fmt_cents).unwrap_or_default(),
                shipping: { let l = o.shipping_label(); if l.is_empty() { l } else { format!("{l} (included)") } },
                ship_to: ship_to(o),
                description: o.product_description.clone(),
                image_url: image,
                listing_url: state.abs(&format!("/product/{}/", o.product_slug)),
                phone: Entity::PHONE.into(),
                email: Entity::EMAIL.into(),
            };
            Some(crate::admin::mail::receipt_message(to, &r))
        }
        "shop.refunded" if o.status == "refunded" => Some(crate::admin::mail::refund_message(to, o.id, &o.product_title, &o.amount_total_cents.map(fmt_cents).unwrap_or_default(), Entity::PHONE, Entity::EMAIL)),
        _ => None,
    }
}

async fn refresh_catalog(state: &State) {
    match crate::catalog::load(&state.pool).await {
        Ok(c) => {
            if let Ok(mut w) = state.catalog.write() {
                *w = Arc::new(c);
            }
        }
        Err(e) => tracing::warn!("catalog refresh after a sale failed: {e:#}"),
    }
}
