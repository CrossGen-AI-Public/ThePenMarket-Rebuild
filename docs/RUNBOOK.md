# Runbook: ThePenMarket.com rebuild

## Where it runs
- sparky (Tailscale): http://100.117.164.79:8140/ as the user service `thepenmarket.service`, working dir `~/thepenmarket/server`, binary `target/release/thepenmarket`, log `~/thepenmarket/server/server.log`.
- Postgres 16 on sparky, port 5433, database and role `thepenmarket`.
- Secrets: `~/.config/thepenmarket.env` (chmod 600): `DATABASE_URL`, `CSRF_SECRET`, `GUIDE_AI_URL`, `GUIDE_AI_KEY`, `GUIDE_AI_MODEL`. Never commit or print it.

## Health
```
curl -s http://100.117.164.79:8140/healthz            # ok
curl -s http://100.117.164.79:8140/api/guide/health   # {"backend":"openai-compatible","model":...,"catalog":248}
systemctl --user status thepenmarket.service
tail -50 ~/thepenmarket/server/server.log
```

## Redeploy after a change
```
cd ~/thepenmarket && bash scripts/deploy.sh          # release build, re-import, restart, health-check
SKIP_IMPORT=1 bash scripts/deploy.sh                 # keep the database as is
```

## Re-import the catalog
```
cd ~/thepenmarket/server && ./target/release/import   # truncates and reloads from ~/thepenmarket/backup; log in research/import-log.md
systemctl --user restart thepenmarket.service         # or wait: the catalog snapshot refreshes every 10 minutes
```

## Switch or fix the model
Edit the three `GUIDE_AI_*` lines in `~/.config/thepenmarket.env` and `systemctl --user restart thepenmarket.service`. With no model configured the assistant says it is offline and shows the phone and e-mail; it never answers from canned text.

## The gate
```
bash scripts/gate.sh http://127.0.0.1:8140            # engine, seo, sitemaps, redirects, links, console, overflow, screenshots, slop
DEPLOYED_URL=http://100.117.164.79:8140 bash scripts/gate.sh http://127.0.0.1:8140 sparky
```
Output under `research/gate/` (results.txt, per-row logs, shots/).

## Rate limits and bodies
- `/api/guide`: 10 requests per minute per IP (`GUIDE_RATE_PER_MIN`), 4 tool rounds per turn, 24 turns of history.
- Forms: 20 per minute per IP, 10 MB photo (re-encoded to JPEG, metadata stripped, stored under `server/uploads/`, not served).
- Everything else: 64 KB request bodies.

## The admin (Nathaniel's sign-in, click-to-edit, preview)
Spec: `docs/spec/0001-admin-login-click-to-edit-preview.md`. Sign-in page: **https://penmarket.crossgen-ai.com/admin/login/** (not linked anywhere; `robots.txt` disallows `/admin/`). Nathaniel's account is `nathaniel@thepenmarket.com`; he changes his password at `/admin/password/` (Password button in the editing bar).
**Until SMTP is configured, his first sign-in from each new device needs the six-digit code read from the droplet's mail log:**
```
ssh crossgen-droplet "grep 'code is' /srv/apps/thepenmarket/uploads/admin-mail.log | tail -1"
```
```
cd ~/thepenmarket/server
ADMIN_PASSWORD='…' ./target/release/admin create nathaniel@example.com --name Nathaniel   # first account; password checked against the policy and HIBP
./target/release/admin set-password nathaniel@example.com     # reads the new password from stdin (or ADMIN_PASSWORD)
./target/release/admin unlock nathaniel@example.com           # clears a lockout (10 wrong passwords in 15 min locks for an hour)
./target/release/admin forget-devices nathaniel@example.com   # every device asks for the e-mail code again
```
- E-mail: with `SMTP_URL` (`smtps://user:pass@host:465`) and `MAIL_FROM` in the env file, codes and reset links are sent. Without them every message is appended to `server/uploads/admin-mail.log` (on the droplet `/srv/apps/thepenmarket/uploads/admin-mail.log`); read the six-digit code from there. `ADMIN_ALERT_EMAIL` gets a copy of lockout notices.
- On the droplet, run the CLI inside the container: `docker exec -it -e ADMIN_PASSWORD='…' thepenmarket /app/admin create …`.
- Data: `admin_account`, `admin_session`, `admin_device`, `admin_code`; every change is an `event_log` row (`admin.*`) shown at `/admin/history/` and downloadable as CSV. Photos he removes are archived (`product_image.archived_at`), never deleted. Hidden pens are `product.status = 'draft'` and 404 for everyone but him.
- **Re-importing is now guarded**: `import` refuses once any admin edit exists, because it truncates `product`. `FORCE_IMPORT=1` overrides. Deploy with `SKIP_IMPORT=1` from here on.
- The droplet keeps its own database after the first restore, so once Nathaniel edits on https://penmarket.crossgen-ai.com/ the droplet is the source of truth; `push-droplet.sh` no longer touches its data or the photos he added (`media/uploads/admin/`).
- Gate row: `ADMIN_TEST_EMAIL=… ADMIN_TEST_PASSWORD=… bash scripts/gate.sh …` adds an `admin` row (sign in, code, inline edit and revert, preview toggle, history, every admin page at 1440 and 390, the writing toolbar, the heat map toggle).

