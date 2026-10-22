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
    fn new(csl: Value, source: &'static str) -> Self {
        Found {
            csl,
            arxiv: None,
            preprint: None,
            pdfs: Vec::new(),
            source,
        }
    }

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
    let mut found =
        by_doi_only(net, &doi).with_context(|| format!("doi.org does not know {doi}"))?;
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
    Ok(Found::new(tidy(csl), "doi.org"))
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
    static CATEGORY: LazyLock<Regex> =
        LazyLock::new(|| Regex::new(r#"<arxiv:primary_category[^>]*term="([^"]+)""#).unwrap());
    let category = CATEGORY.captures(entry).map(|c| c[1].to_string());
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
        arxiv: Some(id.to_string()),
        pdfs: vec![pdf],
        ..Found::new(csl, "arxiv")
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
        None => Found::new(tidy(csl), "pubmed"),
    };
    found.csl["PMID"] = json!(pmid);
    if let Some(pmcid) = pmcid {
        found.csl["PMCID"] = json!(pmcid);
    }
    Ok(found)
}

pub fn by_pmcid(net: &Net, pmcid: &str) -> anyhow::Result<Found> {
    let mut query = vec![("ids", pmcid), ("format", "json"), ("tool", "paperstack")];
    net.sign(&mut query, "email");
    let ids = net.json(
        "https://pmc.ncbi.nlm.nih.gov/tools/idconv/api/v1/articles/",
        "application/json",
        &query,
    )?;
    let record = ids
        .get("records")
        .and_then(|r| r.get(0))
        .with_context(|| format!("NCBI does not know {pmcid}"))?;
    // NCBI gives the PMID as a number.
    let pmid = record.get("pmid").and_then(|p| {
        p.as_str()
            .map(str::to_string)
            .or_else(|| p.as_u64().map(|n| n.to_string()))
    });
    let mut found = if let Some(pmid) = pmid {
        by_pmid(net, &pmid)?
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
pub fn from_metas(metas: &[(String, String)]) -> Option<Source> {
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
    let (at, html) = net.page(url).with_context(
        || "the publisher would not give its page: give the paper's DOI, or save its PDF to inbox/",
    )?;
    let metas = metas(&html);
    let mut found = match from_metas(&metas) {
        Some(source) => by_source(net, &source)?,
        None => from_tags(&metas, &at).with_context(|| {
            format!(
                "{url} names no DOI, arXiv id or PMID, nor a title and authors, in its meta tags"
            )
        })?,
    };
    if let Some((_, pdf)) = metas.into_iter().find(|(k, _)| k == "citation_pdf_url") {
        found.pdfs.insert(0, pdf);
    }
    Ok(found)
}

/// The paper a page's Highwire tags describe, `citation_title`,
/// `citation_author` and the rest, as Google Scholar reads them: for a page
/// of a paper no registry has, as NeurIPS's before 2022 or PMLR's.
pub fn from_tags(metas: &[(String, String)], url: &str) -> Option<Found> {
    let all = |name: &str| -> Vec<&str> {
        metas
            .iter()
            .filter(|(k, v)| k == name && !v.trim().is_empty())
            .map(|(_, v)| v.trim())
            .collect()
    };
    let one = |names: &[&str]| {
        names
            .iter()
            .find_map(|n| all(n).first().map(|v| v.to_string()))
    };
    let title = one(&["citation_title"])?;
    let authors: Vec<Value> = all("citation_author")
        .into_iter()
        .flat_map(|a| a.split(';').map(str::trim).collect::<Vec<_>>())
        .filter(|a| !a.is_empty())
        .map(|a| match a.split_once(',') {
            Some((family, given)) => json!({ "family": family.trim(), "given": given.trim() }),
            None => name(a),
        })
        .collect();
    if authors.is_empty() {
        return None;
    }
    let conference = one(&["citation_conference_title", "citation_conference"]);
    let journal = one(&["citation_journal_title"]);
    let book = one(&["citation_inbook_title", "citation_book_title"]);
    let proceedings = |v: &str| {
        let v = v.to_lowercase();
        v.contains("proceedings")
            || v.contains("conference")
            || v.contains("neural information processing")
    };
    let (kind, venue) = match (conference, journal, book) {
        (Some(c), _, _) => ("paper-conference", Some(c)),
        (None, Some(j), _) if proceedings(&j) => ("paper-conference", Some(j)),
        (None, Some(j), _) => ("article-journal", Some(j)),
        (None, None, Some(b)) => ("chapter", Some(b)),
        _ => ("article", None),
    };
    let date = one(&[
        "citation_publication_date",
        "citation_date",
        "citation_online_date",
        "citation_year",
    ])
    .unwrap_or_default();
    let parts: Vec<i64> = date
        .split(['/', '-', ' '])
        .filter_map(|p| p.parse().ok())
        .take(3)
        .collect();
    let mut csl = json!({ "type": kind, "title": squash(&title), "author": authors, "URL": url });
    if let Some(v) = venue {
        csl["container-title"] = json!(v);
    }
    if parts.first().is_some_and(|&y| y > 1000) {
        csl["issued"] = json!({ "date-parts": [parts] });
    }
    for (tag, field) in [
        ("citation_volume", "volume"),
        ("citation_issue", "issue"),
        ("citation_publisher", "publisher"),
        ("citation_issn", "ISSN"),
        ("citation_isbn", "ISBN"),
        ("citation_abstract", "abstract"),
    ] {
        if let Some(v) = one(&[tag]) {
            csl[field] = json!(squash(&strip_tags(&v)));
        }
    }
    match (one(&["citation_firstpage"]), one(&["citation_lastpage"])) {
        (Some(a), Some(b)) => csl["page"] = json!(format!("{a}-{b}")),
        (Some(a), None) => csl["page"] = json!(a),
        _ => {}
    }
    Some(Found {
        arxiv: one(&["citation_arxiv_id"]),
        ..Found::new(csl, "page")
    })
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
    net.sign(&mut query, "mailto");
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
    if found.csl.get("abstract").is_none()
        && let Some(pmid) = found
            .csl
            .get("PMID")
            .and_then(Value::as_str)
            .map(str::to_string)
        && let Some(text) = pubmed_abstract(net, &pmid)
    {
        found.csl["abstract"] = json!(text);
    }
    let best = work.get("best_oa_location").into_iter();
    let all = work
        .get("locations")
        .and_then(Value::as_array)
        .into_iter()
        .flatten();
    for loc in best.chain(all) {
        if let Some(url) = loc.get("pdf_url").and_then(Value::as_str)
            && !found.pdfs.iter().any(|p| p == url)
        {
            found.pdfs.push(url.to_string());
        }
    }
}

/// PubMed's abstract, its sections' labels kept: for journals, as Nature's,
/// that give Crossref none.
fn pubmed_abstract(net: &Net, pmid: &str) -> Option<String> {
    let mut query = vec![
        ("db", "pubmed"),
        ("id", pmid),
        ("retmode", "xml"),
        ("tool", "paperstack"),
    ];
    net.sign(&mut query, "email");
    let xml = net
        .text(
            "https://eutils.ncbi.nlm.nih.gov/entrez/eutils/efetch.fcgi",
            &query,
        )
        .ok()?;
    pubmed_abstract_of(&xml)
}

pub fn pubmed_abstract_of(xml: &str) -> Option<String> {
    static PART: LazyLock<Regex> = LazyLock::new(|| {
        Regex::new(r#"(?s)<AbstractText(?:\s+[^>]*?Label="([^"]*)")?[^>]*>(.*?)</AbstractText>"#)
            .unwrap()
    });
    let parts: Vec<String> = PART
        .captures_iter(xml)
        .map(|c| {
            let text = squash(&unescape(&strip_tags(&c[2])));
            match c.get(1) {
                Some(label) => format!("{}: {text}", label.as_str()),
                None => text,
            }
        })
        .filter(|p| !p.is_empty())
        .collect();
    (!parts.is_empty()).then(|| parts.join(" "))
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
    net.sign(&mut query, "mailto");
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
    net.sign(&mut query, "mailto");
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
            let id = source::unversioned(id.rsplit("/abs/").next()?).to_string();
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
    if let Some(Value::String(t)) = map.get("type") {
        let t = crate::record::csl_type(t).to_string();
        map.insert("type".to_string(), Value::String(t));
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
/// `SCM team`, whole.
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
