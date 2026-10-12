//! Shop, category, term (brand / era / nib / filling mechanism / price range) and product pages.
//! One template renders every archive: the facet rail is eBay's pattern, the words are his.

use crate::app::{breadcrumb_node, html, org_node, AppError, AppResult, Entity, PageMeta, State};
use crate::db::{self, FacetOption, Image, PostCard, ProductCard, ShopFilter};
use crate::engine;
use crate::money::fmt_dollars;
use crate::security::{self, CsrfCookie};
use askama::Template;
use axum::extract::{Path, Query, State as AxState};
use axum::Extension;
use serde::Deserialize;

const PER_PAGE: i64 = 24;

#[derive(Deserialize, Default, Clone)]
pub struct ShopQuery {
    pub q: Option<String>,
    pub cat: Option<String>,
    pub brand: Option<String>,
    pub era: Option<String>,
    pub nib: Option<String>,
    pub fill: Option<String>,
    pub price: Option<String>,
    pub min: Option<i64>,
    pub max: Option<i64>,
    pub sort: Option<String>,
    pub page: Option<i64>,
    pub sold: Option<String>,
}

pub struct FacetView {
    pub name: String,
    pub count: i64,
    pub selected: bool,
    pub url: String,
}
pub struct FacetGroup {
    pub key: String,
    pub label: String,
    pub options: Vec<FacetView>,
}
pub struct AppliedChip {
    pub label: String,
    pub remove_url: String,
}
pub struct PageLink {
    pub n: i64,
    pub url: String,
    pub current: bool,
}

#[derive(Template)]
#[template(path = "shop.html")]
pub struct ShopTpl {
    pub page: PageMeta,
    pub heading: String,
    pub intro: String,
    pub crumbs: Vec<(String, String)>,
    pub items: Vec<ProductCard>,
    pub total: i64,
    pub facets: Vec<FacetGroup>,
    pub applied: Vec<AppliedChip>,
    pub q: String,
    pub sort: String,
    pub base_path: String,
    pub pages: Vec<PageLink>,
    pub current_page: i64,
    pub prev_url: String,
    pub next_url: String,
    pub related_posts: Vec<PostCard>,
    pub term_kind: String,
    pub term_definition: String,
    /// A search for a word the glossary defines: (name, his short definition, entry URL).
    pub gloss: Option<(String, String, String)>,
    /// "Showing results for “Sheaffer” too." when his spelling list or a near miss changed the search.
    pub search_note: String,
    pub clear_url: String,
    pub hidden_fields: Vec<(String, String)>,
    /// This page is the Sold Pen Archive itself.
    pub archive: bool,
    /// Elsewhere: how many sold pens match the same filters, and the archive page that lists them.
    pub sold_n: i64,
    pub sold_url: String,
}

/// The archive being rendered: a plain shop, a category, or one fixed term.
#[derive(Clone, Default)]
struct Fixed {
    category: Option<String>,
    brand: Option<String>,
    era: Option<String>,
    nib: Option<String>,
    mechanism: Option<String>,
    price_range: Option<String>,
    tag: Option<String>,
    on_sale: bool,
    /// A page that works for visitors but is too thin to index (a tag with one or two items).
    noindex: bool,
    /// The Sold Pen Archive.
    sold_only: bool,
}

/// The same filters on the Sold Pen Archive: a brand page's link goes to /sold-pens/?brand=parker.
fn sold_archive_url(q: &ShopQuery, fixed: &Fixed) -> String {
    let aq = ShopQuery {
        q: q.q.clone(),
        cat: fixed.category.clone().or_else(|| q.cat.clone()),
        brand: fixed.brand.clone().or_else(|| q.brand.clone()),
        era: fixed.era.clone().or_else(|| q.era.clone()),
        nib: fixed.nib.clone().or_else(|| q.nib.clone()),
        fill: fixed.mechanism.clone().or_else(|| q.fill.clone()),
        price: fixed.price_range.clone().or_else(|| q.price.clone()),
        min: q.min,
        max: q.max,
        ..Default::default()
    };
    build_url(SOLD_ARCHIVE, &aq, &Fixed::default(), &[], None)
}

pub const SOLD_ARCHIVE: &str = "/sold-pens/";

