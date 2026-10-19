//! From an id to the paper: its record as the registry that gives the id
//! keeps it, made tidy, and where an open copy of its PDF may be.
//!
//! DOIs are read from doi.org as CSL JSON, arXiv ids from arXiv's API and
//! PubMed's from NCBI's; a preprint since published is read as what it was
//! published as. OpenAlex then fills in an abstract the record lacks, and
//! knows where open copies of the PDF are.

use crate::net::Net;
use crate::source::{self, Source};
use anyhow::{Context, bail};
use regex::Regex;
use serde_json::{Value, json};
use std::sync::LazyLock;

/// A paper found.
#[derive(Clone, Debug)]
pub struct Found {
    pub csl: Value,
    pub arxiv: Option<String>,
    /// The preprint's DOI, where a paper was found through it.
    pub preprint: Option<String>,
    /// Where its PDF may be had, best first.
    pub pdfs: Vec<String>,
    /// The registry it came from.
    pub source: &'static str,
}

impl Found {
    pub fn title(&self) -> &str {
        self.csl.get("title").and_then(Value::as_str).unwrap_or("")
    }

    pub fn doi(&self) -> Option<&str> {
        self.csl.get("DOI").and_then(Value::as_str)
    }
}

/// The paper `source` names, where it is an id.
pub fn by_source(net: &Net, source: &Source) -> anyhow::Result<Found> {
    match source {
        Source::Doi(d) => by_doi(net, d),
        Source::Arxiv(id) => by_arxiv(net, id),
        Source::Pmid(id) => by_pmid(net, id),
        Source::Pmcid(id) => by_pmcid(net, id),
        Source::Pii(id) => by_pii(net, id),
        Source::Url(u) => by_page(net, u),
        other => bail!("{other:?} is not an id"),
    }
}

pub fn by_doi(net: &Net, doi: &str) -> anyhow::Result<Found> {
    let doi = source::clean_doi(doi);
    if let Some(id) = doi.to_lowercase().strip_prefix("10.48550/arxiv.") {
        return by_arxiv(net, id);
    }
    let csl = net
        .json(
            &format!("https://doi.org/{doi}"),
            "application/vnd.citationstyles.csl+json",
            &[],
        )
        .with_context(|| format!("doi.org does not know {doi}"))?;
    let mut found = Found {
        csl: tidy(csl),
        arxiv: None,
        preprint: None,
        pdfs: Vec::new(),
        source: "doi.org",
    };
    // A preprint since published, as bioRxiv's, is read as the paper.
    if let Some(published) = related(&found.csl, "is-preprint-of")
        && let Ok(mut paper) = by_doi_only(net, &published)
    {
        paper.preprint = Some(doi.clone());
        found = paper;
    }
    Ok(found)
}

fn by_doi_only(net: &Net, doi: &str) -> anyhow::Result<Found> {
    let csl = net.json(
        &format!("https://doi.org/{doi}"),
        "application/vnd.citationstyles.csl+json",
        &[],
    )?;
    Ok(Found {
        csl: tidy(csl),
        arxiv: None,
        preprint: None,
        pdfs: Vec::new(),
        source: "doi.org",
    })
}

/// The DOI `kind` relates the paper to, as Crossref's `relation` gives it.
fn related(csl: &Value, kind: &str) -> Option<String> {
    let ids = csl.get("relation")?.get(kind)?.as_array()?;
    ids.iter()
        .find(|r| r.get("id-type").and_then(Value::as_str) == Some("doi"))
        .and_then(|r| r.get("id")?.as_str())
        .map(source::clean_doi)
}

