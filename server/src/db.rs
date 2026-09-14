//! All SQL lives here: row structs and the queries the routes need. Parameterised only.

use crate::media::variant_rel;
use std::path::PathBuf;
use std::sync::OnceLock;

static MEDIA_DIR: OnceLock<PathBuf> = OnceLock::new();
pub fn set_media_dir(p: PathBuf) {
    let _ = MEDIA_DIR.set(p);
}
/// `/media/<variant>` when the resized file exists, else the original.
fn media_url(rel: &str, width: u32) -> String {
    let v = variant_rel(rel, width);
    match MEDIA_DIR.get() {
        Some(dir) if dir.join(&v).exists() => format!("/media/{v}"),
        _ => format!("/media/{rel}"),
    }
}
use crate::money::fmt_cents;
use sqlx::{FromRow, PgPool, Postgres, QueryBuilder, Row};

#[derive(Clone, Debug, FromRow)]
pub struct Term {
    pub id: i32,
    pub slug: String,
    pub name: String,
    pub definition: String,
    pub sort_order: i32,
}

#[derive(Clone, Debug, FromRow)]
pub struct Category {
    pub id: i32,
    pub slug: String,
    pub name: String,
    pub intro: String,
    pub meta_description: String,
    pub sort_order: i32,
}

#[derive(Clone, Debug, FromRow)]
pub struct PriceRange {
    pub id: i32,
    pub slug: String,
    pub name: String,
    pub min_cents: i64,
    pub max_cents: Option<i64>,
}

/// A product as the pages show it: strings pre-formatted, image paths resolved.
#[derive(Clone, Debug, Default)]
pub struct ProductCard {
    pub id: i32,
    pub sku: String,
    pub slug: String,
    pub url: String,
    pub title: String,
    pub short_title: String,
    pub category: String,
    pub category_slug: String,
    pub brand: String,
    pub brand_slug: String,
    pub era: String,
    pub era_short: String,
    pub era_slug: String,
    pub nib: String,
    pub nib_slug: String,
    pub mechanism: String,
    pub mechanism_slug: String,
    pub price: String,
    pub price_cents: i64,
    pub sale_price: String,
    pub on_sale: bool,
    pub status: String,
    pub sold: bool,
    pub length_cm: String,
    pub listed: String,
    pub image: String,
    pub image_alt: String,
    pub is_subscription: bool,
}

#[derive(FromRow)]
struct ProductRow {
    id: i32,
    sku: String,
    slug: String,
    title: String,
    short_title: String,
    category: String,
    category_slug: String,
    brand: Option<String>,
    brand_slug: Option<String>,
    era: Option<String>,
    era_slug: Option<String>,
    nib: Option<String>,
    nib_slug: Option<String>,
    mechanism: Option<String>,
    mechanism_slug: Option<String>,
    price_cents: i64,
    sale_price_cents: Option<i64>,
    status: String,
    length_mm: Option<f64>,
    listed_year: Option<i32>,
    listed_month: Option<i32>,
    is_subscription: bool,
    image: Option<String>,
    image_alt: Option<String>,
    has_480: Option<bool>,
}

const MONTHS: [&str; 12] = ["Jan", "Feb", "Mar", "Apr", "May", "Jun", "Jul", "Aug", "Sep", "Oct", "Nov", "Dec"];

pub fn listed_label(y: Option<i32>, m: Option<i32>) -> String {
    match (y, m) {
        (Some(y), Some(m)) if (1..=12).contains(&m) => format!("{} ’{:02}", MONTHS[(m - 1) as usize].to_uppercase(), y % 100),
        _ => String::new(),
    }
}

pub fn era_short(era: &str) -> String {
    match era {
        "Pre-1900" => "Pre-1900".into(),
        "1900-1919" => "1900s–10s".into(),
        "1920-1929" => "1920s".into(),
        "1930-1939" => "1930s".into(),
        "1940-1949" => "1940s".into(),
        "1950-1959" => "1950s".into(),
        "1960-1979" => "1960s–70s".into(),
        "1980-Present" => "Modern".into(),
        other => other.to_string(),
    }
}