fn build_url(base: &str, q: &ShopQuery, fixed: &Fixed, overrides: &[(&str, Option<String>)], page: Option<i64>) -> String {
    let mut params: Vec<(String, String)> = vec![];
    let mut add = |k: &str, v: Option<&String>| {
        if let Some(v) = v {
            if !v.is_empty() {
                params.push((k.to_string(), v.clone()));
            }
        }
    };
    let get = |k: &str, cur: Option<&String>| -> Option<String> {
        if let Some((_, v)) = overrides.iter().find(|(ok, _)| *ok == k) {
            v.clone()
        } else {
            cur.cloned()
        }
    };
    add("q", get("q", q.q.as_ref()).as_ref());
    if fixed.category.is_none() {
        add("cat", get("cat", q.cat.as_ref()).as_ref());
    }
    if fixed.brand.is_none() {
        add("brand", get("brand", q.brand.as_ref()).as_ref());
    }
    if fixed.era.is_none() {
        add("era", get("era", q.era.as_ref()).as_ref());
    }
    if fixed.nib.is_none() {
        add("nib", get("nib", q.nib.as_ref()).as_ref());
    }
    if fixed.mechanism.is_none() {
        add("fill", get("fill", q.fill.as_ref()).as_ref());
    }
    if fixed.price_range.is_none() {
        add("price", get("price", q.price.as_ref()).as_ref());
    }
    add("min", get("min", q.min.map(|v| v.to_string()).as_ref()).as_ref());
    add("max", get("max", q.max.map(|v| v.to_string()).as_ref()).as_ref());
    add("sort", get("sort", q.sort.as_ref()).as_ref());
    add("sold", get("sold", q.sold.as_ref()).as_ref());
    if let Some(p) = page.filter(|p| *p > 1) {
        params.push(("page".into(), p.to_string()));
    }
    if params.is_empty() {
        base.to_string()
    } else {
        let qs: Vec<String> = params.iter().map(|(k, v)| format!("{}={}", k, urlencode(v))).collect();
        format!("{base}?{}", qs.join("&"))
    }
}

fn urlencode(s: &str) -> String {
    let mut out = String::new();
    for b in s.bytes() {
        match b {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => out.push(b as char),
            b' ' => out.push('+'),
            _ => out.push_str(&format!("%{b:02X}")),
        }
    }
    out
}

fn group(key: &str, label: &str, opts: &[FacetOption], base: &str, q: &ShopQuery, fixed: &Fixed, current: Option<&String>) -> FacetGroup {
    let options = opts
        .iter()
        .map(|o| {
            let next = if current.map(|c| c == &o.slug).unwrap_or(false) { None } else { Some(o.slug.clone()) };
            FacetView { name: o.name.clone(), count: o.count, selected: o.selected, url: build_url(base, q, fixed, &[(key, next), ("page", None)], None) }
        })
        .collect();
    FacetGroup { key: key.to_string(), label: label.to_string(), options }
}

