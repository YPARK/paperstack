//! The index searched: every record, read once and kept in memory with the
//! words of its text, ranked by BM25 as a search engine ranks pages. It is
//! a cache, kept on this machine as one file, not in the library: built
//! again from the records whenever they change, reading only those
//! changed, and from nothing when its form does.

use crate::library::{self, Library};
use crate::pdf::words;
use crate::query::{self, Field, Query, Term};
use crate::record::Record;
use serde::{Deserialize, Serialize};
use std::cmp::Ordering;
use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::sync::Arc;

/// The cache's form; a cache of another is read from nothing.
const VERSION: u32 = 1;

/// The text searched, by field, and how much a word in each counts.
const FIELDS: usize = 6;
const TITLE: usize = 0;
const ABSTRACT: usize = 1;
const AUTHORS: usize = 2;
const VENUE: usize = 3;
const KEYWORDS: usize = 4;
const IDS: usize = 5;
const WEIGHTS: [f64; FIELDS] = [10.0, 2.0, 5.0, 2.0, 2.0, 5.0];
/// BM25's: how soon more of a word stops counting, and how much a long
/// field's words count less.
const K1: f64 = 1.2;
const B: f64 = 0.75;

/// The order papers are listed in.
#[derive(Clone, Copy, Debug, Default, PartialEq, clap::ValueEnum)]
pub enum Sort {
    /// Best match first; newest added where nothing is searched for.
    #[default]
    Relevance,
    /// Newest published first.
    Year,
    /// Newest added first.
    Added,
    /// By first author.
    Author,
    Title,
}

impl Sort {
    pub const ALL: [Sort; 5] = [
        Sort::Relevance,
        Sort::Added,
        Sort::Year,
        Sort::Author,
        Sort::Title,
    ];

    pub fn name(self) -> &'static str {
        match self {
            Sort::Relevance => "relevance",
            Sort::Year => "year",
            Sort::Added => "date added",
            Sort::Author => "first author",
            Sort::Title => "title",
        }
    }
}

/// A paper found.
#[derive(Clone, Debug)]
pub struct Hit {
    pub record: Arc<Record>,
    /// The record's file, from the library's root.
    pub path: PathBuf,
    pub has_pdf: bool,
    /// BM25, higher better; 0 where nothing was searched for.
    pub score: f64,
}

/// What a refresh did.
#[derive(Debug, Default)]
pub struct Refresh {
    pub read: usize,
    pub removed: usize,
    pub conflicts: Vec<PathBuf>,
    /// Records that could not be read, or whose key another has.
    pub errors: Vec<String>,
}

/// Counts for the sidebar.
#[derive(Debug, Default)]
pub struct Shelves {
    pub all: usize,
    pub starred: usize,
    pub no_pdf: usize,
    pub no_abstract: usize,
    pub folders: Vec<(String, usize)>,
    pub labels: Vec<(String, usize)>,
}

/// A record as the cache keeps it, with what tells whether its file
/// changed since.
#[derive(Serialize, Deserialize)]
struct Kept {
    path: String,
    mtime: i64,
    size: i64,
    has_pdf: bool,
    record: Arc<Record>,
}

#[derive(Serialize, Deserialize)]
struct Cache {
    version: u32,
    papers: Vec<Kept>,
}

/// A paper, with what searching and sorting it takes, worked out once.
struct Paper {
    kept: Kept,
    words: [Vec<String>; FIELDS],
    year: Option<i32>,
    first_author: String,
    title: String,
    folders: Vec<String>,
    labels: Vec<String>,
    keys: Vec<String>,
}

impl Paper {
    fn new(kept: Kept) -> Self {
        let r = &kept.record;
        let authors = r.authors();
        let names = authors
            .iter()
            .map(|n| format!("{} {}", n.family, n.given))
            .collect::<Vec<_>>()
            .join(" ");
        let keyword = r
            .csl
            .get("keyword")
            .and_then(|k| k.as_str())
            .unwrap_or_default();
        let ids = [Some(r.key.as_str()), r.doi(), r.paperstack.arxiv.as_deref()]
            .into_iter()
            .flatten()
            .collect::<Vec<_>>()
            .join(" ");
        let lower = |v: &[String]| v.iter().map(|s| s.to_lowercase()).collect();
        let mut keys = vec![r.key.to_lowercase()];
        keys.extend(r.paperstack.aliases.iter().map(|a| a.to_lowercase()));
        Paper {
            words: [
                words(r.title()),
                words(r.abstract_text().unwrap_or_default()),
                words(&names),
                words(r.venue()),
                words(keyword),
                words(&ids),
            ],
            year: r.year(),
            first_author: authors
                .first()
                .map(|n| n.family.to_lowercase())
                .unwrap_or_default(),
            title: r.title().to_lowercase(),
            folders: lower(&r.paperstack.folders),
            labels: lower(&r.paperstack.labels),
            keys,
            kept,
        }
    }