impl From<ProductRow> for ProductCard {
    fn from(r: ProductRow) -> Self {
        let image = r.image.map(|p| if r.has_480.unwrap_or(false) { format!("/media/{}", variant_rel(&p, 480)) } else { format!("/media/{p}") }).unwrap_or_default();
        ProductCard {
            id: r.id,
            sku: r.sku,
            url: format!("/product/{}/", r.slug),
            slug: r.slug,
            title: r.title,
            short_title: r.short_title.clone(),
            category: r.category,
            category_slug: r.category_slug,
            brand: r.brand.unwrap_or_default(),
            brand_slug: r.brand_slug.unwrap_or_default(),
            era_short: era_short(r.era.as_deref().unwrap_or("")),
            era: r.era.unwrap_or_default(),
            era_slug: r.era_slug.unwrap_or_default(),
            nib: r.nib.unwrap_or_default(),
            nib_slug: r.nib_slug.unwrap_or_default(),
            mechanism: r.mechanism.unwrap_or_default(),
            mechanism_slug: r.mechanism_slug.unwrap_or_default(),
            price: fmt_cents(r.price_cents),
            price_cents: r.sale_price_cents.unwrap_or(r.price_cents),
            sale_price: r.sale_price_cents.map(fmt_cents).unwrap_or_default(),
            on_sale: r.sale_price_cents.is_some(),
            sold: r.status == "sold",
            status: r.status,
            length_cm: r.length_mm.map(|mm| format!("{:.1} cm", mm / 10.0)).unwrap_or_default(),
            listed: listed_label(r.listed_year, r.listed_month),
            image,
            image_alt: r.image_alt.unwrap_or(r.short_title),
            is_subscription: r.is_subscription,
        }
    }
}

const PRODUCT_SELECT: &str = "SELECT p.id, p.sku, p.slug, p.title, p.short_title, p.status, p.price_cents, p.sale_price_cents, p.length_mm::float8 AS length_mm, p.listed_year, p.listed_month, p.is_subscription,
        c.name AS category, c.slug AS category_slug, b.name AS brand, b.slug AS brand_slug, e.name AS era, e.slug AS era_slug, n.name AS nib, n.slug AS nib_slug, f.name AS mechanism, f.slug AS mechanism_slug,
        (SELECT path FROM product_image i WHERE i.product_id = p.id AND i.archived_at IS NULL ORDER BY position LIMIT 1) AS image,
        (SELECT alt FROM product_image i WHERE i.product_id = p.id AND i.archived_at IS NULL ORDER BY position LIMIT 1) AS image_alt,
        (SELECT has_480 FROM product_image i WHERE i.product_id = p.id AND i.archived_at IS NULL ORDER BY position LIMIT 1) AS has_480
        FROM product p JOIN category c ON c.id = p.category_id LEFT JOIN brand b ON b.id = p.brand_id LEFT JOIN era e ON e.id = p.era_id LEFT JOIN nib n ON n.id = p.nib_id LEFT JOIN filling_mechanism f ON f.id = p.filling_mechanism_id";

#[derive(Clone, Debug, Default)]
pub struct ShopFilter {
    pub category: Option<String>,
    pub brand: Option<String>,
    pub era: Option<String>,
    pub nib: Option<String>,
    pub mechanism: Option<String>,
    pub price_range: Option<String>,
    pub min_cents: Option<i64>,
    pub max_cents: Option<i64>,
    pub q: Option<String>,
    pub on_sale: bool,
    pub include_sold: bool,
    pub sort: String,
    pub page: i64,
    pub per_page: i64,
}

