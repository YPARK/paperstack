//! The index searched: SQLite, with FTS5 over the text of the records. It
//! is kept on this machine, not in the library, as a cache: built again
//! from the records whenever they change, reading only those changed, and
//! from nothing when its schema does.

use crate::library::{self, Library};
use crate::query::{self, Field, Query, Term};
use crate::record::Record;
use anyhow::Context;
use rusqlite::types::Value as Sql;
use rusqlite::{Connection, OptionalExtension, params, params_from_iter};
use std::collections::HashMap;
use std::path::{Path, PathBuf};

const SCHEMA: i64 = 1;

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

    fn order(self, searched: bool, reverse: bool) -> String {
        let (asc, desc) = if reverse {
            ("DESC", "ASC")
        } else {
            ("ASC", "DESC")
        };
        match self {
            Sort::Relevance if searched => format!("score {asc}, p.added {desc}"),
            Sort::Relevance | Sort::Added => format!("p.added {desc}"),
            Sort::Year => format!("p.year IS NULL, p.year {desc}, p.added {desc}"),
            Sort::Author => format!("p.first_author COLLATE NOCASE {asc}, p.year {asc}"),
            Sort::Title => format!("p.title COLLATE NOCASE {asc}"),
        }
    }
}

/// A paper found.
#[derive(Clone, Debug)]
pub struct Hit {
    pub record: Record,
    /// The record's file, from the library's root.
    pub path: PathBuf,
    pub has_pdf: bool,
    /// FTS5's bm25, lower better; 0 where nothing was searched for.
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

pub struct Index {
    db: Connection,
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
        .join(format!("index-{id:016x}.sqlite"))
}

impl Index {
    pub fn open(path: &Path) -> anyhow::Result<Self> {
        if let Some(dir) = path.parent() {
            std::fs::create_dir_all(dir)?;
        }
        let db = Connection::open(path)
            .with_context(|| format!("cannot open the index {}", path.display()))?;
        let version: i64 = db.query_row("PRAGMA user_version", [], |r| r.get(0))?;
        if version != SCHEMA {
            db.execute_batch(
                "DROP TABLE IF EXISTS paper; DROP TABLE IF EXISTS folder;
                 DROP TABLE IF EXISTS label; DROP TABLE IF EXISTS alias;
                 DROP TABLE IF EXISTS fts;",
            )?;
        }
        db.execute_batch(&format!(
            "PRAGMA journal_mode = WAL;
             CREATE TABLE IF NOT EXISTS paper (
                 id INTEGER PRIMARY KEY, key TEXT NOT NULL UNIQUE, path TEXT NOT NULL UNIQUE,
                 mtime INTEGER, size INTEGER, year INTEGER, added TEXT,
                 first_author TEXT, title TEXT, doi TEXT, arxiv TEXT,
                 starred INTEGER, has_pdf INTEGER, has_abstract INTEGER, record TEXT);
             CREATE TABLE IF NOT EXISTS folder (id INTEGER, name TEXT);
             CREATE TABLE IF NOT EXISTS label (id INTEGER, name TEXT);
             CREATE TABLE IF NOT EXISTS alias (alias TEXT PRIMARY KEY, id INTEGER);
             CREATE INDEX IF NOT EXISTS folder_id ON folder(id);
             CREATE INDEX IF NOT EXISTS label_id ON label(id);
             CREATE VIRTUAL TABLE IF NOT EXISTS fts USING fts5(
                 title, abstract, authors, venue, keywords, ids,
                 tokenize = 'unicode61 remove_diacritics 2');
             PRAGMA user_version = {SCHEMA};"
        ))?;
        Ok(Index { db })
    }

