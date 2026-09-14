# SPEC-0001 — Admin login, click-to-edit listings, and customer preview

Status: Draft · 2026-09-14 · Author: Brittany Iversen with Claude
Repo: ThePenMarket-Rebuild (Rust / Axum / SQLx / Postgres / Askama, hosted on DigitalOcean)

## Goal

Nathaniel needs to run his own store without us. He signs in on his laptop or phone with an
e-mail address and a password he chose, sees the site exactly as customers see it, and changes a
pen by clicking on it: the price, the words, whether it is sold, the photos. One more click shows
him what a customer will see. The sign-in is familiar on purpose (Brittany, 2026-09-14: he is
older and will not take to passkeys), so the protection goes around the password instead of
replacing it: a strong, breach-checked password, a one-time code to his e-mail the first time a
new device signs in, and lockouts that stop guessing.

## Users & flows

**Nathaniel (the only admin).** One account, created by us at setup, never by a sign-up form.

1. **Sign in.** He opens `/admin/` (not linked from the site) and types his e-mail and password.
   The first time from a device we have not seen, the site e-mails him a six-digit code that
   works for ten minutes; he types it once and the device is remembered for 90 days ("this is my
   laptop"). After that it is just e-mail and password. "Show password" is on the form.
   At setup he chooses the password with us on the phone: at least 12 characters, a phrase he
   can remember, checked against known-breached passwords and refused if found.
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
6. **Forgot password.** "Forgot password" sends a one-time link to his e-mail that expires in
   30 minutes and works once; the old password stops working the moment a new one is set, and
   every other session is signed out. The form never says whether an e-mail exists.
7. **Locked out.** After ten wrong passwords for the account in 15 minutes, sign-in is refused for
   an hour, he gets an e-mail saying so, and we get one too. The count is on the account, not the
   IP, so an attacker with many addresses gains nothing.

## Scope

**In**
- E-mail + password sign-in for one account, created by us at setup, never by a sign-up form.
  Password stored with Argon2id; length 12 to 128, no composition rules, checked against the
  Have I Been Pwned range API at set time (k-anonymity: the password never leaves the server).
- New-device verification by a six-digit e-mail code; remembered devices via a signed,
  server-recorded cookie, listed and revocable on a "Devices" page.
- Forgot-password link by e-mail (one-time, 30 minutes) and account lockout as described above.
- Server-side sessions in Postgres; session rotated on sign-in; step-up (re-enter the password)
  for the sensitive changes above.
- Optional passkey as a faster second way in, added later from the Devices page; never required.
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

- Stack stays Rust: `argon2` for password hashing, `tower-sessions` with the Postgres store,
  `lettre` for the code and reset e-mails, `image` for photo re-encoding, `webauthn-rs` only if
  the optional passkey is built. No JS framework; the editor is plain JS posting JSON to
  `/admin/api/...`.
- Security baseline (OWASP Authentication, Forgot Password and Credential Stuffing cheat sheets;
  NIST SP 800-63B rev 4): Argon2id with per-password salt; breached-password check; generic error
  messages ("e-mail or password is wrong"); constant-time comparison; per-account and per-IP rate
  limits on sign-in, code and reset endpoints; lockout keyed to the account, not the IP; codes
  and reset tokens single-use, short-lived, stored hashed; cookies `__Host-` prefixed, `Secure`,
  `HttpOnly`, `SameSite=Strict`;
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
2. Sign-in works with e-mail and password on Safari, Chrome and Edge; a new device is asked for
   the e-mailed code once and not again within 90 days; a code or reset link cannot be used twice
   or after it expires. Automated test with a mail sink.
3. Ten wrong passwords for the account within 15 minutes refuse sign-in for an hour, regardless of
   IP, and send the two e-mails. A password found in the breached list is refused at set time.
   Automated test.
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

- Which e-mail does the code go to: info@thepenmarket.com (shared with the shop inbox) or a
  personal address? A personal one is safer if the shop inbox is ever shared.
- Which devices will he sign in from? Each one gets asked for the code once.
- Should saves go live instantly, or should there be a "Publish" step with drafts? Instant is
  simpler and matches how he works in WooCommerce today.
- Does he want to edit sold pens' pages (they stay visible for reference), or lock them?
- Photo upload limit per pen (WooCommerce allowed up to 14).

## Decisions this SPEC will force (ADRs to write when it settles)

- ADR: e-mail + password with Argon2id, breach check, new-device e-mail code; passkeys optional.
- ADR: server-side sessions in Postgres with step-up re-authentication.
- ADR: inline editing through Rust JSON endpoints, no admin framework.
