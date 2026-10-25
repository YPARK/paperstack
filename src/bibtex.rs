//! Records as plain BibTeX, as reference managers write it: `journal` and `year`, which
//! BibTeX, BibLaTeX and pandoc all read. After ohp's `csl.rs`, which writes
//! BibLaTeX for typst.

use crate::record::Record;
use regex::Regex;
use serde_json::Value;
use std::collections::HashSet;
use std::sync::LazyLock;

pub fn entries(records: &[&Record]) -> String {
    records
        .iter()
        .map(|r| entry(r))
        .collect::<Vec<_>>()
        .join("\n")
}

pub fn entry(r: &Record) -> String {
    let item = &r.csl;
    let kind = r.csl_type();
    let (entry, container) = match kind {
        "article" | "article-journal" | "article-magazine" | "article-newspaper" | "review" => {
            ("article", "journal")
        }
        "book" => ("book", "series"),
        "chapter" | "entry" | "entry-dictionary" | "entry-encyclopedia" => {
            ("incollection", "booktitle")
        }
        "paper-conference" => ("inproceedings", "booktitle"),
        "report" => ("techreport", "institution"),
        "thesis" => ("phdthesis", "school"),
        "manuscript" | "posted-content" => ("unpublished", "howpublished"),
        _ => ("misc", "howpublished"),
    };
    let mut fields: Vec<(&str, String)> = Vec::new();
    for (csl, bib) in [("author", "author"), ("editor", "editor")] {
        if let Some(names) = item.get(csl).and_then(names) {
            fields.push((bib, names));
        }
    }
    for (csl, bib) in [
        ("title", "title"),
        ("container-title", container),
        ("volume", "volume"),
        ("issue", "number"),
        ("number", "number"),
        ("page", "pages"),
        ("edition", "edition"),
        ("publisher", "publisher"),
        ("publisher-place", "address"),
        ("ISBN", "isbn"),
        ("ISSN", "issn"),
        ("PMID", "pmid"),
        ("PMCID", "pmcid"),
        ("language", "language"),
        ("keyword", "keywords"),
        ("abstract", "abstract"),
        ("note", "note"),
    ] {
        if fields.iter().any(|(f, _)| *f == bib) {
            continue;
        }
        if let Some(value) = item.get(csl).and_then(text) {
            let value = match csl {
                "title" => title(&value, item.get("abstract").and_then(text).as_deref()),
                "container-title" => escape(&venue(&value)),
                _ => escape(&value),
            };
            fields.push((bib, value));
        }
    }
    if let Some(year) = r.year() {
        fields.push(("year", year.to_string()));
    }
    if let Some(month) = item
        .get("issued")
        .and_then(|d| d.get("date-parts")?.get(0)?.get(1)?.as_i64())
    {
        fields.push(("month", month.to_string()));
    }
    for (csl, bib) in [("DOI", "doi"), ("URL", "url")] {
        if let Some(value) = item.get(csl).and_then(text) {
            fields.push((bib, value.replace(['{', '}'], "")));
        }
    }
    if let Some(id) = &r.paperstack.arxiv {
        fields.push(("eprint", id.clone()));
        fields.push(("archivePrefix", "arXiv".to_string()));
    }
    let mut out = format!("@{entry}{{{},\n", r.key);
    for (field, value) in fields {
        out.push_str(&format!("  {field} = {{{value}}},\n"));
    }
    out.push_str("}\n");
    out
}

/// A string or a number, or the first of a list, as text.
fn text(value: &Value) -> Option<String> {
    let text = match value {
        Value::String(s) => s.trim().to_string(),
        Value::Number(n) => n.to_string(),
        Value::Array(a) => return a.first().and_then(text),
        _ => return None,
    };
    (!text.is_empty()).then_some(text)
}

/// Names joined by `and`, each as `von Last, Jr, First`, or kept whole in
/// braces where it is a literal, as an organisation's name is.
fn names(value: &Value) -> Option<String> {
    let names: Vec<String> = value
        .as_array()?
        .iter()
        .filter_map(|name| {
            if let Some(literal) = name.get("literal").and_then(text) {
                return Some(format!("{{{}}}", escape(&literal)));
            }
            let part = |key| name.get(key).and_then(text).map(|s| escape(&s));
            let particle = [part("dropping-particle"), part("non-dropping-particle")]
                .into_iter()
                .flatten()
                .collect::<Vec<_>>()
                .join(" ");
            let family = match part("family") {
                Some(f) if particle.is_empty() => f,
                Some(f) => format!("{particle} {f}"),
                None => return part("given").map(|g| format!("{{{g}}}")),
            };
            // A family name of several words, as a consortium's, kept whole.
            let family = if part("given").is_none() && family.contains(' ') {
                format!("{{{family}}}")
            } else {
                family
            };
            Some(
                [Some(family), part("suffix"), part("given")]
                    .into_iter()
                    .flatten()
                    .collect::<Vec<_>>()
                    .join(", "),
            )
        })
        .collect();
    (!names.is_empty()).then(|| names.join(" and "))
}

