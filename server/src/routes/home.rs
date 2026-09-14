//! The homepage: the design canvas's "A + B" layout with every number recomputed from the catalog.

use crate::app::{breadcrumb_node, html, org_node, website_node, AppResult, Entity, PageMeta, State};
use crate::db::{self, PostCard, ProductCard};
use crate::engine;
use crate::money::{fmt_cents, fmt_dollars};
use askama::Template;
use axum::extract::State as AxState;

pub struct Edit {
    pub title: String,
    pub blurb: String,
    pub stat: String,
    pub url: String,
    pub image: String,
    pub image_alt: String,
}

pub struct NibGroup {
    pub name: String,
    pub line: String,
    pub count: usize,
    pub url: String,
    pub slug: String,
}

pub struct Chip {
    pub label: String,
    pub url: String,
}

pub struct CatTile {
    pub name: String,
    pub url: String,
    pub image: String,
    pub count: usize,
}

#[derive(Template)]
#[template(path = "home.html")]
pub struct HomeTpl {
    pub page: PageMeta,
    pub featured: ProductCard,
    pub featured_text: String,
    pub featured_image: String,
    pub hero_thumbs: Vec<ProductCard>,
    pub cat_tiles: Vec<CatTile>,
    pub latest_posts: Vec<PostCard>,
    pub live_total: usize,
    pub added_recent: usize,
    pub recent_label: String,
    pub just_in: Vec<ProductCard>,
    pub edits: Vec<Edit>,
    pub case: Vec<ProductCard>,
    pub chips: Vec<Chip>,
    pub vault: Vec<ProductCard>,
    pub grail_count: usize,
    pub grail_range: String,
    pub nibs: Vec<NibGroup>,
    pub starter_posts: Vec<PostCard>,
    pub cat_counts: Vec<engine::CategoryCount>,
    pub price_min: String,
    pub price_max: String,
    pub brands_with_stock: usize,
    pub tp_listing_price: String,
    pub unlimited_price: String,
    pub sold_count: usize,
}

const MONTHS: [&str; 12] = ["January", "February", "March", "April", "May", "June", "July", "August", "September", "October", "November", "December"];

