//! The glossary visitors see: one searchable index over his vocabulary and model entries plus the
//! makes, nibs, filling mechanisms and eras (which keep their own pages), and a page per entry with
//! the pens in the case and the posts that go with it. Every definition shown is his.

use crate::app::{breadcrumb_node, html, org_node, AppError, AppResult, PageMeta, State};
use crate::db::{self, PostCard, ProductCard, ShopFilter};
use crate::glossary::{self, Entry};
use askama::Template;
use axum::extract::{Path, Query, State as AxState};
use serde::Deserialize;

#[derive(Deserialize, Default)]
pub struct HubQuery {
    #[serde(default)]
    pub q: String,
    #[serde(default)]
    pub kind: String,
}

pub struct HubEntry {
    pub e: Entry,
    /// Live pens under a make, nib, filling system or era; zero for vocabulary and models.
    pub stock: usize,
}
pub struct LetterGroup {
    pub letter: String,
    pub items: Vec<HubEntry>,
}
pub struct KindChip {
    pub key: String,
    pub label: String,
    pub count: usize,
    pub selected: bool,
}

#[derive(Template)]
#[template(path = "glossary.html")]
pub struct HubTpl {
    pub page: PageMeta,
    pub groups: Vec<LetterGroup>,
    pub kinds: Vec<KindChip>,
    pub q: String,
    pub kind: String,
    pub total: usize,
    pub shown: usize,
    pub defined: usize,
}

const KIND_ORDER: [(&str, &str); 6] = [("term", "Terms"), ("model", "Models"), ("make", "Makes"), ("nib", "Nibs"), ("filling", "Filling systems"), ("era", "Eras")];

pub async fn hub(AxState(state): AxState<State>, Query(hq): Query<HubQuery>) -> AppResult {
    let site = state.site();
    let g = &site.glossary;
    let cat = state.catalog();
    let q: String = hq.q.trim().chars().take(60).collect();
    let folded = crate::gloss::fold(&q);
    let kind = if KIND_ORDER.iter().any(|(k, _)| *k == hq.kind) { hq.kind.clone() } else { String::new() };

    let matches = |e: &Entry| (kind.is_empty() || e.kind == kind) && (folded.is_empty() || e.search.contains(&folded));
    let mut groups: Vec<LetterGroup> = vec![];
    let mut shown = 0;
    for e in g.entries.iter().filter(|e| matches(e)) {
        let stock = match e.kind.as_str() {
            "make" => cat.pens.iter().filter(|p| p.is_live() && p.brand_slug == e.slug).count(),
            "nib" => cat.pens.iter().filter(|p| p.is_live() && p.nib_slug == e.slug).count(),
            "filling" => cat.pens.iter().filter(|p| p.is_live() && p.mechanism_slug == e.slug).count(),
            "era" => cat.pens.iter().filter(|p| p.is_live() && p.era_slug == e.slug).count(),
            _ => 0,
        };
        shown += 1;
        let item = HubEntry { e: e.clone(), stock };
        match groups.last_mut().filter(|grp| grp.letter == e.letter) {
            Some(grp) => grp.items.push(item),
            None => groups.push(LetterGroup { letter: e.letter.clone(), items: vec![item] }),
        }
    }
    if shown == 0 && !folded.is_empty() {
        glossary::record_miss(&state.pool, &q).await;
    }
    let kinds: Vec<KindChip> = KIND_ORDER.iter().map(|(k, label)| KindChip { key: k.to_string(), label: label.to_string(), count: g.entries.iter().filter(|e| e.kind == *k).count(), selected: kind == *k }).filter(|c| c.count > 0).collect();

    let mut page = PageMeta::new(&state, "Fountain Pen Glossary: Terms, Makes and Models Explained | ThePenMarket.com", "Look up the words in a vintage pen listing: parts, materials, condition and repair terms, filling systems, nibs, makes and models, explained by Nathaniel Cerf of ThePenMarket.com.", "/glossary/");
    page.section = "glossary".into();
    // A filtered view, or an index with nothing of his in it yet, is not a page for search engines.
    page.noindex = !q.is_empty() || !kind.is_empty() || g.defined == 0;
    let defined_terms: Vec<serde_json::Value> = g.entries.iter().filter(|e| !e.definition.is_empty()).map(|e| serde_json::json!({"@type": "DefinedTerm", "@id": state.abs(&format!("{}#term", e.url)), "name": e.name, "description": e.short, "url": state.abs(&e.url)})).collect();
    page = page.with_jsonld(vec![
        org_node(&state),
        serde_json::json!({"@type": "DefinedTermSet", "@id": state.abs("/glossary/#terms"), "name": "ThePenMarket.com fountain pen glossary", "url": state.abs("/glossary/"), "author": {"@id": state.abs("/about-us/#nathaniel-cerf")}, "hasDefinedTerm": defined_terms}),
        breadcrumb_node(&state, &[("Home", "/"), ("Glossary", "/glossary/")]),
    ]);
    html(&HubTpl { page, groups, kinds, q, kind, total: g.entries.len(), shown, defined: g.defined })
}