/// Text in a BibTeX field, with what BibTeX would read as markup escaped.
fn escape(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for c in s.chars() {
        match c {
            '{' | '}' | '$' | '%' | '&' | '#' | '_' => {
                out.push('\\');
                out.push(c);
            }
            '\\' => out.push_str("\\textbackslash{}"),
            '~' => out.push_str("\\textasciitilde{}"),
            '^' => out.push_str("\\textasciicircum{}"),
            c => out.push(c),
        }
    }
    out
}

/// A title for citeproc, as pandoc's, which lowers in it what is not
/// braced: braced whole, as imported, unbraced, and braced in part as
/// given; its HTML as LaTeX; and the words whose capitals are the authors'
/// braced. In a title in title case, those are the words with capitals
/// past their first letter, as `eQTLs`, and the names its `abstract`
/// gives, by [`names_in`]; in one in sentence case, any word with a
/// capital, but the first.
pub fn title(raw: &str, abstract_text: Option<&str>) -> String {
    let pieces = pieces(crate::record::unbraced(raw.trim()));
    let words = pieces
        .iter()
        .filter_map(|p| match p {
            Piece::Text(t) => Some(t.split_whitespace()),
            _ => None,
        })
        .flatten()
        .skip(1)
        .filter(|w| w.chars().filter(|c| c.is_alphabetic()).count() > 3 && !capitals_within(w));
    let (upper, lower) = words.fold((0, 0), |(u, l), w| match first_letter(w) {
        Some(c) if c.is_uppercase() => (u + 1, l),
        Some(_) => (u, l + 1),
        None => (u, l),
    });
    let title_case = upper > lower;
    let names = if title_case {
        names_in(abstract_text.unwrap_or(""))
    } else {
        HashSet::new()
    };
    let case = Case { title_case, names };
    let mut first = true;
    let mut out = String::new();
    for piece in pieces {
        match piece {
            Piece::Text(t) => out.push_str(&protect(&t, &case, &mut first)),
            Piece::Braced(t) => {
                out.push('{');
                out.push_str(t);
                out.push('}');
                first = false;
            }
            Piece::Math(m) => {
                out.push_str(m);
                first = false;
            }
            Piece::Markup(command, t) => {
                // A gene's or a species' name, set in italics, kept as it is.
                let inner = protect(&t, &Case::default(), &mut false);
                out.push_str(&format!("\\{command}{{{inner}}}"));
                first = false;
            }
        }
    }
    out
}

/// A part of a title: text; what was braced in it, kept as BibTeX; math,
/// `$\beta$`, kept as LaTeX; or what HTML set apart, with the LaTeX
/// command that does.
enum Piece<'a> {
    Text(String),
    Braced(&'a str),
    Math(&'a str),
    Markup(&'static str, String),
}

fn pieces(s: &str) -> Vec<Piece<'_>> {
    static TAG: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"^<(/?)([A-Za-z]+)[^>]*>").unwrap());
    // The text so far, its entities read, as a piece, where there is any.
    fn flush<'a>(pieces: &mut Vec<Piece<'a>>, text: &mut String) {
        if !text.is_empty() {
            pieces.push(Piece::Text(crate::resolve::unescape(&std::mem::take(text))));
        }
    }
    let mut pieces = Vec::new();
    let mut text = String::new();
    let mut at = 0;
    while let Some(c) = s[at..].chars().next() {
        if let Some(len) = crate::record::braced_len(&s[at..]) {
            flush(&mut pieces, &mut text);
            pieces.push(Piece::Braced(&s[at + 1..at + len - 1]));
            at += len;
            continue;
        }
        if let Some(len) = math_len(&s[at..]) {
            flush(&mut pieces, &mut text);
            pieces.push(Piece::Math(&s[at..at + len]));
            at += len;
            continue;
        }
        if c == '<'
            && let Some(tag) = TAG.captures(&s[at..])
        {
            let whole = tag[0].len();
            let name = tag[2].to_ascii_lowercase();
            let command = match name.as_str() {
                "i" | "em" => Some("textit"),
                "b" | "strong" => Some("textbf"),
                "sup" => Some("textsuperscript"),
                "sub" => Some("textsubscript"),
                "sc" => Some("textsc"),
                _ => None,
            };
            let close = format!("</{name}>");
            let end = s[at + whole..].to_ascii_lowercase().find(&close);
            match (command, &tag[1], end) {
                (Some(command), "", Some(end)) => {
                    let inner = &s[at + whole..at + whole + end];
                    flush(&mut pieces, &mut text);
                    pieces.push(Piece::Markup(command, plain(inner)));
                    at += whole + end + close.len();
                }
                // Any other tag, or one not closed, dropped.
                _ => at += whole,
            }
            continue;
        }
        text.push(c);
        at += c.len_utf8();
    }
    flush(&mut pieces, &mut text);
    pieces
}

