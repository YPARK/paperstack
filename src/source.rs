//! What a line given to "add" names: a DOI, an arXiv id, a PMID or PMCID,
//! an Elsevier PII, a page, a PDF, BibTeX or words to search for. Most
//! publishers' addresses carry the paper's DOI, or an id that leads to it,
//! so their pages, which often turn paperstack away, need not be read.

use regex::Regex;
use std::sync::LazyLock;

#[derive(Clone, Debug, PartialEq)]
pub enum Source {
    Doi(String),
    Arxiv(String),
    Pmid(String),
    Pmcid(String),
    /// Elsevier's id, as ScienceDirect's and Cell's addresses carry it.
    Pii(String),
    /// A page to read the paper's ids from.
    Url(String),
    Pdf(String),
    Bibtex,
    Words(String),
    Nothing,
}

static DOI: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r#"(?i)\b(10\.\d{4,9}/[^\s"'<>]+)"#).unwrap());
static ARXIV_NEW: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"(?i)\b(\d{4}\.\d{4,5})(v\d+)?\b").unwrap());
static ARXIV_OLD: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"(?i)\b([a-z-]+(?:\.[a-z]{2})?/\d{7})(v\d+)?\b").unwrap());
static PMC: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"(?i)\b(PMC\d{5,9})\b").unwrap());

/// What ends a DOI in an address, but is not in it.
const DOI_TAILS: &[&str] = &[
    "/abstract",
    "/full",
    "/pdf",
    "/epdf",
    "/fulltext",
    "/meta",
    "/references",
    "/suppinfo",
    "/figures",
    "/tables",
    "/info",
    "/metrics",
    ".full.pdf",
    ".full",
    ".pdf",
    ".abstract",
];

/// A DOI as found, its surrounding punctuation, an address's query and
/// the path after it taken off.
pub fn clean_doi(s: &str) -> String {
    let mut d = s.split(['?', '#']).next().unwrap_or(s).to_string();
    if let Ok(decoded) = percent_decode(&d) {
        d = decoded;
    }
    loop {
        let before = d.len();
        d = d
            .trim_end_matches(['.', ',', ';', ':', ')', ']', '}', '/'])
            .to_string();
        for tail in DOI_TAILS {
            if d.to_lowercase().ends_with(tail) {
                d.truncate(d.len() - tail.len());
            }
        }
        // bioRxiv's version: `10.1101/2023.09.11.557287v1`.
        if d.starts_with("10.1101/")
            && let Some(at) = d.rfind('v')
            && d[at + 1..].chars().all(|c| c.is_ascii_digit())
            && at + 1 < d.len()
        {
            d.truncate(at);
        }
        if d.len() == before {
            break;
        }
    }
    d
}

fn percent_decode(s: &str) -> Result<String, ()> {
    let bytes = s.as_bytes();
    let mut out = Vec::with_capacity(bytes.len());
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == b'%' && i + 2 < bytes.len() {
            let hex = std::str::from_utf8(&bytes[i + 1..i + 3]).map_err(|_| ())?;
            out.push(u8::from_str_radix(hex, 16).map_err(|_| ())?);
            i += 3;
        } else {
            out.push(bytes[i]);
            i += 1;
        }
    }
    String::from_utf8(out).map_err(|_| ())
}

/// The DOIs in `text`, in the order found, each once.
pub fn dois(text: &str) -> Vec<String> {
    let mut out: Vec<String> = Vec::new();
    for m in DOI.captures_iter(text) {
        let d = clean_doi(&m[1]);
        if d.contains('/') && !out.iter().any(|o| o.eq_ignore_ascii_case(&d)) {
            out.push(d);
        }
    }
    out
}

/// arXiv ids in `text`, as its stamp down a PDF's margin gives them,
/// `arXiv:2412.08821v1 [cs.CL]`, without the version.
pub fn arxiv_ids(text: &str) -> Vec<String> {
    let mut out = Vec::new();
    for line in text.lines() {
        let Some(at) = line.to_lowercase().find("arxiv") else {
            continue;
        };
        let rest = &line[at..];
        for re in [&*ARXIV_NEW, &*ARXIV_OLD] {
            if let Some(m) = re.captures(rest) {
                let id = m[1].to_string();
                if !out.contains(&id) {
                    out.push(id);
                }
            }
        }
    }
    out
}

