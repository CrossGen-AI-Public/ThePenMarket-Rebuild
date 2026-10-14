//! The admin's home and the switches that steer the shop: which pen leads the homepage, which
//! brands lead the filter, the guided photo shot list, and the inbox of everything the forms sent.

use super::handlers::{api_csrf_ok, api_err};
use super::{store, Auth, AuthApi};
use crate::app::{html, AppError, AppResult, PageMeta, State};
use crate::security::{self, CsrfCookie};
use crate::settings;
use askama::Template;
use axum::extract::{Form, Multipart, Path, Query, State as AxState};
use axum::http::{header, HeaderMap, StatusCode};
use axum::response::{IntoResponse, Redirect};
use axum::{Extension, Json};
use serde::Deserialize;
use sqlx::Row;
use std::collections::HashMap;

fn meta(state: &State, title: &str, path: &str, cookie: &CsrfCookie) -> PageMeta {
    let mut page = PageMeta::new(state, &format!("{title} | ThePenMarket.com admin"), "Admin area for ThePenMarket.com.", path);
    page.noindex = true;
    page.section = "admin".into();
    page.csrf = security::csrf_token(&state.cfg.csrf_secret, &cookie.0);
    page
}

fn check(state: &State, cookie: &CsrfCookie, admin: &super::AdminCtx, csrf: &str) -> Result<(), AppError> {
    if admin.preview {
        return Err(AppError::Forbidden("Turn off Preview to make changes.".into()));
    }
    if !security::csrf_ok(&state.cfg.csrf_secret, &cookie.0, csrf) {
        return Err(AppError::Forbidden("The form expired. Reload the page and try again.".into()));
    }
    Ok(())
}

// ---- home ------------------------------------------------------------------------------------------

pub struct Tile {
    pub title: &'static str,
    pub url: &'static str,
    pub line: String,
    pub badge: String,
}

#[derive(Template)]
#[template(path = "admin/home.html")]
pub struct HomeTpl {
    pub page: PageMeta,
    pub name: String,
    pub tiles: Vec<Tile>,
}

pub async fn home(AxState(state): AxState<State>, Extension(cookie): Extension<CsrfCookie>, admin: Option<Extension<super::AdminCtx>>) -> AppResult {
    let Some(Extension(admin)) = admin else { return Ok(Redirect::to("/admin/login/").into_response()) };
    let r = sqlx::query(
        "SELECT (SELECT count(*) FROM form_submission WHERE handled_at IS NULL AND kind <> 'mailing_list') AS inbox,
                (SELECT count(*) FROM post WHERE status = 'draft') AS drafts,
                (SELECT count(*) FROM post) AS posts,
                (SELECT count(*) FROM product WHERE status = 'live') AS live,
                (SELECT count(*) FROM product WHERE status = 'draft') AS hidden,
                (SELECT count(*) FROM product WHERE status = 'archived') AS deleted,
                (SELECT count(*) FROM external_link WHERE dead) AS dead_links,
                (SELECT count(*) FROM faq WHERE status = 'published') AS faqs,
                (SELECT count(*) FROM page WHERE custom) AS pages,
                (SELECT count(*) FROM glossary_term WHERE status = 'published') AS gloss_live,
                (SELECT count(*) FROM glossary_term WHERE status = 'draft') AS gloss_drafts,
                (SELECT count(*) FROM experiment WHERE status = 'running') AS running,
                (SELECT count(*) FROM sourcing_find WHERE status = 'new') AS finds,
                (SELECT count(DISTINCT visitor) FROM visit_event WHERE at > now() - interval '7 days') AS visitors,
                (SELECT count(*) FROM shop_order WHERE status = 'paid' AND paid_at > now() - interval '30 days') AS sold30,
                (SELECT count(*) FROM shop_order WHERE status = 'processing') AS processing",
    )
    .fetch_one(&state.pool)
    .await?;
    let n = |k: &str| r.get::<i64, _>(k);
    let badge = |v: i64, word: &str| if v > 0 { format!("{v} {word}") } else { String::new() };
    let tiles = vec![
        Tile { title: "Pens", url: "/shop/", line: format!("{} for sale. Open any pen and click what you want to change; “+ Add a pen” is in the black bar.", n("live")), badge: badge(n("hidden"), "hidden") },
        Tile {
            title: "Orders",
            url: "/admin/orders/",
            line: if state.cfg.checkout_enabled() { "Pens bought online: who bought, what they paid, where to ship. Also Trading Post listing fees and Unlimited Posts members.".into() } else { "Online checkout is off until CrossGen adds your Stripe keys. Orders will appear here.".into() },
            badge: if n("processing") > 0 { badge(n("processing"), "bank payments clearing") } else { badge(n("sold30"), "sold in 30 days") },
        },
        Tile { title: "Hidden and deleted pens", url: "/admin/pens/", line: "Pens customers can’t see: unfinished, added by accident, taken down or deleted. Restore them or erase them for good.".into(), badge: badge(n("hidden") + n("deleted"), "pens") },
        Tile { title: "Inbox", url: "/admin/inbox/", line: "Every repair request, offer to sell, contact message and Trading Post listing the site received.".into(), badge: badge(n("inbox"), "new") },
        Tile { title: "Blog posts", url: "/admin/posts/", line: format!("{} posts. Edit the old ones, put their photos back, write new ones.", n("posts")), badge: badge(n("drafts"), "drafts") },
        Tile { title: "Pages", url: "/admin/pages/", line: "Permanent pages you write: grading system, buyer’s guide, anything Google should find.".into(), badge: badge(n("pages"), "written") },
        Tile { title: "FAQs", url: "/admin/faqs/", line: format!("{} published answers, shared with Ask the Pen Market.", n("faqs")), badge: String::new() },
        Tile { title: "Glossary", url: "/admin/glossary/", line: format!("{} entries published. The words your customers trip over, explained in your words and linked from your listings.", n("gloss_live")), badge: badge(n("gloss_drafts"), "drafts to write") },
        Tile { title: "Homepage and photos", url: "/admin/settings/", line: "The featured pen, the Most Popular row, the guided photo shoot list, where forms are e-mailed.".into(), badge: String::new() },
        Tile { title: "Links to other websites", url: "/admin/links/", line: "Links in your posts and listings to other sites. Ones whose page is gone show as plain words instead of a dead end.".into(), badge: badge(n("dead_links"), "no longer linked") },
        Tile { title: "Old addresses", url: "/admin/redirects/", line: "Every old-site address that moved, and where it sends people. Paste a list of old addresses to add them.".into(), badge: String::new() },
        Tile { title: "Search spellings", url: "/admin/search/", line: "Schaffer finds Sheaffer, Mont Blanc finds Montblanc. Your list of how people misspell a search.".into(), badge: String::new() },
        Tile { title: "Brands", url: "/admin/brands/", line: "Pin Mont Blanc (or any brand) to the top of the brand filter, whatever is in stock.".into(), badge: String::new() },
        Tile { title: "Insights", url: "/admin/insights/", line: "What visitors look at, search for and click. Heat maps are on every page: press “Heat map” in the black bar.".into(), badge: badge(n("visitors"), "visitors this week") },
        Tile { title: "Experiments", url: "/admin/experiments/", line: "Show half your visitors one version and half another, and see which sells better.".into(), badge: badge(n("running"), "running") },
        Tile { title: "Sourcing", url: "/admin/sourcing/", line: "Pens you are hunting for, with marketplace searches and what each would sell for here.".into(), badge: badge(n("finds"), "new finds") },
        Tile { title: "History", url: "/admin/history/", line: "Every change, when, and what it was before.".into(), badge: String::new() },
    ];
    html(&HomeTpl { page: meta(&state, "Admin", "/admin/", &cookie), name: admin.name, tiles })
}

