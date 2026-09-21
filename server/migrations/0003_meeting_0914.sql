-- What the 2026-09-14 meeting with Nathaniel asked for: a featured pen he can choose, brand order he
-- can steer, blog and page editing, site-wide FAQs, first-party visit analytics (heat maps, search
-- log, popularity), A/B experiments, a form inbox, guided photo shots and the sourcing watchlist.
-- Nothing here is ever hard-deleted by the admin; rows are archived or unpublished.

-- One row per switch the admin can flip (featured pen, brand order mode, photo shot list, ...).
CREATE TABLE site_setting (
    key         TEXT PRIMARY KEY,
    value       JSONB NOT NULL,
    updated_at  TIMESTAMPTZ NOT NULL DEFAULT now()
);

-- Brands he pins float to the top of the brand filter in this order, whatever their stock count.
ALTER TABLE brand ADD COLUMN pin_rank INTEGER;

ALTER TABLE post ADD COLUMN status TEXT NOT NULL DEFAULT 'published' CHECK (status IN ('draft','published'));
CREATE INDEX post_status_idx ON post (status);

-- `custom` pages are the ones written in the admin (grading system, glossary, ...) and served at
-- /{slug}/; imported WordPress pages keep custom = FALSE and their dedicated routes.
ALTER TABLE page ADD COLUMN status TEXT NOT NULL DEFAULT 'published' CHECK (status IN ('draft','published'));
ALTER TABLE page ADD COLUMN custom BOOLEAN NOT NULL DEFAULT FALSE;
ALTER TABLE page ADD COLUMN show_in_footer BOOLEAN NOT NULL DEFAULT FALSE;

-- Which guided shot a photo answers ("Nib close-up"); empty for photos added the plain way.
ALTER TABLE product_image ADD COLUMN shot TEXT NOT NULL DEFAULT '';

CREATE TABLE faq (
    id          SERIAL PRIMARY KEY,
    topic       TEXT NOT NULL,
    question    TEXT NOT NULL,
    answer      TEXT NOT NULL,
    sort_order  INTEGER NOT NULL DEFAULT 0,
    status      TEXT NOT NULL DEFAULT 'published' CHECK (status IN ('draft','published')),
    in_guide    BOOLEAN NOT NULL DEFAULT TRUE,    -- Ask the Pen Market may answer with it
    created_at  TIMESTAMPTZ NOT NULL DEFAULT now(),
    updated_at  TIMESTAMPTZ NOT NULL DEFAULT now()
);
CREATE INDEX faq_topic_idx ON faq (topic, sort_order);

-- First-party analytics. `visitor` is a random id kept in the browser's localStorage: no name, no
-- e-mail, no IP address is stored. `detail` carries the per-kind payload (click target, scroll depth,
-- search words and result count, experiment and variant).
CREATE TABLE visit_event (
    id          BIGSERIAL PRIMARY KEY,
    at          TIMESTAMPTZ NOT NULL DEFAULT now(),
    visitor     TEXT NOT NULL,
    kind        TEXT NOT NULL CHECK (kind IN ('view','click','scroll','search','exposure','conversion')),
    path        TEXT NOT NULL DEFAULT '',
    device      TEXT NOT NULL DEFAULT '' CHECK (device IN ('', 'phone', 'tablet', 'desktop')),
    product_id  INTEGER,
    detail      JSONB NOT NULL DEFAULT '{}'::jsonb
);
CREATE INDEX visit_event_kind_at_idx ON visit_event (kind, at DESC);
CREATE INDEX visit_event_path_idx ON visit_event (path, kind);
CREATE INDEX visit_event_product_idx ON visit_event (product_id) WHERE product_id IS NOT NULL;

CREATE TABLE experiment (
    id          SERIAL PRIMARY KEY,
    key         TEXT NOT NULL UNIQUE,            -- the code looks experiments up by key
    name        TEXT NOT NULL,
    description TEXT NOT NULL DEFAULT '',
    variants    JSONB NOT NULL,                  -- [{"key":"a","label":"..."}, ...]
    status      TEXT NOT NULL DEFAULT 'draft' CHECK (status IN ('draft','running','stopped')),
    winner      TEXT,                            -- variant everyone gets once stopped
    started_at  TIMESTAMPTZ,
    stopped_at  TIMESTAMPTZ
);

-- Pens he is hunting for, and what the marketplaces turned up.
CREATE TABLE sourcing_want (
    id          SERIAL PRIMARY KEY,
    label       TEXT NOT NULL,
    keywords    TEXT NOT NULL,
    max_cents   BIGINT,
    notes       TEXT NOT NULL DEFAULT '',
    source      TEXT NOT NULL DEFAULT 'manual' CHECK (source IN ('manual','search-demand','sold-fast')),
    status      TEXT NOT NULL DEFAULT 'active' CHECK (status IN ('active','paused','archived')),
    created_at  TIMESTAMPTZ NOT NULL DEFAULT now(),
    checked_at  TIMESTAMPTZ
);

CREATE TABLE sourcing_find (
    id          SERIAL PRIMARY KEY,
    want_id     INTEGER NOT NULL REFERENCES sourcing_want(id) ON DELETE CASCADE,
    provider    TEXT NOT NULL,
    external_id TEXT NOT NULL,
    title       TEXT NOT NULL,
    price_cents BIGINT,
    url         TEXT NOT NULL,
    image_url   TEXT NOT NULL DEFAULT '',
    status      TEXT NOT NULL DEFAULT 'new' CHECK (status IN ('new','saved','dismissed')),
    found_at    TIMESTAMPTZ NOT NULL DEFAULT now(),
    UNIQUE (provider, external_id)
);
CREATE INDEX sourcing_find_want_idx ON sourcing_find (want_id, status);

-- The inbox: which submissions reached his e-mail, and which he has dealt with.
ALTER TABLE form_submission ADD COLUMN mailed BOOLEAN NOT NULL DEFAULT FALSE;
ALTER TABLE form_submission ADD COLUMN handled_at TIMESTAMPTZ;
