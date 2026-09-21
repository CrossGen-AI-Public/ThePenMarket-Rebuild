//! The reference pages built for search: the site-wide FAQ, the visitor's site map, and the custom
//! pages Nathaniel writes in the admin (grading system, glossary, ...), served at `/{slug}/`.
//! Every answer on the FAQ is his: seeded from the copy already on the service pages, then edited by
//! him in the admin.

use crate::app::{breadcrumb_node, html, org_node, AppResult, PageMeta, State};
use crate::db;
use crate::engine;
use askama::Template;
use axum::extract::State as AxState;

pub struct FaqItem {
    pub id: i32,
    pub anchor: String,
    pub q: String,
    pub a: String,
}
pub struct FaqTopic {
    pub name: String,
    pub anchor: String,
    pub items: Vec<FaqItem>,
}

#[derive(Template)]
#[template(path = "faq.html")]
pub struct FaqTpl {
    pub page: PageMeta,
    pub topics: Vec<FaqTopic>,
    pub total: usize,
}

/// First run only: the questions already answered, in his words, on the repairs, sell, guarantee and
/// Trading Post pages. After that the table is his to edit.
pub async fn seed_faqs(pool: &sqlx::PgPool) -> anyhow::Result<()> {
    let n: i64 = sqlx::query_scalar("SELECT count(*) FROM faq").fetch_one(pool).await?;
    if n > 0 {
        return Ok(());
    }
    let seed: Vec<(&str, &str, String)> = vec![
        ("Buying and returns", "What is ThePenMarket.com's return policy?", "If you are not satisfied with your purchase for any reason, return it in the same condition it arrived via insured U.S. Mail within 14 days of receiving it, and you will be refunded the full price of the item, not including the shipping.".into()),
        ("Buying and returns", "What if I find a problem after 14 days?", "If you discover any problems we might have missed within the first 30 days of receiving your pen, let us know and we'll do what we can to fix it.".into()),
        ("Buying and returns", "Does the guarantee cover Trading Post purchases?", "No. ThePenMarket.com is not responsible for any transactions made via our Trading Post. We do not see, possess or speak for the quality of those pens. However, should you have any difficulty with any pen dealer, we will do our best to mediate the situation.".into()),
        ("Pen repairs", "What pens can ThePenMarket.com repair?", format!("{}. Please note that we are no longer restoring European, Nozac and Sheaffer piston fillers.", engine::REPAIR_SCOPE.join(", "))),
        ("Pen repairs", "How do I get a repair estimate?", "For a free repair estimate, use the form on the Pen Repairs page. If possible, please upload a photo of your pen or pens to be repaired. Discounts are available for shipments of 5 or more pens.".into()),
        ("Pen repairs", "How should I ship a pen for repair?", format!("To ensure your pen arrives safely: {}", engine::SHIPPING_STEPS.join(" "))),
        ("Pen repairs", "Is there any risk in restoring a vintage pen?", "Restoring a pen is generally safe, but due to the age and materials of vintage pens, there is a small risk of damage. By submitting your pen for restoration, you acknowledge these risks and waive any claims in the event of damage.".into()),
        ("Selling your pens", "How do I sell my pens to ThePenMarket.com?", "We are usually looking for more vintage pens and pre-owned luxury pens to restore and sell on our site. Depending upon what you have, we can offer several options from cash to consignment. Please contact us with the form, and let us know what you have. If you aren't sure what you have, please include a photo.".into()),
        ("Selling your pens", "What pens is ThePenMarket.com not looking for?", engine::SELL_EXCLUSION_LINE.into()),
        ("Selling your pens", "Can I sell a pen myself instead?", "Yes. Sell your pens directly to our customers on our classified ads section, the Trading Post. It only costs $5 per listing, and you keep your pen until someone pays you.".into()),
        ("Trading Post", "How much does a Trading Post listing cost?", "It only costs $5 per listing, and you keep your pen until someone pays you. An Unlimited Posts membership is $125 for one calendar year of unlimited listings.".into()),
        ("Trading Post", "Does ThePenMarket.com guarantee Trading Post items?", "No. The merchandise is not guaranteed by ThePenMarket.com, nor does the transaction pass through our hands. We do our best to vet out the bad dealers, and ask that you report any problems immediately to tradingpost@thepenmarket.com.".into()),
    ];
    for (i, (topic, q, a)) in seed.iter().enumerate() {
        sqlx::query("INSERT INTO faq (topic, question, answer, sort_order) VALUES ($1, $2, $3, $4)").bind(topic).bind(q).bind(a).bind(i as i32).execute(pool).await?;
    }
    Ok(())
}

