use crate::app::{App, Popup};
use crate::index::{Index, Sort};
use crate::library::Library;
use crate::query::{Field, Query};
use crate::record::new_key;
use crate::source::Source;
use crate::{bibtex, demo, layout};
use std::path::PathBuf;

/// The demo library in a folder of its own, and its index beside it.
fn library() -> (tempfile::TempDir, Library, Index) {
    let dir = tempfile::tempdir().unwrap();
    let lib = Library::init(&dir.path().join("lib")).unwrap();
    for r in demo::records() {
        lib.save(&r, None).unwrap();
    }
    let mut index = Index::open(&dir.path().join("index.json")).unwrap();
    let done = index.refresh(&lib).unwrap();
    assert!(done.errors.is_empty(), "{:?}", done.errors);
    (dir, lib, index)
}

fn keys(index: &Index, q: &str, sort: Sort) -> Vec<String> {
    index
        .search(&Query::parse(q), sort, false, None)
        .unwrap()
        .into_iter()
        .map(|h| h.record.key.clone())
        .collect()
}

fn screen(app: &mut App, width: u16, height: u16) -> String {
    crate::app::screen(app, width, height)
}

#[test]
fn files_papers_by_year_and_first_author() {
    let records = demo::records();
    let find = |key| records.iter().find(|r| r.key == key).unwrap();
    assert_eq!(
        layout::pdf(find("Ahlmann-Eltze2025-pb")),
        PathBuf::from(
            "2025/Ahlmann-Eltze/C/Ahlmann-Eltze2025-pb_deep-learning-based-gene-perturbation-effect.pdf"
        )
    );
    assert!(layout::record(find("LCM-team2024-lc")).starts_with("2024/LCM team/_/"));
    assert_eq!(layout::slug("Lévy flights: A/B? c"), "levy-flights-a-b-c");
}

#[test]
fn reads_only_what_changed() {
    let (_dir, lib, mut index) = library();
    assert_eq!(index.shelves().unwrap().all, 9);
    let again = index.refresh(&lib).unwrap();
    assert_eq!((again.read, again.removed), (0, 0));

    let hit = index.get("Naik2016-al").unwrap().unwrap();
    std::fs::remove_file(lib.root.join(&hit.path)).unwrap();
    let gone = index.refresh(&lib).unwrap();
    assert_eq!((gone.read, gone.removed), (0, 1));
    assert!(index.get("Naik2016-al").unwrap().is_none());
}

#[test]
fn sees_a_pdf_put_beside_its_record() {
    let (_dir, lib, mut index) = library();
    assert!(keys(&index, "has:pdf", Sort::Added).is_empty());
    let hit = index.get("Kedzierska2025-zs").unwrap().unwrap();
    std::fs::write(lib.root.join(&hit.path).with_extension("pdf"), b"%PDF-1.4").unwrap();
    index.refresh(&lib).unwrap();
    assert_eq!(keys(&index, "has:pdf", Sort::Added), ["Kedzierska2025-zs"]);
}

#[test]
fn searches_words_fields_and_abstracts() {
    let (_dir, _lib, index) = library();
    assert_eq!(
        keys(&index, "linear", Sort::Relevance),
        ["Ahlmann-Eltze2025-pb"]
    );
    assert_eq!(
        keys(&index, "author:Huber", Sort::Relevance),
        ["Ahlmann-Eltze2025-pb"]
    );
    // Only in its abstract.
    assert_eq!(keys(&index, "tokens", Sort::Relevance), ["LCM-team2024-lc"]);
    // A word matches the words it starts, without accents.
    assert_eq!(keys(&index, "perturb", Sort::Relevance).len(), 2);
    assert_eq!(keys(&index, "levy", Sort::Relevance), ["Levine2024-cs"]);
    assert_eq!(
        keys(&index, "\"Cell2Sentence\"", Sort::Relevance),
        ["Levine2024-cs"]
    );
    assert_eq!(
        keys(&index, "10.7554/eLife.10047", Sort::Relevance),
        ["Naik2016-al"]
    );
    assert_eq!(keys(&index, "year:2025..2026", Sort::Relevance).len(), 4);
    assert_eq!(
        keys(&index, "folder:\"single cell data\"", Sort::Relevance).len(),
        3
    );
    assert_eq!(keys(&index, "-label:benchmark", Sort::Relevance).len(), 7);
    assert_eq!(
        keys(&index, "is:starred", Sort::Relevance),
        ["VanderWeele2017-ex"]
    );
    assert_eq!(
        keys(&index, "cell -author:Rosen year:2026", Sort::Relevance),
        ["Adduri2026-st"]
    );
    assert_eq!(
        keys(&index, "doi:10.7326/M16-2607", Sort::Relevance),
        ["VanderWeele2017-ex"]
    );
    assert!(keys(&index, "-", Sort::Relevance).len() == 9);
}

