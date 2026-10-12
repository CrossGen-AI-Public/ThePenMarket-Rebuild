//! Trading Post payments: the $5 listing fee and the Unlimited Posts membership, both through Stripe
//! Checkout like the pens (hosted page, signed and recorded webhooks, refunds before commit).
//!
//! A listing posted through the site is a `tp_listing` row. Without a membership it waits in
//! `pending_payment` (never shown to the public) until its Checkout Session is paid; then it goes
//! `live`. A poster whose e-mail has a membership paid through a date still ahead skips the fee and
//! goes live at once. Memberships are yearly Stripe subscriptions; the paid invoice's period end is
//! the paid-through date. No foreign keys anywhere: the importer truncates `tp_listing`, and it
//! refuses while any listing posted here exists.

use crate::stripe::{InvoicePaid, Session, SubscriptionEnded};
use sqlx::{PgPool, Postgres, Row, Transaction};

/// The poster's own text as safe HTML: everything escaped, blank lines make paragraphs, single line
/// breaks stay. No links or tags from a public form.
pub fn plain_html(text: &str) -> String {
    let t = text.replace('\r', "");
    t.split("\n\n").map(str::trim).filter(|p| !p.is_empty()).map(|p| format!("<p>{}</p>\n", crate::text::escape(p).replace('\n', "<br>"))).collect()
}

/// What the poster typed, from the stored form submission.
#[derive(Clone, Debug, Default)]
pub struct NewListing {
    pub submission_id: i32,
    pub item: String,
    pub era: String,
    pub price: String,
    pub contact: String,
    pub description: String,
    pub email: String,
    pub photo_key: Option<String>,
}

#[derive(Clone, Debug)]
pub struct TpListing {
    pub id: i32,
    pub slug: String,
    pub title: String,
    pub status: String,
    pub paid_via: String,
    pub poster_email: String,
    pub form_submission_id: Option<i32>,
    pub photo_key: Option<String>,
    pub image_path: Option<String>,
    pub fee_cents: i64,
    pub amount_total_cents: Option<i64>,
    pub stripe_payment_intent: String,
    pub note: String,
    pub created_at: chrono::DateTime<chrono::Utc>,
    pub paid_at: Option<chrono::DateTime<chrono::Utc>>,
}

const LISTING_COLS: &str = "id, slug, title, status, paid_via, poster_email, form_submission_id, photo_key, image_path, fee_cents, amount_total_cents, stripe_payment_intent, note, created_at, paid_at";

fn listing_from(r: &sqlx::postgres::PgRow) -> TpListing {
    TpListing {
        id: r.get("id"),
        slug: r.get("slug"),
        title: r.get("title"),
        status: r.get("status"),
        paid_via: r.get("paid_via"),
        poster_email: r.get("poster_email"),
        form_submission_id: r.get("form_submission_id"),
        photo_key: r.get("photo_key"),
        image_path: r.get("image_path"),
        fee_cents: r.get("fee_cents"),
        amount_total_cents: r.get("amount_total_cents"),
        stripe_payment_intent: r.get("stripe_payment_intent"),
        note: r.get("note"),
        created_at: r.get("created_at"),
        paid_at: r.get("paid_at"),
    }
}

/// The date `email`'s membership is paid through, when that is still ahead.
pub async fn member_until(pool: &PgPool, email: &str) -> anyhow::Result<Option<chrono::DateTime<chrono::Utc>>> {
    let email = email.trim();
    if !email.contains('@') {
        return Ok(None);
    }
    Ok(sqlx::query_scalar("SELECT paid_through FROM tp_membership WHERE lower(email) = lower($1) AND paid_through > now() ORDER BY paid_through DESC LIMIT 1").bind(email).fetch_optional(pool).await?)
}