pub fn by_arxiv(net: &Net, id: &str) -> anyhow::Result<Found> {
    let atom = net.text(
        "https://export.arxiv.org/api/query",
        &[("id_list", id), ("max_results", "1")],
    )?;
    let entry = atom
        .split("<entry>")
        .nth(1)
        .with_context(|| format!("arXiv does not know {id}"))?;
    let entry = entry.split("</entry>").next().unwrap_or(entry);
    let title = tag(entry, "title").map(|t| squash(&t)).unwrap_or_default();
    if title.is_empty() || title == "Error" {
        bail!("arXiv does not know {id}");
    }
    let pdf = format!("https://arxiv.org/pdf/{id}");
    if let Some(doi) = tag(entry, "arxiv:doi") {
        let doi = source::clean_doi(&doi);
        if let Ok(mut found) = by_doi(net, &doi) {
            found.arxiv = Some(id.to_string());
            found.pdfs.push(pdf);
            return Ok(found);
        }
    }
    let authors: Vec<Value> = entry
        .split("<author>")
        .skip(1)
        .filter_map(|a| tag(a, "name"))
        .map(|n| name(&squash(&n)))
        .collect();
    let published = tag(entry, "published").unwrap_or_default();
    let date: Vec<i64> = published
        .get(..10)
        .unwrap_or("")
        .split('-')
        .filter_map(|p| p.parse().ok())
        .collect();
    let category = Regex::new(r#"<arxiv:primary_category[^>]*term="([^"]+)""#)
        .unwrap()
        .captures(entry)
        .map(|c| c[1].to_string());
    let venue = match &category {
        Some(c) => format!("arXiv [{c}]"),
        None => "arXiv".to_string(),
    };
    let mut csl = json!({
        "type": "article",
        "title": title,
        "author": authors,
        "container-title": venue,
        "publisher": "arXiv",
        "number": format!("arXiv:{id}"),
        "issued": { "date-parts": [date] },
        "DOI": format!("10.48550/arXiv.{id}"),
        "URL": format!("https://arxiv.org/abs/{id}"),
    });
    if let Some(summary) = tag(entry, "summary") {
        csl["abstract"] = json!(squash(&summary));
    }
    Ok(Found {
        csl,
        arxiv: Some(id.to_string()),
        preprint: None,
        pdfs: vec![pdf],
        source: "arxiv",
    })
}

pub fn by_pmid(net: &Net, pmid: &str) -> anyhow::Result<Found> {
    let csl = net.json(
        "https://api.ncbi.nlm.nih.gov/lit/ctxp/v1/pubmed/",
        "*/*",
        &[("format", "csl"), ("id", pmid)],
    )?;
    let pmcid = csl.get("PMCID").and_then(Value::as_str).map(str::to_string);
    let mut found = match csl.get("DOI").and_then(Value::as_str) {
        Some(doi) => by_doi(net, doi)?,
        None => Found {
            csl: tidy(csl),
            arxiv: None,
            preprint: None,
            pdfs: Vec::new(),
            source: "pubmed",
        },
    };
    found.csl["PMID"] = json!(pmid);
    if let Some(pmcid) = pmcid {
        found.csl["PMCID"] = json!(pmcid);
    }
    Ok(found)
}

pub fn by_pmcid(net: &Net, pmcid: &str) -> anyhow::Result<Found> {
    let mut query = vec![("ids", pmcid), ("format", "json"), ("tool", "paperstack")];
    if let Some(e) = &net.email {
        query.push(("email", e));
    }
    let ids = net.json(
        "https://pmc.ncbi.nlm.nih.gov/tools/idconv/api/v1/articles/",
        "application/json",
        &query,
    )?;
    let record = ids
        .get("records")
        .and_then(|r| r.get(0))
        .with_context(|| format!("NCBI does not know {pmcid}"))?;
    let mut found = if let Some(pmid) = record.get("pmid").map(|p| p.to_string()) {
        by_pmid(net, pmid.trim_matches('"'))?
    } else if let Some(doi) = record.get("doi").and_then(Value::as_str) {
        by_doi(net, doi)?
    } else {
        bail!("NCBI knows no DOI or PMID for {pmcid}");
    };
    found.csl["PMCID"] = json!(pmcid);
    Ok(found)
}

pub fn by_pii(net: &Net, pii: &str) -> anyhow::Result<Found> {
    let works = net.json(
        "https://api.crossref.org/works",
        "application/json",
        &[
            ("filter", &format!("alternative-id:{pii}")),
            ("rows", "1"),
            ("select", "DOI"),
        ],
    )?;
    let doi = works
        .pointer("/message/items/0/DOI")
        .and_then(Value::as_str)
        .with_context(|| format!("Crossref knows no DOI for the PII {pii}"))?;
    by_doi(net, doi)
}

static META: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r#"(?is)<meta\s+[^>]*?(?:name|property)\s*=\s*["']([^"']+)["'][^>]*?content\s*=\s*["']([^"']*)["']|<meta\s+[^>]*?content\s*=\s*["']([^"']*)["'][^>]*?(?:name|property)\s*=\s*["']([^"']+)["']"#).unwrap()
});

