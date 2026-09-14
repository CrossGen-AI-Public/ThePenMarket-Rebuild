-- ThePenMarket.com rebuild: initial schema (ADR-0001..0006 in research/architecture.md).
-- Money is integer cents. Every public URL is a first-class slug. State machines over booleans.

CREATE TABLE category (
    id          SERIAL PRIMARY KEY,
    wp_id       INTEGER UNIQUE,
    slug        TEXT NOT NULL UNIQUE,
    name        TEXT NOT NULL,
    sort_order  INTEGER NOT NULL DEFAULT 0,
    intro       TEXT NOT NULL DEFAULT '',
    meta_description TEXT NOT NULL DEFAULT ''
);

CREATE TABLE brand (
    id          SERIAL PRIMARY KEY,
    wp_id       INTEGER UNIQUE,
    slug        TEXT NOT NULL UNIQUE,
    name        TEXT NOT NULL,
    sort_order  INTEGER NOT NULL DEFAULT 0,
    definition  TEXT NOT NULL DEFAULT ''
);

CREATE TABLE era (
    id          SERIAL PRIMARY KEY,
    wp_id       INTEGER UNIQUE,
    slug        TEXT NOT NULL UNIQUE,
    name        TEXT NOT NULL,
    sort_order  INTEGER NOT NULL DEFAULT 0,
    definition  TEXT NOT NULL DEFAULT ''
);

CREATE TABLE nib (
    id          SERIAL PRIMARY KEY,
    wp_id       INTEGER UNIQUE,
    slug        TEXT NOT NULL UNIQUE,
    name        TEXT NOT NULL,
    sort_order  INTEGER NOT NULL DEFAULT 0,
    definition  TEXT NOT NULL DEFAULT ''
);

CREATE TABLE filling_mechanism (
    id          SERIAL PRIMARY KEY,
    wp_id       INTEGER UNIQUE,
    slug        TEXT NOT NULL UNIQUE,
    name        TEXT NOT NULL,
    sort_order  INTEGER NOT NULL DEFAULT 0,
    definition  TEXT NOT NULL DEFAULT '',
    repairable  BOOLEAN NOT NULL DEFAULT FALSE   -- in his published repair scope
);

CREATE TABLE price_range (
    id          SERIAL PRIMARY KEY,
    wp_id       INTEGER UNIQUE,
    slug        TEXT NOT NULL UNIQUE,
    name        TEXT NOT NULL,
    min_cents   BIGINT NOT NULL,
    max_cents   BIGINT,                          -- NULL = no upper bound
    sort_order  INTEGER NOT NULL DEFAULT 0
);

CREATE TABLE product (
    id                  SERIAL PRIMARY KEY,
    wp_id               INTEGER UNIQUE,
    sku                 TEXT NOT NULL DEFAULT '',
    slug                TEXT NOT NULL UNIQUE,
    title               TEXT NOT NULL,           -- "Vintage Pens: Montblanc 644" (his naming scheme)
    short_title         TEXT NOT NULL,           -- "Montblanc 644"
    category_id         INTEGER NOT NULL REFERENCES category(id),
    brand_id            INTEGER REFERENCES brand(id),
    era_id              INTEGER REFERENCES era(id),
    nib_id              INTEGER REFERENCES nib(id),
    filling_mechanism_id INTEGER REFERENCES filling_mechanism(id),
    price_cents         BIGINT NOT NULL CHECK (price_cents >= 0),
    sale_price_cents    BIGINT CHECK (sale_price_cents IS NULL OR sale_price_cents >= 0),
    length_mm           NUMERIC(6,1),            -- "13.65cm capped" in his prose
    status              TEXT NOT NULL DEFAULT 'live' CHECK (status IN ('draft','live','reserved','sold','archived')),
    is_subscription     BOOLEAN NOT NULL DEFAULT FALSE,
    description_html    TEXT NOT NULL DEFAULT '',
    description_text    TEXT NOT NULL DEFAULT '',
    meta_description    TEXT NOT NULL DEFAULT '',
    listed_year         INTEGER,
    listed_month        INTEGER,
    search_text         TEXT NOT NULL DEFAULT '',
    search_tsv          TSVECTOR GENERATED ALWAYS AS (to_tsvector('english', search_text)) STORED,
    created_at          TIMESTAMPTZ NOT NULL DEFAULT now(),
    updated_at          TIMESTAMPTZ NOT NULL DEFAULT now()
);
CREATE INDEX product_search_idx ON product USING GIN (search_tsv);
CREATE INDEX product_category_idx ON product (category_id);
CREATE INDEX product_brand_idx ON product (brand_id);
CREATE INDEX product_era_idx ON product (era_id);
CREATE INDEX product_nib_idx ON product (nib_id);
CREATE INDEX product_fm_idx ON product (filling_mechanism_id);
CREATE INDEX product_price_idx ON product (price_cents);
CREATE INDEX product_status_idx ON product (status);