impl ShopFilter {
    fn push_where(&self, qb: &mut QueryBuilder<'_, Postgres>, skip: &str) {
        qb.push(" WHERE c.slug <> 'memberships' ");
        if !self.include_sold {
            qb.push(" AND p.status = 'live' ");
        }
        if skip != "category" {
            if let Some(v) = &self.category {
                qb.push(" AND c.slug = ").push_bind(v.clone());
            }
        }
        if skip != "brand" {
            if let Some(v) = &self.brand {
                qb.push(" AND b.slug = ").push_bind(v.clone());
            }
        }
        if skip != "era" {
            if let Some(v) = &self.era {
                qb.push(" AND e.slug = ").push_bind(v.clone());
            }
        }
        if skip != "nib" {
            if let Some(v) = &self.nib {
                qb.push(" AND n.slug = ").push_bind(v.clone());
            }
        }
        if skip != "mechanism" {
            if let Some(v) = &self.mechanism {
                qb.push(" AND f.slug = ").push_bind(v.clone());
            }
        }
        if skip != "price" {
            if let Some(v) = self.min_cents {
                qb.push(" AND COALESCE(p.sale_price_cents, p.price_cents) >= ").push_bind(v);
            }
            if let Some(v) = self.max_cents {
                qb.push(" AND COALESCE(p.sale_price_cents, p.price_cents) <= ").push_bind(v);
            }
        }
        if self.on_sale {
            qb.push(" AND p.sale_price_cents IS NOT NULL ");
        }
        if let Some(q) = self.q.as_deref().map(str::trim).filter(|q| !q.is_empty()) {
            qb.push(" AND (p.search_tsv @@ websearch_to_tsquery('english', ").push_bind(q.to_string()).push(") OR p.title ILIKE ").push_bind(format!("%{q}%")).push(" OR p.sku = ").push_bind(q.to_string()).push(") ");
        }
    }
}

#[derive(Clone, Debug)]
pub struct FacetOption {
    pub slug: String,
    pub name: String,
    pub count: i64,
    pub selected: bool,
}

#[derive(Clone, Debug, Default)]
pub struct Facets {
    pub categories: Vec<FacetOption>,
    pub brands: Vec<FacetOption>,
    pub eras: Vec<FacetOption>,
    pub nibs: Vec<FacetOption>,
    pub mechanisms: Vec<FacetOption>,
    pub prices: Vec<FacetOption>,
}

pub struct ShopPage {
    pub items: Vec<ProductCard>,
    pub total: i64,
    pub facets: Facets,
}