/// A page's `<meta>` tags, name lowercase, then content.
pub fn metas(html: &str) -> Vec<(String, String)> {
    META.captures_iter(html)
        .filter_map(|c| {
            let (name, content) = match (c.get(1), c.get(2), c.get(3), c.get(4)) {
                (Some(n), Some(v), _, _) => (n, v),
                (_, _, Some(v), Some(n)) => (n, v),
                _ => return None,
            };
            Some((name.as_str().to_lowercase(), unescape(content.as_str())))
        })
        .collect()
}

/// The paper a page is of, by the ids in its meta tags, Highwire's
/// `citation_*` first, as Google Scholar reads them.
pub fn from_page(html: &str) -> Option<Source> {
    let metas = metas(html);
    let get = |names: &[&str]| {
        names.iter().find_map(|n| {
            metas
                .iter()
                .find(|(k, v)| k == n && !v.trim().is_empty())
                .map(|(_, v)| v.trim().to_string())
        })
    };
    if let Some(d) = get(&[
        "citation_doi",
        "prism.doi",
        "dc.identifier.doi",
        "bepress_citation_doi",
    ])
    .and_then(|d| source::dois(&d).into_iter().next())
    {
        return Some(Source::Doi(d));
    }
    if let Some(d) = get(&["dc.identifier", "dcterms.identifier", "eprints.doi"])
        .and_then(|d| source::dois(&d).into_iter().next())
    {
        return Some(Source::Doi(d));
    }
    if let Some(id) = get(&["citation_arxiv_id"]) {
        return Some(Source::Arxiv(id));
    }
    if let Some(id) = get(&["citation_pmid"]) {
        return Some(Source::Pmid(id));
    }
    None
}

pub fn by_page(net: &Net, url: &str) -> anyhow::Result<Found> {
    let (_, html) = net.page(url).with_context(
        || "the publisher would not give its page: give the paper's DOI, or save its PDF to inbox/",
    )?;
    let source = from_page(&html)
        .with_context(|| format!("{url} names no DOI, arXiv id or PMID in its meta tags"))?;
    let mut found = by_source(net, &source)?;
    if let Some((_, pdf)) = metas(&html)
        .into_iter()
        .find(|(k, _)| k == "citation_pdf_url")
    {
        found.pdfs.insert(0, pdf);
    }
    Ok(found)
}

