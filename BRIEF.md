# BRIEF — The Pen Market (ThePenMarket.com) rebuild

## Who
Nathaniel Cerf, owner and restorer, ThePenMarket.com. P.O. Box 1086, Norwich, CT 06360-1086. (847) 708-5062. info@thepenmarket.com. Online since 2007 (Chicagoland, now Connecticut). One-person shop: restores and sells vintage fountain pens and pre-owned modern pens, repairs pens, buys and consigns collections, runs the Trading Post classifieds ($5/listing, $125/yr Unlimited Posts), writes the "Drippy Musings" blog (261 posts since 2013).

## Why
Brittany's instruction (2026-09-13): run the `new-customer` skill for penmarket with a full Rust backend and all previously pulled data. The current site is GoDaddy Managed WordPress + WooCommerce + Beaver Builder, 1.0–1.2 MB of HTML per page, 62 CSS/JS files per load, no `<h1>` on home/blog/trading post, default category titles, stock refund boilerplate contradicting the real 14-day guarantee (research/backend-brief.md). The rebuild is the pitch: one compiled Rust binary, typed catalog, faceted shop, SEO/AEO built in, an AI guide whose numbers come from the catalog.

## The one thing the site must do
Let a visitor find the right pen in a one-of-a-kind catalog of 247 items and trust the man behind it (restored, tested, 14-day guarantee), then buy, sell, or send a pen for repair.

## Hard constraints
- Rust: Axum + tokio + SQLx + PostgreSQL + Askama SSR (research/architecture.md, ADR-0001…0006). `#![forbid(unsafe_code)]`, clippy `unwrap_used` denied outside tests, parameterised SQL, CSP/HSTS/nosniff/frame-deny, CSRF on mutating forms, body limits, rate-limited assistant.
- Everything derives from this client: copy verbatim from scrape/ and backup/; numbers computed from the imported catalog; product photos are his own. No bench photo of Nathaniel exists: marked slot, not a stock image.
- Checkout: no Stripe keys. Buy button is a labelled demo ("Demo — checkout not connected").
- Forms (repair estimate, sell my pens, contact) POST to Rust, store a `form_submission` row, labelled demo, no email goes out.
- Customer and order PII never exported; none invented.
- Assistant: CrossGen's hosted model via GUIDE_AI_* (copied from ~/.config/kindlending.env on the box). Engine in Rust (`server/src/engine.rs`, unit-tested), exposed to the model as tools. Artifact `sample` mode carries a JS mirror tested against the same fixtures. No canned-answer mode, ever.
- Design lead: research/design-canvas/Main.dc.html (A + B: Market Floor + Atelier). Palette ivory #F7F5F0, ink #1C222C, navy #1E3A5F, gold #C9A64C. Type Cormorant Garamond + Archivo + IBM Plex Mono. No top utility bars. eBay as design reference only (shop results, facet rail, product page). Dribbble lead shot for craft where both are silent.
- SEO/AEO per ~/.claude/skills/search-first-build: one h1/title/description/canonical per route, JSON-LD graph, split sitemaps, robots.txt allowing AI crawlers, /llms.txt, answer-first copy, definitional cluster for brand/era/nib/filling-mechanism.
- URLs from the crawl's sitemaps preserved; anything changed gets a 301. Nothing 404s.
- Ports: 8140 on sparky. Do not touch 5050 (CE Pros).
- Deadline: before Brittany's morning (REPORT.md is the completion marker).