impl Source {
    pub fn of(line: &str) -> Source {
        let s = line.trim();
        if s.is_empty() {
            return Source::Nothing;
        }
        if s.starts_with('@') {
            return Source::Bibtex;
        }
        if s.to_lowercase().ends_with(".pdf") && !s.contains("://") {
            return Source::Pdf(s.to_string());
        }
        let lower = s.to_lowercase();
        let web = lower.starts_with("http://") || lower.starts_with("https://");
        if web {
            if let Some(source) = from_address(s) {
                return source;
            }
            return Source::Url(s.to_string());
        }
        if let Some(rest) = lower.strip_prefix("arxiv:") {
            return Source::Arxiv(rest.trim().split('v').next().unwrap_or("").to_string());
        }
        if let Some(m) = ARXIV_NEW.captures(s)
            && m.get(0).unwrap().as_str().len() >= s.len() - 1
        {
            return Source::Arxiv(m[1].to_string());
        }
        if let Some(d) = dois(s).into_iter().next()
            && (lower.starts_with("10.") || lower.starts_with("doi"))
        {
            return Source::Doi(d);
        }
        if let Some(m) = PMC.captures(s)
            && m.get(0).unwrap().as_str().len() == s.len()
        {
            return Source::Pmcid(m[1].to_uppercase());
        }
        let pmid = lower.strip_prefix("pmid:").unwrap_or(&lower).trim();
        if !pmid.is_empty() && pmid.len() <= 9 && pmid.bytes().all(|b| b.is_ascii_digit()) {
            return Source::Pmid(pmid.to_string());
        }
        Source::Words(s.to_string())
    }

    /// What it is, and the steps reading it takes.
    pub fn plan(&self) -> (String, Vec<&'static str>) {
        let (what, steps): (String, Vec<&'static str>) = match self {
            Source::Doi(d) => (
                format!("DOI {d}"),
                vec!["doi.org: the paper as CSL JSON (Crossref, DataCite, mEDRA)"],
            ),
            Source::Arxiv(id) => (
                format!("arXiv {id}"),
                vec!["arXiv's API, then the published version's DOI, if it has one"],
            ),
            Source::Pmid(id) => (
                format!("PubMed {id}"),
                vec!["NCBI: the paper as CSL JSON, then its DOI"],
            ),
            Source::Pmcid(id) => (
                format!("PubMed Central {id}"),
                vec!["NCBI: its DOI or PMID"],
            ),
            Source::Pii(id) => (format!("Elsevier PII {id}"), vec!["Crossref: its DOI"]),
            Source::Url(u) => (
                format!("page {u}"),
                vec!["the page's citation_doi, citation_arxiv_id or citation_pmid"],
            ),
            Source::Pdf(p) => (
                format!("file {p}"),
                vec![
                    "a DOI or arXiv id in its first pages, checked against its title",
                    "else its first page searched for in Crossref, OpenAlex and arXiv",
                ],
            ),
            Source::Bibtex => ("BibTeX".to_string(), vec!["not yet"]),
            Source::Words(w) => (
                format!("search \"{w}\""),
                vec!["Crossref: added only if one title matches"],
            ),
            Source::Nothing => return (String::new(), Vec::new()),
        };
        let mut steps = steps;
        steps.extend([
            "OpenAlex: an abstract where there is none, and an open copy of the PDF",
            "already here? by DOI, arXiv id, or title and year",
            "filed as year/lastname/firstname/<key>_<slug>.pdf and .json",
        ]);
        (what, steps)
    }
}

/// The paper an address names, where its form says.
fn from_address(url: &str) -> Option<Source> {
    let lower = url.to_lowercase();
    let host = lower.split("://").nth(1)?.split('/').next()?;
    let path = &url[url.find(host).unwrap_or(0) + host.len()..];
    if host.ends_with("arxiv.org") {
        let id = path
            .trim_start_matches(['/'])
            .split_once('/')
            .map(|(_, id)| id)?
            .trim_end_matches(".pdf");
        let id = id.split('v').next().unwrap_or(id);
        return (!id.is_empty()).then(|| Source::Arxiv(id.to_string()));
    }
    if host.ends_with("pubmed.ncbi.nlm.nih.gov") || path.contains("/pubmed/") {
        let id: String = path
            .trim_start_matches("/pubmed")
            .trim_matches('/')
            .chars()
            .take_while(char::is_ascii_digit)
            .collect();
        if !id.is_empty() {
            return Some(Source::Pmid(id));
        }
    }
    if let Some(m) = PMC.captures(path) {
        return Some(Source::Pmcid(m[1].to_uppercase()));
    }
    if host.ends_with("nature.com")
        && let Some(id) = path.strip_prefix("/articles/")
    {
        let id = id.split(['?', '#', '/']).next()?.trim_end_matches(".pdf");
        return Some(Source::Doi(format!("10.1038/{id}")));
    }
    if let Some(d) = dois(path).into_iter().next() {
        return Some(Source::Doi(d));
    }
    // ScienceDirect's `/pii/S0092867423013442`, Cell's
    // `/fulltext/S0092-8674(23)01344-2`.
    for mark in [
        "/pii/",
        "/fulltext/",
        "/abstract/",
        "/pdf/",
        "/pdfExtended/",
    ] {
        if let Some(at) = path.find(mark) {
            let pii: String = path[at + mark.len()..]
                .split(['/', '?', '#'])
                .next()?
                .chars()
                .filter(char::is_ascii_alphanumeric)
                .collect();
            if pii.len() == 17 && pii.starts_with(['S', 's']) {
                return Some(Source::Pii(pii.to_uppercase()));
            }
        }
    }
    None
}
