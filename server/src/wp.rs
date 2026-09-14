//! WordPress import helpers: media path resolution, HTML rewriting for imported content, and the
//! labelled-field parsing his product and Trading Post texts use ("Era: 1950-1959", "Price: $399").

use crate::{media, text};
use regex::Regex;
use std::collections::HashMap;
use std::path::PathBuf;

pub fn re(p: &'static str) -> Regex {
    Regex::new(p).unwrap_or_else(|e| panic!("static regex {p}: {e}"))
}

/// Lower-case alphanumerics only, with the words that vary between his texts and the term names removed.
pub fn norm(s: &str) -> String {
    let d = text::decode(s).to_lowercase().replace('’', "").replace('\'', "");
    let d = d.replace(" filler", "").replace("fountain pen ", "");
    d.chars().filter(|c| c.is_ascii_alphanumeric()).collect()
}

pub fn nib_alias(s: &str) -> String {
    let n = norm(s);
    match n.as_str() {
        "f" => "fine".into(),
        "m" => "medium".into(),
        "b" => "broad".into(),
        "ef" | "xf" | "extrafine" => "extrafine".into(),
        "bb" | "doublebroad" => "bb".into(),
        "sf" | "semiflex" | "semiflexible" => "semiflexible".into(),
        "flexi" | "flex" | "flexible" => "flexible".into(),
        "o" | "ob" => "oblique".into(),
        "s" => "stub".into(),
        "a" => "accountant".into(),
        "mediumfine" | "finemedium" => "fine".into(),
        _ => n,
    }
}

pub fn mech_alias(s: &str) -> String {
    let n = norm(s);
    match n.as_str() {
        "lever" => "lever".into(),
        "button" => "button".into(),
        "telescopingpiston" | "pistonfiller" | "piston" => "piston".into(),
        "aerometric" => "aerometric".into(),
        "touchdown" => "touchdown".into(),
        "crescent" => "crescent".into(),
        "vacuumfil" | "vacuumfiller" | "plunger" | "plungerfiller" => "vacuumfil".into(),
        "bulbpostal" | "bulb" | "postal" => "bulbpostal".into(),
        "cartridgeconverter" | "cartridge" | "converter" | "cc" => "cartridgeconverter".into(),
        _ => n,
    }
}

pub fn era_alias(s: &str) -> String {
    let n = norm(s);
    match n.as_str() {
        "1980present" | "1980topresent" | "1980" => "1980present".into(),
        "pre1900" | "before1900" => "pre1900".into(),
        _ => n,
    }
}

/// The path after `wp-content/uploads/` in an upload URL, query string dropped.
pub fn upload_rel(url: &str) -> Option<String> {
    let idx = url.find("wp-content/uploads/")?;
    let rest = &url[idx + "wp-content/uploads/".len()..];
    let rest = rest.split('?').next().unwrap_or(rest);
    if rest.is_empty() {
        return None;
    }
    Some(rest.to_string())
}

/// Split a flat text on labels ("Era:", "Price:") and return label -> value, where a value runs up to the
/// next label. Labels are matched case-insensitively; the first occurrence of each wins.
pub fn labelled_fields(flat: &str, labels: &[&str]) -> HashMap<String, String> {
    let mut hits: Vec<(usize, usize, String)> = vec![]; // (start, end, label)
    for label in labels {
        let pat = format!("(?i){}\\s*:", regex::escape(label));
        if let Ok(rx) = Regex::new(&pat) {
            for m in rx.find_iter(flat) {
                hits.push((m.start(), m.end(), label.to_lowercase()));
            }
        }
    }
    hits.sort_by_key(|h| h.0);
    // Drop overlapping matches ("Nib" inside "Nib Size").
    let mut clean: Vec<(usize, usize, String)> = vec![];
    for h in hits {
        if let Some(last) = clean.last() {
            if h.0 < last.1 {
                continue;
            }
        }
        clean.push(h);
    }
    let mut out = HashMap::new();
    for (i, (_, end, label)) in clean.iter().enumerate() {
        let stop = clean.get(i + 1).map(|n| n.0).unwrap_or(flat.len());
        let val = flat[*end..stop].trim().to_string();
        out.entry(label.clone()).or_insert(val);
    }
    out
}

