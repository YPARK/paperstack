//! References set out as a citation style has them: APA's, Chicago's, any
//! CSL style hayagriva keeps, or one from a `.csl` file.

use crate::record::Record;
use anyhow::{Context, bail};
use hayagriva::archive::{ArchivedStyle, locales};
use hayagriva::citationberg::json::Item;
use hayagriva::citationberg::{FontStyle, FontWeight, IndependentStyle, Style};
use hayagriva::{
    BibliographyDriver, BibliographyRequest, BufWriteFormat, CitationItem, CitationRequest,
    ElemChild, ElemChildren,
};
use serde_json::{Value, json};
use std::path::Path;

/// The styles the BibTeX dialog goes through, after BibTeX.
pub const CYCLED: &[&str] = &[
    "apa",
    "chicago-author-date",
    "chicago-notes",
    "mla",
    "ieee",
    "nature",
];

/// As hayagriva 0.10 sets them out, these leave the authors out.
const BROKEN: &[&str] = &["nlm-citation-sequence", "nlm-citation-sequence-superscript"];

/// How the references are written.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, clap::ValueEnum)]
pub enum Markup {
    /// Text alone, one reference a line
    #[default]
    Plain,
    /// Italics as *these*, links as <these>; a blank line between
    Markdown,
    /// A <p> each, italics as <i>
    Html,
}

impl Markup {
    pub fn of(name: &str) -> anyhow::Result<Self> {
        match name {
            "plain" => Ok(Markup::Plain),
            "markdown" => Ok(Markup::Markdown),
            "html" => Ok(Markup::Html),
            _ => bail!("markup is plain, markdown or html, not {name}"),
        }
    }
}

/// The style of `name`, as hayagriva keeps it, or of a `.csl` file there.
fn style(name: &str) -> anyhow::Result<IndependentStyle> {
    if name.ends_with(".csl") {
        let xml = std::fs::read_to_string(Path::new(name))
            .with_context(|| format!("cannot read the style {name}"))?;
        return match Style::from_xml(&xml).with_context(|| format!("{name}: not a CSL style"))? {
            Style::Independent(s) => Ok(s),
            Style::Dependent(_) => bail!("{name} only names another style: give that one"),
        };
    }
    let Some(archived) = ArchivedStyle::by_name(name) else {
        let names: Vec<_> = ArchivedStyle::all()
            .iter()
            .map(|s| s.names()[0])
            .filter(|n| !BROKEN.contains(n))
            .collect();
        bail!(
            "no style {name}: a .csl file, or one of {}",
            names.join(", ")
        );
    };
    if BROKEN.contains(&archived.names()[0]) {
        bail!("{name} leaves the authors out as yet: elsevier-vancouver is the same but for that");
    }
    match archived.get() {
        Style::Independent(s) => Ok(s),
        Style::Dependent(_) => bail!("{name} only names another style"),
    }
}

/// What the dialog calls the style of `name`.
pub fn title(name: &str) -> String {
    ArchivedStyle::by_name(name).map_or_else(|| name.to_string(), |s| s.display_name().into())
}

/// The paper as the style reads it: given names written as initials alone,
/// as PubMed's `TJ`, as initials, `T. J.`; and fields CSL has no place
/// for, as Crossref's `relation`, left out.
fn item(r: &Record) -> anyhow::Result<Item> {
    let mut item = r.item();
    for role in ["author", "editor"] {
        let Some(Value::Array(names)) = item.get_mut(role) else {
            continue;
        };
        for name in names {
            if let Some(Value::String(given)) = name.get_mut("given")
                && (2..=3).contains(&given.len())
                && given.chars().all(|c| c.is_ascii_uppercase())
            {
                *given = given
                    .chars()
                    .map(|c| format!("{c}."))
                    .collect::<Vec<_>>()
                    .join(" ");
            }
        }
    }
    if let Ok(i) = serde_json::from_value(item.clone()) {
        return Ok(i);
    }
    if let Value::Object(map) = &mut item {
        map.retain(|k, v| {
            matches!(k.as_str(), "id" | "type")
                || serde_json::from_value::<Item>(json!({ "id": "", "type": "article", k: v }))
                    .is_ok()
        });
    }
    serde_json::from_value(item).with_context(|| format!("{}: not CSL as a style reads it", r.key))
}

