//! The service and identity pages: repairs, sell my pens, contact, guarantee, about, privacy.
//! Copy is his, verbatim from the backup pages; the forms are labelled demos.

use crate::app::{breadcrumb_node, html, org_node, AppError, AppResult, Entity, PageMeta, State};
use crate::db;
use crate::engine;
use crate::security::{csrf_token, CsrfCookie};
use askama::Template;
use axum::extract::State as AxState;
use axum::Extension;

pub struct Faq {
    pub q: String,
    pub a: String,
}

fn faq_node(items: &[Faq]) -> serde_json::Value {
    serde_json::json!({"@type": "FAQPage", "mainEntity": items.iter().map(|f| serde_json::json!({"@type": "Question", "name": f.q, "acceptedAnswer": {"@type": "Answer", "text": f.a}})).collect::<Vec<_>>()})
}

#[derive(Template)]
#[template(path = "repairs.html")]
pub struct RepairsTpl {
    pub page: PageMeta,
    pub csrf: String,
    pub scope: Vec<&'static str>,
    pub shipping: Vec<&'static str>,
    pub repairable_counts: Vec<(String, String, i64)>,
    pub faqs: Vec<Faq>,
    pub bench_posts: Vec<db::PostCard>,
}

pub async fn repairs(AxState(state): AxState<State>, Extension(cookie): Extension<CsrfCookie>) -> AppResult {
    let faqs = vec![
        Faq { q: "What pens can ThePenMarket.com repair?".into(), a: format!("{}. Please note that we are no longer restoring European, Nozac and Sheaffer piston fillers.", engine::REPAIR_SCOPE.join(", ")) },
        Faq { q: "How do I get a repair estimate?".into(), a: "For a free repair estimate, use the form on the Pen Repairs page. If possible, please upload a photo of your pen or pens to be repaired. Discounts are available for shipments of 5 or more pens.".into() },
        Faq { q: "How should I ship a pen for repair?".into(), a: format!("To ensure your pen arrives safely: {}", engine::SHIPPING_STEPS.join(" ")) },
        Faq { q: "Is there any risk in restoring a vintage pen?".into(), a: "Restoring a pen is generally safe, but due to the age and materials of vintage pens, there is a small risk of damage. By submitting your pen for restoration, you acknowledge these risks and waive any claims in the event of damage.".into() },
    ];
    let cat = state.catalog();
    let mut counts: Vec<(String, String, i64)> = vec![];
    for slug in ["lever-filler", "button-filler", "vacumatic", "snorkel", "touchdown", "crescent-filler", "aerometric-filler"] {
        let n = cat.pens.iter().filter(|p| p.is_live() && p.mechanism_slug == slug).count() as i64;
        let name = cat.pens.iter().find(|p| p.mechanism_slug == slug).map(|p| p.mechanism.clone()).unwrap_or_else(|| slug.replace('-', " "));
        counts.push((name, format!("/pw-filling-mechanism/{slug}/"), n));
    }
    let bench_posts = db::posts_by_slugs(&state.pool, &["how-do-i-restore-a-parker-vacumatic", "how-do-i-restore-a-sheaffer-touchdown", "how-do-i-restore-a-conklin-crescent", "how-do-i-polish-a-pen-celluloid-edition-2"]).await?;
    let mut page = PageMeta::new(&state, "Vintage Pen Repairs & Restoration: Lever, Button, Vacumatic, Snorkel | ThePenMarket.com", "Get vintage pen repairs at ThePenMarket.com. With more than 20 years experience in writing instrument restoration, satisfaction is our goal. Free estimate; discounts for 5 or more pens.", "/pen-repairs/");
    page.section = "repairs".into();
    page = page.with_jsonld(vec![
        org_node(&state),
        serde_json::json!({"@type": "Service", "@id": state.abs("/pen-repairs/#service"), "name": "Vintage pen repair and restoration", "serviceType": "Fountain pen restoration", "provider": {"@id": state.abs("/#organization")}, "areaServed": "US", "description": "Restoration of button fillers, lever fillers, Parker Vacumatics, Sheaffer Snorkels, Sheaffer Touchdowns, crescent fillers and aerometrics. Free estimate.", "url": state.abs("/pen-repairs/")}),
        faq_node(&faqs),
        breadcrumb_node(&state, &[("Home", "/"), ("Pen Repairs", "/pen-repairs/")]),
    ]);
    html(&RepairsTpl { page, csrf: csrf_token(&state.cfg.csrf_secret, &cookie.0), scope: engine::REPAIR_SCOPE.to_vec(), shipping: engine::SHIPPING_STEPS.to_vec(), repairable_counts: counts, faqs, bench_posts })
}

