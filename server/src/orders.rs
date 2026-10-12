//! Orders for one-of-a-kind pens. A buyer who presses "Buy this pen" gets a hold on it for the life
//! of their Stripe Checkout Session; a second buyer is told someone is paying for it. The webhook
//! turns a paid session into a sold pen, in one transaction, or refunds it if the pen was gone.
//!
//! The hold is a `shop_order` row in status `open` (card) or `processing` (a bank payment Stripe is
//! still collecting). A partial unique index allows one such row per pen, so two buyers racing for
//! the same pen cannot both get one. No foreign key to `product`: the importer truncates that table
//! with CASCADE, and orders must survive it.

use crate::shipping::{Region, ShippingOption};
use crate::stripe::Session;
use sqlx::{PgPool, Postgres, Row, Transaction};

/// A pen as checkout needs it, read from the database at the moment of the click.
#[derive(Clone, Debug)]
pub struct ForSale {
    pub id: i32,
    pub slug: String,
    pub title: String,
    pub sku: String,
    pub status: String,
    pub is_subscription: bool,
    /// The sale price when there is one, else the price.
    pub amount_cents: i64,
    /// First photo, relative to the media dir.
    pub image: String,
}

impl ForSale {
    pub fn buyable(&self) -> bool {
        self.status == "live" && !self.is_subscription && self.amount_cents > 0
    }
}

pub async fn for_sale(pool: &PgPool, slug: &str) -> anyhow::Result<Option<ForSale>> {
    let row = sqlx::query(
        "SELECT p.id, p.slug, p.title, p.sku, p.status, p.is_subscription, COALESCE(p.sale_price_cents, p.price_cents) AS amount,
                COALESCE((SELECT path FROM product_image i WHERE i.product_id = p.id AND i.archived_at IS NULL ORDER BY position LIMIT 1), '') AS image
         FROM product p WHERE p.slug = $1",
    )
    .bind(slug)
    .fetch_optional(pool)
    .await?;
    Ok(row.map(|r| ForSale { id: r.get("id"), slug: r.get("slug"), title: r.get("title"), sku: r.get("sku"), status: r.get("status"), is_subscription: r.get("is_subscription"), amount_cents: r.get("amount"), image: r.get("image") }))
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Hold {
    /// A new order row holds the pen; create its Stripe session next.
    Placed(i32),
    /// This visitor already holds it: send them back to their open session (which ships to `region`).
    Yours { order_id: i32, url: String, region: String },
    /// Someone else is paying for it right now.
    Taken,
}

/// Holds that ran past their time without a webhook (Stripe already closed the session) let go of the pen.
async fn lapse_stale(tx: &mut Transaction<'_, Postgres>, product_id: i32) -> anyhow::Result<()> {
    sqlx::query("UPDATE shop_order SET status = 'expired', updated_at = now() WHERE product_id = $1 AND status = 'open' AND expires_at < now()").bind(product_id).execute(&mut **tx).await?;
    Ok(())
}

/// Hold `pen` for `visitor_key` until `hold_until` (unix seconds), shipping to `region` with the
/// shipping options `offered` there (kept on the order to name the one Stripe charged for).
pub async fn place_hold(pool: &PgPool, pen: &ForSale, visitor_key: &str, region: Region, offered: &[ShippingOption], hold_until: i64) -> anyhow::Result<Hold> {
    let mut tx = pool.begin().await?;
    lapse_stale(&mut tx, pen.id).await?;
    if let Some(r) = sqlx::query("SELECT id, visitor_key, status, stripe_session_url, ship_region FROM shop_order WHERE product_id = $1 AND status IN ('open', 'processing')").bind(pen.id).fetch_optional(&mut *tx).await? {
        tx.commit().await?;
        let (key, status, url): (String, String, String) = (r.get("visitor_key"), r.get("status"), r.get("stripe_session_url"));
        return Ok(if key == visitor_key && status == "open" && !url.is_empty() { Hold::Yours { order_id: r.get("id"), url, region: r.get("ship_region") } } else { Hold::Taken });
    }
    let offered: Vec<&ShippingOption> = offered.iter().filter(|o| o.region == region).collect();
    let id: Option<i32> = sqlx::query_scalar(
        "INSERT INTO shop_order (product_id, product_slug, product_title, product_sku, amount_cents, ship_region, shipping_offered, visitor_key, expires_at)
         VALUES ($1, $2, $3, $4, $5, $6, $7, $8, to_timestamp($9))
         ON CONFLICT (product_id) WHERE status IN ('open', 'processing') DO NOTHING RETURNING id",
    )
    .bind(pen.id)
    .bind(&pen.slug)
    .bind(&pen.title)
    .bind(&pen.sku)
    .bind(pen.amount_cents)
    .bind(region.key())
    .bind(serde_json::to_value(&offered)?)
    .bind(visitor_key)
    .bind(hold_until as f64)
    .fetch_optional(&mut *tx)
    .await?;
    tx.commit().await?;
    Ok(id.map(Hold::Placed).unwrap_or(Hold::Taken))
}

/// Pens this visitor (by cookie or by address) is holding at checkout right now, other than `product_id`.
pub async fn holds_elsewhere(pool: &PgPool, visitor_key: &str, holder_ip: &str, product_id: i32) -> anyhow::Result<i64> {
    Ok(sqlx::query_scalar("SELECT count(*) FROM shop_order WHERE status = 'open' AND expires_at > now() AND product_id <> $3 AND (visitor_key = $1 OR (holder_ip <> '' AND holder_ip = $2))")
        .bind(visitor_key).bind(holder_ip).bind(product_id).fetch_one(pool).await?)
}

pub async fn set_holder_ip(pool: &PgPool, order_id: i32, holder_ip: &str) -> anyhow::Result<()> {
    sqlx::query("UPDATE shop_order SET holder_ip = $2 WHERE id = $1").bind(order_id).bind(holder_ip).execute(pool).await?;
    Ok(())
}

pub async fn attach_session(pool: &PgPool, order_id: i32, session_id: &str, url: &str) -> anyhow::Result<()> {
    sqlx::query("UPDATE shop_order SET stripe_session_id = $2, stripe_session_url = $3, updated_at = now() WHERE id = $1").bind(order_id).bind(session_id).bind(url).execute(pool).await?;
    Ok(())
}

/// Stripe refused to open a session: let go of the pen at once.
pub async fn fail_hold(pool: &PgPool, order_id: i32, note: &str) -> anyhow::Result<()> {
    sqlx::query("UPDATE shop_order SET status = 'failed', note = $2, updated_at = now() WHERE id = $1 AND status = 'open'").bind(order_id).bind(note).execute(pool).await?;
    Ok(())
}

/// The buyer let the pen go. Returns the session to close at Stripe, if any.
pub async fn release(pool: &PgPool, product_id: i32, visitor_key: &str) -> anyhow::Result<Option<String>> {
    let r = sqlx::query("UPDATE shop_order SET status = 'cancelled', updated_at = now() WHERE product_id = $1 AND visitor_key = $2 AND status = 'open' RETURNING stripe_session_id").bind(product_id).bind(visitor_key).fetch_optional(pool).await?;
    Ok(r.and_then(|r| r.get::<Option<String>, _>("stripe_session_id")))
}

/// Who holds a pen right now, for its page.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CurrentHold {
    pub mine: bool,
    pub status: String,
    pub url: String,
    pub minutes_left: i64,
}

