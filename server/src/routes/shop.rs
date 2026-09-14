//! Shop, category, term (brand / era / nib / filling mechanism / price range) and product pages.
//! One template renders every archive: the facet rail is eBay's pattern, the words are his.

use crate::app::{breadcrumb_node, html, org_node, AppError, AppResult, Entity, PageMeta, State};
use crate::db::{self, FacetOption, Image, PostCard, ProductCard, ShopFilter};
use crate::engine;
use crate::money::fmt_dollars;
use askama::Template;
use axum::extract::{Path, Query, State as AxState};
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
    pub clear_url: String,
    pub hidden_fields: Vec<(String, String)>,
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
    on_sale: bool,
}

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
    let sort = q.sort.clone().unwrap_or_else(|| "newest".into());
    let page_n = q.page.unwrap_or(1).max(1);
    let filter = ShopFilter {
        category: fixed.category.clone().or_else(|| q.cat.clone()),
        brand: fixed.brand.clone().or_else(|| q.brand.clone()),
        era: fixed.era.clone().or_else(|| q.era.clone()),
        nib: fixed.nib.clone().or_else(|| q.nib.clone()),
        mechanism: fixed.mechanism.clone().or_else(|| q.fill.clone()),
        price_range: price_slug.clone(),
        min_cents: min_c,
        max_cents: max_c,
        q: q.q.clone(),
        on_sale: fixed.on_sale,
        include_sold: q.sold.as_deref() == Some("1"),
        sort: sort.clone(),
        page: page_n,
        per_page: PER_PAGE,
    };
    let result = db::shop(&state.pool, &filter, &ranges).await?;

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
    page.noindex = !applied.is_empty() || page_n > 1;
    if let Some(p) = result.items.first() {
        if !p.image.is_empty() {
            page.og_image = state.abs(&p.image);
        }
    }
    let list: Vec<serde_json::Value> = result.items.iter().enumerate().map(|(i, p)| serde_json::json!({"@type": "ListItem", "position": i + 1 + ((page_n - 1) * PER_PAGE) as usize, "url": state.abs(&p.url), "name": p.title})).collect();
    let mut nodes = vec![
        org_node(state),
        serde_json::json!({"@type": "CollectionPage", "@id": state.abs(&format!("{base}#collection")), "url": state.abs(base), "name": heading, "description": description, "isPartOf": {"@id": state.abs("/#website")}, "mainEntity": {"@type": "ItemList", "numberOfItems": result.total, "itemListElement": list}}),
        breadcrumb_node(state, &crumbs.iter().map(|(a, b)| (a.as_str(), b.as_str())).collect::<Vec<_>>()),
    ];
    if !term_kind.is_empty() && !term_definition.is_empty() {
        nodes.push(serde_json::json!({"@type": "DefinedTerm", "@id": state.abs(&format!("{base}#term")), "name": heading, "description": term_definition, "inDefinedTermSet": {"@type": "DefinedTermSet", "@id": state.abs("/sitemap-terms.xml#terms"), "name": format!("ThePenMarket.com {term_kind} glossary")}}));
    }
    page = page.with_jsonld(nodes);

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
        clear_url: base.to_string(),
        hidden_fields,
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
    let mut q = q;
    q.sold = Some("1".into()); // archives show sold one-offs with their chip rather than an empty grid
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
    let desc = if definition.is_empty() { intro.clone() } else { crate::text::summary(&definition, 155) };
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
}

pub async fn product(AxState(state): AxState<State>, Path(slug): Path<String>) -> AppResult {
    let p = db::product_by_slug(&state.pool, &slug).await?.ok_or(AppError::NotFound)?;
    let editing = crate::admin::editing();
    if matches!(p.status.as_str(), "draft" | "archived") && crate::admin::current().is_none() {
        return Err(AppError::NotFound);
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
    page = page.with_jsonld(vec![
        org_node(&state),
        product_node,
        breadcrumb_node(&state, &[("Home", "/"), ("Shop", "/shop/"), (p.category.as_str(), &format!("/product-category/{}/", p.category_slug)), (p.short_title.as_str(), &format!("/product/{slug}/"))]),
    ]);
    let is_membership = p.is_subscription;
    let ask_url = format!("/#ask?sku={}", urlencode(&p.sku));
    html(&ProductTpl {
        page,
        p,
        images,
        description_html: body.description_html,
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
    })
}

#[allow(dead_code)]
pub fn entity() -> &'static str {
    Entity::NAME
}