## Assumptions (running list; the shakiest go in the report)
1. "Restored one at a time in Chicago since 2007" from the canvas is inaccurate: the crawl shows Norwich, CT and the About page says the business moved from Chicagoland. The site will say "Online since 2007" and "Norwich, Connecticut" (both from the crawl) and not "Chicago".
2. The Chicago-area phone number (847) is kept as shown on the live site; DISCOVERY flagged it as an open question for the client.
3. The 14-day guarantee is named from his own page: "Our Guarantee" wording, "14-day returns, 30-day repair promise" (the canvas's phrasing of the guarantee page's two windows).
4. The `/refund_returns/` stock WooCommerce boilerplate is not carried over; it 301s to /guarantee/ (the real policy). Flagged as a decision for Nathaniel.
5. Trading Post listings are imported read-only from the live REST route (78 listings, pulled 2026-09-14); the $5 listing form and the $125/yr membership are described but the listing form is a labelled demo.
6. Post tags (1,000) and product tags (643) do not migrate as pages; their URLs 301 to the nearest brand/category/blog page so nothing 404s.

## Phase log
- Phase 0 (05:10 UTC): set up. Crawl done. Live taxonomy + Trading Post pulled.

## Phase 2 — the AI feature (decided 05:30 UTC)
**"Ask the Pen Market"**: a grounded guide over the imported catalog and his published policies. It turns "a flexible-nib lever filler under $200" into the real in-stock pens, explains nib widths and filling systems in his terms, says whether a repair is in his scope, triages a sell-in, computes guarantee dates, and hands off to Nathaniel.

Three reasons the owner would buy it (from research/ai-in-industry.md and the crawl):
1. **A number next to the pain.** 53% of consumers abandon and shop elsewhere when they cannot find one item (Google Cloud/Harris Poll, ~13,500 adults, cloud.google.com/blog/topics/retail/new-research-on-search-abandonment-in-retail). His catalog is 247 one-of-a-kind items across 5 facets; the first-time buyer's question is the title of his own 17-post series, "How Do I Start Collecting Pens?".
2. **A rules engine computes, the model only talks.** Every price, count, length, guarantee date and repair-scope answer comes from `server/src/engine.rs` over the imported catalog and the verbatim policies (14-day return, 30-day repair promise, repair list and exclusions, sell-in exclusions). No model-made numbers; no authenticity claim beyond what he wrote on the listing (FTC AI-claims guidance and the Fake Reviews Rule are cited in the research).
3. **It ends with a person.** Repair estimates, sell-in requests and grail-level pens hand off to Nathaniel with a two-sentence summary and the pen list already assembled: the first ten minutes of his job, done, after hours.

Model: CrossGen's hosted model (GUIDE_AI_URL/KEY/MODEL from the box). Two run modes: server `/api/guide` (Rust engine as tools) and the claude.ai artifact `sample` mode (JS mirror of the engine, tested against the same fixtures). Offline notice otherwise; never a canned answer.

## Phase 3 — Direction (decided 05:32 UTC)
**Layout lead: the design canvas** `research/design-canvas/Main.dc.html` ("Homepage — A + B"): masthead with serif wordmark, live-count search, caps nav; taxonomy nav with live counts; compact dark celluloid hero band with kicker / serif headline / mono guarantee strip; "Just in" feed (5 cards: month + SKU, name, mono price, era + state chips); "This week's edits" (three edits computed from the catalog); "The case" grid with filter chips; the expert band (marked photo slot, "Every pen on this site passed through my hands.", the four checks); "The vault" grail strip; "Know your nibs"; sell / trading post / bench strip; footer. No top utility bar.