// ---- featured pen ------------------------------------------------------------------------------------

#[derive(Deserialize)]
pub struct FeaturedBody {
    pub product_id: Option<i32>,
}

/// `POST /admin/api/featured/`: a product id to feature it, null to go back to "newest pen".
pub async fn set_featured(AxState(state): AxState<State>, Extension(cookie): Extension<CsrfCookie>, AuthApi(admin): AuthApi, headers: HeaderMap, Json(body): Json<FeaturedBody>) -> AppResult {
    if admin.preview || !api_csrf_ok(&state, &cookie, &headers) {
        return Ok(api_err(StatusCode::FORBIDDEN, "The page expired. Reload and try again."));
    }
    match body.product_id {
        Some(id) => {
            let Some(p) = store::product_for_edit(&state.pool, id).await? else { return Err(AppError::NotFound) };
            if p.status != "live" {
                return Ok(api_err(StatusCode::BAD_REQUEST, "Only a pen that is for sale can lead the homepage."));
            }
            settings::set(&state.pool, settings::FEATURED_PRODUCT, serde_json::json!(id)).await?;
            // his photo of it without the backdrop, stood upright, ready by the time he looks
            if let Some(rel) = crate::hero_photo::lead_photo(&state, id).await? {
                crate::hero_photo::spawn_make(state.clone(), id, rel, false);
            }
            store::audit(&state.pool, "admin.featured_set", serde_json::json!({"email": admin.email, "ip": admin.ip, "pen": p.short_title, "product_id": id})).await?;
        }
        None => {
            settings::clear(&state.pool, settings::FEATURED_PRODUCT).await?;
            store::audit(&state.pool, "admin.featured_cleared", serde_json::json!({"email": admin.email, "ip": admin.ip})).await?;
        }
    }
    Ok(Json(serde_json::json!({"ok": true})).into_response())
}

// ---- settings ----------------------------------------------------------------------------------------

