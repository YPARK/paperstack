//! Moving a library from another reference manager: its JSON export, with
//! its citation keys, folders, labels and stars, read into records; and
//! the PDFs it names, downloaded from it as a ZIP and unzipped, or from the
//! Google Drive folder it syncs to, put beside them.
//!
//! The export's form is the other program's own, and not documented; what
//! it is known to hold is read, and the rest left. Its citation keys are
//! kept as they are, so what cites them still does. Nothing is fetched.

use crate::index::Index;
use crate::library::{self, Library};
use crate::record::{Meta, Record, new_key};
use anyhow::Context;
use serde_json::{Value, json};
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use unicode_normalization::UnicodeNormalization;

/// What an import did, or would.
#[derive(Debug, Default, serde::Serialize)]
pub struct Imported {
    pub read: usize,
    pub added: usize,
    /// Already here: given the export's folders, labels and star.
    pub merged: usize,
    pub trashed: usize,
    pub pdfs: usize,
    /// References whose PDF the export names, not found.
    pub pdfs_missing: usize,
    pub pdf_dir: Option<PathBuf>,
    pub folders: usize,
    pub labels: usize,
    pub errors: Vec<String>,
}

/// A reference of the export, as a record, and its main PDF's name there.
struct Item {
    record: Record,
    pdf: Option<String>,
}

pub fn read(path: &Path) -> anyhow::Result<Vec<Value>> {
    let data = std::fs::read(path).with_context(|| format!("cannot read {}", path.display()))?;
    let value: Value =
        serde_json::from_slice(&data).with_context(|| format!("{} is not JSON", path.display()))?;
    let items = match value {
        Value::Array(items) => items,
        Value::Object(mut map) => ["items", "references", "data", "records"]
            .iter()
            .find_map(|k| match map.remove(*k) {
                Some(Value::Array(a)) => Some(a),
                _ => None,
            })
            .with_context(|| format!("{} holds no references", path.display()))?,
        _ => anyhow::bail!("{} holds no references", path.display()),
    };
    Ok(items)
}

/// The export's types, as CSL's.
fn csl_type(pubtype: &str) -> &'static str {
    match pubtype {
        "PP_ARTICLE" => "article-journal",
        "PP_CONFERENCE_PAPER" => "paper-conference",
        "PP_BOOK" => "book",
        "PP_BOOK_CHAPTER" => "chapter",
        "PP_THESIS" => "thesis",
        "PP_REPORT" => "report",
        "PP_PREPRINT" | "PP_MANUSCRIPT" => "article",
        "PP_MAGAZINE_ARTICLE" => "article-magazine",
        "PP_NEWSPAPER_ARTICLE" => "article-newspaper",
        "PP_WEBSITE" => "webpage",
        "PP_DATASET" => "dataset",
        "PP_COMPUTER_PROGRAM" => "software",
        "PP_PATENT" => "patent",
        "PP_PRESENTATION" => "speech",
        "PP_LETTER" | "PP_EMAIL" => "personal_communication",
        "PP_VIDEO" => "motion_picture",
        _ => "document",
    }
}

fn text(v: &Value, k: &str) -> Option<String> {
    match v.get(k)? {
        Value::String(s) if !s.trim().is_empty() => Some(s.trim().to_string()),
        Value::Number(n) => Some(n.to_string()),
        Value::Array(a) => a.first().and_then(|f| f.as_str()).map(str::to_string),
        _ => None,
    }
}

fn names(v: &Value, k: &str) -> Vec<Value> {
    v.get(k)
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .filter_map(|a| {
            let part = |k: &str| {
                a.get(k)
                    .and_then(Value::as_str)
                    .map(str::trim)
                    .filter(|s| !s.is_empty())
            };
            match (part("last"), part("first"), part("collective")) {
                (_, _, Some(c)) => Some(json!({ "literal": c })),
                (Some(l), Some(f), _) => Some(json!({ "family": l, "given": f })),
                (Some(l), None, _) => Some(json!({ "literal": l })),
                _ => part("formatted").map(|f| json!({ "literal": f })),
            }
        })
        .collect()
}