/// Where open publishers keep a paper's PDF, by its DOI, as their pages,
/// which are not always to be had, would say.
pub fn publisher_pdfs(doi: &str) -> Vec<String> {
    let lower = doi.to_lowercase();
    let suffix = doi.split_once('/').map_or("", |(_, s)| s);
    let mut out = Vec::new();
    if let Some(n) = lower.strip_prefix("10.7554/elife.") {
        let n = n.split('.').next().unwrap_or(n);
        out.push(format!(
            "https://cdn.elifesciences.org/articles/{n:0>5}/elife-{n:0>5}-v1.pdf"
        ));
    } else if lower.starts_with("10.1371/journal.") {
        let journal = match lower.split('.').nth(2).unwrap_or("") {
            "pone" => "plosone",
            "pbio" => "plosbiology",
            "pcbi" => "ploscompbiol",
            "pgen" => "plosgenetics",
            "pmed" => "plosmedicine",
            "ppat" => "plospathogens",
            "pntd" => "plosntds",
            "pdig" => "digitalhealth",
            "pgph" => "globalpublichealth",
            _ => "plosone",
        };
        out.push(format!(
            "https://journals.plos.org/{journal}/article/file?id={doi}&type=printable"
        ));
    } else if lower.starts_with("10.3389/") {
        out.push(format!("https://www.frontiersin.org/articles/{doi}/pdf"));
    } else if lower.starts_with("10.1101/") {
        out.push(format!("https://www.biorxiv.org/content/{doi}v1.full.pdf"));
        out.push(format!("https://www.medrxiv.org/content/{doi}v1.full.pdf"));
    } else if lower.starts_with("10.1038/") {
        out.push(format!("https://www.nature.com/articles/{suffix}.pdf"));
    }
    out
}

/// What OpenAlex adds: the abstract, a PMID, and open copies of the PDF;
/// open publishers' own copies first.
pub fn enrich(net: &Net, found: &mut Found) {
    let Some(doi) = found.doi().map(str::to_string) else {
        return;
    };
    for url in publisher_pdfs(&doi).into_iter().rev() {
        if !found.pdfs.contains(&url) {
            found.pdfs.insert(0, url);
        }
    }
    let mut query = vec![(
        "select",
        "ids,abstract_inverted_index,best_oa_location,locations",
    )];
    if let Some(e) = &net.email {
        query.push(("mailto", e));
    }
    let Ok(work) = net.json(
        &format!("https://api.openalex.org/works/doi:{doi}"),
        "application/json",
        &query,
    ) else {
        return;
    };
    if found.csl.get("abstract").is_none()
        && let Some(text) = work.get("abstract_inverted_index").and_then(uninvert)
    {
        found.csl["abstract"] = json!(text);
    }
    if found.csl.get("PMID").is_none()
        && let Some(pmid) = work
            .pointer("/ids/pmid")
            .and_then(Value::as_str)
            .and_then(|p| p.trim_end_matches('/').rsplit('/').next())
    {
        found.csl["PMID"] = json!(pmid);
    }
    let mut pdfs: Vec<String> = Vec::new();
    let best = work.get("best_oa_location").into_iter();
    let all = work
        .get("locations")
        .and_then(Value::as_array)
        .into_iter()
        .flatten();
    for loc in best.chain(all) {
        if let Some(url) = loc.get("pdf_url").and_then(Value::as_str)
            && !pdfs.iter().any(|p| p == url)
            && !found.pdfs.iter().any(|p| p == url)
        {
            pdfs.push(url.to_string());
        }
    }
    found.pdfs.extend(pdfs);
}

/// OpenAlex's abstract, given as each word's places, as text.
pub fn uninvert(index: &Value) -> Option<String> {
    let mut words: Vec<(u64, &str)> = Vec::new();
    for (word, places) in index.as_object()? {
        for p in places.as_array()? {
            words.push((p.as_u64()?, word));
        }
    }
    words.sort();
    let text = words.iter().map(|(_, w)| *w).collect::<Vec<_>>().join(" ");
    (!text.is_empty()).then_some(text)
}

/// A candidate a search gives: enough to check against a PDF, or to
/// choose among.
#[derive(Clone, Debug)]
pub struct Candidate {
    pub title: String,
    pub doi: Option<String>,
    pub arxiv: Option<String>,
    pub year: Option<i32>,
    pub kind: String,
}