#[derive(Template)]
#[template(path = "admin/settings.html")]
pub struct SettingsTpl {
    pub page: PageMeta,
    pub saved: bool,
    pub featured_title: String,
    pub featured_url: String,
    /// The homepage image of the pen leading it now (chosen or newest), and whether he turned it around.
    pub hero_image: String,
    pub hero_product_id: i32,
    pub hero_flipped: bool,
    /// The pen leading the homepage (chosen or newest) and its own photo, for the cut-out upload.
    pub hero_title: String,
    pub hero_plain: String,
    pub popular_on: bool,
    pub hero_photo: bool,
    pub hero_experiment: bool,
    pub shots: String,
    pub max_photos: i64,
    pub mail_connected: bool,
    pub forms_to: String,
    pub trading_post_to: String,
    pub place_id: String,
    pub places_key_set: bool,
    pub reviews_line: String,
    /// "off", "test" (Stripe test keys: no real money) or "live".
    pub checkout_mode: &'static str,
    /// His shipping options as rows of the form, plus blank rows to add more.
    pub ship_rows: Vec<ShipRow>,
    /// False while the site runs on the three rates from his e-mail (he has not saved his own list).
    pub ship_saved: bool,
    pub ship_max: usize,
}

pub struct ShipRow {
    pub name: String,
    pub price: String,
    pub region: &'static str,
}

/// Blank rows after his options, so he can add some without a script.
const SHIP_BLANK_ROWS: usize = 3;
/// Rows the form reads back: five per place is Stripe's limit.
const SHIP_FORM_ROWS: usize = 2 * thepenmarket::shipping::MAX_PER_REGION + SHIP_BLANK_ROWS;

fn ship_rows(list: &[thepenmarket::shipping::ShippingOption]) -> Vec<ShipRow> {
    let mut rows: Vec<ShipRow> = list.iter().map(|o| ShipRow { name: o.name.clone(), price: format!("{}.{:02}", o.cents / 100, o.cents % 100), region: o.region.key() }).collect();
    let blanks = SHIP_BLANK_ROWS.min(SHIP_FORM_ROWS.saturating_sub(rows.len()));
    rows.extend((0..blanks).map(|_| ShipRow { name: String::new(), price: String::new(), region: "us" }));
    rows
}

fn checkout_mode(state: &State) -> &'static str {
    if !state.cfg.checkout_enabled() {
        "off"
    } else if state.cfg.stripe_secret_key.starts_with("sk_live_") || state.cfg.stripe_secret_key.starts_with("rk_live_") {
        "live"
    } else {
        "test"
    }
}

#[derive(Deserialize, Default)]
pub struct SavedQuery {
    #[serde(default)]
    pub saved: String,
    #[serde(default)]
    pub kind: String,
    #[serde(default)]
    pub show: String,
}

pub async fn settings_page(AxState(state): AxState<State>, Extension(cookie): Extension<CsrfCookie>, Auth(_admin): Auth, Query(q): Query<SavedQuery>) -> AppResult {
    let featured = match settings::featured_product(&state.pool).await? {
        Some(id) => crate::db::products_by_ids(&state.pool, &[id], true).await?.into_iter().next(),
        None => None,
    };
    let (ship_list, ship_saved) = settings::shipping_options(&state.pool).await?;
    // the pen leading the homepage now: his choice, else the newest (as routes::home picks it)
    let leading = match &featured {
        Some(p) => Some(p.id),
        None => crate::db::products_by_slugs_or_recent(&state.pool, 1, &[], true, true).await?.first().map(|p| p.id),
    };
    let (mut hero_image, mut hero_plain, mut hero_title) = (String::new(), String::new(), String::new());
    if let Some(id) = leading {
        if let Some(rel) = crate::hero_photo::lead_photo(&state, id).await? {
            hero_image = crate::hero_photo::best(&state, id, &rel).await.unwrap_or_default();
            hero_plain = format!("/media/{}", crate::media::variant_rel(&rel, 480));
        }
        hero_title = crate::db::products_by_ids(&state.pool, &[id], true).await?.into_iter().next().map(|p| p.short_title).unwrap_or_default();
    }
    let hero_flipped = match leading { Some(id) => crate::hero_photo::flipped(&state, id).await, None => false };
    html(&SettingsTpl {
        page: meta(&state, "Homepage and photos", "/admin/settings/", &cookie),
        saved: q.saved == "1",
        featured_title: featured.as_ref().map(|p| p.short_title.clone()).unwrap_or_default(),
        featured_url: featured.map(|p| p.url).unwrap_or_default(),
        hero_image,
        hero_product_id: leading.unwrap_or(0),
        hero_flipped,
        hero_title,
        hero_plain,
        popular_on: settings::popular_section(&state.pool).await?,
        hero_photo: settings::hero_photo(&state.pool).await?,
        hero_experiment: crate::insights::variant(&state, "hero-stage").is_some(),
        shots: settings::shot_list(&state.pool).await?.join("\n"),
        max_photos: settings::MAX_PHOTOS,
        mail_connected: state.mailer.name() == "smtp",
        forms_to: state.cfg.forms_to.clone(),
        trading_post_to: state.cfg.trading_post_to.clone(),
        place_id: crate::reviews::place_id(&state.pool).await?,
        places_key_set: !state.cfg.google_places_key.is_empty(),
        reviews_line: crate::reviews::cached(&state.pool).await?.map(|r| format!("Google last returned {} out of 5 from {} reviews ({} with text shown on the homepage).", r.rating_label(), r.count, r.reviews.len())).unwrap_or_default(),
        checkout_mode: checkout_mode(&state),
        ship_rows: ship_rows(&ship_list),
        ship_saved,
        ship_max: thepenmarket::shipping::MAX_PER_REGION,
    })
}

