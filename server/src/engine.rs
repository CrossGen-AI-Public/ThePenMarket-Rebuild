//! The pen guide's engine. Pure functions over a catalog snapshot; every number the assistant
//! shows comes from here, never from the model. Constants carry the date and source they were
//! taken from. A JavaScript mirror (`site/engine.js`) runs the same fixtures for the artifact.

use chrono::{Days, NaiveDate};
use serde::{Deserialize, Serialize};

// ---------- published policies (verbatim sources, crawled 2026-08-17, backup 2026-08-24) ----------
/// "return it in the same condition it arrived via insured U.S. Mail within 14 days of receiving it" (/guarantee/)
pub const GUARANTEE_RETURN_DAYS: u64 = 14;
/// "If you discover any problems we might have missed within the first 30 days of receiving your pen" (/guarantee/)
pub const REPAIR_PROMISE_DAYS: u64 = 30;
/// "Discounts are available for shipments of 5 or more pens." (/pen-repairs/)
pub const REPAIR_DISCOUNT_MIN_PENS: u32 = 5;
/// "It only costs $5 per listing" (home, Trading Post tile)
pub const TRADING_POST_LISTING_CENTS: i64 = 500;
/// "Unlimited Posts" subscription, $125 per year (products.csv, Memberships)
pub const UNLIMITED_POSTS_YEAR_CENTS: i64 = 12_500;
/// "What Can We Repair" list, verbatim (/pen-repairs/)
pub const REPAIR_SCOPE: &[&str] = &["Button Fillers", "Lever Fillers", "Parker Vacumatics", "Sheaffer Snorkels", "Sheaffer Touchdowns", "Crescent Fillers", "Aerometrics"];
/// Verbatim (/pen-repairs/)
pub const REPAIR_EXCLUSION: &str = "we are no longer restoring European, Nozac and Sheaffer piston fillers";
/// "Safe Shipping Guidelines", verbatim (/pen-repairs/)
pub const SHIPPING_STEPS: &[&str] = &[
    "Make sure the pen is empty.",
    "Wrap it securely in bubble wrap.",
    "Place the wrapped pen in a piece of PVC tubing for added protection.",
    "Ship it in a padded, hard cardboard box.",
];
/// Verbatim (/sell-my-pens/)
pub const SELL_EXCLUSIONS: &[&str] = &["cheap advertising pens", "Cross Century pens", "falling apart third-tier vintage pens"];
pub const SELL_EXCLUSION_LINE: &str = "The only things we are not looking for are cheap advertising pens, Cross Century pens and falling apart third-tier vintage pens. (We're looking at you Wearever.)";
/// Design canvas thresholds (research/design-canvas/Main.dc.html), recomputed against the live catalog.
pub const GRAIL_MIN_CENTS: i64 = 149_999;
pub const FIRST_PEN_MAX_CENTS: i64 = 15_000;

