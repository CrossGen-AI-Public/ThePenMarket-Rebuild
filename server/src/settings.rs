//! Switches the admin can flip, one JSONB row each in `site_setting`. Typed getters with the
//! default the site runs on until he changes anything.

use sqlx::{PgPool, Row};

/// Product id of the pen he chose for the homepage hero; absent = the newest pen.
pub const FEATURED_PRODUCT: &str = "featured_product_id";
/// "stock" (most pens first) or "popular" (most looked-at first). Pinned brands lead either way.
pub const BRAND_ORDER: &str = "brand_order";
/// Show the "Most Popular" homepage section once there is enough traffic to fill it.
pub const POPULAR_SECTION: &str = "popular_section";
/// The guided photo shoot, in order. He can reword or reorder it.
pub const SHOT_LIST: &str = "photo_shot_list";

/// His list of the ways people misspell a search ("Schaffer" for Sheaffer). Absent = the examples
/// from his e-mail of 2026-10-07 (`spelling::default_list`).
pub const SEARCH_SPELLINGS: &str = "search_spellings";
/// What the homepage hero shows when no experiment decides: "photo" (his real photograph of the
/// featured pen) or "3d" (the model). Absent = the photograph: on 2026-10-12 he and Brittany agreed
/// the generated model can get details wrong that pen people notice, so the 3D pen is opt-in.
pub const HERO_MODE: &str = "hero_mode";
/// His shipping rates for pens bought online (`shipping::ShippingOption` list). Absent = the three rates
/// from his e-mail of 2026-10-12 (`shipping::default_list`); once saved, his list rules, even empty.
pub const SHIPPING_OPTIONS: &str = "shipping_options";
/// The single flat rate this replaced (before 2026-10-12). Cleared when he saves the new list.
pub const SHIPPING_FLAT_OLD: &str = "shipping_flat_cents";

/// He said 14 photos a pen is more than enough; the uploader and the API both stop there.
pub const MAX_PHOTOS: i64 = 14;

pub const DEFAULT_SHOTS: [&str; 10] = [
    "Whole pen, capped",
    "Whole pen, uncapped",
    "Cap posted on the barrel",
    "Nib, from the top",
    "Nib, from the side",
    "Imprint on the barrel",
    "Clip and cap band",
    "Filling system",
    "Cap top and barrel end",
    "Any flaw worth showing",
];

pub async fn get(pool: &PgPool, key: &str) -> anyhow::Result<Option<serde_json::Value>> {
    let row = sqlx::query("SELECT value FROM site_setting WHERE key = $1").bind(key).fetch_optional(pool).await?;
    Ok(row.map(|r| r.get("value")))
}

pub async fn set(pool: &PgPool, key: &str, value: serde_json::Value) -> anyhow::Result<()> {
    sqlx::query("INSERT INTO site_setting (key, value) VALUES ($1, $2) ON CONFLICT (key) DO UPDATE SET value = EXCLUDED.value, updated_at = now()").bind(key).bind(value).execute(pool).await?;
    Ok(())
}

pub async fn clear(pool: &PgPool, key: &str) -> anyhow::Result<()> {
    sqlx::query("DELETE FROM site_setting WHERE key = $1").bind(key).execute(pool).await?;
    Ok(())
}

pub async fn featured_product(pool: &PgPool) -> anyhow::Result<Option<i32>> {
    Ok(get(pool, FEATURED_PRODUCT).await?.and_then(|v| v.as_i64()).map(|v| v as i32))
}

pub async fn brand_order(pool: &PgPool) -> anyhow::Result<String> {
    let v = get(pool, BRAND_ORDER).await?.and_then(|v| v.as_str().map(str::to_string)).unwrap_or_default();
    Ok(if v == "popular" { v } else { "stock".into() })
}

pub async fn popular_section(pool: &PgPool) -> anyhow::Result<bool> {
    Ok(get(pool, POPULAR_SECTION).await?.and_then(|v| v.as_bool()).unwrap_or(true))
}

pub async fn search_spellings(pool: &PgPool) -> anyhow::Result<Vec<crate::spelling::Spelling>> {
    Ok(match get(pool, SEARCH_SPELLINGS).await? {
        Some(v) => serde_json::from_value(v).unwrap_or_default(),
        None => crate::spelling::default_list(),
    })
}

pub async fn hero_photo(pool: &PgPool) -> anyhow::Result<bool> {
    Ok(get(pool, HERO_MODE).await?.and_then(|v| v.as_str().map(|s| s != "3d")).unwrap_or(true))
}

/// His shipping options, and whether he has saved his own list (false = the defaults from his e-mail).
pub async fn shipping_options(pool: &PgPool) -> anyhow::Result<(Vec<thepenmarket::shipping::ShippingOption>, bool)> {
    Ok(match get(pool, SHIPPING_OPTIONS).await? {
        Some(v) => (serde_json::from_value(v).unwrap_or_default(), true),
        None => (thepenmarket::shipping::default_list(), false),
    })
}

pub async fn shot_list(pool: &PgPool) -> anyhow::Result<Vec<String>> {
    let saved: Vec<String> = get(pool, SHOT_LIST).await?.and_then(|v| serde_json::from_value(v).ok()).unwrap_or_default();
    Ok(if saved.is_empty() { DEFAULT_SHOTS.iter().map(|s| s.to_string()).collect() } else { saved })
}

/// One shot per line, trimmed, bounded, never more than a pen can hold.
pub fn parse_shot_list(text: &str) -> Vec<String> {
    text.lines().map(|l| l.trim().chars().take(60).collect::<String>()).filter(|l| !l.is_empty()).take(MAX_PHOTOS as usize).collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn shot_list_is_bounded() {
        let many = (0..40).map(|i| format!(" shot {i} ")).collect::<Vec<_>>().join("\n\n");
        let got = parse_shot_list(&many);
        assert_eq!(got.len(), MAX_PHOTOS as usize);
        assert_eq!(got[0], "shot 0");
    }
}
