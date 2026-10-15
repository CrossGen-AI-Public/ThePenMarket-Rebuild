//! Addresses from the sites before the WordPress one, still linked from his blog posts and from
//! other people's pages: the ASP.NET shop (`productList.aspx`, `product-detail.aspx`, `new-pens.aspx`,
//! `TradingPostList.aspx`) and WordPress's own dated permalinks (`/2013/10/slug/`). The redirect table
//! covers what the last sitemaps listed; these shapes were never in a sitemap, so they are recognised
//! by pattern and sent to the closest page that exists today. Checked after the redirect table, so an
//! address Nathaniel maps himself in /admin/redirects/ always wins.

use sqlx::PgPool;

fn norm(s: &str) -> String {
    s.chars().filter(|c| c.is_ascii_alphanumeric()).collect::<String>().to_lowercase()
}

fn words(s: &str) -> String {
    s.split(|c: char| !c.is_ascii_alphanumeric()).filter(|w| !w.is_empty()).collect::<Vec<_>>().join(" ")
}

fn search(q: &str) -> String {
    format!("/shop/?q={}", urlencoding(q))
}

fn urlencoding(s: &str) -> String {
    s.bytes()
        .map(|b| match b {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => (b as char).to_string(),
            b' ' => "+".into(),
            _ => format!("%{b:02X}"),
        })
        .collect()
}

/// A brand by its old-site name ("Wahl--Eversharp", "Mont-Blanc", "Parker"), compared letters-only.
async fn brand(pool: &PgPool, name: &str) -> anyhow::Result<Option<String>> {
    let key = norm(name);
    if key.is_empty() {
        return Ok(None);
    }
    let rows: Vec<(String, String)> = sqlx::query_as("SELECT slug, name FROM brand").fetch_all(pool).await?;
    Ok(rows.into_iter().find(|(slug, n)| norm(slug) == key || norm(n) == key).map(|(slug, _)| format!("/brand/{slug}/")))
}

async fn mechanism(pool: &PgPool, name: &str) -> anyhow::Result<Option<String>> {
    let key = norm(name);
    let rows: Vec<String> = sqlx::query_scalar("SELECT slug FROM filling_mechanism").fetch_all(pool).await?;
    Ok(rows.into_iter().find(|slug| !key.is_empty() && norm(slug).starts_with(&key)).map(|slug| format!("/pw-filling-mechanism/{slug}/")))
}

/// "vintage-pens-parker-duofold" or "used-cross-verve": that pen if it is still in the catalog under
/// the same name (the importer kept his slugs), otherwise a search for its words.
async fn product(pool: &PgPool, old_slug: &str) -> anyhow::Result<String> {
    let s = old_slug.to_lowercase();
    let candidates = [s.clone(), s.replacen("used-", "pre-owned-pens-", 1)];
    for c in &candidates {
        let hit: Option<String> = sqlx::query_scalar("SELECT slug FROM product WHERE (slug = $1 OR slug LIKE $1 || '-_') AND status IN ('live', 'sold') ORDER BY (slug = $1) DESC, status LIMIT 1")
            .bind(c)
            .fetch_optional(pool)
            .await?;
        if let Some(slug) = hit {
            return Ok(format!("/product/{slug}/"));
        }
    }
    let q = words(&s.replacen("vintage-pens-", "", 1).replacen("used-", "", 1));
    Ok(search(&q))
}