/// Nib line widths, Western grades, from galenleather.com and nibhaven.com nib-size guides (read 2026-09-13,
/// research/domain-facts.md §1). Labels are not standardised; vintage nibs vary by maker.
#[derive(Clone, Copy, Serialize)]
pub struct NibFact {
    pub slug: &'static str,
    pub name: &'static str,
    pub line_mm: &'static str,
    pub note: &'static str,
    pub source: &'static str,
}
pub const NIB_FACTS: &[NibFact] = &[
    NibFact { slug: "extra-fine", name: "Extra-Fine", line_mm: "about 0.3", note: "Western extra-fine; a Japanese fine runs about this width.", source: "https://www.galenleather.com/blogs/news/fountain-pen-nib-sizes-guide" },
    NibFact { slug: "fine", name: "Fine", line_mm: "0.4 to 0.5", note: "The everyday width; his descriptions sometimes call a generous fine a medium.", source: "https://www.galenleather.com/blogs/news/fountain-pen-nib-sizes-guide" },
    NibFact { slug: "medium", name: "Medium", line_mm: "0.6 to 0.7", note: "The all-rounder.", source: "https://www.galenleather.com/blogs/news/fountain-pen-nib-sizes-guide" },
    NibFact { slug: "broad1", name: "Broad", line_mm: "0.8 to 1.0", note: "Wet and bold; shows shading inks.", source: "https://nibhaven.com/fountain-pen-nib-sizes/" },
    NibFact { slug: "bb1", name: "BB", line_mm: "1.0 to 1.2", note: "Double broad.", source: "https://nibhaven.com/fountain-pen-nib-sizes/" },
    NibFact { slug: "stub", name: "Stub", line_mm: "1.1 to 1.9", note: "A stub is rounded, an italic is crisp; both give thick downstrokes and thin cross strokes.", source: "https://nibhaven.com/fountain-pen-nib-sizes/" },
    NibFact { slug: "oblique", name: "Oblique", line_mm: "varies", note: "Tip cut at an angle; Sheaffer marked left and right obliques with an L or R suffix.", source: "backup post: a-nib-by-any-other-numberis-still-confusing" },
    NibFact { slug: "accountant", name: "Accountant", line_mm: "thinner than extra-fine", note: "Sheaffer's A point, razor thin, made for ledgers.", source: "backup post: a-nib-by-any-other-numberis-still-confusing" },
    NibFact { slug: "semi-flexible", name: "Semi-Flexible", line_mm: "widens with moderate pressure", note: "Line variation under pressure without a wet-noodle feel; he codes these SF.", source: "backup post: search-by-nib-size" },
    NibFact { slug: "flexible", name: "Flexible", line_mm: "hairline to swell with light pressure", note: "The vintage magic; the widest variation of any grade here.", source: "backup post: search-by-nib-size" },
];

// ---------- catalog snapshot ----------
#[derive(Clone, Debug, Serialize, Deserialize, Default)]
pub struct Pen {
    pub sku: String,
    pub slug: String,
    pub title: String,
    pub short_title: String,
    pub category: String,
    pub category_slug: String,
    pub brand: String,
    pub brand_slug: String,
    pub era: String,
    pub era_slug: String,
    pub nib: String,
    pub nib_slug: String,
    pub mechanism: String,
    pub mechanism_slug: String,
    pub repairable: bool,
    pub price_cents: i64,
    pub sale_price_cents: Option<i64>,
    pub status: String,
    pub length_mm: Option<f64>,
    pub listed_year: Option<i32>,
    pub listed_month: Option<i32>,
    pub image: String,
    pub summary: String,
    #[serde(default)]
    pub celluloid: bool,
}

impl Pen {
    pub fn effective_cents(&self) -> i64 {
        self.sale_price_cents.unwrap_or(self.price_cents)
    }
    pub fn is_live(&self) -> bool {
        self.status == "live"
    }
    pub fn url(&self) -> String {
        format!("/product/{}/", self.slug)
    }
    fn is_writing_instrument(&self) -> bool {
        matches!(self.category_slug.as_str(), "vintage-pens" | "pre-owned-pens")
    }
    fn is_fountain_pen(&self) -> bool {
        self.is_writing_instrument() && !matches!(self.mechanism_slug.as_str(), "ballpoint" | "rollerball" | "pencil" | "dip")
    }
}

#[derive(Clone, Debug, Serialize, Deserialize, Default)]
pub struct Catalog {
    pub generated_at: String,
    pub pens: Vec<Pen>,
}

fn norm(s: &str) -> String {
    s.to_lowercase().chars().filter(|c| c.is_ascii_alphanumeric()).collect()
}

fn canon_category(s: &str) -> Option<&'static str> {
    let n = norm(s);
    if n.is_empty() { return None; }
    if n.contains("vintage") { return Some("vintage-pens"); }
    if n.contains("preowned") || n.contains("modern") || n.contains("luxury") { return Some("pre-owned-pens"); }
    if n.contains("pencil") { return Some("pencils"); }
    if n.contains("inkwell") || n.contains("blotter") { return Some("inkwells-blotters"); }
    if n.contains("camera") { return Some("camera"); }
    None
}