## What the 2026-09-14 meeting added (migration 0003)
Everything is reached from **/admin/** (where sign-in now lands). All of it is audited in `event_log` and shown in History.
- **Inbox and e-mail routing.** Every form is stored and shown at `/admin/inbox/` (photos served only to the admin). With `SMTP_URL` set, each one is also e-mailed with Reply-To set to the visitor: repair, sell, contact and guide handoffs go to `FORMS_TO` (default info@thepenmarket.com), Trading Post listings to `TRADING_POST_TO` (default tradingpost@thepenmarket.com). Without SMTP the confirmation page says so and the message is only in the inbox. `form_submission.mailed` is true only when a real server accepted it.
- **Blog and pages.** `/admin/posts/` edits the 261 migrated posts and writes new ones (drafts are visible only to the admin at their future URL); the list flags posts with no photos or a broken photo. `/admin/pages/` makes permanent pages at `/{slug}/` (grading system, glossary), added to `sitemap-pages.xml`, `/site-map/` and optionally the footer. Body HTML passes `admin/content.rs::clean_html` (ammonia) and nothing else. Photos go to `media/uploads/admin/blog/`.
- **FAQs.** `/admin/faqs/` feeds `/faq/` (FAQPage JSON-LD) and, when ticked, the guide's system prompt (his words only; `guide.rs`). Seeded once from the copy on the service pages.
- **Homepage.** "Feature on the homepage" on any live pen's page (setting `featured_product_id`; falls back to the newest pen when it sells). "Most Popular" row: setting `popular_section`, shown only when four pens have three visitors each in 30 days.
- **Brands.** `/admin/brands/`: `brand.pin_rank` pins lead the Brand filter; the rest follow live stock count, or visitor interest with setting `brand_order = popular`.
- **Photos.** 14 per pen (`settings::MAX_PHOTOS`), enforced by the API. "Guided photo shoot" walks the shot list (setting `photo_shot_list`, editable at `/admin/settings/`), and writes the shot into the alt text and `product_image.shot`.
- **Insights, heat maps, experiments.** `static/js/track.js` posts to `/api/t` (first party; cookie `pm_v` is a random id; DNT/GPC browsers get no cookie and are never recorded; the admin is never recorded; rows older than 400 days are pruned). `/admin/insights/` ranks pens, searches, empty searches, guide questions, dead clicks. "Heat map" in the admin bar draws that page's clicks and scroll reach over the live page. `/admin/experiments/` runs the experiments in `insights::REGISTRY` (50/50 by visitor, sticky); adding one means adding a registry entry and the branch that reads `insights::variant`.
- **Search.** A search with no sort chosen ranks by text match, then by 90-day views; "Most viewed" is a sort option; searches several visitors made (letters and digits only, with results) appear as "Popular searches" under the search box.
- **Sourcing.** `/admin/sourcing/`: watchlist with prepared eBay/Etsy/LiveAuctioneers searches and his own price range for comparable pens (from the catalog). With `EBAY_APP_ID` and `EBAY_CERT_ID` (eBay developer keys, Browse API) the server checks eBay daily and on "Check eBay now". **The eBay client has never run against the live API: no keys existed when it was written.** Test it the day keys are added.
- **His 2026-09-18 page list.** `pen-grading`, `defining-nibs` and `buyers-guide` are seeded once as empty drafts (`routes/content.rs::seed_requested_pages`); `scripts/load-his-copy.py` fills them with his copy (see 2026-10-07 below); the editor has "Link to a blog post" for the Buyer's Guide. The three-pen row under the hero was removed at Brittany's request.
- **Glossary** (2026-09-21; `glossary.rs`, `gloss.rs`, `routes/glossary.rs`, `admin/glossary.rs`, migration 0004). `/admin/glossary/` is where he works:
  - "Words found in your own writing" are hidden, empty drafts proposed by `glossary::mine` (first start, and the "Look through my writing again" button). The words to look for are in `server/data/glossary-lexicon.json`: names and spellings only. To have it look for more words, add them there and rebuild. A word is proposed when two or more of his posts or listings use it.
  - An entry's editor shows up to five of his own sentences using the word; "Use these words" copies one into the box and records the post as the source. Publishing needs a definition. Photo upload reuses `/admin/api/media/`.
  - "Makes, nibs, filling systems and eras": the 104 definitions drafted at build time. One counts as his, and joins the glossary, the listing links and the guide, only after he ticks it (`glossary_approved_def`, keyed by table + slug so a re-import keeps it). Editing one trips the importer's "admin has made changes" guard.
  - Published entries feed four places from the site cache (`SiteCache.glossary`, refreshed on every save): the index at `/glossary/`, the card above a shop search that names an entry, the dotted tap-to-define links in listing descriptions (first use of each word, at most 8, never while he is editing), and the guide's `define_term` tool, which answers with his words or says there is no entry.
  - A lookup that finds nothing (index search submitted with Enter, or the guide) adds the bare word to `glossary_miss`; the admin page lists them. Nothing is tallied for Do Not Track / GPC browsers or the admin, and `/privacy/` says so.
  - `/glossary/` stays out of the footer, the menu, the sitemap and `llms.txt`, and is `noindex`, until one entry is published. Gate row `glossary` covers it.
- **Google reviews** (`reviews.rs`). He has none yet. Saving his Google Business Profile Place ID at `/admin/settings/` adds "Review us on Google" to every footer. With `GOOGLE_PLACES_KEY` (Places API New) the server fetches rating and reviews daily and the homepage shows what Google returned, verbatim, only when at least one review exists. **Never run against the live API: no key or profile existed when it was written.**
- **Classic view** (footer switch): larger text, photograph instead of the 3D pen, no motion; remembered per browser. The 3D pen's "+" markers are buttons that move the camera in.
- `import` also refuses after post, page, brand or media edits (same `FORCE_IMPORT=1` override).

## Nathaniel's 2026-10-07 questions (migration 0006)
- **Tags (his q1).** "Tags" row in Item specifics on any product page while signed in: comma separated, his earlier tags offered as he types, 20 at most (`admin/api.rs` field `tags`, `store::set_product_tags`). Each tag has a page at the old WooCommerce shape `/product-tag/{slug}/` (`shop::tag`), `noindex` and out of `sitemap-categories.xml` until it holds `db::TAG_INDEX_MIN` (3) live items. A tag nothing uses is deleted, and an old tag address with no new tag of that name still 301s through the redirect table. Shop search matches tag names. A forced re-import empties the tag links (FK cascade) but keeps the tag names; tag edits are `admin.edit`, so the importer's guard already refuses.
- **Old addresses (q2, q3).** `/admin/redirects/` lists the redirect table (his own first), searches it, downloads it as CSV (`/admin/redirects.csv`), and takes pasted lists: one address per line, optionally followed by its destination. Each line is checked against the running site first (`Probe`, loopback to `HOST:PORT`, no redirects followed): already a page or already redirected = left alone; no destination = matched by the last path segment to a product / post / custom page / brand / category / published glossary entry / Trading Post listing; no match = listed back with a shop-search suggestion to paste again. A destination must be on this site and answer 200. Old URLs with a query string (`/index.php?main_page=…`, `/?page_id=12`) match on path + query first (`db::redirect_for`). His rows carry `redirect.added_by = 'admin'`, only those can be removed, and a forced re-import puts them back.
- **URL audit (q2).** `python3 scripts/url_audit.py <new-base> research/url-audit/<date>.csv` reads the live site's sitemaps today and checks every address on the new site. 2026-10-07 against a local copy: 3,134 addresses, 675 same page, 2,397 redirect, **62 missing: 19 products listed after the 2026-08-24 backup and their 43 tag pages**. Those are fixed by the cutover re-import from a fresh backup and crawl, not by redirects. Re-run the audit after cutover; expect 0 missing.
- **Search spellings (q4).** `/admin/search/`: one line per word, `Sheaffer = Schaffer, Shaeffer, Schaeffer, Shaffer` (setting `search_spellings`; until he saves, the examples from his e-mail, `spelling::default_list`). A shop or blog search for any spelling searches all of them and says "Showing results for “Sheaffer” too." A shop search that still finds nothing tries the make one or two letters off (`spelling::near_miss`). The page also lists searches that found nothing in 90 days.
- **His pages and nib glossary (q5).** The copy he e-mailed on 2026-09-18 lives in `$BACKUP_DIR/client-copy/` and is never committed (the repo is public). `set -a; . ~/.config/thepenmarket.env; set +a; python3 scripts/load-his-copy.py` fills Pen Grading, Defining Nibs and Buyer's Guide only while empty and publishes them, and puts his 34 nib-glossary definitions into glossary drafts (fills empty ones, adds missing words; never publishes). Safe to run again. `--dry-run` prints the SQL.
- **Hero photo (q6).** `/admin/settings/` "Spinning 3D pen or your photograph" (setting `hero_mode`). The photo is the featured pen's first photo. A running or decided `hero-stage` experiment overrides it.
- **Whole pen in frame (q7).** Product photos in cards, category tiles, hero thumbnails, product thumbnails and guide cards use `object-fit: contain` on the grey ground (was `cover`, which cut the ends off long pens).

## Online checkout (Stripe, migration 0005)
Decided 2026-10-05: Nathaniel goes with Stripe, on **his own Stripe account** (he invites Brittany as a Developer; keys are never e-mailed). Built so far: **pens only**. The Trading Post $5 listing fee and the $125/yr Unlimited Posts membership are **not built**; the membership page keeps its demo notice whatever the keys say.

**Turning it on.** Both values go in the env file (`~/.config/thepenmarket.env`, or `/srv/apps/thepenmarket/.env` on a droplet), never in git, then restart:
```
STRIPE_SECRET_KEY=sk_test_…        # Stripe dashboard → Developers → API keys (test mode first; a restricted key with Checkout Sessions write + Refunds write also works)
STRIPE_WEBHOOK_SECRET=whsec_…      # from the webhook endpoint below
```
With either missing, nothing changes: every pen page shows "Demo — checkout not connected" and `/api/stripe/webhook` answers 503.

**The webhook.** Stripe dashboard → Developers → Webhooks → Add endpoint: `https://<host>/api/stripe/webhook` (e.g. `https://thepenmarket.com/api/stripe/webhook` once the domain points at his droplet). Events:
`checkout.session.completed`, `checkout.session.expired`, `checkout.session.async_payment_succeeded`, `checkout.session.async_payment_failed`. Copy the endpoint's signing secret into `STRIPE_WEBHOOK_SECRET`. Test mode and live mode each have their own keys **and their own endpoint and secret**.

**How it works** (`routes/checkout.rs`, `orders.rs`, `stripe.rs`):
- "Buy this pen" (`POST /checkout/{slug}/`, CSRF + the form rate limit) reads the price (sale price first) and name **from the database**, holds the pen with a `shop_order` row (status `open`), and sends the buyer to a Stripe-hosted Checkout Session (31 minutes; the hold lasts 5 minutes longer). A second buyer sees "Someone is checking out this pen right now". A partial unique index allows one open/processing order per pen, so racing buyers cannot both get it. The holder can "Let it go" (`/checkout/{slug}/release/`, which also expires the session at Stripe).
- `checkout.session.completed` with `payment_status=paid` marks the pen `sold` and the order `paid` in one transaction, refreshes the catalog (so the guide stops offering it), writes `event_log` `shop.sold`, and mails `FORMS_TO` (to the mail log without SMTP). If the pen was already sold or held by someone else, the payment is **refunded in full via the API** (idempotency key `pm-refund-<order>`) and the order is `refunded`.
- Bank payments (if he enables ACH): `completed` with `unpaid` → order `processing`, pen stays held; `async_payment_succeeded` sells it, `async_payment_failed` frees it.
- Webhooks are verified (`Stripe-Signature`, HMAC-SHA256, 5-minute tolerance, constant time) and recorded in `stripe_event`; a repeated delivery does nothing. A failure rolls back and returns 500 so Stripe retries.
- `/checkout/thanks/?session_id=…` asks Stripe for the session server-side and shows what was bought, the total and the shipping address; it promises nothing else.
- `/admin/orders/` lists paid, clearing and refunded orders (buyer, e-mail, ship-to, link to the payment in Stripe). Refunds and disputes are done in his Stripe dashboard.
- CSP `form-action` now allows `https://checkout.stripe.com` (the Buy form redirects there). No Stripe.js on our pages.
- `import` refuses while any order is `paid`/`processing` (a re-import would put sold pens back on sale).

**Open decisions for Nathaniel** (nothing invented in the meantime):
- **Shipping**: one flat charge per pen, set at `/admin/settings/` ("Shipping on online orders"). No default; until he sets one, buyers pay the pen's price only and the admin says so. Per-pen or insured-value rates are not built.
- **Sales tax**: nothing is collected. Whether he must charge Connecticut (or other states') sales tax is a question for him and his accountant; Stripe Tax can be added to the session once he decides.
- **Payment methods**: whatever he enables in his Stripe dashboard (cards, Apple/Google Pay, Link, ACH, Klarna/Afterpay) appears on the checkout page with no code change.
- **Receipts**: Stripe sends its own e-mail receipts only if he turns them on (Settings → Customer emails).
- Orders ship to the US only (`allowed_countries = US`).

**Testing once his test keys exist** (never done yet; no Stripe account existed when this was written):
1. Set the two test-mode values, restart, open any live pen: the button reads "Buy this pen".
2. Pay with card `4242 4242 4242 4242`, any future date, any CVC, any ZIP. Expect: thanks page, pen shows Sold, `/admin/orders/` shows it, a mail-log entry.
3. Card `4000 0000 0000 0002` (declined): the pen stays held until the session expires or the buyer lets it go.
4. Two browsers on the same pen: the second sees the hold message.
5. Mark the pen sold in the admin, then pay in a session opened before that: the order shows Refunded and Stripe shows the refund.
6. `stripe listen --forward-to localhost:8140/api/stripe/webhook` (Stripe CLI) drives webhooks to a local build; `stripe trigger checkout.session.completed` alone will be ignored (no matching order), which is correct.
7. Set the test-mode pens back to For sale on their pages, then switch to live keys and the live endpoint.

## The public copy (droplet): https://penmarket.crossgen-ai.com/
Deployed 2026-09-14 by `scripts/push-droplet.sh` (from sparky) + `scripts/deploy-droplet.sh` (on the droplet, as root):
- containers `thepenmarket` (the app, image built from `server/Dockerfile`) and `thepenmarket-db` (postgres:16-alpine, volume `thepenmarket_pgdata`) on the `web` network; Caddy block `penmarket.crossgen-ai.com` in `/root/proxy/Caddyfile`.
- files: `/srv/apps/thepenmarket/build` (build context), `/srv/apps/thepenmarket/media` (photos, mounted read-only), `/srv/apps/thepenmarket/uploads` (form photos), `/srv/apps/thepenmarket/.env` (secrets), `db.sql.gz` (the dump restored on first run).
- redeploy: `bash scripts/push-droplet.sh` from sparky (rsyncs, dumps the sparky database, rebuilds the image, swaps the container, health-checks). The Rust build takes about 8 minutes on the droplet's two cores.
- rollback: `docker run` the `thepenmarket:previous` image with the same flags.
