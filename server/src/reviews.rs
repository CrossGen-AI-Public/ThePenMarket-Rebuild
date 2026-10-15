//! Google reviews. Nathaniel has no Google reviews yet, so this does two jobs: once his Google
//! Business Profile's Place ID is saved in the admin, every page links "Review us on Google"; and
//! with a Places API key (`GOOGLE_PLACES_KEY`) the server fetches his rating and the reviews Google
//! returns (five at most) once a day and the homepage shows them, word for word, with Google's
//! attribution. Nothing is ever shown that Google did not return: no reviews, no section.

use crate::app::AppState;
use serde::{Deserialize, Serialize};

pub const PLACE_ID: &str = "google_place_id";
pub const CACHE: &str = "google_reviews_cache";

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct Review {
    pub author: String,
    pub when: String,
    pub stars: i64,
    pub text: String,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct Reviews {
    pub rating: f64,
    pub count: i64,
    pub maps_url: String,
    pub reviews: Vec<Review>,
    #[serde(default)]
    pub fetched_at: String,
}

impl Reviews {
    pub fn rating_label(&self) -> String {
        format!("{:.1}", self.rating)
    }
}

/// A Place ID is letters, digits, dashes and underscores (they start "ChIJ" for a business).
pub fn valid_place_id(s: &str) -> bool {
    (10..=200).contains(&s.len()) && s.chars().all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_')
}

pub fn write_review_url(place_id: &str) -> String {
    format!("https://search.google.com/local/writereview?placeid={place_id}")
}

pub async fn place_id(pool: &sqlx::PgPool) -> anyhow::Result<String> {
    Ok(crate::settings::get(pool, PLACE_ID).await?.and_then(|v| v.as_str().map(str::to_string)).filter(|s| valid_place_id(s)).unwrap_or_default())
}

pub async fn cached(pool: &sqlx::PgPool) -> anyhow::Result<Option<Reviews>> {
    Ok(crate::settings::get(pool, CACHE).await?.and_then(|v| serde_json::from_value(v).ok()))
}

fn parse(j: &serde_json::Value) -> Reviews {
    let reviews = j
        .get("reviews")
        .and_then(|r| r.as_array())
        .map(|a| {
            a.iter()
                .filter_map(|r| {
                    let text = r.get("originalText").or_else(|| r.get("text")).and_then(|t| t.get("text")).and_then(|t| t.as_str())?.trim().to_string();
                    if text.is_empty() {
                        return None;
                    }
                    Some(Review {
                        author: r.get("authorAttribution").and_then(|a| a.get("displayName")).and_then(|n| n.as_str()).unwrap_or("A Google user").chars().take(80).collect(),
                        when: r.get("relativePublishTimeDescription").and_then(|n| n.as_str()).unwrap_or("").to_string(),
                        stars: r.get("rating").and_then(|n| n.as_i64()).unwrap_or(0).clamp(0, 5),
                        text: text.chars().take(1200).collect(),
                    })
                })
                .take(5)
                .collect()
        })
        .unwrap_or_default();
    Reviews { rating: j.get("rating").and_then(|n| n.as_f64()).unwrap_or(0.0), count: j.get("userRatingCount").and_then(|n| n.as_i64()).unwrap_or(0), maps_url: j.get("googleMapsUri").and_then(|n| n.as_str()).filter(|u| u.starts_with("https://")).unwrap_or("").to_string(), reviews, fetched_at: chrono::Utc::now().to_rfc3339() }
}

/// Fetch from the Places API (New) and cache. Does nothing without both a key and a Place ID.
pub async fn refresh(state: &AppState) -> anyhow::Result<bool> {
    let id = place_id(&state.pool).await?;
    if state.cfg.google_places_key.is_empty() || id.is_empty() {
        return Ok(false);
    }
    let res = state.http.get(format!("https://places.googleapis.com/v1/places/{id}")).header("X-Goog-Api-Key", &state.cfg.google_places_key).header("X-Goog-FieldMask", "rating,userRatingCount,googleMapsUri,reviews").send().await?.error_for_status()?;
    let j: serde_json::Value = res.json().await?;
    crate::settings::set(&state.pool, CACHE, serde_json::to_value(parse(&j))?).await?;
    Ok(true)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn parses_what_google_returns_and_nothing_else() {
        let j = serde_json::json!({"rating": 4.8, "userRatingCount": 12, "googleMapsUri": "https://maps.google.com/?cid=1", "reviews": [
            {"rating": 5, "relativePublishTimeDescription": "a month ago", "originalText": {"text": " Great pen. "}, "authorAttribution": {"displayName": "Pat"}},
            {"rating": 4, "text": {"text": ""}}]});
        let r = parse(&j);
        assert_eq!((r.count, r.reviews.len(), r.rating_label().as_str()), (12, 1, "4.8"));
        assert_eq!((r.reviews[0].author.as_str(), r.reviews[0].text.as_str(), r.reviews[0].stars), ("Pat", "Great pen.", 5));
        assert!(valid_place_id("ChIJN1t_tDeuEmsRUsoyG83frY4") && !valid_place_id("bad id!") && !valid_place_id("short"));
    }
}