/// The pages Nathaniel asked for on 2026-09-18, made once as empty drafts at good addresses. He is a
/// writer and the words are his: nothing is written here, and a draft is invisible to visitors.
pub async fn seed_requested_pages(pool: &sqlx::PgPool) -> anyhow::Result<()> {
    for (slug, title) in [("pen-grading", "Pen Grading"), ("defining-nibs", "Defining Nibs"), ("buyers-guide", "Buyer’s Guide")] {
        sqlx::query("INSERT INTO page (slug, title, body_html, status, custom, show_in_footer, published_at, modified_at) VALUES ($1, $2, '', 'draft', TRUE, TRUE, now(), now()) ON CONFLICT (slug) DO NOTHING").bind(slug).bind(title).execute(pool).await?;
    }
    Ok(())
}

pub async fn faq(AxState(state): AxState<State>) -> AppResult {
    let rows = db::faqs(&state.pool, true).await?;
    let total = rows.len();
    let mut topics: Vec<FaqTopic> = vec![];
    for r in rows {
        let item = FaqItem { id: r.id, anchor: format!("q{}", r.id), q: r.question, a: r.answer };
        match topics.iter_mut().find(|t| t.name == r.topic) {
            Some(t) => t.items.push(item),
            None => topics.push(FaqTopic { anchor: crate::text::slugify(&r.topic), name: r.topic, items: vec![item] }),
        }
    }
    let mut page = PageMeta::new(&state, "Frequently Asked Questions: Buying, Repairs, Selling Your Pens | ThePenMarket.com", "Nathaniel Cerf's answers to the questions ThePenMarket.com gets most: returns and the guarantee, vintage pen repairs, selling a collection, and the Trading Post.", "/faq/");
    page.section = "faq".into();
    let entities: Vec<serde_json::Value> = topics.iter().flat_map(|t| t.items.iter()).map(|f| serde_json::json!({"@type": "Question", "name": f.q, "acceptedAnswer": {"@type": "Answer", "text": f.a}})).collect();
    page = page.with_jsonld(vec![org_node(&state), serde_json::json!({"@type": "FAQPage", "@id": state.abs("/faq/#faq"), "mainEntity": entities}), breadcrumb_node(&state, &[("Home", "/"), ("FAQ", "/faq/")])]);
    html(&FaqTpl { page, topics, total })
}

pub struct MapLink {
    pub name: String,
    pub url: String,
    pub note: String,
}
pub struct MapGroup {
    pub name: String,
    pub links: Vec<MapLink>,
}

#[derive(Template)]
#[template(path = "site_map.html")]
pub struct SiteMapTpl {
    pub page: PageMeta,
    pub groups: Vec<MapGroup>,
}

fn link(name: &str, url: &str, note: String) -> MapLink {
    MapLink { name: name.to_string(), url: url.to_string(), note }
}