pub async fn current_hold(pool: &PgPool, product_id: i32, visitor_key: &str) -> anyhow::Result<Option<CurrentHold>> {
    let r = sqlx::query(
        "SELECT visitor_key, status, stripe_session_url, GREATEST(0, CEIL(EXTRACT(EPOCH FROM (expires_at - now())) / 60))::int8 AS mins
         FROM shop_order WHERE product_id = $1 AND (status = 'processing' OR (status = 'open' AND expires_at > now()))",
    )
    .bind(product_id)
    .fetch_optional(pool)
    .await?;
    Ok(r.map(|r| CurrentHold { mine: r.get::<String, _>("visitor_key") == visitor_key && !visitor_key.is_empty(), status: r.get("status"), url: r.get("stripe_session_url"), minutes_left: r.get("mins") }))
}

/// Remember a webhook event. False when Stripe already delivered it (it retries, and may repeat).
pub async fn record_event(tx: &mut Transaction<'_, Postgres>, id: &str, kind: &str) -> anyhow::Result<bool> {
    let n = sqlx::query("INSERT INTO stripe_event (id, kind) VALUES ($1, $2) ON CONFLICT (id) DO NOTHING").bind(id).bind(kind).execute(&mut **tx).await?.rows_affected();
    Ok(n == 1)
}

