//! Taking papers out of the library, and collections and labels off its
//! papers. A paper deleted is moved, its record and its PDF, into `.trash/`
//! at the library's root, filed there as it was: hidden, so no longer
//! found, but kept, and synced, until the trash is emptied by hand. Moved
//! back, it is found again. A PDF deleted alone goes there too, and the
//! paper stays. A collection deleted is taken off its papers, which stay.

use crate::index::{Hit, Index, Sort};
use crate::library::Library;
use crate::query::Query;
use anyhow::{Context, bail};
use std::fs;
use std::path::{Path, PathBuf};

pub const TRASH: &str = ".trash";

/// What is taken off papers: a collection, or a label.
#[derive(Clone, Copy, PartialEq, Debug)]
pub enum Shelf {
    Collection,
    Label,
}

impl Shelf {
    pub fn name(self) -> &'static str {
        match self {
            Shelf::Collection => "collection",
            Shelf::Label => "label",
        }
    }
}

/// Move the paper `key` names, or a key merged into it, to the trash; its
/// record's path in the trash, from the library's root.
pub fn paper(lib: &Library, index: &mut Index, key: &str) -> anyhow::Result<PathBuf> {
    let hit = index
        .get(key)?
        .with_context(|| format!("no paper with the key {key}"))?;
    let to = Path::new(TRASH).join(&hit.path);
    let (record, pdf) = (
        lib.root.join(&hit.path),
        lib.root.join(&hit.path).with_extension("pdf"),
    );
    let dest = lib.root.join(&to);
    fs::create_dir_all(dest.parent().expect("filed in a folder"))?;
    // The record first: once it is gone the paper is, and a PDF left
    // behind is read as no paper's.
    fs::rename(&record, &dest)
        .with_context(|| format!("cannot move {} to the trash", record.display()))?;
    index.remove(&hit.path);
    if pdf.exists() {
        fs::rename(&pdf, dest.with_extension("pdf")).with_context(|| {
            format!(
                "{} is in the trash, but its PDF is left at {}",
                hit.record.key,
                pdf.display()
            )
        })?;
    }
    // Its folders, if it was the last paper in them.
    let mut dir = record.parent();
    while let Some(d) = dir.filter(|d| *d != lib.root) {
        if fs::remove_dir(d).is_err() {
            break;
        }
        dir = d.parent();
    }
    Ok(to)
}

/// Move the PDF of the paper `key` names to the trash; the paper stays.
/// The PDF's path in the trash, from the library's root. The record's
/// hash of it is cleared, as the file is no longer beside the paper.
pub fn pdf(lib: &Library, index: &mut Index, key: &str) -> anyhow::Result<PathBuf> {
    let hit = index
        .get(key)?
        .with_context(|| format!("no paper with the key {key}"))?;
    let pdf = lib.root.join(&hit.path).with_extension("pdf");
    if !pdf.is_file() {
        bail!("{key} has no PDF");
    }
    let mut record = crate::library::read(&lib.root.join(&hit.path))?;
    let to = Path::new(TRASH).join(&hit.path).with_extension("pdf");
    let dest = lib.root.join(&to);
    fs::create_dir_all(dest.parent().expect("filed in a folder"))?;
    // Where a PDF dropped earlier is in the way, it is replaced: the one
    // beside the paper is the one kept. A rename that fails for another
    // reason leaves the PDF where it is.
    if let Err(e) = fs::rename(&pdf, &dest) {
        if !dest.exists() {
            return Err(e).with_context(|| format!("cannot move {} to the trash", pdf.display()));
        }
        fs::remove_file(&dest)
            .with_context(|| format!("cannot replace {} in the trash", dest.display()))?;
        fs::rename(&pdf, &dest)
            .with_context(|| format!("cannot move {} to the trash", pdf.display()))?;
    }
    record.paperstack.pdf_sha256 = None;
    let saved = lib.save(&record, Some(&hit.path));
    let path = match saved {
        Ok(path) => path,
        Err(e) => {
            let _ = fs::rename(&dest, &pdf);
            return Err(e);
        }
    };
    index.upsert(lib, &path, record, Some(&hit.path))?;
    Ok(to)
}

/// Take the collection or label `name` off every paper with it, and its
/// collections within it, as `name/sub`; how many papers had it. Each
/// record is read again first, as another program may have changed it.
pub fn shelf(lib: &Library, index: &mut Index, shelf: Shelf, name: &str) -> anyhow::Result<usize> {
    let name = name.trim().trim_end_matches('/').to_lowercase();
    if name.is_empty() {
        bail!("give the {}'s name", shelf.name());
    }
    let within = format!("{name}/");
    let gone = |n: &String| {
        let n = n.to_lowercase();
        n == name || (shelf == Shelf::Collection && n.starts_with(&within))
    };
    let names = |r: &crate::record::Record| match shelf {
        Shelf::Collection => r.paperstack.folders.clone(),
        Shelf::Label => r.paperstack.labels.clone(),
    };
    let hits: Vec<Hit> = index
        .search(&Query::parse(""), Sort::Relevance, false, None)?
        .into_iter()
        .filter(|h| names(&h.record).iter().any(gone))
        .collect();
    if hits.is_empty() {
        bail!("no {} {name}", shelf.name());
    }
    for h in &hits {
        let mut record = crate::library::read(&lib.root.join(&h.path))?;
        let m = &mut record.paperstack;
        match shelf {
            Shelf::Collection => m.folders.retain(|n| !gone(n)),
            Shelf::Label => m.labels.retain(|n| !gone(n)),
        }
        let path = lib.save(&record, Some(&h.path))?;
        index.upsert(lib, &path, record, Some(&h.path))?;
    }
    Ok(hits.len())
}
