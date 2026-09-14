//! Repair, sell, contact and mailing-list forms. They POST to this server, pass CSRF and a per-IP
//! rate limit, store a `form_submission` row, and say on screen that nothing was e-mailed.

use crate::app::{html, AppError, AppResult, PageMeta, State};
use crate::db;
use crate::security::{self, CsrfCookie};
use askama::Template;
use axum::extract::{ConnectInfo, Multipart, State as AxState};
use axum::http::HeaderMap;
use axum::Extension;
use axum::Form;
use rand::RngCore;
use serde::Deserialize;
use std::net::SocketAddr;

#[derive(Template)]
#[template(path = "form_done.html")]
pub struct FormDoneTpl {
    pub page: PageMeta,
    pub heading: String,
    pub lines: Vec<String>,
    pub back_url: String,
    pub back_label: String,
}

pub struct Submitted {
    pub fields: serde_json::Value,
    pub photo_key: Option<String>,
}
impl Submitted {
    pub fn get(&self, k: &str) -> String {
        self.fields.get(k).and_then(|v| v.as_str()).unwrap_or("").to_string()
    }
}

const MAX_TEXT: usize = 4000;
const MAX_PHOTO: usize = 10 * 1024 * 1024;

/// Read a multipart form with the site's rules: CSRF field must sign the cookie, required fields
/// present, text bounded, one optional photo re-encoded to JPEG under the private uploads dir.
pub async fn read_multipart(state: &State, cookie: &CsrfCookie, peer: SocketAddr, headers: &HeaderMap, mut mp: Multipart, required: &[&str]) -> Result<Submitted, AppError> {
    if !state.form_limiter.allow(security::client_ip(headers, peer.ip())) {
        return Err(AppError::Forbidden("too many submissions from this address; try again in a minute".into()));
    }
    let mut fields = serde_json::Map::new();
    let mut csrf = String::new();
    let mut photo: Option<Vec<u8>> = None;
    let mut honeypot = String::new();
    while let Some(field) = mp.next_field().await.map_err(|e| AppError::BadRequest(format!("bad form: {e}")))? {
        let name = field.name().unwrap_or("").to_string();
        if name == "photo" {
            let bytes = field.bytes().await.map_err(|e| AppError::BadRequest(format!("photo: {e}")))?;
            if bytes.len() > MAX_PHOTO {
                return Err(AppError::BadRequest("photo larger than 10 MB".into()));
            }
            if !bytes.is_empty() {
                photo = Some(bytes.to_vec());
            }
            continue;
        }
        let text = field.text().await.map_err(|e| AppError::BadRequest(format!("field {name}: {e}")))?;
        let text: String = text.chars().take(MAX_TEXT).collect();
        match name.as_str() {
            "csrf" => csrf = text,
            "website" => honeypot = text,
            "" => {}
            _ => {
                fields.insert(name, serde_json::Value::String(text.trim().to_string()));
            }
        }
    }
    if !security::csrf_ok(&state.cfg.csrf_secret, &cookie.0, &csrf) {
        return Err(AppError::Forbidden("the form expired; go back, reload the page and send it again".into()));
    }
    if !honeypot.is_empty() {
        return Err(AppError::BadRequest("form rejected".into()));
    }
    for r in required {
        if fields.get(*r).and_then(|v| v.as_str()).map(|s| s.is_empty()).unwrap_or(true) {
            return Err(AppError::BadRequest(format!("{r} is required")));
        }
    }
    if let Some(email) = fields.get("email").and_then(|v| v.as_str()) {
        if !email.is_empty() && !(email.contains('@') && email.contains('.')) {
            return Err(AppError::BadRequest("that e-mail address does not look right".into()));
        }
    }
    let mut photo_key = None;
    if let Some(bytes) = photo {
        let mut rnd = [0u8; 12];
        rand::rng().fill_bytes(&mut rnd);
        let key = format!("{}.jpg", hex::encode(rnd));
        let dest = state.cfg.uploads_dir.join(&key);
        match crate::media::reencode_upload(&bytes, &dest) {
            Ok(_) => photo_key = Some(key),
            Err(_) => return Err(AppError::BadRequest("the photo could not be read as an image (JPEG, PNG or WebP)".into())),
        }
    }
    Ok(Submitted { fields: serde_json::Value::Object(fields), photo_key })
}

fn done(state: &State, path: &str, heading: &str, lines: Vec<String>, back_url: &str, back_label: &str) -> AppResult {
    let mut page = PageMeta::new(state, &format!("{heading} | ThePenMarket.com"), "Demo submission stored on the server; nothing was e-mailed.", path);
    page.noindex = true;
    html(&FormDoneTpl { page, heading: heading.to_string(), lines, back_url: back_url.to_string(), back_label: back_label.to_string() })
}

