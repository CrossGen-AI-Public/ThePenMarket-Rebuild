//! Glossary text work with no database in it: the lexicon of words to look for, whole-word matching,
//! pulling his sentences out of a post, and turning the first use of a published term in a listing
//! into a tap-to-define link. Nothing here writes a definition; it only finds and links his words.

use regex::Regex;
use serde::Deserialize;

#[derive(Clone, Debug)]
pub struct LexTerm {
    pub name: String,
    pub aliases: Vec<String>,
    pub topic: String,
    pub kind: String,
    pub brand: String,
}

#[derive(Deserialize)]
struct LexFile {
    groups: Vec<LexGroup>,
}
#[derive(Deserialize)]
struct LexGroup {
    topic: String,
    kind: String,
    #[serde(default)]
    brand: String,
    terms: Vec<String>,
}

/// `data/glossary-lexicon.json`: each entry is "Name|other spelling|other spelling".
pub fn parse_lexicon(json: &str) -> anyhow::Result<Vec<LexTerm>> {
    let file: LexFile = serde_json::from_str(json)?;
    let mut out = vec![];
    for g in file.groups {
        for t in g.terms {
            let mut parts = t.split('|').map(|s| s.trim().to_string()).filter(|s| !s.is_empty());
            let Some(name) = parts.next() else { continue };
            out.push(LexTerm { name, aliases: parts.collect(), topic: g.topic.clone(), kind: g.kind.clone(), brand: g.brand.clone() });
        }
    }
    Ok(out)
}

/// Whole-word matcher for a name and its other spellings. The `regex` crate has no look-around, so
/// the word boundaries are checked by hand: "sac" must not match in "sack", "51" must not match "$51".
#[derive(Clone, Debug)]
pub struct Matcher {
    re: Regex,
}

impl Matcher {
    /// Models match with their capitals ("Balance" the pen, not "balance" the noun); vocabulary does not care.
    pub fn new(names: &[String], case_sensitive: bool) -> Option<Self> {
        let mut names: Vec<&String> = names.iter().filter(|n| n.chars().any(|c| c.is_alphanumeric())).collect();
        if names.is_empty() {
            return None;
        }
        names.sort_by_key(|n| std::cmp::Reverse(n.chars().count()));
        // Either apostrophe, and a space or a hyphen between words ("gold filled" is "gold-filled").
        let alt = |n: &String| -> String {
            n.chars().map(|c| match c {
                '\'' | '’' => "['’]".to_string(),
                ' ' | '-' => r"[\s\u{a0}-]+".to_string(),
                c => regex::escape(&c.to_string()),
            }).collect()
        };
        let alts: Vec<String> = names.iter().map(|n| alt(n)).collect();
        let pattern = format!("{}(?:{})", if case_sensitive { "" } else { "(?i)" }, alts.join("|"));
        Regex::new(&pattern).ok().map(|re| Self { re })
    }

    pub fn find_all(&self, text: &str) -> Vec<(usize, usize)> {
        self.re
            .find_iter(text)
            .filter(|m| {
                let before = text[..m.start()].chars().next_back();
                let after = text[m.end()..].chars().next();
                !before.map(|c| c.is_alphanumeric() || c == '$' || c == '#').unwrap_or(false) && !after.map(|c| c.is_alphanumeric()).unwrap_or(false)
            })
            .map(|m| (m.start(), m.end()))
            .collect()
    }

    pub fn is_match(&self, text: &str) -> bool {
        !self.find_all(text).is_empty()
    }
}

const ABBREVIATIONS: [&str; 14] = ["mr", "mrs", "ms", "dr", "st", "no", "vs", "co", "inc", "jr", "sr", "ca", "approx", "etc"];

/// His sentences, as he wrote them. A full stop after an initial or a common abbreviation does not end one.
pub fn sentences(text: &str) -> Vec<String> {
    let chars: Vec<char> = text.chars().collect();
    let mut out = vec![];
    let mut start = 0;
    let mut i = 0;
    while i < chars.len() {
        let c = chars[i];
        if matches!(c, '.' | '!' | '?') {
            let mut end = i + 1;
            while end < chars.len() && matches!(chars[end], '.' | '!' | '?' | '"' | '”' | '’' | ')') {
                end += 1;
            }
            let at_break = end >= chars.len() || chars[end].is_whitespace();
            let word: String = chars[start..i].iter().rev().take_while(|ch| ch.is_alphanumeric()).collect::<Vec<_>>().into_iter().rev().collect();
            let abbreviation = c == '.' && (word.chars().count() == 1 || ABBREVIATIONS.contains(&word.to_lowercase().as_str()));
            if at_break && !abbreviation {
                let s: String = chars[start..end].iter().collect();
                if !s.trim().is_empty() {
                    out.push(s.trim().to_string());
                }
                start = end;
            }
            i = end;
        } else {
            i += 1;
        }
    }
    let tail: String = chars[start.min(chars.len())..].iter().collect();
    if !tail.trim().is_empty() {
        out.push(tail.trim().to_string());
    }
    out
}