fn canon_nib(s: &str) -> Vec<&'static str> {
    let n = norm(s);
    if n.is_empty() { return vec![]; }
    if n.contains("semi") { return vec!["semi-flexible"]; }
    if n.contains("flex") || n.contains("noodle") { return vec!["flexible", "semi-flexible"]; }
    if n.contains("extrafine") || n == "ef" || n == "xf" { return vec!["extra-fine"]; }
    if n.contains("doublebroad") || n == "bb" { return vec!["bb1"]; }
    if n.contains("broad") || n == "b" { return vec!["broad1"]; }
    if n.contains("fine") || n == "f" { return vec!["fine"]; }
    if n.contains("medium") || n == "m" { return vec!["medium"]; }
    if n.contains("stub") || n.contains("italic") { return vec!["stub"]; }
    if n.contains("oblique") { return vec!["oblique"]; }
    if n.contains("accountant") { return vec!["accountant"]; }
    vec![]
}

fn canon_mechanism(s: &str) -> Option<&'static str> {
    let n = norm(s);
    if n.is_empty() { return None; }
    let table: &[(&str, &str)] = &[
        ("vacumatic", "vacumatic"), ("lever", "lever-filler"), ("button", "button-filler"), ("snorkel", "snorkel"),
        ("touchdown", "touchdown"), ("aerometric", "aerometric-filler"), ("crescent", "crescent-filler"),
        ("cartridge", "cartridge-converter"), ("converter", "cartridge-converter"), ("eyedropper", "eyedropper"),
        ("safety", "safety"), ("capillary", "capillary"), ("vacuumfil", "vacuum-fil"), ("plunger", "vacuum-fil"),
        ("bulb", "bulb-postal-filler"), ("postal", "bulb-postal-filler"), ("piston", "piston"), ("ballpoint", "ballpoint"),
        ("rollerball", "rollerball"), ("pencil", "pencil"), ("dip", "dip"), ("syringe", "syringe"),
    ];
    table.iter().find(|(k, _)| n.contains(k)).map(|(_, v)| *v)
}

fn canon_era(s: &str) -> Option<&'static str> {
    let n = norm(s);
    if n.is_empty() { return None; }
    if n.contains("pre1900") || n.contains("1800") || n.contains("victorian") { return Some("01-pre-1900"); }
    if n.contains("1900") || n.contains("1910") { return Some("02-1900-1919"); }
    if n.contains("1920") || n == "20s" || n.contains("twenties") { return Some("03-1920-1929"); }
    if n.contains("1930") || n == "30s" || n.contains("thirties") { return Some("04-1930-1939"); }
    if n.contains("1940") || n == "40s" || n.contains("forties") { return Some("05-1940-1949"); }
    if n.contains("1950") || n == "50s" || n.contains("fifties") { return Some("06-1950-1959"); }
    if n.contains("1960") || n.contains("1970") || n == "60s" || n == "70s" { return Some("07-1960-1979"); }
    if n.contains("1980") || n.contains("1990") || n.contains("2000") || n.contains("present") || n.contains("modern") { return Some("08-1980-present"); }
    None
}

// ---------- find_pens ----------
#[derive(Clone, Debug, Deserialize, Default)]
pub struct Criteria {
    pub max_price: Option<f64>,
    pub min_price: Option<f64>,
    pub category: Option<String>,
    pub brand: Option<String>,
    pub era: Option<String>,
    pub nib: Option<String>,
    pub mechanism: Option<String>,
    pub flex: Option<bool>,
    pub keywords: Option<String>,
    pub include_sold: Option<bool>,
    pub limit: Option<usize>,
}

#[derive(Clone, Debug, Serialize)]
pub struct Match {
    pub sku: String,
    pub title: String,
    pub short_title: String,
    pub url: String,
    pub image: String,
    pub price: String,
    pub price_cents: i64,
    pub was_price: Option<String>,
    pub brand: String,
    pub era: String,
    pub nib: String,
    pub mechanism: String,
    pub length_mm: Option<f64>,
    pub status: String,
    pub reasons: Vec<String>,
    pub summary: String,
    #[serde(skip)]
    pub score: f64,
}

#[derive(Clone, Debug, Serialize)]
pub struct FindResult {
    pub total_matching: usize,
    pub shown: usize,
    pub note: String,
    pub matches: Vec<Match>,
}

