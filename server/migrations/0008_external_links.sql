-- Links to other websites inside his posts, pages, listings and Trading Post ads (ADR-0001). A daily
-- check records what each one answers. A link is "dead" only after it answered "gone" (404, 410, no
-- such host, connection refused) on two checks at least 20 hours apart; a dead link is then shown as
-- plain text, never as a link. The stored content is never changed: a link that comes back to life is
-- linked again on the next check.

CREATE TABLE external_link (
    url              TEXT PRIMARY KEY,
    last_status      INTEGER,                 -- HTTP status, or 0 when nothing answered
    last_result      TEXT NOT NULL DEFAULT '', -- ok | gone | unsure | skipped, with a short reason
    first_gone_at    TIMESTAMPTZ,             -- start of the current run of "gone" answers
    dead             BOOLEAN NOT NULL DEFAULT FALSE,
    dead_since       TIMESTAMPTZ,
    last_checked_at  TIMESTAMPTZ,
    found_on         TEXT NOT NULL DEFAULT '' -- one page it appears on, for the admin list
);
CREATE INDEX external_link_dead_idx ON external_link (dead) WHERE dead;
