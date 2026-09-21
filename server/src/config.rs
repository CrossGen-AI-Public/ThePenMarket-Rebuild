//! Runtime configuration. Secrets come from the environment (systemd EnvironmentFile or
//! `~/.config/thepenmarket.env`); nothing here is ever printed.

use std::path::PathBuf;

#[derive(Clone, Debug)]
pub struct Config {
    pub database_url: String,
    pub host: String,
    pub port: u16,
    /// Public origin used in canonical URLs, sitemaps and JSON-LD, e.g. `http://100.117.164.79:8140`.
    pub site_origin: String,
    pub media_dir: PathBuf,
    pub static_dir: PathBuf,
    pub uploads_dir: PathBuf,
    pub csrf_secret: String,
    pub guide_ai_url: String,
    pub guide_ai_key: String,
    pub guide_ai_model: String,
    pub guide_rate_per_min: u32,
    /// Where lockout alerts go besides the admin (CrossGen), e.g. brittany.iversen@crossgen-ai.com.
    pub admin_alert_email: String,
    /// Where the repair, sell and contact forms (and guide handoffs) are e-mailed. `FORMS_TO`.
    pub forms_to: String,
    /// Where Trading Post submissions are e-mailed. `TRADING_POST_TO`.
    pub trading_post_to: String,
    /// eBay Browse API keys for the sourcing watchlist (`EBAY_APP_ID`, `EBAY_CERT_ID`); empty = the
    /// watchlist still works, with prepared marketplace searches instead of fetched listings.
    pub ebay_app_id: String,
    pub ebay_cert_id: String,
    /// Google Places API key for his rating and reviews (`GOOGLE_PLACES_KEY`); empty = only the
    /// "Review us on Google" link works.
    pub google_places_key: String,
}

/// Load `~/.config/thepenmarket.env` if present (local runs), then read the environment.
pub fn load() -> anyhow::Result<Config> {
    if let Some(home) = std::env::var_os("HOME") {
        let p = PathBuf::from(home).join(".config").join("thepenmarket.env");
        if p.exists() {
            let _ = dotenvy::from_path(&p);
        }
    }
    let env = |k: &str| std::env::var(k).unwrap_or_default();
    let base = std::env::var("APP_DIR")
        .map(PathBuf::from)
        .unwrap_or_else(|_| std::env::current_dir().unwrap_or_else(|_| PathBuf::from(".")));
    let database_url = env("DATABASE_URL");
    if database_url.is_empty() {
        anyhow::bail!("DATABASE_URL is not set");
    }
    let port: u16 = env("PORT").parse().unwrap_or(8140);
    let host = if env("HOST").is_empty() { "0.0.0.0".to_string() } else { env("HOST") };
    let site_origin = if env("SITE_ORIGIN").is_empty() {
        format!("http://127.0.0.1:{port}")
    } else {
        env("SITE_ORIGIN").trim_end_matches('/').to_string()
    };
    let csrf_secret = if env("CSRF_SECRET").is_empty() {
        // A missing secret would silently weaken CSRF; refuse to start rather than default.
        anyhow::bail!("CSRF_SECRET is not set");
    } else {
        env("CSRF_SECRET")
    };
    Ok(Config {
        database_url,
        host,
        port,
        site_origin,
        media_dir: base.join("media"),
        static_dir: base.join("static"),
        uploads_dir: base.join("uploads"),
        csrf_secret,
        guide_ai_url: env("GUIDE_AI_URL").trim_end_matches('/').to_string(),
        guide_ai_key: env("GUIDE_AI_KEY"),
        guide_ai_model: env("GUIDE_AI_MODEL"),
        guide_rate_per_min: env("GUIDE_RATE_PER_MIN").parse().unwrap_or(10),
        admin_alert_email: env("ADMIN_ALERT_EMAIL"),
        forms_to: if env("FORMS_TO").is_empty() { "info@thepenmarket.com".to_string() } else { env("FORMS_TO") },
        trading_post_to: if env("TRADING_POST_TO").is_empty() { "tradingpost@thepenmarket.com".to_string() } else { env("TRADING_POST_TO") },
        ebay_app_id: env("EBAY_APP_ID"),
        ebay_cert_id: env("EBAY_CERT_ID"),
        google_places_key: env("GOOGLE_PLACES_KEY"),
    })
}