#[derive(Deserialize)]
pub struct SettingsForm {
    pub csrf: String,
    #[serde(default)]
    pub popular: String,
    #[serde(default)]
    pub shots: String,
    #[serde(default)]
    pub clear_featured: String,
    #[serde(default)]
    pub place_id: String,
    #[serde(default)]
    pub hero: String,
    /// "1": forget his list and go back to the rates from his e-mail of 2026-10-12.
    #[serde(default)]
    pub ship_default: String,
    /// The shipping rows: `ship_name_<n>`, `ship_price_<n>`, `ship_where_<n>`.
    #[serde(flatten)]
    pub rest: HashMap<String, String>,
}

/// The shipping rows of the settings form, checked. Err is the message to show; nothing is saved.
fn parse_ship_rows(rest: &HashMap<String, String>) -> Result<Vec<thepenmarket::shipping::ShippingOption>, String> {
    let get = |k: String| rest.get(&k).map(String::as_str).unwrap_or("");
    let typed: Vec<thepenmarket::shipping::Typed> = (0..SHIP_FORM_ROWS).map(|i| thepenmarket::shipping::Typed { name: get(format!("ship_name_{i}")), price: get(format!("ship_price_{i}")), region: get(format!("ship_where_{i}")) }).collect();
    thepenmarket::shipping::parse(&typed)
}

pub async fn settings_save(AxState(state): AxState<State>, Extension(cookie): Extension<CsrfCookie>, Auth(admin): Auth, Form(form): Form<SettingsForm>) -> AppResult {
    check(&state, &cookie, &admin, &form.csrf)?;
    // Shipping first: a typo here must not leave half the form saved.
    let shipping = if form.ship_default == "1" { None } else { Some(parse_ship_rows(&form.rest).map_err(|e| AppError::BadRequest(format!("Shipping: {e} Go back; nothing was saved.")))?) };
    match &shipping {
        Some(list) => settings::set(&state.pool, settings::SHIPPING_OPTIONS, serde_json::to_value(list)?).await?,
        None => settings::clear(&state.pool, settings::SHIPPING_OPTIONS).await?,
    }
    settings::clear(&state.pool, settings::SHIPPING_FLAT_OLD).await?;
    settings::set(&state.pool, settings::POPULAR_SECTION, serde_json::json!(form.popular == "1")).await?;
    settings::set(&state.pool, settings::HERO_MODE, serde_json::json!(if form.hero == "photo" { "photo" } else { "3d" })).await?;
    let shots = settings::parse_shot_list(&form.shots);
    if shots.is_empty() {
        settings::clear(&state.pool, settings::SHOT_LIST).await?;
    } else {
        settings::set(&state.pool, settings::SHOT_LIST, serde_json::json!(shots)).await?;
    }
    if form.clear_featured == "1" {
        settings::clear(&state.pool, settings::FEATURED_PRODUCT).await?;
    }
    let place = form.place_id.trim();
    if place.is_empty() {
        settings::clear(&state.pool, crate::reviews::PLACE_ID).await?;
        settings::clear(&state.pool, crate::reviews::CACHE).await?;
    } else if crate::reviews::valid_place_id(place) {
        settings::set(&state.pool, crate::reviews::PLACE_ID, serde_json::json!(place)).await?;
        if let Err(e) = crate::reviews::refresh(&state).await {
            tracing::warn!("google reviews refresh failed: {e:#}");
        }
    } else {
        return Err(AppError::BadRequest("That does not look like a Google Place ID (letters, digits, dashes and underscores, usually starting ChIJ). Go back; nothing else was lost.".into()));
    }
    crate::app::refresh_site_cache(&state).await;
    store::audit(&state.pool, "admin.settings_saved", serde_json::json!({"email": admin.email, "ip": admin.ip, "popular_section": form.popular == "1", "hero": if form.hero == "photo" { "photo" } else { "3d" }, "shots": shots.len(), "shipping_options": shipping.as_ref().map(|l| serde_json::to_value(l).unwrap_or_default()).unwrap_or_else(|| serde_json::json!("his e-mail of 2026-10-12"))})).await?;
    Ok(Redirect::to("/admin/settings/?saved=1").into_response())
}

#[derive(Deserialize)]
pub struct FlipForm {
    pub csrf: String,
    pub product_id: i32,
}