CREATE TABLE product_image (
    id          SERIAL PRIMARY KEY,
    product_id  INTEGER NOT NULL REFERENCES product(id) ON DELETE CASCADE,
    position    INTEGER NOT NULL DEFAULT 0,
    path        TEXT NOT NULL,                   -- relative to the media dir, e.g. uploads/2026/08/6967-Montblanc-644.jpg
    original_url TEXT NOT NULL DEFAULT '',
    alt         TEXT NOT NULL DEFAULT '',
    width       INTEGER,
    height      INTEGER,
    has_480     BOOLEAN NOT NULL DEFAULT FALSE,
    has_960     BOOLEAN NOT NULL DEFAULT FALSE
);
CREATE INDEX product_image_product_idx ON product_image (product_id, position);

CREATE TABLE blog_category (
    id          SERIAL PRIMARY KEY,
    wp_id       INTEGER UNIQUE,
    slug        TEXT NOT NULL UNIQUE,
    name        TEXT NOT NULL,
    description TEXT NOT NULL DEFAULT '',
    sort_order  INTEGER NOT NULL DEFAULT 0
);

CREATE TABLE post (
    id              SERIAL PRIMARY KEY,
    wp_id           INTEGER UNIQUE,
    slug            TEXT NOT NULL UNIQUE,
    title           TEXT NOT NULL,
    body_html       TEXT NOT NULL,
    excerpt         TEXT NOT NULL DEFAULT '',
    meta_description TEXT NOT NULL DEFAULT '',
    category_id     INTEGER REFERENCES blog_category(id),
    author_name     TEXT NOT NULL DEFAULT 'Nathaniel Cerf',
    featured_path   TEXT,
    featured_alt    TEXT NOT NULL DEFAULT '',
    published_at    TIMESTAMPTZ NOT NULL,
    modified_at     TIMESTAMPTZ NOT NULL,
    word_count      INTEGER NOT NULL DEFAULT 0,
    search_text     TEXT NOT NULL DEFAULT '',
    search_tsv      TSVECTOR GENERATED ALWAYS AS (to_tsvector('english', search_text)) STORED
);
CREATE INDEX post_search_idx ON post USING GIN (search_tsv);
CREATE INDEX post_published_idx ON post (published_at DESC);
CREATE INDEX post_category_idx ON post (category_id);

CREATE TABLE page (
    id              SERIAL PRIMARY KEY,
    wp_id           INTEGER UNIQUE,
    slug            TEXT NOT NULL UNIQUE,
    title           TEXT NOT NULL,
    body_html       TEXT NOT NULL,
    meta_description TEXT NOT NULL DEFAULT '',
    published_at    TIMESTAMPTZ NOT NULL,
    modified_at     TIMESTAMPTZ NOT NULL
);

CREATE TABLE tp_listing (
    id              SERIAL PRIMARY KEY,
    wp_id           INTEGER UNIQUE,
    slug            TEXT NOT NULL UNIQUE,
    title           TEXT NOT NULL,
    era_text        TEXT NOT NULL DEFAULT '',
    price_text      TEXT NOT NULL DEFAULT '',
    price_cents     BIGINT,
    contact_text    TEXT NOT NULL DEFAULT '',
    description_html TEXT NOT NULL DEFAULT '',
    body_html       TEXT NOT NULL DEFAULT '',
    image_path      TEXT,
    image_alt       TEXT NOT NULL DEFAULT '',
    status          TEXT NOT NULL DEFAULT 'live' CHECK (status IN ('pending_payment','live','expired','removed')),
    paid_via        TEXT NOT NULL DEFAULT 'imported',
    published_at    TIMESTAMPTZ NOT NULL,
    modified_at     TIMESTAMPTZ NOT NULL,
    search_text     TEXT NOT NULL DEFAULT '',
    search_tsv      TSVECTOR GENERATED ALWAYS AS (to_tsvector('english', search_text)) STORED
);
CREATE INDEX tp_listing_published_idx ON tp_listing (published_at DESC);

CREATE TABLE form_submission (
    id          SERIAL PRIMARY KEY,
    kind        TEXT NOT NULL CHECK (kind IN ('repair','sell','contact','trading_post','mailing_list','guide_handoff')),
    fields      JSONB NOT NULL,
    photo_key   TEXT,
    status      TEXT NOT NULL DEFAULT 'new',
    created_at  TIMESTAMPTZ NOT NULL DEFAULT now()
);

CREATE TABLE redirect (
    id          SERIAL PRIMARY KEY,
    old_path    TEXT NOT NULL UNIQUE,
    new_path    TEXT NOT NULL,
    status      INTEGER NOT NULL DEFAULT 301,
    reason      TEXT NOT NULL DEFAULT '',
    hits        BIGINT NOT NULL DEFAULT 0
);

CREATE TABLE event_log (
    id          BIGSERIAL PRIMARY KEY,
    at          TIMESTAMPTZ NOT NULL DEFAULT now(),
    kind        TEXT NOT NULL,
    detail      JSONB NOT NULL DEFAULT '{}'::jsonb
);