#[derive(Clone, Debug)]
pub struct OrderRow {
    pub id: i32,
    pub product_id: i32,
    pub product_slug: String,
    pub product_title: String,
    pub product_sku: String,
    pub amount_cents: i64,
    pub shipping_cents: i64,
    pub amount_total_cents: Option<i64>,
    pub status: String,
    pub stripe_session_id: String,
    pub stripe_payment_intent: String,
    pub buyer_email: String,
    pub buyer_name: String,
    pub shipping_address: Option<serde_json::Value>,
    pub note: String,
    pub created_at: chrono::DateTime<chrono::Utc>,
    pub paid_at: Option<chrono::DateTime<chrono::Utc>>,
    /// The listing's description as it read when the buyer paid (copied in at that moment).
    pub product_description: String,
    /// "us" or "intl": where the buyer said it ships, before Checkout opened.
    pub ship_region: String,
    /// The shipping options the session offered.
    pub shipping_offered: Vec<ShippingOption>,
    /// The one Stripe charged for, by name (empty until paid, or when nothing matched).
    pub shipping_option: String,
}

impl OrderRow {
    /// "USPS Insured Priority, $15.00" / "$15.00" / "" (no shipping charged).
    pub fn shipping_label(&self) -> String {
        if self.shipping_cents <= 0 && self.shipping_option.is_empty() {
            return String::new();
        }
        let amount = crate::money::fmt_cents(self.shipping_cents);
        if self.shipping_option.is_empty() { amount } else { format!("{}, {amount}", self.shipping_option) }
    }
}

const ORDER_COLS: &str = "id, product_id, product_slug, product_title, product_sku, amount_cents, shipping_cents, amount_total_cents, status, COALESCE(stripe_session_id, '') AS stripe_session_id, stripe_payment_intent, buyer_email, buyer_name, shipping_address, note, created_at, paid_at, product_description, ship_region, shipping_offered, shipping_option";

fn order_from(r: &sqlx::postgres::PgRow) -> OrderRow {
    OrderRow {
        id: r.get("id"),
        product_id: r.get("product_id"),
        product_slug: r.get("product_slug"),
        product_title: r.get("product_title"),
        product_sku: r.get("product_sku"),
        amount_cents: r.get("amount_cents"),
        shipping_cents: r.get("shipping_cents"),
        amount_total_cents: r.get("amount_total_cents"),
        status: r.get("status"),
        stripe_session_id: r.get("stripe_session_id"),
        stripe_payment_intent: r.get("stripe_payment_intent"),
        buyer_email: r.get("buyer_email"),
        buyer_name: r.get("buyer_name"),
        shipping_address: r.get("shipping_address"),
        note: r.get("note"),
        created_at: r.get("created_at"),
        paid_at: r.get("paid_at"),
        product_description: r.get("product_description"),
        ship_region: r.get("ship_region"),
        shipping_offered: serde_json::from_value(r.get("shipping_offered")).unwrap_or_default(),
        shipping_option: r.get("shipping_option"),
    }
}

pub async fn by_session(pool: &PgPool, session_id: &str) -> anyhow::Result<Option<OrderRow>> {
    let r = sqlx::query(&format!("SELECT {ORDER_COLS} FROM shop_order WHERE stripe_session_id = $1")).bind(session_id).fetch_optional(pool).await?;
    Ok(r.as_ref().map(order_from))
}

/// Orders worth showing him: everything paid, refunded or still being paid, newest first. Holds that
/// lapsed or were let go are noise and stay out unless `all`.
pub async fn list(pool: &PgPool, all: bool, limit: i64) -> anyhow::Result<Vec<OrderRow>> {
    let rows = sqlx::query(&format!("SELECT {ORDER_COLS} FROM shop_order WHERE $1 OR status IN ('paid', 'processing', 'refunded') ORDER BY created_at DESC LIMIT $2")).bind(all).bind(limit).fetch_all(pool).await?;
    Ok(rows.iter().map(order_from).collect())
}