    fn hit(&self, score: f64) -> Hit {
        Hit {
            record: Arc::clone(&self.kept.record),
            path: PathBuf::from(&self.kept.path),
            has_pdf: self.kept.has_pdf,
            score,
        }
    }

    fn record(&self) -> &Record {
        &self.kept.record
    }
}

pub struct Index {
    /// The cache's file.
    path: PathBuf,
    papers: Vec<Paper>,
    /// By key and alias, lowercase.
    by_key: HashMap<String, usize>,
    /// Each field's mean length in words, as BM25 weighs a field's length.
    mean: [f64; FIELDS],
}

/// Where the index of the library at `root` is kept: the user's cache, one
/// file a library.
pub fn default_path(root: &Path) -> PathBuf {
    let home = || PathBuf::from(std::env::var_os("HOME").unwrap_or_default());
    let cache = if cfg!(target_os = "macos") {
        home().join("Library/Caches")
    } else {
        std::env::var_os("XDG_CACHE_HOME").map_or_else(|| home().join(".cache"), PathBuf::from)
    };
    let root = root.canonicalize().unwrap_or_else(|_| root.to_path_buf());
    let id = crate::record::fnv(root.to_string_lossy().as_bytes(), 0);
    cache
        .join("paperstack")
        .join(format!("index-{id:016x}.json"))
}

impl Index {
    /// The index cached at `path`; empty where there is none, or one of
    /// another form, or one that cannot be read, as after a crash.
    pub fn open(path: &Path) -> anyhow::Result<Self> {
        if let Some(dir) = path.parent() {
            std::fs::create_dir_all(dir)?;
        }
        let papers = std::fs::read(path)
            .ok()
            .and_then(|data| serde_json::from_slice::<Cache>(&data).ok())
            .filter(|c| c.version == VERSION)
            .map(|c| c.papers)
            .unwrap_or_default();
        let mut index = Index {
            path: path.to_path_buf(),
            papers: papers.into_iter().map(Paper::new).collect(),
            by_key: HashMap::new(),
            mean: [0.0; FIELDS],
        };
        index.reckon();
        Ok(index)
    }

    /// Work out again what is worked out over all papers.
    fn reckon(&mut self) {
        self.by_key.clear();
        for (i, p) in self.papers.iter().enumerate() {
            for k in &p.keys {
                self.by_key.entry(k.clone()).or_insert(i);
            }
        }
        let n = self.papers.len().max(1) as f64;
        for f in 0..FIELDS {
            let total: usize = self.papers.iter().map(|p| p.words[f].len()).sum();
            self.mean[f] = total as f64 / n;
        }
    }

    /// Bring the index up to the records: read those new or changed since,
    /// drop those gone, and note which have a PDF beside them.
    pub fn refresh(&mut self, lib: &Library) -> anyhow::Result<Refresh> {
        let walk = lib.walk()?;
        let mut done = Refresh {
            conflicts: walk.conflicts,
            ..Refresh::default()
        };
        let mut old: HashMap<String, Paper> = std::mem::take(&mut self.papers)
            .into_iter()
            .map(|p| (p.kept.path.clone(), p))
            .collect();
        let mut keys: HashSet<String> = HashSet::new();
        let mut changed = false;
        for found in &walk.records {
            let rel = found.path.to_string_lossy().into_owned();
            let has_pdf = walk.pdfs.contains(&found.path);
            let paper = match old.remove(&rel) {
                Some(mut p) if p.kept.mtime == found.mtime && p.kept.size == found.size => {
                    changed |= p.kept.has_pdf != has_pdf;
                    p.kept.has_pdf = has_pdf;
                    p
                }
                _ => match library::read(&lib.root.join(&found.path)) {
                    Ok(record) => {
                        done.read += 1;
                        changed = true;
                        Paper::new(Kept {
                            path: rel.clone(),
                            mtime: found.mtime,
                            size: found.size,
                            has_pdf,
                            record: Arc::new(record),
                        })
                    }
                    Err(e) => {
                        done.errors.push(format!("{e:#}"));
                        continue;
                    }
                },
            };
            if !keys.insert(paper.keys[0].clone()) {
                done.errors.push(format!(
                    "{rel}: the key {} is another's too",
                    paper.record().key
                ));
                continue;
            }
            self.papers.push(paper);
        }
        done.removed = old.len();
        changed |= done.removed > 0;
        self.reckon();
        if changed {
            self.save()?;
        }
        Ok(done)
    }