**Design reference, structure only: eBay** (screenshots under `research/ebay/`: `ebay-home.png`, `ebay-results.png` = `/b/Vintage-Fountain-Pens/14016/...` (eBay resolved the category id to a bullion listing set; the layout is what is borrowed), `ebay-item.png`).
- Results page (`ebay-results.png`): breadcrumb above a plain `h1`; left rail "Shop by category" with the current node bold; a row of attribute filter chips (each a dropdown) ending in "All Filters"; "N results" on the left with Sort and a list/grid toggle on the right; item cards with image left, title, one grey sub-line, bold price, a shipping line and a red scarcity line; a heart affordance top-right of the image. **Borrowed:** the rail + chips + count/sort/view row + card anatomy (image, title, sub-line, price, state line). **Why for him:** his owners tout the faceted filter (Search & Filter Pro with brand, era, nib, filling system, price range), and one-of-a-kind stock needs a state line (Restored / Sold / Sale) where eBay shows scarcity.
- Item page (`ebay-item.png`): gallery left with expand and watch affordances; buy box right (title, seller line, price, condition row, quantity, primary and secondary buttons stacked); a "shipping, returns and payments" label/value table; "Shop with confidence" guarantee block; below, an "Item specifics" two-column label/value table and "Item description from the seller". **Borrowed:** gallery + buy box split, the label/value specifics table, the guarantee block under the price, the description heading. **Why for him:** every product carries structured fields (Filling Mechanism / Era / Nib Size / SKU / capped length) that the old site buried in prose; the 14-day guarantee is his proof and belongs beside the price.
- Home (`ebay-home.png`): nothing borrowed; the canvas wins.
- **Never borrowed:** eBay's blue, its logo, Market Sans, "Buy It Now", "Watchlist", "Top Rated Seller", badges, icons, listings or images. Colours and type stay the canvas's; words stay Nathaniel's.

**Dribbble lead shot (craft where canvas and eBay are silent):** "Heritage Classic Car Shop Landing Page" by Pixelz, https://dribbble.com/shots/23179951-Heritage-Classic-Car-Shop-Landing-Page (`research/dribbble/img/vintage-pen-shop-landing-page-01.png`). Borrowed: the dark inset card sitting on warm paper with the product photo, a small outlined "feature" chip and a stat with faces inside the card; a display serif set very large as a section word; the pill CTA that breaks the card corner. Applied to: the hero band's dark card on ivory (the canvas already wants it), section words ("Just in", "The case", "The vault") set in Cormorant at display size, the guarantee strip inside the dark card. Supporting: "Vintage camera shop" by Mike Taylor, https://dribbble.com/shots/26708576-CamCase-Vintage-Camera-Shop-Landing-Page-UI-Design is not the one; the supporting shot is the Kodak/Retina landing (`research/dribbble/img/vintage-pen-shop-hero-section-05.png`, Mike Taylor, https://dribbble.com/shots/26302344--Vintage-Camera-Landing-Page-UI): its numbered stats ledger ("Restored cameras archived and counted 120+") and marque strip ("Canon | Nikon | Kodak | Leica") lend the treatment for the canvas's stats and "Shop by marque" rows. Never borrowed: their palettes (cream/orange, blue/yellow) or type.

