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

**The whole site, not a sample** (before a launch, after a big change; about an hour on ~700 URLs):
```
ADMIN_TEST_EMAIL=… ADMIN_TEST_PASSWORD=… PYTHON=<python with playwright> bash scripts/full_audit.sh http://127.0.0.1:8140 research/audit/$(date +%F)
```
Every URL in the site's own sitemaps: 200, SEO, every link and image, console errors (1440 and 390), overflow (320, 390, 768, 1300); every row of the redirect table (301 and the destination opens); every admin screen signed in (200 and `no-store`); the security headers and `security.txt`. Dead links to other websites inside his old posts are listed in `links-external.txt` for him and do not fail it; any broken link to our own site does.

## Rate limits and bodies
- `/api/guide`: 10 requests per minute per IP (`GUIDE_RATE_PER_MIN`), 80 per IP per day, `GUIDE_DAILY_MAX` for the whole site (default 600), 8,000 characters of conversation, 60 seconds per answer, 4 tool rounds, one hand-off to his inbox per answer and three per IP a day. `GUIDE_ENABLED=0` turns the guide off (the page shows his phone and e-mail).
- Forms: 20 per minute per IP, 10 MB photo (11 MB request), decoded with an 8000 px / 256 MB cap two at a time off the request threads, re-encoded to JPEG (metadata stripped), stored under `server/uploads/`, not served.
- Checkout: 20 per minute per IP; one visitor (cookie or address) holds at most two pens at a time.
- Admin sign-in, code, reset, step-up and erase: 10 per minute per IP. Sign-in codes: 5 per account per hour. Reset links: 3 per account per hour.
- Analytics (`/api/t`): 120 batches per minute per IP, 600 events per visitor per day, signed visitor cookie, same-origin only.
- Everything else: 64 KB request bodies; every request ends after 120 seconds.

