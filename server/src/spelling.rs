//! Search spellings. Nathaniel keeps a list of the ways people actually type a word (Schaffer,
//! Shaeffer, Mont Blanc) and the way he spells it (Sheaffer, Montblanc); a search for any one of them
//! looks for all of them, so a listing written either way is found. When a search still finds
//! nothing, a near miss on a make's name (one or two letters off) is tried before giving up.

use serde::{Deserialize, Serialize};

/// One word on his list: his spelling, then the other ways people type it.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct Spelling {
    pub right: String,
    pub wrong: Vec<String>,
}

impl Spelling {
    fn members(&self) -> impl Iterator<Item = &String> {
        std::iter::once(&self.right).chain(self.wrong.iter())
    }
}

/// The examples in his e-mail of 2026-10-07, in his words. The admin list replaces these once saved.
pub fn default_list() -> Vec<Spelling> {
    vec![
        Spelling { right: "Sheaffer".into(), wrong: vec!["Schaffer".into(), "Shaeffer".into(), "Schaeffer".into(), "Shaffer".into()] },
        Spelling { right: "Montblanc".into(), wrong: vec!["Mont Blanc".into()] },
    ]
}

/// One word per line: `Sheaffer = Schaffer, Shaeffer, Schaeffer`. A colon works as well as `=`.
/// Blank lines and lines starting with `#` are skipped; a line with no other spellings is dropped.
pub fn parse(text: &str) -> Vec<Spelling> {
    let mut out: Vec<Spelling> = vec![];
    for line in text.lines().map(str::trim).filter(|l| !l.is_empty() && !l.starts_with('#')) {
        let Some((right, rest)) = line.split_once('=').or_else(|| line.split_once(':')) else { continue };
        let right = clean(right);
        if right.is_empty() {
            continue;
        }
        let mut wrong: Vec<String> = vec![];
        for w in rest.split([',', ';']).map(clean).filter(|w| !w.is_empty()) {
            if !w.eq_ignore_ascii_case(&right) && !wrong.iter().any(|x| x.eq_ignore_ascii_case(&w)) {
                wrong.push(w);
            }
        }
        if !wrong.is_empty() {
            out.push(Spelling { right, wrong });
        }
        if out.len() >= 500 {
            break;
        }
    }
    out
}

fn clean(s: &str) -> String {
    s.split_whitespace().collect::<Vec<_>>().join(" ").chars().take(60).collect()
}

/// The list as the admin text box shows it.
pub fn render(list: &[Spelling]) -> String {
    list.iter().map(|s| format!("{} = {}", s.right, s.wrong.join(", "))).collect::<Vec<_>>().join("\n")
}

#[derive(Debug, Default, PartialEq)]
pub struct Expanded {
    /// Every spelling of the search, the visitor's own first. One entry when nothing on the list matched.
    pub variants: Vec<String>,
    /// The search in his spelling, when the visitor typed one of the other ways.
    pub corrected: Option<String>,
}

const MAX_VARIANTS: usize = 12;

fn tokens(s: &str) -> Vec<String> {
    s.split_whitespace().map(str::to_string).collect()
}

fn word_key(w: &str) -> String {
    w.trim_matches(|c: char| !c.is_alphanumeric()).to_lowercase()
}

/// Where `needle` (a spelling, maybe several words) first sits in `hay`, whole words, any case.
fn find(hay: &[String], needle: &[String]) -> Option<usize> {
    if needle.is_empty() || needle.len() > hay.len() {
        return None;
    }
    (0..=hay.len() - needle.len()).find(|&i| needle.iter().enumerate().all(|(j, n)| word_key(&hay[i + j]) == word_key(n)))
}

fn splice(hay: &[String], at: usize, len: usize, with: &str) -> String {
    let mut out: Vec<String> = hay[..at].to_vec();
    out.extend(tokens(with));
    out.extend(hay[at + len..].iter().cloned());
    out.join(" ")
}

/// A search with every spelling on his list swapped in.
pub fn expand(q: &str, list: &[Spelling]) -> Expanded {
    let q = q.split_whitespace().collect::<Vec<_>>().join(" ");
    if q.is_empty() {
        return Expanded::default();
    }
    let mut variants = vec![q.clone()];
    let mut corrected = q.clone();
    for s in list {
        // Longest spelling first, so "Mont Blanc" is seen before a one-word spelling inside it.
        let mut members: Vec<&String> = s.members().collect();
        members.sort_by_key(|m| std::cmp::Reverse(m.split_whitespace().count()));
        let mut next: Vec<String> = vec![];
        for v in &variants {
            let hay = tokens(v);
            let hit = members.iter().find_map(|m| find(&hay, &tokens(m)).map(|at| (at, tokens(m).len())));
            match hit {
                Some((at, len)) => {
                    for m in s.members() {
                        let mut swapped = splice(&hay, at, len, m);
                        if swapped.eq_ignore_ascii_case(v) {
                            swapped = v.clone();
                        }
                        if !next.iter().any(|x| x.eq_ignore_ascii_case(&swapped)) {
                            next.push(swapped);
                        }
                    }
                }
                None => next.push(v.clone()),
            }
        }
        // The visitor's own words stay first.
        if let Some(pos) = next.iter().position(|x| x.eq_ignore_ascii_case(&q)) {
            let own = next.remove(pos);
            next.insert(0, own);
        }
        next.truncate(MAX_VARIANTS);
        variants = next;
        let hay = tokens(&corrected);
        if let Some((at, len)) = s.wrong.iter().find_map(|w| find(&hay, &tokens(w)).map(|at| (at, tokens(w).len()))) {
            corrected = splice(&hay, at, len, &s.right);
        }
    }
    Expanded { corrected: (!corrected.eq_ignore_ascii_case(&q)).then_some(corrected), variants }
}

