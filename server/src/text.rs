//! Text helpers: slugs, tag stripping, entity decoding, meta descriptions, HTML rewriting for
//! imported WordPress content.

use regex::Regex;
use std::sync::LazyLock;

fn re(p: &'static str) -> Regex {
    Regex::new(p).unwrap_or_else(|e| panic!("static regex {p}: {e}"))
}

static TAGS: LazyLock<Regex> = LazyLock::new(|| re(r"(?s)<[^>]+>"));
static SCRIPTS: LazyLock<Regex> = LazyLock::new(|| re(r"(?is)<script\b.*?</script>|<style\b.*?</style>"));
static WS: LazyLock<Regex> = LazyLock::new(|| re(r"\s+"));
static NON_SLUG: LazyLock<Regex> = LazyLock::new(|| re(r"[^a-z0-9]+"));

pub fn slugify(s: &str) -> String {
    let lower = decode(s).to_lowercase().replace('’', "").replace('\'', "");
    let s = NON_SLUG.replace_all(&lower, "-").to_string();
    s.trim_matches('-').to_string()
}

pub fn decode(s: &str) -> String {
    html_escape::decode_html_entities(s).replace('\u{a0}', " ")
}

pub fn escape(s: &str) -> String {
    html_escape::encode_text(s).to_string()
}

/// Visible text from HTML: scripts and styles dropped, tags removed, entities decoded, whitespace folded.
pub fn strip_tags(html: &str) -> String {
    let no_scripts = SCRIPTS.replace_all(html, " ");
    let no_tags = TAGS.replace_all(&no_scripts, " ");
    let decoded = decode(&no_tags);
    WS.replace_all(&decoded, " ").trim().to_string()
}

pub fn word_count(text: &str) -> i32 {
    text.split_whitespace().count() as i32
}

/// First sentences of a text up to `max` characters, cut on a word boundary. Used for meta
/// descriptions when Yoast did not carry one; the words are still his.
pub fn summary(text: &str, max: usize) -> String {
    let t = WS.replace_all(text.trim(), " ").to_string();
    if t.chars().count() <= max {
        return t;
    }
    let mut out = String::new();
    for w in t.split(' ') {
        if out.chars().count() + w.chars().count() + 1 > max.saturating_sub(1) {
            break;
        }
        if !out.is_empty() {
            out.push(' ');
        }
        out.push_str(w);
    }
    let trimmed = out.trim_end_matches(|c: char| c == ',' || c == ';' || c == ':').to_string();
    if trimmed.ends_with('.') { trimmed } else { format!("{trimmed}…") }
}

static ALLOWED_TAG: LazyLock<Regex> =
    LazyLock::new(|| re(r#"(?is)^<(/?)(a|b|strong|i|em|u|br|p|ul|ol|li)(\s[^>]*)?/?>$"#));
static ANY_TAG: LazyLock<Regex> = LazyLock::new(|| re(r"(?s)<[^>]+>"));
static HREF: LazyLock<Regex> = LazyLock::new(|| re(r#"(?i)href="([^"]*)""#));

/// Product descriptions from the CSV are mostly prose, occasionally with a link or bold. Keep only
/// a small allowlist of tags (href attributes preserved, everything else dropped) and wrap
/// paragraphs. Output is safe to render with `|safe`.
pub fn description_html(raw: &str) -> String {
    let decoded = raw.replace("\\n", "\n").replace('\r', "");
    let cleaned = ANY_TAG.replace_all(&decoded, |caps: &regex::Captures| {
        let tag = &caps[0];
        if let Some(m) = ALLOWED_TAG.captures(tag) {
            let close = &m[1];
            let name = m[2].to_lowercase();
            if name == "a" && close.is_empty() {
                let href = HREF.captures(tag).map(|h| h[1].to_string()).unwrap_or_default();
                let href = internalise(&href);
                if href.is_empty() || href.starts_with("javascript:") {
                    return String::new();
                }
                let ext = href.starts_with("http");
                return format!(
                    "<a href=\"{}\"{}>",
                    html_escape::encode_double_quoted_attribute(&href),
                    if ext { " rel=\"noopener\" target=\"_blank\"" } else { "" }
                );
            }
            if name == "br" {
                return "<br>".to_string();
            }
            format!("<{}{}>", close, name)
        } else {
            String::new()
        }
    });
    let text = cleaned.to_string();
    // Paragraphs: if the author already used <p>, trust it; otherwise split on blank lines / newlines.
    if text.contains("<p>") {
        return text;
    }
    let mut out = String::new();
    for para in text.split("\n\n").flat_map(|p| p.split('\n')) {
        let p = para.trim();
        if p.is_empty() {
            continue;
        }
        out.push_str("<p>");
        out.push_str(p);
        out.push_str("</p>\n");
    }
    out
}

/// Turn absolute links to the old site into site-relative paths so the redirect table handles them.
pub fn internalise(href: &str) -> String {
    let h = href.trim();
    for prefix in [
        "https://thepenmarket.com",
        "http://thepenmarket.com",
        "https://www.thepenmarket.com",
        "http://www.thepenmarket.com",
    ] {
        if let Some(rest) = h.strip_prefix(prefix) {
            let rest = if rest.is_empty() { "/" } else { rest };
            return rest.to_string();
        }
    }
    h.to_string()
}

pub fn escape_attr(s: &str) -> String {
    html_escape::encode_double_quoted_attribute(s).to_string()
}
