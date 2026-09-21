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
- **His 2026-09-18 page list.** `pen-grading`, `defining-nibs` and `buyers-guide` are seeded once as empty drafts (`routes/content.rs::seed_requested_pages`) for his own copy; the editor has "Link to a blog post" for the Buyer's Guide. The glossary is not built (his Claude Code session). The three-pen row under the hero was removed at Brittany's request.
- **Google reviews** (`reviews.rs`). He has none yet. Saving his Google Business Profile Place ID at `/admin/settings/` adds "Review us on Google" to every footer. With `GOOGLE_PLACES_KEY` (Places API New) the server fetches rating and reviews daily and the homepage shows what Google returned, verbatim, only when at least one review exists. **Never run against the live API: no key or profile existed when it was written.**
- **Classic view** (footer switch): larger text, photograph instead of the 3D pen, no motion; remembered per browser. The 3D pen's "+" markers are buttons that move the camera in.
- `import` also refuses after post, page, brand or media edits (same `FORCE_IMPORT=1` override).

## The public copy (droplet): https://penmarket.crossgen-ai.com/
Deployed 2026-09-14 by `scripts/push-droplet.sh` (from sparky) + `scripts/deploy-droplet.sh` (on the droplet, as root):
- containers `thepenmarket` (the app, image built from `server/Dockerfile`) and `thepenmarket-db` (postgres:16-alpine, volume `thepenmarket_pgdata`) on the `web` network; Caddy block `penmarket.crossgen-ai.com` in `/root/proxy/Caddyfile`.
- files: `/srv/apps/thepenmarket/build` (build context), `/srv/apps/thepenmarket/media` (photos, mounted read-only), `/srv/apps/thepenmarket/uploads` (form photos), `/srv/apps/thepenmarket/.env` (secrets), `db.sql.gz` (the dump restored on first run).
- redeploy: `bash scripts/push-droplet.sh` from sparky (rsyncs, dumps the sparky database, rebuilds the image, swaps the container, health-checks). The Rust build takes about 8 minutes on the droplet's two cores.
- rollback: `docker run` the `thepenmarket:previous` image with the same flags.
