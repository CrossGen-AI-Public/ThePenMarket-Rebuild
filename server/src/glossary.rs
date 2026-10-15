//! The glossary's data: his published entries plus the makes, nibs, filling mechanisms and eras he has
//! approved, held in the site cache for the hub, the shop search card, the tap-to-define links and the
//! guide's `define_term` tool. Also the pass over his own writing that proposes entries: it finds
//! words and quotes his sentences; it never writes one.

use crate::gloss::{self, LinkTerm, Matcher};
use crate::text;
use sqlx::{FromRow, PgPool, Row};
use std::collections::HashSet;

const LEXICON: &str = include_str!("../data/glossary-lexicon.json");
/// A word has to turn up in at least this many of his posts and listings to be proposed.
const MIN_MENTIONS: usize = 2;
const QUOTES_PER_TERM: usize = 5;
pub const MAX_LINKS_PER_LISTING: usize = 8;

/// (table, product column, entry kind, label, URL prefix)
pub const TAXONOMIES: [(&str, &str, &str, &str, &str); 4] = [
    ("brand", "brand_id", "make", "Make", "/brand/"),
    ("nib", "nib_id", "nib", "Nib", "/nib/"),
    ("filling_mechanism", "filling_mechanism_id", "filling", "Filling system", "/pw-filling-mechanism/"),
    ("era", "era_id", "era", "Era", "/era/"),
];

pub fn kind_label(kind: &str) -> &'static str {
    match kind {
        "model" => "Model",
        "make" => "Make",
        "nib" => "Nib",
        "filling" => "Filling system",
        "era" => "Era",
        _ => "Term",
    }
}

#[derive(Clone, Debug, FromRow)]
pub struct TermRow {
    pub id: i32,
    pub slug: String,
    pub name: String,
    pub kind: String,
    pub topic: String,
    pub aliases: String,
    pub definition: String,
    pub brand_slug: String,
    pub image_path: String,
    pub image_alt: String,
    pub see_also: String,
    pub source_url: String,
    pub source_title: String,
    pub status: String,
    pub in_guide: bool,
    pub link_in_listings: bool,
    pub origin: String,
    pub mentions: i32,
}
const TERM_COLS: &str = "id, slug, name, kind, topic, aliases, definition, brand_slug, image_path, image_alt, see_also, source_url, source_title, status, in_guide, link_in_listings, origin, mentions";

impl TermRow {
    pub fn alias_list(&self) -> Vec<String> {
        split_list(&self.aliases)
    }
    pub fn names(&self) -> Vec<String> {
        std::iter::once(self.name.clone()).chain(self.alias_list()).collect()
    }
    pub fn paragraphs(&self) -> Vec<String> {
        paragraphs(&self.definition)
    }
}

pub fn split_list(s: &str) -> Vec<String> {
    s.split([',', '\n']).map(|a| a.trim().to_string()).filter(|a| !a.is_empty()).collect()
}

pub fn paragraphs(s: &str) -> Vec<String> {
    s.replace("\r\n", "\n").split("\n\n").map(|p| p.trim().to_string()).filter(|p| !p.is_empty()).collect()
}

pub async fn term_by_slug(pool: &PgPool, slug: &str, include_drafts: bool) -> anyhow::Result<Option<TermRow>> {
    let sql = format!("SELECT {TERM_COLS} FROM glossary_term WHERE slug = $1 AND ($2 OR status = 'published')");
    Ok(sqlx::query_as(&sql).bind(slug).bind(include_drafts).fetch_optional(pool).await?)
}

pub async fn term_by_id(pool: &PgPool, id: i32) -> anyhow::Result<Option<TermRow>> {
    let sql = format!("SELECT {TERM_COLS} FROM glossary_term WHERE id = $1");
    Ok(sqlx::query_as(&sql).bind(id).fetch_optional(pool).await?)
}

pub async fn all_terms(pool: &PgPool) -> anyhow::Result<Vec<TermRow>> {
    let sql = format!("SELECT {TERM_COLS} FROM glossary_term ORDER BY lower(name)");
    Ok(sqlx::query_as(&sql).fetch_all(pool).await?)
}

/// One line of the public index, whatever table it came from.
#[derive(Clone, Debug, Default)]
pub struct Entry {
    pub kind: String,
    pub kind_label: String,
    pub name: String,
    pub slug: String,
    pub url: String,
    pub topic: String,
    pub aliases: Vec<String>,
    pub definition: String,
    pub short: String,
    pub in_guide: bool,
    pub letter: String,
    /// Folded name, other spellings and topic, for the filter box.
    pub search: String,
    folded: Vec<String>,
}

#[derive(Clone, Debug, Default)]
pub struct Glossary {
    pub entries: Vec<Entry>,
    pub link_terms: Vec<LinkTerm>,
    /// Entries with a definition of his; the hub is only advertised once there is one.
    pub defined: usize,
}

