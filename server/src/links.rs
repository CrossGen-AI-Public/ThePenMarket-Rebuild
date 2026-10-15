//! Links to other websites (ADR-0001: never send a visitor to a page that no longer exists).
//!
//! Once a day every outside link in his published posts, pages, listings and Trading Post ads is
//! requested. A link is dead after it answered "gone" (404, 410, no such host, nothing listening) on
//! two checks at least 20 hours apart. Pages then show a dead link's words as plain text; what he
//! wrote is never edited, and a link that starts answering again is linked again. Anything that only
//! refuses robots (401, 403, 429, a paywall's 402, a server error, a timeout) is "unsure" and stays
//! linked: a page that exists must not lose its link because it dislikes being checked.

use crate::app::AppState;
use regex::Regex;
use sqlx::PgPool;
use std::collections::HashSet;
use std::sync::OnceLock;
use std::time::Duration;

const MIN_GAP_HOURS: i64 = 20;

fn anchor_re() -> &'static Regex {
    static R: OnceLock<Regex> = OnceLock::new();
    R.get_or_init(|| Regex::new(r#"(?is)<a\b[^>]*?\bhref\s*=\s*"([^"]*)"[^>]*>(.*?)</a\s*>"#).unwrap_or_else(|_| unreachable!("static pattern")))
}

fn clean(href: &str) -> String {
    href.trim().replace("&amp;", "&")
}

fn is_external(href: &str) -> bool {
    let h = href.to_ascii_lowercase();
    (h.starts_with("http://") || h.starts_with("https://")) && !["thepenmarket.com", "www.thepenmarket.com"].iter().any(|own| h.split('/').nth(2).map(|host| host == *own).unwrap_or(false))
}

/// The page's HTML with every dead link replaced by its own words.
pub fn unlink(html: &str, dead: &HashSet<String>) -> String {
    if dead.is_empty() || !html.contains("<a") {
        return html.to_string();
    }
    anchor_re()
        .replace_all(html, |c: &regex::Captures| if dead.contains(&clean(&c[1])) { format!("<span class=\"link-gone\">{}</span>", &c[2]) } else { c[0].to_string() })
        .into_owned()
}

/// Every outside link in published content, with one page it appears on.
pub async fn collect(pool: &PgPool) -> anyhow::Result<Vec<(String, String)>> {
    let rows: Vec<(String, String)> = sqlx::query_as(
        "SELECT body_html, '/' || slug || '/' FROM post WHERE status = 'published'
         UNION ALL SELECT body_html, '/' || slug || '/' FROM page WHERE status = 'published'
         UNION ALL SELECT description_html, '/product/' || slug || '/' FROM product WHERE status IN ('live', 'sold')
         UNION ALL SELECT description_html, '/trading-post/' || slug || '/' FROM tp_listing WHERE status = 'live'",
    )
    .fetch_all(pool)
    .await?;
    let mut seen = HashSet::new();
    let mut out = vec![];
    for (html, path) in rows {
        for c in anchor_re().captures_iter(&html) {
            let u = clean(&c[1]);
            if is_external(&u) && u.len() <= 2000 && seen.insert(u.clone()) {
                out.push((u, path.clone()));
            }
        }
    }
    Ok(out)
}

pub async fn dead_set(pool: &PgPool) -> anyhow::Result<HashSet<String>> {
    Ok(sqlx::query_scalar::<_, String>("SELECT url FROM external_link WHERE dead").fetch_all(pool).await?.into_iter().collect())
}

#[derive(Debug, PartialEq)]
pub enum Answer {
    Ok(u16),
    Gone(u16, String),
    Unsure(u16, String),
    Skipped(String),
}

pub fn classify(status: u16) -> Answer {
    match status {
        200..=399 => Answer::Ok(status),
        404 | 410 => Answer::Gone(status, format!("answered {status}")),
        s => Answer::Unsure(s, format!("answered {s}")),
    }
}

fn private(ip: std::net::IpAddr) -> bool {
    match ip {
        std::net::IpAddr::V4(v) => v.is_private() || v.is_loopback() || v.is_link_local() || v.is_unspecified() || v.is_broadcast() || v.octets()[0] == 100 && (64..128).contains(&v.octets()[1]),
        std::net::IpAddr::V6(v) => v.is_loopback() || v.is_unspecified() || (v.segments()[0] & 0xfe00) == 0xfc00 || (v.segments()[0] & 0xffc0) == 0xfe80 || v.to_ipv4_mapped().map(|m| private(std::net::IpAddr::V4(m))).unwrap_or(false),
    }
}

async fn ask(client: &reqwest::Client, url: &str) -> Answer {
    let Ok(parsed) = reqwest::Url::parse(url) else { return Answer::Gone(0, "not a valid address".into()) };
    let Some(host) = parsed.host_str().map(str::to_string) else { return Answer::Gone(0, "no host".into()) };
    // Only the public internet: a link must never make the server knock on its own network.
    match tokio::net::lookup_host((host.as_str(), parsed.port_or_known_default().unwrap_or(443))).await {
        Ok(addrs) => {
            let addrs: Vec<_> = addrs.collect();
            if addrs.is_empty() {
                return Answer::Gone(0, "no such host".into());
            }
            if addrs.iter().any(|a| private(a.ip())) {
                return Answer::Skipped("points at a private address".into());
            }
        }
        Err(_) => return Answer::Gone(0, "no such host".into()),
    }
    let head = client.head(url).send().await;
    let res = match head {
        Ok(r) if r.status().as_u16() < 400 => return Answer::Ok(r.status().as_u16()),
        _ => client.get(url).send().await,
    };
    match res {
        Ok(r) => classify(r.status().as_u16()),
        Err(e) if e.is_connect() => Answer::Gone(0, "nothing answered".into()),
        Err(e) if e.is_timeout() => Answer::Unsure(0, "timed out".into()),
        Err(_) => Answer::Unsure(0, "the connection failed".into()),
    }
}

/// One pass over every outside link. Returns (checked, dead now).
pub async fn check_all(state: &AppState) -> anyhow::Result<(usize, usize)> {
    let client = reqwest::Client::builder()
        .timeout(Duration::from_secs(20))
        .connect_timeout(Duration::from_secs(10))
        .redirect(reqwest::redirect::Policy::limited(6))
        .user_agent("Mozilla/5.0 (compatible; ThePenMarket-linkcheck; +https://thepenmarket.com/.well-known/security.txt)")
        .build()?;
    let links = collect(&state.pool).await?;
    let current: HashSet<&str> = links.iter().map(|(u, _)| u.as_str()).collect();
    // Links no longer in any published content are forgotten.
    let known: Vec<String> = sqlx::query_scalar("SELECT url FROM external_link").fetch_all(&state.pool).await?;
    for u in known.iter().filter(|u| !current.contains(u.as_str())) {
        sqlx::query("DELETE FROM external_link WHERE url = $1").bind(u).execute(&state.pool).await?;
    }
    for (url, found_on) in &links {
        let answer = ask(&client, url).await;
        let (status, result) = match &answer {
            Answer::Ok(s) => (*s as i32, "ok".to_string()),
            Answer::Gone(s, why) => (*s as i32, format!("gone: {why}")),
            Answer::Unsure(s, why) => (*s as i32, format!("unsure: {why}")),
            Answer::Skipped(why) => (0, format!("skipped: {why}")),
        };
        match answer {
            Answer::Gone(..) => {
                sqlx::query(
                    "INSERT INTO external_link (url, last_status, last_result, first_gone_at, last_checked_at, found_on) VALUES ($1, $2, $3, now(), now(), $4)
                     ON CONFLICT (url) DO UPDATE SET last_status = $2, last_result = $3, last_checked_at = now(), found_on = $4,
                       first_gone_at = COALESCE(external_link.first_gone_at, now()),
                       dead = external_link.dead OR (external_link.first_gone_at IS NOT NULL AND external_link.first_gone_at < now() - make_interval(hours => $5)),
                       dead_since = CASE WHEN external_link.dead THEN external_link.dead_since WHEN external_link.first_gone_at IS NOT NULL AND external_link.first_gone_at < now() - make_interval(hours => $5) THEN now() ELSE NULL END",
                )
                .bind(url).bind(status).bind(&result).bind(found_on).bind(MIN_GAP_HOURS as i32)
                .execute(&state.pool).await?;
            }
            Answer::Ok(_) => {
                sqlx::query(
                    "INSERT INTO external_link (url, last_status, last_result, last_checked_at, found_on) VALUES ($1, $2, $3, now(), $4)
                     ON CONFLICT (url) DO UPDATE SET last_status = $2, last_result = $3, last_checked_at = now(), found_on = $4, first_gone_at = NULL, dead = FALSE, dead_since = NULL",
                )
                .bind(url).bind(status).bind(&result).bind(found_on)
                .execute(&state.pool).await?;
            }
            // Unsure or skipped: recorded, and a dead link stays dead / a live one stays live.
            _ => {
                sqlx::query(
                    "INSERT INTO external_link (url, last_status, last_result, last_checked_at, found_on) VALUES ($1, $2, $3, now(), $4)
                     ON CONFLICT (url) DO UPDATE SET last_status = $2, last_result = $3, last_checked_at = now(), found_on = $4",
                )
                .bind(url).bind(status).bind(&result).bind(found_on)
                .execute(&state.pool).await?;
            }
        }
        tokio::time::sleep(Duration::from_millis(300)).await;
    }
    let dead: i64 = sqlx::query_scalar("SELECT count(*) FROM external_link WHERE dead").fetch_one(&state.pool).await?;
    Ok((links.len(), dead as usize))
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn dead_links_become_words() {
        let dead: HashSet<String> = ["http://www.nancybach.com".to_string(), "https://x.example/a?b=1&c=2".to_string()].into_iter().collect();
        let html = r#"<p>See <a href="http://www.nancybach.com" rel="noopener">Nancy's site</a> and <a href="https://alive.example/">this</a>, or <a href="https://x.example/a?b=1&amp;c=2">that</a>.</p>"#;
        let out = unlink(html, &dead);
        assert_eq!(out, r#"<p>See <span class="link-gone">Nancy's site</span> and <a href="https://alive.example/">this</a>, or <span class="link-gone">that</span>.</p>"#);
    }
    #[test]
    fn what_counts_as_gone() {
        assert!(matches!(classify(404), Answer::Gone(..)));
        assert!(matches!(classify(410), Answer::Gone(..)));
        assert!(matches!(classify(403), Answer::Unsure(..)));
        assert!(matches!(classify(402), Answer::Unsure(..)));
        assert!(matches!(classify(503), Answer::Unsure(..)));
        assert!(matches!(classify(301), Answer::Ok(..)));
        assert!(!is_external("https://www.thepenmarket.com/shop/"));
        assert!(is_external("https://www.dallaspenshow.com/wp/"));
        assert!(private("10.1.2.3".parse().unwrap()) && private("127.0.0.1".parse().unwrap()) && private("100.117.164.79".parse().unwrap()) && !private("93.184.216.34".parse().unwrap()));
    }
}