/// What a webhook did, for the handler to act on after (or, for a refund, before) committing.
#[derive(Clone, Debug)]
pub enum Outcome {
    /// The pen is now sold to this order.
    Sold(OrderRow),
    /// A bank payment Stripe is still collecting; the pen stays held.
    Processing(OrderRow),
    /// Paid for a pen that was already gone. Refund this payment intent, then commit.
    Refund { order: OrderRow, payment_intent: String },
    /// The hold ended without a payment.
    Released,
    /// Nothing to do (unknown session, or already handled).
    Ignored(&'static str),
}

async fn locked_order(tx: &mut Transaction<'_, Postgres>, s: &Session) -> anyhow::Result<Option<OrderRow>> {
    let r = sqlx::query(&format!("SELECT {ORDER_COLS} FROM shop_order WHERE stripe_session_id = $1 OR ($2::int IS NOT NULL AND id = $2 AND stripe_session_id IS NULL) ORDER BY id LIMIT 1 FOR UPDATE"))
        .bind(&s.id)
        .bind(s.order_id())
        .fetch_optional(&mut **tx)
        .await?;
    Ok(r.as_ref().map(order_from))
}

async fn reload(tx: &mut Transaction<'_, Postgres>, id: i32) -> anyhow::Result<OrderRow> {
    let r = sqlx::query(&format!("SELECT {ORDER_COLS} FROM shop_order WHERE id = $1")).bind(id).fetch_one(&mut **tx).await?;
    Ok(order_from(&r))
}

/// Store what Stripe collected (e-mail, name, address, totals, the shipping charged) on the order.
async fn store_buyer(tx: &mut Transaction<'_, Postgres>, order: &OrderRow, s: &Session) -> anyhow::Result<()> {
    let option = s.shipping_rate_amount().map(|c| crate::shipping::chosen_name(&order.shipping_offered, c)).unwrap_or_default();
    sqlx::query(
        "UPDATE shop_order SET stripe_session_id = $2, buyer_email = $3, buyer_name = $4, shipping_address = COALESCE($5, shipping_address),
                amount_total_cents = COALESCE($6, amount_total_cents), currency = COALESCE($7, currency),
                stripe_payment_intent = CASE WHEN $8 = '' THEN stripe_payment_intent ELSE $8 END,
                shipping_cents = COALESCE($9, shipping_cents), shipping_option = CASE WHEN $10 = '' THEN shipping_option ELSE $10 END,
                stripe_shipping_rate = CASE WHEN $11 = '' THEN stripe_shipping_rate ELSE $11 END, updated_at = now() WHERE id = $1",
    )
    .bind(order.id)
    .bind(&s.id)
    .bind(s.email())
    .bind(s.buyer_name())
    .bind(s.shipping_json())
    .bind(s.amount_total)
    .bind(s.currency.as_deref())
    .bind(s.payment_intent_id())
    .bind(s.shipping_charged())
    .bind(option)
    .bind(s.shipping_rate_id())
    .execute(&mut **tx)
    .await?;
    Ok(())
}

/// A note when what Stripe charged is not the pen's price plus one of the shipping options offered
/// (the pen still sells; he sees the note on the order). Empty when it adds up.
pub fn amount_check(order: &OrderRow, s: &Session) -> String {
    let Some(total) = s.amount_total else { return String::new() };
    let shipping = s.shipping_charged().unwrap_or(0);
    let rate = s.shipping_rate_amount().unwrap_or(shipping);
    let offered_ok = if order.shipping_offered.is_empty() { rate == 0 } else { order.shipping_offered.iter().any(|o| o.cents == rate) };
    let mut note = String::new();
    if total != order.amount_cents + shipping {
        note.push_str(&format!("Stripe charged {total} cents; the pen is {} and shipping {shipping}. ", order.amount_cents));
    }
    if !offered_ok {
        note.push_str(&format!("Shipping of {rate} cents was not one of the options offered. "));
    }
    note
}

/// The money arrived: mark the pen sold to this order, unless it is already sold or held by someone
/// else, in which case the payment goes back.
async fn settle(tx: &mut Transaction<'_, Postgres>, order: &OrderRow, s: &Session) -> anyhow::Result<Outcome> {
    let product: Option<String> = sqlx::query_scalar("SELECT status FROM product WHERE id = $1 FOR UPDATE").bind(order.product_id).fetch_optional(&mut **tx).await?;
    let other_hold: bool = sqlx::query_scalar("SELECT EXISTS (SELECT 1 FROM shop_order WHERE product_id = $1 AND id <> $2 AND (status = 'paid' OR status = 'processing' OR (status = 'open' AND expires_at > now())))").bind(order.product_id).bind(order.id).fetch_one(&mut **tx).await?;
    let mismatch = amount_check(order, s);
    if product.as_deref() != Some("live") || other_hold {
        let why = match product.as_deref() {
            None => "the pen is no longer in the catalog",
            Some("sold") => "the pen was already sold",
            Some("live") => "another buyer holds the pen",
            Some(_) => "the pen is not for sale",
        };
        sqlx::query("UPDATE shop_order SET status = 'refunded', note = $2, updated_at = now() WHERE id = $1").bind(order.id).bind(format!("{mismatch}Paid after {why}; refunded in full.")).execute(&mut **tx).await?;
        return Ok(Outcome::Refund { order: reload(tx, order.id).await?, payment_intent: s.payment_intent_id() });
    }
    sqlx::query("UPDATE product SET status = 'sold', updated_at = now() WHERE id = $1").bind(order.product_id).execute(&mut **tx).await?;
    sqlx::query("UPDATE shop_order SET status = 'paid', paid_at = now(), note = $2, product_description = (SELECT description_text FROM product WHERE id = $3), updated_at = now() WHERE id = $1").bind(order.id).bind(mismatch).bind(order.product_id).execute(&mut **tx).await?;
    Ok(Outcome::Sold(reload(tx, order.id).await?))
}

/// `checkout.session.completed`: paid by card (sold now) or by bank (held while Stripe collects).
pub async fn on_completed(tx: &mut Transaction<'_, Postgres>, s: &Session) -> anyhow::Result<Outcome> {
    let Some(order) = locked_order(tx, s).await? else { return Ok(Outcome::Ignored("no order for this session")) };
    if matches!(order.status.as_str(), "paid" | "refunded") {
        return Ok(Outcome::Ignored("order already settled"));
    }
    store_buyer(tx, &order, s).await?;
    if s.paid() {
        return settle(tx, &order, s).await;
    }
    // A bank payment: keep the pen out of other buyers' reach until Stripe says it cleared or failed.
    let n = sqlx::query("UPDATE shop_order SET status = 'processing', updated_at = now() WHERE id = $1 AND status = 'open'").bind(order.id).execute(&mut **tx).await?.rows_affected();
    if n == 0 {
        // The hold had already lapsed or been let go; the payment can still settle when it clears.
        sqlx::query("UPDATE shop_order SET note = 'Bank payment started after the hold ended.', updated_at = now() WHERE id = $1").bind(order.id).execute(&mut **tx).await?;
    }
    Ok(Outcome::Processing(reload(tx, order.id).await?))
}

/// `checkout.session.async_payment_succeeded`: the bank payment cleared.
pub async fn on_async_succeeded(tx: &mut Transaction<'_, Postgres>, s: &Session) -> anyhow::Result<Outcome> {
    let Some(order) = locked_order(tx, s).await? else { return Ok(Outcome::Ignored("no order for this session")) };
    if matches!(order.status.as_str(), "paid" | "refunded") {
        return Ok(Outcome::Ignored("order already settled"));
    }
    store_buyer(tx, &order, s).await?;
    settle(tx, &order, s).await
}

/// `checkout.session.async_payment_failed`: the bank payment bounced; the pen is free again.
pub async fn on_async_failed(tx: &mut Transaction<'_, Postgres>, s: &Session) -> anyhow::Result<Outcome> {
    let n = sqlx::query("UPDATE shop_order SET status = 'failed', note = 'The bank payment failed.', updated_at = now() WHERE stripe_session_id = $1 AND status IN ('open', 'processing')").bind(&s.id).execute(&mut **tx).await?.rows_affected();
    Ok(if n > 0 { Outcome::Released } else { Outcome::Ignored("no open order for this session") })
}

/// `checkout.session.expired`: the buyer never paid; the pen is free again.
pub async fn on_expired(tx: &mut Transaction<'_, Postgres>, s: &Session) -> anyhow::Result<Outcome> {
    let n = sqlx::query("UPDATE shop_order SET status = 'expired', updated_at = now() WHERE stripe_session_id = $1 AND status = 'open'").bind(&s.id).execute(&mut **tx).await?.rows_affected();
    Ok(if n > 0 { Outcome::Released } else { Outcome::Ignored("no open order for this session") })
}

#[cfg(test)]
mod tests {
    //! These need a Postgres with the migrations applied: `TEST_DATABASE_URL=… cargo test --lib`.
    //! Without it they say so and pass. Each test makes its own pen and removes it afterwards.
    use super::*;

