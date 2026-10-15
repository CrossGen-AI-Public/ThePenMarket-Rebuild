//! Stripe, the parts that do not need the web server: webhook signature checks, the form a hosted
//! Checkout Session is created from, the event and session shapes the shop reads, and the four REST
//! calls it makes. Plain form-encoded requests over the shared reqwest client; no Stripe SDK, no
//! Stripe.js on our pages (Checkout is Stripe's own page). Keys are never logged.

use hmac::{Hmac, Mac};
use serde::Deserialize;
use sha2::Sha256;
use crate::shipping::{Region, ShippingOption};

pub const API: &str = "https://api.stripe.com/v1";
/// How old a signed webhook may be. Stripe's own libraries use five minutes.
pub const SIGNATURE_TOLERANCE_SECS: i64 = 300;
/// Stripe needs a Checkout Session to stay open at least 30 minutes; one more covers clock drift.
pub const SESSION_MINUTES: i64 = 31;
/// The hold on a one-of-a-kind pen outlives its session a little, so a payment finished in the last
/// second still finds the pen held when the webhook arrives.
pub const HOLD_GRACE_MINUTES: i64 = 5;

#[derive(Debug, PartialEq, Eq)]
pub enum SigError {
    Missing,
    Malformed,
    Stale,
    Mismatch,
}

impl std::fmt::Display for SigError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(match self {
            SigError::Missing => "no Stripe-Signature header",
            SigError::Malformed => "Stripe-Signature header could not be read",
            SigError::Stale => "signature timestamp outside the tolerance",
            SigError::Mismatch => "signature does not match",
        })
    }
}

fn sign(secret: &str, timestamp: &str, payload: &[u8]) -> Option<Vec<u8>> {
    let mut mac = Hmac::<Sha256>::new_from_slice(secret.as_bytes()).ok()?;
    mac.update(timestamp.as_bytes());
    mac.update(b".");
    mac.update(payload);
    Some(mac.finalize().into_bytes().to_vec())
}

fn eq_ct(a: &[u8], b: &[u8]) -> bool {
    a.len() == b.len() && a.iter().zip(b).fold(0u8, |acc, (x, y)| acc | (x ^ y)) == 0
}

/// Check a `Stripe-Signature` header (`t=<unix>,v1=<hex>[,v1=<hex>…]`) against the raw body:
/// HMAC-SHA256 of `"<t>.<body>"` with the endpoint's signing secret, compared in constant time,
/// timestamp within `tolerance` seconds of `now`. Any one matching `v1` passes (Stripe sends two
/// while a secret is being rolled).
pub fn verify_signature(header: Option<&str>, payload: &[u8], secret: &str, now: i64, tolerance: i64) -> Result<(), SigError> {
    let header = header.filter(|h| !h.trim().is_empty()).ok_or(SigError::Missing)?;
    let mut t: Option<&str> = None;
    let mut sigs: Vec<Vec<u8>> = vec![];
    for part in header.split(',') {
        let (k, v) = part.trim().split_once('=').ok_or(SigError::Malformed)?;
        match k {
            "t" => t = Some(v),
            "v1" => sigs.push(hex::decode(v).map_err(|_| SigError::Malformed)?),
            _ => {}
        }
    }
    let t = t.ok_or(SigError::Malformed)?;
    let ts: i64 = t.parse().map_err(|_| SigError::Malformed)?;
    if sigs.is_empty() {
        return Err(SigError::Malformed);
    }
    if (now - ts).abs() > tolerance {
        return Err(SigError::Stale);
    }
    let expected = sign(secret, t, payload).ok_or(SigError::Mismatch)?;
    if sigs.iter().any(|s| eq_ct(s, &expected)) {
        Ok(())
    } else {
        Err(SigError::Mismatch)
    }
}

/// The header Stripe would send for `payload` at `t` (tests and local webhook drills).
pub fn signature_header(secret: &str, t: i64, payload: &[u8]) -> String {
    format!("t={t},v1={}", hex::encode(sign(secret, &t.to_string(), payload).unwrap_or_default()))
}

/// One pen, as the database has it. Nothing here comes from the buyer's request.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CheckoutItem {
    pub product_id: i32,
    pub order_id: i32,
    pub name: String,
    pub sku: String,
    pub slug: String,
    /// What the pen sells for now: the sale price when it is on sale.
    pub unit_amount_cents: i64,
    /// Absolute URL of the first photo, sent only when the site is on https (Stripe fetches it).
    pub image_url: String,
}