/// Where an address from an older site belongs now, or None when it is not one of those shapes.
pub async fn resolve(pool: &PgPool, path: &str) -> anyhow::Result<Option<String>> {
    let lower = path.to_lowercase();
    let parts: Vec<&str> = path.split('/').filter(|p| !p.is_empty()).collect();
    let lparts: Vec<String> = parts.iter().map(|p| p.to_lowercase()).collect();

    // WordPress dated permalinks: /2013/10/some-post/ and month archives /2016/05/.
    if lparts.len() >= 2 && lparts[0].len() == 4 && lparts[0].chars().all(|c| c.is_ascii_digit()) && lparts[1].len() == 2 && lparts[1].chars().all(|c| c.is_ascii_digit()) {
        if let Some(slug) = parts.get(2) {
            let exists: bool = sqlx::query_scalar("SELECT EXISTS (SELECT 1 FROM post WHERE slug = $1 AND status = 'published')").bind(slug.to_lowercase()).fetch_one(pool).await?;
            return Ok(Some(if exists { format!("/{}/", slug.to_lowercase()) } else { format!("/blog/?q={}", urlencoding(&words(slug))) }));
        }
        return Ok(Some("/blog/".into()));
    }
    if lower == "/on-sale/" || lower == "/on-sale" {
        return Ok(Some("/on-sale-pens/".into()));
    }

    let Some(first) = lparts.first() else { return Ok(None) };
    // Drop "page-2" style paging; the new lists page themselves.
    let rest: Vec<&str> = parts[1..].iter().copied().filter(|p| !(p.to_lowercase().starts_with("page-") && p[5..].chars().all(|c| c.is_ascii_digit()))).collect();
    match first.as_str() {
        "productlist.aspx" => {
            let a = rest.first().map(|s| s.to_lowercase()).unwrap_or_default();
            let b = rest.get(1).copied().unwrap_or("");
            Ok(Some(match a.as_str() {
                "" | "latest-pen-pencil-items" => "/shop/".into(),
                "vintage-pens" | "old-pens" if b.is_empty() => "/product-category/vintage-pens/".into(),
                "pre-owned-pens" | "used-pens" if b.is_empty() => "/product-category/pre-owned-pens/".into(),
                "pens-pencils-on-sale" | "on-sale" => "/on-sale-pens/".into(),
                "inkwells-and-blotters" => "/product-category/inkwells-blotters/".into(),
                "pencils" => "/product-category/pencils/".into(),
                "filling-mechanism" => match mechanism(pool, b).await? {
                    Some(u) => u,
                    None => search(&words(b)),
                },
                _ if b.to_lowercase().starts_with("brand-") => {
                    let name = b[6..].trim_end_matches("-pens").trim_end_matches("-Pens");
                    match brand(pool, name).await? {
                        Some(u) => u,
                        None => search(&words(name)),
                    }
                }
                other => search(&words(other)),
            }))
        }
        "product-detail.aspx" => {
            // "821-0583-vintage-pens-mabie-todd-swan-eternal": an id, his SKU, then the name.
            let Some(tail) = rest.first() else { return Ok(Some("/shop/".into())) };
            let segs: Vec<&str> = tail.splitn(3, '-').collect();
            let name = if segs.len() == 3 && segs[0].chars().all(|c| c.is_ascii_digit()) { segs[2] } else { tail };
            Ok(Some(product(pool, name).await?))
        }
        "new-pens.aspx" => {
            // /new-pens.aspx/brand/1/lamy, /new-pens.aspx/1-1-2/lamy/safari/fountain-pen
            if rest.is_empty() {
                return Ok(Some("/shop/".into()));
            }
            let named: Vec<&str> = rest.iter().copied().filter(|p| !p.chars().all(|c| c.is_ascii_digit() || c == '-') && !p.eq_ignore_ascii_case("brand")).collect();
            if named.len() == 1 {
                if let Some(u) = brand(pool, named[0]).await? {
                    return Ok(Some(u));
                }
            }
            Ok(Some(search(&words(&named.join(" ")))))
        }
        "tradingpostlist.aspx" | "tradingpost.aspx" | "tradingpostdetail.aspx" => Ok(Some("/trading-post/".into())),
        _ if first.ends_with(".aspx") => Ok(Some("/".into())),
        _ => Ok(None),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn helpers() {
        assert_eq!(norm("Wahl--Eversharp"), "wahleversharp");
        assert_eq!(words("pencil-(0.5mm)"), "pencil 0 5mm");
        assert_eq!(search("lamy safari"), "/shop/?q=lamy+safari");
    }
}