/// Store a listing from the form: `live` for a member (paid_via 'membership'), otherwise
/// `pending_payment` with the fee it owes.
pub async fn create_listing(pool: &PgPool, l: &NewListing, live: bool, fee_cents: i64) -> anyhow::Result<TpListing> {
    let mut tx = pool.begin().await?;
    let placeholder = format!("tp-new-{}-{}", l.submission_id, chrono::Utc::now().timestamp_nanos_opt().unwrap_or(0));
    let search = [l.item.as_str(), l.era.as_str(), l.price.as_str(), l.description.as_str()].join(" ");
    let id: i32 = sqlx::query_scalar(
        "INSERT INTO tp_listing (slug, title, era_text, price_text, contact_text, description_html, status, paid_via, published_at, modified_at, search_text,
                                 form_submission_id, poster_email, photo_key, fee_cents, image_alt)
         VALUES ($1, $2, $3, $4, $5, $6, $7, $8, now(), now(), $9, $10, $11, $12, $13, $2) RETURNING id",
    )
    .bind(&placeholder)
    .bind(l.item.trim())
    .bind(l.era.trim())
    .bind(l.price.trim())
    .bind(l.contact.trim())
    .bind(plain_html(&l.description))
    .bind(if live { "live" } else { "pending_payment" })
    .bind(if live { "membership" } else { "stripe" })
    .bind(search)
    .bind(l.submission_id)
    .bind(l.email.trim())
    .bind(&l.photo_key)
    .bind(if live { 0 } else { fee_cents })
    .fetch_one(&mut *tx)
    .await?;
    let base: String = crate::text::slugify(&crate::text::strip_tags(&l.item)).chars().take(60).collect::<String>().trim_matches('-').to_string();
    let mut slug = if base.is_empty() { format!("listing-{id}") } else { format!("{base}-{id}") };
    let taken: bool = sqlx::query_scalar("SELECT EXISTS (SELECT 1 FROM tp_listing WHERE slug = $1)").bind(&slug).fetch_one(&mut *tx).await?;
    if taken {
        slug = format!("{slug}-tp");
    }
    sqlx::query("UPDATE tp_listing SET slug = $2 WHERE id = $1").bind(id).bind(&slug).execute(&mut *tx).await?;
    let r = sqlx::query(&format!("SELECT {LISTING_COLS} FROM tp_listing WHERE id = $1")).bind(id).fetch_one(&mut *tx).await?;
    tx.commit().await?;
    Ok(listing_from(&r))
}

pub async fn attach_session(pool: &PgPool, listing_id: i32, session_id: &str) -> anyhow::Result<()> {
    sqlx::query("UPDATE tp_listing SET stripe_session_id = $2 WHERE id = $1").bind(listing_id).bind(session_id).execute(pool).await?;
    Ok(())
}

pub async fn note(pool: &PgPool, listing_id: i32, text: &str) -> anyhow::Result<()> {
    sqlx::query("UPDATE tp_listing SET note = $2 WHERE id = $1").bind(listing_id).bind(text).execute(pool).await?;
    Ok(())
}

/// The listing's photo, once copied into the public media folder (only when it goes live).
pub async fn set_image(pool: &PgPool, listing_id: i32, rel: &str) -> anyhow::Result<()> {
    sqlx::query("UPDATE tp_listing SET image_path = $2, modified_at = now() WHERE id = $1").bind(listing_id).bind(rel).execute(pool).await?;
    Ok(())
}

pub async fn listing_by_session(pool: &PgPool, session_id: &str) -> anyhow::Result<Option<TpListing>> {
    let r = sqlx::query(&format!("SELECT {LISTING_COLS} FROM tp_listing WHERE stripe_session_id = $1")).bind(session_id).fetch_optional(pool).await?;
    Ok(r.as_ref().map(listing_from))
}

