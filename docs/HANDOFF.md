# Handoff checklist (ADR-0002)

What still stands between the rebuild and handing ThePenMarket.com to Nathaniel. Built 2026-10-14 from every e-mail (20 threads, 2026-08-13 to 10-14) and every Read AI meeting with him (08-24, 09-14, 09-21, 10-02, 10-05, 10-12), checked against the code on `main`. Owner: **B** Brittany/CrossGen, **N** Nathaniel, **S** Sean. Tick an item only when its check in ADR-0002 passes.

## Do first: security incidents found in the sweep
- [ ] **B+N** His GoDaddy password and a live 2FA code were read aloud on the 08-24 call and are in that Read AI transcript. Change the GoDaddy password and turn on 2FA (Thursday); ask Read AI to remove or restrict that recording.
- [ ] **B** The 09-22 e-mail sent his admin username and a temporary password in plain text for https://penmarket.crossgen-ai.com/ (the public demo). Treat it as exposed: set a new password on every copy (demo droplet, sparky, his droplet), or take the demo's admin offline.
- [ ] **B** A Read AI notice (09-21) shows "Reginald Tallywacker" requesting access to the 09-14 Nathaniel meeting notes. Check it was not granted; remove if it was.
- [ ] **B** Local Read AI tool output with the 08-24 transcript is cached under `~/.claude/projects/...thepenmarket/.../tool-results/`; delete it.

## 1. Secure
- [ ] **B** Connect e-mail (`SMTP_URL`, `MAIL_FROM`) through info@ (his Microsoft/GoDaddy mailbox; may need SMTP AUTH or an app password). Until then the new-device code is skipped and reset links, order mail and lockout alerts don't arrive. **N** confirm info@ is the Microsoft mailbox (asked 10-05, unanswered).
- [ ] **B** Add an authenticator-app (TOTP) or passkey second step for his admin, in place of or beside the e-mailed code.
- [ ] **B** His droplet: SSH keys only, DigitalOcean firewall (80/443, 22 limited), unattended upgrades, Postgres internal only (scripts already do this), nightly `pg_dump` kept 14 days plus DO weekly backups.
- [ ] **N** (with B on screen) Two-step sign-in on his DigitalOcean (then team Secure Sign-In, currently Disabled), Stripe, GoDaddy, Anthropic console and Microsoft e-mail.
- [ ] **B** Stripe: live webhook endpoint and secret on his droplet only; test keys never on it.
- [ ] **B** Dependabot alert 1 (moderate): `rustls` in `server/Cargo.lock` accepts some TLS 1.3 handshake messages across encryption levels; update to 0.23.45 or later (`cargo update -p rustls`), test, deploy.
- [ ] **B** Code scanning: CodeQL's Rust analysis has never completed on this repo; get it to run and clear any high/critical alert. Secret scan of the git history.
- [ ] **B** Decide CrossGen's ongoing access (named accounts on his DO/Stripe, a separate CrossGen admin login or none) with **S**; never share his password (10-05 said both "invite" and "send me the passwords").
- [ ] **B** Old "www" pages Claude found that he thought were private (09-21): decide with him whether they stay public.

## 2. SEO-ready
- [ ] **B** Test every row of his 301 spreadsheet (`000ThePenMarket.com 301 Redirects.numbers`, e-mail 10-14; promised for Thursday) and fix misses. Needs the attachment downloaded.
- [ ] **B** Re-run `scripts/url_audit.py` after the catalog refresh (pens listed since 08-24 need addresses).
- [ ] **B** `gate.sh` and `full_audit.sh` green against the live domain (TC 7, 8, 10).
- [ ] **N+B** Google Search Console on his Google account: verify thepenmarket.com, submit the sitemap, check coverage a week after launch.
- [ ] **N** Google Business Profile / Place ID for reviews (code ready, `reviews.rs`; he has no reviews set up yet, 09-15, 10-07).
- [ ] **N** Review the 91 build-drafted term definitions and 6 category intros still shown on their own pages, and the 12 FAQ answers taken from his pages on 09-22 (meeting 09-21: "kind of mock data").
- [ ] **N** Claims flagged in discovery: "one of the fastest-growing pen retailers in the country" (About) and similar; keep, substantiate or drop.

## 3. Laptop and phone
- [ ] **B** Full overflow/console sweep 320 to 1920 px on the live site after the last design changes (done locally 10-14; repeat on the droplet).
- [ ] **B** Accessibility pass (contrast, labels, keyboard) on home, shop, product, checkout, Trading Post form, admin.
- [ ] **B** Undecided extras from 09-14: dark/light theme toggle (not built), basic/advanced mode (Classic view built). Ask him whether either is wanted for launch or later.

