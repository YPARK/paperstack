//! What `paperstack find` and `get` print: for a person, a line a paper;
//! for an agent, JSON; for a document, BibTeX, CSL JSON, citation keys or
//! references in a citation style.

use crate::add::{Added, Adder, Ambiguous};
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
    /// References in a citation style: apa, chicago-author-date,
    /// chicago-notes, mla, ieee, nature, another CSL style by its name, or
    /// a .csl file
    #[arg(long, value_name = "STYLE")]
    pub style: Option<String>,
}

/// How references in a style are written.
#[derive(clap::Args, Debug, Default)]
pub struct Markup {
    /// With --style: plain, markdown or html
    #[arg(long, value_enum, default_value_t, requires = "style")]
    pub markup: crate::style::Markup,
}

pub fn print(lib: &Library, hits: &[Hit], format: &Format, markup: &Markup) -> anyhow::Result<()> {
    if let Some(style) = &format.style {
        let records: Vec<_> = hits.iter().map(|h| h.record.as_ref()).collect();
        println!(
            "{}",
            crate::style::references(&records, style, markup.markup)?
        );
    } else if format.json {
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
/// What came of adding: where a PDF held more than one paper, and `ask`,
/// the one meant asked for on the terminal, and filed with it.
pub fn settle(adder: &mut Adder, done: anyhow::Result<Added>, ask: bool) -> anyhow::Result<Added> {
    match done {
        Err(e) if ask && e.is::<Ambiguous>() => {
            let a = e.downcast::<Ambiguous>()?;
            let found = choose(adder.net, &a)?;
            adder.file_chosen(found, &a.pdf)
        }
        done => done,
    }
}

/// Which of the papers a PDF holds is meant, asked on stderr and read from
/// stdin: by its number or a DOI; nothing, or `q`, is none.
fn choose(net: &crate::net::Net, a: &Ambiguous) -> anyhow::Result<crate::resolve::Found> {
    let name = &a.name;
    eprintln!("{name} holds more than one paper:");
    for (n, p) in a.papers.iter().enumerate() {
        eprintln!("  {}. {}", n + 1, crate::add::listed(p));
    }
    eprint!(
        "add which? 1-{}, or a DOI; nothing or q adds none: ",
        a.papers.len()
    );
    let mut answer = String::new();
    std::io::stdin().read_line(&mut answer)?;
    let answer = answer.trim();
    if answer.is_empty() || answer.eq_ignore_ascii_case("q") {
        anyhow::bail!("{name}: nothing added");
    }
    if let Ok(n) = answer.parse::<usize>() {
        return match a.papers.get(n.wrapping_sub(1)) {
            Some(p) => Ok(p.clone()),
            None => anyhow::bail!("{name}: no paper {n}; nothing added"),
        };
    }
    let crate::source::Source::Doi(doi) = crate::source::Source::of(answer) else {
        anyhow::bail!("{name}: {answer} is neither a number nor a DOI; nothing added");
    };
    match a
        .papers
        .iter()
        .find(|p| p.doi().is_some_and(|d| d.eq_ignore_ascii_case(&doi)))
    {
        Some(p) => Ok(p.clone()),
        None => crate::resolve::by_doi(net, &doi),
    }
}

pub fn print_added(
    lib: &Library,
    done: &[(String, anyhow::Result<Added>)],
    as_json: bool,
) -> anyhow::Result<()> {
    if as_json {
        let all: Vec<Value> = done
            .iter()
            .map(|(given, r)| match r {
                Ok(a) => added_json(lib, a, given),
                Err(e) => failed_json(e, given),
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

/// What was added, for an agent: as `print_added` gives it.
pub fn added_json(lib: &Library, a: &Added, given: &str) -> Value {
    let record = lib.root.join(&a.path);
    json!({
        "given": given,
        "key": a.key,
        "outcome": a.outcome,
        "how": a.how,
        "record": record,
        "pdf": a.has_pdf.then(|| record.with_extension("pdf")),
        "pdf_version": a.pdf_version,
    })
}

/// What could not be added, and why, for an agent: as `reason` names it,
/// and, of a PDF not had, each copy tried and why it was not taken.
pub fn failed_json(e: &anyhow::Error, given: &str) -> Value {
    let mut v = json!({
        "given": given,
        "outcome": "failed",
        "reason": crate::add::reason(e).unwrap_or_else(|| "error".to_string()),
        "error": format!("{e:#}"),
    });
    if let Some(none) = e.downcast_ref::<crate::add::NoOpenCopy>() {
        v["tried"] = json!(none.tried);
        v["page"] = json!(none.page);
    }
    v
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
        "pdf_version": h.has_pdf.then_some(m.pdf_version).flatten(),
        "record": record,
        "score": h.score,
    })
}

/// What `paperstack themes` prints: for a person, the words most papers
/// have, then each theme of `MIN_THEME` papers or more, its words and its
/// authors; and where the papers new since would go. For an agent, JSON.
pub fn themes(
    map: &crate::map::Map,
    placed: &[crate::map::Placing],
    json: bool,
) -> anyhow::Result<()> {
    if json {
        let themes: Vec<Value> = map
            .themes
            .iter()
            .enumerate()
            .map(|(i, t)| {
                let keys: Vec<&str> = map.members(i).map(|(_, k)| k).collect();
                json!({
                    "theme": i + 1,
                    "name": t.name(),
                    "words": t.words,
                    "size": t.size,
                    "authors": t.authors.iter().map(|(a, n)| json!({"name": a, "papers": n})).collect::<Vec<_>>(),
                    "keys": keys,
                })
            })
            .collect();
        let new: Vec<Value> = placed
            .iter()
            .map(|p| json!({"key": p.key, "theme": p.theme + 1, "share": p.share}))
            .collect();
        let common: Vec<Value> = map
            .common
            .iter()
            .map(|(w, n)| json!({"word": w, "papers": n}))
            .collect();
        let out = json!({"built": map.built, "resolution": map.resolution, "common": common, "themes": themes, "new": new});
        println!("{}", serde_json::to_string_pretty(&out)?);
        return Ok(());
    }
    let common: Vec<String> = map
        .common
        .iter()
        .take(15)
        .map(|(w, n)| format!("{w} {n}"))
        .collect();
    println!(
        "{} papers, built {} at resolution {:.2}",
        map.keys.len(),
        map.built,
        map.resolution
    );
    println!("most papers have: {}\n", common.join(", "));
    let named = map.named();
    let small = map.themes.len() - named;
    for (i, t) in map.themes.iter().enumerate().take(named) {
        let words = t.words.join(" · ");
        match &t.called {
            Some(called) => println!("{:>3} {:>5}  {called}: {words}", i + 1, t.size),
            None => println!("{:>3} {:>5}  {words}", i + 1, t.size),
        }
        if !t.authors.is_empty() {
            let a: Vec<String> = t
                .authors
                .iter()
                .map(|(a, n)| format!("{a} ({n})"))
                .collect();
            println!("           {}", a.join(", "));
        }
    }
    if small > 0 {
        println!(
            "    and {small} themes of fewer than {} papers",
            crate::map::MIN_THEME
        );
    }
    if !placed.is_empty() {
        println!("\n{} papers new since, by their nearest:", placed.len());
        for p in placed {
            println!(
                "  {:<24} {:>3}% {}",
                p.key,
                (p.share * 100.0).round(),
                map.themes[p.theme].name()
            );
        }
        println!("`--assign` puts them there; `--rebuild` gathers the themes again");
    }
    Ok(())
}