/// The references of `records`, in the style of `name`: in its order, as
/// author and year, or numbered in the order given.
pub fn references(records: &[&Record], name: &str, markup: Markup) -> anyhow::Result<String> {
    let style = style(name)?;
    let items = records
        .iter()
        .map(|r| item(r))
        .collect::<anyhow::Result<Vec<_>>>()?;
    let locales = locales();
    let mut driver = BibliographyDriver::new();
    for i in &items {
        driver.citation(CitationRequest::from_items(
            vec![CitationItem::with_entry(i)],
            &style,
            &locales,
        ));
    }
    let done = driver.finish(BibliographyRequest::new(&style, None, &locales));
    // A style of notes alone: the notes.
    let refs: Vec<String> = match done.bibliography {
        Some(b) => b
            .items
            .into_iter()
            .map(|i| {
                let mut s = String::new();
                if let Some(first) = &i.first_field {
                    s += &write(&ElemChildren(vec![first.clone()]), markup);
                    s.push(' ');
                }
                s + &write(&i.content, markup)
            })
            .collect(),
        None => done
            .citations
            .into_iter()
            .map(|c| write(&c.citation, markup))
            .collect(),
    };
    // A field left out, its comma kept before the full stop: as
    // Chicago's of a paper without a volume.
    let refs = refs.iter().map(|r| r.trim().replace(",.", "."));
    Ok(match markup {
        Markup::Plain => refs.collect::<Vec<_>>().join("\n"),
        Markup::Markdown => refs.collect::<Vec<_>>().join("\n\n"),
        Markup::Html => refs
            .map(|r| format!("<p>{r}</p>"))
            .collect::<Vec<_>>()
            .join("\n"),
    })
}

fn write(children: &ElemChildren, markup: Markup) -> String {
    match markup {
        Markup::Plain => plain(children),
        Markup::Html => {
            let mut s = String::new();
            let _ = children.write_buf(&mut s, BufWriteFormat::Html);
            s
        }
        Markup::Markdown => {
            let mut runs = Vec::new();
            runs_of(children, &mut runs);
            markdown(&runs)
        }
    }
}

fn plain(children: &ElemChildren) -> String {
    let mut s = String::new();
    let _ = children.write_buf(&mut s, BufWriteFormat::Plain);
    s
}

/// A stretch of text set alike: italic, bold, or a link to its address.
struct Run {
    text: String,
    italic: bool,
    bold: bool,
    link: bool,
}

fn runs_of(children: &ElemChildren, runs: &mut Vec<Run>) {
    for child in &children.0 {
        let (f, link) = match child {
            ElemChild::Text(f) => (f, false),
            ElemChild::Link { text, .. } => (text, true),
            ElemChild::Elem(e) => {
                runs_of(&e.children, runs);
                continue;
            }
            ElemChild::Markup(m) => {
                runs.push(Run {
                    text: m.clone(),
                    italic: false,
                    bold: false,
                    link: false,
                });
                continue;
            }
            ElemChild::Transparent { .. } => continue,
        };
        let italic = f.formatting.font_style == FontStyle::Italic;
        let bold = f.formatting.font_weight == FontWeight::Bold;
        match runs.last_mut() {
            Some(r) if (r.italic, r.bold, r.link) == (italic, bold, link) => r.text += &f.text,
            _ => runs.push(Run {
                text: f.text.clone(),
                italic,
                bold,
                link,
            }),
        }
    }
}

/// The runs as Markdown: the marks inside the spaces around what they mark,
/// as Markdown reads them only there.
fn markdown(runs: &[Run]) -> String {
    let mut s = String::new();
    for r in runs {
        let mark = match (r.italic, r.bold) {
            (true, true) => "***",
            (false, true) => "**",
            (true, false) => "*",
            (false, false) => "",
        };
        let body = r.text.trim();
        if body.is_empty() || (mark.is_empty() && !r.link) {
            s += &escaped(&r.text);
            continue;
        }
        let lead = &r.text[..r.text.len() - r.text.trim_start().len()];
        let tail = &r.text[r.text.trim_end().len()..];
        let body = if r.link {
            format!("<{body}>")
        } else {
            escaped(body)
        };
        s += &format!("{lead}{mark}{body}{mark}{tail}");
    }
    s
}