**Tokens (all from the canvas, which drew them from his product photography: ivory felt, blue-black ink, 14k gold nibs):**
- `--ivory #F7F5F0` page; `--ink #1C222C` text and dark band; `--navy #1E3A5F` links, primary actions, chips; `--gold #C9A64C` kicker and rule accents; `--line #DDD8CB` hairlines; `--muted #6E6959` secondary text; `--paper #FFFFFF` cards.
- Type: Cormorant Garamond 500/600 + italic 500 (display and headings; the canvas's serif); Archivo 400/500/600/700 (UI and body); IBM Plex Mono 400/500/600 (prices, SKUs, counts, dates). Self-hosted woff2 under `server/static/fonts/`. Display `clamp(34px, 1rem + 3.2vw, 56px)`; eyebrows 11px caps +0.22em (the canvas's kicker).
- Radius: 0 on cards and inputs (the canvas is square-cornered, ledger-like); pill only on the primary button (the Dribbble lead's one break of the rule).
- One motion idea: the hero's celluloid barrel slowly rolls; pointer tilts it. Everything else is static; reduced-motion renders a still.
- No mascot (the brand has none; the red "drippy pen" only names the blog).

**Hero scene (three concepts, `references/three-hero.md` §1):**
1. **Laminated celluloid barrel, raymarched.** Source: the canvas hero photo (a Parker Vacumatic's golden striped celluloid, `research/design-canvas/celluloid-band.jpg`) and the 11 Vacumatics in the catalog. Picture: a single translucent barrel lying across the dark band, lit from behind so the amber and pearl laminations glow and the ink level shows through, the two-tone nib catching one highlight. Technique: signed-distance raymarching of the barrel and nib in a fragment shader, a procedural laminated-celluloid material (stacked translucent rings with fbm pearl) with a thin-slab transmission approximation and a bloom pass baked as additive glow. Motion: the barrel rolls slowly; the pointer tilts it and the ink inside sloshes.
2. **The Vacumatic filling cutaway.** Source: pen-repairs page ("Vacumatic won't fill?") and his "How do I restore a Parker Vacumatic" post. Picture: a translucent barrel in section, the diaphragm and plunger pumping, ink rising with each stroke. Technique: procedural geometry with a custom vertex deformation for the diaphragm and a fill-level shader. Motion: the pump cycle. Rejected: too instructional for the homepage band and it reads as a diagram, not a shop.
3. **Ink on paper.** Source: his ink-test posts. Picture: a nib laying down a wet line that dries and shades. Technique: GPGPU fluid on a paper texture. Rejected: any ink brand could use it (fails the competitor test) and it approaches the "particles" cliché.
Winner: 1. A raymarched laminated-celluloid barrel is a material no other shop's site would render; a WebGL engineer would recognise it as "SDF raymarch with a procedural translucent layered material and pointer-driven rotation". Honest question (§4): a designer who has seen a hundred AI hero sections would stop on a glowing Vacumatic barrel rendered in shader code; the cliché list contains no pens, no celluloid, no laminated translucency.

**Client-derivation audit (one sentence each):** serif + caps type because the canvas chose it for the Atelier voice and his product names are proper nouns; ivory/ink/navy/gold because those are the felt, ink and nib colours in his own photographs; square cards because his catalog is a ledger of one-offs with SKUs; mono for prices/SKUs because his product fields are data; the facet rail because he tells owners the faceted filter matters; the specifics table because his products carry structured fields; the guarantee beside the price because "restored, tested and guaranteed" is his Shop meta description; the marked photo slot because no bench photo exists and a stock one would be a lie; no top utility bar because Brittany had them removed.

## Assumptions added during the build
7. Copy from the design canvas is used where Nathaniel has no equivalent line and Brittany chose it with the client work: the hero headline "Pens with a past, ready for your desk.", the three edit blurbs, the expert-band headline "Every pen on this site passed through my hands." and the nib-strip one-liners. Everything else on the site is his, verbatim, or computed. Flagged for his approval.
8. The About page omits one sentence of his own copy, "one of the fastest-growing pen retailers in the country", because no source supports it (site-spike rule: never carry an unverifiable claim silently). It quotes The Day (2022) and Dr. Tobias Goodman instead, both sourced.
9. "Just in" and "The case" show fountain pens first (ballpoints, rollerballs and pencils stay in the shop); the newest additions on Aug 24 were pre-owned ballpoints, which would have led a vintage shop's homepage otherwise.
10. Trading Post listings show the seller contact exactly as the live site does (the listing is the seller's public ad); whether the rebuild should put contact behind a form is a decision for Nathaniel.
11. The hero background is one of his own product photographs (SKU 5740, Parker Duofold Geometric) under the raymarched barrel; the canvas's celluloid macro was a base64 crop too small to stretch across 1440px.

## Phase log (continued)
- Phase 4 (05:35–07:30 UTC): importer, engine, server, templates, CSS/JS, hero, guide. First gate rows green by hand (routes, console, engine fixtures Rust + JS).
- Phase 4b (07:30 UTC on): full gate run against the systemd service on :8140; chat drive PASS (two live turns, engine cards rendered, plain-text replies).
- Phase 5: artifact dist/index.html built (315 KB, 248 pens inlined); repo committed; droplet deploy attempted via scripts/push-droplet.sh.