#[test]
fn sorts() {
    let (_dir, _lib, index) = library();
    let by_year = keys(&index, "", Sort::Year);
    assert!(by_year[0].contains("2026") && by_year[8] == "Naik2016-al");
    assert_eq!(keys(&index, "", Sort::Added)[0], "VanderWeele2017-ex");
    assert_eq!(keys(&index, "", Sort::Author)[0], "Adduri2026-st");
    let oldest = index
        .search(&Query::default(), Sort::Year, true, Some(1))
        .unwrap();
    assert_eq!(oldest[0].record.key, "Naik2016-al");
}

#[test]
fn parses_queries() {
    let q = Query::parse(r#"-folder:"Single Cell Data" au:Huber "a b" -x"#);
    assert_eq!(q.terms.len(), 4);
    assert!(q.terms[0].negated && q.terms[0].field == Field::Folder);
    assert_eq!(q.terms[0].value, "Single Cell Data");
    assert_eq!(q.terms[1].field, Field::Author);
    assert!(q.terms[2].phrase && q.terms[2].field == Field::Any);
    assert!(q.terms[3].negated && q.terms[3].value == "x");
    // Not a field: a word with a colon in it.
    assert_eq!(Query::parse("arXiv:2412.08821").terms[0].field, Field::Any);
}

#[test]
fn writes_bibtex_as_paperpile_does() {
    let records = demo::records();
    let find = |key| records.iter().find(|r| r.key == key).unwrap();
    let bib = bibtex::entry(find("VanderWeele2017-ex"));
    assert!(bib.starts_with("@article{VanderWeele2017-ex,\n"), "{bib}");
    for field in [
        "author = {VanderWeele, TJ and Ding, P}",
        "journal = {Ann Intern Med}",
        "year = {2017}",
        "doi = {10.7326/M16-2607}",
    ] {
        assert!(bib.contains(field), "{field} not in\n{bib}");
    }
    let bib = bibtex::entry(find("Rosen2026-uc"));
    assert!(bib.contains("{Tabula Sapiens Consortium}"), "{bib}");
    let bib = bibtex::entry(find("LCM-team2024-lc"));
    assert!(bib.contains("eprint = {2412.08821}"), "{bib}");
}

#[test]
fn reads_crossrefs_types_as_csls() {
    let mut r = demo::records().remove(0);
    r.csl["type"] = serde_json::json!("journal-article");
    assert_eq!(r.kind(), "Journal article");
    let bib = bibtex::entry(&r);
    assert!(
        bib.starts_with("@article{") && bib.contains("journal = {Ann Intern Med}"),
        "{bib}"
    );
    r.csl["type"] = serde_json::json!("posted-content");
    assert_eq!(r.kind(), "Preprint");
    let t = crate::resolve::tidy(serde_json::json!({ "type": "proceedings-article" }));
    assert_eq!(t["type"], "paper-conference");
}

#[test]
fn makes_keys_in_paperpiles_form() {
    let mut r = demo::records().remove(6);
    r.key.clear();
    let a = new_key(&r, "10.1038/s41592-025-02772-6", |_| false);
    assert!(a.starts_with("Ahlmann-Eltze2025-") && a.len() == "Ahlmann-Eltze2025-xy".len());
    assert_eq!(a, new_key(&r, "10.1038/S41592-025-02772-6", |_| false));
    let b = new_key(&r, "10.1038/s41592-025-02772-6", |k| k == a);
    assert_ne!(a, b);
}

#[test]
fn stars_in_the_record() {
    let (_dir, lib, index) = library();
    let mut app = App::new(lib, index).unwrap();
    let key = app.current().unwrap().record.key.clone();
    let starred = app.current().unwrap().record.paperstack.starred;
    app.star();
    let saved = app.index.get(&key).unwrap().unwrap();
    assert_eq!(saved.record.paperstack.starred, !starred);
    let file = crate::library::read(&app.lib.root.join(&saved.path)).unwrap();
    assert_eq!(file.paperstack.starred, !starred);
}

#[test]
fn draws_the_library() {
    let (_dir, lib, index) = library();
    let mut app = App::new(lib, index).unwrap();
    let s = screen(&mut app, 160, 44);
    println!("{s}");
    assert!(s.contains("Sensitivity analysis"));
    assert!(s.contains("Single Cell Data"));
    assert!(s.contains("2017/VanderWeele/TJ/"));
}

#[test]
fn draws_add() {
    let (_dir, lib, index) = library();
    let mut app = App::new(lib, index).unwrap();
    app.popup = Popup::Add("https://arxiv.org/abs/2412.08821".to_string());
    let s = screen(&mut app, 160, 44);
    assert!(s.contains("arXiv 2412.08821"));
}

#[test]
fn reads_sources() {
    assert!(
        matches!(Source::of("https://doi.org/10.7554/eLife.10047"), Source::Doi(d) if d == "10.7554/eLife.10047")
    );
    assert!(matches!(Source::of("2412.08821v2"), Source::Arxiv(_)));
    assert!(matches!(Source::of("PMID: 28693043"), Source::Pmid(_)));
    assert!(matches!(Source::of("~/Downloads/x.pdf"), Source::Pdf(_)));
    assert!(matches!(Source::of("@article{x,"), Source::Bibtex));
    assert!(matches!(Source::of("linear baselines"), Source::Words(_)));
}

#[test]
fn finds_titles_on_a_page() {
    // deunicode once ran "33" into "Purine", and "pathogens" into the next line.
    let page = "eLife 2023;12:RP91040.   1 of 33\n\nPurine nucleosides replace cAMP \nin \
                allosteric regulation of PKA in \ntrypanosomatid\u{a0}pathogens\n\nVeronica Ober";
    let title = "Purine nucleosides replace cAMP in allosteric regulation of PKA in trypanosomatid pathogens";
    assert!(crate::pdf::has_title(title, page));
    // Hyphenated across a line.
    assert!(crate::pdf::has_title(
        "Quantifying organismal complexity",
        "Quan-\ntifying organismal complexity using"
    ));
    // Words run together in the registry's title.
    assert!(crate::pdf::has_title(
        "StatisticallyCalibrated Tokenization forGenomic Models",
        "Statistically Calibrated Tokenization\nfor Genomic Models"
    ));
    assert!(!crate::pdf::has_title(
        title,
        "Purine nucleosides in PKA signalling"
    ));
    let csl = serde_json::json!({ "author": [{ "family": "Ober", "given": "V" }] });
    assert!(crate::pdf::has_author(&csl, page));
    assert!(!crate::pdf::has_author(&csl, "San Juan, Puerto Rico"));
}

#[test]
fn reads_ids_in_publishers_addresses() {
    use Source::*;
    let cases = [
        (
            "https://onlinelibrary.wiley.com/doi/full/10.1002/sim.7400",
            Doi("10.1002/sim.7400".into()),
        ),
        (
            "https://link.springer.com/article/10.1007/s10654-021-00757-1",
            Doi("10.1007/s10654-021-00757-1".into()),
        ),
        (
            "https://www.biorxiv.org/content/10.1101/2023.09.11.557287v1.full.pdf",
            Doi("10.1101/2023.09.11.557287".into()),
        ),
        (
            "https://www.nature.com/articles/s41586-020-2649-2.pdf",
            Doi("10.1038/s41586-020-2649-2".into()),
        ),
        (
            "https://journals.plos.org/plosone/article?id=10.1371/journal.pone.0000217",
            Doi("10.1371/journal.pone.0000217".into()),
        ),
        (
            "https://www.sciencedirect.com/science/article/pii/S0092867423013442",
            Pii("S0092867423013442".into()),
        ),
        (
            "https://www.cell.com/cell/fulltext/S0092-8674(23)01344-2",
            Pii("S0092867423013442".into()),
        ),
        (
            "https://www.cell.com/cell-genomics/pdf/S2666-979X(26)00051-0.pdf",
            Pii("S2666979X26000510".into()),
        ),
        (
            "https://www.cell.com/action/showPdf?pii=S1097-2765(24)00526-4",
            Pii("S1097276524005264".into()),
        ),
        (
            "https://linkinghub.elsevier.com/retrieve/pii/S2211124726004389",
            Pii("S2211124726004389".into()),
        ),
        (
            "https://arxiv.org/pdf/2412.08821v2",
            Arxiv("2412.08821".into()),
        ),
        (
            "https://pubmed.ncbi.nlm.nih.gov/28693043/",
            Pmid("28693043".into()),
        ),
        (
            "https://pmc.ncbi.nlm.nih.gov/articles/PMC5629050/",
            Pmcid("PMC5629050".into()),
        ),
        (
            "https://academic.oup.com/bioinformatics/article/34/17/i884/5093234",
            Url("https://academic.oup.com/bioinformatics/article/34/17/i884/5093234".into()),
        ),
        (
            "doi:10.7554/eLife.10047.",
            Doi("10.7554/eLife.10047".into()),
        ),
        ("arXiv:2412.08821v1", Arxiv("2412.08821".into())),
        ("PMC5629050", Pmcid("PMC5629050".into())),
    ];
    for (given, want) in cases {
        assert_eq!(Source::of(given), want, "{given}");
    }
    assert_eq!(
        crate::source::dois("see https://doi.org/10.1038/nature12373), and 10.1038/NATURE12373."),
        ["10.1038/nature12373"]
    );
    assert_eq!(
        crate::source::arxiv_ids("arXiv:2412.08821v1 [cs.CL] 11 Dec 2024"),
        ["2412.08821"]
    );
    assert_eq!(
        crate::source::unversioned("solv-int/9901001v2"),
        "solv-int/9901001"
    );
    assert_eq!(crate::source::unversioned("2412.08821"), "2412.08821");
    assert_eq!(
        Source::of("https://arxiv.org/abs/solv-int/9901001v1"),
        Source::Arxiv("solv-int/9901001".into())
    );
}

#[test]
fn reads_a_pages_meta_tags() {
    let html = r#"<head><meta name="citation_title" content="A &amp; B">
        <meta content="10.1038/s41586-020-2649-2" name="citation_doi">
        <meta name="citation_pdf_url" content="https://x.org/a.pdf"/></head>"#;
    assert_eq!(
        crate::resolve::from_metas(&crate::resolve::metas(html)),
        Some(Source::Doi("10.1038/s41586-020-2649-2".into()))
    );
    let metas = crate::resolve::metas(html);
    assert!(metas.contains(&("citation_title".into(), "A & B".into())));
}

#[test]
fn tidies_what_registries_give() {
    let csl = serde_json::json!({
        "id": "x", "reference": [1, 2], "title": ["A <i>B</i>\n c"],
        "container-title": ["Nature"], "abstract": "<jats:title>Abstract</jats:title><jats:p>It works.</jats:p>",
        "author": [{ "family": "Ng", "given": "A", "sequence": "first", "affiliation": [] }],
    });
    let t = crate::resolve::tidy(csl);
    assert_eq!(t["title"], "A B c");
    assert_eq!(t["container-title"], "Nature");
    assert_eq!(t["abstract"], "It works.");
    assert!(t.get("reference").is_none() && t.get("id").is_none());
    assert!(t["author"][0].get("sequence").is_none());
    let index = serde_json::json!({ "works": [1], "It": [0], "well": [2] });
    assert_eq!(crate::resolve::uninvert(&index).unwrap(), "It works well");
}

#[test]
fn writes_initials_as_paperpile_does() {
    let mut r = demo::records().remove(0);
    r.csl["author"] = serde_json::json!([{ "family": "Abbott", "given": "B. P." }]);
    assert!(layout::record(&r).starts_with("2017/Abbott/BP/"));
    r.csl["author"] = serde_json::json!([{ "family": "Naik", "given": "Armaghan W." }]);
    assert!(layout::record(&r).starts_with("2017/Naik/Armaghan W/"));
}

#[test]
fn knows_a_paper_already_here() {
    let (_dir, _lib, index) = library();
    assert_eq!(
        index
            .by_doi("10.7326/m16-2607")
            .unwrap()
            .unwrap()
            .record
            .key,
        "VanderWeele2017-ex"
    );
    assert_eq!(
        index.by_arxiv("2412.08821").unwrap().unwrap().record.key,
        "LCM-team2024-lc"
    );
    let t = "Sensitivity analysis in observational research: introducing the E-value";
    assert!(index.by_title(t, Some(2018)).unwrap().is_some());
    assert!(index.by_title(t, Some(2020)).unwrap().is_none());
}

#[test]
fn reads_a_paper_from_a_pages_tags() {
    // NeurIPS's pages before 2022: Highwire's tags, no DOI.
    let html = r#"<meta name="citation_title" content="Attention is All you Need">
        <meta name="citation_author" content="Vaswani, Ashish">
        <meta name="citation_author" content="Shazeer, Noam">
        <meta name="citation_journal_title" content="Advances in Neural Information Processing Systems">
        <meta name="citation_volume" content="30">
        <meta name="citation_firstpage" content="5998"><meta name="citation_lastpage" content="6008">
        <meta name="citation_publication_date" content="2017">"#;
    assert_eq!(
        crate::resolve::from_metas(&crate::resolve::metas(html)),
        None
    );
    let found = crate::resolve::from_tags(&crate::resolve::metas(html), "https://papers.nips.cc/x")
        .unwrap();
    assert_eq!(found.csl["type"], "paper-conference");
    assert_eq!(found.csl["author"][0]["family"], "Vaswani");
    assert_eq!(found.csl["issued"]["date-parts"][0][0], 2017);
    assert_eq!(found.csl["page"], "5998-6008");
    let r = crate::record::Record {
        key: "Vaswani2017-x".into(),
        csl: found.csl,
        paperstack: Default::default(),
    };
    let bib = bibtex::entry(&r);
    assert!(
        bib.starts_with("@inproceedings{") && bib.contains("booktitle = {Advances in Neural"),
        "{bib}"
    );
}

#[test]
fn reads_pubmeds_abstract() {
    let xml = r#"<Abstract><AbstractText Label="BACKGROUND" NlmCategory="BACKGROUND">Deep <i>learning</i> &amp; more.</AbstractText>
        <AbstractText Label="RESULTS">It works.</AbstractText></Abstract>"#;
    assert_eq!(
        crate::resolve::pubmed_abstract_of(xml).unwrap(),
        "BACKGROUND: Deep learning & more. RESULTS: It works."
    );
    assert_eq!(
        crate::resolve::pubmed_abstract_of("<AbstractText>One part.</AbstractText>").unwrap(),
        "One part."
    );
}

/// How long a search, as one key typed in the TUI makes, takes among 6,000
/// papers: `cargo test --release -- --ignored --nocapture searches_6000`.
#[test]
#[ignore]
fn searches_6000_quickly() {
    let dir = tempfile::tempdir().unwrap();
    let lib = Library::init(&dir.path().join("lib")).unwrap();
    let demo = demo::records();
    for i in 0..6000 {
        let mut r = demo[i % demo.len()].clone();
        r.key = format!("Author{}{}-{i:04}", i % 900, 1990 + i % 36);
        r.csl["author"][0] =
            serde_json::json!({ "family": format!("Author{}", i % 900), "given": "A" });
        r.csl["issued"] = serde_json::json!({ "date-parts": [[1990 + i % 36]] });
        lib.save(&r, None).unwrap();
    }
    let mut index = Index::open(&dir.path().join("index.json")).unwrap();
    index.refresh(&lib).unwrap();
    for q in [
        "",
        "p",
        "pert",
        "perturbation linear",
        "author:Huber year:2020..",
        "\"single cell\" -label:benchmark",
    ] {
        let t = std::time::Instant::now();
        let n = index
            .search(&Query::parse(q), Sort::Relevance, false, None)
            .unwrap()
            .len();
        println!(
            "{:>8.2} ms  {n:>5} found  {q:?}",
            t.elapsed().as_secs_f64() * 1e3
        );
    }
}
