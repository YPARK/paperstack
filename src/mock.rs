//! Papers for the mock-up, as the library would hold them. Identifiers are
//! given only where they are known; the rest are left out.

pub struct Paper {
    /// Paperpile's form, `Surname2023-xy`: the citation key, and the PDF's
    /// name, `pdf/<key>.pdf`.
    pub key: &'static str,
    pub title: &'static str,
    /// Family name, then initials.
    pub authors: &'static [(&'static str, &'static str)],
    /// Authors past those listed.
    pub more: usize,
    pub venue: &'static str,
    pub year: u16,
    pub kind: Kind,
    pub doi: Option<&'static str>,
    pub arxiv: Option<&'static str>,
    pub folders: &'static [&'static str],
    pub labels: &'static [&'static str],
    pub starred: bool,
    pub pdf: bool,
    /// Documents that cite it, as `paperstack bib` finds them.
    pub cited_in: &'static [&'static str],
    pub summary: &'static str,
}

#[derive(Clone, Copy, PartialEq)]
pub enum Kind {
    Article,
    Conference,
    Preprint,
}

impl Kind {
    pub fn name(self) -> &'static str {
        match self {
            Kind::Article => "Journal article",
            Kind::Conference => "Conference paper",
            Kind::Preprint => "Preprint",
        }
    }

    pub fn bibtex(self) -> &'static str {
        match self {
            Kind::Article => "article",
            Kind::Conference => "inproceedings",
            Kind::Preprint => "misc",
        }
    }
}

pub const FOLDERS: &[&str] = &[
    "0_grant_m6a_llsc",
    "AML",
    "Cancer GWAS",
    "CCC",
    "colorectal cancer",
    "CRISPR_Active_Learning",
    "Deconv Paper",
    "DeltaTopic",
    "grant_NFRF",
    "grant_OCRA_AI",
    "gtex",
    "GWAS",
    "heritability",
    "mediation",
    "My Papers",
    "Pleiotropy",
    "Single Cell Data",
    "study design",
];

pub const LABELS: &[&str] = &["to-read", "method", "benchmark"];

pub const PROJECTS: &[&str] = &["grant/OCRA_AI/aims.rmd", "talk/perturb.md"];