/// The form body for `POST /v1/checkout/sessions` for one pen shipping to `region`, offering the
/// shipping `options` for that place (his list, already filtered; at most five reach Stripe).
pub fn checkout_form(item: &CheckoutItem, origin: &str, expires_at: i64, region: Region, options: &[ShippingOption]) -> Vec<(String, String)> {
    let mut f: Vec<(String, String)> = vec![];
    let mut add = |k: &str, v: String| f.push((k.to_string(), v));
    add("mode", "payment".into());
    add("line_items[0][quantity]", "1".into());
    add("line_items[0][price_data][currency]", "usd".into());
    add("line_items[0][price_data][unit_amount]", item.unit_amount_cents.to_string());
    add("line_items[0][price_data][product_data][name]", item.name.clone());
    if !item.sku.is_empty() {
        add("line_items[0][price_data][product_data][description]", format!("SKU {}", item.sku));
    }
    if item.image_url.starts_with("https://") {
        add("line_items[0][price_data][product_data][images][0]", item.image_url.clone());
    }
    add("client_reference_id", item.product_id.to_string());
    for prefix in ["metadata", "payment_intent_data[metadata]"] {
        add(&format!("{prefix}[product_id]"), item.product_id.to_string());
        add(&format!("{prefix}[order_id]"), item.order_id.to_string());
        if !item.sku.is_empty() {
            add(&format!("{prefix}[sku]"), item.sku.clone());
        }
        add(&format!("{prefix}[ship_to]"), region.key().into());
    }
    add("success_url", format!("{origin}/checkout/thanks/?session_id={{CHECKOUT_SESSION_ID}}"));
    add("cancel_url", format!("{origin}/product/{}/", item.slug));
    add("expires_at", expires_at.to_string());
    for (i, c) in crate::shipping::allowed_countries(region).iter().enumerate() {
        add(&format!("shipping_address_collection[allowed_countries][{i}]"), (*c).to_string());
    }
    for (i, o) in options.iter().filter(|o| o.region == region).take(crate::shipping::MAX_PER_REGION).enumerate() {
        let p = format!("shipping_options[{i}][shipping_rate_data]");
        add(&format!("{p}[type]"), "fixed_amount".into());
        add(&format!("{p}[display_name]"), o.name.chars().take(crate::shipping::MAX_NAME_CHARS).collect());
        add(&format!("{p}[fixed_amount][amount]"), o.cents.max(0).to_string());
        add(&format!("{p}[fixed_amount][currency]"), "usd".into());
    }
    f
}

/// What a Trading Post checkout is for, carried in the session's `metadata[kind]` so the webhook can
/// tell it from a pen (pens carry no kind).
pub const KIND_LISTING: &str = "tp_listing";
pub const KIND_MEMBERSHIP: &str = "unlimited_posts";

/// The form for the $5 Trading Post listing fee. `email` is the address the poster typed (prefilled
/// at Stripe so the receipt reaches them).
pub fn listing_form(listing_id: i32, item: &str, email: &str, fee_cents: i64, origin: &str) -> Vec<(String, String)> {
    let mut f: Vec<(String, String)> = vec![];
    let mut add = |k: &str, v: String| f.push((k.to_string(), v));
    add("mode", "payment".into());
    add("line_items[0][quantity]", "1".into());
    add("line_items[0][price_data][currency]", "usd".into());
    add("line_items[0][price_data][unit_amount]", fee_cents.to_string());
    add("line_items[0][price_data][product_data][name]", "Trading Post listing".into());
    let item: String = item.replace(['\r', '\n'], " ").chars().take(200).collect();
    if !item.trim().is_empty() {
        add("line_items[0][price_data][product_data][description]", item.trim().to_string());
    }
    add("client_reference_id", format!("tp-{listing_id}"));
    for prefix in ["metadata", "payment_intent_data[metadata]"] {
        add(&format!("{prefix}[kind]"), KIND_LISTING.into());
        add(&format!("{prefix}[listing_id]"), listing_id.to_string());
    }
    if email.contains('@') && email.len() <= 254 {
        add("customer_email", email.to_string());
    }
    add("success_url", format!("{origin}/trading-post/thanks/?session_id={{CHECKOUT_SESSION_ID}}"));
    add("cancel_url", format!("{origin}/post-your-product/"));
    f
}

