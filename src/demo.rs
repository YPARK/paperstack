//! Papers for `paperstack init --demo`, to try the library with: made up,
//! as are their authors, venues and identifiers, none of them a real
//! paper's; the tests' library too.

use crate::record::{Meta, Record};
use serde_json::{Value, json};

struct Paper {
    key: &'static str,
    kind: &'static str,
    title: &'static str,
    /// Family name, then initials; none for an organisation.
    authors: &'static [(&'static str, &'static str)],
    venue: &'static str,
    year: i32,
    doi: Option<&'static str>,
    arxiv: Option<&'static str>,
    folders: &'static [&'static str],
    labels: &'static [&'static str],
    starred: bool,
    summary: &'static str,
}

const PAPERS: &[Paper] = &[
    Paper {
        key: "Whitlock2017-ex",
        kind: "article-journal",
        title: "Bounding hidden bias in observational research: Introducing the W-value",
        authors: &[("Whitlock", "TJ"), ("Dunmore", "P")],
        venue: "Ann Fict Med",
        year: 2017,
        doi: Some("10.5555/demo.2017.01"),
        arxiv: None,
        folders: &["mediation", "study design"],
        labels: &["method"],
        starred: true,
        summary: "The W-value: how strongly a hidden confounder would have to be \
                  associated with exposure and outcome to explain away an association.",
    },
    Paper {
        key: "Corbett2021-mr",
        kind: "article-journal",
        title: "Instrumental variables for mediation analysis: current methods and \
                challenges for practice",
        authors: &[
            ("Corbett", "AR"),
            ("Sandoval", "E"),
            ("Hartley", "G"),
            ("Redmond", "RC"),
            ("Ashby Grant", "G"),
            ("Hollis", "J"),
            ("Tanner", "AE"),
            ("Dalton", "NM"),
            ("Howell", "LD"),
        ],
        venue: "Eur J Fict Epidemiol",
        year: 2021,
        doi: Some("10.5555/demo.2021.02"),
        arxiv: None,
        folders: &["mediation", "Instruments"],
        labels: &[],
        starred: false,
        summary: "A review of instrumental variable methods for mediation analysis.",
    },
    Paper {
        key: "Nakamura2016-al",
        kind: "article-journal",
        title: "Active learning to choose which compound experiments to run on protein patterns",
        authors: &[
            ("Nakamura", "AW"),
            ("Kessler", "JD"),
            ("Sutton", "DP"),
            ("Moreau", "RF"),
        ],
        venue: "Fict Biol",
        year: 2016,
        doi: Some("10.5555/demo.2016.03"),
        arxiv: None,
        folders: &["Active_Screens"],
        labels: &["method"],
        starred: false,
        summary: "Active learning picks which compound and target experiments to run next.",
    },
    Paper {
        key: "Lindqvist2024-cs",
        kind: "paper-conference",
        title: "Gene2Prose: Teaching Large Language Models the Language of Tissues",
        authors: &[
            ("Lindqvist", "D"),
            ("Rahimi", "SA"),
            ("Lévesque", "S"),
            ("Pemberton", "N"),
            ("Zhou", "D"),
        ],
        venue: "International Conference on Imagined Learning",
        year: 2024,
        doi: None,
        arxiv: None,
        folders: &["Single Cell Data"],
        labels: &["to-read"],
        starred: false,
        summary: "Single-cell expression written as ranked gene names, for a language model.",
    },
    Paper {
        key: "Abbas2026-st",
        kind: "article-journal",
        title: "Predicting cellular responses to perturbation across many contexts with Meridian",
        authors: &[
            ("Abbas", "AK"),
            ("Gill", "D"),
            ("Bellamy", "B"),
            ("Navarro", "M"),
            ("Iqbal", "A"),
        ],
        venue: "Fict Cell",
        year: 2026,
        doi: None,
        arxiv: None,
        folders: &["Active_Screens", "Single Cell Data"],
        labels: &["to-read"],
        starred: false,
        summary: "A model of how cells respond to genetic and chemical perturbations, \
                  across cell contexts.",
    },
    Paper {
        key: "Rourke2026-uc",
        kind: "article-journal",
        title: "A universal cell embedding as a foundation model for tissue biology",
        authors: &[
            ("Rourke", "Y"),
            ("Rasmussen", "Y"),
            ("Aguilar", "A"),
            ("Šimunović", "L"),
            ("Imagined Atlas Consortium", ""),
            ("Quinlan", "SR"),
            ("Lindgren", "J"),
        ],
        venue: "Journal of Imagined Biology",
        year: 2026,
        doi: None,
        arxiv: None,
        folders: &["Single Cell Data", "grant_demo"],
        labels: &[],
        starred: false,
        summary: "One embedding of single cells across tissues and species.",
    },
    Paper {
        key: "Brandt-Okafor2025-pb",
        kind: "article-journal",
        title: "Neural gene perturbation effect models are matched by simple linear baselines",
        authors: &[
            ("Brandt-Okafor", "C"),
            ("Halvorsen", "W"),
            ("Andersen", "S"),
        ],
        venue: "Nat Fict Methods",
        year: 2025,
        doi: None,
        arxiv: None,
        folders: &["grant_demo", "Active_Screens"],
        labels: &["benchmark"],
        starred: false,
        summary: "Linear baselines match deep models at predicting the effects of \
                  gene perturbations.",
    },
    Paper {
        key: "Kowalczyk2025-zs",
        kind: "article-journal",
        title: "Zero-shot evaluation shows the limits of tissue foundation models",
        authors: &[
            ("Kowalczyk", "KZ"),
            ("Crane", "L"),
            ("Amari", "AP"),
            ("Lowe", "AX"),
        ],
        venue: "Fict Genome Biol",
        year: 2025,
        doi: None,
        arxiv: None,
        folders: &["grant_demo"],
        labels: &["benchmark"],
        starred: false,
        summary: "Single-cell foundation models, tested without fine-tuning.",
    },
    Paper {
        key: "SCM-team2024-lc",
        kind: "manuscript",
        title: "Sentence concept models: Language modeling in an embedding space",
        authors: &[
            ("SCM team", ""),
            ("Barlow", "L"),
            ("Dufresne", "PA"),
            ("Elkins", "M"),
            ("Kovacs", "A"),
        ],
        venue: "arXiv [cs.CL]",
        year: 2024,
        doi: None,
        arxiv: Some("2412.99901"),
        folders: &["grant_demo"],
        labels: &["to-read"],
        starred: false,
        summary: "Language modelling over sentence embeddings rather than tokens.",
    },
];

/// The demo papers, the first of them added last.
pub fn records() -> Vec<Record> {
    PAPERS
        .iter()
        .enumerate()
        .map(|(i, p)| {
            let authors: Vec<Value> = p
                .authors
                .iter()
                .map(|(family, given)| {
                    if given.is_empty() {
                        json!({ "literal": family })
                    } else {
                        json!({ "family": family, "given": given })
                    }
                })
                .collect();
            let mut csl = json!({
                "type": p.kind,
                "title": p.title,
                "author": authors,
                "container-title": p.venue,
                "issued": { "date-parts": [[p.year]] },
                "abstract": p.summary,
            });
            if let Some(doi) = p.doi {
                csl["DOI"] = json!(doi);
            }
            Record {
                key: p.key.to_string(),
                csl,
                paperstack: Meta {
                    added: format!("2026-10-{:02}T09:00:00-07:00", 19 - i),
                    folders: p.folders.iter().map(|s| s.to_string()).collect(),
                    labels: p.labels.iter().map(|s| s.to_string()).collect(),
                    starred: p.starred,
                    arxiv: p.arxiv.map(str::to_string),
                    source: Some("demo".to_string()),
                    ..Meta::default()
                },
            }
        })
        .collect()
}