#[derive(Template)]
#[template(path = "sell.html")]
pub struct SellTpl {
    pub page: PageMeta,
    pub csrf: String,
    pub faqs: Vec<Faq>,
    pub exclusion_line: &'static str,
    pub listing_price: String,
}

pub async fn sell(AxState(state): AxState<State>, Extension(cookie): Extension<CsrfCookie>) -> AppResult {
    let faqs = vec![
        Faq { q: "How do I sell my pens to ThePenMarket.com?".into(), a: "We are usually looking for more vintage pens and pre-owned luxury pens to restore and sell on our site. Depending upon what you have, we can offer several options from cash to consignment. Please contact us with the form, and let us know what you have. If you aren't sure what you have, please include a photo.".into() },
        Faq { q: "What pens is ThePenMarket.com not looking for?".into(), a: engine::SELL_EXCLUSION_LINE.into() },
        Faq { q: "Can I sell a pen myself instead?".into(), a: "Yes. Sell your pens directly to our customers on our classified ads section, the Trading Post. It only costs $5 per listing, and you keep your pen until someone pays you.".into() },
    ];
    let mut page = PageMeta::new(&state, "Sell My Pens: Cash or Consignment for Vintage & Luxury Pen Collections | ThePenMarket.com", "Looking to cash in on your pen collection, or inherited one you don't want? ThePenMarket.com buys vintage pens and pre-owned luxury pens, cash to consignment. Send a photo.", "/sell-my-pens/");
    page.section = "sell".into();
    page = page.with_jsonld(vec![org_node(&state), faq_node(&faqs), breadcrumb_node(&state, &[("Home", "/"), ("How Do I Sell My Pens?", "/sell-my-pens/")])]);
    html(&SellTpl { page, csrf: csrf_token(&state.cfg.csrf_secret, &cookie.0), faqs, exclusion_line: engine::SELL_EXCLUSION_LINE, listing_price: crate::money::fmt_dollars(engine::TRADING_POST_LISTING_CENTS) })
}

#[derive(Template)]
#[template(path = "contact.html")]
pub struct ContactTpl {
    pub page: PageMeta,
    pub csrf: String,
}

pub async fn contact(AxState(state): AxState<State>, Extension(cookie): Extension<CsrfCookie>) -> AppResult {
    let mut page = PageMeta::new(&state, "Contact ThePenMarket.com: (847) 708-5062, info@thepenmarket.com, Norwich CT", "You got questions? We got answers. Reach ThePenMarket.com by phone at (847) 708-5062, by e-mail at info@thepenmarket.com, or by mail at P.O. Box 1086, Norwich, CT 06360-1086.", "/contact/");
    page.section = "contact".into();
    page = page.with_jsonld(vec![
        org_node(&state),
        serde_json::json!({"@type": "ContactPage", "@id": state.abs("/contact/#page"), "url": state.abs("/contact/"), "name": "Contact", "isPartOf": {"@id": state.abs("/#website")}, "about": {"@id": state.abs("/#organization")}}),
        breadcrumb_node(&state, &[("Home", "/"), ("Contact", "/contact/")]),
    ]);
    html(&ContactTpl { page, csrf: csrf_token(&state.cfg.csrf_secret, &cookie.0) })
}

#[derive(Template)]
#[template(path = "guarantee.html")]
pub struct GuaranteeTpl {
    pub page: PageMeta,
    pub terms: &'static str,
    pub return_days: u64,
    pub fix_days: u64,
    pub faqs: Vec<Faq>,
}