impl Candidate {
    /// Not a paper's software, data, review or figure, which registries
    /// give papers' titles too.
    pub fn is_paper(&self) -> bool {
        !matches!(
            self.kind.as_str(),
            "software"
                | "dataset"
                | "other"
                | "paratext"
                | "peer-review"
                | "component"
                | "grant"
                | "standard"
                | "supplementary-materials"
        )
    }
}

/// Crossref's best guesses for a reference, or a page's first lines.
pub fn search_crossref(net: &Net, text: &str) -> anyhow::Result<Vec<Candidate>> {
    let mut query = vec![
        ("query.bibliographic", text),
        ("rows", "8"),
        ("select", "DOI,title,issued,type"),
    ];
    if let Some(e) = &net.email {
        query.push(("mailto", e));
    }
    let works = net.json("https://api.crossref.org/works", "application/json", &query)?;
    Ok(works
        .pointer("/message/items")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .filter_map(|w| {
            Some(Candidate {
                title: w.get("title")?.get(0)?.as_str()?.to_string(),
                doi: w.get("DOI").and_then(Value::as_str).map(str::to_string),
                arxiv: None,
                year: w
                    .pointer("/issued/date-parts/0/0")
                    .and_then(Value::as_i64)
                    .and_then(|y| i32::try_from(y).ok()),
                kind: w
                    .get("type")
                    .and_then(Value::as_str)
                    .unwrap_or("")
                    .to_string(),
            })
        })
        .collect())
}

pub fn search_openalex(net: &Net, title: &str) -> anyhow::Result<Vec<Candidate>> {
    let mut query = vec![
        ("search", title),
        ("per_page", "8"),
        ("select", "title,doi,publication_year,type"),
    ];
    if let Some(e) = &net.email {
        query.push(("mailto", e));
    }
    let works = net.json("https://api.openalex.org/works", "application/json", &query)?;
    Ok(works
        .get("results")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .filter_map(|w| {
            Some(Candidate {
                title: w.get("title")?.as_str()?.to_string(),
                doi: w
                    .get("doi")
                    .and_then(Value::as_str)
                    .map(|d| d.trim_start_matches("https://doi.org/").to_string()),
                arxiv: None,
                year: w
                    .get("publication_year")
                    .and_then(Value::as_i64)
                    .and_then(|y| i32::try_from(y).ok()),
                kind: w
                    .get("type")
                    .and_then(Value::as_str)
                    .unwrap_or("")
                    .to_string(),
            })
        })
        .collect())
}

pub fn search_arxiv(net: &Net, title: &str) -> anyhow::Result<Vec<Candidate>> {
    let words: Vec<&str> = title.split_whitespace().take(12).collect();
    let query = format!("ti:\"{}\"", words.join(" ").replace('"', ""));
    let atom = net.text(
        "https://export.arxiv.org/api/query",
        &[("search_query", &query), ("max_results", "5")],
    )?;
    Ok(atom
        .split("<entry>")
        .skip(1)
        .filter_map(|e| {
            let id = tag(e, "id")?;
            let id = id.rsplit("/abs/").next()?.split('v').next()?.to_string();
            Some(Candidate {
                title: squash(&tag(e, "title")?),
                doi: tag(e, "arxiv:doi").map(|d| source::clean_doi(&d)),
                arxiv: Some(id),
                year: tag(e, "published").and_then(|p| p.get(..4)?.parse().ok()),
                kind: "arXiv".to_string(),
            })
        })
        .collect())
}

