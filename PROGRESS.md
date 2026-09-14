# PROGRESS — The Pen Market rebuild (headless overnight run, started 2026-09-14 05:10 UTC)

## Timeline
- 05:10 UTC  Step 0 done: read new-customer SKILL + references/scripts/templates, site-spike refs, search-first-build refs, DISCOVERY.md, architecture.md, backend-brief.md, design canvas (Main.dc.html + sketches), rejected draft, backup layout, crawl sitemaps. Toolchain checked: Rust 1.98.1, sqlx-cli 0.8.6, Postgres 16 on port 5433 (not 5432), Chromium, Docker, gh (account Rivzz). Port 8140 free.
- 05:12 UTC  Phase 0: research/industry.md, BRIEF.md, PROGRESS.md written. Crawl (scripts/crawl.sh) finished into scrape/ (40 core pages). Live REST pulled: 78 Trading Post listings (scrape/trading-post/page1.json) and taxonomy terms with counts (scrape/taxonomies/*.json).

## Decisions made
- Postgres lives on sparky port 5433 (the only cluster). Role+db `thepenmarket`, DATABASE_URL in ~/.config/thepenmarket.env (chmod 600).
- Trading Post listing bodies were not in the Aug 24 backup (only the sitemap URLs). The public WordPress REST route for `trading_post` still answers, so the listings were pulled live during the crawl. They are imported read-only.
- Missing blog images that are not in the backup keep their original thepenmarket.com URL (real links, CSP allows that host for images) rather than being dropped or faked.

## Known gaps
- No photo of Nathaniel at the bench exists in the backup; the "expert band" gets a clearly marked slot.
- No Stripe keys: buy button is a labelled demo.
- 05:25 UTC  Phase 1 done: crawl (40 pages) + live taxonomy and Trading Post REST pulls; Dribbble pull (32 shots, 4 grids); current-site screenshot; eBay home/results/item captured via Playwright (headless Chrome was bot-blocked; the category id resolved to bullion listings, layout is what is used). Four research agents finished: research/evidence.md, ai-in-industry.md, domain-facts.md, scrape/inventory/*.md.
- 05:32 UTC  Phases 2-3 written into BRIEF.md: "Ask the Pen Market" (engine computes, model talks), canvas leads the homepage, eBay leads shop/product structure, Heritage Classic Car shot leads craft, hero = raymarched laminated celluloid barrel.
- 06:10 UTC  Rust crate: migrations (13 tables), importer (248 products / 745 photos with 480+960 variants / 261 posts / 20 pages / 78 listings / 2,502 redirects), engine.rs with 24 shared fixture cases (Rust + JS mirror both green), terms.json definitions (8 eras, 10 nibs, 19 mechanisms, 43 brands, 6 categories, 11 blog categories).
- 06:40 UTC  Server: Axum router (every route explicit), Askama templates (20), CSS, site.js, guide.js, hero.js, security middleware (CSP/HSTS/nosniff/frame-deny, CSRF double-submit + HMAC, per-IP rate limits, 301 canonicaliser, rendered 404). Clippy clean (unwrap/expect denied).
- 07:20 UTC  First render: every route 200 with one h1; console clean at 1440 and 500 on home/shop/product/blog/trading-post; assistant answered two real questions through the hosted model calling find_pens and repair_scope. Fixed: `.phone` class collision (masthead vs chat panel), 308 -> 301, rendered 404, hero composition, fountain pens first in "Just in", distinct edit photos, flex nib glyph, small-featured-image fallback, phone hero scale.

## Decisions made (continued)
- Category archives include sold one-offs (with a "Sold" chip) so a category whose stock sold out (cameras: 3 of 3 sold) is not an empty page; the shop and facet pages show live items only, with an "Include sold pens" link.
- 26 product URLs in the Aug 17 sitemap are not in the Aug 24 CSV (sold and removed between the two pulls); they 301 to /shop/. 1,000 post-tag and 643 product-tag archives 301 to blog/shop searches or the matching brand page.
- 459 inline blog images were already 404 on the live site (the old /vintage-pens-blog/ upload path was not migrated); Wayback recovered 46 files; the rest are omitted from the post HTML and listed in research/import-log.md. All 235 featured images are present.