    async fn pool() -> Option<PgPool> {
        let url = std::env::var("TEST_DATABASE_URL").ok()?;
        let pool = sqlx::postgres::PgPoolOptions::new().max_connections(4).connect(&url).await.ok()?;
        sqlx::migrate!("./migrations").run(&pool).await.ok()?;
        Some(pool)
    }

    async fn pen(pool: &PgPool, tag: &str, price: i64, sale: Option<i64>) -> ForSale {
        let slug = format!("zz-test-{tag}-{}", std::process::id());
        sqlx::query("DELETE FROM shop_order WHERE product_slug = $1").bind(&slug).execute(pool).await.unwrap();
        sqlx::query("DELETE FROM product WHERE slug = $1").bind(&slug).execute(pool).await.unwrap();
        let cat: i32 = sqlx::query_scalar("SELECT id FROM category ORDER BY id LIMIT 1").fetch_one(pool).await.unwrap();
        sqlx::query("INSERT INTO product (slug, title, short_title, sku, category_id, price_cents, sale_price_cents) VALUES ($1, 'Test pen', 'Test pen', 'T1', $2, $3, $4)").bind(&slug).bind(cat).bind(price).bind(sale).execute(pool).await.unwrap();
        for_sale(pool, &slug).await.unwrap().unwrap()
    }