/// Listings posted through the site (not imported), newest first, for the admin.
pub async fn posted_listings(pool: &PgPool, limit: i64) -> anyhow::Result<Vec<TpListing>> {
    let rows = sqlx::query(&format!("SELECT {LISTING_COLS} FROM tp_listing WHERE form_submission_id IS NOT NULL ORDER BY created_at DESC LIMIT $1")).bind(limit).fetch_all(pool).await?;
    Ok(rows.iter().map(listing_from).collect())
}

/// What a listing-fee webhook did.
#[derive(Clone, Debug)]
pub enum ListingOutcome {
    /// Paid: the listing is live now.
    Live(TpListing),
    /// A bank payment Stripe is still collecting; the listing stays pending.
    Processing(TpListing),
    /// Paid for a listing that was taken down meanwhile. Refund, then commit.
    Refund { listing: TpListing, payment_intent: String },
    /// Not paid (expired or failed): the listing stays pending and private.
    Unpaid(TpListing),
    Ignored(&'static str),
}

async fn locked_listing(tx: &mut Transaction<'_, Postgres>, s: &Session) -> anyhow::Result<Option<TpListing>> {
    let r = sqlx::query(&format!("SELECT {LISTING_COLS} FROM tp_listing WHERE stripe_session_id = $1 OR ($2::int IS NOT NULL AND id = $2 AND stripe_session_id IS NULL) ORDER BY id LIMIT 1 FOR UPDATE"))
        .bind(&s.id)
        .bind(s.listing_id())
        .fetch_optional(&mut **tx)
        .await?;
    Ok(r.as_ref().map(listing_from))
}

async fn reload(tx: &mut Transaction<'_, Postgres>, id: i32) -> anyhow::Result<TpListing> {
    let r = sqlx::query(&format!("SELECT {LISTING_COLS} FROM tp_listing WHERE id = $1")).bind(id).fetch_one(&mut **tx).await?;
    Ok(listing_from(&r))
}

async fn settle_listing(tx: &mut Transaction<'_, Postgres>, l: &TpListing, s: &Session) -> anyhow::Result<ListingOutcome> {
    let mismatch = match s.amount_total {
        Some(t) if t != l.fee_cents => format!("Stripe charged {t} cents; the fee is {}. ", l.fee_cents),
        _ => String::new(),
    };
    if l.status != "pending_payment" {
        sqlx::query("UPDATE tp_listing SET note = $2 WHERE id = $1").bind(l.id).bind(format!("{mismatch}Paid after the listing was {}; refunded in full.", if l.status == "removed" { "taken down" } else { "closed" })).execute(&mut **tx).await?;
        return Ok(ListingOutcome::Refund { listing: reload(tx, l.id).await?, payment_intent: s.payment_intent_id() });
    }
    sqlx::query("UPDATE tp_listing SET status = 'live', paid_via = 'stripe', paid_at = now(), published_at = now(), modified_at = now(), note = $2 WHERE id = $1").bind(l.id).bind(mismatch).execute(&mut **tx).await?;
    Ok(ListingOutcome::Live(reload(tx, l.id).await?))
}

async fn store_payer(tx: &mut Transaction<'_, Postgres>, id: i32, s: &Session) -> anyhow::Result<()> {
    sqlx::query(
        "UPDATE tp_listing SET stripe_session_id = $2, poster_email = CASE WHEN poster_email = '' THEN $3 ELSE poster_email END, amount_total_cents = COALESCE($4, amount_total_cents),
                stripe_payment_intent = CASE WHEN $5 = '' THEN stripe_payment_intent ELSE $5 END WHERE id = $1",
    )
    .bind(id)
    .bind(&s.id)
    .bind(s.email())
    .bind(s.amount_total)
    .bind(s.payment_intent_id())
    .execute(&mut **tx)
    .await?;
    Ok(())
}

/// `checkout.session.completed` for a listing fee (and `async_payment_succeeded`, with `cleared`).
pub async fn on_listing_paid(tx: &mut Transaction<'_, Postgres>, s: &Session, cleared: bool) -> anyhow::Result<ListingOutcome> {
    let Some(l) = locked_listing(tx, s).await? else { return Ok(ListingOutcome::Ignored("no listing for this session")) };
    if l.status == "live" || l.paid_at.is_some() || l.note.contains("refunded in full") {
        return Ok(ListingOutcome::Ignored("listing already settled"));
    }
    store_payer(tx, l.id, s).await?;
    if s.paid() || cleared {
        return settle_listing(tx, &l, s).await;
    }
    sqlx::query("UPDATE tp_listing SET note = 'Bank payment started; the listing goes live when it clears.' WHERE id = $1").bind(l.id).execute(&mut **tx).await?;
    Ok(ListingOutcome::Processing(reload(tx, l.id).await?))
}

/// `checkout.session.expired` / `async_payment_failed`: nothing paid; the listing stays private.
pub async fn on_listing_unpaid(tx: &mut Transaction<'_, Postgres>, s: &Session, why: &str) -> anyhow::Result<ListingOutcome> {
    let Some(l) = locked_listing(tx, s).await? else { return Ok(ListingOutcome::Ignored("no listing for this session")) };
    if l.status != "pending_payment" {
        return Ok(ListingOutcome::Ignored("listing is not waiting for payment"));
    }
    sqlx::query("UPDATE tp_listing SET note = $2 WHERE id = $1").bind(l.id).bind(why).execute(&mut **tx).await?;
    Ok(ListingOutcome::Unpaid(reload(tx, l.id).await?))
}

// ---- Unlimited Posts --------------------------------------------------------------------------------

#[derive(Clone, Debug)]
pub struct Member {
    pub id: i32,
    pub email: String,
    pub status: String,
    pub stripe_subscription: String,
    pub paid_through: chrono::DateTime<chrono::Utc>,
    pub created_at: chrono::DateTime<chrono::Utc>,
}

const MEMBER_COLS: &str = "id, email, status, stripe_subscription, paid_through, created_at";

fn member_from(r: &sqlx::postgres::PgRow) -> Member {
    Member { id: r.get("id"), email: r.get("email"), status: r.get("status"), stripe_subscription: r.get("stripe_subscription"), paid_through: r.get("paid_through"), created_at: r.get("created_at") }
}

pub async fn members(pool: &PgPool, limit: i64) -> anyhow::Result<Vec<Member>> {
    let rows = sqlx::query(&format!("SELECT {MEMBER_COLS} FROM tp_membership ORDER BY paid_through DESC LIMIT $1")).bind(limit).fetch_all(pool).await?;
    Ok(rows.iter().map(member_from).collect())
}

pub async fn member_by_session(pool: &PgPool, session_id: &str) -> anyhow::Result<Option<Member>> {
    let r = sqlx::query(&format!("SELECT {MEMBER_COLS} FROM tp_membership WHERE stripe_session_id = $1")).bind(session_id).fetch_optional(pool).await?;
    Ok(r.as_ref().map(member_from))
}

#[derive(Clone, Debug)]
pub enum MemberOutcome {
    Joined(Member),
    Renewed(Member),
    Ended(Member),
    Ignored(&'static str),
}

/// `checkout.session.completed` in subscription mode. Until the first invoice says exactly, a new
/// membership runs one year (the subscription's interval) from now.
pub async fn on_membership_completed(tx: &mut Transaction<'_, Postgres>, s: &Session) -> anyhow::Result<MemberOutcome> {
    let sub = s.subscription_id();
    if sub.is_empty() {
        return Ok(MemberOutcome::Ignored("membership session without a subscription"));
    }
    if !s.paid() {
        return Ok(MemberOutcome::Ignored("membership not paid yet; invoice.paid will record it"));
    }
    let r = sqlx::query(&format!(
        "INSERT INTO tp_membership (email, stripe_subscription, stripe_customer, stripe_session_id, paid_through) VALUES ($1, $2, $3, $4, now() + interval '1 year')
         ON CONFLICT (stripe_subscription) DO UPDATE SET email = CASE WHEN EXCLUDED.email <> '' THEN EXCLUDED.email ELSE tp_membership.email END,
             stripe_customer = CASE WHEN EXCLUDED.stripe_customer <> '' THEN EXCLUDED.stripe_customer ELSE tp_membership.stripe_customer END,
             stripe_session_id = EXCLUDED.stripe_session_id, status = 'active', updated_at = now()
         RETURNING {MEMBER_COLS}"
    ))
    .bind(s.email().trim())
    .bind(&sub)
    .bind(s.customer_id())
    .bind(&s.id)
    .fetch_one(&mut **tx)
    .await?;
    Ok(MemberOutcome::Joined(member_from(&r)))
}

/// `invoice.paid`: the first year or a renewal. Only invoices of an Unlimited Posts subscription (by
/// its metadata, or one already on file) count.
pub async fn on_invoice_paid(tx: &mut Transaction<'_, Postgres>, inv: &InvoicePaid) -> anyhow::Result<MemberOutcome> {
    if inv.subscription.is_empty() {
        return Ok(MemberOutcome::Ignored("invoice without a subscription"));
    }
    let known: bool = sqlx::query_scalar("SELECT EXISTS (SELECT 1 FROM tp_membership WHERE stripe_subscription = $1)").bind(&inv.subscription).fetch_one(&mut **tx).await?;
    if !known && inv.kind != crate::stripe::KIND_MEMBERSHIP {
        return Ok(MemberOutcome::Ignored("not an Unlimited Posts subscription"));
    }
    let Some(end) = inv.period_end else { return Ok(MemberOutcome::Ignored("invoice without a period")) };
    let r = sqlx::query(&format!(
        "INSERT INTO tp_membership (email, stripe_subscription, stripe_customer, paid_through) VALUES ($1, $2, $3, to_timestamp($4))
         ON CONFLICT (stripe_subscription) DO UPDATE SET paid_through = GREATEST(tp_membership.paid_through, EXCLUDED.paid_through), status = 'active',
             email = CASE WHEN tp_membership.email = '' THEN EXCLUDED.email ELSE tp_membership.email END, updated_at = now()
         RETURNING {MEMBER_COLS}"
    ))
    .bind(inv.email.trim())
    .bind(&inv.subscription)
    .bind(&inv.customer)
    .bind(end as f64)
    .fetch_one(&mut **tx)
    .await?;
    Ok(if known { MemberOutcome::Renewed(member_from(&r)) } else { MemberOutcome::Joined(member_from(&r)) })
}

/// `customer.subscription.deleted`: the membership ends when the subscription did (never later than
/// what was paid for).
pub async fn on_subscription_ended(tx: &mut Transaction<'_, Postgres>, e: &SubscriptionEnded) -> anyhow::Result<MemberOutcome> {
    let r = sqlx::query(&format!(
        "UPDATE tp_membership SET status = 'cancelled', paid_through = LEAST(paid_through, COALESCE(to_timestamp($2), now())), updated_at = now()
         WHERE stripe_subscription = $1 RETURNING {MEMBER_COLS}"
    ))
    .bind(&e.subscription)
    .bind(e.ended_at.map(|t| t as f64))
    .fetch_optional(&mut **tx)
    .await?;
    Ok(r.as_ref().map(|r| MemberOutcome::Ended(member_from(r))).unwrap_or(MemberOutcome::Ignored("no membership for this subscription")))
}

#[cfg(test)]
mod tests {
    //! Need `TEST_DATABASE_URL` (migrations applied); without it they say so and pass.
    use super::*;

