-- Nathaniel's shipping rates and Trading Post payments (2026-10-12).
--
-- Pens: the buyer says where the pen ships before Checkout opens; the order keeps where that was,
-- the options it was offered, and which one Stripe charged for (shipping_cents is now what Stripe
-- reports, not a flat rate copied in at the click).
ALTER TABLE shop_order
    ADD COLUMN ship_region          TEXT NOT NULL DEFAULT 'us' CHECK (ship_region IN ('us', 'intl')),
    ADD COLUMN shipping_offered     JSONB NOT NULL DEFAULT '[]'::jsonb,
    ADD COLUMN shipping_option      TEXT NOT NULL DEFAULT '',
    ADD COLUMN stripe_shipping_rate TEXT NOT NULL DEFAULT '';

-- Trading Post listings posted through the site. Imported listings keep paid_via = 'imported' and none
-- of these columns. A listing waits in 'pending_payment' (not public) until its $5 fee is paid, or goes
-- straight to 'live' for an Unlimited Posts member. The importer truncates tp_listing, so it refuses
-- while any listing posted here exists.
ALTER TABLE tp_listing
    ADD COLUMN form_submission_id    INTEGER,
    ADD COLUMN poster_email          TEXT NOT NULL DEFAULT '',
    ADD COLUMN photo_key             TEXT,
    ADD COLUMN fee_cents             BIGINT NOT NULL DEFAULT 0,
    ADD COLUMN amount_total_cents    BIGINT,
    ADD COLUMN stripe_session_id     TEXT UNIQUE,
    ADD COLUMN stripe_payment_intent TEXT NOT NULL DEFAULT '',
    ADD COLUMN paid_at               TIMESTAMPTZ,
    ADD COLUMN note                  TEXT NOT NULL DEFAULT '',
    ADD COLUMN created_at            TIMESTAMPTZ NOT NULL DEFAULT now();
CREATE INDEX tp_listing_submission_idx ON tp_listing (form_submission_id) WHERE form_submission_id IS NOT NULL;

-- Unlimited Posts members: a yearly Stripe subscription. paid_through comes from Stripe (the paid
-- invoice's period end); a member is anyone whose paid_through is still ahead. No foreign keys: like
-- shop_order, a record of money taken outlives any re-import.
CREATE TABLE tp_membership (
    id                  SERIAL PRIMARY KEY,
    email               TEXT NOT NULL,
    stripe_subscription TEXT NOT NULL UNIQUE,
    stripe_customer     TEXT NOT NULL DEFAULT '',
    stripe_session_id   TEXT UNIQUE,
    status              TEXT NOT NULL DEFAULT 'active' CHECK (status IN ('active', 'cancelled')),
    paid_through        TIMESTAMPTZ NOT NULL,
    created_at          TIMESTAMPTZ NOT NULL DEFAULT now(),
    updated_at          TIMESTAMPTZ NOT NULL DEFAULT now()
);
CREATE INDEX tp_membership_email_idx ON tp_membership (lower(email), paid_through DESC);
