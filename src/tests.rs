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
fn writes_plain_bibtex() {
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
fn makes_keys_as_surname_year() {
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

/// A left click where any of `what` is drawn the `nth` time, on the
/// sidebar or right of it.
fn click(app: &mut App, what: &str, nth: usize, side: bool) -> Option<crate::app::Action> {
    use ratatui::crossterm::event::{KeyModifiers, MouseButton, MouseEvent, MouseEventKind};
    let s = screen(app, 160, 44);
    let (row, column) = s
        .lines()
        .enumerate()
        .flat_map(|(y, l)| {
            l.chars()
                .enumerate()
                .filter(|&(x, c)| what.contains(c) && (x < 30) == side)
                .map(move |(x, _)| (y as u16, x as u16))
        })
        .nth(nth)
        .unwrap_or_else(|| panic!("no {what} drawn\n{s}"));
    app.mouse(MouseEvent {
        kind: MouseEventKind::Down(MouseButton::Left),
        column,
        row,
        modifiers: KeyModifiers::NONE,
    })
}

#[test]
fn clicks() {
    use ratatui::crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
    let (_dir, lib, index) = library();
    let mut app = App::new(lib, index).unwrap();
    let second = app.shown[1].record.key.clone();
    click(&mut app, "☐☑", 1, false);
    assert!(app.marked.contains(&second));
    assert_eq!(app.list.selected(), Some(0));
    let starred = app.shown[1].record.paperstack.starred;
    click(&mut app, "★☆", 1, false);
    assert_eq!(app.list.selected(), Some(1));
    assert_eq!(app.current().unwrap().record.paperstack.starred, !starred);

    app.marked.clear();
    app.key(KeyEvent::new(KeyCode::Char('e'), KeyModifiers::CONTROL));
    assert!(matches!(&app.popup, Popup::Cite(text, _) if text.contains(&second)));
    click(&mut app, "▤", 0, true);
    assert!(matches!(app.popup, Popup::None));

    click(&mut app, "∅", 0, true);
    assert_eq!(app.shelves[app.shelf].0.name(), "No PDF");
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
fn writes_initials_run_together() {
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

#[test]
fn reads_text_whose_letters_change_length_lowercase() {
    // The Kelvin sign is three bytes, its lowercase `k` one: a place found
    // in the one once cut the other inside a letter.
    assert_eq!(
        crate::source::arxiv_ids("\u{212A} arXiv:2412.08821v1 [cs.CL]"),
        ["2412.08821"]
    );
    assert_eq!(
        Source::of("https://www.Nature.com/articles/s41586-020-2649-2"),
        Source::Doi("10.1038/s41586-020-2649-2".into())
    );
    assert!(matches!(Source::of("https://ÄÄÄ.de/x"), Source::Url(_)));
    // An escape at the very end is read, and one that is none is kept.
    assert_eq!(crate::source::clean_doi("10.1000/ab%41"), "10.1000/abA");
    assert_eq!(crate::source::clean_doi("10.1000/a%zz"), "10.1000/a%zz");
}

#[test]
fn reads_years_written_with_a_hyphen() {
    assert_eq!(crate::query::years("2020-2024"), Some((2020, 2024)));
    let (_dir, _lib, index) = library();
    assert_eq!(keys(&index, "year:2025-2026", Sort::Relevance).len(), 4);
}

#[test]
fn keeps_a_key_for_its_paper_over_anothers_alias() {
    let (_dir, lib, mut index) = library();
    let mut r = demo::records().remove(1);
    r.paperstack.aliases = vec!["VanderWeele2017-ex".into()];
    lib.save(&r, None).unwrap();
    let done = index.refresh(&lib).unwrap();
    assert!(
        done.errors.iter().any(|e| e.contains("alias")),
        "{:?}",
        done.errors
    );
    assert_eq!(
        index
            .get("VanderWeele2017-ex")
            .unwrap()
            .unwrap()
            .record
            .title(),
        "Sensitivity analysis in observational research: Introducing the E-value"
    );
}

#[test]
fn moves_no_pdf_over_anothers() {
    let (_dir, lib, index) = library();
    let hit = index.get("Naik2016-al").unwrap().unwrap();
    std::fs::write(lib.root.join(&hit.path).with_extension("pdf"), b"%PDF-mine").unwrap();
    let mut moved = crate::record::Record::clone(&hit.record);
    moved.csl["issued"] = serde_json::json!({ "date-parts": [[2017]] });
    let target = lib.root.join(layout::pdf(&moved));
    std::fs::create_dir_all(target.parent().unwrap()).unwrap();
    std::fs::write(&target, b"%PDF-another's").unwrap();
    assert!(lib.save(&moved, Some(&hit.path)).is_err());
    assert_eq!(std::fs::read(&target).unwrap(), b"%PDF-another's");
    assert!(lib.root.join(&hit.path).exists(), "left where it was");
}

#[test]
fn extension_id_is_its_manifest_keys() {
    use base64::Engine;
    use sha2::Digest;
    let manifest: serde_json::Value =
        serde_json::from_str(include_str!("../extension/manifest.json")).unwrap();
    let key = base64::engine::general_purpose::STANDARD
        .decode(manifest["key"].as_str().unwrap())
        .unwrap();
    // Chrome's id: the first 32 hex digits of the key's SHA-256, 0-f as a-p.
    let id: String = sha2::Sha256::digest(&key)[..16]
        .iter()
        .flat_map(|b| [b >> 4, b & 15])
        .map(|d| (b'a' + d) as char)
        .collect();
    assert_eq!(id, crate::daemon::EXTENSION);
}

#[test]
fn serves_only_the_extensions_named() {
    use crate::daemon::{EXTENSION, origin, refusal};
    let origins = [origin(EXTENSION), origin("moz-extension://ID1")];
    let ask = |host: &str, from: Option<&str>, length: usize| {
        let mut headers = std::collections::HashMap::new();
        headers.insert("host".to_string(), host.to_string());
        if let Some(o) = from {
            headers.insert("origin".to_string(), o.to_string());
        }
        refusal(&headers, 8765, &origins, length).map(|(code, _)| code)
    };
    let ours = format!("chrome-extension://{EXTENSION}");
    assert_eq!(ask("127.0.0.1:8765", Some(&ours), 10), None);
    assert_eq!(ask("localhost:8765", None, 10), None, "a program here");
    assert_eq!(ask("127.0.0.1:8765", Some("moz-extension://ID1"), 10), None);
    assert_eq!(
        ask("127.0.0.1:8765", Some("chrome-extension://ID2"), 10),
        Some(403)
    );
    assert_eq!(
        ask("127.0.0.1:8765", Some("moz-extension://ID2"), 10),
        Some(403)
    );
    assert_eq!(
        ask("127.0.0.1:8765", Some("https://example.org"), 10),
        Some(403)
    );
    assert_eq!(
        ask("example.org:8765", Some(&ours), 10),
        Some(403),
        "rebound"
    );
    assert_eq!(ask("127.0.0.1:9999", None, 10), Some(403));
    assert_eq!(ask("127.0.0.1:8765", Some(&ours), usize::MAX), Some(413));
}

/// A PDF of one page, its lines set in Helvetica, as `pdf-extract` reads
/// it: to tell papers by, without PDFs kept in the repository.
fn tiny_pdf(lines: &[&str]) -> Vec<u8> {
    let text: String = lines
        .iter()
        .enumerate()
        .map(|(i, l)| {
            format!(
                "BT /F1 11 Tf 72 {} Td ({}) Tj ET\n",
                740 - 16 * i,
                l.replace('(', "\\(").replace(')', "\\)")
            )
        })
        .collect();
    let objects = [
        "<< /Type /Catalog /Pages 2 0 R >>".to_string(),
        "<< /Type /Pages /Kids [3 0 R] /Count 1 >>".to_string(),
        "<< /Type /Page /Parent 2 0 R /MediaBox [0 0 612 792] /Contents 4 0 R /Resources << /Font << /F1 5 0 R >> >> >>".to_string(),
        format!("<< /Length {} >>\nstream\n{text}endstream", text.len()),
        "<< /Type /Font /Subtype /Type1 /BaseFont /Helvetica >>".to_string(),
    ];
    let mut out = b"%PDF-1.4\n".to_vec();
    let mut offsets = Vec::new();
    for (i, o) in objects.iter().enumerate() {
        offsets.push(out.len());
        out.extend(format!("{} 0 obj\n{o}\nendobj\n", i + 1).bytes());
    }
    let xref = out.len();
    out.extend(format!("xref\n0 {}\n0000000000 65535 f \n", objects.len() + 1).bytes());
    for o in offsets {
        out.extend(format!("{o:010} 00000 n \n").bytes());
    }
    out.extend(
        format!(
            "trailer\n<< /Size {} /Root 1 0 R >>\nstartxref\n{xref}\n%%EOF\n",
            objects.len() + 1
        )
        .bytes(),
    );
    out
}

/// The first page of the demo paper `key`'s PDF, or another's.
fn first_page(title: &str) -> Vec<u8> {
    tiny_pdf(&[
        title,
        "A. Author, B. Author",
        "Abstract",
        "We study what the title says, and report what we found, in some detail,",
        "with methods, data and results set out over the pages that follow this one.",
    ])
}

#[test]
fn attaches_only_the_papers_own_pdf() {
    let (_dir, lib, mut index) = library();
    let key = "Kedzierska2025-zs";
    let title = index.get(key).unwrap().unwrap().record.title().to_string();
    let other = crate::add::attach(
        &lib,
        &mut index,
        key,
        &first_page("Another paper altogether, on other things"),
        "x.pdf",
    );
    assert!(other.unwrap_err().to_string().contains("is not"));
    let added = crate::add::attach(&lib, &mut index, key, &first_page(&title), "x.pdf").unwrap();
    assert_eq!(added.outcome, crate::add::Outcome::PdfAttached);
    let hit = index.get(key).unwrap().unwrap();
    assert!(hit.has_pdf && hit.record.paperstack.pdf_sha256.is_some());
    assert!(lib.root.join(&hit.path).with_extension("pdf").exists());
    let again = crate::add::attach(&lib, &mut index, key, &first_page(&title), "x.pdf");
    assert!(again.unwrap_err().to_string().contains("already"));
}

#[test]
fn notices_a_pdf_once_it_is_whole() {
    let dir = tempfile::tempdir().unwrap();
    std::fs::write(dir.path().join("old.pdf"), b"%PDF").unwrap();
    std::thread::sleep(std::time::Duration::from_millis(20));
    let mut arrivals = crate::browser::Arrivals::new(dir.path().to_path_buf());
    std::fs::write(dir.path().join("new.pdf"), b"%PDF-1.4").unwrap();
    std::fs::write(dir.path().join("new.pdf.crdownload"), b"%PDF").unwrap();
    assert!(
        arrivals.next().is_empty(),
        "seen once, not yet known to be whole"
    );
    assert_eq!(arrivals.next(), [dir.path().join("new.pdf")]);
    assert!(arrivals.next().is_empty(), "given once");
}

#[test]
fn files_the_pdf_downloaded_for_the_paper_awaited() {
    let (dir, lib, index) = library();
    let downloads = dir.path().join("Downloads");
    std::fs::create_dir(&downloads).unwrap();
    let mut app = App::new(lib, index).unwrap();
    let key = "Kedzierska2025-zs".to_string();
    let title = app
        .index
        .get(&key)
        .unwrap()
        .unwrap()
        .record
        .title()
        .to_string();
    app.popup = Popup::Fetch(Box::new(crate::app::Fetch {
        key: key.clone(),
        title: title.clone(),
        page: "https://doi.org/x".into(),
        line: String::new(),
        arrivals: crate::browser::Arrivals::new(downloads.clone()),
        note: String::new(),
    }));
    std::fs::write(
        downloads.join("unrelated.pdf"),
        first_page("A bank statement, of no paper"),
    )
    .unwrap();
    std::fs::write(downloads.join("paper.pdf"), first_page(&title)).unwrap();
    app.tick();
    app.tick();
    assert!(
        matches!(app.popup, Popup::None),
        "filed, and the dialog closed"
    );
    assert!(app.index.get(&key).unwrap().unwrap().has_pdf);
    assert!(app.status.contains("given its PDF"), "{}", app.status);
}

/// A library's JSON export, its references with their keys, folders,
/// labels and PDFs' names.
fn library_export() -> serde_json::Value {
    serde_json::json!([
        {
            "_id": "1", "pubtype": "PP_ARTICLE", "citekey": "Gonzalez-Rosa2018-vd",
            "title": "Myocardial polyploidization creates a barrier to heart regeneration in zebrafish",
            "author": [
                { "first": "Juan Manuel", "last": "González-Rosa", "initials": "JM" },
                { "first": "Michka", "last": "Sharpe", "initials": "M" }
            ],
            "published": { "year": "2018", "month": "2", "day": "26" },
            "journal": "Dev. Cell", "journalfull": "Developmental Cell",
            "volume": "44", "issue": "4", "pages": "433-446.e7",
            "doi": "10.1016/j.devcel.2018.01.021", "pmid": "29486195", "pmc": "PMC5830170",
            "abstract": "Polyploid hearts regenerate less.",
            "foldersNamed": ["Research/Heart"], "labelsNamed": ["to-read"], "star": 1,
            "attachments": [{
                "article_pdf": 1,
                "filename": "All Papers/G/González-Rosa et al. 2018 - Myocardial polyploidization creates a barrier to heart regeneration in zebrafish.pdf"
            }],
            "url": ["http://dx.doi.org/10.1016/j.devcel.2018.01.021"]
        },
        {
            "_id": "2", "pubtype": "PP_CONFERENCE_PAPER", "citekey": "Vaswani2017-pp",
            "title": "Attention is all you need",
            "author": [{ "first": "Ashish", "last": "Vaswani" }],
            "published": { "year": "2017" }, "booktitle": "Advances in Neural Information Processing Systems",
            "foldersNamed": ["Research/ML/NLP", "Research/ML/Transformers"], "labelsNamed": ["Machine Learning"]
        },
        {
            "_id": "3", "pubtype": "PP_MISC", "title": "Notes on nothing in particular, unkeyed",
            "author": [{ "first": "A", "last": "Nobody" }], "published": { "year": "2020" }
        },
        { "_id": "4", "pubtype": "PP_ARTICLE", "citekey": "Gone2019-xx", "title": "In the trash", "trashed": 1 },
        {
            "_id": "5", "pubtype": "PP_ARTICLE", "citekey": "VanderWeele2017-pp",
            "title": "Sensitivity analysis in observational research: Introducing the E-value",
            "doi": "10.7326/M16-2607", "foldersNamed": ["grant_NFRF"]
        }
    ])
}

#[test]
fn moves_a_library_from_its_export() {
    let (dir, lib, mut index) = library();
    let export = dir.path().join("export.json");
    std::fs::write(&export, serde_json::to_vec(&library_export()).unwrap()).unwrap();
    // Unzipped from the library's ZIP, its accents stored apart, as a Mac may.
    let pdfs = dir.path().join("Library PDFs");
    let name = "González-Rosa et al. 2018 - Myocardial polyploidization creates a barrier to heart regeneration in zebrafish.pdf";
    let nfd: String = unicode_normalization::UnicodeNormalization::nfd(name).collect();
    std::fs::create_dir_all(pdfs.join("All Papers/G")).unwrap();
    std::fs::write(
        pdfs.join("All Papers/G").join(nfd),
        tiny_pdf(&["Myocardial polyploidization"]),
    )
    .unwrap();

    let dry = crate::migrate::import(&lib, &mut index, &export, Some(&pdfs), true).unwrap();
    assert_eq!((dry.added, dry.merged, dry.trashed, dry.pdfs), (3, 1, 1, 1));
    assert!(
        index.get("Gonzalez-Rosa2018-vd").unwrap().is_none(),
        "a dry run writes nothing"
    );

    let done = crate::migrate::import(&lib, &mut index, &export, Some(&pdfs), false).unwrap();
    assert_eq!(
        (done.read, done.added, done.merged, done.trashed, done.pdfs),
        (5, 3, 1, 1, 1)
    );
    assert!(done.errors.is_empty(), "{:?}", done.errors);

    let hit = index.get("Gonzalez-Rosa2018-vd").unwrap().unwrap();
    let r = &hit.record;
    assert_eq!(r.csl_type(), "article-journal");
    assert_eq!(r.venue(), "Developmental Cell");
    assert_eq!(r.year(), Some(2018));
    assert_eq!(r.csl["PMCID"], "PMC5830170");
    assert_eq!(r.paperstack.folders, ["Research/Heart"]);
    assert!(r.paperstack.starred && hit.has_pdf);
    assert!(hit.path.starts_with("2018/González-Rosa/Juan Manuel/"));
    let bib = bibtex::entry(r);
    assert!(
        bib.starts_with("@article{Gonzalez-Rosa2018-vd,")
            && bib.contains("journal = {Developmental Cell}"),
        "{bib}"
    );

    let ml = index.get("Vaswani2017-pp").unwrap().unwrap();
    assert_eq!(ml.record.csl_type(), "paper-conference");
    assert_eq!(
        keys(&index, "folder:\"Research/ML/NLP\"", Sort::Added),
        ["Vaswani2017-pp"]
    );
    assert!(
        index.get("Gone2019-xx").unwrap().is_none(),
        "the trash is left"
    );
    assert_eq!(
        keys(&index, "unkeyed", Sort::Added).len(),
        1,
        "given a key of its own"
    );

    // Already here, by its DOI: given the folder, its other key an alias.
    let here = index.get("VanderWeele2017-pp").unwrap().unwrap();
    assert_eq!(here.record.key, "VanderWeele2017-ex");
    assert!(
        here.record
            .paperstack
            .folders
            .contains(&"grant_NFRF".to_string())
    );

    // Again: nothing added twice.
    let again = crate::migrate::import(&lib, &mut index, &export, Some(&pdfs), false).unwrap();
    assert_eq!((again.added, again.merged), (0, 4));
}

#[test]
fn serves_the_library_over_mcp() {
    let (_dir, lib, mut index) = library();
    let key = "Kedzierska2025-zs";
    let title = index.get(key).unwrap().unwrap().record.title().to_string();
    crate::add::attach(&lib, &mut index, key, &first_page(&title), "x.pdf").unwrap();
    let mut server = crate::mcp::Server {
        lib,
        index,
        net: crate::net::Net::new(None),
    };
    let mut ask = |id: i64, method: &str, params: serde_json::Value| {
        server
            .answer(&serde_json::json!({ "jsonrpc": "2.0", "id": id, "method": method, "params": params }))
            .unwrap()
    };
    let init = ask(
        1,
        "initialize",
        serde_json::json!({ "protocolVersion": "2025-06-18", "capabilities": {} }),
    );
    assert_eq!(init["result"]["protocolVersion"], "2025-06-18");
    assert_eq!(init["result"]["serverInfo"]["name"], "paperstack");
    let tools = ask(2, "tools/list", serde_json::json!({}));
    let names: Vec<&str> = tools["result"]["tools"]
        .as_array()
        .unwrap()
        .iter()
        .map(|t| t["name"].as_str().unwrap())
        .collect();
    assert!(names.contains(&"search_papers") && names.contains(&"read_paper") && names.len() == 8);

    let call = |server: &mut crate::mcp::Server, name: &str, args: serde_json::Value| {
        server
            .answer(
                &serde_json::json!({ "jsonrpc": "2.0", "id": 9, "method": "tools/call",
                "params": { "name": name, "arguments": args } }),
            )
            .unwrap()["result"]
            .clone()
    };
    let found = call(
        &mut server,
        "search_papers",
        serde_json::json!({ "query": "linear baselines", "limit": 5 }),
    );
    assert_eq!(found["isError"], false);
    assert_eq!(
        found["structuredContent"]["papers"][0]["key"],
        "Ahlmann-Eltze2025-pb"
    );

    let bib = call(
        &mut server,
        "get_paper",
        serde_json::json!({ "key": "VanderWeele2017-ex", "format": "bibtex" }),
    );
    assert!(
        bib["content"][0]["text"]
            .as_str()
            .unwrap()
            .starts_with("@article{VanderWeele2017-ex,")
    );
    let keys = call(
        &mut server,
        "cite",
        serde_json::json!({ "query": "collection:mediation", "format": "keys" }),
    );
    assert!(
        keys["content"][0]["text"]
            .as_str()
            .unwrap()
            .contains("@Carter2021-mr")
    );

    let read = call(&mut server, "read_paper", serde_json::json!({ "key": key }));
    let text = read["content"][0]["text"].as_str().unwrap();
    assert!(
        text.contains("--- page 1 ---") && text.contains("of 1]"),
        "{text}"
    );
    let past = call(
        &mut server,
        "read_paper",
        serde_json::json!({ "key": key, "first_page": 5 }),
    );
    assert_eq!(past["isError"], false, "{past}");
    let none = call(
        &mut server,
        "read_paper",
        serde_json::json!({ "key": "VanderWeele2017-ex" }),
    );
    assert_eq!(none["isError"], true);

    let updated = call(
        &mut server,
        "update_paper",
        serde_json::json!({
        "key": "Naik2016-al", "add_collections": ["grant_NFRF"], "starred": true }),
    );
    assert_eq!(updated["structuredContent"]["starred"], true);
    assert!(
        updated["structuredContent"]["collections"]
            .as_array()
            .unwrap()
            .contains(&serde_json::json!("grant_NFRF"))
    );
    let unknown = call(&mut server, "no_such_tool", serde_json::json!({}));
    assert_eq!(unknown["isError"], true);
    // A notification is answered with nothing.
    assert!(
        server
            .answer(&serde_json::json!({ "jsonrpc": "2.0", "method": "notifications/initialized" }))
            .is_none()
    );
}