impl Glossary {
    /// The entry a visitor's words name exactly (any spelling, singular or plural), if it has a definition.
    pub fn find(&self, q: &str) -> Option<&Entry> {
        let f = gloss::fold(q);
        if f.is_empty() || f.len() > 60 {
            return None;
        }
        let mut tries = vec![f.clone()];
        for suffix in ["es", "s"] {
            if let Some(stem) = f.strip_suffix(suffix) {
                tries.push(stem.to_string());
            }
        }
        tries.push(format!("{f}s"));
        self.entries.iter().filter(|e| !e.definition.is_empty()).find(|e| e.folded.iter().any(|n| tries.contains(n)))
    }
}

fn entry(kind: &str, name: &str, slug: &str, url: String, topic: &str, aliases: Vec<String>, definition: &str, in_guide: bool) -> Entry {
    let folded: Vec<String> = std::iter::once(name.to_string()).chain(aliases.iter().cloned()).map(|n| gloss::fold(&n)).filter(|n| !n.is_empty()).collect();
    let first = gloss::fold(name).chars().next().unwrap_or('#');
    Entry {
        kind: kind.to_string(),
        kind_label: kind_label(kind).to_string(),
        name: name.to_string(),
        slug: slug.to_string(),
        url,
        topic: topic.to_string(),
        short: text::summary(definition, 220),
        definition: definition.to_string(),
        in_guide,
        letter: if first.is_ascii_alphabetic() { first.to_ascii_uppercase().to_string() } else { "0–9".to_string() },
        search: format!("{} {}", folded.join(" "), gloss::fold(topic)),
        aliases,
        folded,
    }
}

pub async fn load(pool: &PgPool) -> anyhow::Result<Glossary> {
    let mut entries = vec![];
    let mut link_terms = vec![];
    let sql = format!("SELECT {TERM_COLS} FROM glossary_term WHERE status = 'published' AND definition <> ''");
    let rows: Vec<TermRow> = sqlx::query_as(&sql).fetch_all(pool).await?;
    for r in rows {
        let e = entry(&r.kind, &r.name, &r.slug, format!("/glossary/{}/", r.slug), &r.topic, r.alias_list(), &r.definition, r.in_guide);
        if r.link_in_listings {
            if let Some(matcher) = Matcher::new(&r.names(), r.kind == "model") {
                link_terms.push(LinkTerm { name: e.name.clone(), url: e.url.clone(), short: e.short.clone(), matcher });
            }
        }
        entries.push(e);
    }
    for (table, fk, kind, _, base) in TAXONOMIES {
        let sql = format!("SELECT t.slug, t.name, t.definition, (a.slug IS NOT NULL) AS approved, EXISTS (SELECT 1 FROM product p WHERE p.{fk} = t.id AND p.status = 'live') AS live FROM {table} t LEFT JOIN glossary_approved_def a ON a.tbl = $1 AND a.slug = t.slug");
        for r in sqlx::query(&sql).bind(table).fetch_all(pool).await? {
            let (slug, name, definition, approved, live): (String, String, String, bool, bool) = (r.get("slug"), r.get("name"), r.get("definition"), r.get("approved"), r.get("live"));
            let definition = if approved { definition } else { String::new() };
            if definition.is_empty() && !live {
                continue;
            }
            let name = text::decode(&name);
            let e = entry(kind, &name, &slug, format!("{base}{slug}/"), "", vec![], &definition, true);
            // Makes and eras are already links on every listing; nibs and filling systems are the jargon.
            if !definition.is_empty() && matches!(kind, "nib" | "filling") {
                if let Some(matcher) = Matcher::new(std::slice::from_ref(&name), false) {
                    link_terms.push(LinkTerm { name: e.name.clone(), url: e.url.clone(), short: e.short.clone(), matcher });
                }
            }
            entries.push(e);
        }
    }
    entries.sort_by(|a, b| a.folded.first().cmp(&b.folded.first()));
    let defined = entries.iter().filter(|e| !e.definition.is_empty()).count();
    Ok(Glossary { entries, link_terms, defined })
}

/// A lookup that found nothing, kept as a bare word and a count (no visitor, no time of day): the
/// list of entries his customers are waiting for. Letters, digits, spaces and hyphens only.
pub async fn record_miss(pool: &PgPool, q: &str) {
    // Same promise as the visit counter: a Do Not Track / Global Privacy Control browser, and the admin, count for nothing.
    if crate::insights::current_visitor().is_none() || crate::admin::current().is_some() {
        return;
    }
    let word = gloss::fold(q);
    if word.len() < 3 || word.len() > 40 || word.split(' ').count() > 4 {
        return;
    }
    if let Err(e) = sqlx::query("INSERT INTO glossary_miss (word) VALUES ($1) ON CONFLICT (word) DO UPDATE SET hits = glossary_miss.hits + 1, last_seen = now()").bind(&word).execute(pool).await {
        tracing::warn!("glossary miss not recorded: {e}");
    }
}

// ---- finding entries in his own writing -------------------------------------------------------------

struct Doc {
    url: String,
    title: String,
    text: String,
    sentences: Vec<String>,
    is_post: bool,
}

#[derive(Debug, Default)]
pub struct MineReport {
    pub proposed: usize,
    pub quotes: usize,
}