/// When it was added to the other library, if the export says, as RFC 3339.
fn added(v: &Value) -> Option<String> {
    for k in ["created", "added", "imported", "date_added"] {
        match v.get(k) {
            Some(Value::String(s)) if s.len() >= 10 => return Some(s.clone()),
            Some(Value::Number(n)) => {
                let secs = n.as_f64()?;
                // Milliseconds, as JavaScript counts, or seconds.
                let secs = if secs > 1e11 { secs / 1000.0 } else { secs };
                return Some(crate::add::rfc3339(secs as i64));
            }
            _ => {}
        }
    }
    None
}

fn item(v: &Value) -> Option<Item> {
    let title = text(v, "title")?;
    let mut csl = json!({
        "type": csl_type(v.get("pubtype").and_then(Value::as_str).unwrap_or("")),
        "title": title,
    });
    for (k, field) in [("author", "author"), ("editor", "editor")] {
        let n = names(v, k);
        if !n.is_empty() {
            csl[field] = json!(n);
        }
    }
    if let Some(p) = v.get("published") {
        let parts: Vec<i64> = ["year", "month", "day"]
            .iter()
            .map_while(|k| match p.get(*k) {
                Some(Value::String(s)) => s.trim().parse().ok(),
                Some(Value::Number(n)) => n.as_i64(),
                _ => None,
            })
            .filter(|n| *n > 0)
            .collect();
        if !parts.is_empty() {
            csl["issued"] = json!({ "date-parts": [parts] });
        }
    }
    let venue = text(v, "journalfull")
        .or_else(|| text(v, "journal"))
        .or_else(|| text(v, "booktitle"));
    if let Some(venue) = venue {
        csl["container-title"] = json!(venue);
    }
    if let (Some(short), Some(_)) = (text(v, "journal"), text(v, "journalfull")) {
        csl["container-title-short"] = json!(short);
    }
    for (k, field) in [
        ("volume", "volume"),
        ("issue", "issue"),
        ("pages", "page"),
        ("doi", "DOI"),
        ("pmid", "PMID"),
        ("pmc", "PMCID"),
        ("issn", "ISSN"),
        ("isbn", "ISBN"),
        ("publisher", "publisher"),
        ("address", "publisher-place"),
        ("language", "language"),
        ("abstract", "abstract"),
        ("keywords", "keyword"),
        ("url", "URL"),
        ("edition", "edition"),
        ("series", "collection-title"),
        ("school", "publisher"),
        ("note", "note"),
    ] {
        if let Some(t) = text(v, k)
            && csl.get(field).is_none()
        {
            csl[field] = json!(t);
        }
    }
    let list = |k: &str| -> Vec<String> {
        match v.get(k) {
            Some(Value::Array(a)) => a
                .iter()
                .filter_map(|s| s.as_str().map(str::to_string))
                .collect(),
            Some(Value::String(s)) if !s.is_empty() => vec![s.clone()],
            _ => Vec::new(),
        }
    };
    let truthy = |k: &str| match v.get(k) {
        Some(Value::Bool(b)) => *b,
        Some(Value::Number(n)) => n.as_i64().unwrap_or(0) != 0,
        _ => false,
    };
    let arxiv = text(v, "arxivid").or_else(|| {
        text(v, "doi").and_then(|d| {
            d.to_lowercase()
                .strip_prefix("10.48550/arxiv.")
                .map(str::to_string)
        })
    });
    let pdf = v
        .get("attachments")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .filter(|a| {
            a.get("filename")
                .and_then(Value::as_str)
                .is_some_and(|f| f.to_lowercase().ends_with(".pdf"))
        })
        .max_by_key(|a| a.get("article_pdf").and_then(Value::as_i64).unwrap_or(0))
        .and_then(|a| a.get("filename")?.as_str().map(str::to_string));
    Some(Item {
        record: Record {
            key: text(v, "citekey").unwrap_or_default(),
            csl,
            paperstack: Meta {
                added: added(v).unwrap_or_else(crate::add::now),
                folders: list("foldersNamed"),
                labels: list("labelsNamed"),
                starred: truthy("star") || truthy("starred"),
                arxiv,
                source: Some("import".to_string()),
                ..Meta::default()
            },
        },
        pdf,
    })
}

