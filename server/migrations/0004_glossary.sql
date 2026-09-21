-- The glossary (decided 2026-09-21; replaces the 2026-09-14 plan to leave it to Nathaniel's first
-- Claude Code session). Pen vocabulary and models live here; makes, nibs, filling mechanisms and eras
-- stay in their own tables and join the same index. Every definition is his: entries found in his
-- writing arrive as hidden drafts with the sentences where he used the word, and nothing is visible
-- until he publishes it.
--
-- No foreign keys to brand / post / product on purpose: the importer truncates those tables with
-- CASCADE, and a re-import must never take his glossary with it.

CREATE TABLE glossary_term (
    id               SERIAL PRIMARY KEY,
    slug             TEXT NOT NULL UNIQUE,
    name             TEXT NOT NULL,
    kind             TEXT NOT NULL DEFAULT 'term' CHECK (kind IN ('term', 'model')),
    topic            TEXT NOT NULL DEFAULT '',
    aliases          TEXT NOT NULL DEFAULT '',          -- other spellings, comma separated
    definition       TEXT NOT NULL DEFAULT '',          -- plain text, blank line between paragraphs
    brand_slug       TEXT NOT NULL DEFAULT '',          -- models: the make
    image_path       TEXT NOT NULL DEFAULT '',
    image_alt        TEXT NOT NULL DEFAULT '',
    see_also         TEXT NOT NULL DEFAULT '',          -- names of other entries, comma separated
    source_url       TEXT NOT NULL DEFAULT '',          -- the post or listing the words came from
    source_title     TEXT NOT NULL DEFAULT '',
    status           TEXT NOT NULL DEFAULT 'draft' CHECK (status IN ('draft', 'published')),
    in_guide         BOOLEAN NOT NULL DEFAULT TRUE,
    link_in_listings BOOLEAN NOT NULL DEFAULT TRUE,
    origin           TEXT NOT NULL DEFAULT 'admin' CHECK (origin IN ('admin', 'mined')),
    mentions         INTEGER NOT NULL DEFAULT 0,        -- posts + listings of his that use the word
    created_at       TIMESTAMPTZ NOT NULL DEFAULT now(),
    updated_at       TIMESTAMPTZ NOT NULL DEFAULT now()
);
CREATE INDEX glossary_term_status_idx ON glossary_term (status);

-- Sentences of his that use the word, verbatim, offered in the editor as a starting point.
CREATE TABLE glossary_quote (
    id           SERIAL PRIMARY KEY,
    term_id      INTEGER NOT NULL REFERENCES glossary_term(id) ON DELETE CASCADE,
    quote        TEXT NOT NULL,
    source_url   TEXT NOT NULL,
    source_title TEXT NOT NULL,
    score        INTEGER NOT NULL DEFAULT 0
);
CREATE INDEX glossary_quote_term_idx ON glossary_quote (term_id);

-- The brand / era / nib / filling-mechanism definitions were drafted from his posts by the build, not
-- written by him. The glossary (and the guide, and the tap-to-define links) only use one after he has
-- ticked it as his own. Keyed by table + slug so a re-import does not forget his approvals.
CREATE TABLE glossary_approved_def (
    tbl         TEXT NOT NULL CHECK (tbl IN ('brand', 'era', 'nib', 'filling_mechanism')),
    slug        TEXT NOT NULL,
    approved_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    PRIMARY KEY (tbl, slug)
);

-- Words visitors looked up that have no entry yet: his to-do list, most wanted first.
CREATE TABLE glossary_miss (
    word      TEXT PRIMARY KEY,
    hits      INTEGER NOT NULL DEFAULT 1,
    last_seen TIMESTAMPTZ NOT NULL DEFAULT now()
);
