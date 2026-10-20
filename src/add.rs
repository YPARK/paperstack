//! Adding papers: from an id, a page, words or a PDF, as `source.rs`
//! reads what is given; filed with a key of their own and, where an open
//! copy is to be had, their PDF. A paper already here is not added again:
//! it is given the PDF it lacked, or, a preprint since published, what it
//! was published as, keeping its key.

use crate::index::{Hit, Index};
use crate::library::{self, Library};
use crate::net::Net;
use crate::pdf;
use crate::record::{Meta, Record, new_key};
use crate::resolve::{self, Found};
use crate::source::Source;
use anyhow::{Context, bail};
use serde_json::Value;
use sha2::{Digest, Sha256};
use std::path::{Path, PathBuf};

/// Open copies of a PDF tried before giving up on one.
const PDF_TRIES: usize = 4;
/// Ids named in a PDF tried before searching for it.
const ID_TRIES: usize = 6;

#[derive(Clone, Debug, Default)]
pub struct Options {
    /// Download an open copy of the PDF, where there is one.
    pub pdf: bool,
    pub folders: Vec<String>,
    pub labels: Vec<String>,
}

#[derive(Clone, Copy, Debug, PartialEq, serde::Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum Outcome {
    Added,
    /// Already here, and nothing to add to it.
    Known,
    /// Already here as a preprint, now as what it was published as.
    Published,
    /// Already here, and given the PDF it lacked.
    PdfAttached,
}

#[derive(Clone, Debug)]
pub struct Added {
    pub key: String,
    /// The record's file, from the library's root.
    pub path: PathBuf,
    pub has_pdf: bool,
    pub outcome: Outcome,
    /// How it was told which paper it is.
    pub how: String,
}

/// What the browser extension sends of the page it was clicked on.
#[derive(Clone, Debug, Default, serde::Deserialize)]
pub struct Page {
    pub url: String,
    /// The page's `<meta>` tags, name lowercase, then content.
    #[serde(default)]
    pub tags: Vec<(String, String)>,
    /// Where the PDF was fetched from.
    #[serde(default)]
    pub pdf_url: Option<String>,
    /// The PDF, fetched by the browser, as the extension sends it, in
    /// base64; decoded on arrival.
    #[serde(skip)]
    pub pdf: Option<Vec<u8>>,
}

impl Added {
    /// What became of it, and how it was told: `added, by DOI …, no PDF`.
    pub fn summary(&self) -> String {
        let what = match self.outcome {
            Outcome::Added => "added",
            Outcome::Known => "already here",
            Outcome::Published => "now as published",
            Outcome::PdfAttached => "given its PDF",
        };
        let pdf = if self.has_pdf { "" } else { ", no PDF" };
        format!("{what}, by {}{pdf}", self.how)
    }
}

/// Give the paper `key` the PDF `bytes`, from `from`, as one got in the
/// browser: only if it is that paper's, its title on its first pages, and
/// only if it has none yet. A PDF with no text to tell by, as a scan, is
/// taken on trust, and said to be.
pub fn attach(
    lib: &Library,
    index: &mut Index,
    key: &str,
    bytes: &[u8],
    from: &str,
) -> anyhow::Result<Added> {
    let hit = index
        .get(key)?
        .with_context(|| format!("no paper with the key {key}"))?;
    if hit.has_pdf {
        bail!("{key} has its PDF already");
    }
    if !bytes.starts_with(b"%PDF") {
        bail!("{from} is not a PDF");
    }
    let text = pdf::read(bytes)?.text();
    let unread = text.split_whitespace().count() < 20;
    if !unread && !pdf::has_title(hit.record.title(), &text) {
        bail!("{from} is not \"{}\"", hit.record.title());
    }
    let mut record = Record::clone(&hit.record);
    record.paperstack.pdf_sha256 = Some(sha256(bytes));
    let path = lib.save(&record, Some(&hit.path))?;
    library::write(&lib.root.join(&path).with_extension("pdf"), bytes)?;
    index.refresh(lib)?;
    let how = if unread {
        format!("{from}, with no text to check it by")
    } else {
        from.to_string()
    };
    Ok(Added {
        key: record.key,
        path,
        has_pdf: true,
        outcome: Outcome::PdfAttached,
        how,
    })
}