pub async fn home(AxState(state): AxState<State>) -> AppResult {
    let cat = state.catalog();
    let s = engine::stats(&cat);
    let just_in = db::products_by_slugs_or_recent(&state.pool, 5, &[], true, true).await?;
    let exclude: Vec<i32> = just_in.iter().map(|p| p.id).collect();
    let case = db::products_by_slugs_or_recent(&state.pool, 8, &exclude, true, true).await?;
    let vault = db::top_priced(&state.pool, 3).await?;

    // Featured pen: the newest fountain pen with a photo; its own description and specs fill the hero.
    let featured = just_in.first().cloned().unwrap_or_default();
    let featured_text = if featured.id > 0 {
        let body = db::product_body(&state.pool, featured.id).await?;
        crate::text::summary(&body.description_text, 300)
    } else {
        String::new()
    };
    let featured_image = if featured.id > 0 {
        db::product_images(&state.pool, featured.id).await?.first().map(|i| i.large.clone()).unwrap_or_else(|| featured.image.clone())
    } else {
        String::new()
    };
    let hero_thumbs: Vec<ProductCard> = just_in.iter().skip(1).take(3).cloned().collect();
    let tile_image = |slug: &str| cat.pens.iter().find(|p| p.is_live() && p.category_slug == slug && !p.image.is_empty() && p.slug != featured.slug).map(|p| p.image.clone()).unwrap_or_default();
    let sale_image = cat.pens.iter().find(|p| p.is_live() && p.sale_price_cents.is_some() && !p.image.is_empty()).map(|p| p.image.clone()).unwrap_or_default();
    let count_cat = |slug: &str| cat.pens.iter().filter(|p| p.is_live() && p.category_slug == slug).count();
    let cat_tiles = vec![
        CatTile { name: "Vintage Pens".into(), url: "/product-category/vintage-pens/".into(), image: tile_image("vintage-pens"), count: count_cat("vintage-pens") },
        CatTile { name: "Pre-Owned Pens".into(), url: "/product-category/pre-owned-pens/".into(), image: tile_image("pre-owned-pens"), count: count_cat("pre-owned-pens") },
        CatTile { name: "Pencils".into(), url: "/product-category/pencils/".into(), image: tile_image("pencils"), count: count_cat("pencils") },
        CatTile { name: "Inkwells & Blotters".into(), url: "/product-category/inkwells-blotters/".into(), image: tile_image("inkwells-blotters"), count: count_cat("inkwells-blotters") },
        CatTile { name: "Best Bargains".into(), url: "/on-sale-pens/".into(), image: sale_image, count: s.on_sale },
    ];
    let (latest_posts, _) = db::posts(&state.pool, None, None, 1, 3).await?;

    // Edit photographs: one distinct pen per edit, chosen from the catalog snapshot (newest first).
    let pick = |pred: &dyn Fn(&engine::Pen) -> bool, used: &[String]| cat.pens.iter().find(|p| p.is_live() && !p.image.is_empty() && pred(p) && !used.contains(&p.slug)).map(|p| (p.image.clone(), p.short_title.clone(), p.slug.clone()));
    let e1 = pick(&|p| p.brand_slug == "parker" && p.category_slug == "vintage-pens" && (p.mechanism_slug == "vacumatic" || p.mechanism_slug == "button-filler"), &[]).unwrap_or_default();
    let e2 = pick(&|p| p.category_slug == "vintage-pens" && p.effective_cents() <= engine::FIRST_PEN_MAX_CENTS && !matches!(p.mechanism_slug.as_str(), "ballpoint" | "rollerball" | "pencil" | "dip"), &[e1.2.clone()]).unwrap_or_default();
    let e3 = pick(&|p| p.celluloid && p.category_slug == "vintage-pens", &[e1.2.clone(), e2.2.clone()]).unwrap_or_default();
    let edits = vec![
        Edit {
            title: "Golden-Age Parkers".into(),
            blurb: "Vacumatics and Duofolds from the decades Parker got everything right. Laminated celluloid, arrow clips, nibs with real character.".into(),
            stat: format!("{} PENS · {}–{}", s.vintage_parker_count, fmt_dollars(s.vintage_parker_min_cents), fmt_dollars(s.vintage_parker_max_cents)),
            url: "/brand/parker/?cat=vintage-pens".into(),
            image: e1.0.clone(),
            image_alt: e1.1.clone(),
        },
        Edit {
            title: "A First Fountain Pen, Under $150".into(),
            blurb: "Restored workhorses that forgive a new hand. Every one has been resacced, tuned, and written with before it went in the case.".into(),
            stat: format!("{} PENS · UNDER {}", s.first_pen_count, fmt_dollars(engine::FIRST_PEN_MAX_CENTS)),
            url: format!("/shop/?max={}&sort=price-asc", engine::FIRST_PEN_MAX_CENTS / 100),
            image: e2.0.clone(),
            image_alt: e2.1.clone(),
        },
        Edit {
            title: "Celluloid Worth Staring At".into(),
            blurb: "Marbled, striped, and laminated barrels from the material's golden decades, patterns no modern resin quite reproduces.".into(),
            stat: format!("{} PENS · SEARCH “CELLULOID”", s.celluloid_count),
            url: "/shop/?q=celluloid".into(),
            image: e3.0.clone(),
            image_alt: e3.1.clone(),
        },
    ];

    let chips = vec![
        Chip { label: "All".into(), url: "/shop/".into() },
        Chip { label: "Under $100".into(), url: "/shop/?max=100&sort=price-asc".into() },
        Chip { label: "Lever".into(), url: "/pw-filling-mechanism/lever-filler/".into() },
        Chip { label: "Vacumatic".into(), url: "/pw-filling-mechanism/vacumatic/".into() },
        Chip { label: "Flex nib".into(), url: "/nib/flexible/".into() },
        Chip { label: "Semi-flex".into(), url: "/nib/semi-flexible/".into() },
        Chip { label: "On sale".into(), url: "/on-sale-pens/".into() },
    ];

    let count_nib = |slugs: &[&str]| cat.pens.iter().filter(|p| p.is_live() && slugs.contains(&p.nib_slug.as_str())).count();
    let nibs = vec![
        NibGroup { name: "Fine".into(), line: "Precise, quiet, everyday".into(), count: count_nib(&["fine", "extra-fine"]), url: "/nib/fine/".into(), slug: "fine".into() },
        NibGroup { name: "Medium".into(), line: "The all-rounder".into(), count: count_nib(&["medium"]), url: "/nib/medium/".into(), slug: "medium".into() },
        NibGroup { name: "Broad".into(), line: "Wet, bold, shading inks".into(), count: count_nib(&["broad1", "bb1"]), url: "/nib/broad1/".into(), slug: "broad".into() },
        NibGroup { name: "Stub & oblique".into(), line: "Crisp edges, calligraphic".into(), count: count_nib(&["stub", "oblique"]), url: "/nib/stub/".into(), slug: "stub".into() },
        NibGroup { name: "Flex".into(), line: "Hairline to swell, the vintage magic".into(), count: count_nib(&["flexible", "semi-flexible"]), url: "/nib/flexible/".into(), slug: "flex".into() },
    ];

    let starter_posts = db::posts_by_slugs(&state.pool, &["how-do-i-start-collecting-pens-know-thy-obsession", "how-do-i-start-collecting-pens-vintage-vs-modern", "how-do-i-start-collecting-pens-why-are-some-pens-more-than-others"]).await?;

    let recent_label = if (1..=12).contains(&s.newest_month) { format!("{} PENS ADDED IN {}", s.added_in_newest_month, MONTHS[(s.newest_month - 1) as usize].to_uppercase()) } else { String::new() };
    let mut page = PageMeta::new(
        &state,
        "Vintage Fountain Pens, Pre-Owned Luxury Pens & Repairs | ThePenMarket.com",
        &format!("{} restored vintage and pre-owned pens, one of each, from {} to {}. Restored, tested and guaranteed by Nathaniel Cerf since 2007. Repairs, buying and the Trading Post classifieds.", s.live_total, fmt_dollars(s.price_min_cents), fmt_dollars(s.price_max_cents)),
        "/",
    );
    page.section = "home".into();
    if let Some(v) = vault.first() {
        if !v.image.is_empty() {
            page.og_image = state.abs(&v.image);
        }
    }
    let (page_title, page_desc) = (page.title.clone(), page.description.clone());
    let item_list: Vec<serde_json::Value> = just_in.iter().enumerate().map(|(i, p)| serde_json::json!({"@type": "ListItem", "position": i + 1, "url": state.abs(&p.url), "name": p.title})).collect();
    page = page.with_jsonld(vec![
        org_node(&state),
        website_node(&state),
        serde_json::json!({"@type": "WebPage", "@id": state.abs("/#webpage"), "url": state.abs("/"), "name": page_title, "isPartOf": {"@id": state.abs("/#website")}, "about": {"@id": state.abs("/#organization")}, "description": page_desc}),
        serde_json::json!({"@type": "ItemList", "name": "Just in", "itemListElement": item_list}),
        breadcrumb_node(&state, &[("Home", "/")]),
    ]);

    html(&HomeTpl {
        page,
        featured,
        featured_text,
        featured_image,
        hero_thumbs,
        cat_tiles,
        latest_posts,
        live_total: s.live_total,
        added_recent: s.added_in_newest_month,
        recent_label,
        just_in,
        edits,
        case,
        chips,
        vault,
        grail_count: s.grail_count,
        grail_range: format!("{}–{}", fmt_dollars(s.grail_min_cents), fmt_dollars(s.grail_max_cents)),
        nibs,
        starter_posts,
        cat_counts: s.by_category.clone(),
        price_min: fmt_cents(s.price_min_cents),
        price_max: fmt_cents(s.price_max_cents),
        brands_with_stock: s.brands_with_stock,
        tp_listing_price: fmt_dollars(engine::TRADING_POST_LISTING_CENTS),
        unlimited_price: fmt_dollars(engine::UNLIMITED_POSTS_YEAR_CENTS),
        sold_count: s.sold,
    })
}

#[allow(dead_code)]
pub fn entity_name() -> &'static str {
    Entity::NAME
}