/// "Turn the pen the other way up": for the rare photo where the stand-up put the nib at the bottom.
pub async fn hero_flip(AxState(state): AxState<State>, Extension(cookie): Extension<CsrfCookie>, Auth(admin): Auth, Form(form): Form<FlipForm>) -> AppResult {
    check(&state, &cookie, &admin, &form.csrf)?;
    let mut ids: Vec<i64> = settings::get(&state.pool, crate::hero_photo::FLIP_SETTING).await?.and_then(|v| serde_json::from_value(v).ok()).unwrap_or_default();
    let id = form.product_id as i64;
    let now_flipped = if ids.contains(&id) { ids.retain(|x| *x != id); false } else { ids.push(id); true };
    settings::set(&state.pool, crate::hero_photo::FLIP_SETTING, serde_json::json!(ids)).await?;
    if let Some(rel) = crate::hero_photo::lead_photo(&state, form.product_id).await? {
        if let Err(e) = crate::hero_photo::remake(&state, form.product_id, &rel).await {
            tracing::warn!("homepage photo remake failed: {e:#}");
        }
    }
    store::audit(&state.pool, "admin.hero_flip", serde_json::json!({"email": admin.email, "ip": admin.ip, "product_id": form.product_id, "flipped": now_flipped})).await?;
    Ok(Redirect::to("/admin/settings/?saved=1").into_response())
}

/// His cut-out of the homepage pen's photo (made on his Mac: Finder, Quick Actions, Remove Background).
/// Stored as that photo's cutout; the site then stands it upright as for any other.
pub async fn hero_cutout(AxState(state): AxState<State>, Extension(cookie): Extension<CsrfCookie>, Auth(admin): Auth, mut mp: Multipart) -> AppResult {
    let (mut csrf, mut product_id, mut bytes) = (String::new(), 0i32, None::<Vec<u8>>);
    while let Some(field) = mp.next_field().await.map_err(|e| AppError::BadRequest(format!("upload: {e}")))? {
        match field.name() {
            Some("csrf") => csrf = field.text().await.unwrap_or_default(),
            Some("product_id") => product_id = field.text().await.unwrap_or_default().trim().parse().unwrap_or(0),
            Some("photo") => {
                let b = field.bytes().await.map_err(|e| AppError::BadRequest(format!("photo: {e}")))?;
                if b.len() > 10 * 1024 * 1024 {
                    return Err(AppError::BadRequest("Photos must be under 10 MB. Go back and try a smaller file.".into()));
                }
                if !b.is_empty() {
                    bytes = Some(b.to_vec());
                }
            }
            _ => {}
        }
    }
    check(&state, &cookie, &admin, &csrf)?;
    let Some(bytes) = bytes else {
        return Err(AppError::BadRequest("No photo was attached. Go back and choose the cut-out file.".into()));
    };
    let Some(rel) = crate::hero_photo::lead_photo(&state, product_id).await? else {
        return Err(AppError::BadRequest("That pen has no photo yet. Add one on its page first.".into()));
    };
    let dest = state.cfg.media_dir.join(crate::media::cutout_rel(&rel));
    match crate::media::save_cutout_upload_async(bytes, dest).await {
        Ok(crate::media::CutoutUpload::Saved { .. }) => {}
        Ok(crate::media::CutoutUpload::StillHasBackground) => {
            return Err(AppError::BadRequest("That photo still has its background. On your Mac, right-click the photo in Finder and choose Quick Actions, then Remove Background (or open it in Preview: Tools, Remove Background). Then upload the new file it makes.".into()));
        }
        Err(_) => return Err(AppError::BadRequest("That file could not be read as a photo (PNG, JPEG or WebP). Go back and try again.".into())),
    }
    if let Err(e) = crate::hero_photo::remake(&state, product_id, &rel).await {
        tracing::warn!("homepage photo remake failed: {e:#}");
    }
    store::audit(&state.pool, "admin.hero_cutout", serde_json::json!({"email": admin.email, "ip": admin.ip, "product_id": product_id})).await?;
    Ok(Redirect::to("/admin/settings/?saved=1").into_response())
}

// ---- brands ------------------------------------------------------------------------------------------

pub struct BrandRow {
    pub id: i32,
    pub name: String,
    pub url: String,
    pub live: i64,
    pub interest: i64,
    pub pin: String,
}

#[derive(Template)]
#[template(path = "admin/brands.html")]
pub struct BrandsTpl {
    pub page: PageMeta,
    pub saved: bool,
    pub rows: Vec<BrandRow>,
    pub mode: String,
    pub experiment_running: bool,
}

pub async fn brands(AxState(state): AxState<State>, Extension(cookie): Extension<CsrfCookie>, Auth(_admin): Auth, Query(q): Query<SavedQuery>) -> AppResult {
    let interest = crate::insights::brand_interest(&state.pool, 90).await?;
    let rows = sqlx::query("SELECT b.id, b.slug, b.name, b.pin_rank, (SELECT count(*) FROM product p WHERE p.brand_id = b.id AND p.status = 'live') AS live FROM brand b ORDER BY b.pin_rank NULLS LAST, live DESC, b.name").fetch_all(&state.pool).await?;
    let rows = rows
        .iter()
        .map(|r| {
            let slug: String = r.get("slug");
            BrandRow { id: r.get("id"), name: r.get("name"), url: format!("/brand/{slug}/"), live: r.get("live"), interest: interest.get(&slug).copied().unwrap_or(0), pin: r.get::<Option<i32>, _>("pin_rank").map(|n| n.to_string()).unwrap_or_default() }
        })
        .collect();
    let experiment_running = state.site().experiments.get("brand-order").map(|e| e.status == "running" || e.winner.is_some()).unwrap_or(false);
    html(&BrandsTpl { page: meta(&state, "Brands", "/admin/brands/", &cookie), saved: q.saved == "1", rows, mode: settings::brand_order(&state.pool).await?, experiment_running })
}