    /// Bring the index up to the records: read those new or changed since,
    /// drop those gone, and note which have a PDF beside them.
    pub fn refresh(&mut self, lib: &Library) -> anyhow::Result<Refresh> {
        let walk = lib.walk()?;
        let mut done = Refresh {
            conflicts: walk.conflicts,
            ..Refresh::default()
        };
        let tx = self.db.transaction()?;
        let known: HashMap<String, (i64, i64, i64)> = {
            let mut q = tx.prepare("SELECT path, id, mtime, size FROM paper")?;
            q.query_map([], |r| Ok((r.get(0)?, (r.get(1)?, r.get(2)?, r.get(3)?))))?
                .collect::<Result<_, _>>()?
        };
        let mut seen = std::collections::HashSet::new();
        for found in &walk.records {
            let rel = found.path.to_string_lossy().into_owned();
            seen.insert(rel.clone());
            if let Some(&(_, mtime, size)) = known.get(&rel)
                && mtime == found.mtime
                && size == found.size
            {
                continue;
            }
            if let Some(&(id, _, _)) = known.get(&rel) {
                remove(&tx, id)?;
            }
            let record = match library::read(&lib.root.join(&found.path)) {
                Ok(r) => r,
                Err(e) => {
                    done.errors.push(format!("{e:#}"));
                    continue;
                }
            };
            match insert(&tx, &record, &rel, found.mtime, found.size) {
                Ok(()) => done.read += 1,
                Err(e) => done.errors.push(format!("{rel}: {e}")),
            }
        }
        for (path, (id, _, _)) in &known {
            if !seen.contains(path) {
                remove(&tx, *id)?;
                done.removed += 1;
            }
        }
        {
            let mut q = tx.prepare("SELECT id, path, has_pdf FROM paper")?;
            let rows: Vec<(i64, String, bool)> = q
                .query_map([], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)))?
                .collect::<Result<_, _>>()?;
            let mut set = tx.prepare("UPDATE paper SET has_pdf = ?2 WHERE id = ?1")?;
            for (id, path, had) in rows {
                let has = walk.pdfs.contains(Path::new(&path));
                if has != had {
                    set.execute(params![id, has])?;
                }
            }
        }
        tx.commit()?;
        Ok(done)
    }

    pub fn search(
        &self,
        q: &Query,
        sort: Sort,
        reverse: bool,
        limit: Option<usize>,
    ) -> anyhow::Result<Vec<Hit>> {
        let mut matched = Vec::new();
        let mut wheres = Vec::new();
        let mut args: Vec<Sql> = Vec::new();
        for term in &q.terms {
            filter(term, &mut matched, &mut wheres, &mut args);
        }
        let searched = !matched.is_empty();
        let mut sql = if searched {
            // Weights by column: a word in the title counts most.
            "SELECT p.path, p.record, p.has_pdf, bm25(fts, 10.0, 2.0, 5.0, 2.0, 2.0, 5.0) AS score
             FROM paper p JOIN fts ON fts.rowid = p.id WHERE fts MATCH ?"
                .to_string()
        } else {
            "SELECT p.path, p.record, p.has_pdf, 0.0 AS score FROM paper p WHERE 1".to_string()
        };
        let mut all: Vec<Sql> = Vec::new();
        if searched {
            all.push(Sql::Text(matched.join(" AND ")));
        }
        all.extend(args);
        for w in &wheres {
            sql.push_str(" AND ");
            sql.push_str(w);
        }
        sql.push_str(" ORDER BY ");
        sql.push_str(&sort.order(searched, reverse));
        if let Some(n) = limit {
            sql.push_str(&format!(" LIMIT {n}"));
        }
        let mut stmt = self.db.prepare(&sql)?;
        let rows = stmt.query_map(params_from_iter(all), |r| {
            Ok((
                r.get::<_, String>(0)?,
                r.get::<_, String>(1)?,
                r.get::<_, bool>(2)?,
                r.get::<_, f64>(3)?,
            ))
        })?;
        let mut hits = Vec::new();
        for row in rows {
            let (path, record, has_pdf, score) = row?;
            hits.push(Hit {
                record: serde_json::from_str(&record)?,
                path: PathBuf::from(path),
                has_pdf,
                score,
            });
        }
        Ok(hits)
    }

    /// The paper with the key, or once with it, as an alias.
    pub fn get(&self, key: &str) -> anyhow::Result<Option<Hit>> {
        self.one(
            "key = ?1 OR id = (SELECT id FROM alias WHERE alias = ?1)",
            key,
        )
    }

    pub fn by_doi(&self, doi: &str) -> anyhow::Result<Option<Hit>> {
        self.one("doi = ?1 COLLATE NOCASE", doi)
    }

    pub fn by_arxiv(&self, id: &str) -> anyhow::Result<Option<Hit>> {
        self.one("arxiv = ?1 COLLATE NOCASE", id)
    }

    /// The paper with the same title, in its words, published within a
    /// year of `year`, as a preprint and its paper may be.
    pub fn by_title(&self, title: &str, year: Option<i32>) -> anyhow::Result<Option<Hit>> {
        let words = |s: &str| -> String {
            deunicode::deunicode(s)
                .to_lowercase()
                .split(|c: char| !c.is_ascii_alphanumeric())
                .filter(|w| !w.is_empty())
                .collect::<Vec<_>>()
                .join(" ")
        };
        let wanted = words(title);
        if wanted.len() < 12 {
            return Ok(None);
        }
        let (from, to) = year.map_or((i32::MIN, i32::MAX), |y| (y - 1, y + 1));
        let mut q = self
            .db
            .prepare("SELECT key, title FROM paper WHERE year IS NULL OR year BETWEEN ?1 AND ?2")?;
        let rows: Vec<(String, String)> = q
            .query_map(params![from, to], |r| Ok((r.get(0)?, r.get(1)?)))?
            .collect::<Result<_, _>>()?;
        match rows.into_iter().find(|(_, t)| words(t) == wanted) {
            Some((key, _)) => self.get(&key),
            None => Ok(None),
        }
    }

    fn one(&self, condition: &str, arg: &str) -> anyhow::Result<Option<Hit>> {
        let row = self
            .db
            .query_row(
                &format!("SELECT path, record, has_pdf FROM paper WHERE {condition}"),
                [arg],
                |r| {
                    Ok((
                        r.get::<_, String>(0)?,
                        r.get::<_, String>(1)?,
                        r.get::<_, bool>(2)?,
                    ))
                },
            )
            .optional()?;
        row.map(|(path, record, has_pdf)| {
            Ok(Hit {
                record: serde_json::from_str(&record)?,
                path: PathBuf::from(path),
                has_pdf,
                score: 0.0,
            })
        })
        .transpose()
    }

    pub fn shelves(&self) -> anyhow::Result<Shelves> {
        let count = |sql: &str| -> anyhow::Result<usize> {
            Ok(self.db.query_row(sql, [], |r| r.get::<_, i64>(0))? as usize)
        };
        let names = |table: &str| -> anyhow::Result<Vec<(String, usize)>> {
            let mut q = self.db.prepare(&format!(
                "SELECT name, count(*) FROM {table} GROUP BY name ORDER BY name COLLATE NOCASE"
            ))?;
            let rows = q.query_map([], |r| Ok((r.get(0)?, r.get::<_, i64>(1)? as usize)))?;
            Ok(rows.collect::<Result<_, _>>()?)
        };
        Ok(Shelves {
            all: count("SELECT count(*) FROM paper")?,
            starred: count("SELECT count(*) FROM paper WHERE starred")?,
            no_pdf: count("SELECT count(*) FROM paper WHERE NOT has_pdf")?,
            no_abstract: count("SELECT count(*) FROM paper WHERE NOT has_abstract")?,
            folders: names("folder")?,
            labels: names("label")?,
        })
    }
}