/// His product short description: "Pre-Owned Pens: X / Filling Mechanism: Y / Era: Z / Nib Size: W / SKU: N".
pub fn parse_fields(short: &str) -> HashMap<String, String> {
    let t = text::strip_tags(&short.replace("\\n", "\n").replace("<br", "\n<br"));
    let raw = labelled_fields(&t, &["Filling Mechanism", "Fountain Pen Nib Size", "Nib Size", "Nib", "Era", "SKU"]);
    let mut out = HashMap::new();
    for (k, v) in raw {
        let key = if k.contains("nib") { "nib" } else if k.starts_with("filling") { "mechanism" } else if k == "era" { "era" } else { "sku" };
        let first_line = v.lines().next().unwrap_or("").trim().to_string();
        out.entry(key.to_string()).or_insert(first_line);
    }
    out
}

pub struct MediaResolver {
    pub backup_uploads: PathBuf,
    pub media_dir: PathBuf,
    pub cache: HashMap<String, Option<String>>,
}

impl MediaResolver {
    /// For a path relative to `wp-content/uploads/`, return the served media path (`uploads/...`),
    /// importing from the backup (copy + 480/960 variants) or accepting a file already recovered.
    pub fn resolve(&mut self, rel_in: &str) -> Option<String> {
        if let Some(hit) = self.cache.get(rel_in) {
            return hit.clone();
        }
        let sized = re(r"-\d+x\d+(\.[A-Za-z0-9]+)$");
        let orig = sized.replace(rel_in, "$1").to_string();
        let mut candidates = vec![rel_in.to_string()];
        if orig != rel_in {
            candidates.push(orig.clone());
        }
        if let Some(dot) = orig.rfind('.') {
            candidates.push(format!("{}-scaled{}", &orig[..dot], &orig[dot..]));
        }
        let mut found: Option<String> = None;
        for c in candidates {
            let src = self.backup_uploads.join(&c);
            let rel_out = format!("uploads/{c}");
            if src.exists() {
                match media::import_image(&src, &self.media_dir, &rel_out) {
                    Ok(_) => {
                        found = Some(rel_out);
                        break;
                    }
                    Err(e) => eprintln!("media: {} failed: {e}", src.display()),
                }
            }
            if self.media_dir.join(&rel_out).exists() {
                found = Some(rel_out);
                break;
            }
        }
        self.cache.insert(rel_in.to_string(), found.clone());
        found
    }
}

#[derive(Default)]
pub struct RewriteStats {
    pub images_kept: i64,
    pub images_dropped: i64,
    pub dropped: Vec<String>,
    pub iframes_dropped: i64,
}

