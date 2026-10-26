//! Taking papers out of the library, and collections and labels off its
//! papers. A paper deleted is moved, its record and its PDF, into `.trash/`
//! at the library's root, filed there as it was: hidden, so no longer
//! found, but kept, and synced, until the trash is emptied by hand. Moved
//! back, it is found again. A PDF deleted alone goes there too, and the
//! paper stays. A collection deleted is taken off its papers, which stay.
//! A label is put on papers here too, as a theme's, to be taken off again
//! as any label is.

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
    lib.prune(&record);
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
    record.paperstack.pdf_version = None;
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

/// The files in the trash of the paper `key`, by their names, `<key>_…`,
/// ending in `.ext`: from the library's root.
fn in_trash(lib: &Library, key: &str, ext: &str) -> Vec<PathBuf> {
    let (start, end) = (format!("{}_", key.to_lowercase()), format!(".{ext}"));
    let mut found = Vec::new();
    let mut dirs = vec![lib.root.join(TRASH)];
    while let Some(dir) = dirs.pop() {
        for entry in fs::read_dir(&dir).into_iter().flatten().flatten() {
            let path = entry.path();
            if entry.file_type().is_ok_and(|t| t.is_dir()) {
                dirs.push(path);
                continue;
            }
            let name = entry.file_name().to_string_lossy().to_lowercase();
            if name.starts_with(&start) && name.ends_with(&end) {
                found.push(path.strip_prefix(&lib.root).unwrap_or(&path).to_path_buf());
            }
        }
    }
    found
}

/// The one file of `found`, the trash's of `what`: none guessed between.
fn only_one(mut found: Vec<PathBuf>, what: &str) -> anyhow::Result<PathBuf> {
    match found.len() {
        1 => Ok(found.remove(0)),
        0 => bail!("no {what} is in the trash"),
        _ => bail!(
            "more than one {what} is in the trash: {}; move the one meant back by hand",
            found
                .iter()
                .map(|p| p.display().to_string())
                .collect::<Vec<_>>()
                .join(", ")
        ),
    }
}

/// Bring the paper `key` back from the trash, its record and its PDF, where
/// it was filed; its record's path, from the library's root. Not where a
/// paper with that key is here again, as added since.
pub fn restore(lib: &Library, index: &mut Index, key: &str) -> anyhow::Result<PathBuf> {
    if let Some(hit) = index.get(key)? {
        bail!("{} is in the library already", hit.record.key);
    }
    let trashed = only_one(
        in_trash(lib, key, "json"),
        &format!("paper with the key {key}"),
    )?;
    let rel = trashed.strip_prefix(TRASH)?.to_path_buf();
    let (from, to) = (lib.root.join(&trashed), lib.root.join(&rel));
    if to.exists() {
        bail!("{} is there already", to.display());
    }
    fs::create_dir_all(to.parent().expect("filed in a folder"))?;
    // The PDF first: once the record is back, the paper is.
    let pdf = from.with_extension("pdf");
    if pdf.exists() && !to.with_extension("pdf").exists() {
        fs::rename(&pdf, to.with_extension("pdf"))?;
    }
    fs::rename(&from, &to).with_context(|| format!("cannot move {} back", from.display()))?;
    lib.prune(&from);
    let record = crate::library::read(&to)?;
    index.upsert(lib, &rel, record, None)?;
    Ok(rel)
}

/// Bring the PDF of the paper `key` back from the trash: the one trashed
/// where it is filed, as when the paper's PDF was deleted or replaced, or
/// the only one there is; none is guessed between. The PDF it has,
/// if any, goes to the trash in its place. Its path in the trash, from the
/// library's root.
pub fn restore_pdf(lib: &Library, index: &mut Index, key: &str) -> anyhow::Result<PathBuf> {
    let hit = index
        .get(key)?
        .with_context(|| format!("no paper with the key {key}"))?;
    // The one trashed where the paper is filed, else the only one.
    let filed = Path::new(TRASH).join(&hit.path).with_extension("pdf");
    let all = in_trash(lib, &hit.record.key, "pdf");
    let trashed = if all.contains(&filed) {
        filed
    } else {
        only_one(all, &format!("PDF of {key}"))?
    };
    let (from, here) = (
        lib.root.join(&trashed),
        lib.root.join(&hit.path).with_extension("pdf"),
    );
    let bytes = fs::read(&from)?;
    if here.exists() {
        // The two swap places, through a third name.
        let aside = here.with_extension("pdf.swap");
        fs::rename(&here, &aside)?;
        fs::rename(&from, &here)?;
        fs::rename(&aside, &from)?;
    } else {
        fs::rename(&from, &here)?;
        lib.prune(&from);
    }
    let mut record = crate::library::read(&lib.root.join(&hit.path))?;
    crate::add::stamp(&mut record, &bytes, "");
    let path = lib.save(&record, Some(&hit.path))?;
    index.upsert(lib, &path, record, Some(&hit.path))?;
    Ok(trashed)
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
    let keys: Vec<String> = hits.iter().map(|h| h.record.key.clone()).collect();
    edit(lib, index, &keys, |r| {
        let m = &mut r.paperstack;
        match shelf {
            Shelf::Collection => m.folders.retain(|n| !gone(n)),
            Shelf::Label => m.labels.retain(|n| !gone(n)),
        }
        Ok(true)
    })?;
    Ok(hits.len())
}

/// The label `name` put on each paper of `keys` that has not got it; how
/// many had not.
pub fn label(
    lib: &Library,
    index: &mut Index,
    keys: &[String],
    name: &str,
) -> anyhow::Result<usize> {
    let name = name.trim();
    if name.is_empty() {
        bail!("give the label's name");
    }
    edit(lib, index, keys, |r| {
        let m = &mut r.paperstack;
        let has = m.labels.iter().any(|l| l.eq_ignore_ascii_case(name));
        if !has {
            m.labels.push(name.to_string());
        }
        Ok(!has)
    })
}

/// Each paper of `keys` changed by `change`, and written where it says it
/// changed; how many were. Each record is read again first, as another
/// program may have changed it; the index is brought up to them once.
pub fn edit(
    lib: &Library,
    index: &mut Index,
    keys: &[String],
    change: impl Fn(&mut crate::record::Record) -> anyhow::Result<bool>,
) -> anyhow::Result<usize> {
    let mut written = Vec::new();
    let mut each = || -> anyhow::Result<()> {
        for key in keys {
            let Some(h) = index.get(key)? else { continue };
            let mut record = crate::library::read(&lib.root.join(&h.path))?;
            if change(&mut record)? {
                let path = lib.save(&record, Some(&h.path))?;
                written.push((path, record, h.path));
            }
        }
        Ok(())
    };
    // Those written before any that could not be, in the index all the same.
    let done = each();
    let n = written.len();
    index.upsert_all(lib, written)?;
    done.map(|()| n)
}