    async fn pool() -> Option<PgPool> {
        let url = std::env::var("TEST_DATABASE_URL").ok()?;
        let pool = sqlx::postgres::PgPoolOptions::new().max_connections(4).connect(&url).await.ok()?;
        sqlx::migrate!("./migrations").run(&pool).await.ok()?;
        Some(pool)
    }

    fn new(tag: &str, email: &str) -> NewListing {
        NewListing { submission_id: 900_000 + std::process::id() as i32 % 1000, item: format!("Test <b>{tag}</b> Pelikan"), era: "1950s".into(), price: "$450".into(), contact: "Sam".into(), description: "Line one\nline <two>\n\nPara".into(), email: email.into(), photo_key: None }
    }

    fn paid_session(id: &str, listing_id: i32, total: i64, paid: bool) -> Session {
        Session::from_value(&serde_json::json!({"id": id, "payment_status": if paid { "paid" } else { "unpaid" }, "amount_total": total, "payment_intent": format!("pi_{id}"),
            "metadata": {"kind": "tp_listing", "listing_id": listing_id.to_string()}, "customer_details": {"email": "poster@example.com"}})).unwrap()
    }

    async fn drop_listing(pool: &PgPool, id: i32) {
        sqlx::query("DELETE FROM tp_listing WHERE id = $1").bind(id).execute(pool).await.unwrap();
    }