/// An error, its causes after it, on one line.
pub fn one_line(e: &anyhow::Error) -> String {
    format!("{e:#}").replace('\n', " ")
}

pub struct Adder<'a> {
    pub lib: &'a Library,
    pub index: &'a mut Index,
    pub net: &'a Net,
    pub options: Options,
}

impl Adder<'_> {
    pub fn add(&mut self, line: &str) -> anyhow::Result<Added> {
        let source = Source::of(line);
        match &source {
            Source::Nothing => bail!("nothing to add"),
            Source::Bibtex => bail!("BibTeX is not read yet: give the paper's DOI"),
            Source::Pdf(path) => {
                let path = tilde(path);
                let bytes = std::fs::read(&path)
                    .with_context(|| format!("cannot read {}", path.display()))?;
                self.add_pdf(&bytes, &path.display().to_string())
            }
            Source::Words(words) => {
                let found = self.by_words(words)?;
                self.file(found, None, "its title")
            }
            id => {
                let found = resolve::by_source(self.net, id)?;
                let how = id_name(id);
                self.file(found, None, &how)
            }
        }
    }

    /// A paper by its title: added only where one paper Crossref, OpenAlex
    /// or arXiv knows has just that title. Registries hold copies of famous
    /// papers reposted under other DOIs, so where several do, none is
    /// guessed at.
    fn by_words(&self, words: &str) -> anyhow::Result<Found> {
        let mut candidates = resolve::search_openalex(self.net, words).unwrap_or_default();
        candidates.extend(resolve::search_crossref(self.net, words).unwrap_or_default());
        candidates.extend(resolve::search_arxiv(self.net, words).unwrap_or_default());
        // A preprint on arXiv and what it was published as are one paper.
        let mut exact: Vec<resolve::Candidate> = Vec::new();
        for c in candidates
            .iter()
            .filter(|c| c.is_paper() && pdf::same_title(&c.title, words))
        {
            if c.doi.is_none() && c.arxiv.is_none() {
                continue;
            }
            let same_doi = |e: &resolve::Candidate| matches!((&e.doi, &c.doi), (Some(a), Some(b)) if a.eq_ignore_ascii_case(b));
            match exact
                .iter_mut()
                .find(|e| same_doi(e) || (e.arxiv.is_some() && e.arxiv == c.arxiv))
            {
                Some(e) => {
                    e.arxiv = e.arxiv.take().or(c.arxiv.clone());
                    e.doi = e.doi.take().or(c.doi.clone());
                }
                None => exact.push(c.clone()),
            }
        }
        if let [one] = &exact[..] {
            return match (&one.doi, &one.arxiv) {
                (_, Some(id)) => resolve::by_arxiv(self.net, id),
                (Some(doi), None) => resolve::by_doi(self.net, doi),
                (None, None) => unreachable!("kept only with an id"),
            };
        }
        let exact: Vec<&resolve::Candidate> = exact.iter().collect();
        let shown: Vec<&resolve::Candidate> = if exact.is_empty() {
            candidates
                .iter()
                .filter(|c| c.doi.is_some() || c.arxiv.is_some())
                .take(6)
                .collect()
        } else {
            exact
        };
        let list = shown
            .iter()
            .map(|c| {
                let id = match (&c.doi, &c.arxiv) {
                    (Some(d), _) => d.clone(),
                    (None, Some(a)) => format!("arXiv:{a}"),
                    (None, None) => "-".to_string(),
                };
                let year = c.year.map_or_else(|| "    ".to_string(), |y| y.to_string());
                format!("  {id:<40} {year} {:<16} {}", c.kind, c.title)
            })
            .collect::<Vec<_>>()
            .join("\n");
        bail!("not one paper is titled \"{words}\"; add the one meant by its id:\n{list}")
    }

    /// A paper as a browser saw it, its page read and its PDF fetched there,
    /// past the publisher's paywall: the page's ids or tags say which paper
    /// it is, and a PDF that is not that paper's, as a supplement, is not
    /// filed with it.
    pub fn add_from_browser(&mut self, page: &Page) -> anyhow::Result<Added> {
        let id = resolve::from_metas(&page.tags).or_else(|| match Source::of(&page.url) {
            Source::Url(_) | Source::Words(_) | Source::Nothing => None,
            id => Some(id),
        });
        let found = match id {
            Some(id) => Some(resolve::by_source(self.net, &id)?),
            None => resolve::from_tags(&page.tags, &page.url),
        };
        let Some(mut found) = found else {
            return match &page.pdf {
                Some(pdf) => self.add_pdf(pdf, &page.url),
                None => bail!("{} names no paper paperstack can tell", page.url),
            };
        };
        if let Some(pdf) = &page.pdf {
            let reading = pdf::read(pdf)?;
            let text = reading.text();
            if text.split_whitespace().count() >= 20 && !pdf::has_title(found.title(), &text) {
                bail!(
                    "the PDF is not \"{}\": a supplement, or another paper's; nothing added",
                    found.title()
                );
            }
        }
        if let Some(url) = &page.pdf_url {
            found.pdfs.insert(0, url.clone());
        }
        let how = format!("the page {}", page.url);
        self.file(found, page.pdf.as_deref(), &how)
    }

    /// The paper a PDF is, with the PDF.
    pub fn add_pdf(&mut self, bytes: &[u8], name: &str) -> anyhow::Result<Added> {
        self.add_pdf_with(bytes, name, true)
    }

    /// The same, its text searched for only if `search`: a PDF from a folder
    /// as ~/Downloads may be anyone's letter, not to be sent to registries;
    /// without an id in it, it is not filed.
    pub fn add_pdf_with(
        &mut self,
        bytes: &[u8],
        name: &str,
        search: bool,
    ) -> anyhow::Result<Added> {
        let reading = pdf::read(bytes)?;
        let text = reading.text();
        if text.split_whitespace().count() < 20 {
            bail!("{name}: no text in its first pages to tell which paper it is");
        }
        for id in pdf::ids(&reading).into_iter().take(ID_TRIES) {
            if let Ok(found) = resolve::by_source(self.net, &id)
                && pdf::has_title(found.title(), &text)
                && pdf::has_author(&found.csl, &text)
            {
                let how = format!("the {} in it", id_name(&id));
                return self.file(found, Some(bytes), &how);
            }
        }
        if !search {
            bail!("{name}: no DOI or arXiv id in it; saved to inbox/, its text is searched for");
        }
        // The same text may be a block and a line; a candidate turned down
        // for one query would be for the next.
        let mut queries = pdf::queries(&reading);
        let mut seen = std::collections::HashSet::new();
        queries.retain(|q| seen.insert(q.clone()));
        let mut tried = std::collections::HashSet::new();
        for query in queries {
            let mut candidates = resolve::search_crossref(self.net, &query).unwrap_or_default();
            if query.len() < 300 {
                candidates.extend(resolve::search_openalex(self.net, &query).unwrap_or_default());
                candidates.extend(resolve::search_arxiv(self.net, &query).unwrap_or_default());
            }
            for c in candidates
                .iter()
                .filter(|c| c.is_paper() && pdf::has_title(&c.title, &text))
            {
                if !tried.insert((c.doi.clone(), c.arxiv.clone())) {
                    continue;
                }
                let found = match (&c.doi, &c.arxiv) {
                    (Some(doi), _) => resolve::by_doi(self.net, doi),
                    (None, Some(id)) => resolve::by_arxiv(self.net, id),
                    (None, None) => continue,
                };
                if let Ok(found) = found
                    && pdf::has_title(found.title(), &text)
                    && pdf::has_author(&found.csl, &text)
                {
                    return self.file(found, Some(bytes), "its title, searched for");
                }
            }
        }
        bail!("{name}: cannot tell which paper it is")
    }

    /// File what was found, or add to the paper it already is.
    fn file(&mut self, mut found: Found, pdf: Option<&[u8]>, how: &str) -> anyhow::Result<Added> {
        resolve::enrich(self.net, &mut found);
        if let Some(hit) = self.known(&found)? {
            return self.merge(hit, found, pdf, how);
        }
        let mut record = Record {
            key: String::new(),
            csl: found.csl.clone(),
            paperstack: Meta {
                added: now(),
                folders: self.options.folders.clone(),
                labels: self.options.labels.clone(),
                arxiv: found.arxiv.clone(),
                source: Some(found.source.to_string()),
                ..Meta::default()
            },
        };
        let identity = found
            .doi()
            .map(str::to_string)
            .or_else(|| found.arxiv.clone())
            .unwrap_or_else(|| found.title().to_string());
        let index = &*self.index;
        record.key = new_key(&record, &identity, |k| {
            index.get(k).ok().flatten().is_some()
        });
        let bytes = self.pdf_bytes(pdf, &found.pdfs);
        if let Some(b) = &bytes {
            record.paperstack.pdf_sha256 = Some(sha256(b));
        }
        let path = self.lib.save(&record, None)?;
        if let Some(b) = &bytes {
            library::write(&self.lib.root.join(&path).with_extension("pdf"), b)?;
        }
        self.index.refresh(self.lib)?;
        Ok(Added {
            key: record.key,
            path,
            has_pdf: bytes.is_some(),
            outcome: Outcome::Added,
            how: how.to_string(),
        })
    }

    /// The paper here that `found` is: by DOI, by the preprint's DOI or
    /// arXiv id, or by its title and year.
    fn known(&self, found: &Found) -> anyhow::Result<Option<Hit>> {
        for doi in [found.doi(), found.preprint.as_deref()]
            .into_iter()
            .flatten()
        {
            if let Some(hit) = self.index.by_doi(doi)? {
                return Ok(Some(hit));
            }
        }
        if let Some(id) = &found.arxiv
            && let Some(hit) = self.index.by_arxiv(id)?
        {
            return Ok(Some(hit));
        }
        self.index
            .by_title(found.title(), crate::record::year(&found.csl))
    }

    fn merge(
        &mut self,
        hit: Hit,
        found: Found,
        pdf: Option<&[u8]>,
        how: &str,
    ) -> anyhow::Result<Added> {
        let mut record = Record::clone(&hit.record);
        let mut outcome = Outcome::Known;
        let mut changed = false;
        if is_preprint(&record.csl) && !is_preprint(&found.csl) {
            let pmid = record.csl.get("PMID").cloned();
            record.csl = found.csl.clone();
            if let (Some(pmid), Value::Object(map)) = (pmid, &mut record.csl) {
                map.entry("PMID").or_insert(pmid);
            }
            record.paperstack.arxiv = record.paperstack.arxiv.or(found.arxiv.clone());
            outcome = Outcome::Published;
            changed = true;
        } else if record.abstract_text().is_none()
            && let Some(a) = found.csl.get("abstract")
        {
            record.csl["abstract"] = a.clone();
            changed = true;
        }
        let bytes = if hit.has_pdf {
            None
        } else {
            self.pdf_bytes(pdf, &found.pdfs)
        };
        if let Some(b) = &bytes {
            record.paperstack.pdf_sha256 = Some(sha256(b));
            changed = true;
            if outcome == Outcome::Known {
                outcome = Outcome::PdfAttached;
            }
        }
        for (have, wanted) in [
            (&mut record.paperstack.folders, &self.options.folders),
            (&mut record.paperstack.labels, &self.options.labels),
        ] {
            for w in wanted {
                if !have.contains(w) {
                    have.push(w.clone());
                    changed = true;
                }
            }
        }
        let path = if changed {
            self.lib.save(&record, Some(&hit.path))?
        } else {
            hit.path.clone()
        };
        if let Some(b) = &bytes {
            library::write(&self.lib.root.join(&path).with_extension("pdf"), b)?;
        }
        if changed {
            self.index.refresh(self.lib)?;
        }
        Ok(Added {
            key: record.key,
            path,
            has_pdf: hit.has_pdf || bytes.is_some(),
            outcome,
            how: how.to_string(),
        })
    }

    /// The PDF given, else, if asked for, an open copy downloaded.
    fn pdf_bytes(&self, given: Option<&[u8]>, urls: &[String]) -> Option<Vec<u8>> {
        match given {
            Some(b) => Some(b.to_vec()),
            None if self.options.pdf => self.download(urls),
            None => None,
        }
    }

    fn download(&self, urls: &[String]) -> Option<Vec<u8>> {
        urls.iter()
            .take(PDF_TRIES)
            .find_map(|url| self.net.pdf(url).ok())
    }

    /// File each PDF in the inbox, taking it out of the inbox once filed.
    pub fn inbox(&mut self) -> anyhow::Result<Vec<(PathBuf, anyhow::Result<Added>)>> {
        let dir = self.lib.root.join(library::INBOX);
        let mut files: Vec<PathBuf> = std::fs::read_dir(&dir)
            .with_context(|| format!("cannot read {}", dir.display()))?
            .flatten()
            .map(|e| e.path())
            .filter(|p| p.extension().is_some_and(|e| e.eq_ignore_ascii_case("pdf")))
            .collect();
        files.sort();
        let mut done = Vec::new();
        for path in files {
            let result = std::fs::read(&path)
                .with_context(|| format!("cannot read {}", path.display()))
                .and_then(|bytes| self.add_pdf(&bytes, &path.display().to_string()));
            if result.is_ok() {
                let _ = std::fs::remove_file(&path);
            }
            done.push((path, result));
        }
        Ok(done)
    }
}