/// The form posts `pin_<brand id>` = a number (1 is the very top) or blank, plus `mode`.
pub async fn brands_save(AxState(state): AxState<State>, Extension(cookie): Extension<CsrfCookie>, Auth(admin): Auth, Form(form): Form<HashMap<String, String>>) -> AppResult {
    check(&state, &cookie, &admin, form.get("csrf").map(String::as_str).unwrap_or(""))?;
    let mut pinned = 0;
    let mut tx = state.pool.begin().await?;
    for (k, v) in &form {
        let Some(id) = k.strip_prefix("pin_").and_then(|s| s.parse::<i32>().ok()) else { continue };
        let rank: Option<i32> = v.trim().parse::<i32>().ok().filter(|n| (1..=999).contains(n));
        if rank.is_some() {
            pinned += 1;
        }
        sqlx::query("UPDATE brand SET pin_rank = $2 WHERE id = $1").bind(id).bind(rank).execute(&mut *tx).await?;
    }
    tx.commit().await?;
    let mode = if form.get("mode").map(String::as_str) == Some("popular") { "popular" } else { "stock" };
    settings::set(&state.pool, settings::BRAND_ORDER, serde_json::json!(mode)).await?;
    store::audit(&state.pool, "admin.brands_saved", serde_json::json!({"email": admin.email, "ip": admin.ip, "pinned": pinned, "order": mode})).await?;
    Ok(Redirect::to("/admin/brands/?saved=1").into_response())
}

// ---- inbox -------------------------------------------------------------------------------------------

pub struct InboxRow {
    pub id: i32,
    pub kind: String,
    pub kind_label: &'static str,
    pub when: String,
    pub fields: Vec<(String, String)>,
    pub reply_to: String,
    pub photo: String,
    pub mailed: bool,
    pub handled: bool,
    /// A Trading Post listing posted with checkout on: where its fee stands, and its page once live.
    pub listing_status: String,
    pub listing_url: String,
}

#[derive(Template)]
#[template(path = "admin/inbox.html")]
pub struct InboxTpl {
    pub page: PageMeta,
    pub rows: Vec<InboxRow>,
    pub kind: String,
    pub show: String,
    pub new_count: usize,
    pub mail_connected: bool,
}

fn kind_label(kind: &str) -> &'static str {
    match kind {
        "repair" => "Repair estimate",
        "sell" => "Pens to sell",
        "contact" => "Contact",
        "trading_post" => "Trading Post listing",
        "mailing_list" => "Mailing list signup",
        "guide_handoff" => "From Ask the Pen Market",
        _ => "Message",
    }
}

pub async fn inbox(AxState(state): AxState<State>, Extension(cookie): Extension<CsrfCookie>, Auth(_admin): Auth, Query(q): Query<SavedQuery>) -> AppResult {
    let rows = sqlx::query(
        "SELECT f.id, f.kind, f.fields, f.photo_key, f.mailed, f.handled_at, f.created_at, l.status AS tp_status, l.paid_via AS tp_paid_via, l.slug AS tp_slug, l.note AS tp_note
         FROM form_submission f LEFT JOIN tp_listing l ON l.form_submission_id = f.id
         WHERE ($1 = '' OR f.kind = $1) AND ($2 = 'all' OR f.handled_at IS NULL) ORDER BY f.created_at DESC LIMIT 300",
    )
    .bind(&q.kind)
    .bind(&q.show)
    .fetch_all(&state.pool)
    .await?;
    let rows: Vec<InboxRow> = rows
        .iter()
        .map(|r| {
            let kind: String = r.get("kind");
            let fields: serde_json::Value = r.get("fields");
            let pairs: Vec<(String, String)> = fields.as_object().map(|o| o.iter().map(|(k, v)| (k.replace('_', " "), v.as_str().unwrap_or("").to_string())).filter(|(_, v)| !v.is_empty()).collect()).unwrap_or_default();
            let reply_to = fields.get("email").and_then(|v| v.as_str()).filter(|e| e.contains('@')).unwrap_or("").to_string();
            InboxRow {
                id: r.get("id"),
                kind_label: kind_label(&kind),
                kind,
                when: super::handlers::fmt_time(r.get("created_at")),
                fields: pairs,
                reply_to,
                photo: r.get::<Option<String>, _>("photo_key").unwrap_or_default(),
                mailed: r.get("mailed"),
                handled: r.get::<Option<chrono::DateTime<chrono::Utc>>, _>("handled_at").is_some(),
                listing_status: match (r.get::<Option<String>, _>("tp_status").as_deref(), r.get::<Option<String>, _>("tp_paid_via").as_deref(), r.get::<Option<String>, _>("tp_note").unwrap_or_default()) {
                    (Some("live"), Some("membership"), _) => "Live on the Trading Post (Unlimited Posts member, no fee)".into(),
                    (Some("live"), _, _) => "Live on the Trading Post (fee paid)".into(),
                    (Some("pending_payment"), _, n) if n.starts_with("Bank payment") => "Not published yet: bank payment clearing".into(),
                    (Some("pending_payment"), _, _) => "Not published: the listing fee has not been paid".into(),
                    (Some(_), _, n) if n.contains("refunded") => "Taken down; the fee was refunded".into(),
                    (Some(_), _, _) => "Taken down".into(),
                    (None, _, _) => String::new(),
                },
                listing_url: if r.get::<Option<String>, _>("tp_status").as_deref() == Some("live") { r.get::<Option<String>, _>("tp_slug").map(|s| format!("/trading-post/{s}/")).unwrap_or_default() } else { String::new() },
            }
        })
        .collect();
    let new_count = rows.iter().filter(|r| !r.handled).count();
    html(&InboxTpl { page: meta(&state, "Inbox", "/admin/inbox/", &cookie), rows, kind: q.kind, show: q.show, new_count, mail_connected: state.mailer.name() == "smtp" })
}

