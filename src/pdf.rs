//! Which paper a PDF is. Its ids, best first: the DOI in its XMP metadata
//! or its Crossmark link, then those on its first page, then arXiv's stamp
//! down the margin, then the DOIs on its second page. A first page often
//! names other papers' DOIs too, so an id is taken only where the title it
//! leads to is on the page. With no id, what the first page begins with is
//! searched for, and the same check made of what is found.
//!
//! What is searched for comes from `queries`; a reader of the page that
//! knows a title when it sees one, as a language model, would add to them.

use crate::resolve::squash;
use crate::source::{self, Source};
use anyhow::Context;
use regex::bytes::Regex as BytesRegex;
use std::collections::HashSet;
use std::sync::LazyLock;

/// Pages read: the title, authors and ids are on the first, or the
/// second where the first is a cover.
const PAGES: u32 = 2;

pub struct Reading {
    /// The text of the first pages.
    pub pages: Vec<String>,
    /// DOIs in its metadata and Crossmark links.
    pub embedded: Vec<String>,
}

impl Reading {
    pub fn text(&self) -> String {
        self.pages.join("\n")
    }
}

pub fn read(bytes: &[u8]) -> anyhow::Result<Reading> {
    let embedded = embedded_dois(bytes);
    let (pages, _) = pages(bytes, 1, PAGES)?;
    Ok(Reading { pages, embedded })
}

/// The text of `count` pages from page `first`, counted from 1, and how
/// many pages the PDF has. A page that cannot be read is left empty; a PDF
/// that makes the reader panic gives none.
pub fn pages(bytes: &[u8], first: u32, count: u32) -> anyhow::Result<(Vec<String>, u32)> {
    std::panic::catch_unwind(|| -> anyhow::Result<(Vec<String>, u32)> {
        let mut doc =
            pdf_extract::Document::load_mem(bytes).context("not a PDF paperstack can read")?;
        if doc.is_encrypted() {
            let _ = doc.decrypt("");
        }
        let total = doc.get_pages().len() as u32;
        let mut pages = Vec::new();
        for page in first.max(1)..=total.min(first.max(1) + count.saturating_sub(1)) {
            let mut text = String::new();
            let mut out = pdf_extract::PlainTextOutput::new(&mut text);
            let _ = pdf_extract::output_doc_page(&doc, &mut out, page);
            pages.push(text);
        }
        Ok((pages, total))
    })
    .unwrap_or_else(|_| Ok((Vec::new(), 0)))
}

/// DOIs a PDF carries for itself: in its XMP metadata, which is mostly not
/// compressed, and in the Crossmark link publishers put on its first page.
fn embedded_dois(bytes: &[u8]) -> Vec<String> {
    static XMP: LazyLock<BytesRegex> =
        LazyLock::new(|| BytesRegex::new(r"(?s)<x:xmpmeta.*?</x:xmpmeta>").unwrap());
    static CROSSMARK: LazyLock<BytesRegex> = LazyLock::new(|| {
        BytesRegex::new(r"crossmark\.crossref\.org/dialog/?\?doi=(10\.[^&\s)>]+)").unwrap()
    });
    let mut out: Vec<String> = Vec::new();
    for m in XMP.find_iter(bytes) {
        let text = String::from_utf8_lossy(m.as_bytes());
        out.extend(source::dois(&text));
    }
    for c in CROSSMARK.captures_iter(bytes) {
        out.extend(source::dois(&String::from_utf8_lossy(&c[1])));
    }
    let mut seen = HashSet::new();
    out.retain(|d| seen.insert(d.to_lowercase()));
    out
}

/// The ids a PDF names, best first.
pub fn ids(r: &Reading) -> Vec<Source> {
    let mut out: Vec<Source> = r.embedded.iter().cloned().map(Source::Doi).collect();
    let first = r.pages.first().map(String::as_str).unwrap_or("");
    let rest = r.pages.get(1..).unwrap_or(&[]).join("\n");
    out.extend(source::dois(first).into_iter().map(Source::Doi));
    out.extend(source::arxiv_ids(first).into_iter().map(Source::Arxiv));
    out.extend(source::dois(&rest).into_iter().map(Source::Doi));
    out.extend(source::arxiv_ids(&rest).into_iter().map(Source::Arxiv));
    let mut seen = HashSet::new();
    out.retain(|s| seen.insert(format!("{s:?}").to_lowercase()));
    out
}