fn remove(tx: &rusqlite::Transaction, id: i64) -> rusqlite::Result<()> {
    for table in ["folder", "label", "alias"] {
        tx.execute(&format!("DELETE FROM {table} WHERE id = ?1"), [id])?;
    }
    tx.execute("DELETE FROM fts WHERE rowid = ?1", [id])?;
    tx.execute("DELETE FROM paper WHERE id = ?1", [id])?;
    Ok(())
}

fn insert(
    tx: &rusqlite::Transaction,
    r: &Record,
    path: &str,
    mtime: i64,
    size: i64,
) -> rusqlite::Result<()> {
    let authors = r.authors();
    let first = authors
        .first()
        .map(|n| n.family.clone())
        .unwrap_or_default();
    let names = authors
        .iter()
        .map(|n| format!("{} {}", n.family, n.given))
        .collect::<Vec<_>>()
        .join("; ");
    let m = &r.paperstack;
    tx.execute(
        "INSERT INTO paper (key, path, mtime, size, year, added, first_author, title, doi,
                            arxiv, starred, has_pdf, has_abstract, record)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, 0, ?12, ?13)",
        params![
            r.key,
            path,
            mtime,
            size,
            r.year(),
            m.added,
            first,
            r.title(),
            r.doi(),
            m.arxiv,
            m.starred,
            r.abstract_text().is_some(),
            serde_json::to_string(r).unwrap_or_default(),
        ],
    )?;
    let id = tx.last_insert_rowid();
    for f in &m.folders {
        tx.execute(
            "INSERT INTO folder (id, name) VALUES (?1, ?2)",
            params![id, f],
        )?;
    }
    for l in &m.labels {
        tx.execute(
            "INSERT INTO label (id, name) VALUES (?1, ?2)",
            params![id, l],
        )?;
    }
    for a in &m.aliases {
        tx.execute(
            "INSERT OR IGNORE INTO alias (alias, id) VALUES (?1, ?2)",
            params![a, id],
        )?;
    }
    let keywords = r
        .csl
        .get("keyword")
        .and_then(|k| k.as_str())
        .unwrap_or_default()
        .to_string();
    let ids = [Some(r.key.as_str()), r.doi(), m.arxiv.as_deref()]
        .into_iter()
        .flatten()
        .collect::<Vec<_>>()
        .join(" ");
    tx.execute(
        "INSERT INTO fts (rowid, title, abstract, authors, venue, keywords, ids)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
        params![
            id,
            r.title(),
            r.abstract_text().unwrap_or_default(),
            names,
            r.venue(),
            keywords,
            ids
        ],
    )?;
    Ok(())
}

