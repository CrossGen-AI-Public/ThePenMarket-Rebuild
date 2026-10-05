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
use thepenmarket::stripe;

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

#[derive(Deserialize)]
pub struct BuyForm {
    pub csrf: String,
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
    let shipping = crate::settings::shipping_flat(&state.pool).await?.unwrap_or(0);
    let now = chrono::Utc::now().timestamp();
    let session_expires = now + stripe::SESSION_MINUTES * 60;
    let hold_until = session_expires + stripe::HOLD_GRACE_MINUTES * 60;
    let order_id = match orders::place_hold(&state.pool, &pen, &visitor_key(&cookie), shipping, hold_until).await? {
        Hold::Taken => return Ok(Redirect::to(&back).into_response()),
        Hold::Yours { url, .. } => return Ok(Redirect::to(&url).into_response()),
        Hold::Placed(id) => id,
    };
    let item = stripe::CheckoutItem {
        product_id: pen.id,
        order_id,
        name: pen.title.clone(),
        sku: pen.sku.clone(),
        slug: pen.slug.clone(),
        unit_amount_cents: pen.amount_cents,
        image_url: if pen.image.is_empty() { String::new() } else { state.abs(&format!("/media/{}", pen.image)) },
    };
    let form = stripe::checkout_form(&item, state.origin(), session_expires, Some(shipping));
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
    let parts: Vec<String> = [g("name"), g("line1"), g("line2"), format!("{} {} {}", g("city"), g("state"), g("postal_code")).trim().to_string()].into_iter().filter(|s| !s.is_empty()).collect();
    parts.join(", ")
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
    let paid_line = if order.shipping_cents > 0 { format!("Total: {}, including {} shipping.", fmt_cents(total), fmt_cents(order.shipping_cents)) } else { format!("Total: {}.", fmt_cents(total)) };
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

async fn handle_event(state: &State, event: &stripe::Event) -> anyhow::Result<()> {
    let mut tx = state.pool.begin().await?;
    if !orders::record_event(&mut tx, &event.id, &event.kind).await? {
        return Ok(()); // already handled
    }
    let session = if event.kind.starts_with("checkout.session.") { stripe::Session::from_value(&event.data.object) } else { None };
    let out = match (event.kind.as_str(), &session) {
        ("checkout.session.completed", Some(s)) => orders::on_completed(&mut tx, s).await?,
        ("checkout.session.async_payment_succeeded", Some(s)) => orders::on_async_succeeded(&mut tx, s).await?,
        ("checkout.session.async_payment_failed", Some(s)) => orders::on_async_failed(&mut tx, s).await?,
        ("checkout.session.expired", Some(s)) => orders::on_expired(&mut tx, s).await?,
        _ => Outcome::Ignored("not an event the shop uses"),
    };
    if let Outcome::Refund { order, payment_intent } = &out {
        if payment_intent.is_empty() {
            sqlx::query("UPDATE shop_order SET status = 'failed', note = note || ' Stripe sent no payment id, so nothing was refunded automatically: refund it by hand in the Stripe dashboard.' WHERE id = $1").bind(order.id).execute(&mut *tx).await?;
        } else {
            // Before committing: if the refund fails, the event is not recorded and Stripe retries.
            stripe::refund(&state.http, &state.cfg.stripe_secret_key, payment_intent, &format!("pm-refund-{}", order.id)).await?;
        }
    }
    tx.commit().await?;
    after(state, event, out).await;
    Ok(())
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
    let detail = serde_json::json!({"order_id": o.id, "product_id": o.product_id, "pen": o.product_title, "sku": o.product_sku, "amount_total_cents": o.amount_total_cents, "event": event.id, "note": o.note});
    if let Err(e) = crate::admin::store::audit(&state.pool, kind, detail).await {
        tracing::warn!("order #{} audit failed: {e:#}", o.id);
    }
    let lines = vec![
        ("Order".to_string(), format!("#{}", o.id)),
        ("Pen".into(), o.product_title.clone()),
        ("SKU".into(), o.product_sku.clone()),
        ("Total charged".into(), o.amount_total_cents.map(fmt_cents).unwrap_or_default()),
        ("Shipping charged".into(), if o.shipping_cents > 0 { fmt_cents(o.shipping_cents) } else { "none".into() }),
        ("Buyer".into(), o.buyer_name.clone()),
        ("Buyer e-mail".into(), o.buyer_email.clone()),
        ("Ship to".into(), ship_to(&o)),
        ("Stripe payment".into(), o.stripe_payment_intent.clone()),
        ("Note".into(), o.note.trim().to_string()),
    ];
    let msg = crate::admin::mail::order_message(&state.cfg.forms_to, &subject, &lines, &o.buyer_email, &state.abs("/admin/orders/"));
    let state = Arc::clone(state);
    tokio::spawn(async move {
        if let Err(e) = state.mailer.send(msg).await {
            tracing::warn!("order #{} could not be e-mailed: {e:#}", o.id);
        }
    });
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