#[derive(Deserialize)]
pub struct HandledForm {
    pub csrf: String,
    #[serde(default)]
    pub handled: String,
    #[serde(default)]
    pub back: String,
}

pub async fn inbox_handled(AxState(state): AxState<State>, Extension(cookie): Extension<CsrfCookie>, Auth(admin): Auth, Path(id): Path<i32>, Form(form): Form<HandledForm>) -> AppResult {
    check(&state, &cookie, &admin, &form.csrf)?;
    sqlx::query("UPDATE form_submission SET handled_at = CASE WHEN $2 THEN now() ELSE NULL END, status = CASE WHEN $2 THEN 'handled' ELSE 'new' END WHERE id = $1").bind(id).bind(form.handled == "1").execute(&state.pool).await?;
    let back = if form.back.starts_with("/admin/inbox/") { form.back } else { "/admin/inbox/".into() };
    Ok(Redirect::to(&back).into_response())
}

/// Photos visitors attach live outside the public media folder; only the signed-in admin can read them.
pub async fn inbox_photo(AxState(state): AxState<State>, Auth(_admin): Auth, Path(key): Path<String>) -> AppResult {
    if key.len() > 40 || !key.ends_with(".jpg") || !key.trim_end_matches(".jpg").chars().all(|c| c.is_ascii_hexdigit()) {
        return Err(AppError::NotFound);
    }
    let bytes = tokio::fs::read(state.cfg.uploads_dir.join(&key)).await.map_err(|_| AppError::NotFound)?;
    Ok(([(header::CONTENT_TYPE, "image/jpeg"), (header::CACHE_CONTROL, "private, no-store")], bytes).into_response())
}

// ---- orders ------------------------------------------------------------------------------------------

pub struct OrderView {
    pub id: i32,
    pub when: String,
    pub pen: String,
    pub pen_url: String,
    pub sku: String,
    pub total: String,
    /// "USPS Insured Priority, $15.00", or "" when no shipping was charged.
    pub shipping: String,
    pub ships: &'static str,
    pub status: &'static str,
    pub attention: bool,
    pub buyer: String,
    pub email: String,
    pub ship_to: Vec<String>,
    pub stripe_url: String,
    pub note: String,
}

#[derive(Template)]
#[template(path = "admin/orders.html")]
pub struct OrdersTpl {
    pub page: PageMeta,
    pub rows: Vec<OrderView>,
    pub show_all: bool,
    pub checkout_mode: &'static str,
    /// "Shipping: Uninsured USPS Ground Advantage $6.00 (US), …", what buyers are offered now.
    pub shipping_line: String,
    pub ship_saved: bool,
    pub mail_connected: bool,
    pub listings: Vec<ListingView>,
    pub members: Vec<MemberView>,
    pub listing_fee: String,
}

pub struct ListingView {
    pub id: i32,
    pub when: String,
    pub title: String,
    pub url: String,
    pub status: &'static str,
    pub attention: bool,
    pub email: String,
    pub paid: String,
    pub inbox_url: String,
    pub stripe_url: String,
    pub note: String,
}

pub struct MemberView {
    pub email: String,
    pub since: String,
    pub paid_through: String,
    pub status: &'static str,
    pub stripe_url: String,
}

fn listing_status(l: &thepenmarket::tp_pay::TpListing) -> (&'static str, bool) {
    match (l.status.as_str(), l.paid_via.as_str()) {
        ("live", "membership") => ("Live: Unlimited Posts member", false),
        ("live", _) => ("Live: $5 fee paid", false),
        ("pending_payment", _) if l.note.starts_with("Bank payment") => ("Waiting: bank payment clearing", false),
        ("pending_payment", _) => ("Not published: fee not paid", false),
        ("removed", _) if l.note.contains("refunded") => ("Taken down; payment refunded", true),
        ("removed", _) => ("Taken down", false),
        _ => ("Closed", false),
    }
}