/// A preprint's record: arXiv's, or a preprint server's.
fn is_preprint(csl: &Value) -> bool {
    let kind = csl.get("type").and_then(Value::as_str).unwrap_or("");
    let doi = csl.get("DOI").and_then(Value::as_str).unwrap_or("");
    matches!(crate::record::csl_type(kind), "article" | "manuscript")
        || doi.to_lowercase().starts_with("10.48550/")
}

fn id_name(id: &Source) -> String {
    match id {
        Source::Doi(d) => format!("DOI {d}"),
        Source::Arxiv(a) => format!("arXiv id {a}"),
        Source::Pmid(p) => format!("PMID {p}"),
        Source::Pmcid(p) => format!("PMCID {p}"),
        Source::Pii(p) => format!("PII {p}"),
        Source::Url(u) => format!("the page {u}"),
        _ => "it".to_string(),
    }
}

fn sha256(bytes: &[u8]) -> String {
    Sha256::digest(bytes)
        .iter()
        .map(|b| format!("{b:02x}"))
        .collect()
}

fn tilde(path: &str) -> PathBuf {
    match path.strip_prefix("~/") {
        Some(rest) => Path::new(&std::env::var_os("HOME").unwrap_or_default()).join(rest),
        None => PathBuf::from(path),
    }
}

/// Now, in RFC 3339, UTC.
pub fn now() -> String {
    let secs = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |d| d.as_secs() as i64);
    let (days, rem) = (secs.div_euclid(86_400), secs.rem_euclid(86_400));
    // Days to a civil date, after Howard Hinnant's `civil_from_days`.
    let z = days + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z - era * 146_097;
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let day = doy - (153 * mp + 2) / 5 + 1;
    let month = if mp < 10 { mp + 3 } else { mp - 9 };
    let year = yoe + era * 400 + i64::from(month <= 2);
    format!(
        "{year:04}-{month:02}-{day:02}T{:02}:{:02}:{:02}Z",
        rem / 3600,
        rem % 3600 / 60,
        rem % 60
    )
}
