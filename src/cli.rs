//! What `paperstack find` and `get` print: for a person, a line a paper;
//! for an agent, JSON; for a document, BibTeX, CSL JSON or citation keys.

use crate::add::Added;
use crate::bibtex;
use crate::index::Hit;
use crate::library::Library;
use serde_json::{Value, json};

#[derive(clap::Args, Debug, Default)]
#[group(multiple = false)]
pub struct Format {
    /// Each paper as JSON: key, title, authors, year, ids, folders, files
    #[arg(long)]
    pub json: bool,
    /// BibTeX, plain: `journal` and `year`, as reference managers write it
    #[arg(long)]
    pub bib: bool,
    /// A CSL JSON list, for pandoc's and ohp's `bibliography:`
    #[arg(long)]
    pub csl: bool,
    /// The keys, to cite: [@a; @b]
    #[arg(long)]
    pub keys: bool,
    /// The PDFs' paths, one a line, for papers with one
    #[arg(long, alias = "path")]
    pub paths: bool,
}

pub fn print(lib: &Library, hits: &[Hit], format: &Format) -> anyhow::Result<()> {
    if format.json {
        let all: Vec<Value> = hits.iter().map(|h| json(lib, h)).collect();
        println!("{}", serde_json::to_string_pretty(&all)?);
    } else if format.bib {
        let records: Vec<_> = hits.iter().map(|h| h.record.as_ref()).collect();
        print!("{}", bibtex::entries(&records));
    } else if format.csl {
        let items: Vec<Value> = hits.iter().map(|h| h.record.item()).collect();
        println!("{}", serde_json::to_string_pretty(&items)?);
    } else if format.keys {
        let keys: Vec<String> = hits.iter().map(|h| format!("@{}", h.record.key)).collect();
        println!("[{}]", keys.join("; "));
    } else if format.paths {
        for h in hits.iter().filter(|h| h.has_pdf) {
            println!("{}", lib.root.join(&h.path).with_extension("pdf").display());
        }
    } else {
        for h in hits {
            let r = &h.record;
            let first = r
                .authors()
                .first()
                .map(|n| n.family.clone())
                .unwrap_or_default();
            let year = r
                .year()
                .map_or_else(|| "n.d.".to_string(), |y| y.to_string());
            println!("{:<24} {year:<5} {first:<16} {}", r.key, r.title());
        }
    }
    Ok(())
}

/// What `add` and `inbox` did with each thing given; an error if any
/// could not be added, after saying what was.
pub fn print_added(
    lib: &Library,
    done: &[(String, anyhow::Result<Added>)],
    as_json: bool,
) -> anyhow::Result<()> {
    if as_json {
        let all: Vec<Value> = done
            .iter()
            .map(|(given, r)| match r {
                Ok(a) => json!({
                    "given": given,
                    "key": a.key,
                    "outcome": a.outcome,
                    "how": a.how,
                    "record": lib.root.join(&a.path),
                    "pdf": a.has_pdf.then(|| lib.root.join(&a.path).with_extension("pdf")),
                }),
                Err(e) => json!({ "given": given, "error": format!("{e:#}") }),
            })
            .collect();
        println!("{}", serde_json::to_string_pretty(&all)?);
    } else {
        for (given, r) in done {
            match r {
                Ok(a) => println!("{:<24} {}: {}", a.key, a.summary(), a.path.display()),
                Err(e) => eprintln!("paperstack: {given}: {e:#}"),
            }
        }
    }
    let failed = done.iter().filter(|(_, r)| r.is_err()).count();
    if failed > 0 {
        anyhow::bail!("{failed} of {} not added", done.len());
    }
    Ok(())
}

pub fn json(lib: &Library, h: &Hit) -> Value {
    let r = &h.record;
    let m = &r.paperstack;
    let record = lib.root.join(&h.path);
    json!({
        "key": r.key,
        "title": r.title(),
        "authors": r.authors().iter().map(|n| if n.given.is_empty() { n.family.clone() } else { format!("{}, {}", n.family, n.given) }).collect::<Vec<_>>(),
        "year": r.year(),
        "venue": r.venue(),
        "type": r.kind(),
        "doi": r.doi(),
        "arxiv": m.arxiv,
        "folders": m.folders,
        "labels": m.labels,
        "starred": m.starred,
        "has_abstract": r.abstract_text().is_some(),
        "pdf": h.has_pdf.then(|| record.with_extension("pdf")),
        "record": record,
        "score": h.score,
    })
}