async fn render_archive(state: &State, base: &str, q: ShopQuery, fixed: Fixed, heading: String, intro: String, title: String, description: String, crumbs: Vec<(String, String)>, term_kind: &str, term_definition: String, related_posts: Vec<PostCard>) -> AppResult {
    let ranges = db::price_ranges(&state.pool).await?;
    let price_slug = fixed.price_range.clone().or_else(|| q.price.clone());
    let (mut min_c, mut max_c) = (q.min.map(|d| d * 100), q.max.map(|d| d * 100));
    if let Some(pr) = price_slug.as_ref().and_then(|s| ranges.iter().find(|r| &r.slug == s)) {
        min_c = Some(pr.min_cents);
        max_c = pr.max_cents;
    }
    let has_q = q.q.as_deref().map(|s| !s.trim().is_empty()).unwrap_or(false);
    let sort = q.sort.clone().unwrap_or_else(|| if has_q { "relevance".into() } else { "newest".into() });
    let page_n = q.page.unwrap_or(1).max(1);
    // Every spelling on his list: "Schaffer" also searches Sheaffer, "Mont Blanc" also Montblanc.
    let spelled = crate::spelling::expand(q.q.as_deref().unwrap_or(""), &state.site().spellings);
    let mut filter = ShopFilter {
        category: fixed.category.clone().or_else(|| q.cat.clone()),
        brand: fixed.brand.clone().or_else(|| q.brand.clone()),
        era: fixed.era.clone().or_else(|| q.era.clone()),
        nib: fixed.nib.clone().or_else(|| q.nib.clone()),
        mechanism: fixed.mechanism.clone().or_else(|| q.fill.clone()),
        price_range: price_slug.clone(),
        min_cents: min_c,
        max_cents: max_c,
        q: q.q.clone(),
        q_alts: spelled.variants.iter().skip(1).cloned().collect(),
        tag: fixed.tag.clone(),
        on_sale: fixed.on_sale,
        include_sold: q.sold.as_deref() == Some("1"),
        sold_only: fixed.sold_only,
        sort: sort.clone(),
        page: page_n,
        per_page: PER_PAGE,
    };
    let mut result = db::shop(&state.pool, &filter, &ranges).await?;
    let mut search_note = spelled.corrected.as_ref().map(|c| format!("Showing results for “{c}” too.")).unwrap_or_default();
    let mut gloss_word = spelled.corrected.clone().or_else(|| q.q.clone());
    // Nothing found and not on his list: a make's name one or two letters off ("Sheafer") is tried.
    if result.total == 0 && has_q && spelled.corrected.is_none() {
        let makes: Vec<String> = {
            let cat = state.catalog();
            let mut m: Vec<String> = cat.pens.iter().map(|p| p.brand.clone()).filter(|b| !b.is_empty()).collect();
            m.sort();
            m.dedup();
            m
        };
        if let Some(fix) = q.q.as_deref().and_then(|w| crate::spelling::near_miss(w, &makes)) {
            let retry = ShopFilter { q: Some(fix.clone()), q_alts: crate::spelling::expand(&fix, &state.site().spellings).variants.into_iter().skip(1).collect(), ..filter.clone() };
            let found = db::shop(&state.pool, &retry, &ranges).await?;
            if found.total > 0 {
                search_note = format!("Nothing matched “{}”. Showing results for “{fix}”.", q.q.as_deref().unwrap_or("").trim());
                gloss_word = Some(fix);
                result = found;
                filter = retry;
            }
        }
    }

    // Brand order: his pins first, then stock count (the query's order) or visitor interest.
    let brand_exp = crate::insights::variant(state, "brand-order");
    let brand_mode = match &brand_exp {
        Some(a) => a.variant.clone(),
        None => crate::settings::brand_order(&state.pool).await?,
    };
    if brand_mode == "popular" {
        let interest = crate::insights::brand_interest(&state.pool, 90).await?;
        let pinned = db::pinned_brand_slugs(&state.pool).await?;
        let (mut head, mut tail): (Vec<FacetOption>, Vec<FacetOption>) = result.facets.brands.drain(..).partition(|o| pinned.contains(&o.slug));
        tail.sort_by(|a, b| interest.get(&b.slug).unwrap_or(&0).cmp(interest.get(&a.slug).unwrap_or(&0)).then(b.count.cmp(&a.count)).then(a.name.cmp(&b.name)));
        head.append(&mut tail);
        result.facets.brands = head;
    }

    let mut facets = vec![];
    if fixed.category.is_none() {
        facets.push(group("cat", "Category", &result.facets.categories, base, &q, &fixed, filter.category.as_ref()));
    }
    if fixed.brand.is_none() {
        facets.push(group("brand", "Brand", &result.facets.brands, base, &q, &fixed, filter.brand.as_ref()));
    }
    if fixed.era.is_none() {
        facets.push(group("era", "Era", &result.facets.eras, base, &q, &fixed, filter.era.as_ref()));
    }
    if fixed.mechanism.is_none() {
        facets.push(group("fill", "Filling mechanism", &result.facets.mechanisms, base, &q, &fixed, filter.mechanism.as_ref()));
    }
    if fixed.nib.is_none() {
        facets.push(group("nib", "Nib", &result.facets.nibs, base, &q, &fixed, filter.nib.as_ref()));
    }
    if fixed.price_range.is_none() {
        facets.push(group("price", "Price", &result.facets.prices, base, &q, &fixed, price_slug.as_ref()));
    }

    let mut applied = vec![];
    let name_of = |opts: &[FacetOption], slug: &Option<String>| slug.as_ref().and_then(|s| opts.iter().find(|o| &o.slug == s).map(|o| o.name.clone())).or_else(|| slug.clone());
    if fixed.category.is_none() {
        if let Some(n) = name_of(&result.facets.categories, &q.cat) {
            applied.push(AppliedChip { label: n, remove_url: build_url(base, &q, &fixed, &[("cat", None), ("page", None)], None) });
        }
    }
    if fixed.brand.is_none() {
        if let Some(n) = name_of(&result.facets.brands, &q.brand) {
            applied.push(AppliedChip { label: n, remove_url: build_url(base, &q, &fixed, &[("brand", None)], None) });
        }
    }
    if fixed.era.is_none() {
        if let Some(n) = name_of(&result.facets.eras, &q.era) {
            applied.push(AppliedChip { label: n, remove_url: build_url(base, &q, &fixed, &[("era", None)], None) });
        }
    }
    if fixed.mechanism.is_none() {
        if let Some(n) = name_of(&result.facets.mechanisms, &q.fill) {
            applied.push(AppliedChip { label: n, remove_url: build_url(base, &q, &fixed, &[("fill", None)], None) });
        }
    }
    if fixed.nib.is_none() {
        if let Some(n) = name_of(&result.facets.nibs, &q.nib) {
            applied.push(AppliedChip { label: n, remove_url: build_url(base, &q, &fixed, &[("nib", None)], None) });
        }
    }
    if fixed.price_range.is_none() {
        if let Some(n) = name_of(&result.facets.prices, &q.price) {
            applied.push(AppliedChip { label: n, remove_url: build_url(base, &q, &fixed, &[("price", None)], None) });
        }
    }
    if let Some(m) = q.max {
        applied.push(AppliedChip { label: format!("Under ${m}"), remove_url: build_url(base, &q, &fixed, &[("max", None)], None) });
    }
    if let Some(m) = q.min {
        applied.push(AppliedChip { label: format!("Over ${m}"), remove_url: build_url(base, &q, &fixed, &[("min", None)], None) });
    }
    if let Some(s) = q.q.as_ref().filter(|s| !s.trim().is_empty()) {
        applied.push(AppliedChip { label: format!("“{}”", s.trim()), remove_url: build_url(base, &q, &fixed, &[("q", None)], None) });
    }

    let total_pages = ((result.total + PER_PAGE - 1) / PER_PAGE).max(1);
    let pages: Vec<PageLink> = (1..=total_pages).filter(|n| total_pages <= 9 || (*n - page_n).abs() <= 2 || *n == 1 || *n == total_pages).map(|n| PageLink { n, url: build_url(base, &q, &fixed, &[], Some(n)), current: n == page_n }).collect();
    let prev_url = if page_n > 1 { build_url(base, &q, &fixed, &[], Some(page_n - 1)) } else { String::new() };
    let next_url = if page_n < total_pages { build_url(base, &q, &fixed, &[], Some(page_n + 1)) } else { String::new() };

    let canonical = build_url(base, &ShopQuery { q: q.q.clone(), cat: q.cat.clone(), brand: q.brand.clone(), era: q.era.clone(), nib: q.nib.clone(), fill: q.fill.clone(), price: q.price.clone(), min: q.min, max: q.max, sort: None, page: None, sold: None }, &fixed, &[], Some(page_n));
    let mut page = PageMeta::new(state, &title, &description, &canonical);
    page.section = "shop".into();
    // Filtered views of an archive are not separate documents; only the clean archive is indexed.
    page.noindex = !applied.is_empty() || page_n > 1 || fixed.noindex;
    if let Some(p) = result.items.first() {
        if !p.image.is_empty() {
            page.og_image = state.abs(&p.image);
        }
    }
    let list: Vec<serde_json::Value> = result.items.iter().enumerate().map(|(i, p)| serde_json::json!({"@type": "ListItem", "position": i + 1 + ((page_n - 1) * PER_PAGE) as usize, "url": state.abs(&p.url), "name": p.title})).collect();
    let mut nodes = vec![
        org_node(state),
        {
            let mut cp = serde_json::json!({"@type": "CollectionPage", "@id": state.abs(&format!("{base}#collection")), "url": state.abs(base), "name": heading, "description": description, "isPartOf": {"@id": state.abs("/#website")}});
            // An empty list (nothing in a category for sale right now) is left out rather than described as empty.
            if !list.is_empty() {
                cp["mainEntity"] = serde_json::json!({"@type": "ItemList", "numberOfItems": result.total, "itemListElement": list});
            }
            cp
        },
        breadcrumb_node(state, &crumbs.iter().map(|(a, b)| (a.as_str(), b.as_str())).collect::<Vec<_>>()),
    ];
    if !term_kind.is_empty() && !term_definition.is_empty() {
        nodes.push(serde_json::json!({"@type": "DefinedTerm", "@id": state.abs(&format!("{base}#term")), "name": heading, "description": term_definition, "inDefinedTermSet": {"@type": "DefinedTermSet", "@id": state.abs("/sitemap-terms.xml#terms"), "name": format!("ThePenMarket.com {term_kind} glossary")}}));
    }
    page = page.with_jsonld(nodes);
    if fixed.brand.is_none() {
        page.experiments = crate::insights::exposure_attr(&[("brand-order", &brand_exp)]);
    }

    // Sold pens are not mixed into the lists of pens for sale; each list says how many sold ones match
    // and links to them in the archive. Tag and on-sale pages have no archive equivalent.
    let (sold_n, sold_url) = if fixed.sold_only || filter.include_sold || fixed.tag.is_some() || fixed.on_sale {
        (0, String::new())
    } else {
        (db::sold_count(&state.pool, &filter).await?, sold_archive_url(&q, &fixed))
    };

    let mut hidden_fields = vec![];
    for (k, v) in [("cat", &q.cat), ("brand", &q.brand), ("era", &q.era), ("nib", &q.nib), ("fill", &q.fill), ("price", &q.price)] {
        if let Some(v) = v {
            if !v.is_empty() {
                hidden_fields.push((k.to_string(), v.clone()));
            }
        }
    }
    if let Some(m) = q.max {
        hidden_fields.push(("max".into(), m.to_string()));
    }
    if let Some(m) = q.min {
        hidden_fields.push(("min".into(), m.to_string()));
    }

    html(&ShopTpl {
        page,
        heading,
        intro,
        crumbs,
        items: result.items,
        total: result.total,
        facets,
        applied,
        q: q.q.clone().unwrap_or_default(),
        sort,
        base_path: base.to_string(),
        pages,
        current_page: page_n,
        prev_url,
        next_url,
        related_posts,
        term_kind: term_kind.to_string(),
        term_definition,
        gloss: if page_n == 1 { gloss_word.as_deref().and_then(|w| state.site().glossary.find(w).map(|e| (e.name.clone(), e.short.clone(), e.url.clone()))) } else { None },
        search_note,
        clear_url: base.to_string(),
        hidden_fields,
        archive: fixed.sold_only,
        sold_n,
        sold_url,
    })
}

