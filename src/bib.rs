//! BibTeX for papers, and what a line pasted into "add" names.

use crate::mock::Paper;
use std::fmt::Write;

pub fn bibtex(p: &Paper) -> String {
    let authors = p
        .authors
        .iter()
        .map(|(family, given)| {
            if given.is_empty() {
                format!("{{{family}}}")
            } else {
                format!("{family}, {given}")
            }
        })
        .collect::<Vec<_>>()
        .join(" and ");
    let authors = if p.more > 0 {
        format!("{authors} and others")
    } else {
        authors
    };
    let venue = match p.kind {
        crate::mock::Kind::Conference => "booktitle",
        crate::mock::Kind::Preprint => "howpublished",
        crate::mock::Kind::Article => "journal",
    };
    let mut s = format!("@{}{{{},\n", p.kind.bibtex(), p.key);
    let _ = writeln!(s, "  title = \"{}\",", p.title);
    let _ = writeln!(s, "  author = \"{authors}\",");
    let _ = writeln!(s, "  {venue} = \"{}\",", p.venue);
    let _ = writeln!(s, "  year = {},", p.year);
    if let Some(doi) = p.doi {
        let _ = writeln!(s, "  doi = \"{doi}\",");
    }
    if let Some(id) = p.arxiv {
        let _ = writeln!(s, "  eprint = \"{id}\",\n  archivePrefix = \"arXiv\",");
    }
    s.push_str("}\n");
    s
}

/// What a line given to "add" names, and how it would be read.
pub enum Source {
    Doi(String),
    Arxiv(String),
    Pmid(String),
    Url(String),
    Pdf(String),
    Bibtex,
    Words(String),
    Nothing,
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
        let bare = s
            .trim_start_matches("https://")
            .trim_start_matches("http://")
            .trim_start_matches("dx.")
            .trim_start_matches("doi.org/")
            .trim_start_matches("doi:");
        if bare.starts_with("10.") && bare.contains('/') {
            return Source::Doi(bare.to_string());
        }
        let lower = s.to_lowercase();
        if let Some(at) = lower.find("arxiv.org/") {
            let id = s[at..]
                .rsplit('/')
                .next()
                .unwrap_or("")
                .trim_end_matches(".pdf");
            return Source::Arxiv(id.to_string());
        }
        if let Some(id) = lower.strip_prefix("arxiv:") {
            return Source::Arxiv(id.trim().to_string());
        }
        if arxiv_id(s) {
            return Source::Arxiv(s.to_string());
        }
        let pmid = lower.strip_prefix("pmid:").unwrap_or(&lower).trim();
        if !pmid.is_empty() && pmid.len() <= 9 && pmid.bytes().all(|b| b.is_ascii_digit()) {
            return Source::Pmid(pmid.to_string());
        }
        if s.starts_with("http://") || s.starts_with("https://") {
            return Source::Url(s.to_string());
        }
        Source::Words(s.to_string())
    }

    /// What it is, and the steps reading it would take.
    pub fn plan(&self) -> (String, Vec<&'static str>) {
        let resolve = [
            "dedupe: DOI, arXiv id, PMID, then title + year + first author",
            "key Surname2026-xy; filed as year/lastname/firstname/<key>_<slug>.pdf",
            "the record written beside it as .json, the PDF's sha256 in it",
        ];
        let (what, mut steps): (String, Vec<&'static str>) = match self {
            Source::Doi(d) => (
                format!("DOI {d}"),
                vec![
                    "doi.org, Accept: CSL JSON (Crossref, DataCite, mEDRA)",
                    "PDF: Unpaywall's open copy, else citation_pdf_url",
                ],
            ),
            Source::Arxiv(id) => (
                format!("arXiv {id}"),
                vec![
                    "export.arxiv.org API: metadata, and the published DOI if any",
                    "a published DOI is read as a DOI, the arXiv id kept beside it",
                    "PDF: arxiv.org/pdf/<id>",
                ],
            ),
            Source::Pmid(id) => (
                format!("PubMed {id}"),
                vec![
                    "NCBI citation exporter: CSL JSON, with the DOI",
                    "PDF: PMC's open copy, else via the DOI",
                ],
            ),
            Source::Url(u) => (
                format!("page {u}"),
                vec![
                    "fetch the page: citation_doi, citation_title, citation_pdf_url",
                    "else JSON-LD ScholarlyArticle, else a DOI in the text",
                    "then read as that DOI",
                ],
            ),
            Source::Pdf(p) => (
                format!("file {p}"),
                vec![
                    "first pages' text (hayro, as ohp reads it): a DOI or arXiv id",
                    "else the largest line as title: Crossref bibliographic search",
                    "the file is moved, not copied, into its folder",
                ],
            ),
            Source::Bibtex => (
                "BibTeX".to_string(),
                vec!["each entry read; its key kept if free, its DOI looked up"],
            ),
            Source::Words(w) => (
                format!("search \"{w}\""),
                vec!["Crossref and PubMed search; pick one, then read as its DOI"],
            ),
            Source::Nothing => (String::new(), Vec::new()),
        };
        if !matches!(self, Source::Nothing) {
            steps.extend(resolve);
        }
        (what, steps)
    }
}

/// `2412.08821` or `2412.08821v2`.
fn arxiv_id(s: &str) -> bool {
    let s = s.split('v').next().unwrap_or(s);
    let Some((yymm, n)) = s.split_once('.') else {
        return false;
    };
    yymm.len() == 4
        && (4..=5).contains(&n.len())
        && yymm.bytes().chain(n.bytes()).all(|b| b.is_ascii_digit())
}