const CUES: [&str; 16] = [" is a ", " is an ", " is the ", " are the ", " are a ", " means ", " meant ", " known as ", " called ", " refers to ", " which is ", " that is ", " because ", " so that ", " in other words", " what is "];

/// How useful a sentence of his is as the start of a definition: it explains rather than mentions.
/// Zero means do not offer it.
pub fn quote_score(sentence: &str, m: &Matcher) -> i32 {
    let len = sentence.chars().count();
    if !(40..=420).contains(&len) {
        return 0;
    }
    let hits = m.find_all(sentence);
    let Some(first) = hits.first() else { return 0 };
    let lower = format!(" {} ", sentence.to_lowercase());
    let mut score = 10;
    score += CUES.iter().filter(|c| lower.contains(*c)).count() as i32 * 2;
    // "Brassing is the wear ...": the explaining verb straight after the word is the strongest sign.
    let after: String = sentence[first.1..].chars().take(14).collect::<String>().to_lowercase();
    if CUES.iter().any(|c| after.starts_with(c.trim_end())) {
        score += 12;
    }
    if first.0 < 30 {
        score += 5;
    }
    if (80..=300).contains(&len) {
        score += 4;
    }
    if sentence.ends_with('?') {
        score -= 6;
    }
    // A sentence that is mostly price, size or stock talk says nothing about the word.
    if lower.contains('$') || lower.contains(" cm ") || lower.contains("in stock") {
        score -= 5;
    }
    score.max(1)
}

/// A published term as the listing linker needs it.
#[derive(Clone, Debug)]
pub struct LinkTerm {
    pub name: String,
    pub url: String,
    pub short: String,
    pub matcher: Matcher,
}

fn tag_name(tag: &str) -> (bool, String) {
    let inner = tag.trim_start_matches('<');
    let closing = inner.starts_with('/');
    let name: String = inner.trim_start_matches('/').chars().take_while(|c| c.is_ascii_alphanumeric()).collect();
    (closing, name.to_lowercase())
}

/// The first use of each published term in a listing's description becomes a link to its entry,
/// carrying the short definition for the popover. Text inside links, headings and buttons is left
/// alone, each term is linked once, and a description gets at most `max_links`.
pub fn link_terms(html: &str, terms: &[LinkTerm], max_links: usize) -> String {
    if terms.is_empty() || max_links == 0 {
        return html.to_string();
    }
    let mut out = String::with_capacity(html.len() + 256);
    let mut used = vec![false; terms.len()];
    let mut linked = 0;
    let mut skip_depth = 0usize;
    let mut rest = html;
    while !rest.is_empty() {
        if rest.starts_with('<') {
            let end = rest.find('>').map(|i| i + 1).unwrap_or(rest.len());
            let tag = &rest[..end];
            let (closing, name) = tag_name(tag);
            if matches!(name.as_str(), "a" | "h1" | "h2" | "h3" | "h4" | "h5" | "h6" | "button" | "script" | "style") && !tag.ends_with("/>") {
                if closing {
                    skip_depth = skip_depth.saturating_sub(1);
                } else {
                    skip_depth += 1;
                }
            }
            out.push_str(tag);
            rest = &rest[end..];
            continue;
        }
        let end = rest.find('<').unwrap_or(rest.len());
        let text = &rest[..end];
        rest = &rest[end..];
        if skip_depth > 0 || linked >= max_links {
            out.push_str(text);
            continue;
        }
        // Every unused term's first hit in this run of text; earliest first, longer wins a tie.
        let mut hits: Vec<(usize, usize, usize)> = terms.iter().enumerate().filter(|(i, _)| !used[*i]).filter_map(|(i, t)| t.matcher.find_all(text).first().map(|(s, e)| (*s, *e, i))).collect();
        hits.sort_by(|a, b| a.0.cmp(&b.0).then((b.1 - b.0).cmp(&(a.1 - a.0))));
        let mut cursor = 0;
        for (s, e, i) in hits {
            if s < cursor || linked >= max_links {
                continue;
            }
            let t = &terms[i];
            out.push_str(&text[cursor..s]);
            out.push_str(&format!("<a class=\"gl\" href=\"{}\" data-gl-name=\"{}\" data-gl=\"{}\">{}</a>", crate::text::escape_attr(&t.url), crate::text::escape_attr(&t.name), crate::text::escape_attr(&t.short), &text[s..e]));
            cursor = e;
            used[i] = true;
            linked += 1;
        }
        out.push_str(&text[cursor..]);
    }
    out
}