/// The length of the math `s` starts with, as `$\beta$`, its dollars
/// with it: no space within them at either end, as there is after a price.
fn math_len(s: &str) -> Option<usize> {
    let inner = s.strip_prefix('$')?;
    let end = inner.find('$')?;
    let math = &inner[..end];
    let spaced = math.starts_with(char::is_whitespace) || math.ends_with(char::is_whitespace);
    (!math.is_empty() && !spaced).then_some(end + 2)
}

/// Text within HTML, its tags dropped and its entities read.
fn plain(s: &str) -> String {
    static TAG: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"<[^>]*>").unwrap());
    crate::resolve::unescape(&TAG.replace_all(s, ""))
}

/// How a title is cased, as [`title`] tells.
#[derive(Default)]
struct Case {
    title_case: bool,
    /// Names, lowercase, its words that are names are known by.
    names: HashSet<String>,
}

/// The words an abstract gives with a capital within a sentence, and never
/// without, as names are: lowercase.
fn names_in(text: &str) -> HashSet<String> {
    let (mut capital, mut small) = (HashSet::new(), HashSet::new());
    let mut starts = true;
    for w in text.split_whitespace() {
        if let Some(letter) = first_letter(w) {
            let k = word_key(w);
            if letter.is_lowercase() {
                small.insert(k);
            } else if !starts {
                capital.insert(k);
            }
        }
        starts = w.ends_with(['.', '!', '?', ':']);
    }
    capital.retain(|w| !small.contains(w));
    capital
}

/// A word as [`names_in`] keeps it: lowercase, without the punctuation
/// around it.
fn word_key(w: &str) -> String {
    w.trim_matches(|c: char| !c.is_alphanumeric())
        .to_lowercase()
}

/// `text` escaped, its words braced where their capitals are to be kept:
/// as [`title`] says, `first` while no word has come before.
fn protect(text: &str, case: &Case, first: &mut bool) -> String {
    static WORD: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"\S+|\s+").unwrap());
    let mut out = String::new();
    for m in WORD.find_iter(text) {
        let w = m.as_str();
        let Some(letter) = first_letter(w) else {
            out.push_str(&escape(w));
            continue;
        };
        let keep = capitals_within(w)
            || (!case.title_case && letter.is_uppercase() && !*first)
            || (case.title_case && letter.is_uppercase() && case.names.contains(&word_key(w)));
        *first = false;
        if keep {
            out.push_str(&format!("{{{}}}", escape(w)));
        } else {
            out.push_str(&escape(w));
        }
    }
    out
}

fn first_letter(w: &str) -> Option<char> {
    w.chars().find(|c| c.is_alphabetic())
}

/// Whether a part of `w`, between hyphens and slashes, has a capital past
/// its first letter, as `GENE1`, `eQTLs` or `scRNA-seq`.
fn capitals_within(w: &str) -> bool {
    w.split(['-', '/']).any(|part| {
        part.chars()
            .skip_while(|c| !c.is_alphabetic())
            .skip(1)
            .any(char::is_uppercase)
    })
}

/// A journal's name in title case, where it is in sentence case, as
/// `Nature methods`: capitalised, and no other word with a capital. A
/// name with a capital elsewhere, or none, as `eLife`, is as it is.
fn venue(name: &str) -> String {
    const SMALL: &[&str] = &[
        "a", "an", "and", "at", "by", "de", "for", "in", "of", "on", "or", "the", "to", "&",
    ];
    let mut words = name.split(' ');
    let sentence = words
        .next()
        .and_then(|w| w.chars().next())
        .is_some_and(char::is_uppercase)
        && words.clone().next().is_some()
        && !words.any(|w| w.chars().any(char::is_uppercase));
    if !sentence {
        return name.to_string();
    }
    name.split(' ')
        .enumerate()
        .map(|(i, w)| {
            if i > 0 && SMALL.contains(&w) {
                return w.to_string();
            }
            let mut c = w.chars();
            c.next()
                .map_or_else(String::new, |f| f.to_uppercase().chain(c).collect())
        })
        .collect::<Vec<_>>()
        .join(" ")
}
