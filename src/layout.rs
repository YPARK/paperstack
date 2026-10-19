//! Where a paper's files go in the library:
//! `year/lastname/firstname/<key>_<slug>.pdf`, its record beside it as
//! `.json`. The year is the year issued; the names are the first author's,
//! the given name or, where only that is known, the initials; the slug is
//! the title's first words, lowercase ASCII.

use crate::mock::Paper;
use std::path::PathBuf;

/// Words of the title in the slug, and its longest.
const SLUG_WORDS: usize = 6;
const SLUG_CHARS: usize = 60;

pub fn pdf(p: &Paper) -> PathBuf {
    stem(p).with_extension("pdf")
}

pub fn record(p: &Paper) -> PathBuf {
    stem(p).with_extension("json")
}

/// The path, from the library's root, without the extension.
fn stem(p: &Paper) -> PathBuf {
    let (family, given) = p.authors.first().copied().unwrap_or(("", ""));
    let mut path = PathBuf::from(p.year.to_string());
    path.push(part(family));
    path.push(part(given));
    path.push(format!("{}_{}", p.key, slug(p.title)));
    path
}

/// A name as a folder: what no file system takes replaced, `_` for none.
fn part(name: &str) -> String {
    let s: String = name
        .trim()
        .chars()
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
    let words = title
        .split(|c: char| !c.is_alphanumeric())
        .map(|w| {
            w.chars()
                .filter(char::is_ascii_alphanumeric)
                .collect::<String>()
                .to_lowercase()
        })
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
        slug.push_str(&w);
    }
    slug.truncate(SLUG_CHARS);
    slug
}