pub async fn shop(pool: &PgPool, f: &ShopFilter, ranges: &[PriceRange]) -> anyhow::Result<ShopPage> {
    // count
    let mut qb = QueryBuilder::new("SELECT count(*) AS n FROM product p JOIN category c ON c.id = p.category_id LEFT JOIN brand b ON b.id = p.brand_id LEFT JOIN era e ON e.id = p.era_id LEFT JOIN nib n ON n.id = p.nib_id LEFT JOIN filling_mechanism f ON f.id = p.filling_mechanism_id");
    f.push_where(&mut qb, "");
    let total: i64 = qb.build().fetch_one(pool).await?.get("n");

    let mut qb = QueryBuilder::new(PRODUCT_SELECT);
    f.push_where(&mut qb, "");
    qb.push(match f.sort.as_str() {
        "price-asc" => " ORDER BY COALESCE(p.sale_price_cents, p.price_cents) ASC, p.title ",
        "price-desc" => " ORDER BY COALESCE(p.sale_price_cents, p.price_cents) DESC, p.title ",
        "name" => " ORDER BY p.short_title ",
        _ => " ORDER BY p.status ASC, p.listed_year DESC NULLS LAST, p.listed_month DESC NULLS LAST, p.wp_id DESC ",
    });
    qb.push(" LIMIT ").push_bind(f.per_page).push(" OFFSET ").push_bind((f.page - 1).max(0) * f.per_page);
    let rows: Vec<ProductRow> = qb.build_query_as().fetch_all(pool).await?;
    let items = rows.into_iter().map(ProductCard::from).collect();

    let mut facets = Facets::default();
    for (dim, table_alias, target) in [("category", "c", "categories"), ("brand", "b", "brands"), ("era", "e", "eras"), ("nib", "n", "nibs"), ("mechanism", "f", "mechanisms")] {
        let mut qb = QueryBuilder::new(format!(
            "SELECT {a}.slug AS slug, {a}.name AS name, count(*) AS n FROM product p JOIN category c ON c.id = p.category_id LEFT JOIN brand b ON b.id = p.brand_id LEFT JOIN era e ON e.id = p.era_id LEFT JOIN nib n ON n.id = p.nib_id LEFT JOIN filling_mechanism f ON f.id = p.filling_mechanism_id",
            a = table_alias
        ));
        f.push_where(&mut qb, dim);
        qb.push(format!(" AND {a}.slug IS NOT NULL GROUP BY {a}.slug, {a}.name, {a}.sort_order ORDER BY {a}.sort_order, {a}.name", a = table_alias));
        let rows = qb.build().fetch_all(pool).await?;
        let selected = match dim {
            "category" => f.category.clone(),
            "brand" => f.brand.clone(),
            "era" => f.era.clone(),
            "nib" => f.nib.clone(),
            _ => f.mechanism.clone(),
        };
        let opts: Vec<FacetOption> = rows
            .iter()
            .map(|r| {
                let slug: String = r.get("slug");
                FacetOption { selected: selected.as_deref() == Some(slug.as_str()), slug, name: r.get("name"), count: r.get("n") }
            })
            .collect();
        match target {
            "categories" => facets.categories = opts,
            "brands" => facets.brands = opts,
            "eras" => facets.eras = opts,
            "nibs" => facets.nibs = opts,
            _ => facets.mechanisms = opts,
        }
    }
    // price bands: his five ranges, counted over the other filters
    for pr in ranges {
        let mut qb = QueryBuilder::new("SELECT count(*) AS n FROM product p JOIN category c ON c.id = p.category_id LEFT JOIN brand b ON b.id = p.brand_id LEFT JOIN era e ON e.id = p.era_id LEFT JOIN nib n ON n.id = p.nib_id LEFT JOIN filling_mechanism f ON f.id = p.filling_mechanism_id");
        f.push_where(&mut qb, "price");
        qb.push(" AND COALESCE(p.sale_price_cents, p.price_cents) >= ").push_bind(pr.min_cents);
        if let Some(mx) = pr.max_cents {
            qb.push(" AND COALESCE(p.sale_price_cents, p.price_cents) <= ").push_bind(mx);
        }
        let n: i64 = qb.build().fetch_one(pool).await?.get("n");
        facets.prices.push(FacetOption { selected: f.price_range.as_deref() == Some(pr.slug.as_str()), slug: pr.slug.clone(), name: pr.name.clone(), count: n });
    }
    Ok(ShopPage { items, total, facets })
}

pub async fn products_by_slugs_or_recent(pool: &PgPool, limit: i64, exclude_ids: &[i32], only_live: bool, fountain_only: bool) -> anyhow::Result<Vec<ProductCard>> {
    let mut qb = QueryBuilder::new(PRODUCT_SELECT);
    qb.push(" WHERE c.slug IN ('vintage-pens','pre-owned-pens') ");
    if fountain_only {
        qb.push(" AND (f.slug IS NULL OR f.slug NOT IN ('ballpoint','rollerball','pencil','dip')) ");
    }
    if only_live {
        qb.push(" AND p.status = 'live' ");
    }
    if !exclude_ids.is_empty() {
        qb.push(" AND p.id <> ALL(").push_bind(exclude_ids.to_vec()).push(") ");
    }
    qb.push(" ORDER BY p.listed_year DESC NULLS LAST, p.listed_month DESC NULLS LAST, p.wp_id DESC LIMIT ").push_bind(limit);
    let rows: Vec<ProductRow> = qb.build_query_as().fetch_all(pool).await?;
    Ok(rows.into_iter().map(ProductCard::from).collect())
}

pub async fn top_priced(pool: &PgPool, limit: i64) -> anyhow::Result<Vec<ProductCard>> {
    let sql = format!("{PRODUCT_SELECT} WHERE p.status = 'live' AND c.slug <> 'memberships' ORDER BY COALESCE(p.sale_price_cents, p.price_cents) DESC LIMIT $1");
    let rows: Vec<ProductRow> = sqlx::query_as(&sql).bind(limit).fetch_all(pool).await?;
    Ok(rows.into_iter().map(ProductCard::from).collect())
}