/// What a visitor typed, folded for comparing with entry names: case, punctuation and spacing gone.
pub fn fold(s: &str) -> String {
    let lower = s.to_lowercase().replace(['’', '\''], "");
    let cleaned: String = lower.chars().map(|c| if c.is_alphanumeric() { c } else { ' ' }).collect();
    cleaned.split_whitespace().collect::<Vec<_>>().join(" ")
}

#[cfg(test)]
mod tests {
    use super::*;

    fn m(names: &[&str], cs: bool) -> Matcher {
        Matcher::new(&names.iter().map(|s| s.to_string()).collect::<Vec<_>>(), cs).unwrap_or_else(|| panic!("matcher"))
    }

    #[test]
    fn matcher_respects_word_edges_prices_and_case() {
        let sac = m(&["ink sac", "sac"], false);
        assert!(sac.is_match("We installed a new Sac."));
        assert!(!sac.is_match("A sack of pens and a sacrifice."));
        let p51 = m(&["Parker 51", "51"], true);
        assert!(p51.is_match("A lovely Parker 51 in cedar blue"));
        assert!(!p51.is_match("It sold for $51 last year") && !p51.is_match("Lot 1951"));
        let balance = m(&["Balance"], true);
        assert!(balance.is_match("a Sheaffer Balance in marine green") && !balance.is_match("a fine balance in the hand"));
        assert!(m(&["baby's bottom"], false).is_match("a touch of baby’s bottom on the nib"));
        assert!(m(&["gold filled"], false).is_match("a gold-filled cap"));
    }

    #[test]
    fn sentences_keep_initials_and_abbreviations_together() {
        let s = sentences("George S. Parker made pens. Dr. Smith bought one! Did it write? Yes");
        assert_eq!(s, vec!["George S. Parker made pens.", "Dr. Smith bought one!", "Did it write?", "Yes"]);
    }

    #[test]
    fn explaining_sentences_outscore_mentions() {
        let brassing = m(&["brassing"], false);
        let explains = "Brassing is the wear you see when the gold plating rubs through to the brass underneath on a clip or lever.";
        let mentions = "There is a touch of brassing on the clip of this otherwise lovely and well preserved fountain pen.";
        assert!(quote_score(explains, &brassing) > quote_score(mentions, &brassing));
        assert_eq!(quote_score("Brassing.", &brassing), 0);
        assert_eq!(quote_score(mentions, &m(&["crazing"], false)), 0);
    }

    #[test]
    fn linker_links_first_use_once_and_leaves_links_and_headings_alone() {
        let terms = vec![
            LinkTerm { name: "Brassing".into(), url: "/glossary/brassing/".into(), short: "Wear \"through\" the plating.".into(), matcher: m(&["brassing", "brassed"], false) },
            LinkTerm { name: "Ink sac".into(), url: "/glossary/ink-sac/".into(), short: "The rubber bag.".into(), matcher: m(&["ink sac", "sac"], false) },
        ];
        let html = "<h2>Brassing report</h2><p>Light brassing on the clip; more brassing on the lever. See <a href=\"/x/\">our sac notes</a>. New sac fitted.</p>";
        let out = link_terms(html, &terms, 8);
        assert!(out.starts_with("<h2>Brassing report</h2><p>Light <a class=\"gl\" href=\"/glossary/brassing/\""));
        assert_eq!(out.matches("/glossary/brassing/").count(), 1);
        assert!(out.contains("<a href=\"/x/\">our sac notes</a>"));
        assert!(out.contains("New <a class=\"gl\" href=\"/glossary/ink-sac/\""));
        assert!(out.contains("data-gl=\"Wear &quot;through&quot; the plating.\""));
        assert_eq!(link_terms(html, &terms, 1).matches("class=\"gl\"").count(), 1);
        assert_eq!(link_terms(html, &[], 8), html);
    }

    #[test]
    fn fold_makes_lookups_forgiving() {
        assert_eq!(fold("  Baby’s  Bottom? "), "babys bottom");
        assert_eq!(fold("J-Bar"), "j bar");
    }

    #[test]
    fn lexicon_file_parses_and_has_no_duplicate_names() {
        let lex = parse_lexicon(include_str!("../data/glossary-lexicon.json")).unwrap_or_else(|e| panic!("lexicon: {e}"));
        assert!(lex.len() > 300);
        let mut slugs: Vec<String> = lex.iter().map(|t| crate::text::slugify(&t.name)).collect();
        slugs.sort();
        let n = slugs.len();
        slugs.dedup();
        assert_eq!(n, slugs.len(), "two lexicon entries make the same slug");
        assert!(lex.iter().all(|t| t.kind == "term" || t.kind == "model"));
    }
}