/// Words of `s`, in ASCII, lowercase. Split before they are made ASCII:
/// deunicode drops some line breaks, and would run words together.
pub fn words(s: &str) -> Vec<String> {
    let mut out = Vec::new();
    for w in s
        .split(|c: char| !c.is_alphanumeric())
        .filter(|w| !w.is_empty())
    {
        // Most words are ASCII already: lowercased, and nothing more.
        if w.is_ascii() {
            out.push(w.to_ascii_lowercase());
            continue;
        }
        out.extend(
            deunicode::deunicode(w)
                .to_lowercase()
                .split(|c: char| !c.is_ascii_alphanumeric())
                .filter(|w| !w.is_empty())
                .map(str::to_string),
        );
    }
    out
}

/// Whether one of the first authors' family names is in `text`, as on
/// the first page of a paper they wrote. True of a paper with none.
pub fn has_author(csl: &serde_json::Value, text: &str) -> bool {
    let Some(authors) = csl.get("author").and_then(|a| a.as_array()) else {
        return true;
    };
    let families: Vec<Vec<String>> = authors
        .iter()
        .take(3)
        .filter_map(|a| a.get("family").or_else(|| a.get("literal"))?.as_str())
        .map(words)
        .filter(|w| !w.is_empty())
        .collect();
    if families.is_empty() {
        return true;
    }
    let text: HashSet<String> = words(text).into_iter().collect();
    families
        .iter()
        .any(|family| family.iter().all(|w| text.contains(w)))
}

/// Whether two titles are the same, in their words.
pub fn same_title(a: &str, b: &str) -> bool {
    let (a, b) = (words(a), words(b));
    !a.is_empty() && a == b
}

/// Whether `title` is in `text`: nine in ten of its words, in order, close
/// together, as a title broken over lines still is. Words hyphenated at
/// the end of a line are joined first.
pub fn has_title(title: &str, text: &str) -> bool {
    let title = words(title);
    if title.len() < 2 {
        return false;
    }
    static BROKEN: LazyLock<regex::Regex> =
        LazyLock::new(|| regex::Regex::new(r"(\w)-\s*\n\s*(\w)").unwrap());
    let text = words(&BROKEN.replace_all(text, "$1$2"));
    // Its letters, as a title given with words run together, or split, is.
    let letters: String = title.concat();
    if letters.len() >= 20 && text.concat().contains(&letters) {
        return true;
    }
    let need = (title.len() * 9).div_ceil(10);
    // From each place a title's first or second word is, its words in order
    // within a span not much longer than it.
    (0..text.len())
        .filter(|&i| text[i] == title[0] || text[i] == title[1])
        .any(|start| {
            let span = &text[start..text.len().min(start + title.len() * 2 + 4)];
            let mut at = 0;
            let mut found = 0;
            for w in span {
                if let Some(skip) = title[at..].iter().take(2).position(|t| t == w) {
                    found += 1;
                    at += skip + 1;
                    if at == title.len() {
                        break;
                    }
                }
            }
            found >= need
        })
}

/// What to search for, with no id to go on: the start of the first page,
/// as a reference is searched for; its first blocks of lines, one of which
/// is mostly the title, below a running head if there is one; and its
/// longest early lines.
pub fn queries(r: &Reading) -> Vec<String> {
    let page = r.pages.first().map(String::as_str).unwrap_or("");
    let mut out = Vec::new();
    let start = squash(page);
    let start: String = start.chars().take(400).collect();
    if start.len() > 20 {
        out.push(start);
    }
    let blocks = page
        .split("\n\n")
        .map(squash)
        .filter(|b| {
            let n = b.split(' ').count();
            (4..=40).contains(&n) && !b.contains('@') && !b.to_lowercase().contains("doi")
        })
        .take(3);
    out.extend(blocks);
    let mut lines: Vec<&str> = page
        .lines()
        .map(str::trim)
        .filter(|l| l.len() > 20 && !l.contains('@') && !l.to_lowercase().contains("doi"))
        .take(12)
        .collect();
    lines.sort_by_key(|l| std::cmp::Reverse(l.len()));
    out.extend(lines.into_iter().take(2).map(str::to_string));
    out
}