pub async fn product_by_slug(pool: &PgPool, slug: &str) -> anyhow::Result<Option<ProductCard>> {
    let sql = format!("{PRODUCT_SELECT} WHERE p.slug = $1");
    let row: Option<ProductRow> = sqlx::query_as(&sql).bind(slug).fetch_optional(pool).await?;
    Ok(row.map(ProductCard::from))
}

#[derive(Clone, Debug, FromRow)]
pub struct ProductBody {
    pub description_html: String,
    pub description_text: String,
    pub meta_description: String,
    pub wp_id: Option<i32>,
}

pub async fn product_body(pool: &PgPool, id: i32) -> anyhow::Result<ProductBody> {
    Ok(sqlx::query_as("SELECT description_html, description_text, meta_description, wp_id FROM product WHERE id = $1").bind(id).fetch_one(pool).await?)
}

#[derive(Clone, Debug)]
pub struct Image {
    pub id: i32,
    pub full: String,
    pub large: String,
    pub thumb: String,
    pub alt: String,
    pub width: i32,
    pub height: i32,
}

pub async fn product_images(pool: &PgPool, id: i32) -> anyhow::Result<Vec<Image>> {
    let rows = sqlx::query("SELECT id, path, alt, width, height, has_480, has_960 FROM product_image WHERE product_id = $1 AND archived_at IS NULL ORDER BY position").bind(id).fetch_all(pool).await?;
    Ok(rows
        .iter()
        .map(|r| {
            let p: String = r.get("path");
            let has_480: bool = r.get("has_480");
            let has_960: bool = r.get("has_960");
            Image {
                id: r.get("id"),
                full: format!("/media/{p}"),
                large: if has_960 { format!("/media/{}", variant_rel(&p, 960)) } else { format!("/media/{p}") },
                thumb: if has_480 { format!("/media/{}", variant_rel(&p, 480)) } else { format!("/media/{p}") },
                alt: r.get("alt"),
                width: r.get::<Option<i32>, _>("width").unwrap_or(0),
                height: r.get::<Option<i32>, _>("height").unwrap_or(0),
            }
        })
        .collect())
}

pub async fn related(pool: &PgPool, id: i32, brand_slug: &str, category_slug: &str, limit: i64) -> anyhow::Result<Vec<ProductCard>> {
    let sql = format!("{PRODUCT_SELECT} WHERE p.id <> $1 AND p.status = 'live' AND c.slug <> 'memberships' AND (b.slug = $2 OR c.slug = $3) ORDER BY (b.slug = $2) DESC, p.listed_year DESC NULLS LAST, p.listed_month DESC NULLS LAST, p.wp_id DESC LIMIT $4");
    let rows: Vec<ProductRow> = sqlx::query_as(&sql).bind(id).bind(brand_slug).bind(category_slug).bind(limit).fetch_all(pool).await?;
    Ok(rows.into_iter().map(ProductCard::from).collect())
}

pub async fn terms(pool: &PgPool, table: &str) -> anyhow::Result<Vec<Term>> {
    let sql = format!("SELECT id, slug, name, definition, sort_order FROM {table} ORDER BY sort_order, name");
    Ok(sqlx::query_as(&sql).fetch_all(pool).await?)
}

pub async fn term_by_slug(pool: &PgPool, table: &str, slug: &str) -> anyhow::Result<Option<Term>> {
    let sql = format!("SELECT id, slug, name, definition, sort_order FROM {table} WHERE slug = $1");
    Ok(sqlx::query_as(&sql).bind(slug).fetch_optional(pool).await?)
}

pub async fn categories(pool: &PgPool) -> anyhow::Result<Vec<Category>> {
    Ok(sqlx::query_as("SELECT id, slug, name, intro, meta_description, sort_order FROM category ORDER BY sort_order").fetch_all(pool).await?)
}

pub async fn price_ranges(pool: &PgPool) -> anyhow::Result<Vec<PriceRange>> {
    Ok(sqlx::query_as("SELECT id, slug, name, min_cents, max_cents FROM price_range ORDER BY sort_order").fetch_all(pool).await?)
}