pub fn find_pens(catalog: &Catalog, c: &Criteria) -> FindResult {
    let cat = c.category.as_deref().and_then(canon_category);
    let brand_n = c.brand.as_deref().map(norm).filter(|s| !s.is_empty());
    let era = c.era.as_deref().and_then(canon_era);
    let mut nibs: Vec<&str> = c.nib.as_deref().map(canon_nib).unwrap_or_default();
    if c.flex == Some(true) && nibs.is_empty() {
        nibs = vec!["flexible", "semi-flexible"];
    }
    let mech = c.mechanism.as_deref().and_then(canon_mechanism);
    let max_c = c.max_price.map(|d| (d * 100.0).round() as i64);
    let min_c = c.min_price.map(|d| (d * 100.0).round() as i64);
    let keywords: Vec<String> = c
        .keywords
        .as_deref()
        .unwrap_or("")
        .split(|ch: char| !ch.is_alphanumeric())
        .map(norm)
        .filter(|w| w.len() >= 3 && !["pen", "pens", "the", "and", "for", "with", "under", "vintage", "fountain"].contains(&w.as_str()))
        .collect();
    let include_sold = c.include_sold.unwrap_or(false);

    let mut all: Vec<Match> = vec![];
    for p in &catalog.pens {
        if !include_sold && !p.is_live() { continue; }
        if p.category_slug == "memberships" { continue; }
        if let Some(cs) = cat { if p.category_slug != cs { continue; } }
        if let Some(b) = &brand_n {
            let pb = norm(&p.brand);
            let pt = norm(&p.title);
            if !(pb.contains(b.as_str()) || b.contains(pb.as_str()) && !pb.is_empty() || pt.contains(b.as_str())) { continue; }
        }
        if let Some(e) = era { if p.era_slug != e { continue; } }
        if !nibs.is_empty() && !nibs.contains(&p.nib_slug.as_str()) { continue; }
        if let Some(m) = mech { if p.mechanism_slug != m { continue; } }
        if cat.is_none() && (!nibs.is_empty() || c.flex == Some(true)) && !p.is_fountain_pen() { continue; }
        let price = p.effective_cents();
        if let Some(mx) = max_c { if price > mx { continue; } }
        if let Some(mn) = min_c { if price < mn { continue; } }
        let hay = norm(&format!("{} {} {} {} {} {}", p.title, p.brand, p.era, p.nib, p.mechanism, p.summary));
        let kw_hits = keywords.iter().filter(|k| hay.contains(k.as_str())).count();
        if !keywords.is_empty() && kw_hits == 0 { continue; }

        let mut score = kw_hits as f64 * 3.0;
        let mut reasons: Vec<String> = vec![];
        let mut facts: Vec<String> = vec![];
        if !p.mechanism.is_empty() { facts.push(p.mechanism.clone()); }
        if !p.era.is_empty() { facts.push(p.era.clone()); }
        if !p.nib.is_empty() { facts.push(format!("{} nib", p.nib)); }
        if !facts.is_empty() { reasons.push(facts.join(", ")); }
        if let Some(sale) = p.sale_price_cents {
            score += 1.0;
            reasons.push(format!("On sale: {} (was {})", crate::money::fmt_cents(sale), crate::money::fmt_cents(p.price_cents)));
        }
        if let Some(mx) = max_c {
            let room = mx - price;
            if room >= mx / 5 { score += 1.0; }
            reasons.push(format!("{} under your {} limit", crate::money::fmt_cents(room), crate::money::fmt_dollars(mx)));
        }
        if let Some(mm) = p.length_mm { reasons.push(format!("{:.1} cm capped", mm / 10.0)); }
        if nibs.contains(&p.nib_slug.as_str()) { score += 2.0; }
        if let Some(y) = p.listed_year { if y >= 2026 { score += 0.5; } }
        all.push(Match {
            sku: p.sku.clone(),
            title: p.title.clone(),
            short_title: p.short_title.clone(),
            url: p.url(),
            image: p.image.clone(),
            price: crate::money::fmt_cents(price),
            price_cents: price,
            was_price: p.sale_price_cents.map(|_| crate::money::fmt_cents(p.price_cents)),
            brand: p.brand.clone(),
            era: p.era.clone(),
            nib: p.nib.clone(),
            mechanism: p.mechanism.clone(),
            length_mm: p.length_mm,
            status: p.status.clone(),
            reasons,
            summary: p.summary.clone(),
            score,
        });
    }
    all.sort_by(|a, b| b.score.partial_cmp(&a.score).unwrap_or(std::cmp::Ordering::Equal).then(a.price_cents.cmp(&b.price_cents)));
    let total = all.len();
    let limit = c.limit.unwrap_or(5).clamp(1, 12);
    let note = if total == 0 {
        // Nothing fits: report the cheapest pen that meets everything except the budget.
        let relaxed = Criteria { max_price: None, min_price: None, ..c.clone() };
        let alt = if max_c.is_some() { find_pens(catalog, &relaxed) } else { FindResult { total_matching: 0, shown: 0, note: String::new(), matches: vec![] } };
        if let Some(cheapest) = alt.matches.iter().min_by_key(|m| m.price_cents) {
            format!("Nothing in the catalog fits that budget. The least expensive pen that fits the rest is {} at {} (SKU {}).", cheapest.short_title, cheapest.price, cheapest.sku)
        } else {
            "Nothing in the catalog matches all of that. Loosen one filter (era, nib, filling system or brand) and try again.".to_string()
        }
    } else {
        format!("{} of {} live items match; showing {}.", total, catalog.pens.iter().filter(|p| p.is_live() && p.category_slug != "memberships").count(), total.min(limit))
    };
    let shown: Vec<Match> = all.into_iter().take(limit).collect();
    FindResult { total_matching: total, shown: shown.len(), note, matches: shown }
}