pub fn shipping_summary(list: &[thepenmarket::shipping::ShippingOption]) -> String {
    list.iter().map(|o| format!("{} {} ({})", o.name, thepenmarket::money::fmt_cents(o.cents), if o.region == thepenmarket::shipping::Region::Us { "US" } else { "outside the US" })).collect::<Vec<_>>().join(" · ")
}

fn order_status(s: &str) -> (&'static str, bool) {
    match s {
        "paid" => ("Paid: ship it", true),
        "processing" => ("Bank payment clearing: pen held, don't ship yet", false),
        "refunded" => ("Refunded: paid after the pen was gone", true),
        "open" => ("Checking out", false),
        "expired" => ("Not finished", false),
        "cancelled" => ("Let go by the buyer", false),
        "failed" => ("Failed", true),
        _ => ("Unknown", true),
    }
}

pub async fn orders(AxState(state): AxState<State>, Extension(cookie): Extension<CsrfCookie>, Auth(_admin): Auth, Query(q): Query<SavedQuery>) -> AppResult {
    let show_all = q.show == "all";
    let live_mode = state.cfg.stripe_secret_key.starts_with("sk_live_") || state.cfg.stripe_secret_key.starts_with("rk_live_");
    let rows = thepenmarket::orders::list(&state.pool, show_all, 300).await?;
    let rows = rows
        .into_iter()
        .map(|o| {
            let (status, attention) = order_status(&o.status);
            let a = o.shipping_address.clone().unwrap_or_default();
            let g = |k: &str| a.get(k).and_then(|v| v.as_str()).unwrap_or("").trim().to_string();
            let ship_to = [g("name"), g("line1"), g("line2"), format!("{} {} {}", g("city"), g("state"), g("postal_code")).trim().to_string(), g("country")].into_iter().filter(|s| !s.is_empty()).collect();
            let total = o.amount_total_cents.unwrap_or(o.amount_cents + o.shipping_cents);
            let shipping = o.shipping_label();
            OrderView {
                id: o.id,
                when: super::handlers::fmt_time(o.paid_at.unwrap_or(o.created_at)),
                pen_url: format!("/product/{}/", o.product_slug),
                pen: o.product_title,
                sku: o.product_sku,
                total: thepenmarket::money::fmt_cents(total),
                shipping,
                ships: if o.ship_region == "intl" { "Outside the US" } else { "In the US" },
                status,
                attention,
                buyer: o.buyer_name,
                email: o.buyer_email,
                ship_to,
                stripe_url: if o.stripe_payment_intent.is_empty() { String::new() } else { format!("https://dashboard.stripe.com/{}payments/{}", if live_mode { "" } else { "test/" }, o.stripe_payment_intent) },
                note: o.note,
            }
        })
        .collect();
    let (ship_list, ship_saved) = settings::shipping_options(&state.pool).await?;
    let dash = format!("https://dashboard.stripe.com/{}", if live_mode { "" } else { "test/" });
    let listings = thepenmarket::tp_pay::posted_listings(&state.pool, 100)
        .await?
        .into_iter()
        .filter(|l| show_all || l.status != "pending_payment" || l.note.starts_with("Bank payment"))
        .map(|l| {
            let (status, attention) = listing_status(&l);
            ListingView {
                id: l.id,
                when: super::handlers::fmt_time(l.paid_at.unwrap_or(l.created_at)),
                url: if l.status == "live" { format!("/trading-post/{}/", l.slug) } else { String::new() },
                title: l.title,
                status,
                attention,
                email: l.poster_email,
                paid: l.amount_total_cents.filter(|_| l.paid_at.is_some()).map(thepenmarket::money::fmt_cents).unwrap_or_default(),
                inbox_url: l.form_submission_id.map(|_| "/admin/inbox/?kind=trading_post&show=all".to_string()).unwrap_or_default(),
                stripe_url: if l.stripe_payment_intent.is_empty() { String::new() } else { format!("{dash}payments/{}", l.stripe_payment_intent) },
                note: l.note,
            }
        })
        .collect();
    let now = chrono::Utc::now();
    let members = thepenmarket::tp_pay::members(&state.pool, 300)
        .await?
        .into_iter()
        .map(|m| MemberView {
            since: m.created_at.format("%b %-d, %Y").to_string(),
            paid_through: m.paid_through.format("%b %-d, %Y").to_string(),
            status: if m.paid_through > now { if m.status == "cancelled" { "Cancelled; paid until" } else { "Member through" } } else { "Ended" },
            stripe_url: format!("{dash}subscriptions/{}", m.stripe_subscription),
            email: m.email,
        })
        .collect();
    html(&OrdersTpl {
        page: meta(&state, "Orders", "/admin/orders/", &cookie),
        rows,
        show_all,
        checkout_mode: checkout_mode(&state),
        shipping_line: shipping_summary(&ship_list),
        ship_saved,
        mail_connected: state.mailer.name() == "smtp",
        listings,
        members,
        listing_fee: thepenmarket::money::fmt_dollars(thepenmarket::engine::TRADING_POST_LISTING_CENTS),
    })
}