    #[test]
    fn poster_text_is_escaped() {
        assert_eq!(plain_html("a <script>\nb\n\n\nc & d"), "<p>a &lt;script&gt;<br>b</p>\n<p>c &amp; d</p>\n");
    }

    #[tokio::test]
    async fn listing_waits_for_its_fee_then_goes_live_once() {
        let Some(pool) = pool().await else { eprintln!("skipped: TEST_DATABASE_URL not set"); return };
        let l = create_listing(&pool, &new("fee", "poster@example.com"), false, 500).await.unwrap();
        assert_eq!((l.status.as_str(), l.paid_via.as_str(), l.fee_cents), ("pending_payment", "stripe", 500));
        assert!(l.slug.starts_with("test-fee-pelikan-") && l.slug.ends_with(&l.id.to_string()), "{}", l.slug);
        let html: String = sqlx::query_scalar("SELECT description_html FROM tp_listing WHERE id = $1").bind(l.id).fetch_one(&pool).await.unwrap();
        assert!(html.contains("&lt;two&gt;") && !html.contains("<two>"));
        let sid = format!("cs_test_tp{}", l.id);
        attach_session(&pool, l.id, &sid).await.unwrap();

        // Expired session: still pending, still private.
        let mut tx = pool.begin().await.unwrap();
        assert!(matches!(on_listing_unpaid(&mut tx, &paid_session(&sid, l.id, 500, false), "expired").await.unwrap(), ListingOutcome::Unpaid(_)));
        tx.commit().await.unwrap();

        let mut tx = pool.begin().await.unwrap();
        let out = on_listing_paid(&mut tx, &paid_session(&sid, l.id, 500, true), false).await.unwrap();
        tx.commit().await.unwrap();
        let ListingOutcome::Live(live) = out else { panic!("expected Live, got {out:?}") };
        assert_eq!((live.status.as_str(), live.note.as_str(), live.amount_total_cents), ("live", "", Some(500)));
        assert!(live.paid_at.is_some());
        let mut tx = pool.begin().await.unwrap();
        assert!(matches!(on_listing_paid(&mut tx, &paid_session(&sid, l.id, 500, true), false).await.unwrap(), ListingOutcome::Ignored(_)));
        tx.rollback().await.unwrap();
        drop_listing(&pool, l.id).await;
    }