fn stats_line(cat: &engine::Catalog, pred: impl Fn(&engine::Pen) -> bool) -> (usize, i64, i64) {
    let pens: Vec<&engine::Pen> = cat.pens.iter().filter(|p| p.is_live() && pred(p)).collect();
    (pens.len(), pens.iter().map(|p| p.effective_cents()).min().unwrap_or(0), pens.iter().map(|p| p.effective_cents()).max().unwrap_or(0))
}

pub async fn shop(AxState(state): AxState<State>, Query(q): Query<ShopQuery>) -> AppResult {
    let cat = state.catalog();
    let s = engine::stats(&cat);
    let (heading, intro) = match q.q.as_deref().map(str::trim).filter(|s| !s.is_empty()) {
        Some(term) => (format!("Search: {term}"), format!("Every pen, pencil, inkwell and camera in the case that matches “{term}”. Search covers names, brands, SKUs, eras, nibs, filling systems and his descriptions.")),
        None => ("The case".into(), format!("Every one of the {} items in stock: {} vintage pens, {} pre-owned pens, pencils, inkwells and a few cameras, from {} to {}. All pens restored, tested and guaranteed; one of each.", s.live_total, s.by_category.iter().find(|c| c.slug == "vintage-pens").map(|c| c.count).unwrap_or(0), s.by_category.iter().find(|c| c.slug == "pre-owned-pens").map(|c| c.count).unwrap_or(0), fmt_dollars(s.price_min_cents), fmt_dollars(s.price_max_cents))),
    };
    let title = match q.q.as_deref().map(str::trim).filter(|s| !s.is_empty()) {
        Some(term) => format!("“{term}” in the case | ThePenMarket.com"),
        None => format!("Shop {} Restored Vintage & Pre-Owned Pens | ThePenMarket.com", s.live_total),
    };
    render_archive(&state, "/shop/", q, Fixed::default(), heading, intro.clone(), title, intro, vec![("Home".into(), "/".into()), ("Shop".into(), "/shop/".into())], "", String::new(), vec![]).await
}

