#![forbid(unsafe_code)]
//! ThePenMarket.com: one Axum binary. Server-rendered pages over Postgres, the pen guide API,
//! sitemaps, redirects. Every public path is defined in `routes::router`.

mod admin;
mod app;
mod catalog;
mod db;
mod guide;
mod routes;
mod security;

pub use thepenmarket::{config, engine, media, money, text};

use app::{AppState, RateLimiter};
use sqlx::postgres::PgPoolOptions;
use std::sync::{Arc, RwLock};
use std::time::Duration;
use tracing_subscriber::EnvFilter;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    tracing_subscriber::fmt().with_env_filter(EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new("info,sqlx=warn"))).init();
    let cfg = thepenmarket::config::load()?;
    let pool = PgPoolOptions::new().max_connections(8).acquire_timeout(Duration::from_secs(5)).connect(&cfg.database_url).await?;
    sqlx::migrate!("./migrations").run(&pool).await?;
    db::set_media_dir(cfg.media_dir.clone());
    let initial = catalog::load(&pool).await?;
    tracing::info!(pens = initial.pens.len(), "catalog loaded");
    let state: app::State = Arc::new(AppState {
        guide_limiter: RateLimiter::new(cfg.guide_rate_per_min),
        form_limiter: RateLimiter::new(20),
        http: reqwest::Client::builder().timeout(Duration::from_secs(60)).build()?,
        catalog: RwLock::new(Arc::new(initial)),
        pool: pool.clone(),
        asset_v: app::asset_version(&cfg.static_dir),
        mailer: admin::mail::from_env(cfg.uploads_dir.join("admin-mail.log")),
        admin_limiter: RateLimiter::new(10),
        cfg: cfg.clone(),
    });

    // Refresh the catalog snapshot every ten minutes so the guide and the homepage follow the database.
    let refresh = state.clone();
    tokio::spawn(async move {
        loop {
            tokio::time::sleep(Duration::from_secs(600)).await;
            match catalog::load(&refresh.pool).await {
                Ok(c) => {
                    if let Ok(mut w) = refresh.catalog.write() {
                        *w = Arc::new(c);
                    }
                }
                Err(e) => tracing::warn!("catalog refresh failed: {e:#}"),
            }
        }
    });

    let app = routes::router(state.clone());
    let addr = format!("{}:{}", cfg.host, cfg.port);
    let listener = tokio::net::TcpListener::bind(&addr).await?;
    tracing::info!("thepenmarket listening on http://{addr} (origin {})", cfg.site_origin);
    axum::serve(listener, app.into_make_service_with_connect_info::<std::net::SocketAddr>())
        .with_graceful_shutdown(async {
            let _ = tokio::signal::ctrl_c().await;
        })
        .await?;
    Ok(())
}