// ---------- pen_details ----------
pub fn pen_details<'a>(catalog: &'a Catalog, key: &str) -> Option<&'a Pen> {
    let k = norm(key);
    if k.is_empty() { return None; }
    catalog
        .pens
        .iter()
        .find(|p| norm(&p.sku) == k || norm(&p.slug) == k)
        .or_else(|| catalog.pens.iter().find(|p| norm(&p.short_title) == k))
        .or_else(|| catalog.pens.iter().filter(|p| p.is_live()).find(|p| norm(&p.title).contains(&k)))
        .or_else(|| catalog.pens.iter().find(|p| norm(&p.title).contains(&k)))
}

// ---------- repair_scope ----------
#[derive(Clone, Debug, Serialize)]
pub struct RepairAnswer {
    pub verdict: String,          // in_scope | out_of_scope | ask
    pub matched: String,
    pub message: String,
    pub scope: Vec<&'static str>,
    pub exclusion: &'static str,
    pub discount_note: String,
    pub shipping_steps: Vec<&'static str>,
    pub next_step: String,
}

pub fn repair_scope(text: &str) -> RepairAnswer {
    let n = norm(text);
    let out: &[(&str, &str)] = &[("nozac", "Nozac"), ("sheafferpiston", "Sheaffer piston filler"), ("pfm", "Sheaffer PFM"), ("european", "European piston filler"), ("montblanc", "Montblanc piston filler"), ("pelikan", "Pelikan piston filler"), ("piston", "piston filler"), ("vacuumfil", "Sheaffer Vacuum-Fil"), ("plunger", "plunger filler")];
    let inn: &[(&str, &str)] = &[("vacumatic", "Parker Vacumatic"), ("snorkel", "Sheaffer Snorkel"), ("touchdown", "Sheaffer TouchDown"), ("crescent", "crescent filler"), ("aerometric", "aerometric"), ("lever", "lever filler"), ("button", "button filler"), ("duofold", "Parker Duofold (button filler)"), ("parker51", "Parker 51"), ("51", "Parker 51 (Vacumatic or aerometric)"), ("esterbrook", "Esterbrook (lever filler)"), ("balance", "Sheaffer Balance (lever filler)")];
    let discount = format!("Discounts are available for shipments of {} or more pens.", REPAIR_DISCOUNT_MIN_PENS);
    let next = "Send a free repair estimate request from the Pen Repairs page with a photo of the pen; the estimate comes from Nathaniel, not from this guide.".to_string();
    if let Some((_, label)) = out.iter().find(|(k, _)| n.contains(k)) {
        return RepairAnswer { verdict: "out_of_scope".into(), matched: label.to_string(), message: format!("{label}: the repairs page says {REPAIR_EXCLUSION}."), scope: REPAIR_SCOPE.to_vec(), exclusion: REPAIR_EXCLUSION, discount_note: discount, shipping_steps: SHIPPING_STEPS.to_vec(), next_step: next };
    }
    if let Some((_, label)) = inn.iter().find(|(k, _)| n.contains(k)) {
        return RepairAnswer { verdict: "in_scope".into(), matched: label.to_string(), message: format!("{label}: on the published repair list ({}).", REPAIR_SCOPE.join(", ")), scope: REPAIR_SCOPE.to_vec(), exclusion: REPAIR_EXCLUSION, discount_note: discount, shipping_steps: SHIPPING_STEPS.to_vec(), next_step: next };
    }
    RepairAnswer { verdict: "ask".into(), matched: String::new(), message: format!("Could not tell the filling system from that. The published list is {}; {}.", REPAIR_SCOPE.join(", "), REPAIR_EXCLUSION), scope: REPAIR_SCOPE.to_vec(), exclusion: REPAIR_EXCLUSION, discount_note: discount, shipping_steps: SHIPPING_STEPS.to_vec(), next_step: next }
}