pub async fn on_sale(AxState(state): AxState<State>, Query(q): Query<ShopQuery>) -> AppResult {
    let cat = state.catalog();
    let s = engine::stats(&cat);
    let intro = format!("At ThePenMarket.com we pride ourselves on our competitive pricing. Click here to find our deepest discounts and “on sale” items: {} pens and pencils are marked down right now.", s.on_sale);
    render_archive(&state, "/on-sale-pens/", q, Fixed { on_sale: true, ..Default::default() }, "Best Bargains".into(), intro.clone(), format!("On Sale: {} Vintage & Pre-Owned Pens Marked Down | ThePenMarket.com", s.on_sale), intro, vec![("Home".into(), "/".into()), ("Best Bargains".into(), "/on-sale-pens/".into())], "", String::new(), vec![]).await
}

pub async fn category(AxState(state): AxState<State>, Path(slug): Path<String>, Query(q): Query<ShopQuery>) -> AppResult {
    let cats = db::categories(&state.pool).await?;
    let c = cats.iter().find(|c| c.slug == slug).ok_or(AppError::NotFound)?;
    let cat = state.catalog();
    let (n, lo, hi) = stats_line(&cat, |p| p.category_slug == slug);
    let intro = if c.intro.is_empty() { format!("{} items in {}, {} to {}.", n, c.name, fmt_dollars(lo), fmt_dollars(hi)) } else { format!("{} {} in stock, {} to {}. {}", n, c.name.to_lowercase(), fmt_dollars(lo), fmt_dollars(hi), c.intro) };
    let title = format!("{} for Sale: {} Restored & Guaranteed | ThePenMarket.com", c.name, n);
    let desc = if c.meta_description.is_empty() { intro.clone() } else { c.meta_description.clone() };
    let posts = db::posts_matching(&state.pool, &c.name, 3).await?;
    render_archive(&state, &format!("/product-category/{slug}/"), q, Fixed { category: Some(slug.clone()), ..Default::default() }, c.name.clone(), intro, title, desc, vec![("Home".into(), "/".into()), ("Shop".into(), "/shop/".into()), (c.name.clone(), format!("/product-category/{slug}/"))], "", String::new(), posts).await
}

async fn term_page(state: State, table: &str, kind: &str, base_prefix: &str, slug: String, q: ShopQuery, pick: fn(&engine::Pen, &str) -> bool) -> AppResult {
    let t = db::term_by_slug(&state.pool, table, &slug).await?.ok_or(AppError::NotFound)?;
    let cat = state.catalog();
    let (n, lo, hi) = stats_line(&cat, |p| pick(p, &slug));
    let definition = t.definition.clone();
    let intro = if n > 0 { format!("{} {} in stock, {} to {}.", n, if n == 1 { "item" } else { "items" }, fmt_dollars(lo), fmt_dollars(hi)) } else { "Nothing in stock right now; the definition stays so the page does.".to_string() };
    let title = match kind {
        "brand" => format!("{} Pens for Sale: {} Restored & Guaranteed | ThePenMarket.com", t.name, n),
        "era" => format!("Pens from {}: {} in Stock, Restored | ThePenMarket.com", t.name, n),
        "nib" => format!("{} Nib Fountain Pens: {} in Stock | ThePenMarket.com", t.name, n),
        "filling mechanism" => format!("{} Fountain Pens: What It Is, {} in Stock | ThePenMarket.com", t.name, n),
        _ => format!("Pens {}: {} in Stock | ThePenMarket.com", t.name, n),
    };
    let desc = if definition.is_empty() { format!("{} at ThePenMarket.com: {intro}", t.name) } else { crate::text::summary(&definition, 155) };
    // A one-line definition still needs a description a search result can show: add the stock line.
    let desc = if desc.chars().count() >= 50 { desc } else { crate::text::summary(&format!("{desc} {intro}"), 155) };
    let posts = db::posts_matching(&state.pool, &t.name, 3).await?;
    let fixed = match kind {
        "brand" => Fixed { brand: Some(slug.clone()), ..Default::default() },
        "era" => Fixed { era: Some(slug.clone()), ..Default::default() },
        "nib" => Fixed { nib: Some(slug.clone()), ..Default::default() },
        "filling mechanism" => Fixed { mechanism: Some(slug.clone()), ..Default::default() },
        _ => Fixed { price_range: Some(slug.clone()), ..Default::default() },
    };
    let crumbs = vec![("Home".into(), "/".into()), ("Shop".into(), "/shop/".into()), (t.name.clone(), format!("{base_prefix}{slug}/"))];
    let mut fq = q;
    if kind == "brand" && fq.cat.is_none() {
        fq.cat = None;
    }
    render_archive(&state, &format!("{base_prefix}{slug}/"), fq, fixed, t.name.clone(), intro, title, desc, crumbs, kind, definition, posts).await
}