// ---------- blog ----------
#[derive(Clone, Debug, FromRow)]
pub struct PostRow {
    pub id: i32,
    pub slug: String,
    pub title: String,
    pub excerpt: String,
    pub meta_description: String,
    pub author_name: String,
    pub featured_path: Option<String>,
    pub featured_alt: String,
    pub published_at: chrono::DateTime<chrono::Utc>,
    pub modified_at: chrono::DateTime<chrono::Utc>,
    pub word_count: i32,
    pub category_name: Option<String>,
    pub category_slug: Option<String>,
}

#[derive(Clone, Debug)]
pub struct PostCard {
    pub slug: String,
    pub url: String,
    pub title: String,
    pub excerpt: String,
    pub date: String,
    pub iso_date: String,
    pub image: String,
    pub image_alt: String,
    pub category: String,
    pub category_slug: String,
    pub minutes: i32,
}

impl From<PostRow> for PostCard {
    fn from(r: PostRow) -> Self {
        PostCard {
            url: format!("/{}/", r.slug),
            slug: r.slug,
            title: r.title,
            excerpt: r.excerpt,
            date: r.published_at.format("%B %-d, %Y").to_string(),
            iso_date: r.published_at.to_rfc3339(),
            image: r.featured_path.map(|p| media_url(&p, 480)).unwrap_or_default(),
            image_alt: r.featured_alt,
            category: r.category_name.unwrap_or_default(),
            category_slug: r.category_slug.unwrap_or_default(),
            minutes: (r.word_count / 220).max(1),
        }
    }
}

const POST_SELECT: &str = "SELECT p.id, p.slug, p.title, p.excerpt, p.meta_description, p.author_name, p.featured_path, p.featured_alt, p.published_at, p.modified_at, p.word_count, bc.name AS category_name, bc.slug AS category_slug FROM post p LEFT JOIN blog_category bc ON bc.id = p.category_id";

pub async fn posts(pool: &PgPool, category: Option<&str>, q: Option<&str>, page: i64, per_page: i64) -> anyhow::Result<(Vec<PostCard>, i64)> {
    let mut count = QueryBuilder::new("SELECT count(*) AS n FROM post p LEFT JOIN blog_category bc ON bc.id = p.category_id WHERE 1=1 ");
    let mut list = QueryBuilder::new(POST_SELECT);
    list.push(" WHERE 1=1 ");
    for qb in [&mut count, &mut list] {
        if let Some(c) = category {
            qb.push(" AND bc.slug = ").push_bind(c.to_string());
        }
        if let Some(q) = q.map(str::trim).filter(|q| !q.is_empty()) {
            qb.push(" AND (p.search_tsv @@ websearch_to_tsquery('english', ").push_bind(q.to_string()).push(") OR p.title ILIKE ").push_bind(format!("%{q}%")).push(") ");
        }
    }
    let total: i64 = count.build().fetch_one(pool).await?.get("n");
    list.push(" ORDER BY p.published_at DESC LIMIT ").push_bind(per_page).push(" OFFSET ").push_bind((page - 1).max(0) * per_page);
    let rows: Vec<PostRow> = list.build_query_as().fetch_all(pool).await?;
    Ok((rows.into_iter().map(PostCard::from).collect(), total))
}

pub async fn post_by_slug(pool: &PgPool, slug: &str) -> anyhow::Result<Option<(PostRow, String)>> {
    let sql = format!("{POST_SELECT} WHERE p.slug = $1");
    let row: Option<PostRow> = sqlx::query_as(&sql).bind(slug).fetch_optional(pool).await?;
    match row {
        Some(r) => {
            let body: String = sqlx::query("SELECT body_html FROM post WHERE id = $1").bind(r.id).fetch_one(pool).await?.get("body_html");
            Ok(Some((r, body)))
        }
        None => Ok(None),
    }
}

pub async fn posts_by_slugs(pool: &PgPool, slugs: &[&str]) -> anyhow::Result<Vec<PostCard>> {
    let sql = format!("{POST_SELECT} WHERE p.slug = ANY($1) ORDER BY p.published_at DESC");
    let owned: Vec<String> = slugs.iter().map(|s| s.to_string()).collect();
    let rows: Vec<PostRow> = sqlx::query_as(&sql).bind(owned).fetch_all(pool).await?;
    Ok(rows.into_iter().map(PostCard::from).collect())
}