// ---------- sell_triage ----------
#[derive(Clone, Debug, Serialize)]
pub struct SellAnswer {
    pub verdict: String,          // interested | not_looking | ask
    pub matched_exclusion: Option<String>,
    pub message: String,
    pub options: Vec<&'static str>,
    pub exclusion_line: &'static str,
    pub next_step: String,
}

pub fn sell_triage(text: &str) -> SellAnswer {
    let n = norm(text);
    let excl: &[(&str, &str)] = &[("advertising", "cheap advertising pens"), ("promotional", "cheap advertising pens"), ("crosscentury", "Cross Century pens"), ("wearever", "falling apart third-tier vintage pens (Wearever)"), ("thirdtier", "falling apart third-tier vintage pens")];
    let next = "Use the Sell My Pens form with a photo; Nathaniel answers with the options that fit what you have.".to_string();
    if let Some((_, label)) = excl.iter().find(|(k, _)| n.contains(k)) {
        return SellAnswer { verdict: "not_looking".into(), matched_exclusion: Some(label.to_string()), message: format!("{label} are on the list he is not looking for: \"{SELL_EXCLUSION_LINE}\""), options: vec!["cash", "consignment"], exclusion_line: SELL_EXCLUSION_LINE, next_step: next };
    }
    if n.len() < 4 {
        return SellAnswer { verdict: "ask".into(), matched_exclusion: None, message: "Need to know what the pens are (maker, model, how many, condition) to say more.".into(), options: vec!["cash", "consignment"], exclusion_line: SELL_EXCLUSION_LINE, next_step: next };
    }
    SellAnswer { verdict: "interested".into(), matched_exclusion: None, message: "\"We are usually looking for more vintage pens and pre-owned luxury pens to restore and sell on our site. Depending upon what you have, we can offer several options from cash to consignment.\"".into(), options: vec!["cash", "consignment"], exclusion_line: SELL_EXCLUSION_LINE, next_step: next }
}

// ---------- guarantee_dates ----------
#[derive(Clone, Debug, Serialize)]
pub struct GuaranteeDates {
    pub received_on: String,
    pub return_by: String,
    pub fix_by: String,
    pub return_days: u64,
    pub fix_days: u64,
    pub terms: &'static str,
}
pub const GUARANTEE_TERMS: &str = "If you are not satisfied with your purchase for any reason, return it in the same condition it arrived via insured U.S. Mail within 14 days of receiving it, and you will be refunded the full price of the item, not including the shipping. If you discover any problems we might have missed within the first 30 days of receiving your pen, let us know and we'll do what we can to fix it.";