pub async fn brand(AxState(state): AxState<State>, Path(slug): Path<String>, Query(q): Query<ShopQuery>) -> AppResult {
    term_page(state, "brand", "brand", "/brand/", slug, q, |p, s| p.brand_slug == s).await
}
pub async fn era(AxState(state): AxState<State>, Path(slug): Path<String>, Query(q): Query<ShopQuery>) -> AppResult {
    term_page(state, "era", "era", "/era/", slug, q, |p, s| p.era_slug == s).await
}
pub async fn nib(AxState(state): AxState<State>, Path(slug): Path<String>, Query(q): Query<ShopQuery>) -> AppResult {
    term_page(state, "nib", "nib", "/nib/", slug, q, |p, s| p.nib_slug == s).await
}
pub async fn mechanism(AxState(state): AxState<State>, Path(slug): Path<String>, Query(q): Query<ShopQuery>) -> AppResult {
    term_page(state, "filling_mechanism", "filling mechanism", "/pw-filling-mechanism/", slug, q, |p, s| p.mechanism_slug == s).await
}
pub async fn price_range(AxState(state): AxState<State>, Path(slug): Path<String>, Query(q): Query<ShopQuery>) -> AppResult {
    let ranges = db::price_ranges(&state.pool).await?;
    let r = ranges.iter().find(|r| r.slug == slug).ok_or(AppError::NotFound)?;
    let (lo, hi) = (r.min_cents, r.max_cents);
    let cat = state.catalog();
    let (n, _, _) = stats_line(&cat, |p| p.effective_cents() >= lo && hi.map(|h| p.effective_cents() <= h).unwrap_or(true));
    let intro = format!("{} items priced {}. Prices are his listed prices; sale prices count.", n, r.name.replace("&amp;", "&"));
    let name = r.name.replace("&amp;", "&");
    render_archive(&state, &format!("/pw-price-range/{slug}/"), q, Fixed { price_range: Some(slug.clone()), ..Default::default() }, format!("Pens {name}"), intro.clone(), format!("Pens {name}: {n} in Stock | ThePenMarket.com"), intro, vec![("Home".into(), "/".into()), ("Shop".into(), "/shop/".into()), (name, format!("/pw-price-range/{slug}/"))], "", String::new(), vec![]).await
}

/// His own tags on the items he adds, at the old site's /product-tag/{slug}/ address. An old tag
/// address with no tag of that name today falls through to the redirect table (see routes::mod).
pub async fn tag(AxState(state): AxState<State>, Path(slug): Path<String>, Query(q): Query<ShopQuery>) -> AppResult {
    let t = db::tag_by_slug(&state.pool, &slug).await?.ok_or(AppError::NotFound)?;
    let intro = match t.live {
        0 => "Nothing with this tag is for sale right now.".to_string(),
        1 => "1 item for sale with this tag.".to_string(),
        n => format!("{n} items for sale with this tag."),
    };
    let base = t.url();
    let crumbs = vec![("Home".into(), "/".into()), ("Shop".into(), "/shop/".into()), (t.name.clone(), base.clone())];
    let fixed = Fixed { tag: Some(slug.clone()), noindex: t.live < db::TAG_INDEX_MIN, ..Default::default() };
    render_archive(&state, &base, q, fixed, t.name.clone(), intro.clone(), format!("{}: {} for Sale | ThePenMarket.com", t.name, t.live), format!("{} at ThePenMarket.com. {intro}", t.name), crumbs, "", String::new(), vec![]).await
}

/// The Sold Pen Archive (Nathaniel, 2026-10-05): every sold pen with its photos, description and last
/// asking price, most recently sold first, searchable and filterable like the shop. Sold pens leave the
/// lists of pens for sale the moment they sell and are found here instead.
pub async fn sold_pens(AxState(state): AxState<State>, Query(q): Query<ShopQuery>) -> AppResult {
    let n: i64 = sqlx::query_scalar("SELECT count(*) FROM product p JOIN category c ON c.id = p.category_id WHERE p.status = 'sold' AND c.slug <> 'memberships'").fetch_one(&state.pool).await.map_err(anyhow::Error::from)?;
    let intro = format!("{n} pens that have sold at ThePenMarket.com, kept with their photos, descriptions and last asking prices for reference. Search them by name or filter by brand, era, nib or filling system.");
    let crumbs = vec![("Home".into(), "/".into()), ("Shop".into(), "/shop/".into()), ("Sold Pen Archive".into(), SOLD_ARCHIVE.into())];
    let (heading, title) = match q.q.as_deref().map(str::trim).filter(|s| !s.is_empty()) {
        Some(term) => (format!("Sold pens: {term}"), format!("Sold “{term}” pens: past listings and prices | ThePenMarket.com")),
        None => ("Sold Pen Archive".to_string(), format!("Sold Pen Archive: {n} Past Listings and Prices | ThePenMarket.com")),
    };
    render_archive(&state, SOLD_ARCHIVE, q, Fixed { sold_only: true, ..Default::default() }, heading, intro.clone(), title, intro, crumbs, "", String::new(), vec![]).await
}