/// A record as doi.org gives it, without what a library need not keep:
/// Crossref's references, links, licenses and counts. Titles and venues
/// given as lists are given as text, and an abstract's JATS markup is
/// taken off.
pub fn tidy(mut csl: Value) -> Value {
    const DROP: &[&str] = &[
        "id",
        "reference",
        "link",
        "license",
        "content-domain",
        "assertion",
        "funder",
        "indexed",
        "deposited",
        "created",
        "source",
        "score",
        "prefix",
        "member",
        "references-count",
        "is-referenced-by-count",
        "update-policy",
        "accessed",
        "published",
        "published-print",
        "published-online",
        "journal-issue",
        "subtitle",
        "original-title",
        "short-title",
        "alternative-id",
        "institution",
        "archive",
        "epub-date",
        "status",
        "subject",
        "posted",
        "reference-count",
        "resource",
        "subtype",
        "published-other",
        "approved",
        "content-created",
        "group-title",
    ];
    let Value::Object(map) = &mut csl else {
        return csl;
    };
    for k in DROP {
        map.remove(*k);
    }
    for k in [
        "title",
        "container-title",
        "container-title-short",
        "ISSN",
        "ISBN",
    ] {
        if let Some(Value::Array(a)) = map.get(k) {
            let first = a.first().cloned();
            match first {
                Some(v) => {
                    map.insert(k.to_string(), v);
                }
                None => {
                    map.remove(k);
                }
            }
        }
    }
    for k in ["title", "container-title"] {
        if let Some(Value::String(s)) = map.get(k) {
            let s = squash(&strip_tags(s));
            map.insert(k.to_string(), Value::String(s));
        }
    }
    if let Some(Value::String(a)) = map.get("abstract") {
        let text = squash(&strip_tags(a));
        let text = text
            .strip_prefix("Abstract")
            .map(str::trim_start)
            .unwrap_or(&text)
            .to_string();
        if text.is_empty() {
            map.remove("abstract");
        } else {
            map.insert("abstract".to_string(), Value::String(text));
        }
    }
    if let Some(Value::Array(authors)) = map.get_mut("author") {
        for a in authors {
            if let Value::Object(a) = a {
                a.remove("sequence");
                a.remove("affiliation");
                a.remove("authenticated-orcid");
            }
        }
    }
    csl
}

/// A name as arXiv gives it, `Given Family`, as CSL's; a group's, as
/// `LCM team`, whole.
fn name(full: &str) -> Value {
    const PARTICLES: &[&str] = &[
        "van", "von", "de", "der", "den", "di", "da", "du", "del", "la", "le",
    ];
    const GROUPS: &[&str] = &[
        "team",
        "consortium",
        "collaboration",
        "group",
        "project",
        "network",
        "initiative",
        "committee",
        "lab",
        "laboratory",
        "institute",
    ];
    let parts: Vec<&str> = full.split_whitespace().collect();
    let last = parts.last().map(|p| p.to_lowercase()).unwrap_or_default();
    if parts.len() < 2 || GROUPS.contains(&last.as_str()) {
        return json!({ "literal": full });
    }
    let mut at = parts.len() - 1;
    while at > 1 && PARTICLES.contains(&parts[at - 1].to_lowercase().as_str()) {
        at -= 1;
    }
    json!({ "family": parts[at..].join(" "), "given": parts[..at].join(" ") })
}

/// The text of the first `<name>` element in `xml`.
fn tag(xml: &str, name: &str) -> Option<String> {
    let open = format!("<{name}");
    let start = xml.find(&open)?;
    let after = &xml[start + open.len()..];
    let body = after.find('>')? + 1;
    if after[..body].ends_with("/>") {
        return None;
    }
    let close = format!("</{name}>");
    let end = after[body..].find(&close)?;
    Some(unescape(&after[body..body + end]))
}

fn strip_tags(s: &str) -> String {
    static TAG: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"<[^>]+>").unwrap());
    TAG.replace_all(s, " ").into_owned()
}

/// Whitespace, as of a title broken over lines, as single spaces.
pub fn squash(s: &str) -> String {
    s.split_whitespace().collect::<Vec<_>>().join(" ")
}

fn unescape(s: &str) -> String {
    s.replace("&lt;", "<")
        .replace("&gt;", ">")
        .replace("&quot;", "\"")
        .replace("&#39;", "'")
        .replace("&#x27;", "'")
        .replace("&apos;", "'")
        .replace("&amp;", "&")
}
