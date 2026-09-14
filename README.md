# ThePenMarket.com rebuild

A concept rebuild of [thepenmarket.com](https://thepenmarket.com) (Nathaniel Cerf's vintage pen shop, Norwich CT) for a CrossGen AI pitch: one Rust binary (Axum + SQLx + PostgreSQL + Askama), the client's whole catalog imported from his site backup, every old URL forwarded, SEO and AEO built in, and "Ask the Pen Market", an AI guide whose numbers come from a Rust engine over the live catalog while CrossGen's hosted model only talks.

- Live demo (Tailscale): http://100.117.164.79:8140/
- Shareable preview: `dist/index.html` (the homepage + the assistant in claude.ai sample mode, catalog snapshot inlined)
- Brief, decisions and assumptions: `BRIEF.md`; run log: `PROGRESS.md`; client report: `REPORT.md`
- Operations: `docs/RUNBOOK.md`

## Layout

```
server/            the Cargo crate `thepenmarket`
  src/main.rs      Axum server (bin), src/bin/import.rs (importer)
  src/engine.rs    the pen guide's engine: pure, unit-tested, fixtures shared with static/js/engine.js
  src/guide.rs     /api/guide: system prompt, tools, hosted-model call
  src/routes/      every public path, explicitly
  src/db.rs        all SQL (parameterised), src/wp.rs WordPress import helpers
  migrations/      sqlx migrations
  templates/       Askama (server-rendered, complete HTML on every route)
  static/          css, js (site, engine, guide, hero), self-hosted fonts, three r160 UMD
  data/terms.json  definitions for brand / era / nib / filling-mechanism pages, in his voice
  media/           product photos copied from the backup at import (gitignored)
scripts/           gate.sh (the whole test gate), seo_check.py, sweep_url.py, chat-drive.js,
                   build-artifact.py, deploy.sh, deploy-droplet.sh, test-engine-js.mjs
research/          discovery, evidence, AI-in-industry, domain facts, design canvas, import log, gate output
```

## Run locally

```
# secrets: ~/.config/thepenmarket.env with DATABASE_URL, CSRF_SECRET, GUIDE_AI_URL, GUIDE_AI_KEY, GUIDE_AI_MODEL
cd server
cargo run --release --bin import      # backup/ -> Postgres + media/ (idempotent; writes research/import-log.md)
cargo run --release                   # http://0.0.0.0:8140
cargo test --lib && node ../scripts/test-engine-js.mjs
bash ../scripts/gate.sh http://127.0.0.1:8140
```

## What is demo-only

No payment processor is connected: the buy button is labelled "Demo — checkout not connected" and explains how to buy by phone or e-mail. The repair, sell, contact, mailing-list and Trading Post listing forms store a `form_submission` row on the server and say so; nothing is e-mailed and no listing fee is charged. Trading Post listings are imported read-only from the live site.