/// Edit distance with adjacent swaps counted as one ("Sheafer", "Shaeffer").
pub fn distance(a: &str, b: &str) -> usize {
    let a: Vec<char> = a.chars().collect();
    let b: Vec<char> = b.chars().collect();
    let (n, m) = (a.len(), b.len());
    let mut d = vec![vec![0usize; m + 1]; n + 1];
    for (i, row) in d.iter_mut().enumerate() {
        row[0] = i;
    }
    for (j, cell) in d[0].iter_mut().enumerate() {
        *cell = j;
    }
    for i in 1..=n {
        for j in 1..=m {
            let cost = usize::from(a[i - 1] != b[j - 1]);
            d[i][j] = (d[i - 1][j] + 1).min(d[i][j - 1] + 1).min(d[i - 1][j - 1] + cost);
            if i > 1 && j > 1 && a[i - 1] == b[j - 2] && a[i - 2] == b[j - 1] {
                d[i][j] = d[i][j].min(d[i - 2][j - 2] + 1);
            }
        }
    }
    d[n][m]
}

/// For a search that found nothing: each word that is a near miss on one of `names` (the makes in the
/// catalog) is replaced by that name. Short words are left alone; four letters can be anything.
pub fn near_miss(q: &str, names: &[String]) -> Option<String> {
    let keys: Vec<(String, &String)> = names.iter().map(|n| (n.to_lowercase().replace([' ', '-', '.'], ""), n)).filter(|(k, _)| k.chars().count() >= 4).collect();
    let words = tokens(q);
    let mut changed = false;
    let mut out: Vec<String> = vec![];
    let mut i = 0;
    while i < words.len() {
        // A two-word near miss ("mont blank") is tried before a single word.
        let pair = (i + 1 < words.len()).then(|| format!("{}{}", word_key(&words[i]), word_key(&words[i + 1])));
        let best = |w: &str| -> Option<&String> {
            let len = w.chars().count();
            if len < 5 || keys.iter().any(|(k, _)| k == w) {
                return None;
            }
            let allowed = if len >= 7 { 2 } else { 1 };
            keys.iter().map(|(k, n)| (distance(w, k), *n)).filter(|(d, _)| *d <= allowed).min_by_key(|(d, _)| *d).map(|(_, n)| n)
        };
        if let Some(n) = pair.as_deref().and_then(best) {
            out.push(n.clone());
            changed = true;
            i += 2;
            continue;
        }
        match best(&word_key(&words[i])) {
            Some(n) => {
                out.push(n.clone());
                changed = true;
            }
            None => out.push(words[i].clone()),
        }
        i += 1;
    }
    changed.then(|| out.join(" "))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_his_list() {
        let list = parse("Sheaffer = Schaffer, Shaeffer,Schaeffer ,Shaffer\n\n# comment\nMontblanc: Mont Blanc\nParker =\nWaterman = waterman, Watermen");
        assert_eq!(list.len(), 3);
        assert_eq!(list[0].wrong, vec!["Schaffer", "Shaeffer", "Schaeffer", "Shaffer"]);
        assert_eq!(list[1], Spelling { right: "Montblanc".into(), wrong: vec!["Mont Blanc".into()] });
        assert_eq!(list[2].wrong, vec!["Watermen"]);
        assert_eq!(parse(&render(&list)), list);
    }

    #[test]
    fn a_misspelling_searches_every_spelling() {
        let e = expand("schaffer snorkel", &default_list());
        assert_eq!(e.variants[0], "schaffer snorkel");
        assert!(e.variants.iter().any(|v| v == "Sheaffer snorkel"));
        assert!(e.variants.iter().any(|v| v == "Shaffer snorkel"));
        assert_eq!(e.corrected.as_deref(), Some("Sheaffer snorkel"));
    }

    #[test]
    fn both_ways_for_montblanc() {
        let e = expand("Mont Blanc 146", &default_list());
        assert!(e.variants.contains(&"Montblanc 146".to_string()));
        assert_eq!(e.corrected.as_deref(), Some("Montblanc 146"));
        // His own spelling is not "corrected", but listings that say Mont Blanc are still found.
        let e = expand("montblanc", &default_list());
        assert_eq!(e.corrected, None);
        assert!(e.variants.contains(&"Mont Blanc".to_string()));
    }

    #[test]
    fn words_not_on_the_list_are_left_alone() {
        let e = expand("parker 51", &default_list());
        assert_eq!(e.variants, vec!["parker 51"]);
        assert_eq!(e.corrected, None);
        // Whole words only: "Montblancs" is not "Montblanc".
        assert_eq!(expand("schafferish", &default_list()).variants.len(), 1);
    }

    #[test]
    fn near_misses_on_makes() {
        let names = vec!["Sheaffer".to_string(), "Waterman".to_string(), "Mont Blanc".to_string(), "Parker".to_string(), "Pelikan".to_string()];
        assert_eq!(near_miss("sheafer", &names).as_deref(), Some("Sheaffer"));
        assert_eq!(near_miss("watreman lever", &names).as_deref(), Some("Waterman lever"));
        assert_eq!(near_miss("mont blank", &names).as_deref(), Some("Mont Blanc"));
        assert_eq!(near_miss("pelican 400", &names).as_deref(), Some("Pelikan 400"));
        assert_eq!(near_miss("parker", &names), None);
        assert_eq!(near_miss("pen", &names), None);
    }
}