/// The visitor's site map: every section, every term with pens in it, every blog category and every
/// custom page, from the database, so it never goes stale. (`/sitemap.xml` is the one for crawlers.)
pub async fn site_map(AxState(state): AxState<State>) -> AppResult {
    let cat = state.catalog();
    let s = engine::stats(&cat);
    let mut groups = vec![MapGroup {
        name: "Shop".into(),
        links: std::iter::once(link("All pens", "/shop/", format!("{} in stock", s.live_total)))
            .chain(s.by_category.iter().filter(|c| c.slug != "memberships").map(|c| link(&c.name, &format!("/product-category/{}/", c.slug), format!("{} in stock", c.count))))
            .chain(std::iter::once(link("Best Bargains", "/on-sale-pens/", format!("{} on sale", s.on_sale))))
            .collect(),
    }];
    for (table, fk, base, label) in [("brand", "brand_id", "/brand/", "Pens by brand"), ("era", "era_id", "/era/", "Pens by era"), ("filling_mechanism", "filling_mechanism_id", "/pw-filling-mechanism/", "Pens by filling mechanism"), ("nib", "nib_id", "/nib/", "Pens by nib")] {
        let live = db::term_slugs_with_products(&state.pool, table, fk).await?;
        let terms = db::terms(&state.pool, table).await?;
        let links: Vec<MapLink> = terms.iter().filter(|t| live.contains(&t.slug)).map(|t| link(&t.name, &format!("{base}{}/", t.slug), String::new())).collect();
        if !links.is_empty() {
            groups.push(MapGroup { name: label.into(), links });
        }
    }
    groups.push(MapGroup {
        name: "Services".into(),
        links: vec![
            link("Pen Repairs", "/pen-repairs/", String::new()),
            link("How Do I Sell My Pens?", "/sell-my-pens/", String::new()),
            link("Trading Post", "/trading-post/", String::new()),
            link("Post a listing", "/post-your-product/", String::new()),
            link("Our Guarantee", "/guarantee/", String::new()),
            link("Ask the Pen Market", "/#ask", String::new()),
        ],
    });
    let cats = db::blog_categories(&state.pool).await?;
    groups.push(MapGroup {
        name: "Drippy Musings, the blog".into(),
        links: std::iter::once(link("All posts", "/blog/", String::new())).chain(cats.iter().filter(|c| c.count > 0).map(|c| link(&c.name, &format!("/category/{}/", c.slug), format!("{} posts", c.count)))).collect(),
    });
    let mut about = vec![link("About Us", "/about-us/", String::new()), link("Contact Us", "/contact/", String::new()), link("Frequently Asked Questions", "/faq/", String::new())];
    if state.site().glossary.defined > 0 {
        about.push(link("Pen Glossary", "/glossary/", format!("{} entries", state.site().glossary.defined)));
    }
    about.extend(db::custom_pages(&state.pool).await?.into_iter().map(|(title, slug)| link(&title, &format!("/{slug}/"), String::new())));
    about.push(link("Privacy Policy", "/privacy/", String::new()));
    groups.push(MapGroup { name: "About ThePenMarket.com".into(), links: about });

    let mut page = PageMeta::new(&state, "Site Map: Every Section of ThePenMarket.com", "Every section of ThePenMarket.com on one page: pens by brand, era, nib and filling mechanism, repairs, selling, the Trading Post and the blog.", "/site-map/");
    page.section = "site-map".into();
    page = page.with_jsonld(vec![org_node(&state), breadcrumb_node(&state, &[("Home", "/"), ("Site Map", "/site-map/")])]);
    html(&SiteMapTpl { page, groups })
}

#[derive(Template)]
#[template(path = "page.html")]
pub struct CustomPageTpl {
    pub page: PageMeta,
    pub heading: String,
    pub modified: String,
    pub body_html: String,
}

/// A custom page at `/{slug}/`, if one exists (drafts only for the signed-in admin).
pub async fn custom_page(state: &State, slug: &str) -> Result<Option<axum::response::Response>, crate::app::AppError> {
    let include_drafts = crate::admin::current().is_some();
    let Some(p) = db::custom_page(&state.pool, slug, include_drafts).await? else { return Ok(None) };
    let path = format!("/{slug}/");
    let description = if p.meta_description.is_empty() { crate::text::summary(&crate::text::strip_tags(&p.body_html), 155) } else { p.meta_description.clone() };
    let mut page = PageMeta::new(state, &format!("{} | ThePenMarket.com", p.title), &description, &path);
    page.section = "page".into();
    page.noindex = p.status != "published";
    page.edit_link = Some(("Edit this page".into(), format!("/admin/pages/{}/", p.id)));
    page = page.with_jsonld(vec![
        org_node(state),
        serde_json::json!({"@type": "WebPage", "@id": state.abs(&format!("{path}#webpage")), "url": state.abs(&path), "name": p.title, "description": description, "dateModified": p.modified_at.to_rfc3339(), "isPartOf": {"@id": state.abs("/#website")}, "author": {"@id": state.abs("/about-us/#nathaniel-cerf")}}),
        breadcrumb_node(state, &[("Home", "/"), (p.title.as_str(), path.as_str())]),
    ]);
    Ok(Some(html(&CustomPageTpl { page, heading: p.title, modified: p.modified_at.format("%B %-d, %Y").to_string(), body_html: p.body_html })?))
}