/// A file's name, or path, as compared: lowercase, in NFC, as a zip or a
/// Mac may have stored its accents otherwise.
fn norm(s: &str) -> String {
    s.nfc().collect::<String>().to_lowercase()
}

/// Every PDF under `dir`, by its path from `dir` and by its name.
fn pdfs(dir: &Path) -> (HashMap<String, PathBuf>, HashMap<String, Vec<PathBuf>>) {
    let mut by_path = HashMap::new();
    let mut by_name: HashMap<String, Vec<PathBuf>> = HashMap::new();
    let mut dirs = vec![dir.to_path_buf()];
    while let Some(d) = dirs.pop() {
        let Ok(entries) = std::fs::read_dir(&d) else {
            continue;
        };
        for e in entries.flatten() {
            let p = e.path();
            if e.file_type().is_ok_and(|t| t.is_dir()) {
                dirs.push(p);
            } else if p.extension().is_some_and(|x| x.eq_ignore_ascii_case("pdf")) {
                let rel = p
                    .strip_prefix(dir)
                    .unwrap_or(&p)
                    .to_string_lossy()
                    .into_owned();
                by_path.insert(norm(&rel), p.clone());
                let name = p
                    .file_name()
                    .map(|n| norm(&n.to_string_lossy()))
                    .unwrap_or_default();
                by_name.entry(name).or_default().push(p);
            }
        }
    }
    (by_path, by_name)
}

/// Where the export's PDFs may be, as `--pdfs` was not given: unzipped
/// beside the export, or in the Downloads folder, or in Google Drive.
pub fn find_pdf_dir(export: &Path) -> Option<PathBuf> {
    let home = PathBuf::from(std::env::var_os("HOME").unwrap_or_default());
    let mut places: Vec<PathBuf> = Vec::new();
    if let Some(dir) = export.parent() {
        places.push(dir.to_path_buf());
    }
    places.push(home.join("Downloads"));
    if let Ok(clouds) = std::fs::read_dir(home.join("Library/CloudStorage")) {
        for c in clouds.flatten() {
            places.push(c.path().join("My Drive"));
            places.push(c.path());
        }
    }
    places.push(home.join("Google Drive/My Drive"));
    // A folder holding the PDFs' "All Papers", there or one level down.
    for place in places {
        if place.join("All Papers").is_dir() {
            return Some(place);
        }
        if let Ok(entries) = std::fs::read_dir(&place) {
            for e in entries.flatten() {
                if e.path().join("All Papers").is_dir() {
                    return Some(e.path());
                }
            }
        }
    }
    None
}