/// The form for the Unlimited Posts membership: a subscription renewed every year at `year_cents`.
pub fn membership_form(year_cents: i64, origin: &str) -> Vec<(String, String)> {
    let mut f: Vec<(String, String)> = vec![];
    let mut add = |k: &str, v: String| f.push((k.to_string(), v));
    add("mode", "subscription".into());
    add("line_items[0][quantity]", "1".into());
    add("line_items[0][price_data][currency]", "usd".into());
    add("line_items[0][price_data][unit_amount]", year_cents.to_string());
    add("line_items[0][price_data][product_data][name]", "Unlimited Posts".into());
    add("line_items[0][price_data][recurring][interval]", "year".into());
    add("line_items[0][price_data][recurring][interval_count]", "1".into());
    add("metadata[kind]", KIND_MEMBERSHIP.into());
    add("subscription_data[metadata][kind]", KIND_MEMBERSHIP.into());
    add("success_url", format!("{origin}/trading-post/thanks/?session_id={{CHECKOUT_SESSION_ID}}"));
    add("cancel_url", format!("{origin}/product/unlimited-posts/"));
    f
}

#[derive(Clone, Debug, Deserialize)]
pub struct Event {
    pub id: String,
    #[serde(rename = "type")]
    pub kind: String,
    pub data: EventData,
}

#[derive(Clone, Debug, Deserialize)]
pub struct EventData {
    pub object: serde_json::Value,
}

#[derive(Clone, Debug, Default, Deserialize)]
pub struct Address {
    #[serde(default)]
    pub line1: Option<String>,
    #[serde(default)]
    pub line2: Option<String>,
    #[serde(default)]
    pub city: Option<String>,
    #[serde(default)]
    pub state: Option<String>,
    #[serde(default)]
    pub postal_code: Option<String>,
    #[serde(default)]
    pub country: Option<String>,
}

#[derive(Clone, Debug, Default, Deserialize)]
pub struct Shipping {
    #[serde(default)]
    pub name: Option<String>,
    #[serde(default)]
    pub address: Option<Address>,
}

#[derive(Clone, Debug, Default, Deserialize)]
pub struct CustomerDetails {
    #[serde(default)]
    pub email: Option<String>,
    #[serde(default)]
    pub name: Option<String>,
}

#[derive(Clone, Debug, Default, Deserialize)]
pub struct Collected {
    #[serde(default)]
    pub shipping_details: Option<Shipping>,
}

/// The fields of a Checkout Session the shop reads.
#[derive(Clone, Debug, Default, Deserialize)]
pub struct Session {
    pub id: String,
    #[serde(default)]
    pub url: Option<String>,
    #[serde(default)]
    pub status: Option<String>,
    #[serde(default)]
    pub payment_status: Option<String>,
    #[serde(default)]
    pub amount_total: Option<i64>,
    #[serde(default)]
    pub currency: Option<String>,
    #[serde(default)]
    pub client_reference_id: Option<String>,
    #[serde(default)]
    pub metadata: std::collections::HashMap<String, String>,
    #[serde(default)]
    pub payment_intent: Option<serde_json::Value>,
    #[serde(default)]
    pub customer_details: Option<CustomerDetails>,
    /// Where API versions before 2025-03-31 put the shipping address.
    #[serde(default)]
    pub shipping_details: Option<Shipping>,
    /// Where later API versions put it.
    #[serde(default)]
    pub collected_information: Option<Collected>,
    /// The shipping the buyer chose and was charged (`amount_total`, and the `shipping_rate` id).
    #[serde(default)]
    pub shipping_cost: Option<ShippingCost>,
    #[serde(default)]
    pub total_details: Option<TotalDetails>,
    #[serde(default)]
    pub mode: Option<String>,
    /// Subscription mode: the subscription and customer Checkout created.
    #[serde(default)]
    pub subscription: Option<serde_json::Value>,
    #[serde(default)]
    pub customer: Option<serde_json::Value>,
}

#[derive(Clone, Debug, Default, Deserialize)]
pub struct ShippingCost {
    #[serde(default)]
    pub amount_subtotal: Option<i64>,
    #[serde(default)]
    pub amount_total: Option<i64>,
    #[serde(default)]
    pub shipping_rate: Option<serde_json::Value>,
}

#[derive(Clone, Debug, Default, Deserialize)]
pub struct TotalDetails {
    #[serde(default)]
    pub amount_shipping: Option<i64>,
}

/// An id field that Stripe may also send expanded into an object.
fn id_of(v: &Option<serde_json::Value>) -> String {
    match v {
        Some(serde_json::Value::String(s)) => s.clone(),
        Some(serde_json::Value::Object(o)) => o.get("id").and_then(|v| v.as_str()).unwrap_or("").to_string(),
        _ => String::new(),
    }
}