async fn his_writing(pool: &PgPool) -> anyhow::Result<Vec<Doc>> {
    let mut docs = vec![];
    for r in sqlx::query("SELECT slug, title, body_html FROM post WHERE status = 'published'").fetch_all(pool).await? {
        let body = text::strip_tags(&r.get::<String, _>("body_html"));
        let title = text::decode(&r.get::<String, _>("title"));
        docs.push(Doc { url: format!("/{}/", r.get::<String, _>("slug")), sentences: gloss::sentences(&body), text: format!("{title}. {body}"), title, is_post: true });
    }
    for r in sqlx::query("SELECT slug, title, description_text FROM product WHERE status IN ('live', 'sold')").fetch_all(pool).await? {
        let body: String = r.get("description_text");
        let title = text::decode(&r.get::<String, _>("title"));
        docs.push(Doc { url: format!("/product/{}/", r.get::<String, _>("slug")), sentences: gloss::sentences(&body), text: format!("{title}. {body}"), title, is_post: false });
    }
    Ok(docs)
}

/// Read his posts and listings for the lexicon's words. A word he uses becomes a hidden draft with an
/// empty definition; every entry (found or hand-made) gets the sentences where he used it, best
/// explanations first. Existing entries are never overwritten, so this is safe to run again.
pub async fn mine(pool: &PgPool) -> anyhow::Result<MineReport> {
    let docs = his_writing(pool).await?;
    let lexicon = gloss::parse_lexicon(LEXICON)?;
    let mut report = MineReport::default();

    // Words the makes / nibs / filling mechanisms / eras already cover are theirs.
    let mut taken: HashSet<String> = HashSet::new();
    for (table, ..) in TAXONOMIES {
        for name in sqlx::query_scalar::<_, String>(&format!("SELECT name FROM {table}")).fetch_all(pool).await? {
            taken.insert(gloss::fold(&text::decode(&name)));
        }
    }
    let brands: HashSet<String> = sqlx::query_scalar::<_, String>("SELECT slug FROM brand").fetch_all(pool).await?.into_iter().collect();

    for t in &lexicon {
        if taken.contains(&gloss::fold(&t.name)) {
            continue;
        }
        let names: Vec<String> = std::iter::once(t.name.clone()).chain(t.aliases.iter().cloned()).collect();
        let Some(matcher) = Matcher::new(&names, t.kind == "model") else { continue };
        let mentions = docs.iter().filter(|d| matcher.is_match(&d.text)).count();
        if mentions < MIN_MENTIONS {
            continue;
        }
        let brand = if brands.contains(&t.brand) { t.brand.as_str() } else { "" };
        let done = sqlx::query("INSERT INTO glossary_term (slug, name, kind, topic, aliases, brand_slug, origin, mentions) VALUES ($1, $2, $3, $4, $5, $6, 'mined', $7) ON CONFLICT (slug) DO NOTHING")
            .bind(text::slugify(&t.name)).bind(&t.name).bind(&t.kind).bind(&t.topic).bind(t.aliases.join(", ")).bind(brand).bind(mentions as i32)
            .execute(pool).await?;
        report.proposed += done.rows_affected() as usize;
    }

    for term in all_terms(pool).await? {
        let Some(matcher) = Matcher::new(&term.names(), term.kind == "model") else { continue };
        let mentions = docs.iter().filter(|d| matcher.is_match(&d.text)).count();
        let mut found: Vec<(i32, &Doc, &String)> = vec![];
        for d in docs.iter().filter(|d| matcher.is_match(&d.text)) {
            for s in &d.sentences {
                let score = gloss::quote_score(s, &matcher);
                if score > 0 {
                    // His posts explain; his listings mostly mention.
                    found.push((score + if d.is_post { 4 } else { 0 }, d, s));
                }
            }
        }
        found.sort_by(|a, b| b.0.cmp(&a.0));
        let mut seen: HashSet<&String> = HashSet::new();
        found.retain(|(_, _, s)| seen.insert(*s));
        found.truncate(QUOTES_PER_TERM);
        let mut tx = pool.begin().await?;
        sqlx::query("UPDATE glossary_term SET mentions = $2 WHERE id = $1").bind(term.id).bind(mentions as i32).execute(&mut *tx).await?;
        sqlx::query("DELETE FROM glossary_quote WHERE term_id = $1").bind(term.id).execute(&mut *tx).await?;
        for (score, d, s) in &found {
            sqlx::query("INSERT INTO glossary_quote (term_id, quote, source_url, source_title, score) VALUES ($1, $2, $3, $4, $5)").bind(term.id).bind(s.as_str()).bind(&d.url).bind(&d.title).bind(score).execute(&mut *tx).await?;
        }
        tx.commit().await?;
        report.quotes += found.len();
    }
    Ok(report)
}

/// First run only, like the FAQ seed: propose entries from his writing so the admin is not an empty page.
pub async fn seed(pool: &PgPool) -> anyhow::Result<()> {
    let n: i64 = sqlx::query_scalar("SELECT count(*) FROM glossary_term").fetch_one(pool).await?;
    if n == 0 {
        let r = mine(pool).await?;
        tracing::info!(proposed = r.proposed, quotes = r.quotes, "glossary drafts proposed from his writing");
    }
    Ok(())
}