/// What Markdown would read as its own, as text.
fn escaped(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for c in s.chars() {
        if matches!(c, '*' | '_' | '[' | ']' | '<' | '>' | '`' | '\\') {
            out.push('\\');
        }
        out.push(c);
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn paper() -> Record {
        let mut r = crate::demo::records().remove(0);
        r.key = "Okafor2024-ab".to_string();
        r.csl = json!({
            "type": "article-journal",
            "title": "Tidal rhythms in a made-up estuary of glass eels",
            "author": [{ "family": "Okafor", "given": "Ada B." }, { "family": "Lindqvist", "given": "Per" }],
            "container-title": "Journal of Imaginary Ecology",
            "volume": "12", "issue": "3", "page": "101-119",
            "issued": { "date-parts": [[2024, 5]] },
            "DOI": "10.5555/fict.2024.01",
            "relation": { "is-preprint-of": [{ "id-type": "doi", "id": "10.5555/x" }] }
        });
        r
    }

    #[test]
    fn sets_out_apa_and_chicago() {
        let r = paper();
        assert_eq!(
            references(&[&r], "apa", Markup::Plain).unwrap(),
            "Okafor, A. B., & Lindqvist, P. (2024). Tidal rhythms in a made-up estuary of \
             glass eels. Journal of Imaginary Ecology, 12(3), 101–119. \
             https://doi.org/10.5555/fict.2024.01"
        );
        assert_eq!(
            references(&[&r], "chicago-author-date", Markup::Plain).unwrap(),
            "Okafor, Ada B., and Per Lindqvist. 2024. “Tidal Rhythms in a Made-up Estuary of \
             Glass Eels.” Journal of Imaginary Ecology 12 (3): 101–19. \
             https://doi.org/10.5555/fict.2024.01."
        );
    }

    #[test]
    fn marks_italics_up() {
        let r = paper();
        let md = references(&[&r], "apa", Markup::Markdown).unwrap();
        assert!(
            md.contains("*Journal of Imaginary Ecology*, *12*(3)"),
            "{md}"
        );
        assert!(
            md.ends_with("<https://doi.org/10.5555/fict.2024.01>"),
            "{md}"
        );
        let html = references(&[&r], "apa", Markup::Html).unwrap();
        assert!(html.starts_with("<p>Okafor"), "{html}");
        assert!(html.contains("font-style: italic"), "{html}");
    }

    #[test]
    fn reads_initials_as_initials() {
        let mut r = paper();
        r.csl["author"] = json!([{ "family": "Okafor", "given": "AB" }]);
        for field in ["volume", "issue", "page"] {
            r.csl.as_object_mut().unwrap().remove(field);
        }
        let apa = references(&[&r], "apa", Markup::Plain).unwrap();
        assert!(apa.starts_with("Okafor, A. B. (2024)."), "{apa}");
        let chicago = references(&[&r], "chicago-author-date", Markup::Plain).unwrap();
        assert!(!chicago.contains(",."), "{chicago}");
    }

    #[test]
    fn numbers_in_the_order_given() {
        let a = paper();
        let mut b = paper();
        b.key = "Brandt2025-pb".to_string();
        b.csl["author"] = json!([{ "family": "Brandt", "given": "Jo" }]);
        let ieee = references(&[&b, &a], "ieee", Markup::Plain).unwrap();
        let lines: Vec<_> = ieee.lines().collect();
        assert!(lines[0].starts_with("[1] J. Brandt"), "{ieee}");
        assert!(lines[1].starts_with("[2] A. B. Okafor"), "{ieee}");
    }

    #[test]
    fn every_cycled_style_names_the_authors() {
        let r = paper();
        for name in CYCLED {
            let s = references(&[&r], name, Markup::Plain).unwrap();
            assert!(s.to_lowercase().contains("okafor"), "{name}: {s}");
        }
    }

    #[test]
    fn says_which_styles_there_are() {
        let r = paper();
        let e = references(&[&r], "nope", Markup::Plain)
            .unwrap_err()
            .to_string();
        assert!(e.contains("chicago-author-date"), "{e}");
        let e = references(&[&r], "vancouver", Markup::Plain)
            .unwrap_err()
            .to_string();
        assert!(e.contains("elsevier-vancouver"), "{e}");
    }
}
