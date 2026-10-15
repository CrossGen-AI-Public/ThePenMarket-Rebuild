-- SPEC-0001: one admin account (Nathaniel), password sign-in with new-device e-mail codes,
-- server-side sessions, remembered devices, and soft-deleted photos. Nothing here is ever hard-deleted.

CREATE TABLE admin_account (
    id              SERIAL PRIMARY KEY,
    email           TEXT NOT NULL UNIQUE,            -- stored lower-cased
    display_name    TEXT NOT NULL DEFAULT 'Nathaniel',
    password_hash   TEXT NOT NULL,                   -- Argon2id PHC string
    failed_count    INTEGER NOT NULL DEFAULT 0,
    failed_since    TIMESTAMPTZ,
    locked_until    TIMESTAMPTZ,
    created_at      TIMESTAMPTZ NOT NULL DEFAULT now(),
    password_set_at TIMESTAMPTZ NOT NULL DEFAULT now()
);

CREATE TABLE admin_session (
    id              SERIAL PRIMARY KEY,
    account_id      INTEGER NOT NULL REFERENCES admin_account(id) ON DELETE CASCADE,
    token_hash      TEXT NOT NULL UNIQUE,            -- sha256 of the cookie value
    device_id       INTEGER,
    created_at      TIMESTAMPTZ NOT NULL DEFAULT now(),
    last_seen_at    TIMESTAMPTZ NOT NULL DEFAULT now(),
    step_up_at      TIMESTAMPTZ,                     -- last time the password was re-entered
    preview         BOOLEAN NOT NULL DEFAULT FALSE,  -- "preview as customer" mode
    ip              TEXT NOT NULL DEFAULT '',
    user_agent      TEXT NOT NULL DEFAULT '',
    revoked_at      TIMESTAMPTZ
);
CREATE INDEX admin_session_account_idx ON admin_session (account_id);

CREATE TABLE admin_device (
    id              SERIAL PRIMARY KEY,
    account_id      INTEGER NOT NULL REFERENCES admin_account(id) ON DELETE CASCADE,
    token_hash      TEXT NOT NULL UNIQUE,            -- sha256 of the device cookie
    name            TEXT NOT NULL DEFAULT '',        -- from the user agent, e.g. "Mac · Safari"
    ip              TEXT NOT NULL DEFAULT '',
    first_seen_at   TIMESTAMPTZ NOT NULL DEFAULT now(),
    last_seen_at    TIMESTAMPTZ NOT NULL DEFAULT now(),
    expires_at      TIMESTAMPTZ NOT NULL,
    revoked_at      TIMESTAMPTZ
);
CREATE INDEX admin_device_account_idx ON admin_device (account_id);

-- One-time codes: six digits for a new device, a long token for a password reset.
CREATE TABLE admin_code (
    id              SERIAL PRIMARY KEY,
    account_id      INTEGER NOT NULL REFERENCES admin_account(id) ON DELETE CASCADE,
    kind            TEXT NOT NULL CHECK (kind IN ('device','reset')),
    code_hash       TEXT NOT NULL,
    created_at      TIMESTAMPTZ NOT NULL DEFAULT now(),
    expires_at      TIMESTAMPTZ NOT NULL,
    used_at         TIMESTAMPTZ,
    attempts        INTEGER NOT NULL DEFAULT 0,
    ip              TEXT NOT NULL DEFAULT ''
);
CREATE INDEX admin_code_account_idx ON admin_code (account_id, kind);

ALTER TABLE product_image ADD COLUMN archived_at TIMESTAMPTZ;
CREATE INDEX event_log_kind_idx ON event_log (kind, at DESC);
