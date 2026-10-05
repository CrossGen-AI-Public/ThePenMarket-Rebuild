-- Stripe checkout for one-of-a-kind pens (2026-10-05). A `shop_order` row in status 'open' (card) or
-- 'processing' (a bank payment Stripe is still collecting) holds its pen; the partial unique index
-- below allows one such hold per pen, so two buyers can never both pay for the same pen.
--
-- No foreign key to product on purpose: the importer truncates product with CASCADE, and an order is
-- a record of money taken that must outlive any re-import. Title, slug and SKU are copied in.

CREATE TABLE shop_order (
    id                    SERIAL PRIMARY KEY,
    product_id            INTEGER NOT NULL,
    product_slug          TEXT NOT NULL,
    product_title         TEXT NOT NULL,
    product_sku           TEXT NOT NULL DEFAULT '',
    amount_cents          BIGINT NOT NULL CHECK (amount_cents >= 0),        -- the pen, from the database at the click
    shipping_cents        BIGINT NOT NULL DEFAULT 0 CHECK (shipping_cents >= 0),
    amount_total_cents    BIGINT,                                           -- what Stripe says it charged
    currency              TEXT NOT NULL DEFAULT 'usd',
    status                TEXT NOT NULL DEFAULT 'open' CHECK (status IN ('open', 'processing', 'paid', 'expired', 'cancelled', 'failed', 'refunded')),
    stripe_session_id     TEXT UNIQUE,
    stripe_session_url    TEXT NOT NULL DEFAULT '',
    stripe_payment_intent TEXT NOT NULL DEFAULT '',
    visitor_key           TEXT NOT NULL DEFAULT '',                         -- sha256 of the buyer's CSRF cookie: "is this hold mine?"
    buyer_email           TEXT NOT NULL DEFAULT '',
    buyer_name            TEXT NOT NULL DEFAULT '',
    shipping_address      JSONB,
    note                  TEXT NOT NULL DEFAULT '',
    expires_at            TIMESTAMPTZ NOT NULL,
    paid_at               TIMESTAMPTZ,
    created_at            TIMESTAMPTZ NOT NULL DEFAULT now(),
    updated_at            TIMESTAMPTZ NOT NULL DEFAULT now()
);
CREATE UNIQUE INDEX shop_order_one_hold_idx ON shop_order (product_id) WHERE status IN ('open', 'processing');
CREATE INDEX shop_order_status_idx ON shop_order (status, created_at DESC);

-- Every webhook event processed, so a retried or repeated delivery does nothing twice.
CREATE TABLE stripe_event (
    id          TEXT PRIMARY KEY,
    kind        TEXT NOT NULL,
    received_at TIMESTAMPTZ NOT NULL DEFAULT now()
);
