# ADR-0002: Handoff is done only when the site is secure, SEO-ready, works on laptop and phone, is easy for customers, runs on his DigitalOcean, and he signs in with his own login

> Decision date: 2026-10-14
> Decided by: Brittany ("done is a completely secure and SEO compliant site which is both laptop and mobile friendly and also is easy for customers to use and upload/search for pens. It should be deployed on DigitalOcean and have Nathaniel able to access the admin panel with just his login info.")
> Status: Accepted (Brittany, 2026-10-14)
> Scope: the whole ThePenMarket.com rebuild; the handoff to Nathaniel Cerf. The working checklist is `docs/HANDOFF.md`.
> Pairs with: ADR-0001 (never link to a page that no longer exists); the decision of 2026-10-05 that every paid service runs on his own accounts (memory "Nathaniel owns all accounts")

## Context

By 2026-10-14 the site is feature-complete enough to demo: catalog, blog, Trading Post, his pages and glossary, Stripe checkout with his shipping rates, the admin panel, the homepage photo tooling. It runs on sparky (CrossGen's private network) and an older copy runs on CrossGen's droplet. What it is not yet: on his own DigitalOcean account, on thepenmarket.com, taking real payments, sending e-mail, or signed into by him from his own computer. Several of his requests from the meetings and e-mails are still open, and the work has been tracked across PROGRESS.md, meeting notes and e-mail threads. Without one written definition of "done", the project risks being called finished at "it works on sparky", or never being called finished at all.

## Decision

The project is handed off only when all six of these hold, each checked as in Testable Criteria:

1. **Secure.** HTTPS everywhere with the security headers already in the code; his admin protected by his password plus a second step that reaches him (e-mail code at least, an authenticator app or passkey preferred); sign-in rate limits and lockout active; secrets only in the server's env file; no open high or critical code-scanning alerts; the server hardened (SSH keys only, firewall, automatic security updates, database not reachable from outside, nightly database backups); two-step sign-in on his DigitalOcean, Stripe, GoDaddy, Anthropic and e-mail accounts.
2. **SEO-ready.** Every indexable page passes the gate's SEO check; every old address (the crawl, the redirect table, and his 301 spreadsheet) lands on a live page; no broken links; sitemaps and robots correct; the site verified in his Google Search Console with the sitemap submitted.
3. **Laptop and phone friendly.** No sideways scrolling and no console errors from 320 to 1920 px on every route in the gate; text never splits inside a word; tap targets usable on a phone.
4. **Easy for customers.** A customer can find a pen by name, brand, era or nib (misspellings of makes corrected), buy it with a test card on a phone and on a laptop, and post a Trading Post listing with a photo from a phone; Nathaniel can list a pen with photos and feature it on the homepage without help.
5. **Live on his DigitalOcean.** The site, its database and photos run on his account, under thepenmarket.com with a valid certificate; his e-mail (MX) untouched; the catalog refreshed from his live store immediately before the switch; rollback written down and tried once.
6. **His own login.** He signs in at /admin/ from his own computer with only his e-mail and password (and the code or app on his own device), changes his password, and resets it by e-mail, with nobody from CrossGen in the loop. CrossGen keeps access only through its own named accounts, never his.

## Alternatives Considered

- **Done = feature list complete.** Rejected: features were mostly complete on 2026-10-14 while none of 1, 5 or 6 held.
- **Done = live on thepenmarket.com.** Rejected: going live without e-mail, backups or his own sign-in would hand him a shop he cannot run alone.
- **Host on CrossGen's DigitalOcean and bill him.** Rejected on 2026-10-05: his accounts, his costs; CrossGen is invited, not the owner.

## Consequences

- Positive: one checklist everyone can read; "is it done?" has a yes/no answer; security and SEO cannot be skipped to hit a date.
- Positive: his independence is part of done, which is what the handoff is for.
- Negative: handoff waits on things outside the code: his Stripe bank account, GoDaddy access, e-mail credentials, his availability after surgery, and Sean's answer on monitoring.
- Negative: some items cost money every month on his card (server about $14.40, Stripe fees, Stripe Tax 0.5% on Connecticut orders if used).
- Negative: "completely secure" is a target, not a guarantee; this ADR fixes what we check, not that nothing can ever go wrong.

## Testable Criteria

| TC | Criterion | How verified |
|----|-----------|--------------|
| 1 | Every response on thepenmarket.com is HTTPS with HSTS, CSP, nosniff, frame-deny, referrer and permissions policies | `scripts/full_audit.sh https://thepenmarket.com` headers row; a manual check at securityheaders.com |
| 2 | Admin sign-in: 15-character minimum and breach check, argon2 hashes, 10/min per address, lockout after 10 failures, new-device code delivered to his inbox | `cargo test`; sign in from a new browser and receive the code by e-mail; 11 wrong passwords lock new devices only |
| 3 | E-mail connected (`SMTP_URL`, `MAIL_FROM`): codes, reset links, order and Trading Post mail arrive | Send each once on the live site; check `/admin/history/` |
| 4 | No open high/critical code-scanning alerts; no secrets in the repository | GitHub code scanning page; a secret scan of the history |
| 5 | Server: SSH keys only, DigitalOcean firewall allowing 80/443 (22 from known addresses), unattended upgrades, Postgres on the internal network only, nightly `pg_dump` kept 14 days | `ssh` with a password refused; `nmap` from outside shows 80/443 only; backup files present |
| 6 | Two-step sign-in on his DigitalOcean (and team Secure Sign-In), Stripe, GoDaddy, Anthropic and Microsoft e-mail accounts | Shown on screen during the handoff call |
| 7 | Gate green on the live site: engine, SEO, sitemaps, glossary, redirects, links, console, overflow, screenshots, slop | `bash scripts/gate.sh https://thepenmarket.com` |
| 8 | Every old address lands on a 200: the crawl, the 2,500+ redirect rows, every row of his 301 spreadsheet | `full_audit.sh` redirects row; the spreadsheet check in `research/url-audit/` |
| 9 | Site verified in his Google Search Console, sitemap submitted, no coverage errors after one week | Search Console screenshot |
| 10 | No overflow and no console errors 320 to 1920 px on every gate route; hero text never splits a word | `full_audit.sh` overflow and console rows; `hero_check` |
| 11 | Search finds pens by name, brand, era, nib; brand misspellings corrected | Gate redirect/search rows; the misspelling list from 2026-10-14 |
| 12 | Test-card purchase completes on a phone and a laptop (US and international shipping), Trading Post listing with photo posts from a phone and waits for his approval | Stripe test mode, recorded in PROGRESS.md |
| 13 | He lists a pen with photos and features it without help | Watched on the handoff call |
| 14 | Site, database and photos on his DigitalOcean; thepenmarket.com resolves to it; MX unchanged; catalog refreshed from his live store the day of the switch | DNS lookup; product count matches his store; mail to info@ still arrives |
| 15 | Rollback written and tried once (previous image, DNS back to GoDaddy hosting) | RUNBOOK entry dated with the trial |
| 16 | He signs in, changes and resets his password from his own computer, unaided | Watched on the handoff call; `/admin/history/` entries |
| 17 | CrossGen access is through named CrossGen accounts only; nobody but him knows his password | Admin accounts list; DigitalOcean/Stripe team lists |

## References

- `docs/HANDOFF.md`: the working checklist this ADR is measured by, built from every e-mail and Read AI meeting with him.
- `docs/RUNBOOK.md` sections "Security (review of 2026-10-12)", "The admin", "Nathaniel's droplet", "Online checkout".
- `scripts/gate.sh`, `scripts/full_audit.sh`.
- ADR-0001.
