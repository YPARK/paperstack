//! Records as BibTeX, as Paperpile writes it: `journal` and `year`, which
//! BibTeX, BibLaTeX and pandoc all read. After ohp's `csl.rs`, which writes
//! BibLaTeX for typst.

use crate::record::Record;
use serde_json::Value;

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
            fields.push((bib, escape(&value)));
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
