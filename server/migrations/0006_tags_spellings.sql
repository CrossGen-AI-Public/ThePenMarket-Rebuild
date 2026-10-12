-- Nathaniel's 2026-10-07 questions: product tags for what he adds, and old addresses he hands us.
--
-- Tags are his own words on his own items. The old site's 643 tag archives were folded into brand
-- pages and shop searches at import (the redirect table); these are new tags, not those. A tag URL
-- keeps the old /product-tag/{slug}/ shape, and an old tag address with no new tag of the same name
-- still 301s through the redirect table.
--
-- The link table has a foreign key to product (a tag on a pen that is deleted goes with it); the
-- importer's TRUNCATE product CASCADE empties the links but keeps the tag names.

CREATE TABLE product_tag (
    id         SERIAL PRIMARY KEY,
    slug       TEXT NOT NULL UNIQUE,
    name       TEXT NOT NULL,
    created_at TIMESTAMPTZ NOT NULL DEFAULT now()
);

CREATE TABLE product_tag_link (
    product_id INTEGER NOT NULL REFERENCES product(id) ON DELETE CASCADE,
    tag_id     INTEGER NOT NULL REFERENCES product_tag(id) ON DELETE CASCADE,
    position   INTEGER NOT NULL DEFAULT 0,
    PRIMARY KEY (product_id, tag_id)
);
CREATE INDEX product_tag_link_tag_idx ON product_tag_link (tag_id);

-- Old addresses he adds himself (a list of the really old site's URLs) are marked so the admin can
-- tell them from the 2,502 the importer built.
ALTER TABLE redirect ADD COLUMN added_by TEXT NOT NULL DEFAULT 'import';
ALTER TABLE redirect ADD COLUMN created_at TIMESTAMPTZ NOT NULL DEFAULT now();
