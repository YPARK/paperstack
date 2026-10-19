//! Where a paper's files go in the library:
//! `year/lastname/firstname/<key>_<slug>.pdf`, its record beside it as
//! `.json`. The year is the year issued; the names are the first author's,
//! the given name or, where only that is known, the initials; the slug is
//! the title's first words, lowercase ASCII.

use crate::record::Record;
use std::path::PathBuf;
use unicode_normalization::UnicodeNormalization;

/// Words of the title in the slug, and its longest.
const SLUG_WORDS: usize = 6;
const SLUG_CHARS: usize = 60;

pub fn pdf(r: &Record) -> PathBuf {
    stem(r).with_extension("pdf")
}

pub fn record(r: &Record) -> PathBuf {
    stem(r).with_extension("json")
}

/// The path, from the library's root, without the extension.
fn stem(r: &Record) -> PathBuf {
    let first = r.authors().into_iter().next();
    let (family, given) = first.map_or_else(Default::default, |n| (n.family, n.given));
    let year = r
        .year()
        .map_or_else(|| "n.d.".to_string(), |y| y.to_string());
    let mut path = PathBuf::from(year);
    path.push(part(&family));
    path.push(part(&initials(&given).unwrap_or(given)));
    path.push(format!("{}_{}", r.key, slug(r.title())));
    path
}

/// Given names that are initials alone, `B. P.` or `B.-P.`, as `BP`, as
/// a reference manager writes them.
fn initials(given: &str) -> Option<String> {
    let letters: Vec<&str> = given
        .split(|c: char| c.is_whitespace() || c == '.' || c == '-')
        .filter(|p| !p.is_empty())
        .collect();
    (!letters.is_empty() && letters.iter().all(|p| p.chars().count() == 1))
        .then(|| letters.concat())
}

/// A name as a folder, in NFC: what no file system takes replaced, `_` for
/// none.
fn part(name: &str) -> String {
    let s: String = name
        .trim()
        .nfc()
        .map(|c| match c {
            '/' | '\\' | ':' | '*' | '?' | '"' | '<' | '>' | '|' => '-',
            c if c.is_control() => '-',
            c => c,
        })
        .collect();
    let s = s.trim_matches('.').to_string();
    if s.is_empty() { "_".to_string() } else { s }
}

/// `deep-learning-based-gene-perturbation-effect`: the title's first words,
/// lowercase, in ASCII letters and digits.
pub fn slug(title: &str) -> String {
    let ascii = deunicode::deunicode(title).to_lowercase();
    let words = ascii
        .split(|c: char| !c.is_ascii_alphanumeric())
        .filter(|w| !w.is_empty())
        .take(SLUG_WORDS);
    let mut slug = String::new();
    for w in words {
        if !slug.is_empty() && slug.len() + 1 + w.len() > SLUG_CHARS {
            break;
        }
        if !slug.is_empty() {
            slug.push('-');
        }
        slug.push_str(w);
    }
    slug.truncate(SLUG_CHARS);
    slug
}