/// Rewrite imported WordPress HTML for the new site: local media, relative internal links, no scripts,
/// only YouTube iframes kept. Images that exist nowhere (404 on the live site, absent from the backup and
/// from the Wayback recovery) are removed and counted.
pub fn rewrite_html(html: &str, resolver: &mut MediaResolver, stats: &mut RewriteStats, label: &str) -> String {
    let unwrap_anchor = re(r#"(?is)<a\b[^>]*href="[^"]*wp-content/uploads/[^"]*"[^>]*>\s*(<img\b[^>]*>)\s*</a>"#);
    let img_tag = re(r#"(?is)<img\b[^>]*>"#);
    let src_attr = re(r#"(?i)\bsrc="([^"]*)""#);
    let attr = re(r#"(?is)\b(alt|width|height|class|title)="([^"]*)""#);
    let script = re(r"(?is)<script\b.*?</script>|<style\b.*?</style>|<noscript\b.*?</noscript>");
    let iframe = re(r#"(?is)<iframe\b[^>]*>.*?</iframe>"#);
    let href = re(r#"(?i)href="(https?://(?:www\.)?thepenmarket\.com[^"]*)""#);
    let vpb = re(r#"href="/vintage-pens-blog/([^"]*)""#);

    let h = script.replace_all(html, "");
    let h = unwrap_anchor.replace_all(&h, "$1");
    let h = iframe.replace_all(&h, |caps: &regex::Captures| {
        let tag = &caps[0];
        if tag.contains("youtube.com/embed") || tag.contains("youtube-nocookie.com/embed") {
            tag.to_string()
        } else {
            stats.iframes_dropped += 1;
            String::new()
        }
    });
    let h = img_tag.replace_all(&h, |caps: &regex::Captures| {
        let tag = &caps[0];
        let src = src_attr.captures(tag).map(|c| c[1].to_string()).unwrap_or_default();
        let resolved = upload_rel(&src).and_then(|r| resolver.resolve(&r));
        match resolved {
            Some(rel_out) => {
                stats.images_kept += 1;
                let mut extra = String::new();
                for a in attr.captures_iter(tag) {
                    let name = a[1].to_lowercase();
                    let val = &a[2];
                    if name == "class" && !val.contains("align") && !val.contains("wp-image") && !val.contains("size-") {
                        continue;
                    }
                    extra.push_str(&format!(" {}=\"{}\"", name, text::escape_attr(&text::decode(val))));
                }
                format!("<img src=\"/media/{}\" loading=\"lazy\" decoding=\"async\"{}>", rel_out, extra)
            }
            None => {
                stats.images_dropped += 1;
                if stats.dropped.len() < 2000 {
                    stats.dropped.push(format!("{label}: {src}"));
                }
                String::new()
            }
        }
    });
    let h = href.replace_all(&h, |caps: &regex::Captures| format!("href=\"{}\"", text::internalise(&caps[1])));
    let h = vpb.replace_all(&h, "href=\"/$1\"");
    h.to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_his_short_description() {
        let f = parse_fields("Pre-Owned Pens:<b> Waterman Charleston</b><br />\nFilling Mechanism: <b>Cartridge/Converter</b><br />\nEra: <b>1980-present</b><br />\nNib Size:<b> Medium<br />\n</b>SKU: <b>6972</b>");
        assert_eq!(f.get("mechanism").map(String::as_str), Some("Cartridge/Converter"));
        assert_eq!(f.get("era").map(String::as_str), Some("1980-present"));
        assert_eq!(f.get("nib").map(String::as_str), Some("Medium"));
        assert_eq!(f.get("sku").map(String::as_str), Some("6972"));
    }

    #[test]
    fn parses_csv_style_with_escaped_newlines() {
        let f = parse_fields("Pre-Owned Pens:<b>\u{a0}Mont Blanc Louis XIV</b>\n\\nFilling Mechanism: <b>Piston</b>\n\\nEra: <b>1980-present</b>\n\\nNib Size: <b>F\n\\n</b>SKU: <b>5466</b>");
        assert_eq!(f.get("mechanism").map(String::as_str), Some("Piston"));
        assert_eq!(f.get("nib").map(String::as_str), Some("F"));
        assert_eq!(f.get("sku").map(String::as_str), Some("5466"));
    }

    #[test]
    fn trading_post_fields() {
        let f = labelled_fields("Era: 2010 Price: 225. 00 Contact: bsheeter@gmail.com Description: Montblanc fountain pen. Medium nib.", &["Era", "Price", "Contact", "Description"]);
        assert_eq!(f.get("price").map(String::as_str), Some("225. 00"));
        assert_eq!(f.get("description").map(String::as_str), Some("Montblanc fountain pen. Medium nib."));
    }

    #[test]
    fn upload_paths() {
        assert_eq!(upload_rel("https://thepenmarket.com/vintage-pens-blog/wp-content/uploads/2013/09/x.jpg?lossy=1").as_deref(), Some("2013/09/x.jpg"));
        assert_eq!(upload_rel("https://example.com/a.jpg"), None);
    }
}
