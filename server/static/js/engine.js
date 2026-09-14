/* JavaScript mirror of server/src/engine.rs for the claude.ai artifact (sample mode). Same fixtures
   (server/tests/fixtures/engine-cases.json) run against both. Constants carry the same sources. */
(function (root) {
  "use strict";
  var GUARANTEE_RETURN_DAYS = 14, REPAIR_PROMISE_DAYS = 30, REPAIR_DISCOUNT_MIN_PENS = 5;
  var TRADING_POST_LISTING_CENTS = 500, UNLIMITED_POSTS_YEAR_CENTS = 12500;
  var REPAIR_SCOPE = ["Button Fillers", "Lever Fillers", "Parker Vacumatics", "Sheaffer Snorkels", "Sheaffer Touchdowns", "Crescent Fillers", "Aerometrics"];
  var REPAIR_EXCLUSION = "we are no longer restoring European, Nozac and Sheaffer piston fillers";
  var SHIPPING_STEPS = ["Make sure the pen is empty.", "Wrap it securely in bubble wrap.", "Place the wrapped pen in a piece of PVC tubing for added protection.", "Ship it in a padded, hard cardboard box."];
  var SELL_EXCLUSION_LINE = "The only things we are not looking for are cheap advertising pens, Cross Century pens and falling apart third-tier vintage pens. (We're looking at you Wearever.)";
  var GRAIL_MIN_CENTS = 149999, FIRST_PEN_MAX_CENTS = 15000;
  var GUARANTEE_TERMS = "If you are not satisfied with your purchase for any reason, return it in the same condition it arrived via insured U.S. Mail within 14 days of receiving it, and you will be refunded the full price of the item, not including the shipping. If you discover any problems we might have missed within the first 30 days of receiving your pen, let us know and we'll do what we can to fix it.";
  var NIB_FACTS = [
    { slug: "extra-fine", name: "Extra-Fine", line_mm: "about 0.3", note: "Western extra-fine; a Japanese fine runs about this width.", source: "https://www.galenleather.com/blogs/news/fountain-pen-nib-sizes-guide" },
    { slug: "fine", name: "Fine", line_mm: "0.4 to 0.5", note: "The everyday width; his descriptions sometimes call a generous fine a medium.", source: "https://www.galenleather.com/blogs/news/fountain-pen-nib-sizes-guide" },
    { slug: "medium", name: "Medium", line_mm: "0.6 to 0.7", note: "The all-rounder.", source: "https://www.galenleather.com/blogs/news/fountain-pen-nib-sizes-guide" },
    { slug: "broad1", name: "Broad", line_mm: "0.8 to 1.0", note: "Wet and bold; shows shading inks.", source: "https://nibhaven.com/fountain-pen-nib-sizes/" },
    { slug: "bb1", name: "BB", line_mm: "1.0 to 1.2", note: "Double broad.", source: "https://nibhaven.com/fountain-pen-nib-sizes/" },
    { slug: "stub", name: "Stub", line_mm: "1.1 to 1.9", note: "A stub is rounded, an italic is crisp; both give thick downstrokes and thin cross strokes.", source: "https://nibhaven.com/fountain-pen-nib-sizes/" },
    { slug: "oblique", name: "Oblique", line_mm: "varies", note: "Tip cut at an angle; Sheaffer marked left and right obliques with an L or R suffix.", source: "backup post: a-nib-by-any-other-numberis-still-confusing" },
    { slug: "accountant", name: "Accountant", line_mm: "thinner than extra-fine", note: "Sheaffer's A point, razor thin, made for ledgers.", source: "backup post: a-nib-by-any-other-numberis-still-confusing" },
    { slug: "semi-flexible", name: "Semi-Flexible", line_mm: "widens with moderate pressure", note: "Line variation under pressure without a wet-noodle feel; he codes these SF.", source: "backup post: search-by-nib-size" },
    { slug: "flexible", name: "Flexible", line_mm: "hairline to swell with light pressure", note: "The vintage magic; the widest variation of any grade here.", source: "backup post: search-by-nib-size" }
  ];

  function fmtCents(c) { var d = Math.floor(c / 100), r = c % 100; return "$" + d.toString().replace(/\B(?=(\d{3})+(?!\d))/g, ",") + "." + (r < 10 ? "0" : "") + r; }
  function fmtDollars(c) { return fmtCents(c).replace(/\.00$/, ""); }
  function norm(s) { return String(s || "").toLowerCase().replace(/[^a-z0-9]/g, ""); }
  function eff(p) { return p.sale_price_cents != null ? p.sale_price_cents : p.price_cents; }
  function isLive(p) { return p.status === "live"; }
  function isWriting(p) { return p.category_slug === "vintage-pens" || p.category_slug === "pre-owned-pens"; }
  function isFountain(p) { return isWriting(p) && ["ballpoint", "rollerball", "pencil", "dip"].indexOf(p.mechanism_slug) < 0; }
  function url(p) { return "/product/" + p.slug + "/"; }

  function canonCategory(s) { var n = norm(s); if (!n) return null; if (n.indexOf("vintage") >= 0) return "vintage-pens"; if (n.indexOf("preowned") >= 0 || n.indexOf("modern") >= 0 || n.indexOf("luxury") >= 0) return "pre-owned-pens"; if (n.indexOf("pencil") >= 0) return "pencils"; if (n.indexOf("inkwell") >= 0 || n.indexOf("blotter") >= 0) return "inkwells-blotters"; if (n.indexOf("camera") >= 0) return "camera"; return null; }
  function canonNib(s) {
    var n = norm(s); if (!n) return [];
    if (n.indexOf("semi") >= 0) return ["semi-flexible"];
    if (n.indexOf("flex") >= 0 || n.indexOf("noodle") >= 0) return ["flexible", "semi-flexible"];
    if (n.indexOf("extrafine") >= 0 || n === "ef" || n === "xf") return ["extra-fine"];
    if (n.indexOf("doublebroad") >= 0 || n === "bb") return ["bb1"];
    if (n.indexOf("broad") >= 0 || n === "b") return ["broad1"];
    if (n.indexOf("fine") >= 0 || n === "f") return ["fine"];
    if (n.indexOf("medium") >= 0 || n === "m") return ["medium"];
    if (n.indexOf("stub") >= 0 || n.indexOf("italic") >= 0) return ["stub"];
    if (n.indexOf("oblique") >= 0) return ["oblique"];
    if (n.indexOf("accountant") >= 0) return ["accountant"];
    return [];
  }
  var MECH = [["vacumatic", "vacumatic"], ["lever", "lever-filler"], ["button", "button-filler"], ["snorkel", "snorkel"], ["touchdown", "touchdown"], ["aerometric", "aerometric-filler"], ["crescent", "crescent-filler"], ["cartridge", "cartridge-converter"], ["converter", "cartridge-converter"], ["eyedropper", "eyedropper"], ["safety", "safety"], ["capillary", "capillary"], ["vacuumfil", "vacuum-fil"], ["plunger", "vacuum-fil"], ["bulb", "bulb-postal-filler"], ["postal", "bulb-postal-filler"], ["piston", "piston"], ["ballpoint", "ballpoint"], ["rollerball", "rollerball"], ["pencil", "pencil"], ["dip", "dip"], ["syringe", "syringe"]];
  function canonMechanism(s) { var n = norm(s); if (!n) return null; for (var i = 0; i < MECH.length; i++) if (n.indexOf(MECH[i][0]) >= 0) return MECH[i][1]; return null; }
  function canonEra(s) {
    var n = norm(s); if (!n) return null;
    if (n.indexOf("pre1900") >= 0 || n.indexOf("1800") >= 0 || n.indexOf("victorian") >= 0) return "01-pre-1900";
    if (n.indexOf("1900") >= 0 || n.indexOf("1910") >= 0) return "02-1900-1919";
    if (n.indexOf("1920") >= 0 || n === "20s" || n.indexOf("twenties") >= 0) return "03-1920-1929";
    if (n.indexOf("1930") >= 0 || n === "30s" || n.indexOf("thirties") >= 0) return "04-1930-1939";
    if (n.indexOf("1940") >= 0 || n === "40s" || n.indexOf("forties") >= 0) return "05-1940-1949";
    if (n.indexOf("1950") >= 0 || n === "50s" || n.indexOf("fifties") >= 0) return "06-1950-1959";
    if (n.indexOf("1960") >= 0 || n.indexOf("1970") >= 0 || n === "60s" || n === "70s") return "07-1960-1979";
    if (n.indexOf("1980") >= 0 || n.indexOf("1990") >= 0 || n.indexOf("2000") >= 0 || n.indexOf("present") >= 0 || n.indexOf("modern") >= 0) return "08-1980-present";
    return null;
  }
  var STOP = ["pen", "pens", "the", "and", "for", "with", "under", "vintage", "fountain"];

  function findPens(catalog, c) {
    c = c || {};
    var cat = canonCategory(c.category), brand = norm(c.brand) || null, era = canonEra(c.era);
    var nibs = c.nib ? canonNib(c.nib) : [];
    if (c.flex === true && !nibs.length) nibs = ["flexible", "semi-flexible"];
    var mech = canonMechanism(c.mechanism);
    var maxC = c.max_price != null ? Math.round(c.max_price * 100) : null, minC = c.min_price != null ? Math.round(c.min_price * 100) : null;
    var keywords = String(c.keywords || "").split(/[^A-Za-z0-9]+/).map(norm).filter(function (w) { return w.length >= 3 && STOP.indexOf(w) < 0; });
    var includeSold = !!c.include_sold, all = [];
    catalog.pens.forEach(function (p) {
      if (!includeSold && !isLive(p)) return;
      if (p.category_slug === "memberships") return;
      if (cat && p.category_slug !== cat) return;
      if (brand) { var pb = norm(p.brand), pt = norm(p.title); if (!(pb.indexOf(brand) >= 0 || (pb && brand.indexOf(pb) >= 0) || pt.indexOf(brand) >= 0)) return; }
      if (era && p.era_slug !== era) return;
      if (nibs.length && nibs.indexOf(p.nib_slug) < 0) return;
      if (mech && p.mechanism_slug !== mech) return;
      if (!cat && (nibs.length || c.flex === true) && !isFountain(p)) return;
      var price = eff(p);
      if (maxC != null && price > maxC) return;
      if (minC != null && price < minC) return;
      var hay = norm(p.title + " " + p.brand + " " + p.era + " " + p.nib + " " + p.mechanism + " " + p.summary);
      var hits = keywords.filter(function (k) { return hay.indexOf(k) >= 0; }).length;
      if (keywords.length && !hits) return;
      var score = hits * 3, reasons = [], facts = [];
      if (p.mechanism) facts.push(p.mechanism); if (p.era) facts.push(p.era); if (p.nib) facts.push(p.nib + " nib");
      if (facts.length) reasons.push(facts.join(", "));
      if (p.sale_price_cents != null) { score += 1; reasons.push("On sale: " + fmtCents(p.sale_price_cents) + " (was " + fmtCents(p.price_cents) + ")"); }
      if (maxC != null) { var room = maxC - price; if (room >= Math.floor(maxC / 5)) score += 1; reasons.push(fmtCents(room) + " under your " + fmtDollars(maxC) + " limit"); }
      if (p.length_mm != null) reasons.push((p.length_mm / 10).toFixed(1) + " cm capped");
      if (nibs.indexOf(p.nib_slug) >= 0) score += 2;
      if (p.listed_year != null && p.listed_year >= 2026) score += 0.5;
      all.push({ sku: p.sku, title: p.title, short_title: p.short_title, url: url(p), image: p.image, price: fmtCents(price), price_cents: price, was_price: p.sale_price_cents != null ? fmtCents(p.price_cents) : null, brand: p.brand, era: p.era, nib: p.nib, mechanism: p.mechanism, length_mm: p.length_mm, status: p.status, reasons: reasons, summary: p.summary, score: score });
    });
    all.sort(function (a, b) { return b.score - a.score || a.price_cents - b.price_cents; });
    var total = all.length, limit = Math.min(12, Math.max(1, c.limit || 5)), note;
    if (!total) {
      var alt = maxC != null ? findPens(catalog, Object.assign({}, c, { max_price: null, min_price: null })) : { matches: [] };
      var cheapest = alt.matches.slice().sort(function (a, b) { return a.price_cents - b.price_cents; })[0];
      note = cheapest ? "Nothing in the catalog fits that budget. The least expensive pen that fits the rest is " + cheapest.short_title + " at " + cheapest.price + " (SKU " + cheapest.sku + ")." : "Nothing in the catalog matches all of that. Loosen one filter (era, nib, filling system or brand) and try again.";
    } else {
      var live = catalog.pens.filter(function (p) { return isLive(p) && p.category_slug !== "memberships"; }).length;
      note = total + " of " + live + " live items match; showing " + Math.min(total, limit) + ".";
    }
    var shown = all.slice(0, limit).map(function (m) { var o = Object.assign({}, m); delete o.score; return o; });
    return { total_matching: total, shown: shown.length, note: note, matches: shown };
  }

  function penDetails(catalog, key) {
    var k = norm(key); if (!k) return null;
    var pens = catalog.pens;
    return pens.filter(function (p) { return norm(p.sku) === k || norm(p.slug) === k; })[0]
      || pens.filter(function (p) { return norm(p.short_title) === k; })[0]
      || pens.filter(function (p) { return isLive(p) && norm(p.title).indexOf(k) >= 0; })[0]
      || pens.filter(function (p) { return norm(p.title).indexOf(k) >= 0; })[0] || null;
  }

  var OUT = [["nozac", "Nozac"], ["sheafferpiston", "Sheaffer piston filler"], ["pfm", "Sheaffer PFM"], ["european", "European piston filler"], ["montblanc", "Montblanc piston filler"], ["pelikan", "Pelikan piston filler"], ["piston", "piston filler"], ["vacuumfil", "Sheaffer Vacuum-Fil"], ["plunger", "plunger filler"]];
  var INN = [["vacumatic", "Parker Vacumatic"], ["snorkel", "Sheaffer Snorkel"], ["touchdown", "Sheaffer TouchDown"], ["crescent", "crescent filler"], ["aerometric", "aerometric"], ["lever", "lever filler"], ["button", "button filler"], ["duofold", "Parker Duofold (button filler)"], ["parker51", "Parker 51"], ["51", "Parker 51 (Vacumatic or aerometric)"], ["esterbrook", "Esterbrook (lever filler)"], ["balance", "Sheaffer Balance (lever filler)"]];
  function repairScope(text) {
    var n = norm(text), i;
    var base = { scope: REPAIR_SCOPE, exclusion: REPAIR_EXCLUSION, discount_note: "Discounts are available for shipments of " + REPAIR_DISCOUNT_MIN_PENS + " or more pens.", shipping_steps: SHIPPING_STEPS, next_step: "Send a free repair estimate request from the Pen Repairs page with a photo of the pen; the estimate comes from Nathaniel, not from this guide." };
    for (i = 0; i < OUT.length; i++) if (n.indexOf(OUT[i][0]) >= 0) return Object.assign({ verdict: "out_of_scope", matched: OUT[i][1], message: OUT[i][1] + ": the repairs page says " + REPAIR_EXCLUSION + "." }, base);
    for (i = 0; i < INN.length; i++) if (n.indexOf(INN[i][0]) >= 0) return Object.assign({ verdict: "in_scope", matched: INN[i][1], message: INN[i][1] + ": on the published repair list (" + REPAIR_SCOPE.join(", ") + ")." }, base);
    return Object.assign({ verdict: "ask", matched: "", message: "Could not tell the filling system from that. The published list is " + REPAIR_SCOPE.join(", ") + "; " + REPAIR_EXCLUSION + "." }, base);
  }

  var EXCL = [["advertising", "cheap advertising pens"], ["promotional", "cheap advertising pens"], ["crosscentury", "Cross Century pens"], ["wearever", "falling apart third-tier vintage pens (Wearever)"], ["thirdtier", "falling apart third-tier vintage pens"]];
  function sellTriage(text) {
    var n = norm(text), next = "Use the Sell My Pens form with a photo; Nathaniel answers with the options that fit what you have.";
    for (var i = 0; i < EXCL.length; i++) if (n.indexOf(EXCL[i][0]) >= 0) return { verdict: "not_looking", matched_exclusion: EXCL[i][1], message: EXCL[i][1] + " are on the list he is not looking for: \"" + SELL_EXCLUSION_LINE + "\"", options: ["cash", "consignment"], exclusion_line: SELL_EXCLUSION_LINE, next_step: next };
    if (n.length < 4) return { verdict: "ask", matched_exclusion: null, message: "Need to know what the pens are (maker, model, how many, condition) to say more.", options: ["cash", "consignment"], exclusion_line: SELL_EXCLUSION_LINE, next_step: next };
    return { verdict: "interested", matched_exclusion: null, message: "\"We are usually looking for more vintage pens and pre-owned luxury pens to restore and sell on our site. Depending upon what you have, we can offer several options from cash to consignment.\"", options: ["cash", "consignment"], exclusion_line: SELL_EXCLUSION_LINE, next_step: next };
  }

  function addDays(iso, n) { var d = new Date(iso + "T00:00:00Z"); d.setUTCDate(d.getUTCDate() + n); return d.toISOString().slice(0, 10); }
  function guaranteeDates(receivedOn) { return { received_on: receivedOn, return_by: addDays(receivedOn, GUARANTEE_RETURN_DAYS), fix_by: addDays(receivedOn, REPAIR_PROMISE_DAYS), return_days: GUARANTEE_RETURN_DAYS, fix_days: REPAIR_PROMISE_DAYS, terms: GUARANTEE_TERMS }; }
  function nibFact(name) { var slugs = canonNib(name); for (var i = 0; i < slugs.length; i++) { var f = NIB_FACTS.filter(function (x) { return x.slug === slugs[i]; })[0]; if (f) return f; } return null; }

  function stats(catalog) {
    var live = catalog.pens.filter(function (p) { return isLive(p) && p.category_slug !== "memberships"; });
    var byCat = [], order = ["vintage-pens", "pre-owned-pens", "pencils", "inkwells-blotters", "camera"];
    live.forEach(function (p) { var c = byCat.filter(function (x) { return x.slug === p.category_slug; })[0]; if (c) c.count++; else byCat.push({ name: p.category, slug: p.category_slug, count: 1 }); });
    byCat.sort(function (a, b) { return (order.indexOf(a.slug) + 1 || 99) - (order.indexOf(b.slug) + 1 || 99); });
    var grails = live.filter(function (p) { return eff(p) >= GRAIL_MIN_CENTS; }), parkers = live.filter(function (p) { return p.brand_slug === "parker" && p.category_slug === "vintage-pens"; });
    var prices = live.map(eff).sort(function (a, b) { return a - b; });
    var newest = live.reduce(function (m, p) { if (p.listed_year == null || p.listed_month == null) return m; var v = p.listed_year * 100 + p.listed_month; return v > m ? v : m; }, 0);
    var brands = {}; live.forEach(function (p) { if (p.brand_slug) brands[p.brand_slug] = 1; });
    var min = function (a) { return a.length ? Math.min.apply(null, a) : 0; }, max = function (a) { return a.length ? Math.max.apply(null, a) : 0; };
    return { live_total: live.length, sold: catalog.pens.filter(function (p) { return p.status === "sold"; }).length, on_sale: live.filter(function (p) { return p.sale_price_cents != null; }).length, by_category: byCat,
      first_pen_count: live.filter(function (p) { return isFountain(p) && eff(p) <= FIRST_PEN_MAX_CENTS; }).length,
      grail_count: grails.length, grail_min_cents: min(grails.map(eff)), grail_max_cents: max(grails.map(eff)),
      vintage_parker_count: parkers.length, vintage_parker_min_cents: min(parkers.map(eff)), vintage_parker_max_cents: max(parkers.map(eff)),
      celluloid_count: live.filter(function (p) { return p.celluloid || norm(p.summary).indexOf("celluloid") >= 0 || norm(p.title).indexOf("celluloid") >= 0; }).length,
      price_min_cents: prices[0] || 0, price_max_cents: prices[prices.length - 1] || 0, price_median_cents: prices.length ? prices[Math.floor(prices.length / 2)] : 0,
      newest_year: Math.floor(newest / 100), newest_month: newest % 100, added_in_newest_month: live.filter(function (p) { return p.listed_year * 100 + p.listed_month === newest; }).length, brands_with_stock: Object.keys(brands).length };
  }

  var api = { findPens: findPens, penDetails: penDetails, repairScope: repairScope, sellTriage: sellTriage, guaranteeDates: guaranteeDates, nibFact: nibFact, stats: stats, fmtCents: fmtCents, fmtDollars: fmtDollars, NIB_FACTS: NIB_FACTS, REPAIR_SCOPE: REPAIR_SCOPE, GUARANTEE_TERMS: GUARANTEE_TERMS, TRADING_POST_LISTING_CENTS: TRADING_POST_LISTING_CENTS, UNLIMITED_POSTS_YEAR_CENTS: UNLIMITED_POSTS_YEAR_CENTS, GRAIL_MIN_CENTS: GRAIL_MIN_CENTS, FIRST_PEN_MAX_CENTS: FIRST_PEN_MAX_CENTS };
  root.PEN_ENGINE = api;
  if (typeof module !== "undefined" && module.exports) module.exports = api;
})(typeof window !== "undefined" ? window : globalThis);