    async fn cleanup(pool: &PgPool, p: &ForSale) {
        sqlx::query("DELETE FROM shop_order WHERE product_id = $1").bind(p.id).execute(pool).await.unwrap();
        sqlx::query("DELETE FROM product WHERE id = $1").bind(p.id).execute(pool).await.unwrap();
    }

    fn later(mins: i64) -> i64 {
        chrono::Utc::now().timestamp() + mins * 60
    }

    fn session(id: &str, order_id: i32, product_id: i32, paid: bool, total: i64) -> Session {
        Session::from_value(&serde_json::json!({"id": id, "payment_status": if paid { "paid" } else { "unpaid" }, "amount_total": total, "currency": "usd", "payment_intent": format!("pi_{id}"),
            "metadata": {"order_id": order_id.to_string(), "product_id": product_id.to_string()}, "customer_details": {"email": "buyer@example.com", "name": "B"}})).unwrap()
    }

    #[tokio::test]
    async fn price_comes_from_the_database_sale_price_first() {
        let Some(pool) = pool().await else { eprintln!("skipped: TEST_DATABASE_URL not set"); return };
        let p = pen(&pool, "price", 20000, Some(15000)).await;
        assert_eq!(p.amount_cents, 15000);
        assert!(p.buyable());
        sqlx::query("UPDATE product SET sale_price_cents = NULL WHERE id = $1").bind(p.id).execute(&pool).await.unwrap();
        assert_eq!(for_sale(&pool, &p.slug).await.unwrap().unwrap().amount_cents, 20000);
        cleanup(&pool, &p).await;
    }

    #[tokio::test]
    async fn one_buyer_holds_a_pen_at_a_time() {
        let Some(pool) = pool().await else { eprintln!("skipped: TEST_DATABASE_URL not set"); return };
        let p = pen(&pool, "hold", 10000, None).await;
        let Hold::Placed(id) = place_hold(&pool, &p, "visitor-a", Region::Us, &[], later(36)).await.unwrap() else { panic!("first buyer should get the hold") };
        attach_session(&pool, id, &format!("cs_test_hold{id}"), "https://checkout.stripe.com/c/pay/x").await.unwrap();
        assert_eq!(place_hold(&pool, &p, "visitor-b", Region::Us, &[], later(36)).await.unwrap(), Hold::Taken);
        assert_eq!(place_hold(&pool, &p, "visitor-a", Region::Us, &[], later(36)).await.unwrap(), Hold::Yours { order_id: id, url: "https://checkout.stripe.com/c/pay/x".into(), region: "us".into() });
        let h = current_hold(&pool, p.id, "visitor-b").await.unwrap().unwrap();
        assert!(!h.mine && h.minutes_left > 30);
        // A hold past its time lets the next buyer in.
        sqlx::query("UPDATE shop_order SET expires_at = now() - interval '1 minute' WHERE id = $1").bind(id).execute(&pool).await.unwrap();
        assert!(current_hold(&pool, p.id, "visitor-b").await.unwrap().is_none());
        assert!(matches!(place_hold(&pool, &p, "visitor-b", Region::Us, &[], later(36)).await.unwrap(), Hold::Placed(_)));
        // Letting go frees it.
        assert!(release(&pool, p.id, "visitor-b").await.unwrap().is_none());
        assert!(matches!(place_hold(&pool, &p, "visitor-c", Region::Us, &[], later(36)).await.unwrap(), Hold::Placed(_)));
        cleanup(&pool, &p).await;
    }

    #[tokio::test]
    async fn racing_buyers_get_one_hold() {
        let Some(pool) = pool().await else { eprintln!("skipped: TEST_DATABASE_URL not set"); return };
        let p = pen(&pool, "race", 10000, None).await;
        let tasks: Vec<_> = (0..8).map(|i| { let (pool, p) = (pool.clone(), p.clone()); tokio::spawn(async move { place_hold(&pool, &p, &format!("v{i}"), Region::Us, &[], later(36)).await.unwrap() }) }).collect();
        let mut placed = 0;
        for t in tasks {
            if matches!(t.await.unwrap(), Hold::Placed(_)) {
                placed += 1;
            }
        }
        assert_eq!(placed, 1);
        cleanup(&pool, &p).await;
    }

