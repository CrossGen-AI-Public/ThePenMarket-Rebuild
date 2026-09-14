# ThePenMarket.com rebuild

Concept rebuild of thepenmarket.com for a CrossGen AI pitch: a Rust site (Axum + SQLx + Postgres + Askama) with the client's catalog, blog and Trading Post imported from his backup, and "Ask the Pen Market", an AI guide whose numbers come from `server/src/engine.rs`.

## CrossGen process
Work on a branch, `bash scripts/gate.sh` green, then `/cg:ship`. Production is the user service `thepenmarket.service` on sparky :8140; `scripts/deploy.sh` is what runs after a merge.

## Things the code cannot tell you
- Every string on the site is Nathaniel's (backup pages, posts, products) or computed from the imported catalog. Do not invent facts, numbers, reviews or photos. The expert band has a marked photo slot because no bench photo exists.
- The assistant never states a number from memory. `engine.rs` computes; `guide.rs` only describes tools to the model. `static/js/engine.js` mirrors it for the claude.ai artifact; both run `server/tests/fixtures/engine-cases.json`.
- Old URLs must keep working: the router serves the crawl's URL shapes and the `redirect` table (built at import from the old sitemaps) 301s everything else. Nothing may 404 that used to exist.
- Checkout is not connected (no Stripe keys). Forms store `form_submission` rows and say "demo" on screen.
- `backup/`, `scrape/`, `research/docs/crawl-*`, `server/media/` and `*.env` are never committed.
- Secrets live in `~/.config/thepenmarket.env` on the box. Never print them.