impl Session {
    pub fn from_value(v: &serde_json::Value) -> Option<Session> {
        serde_json::from_value(v.clone()).ok()
    }
    pub fn paid(&self) -> bool {
        matches!(self.payment_status.as_deref(), Some("paid") | Some("no_payment_required"))
    }
    pub fn order_id(&self) -> Option<i32> {
        self.metadata.get("order_id").and_then(|v| v.parse().ok())
    }
    pub fn product_id(&self) -> Option<i32> {
        self.metadata.get("product_id").or(self.client_reference_id.as_ref()).and_then(|v| v.parse().ok())
    }
    /// `payment_intent` is an id, or an object when expanded.
    pub fn payment_intent_id(&self) -> String {
        id_of(&self.payment_intent)
    }
    pub fn subscription_id(&self) -> String {
        id_of(&self.subscription)
    }
    pub fn customer_id(&self) -> String {
        id_of(&self.customer)
    }
    /// `metadata[kind]`: a Trading Post fee or membership; empty for a pen.
    pub fn kind(&self) -> &str {
        self.metadata.get("kind").map(String::as_str).unwrap_or("")
    }
    pub fn listing_id(&self) -> Option<i32> {
        self.metadata.get("listing_id").and_then(|v| v.parse().ok())
    }
    /// What Stripe charged for shipping: `shipping_cost.amount_total`, else
    /// `total_details.amount_shipping`; None when the session reports neither.
    pub fn shipping_charged(&self) -> Option<i64> {
        self.shipping_cost.as_ref().and_then(|c| c.amount_total).or_else(|| self.total_details.as_ref().and_then(|t| t.amount_shipping))
    }
    /// The shipping rate's own price before any tax, to match against the options offered.
    pub fn shipping_rate_amount(&self) -> Option<i64> {
        self.shipping_cost.as_ref().and_then(|c| c.amount_subtotal.or(c.amount_total)).or_else(|| self.total_details.as_ref().and_then(|t| t.amount_shipping))
    }
    pub fn shipping_rate_id(&self) -> String {
        self.shipping_cost.as_ref().map(|c| id_of(&c.shipping_rate)).unwrap_or_default()
    }
    pub fn email(&self) -> String {
        self.customer_details.as_ref().and_then(|c| c.email.clone()).unwrap_or_default()
    }
    pub fn buyer_name(&self) -> String {
        self.shipping().and_then(|s| s.name.clone()).or_else(|| self.customer_details.as_ref().and_then(|c| c.name.clone())).unwrap_or_default()
    }
    pub fn shipping(&self) -> Option<&Shipping> {
        self.collected_information.as_ref().and_then(|c| c.shipping_details.as_ref()).or(self.shipping_details.as_ref())
    }
    pub fn shipping_json(&self) -> Option<serde_json::Value> {
        let s = self.shipping()?;
        let a = s.address.clone().unwrap_or_default();
        Some(serde_json::json!({
            "name": s.name.clone().unwrap_or_default(),
            "line1": a.line1.unwrap_or_default(),
            "line2": a.line2.unwrap_or_default(),
            "city": a.city.unwrap_or_default(),
            "state": a.state.unwrap_or_default(),
            "postal_code": a.postal_code.unwrap_or_default(),
            "country": a.country.unwrap_or_default(),
        }))
    }
}

/// The parts of an Invoice (`invoice.paid`) that keep a membership's paid-through date. API versions
/// from 2025-03-31 put the subscription under `parent.subscription_details`; older ones at the top.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct InvoicePaid {
    pub subscription: String,
    /// `metadata[kind]` copied from the subscription (set through `subscription_data[metadata]`).
    pub kind: String,
    pub email: String,
    pub customer: String,
    /// The end of the period this invoice paid for: the latest `lines.data[].period.end`.
    pub period_end: Option<i64>,
}

fn str_or_id(v: Option<&serde_json::Value>) -> String {
    id_of(&v.cloned())
}

pub fn invoice_paid(v: &serde_json::Value) -> InvoicePaid {
    let sub = v.pointer("/parent/subscription_details/subscription").or_else(|| v.get("subscription"));
    let kind = v.pointer("/parent/subscription_details/metadata/kind").or_else(|| v.pointer("/subscription_details/metadata/kind")).and_then(|k| k.as_str()).unwrap_or("");
    let period_end = v.pointer("/lines/data").and_then(|d| d.as_array()).and_then(|lines| lines.iter().filter_map(|l| l.pointer("/period/end").and_then(|e| e.as_i64())).max());
    InvoicePaid {
        subscription: str_or_id(sub),
        kind: kind.to_string(),
        email: v.get("customer_email").and_then(|e| e.as_str()).unwrap_or("").to_string(),
        customer: str_or_id(v.get("customer")),
        period_end,
    }
}