/// What `term` asks, as FTS5 to match or SQL to hold.
fn filter(term: &Term, matched: &mut Vec<String>, wheres: &mut Vec<String>, args: &mut Vec<Sql>) {
    let not = if term.negated { "NOT " } else { "" };
    let column = match term.field {
        Field::Any => Some(""),
        Field::Title => Some("title : "),
        Field::Abstract => Some("abstract : "),
        Field::Author => Some("authors : "),
        Field::Venue => Some("venue : "),
        _ => None,
    };
    if let Some(column) = column {
        // Only what has a letter or digit is a word to FTS5.
        if !term.value.chars().any(char::is_alphanumeric) {
            return;
        }
        let quoted = format!("\"{}\"", term.value.replace('"', "\"\""));
        let expr = if term.phrase {
            format!("{column}{quoted}")
        } else {
            format!("{column}{quoted}*")
        };
        if term.negated {
            wheres.push("p.id NOT IN (SELECT rowid FROM fts WHERE fts MATCH ?)".to_string());
            args.push(Sql::Text(expr));
        } else {
            matched.push(expr);
        }
        return;
    }
    let value = term.value.clone();
    let sql = match term.field {
        Field::Year => {
            let Some((from, to)) = query::years(&value) else {
                return;
            };
            args.push(Sql::Integer(from.into()));
            args.push(Sql::Integer(to.into()));
            format!("{not}(p.year BETWEEN ? AND ?)")
        }
        Field::Folder | Field::Label => {
            let table = if term.field == Field::Folder {
                "folder"
            } else {
                "label"
            };
            args.push(Sql::Text(value));
            format!(
                "{not}EXISTS (SELECT 1 FROM {table} t WHERE t.id = p.id AND t.name = ? COLLATE NOCASE)"
            )
        }
        Field::Key => {
            args.push(Sql::Text(value.clone()));
            args.push(Sql::Text(value));
            format!(
                "{not}(p.key = ? COLLATE NOCASE OR p.id IN (SELECT id FROM alias WHERE alias = ? COLLATE NOCASE))"
            )
        }
        Field::Doi => {
            let doi = value.trim_start_matches("https://doi.org/").to_string();
            args.push(Sql::Text(doi));
            format!("{not}(p.doi = ? COLLATE NOCASE)")
        }
        Field::Has => match value.to_lowercase().as_str() {
            "pdf" => format!("{not}p.has_pdf"),
            "abstract" => format!("{not}p.has_abstract"),
            "doi" => format!("{not}(p.doi IS NOT NULL)"),
            _ => return,
        },
        Field::Is => match value.to_lowercase().as_str() {
            "starred" | "star" => format!("{not}p.starred"),
            _ => return,
        },
        _ => return,
    };
    wheres.push(sql);
}