    fn with_shipping(id: &str, order_id: i32, product_id: i32, total: i64, shipping: i64) -> Session {
        Session::from_value(&serde_json::json!({"id": id, "payment_status": "paid", "amount_total": total, "currency": "usd", "payment_intent": format!("pi_{id}"),
            "shipping_cost": {"amount_subtotal": shipping, "amount_total": shipping, "shipping_rate": "shr_test"}, "total_details": {"amount_shipping": shipping},
            "metadata": {"order_id": order_id.to_string(), "product_id": product_id.to_string()}, "customer_details": {"email": "buyer@example.com", "name": "B"}})).unwrap()
    }

    #[tokio::test]
    async fn paid_session_sells_the_pen_once() {
        let Some(pool) = pool().await else { eprintln!("skipped: TEST_DATABASE_URL not set"); return };
        let p = pen(&pool, "sell", 10000, None).await;
        let Hold::Placed(id) = place_hold(&pool, &p, "va", Region::Us, &crate::shipping::default_list(), later(36)).await.unwrap() else { panic!() };
        let sid = format!("cs_test_sell{id}");
        attach_session(&pool, id, &sid, "u").await.unwrap();
        let s = with_shipping(&sid, id, p.id, 11500, 1500);
        let evt = format!("evt_test_sell{id}");

        let mut tx = pool.begin().await.unwrap();
        assert!(record_event(&mut tx, &evt, "checkout.session.completed").await.unwrap());
        let out = on_completed(&mut tx, &s).await.unwrap();
        tx.commit().await.unwrap();
        let Outcome::Sold(o) = out else { panic!("expected Sold, got {out:?}") };
        assert_eq!((o.status.as_str(), o.buyer_email.as_str(), o.amount_total_cents, o.note.as_str()), ("paid", "buyer@example.com", Some(11500), ""));
        assert_eq!((o.shipping_cents, o.shipping_option.as_str(), o.ship_region.as_str()), (1500, "USPS Insured Priority", "us"));
        assert_eq!(o.shipping_offered.len(), 2, "only the US options were offered");
        assert_eq!(o.shipping_label(), "USPS Insured Priority, $15.00");
        assert_eq!(for_sale(&pool, &p.slug).await.unwrap().unwrap().status, "sold");

        // Stripe delivers the same event again: recorded once, nothing happens.
        let mut tx = pool.begin().await.unwrap();
        assert!(!record_event(&mut tx, &evt, "checkout.session.completed").await.unwrap());
        tx.rollback().await.unwrap();
        // And a different event for the same session finds it settled.
        let mut tx = pool.begin().await.unwrap();
        assert!(matches!(on_completed(&mut tx, &s).await.unwrap(), Outcome::Ignored(_)));
        tx.rollback().await.unwrap();
        sqlx::query("DELETE FROM stripe_event WHERE id = $1").bind(&evt).execute(&pool).await.unwrap();
        cleanup(&pool, &p).await;
    }

    #[tokio::test]
    async fn settle_checks_price_plus_the_shipping_stripe_reports() {
        let Some(pool) = pool().await else { eprintln!("skipped: TEST_DATABASE_URL not set"); return };
        // Abroad: the one international rate, and every US-only total would be wrong.
        let p = pen(&pool, "intl", 20000, None).await;
        let Hold::Placed(id) = place_hold(&pool, &p, "va", Region::Intl, &crate::shipping::default_list(), later(36)).await.unwrap() else { panic!() };
        let sid = format!("cs_test_intl{id}");
        attach_session(&pool, id, &sid, "u").await.unwrap();
        let mut tx = pool.begin().await.unwrap();
        let out = on_completed(&mut tx, &with_shipping(&sid, id, p.id, 25500, 5500)).await.unwrap();
        tx.commit().await.unwrap();
        let Outcome::Sold(o) = out else { panic!("expected Sold, got {out:?}") };
        assert_eq!((o.note.as_str(), o.shipping_cents, o.shipping_option.as_str(), o.ship_region.as_str()), ("", 5500, "International Insured Priority", "intl"));
        cleanup(&pool, &p).await;

        // A charge that does not add up still sells (the money is in), with a note for him.
        let p = pen(&pool, "odd", 20000, None).await;
        let Hold::Placed(id) = place_hold(&pool, &p, "va", Region::Us, &crate::shipping::default_list(), later(36)).await.unwrap() else { panic!() };
        let sid = format!("cs_test_odd{id}");
        attach_session(&pool, id, &sid, "u").await.unwrap();
        let mut tx = pool.begin().await.unwrap();
        let out = on_completed(&mut tx, &with_shipping(&sid, id, p.id, 20700, 700)).await.unwrap();
        tx.commit().await.unwrap();
        let Outcome::Sold(o) = out else { panic!("expected Sold, got {out:?}") };
        assert!(o.note.contains("not one of the options offered"), "{}", o.note);
        assert!(!o.note.contains("Stripe charged"), "price + reported shipping adds up: {}", o.note);
        cleanup(&pool, &p).await;
    }