// ---------- product ----------
pub struct Spec {
    pub label: String,
    pub value: String,
    pub url: String,
    pub key: String,
    pub slug: String,
}

#[derive(Template)]
#[template(path = "product.html")]
pub struct ProductTpl {
    pub page: PageMeta,
    pub p: ProductCard,
    pub images: Vec<Image>,
    pub description_html: String,
    pub description_text: String,
    pub specs: Vec<Spec>,
    pub related: Vec<ProductCard>,
    pub related_posts: Vec<PostCard>,
    pub repairable: bool,
    pub guarantee_terms: String,
    pub return_days: u64,
    pub fix_days: u64,
    pub ask_url: String,
    pub is_membership: bool,
    pub tags: Vec<db::Tag>,
    /// The tags as he types them in the editor: "gift, Parker 51".
    pub tags_text: String,
    pub show_tags: bool,
    /// Stripe keys are set and this is a pen (not the membership) that is for sale.
    pub checkout_on: bool,
    /// "" (free), "yours" (this visitor is checking out) or "other" (someone else is).
    pub hold: String,
    pub hold_url: String,
    pub hold_minutes: i64,
    /// "Plus $12.00 shipping" once he sets a flat rate; empty until then.
    pub shipping_line: String,
}

pub async fn product(AxState(state): AxState<State>, Extension(cookie): Extension<CsrfCookie>, Path(slug): Path<String>) -> AppResult {
    let p = db::product_by_slug(&state.pool, &slug).await?.ok_or(AppError::NotFound)?;
    let editing = crate::admin::editing();
    if crate::admin::current().is_none() {
        match p.status.as_str() {
            "draft" => return Err(AppError::NotFound),
            // A deleted pen's address may be bookmarked or in Google: its category, not a dead end.
            "archived" => return Ok(crate::app::moved(&format!("/product-category/{}/", p.category_slug))),
            _ => {}
        }
    }
    let body = db::product_body(&state.pool, p.id).await?;
    let images = db::product_images(&state.pool, p.id).await?;
    let related = db::related(&state.pool, p.id, &p.brand_slug, &p.category_slug, 4).await?;
    let related_posts = if p.brand.is_empty() { vec![] } else { db::posts_matching(&state.pool, &p.brand, 2).await? };
    let cat = state.catalog();
    let repairable = cat.pens.iter().find(|x| x.slug == slug).map(|x| x.repairable).unwrap_or(false);

    let mut specs = vec![];
    if !p.mechanism.is_empty() || editing {
        specs.push(Spec { label: "Filling Mechanism".into(), value: p.mechanism.clone(), url: format!("/pw-filling-mechanism/{}/", p.mechanism_slug), key: "filling_mechanism".into(), slug: p.mechanism_slug.clone() });
    }
    if !p.era.is_empty() || editing {
        specs.push(Spec { label: "Era".into(), value: p.era.clone(), url: format!("/era/{}/", p.era_slug), key: "era".into(), slug: p.era_slug.clone() });
    }
    if !p.nib.is_empty() || editing {
        specs.push(Spec { label: "Nib Size".into(), value: p.nib.clone(), url: format!("/nib/{}/", p.nib_slug), key: "nib".into(), slug: p.nib_slug.clone() });
    }
    if !p.sku.is_empty() || editing {
        specs.push(Spec { label: "SKU".into(), value: p.sku.clone(), url: String::new(), key: "sku".into(), slug: String::new() });
    }
    if !p.length_cm.is_empty() || editing {
        specs.push(Spec { label: "Capped length".into(), value: p.length_cm.clone(), url: String::new(), key: "length_cm".into(), slug: String::new() });
    }
    if !p.brand.is_empty() || editing {
        specs.push(Spec { label: "Brand".into(), value: p.brand.clone(), url: format!("/brand/{}/", p.brand_slug), key: "brand".into(), slug: p.brand_slug.clone() });
    }
    specs.push(Spec { label: "Category".into(), value: p.category.clone(), url: format!("/product-category/{}/", p.category_slug), key: "category".into(), slug: p.category_slug.clone() });
    specs.push(Spec { label: "Condition".into(), value: if p.sold { "Sold".into() } else if p.category_slug == "vintage-pens" { "Restored, tested and guaranteed".into() } else { "Pre-owned, tested and guaranteed".into() }, url: String::new(), key: String::new(), slug: String::new() });

    let price_now = if p.on_sale { p.sale_price.clone() } else { p.price.clone() };
    let title = format!("{} | {} | ThePenMarket.com", p.title, price_now);
    let description = if body.meta_description.is_empty() { crate::text::summary(&body.description_text, 155) } else { body.meta_description.clone() };
    // A one-line listing still gets a description a search result can show: its name and price first.
    let description = if description.chars().count() >= 50 { description } else { crate::text::summary(&format!("{}, {} at ThePenMarket.com. {}", p.title, price_now, description), 155) };
    let mut page = PageMeta::new(&state, &title, &description, &format!("/product/{slug}/"));
    page.section = "shop".into();
    if let Some(i) = images.first() {
        page.og_image = state.abs(&i.large);
    }
    let mut img_urls: Vec<String> = images.iter().map(|i| state.abs(&i.full)).collect();
    if img_urls.is_empty() {
        img_urls.push(state.abs("/static/img/og-default.jpg"));
    }
    let offer = serde_json::json!({
        "@type": "Offer",
        "url": state.abs(&format!("/product/{slug}/")),
        "priceCurrency": "USD",
        "price": format!("{:.2}", p.price_cents as f64 / 100.0),
        "availability": if p.sold { "https://schema.org/SoldOut" } else { "https://schema.org/InStock" },
        "itemCondition": if p.category_slug == "vintage-pens" { "https://schema.org/RefurbishedCondition" } else { "https://schema.org/UsedCondition" },
        "seller": {"@id": state.abs("/#organization")},
        "hasMerchantReturnPolicy": {"@type": "MerchantReturnPolicy", "returnPolicyCategory": "https://schema.org/MerchantReturnFiniteReturnWindow", "merchantReturnDays": engine::GUARANTEE_RETURN_DAYS, "returnMethod": "https://schema.org/ReturnByMail", "returnFees": "https://schema.org/ReturnShippingFees", "applicableCountry": "US"}
    });
    let mut product_node = serde_json::json!({
        "@type": "Product",
        "@id": state.abs(&format!("/product/{slug}/#product")),
        "name": p.title,
        "sku": p.sku,
        "description": description,
        "image": img_urls,
        "url": state.abs(&format!("/product/{slug}/")),
        "category": p.category,
        "offers": offer
    });
    if !p.brand.is_empty() {
        product_node["brand"] = serde_json::json!({"@type": "Brand", "name": p.brand});
    }
    let mut props = vec![];
    for s in &specs {
        if s.label != "Category" && s.label != "Brand" && s.label != "SKU" {
            props.push(serde_json::json!({"@type": "PropertyValue", "name": s.label, "value": s.value}));
        }
    }
    product_node["additionalProperty"] = serde_json::json!(props);
    let tags = db::product_tags(&state.pool, p.id).await?;
    if !tags.is_empty() {
        product_node["keywords"] = serde_json::json!(tags.iter().map(|t| t.name.clone()).collect::<Vec<_>>().join(", "));
    }
    page = page.with_jsonld(vec![
        org_node(&state),
        product_node,
        breadcrumb_node(&state, &[("Home", "/"), ("Shop", "/shop/"), (p.category.as_str(), &format!("/product-category/{}/", p.category_slug)), (p.short_title.as_str(), &format!("/product/{slug}/"))]),
    ]);
    let is_membership = p.is_subscription;
    let ask_url = format!("/#ask?sku={}", urlencode(&p.sku));
    let checkout_on = state.cfg.checkout_enabled() && p.status == "live" && !is_membership;
    let (mut hold, mut hold_url, mut hold_minutes, mut shipping_line) = (String::new(), String::new(), 0, String::new());
    if checkout_on {
        page.csrf = security::csrf_token(&state.cfg.csrf_secret, &cookie.0);
        if let Some(h) = thepenmarket::orders::current_hold(&state.pool, p.id, &super::checkout::visitor_key(&cookie)).await? {
            hold = if h.mine && h.status == "open" { "yours".into() } else { "other".into() };
            hold_url = h.url;
            hold_minutes = if h.status == "open" { h.minutes_left } else { 0 };
        }
        if let Some(c) = crate::settings::shipping_flat(&state.pool).await?.filter(|c| *c > 0) {
            shipping_line = format!("Plus {} shipping.", thepenmarket::money::fmt_cents(c));
        }
    }
    html(&ProductTpl {
        page,
        p,
        images,
        // A visitor gets tap-to-define links on the glossary's words; the editor gets his text untouched.
        description_html: if editing { body.description_html } else { crate::gloss::link_terms(&body.description_html, &state.site().glossary.link_terms, crate::glossary::MAX_LINKS_PER_LISTING) },
        description_text: body.description_text,
        specs,
        related,
        related_posts,
        repairable,
        guarantee_terms: engine::GUARANTEE_TERMS.to_string(),
        return_days: engine::GUARANTEE_RETURN_DAYS,
        fix_days: engine::REPAIR_PROMISE_DAYS,
        ask_url,
        is_membership,
        tags_text: tags.iter().map(|t| t.name.clone()).collect::<Vec<_>>().join(", "),
        show_tags: !tags.is_empty() || editing,
        tags,
        checkout_on,
        hold,
        hold_url,
        hold_minutes,
        shipping_line,
    })
}

#[allow(dead_code)]
pub fn entity() -> &'static str {
    Entity::NAME
}
