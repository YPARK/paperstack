//! Bibliographies kept up to date: a `.bib` or CSL JSON file of the papers
//! a query finds, or of keys a manuscript cites, written again whenever the
//! index changes and the file would. The list of them is this machine's,
//! beside the index, as the files' paths are.

use crate::index::{Index, Picked};
use crate::record::Record;
use anyhow::{Context, bail};
use serde::{Deserialize, Serialize};
use std::borrow::Cow;
use std::path::{Path, PathBuf};

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Bib {
    pub output: PathBuf,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub query: Option<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub keys: Vec<String>,
    pub format: Format,
}

#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Format {
    Bibtex,
    Csl,
}

impl Format {
    /// The format a file's name says: `.json` is CSL JSON, `.bib` BibTeX.
    pub fn of(path: &Path) -> anyhow::Result<Format> {
        match path.extension().and_then(|e| e.to_str()) {
            Some(e) if e.eq_ignore_ascii_case("bib") => Ok(Format::Bibtex),
            Some(e) if e.eq_ignore_ascii_case("json") => Ok(Format::Csl),
            _ => bail!("a bibliography is a .bib file, or, as CSL JSON, a .json one"),
        }
    }

    /// The format a file's name says, where it is the one `asked`, if any.
    pub fn asked(path: &Path, asked: Option<Format>) -> anyhow::Result<Format> {
        let of = Format::of(path)?;
        if asked.is_some_and(|a| a != of) {
            bail!(
                "{} is not of the format asked for: BibTeX is written to a .bib file, CSL JSON to a .json one",
                path.display()
            );
        }
        Ok(of)
    }
}

/// `papers` as a bibliography, each under the key it was asked by, as a
/// manuscript cites it, though merged since.
pub fn render(papers: &[Picked], format: Format) -> anyhow::Result<String> {
    let records: Vec<Cow<Record>> = papers
        .iter()
        .map(|p| {
            if p.asked == p.hit.record.key {
                Cow::Borrowed(p.hit.record.as_ref())
            } else {
                Cow::Owned(Record {
                    key: p.asked.clone(),
                    ..Record::clone(&p.hit.record)
                })
            }
        })
        .collect();
    Ok(match format {
        Format::Bibtex => {
            crate::bibtex::entries(&records.iter().map(AsRef::as_ref).collect::<Vec<_>>())
        }
        Format::Csl => {
            serde_json::to_string_pretty(&records.iter().map(|r| r.item()).collect::<Vec<_>>())?
        }
    })
}

/// Write `bib` as the index has it now: only where every key it cites is
/// had, and something is found, so a file is never cut short; and only
/// where it would change. The papers in it.
pub fn write(index: &Index, bib: &Bib) -> anyhow::Result<Vec<Picked>> {
    let chosen = index.choose(bib.keys.iter().map(String::as_str), bib.query.as_deref())?;
    if !chosen.missing.is_empty() {
        bail!(
            "{} not written, as no paper has {}",
            bib.output.display(),
            chosen.missing.join(", ")
        );
    }
    if chosen.papers.is_empty() {
        bail!("{} not written: no paper is found", bib.output.display());
    }
    // By key, so a paper added elsewhere does not reorder the file.
    let mut papers = chosen.papers;
    papers.sort_by(|a, b| a.asked.cmp(&b.asked));
    let text = render(&papers, bib.format)?;
    if !std::fs::read_to_string(&bib.output).is_ok_and(|was| was == text) {
        if let Some(folder) = bib.output.parent() {
            std::fs::create_dir_all(folder)
                .with_context(|| format!("cannot make {}", folder.display()))?;
        }
        crate::library::write(&bib.output, text.as_bytes())?;
    }
    Ok(papers)
}

/// Write `bib`, and, if `keep`, keep it up to date: as `find --output` and
/// `cite` with an output do. The papers in it.
pub fn write_out(index: &Index, bib: Bib, keep: bool) -> anyhow::Result<Vec<Picked>> {
    if keep {
        self::keep(index, bib)
    } else {
        write(index, &bib)
    }
}

/// Write `bib` now, and keep it up to date from here on, in place of one
/// kept at the same path.
pub fn keep(index: &Index, bib: Bib) -> anyhow::Result<Vec<Picked>> {
    let bib = Bib {
        output: std::path::absolute(&bib.output)?,
        ..bib
    };
    let papers = write(index, &bib)?;
    let mut all = list(index);
    all.retain(|b| b.output != bib.output);
    all.push(bib);
    save(index, &all)?;
    Ok(papers)
}

/// Keep `output` up to date no more; whether it was.
pub fn forget(index: &Index, output: &Path) -> anyhow::Result<bool> {
    let output = std::path::absolute(output)?;
    let mut all = list(index);
    let before = all.len();
    all.retain(|b| b.output != output);
    if all.len() == before {
        return Ok(false);
    }
    save(index, &all)?;
    Ok(true)
}

/// The bibliographies kept up to date.
pub fn list(index: &Index) -> Vec<Bib> {
    std::fs::read(path(index))
        .ok()
        .and_then(|data| serde_json::from_slice(&data).ok())
        .unwrap_or_default()
}

/// Write each kept bibliography that would change; what could not be is
/// said on stderr, once a run, the file left as it was.
pub fn sync(index: &Index) {
    static SAID: std::sync::Mutex<Vec<String>> = std::sync::Mutex::new(Vec::new());
    for bib in list(index) {
        if let Err(e) = write(index, &bib) {
            let why = crate::add::one_line(&e);
            let mut said = SAID.lock().unwrap_or_else(|e| e.into_inner());
            if !said.contains(&why) {
                eprintln!("paperstack: {why}");
                said.push(why);
            }
        }
    }
}

fn save(index: &Index, all: &[Bib]) -> anyhow::Result<()> {
    crate::library::write(&path(index), &serde_json::to_vec_pretty(all)?)
}

/// Beside the index: `index-<id>.kept.json` for `index-<id>.json`.
fn path(index: &Index) -> PathBuf {
    index.path().with_extension("kept.json")
}