pub fn guarantee_dates(received_on: NaiveDate) -> GuaranteeDates {
    let ret = received_on.checked_add_days(Days::new(GUARANTEE_RETURN_DAYS)).unwrap_or(received_on);
    let fix = received_on.checked_add_days(Days::new(REPAIR_PROMISE_DAYS)).unwrap_or(received_on);
    GuaranteeDates { received_on: received_on.to_string(), return_by: ret.to_string(), fix_by: fix.to_string(), return_days: GUARANTEE_RETURN_DAYS, fix_days: REPAIR_PROMISE_DAYS, terms: GUARANTEE_TERMS }
}

pub fn nib_fact(name: &str) -> Option<&'static NibFact> {
    canon_nib(name).iter().find_map(|s| NIB_FACTS.iter().find(|f| f.slug == *s))
}

// ---------- catalog statistics for the pages ----------
#[derive(Clone, Debug, Serialize, Default)]
pub struct CategoryCount {
    pub name: String,
    pub slug: String,
    pub count: usize,
}
#[derive(Clone, Debug, Serialize, Default)]
pub struct Stats {
    pub live_total: usize,
    pub sold: usize,
    pub on_sale: usize,
    pub by_category: Vec<CategoryCount>,
    pub first_pen_count: usize,
    pub grail_count: usize,
    pub grail_min_cents: i64,
    pub grail_max_cents: i64,
    pub vintage_parker_count: usize,
    pub vintage_parker_min_cents: i64,
    pub vintage_parker_max_cents: i64,
    pub celluloid_count: usize,
    pub price_min_cents: i64,
    pub price_max_cents: i64,
    pub price_median_cents: i64,
    pub newest_year: i32,
    pub newest_month: i32,
    pub added_in_newest_month: usize,
    pub brands_with_stock: usize,
}