/// Read the export at `path` into the library; with `pdf_dir`, the PDFs
/// it names put beside their records. With `dry`, nothing is written.
pub fn import(
    lib: &Library,
    index: &mut Index,
    path: &Path,
    pdf_dir: Option<&Path>,
    dry: bool,
) -> anyhow::Result<Imported> {
    let raw = read(path)?;
    let mut done = Imported {
        read: raw.len(),
        pdf_dir: pdf_dir.map(Path::to_path_buf),
        ..Imported::default()
    };
    let (by_path, by_name) = pdf_dir.map(pdfs).unwrap_or_default();
    let mut keys: std::collections::HashSet<String> = std::collections::HashSet::new();
    let mut folders = std::collections::HashSet::new();
    let mut labels = std::collections::HashSet::new();
    for v in &raw {
        if v.get("trashed")
            .is_some_and(|t| t.as_bool() == Some(true) || t.as_i64().unwrap_or(0) != 0)
        {
            done.trashed += 1;
            continue;
        }
        let Some(Item { mut record, pdf }) = item(v) else {
            done.errors.push(format!(
                "a reference with no title: {}",
                text(v, "_id").unwrap_or_default()
            ));
            continue;
        };
        folders.extend(record.paperstack.folders.iter().cloned());
        labels.extend(record.paperstack.labels.iter().cloned());
        // Already here, by its key, its DOI, or its title and year: given
        // what the export adds.
        let known = match index.get(&record.key)? {
            Some(h) => Some(h),
            None => match record.doi() {
                Some(doi) => index.by_doi(doi)?,
                None => index.by_title(record.title(), record.year())?,
            },
        };
        if let Some(hit) = known {
            let mut here = Record::clone(&hit.record);
            let mut changed = false;
            for (have, wanted) in [
                (&mut here.paperstack.folders, &record.paperstack.folders),
                (&mut here.paperstack.labels, &record.paperstack.labels),
            ] {
                for w in wanted {
                    if !have.contains(w) {
                        have.push(w.clone());
                        changed = true;
                    }
                }
            }
            if record.paperstack.starred && !here.paperstack.starred {
                here.paperstack.starred = true;
                changed = true;
            }
            if !record.key.is_empty()
                && !record.key.eq_ignore_ascii_case(&here.key)
                && !here.paperstack.aliases.contains(&record.key)
            {
                here.paperstack.aliases.push(record.key.clone());
                changed = true;
            }
            if changed && !dry {
                lib.save(&here, Some(&hit.path))?;
            }
            done.merged += 1;
            continue;
        }
        let identity = record
            .doi()
            .map(str::to_string)
            .unwrap_or_else(|| record.title().to_string());
        if record.key.is_empty() || keys.contains(&record.key.to_lowercase()) {
            let taken =
                |k: &str| keys.contains(&k.to_lowercase()) || index.get(k).ok().flatten().is_some();
            record.key = new_key(&record, &identity, taken);
        }
        keys.insert(record.key.to_lowercase());
        // Its PDF: by the path the export gives, or its name; else by the
        // key, as a PDF named for it.
        let found = pdf.as_deref().and_then(|name| {
            by_path.get(&norm(name)).cloned().or_else(|| {
                let base = Path::new(name).file_name()?.to_string_lossy().into_owned();
                match by_name.get(&norm(&base)).map(Vec::as_slice) {
                    Some([one]) => Some(one.clone()),
                    _ => None,
                }
            })
        });
        let found = found.or_else(|| {
            match by_name
                .get(&norm(&format!("{}.pdf", record.key)))
                .map(Vec::as_slice)
            {
                Some([one]) => Some(one.clone()),
                _ => None,
            }
        });
        if pdf.is_some() && found.is_none() && pdf_dir.is_some() {
            done.pdfs_missing += 1;
        }
        done.added += 1;
        if dry {
            done.pdfs += usize::from(found.is_some());
            continue;
        }
        let bytes = found.as_ref().and_then(|p| std::fs::read(p).ok());
        if let Some(b) = &bytes {
            record.paperstack.pdf_sha256 = Some(crate::add::sha256(b));
        }
        let rel = match lib.save(&record, None) {
            Ok(rel) => rel,
            Err(e) => {
                done.errors.push(format!("{}: {e:#}", record.key));
                continue;
            }
        };
        if let Some(b) = bytes {
            library::write(&lib.root.join(&rel).with_extension("pdf"), &b)?;
            done.pdfs += 1;
        }
    }
    done.folders = folders.len();
    done.labels = labels.len();
    if !dry {
        let refresh = index.refresh(lib)?;
        done.errors.extend(refresh.errors);
    }
    Ok(done)
}

impl Imported {
    pub fn summary(&self) -> String {
        let pdfs = match &self.pdf_dir {
            Some(d) if self.pdfs_missing > 0 => format!(
                ", {} PDFs from {} ({} named there not found)",
                self.pdfs,
                crate::browser::tilde(d),
                self.pdfs_missing
            ),
            Some(d) => format!(", {} PDFs from {}", self.pdfs, crate::browser::tilde(d)),
            None => ", no PDFs: give --pdfs, the folder its PDFs, downloaded as a ZIP, unzip to"
                .to_string(),
        };
        format!(
            "{} references: {} added, {} already here, {} in the trash left out; {} folders, {} labels{pdfs}",
            self.read, self.added, self.merged, self.trashed, self.folders, self.labels
        )
    }
}