    fn save(&self) -> anyhow::Result<()> {
        #[derive(Serialize)]
        struct CacheRef<'a> {
            version: u32,
            papers: Vec<&'a Kept>,
        }
        let cache = CacheRef {
            version: VERSION,
            papers: self.papers.iter().map(|p| &p.kept).collect(),
        };
        library::write(&self.path, &serde_json::to_vec(&cache)?)
    }

    pub fn search(
        &self,
        q: &Query,
        sort: Sort,
        reverse: bool,
        limit: Option<usize>,
    ) -> anyhow::Result<Vec<Hit>> {
        let text: Vec<Text> = q.terms.iter().filter_map(Text::of).collect();
        let (wanted, unwanted): (Vec<&Text>, Vec<&Text>) = text.iter().partition(|t| !t.negated);
        let filters: Vec<&Term> = q.terms.iter().filter(|t| !t.field.is_text()).collect();
        let searched = !wanted.is_empty();
        let idf: Vec<f64> = wanted.iter().map(|t| self.idf(t)).collect();
        let mut found: Vec<(usize, f64)> = Vec::new();
        'papers: for (i, p) in self.papers.iter().enumerate() {
            if !filters.iter().all(|t| holds(t, p)) || unwanted.iter().any(|t| t.count(p) > 0) {
                continue;
            }
            let mut score = 0.0;
            for (t, idf) in wanted.iter().zip(&idf) {
                let s = self.score(t, p, *idf);
                if s == 0.0 {
                    continue 'papers;
                }
                score += s;
            }
            found.push((i, score));
        }
        let by = |a: &(usize, f64), b: &(usize, f64)| -> Ordering {
            let (pa, pb) = (&self.papers[a.0], &self.papers[b.0]);
            let added = || {
                pb.record()
                    .paperstack
                    .added
                    .cmp(&pa.record().paperstack.added)
            };
            match sort {
                Sort::Relevance if searched => b.1.total_cmp(&a.1).then_with(added),
                Sort::Relevance | Sort::Added => added(),
                Sort::Year => match (pa.year, pb.year) {
                    (Some(x), Some(y)) => y.cmp(&x).then_with(added),
                    (Some(_), None) => Ordering::Less,
                    (None, Some(_)) => Ordering::Greater,
                    (None, None) => added(),
                },
                Sort::Author => pa
                    .first_author
                    .cmp(&pb.first_author)
                    .then(pa.year.cmp(&pb.year)),
                Sort::Title => pa.title.cmp(&pb.title),
            }
        };
        found.sort_by(|a, b| if reverse { by(b, a) } else { by(a, b) });
        Ok(found
            .into_iter()
            .take(limit.unwrap_or(usize::MAX))
            .map(|(i, score)| self.papers[i].hit(score))
            .collect())
    }

    /// How rare the papers `t` is in are, as BM25 weighs a word.
    fn idf(&self, t: &Text) -> f64 {
        let n = self.papers.len() as f64;
        let df = self.papers.iter().filter(|p| t.count(p) > 0).count() as f64;
        ((n - df + 0.5) / (df + 0.5) + 1.0).ln()
    }

    /// BM25 of `t` in `p`, over the fields it is looked for in.
    fn score(&self, t: &Text, p: &Paper, idf: f64) -> f64 {
        t.fields()
            .iter()
            .map(|&f| {
                let tf = t.count_in(&p.words[f]) as f64;
                if tf == 0.0 {
                    return 0.0;
                }
                let len = p.words[f].len() as f64 / self.mean[f].max(1.0);
                WEIGHTS[f] * idf * tf * (K1 + 1.0) / (tf + K1 * (1.0 - B + B * len))
            })
            .sum()
    }

    /// The paper with the key, or once with it, as an alias.
    pub fn get(&self, key: &str) -> anyhow::Result<Option<Hit>> {
        Ok(self
            .by_key
            .get(&key.to_lowercase())
            .map(|&i| self.papers[i].hit(0.0)))
    }

    pub fn by_doi(&self, doi: &str) -> anyhow::Result<Option<Hit>> {
        Ok(self.find(|r| r.doi().is_some_and(|d| d.eq_ignore_ascii_case(doi))))
    }

    pub fn by_arxiv(&self, id: &str) -> anyhow::Result<Option<Hit>> {
        Ok(self.find(|r| {
            r.paperstack
                .arxiv
                .as_deref()
                .is_some_and(|a| a.eq_ignore_ascii_case(id))
        }))
    }

    /// The paper with the same title, in its words, published within a
    /// year of `year`, as a preprint and its paper may be.
    pub fn by_title(&self, title: &str, year: Option<i32>) -> anyhow::Result<Option<Hit>> {
        let wanted = words(title);
        if wanted.join(" ").len() < 12 {
            return Ok(None);
        }
        Ok(self
            .papers
            .iter()
            .find(|p| {
                let near = match (year, p.year) {
                    (Some(y), Some(py)) => (py - y).abs() <= 1,
                    _ => true,
                };
                near && p.words[TITLE] == wanted
            })
            .map(|p| p.hit(0.0)))
    }

    fn find(&self, holds: impl Fn(&Record) -> bool) -> Option<Hit> {
        self.papers
            .iter()
            .find(|p| holds(p.record()))
            .map(|p| p.hit(0.0))
    }

    pub fn shelves(&self) -> anyhow::Result<Shelves> {
        let count =
            |holds: &dyn Fn(&Paper) -> bool| self.papers.iter().filter(|p| holds(p)).count();
        let names = |of: &dyn Fn(&Record) -> &Vec<String>| {
            let mut n: HashMap<&str, usize> = HashMap::new();
            for p in &self.papers {
                for name in of(p.record()) {
                    *n.entry(name.as_str()).or_default() += 1;
                }
            }
            let mut v: Vec<(String, usize)> =
                n.into_iter().map(|(k, c)| (k.to_string(), c)).collect();
            v.sort_by_key(|(k, _)| k.to_lowercase());
            v
        };
        Ok(Shelves {
            all: self.papers.len(),
            starred: count(&|p| p.record().paperstack.starred),
            no_pdf: count(&|p| !p.kept.has_pdf),
            no_abstract: count(&|p| p.record().abstract_text().is_none()),
            folders: names(&|r| &r.paperstack.folders),
            labels: names(&|r| &r.paperstack.labels),
        })
    }
}

