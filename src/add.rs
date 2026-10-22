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

/// A PDF whose first page holds more than one paper, as a journal's page
/// of letters: none is guessed at. The one chosen is filed with the PDF
/// by [`Adder::file_chosen`].
pub struct Ambiguous {
    /// The PDF, as it was named.
    pub name: String,
    pub papers: Vec<Found>,
    pub pdf: Vec<u8>,
}

impl std::fmt::Display for Ambiguous {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "{}: this PDF holds more than one paper; add the one meant by its id:",
            self.name
        )?;
        for p in &self.papers {
            write!(f, "\n  {}", listed(p))?;
        }
        Ok(())
    }
}

impl std::fmt::Debug for Ambiguous {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        std::fmt::Display::fmt(self, f)
    }
}

impl std::error::Error for Ambiguous {}

/// A paper found, on a line of a list: its id, year, first author, title.
pub fn listed(found: &Found) -> String {
    let author = crate::record::authors(&found.csl)
        .into_iter()
        .next()
        .map(|n| n.family)
        .unwrap_or_default();
    id_line(
        found.doi(),
        found.arxiv.as_deref(),
        crate::record::year(&found.csl),
        &author,
        found.title(),
    )
}

/// A paper on a line of a list, as one is added by its id.
fn id_line(
    doi: Option<&str>,
    arxiv: Option<&str>,
    year: Option<i32>,
    middle: &str,
    title: &str,
) -> String {
    let id = match (doi, arxiv) {
        (Some(d), _) => d.to_string(),
        (None, Some(a)) => format!("arXiv:{a}"),
        (None, None) => "-".to_string(),
    };
    let year = year.map_or_else(|| "    ".to_string(), |y| y.to_string());
    format!("{id:<40} {year} {middle:<16} {title}")
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

/// Whether the PDF read is the paper titled `title`, with this DOI and
/// arXiv id: its title on its first pages, or, with too little text to show
/// it, as a scan, the paper's DOI or arXiv id in it. `None` where it cannot
/// be told either way.
pub fn shows(
    reading: &pdf::Reading,
    title: &str,
    doi: Option<&str>,
    arxiv: Option<&str>,
) -> Option<bool> {
    let text = reading.text();
    if text.split_whitespace().count() >= 20 {
        return Some(pdf::has_title(title, &text));
    }
    let named = pdf::ids(reading).iter().any(|id| match id {
        Source::Doi(d) => doi.is_some_and(|ours| ours.eq_ignore_ascii_case(d)),
        Source::Arxiv(a) => arxiv.is_some_and(|ours| {
            crate::source::unversioned(ours).eq_ignore_ascii_case(crate::source::unversioned(a))
        }),
        _ => false,
    });
    named.then_some(true)
}

/// Give the paper `key` the PDF `bytes`, from `from`, as one got in the
/// browser: only if it is that paper's, by [`shows`], and only if it has
/// none yet. A PDF that cannot be told by, as a scan with no DOI in it, is
/// taken only where `named`, the person having named that file, as by its
/// path; not one found, as the next download, or given by an agent.
pub fn attach(
    lib: &Library,
    index: &mut Index,
    key: &str,
    bytes: &[u8],
    from: &str,
    named: bool,
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
    let r = &hit.record;
    let how = match shows(
        &pdf::read(bytes)?,
        r.title(),
        r.doi(),
        r.paperstack.arxiv.as_deref(),
    ) {
        Some(true) => from.to_string(),
        Some(false) => bail!("{from} is not \"{}\"", r.title()),
        None if named => format!("{from}, with no text to check it by"),
        None => bail!(
            "{from} has no text to tell it by, nor {key}'s DOI: if it is {key}'s, \
             attach it by its path, as `paperstack attach {key} <file>`"
        ),
    };
    let mut record = Record::clone(&hit.record);
    record.paperstack.pdf_sha256 = Some(sha256(bytes));
    let path = lib.save(&record, Some(&hit.path))?;
    library::write(&lib.root.join(&path).with_extension("pdf"), bytes)?;
    index.upsert(lib, &path, record.clone(), Some(&hit.path))?;
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
            Source::Export(path) => {
                bail!("{path}: a whole library is moved with paperstack import")
            }
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
        // The three asked at once: each takes a second or so to answer.
        let net = self.net;
        let candidates = std::thread::scope(|s| {
            let arxiv = s.spawn(|| resolve::search_arxiv(net, words).unwrap_or_default());
            let mut all = search_both(net, words);
            all.extend(arxiv.join().unwrap_or_default());
            all
        });
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
            return candidate(self.net, one);
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
                let line = id_line(
                    c.doi.as_deref(),
                    c.arxiv.as_deref(),
                    c.year,
                    &c.kind,
                    &c.title,
                );
                format!("  {line}")
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
        let mut how = format!("the page {}", page.url);
        let mut pdf = page.pdf.as_deref();
        if let Some(bytes) = pdf {
            match shows(
                &pdf::read(bytes)?,
                found.title(),
                found.doi(),
                found.arxiv.as_deref(),
            ) {
                Some(true) => {}
                Some(false) => bail!(
                    "the PDF is not \"{}\": a supplement, or another paper's; nothing added",
                    found.title()
                ),
                // The page says which paper, and the PDF it gave is not shown
                // to be it: the paper is filed, and its PDF left for the person.
                None => {
                    pdf = None;
                    how.push_str("; its PDF, with no text to tell it by, not kept");
                }
            }
        }
        if let Some(url) = page.pdf_url.as_ref().filter(|_| pdf.is_some()) {
            found.pdfs.insert(0, url.clone());
        }
        self.file(found, pdf, &how)
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
        // A page naming its own article, as a letter does: the papers on it
        // are what is filed from, not the DOIs it cites.
        let mut papers = self.papers_in(&reading, &text);
        match papers.len() {
            0 => {}
            1 => {
                let found = papers.remove(0);
                let doi = Source::Doi(found.doi().unwrap_or_default().to_string());
                let how = format!("the {} in it", id_name(&doi));
                return self.file(found, Some(bytes), &how);
            }
            _ => {
                return Err(Ambiguous {
                    name: name.to_string(),
                    papers,
                    pdf: bytes.to_vec(),
                }
                .into());
            }
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
        let net = self.net;
        let mut tried = std::collections::HashSet::new();
        let mut pick = |candidates: Vec<resolve::Candidate>| -> Option<Found> {
            candidates
                .iter()
                .filter(|c| tried.insert((c.doi.clone(), c.arxiv.clone())))
                .filter_map(|c| confirmed(net, c, &text))
                .find(|found| pdf::has_author(&found.csl, &text))
        };
        // Crossref and OpenAlex, for each query; OpenAlex holds arXiv's
        // papers too.
        for query in &queries {
            if let Some(found) = pick(search_both(net, query)) {
                return self.file(found, Some(bytes), "its title, searched for");
            }
        }
        // arXiv last, and once: it asks for three seconds between requests.
        if let Some(query) = queries.iter().find(|q| q.len() < 300)
            && let Some(found) = pick(resolve::search_arxiv(net, query).unwrap_or_default())
        {
            return self.file(found, Some(bytes), "its title, searched for");
        }
        bail!("{name}: cannot tell which paper it is")
    }

    /// The papers a first page holds, where it names its own article's DOI,
    /// as a letter's article information does: that article, where its
    /// title or an author is on the page, and those whose headings are on
    /// the page too, searched for. None where it names no article of its
    /// own, and the PDF is told as any other.
    ///
    /// The headings are searched for even of a PDF from a watched folder:
    /// a page with its article information is a journal's, not a letter.
    /// They are asked one after another, as Crossref asks of its callers.
    fn papers_in(&self, reading: &pdf::Reading, text: &str) -> Vec<Found> {
        let net = self.net;
        let mut papers: Vec<Found> = pdf::article_info_dois(reading)
            .iter()
            .filter_map(|d| resolve::by_doi(net, d).ok())
            .filter(|f| pdf::has_title(f.title(), text) || pdf::has_author(&f.csl, text))
            .collect();
        if papers.is_empty() {
            return papers;
        }
        let page = reading.pages.first().map(String::as_str).unwrap_or("");
        for heading in pdf::title_lines(page) {
            if papers.iter().any(|f| agrees(f.title(), &heading)) {
                continue;
            }
            // Its authors may be on a later page: its title, as the
            // heading, is enough.
            let found = search_both(net, &heading)
                .iter()
                .filter(|c| agrees(&c.title, &heading))
                .find_map(|c| confirmed(net, c, text));
            papers.extend(found);
        }
        let mut seen = std::collections::HashSet::new();
        papers.retain(|f| {
            let id = f.doi().map(str::to_lowercase).or_else(|| {
                f.arxiv
                    .as_deref()
                    .map(|a| crate::source::unversioned(a).to_lowercase())
            });
            id.is_none_or(|id| seen.insert(id))
        });
        papers
    }

    /// File `found`, chosen of the papers the PDF `bytes` holds, with it.
    pub fn file_chosen(&mut self, found: Found, bytes: &[u8]) -> anyhow::Result<Added> {
        self.file(found, Some(bytes), "the paper chosen of those in it")
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
        self.index.upsert(self.lib, &path, record.clone(), None)?;
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
            self.index
                .upsert(self.lib, &path, record.clone(), Some(&hit.path))?;
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
    fn pdf_bytes<'b>(
        &self,
        given: Option<&'b [u8]>,
        urls: &[String],
    ) -> Option<std::borrow::Cow<'b, [u8]>> {
        match given {
            Some(b) => Some(std::borrow::Cow::Borrowed(b)),
            None if self.options.pdf => self.download(urls).map(std::borrow::Cow::Owned),
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

/// Crossref's and OpenAlex's candidates for `query`, both asked at once;
/// OpenAlex not for a query as long as a page's start.
fn search_both(net: &Net, query: &str) -> Vec<resolve::Candidate> {
    std::thread::scope(|s| {
        let openalex = (query.len() < 300)
            .then(|| s.spawn(|| resolve::search_openalex(net, query).unwrap_or_default()));
        let mut all = resolve::search_crossref(net, query).unwrap_or_default();
        if let Some(o) = openalex {
            all.extend(o.join().unwrap_or_default());
        }
        all
    })
}

/// The paper candidate `c` is, if it is a paper, with an id, whose title,
/// as the search gave it and as its record has it, is in `text`.
fn confirmed(net: &Net, c: &resolve::Candidate, text: &str) -> Option<Found> {
    if !c.is_paper() || (c.doi.is_none() && c.arxiv.is_none()) || !pdf::has_title(&c.title, text) {
        return None;
    }
    candidate(net, c)
        .ok()
        .filter(|f| pdf::has_title(f.title(), text))
}

/// The paper a search found: by its DOI where it has one, its arXiv id kept
/// beside it, rather than asking arXiv again for what the DOI gives.
fn candidate(net: &Net, c: &resolve::Candidate) -> anyhow::Result<Found> {
    match (&c.doi, &c.arxiv) {
        (Some(doi), arxiv) => {
            let mut found = resolve::by_doi(net, doi)?;
            if let Some(id) = arxiv
                && found.arxiv.is_none()
            {
                found.arxiv = Some(id.clone());
                found.pdfs.push(format!("https://arxiv.org/pdf/{id}"));
            }
            Ok(found)
        }
        (None, Some(id)) => resolve::by_arxiv(net, id),
        (None, None) => bail!("{} has no id", c.title),
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

/// A file's SHA-256, read a piece at a time.
pub fn sha256_file(path: &Path) -> anyhow::Result<String> {
    use std::io::Read;
    let mut file = std::fs::File::open(path)?;
    let mut hasher = Sha256::new();
    let mut piece = vec![0; 1 << 20];
    loop {
        let n = file.read(&mut piece)?;
        if n == 0 {
            break;
        }
        hasher.update(&piece[..n]);
    }
    Ok(hasher
        .finalize()
        .iter()
        .map(|b| format!("{b:02x}"))
        .collect())
}

pub fn sha256(bytes: &[u8]) -> String {
    Sha256::digest(bytes)
        .iter()
        .map(|b| format!("{b:02x}"))
        .collect()
}

/// Whether a registry's title and a heading on the page are one paper's:
/// the same words, or one within the other, neither twice the other's
/// length.
fn agrees(title: &str, heading: &str) -> bool {
    let (a, b) = (pdf::words(title).len(), pdf::words(heading).len());
    a <= 2 * b
        && b <= 2 * a
        && (pdf::same_title(title, heading)
            || pdf::has_title(title, heading)
            || pdf::has_title(heading, title))
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
    rfc3339(secs)
}

/// A time, in seconds since 1970, in RFC 3339, UTC.
pub fn rfc3339(secs: i64) -> String {
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