## 4. Easy for customers (and for him)
- [ ] **B** Stripe end to end in test mode with his test key: pen (US, international, Puerto Rico), $5 listing, membership; receipts with photo; then live keys and one real ~5-cent purchase to info@ (10-12).
- [ ] **B** Stripe Tax: Connecticut only, threshold monitoring on (10-14 e-mail). **N** approve, register CT in Stripe.
- [ ] **B** Payment methods: "all methods", "as long as it doesn't bite me" (10-12). Answer his fee question (wallets, bank transfer for expensive pens) before enabling.
- [ ] **B** Unlimited Posts becomes one $125 payment for a year, no auto-renew (his e-mail 10-14); expiry reminder e-mail if he wants it.
- [ ] **B** Trading Post: every listing, paid or member, waits for his approval with an e-mail (10-14).
- [ ] **B** Trading Post: "Sold, remove it" deletes for good, not into the Sold Pen Archive (10-14).
- [ ] **N** then **B** Seller self-edit by private link (price, description, sold; title and photo locked) once he says yes.
- [ ] **B** Add the one new Trading Post listing he names by hand (old site's images are broken: do not scrape it, 10-14).
- [ ] **N** Approve the FAQ line on import duties ("International buyers are responsible for…"), or leave it out.
- [ ] **B** Search misspellings beyond makes (models, filling systems, "duo fold", "parker51") if wanted; makes are done.
- [ ] **B** Brand order by drag-to-rank (09-21); pinning exists, drag does not.
- [ ] **B** Repairs page clean-up offered on 09-14; check with him.
- [ ] **B** Sourcing tool: finds pens at his selling price, not a buyable margin (10-05); needs eBay developer keys from **N**.
- [ ] **N** On the handoff call: list a pen with photos, feature it, upload/let the Mac make its cut-out, post a blog entry, edit a page, approve a Trading Post listing, without help.

## 5. Live on his DigitalOcean
- [x] Brittany is a Member of his team "My Team" (checked 10-14).
- [ ] **B** Create the droplet (Basic 2 GB, about $14.40 with backups), billing alert $20 (**N** as Owner), deploy with `push-droplet.sh` to a preview name first (no domain).
- [ ] **N+B** Look at his DigitalOcean billing together (the "$13" he saw; no droplets exist yet).
- [ ] **N** Re-send GoDaddy delegate access ("Products & Domains"); the 10-12 invite expired.
- [ ] **N** Anthropic console invite (he thinks he sent it; nothing arrived) and credit; then **B** moves the guide to his Claude key (code still speaks the OpenAI shape to CrossGen's model).
- [ ] **B** Catalog refresh from his live store (264 products on 10-14 vs 238 here): new pens with photos, sold pens marked, posts since 08-24; Trading Post excluded. Run again on switch day.
- [ ] **B** Deploy script parameterised (done 10-14) and run once on his droplet; `www` redirect added before go-live.
- [ ] **B** After his recovery (he prefers about two weeks after the 10-16 surgery): lower DNS TTL a day before, point thepenmarket.com and www at the droplet, MX untouched, check mail still arrives.
- [ ] **B** Rollback written and tried once (TC 15). Keep GoDaddy hosting a few weeks after launch; **N** cancels it only then.
- [ ] **B** Retire or lock the public demo at penmarket.crossgen-ai.com once his is live.

## 6. His own login
- [ ] **B** Set his password on the copy that goes live (rotating the e-mailed one); he signs in from his own computer, changes it at `/admin/password/`, and resets it by e-mail (needs SMTP).
- [ ] **B** "Set up a Mac" for automatic cut-outs on his Mac (needs the public preview address).
- [ ] **B** A short written guide for him: signing in, listing a pen, featuring, Trading Post approvals, orders, where help is (he asked for a manual/syllabus on 09-22; none was sent).

## Agreed but still to schedule
- [ ] **B** First month after launch: A/B test two homepage versions and keyword variants with the heat map (10-12). Experiments tool exists; define versions and what counts as winning.
- [ ] **S** What monitoring and reporting the agreement covers after handoff (10-12; draft e-mail to Sean waiting). **B** then set up uptime and error alerts accordingly.
- [ ] **B** Architecture notes for maintaining the site with Claude after handoff (promised 10-02).
- [ ] **B** Training: dedicated Claude basics session offered 09-22; Blender method for Dawn offered 10-12; Sell-my-pens AI intake and consignment-report skill deferred (10-02, 10-05).
- [ ] **B** Reply to the e-mail questions with no written answer yet: product tags, URL list, misspellings, pages, real photo for the pen, cropped photos (all from 10-07, now built: say so), "is there a better shipping option" (10-12), A/B samples and a training syllabus (09-22).

## Built and verified (from his requests, for the record)
Product tags; old-address redirects (2,504 rows + pre-WordPress shapes); search spellings for makes; his three pages and his nib glossary published; glossary and site map; hero photo default with optional 3D; whole pen in every frame; standing cut-out on the homepage with turn-around, upload and Mac helper; delete pens with password; Sold Pen Archive; receipt with photo and description; shipping rates ($6/$15/$55) with territories at US rates; Trading Post $5 fee and membership (to be changed above); 14-photo cap; guided photo shoot; click-to-edit and preview; blog and page editors; FAQ fed to the guide; insights, heat maps, experiments; most popular row; brand pinning; Classic view; "Vintage Pens"/"Pre-Owned Pens" labels; nav with Pen Grading and Glossary; search icon; security review of 2026-10-12 (rate limits, lockout, argon2, headers, CSRF).