    #[tokio::test]
    async fn listing_taken_down_before_payment_is_refunded() {
        let Some(pool) = pool().await else { eprintln!("skipped: TEST_DATABASE_URL not set"); return };
        let l = create_listing(&pool, &new("gone", ""), false, 500).await.unwrap();
        let sid = format!("cs_test_tpg{}", l.id);
        attach_session(&pool, l.id, &sid).await.unwrap();
        sqlx::query("UPDATE tp_listing SET status = 'removed' WHERE id = $1").bind(l.id).execute(&pool).await.unwrap();
        let mut tx = pool.begin().await.unwrap();
        let out = on_listing_paid(&mut tx, &paid_session(&sid, l.id, 500, true), false).await.unwrap();
        tx.commit().await.unwrap();
        let ListingOutcome::Refund { listing, payment_intent } = out else { panic!("expected Refund, got {out:?}") };
        assert_eq!((listing.status.as_str(), payment_intent), ("removed", format!("pi_{sid}")));
        assert_eq!(listing.poster_email, "poster@example.com", "taken from Stripe when the form had none");
        drop_listing(&pool, l.id).await;
    }

    #[tokio::test]
    async fn membership_lifecycle() {
        let Some(pool) = pool().await else { eprintln!("skipped: TEST_DATABASE_URL not set"); return };
        let tag = std::process::id();
        let email = format!("Member{tag}@Example.com");
        let sub = format!("sub_test_{tag}");
        sqlx::query("DELETE FROM tp_membership WHERE stripe_subscription = $1").bind(&sub).execute(&pool).await.unwrap();
        assert!(member_until(&pool, &email).await.unwrap().is_none());
        // invoice.paid may arrive before checkout.session.completed.
        let end = chrono::Utc::now().timestamp() + 365 * 86400;
        let inv = InvoicePaid { subscription: sub.clone(), kind: "unlimited_posts".into(), email: email.clone(), customer: "cus_1".into(), period_end: Some(end) };
        let mut tx = pool.begin().await.unwrap();
        assert!(matches!(on_invoice_paid(&mut tx, &inv).await.unwrap(), MemberOutcome::Joined(_)));
        tx.commit().await.unwrap();
        let s = Session::from_value(&serde_json::json!({"id": format!("cs_test_m{tag}"), "mode": "subscription", "payment_status": "paid", "subscription": sub, "customer": "cus_1",
            "metadata": {"kind": "unlimited_posts"}, "customer_details": {"email": email}})).unwrap();
        let mut tx = pool.begin().await.unwrap();
        let MemberOutcome::Joined(m) = on_membership_completed(&mut tx, &s).await.unwrap() else { panic!() };
        tx.commit().await.unwrap();
        assert!((m.paid_through.timestamp() - end).abs() < 5, "the invoice's period end is kept");
        // Case and spaces do not matter.
        assert!(member_until(&pool, &format!(" member{tag}@example.COM ")).await.unwrap().is_some());
        // Someone else's subscription invoice is ignored.
        let mut tx = pool.begin().await.unwrap();
        assert!(matches!(on_invoice_paid(&mut tx, &InvoicePaid { subscription: "sub_other".into(), ..Default::default() }).await.unwrap(), MemberOutcome::Ignored(_)));
        // Renewal moves the date forward.
        let next = InvoicePaid { period_end: Some(end + 365 * 86400), kind: String::new(), ..inv.clone() };
        let MemberOutcome::Renewed(m) = on_invoice_paid(&mut tx, &next).await.unwrap() else { panic!() };
        assert_eq!(m.paid_through.timestamp(), end + 365 * 86400);
        tx.commit().await.unwrap();
        // Cancelled now: no longer a member.
        let mut tx = pool.begin().await.unwrap();
        assert!(matches!(on_subscription_ended(&mut tx, &SubscriptionEnded { subscription: sub.clone(), kind: String::new(), ended_at: Some(chrono::Utc::now().timestamp() - 1) }).await.unwrap(), MemberOutcome::Ended(_)));
        tx.commit().await.unwrap();
        assert!(member_until(&pool, &email).await.unwrap().is_none());
        sqlx::query("DELETE FROM tp_membership WHERE stripe_subscription = $1").bind(&sub).execute(&pool).await.unwrap();
    }
}