## Security (review of 2026-10-12)
Reviewed against OWASP ASVS 5.0 and the OWASP cheat sheets; the findings and fixes are in the commit "Security hardening from the 2026-10-12 review". What to know when running it:
- **`TRUSTED_PROXIES`** (comma-separated addresses or CIDRs): the only peers whose `X-Forwarded-For` / `X-Forwarded-Proto` are believed; from anyone else those headers are stripped. Default: loopback. `deploy-droplet.sh` passes the `web` Docker network's subnet (where Caddy is). Get it wrong and every visitor shares Caddy's address in the rate limiters, or (worse) clients pick their own.
- **CSRF**: every form and `X-CSRF` call needs the HMAC token the page renders (`PageMeta.csrf`, filled for every page); the raw cookie is not a token. The cookie is `__Host-pm_csrf` (HttpOnly, Secure) on HTTPS, `pm_csrf` on plain HTTP. POSTs a browser marks cross-site (`Sec-Fetch-Site`, else `Origin`) are refused before any handler; the Stripe webhook is exempt (it has its signature).
- **Sign-in**: one message for every failure. 10 wrong passwords in 15 minutes pause sign-in for an hour **from new devices only**; a device he confirmed with the e-mailed code still gets in with the right password. Forgot password always works and clears the pause. New passwords need 15 characters (NIST 800-63B rev 4) and are checked against Have I Been Pwned. A reset or a password change signs out everything and forgets remembered devices.
- **Headers** on every response (redirects and errors included): CSP, HSTS, nosniff, `X-Frame-Options: DENY`, Referrer-Policy, Permissions-Policy, COOP `same-origin`, CORP `same-origin` (`cross-origin` for photos, so the buyer's receipt can show one). `/admin/*` is `no-store` and `noindex`. `/.well-known/security.txt` names info@thepenmarket.com.
- **Container**: uid 10001, read-only filesystem, no capabilities, `no-new-privileges`, 1 GB / 256 processes; only `uploads/` and `media/` are writable. Postgres is on `thepenmarket-internal`, a Docker network only the app joins. The database dump is private in transit and deleted after the restore.
- **Not done, on purpose**: `style-src` still allows inline styles (templates use `style=` attributes; removing it means moving them to classes); the CSRF token is not tied to the admin session (the `__Host-` cookie and the same-origin check cover the attack it would stop).

## Outgoing e-mail (before launch)
The site sends through any SMTP service: `SMTP_URL` and `MAIL_FROM` in the env file. Until they are set nothing is e-mailed (codes, resets, form copies, order notices and buyer receipts go to `uploads/admin-mail.log`), and the new-device sign-in code is skipped. Decided direction: Twilio SendGrid **on Nathaniel's account**, sending as info@thepenmarket.com:
```
SMTP_URL=smtp://apikey:<SendGrid API key>@smtp.sendgrid.net:2525?tls=required
MAIL_FROM=ThePenMarket.com <info@thepenmarket.com>
```
Port 2525 because DigitalOcean blocks the usual SMTP ports on new droplets (check on his droplet). Authenticate the domain in SendGrid (it gives DNS records to add at GoDaddy: SPF/DKIM) or receipts land in spam.


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
- Data: `admin_account`, `admin_session`, `admin_device`, `admin_code`; every change is an `event_log` row (`admin.*`) shown at `/admin/history/` and downloadable as CSV. Photos he removes are archived (`product_image.archived_at`), never deleted. Hidden pens are `product.status = 'draft'` and 404 for everyone but him; deleted pens are `'archived'` and 301 to their category (see "Sold Pen Archive and deleting pens").
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

## Links to other websites (ADR-0001, migration 0008)
A daily check (`links.rs`, five minutes after start, then every 24 h) requests every outside link in published posts, pages, products and Trading Post ads. Gone (404, 410, no such host, nothing listening) on two checks at least 20 hours apart: the link is shown as plain words (`<span class="link-gone">`) until it answers again; the stored HTML is never changed. 401/402/403/429/5xx/timeouts stay linked. `/admin/links/` lists what was unlinked and why and has a "Check every link now" button. `LINK_CHECK=0` turns the daily check off. First run on 2026-10-12: 50 outside links, 3 gone (Dallas and Arkansas pen show pages, nancybach.com).

## Old addresses from before WordPress
`routes/legacy.rs` answers the shapes his ASP.NET shop and WordPress's dated permalinks used, which were never in a sitemap and still appear in his posts: `productList.aspx/...` (category, brand, filler), `product-detail.aspx/<id>-<sku>-<name>` (that pen if its slug survives, else a search), `new-pens.aspx/...` (brand or search), `TradingPostList.aspx` (the Trading Post), `/YYYY/MM/slug/` (the post, or a blog search), `/on-sale/`. It runs after the redirect table, so his own entries in `/admin/redirects/` win.

## Nathaniel's 2026-10-07 questions (migration 0006)
- **Tags (his q1).** "Tags" row in Item specifics on any product page while signed in: comma separated, his earlier tags offered as he types, 20 at most (`admin/api.rs` field `tags`, `store::set_product_tags`). Each tag has a page at the old WooCommerce shape `/product-tag/{slug}/` (`shop::tag`), `noindex` and out of `sitemap-categories.xml` until it holds `db::TAG_INDEX_MIN` (3) live items. A tag nothing uses is deleted, and an old tag address with no new tag of that name still 301s through the redirect table. Shop search matches tag names. A forced re-import empties the tag links (FK cascade) but keeps the tag names; tag edits are `admin.edit`, so the importer's guard already refuses.
- **Old addresses (q2, q3).** `/admin/redirects/` lists the redirect table (his own first), searches it, downloads it as CSV (`/admin/redirects.csv`), and takes pasted lists: one address per line, optionally followed by its destination. Each line is checked against the running site first (`Probe`, loopback to `HOST:PORT`, no redirects followed): already a page or already redirected = left alone; no destination = matched by the last path segment to a product / post / custom page / brand / category / published glossary entry / Trading Post listing; no match = listed back with a shop-search suggestion to paste again. A destination must be on this site and answer 200. Old URLs with a query string (`/index.php?main_page=…`, `/?page_id=12`) match on path + query first (`db::redirect_for`). His rows carry `redirect.added_by = 'admin'`, only those can be removed, and a forced re-import puts them back.
- **URL audit (q2).** `python3 scripts/url_audit.py <new-base> research/url-audit/<date>.csv` reads the live site's sitemaps today and checks every address on the new site. 2026-10-07 against a local copy: 3,134 addresses, 675 same page, 2,397 redirect, **62 missing: 19 products listed after the 2026-08-24 backup and their 43 tag pages**. Those are fixed by the cutover re-import from a fresh backup and crawl, not by redirects. Re-run the audit after cutover; expect 0 missing.
- **Search spellings (q4).** `/admin/search/`: one line per word, `Sheaffer = Schaffer, Shaeffer, Schaeffer, Shaffer` (setting `search_spellings`; until he saves, the examples from his e-mail, `spelling::default_list`). A shop or blog search for any spelling searches all of them and says "Showing results for “Sheaffer” too." A shop search that still finds nothing tries the make one or two letters off (`spelling::near_miss`). The page also lists searches that found nothing in 90 days.
- **His pages and nib glossary (q5).** The copy he e-mailed on 2026-09-18 lives in `$BACKUP_DIR/client-copy/` and is never committed (the repo is public). `set -a; . ~/.config/thepenmarket.env; set +a; python3 scripts/load-his-copy.py` fills Pen Grading, Defining Nibs and Buyer's Guide only while empty and publishes them, and puts his 34 nib-glossary definitions into glossary drafts (fills empty ones, adds missing words; never publishes). Safe to run again. `--dry-run` prints the SQL.
- **Hero photo (q6).** `/admin/settings/` "Spinning 3D pen or your photograph" (setting `hero_mode`). The photo is the featured pen's first photo. A running or decided `hero-stage` experiment overrides it.
- **Whole pen in frame (q7).** Product photos in cards, category tiles, hero thumbnails, product thumbnails and guide cards use `object-fit: contain` on the grey ground (was `cover`, which cut the ends off long pens).

## Sold Pen Archive, deleting pens, buyer receipts (migration 0007; meeting of 2026-10-05)
- **`/sold-pens/`**: every sold pen, most recently sold first (`product.sold_at`, kept by a trigger whenever status becomes `sold`; pens sold before 0007 have no date and follow the dated ones), searchable and filterable like the shop, in the menu, footer and `sitemap-categories.xml`. Sold pens no longer appear in category lists; every list says how many sold pens match its filters and links to them in the archive. `?sold=1` on the shop still works for old links.
- **Deleting a pen**: "Delete this pen…" under the status menu on its page (password re-entered, refused while someone is checking it out). The pen becomes `archived`: gone from every list, search, sitemap, the archive and the guide; its address 301s to its category. `/admin/pens/` ("Hidden and deleted pens" on the admin home) lists hidden pens (including ones added by accident) and deleted ones: **Restore** brings a pen back hidden; **Erase for good** (password typed again) deletes the row, its photo rows and tags, and adds a redirect from its address to its category (`added_by='admin'`, so it survives a re-import). Orders keep their own copy of name, SKU and price.
- **Buyer receipt**: when Stripe reports a pen paid, the buyer is e-mailed (from `MAIL_FROM`, reply-to info@) the order, the pen's photo and its description as it read at that moment; the description and photo URL are copied onto the order (`shop_order.product_description`, `product_image`). A buyer whose payment was refunded because the pen had just sold is told so. Both go to the mail log until SMTP is set.

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
