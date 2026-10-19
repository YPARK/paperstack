//! Papers for `paperstack init --demo`, to try the library with: their
//! keys as Paperpile gave them, identifiers only where they are known, and
//! for an abstract a line written for the demo, not the paper's own.

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
        key: "VanderWeele2017-ex",
        kind: "article-journal",
        title: "Sensitivity analysis in observational research: Introducing the E-value",
        authors: &[("VanderWeele", "TJ"), ("Ding", "P")],
        venue: "Ann Intern Med",
        year: 2017,
        doi: Some("10.7326/M16-2607"),
        arxiv: None,
        folders: &["mediation", "study design"],
        labels: &["method"],
        starred: true,
        summary: "The E-value: how strongly an unmeasured confounder would have to be \
                  associated with treatment and outcome to explain away an association.",
    },
    Paper {
        key: "Carter2021-mr",
        kind: "article-journal",
        title: "Mendelian randomisation for mediation analysis: current methods and \
                challenges for implementation",
        authors: &[
            ("Carter", "AR"),
            ("Sanderson", "E"),
            ("Hammerton", "G"),
            ("Richmond", "RC"),
            ("Davey Smith", "G"),
            ("Heron", "J"),
            ("Taylor", "AE"),
            ("Davies", "NM"),
            ("Howe", "LD"),
        ],
        venue: "Eur J Epidemiol",
        year: 2021,
        doi: Some("10.1007/s10654-021-00757-1"),
        arxiv: None,
        folders: &["mediation", "Pleiotropy"],
        labels: &[],
        starred: false,
        summary: "A review of Mendelian randomisation methods for mediation analysis.",
    },
    Paper {
        key: "Naik2016-al",
        kind: "article-journal",
        title: "Active machine learning-driven experimentation to determine compound \
                effects on protein patterns",
        authors: &[
            ("Naik", "AW"),
            ("Kangas", "JD"),
            ("Sullivan", "DP"),
            ("Murphy", "RF"),
        ],
        venue: "Elife",
        year: 2016,
        doi: Some("10.7554/eLife.10047"),
        arxiv: None,
        folders: &["CRISPR_Active_Learning"],
        labels: &["method"],
        starred: false,
        summary: "Active learning picks which compound and target experiments to run next.",
    },
    Paper {
        key: "Levine2024-cs",
        kind: "paper-conference",
        title: "Cell2Sentence: Teaching Large Language Models the Language of Biology",
        authors: &[
            ("Levine", "D"),
            ("Rizvi", "SA"),
            ("Lévy", "S"),
            ("Pallikkavaliyaveetil", "N"),
            ("Zhang", "D"),
        ],
        venue: "International Conference on Machine Learning",
        year: 2024,
        doi: None,
        arxiv: None,
        folders: &["Single Cell Data"],
        labels: &["to-read"],
        starred: false,
        summary: "Single-cell expression written as ranked gene names, for a language model.",
    },
    Paper {
        key: "Adduri2026-st",
        kind: "article-journal",
        title: "Predicting cellular responses to perturbation across diverse contexts with State",
        authors: &[
            ("Adduri", "AK"),
            ("Gautam", "D"),
            ("Bevilacqua", "B"),
            ("Naghipourfar", "M"),
            ("Imran", "A"),
        ],
        venue: "Cell",
        year: 2026,
        doi: None,
        arxiv: None,
        folders: &["CRISPR_Active_Learning", "Single Cell Data"],
        labels: &["to-read"],
        starred: false,
        summary: "A model of how cells respond to genetic and chemical perturbations, \
                  across cell contexts.",
    },
    Paper {
        key: "Rosen2026-uc",
        kind: "article-journal",
        title: "Universal cell embedding provides a foundation model for cell biology",
        authors: &[
            ("Rosen", "Y"),
            ("Roohani", "Y"),
            ("Agrawal", "A"),
            ("Samotorčan", "L"),
            ("Tabula Sapiens Consortium", ""),
            ("Quake", "SR"),
            ("Leskovec", "J"),
        ],
        venue: "Nature",
        year: 2026,
        doi: None,
        arxiv: None,
        folders: &["Single Cell Data", "grant_OCRA_AI"],
        labels: &[],
        starred: false,
        summary: "One embedding of single cells across tissues and species.",
    },
    Paper {
        key: "Ahlmann-Eltze2025-pb",
        kind: "article-journal",
        title: "Deep-learning-based gene perturbation effect prediction does not yet \
                outperform simple linear baselines",
        authors: &[("Ahlmann-Eltze", "C"), ("Huber", "W"), ("Anders", "S")],
        venue: "Nat Methods",
        year: 2025,
        doi: None,
        arxiv: None,
        folders: &["grant_OCRA_AI", "CRISPR_Active_Learning"],
        labels: &["benchmark"],
        starred: false,
        summary: "Linear baselines match deep models at predicting the effects of \
                  gene perturbations.",
    },
    Paper {
        key: "Kedzierska2025-zs",
        kind: "article-journal",
        title: "Zero-shot evaluation reveals limitations of single-cell foundation models",
        authors: &[
            ("Kedzierska", "KZ"),
            ("Crawford", "L"),
            ("Amini", "AP"),
            ("Lu", "AX"),
        ],
        venue: "Genome Biol",
        year: 2025,
        doi: None,
        arxiv: None,
        folders: &["grant_OCRA_AI"],
        labels: &["benchmark"],
        starred: false,
        summary: "Single-cell foundation models, tested without fine-tuning.",
    },
    Paper {
        key: "LCM-team2024-lc",
        kind: "manuscript",
        title: "Large concept models: Language modeling in a sentence representation space",
        authors: &[
            ("LCM team", ""),
            ("Barrault", "L"),
            ("Duquenne", "PA"),
            ("Elbayad", "M"),
            ("Kozhevnikov", "A"),
        ],
        venue: "arXiv [cs.CL]",
        year: 2024,
        doi: None,
        arxiv: Some("2412.08821"),
        folders: &["grant_OCRA_AI"],
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
