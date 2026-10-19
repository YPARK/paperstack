use crate::app::{App, Popup};
use crate::index::{Index, Sort};
use crate::library::Library;
use crate::query::{Field, Query};
use crate::record::new_key;
use crate::source::Source;
use crate::{bibtex, demo, layout, ui};
use ratatui::Terminal;
use ratatui::backend::TestBackend;
use std::path::PathBuf;

/// The demo library in a folder of its own, and its index beside it.
fn library() -> (tempfile::TempDir, Library, Index) {
    let dir = tempfile::tempdir().unwrap();
    let lib = Library::init(&dir.path().join("lib")).unwrap();
    for r in demo::records() {
        lib.save(&r, None).unwrap();
    }
    let mut index = Index::open(&dir.path().join("index.sqlite")).unwrap();
    let done = index.refresh(&lib).unwrap();
    assert!(done.errors.is_empty(), "{:?}", done.errors);
    (dir, lib, index)
}

fn keys(index: &Index, q: &str, sort: Sort) -> Vec<String> {
    index
        .search(&Query::parse(q), sort, false, None)
        .unwrap()
        .into_iter()
        .map(|h| h.record.key)
        .collect()
}

fn screen(app: &mut App, width: u16, height: u16) -> String {
    let mut terminal = Terminal::new(TestBackend::new(width, height)).unwrap();
    terminal.draw(|f| ui::draw(f, app)).unwrap();
    let buffer = terminal.backend().buffer();
    (0..height)
        .map(|y| {
            (0..width)
                .map(|x| buffer[(x, y)].symbol().to_string())
                .collect::<String>()
                .trim_end()
                .to_string()
        })
        .collect::<Vec<_>>()
        .join("\n")
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