/// `customer.subscription.deleted`: which subscription ended, and when.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct SubscriptionEnded {
    pub subscription: String,
    pub kind: String,
    pub ended_at: Option<i64>,
}

pub fn subscription_ended(v: &serde_json::Value) -> SubscriptionEnded {
    SubscriptionEnded {
        subscription: v.get("id").and_then(|i| i.as_str()).unwrap_or("").to_string(),
        kind: v.pointer("/metadata/kind").and_then(|k| k.as_str()).unwrap_or("").to_string(),
        ended_at: v.get("ended_at").and_then(|e| e.as_i64()).or_else(|| v.get("canceled_at").and_then(|e| e.as_i64())),
    }
}

/// A Checkout Session id as Stripe issues them (`cs_test_…` / `cs_live_…`); anything else is refused
/// before it reaches a URL.
pub fn valid_session_id(s: &str) -> bool {
    s.starts_with("cs_") && s.len() <= 255 && s.chars().all(|c| c.is_ascii_alphanumeric() || c == '_')
}

/// Stripe's error message from a failed call, without anything that could carry a key.
fn api_error(status: reqwest::StatusCode, body: &str) -> anyhow::Error {
    let msg = serde_json::from_str::<serde_json::Value>(body).ok().and_then(|v| v.pointer("/error/message").and_then(|m| m.as_str()).map(str::to_string)).unwrap_or_else(|| "no message".into());
    anyhow::anyhow!("Stripe answered {status}: {}", redact_keys(&msg))
}

/// Stripe's "Invalid API Key provided: sk_test_****abcd" keeps the key's last characters; logs and
/// order notes get none of it.
pub fn redact_keys(msg: &str) -> String {
    msg.split(' ').map(|w| if ["sk_", "rk_", "pk_", "whsec_"].iter().any(|p| w.trim_matches(|c: char| !c.is_ascii_alphanumeric() && c != '_').starts_with(p)) { "[key]" } else { w }).collect::<Vec<_>>().join(" ")
}

async fn send(req: reqwest::RequestBuilder) -> anyhow::Result<serde_json::Value> {
    let res = req.send().await.map_err(|e| anyhow::anyhow!("Stripe unreachable: {}", e.without_url()))?;
    let status = res.status();
    let body = res.text().await.unwrap_or_default();
    if !status.is_success() {
        return Err(api_error(status, &body));
    }
    Ok(serde_json::from_str(&body)?)
}

pub async fn create_session(http: &reqwest::Client, key: &str, form: &[(String, String)], idempotency_key: &str) -> anyhow::Result<Session> {
    let v = send(http.post(format!("{API}/checkout/sessions")).bearer_auth(key).header("Idempotency-Key", idempotency_key).form(form)).await?;
    Session::from_value(&v).ok_or_else(|| anyhow::anyhow!("Stripe returned a session the shop could not read"))
}

pub async fn retrieve_session(http: &reqwest::Client, key: &str, id: &str) -> anyhow::Result<Session> {
    if !valid_session_id(id) {
        anyhow::bail!("not a Checkout Session id");
    }
    let v = send(http.get(format!("{API}/checkout/sessions/{id}")).bearer_auth(key)).await?;
    Session::from_value(&v).ok_or_else(|| anyhow::anyhow!("Stripe returned a session the shop could not read"))
}

/// Close an open session so nobody can pay on it (the buyer let the pen go, or it sold elsewhere).
pub async fn expire_session(http: &reqwest::Client, key: &str, id: &str) -> anyhow::Result<()> {
    if !valid_session_id(id) {
        anyhow::bail!("not a Checkout Session id");
    }
    send(http.post(format!("{API}/checkout/sessions/{id}/expire")).bearer_auth(key)).await.map(|_| ())
}