pub fn papers() -> Vec<Paper> {
    vec![
        Paper {
            key: "VanderWeele2017-ex",
            title: "Sensitivity analysis in observational research: Introducing the E-value",
            authors: &[("VanderWeele", "TJ"), ("Ding", "P")],
            more: 0,
            venue: "Ann Intern Med",
            year: 2017,
            kind: Kind::Article,
            doi: Some("10.7326/M16-2607"),
            arxiv: None,
            folders: &["mediation", "study design"],
            labels: &["method"],
            starred: true,
            pdf: false,
            cited_in: &[],
            summary: "The E-value: how strongly an unmeasured confounder would have to be \
                      associated with treatment and outcome to explain away an association.",
        },
        Paper {
            key: "Carter2021-mr",
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
            more: 0,
            venue: "Eur J Epidemiol",
            year: 2021,
            kind: Kind::Article,
            doi: Some("10.1007/s10654-021-00757-1"),
            arxiv: None,
            folders: &["mediation", "Pleiotropy"],
            labels: &[],
            starred: false,
            pdf: true,
            cited_in: &[],
            summary: "A review of Mendelian randomisation methods for mediation.",
        },
        Paper {
            key: "Naik2016-al",
            title: "Active machine learning-driven experimentation to determine compound \
                    effects on protein patterns",
            authors: &[
                ("Naik", "AW"),
                ("Kangas", "JD"),
                ("Sullivan", "DP"),
                ("Murphy", "RF"),
            ],
            more: 0,
            venue: "Elife",
            year: 2016,
            kind: Kind::Article,
            doi: Some("10.7554/eLife.10047"),
            arxiv: None,
            folders: &["CRISPR_Active_Learning"],
            labels: &["method"],
            starred: false,
            pdf: true,
            cited_in: &["grant/OCRA_AI/aims.rmd"],
            summary: "Active learning picks which compound-target experiments to run.",
        },
        Paper {
            key: "Levine2024-cs",
            title: "Cell2Sentence: Teaching Large Language Models the Language of Biology",
            authors: &[
                ("Levine", "D"),
                ("Rizvi", "SA"),
                ("Lévy", "S"),
                ("Pallikkavaliyaveetil", "N"),
                ("Zhang", "D"),
            ],
            more: 8,
            venue: "International Conference on Machine Learning",
            year: 2024,
            kind: Kind::Conference,
            doi: None,
            arxiv: None,
            folders: &["Single Cell Data"],
            labels: &["to-read"],
            starred: false,
            pdf: true,
            cited_in: &["talk/perturb.md"],
            summary: "Single-cell expression written as ranked gene names, for a language model.",
        },
        Paper {
            key: "Adduri2026-st",
            title: "Predicting cellular responses to perturbation across diverse contexts \
                    with State",
            authors: &[
                ("Adduri", "AK"),
                ("Gautam", "D"),
                ("Bevilacqua", "B"),
                ("Naghipourfar", "M"),
                ("Imran", "A"),
            ],
            more: 22,
            venue: "Cell",
            year: 2026,
            kind: Kind::Article,
            doi: None,
            arxiv: None,
            folders: &["CRISPR_Active_Learning", "Single Cell Data"],
            labels: &["to-read"],
            starred: false,
            pdf: true,
            cited_in: &["grant/OCRA_AI/aims.rmd", "talk/perturb.md"],
            summary: "A model of how cells respond to perturbations, across cell contexts.",
        },
        Paper {
            key: "Rosen2026-uc",
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
            more: 0,
            venue: "Nature",
            year: 2026,
            kind: Kind::Article,
            doi: None,
            arxiv: None,
            folders: &["Single Cell Data", "grant_OCRA_AI"],
            labels: &[],
            starred: false,
            pdf: true,
            cited_in: &["grant/OCRA_AI/aims.rmd"],
            summary: "One embedding of cells across tissues and species.",
        },
        Paper {
            key: "Ahlmann-Eltze2025-pb",
            title: "Deep-learning-based gene perturbation effect prediction does not yet \
                    outperform simple linear baselines",
            authors: &[("Ahlmann-Eltze", "C"), ("Huber", "W"), ("Anders", "S")],
            more: 0,
            venue: "Nat Methods",
            year: 2025,
            kind: Kind::Article,
            doi: None,
            arxiv: None,
            folders: &["grant_OCRA_AI", "CRISPR_Active_Learning"],
            labels: &["benchmark"],
            starred: false,
            pdf: true,
            cited_in: &["grant/OCRA_AI/aims.rmd", "talk/perturb.md"],
            summary: "Linear baselines match deep models at predicting perturbation effects.",
        },
        Paper {
            key: "Kedzierska2025-zs",
            title: "Zero-shot evaluation reveals limitations of single-cell foundation models",
            authors: &[
                ("Kedzierska", "KZ"),
                ("Crawford", "L"),
                ("Amini", "AP"),
                ("Lu", "AX"),
            ],
            more: 0,
            venue: "Genome Biol",
            year: 2025,
            kind: Kind::Article,
            doi: None,
            arxiv: None,
            folders: &["grant_OCRA_AI"],
            labels: &["benchmark"],
            starred: false,
            pdf: true,
            cited_in: &["grant/OCRA_AI/aims.rmd"],
            summary: "Single-cell foundation models, tested without fine-tuning.",
        },
        Paper {
            key: "LCM-team2024-lc",
            title: "Large concept models: Language modeling in a sentence representation space",
            authors: &[
                ("LCM team", ""),
                ("Barrault", "L"),
                ("Duquenne", "PA"),
                ("Elbayad", "M"),
                ("Kozhevnikov", "A"),
            ],
            more: 11,
            venue: "arXiv [cs.CL]",
            year: 2024,
            kind: Kind::Preprint,
            doi: None,
            arxiv: Some("2412.08821"),
            folders: &["grant_OCRA_AI"],
            labels: &["to-read"],
            starred: false,
            pdf: false,
            cited_in: &[],
            summary: "Language modelling over sentence embeddings rather than tokens.",
        },
    ]
}