pub async fn guarantee(AxState(state): AxState<State>) -> AppResult {
    let faqs = vec![
        Faq { q: "What is ThePenMarket.com's return policy?".into(), a: "If you are not satisfied with your purchase for any reason, return it in the same condition it arrived via insured U.S. Mail within 14 days of receiving it, and you will be refunded the full price of the item, not including the shipping.".into() },
        Faq { q: "What if I find a problem after 14 days?".into(), a: "If you discover any problems we might have missed within the first 30 days of receiving your pen, let us know and we'll do what we can to fix it.".into() },
        Faq { q: "Does the guarantee cover Trading Post purchases?".into(), a: "No. ThePenMarket.com is not responsible for any transactions made via our Trading Post. We do not see, possess or speak for the quality of those pens. However, should you have any difficulty with any pen dealer, we will do our best to mediate the situation.".into() },
    ];
    let mut page = PageMeta::new(&state, "Our Guarantee: 14-Day Returns and a 30-Day Repair Promise | ThePenMarket.com", "Every pen from ThePenMarket.com carries Our Guarantee: return it within 14 days for a full refund of the item price, and problems found within 30 days get fixed. Trading Post sales are between buyer and seller.", "/guarantee/");
    page.section = "guarantee".into();
    page = page.with_jsonld(vec![
        org_node(&state),
        serde_json::json!({"@type": "MerchantReturnPolicy", "@id": state.abs("/guarantee/#policy"), "name": Entity::GUARANTEE_NAME, "url": state.abs("/guarantee/"), "returnPolicyCategory": "https://schema.org/MerchantReturnFiniteReturnWindow", "merchantReturnDays": engine::GUARANTEE_RETURN_DAYS, "returnMethod": "https://schema.org/ReturnByMail", "returnFees": "https://schema.org/ReturnShippingFees", "refundType": "https://schema.org/FullRefund", "applicableCountry": "US"}),
        faq_node(&faqs),
        breadcrumb_node(&state, &[("Home", "/"), ("Guarantee", "/guarantee/")]),
    ]);
    html(&GuaranteeTpl { page, terms: engine::GUARANTEE_TERMS, return_days: engine::GUARANTEE_RETURN_DAYS, fix_days: engine::REPAIR_PROMISE_DAYS, faqs })
}

#[derive(Template)]
#[template(path = "about.html")]
pub struct AboutTpl {
    pub page: PageMeta,
    pub live_total: usize,
    pub post_count: i64,
    pub listing_count: i64,
}

pub async fn about(AxState(state): AxState<State>) -> AppResult {
    let cat = state.catalog();
    let s = engine::stats(&cat);
    let (_, post_count) = db::posts(&state.pool, None, None, 1, 1).await?;
    let (_, listing_count) = db::listings(&state.pool, 1, 1).await?;
    let mut page = PageMeta::new(&state, "About ThePenMarket.com: Nathaniel Cerf, Vintage Pen Restorer Since 2007", "ThePenMarket.com is a place where folks can buy, sell and trade pens of any era. Owner Nathaniel Cerf has restored vintage pens for more than 20 years; online since 2007, now in Norwich, Connecticut.", "/about-us/");
    page.section = "about".into();
    page.og_image = state.abs("/media/uploads/2025/01/675de49e29803_Nathaniel-Cerf-Profile-1-of-1-scaled.jpg");
    page = page.with_jsonld(vec![
        org_node(&state),
        serde_json::json!({
            "@type": "Person",
            "@id": state.abs("/about-us/#nathaniel-cerf"),
            "name": Entity::OWNER,
            "jobTitle": "Owner and restorer",
            "worksFor": {"@id": state.abs("/#organization")},
            "url": state.abs("/about-us/"),
            "knowsAbout": ["vintage fountain pens", "fountain pen restoration", "Parker Vacumatic", "Sheaffer Snorkel", "fountain pen ink"],
            "sameAs": [Entity::FACEBOOK, Entity::INSTAGRAM]
        }),
        serde_json::json!({"@type": "AboutPage", "url": state.abs("/about-us/"), "name": "About Us", "isPartOf": {"@id": state.abs("/#website")}, "about": {"@id": state.abs("/#organization")}}),
        breadcrumb_node(&state, &[("Home", "/"), ("About Us", "/about-us/")]),
    ]);
    html(&AboutTpl { page, live_total: s.live_total, post_count, listing_count })
}

#[derive(Template)]
#[template(path = "page.html")]
pub struct GenericPageTpl {
    pub page: PageMeta,
    pub heading: String,
    pub body_html: String,
    pub modified: String,
}

pub async fn privacy(AxState(state): AxState<State>) -> AppResult {
    let p = db::page(&state.pool, "privacy").await?.ok_or(AppError::NotFound)?;
    let mut page = PageMeta::new(&state, "Privacy Policy | ThePenMarket.com", "How ThePenMarket.com collects, uses and protects your personal information.", "/privacy/");
    page.section = "privacy".into();
    page = page.with_jsonld(vec![org_node(&state), breadcrumb_node(&state, &[("Home", "/"), ("Privacy Policy", "/privacy/")])]);
    html(&GenericPageTpl { page, heading: p.title, body_html: p.body_html, modified: p.modified_at.format("%B %-d, %Y").to_string() })
}
