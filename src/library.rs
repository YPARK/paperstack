//! The library: a folder, as in Dropbox, of papers filed as `layout.rs`
//! files them, with `paperstack.toml` at its root. Files are written whole
//! or not at all, so a sync never sees half a record.

use crate::layout;
use crate::record::Record;
use anyhow::{Context, bail};
use std::collections::HashSet;
use std::fs;
use std::path::{Path, PathBuf};
use std::time::UNIX_EPOCH;

pub const MARKER: &str = "paperstack.toml";
pub const INBOX: &str = "inbox";
pub const AGENTS: &str = "AGENTS.md";

pub struct Library {
    pub root: PathBuf,
}

/// A record file found, with what tells whether it changed since read.
pub struct Found {
    /// From the library's root.
    pub path: PathBuf,
    pub mtime: i64,
    pub size: i64,
}

#[derive(Default)]
pub struct Walk {
    pub records: Vec<Found>,
    /// PDFs, from the root, as their records' paths with `.pdf`.
    pub pdfs: HashSet<PathBuf>,
    /// Dropbox's "conflicted copy" files, left for `check` to settle.
    pub conflicts: Vec<PathBuf>,
}

impl Library {
    pub fn open(root: &Path) -> anyhow::Result<Self> {
        if !root.join(MARKER).is_file() {
            bail!(
                "no library at {}: `paperstack init` makes one (`--demo` with papers to try)",
                root.display()
            );
        }
        Ok(Library {
            root: root.to_path_buf(),
        })
    }

    pub fn init(root: &Path) -> anyhow::Result<Self> {
        fs::create_dir_all(root.join(INBOX))
            .with_context(|| format!("cannot make {}", root.display()))?;
        let marker = root.join(MARKER);
        if !marker.exists() {
            write(
                &marker,
                b"# A paperstack library: papers filed as year/lastname/firstname/<key>_<slug>.\nversion = 1\nkeys = \"paperpile\"\n",
            )?;
        }
        write(&root.join(AGENTS), crate::agents::TEXT.as_bytes())?;
        Ok(Library {
            root: root.to_path_buf(),
        })
    }

    /// Write `r` where it is filed. Where it was filed elsewhere, at `was`
    /// from the root, as before its name or year was corrected, its record
    /// and PDF are moved there first. Returns its path from the root.
    pub fn save(&self, r: &Record, was: Option<&Path>) -> anyhow::Result<PathBuf> {
        let rel = layout::record(r);
        let path = self.root.join(&rel);
        fs::create_dir_all(path.parent().expect("filed in a folder"))?;
        if let Some(was) = was.filter(|w| *w != rel) {
            let old_pdf = self.root.join(was).with_extension("pdf");
            if old_pdf.exists() {
                fs::rename(&old_pdf, self.root.join(layout::pdf(r)))?;
            }
            match fs::remove_file(self.root.join(was)) {
                Err(e) if e.kind() != std::io::ErrorKind::NotFound => {
                    return Err(e).context("cannot take away the record where it was");
                }
                _ => {}
            }
        }
        let mut json = serde_json::to_vec_pretty(r)?;
        json.push(b'\n');
        write(&path, &json)?;
        Ok(rel)
    }

    /// Every record and PDF under the root, but in the inbox and in hidden
    /// folders.
    pub fn walk(&self) -> anyhow::Result<Walk> {
        let mut walk = Walk::default();
        let mut dirs = vec![self.root.clone()];
        while let Some(dir) = dirs.pop() {
            let entries = match fs::read_dir(&dir) {
                Ok(entries) => entries,
                Err(e) if dir == self.root => {
                    return Err(e).with_context(|| format!("cannot read {}", dir.display()));
                }
                Err(_) => continue,
            };
            for entry in entries.flatten() {
                let name = entry.file_name();
                let name = name.to_string_lossy();
                if name.starts_with('.') {
                    continue;
                }
                let path = entry.path();
                let Ok(meta) = entry.metadata() else { continue };
                if meta.is_dir() {
                    if !(dir == self.root && name == INBOX) {
                        dirs.push(path);
                    }
                    continue;
                }
                let rel = path.strip_prefix(&self.root).unwrap_or(&path).to_path_buf();
                if name.contains("conflicted copy") {
                    walk.conflicts.push(rel);
                } else if name.ends_with(".json") {
                    let mtime = meta
                        .modified()
                        .ok()
                        .and_then(|t| t.duration_since(UNIX_EPOCH).ok())
                        .map_or(0, |d| d.as_nanos() as i64);
                    walk.records.push(Found {
                        path: rel,
                        mtime,
                        size: meta.len() as i64,
                    });
                } else if name.ends_with(".pdf") {
                    walk.pdfs.insert(rel.with_extension("json"));
                }
            }
        }
        Ok(walk)
    }
}

pub fn read(path: &Path) -> anyhow::Result<Record> {
    let data = fs::read(path).with_context(|| format!("cannot read {}", path.display()))?;
    serde_json::from_slice(&data).with_context(|| format!("cannot read {}", path.display()))
}

/// Write `data` to `path` whole: to a hidden file beside it, then renamed
/// over it.
pub fn write(path: &Path, data: &[u8]) -> anyhow::Result<()> {
    let name = path.file_name().expect("a file").to_string_lossy();
    let tmp = path.with_file_name(format!(".{name}.{}.tmp", std::process::id()));
    fs::write(&tmp, data).with_context(|| format!("cannot write {}", tmp.display()))?;
    fs::rename(&tmp, path).with_context(|| format!("cannot write {}", path.display()))
}