/// Full refund of a payment. The idempotency key makes a retried webhook refund at most once.
pub async fn refund(http: &reqwest::Client, key: &str, payment_intent: &str, idempotency_key: &str) -> anyhow::Result<String> {
    let form = [("payment_intent", payment_intent), ("reason", "requested_by_customer")];
    let v = send(http.post(format!("{API}/refunds")).bearer_auth(key).header("Idempotency-Key", idempotency_key).form(&form)).await?;
    Ok(v.get("id").and_then(|i| i.as_str()).unwrap_or("").to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    const SECRET: &str = "whsec_test_secret";
    const BODY: &[u8] = br#"{"id":"evt_1","type":"checkout.session.completed","data":{"object":{"id":"cs_test_a1"}}}"#;

    #[test]
    fn good_signature_passes() {
        let h = signature_header(SECRET, 1_700_000_000, BODY);
        assert_eq!(verify_signature(Some(&h), BODY, SECRET, 1_700_000_010, SIGNATURE_TOLERANCE_SECS), Ok(()));
    }

    #[test]
    fn any_of_two_v1_signatures_passes() {
        let good = signature_header(SECRET, 1_700_000_000, BODY);
        let v1 = good.split_once(",v1=").map(|(_, s)| s.to_string()).unwrap_or_default();
        let h = format!("t=1700000000,v1={},v1={v1}", "00".repeat(32));
        assert_eq!(verify_signature(Some(&h), BODY, SECRET, 1_700_000_000, SIGNATURE_TOLERANCE_SECS), Ok(()));
    }

    #[test]
    fn wrong_secret_or_changed_body_fails() {
        let h = signature_header("whsec_other", 1_700_000_000, BODY);
        assert_eq!(verify_signature(Some(&h), BODY, SECRET, 1_700_000_000, SIGNATURE_TOLERANCE_SECS), Err(SigError::Mismatch));
        let h = signature_header(SECRET, 1_700_000_000, BODY);
        let tampered = String::from_utf8_lossy(BODY).replace("evt_1", "evt_2");
        assert_eq!(verify_signature(Some(&h), tampered.as_bytes(), SECRET, 1_700_000_000, SIGNATURE_TOLERANCE_SECS), Err(SigError::Mismatch));
    }

    #[test]
    fn stale_signature_fails_both_ways() {
        let h = signature_header(SECRET, 1_700_000_000, BODY);
        assert_eq!(verify_signature(Some(&h), BODY, SECRET, 1_700_000_000 + 301, SIGNATURE_TOLERANCE_SECS), Err(SigError::Stale));
        assert_eq!(verify_signature(Some(&h), BODY, SECRET, 1_700_000_000 - 301, SIGNATURE_TOLERANCE_SECS), Err(SigError::Stale));
    }

    #[test]
    fn missing_or_malformed_header_fails() {
        assert_eq!(verify_signature(None, BODY, SECRET, 0, 300), Err(SigError::Missing));
        assert_eq!(verify_signature(Some(""), BODY, SECRET, 0, 300), Err(SigError::Missing));
        assert_eq!(verify_signature(Some("v1=abcd"), BODY, SECRET, 0, 300), Err(SigError::Malformed));
        assert_eq!(verify_signature(Some("t=1,v1=zz"), BODY, SECRET, 1, 300), Err(SigError::Malformed));
        assert_eq!(verify_signature(Some("t=1"), BODY, SECRET, 1, 300), Err(SigError::Malformed));
    }

    fn item() -> CheckoutItem {
        CheckoutItem { product_id: 7, order_id: 42, name: "Vintage Pens: Parker Vacumatic".into(), sku: "6972".into(), slug: "parker-vacumatic".into(), unit_amount_cents: 18999, image_url: "https://example.com/a.jpg".into() }
    }

    fn get<'a>(f: &'a [(String, String)], k: &str) -> Option<&'a str> {
        f.iter().find(|(key, _)| key == k).map(|(_, v)| v.as_str())
    }

    fn all<'a>(f: &'a [(String, String)], prefix: &str) -> Vec<&'a str> {
        f.iter().filter(|(k, _)| k.starts_with(prefix)).map(|(_, v)| v.as_str()).collect()
    }

    #[test]
    fn checkout_form_carries_the_database_price_and_ids() {
        let f = checkout_form(&item(), "https://penmarket.example", 1_700_001_860, Region::Us, &crate::shipping::default_list());
        assert_eq!(get(&f, "mode"), Some("payment"));
        assert_eq!(get(&f, "line_items[0][price_data][unit_amount]"), Some("18999"));
        assert_eq!(get(&f, "line_items[0][price_data][product_data][name]"), Some("Vintage Pens: Parker Vacumatic"));
        assert_eq!(get(&f, "client_reference_id"), Some("7"));
        assert_eq!(get(&f, "metadata[order_id]"), Some("42"));
        assert_eq!(get(&f, "metadata[ship_to]"), Some("us"));
        assert_eq!(get(&f, "payment_intent_data[metadata][product_id]"), Some("7"));
        assert_eq!(get(&f, "success_url"), Some("https://penmarket.example/checkout/thanks/?session_id={CHECKOUT_SESSION_ID}"));
        assert_eq!(get(&f, "cancel_url"), Some("https://penmarket.example/product/parker-vacumatic/"));
        assert_eq!(get(&f, "line_items[0][price_data][product_data][images][0]"), Some("https://example.com/a.jpg"));
    }

    #[test]
    fn us_buyers_get_the_us_rates_and_us_addresses_only() {
        let f = checkout_form(&item(), "https://x", 1, Region::Us, &crate::shipping::default_list());
        assert_eq!(all(&f, "shipping_address_collection[allowed_countries]"), vec!["US"]);
        assert_eq!(get(&f, "shipping_address_collection[allowed_countries][0]"), Some("US"));
        assert_eq!(all(&f, "shipping_options["), vec!["fixed_amount", "Uninsured USPS Ground Advantage", "600", "usd", "fixed_amount", "USPS Insured Priority", "1500", "usd"]);
        assert_eq!(get(&f, "shipping_options[1][shipping_rate_data][display_name]"), Some("USPS Insured Priority"));
        assert_eq!(get(&f, "shipping_options[1][shipping_rate_data][fixed_amount][amount]"), Some("1500"));
        assert_eq!(get(&f, "shipping_options[1][shipping_rate_data][fixed_amount][currency]"), Some("usd"));
        assert!(get(&f, "shipping_options[2][shipping_rate_data][type]").is_none());
    }

    #[test]
    fn buyers_abroad_get_every_other_country_and_the_international_rate() {
        let f = checkout_form(&item(), "https://x", 1, Region::Intl, &crate::shipping::default_list());
        let countries = all(&f, "shipping_address_collection[allowed_countries]");
        assert_eq!(countries.len(), crate::shipping::OUTSIDE_US.len());
        assert!(!countries.contains(&"US") && countries.contains(&"CA") && countries.contains(&"GB"));
        assert_eq!(get(&f, "shipping_address_collection[allowed_countries][235]"), Some("ZW"));
        assert_eq!(get(&f, "shipping_options[0][shipping_rate_data][type]"), Some("fixed_amount"));
        assert_eq!(get(&f, "shipping_options[0][shipping_rate_data][display_name]"), Some("International Insured Priority"));
        assert_eq!(get(&f, "shipping_options[0][shipping_rate_data][fixed_amount][amount]"), Some("5500"));
        assert!(get(&f, "shipping_options[1][shipping_rate_data][type]").is_none());
        assert_eq!(get(&f, "metadata[ship_to]"), Some("intl"));
    }

    #[test]
    fn at_most_five_options_and_none_when_his_list_is_empty() {
        let mut it = item();
        it.image_url = "http://127.0.0.1:8140/media/a.jpg".into();
        let many: Vec<ShippingOption> = (0..7).map(|i| ShippingOption { name: format!("R{i}"), cents: 100 * i, region: Region::Us }).collect();
        let f = checkout_form(&it, "http://127.0.0.1:8140", 1, Region::Us, &many);
        assert!(get(&f, "shipping_options[4][shipping_rate_data][type]").is_some());
        assert!(get(&f, "shipping_options[5][shipping_rate_data][type]").is_none());
        assert!(get(&f, "line_items[0][price_data][product_data][images][0]").is_none());
        let f = checkout_form(&it, "http://127.0.0.1:8140", 1, Region::Us, &[]);
        assert!(get(&f, "shipping_options[0][shipping_rate_data][type]").is_none());
        assert_eq!(get(&f, "shipping_address_collection[allowed_countries][0]"), Some("US"));
    }

    #[test]
    fn listing_and_membership_forms() {
        let f = listing_form(12, "Pelikan M800\r\nBcc: x", "seller@example.com", 500, "https://x");
        assert_eq!((get(&f, "mode"), get(&f, "line_items[0][price_data][unit_amount]"), get(&f, "metadata[kind]"), get(&f, "metadata[listing_id]")), (Some("payment"), Some("500"), Some("tp_listing"), Some("12")));
        assert_eq!(get(&f, "line_items[0][price_data][product_data][description]"), Some("Pelikan M800  Bcc: x"));
        assert_eq!(get(&f, "customer_email"), Some("seller@example.com"));
        assert_eq!(get(&f, "payment_intent_data[metadata][kind]"), Some("tp_listing"));
        assert_eq!(get(&f, "success_url"), Some("https://x/trading-post/thanks/?session_id={CHECKOUT_SESSION_ID}"));
        assert!(all(&f, "shipping_").is_empty());
        let m = membership_form(12500, "https://x");
        assert_eq!(get(&m, "mode"), Some("subscription"));
        assert_eq!(get(&m, "line_items[0][price_data][unit_amount]"), Some("12500"));
        assert_eq!(get(&m, "line_items[0][price_data][recurring][interval]"), Some("year"));
        assert_eq!(get(&m, "subscription_data[metadata][kind]"), Some("unlimited_posts"));
        assert_eq!(get(&m, "metadata[kind]"), Some("unlimited_posts"));
        assert!(get(&m, "payment_intent_data[metadata][kind]").is_none(), "not allowed in subscription mode");
    }

    #[test]
    fn session_reports_the_shipping_charged() {
        let s = Session::from_value(&serde_json::json!({"id": "cs_test_s", "amount_total": 20499, "shipping_cost": {"amount_subtotal": 1500, "amount_total": 1500, "shipping_rate": "shr_1"}, "total_details": {"amount_shipping": 1500}})).unwrap();
        assert_eq!((s.shipping_charged(), s.shipping_rate_amount(), s.shipping_rate_id().as_str()), (Some(1500), Some(1500), "shr_1"));
        let s = Session::from_value(&serde_json::json!({"id": "cs_test_t", "total_details": {"amount_shipping": 600}})).unwrap();
        assert_eq!(s.shipping_charged(), Some(600));
        let s = Session::from_value(&serde_json::json!({"id": "cs_test_u", "shipping_cost": null, "metadata": {"kind": "tp_listing", "listing_id": "4"}})).unwrap();
        assert_eq!((s.shipping_charged(), s.kind(), s.listing_id()), (None, "tp_listing", Some(4)));
    }

    #[test]
    fn invoice_and_subscription_shapes() {
        let new = serde_json::json!({"customer_email": "m@example.com", "customer": "cus_1", "parent": {"subscription_details": {"subscription": "sub_1", "metadata": {"kind": "unlimited_posts"}}},
            "lines": {"data": [{"period": {"start": 1, "end": 1_800_000_000}}, {"period": {"start": 1, "end": 1_700_000_000}}]}});
        assert_eq!(invoice_paid(&new), InvoicePaid { subscription: "sub_1".into(), kind: "unlimited_posts".into(), email: "m@example.com".into(), customer: "cus_1".into(), period_end: Some(1_800_000_000) });
        let old = serde_json::json!({"subscription": "sub_2", "subscription_details": {"metadata": {"kind": "unlimited_posts"}}, "lines": {"data": []}});
        let i = invoice_paid(&old);
        assert_eq!((i.subscription.as_str(), i.kind.as_str(), i.period_end), ("sub_2", "unlimited_posts", None));
        let e = subscription_ended(&serde_json::json!({"id": "sub_1", "metadata": {"kind": "unlimited_posts"}, "ended_at": 1_750_000_000}));
        assert_eq!(e, SubscriptionEnded { subscription: "sub_1".into(), kind: "unlimited_posts".into(), ended_at: Some(1_750_000_000) });
    }

    #[test]
    fn session_reads_both_shipping_shapes() {
        let old = serde_json::json!({"id": "cs_test_1", "payment_status": "paid", "metadata": {"order_id": "3", "product_id": "9"}, "payment_intent": "pi_1",
            "customer_details": {"email": "a@b.co", "name": "Card Name"}, "shipping_details": {"name": "Ship Name", "address": {"line1": "1 Main St", "city": "Norwich", "state": "CT", "postal_code": "06360", "country": "US"}}});
        let s = Session::from_value(&old).unwrap();
        assert!(s.paid());
        assert_eq!((s.order_id(), s.product_id(), s.payment_intent_id().as_str(), s.email().as_str(), s.buyer_name().as_str()), (Some(3), Some(9), "pi_1", "a@b.co", "Ship Name"));
        assert_eq!(s.shipping_json().unwrap()["city"], "Norwich");
        let new = serde_json::json!({"id": "cs_test_2", "payment_status": "unpaid", "client_reference_id": "11", "payment_intent": {"id": "pi_2"},
            "collected_information": {"shipping_details": {"name": "N", "address": {"line1": "2 Elm"}}}});
        let s = Session::from_value(&new).unwrap();
        assert!(!s.paid());
        assert_eq!((s.product_id(), s.payment_intent_id().as_str()), (Some(11), "pi_2"));
        assert_eq!(s.shipping_json().unwrap()["line1"], "2 Elm");
    }

    #[test]
    fn key_fragments_never_reach_logs() {
        assert_eq!(redact_keys("Invalid API Key provided: sk_test_**********_key"), "Invalid API Key provided: [key]");
        assert_eq!(redact_keys("No such checkout.session: 'cs_test_1'"), "No such checkout.session: 'cs_test_1'");
    }

    #[test]
    fn session_ids_are_checked_before_use() {
        assert!(valid_session_id("cs_test_a1B2c3"));
        assert!(!valid_session_id("cs_test/../../v1/charges"));
        assert!(!valid_session_id("pi_123"));
        assert!(!valid_session_id(""));
    }
}