pub async fn posts_matching(pool: &PgPool, q: &str, limit: i64) -> anyhow::Result<Vec<PostCard>> {
    let sql = format!("{POST_SELECT} WHERE p.search_tsv @@ websearch_to_tsquery('english', $1) OR p.title ILIKE $2 ORDER BY ts_rank(p.search_tsv, websearch_to_tsquery('english', $1)) DESC, p.published_at DESC LIMIT $3");
    let rows: Vec<PostRow> = sqlx::query_as(&sql).bind(q).bind(format!("%{q}%")).bind(limit).fetch_all(pool).await?;
    Ok(rows.into_iter().map(PostCard::from).collect())
}

#[derive(Clone, Debug, FromRow)]
pub struct BlogCategory {
    pub slug: String,
    pub name: String,
    pub description: String,
    pub count: i64,
}

pub async fn blog_categories(pool: &PgPool) -> anyhow::Result<Vec<BlogCategory>> {
    Ok(sqlx::query_as("SELECT bc.slug, bc.name, bc.description, (SELECT count(*) FROM post p WHERE p.category_id = bc.id) AS count FROM blog_category bc WHERE bc.slug <> 'uncategorized-new' ORDER BY count DESC").fetch_all(pool).await?)
}

pub async fn blog_category(pool: &PgPool, slug: &str) -> anyhow::Result<Option<BlogCategory>> {
    Ok(sqlx::query_as("SELECT bc.slug, bc.name, bc.description, (SELECT count(*) FROM post p WHERE p.category_id = bc.id) AS count FROM blog_category bc WHERE bc.slug = $1").bind(slug).fetch_optional(pool).await?)
}

// ---------- pages ----------
#[derive(Clone, Debug, FromRow)]
pub struct Page {
    pub slug: String,
    pub title: String,
    pub body_html: String,
    pub meta_description: String,
    pub modified_at: chrono::DateTime<chrono::Utc>,
}

pub async fn page(pool: &PgPool, slug: &str) -> anyhow::Result<Option<Page>> {
    Ok(sqlx::query_as("SELECT slug, title, body_html, meta_description, modified_at FROM page WHERE slug = $1").bind(slug).fetch_optional(pool).await?)
}

// ---------- trading post ----------
#[derive(Clone, Debug, FromRow)]
pub struct ListingRow {
    pub slug: String,
    pub title: String,
    pub era_text: String,
    pub price_text: String,
    pub price_cents: Option<i64>,
    pub contact_text: String,
    pub description_html: String,
    pub image_path: Option<String>,
    pub image_alt: String,
    pub published_at: chrono::DateTime<chrono::Utc>,
    pub modified_at: chrono::DateTime<chrono::Utc>,
}

#[derive(Clone, Debug)]
pub struct Listing {
    pub slug: String,
    pub url: String,
    pub title: String,
    pub era: String,
    pub price: String,
    pub contact: String,
    pub description_html: String,
    pub image: String,
    pub image_large: String,
    pub image_alt: String,
    pub date: String,
    pub iso_date: String,
    pub iso_modified: String,
}

impl From<ListingRow> for Listing {
    fn from(r: ListingRow) -> Self {
        let price = r.price_cents.map(fmt_cents).unwrap_or_else(|| r.price_text.clone());
        Listing {
            url: format!("/trading-post/{}/", r.slug),
            slug: r.slug,
            title: r.title,
            era: r.era_text,
            price,
            contact: r.contact_text,
            description_html: r.description_html,
            image: r.image_path.as_ref().map(|p| media_url(p, 480)).unwrap_or_default(),
            image_large: r.image_path.map(|p| media_url(&p, 960)).unwrap_or_default(),
            image_alt: r.image_alt,
            date: r.published_at.format("%B %-d, %Y").to_string(),
            iso_date: r.published_at.to_rfc3339(),
            iso_modified: r.modified_at.to_rfc3339(),
        }
    }
}

