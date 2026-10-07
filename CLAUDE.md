# ThePenMarket.com rebuild

Concept rebuild of thepenmarket.com for a CrossGen AI pitch: a Rust site (Axum + SQLx + Postgres + Askama) with the client's catalog, blog and Trading Post imported from his backup, and "Ask the Pen Market", an AI guide whose numbers come from `server/src/engine.rs`.

## CrossGen process
Work on a branch, `bash scripts/gate.sh` green, then `/cg:ship`. Production is the user service `thepenmarket.service` on sparky :8140; `scripts/deploy.sh` is what runs after a merge.

## Things the code cannot tell you
- Every string on the site is Nathaniel's (backup pages, posts, products) or computed from the imported catalog. Do not invent facts, numbers, reviews or photos. The expert band has a marked photo slot because no bench photo exists.
- The assistant never defines a pen word from memory either: `define_term` returns his published entry or `found: false`.
- The assistant never states a number from memory. `engine.rs` computes; `guide.rs` only describes tools to the model. `static/js/engine.js` mirrors it for the claude.ai artifact; both run `server/tests/fixtures/engine-cases.json`.
- Old URLs must keep working: the router serves the crawl's URL shapes and the `redirect` table (built at import from the old sitemaps) 301s everything else. Nothing may 404 that used to exist.
- Checkout is not connected (no Stripe keys). Forms store `form_submission` rows (the admin inbox) and are e-mailed only when `SMTP_URL` is set; the confirmation page says which happened.
- The glossary is built (Brittany, 2026-09-21; this replaced the 2026-09-14 plan to leave it as Nathaniel's first Claude Code exercise). The system is ours; the words are his. `glossary::mine` reads his posts and listings for the words in `server/data/glossary-lexicon.json` (names only, no definitions) and proposes hidden drafts with his own sentences attached. Nothing is visible until he publishes it at `/admin/glossary/`. Never write or generate a definition.
- The brand / era / nib / filling-mechanism definitions from `data/terms.json` were drafted by the build, not by him. The glossary, the tap-to-define links and the guide use one only after he ticks it as his (`glossary_approved_def`). Their own pages still show them; that is an open question for him, not something to settle for him.
- Glossary tables have no foreign keys to brand / post / product on purpose: the importer truncates those with CASCADE.
- FAQ answers, custom pages and posts are his words. Seed or draft nothing on his behalf.
- Analytics are first party (`insights.rs`, `track.js`). Never add a third-party tracker; `/privacy/` describes exactly what is recorded.
- `backup/`, `scrape/`, `research/docs/crawl-*`, `server/media/` and `*.env` are never committed.
- Secrets live in `~/.config/thepenmarket.env` on the box. Never print them.
- His unpublished copy (pages, nib glossary) stays in `$BACKUP_DIR/client-copy/` and reaches a database only through `scripts/load-his-copy.py`; the repo is public, so never commit it.
- Search spellings default to the examples in his 2026-10-07 e-mail; once he saves `/admin/search/`, his list rules, even empty.