/// A term looked for in the text: its words, together and in order, the
/// last matching the words it starts unless in quotes.
struct Text {
    words: Vec<String>,
    prefix: bool,
    field: Field,
    negated: bool,
}

impl Text {
    fn of(t: &Term) -> Option<Text> {
        if !t.field.is_text() {
            return None;
        }
        let words = words(&t.value);
        (!words.is_empty()).then_some(Text {
            words,
            prefix: !t.phrase,
            field: t.field,
            negated: t.negated,
        })
    }

    fn fields(&self) -> &'static [usize] {
        match self.field {
            Field::Title => &[TITLE],
            Field::Abstract => &[ABSTRACT],
            Field::Author => &[AUTHORS],
            Field::Venue => &[VENUE],
            _ => &[TITLE, ABSTRACT, AUTHORS, VENUE, KEYWORDS, IDS],
        }
    }

    fn count(&self, p: &Paper) -> usize {
        self.fields()
            .iter()
            .map(|&f| self.count_in(&p.words[f]))
            .sum()
    }

    /// How many times the words are in `text`.
    fn count_in(&self, text: &[String]) -> usize {
        let n = self.words.len();
        if text.len() < n {
            return 0;
        }
        let last = n - 1;
        (0..=text.len() - n)
            .filter(|&i| {
                self.words.iter().enumerate().all(|(j, w)| {
                    let at = &text[i + j];
                    if j == last && self.prefix {
                        at.starts_with(w.as_str())
                    } else {
                        at == w
                    }
                })
            })
            .count()
    }
}

/// Whether `p` holds to a term not of its text, as `year:` or `has:pdf`;
/// one paperstack does not know holds of every paper.
fn holds(t: &Term, p: &Paper) -> bool {
    let r = p.record();
    let value = t.value.to_lowercase();
    let yes = match t.field {
        Field::Year => match query::years(&t.value) {
            Some((from, to)) => p.year.is_some_and(|y| (from..=to).contains(&y)),
            None => return true,
        },
        Field::Folder => p.folders.contains(&value),
        Field::Label => p.labels.contains(&value),
        Field::Key => p.keys.contains(&value),
        Field::Doi => {
            let doi = value.trim_start_matches("https://doi.org/");
            r.doi().is_some_and(|d| d.eq_ignore_ascii_case(doi))
        }
        Field::Has => match value.as_str() {
            "pdf" => p.kept.has_pdf,
            "abstract" => r.abstract_text().is_some(),
            "doi" => r.doi().is_some(),
            _ => return true,
        },
        Field::Is => match value.as_str() {
            "starred" | "star" => r.paperstack.starred,
            _ => return true,
        },
        _ => return true,
    };
    yes != t.negated
}