pub async fn listings(pool: &PgPool, page: i64, per_page: i64) -> anyhow::Result<(Vec<Listing>, i64)> {
    let total: i64 = sqlx::query("SELECT count(*) AS n FROM tp_listing WHERE status = 'live'").fetch_one(pool).await?.get("n");
    let rows: Vec<ListingRow> = sqlx::query_as("SELECT slug, title, era_text, price_text, price_cents, contact_text, description_html, image_path, image_alt, published_at, modified_at FROM tp_listing WHERE status = 'live' ORDER BY published_at DESC LIMIT $1 OFFSET $2")
        .bind(per_page)
        .bind((page - 1).max(0) * per_page)
        .fetch_all(pool)
        .await?;
    Ok((rows.into_iter().map(Listing::from).collect(), total))
}

pub async fn listing(pool: &PgPool, slug: &str) -> anyhow::Result<Option<Listing>> {
    let row: Option<ListingRow> = sqlx::query_as("SELECT slug, title, era_text, price_text, price_cents, contact_text, description_html, image_path, image_alt, published_at, modified_at FROM tp_listing WHERE slug = $1").bind(slug).fetch_optional(pool).await?;
    Ok(row.map(Listing::from))
}

// ---------- redirects and forms ----------
pub async fn redirect_for(pool: &PgPool, path: &str) -> anyhow::Result<Option<String>> {
    let alt = if path.ends_with('/') { path.trim_end_matches('/').to_string() } else { format!("{path}/") };
    let row = sqlx::query("UPDATE redirect SET hits = hits + 1 WHERE old_path = $1 OR old_path = $2 RETURNING new_path").bind(path).bind(alt).fetch_optional(pool).await?;
    Ok(row.map(|r| r.get("new_path")))
}

pub async fn save_form(pool: &PgPool, kind: &str, fields: serde_json::Value, photo_key: Option<String>) -> anyhow::Result<i32> {
    let row = sqlx::query("INSERT INTO form_submission (kind, fields, photo_key) VALUES ($1, $2, $3) RETURNING id").bind(kind).bind(fields).bind(photo_key).fetch_one(pool).await?;
    Ok(row.get("id"))
}

// ---------- sitemaps ----------
pub async fn sitemap_products(pool: &PgPool) -> anyhow::Result<Vec<(String, String)>> {
    let rows = sqlx::query("SELECT slug, updated_at FROM product WHERE status IN ('live', 'sold') ORDER BY slug").fetch_all(pool).await?;
    Ok(rows.iter().map(|r| (format!("/product/{}/", r.get::<String, _>("slug")), r.get::<chrono::DateTime<chrono::Utc>, _>("updated_at").format("%Y-%m-%d").to_string())).collect())
}

pub async fn sitemap_posts(pool: &PgPool) -> anyhow::Result<Vec<(String, String)>> {
    let rows = sqlx::query("SELECT slug, modified_at FROM post ORDER BY published_at DESC").fetch_all(pool).await?;
    Ok(rows.iter().map(|r| (format!("/{}/", r.get::<String, _>("slug")), r.get::<chrono::DateTime<chrono::Utc>, _>("modified_at").format("%Y-%m-%d").to_string())).collect())
}

pub async fn sitemap_listings(pool: &PgPool) -> anyhow::Result<Vec<(String, String)>> {
    let rows = sqlx::query("SELECT slug, modified_at FROM tp_listing WHERE status = 'live' ORDER BY published_at DESC").fetch_all(pool).await?;
    Ok(rows.iter().map(|r| (format!("/trading-post/{}/", r.get::<String, _>("slug")), r.get::<chrono::DateTime<chrono::Utc>, _>("modified_at").format("%Y-%m-%d").to_string())).collect())
}

pub async fn term_slugs_with_products(pool: &PgPool, table: &str, fk: &str) -> anyhow::Result<Vec<String>> {
    let sql = format!("SELECT t.slug FROM {table} t WHERE EXISTS (SELECT 1 FROM product p WHERE p.{fk} = t.id AND p.status IN ('live', 'sold')) ORDER BY t.sort_order, t.name");
    let rows = sqlx::query(&sql).fetch_all(pool).await?;
    Ok(rows.iter().map(|r| r.get::<String, _>("slug")).collect())
}