pub fn stats(catalog: &Catalog) -> Stats {
    let live: Vec<&Pen> = catalog.pens.iter().filter(|p| p.is_live() && p.category_slug != "memberships").collect();
    let mut by_cat: Vec<CategoryCount> = vec![];
    for p in &live {
        if let Some(c) = by_cat.iter_mut().find(|c| c.slug == p.category_slug) { c.count += 1; } else { by_cat.push(CategoryCount { name: p.category.clone(), slug: p.category_slug.clone(), count: 1 }); }
    }
    let order = ["vintage-pens", "pre-owned-pens", "pencils", "inkwells-blotters", "camera"];
    by_cat.sort_by_key(|c| order.iter().position(|o| *o == c.slug).unwrap_or(99));
    let grails: Vec<&&Pen> = live.iter().filter(|p| p.effective_cents() >= GRAIL_MIN_CENTS).collect();
    let parkers: Vec<&&Pen> = live.iter().filter(|p| p.brand_slug == "parker" && p.category_slug == "vintage-pens").collect();
    let mut prices: Vec<i64> = live.iter().map(|p| p.effective_cents()).collect();
    prices.sort_unstable();
    let median = if prices.is_empty() { 0 } else { prices[prices.len() / 2] };
    let (ny, nm) = live.iter().filter_map(|p| Some((p.listed_year?, p.listed_month?))).max().unwrap_or((0, 0));
    let mut brands: Vec<&str> = live.iter().map(|p| p.brand_slug.as_str()).filter(|b| !b.is_empty()).collect();
    brands.sort_unstable();
    brands.dedup();
    Stats {
        live_total: live.len(),
        sold: catalog.pens.iter().filter(|p| p.status == "sold").count(),
        on_sale: live.iter().filter(|p| p.sale_price_cents.is_some()).count(),
        by_category: by_cat,
        first_pen_count: live.iter().filter(|p| p.is_fountain_pen() && p.effective_cents() <= FIRST_PEN_MAX_CENTS).count(),
        grail_count: grails.len(),
        grail_min_cents: grails.iter().map(|p| p.effective_cents()).min().unwrap_or(0),
        grail_max_cents: grails.iter().map(|p| p.effective_cents()).max().unwrap_or(0),
        vintage_parker_count: parkers.len(),
        vintage_parker_min_cents: parkers.iter().map(|p| p.effective_cents()).min().unwrap_or(0),
        vintage_parker_max_cents: parkers.iter().map(|p| p.effective_cents()).max().unwrap_or(0),
        celluloid_count: live.iter().filter(|p| p.celluloid || norm(&p.summary).contains("celluloid") || norm(&p.title).contains("celluloid")).count(),
        price_min_cents: prices.first().copied().unwrap_or(0),
        price_max_cents: prices.last().copied().unwrap_or(0),
        price_median_cents: median,
        newest_year: ny,
        newest_month: nm,
        added_in_newest_month: live.iter().filter(|p| p.listed_year == Some(ny) && p.listed_month == Some(nm)).count(),
        brands_with_stock: brands.len(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn fixture() -> (Catalog, serde_json::Value) {
        let raw = include_str!("../tests/fixtures/engine-cases.json");
        let v: serde_json::Value = serde_json::from_str(raw).unwrap_or_else(|e| panic!("fixture json: {e}"));
        let pens: Vec<Pen> = serde_json::from_value(v["catalog"].clone()).unwrap_or_else(|e| panic!("fixture catalog: {e}"));
        (Catalog { generated_at: "fixture".into(), pens }, v)
    }

    #[test]
    fn fixtures_pass() {
        let (cat, v) = fixture();
        let cases = v["cases"].as_array().cloned().unwrap_or_default();
        assert!(!cases.is_empty());
        for case in cases {
            let name = case["name"].as_str().unwrap_or("?");
            let tool = case["tool"].as_str().unwrap_or("");
            let expect = &case["expect"];
            match tool {
                "find_pens" => {
                    let c: Criteria = serde_json::from_value(case["args"].clone()).unwrap_or_else(|e| panic!("{name}: args {e}"));
                    let r = find_pens(&cat, &c);
                    if let Some(t) = expect["total"].as_u64() { assert_eq!(r.total_matching as u64, t, "{name}: total"); }
                    if let Some(skus) = expect["top_skus"].as_array() {
                        let got: Vec<String> = r.matches.iter().map(|m| m.sku.clone()).collect();
                        let want: Vec<String> = skus.iter().filter_map(|s| s.as_str().map(String::from)).collect();
                        assert_eq!(got, want, "{name}: order");
                    }
                    if let Some(sub) = expect["note_contains"].as_str() { assert!(r.note.contains(sub), "{name}: note '{}' lacks '{sub}'", r.note); }
                }
                "pen_details" => {
                    let key = case["args"]["key"].as_str().unwrap_or("");
                    let p = pen_details(&cat, key);
                    assert_eq!(p.map(|p| p.sku.as_str()), expect["sku"].as_str(), "{name}");
                }
                "repair_scope" => {
                    let r = repair_scope(case["args"]["text"].as_str().unwrap_or(""));
                    assert_eq!(r.verdict, expect["verdict"].as_str().unwrap_or(""), "{name}");
                }
                "sell_triage" => {
                    let r = sell_triage(case["args"]["text"].as_str().unwrap_or(""));
                    assert_eq!(r.verdict, expect["verdict"].as_str().unwrap_or(""), "{name}");
                }
                "guarantee_dates" => {
                    let d: NaiveDate = case["args"]["received_on"].as_str().unwrap_or("").parse().unwrap_or_else(|e| panic!("{name}: date {e}"));
                    let r = guarantee_dates(d);
                    assert_eq!(r.return_by, expect["return_by"].as_str().unwrap_or(""), "{name}");
                    assert_eq!(r.fix_by, expect["fix_by"].as_str().unwrap_or(""), "{name}");
                }
                "stats" => {
                    let s = stats(&cat);
                    assert_eq!(s.live_total as u64, expect["live_total"].as_u64().unwrap_or(0), "{name}: live");
                    assert_eq!(s.grail_count as u64, expect["grail_count"].as_u64().unwrap_or(0), "{name}: grails");
                    assert_eq!(s.first_pen_count as u64, expect["first_pen_count"].as_u64().unwrap_or(0), "{name}: first pen");
                }
                "nib_fact" => {
                    let f = nib_fact(case["args"]["name"].as_str().unwrap_or(""));
                    assert_eq!(f.map(|f| f.slug), expect["slug"].as_str(), "{name}");
                }
                other => panic!("unknown tool in fixture: {other}"),
            }
        }
    }
}
