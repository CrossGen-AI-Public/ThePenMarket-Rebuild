//! Load the engine's catalog snapshot from Postgres. Refreshed on a timer so the guide and the
//! homepage numbers follow the database without a restart.

use crate::engine::{Catalog, Pen};
use crate::media::variant_rel;
use crate::text;
use sqlx::{PgPool, Row};

pub async fn load(pool: &PgPool) -> anyhow::Result<Catalog> {
    let rows = sqlx::query(
        "SELECT p.sku, p.slug, p.title, p.short_title, p.status, p.price_cents, p.sale_price_cents, p.length_mm::float8 AS length_mm,
                p.listed_year, p.listed_month, p.description_text,
                c.name AS category, c.slug AS category_slug,
                COALESCE(b.name,'') AS brand, COALESCE(b.slug,'') AS brand_slug,
                COALESCE(e.name,'') AS era, COALESCE(e.slug,'') AS era_slug,
                COALESCE(n.name,'') AS nib, COALESCE(n.slug,'') AS nib_slug,
                COALESCE(f.name,'') AS mechanism, COALESCE(f.slug,'') AS mechanism_slug, COALESCE(f.repairable,false) AS repairable,
                (SELECT path FROM product_image i WHERE i.product_id = p.id ORDER BY position LIMIT 1) AS image,
                (SELECT has_480 FROM product_image i WHERE i.product_id = p.id ORDER BY position LIMIT 1) AS has_480
         FROM product p
         JOIN category c ON c.id = p.category_id
         LEFT JOIN brand b ON b.id = p.brand_id
         LEFT JOIN era e ON e.id = p.era_id
         LEFT JOIN nib n ON n.id = p.nib_id
         LEFT JOIN filling_mechanism f ON f.id = p.filling_mechanism_id
         ORDER BY p.listed_year DESC NULLS LAST, p.listed_month DESC NULLS LAST, p.wp_id DESC",
    )
    .fetch_all(pool)
    .await?;
    let pens = rows
        .iter()
        .map(|r| {
            let image: Option<String> = r.get("image");
            let has_480: Option<bool> = r.get("has_480");
            let image = image.map(|p| if has_480.unwrap_or(false) { format!("/media/{}", variant_rel(&p, 480)) } else { format!("/media/{p}") }).unwrap_or_default();
            let desc: String = r.get("description_text");
            Pen {
                sku: r.get("sku"),
                slug: r.get("slug"),
                title: r.get("title"),
                short_title: r.get("short_title"),
                category: r.get("category"),
                category_slug: r.get("category_slug"),
                brand: r.get("brand"),
                brand_slug: r.get("brand_slug"),
                era: r.get("era"),
                era_slug: r.get("era_slug"),
                nib: r.get("nib"),
                nib_slug: r.get("nib_slug"),
                mechanism: r.get("mechanism"),
                mechanism_slug: r.get("mechanism_slug"),
                repairable: r.get("repairable"),
                price_cents: r.get("price_cents"),
                sale_price_cents: r.get("sale_price_cents"),
                status: r.get("status"),
                length_mm: r.get("length_mm"),
                listed_year: r.get("listed_year"),
                listed_month: r.get("listed_month"),
                image,
                celluloid: desc.to_lowercase().contains("celluloid"),
                summary: text::summary(&desc, 240),
            }
        })
        .collect();
    Ok(Catalog { generated_at: chrono::Utc::now().to_rfc3339(), pens })
}