    #[test]
    fn amount_check_without_a_database() {
        let order = OrderRow {
            id: 1, product_id: 1, product_slug: String::new(), product_title: String::new(), product_sku: String::new(), amount_cents: 10000, shipping_cents: 0, amount_total_cents: None,
            status: "open".into(), stripe_session_id: String::new(), stripe_payment_intent: String::new(), buyer_email: String::new(), buyer_name: String::new(), shipping_address: None,
            note: String::new(), created_at: chrono::Utc::now(), paid_at: None, product_description: String::new(), ship_region: "us".into(),
            shipping_offered: crate::shipping::for_region(&crate::shipping::default_list(), Region::Us), shipping_option: String::new(),
        };
        assert_eq!(amount_check(&order, &with_shipping("cs_test_a", 1, 1, 10600, 600)), "");
        assert_eq!(amount_check(&order, &with_shipping("cs_test_b", 1, 1, 11500, 1500)), "");
        assert!(amount_check(&order, &with_shipping("cs_test_c", 1, 1, 10000, 600)).contains("Stripe charged 10000"));
        assert!(amount_check(&order, &with_shipping("cs_test_d", 1, 1, 15500, 5500)).contains("not one of the options"));
        let none = OrderRow { shipping_offered: vec![], ..order };
        assert_eq!(amount_check(&none, &session("cs_test_e", 1, 1, true, 10000)), "", "no options offered, no shipping charged");
    }

    #[tokio::test]
    async fn payment_for_a_pen_already_sold_is_refunded() {
        let Some(pool) = pool().await else { eprintln!("skipped: TEST_DATABASE_URL not set"); return };
        let p = pen(&pool, "gone", 10000, None).await;
        let Hold::Placed(id) = place_hold(&pool, &p, "va", Region::Us, &[], later(36)).await.unwrap() else { panic!() };
        let sid = format!("cs_test_gone{id}");
        attach_session(&pool, id, &sid, "u").await.unwrap();
        // He sold it by phone in the meantime.
        sqlx::query("UPDATE product SET status = 'sold' WHERE id = $1").bind(p.id).execute(&pool).await.unwrap();
        let mut tx = pool.begin().await.unwrap();
        let out = on_completed(&mut tx, &session(&sid, id, p.id, true, 10000)).await.unwrap();
        tx.commit().await.unwrap();
        let Outcome::Refund { order, payment_intent } = out else { panic!("expected Refund, got {out:?}") };
        assert_eq!((order.status.as_str(), payment_intent), ("refunded", format!("pi_{sid}")));
        assert!(order.note.contains("already sold"));
        cleanup(&pool, &p).await;
    }

    #[tokio::test]
    async fn bank_payment_holds_until_it_clears_or_fails() {
        let Some(pool) = pool().await else { eprintln!("skipped: TEST_DATABASE_URL not set"); return };
        let p = pen(&pool, "bank", 10000, None).await;
        let Hold::Placed(id) = place_hold(&pool, &p, "va", Region::Us, &[], later(36)).await.unwrap() else { panic!() };
        let sid = format!("cs_test_bank{id}");
        attach_session(&pool, id, &sid, "u").await.unwrap();
        let mut tx = pool.begin().await.unwrap();
        assert!(matches!(on_completed(&mut tx, &session(&sid, id, p.id, false, 10000)).await.unwrap(), Outcome::Processing(_)));
        tx.commit().await.unwrap();
        // Still held even after the card window would have closed.
        sqlx::query("UPDATE shop_order SET expires_at = now() - interval '1 hour' WHERE id = $1").bind(id).execute(&pool).await.unwrap();
        assert_eq!(place_hold(&pool, &p, "vb", Region::Us, &[], later(36)).await.unwrap(), Hold::Taken);
        let mut tx = pool.begin().await.unwrap();
        assert!(matches!(on_async_failed(&mut tx, &session(&sid, id, p.id, false, 10000)).await.unwrap(), Outcome::Released));
        tx.commit().await.unwrap();
        assert!(matches!(place_hold(&pool, &p, "vb", Region::Us, &[], later(36)).await.unwrap(), Hold::Placed(_)));
        cleanup(&pool, &p).await;
    }
}