pub async fn repair(AxState(state): AxState<State>, Extension(cookie): Extension<CsrfCookie>, ConnectInfo(peer): ConnectInfo<SocketAddr>, headers: HeaderMap, mp: Multipart) -> AppResult {
    let f = read_multipart(&state, &cookie, peer, &headers, mp, &["name", "email"]).await?;
    let id = db::save_form(&state.pool, "repair", f.fields.clone(), f.photo_key.clone()).await?;
    done(&state, "/pen-repairs/", "Repair estimate request received, as a demo", vec![
        format!("Reference #{id}. This build stores the request on the server; it does not e-mail Nathaniel. On the live site the same form reaches info@thepenmarket.com."),
        format!("From {} ({}). {}", f.get("name"), f.get("email"), if f.get("message").is_empty() { String::new() } else { format!("Message: {}", f.get("message")) }),
        if f.photo_key.is_some() { "Your photo was received and stored (re-encoded, metadata stripped)." } else { "No photo was attached; a photo helps the estimate." }.to_string(),
        "Reminder from the repairs page: discounts are available for shipments of 5 or more pens.".into(),
    ], "/pen-repairs/", "Back to Pen Repairs")
}

pub async fn sell(AxState(state): AxState<State>, Extension(cookie): Extension<CsrfCookie>, ConnectInfo(peer): ConnectInfo<SocketAddr>, headers: HeaderMap, mp: Multipart) -> AppResult {
    let f = read_multipart(&state, &cookie, peer, &headers, mp, &["name", "email"]).await?;
    let id = db::save_form(&state.pool, "sell", f.fields.clone(), f.photo_key.clone()).await?;
    done(&state, "/sell-my-pens/", "Your pens are on the list, as a demo", vec![
        format!("Reference #{id}. This build stores the message on the server; it does not e-mail Nathaniel. On the live site the same form reaches info@thepenmarket.com."),
        format!("From {} ({}). {}", f.get("name"), f.get("email"), if f.get("message").is_empty() { String::new() } else { format!("What you have: {}", f.get("message")) }),
        if f.photo_key.is_some() { "Your photo was received and stored (re-encoded, metadata stripped)." } else { "No photo was attached; if you aren't sure what you have, a photo helps." }.to_string(),
    ], "/sell-my-pens/", "Back to Sell My Pens")
}

pub async fn contact(AxState(state): AxState<State>, Extension(cookie): Extension<CsrfCookie>, ConnectInfo(peer): ConnectInfo<SocketAddr>, headers: HeaderMap, mp: Multipart) -> AppResult {
    let f = read_multipart(&state, &cookie, peer, &headers, mp, &["name", "email"]).await?;
    let id = db::save_form(&state.pool, "contact", f.fields.clone(), f.photo_key.clone()).await?;
    done(&state, "/contact/", "Message received, as a demo", vec![
        format!("Reference #{id}. This build stores the message on the server; it does not e-mail Nathaniel. On the live site the same form reaches info@thepenmarket.com."),
        format!("From {} ({}).", f.get("name"), f.get("email")),
        if f.photo_key.is_some() { "Your photo was received and stored." } else { "No photo was attached." }.to_string(),
    ], "/contact/", "Back to Contact")
}

#[derive(Deserialize)]
pub struct MailingForm {
    pub csrf: String,
    pub name: String,
    pub email: String,
    #[serde(default)]
    pub website: String,
}

pub async fn mailing_list(AxState(state): AxState<State>, Extension(cookie): Extension<CsrfCookie>, ConnectInfo(peer): ConnectInfo<SocketAddr>, headers: HeaderMap, Form(form): Form<MailingForm>) -> AppResult {
    if !state.form_limiter.allow(security::client_ip(&headers, peer.ip())) {
        return Err(AppError::Forbidden("too many submissions from this address; try again in a minute".into()));
    }
    if !security::csrf_ok(&state.cfg.csrf_secret, &cookie.0, &form.csrf) {
        return Err(AppError::Forbidden("the form expired; reload the page and try again".into()));
    }
    if !form.website.is_empty() {
        return Err(AppError::BadRequest("form rejected".into()));
    }
    let (name, email): (String, String) = (form.name.chars().take(200).collect(), form.email.chars().take(200).collect());
    if name.trim().is_empty() || !(email.contains('@') && email.contains('.')) {
        return Err(AppError::BadRequest("Please enter your name and a valid email address.".into()));
    }
    let id = db::save_form(&state.pool, "mailing_list", serde_json::json!({"name": name.trim(), "email": email.trim()}), None).await?;
    done(&state, "/", "Thanks for subscribing, as a demo", vec![
        format!("Reference #{id}. This build stores the signup on the server; no mailing-list provider is connected and no confirmation e-mail goes out."),
        format!("Name: {}. Email: {}.", name.trim(), email.trim()),
    ], "/", "Back to the shop")
}
