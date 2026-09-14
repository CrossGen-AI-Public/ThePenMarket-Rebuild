# SPEC-0001 — Admin login, click-to-edit listings, and customer preview

Status: Draft · 2026-09-14 · Author: Brittany Iversen with Claude
Repo: ThePenMarket-Rebuild (Rust / Axum / SQLx / Postgres / Askama, hosted on DigitalOcean)

## Goal

Nathaniel needs to run his own store without us. He signs in on his laptop or phone, sees the
site exactly as customers see it, and changes a pen by clicking on it: the price, the words,
whether it is sold, the photos. One more click shows him what a customer will see. Nobody else
can get in, and nothing he can do can be done by an attacker who steals a password, because there
is no password to steal.

## Users & flows

**Nathaniel (the only admin).** One account, created by us at setup, never by a sign-up form.

1. **Sign in.** He opens `/admin/` (not linked from the site). He touches his fingerprint or face
   on the device, or the security key. No username, no password. If the device has no passkey
   yet, he uses a one-time recovery code from the printed sheet we gave him, then registers a
   passkey for that device.
2. **Admin view.** The normal site with a thin bar across the top: "Editing as Nathaniel ·
   Preview as customer · Sign out". Every listing field shows a pencil on hover and on tap.
   Clicking a field turns it into an editor in place: price, sale price, title, the description,
   era / nib / filling mechanism / brand / category (pickers), status (live, sold, hidden), photo
   order and photo upload. Save writes to the database and the change is live immediately.
   The bar also shows "Add a pen", which creates a draft pen and opens it for editing.
3. **Preview as customer.** One click renders the same page with no editing affordances, marked
   with a small "Preview" pill so he knows he is still signed in. One click back.
4. **Sensitive changes ask again.** Marking a pen sold, deleting a photo, or changing a price by
   more than 50% asks him to touch the passkey again before it saves.
5. **Sign out**, and automatic sign-out after 60 minutes idle or 12 hours absolute.
6. **Recovery.** Lost every device: a recovery code signs him in once and lets him register a new
   passkey; each code works once; three codes wrong in a row locks recovery for an hour and e-mails
   him and us.

## Scope

**In**
- Passkey (FIDO2 / WebAuthn) sign-in with discoverable credentials, up to five registered
  authenticators (laptop, phone, one hardware key), managed from an "Devices" page.
- One-time recovery codes (ten, generated at setup, shown once, stored hashed).
- Server-side sessions in Postgres; session rotated on sign-in; step-up re-authentication for
  the sensitive changes above.
- Click-to-edit on product listings: all fields customers see, plus status and photos.
  Photo uploads size-capped, re-encoded server-side, EXIF stripped, random file names.
- "Add a pen" and "Hide a pen" (hidden pens 404 for customers, redirect to category).
- Preview-as-customer mode.
- Audit log: every change with who, when, field, before, after, IP; visible on a "History" page
  and exportable. Nothing is ever hard-deleted; photos and pens are archived.
- Security controls listed under Constraints, verified by the success criteria.

**Explicitly out (later specs)**
- Editing blog posts, pages, and Trading Post listings (Trading Post posts belong to sellers;
  moderation is its own spec).
- More than one admin, roles, or customer accounts.
- Checkout, orders, Stripe.
- Bulk import/export beyond the audit CSV.

## Constraints

- Stack stays Rust: `webauthn-rs` for passkeys, `tower-sessions` with the Postgres store,
  `argon2` for hashing recovery codes, `image` for photo re-encoding. No JS framework; the
  editor is plain JS posting JSON to `/admin/api/...`.
- Security baseline (from OWASP Authentication, MFA and Credential Stuffing cheat sheets and
  NIST SP 800-63B rev 4): phishing-resistant authenticators only, no SMS and no e-mail codes for
  sign-in; per-account and per-IP rate limits on every auth endpoint; lockout keyed to the
  account, not the IP; cookies `__Host-` prefixed, `Secure`, `HttpOnly`, `SameSite=Strict`;
  CSRF token on every mutating request in addition to SameSite; CSP with no inline scripts;
  `/admin/` under `robots.txt` Disallow and `noindex`; optional IP allowlist by env var;
  HSTS, nosniff, frame-deny (already shipped); `cargo audit` in CI; DigitalOcean managed
  Postgres with daily backups; TLS only.
- Every admin endpoint checks the session server-side; nothing is trusted from the client.
  Edits are validated with the same rules the importer uses (money in cents, non-empty title,
  known taxonomy ids).
- Brand: the admin bar and editors use the site's own type and red; nothing that looks like a
  different product.
- No deadline set. Demo target: after the homepage design is signed off.

## Success criteria

1. With no session, every `/admin/*` route returns 401 or redirects to sign-in; no admin HTML,
   JSON or field is ever served. Automated test.
2. Sign-in works with a passkey on Safari (iPhone/Mac), Chrome and Edge (Windows Hello) with no
   username field; a recovery code signs in once and cannot be reused. Automated where possible,
   manual on real devices otherwise.
3. Ten failed recovery attempts from one IP or three for the account within 15 minutes are
   refused for an hour and logged. Automated test.
4. A session cookie replayed after sign-out, after 60 minutes idle, or after 12 hours is rejected.
   Automated test.
5. A forged cross-site POST to any admin endpoint is rejected (SameSite and CSRF). Automated test.
6. Clicking any customer-visible field on a product page in admin view opens an editor; saving
   changes the database, the customer page, the sitemap and the JSON-LD within one request; the
   audit log has the row. Automated test over every field.
7. Preview-as-customer renders byte-identical HTML to what a signed-out visitor gets, apart from
   the preview pill. Automated test.
8. A 20 MB or non-image upload is rejected; an accepted photo is re-encoded, EXIF-free, and served
   at the same sizes as imported photos. Automated test.
9. The existing gate stays green (links, console, overflow, SEO, sitemaps) with admin routes
   excluded from crawling. Gate.
10. A written five-minute walkthrough exists that Nathaniel can follow on his own phone.

## Open questions

- Which devices will Nathaniel sign in from, and do they support passkeys today (iPhone, Windows
  laptop with Hello, or neither)? Decides whether we hand him a hardware key at setup.
- Who keeps the printed recovery codes, and where?
- Should saves go live instantly, or should there be a "Publish" step with drafts? Instant is
  simpler and matches how he works in WooCommerce today.
- Does he want to edit sold pens' pages (they stay visible for reference), or lock them?
- Photo upload limit per pen (WooCommerce allowed up to 14).

## Decisions this SPEC will force (ADRs to write when it settles)

- ADR: passkeys only, no passwords, recovery codes as the fallback.
- ADR: server-side sessions in Postgres with step-up re-authentication.
- ADR: inline editing through Rust JSON endpoints, no admin framework.