pub struct SeeAlso {
    pub name: String,
    pub url: String,
}

#[derive(Template)]
#[template(path = "glossary_term.html")]
pub struct TermTpl {
    pub page: PageMeta,
    pub t: glossary::TermRow,
    pub kind_label: String,
    pub paragraphs: Vec<String>,
    pub aliases: Vec<String>,
    pub make: Option<SeeAlso>,
    pub see_also: Vec<SeeAlso>,
    pub pens: Vec<ProductCard>,
    pub pens_total: i64,
    pub shop_url: String,
    pub posts: Vec<PostCard>,
    pub image: String,
}

pub async fn term(AxState(state): AxState<State>, Path(slug): Path<String>) -> AppResult {
    let include_drafts = crate::admin::current().is_some();
    let t = glossary::term_by_slug(&state.pool, &slug, include_drafts).await?.ok_or(AppError::NotFound)?;
    // A published entry with no words in it is not an entry.
    if t.definition.trim().is_empty() && !include_drafts {
        return Err(AppError::NotFound);
    }
    let site = state.site();
    let path = format!("/glossary/{slug}/");
    let kind_label = glossary::kind_label(&t.kind).to_string();

    let make = if t.brand_slug.is_empty() { None } else { db::term_by_slug(&state.pool, "brand", &t.brand_slug).await?.map(|b| SeeAlso { name: crate::text::decode(&b.name), url: format!("/brand/{}/", b.slug) }) };
    let see_also: Vec<SeeAlso> = glossary::split_list(&t.see_also).iter().filter_map(|n| site.glossary.find(n)).filter(|e| e.url != path).map(|e| SeeAlso { name: e.name.clone(), url: e.url.clone() }).collect();

    let ranges = db::price_ranges(&state.pool).await?;
    let filter = ShopFilter { q: Some(t.name.clone()), sort: "relevance".into(), page: 1, per_page: 4, ..Default::default() };
    let found = db::shop(&state.pool, &filter, &ranges).await?;
    let posts = db::posts_matching(&state.pool, &t.name, 3).await?;

    let description = crate::text::summary(&t.definition, 155);
    let title = match t.kind.as_str() {
        "model" => format!("{}: What to Know About the Model | ThePenMarket.com Glossary", t.name),
        _ => format!("What Does “{}” Mean on a Fountain Pen? | ThePenMarket.com Glossary", t.name),
    };
    let mut page = PageMeta::new(&state, &title, &description, &path);
    page.section = "glossary".into();
    page.noindex = t.status != "published";
    page.edit_link = Some(("Edit this entry".into(), format!("/admin/glossary/{}/", t.id)));
    let image = if t.image_path.is_empty() { String::new() } else { t.image_path.clone() };
    if !image.is_empty() {
        page.og_image = state.abs(&image);
    }
    let mut node = serde_json::json!({"@type": "DefinedTerm", "@id": state.abs(&format!("{path}#term")), "name": t.name, "description": t.definition, "url": state.abs(&path), "inDefinedTermSet": {"@id": state.abs("/glossary/#terms")}});
    let aliases = t.alias_list();
    if !aliases.is_empty() {
        node["alternateName"] = serde_json::json!(aliases);
    }
    if !image.is_empty() {
        node["image"] = serde_json::json!(state.abs(&image));
    }
    page = page.with_jsonld(vec![org_node(&state), node, breadcrumb_node(&state, &[("Home", "/"), ("Glossary", "/glossary/"), (t.name.as_str(), path.as_str())])]);

    let shop_url = format!("/shop/?q={}", urlencoding::encode(&t.name));
    html(&TermTpl { page, kind_label, paragraphs: t.paragraphs(), aliases, make, see_also, pens: found.items, pens_total: found.total, shop_url, posts, image, t })
}
