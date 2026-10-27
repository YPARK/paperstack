use crate::app::{App, Popup};
use crate::index::{Index, Sort};
use crate::library::Library;
use crate::query::{Field, Query};
use crate::record::new_key;
use crate::source::Source;
use crate::{bibtex, demo, layout};
use std::path::PathBuf;

/// The demo library in a folder of its own, and its index beside it.
pub(crate) fn library() -> (tempfile::TempDir, Library, Index) {
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

pub(crate) fn screen(app: &mut App, width: u16, height: u16) -> String {
    crate::app::screen(app, width, height)
}

#[test]
fn files_papers_by_year_and_first_author() {
    let records = demo::records();
    let find = |key| records.iter().find(|r| r.key == key).unwrap();
    assert_eq!(
        layout::pdf(find("Brandt-Okafor2025-pb")),
        PathBuf::from(
            "2025/Brandt-Okafor/C/Brandt-Okafor2025-pb_neural-gene-perturbation-effect-models-are.pdf"
        )
    );
    assert!(layout::record(find("SCM-team2024-lc")).starts_with("2024/SCM team/_/"));
    assert_eq!(layout::slug("Lévy flights: A/B? c"), "levy-flights-a-b-c");
}

#[test]
fn reads_only_what_changed() {
    let (_dir, lib, mut index) = library();
    assert_eq!(index.shelves().unwrap().all, 9);
    let again = index.refresh(&lib).unwrap();
    assert_eq!((again.read, again.removed), (0, 0));

    let hit = index.get("Nakamura2016-al").unwrap().unwrap();
    std::fs::remove_file(lib.root.join(&hit.path)).unwrap();
    let gone = index.refresh(&lib).unwrap();
    assert_eq!((gone.read, gone.removed), (0, 1));
    assert!(index.get("Nakamura2016-al").unwrap().is_none());
}

#[test]
fn sees_a_pdf_put_beside_its_record() {
    let (_dir, lib, mut index) = library();
    assert!(keys(&index, "has:pdf", Sort::Added).is_empty());
    let hit = index.get("Kowalczyk2025-zs").unwrap().unwrap();
    std::fs::write(lib.root.join(&hit.path).with_extension("pdf"), b"%PDF-1.4").unwrap();
    index.refresh(&lib).unwrap();
    assert_eq!(keys(&index, "has:pdf", Sort::Added), ["Kowalczyk2025-zs"]);
}

#[test]
fn searches_words_fields_and_abstracts() {
    let (_dir, _lib, index) = library();
    assert_eq!(
        keys(&index, "linear", Sort::Relevance),
        ["Brandt-Okafor2025-pb"]
    );
    assert_eq!(
        keys(&index, "author:Halvorsen", Sort::Relevance),
        ["Brandt-Okafor2025-pb"]
    );
    // Only in its abstract.
    assert_eq!(keys(&index, "tokens", Sort::Relevance), ["SCM-team2024-lc"]);
    // A word matches the words it starts, without accents.
    assert_eq!(keys(&index, "perturb", Sort::Relevance).len(), 2);
    assert_eq!(keys(&index, "leves", Sort::Relevance), ["Lindqvist2024-cs"]);
    assert_eq!(
        keys(&index, "\"Gene2Prose\"", Sort::Relevance),
        ["Lindqvist2024-cs"]
    );
    assert_eq!(
        keys(&index, "10.5555/demo.2016.03", Sort::Relevance),
        ["Nakamura2016-al"]
    );
    assert_eq!(keys(&index, "year:2025..2026", Sort::Relevance).len(), 4);
    assert_eq!(
        keys(&index, "folder:\"single cell data\"", Sort::Relevance).len(),
        3
    );
    assert_eq!(keys(&index, "-label:benchmark", Sort::Relevance).len(), 7);
    assert_eq!(
        keys(&index, "is:starred", Sort::Relevance),
        ["Whitlock2017-ex"]
    );
    assert_eq!(
        keys(&index, "cell -author:Rourke year:2026", Sort::Relevance),
        ["Abbas2026-st"]
    );
    assert_eq!(
        keys(&index, "doi:10.5555/demo.2017.01", Sort::Relevance),
        ["Whitlock2017-ex"]
    );
    assert!(keys(&index, "-", Sort::Relevance).len() == 9);
}

#[test]
fn sorts() {
    let (_dir, _lib, index) = library();
    let by_year = keys(&index, "", Sort::Year);
    assert!(by_year[0].contains("2026") && by_year[8] == "Nakamura2016-al");
    assert_eq!(keys(&index, "", Sort::Added)[0], "Whitlock2017-ex");
    assert_eq!(keys(&index, "", Sort::Author)[0], "Abbas2026-st");
    let oldest = index
        .search(&Query::default(), Sort::Year, true, Some(1))
        .unwrap();
    assert_eq!(oldest[0].record.key, "Nakamura2016-al");
}

#[test]
fn parses_queries() {
    let q = Query::parse(r#"-folder:"Single Cell Data" au:Halvorsen "a b" -x"#);
    assert_eq!(q.terms.len(), 4);
    assert!(q.terms[0].negated && q.terms[0].field == Field::Folder);
    assert_eq!(q.terms[0].value, "Single Cell Data");
    assert_eq!(q.terms[1].field, Field::Author);
    assert!(q.terms[2].phrase && q.terms[2].field == Field::Any);
    assert!(q.terms[3].negated && q.terms[3].value == "x");
    // Not a field: a word with a colon in it.
    assert_eq!(Query::parse("arXiv:2412.99901").terms[0].field, Field::Any);
}

#[test]
fn writes_plain_bibtex() {
    let records = demo::records();
    let find = |key| records.iter().find(|r| r.key == key).unwrap();
    let bib = bibtex::entry(find("Whitlock2017-ex"));
    assert!(bib.starts_with("@article{Whitlock2017-ex,\n"), "{bib}");
    for field in [
        "author = {Whitlock, TJ and Dunmore, P}",
        "journal = {Ann Fict Med}",
        "year = {2017}",
        "doi = {10.5555/demo.2017.01}",
    ] {
        assert!(bib.contains(field), "{field} not in\n{bib}");
    }
    let bib = bibtex::entry(find("Rourke2026-uc"));
    assert!(bib.contains("{Imagined Atlas Consortium}"), "{bib}");
    let bib = bibtex::entry(find("SCM-team2024-lc"));
    assert!(bib.contains("eprint = {2412.99901}"), "{bib}");
}

#[test]
fn reads_crossrefs_types_as_csls() {
    let mut r = demo::records().remove(0);
    r.csl["type"] = serde_json::json!("journal-article");
    assert_eq!(r.kind(), "Journal article");
    let bib = bibtex::entry(&r);
    assert!(
        bib.starts_with("@article{") && bib.contains("journal = {Ann Fict Med}"),
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
    let a = new_key(&r, "10.5555/demo.2025.07", |_| false);
    assert!(a.starts_with("Brandt-Okafor2025-") && a.len() == "Brandt-Okafor2025-xy".len());
    assert_eq!(a, new_key(&r, "10.5555/DEMO.2025.07", |_| false));
    let b = new_key(&r, "10.5555/demo.2025.07", |k| k == a);
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
fn deletes_papers_to_the_trash() {
    let (_dir, lib, mut index) = library();
    let key = "Kowalczyk2025-zs";
    let title = index.get(key).unwrap().unwrap().record.title().to_string();
    crate::add::attach(&lib, &mut index, key, &first_page(&title), "x.pdf", false).unwrap();
    let was = index.get(key).unwrap().unwrap().path.clone();
    let to = crate::remove::paper(&lib, &mut index, key).unwrap();
    assert!(index.get(key).unwrap().is_none());
    assert!(!lib.root.join(&was).exists());
    assert!(!lib.root.join(&was).with_extension("pdf").exists());
    // Its folders, emptied, are taken away too.
    assert!(!lib.root.join(&was).parent().unwrap().exists());
    assert!(lib.root.join(&to).is_file());
    assert!(lib.root.join(&to).with_extension("pdf").is_file());
    // The trash is not read as the library's.
    let done = index.refresh(&lib).unwrap();
    assert!(index.get(key).unwrap().is_none() && done.errors.is_empty());
    assert!(crate::remove::paper(&lib, &mut index, key).is_err());
    // Moved back, it is found again.
    std::fs::create_dir_all(lib.root.join(&was).parent().unwrap()).unwrap();
    std::fs::rename(lib.root.join(&to), lib.root.join(&was)).unwrap();
    index.refresh(&lib).unwrap();
    assert!(index.get(key).unwrap().is_some());
}

/// The name Dropbox gives a conflicted copy of the file at `rel`.
fn conflicted(rel: &std::path::Path) -> PathBuf {
    let stem = rel.file_stem().unwrap().to_string_lossy();
    let ext = rel.extension().unwrap().to_string_lossy();
    rel.with_file_name(format!(
        "{stem} (Laptop's conflicted copy 2026-10-27).{ext}"
    ))
}

#[test]
fn settles_conflicted_copies() {
    use std::fs;
    let (_dir, lib, mut index) = library();
    let trash = lib.root.join(crate::remove::TRASH);

    // A copy the same as its record goes to the trash.
    let same = PathBuf::from(&index.get("Kowalczyk2025-zs").unwrap().unwrap().path);
    let copy = conflicted(&same);
    fs::copy(lib.root.join(&same), lib.root.join(&copy)).unwrap();
    // So does a PDF's copy the same as it.
    let pdf = same.with_extension("pdf");
    fs::write(lib.root.join(&pdf), b"%PDF-1.4").unwrap();
    fs::write(lib.root.join(conflicted(&pdf)), b"%PDF-1.4").unwrap();

    // A copy of a record of the same key is merged into it, the newer
    // kept, the labels and stars of both.
    let path = PathBuf::from(&index.get("Nakamura2016-al").unwrap().unwrap().path);
    let mut ours = record_of(&lib, &index, "Nakamura2016-al");
    ours.paperstack.labels = vec!["mine".into()];
    let mut theirs = ours.clone();
    theirs.paperstack.labels = vec!["theirs".into()];
    theirs.paperstack.starred = true;
    theirs.csl["title"] = "Retitled".into();
    let merged = conflicted(&path);
    let older = std::time::SystemTime::now() - std::time::Duration::from_secs(60);
    fs::write(lib.root.join(&path), serde_json::to_vec(&ours).unwrap()).unwrap();
    fs::File::options()
        .write(true)
        .open(lib.root.join(&path))
        .unwrap()
        .set_modified(older)
        .unwrap();
    fs::write(lib.root.join(&merged), serde_json::to_vec(&theirs).unwrap()).unwrap();

    // A copy of nothing takes its place.
    let lone = PathBuf::from(&index.get("Brandt-Okafor2025-pb").unwrap().unwrap().path);
    fs::rename(lib.root.join(&lone), lib.root.join(conflicted(&lone))).unwrap();

    // A PDF's copy that differs is left.
    let other = lone.with_extension("pdf");
    fs::write(lib.root.join(&other), b"%PDF-1.4 a").unwrap();
    fs::write(lib.root.join(conflicted(&other)), b"%PDF-1.4 b").unwrap();

    let done = index.refresh(&lib).unwrap();
    assert!(done.errors.is_empty(), "{:?}", done.errors);
    assert_eq!(done.settled.len(), 4);
    assert_eq!(done.conflicts, [conflicted(&other)]);
    assert!(!lib.root.join(&copy).exists() && trash.join(&copy).is_file());
    assert!(trash.join(conflicted(&pdf)).is_file());
    assert!(trash.join(&merged).is_file());
    let r = record_of(&lib, &index, "Nakamura2016-al");
    assert_eq!(r.title(), "Retitled");
    assert_eq!(r.paperstack.labels, ["theirs", "mine"]);
    assert!(r.paperstack.starred);
    assert!(index.get("Brandt-Okafor2025-pb").unwrap().is_some());
    assert!(lib.root.join(&lone).is_file());
    // Settled, they are not seen again.
    let again = index.refresh(&lib).unwrap();
    assert!(again.settled.is_empty() && again.conflicts.len() == 1);
}

#[test]
fn leaves_a_conflicted_copy_of_another_key() {
    let (_dir, lib, mut index) = library();
    let path = PathBuf::from(&index.get("Nakamura2016-al").unwrap().unwrap().path);
    let mut other = record_of(&lib, &index, "Nakamura2016-al");
    other.key = "Nakamura2016-zz".into();
    let copy = conflicted(&path);
    std::fs::write(lib.root.join(&copy), serde_json::to_vec(&other).unwrap()).unwrap();
    let done = index.refresh(&lib).unwrap();
    assert!(done.settled.is_empty());
    assert_eq!(done.conflicts, [copy]);
}

#[test]
fn names_the_file_a_conflicted_copy_is_of() {
    use crate::library::conflicted_of;
    use std::path::Path;
    assert_eq!(
        conflicted_of(Path::new(
            "2025/A/B/A2025-xy_t (Mac's conflicted copy 2026-10-27 (1)).json"
        )),
        Some(PathBuf::from("2025/A/B/A2025-xy_t.json"))
    );
    assert_eq!(
        conflicted_of(Path::new("A2025-xy_t (conflicted copy 2016-01-02).pdf")),
        Some(PathBuf::from("A2025-xy_t.pdf"))
    );
    assert_eq!(conflicted_of(Path::new("A2025-xy_t.json")), None);
}

#[test]
fn quits_by_ctrl_x_ctrl_c_alone() {
    use crate::app::keys;
    let (_dir, lib, index) = library();
    let mut app = App::new(lib, index).unwrap();
    for spec in ["q", "<c-c>", "<c-x>", "j", "<c-c>"] {
        app.key(keys(spec)[0]);
        assert!(!app.quitting(), "{spec} quit");
    }
    app.key(keys("<c-x>")[0]);
    app.key(keys("<c-c>")[0]);
    assert!(app.quitting());
}

#[test]
fn drops_a_pdf_keeping_the_paper() {
    let (_dir, lib, mut index) = library();
    let key = "Kowalczyk2025-zs";
    let title = index.get(key).unwrap().unwrap().record.title().to_string();
    let bytes = first_page(&title);
    crate::add::attach(&lib, &mut index, key, &bytes, "x.pdf", false).unwrap();
    let was = index.get(key).unwrap().unwrap().path.clone();
    let to = crate::remove::pdf(&lib, &mut index, key).unwrap();
    let hit = index.get(key).unwrap().unwrap();
    assert_eq!(hit.record.key, key);
    assert!(!hit.has_pdf && hit.record.paperstack.pdf_sha256.is_none());
    assert!(lib.root.join(&was).is_file());
    assert!(!lib.root.join(&was).with_extension("pdf").exists());
    assert_eq!(std::fs::read(lib.root.join(&to)).unwrap(), bytes);
    let file = record_of(&lib, &index, key);
    assert!(file.paperstack.pdf_sha256.is_none());
    assert!(crate::remove::pdf(&lib, &mut index, key).is_err());
    // Filed again, and dropped, the paper is still the one it was.
    crate::add::attach(&lib, &mut index, key, &bytes, "y.pdf", true).unwrap();
    crate::remove::pdf(&lib, &mut index, key).unwrap();
    assert!(index.get(key).unwrap().is_some());
    assert!(lib.root.join(&to).is_file());
}

#[test]
fn deletes_a_pdf_from_the_tui_when_asked() {
    use ratatui::crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
    let press = |app: &mut App, c: char| {
        app.key(KeyEvent::new(KeyCode::Char(c), KeyModifiers::NONE));
    };
    let (_dir, lib, mut index) = library();
    let key = "Kowalczyk2025-zs";
    let title = index.get(key).unwrap().unwrap().record.title().to_string();
    crate::add::attach(&lib, &mut index, key, &first_page(&title), "x.pdf", false).unwrap();
    let mut app = App::new(lib, index).unwrap();
    let at = app.shown.iter().position(|h| h.record.key == key).unwrap();
    app.list.select(Some(at));
    press(&mut app, 'x');
    let s = screen(&mut app, 160, 44);
    assert!(
        s.contains("Delete this PDF?") && s.contains("The paper stays") && s.contains(key),
        "{s}"
    );
    press(&mut app, 'n');
    assert!(app.index.get(key).unwrap().unwrap().has_pdf);
    press(&mut app, 'x');
    press(&mut app, 'y');
    let hit = app.index.get(key).unwrap().unwrap();
    assert!(!hit.has_pdf);
    assert!(app.lib.root.join(&hit.path).is_file());
    assert!(!app.lib.root.join(&hit.path).with_extension("pdf").exists());
    assert!(app.status.contains(".trash/"));
}

#[test]
fn locates_a_pdf_on_disk_for_the_paper_picked() {
    use crate::app::Action;
    use ratatui::crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
    let (dir, lib, index) = library();
    let mut app = App::new(lib, index).unwrap();
    let key = "Kowalczyk2025-zs";
    let at = app.shown.iter().position(|h| h.record.key == key).unwrap();
    app.list.select(Some(at));
    let title = app.current().unwrap().record.title().to_string();
    let action = app.key(KeyEvent::new(KeyCode::Char('f'), KeyModifiers::NONE));
    assert!(matches!(action, Some(Action::Locate)));
    let path = dir.path().join("paper.pdf");
    std::fs::write(&path, first_page(&title)).unwrap();
    app.give_pdf(&path);
    let hit = app.index.get(key).unwrap().unwrap();
    assert!(hit.has_pdf);
    assert!(app.lib.root.join(&hit.path).with_extension("pdf").is_file());
    // A paper that has its PDF is not given another until that one is deleted.
    app.give_pdf(&path);
    assert!(app.status.contains("has its PDF"));
    assert!(app.index.get(key).unwrap().unwrap().has_pdf);
}

#[test]
fn jumps_to_the_first_and_last_paper_and_says_the_sort() {
    use crate::index::Sort;
    use ratatui::crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
    let (_dir, lib, index) = library();
    let mut app = App::new(lib, index).unwrap();
    let last = app.shown.len() - 1;
    assert!(last > 0);
    let press = |app: &mut App, code, modifiers| app.key(KeyEvent::new(code, modifiers));
    press(&mut app, KeyCode::Char('G'), KeyModifiers::NONE);
    assert_eq!(app.list.selected(), Some(last));
    press(&mut app, KeyCode::Up, KeyModifiers::CONTROL);
    assert_eq!(app.list.selected(), Some(0));
    press(&mut app, KeyCode::Down, KeyModifiers::CONTROL);
    assert_eq!(app.list.selected(), Some(last));
    press(&mut app, KeyCode::Home, KeyModifiers::NONE);
    assert_eq!(app.list.selected(), Some(0));
    // With no words to rank by, relevance is newest added, and said so.
    assert_eq!(app.sort.of(&app.wanted()), Sort::Added);
    press(&mut app, KeyCode::Char('S'), KeyModifiers::NONE);
    assert_eq!(app.status, format!("sorted by {}", app.sort.name()));
}

/// A popup for work on its own thread, fed by `tx` as the work would.
fn busy(
    then: crate::app::Then,
) -> (
    crate::app::Busy,
    std::sync::mpsc::Sender<crate::app::Report>,
) {
    let (tx, events) = std::sync::mpsc::channel();
    let busy = crate::app::Busy {
        title: "Looking for an open copy",
        what: "A made-up paper".to_string(),
        said: Vec::new(),
        since: std::time::Instant::now(),
        events,
        stop: Default::default(),
        browse: false,
        then,
    };
    (busy, tx)
}

#[test]
fn shows_what_work_on_its_own_thread_says_until_it_is_done() {
    use crate::app::{Report, Then};
    let (_dir, lib, index) = library();
    let mut app = App::new(lib, index).unwrap();
    let (b, tx) = busy(Then::Add);
    app.popup = Popup::Busy(Box::new(b));
    tx.send(Report::Said("downloading https://example.org/a.pdf".into()))
        .unwrap();
    assert!(app.tick(), "drawn again, the spinner turned");
    let s = screen(&mut app, 120, 40);
    assert!(s.contains("Looking for an open copy"), "{s}");
    assert!(s.contains("downloading https://example.org/a.pdf"), "{s}");
    // A click elsewhere does not lose the work.
    app.mouse(ratatui::crossterm::event::MouseEvent {
        kind: ratatui::crossterm::event::MouseEventKind::Down(
            ratatui::crossterm::event::MouseButton::Left,
        ),
        column: 0,
        row: 0,
        modifiers: ratatui::crossterm::event::KeyModifiers::NONE,
    });
    assert!(matches!(app.popup, Popup::Busy(_)));
    tx.send(Report::Done(Err(anyhow::anyhow!("no such paper"))))
        .unwrap();
    app.tick();
    assert!(matches!(app.popup, Popup::None), "closed when done");
    assert!(app.status.contains("no such paper"), "{}", app.status);
}

#[test]
fn stops_looking_for_an_open_copy_when_asked() {
    use crate::app::{Report, Then};
    use ratatui::crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
    let (_dir, lib, index) = library();
    let mut app = App::new(lib, index).unwrap();
    let hit = app.shown[0].clone();
    let (b, tx) = busy(Then::Fetch(hit.clone(), None));
    let stop = b.stop.clone();
    app.popup = Popup::Busy(Box::new(b));
    app.key(KeyEvent::new(KeyCode::Esc, KeyModifiers::NONE));
    assert!(stop.load(std::sync::atomic::Ordering::Relaxed));
    assert!(
        matches!(app.popup, Popup::Busy(_)),
        "until the step is done"
    );
    tx.send(Report::Done(Err(anyhow::anyhow!("stopped"))))
        .unwrap();
    app.tick();
    assert!(matches!(app.popup, Popup::None), "and not to the browser");
    assert!(app.status.contains("stopped"), "{}", app.status);
}

#[test]
fn deletes_a_collection_keeping_its_papers() {
    use crate::remove::Shelf;
    let (_dir, lib, mut index) = library();
    let mut r = record_of(&lib, &index, "Nakamura2016-al");
    r.paperstack
        .folders
        .push("Active_Screens/screens".to_string());
    let path = index.get("Nakamura2016-al").unwrap().unwrap().path.clone();
    lib.save(&r, Some(&path)).unwrap();
    index.refresh(&lib).unwrap();
    let all = index.shelves().unwrap().all;

    let n = crate::remove::shelf(&lib, &mut index, Shelf::Collection, "active_screens").unwrap();
    assert_eq!(n, 3);
    assert_eq!(index.shelves().unwrap().all, all);
    assert!(keys(&index, "folder:Active_Screens", Sort::Relevance).is_empty());
    assert!(keys(&index, "folder:Active_Screens/screens", Sort::Relevance).is_empty());
    let nakamura = record_of(&lib, &index, "Nakamura2016-al");
    assert!(nakamura.paperstack.folders.is_empty());
    assert_eq!(nakamura.paperstack.labels, ["method"]);
    // Others' collections are left as they were.
    let abbas = record_of(&lib, &index, "Abbas2026-st");
    assert_eq!(abbas.paperstack.folders, ["Single Cell Data"]);

    crate::remove::shelf(&lib, &mut index, Shelf::Label, "to-read").unwrap();
    assert!(keys(&index, "label:to-read", Sort::Relevance).is_empty());
    assert!(crate::remove::shelf(&lib, &mut index, Shelf::Label, "to-read").is_err());
}

/// The record of the paper `key`, as its file has it.
fn record_of(lib: &Library, index: &Index, key: &str) -> crate::record::Record {
    let hit = index.get(key).unwrap().unwrap();
    crate::library::read(&lib.root.join(&hit.path)).unwrap()
}

#[test]
fn deletes_from_the_tui_when_asked() {
    use ratatui::crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
    let press = |app: &mut App, c: char| {
        app.key(KeyEvent::new(KeyCode::Char(c), KeyModifiers::NONE));
    };
    let (_dir, lib, index) = library();
    let mut app = App::new(lib, index).unwrap();
    let all = app.total();
    let key = app.current().unwrap().record.key.clone();
    press(&mut app, 'd');
    let s = screen(&mut app, 160, 44);
    assert!(s.contains("Delete this paper?") && s.contains(&key), "{s}");
    press(&mut app, 'n');
    assert_eq!(app.total(), all);
    press(&mut app, 'd');
    press(&mut app, 'y');
    assert_eq!(app.total(), all - 1);
    assert!(app.index.get(&key).unwrap().is_none());

    // On the left, the collection picked; its papers stay.
    press(&mut app, 'h');
    while app.shelves[app.shelf].0.name() != "mediation" {
        app.key(KeyEvent::new(KeyCode::Down, KeyModifiers::NONE));
    }
    press(&mut app, 'd');
    assert!(screen(&mut app, 160, 44).contains("Delete this collection?"));
    press(&mut app, 'y');
    assert!(app.shelves.iter().all(|(s, _)| s.name() != "mediation"));
    assert_eq!(app.total(), all - 1);
}

#[test]
fn draws_the_library() {
    let (_dir, lib, index) = library();
    let mut app = App::new(lib, index).unwrap();
    let s = screen(&mut app, 160, 44);
    println!("{s}");
    assert!(s.contains("Bounding hidden bias"));
    assert!(s.contains("Single Cell Data"));
    assert!(s.contains("2017/Whitlock/TJ/"));
}

/// A left click where any of `what` is drawn the `nth` time, on the
/// sidebar or right of it.
pub(crate) fn click(
    app: &mut App,
    what: &str,
    nth: usize,
    side: bool,
) -> Option<crate::app::Action> {
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
    assert!(matches!(&app.popup, Popup::Cite(c) if c.text.contains(&second)));
    click(&mut app, "▤", 0, true);
    assert!(matches!(app.popup, Popup::None));

    click(&mut app, "∅", 0, true);
    assert_eq!(app.shelves[app.shelf].0.name(), "No PDF");
}

#[test]
fn draws_add() {
    let (_dir, lib, index) = library();
    let mut app = App::new(lib, index).unwrap();
    app.popup = Popup::Add("https://arxiv.org/abs/2412.99901".to_string());
    let s = screen(&mut app, 160, 44);
    assert!(s.contains("arXiv 2412.99901"));
}

/// A paper found, as a registry gives it.
fn found(doi: &str, family: &str, title: &str) -> crate::resolve::Found {
    crate::resolve::Found {
        csl: serde_json::json!({
            "DOI": doi,
            "title": title,
            "author": [{ "family": family, "given": "A" }],
            "issued": { "date-parts": [[2025]] },
        }),
        arxiv: None,
        preprint: None,
        pdfs: Vec::new(),
        source: "crossref",
    }
}

#[test]
fn draws_choose() {
    let (_dir, lib, index) = library();
    let mut app = App::new(lib, index).unwrap();
    app.popup = Popup::Choose(Box::new(crate::app::Choose {
        name: "~/Downloads/letters.pdf".to_string(),
        papers: vec![
            found(
                "10.1000/letters.2025.101",
                "Alder",
                "Clinic Visits Among Rural Adults, 2013-2023",
            ),
            found(
                "10.1000/letters.2025.102",
                "Birch",
                "Inherited Variants in Kidney Stone Disease",
            ),
        ],
        pdf: Vec::new(),
        at: 0,
    }));
    let s = screen(&mut app, 160, 44);
    assert!(s.contains("Which paper?"));
    assert!(s.contains("1. 10.1000/letters.2025.101"));
    assert!(s.contains("Birch"));
    for k in crate::app::keys("2") {
        assert!(app.key(k).is_none());
    }
    let Popup::Choose(c) = &app.popup else {
        panic!("the popup closed")
    };
    assert_eq!(c.at, 1);
    let enter = crate::app::keys("<enter>").remove(0);
    assert!(matches!(app.key(enter), Some(crate::app::Action::Choose)));
    let esc = crate::app::keys("<esc>").remove(0);
    app.key(esc);
    assert!(matches!(app.popup, Popup::None));
}

#[test]
fn lists_the_papers_in_a_pdf() {
    let e: anyhow::Error = crate::add::Ambiguous {
        name: "letters.pdf".to_string(),
        papers: vec![
            found(
                "10.1000/letters.2025.101",
                "Alder",
                "Clinic Visits Among Rural Adults, 2013-2023",
            ),
            found(
                "10.1000/letters.2025.102",
                "Birch",
                "Inherited Variants in Kidney Stone Disease",
            ),
        ],
        pdf: Vec::new(),
    }
    .into();
    let s = e.to_string();
    assert!(s.starts_with(
        "letters.pdf: this PDF holds more than one paper; add the one meant by its id:\n"
    ));
    let line = format!(
        "{:<40} 2025 {:<16} Inherited",
        "10.1000/letters.2025.102", "Birch"
    );
    assert!(s.contains(&line));
    assert!(e.downcast_ref::<crate::add::Ambiguous>().is_some());
}

/// A page of a journal's letters: the end of one, with its article
/// information and references, and the start of the next.
const LETTERS: &str = "\n\nResults | Between 2013 and 2023, there were 1200 visits\n\
to the clinics studied.\n\nAnn Alder, PhD\nBen Birch, MPP\nCara L. Cedar, PhD\n\
Author Affiliations: An Institute, Somewhere (Alder, Birch, Cedar);\n\
Accepted for Publication: August 9, 2025.\n\n\
Published Online: May 1, 2025. doi:10.1000/letters.2025.101\n\
Corresponding Author: Dan M. Elm, PhD, Some University School\n\
Funding/Support: This study was supported by A Foundation\n\
One Foundation for Kidney Health and the Other Foundation for Stones and\n\
1. Fir A, Gum B. Trends in Something Else. Letters.\n\n\
Published online June 16, 2025. doi:10.1000/letters.2025.90\n\
3. Hazel HT, Ivy ME. Rural clinic closures in the treatment\n\
of older patients during a pandemic. Letters. 2021;1(2):3-\n\
4. doi:10.1000/letters.2021.7\n\
Juniper J, Kauri K. A Study. Published online May 1, 2025. doi:10.1000/letters.2025.80\n\n\
Inherited Variants in Kidney\n\
Stone Disease\n\
Genetic testing for kidney stones has typi-\n\
Supplemental content\n\nLetters\n\
letters.example (Reprinted) Letters November 18, 2025 Volume 1, Number 2 3\n";

#[test]
fn reads_a_page_of_letters() {
    let reading = crate::pdf::Reading {
        pages: vec![
            LETTERS.to_string(),
            "Published Online: May 2, 2025. doi:10.1/later".to_string(),
        ],
        embedded: Vec::new(),
    };
    // Its own article's, not a reference's, nor a later page's.
    assert_eq!(
        crate::pdf::article_info_dois(&reading),
        vec!["10.1000/letters.2025.101".to_string()]
    );
    assert_eq!(
        crate::pdf::title_lines(LETTERS),
        vec!["Inherited Variants in Kidney Stone Disease".to_string()]
    );
    // An ordinary paper's first page names no article of its own.
    let paper = crate::pdf::Reading {
        pages: vec![
            "A Title of a Paper\n\nAnn Author\n\nAbstract. Words.\n\n\
                     https://doi.org/10.1000/paper.2026.1"
                .to_string(),
        ],
        embedded: Vec::new(),
    };
    assert!(crate::pdf::article_info_dois(&paper).is_empty());
}

#[test]
fn reads_sources() {
    assert!(
        matches!(Source::of("https://doi.org/10.5555/demo.2016.03"), Source::Doi(d) if d == "10.5555/demo.2016.03")
    );
    assert!(matches!(Source::of("2412.99901v2"), Source::Arxiv(_)));
    assert!(matches!(Source::of("PMID: 99999901"), Source::Pmid(_)));
    assert!(matches!(Source::of("~/Downloads/x.pdf"), Source::Pdf(_)));
    assert!(matches!(Source::of("@article{x,"), Source::Bibtex));
    assert!(matches!(Source::of("linear baselines"), Source::Words(_)));
}

#[test]
fn finds_titles_on_a_page() {
    // deunicode once ran "33" into "Purine", and "parasites" into the next line.
    let page = "Fict Biol 2023;12:RP00001.   1 of 33\n\nPurine sensors replace cGMP \nin \
                allosteric control of PKX in \nimaginary\u{a0}parasites\n\nVera Oakes";
    let title = "Purine sensors replace cGMP in allosteric control of PKX in imaginary parasites";
    assert!(crate::pdf::has_title(title, page));
    // Hyphenated across a line.
    assert!(crate::pdf::has_title(
        "Quantifying imaginary complexity",
        "Quan-\ntifying imaginary complexity using"
    ));
    // Words run together in the registry's title.
    assert!(crate::pdf::has_title(
        "CarefullyCalibrated Tokenization forImaginary Models",
        "Carefully Calibrated Tokenization\nfor Imaginary Models"
    ));
    assert!(!crate::pdf::has_title(
        title,
        "Purine sensors in PKX signalling"
    ));
    let csl = serde_json::json!({ "author": [{ "family": "Oakes", "given": "V" }] });
    assert!(crate::pdf::has_author(&csl, page));
    assert!(!crate::pdf::has_author(&csl, "San Juan, Puerto Rico"));
}

#[test]
fn reads_ids_in_publishers_addresses() {
    use Source::*;
    let cases = [
        (
            "https://onlinelibrary.wiley.com/doi/full/10.1002/fict.0001",
            Doi("10.1002/fict.0001".into()),
        ),
        (
            "https://link.springer.com/article/10.1007/s99999-021-00001-1",
            Doi("10.1007/s99999-021-00001-1".into()),
        ),
        (
            "https://www.biorxiv.org/content/10.1101/2023.09.11.999901v1.full.pdf",
            Doi("10.1101/2023.09.11.999901".into()),
        ),
        (
            "https://www.biorxiv.org/content/10.64898/2026.01.01.999901v2.full",
            Doi("10.64898/2026.01.01.999901".into()),
        ),
        (
            "https://www.nature.com/articles/s99999-020-0001-2.pdf",
            Doi("10.1038/s99999-020-0001-2".into()),
        ),
        (
            "https://journals.plos.org/plosone/article?id=10.1371/journal.pone.9999999",
            Doi("10.1371/journal.pone.9999999".into()),
        ),
        (
            "https://www.sciencedirect.com/science/article/pii/S0000000023999991",
            Pii("S0000000023999991".into()),
        ),
        (
            "https://www.cell.com/cell/fulltext/S0000-0000(23)99999-1",
            Pii("S0000000023999991".into()),
        ),
        (
            "https://www.cell.com/cell-genomics/pdf/S0000-000X(26)99999-0.pdf",
            Pii("S0000000X26999990".into()),
        ),
        (
            "https://www.cell.com/action/showPdf?pii=S0000-0001(24)99999-4",
            Pii("S0000000124999994".into()),
        ),
        (
            "https://linkinghub.elsevier.com/retrieve/pii/S0000000226999998",
            Pii("S0000000226999998".into()),
        ),
        (
            "https://arxiv.org/pdf/2412.99901v2",
            Arxiv("2412.99901".into()),
        ),
        (
            "https://pubmed.ncbi.nlm.nih.gov/99999901/",
            Pmid("99999901".into()),
        ),
        (
            "https://pmc.ncbi.nlm.nih.gov/articles/PMC9999901/",
            Pmcid("PMC9999901".into()),
        ),
        (
            "https://academic.oup.com/bioinformatics/article/99/99/i999/9999999",
            Url("https://academic.oup.com/bioinformatics/article/99/99/i999/9999999".into()),
        ),
        (
            "doi:10.5555/demo.2016.03.",
            Doi("10.5555/demo.2016.03".into()),
        ),
        ("arXiv:2412.99901v1", Arxiv("2412.99901".into())),
        ("PMC9999901", Pmcid("PMC9999901".into())),
    ];
    for (given, want) in cases {
        assert_eq!(Source::of(given), want, "{given}");
    }
    assert_eq!(
        crate::source::dois("see https://doi.org/10.5555/fict12373), and 10.5555/FICT12373."),
        ["10.5555/fict12373"]
    );
    assert_eq!(
        crate::source::arxiv_ids("arXiv:2412.99901v1 [cs.CL] 11 Dec 2024"),
        ["2412.99901"]
    );
    assert_eq!(
        crate::source::unversioned("solv-int/9912999v2"),
        "solv-int/9912999"
    );
    assert_eq!(crate::source::unversioned("2412.99901"), "2412.99901");
    assert_eq!(
        Source::of("https://arxiv.org/abs/solv-int/9912999v1"),
        Source::Arxiv("solv-int/9912999".into())
    );
}

#[test]
fn reads_a_pages_meta_tags() {
    let html = r#"<head><meta name="citation_title" content="A &amp; B">
        <meta content="10.1038/s99999-020-0001-2" name="citation_doi">
        <meta name="citation_pdf_url" content="https://x.org/a.pdf"/></head>"#;
    assert_eq!(
        crate::resolve::from_metas(&crate::resolve::metas(html)),
        Some(Source::Doi("10.1038/s99999-020-0001-2".into()))
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
    r.csl["author"] = serde_json::json!([{ "family": "Abernathy", "given": "B. P." }]);
    assert!(layout::record(&r).starts_with("2017/Abernathy/BP/"));
    r.csl["author"] = serde_json::json!([{ "family": "Nakamura", "given": "Aiko W." }]);
    assert!(layout::record(&r).starts_with("2017/Nakamura/Aiko W/"));
}

#[test]
fn knows_a_paper_already_here() {
    let (_dir, _lib, index) = library();
    assert_eq!(
        index
            .by_doi("10.5555/DEMO.2017.01")
            .unwrap()
            .unwrap()
            .record
            .key,
        "Whitlock2017-ex"
    );
    assert_eq!(
        index.by_arxiv("2412.99901").unwrap().unwrap().record.key,
        "SCM-team2024-lc"
    );
    let t = "Bounding hidden bias in observational research: introducing the W-value";
    assert!(index.by_title(t, Some(2018)).unwrap().is_some());
    assert!(index.by_title(t, Some(2020)).unwrap().is_none());
}

#[test]
fn reads_a_paper_from_a_pages_tags() {
    // NeurIPS's pages before 2022: Highwire's tags, no DOI.
    let html = r#"<meta name="citation_title" content="Notice is All you Want">
        <meta name="citation_author" content="Vance, Ada">
        <meta name="citation_author" content="Shearer, Noel">
        <meta name="citation_journal_title" content="Advances in Neural Information Processing Systems">
        <meta name="citation_volume" content="30">
        <meta name="citation_firstpage" content="101"><meta name="citation_lastpage" content="110">
        <meta name="citation_publication_date" content="2017">"#;
    assert_eq!(
        crate::resolve::from_metas(&crate::resolve::metas(html)),
        None
    );
    let found = crate::resolve::from_tags(&crate::resolve::metas(html), "https://papers.nips.cc/x")
        .unwrap();
    assert_eq!(found.csl["type"], "paper-conference");
    assert_eq!(found.csl["author"][0]["family"], "Vance");
    assert_eq!(found.csl["issued"]["date-parts"][0][0], 2017);
    assert_eq!(found.csl["page"], "101-110");
    let r = crate::record::Record {
        key: "Vance2017-x".into(),
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
        "author:Halvorsen year:2020..",
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
        crate::source::arxiv_ids("\u{212A} arXiv:2412.99901v1 [cs.CL]"),
        ["2412.99901"]
    );
    assert_eq!(
        Source::of("https://www.Nature.com/articles/s99999-020-0001-2"),
        Source::Doi("10.1038/s99999-020-0001-2".into())
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
    r.paperstack.aliases = vec!["Whitlock2017-ex".into()];
    lib.save(&r, None).unwrap();
    let done = index.refresh(&lib).unwrap();
    assert!(
        done.errors.iter().any(|e| e.contains("alias")),
        "{:?}",
        done.errors
    );
    assert_eq!(
        index
            .get("Whitlock2017-ex")
            .unwrap()
            .unwrap()
            .record
            .title(),
        "Bounding hidden bias in observational research: Introducing the W-value"
    );
}

#[test]
fn moves_no_pdf_over_anothers() {
    let (_dir, lib, index) = library();
    let hit = index.get("Nakamura2016-al").unwrap().unwrap();
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
fn serves_only_the_extensions_named_with_the_token() {
    use crate::daemon::{Allowed, EXTENSION, TOKEN_HEADER, origin, refusal};
    let allowed = Allowed {
        origins: vec![origin(EXTENSION), origin("moz-extension://ID1")],
        token: "a".repeat(64),
    };
    let token = allowed.token.clone();
    let ask = |method: &str, host: &str, from: Option<&str>, token: Option<&str>, length: usize| {
        let mut headers = std::collections::HashMap::new();
        headers.insert("host".to_string(), host.to_string());
        if let Some(o) = from {
            headers.insert("origin".to_string(), o.to_string());
        }
        if let Some(t) = token {
            headers.insert(TOKEN_HEADER.to_string(), t.to_string());
        }
        refusal(method, &headers, 8765, &allowed, length).map(|(code, _)| code)
    };
    let ours = format!("chrome-extension://{EXTENSION}");
    let t = Some(token.as_str());
    let post = |host: &str, from: Option<&str>, length: usize| ask("POST", host, from, t, length);
    assert_eq!(post("127.0.0.1:8765", Some(&ours), 10), None);
    assert_eq!(
        post("127.0.0.1:8765", Some("moz-extension://ID1"), 10),
        None
    );
    for other in [
        "chrome-extension://ID2",
        "moz-extension://ID2",
        "https://example.org",
    ] {
        assert_eq!(
            post("127.0.0.1:8765", Some(other), 10),
            Some(403),
            "{other}"
        );
    }
    assert_eq!(
        post("example.org:8765", Some(&ours), 10),
        Some(403),
        "rebound"
    );
    assert_eq!(post("127.0.0.1:9999", None, 10), Some(403));
    assert_eq!(post("127.0.0.1:8765", Some(&ours), usize::MAX), Some(413));

    // Without the token, as another user's program: refused, but for a
    // browser's preflight.
    let here = "localhost:8765";
    assert_eq!(
        ask("POST", here, None, None, 10),
        Some(401),
        "a program here"
    );
    assert_eq!(ask("POST", here, Some(&ours), Some("guess"), 10), Some(401));
    assert_eq!(ask("GET", here, None, Some(&"a".repeat(63)), 0), Some(401));
    assert_eq!(ask("POST", here, None, t, 10), None, "a program with it");
    assert_eq!(ask("OPTIONS", here, Some(&ours), None, 0), None);
}

#[test]
fn keeps_one_token_private() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("paperstack/token");
    let first = crate::daemon::token_at(&path).unwrap();
    assert_eq!(first.len(), 64);
    assert_eq!(crate::daemon::token_at(&path).unwrap(), first, "kept");
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let mode = std::fs::metadata(&path).unwrap().permissions().mode();
        assert_eq!(mode & 0o777, 0o600);
    }
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
pub(crate) fn first_page(title: &str) -> Vec<u8> {
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
    let key = "Kowalczyk2025-zs";
    let title = index.get(key).unwrap().unwrap().record.title().to_string();
    let other = crate::add::attach(
        &lib,
        &mut index,
        key,
        &first_page("Another paper altogether, on other things"),
        "x.pdf",
        true,
    );
    assert!(other.unwrap_err().to_string().contains("is not"));
    let added =
        crate::add::attach(&lib, &mut index, key, &first_page(&title), "x.pdf", false).unwrap();
    assert_eq!(added.outcome, crate::add::Outcome::PdfAttached);
    let hit = index.get(key).unwrap().unwrap();
    assert!(hit.has_pdf && hit.record.paperstack.pdf_sha256.is_some());
    assert!(lib.root.join(&hit.path).with_extension("pdf").exists());
    let again = crate::add::attach(&lib, &mut index, key, &first_page(&title), "x.pdf", false);
    assert!(again.unwrap_err().to_string().contains("already"));
}

#[test]
fn takes_a_scan_only_named_or_by_its_doi() {
    let (_dir, lib, mut index) = library();
    let key = "Nakamura2016-al";
    let doi = index
        .get(key)
        .unwrap()
        .unwrap()
        .record
        .doi()
        .unwrap()
        .to_string();
    let scan = tiny_pdf(&["Scanned"]);
    // Found, as the next download, or given by an agent: not taken.
    let found = crate::add::attach(&lib, &mut index, key, &scan, "x.pdf", false);
    assert!(
        found
            .unwrap_err()
            .to_string()
            .contains("no text to tell it by")
    );
    // Another paper's DOI on it is not this paper's.
    let other = tiny_pdf(&["Scanned", "doi:10.1000/another.paper"]);
    assert!(crate::add::attach(&lib, &mut index, key, &other, "x.pdf", false).is_err());
    // Its own DOI on it is.
    let ours = tiny_pdf(&["Scanned", &format!("doi:{doi}")]);
    let added = crate::add::attach(&lib, &mut index, key, &ours, "x.pdf", false).unwrap();
    assert_eq!(added.how, "x.pdf");

    // Named by the person: taken, and said to be unchecked.
    let key = "Whitlock2017-ex";
    let added = crate::add::attach(&lib, &mut index, key, &scan, "x.pdf", true).unwrap();
    assert!(added.how.contains("no text to check it by"));
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
    let key = "Kowalczyk2025-zs".to_string();
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
        back: None,
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
            "_id": "1", "pubtype": "PP_ARTICLE", "citekey": "Gomez-Ruiz2018-vd",
            "title": "Cardiac polyploidy builds a barrier to fin regrowth in imaginary fish",
            "author": [
                { "first": "Juana María", "last": "Gómez-Ruiz", "initials": "JM" },
                { "first": "Mika", "last": "Sharma", "initials": "M" }
            ],
            "published": { "year": "2018", "month": "2", "day": "26" },
            "journal": "Dev. Fict", "journalfull": "Developmental Fiction",
            "volume": "1", "issue": "2", "pages": "3-14.e7",
            "doi": "10.5555/demo.2018.01", "pmid": "99999902", "pmc": "PMC9999902",
            "abstract": "Polyploid fins regrow less.",
            "foldersNamed": ["Research/Fins"], "labelsNamed": ["to-read"], "star": 1,
            "attachments": [{
                "article_pdf": 1,
                "filename": "All Papers/G/Gómez-Ruiz et al. 2018 - Cardiac polyploidy builds a barrier to fin regrowth in imaginary fish.pdf"
            }],
            "url": ["http://dx.doi.org/10.5555/demo.2018.01"]
        },
        {
            "_id": "2", "pubtype": "PP_CONFERENCE_PAPER", "citekey": "Vance2017-pp",
            "title": "{Notice is all you want}",
            "author": [{ "first": "Ada", "last": "Vance" }],
            "published": { "year": "2017" }, "booktitle": "Advances in Neural Information Processing Systems",
            "foldersNamed": ["Research/ML/NLP", "Research/ML/Transformers"], "labelsNamed": ["Machine Learning"]
        },
        {
            "_id": "3", "pubtype": "PP_MISC", "title": "Notes on nothing in particular, unkeyed",
            "author": [{ "first": "A", "last": "Nobody" }], "published": { "year": "2020" }
        },
        { "_id": "4", "pubtype": "PP_ARTICLE", "citekey": "Gone2019-xx", "title": "In the trash", "trashed": 1 },
        {
            "_id": "5", "pubtype": "PP_ARTICLE", "citekey": "Whitlock2017-pp",
            "title": "Bounding hidden bias in observational research: Introducing the W-value",
            "doi": "10.5555/demo.2017.01", "foldersNamed": ["grant_xyz"]
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
    let name = "Gómez-Ruiz et al. 2018 - Cardiac polyploidy builds a barrier to fin regrowth in imaginary fish.pdf";
    let nfd: String = unicode_normalization::UnicodeNormalization::nfd(name).collect();
    std::fs::create_dir_all(pdfs.join("All Papers/G")).unwrap();
    std::fs::write(
        pdfs.join("All Papers/G").join(nfd),
        tiny_pdf(&["Cardiac polyploidy"]),
    )
    .unwrap();

    let dry = crate::migrate::import(&lib, &mut index, &export, Some(&pdfs), true).unwrap();
    assert_eq!((dry.added, dry.merged, dry.trashed, dry.pdfs), (3, 1, 1, 1));
    assert!(
        index.get("Gomez-Ruiz2018-vd").unwrap().is_none(),
        "a dry run writes nothing"
    );

    let done = crate::migrate::import(&lib, &mut index, &export, Some(&pdfs), false).unwrap();
    assert_eq!(
        (done.read, done.added, done.merged, done.trashed, done.pdfs),
        (5, 3, 1, 1, 1)
    );
    assert!(done.errors.is_empty(), "{:?}", done.errors);

    let hit = index.get("Gomez-Ruiz2018-vd").unwrap().unwrap();
    let r = &hit.record;
    assert_eq!(r.csl_type(), "article-journal");
    assert_eq!(r.venue(), "Developmental Fiction");
    assert_eq!(r.year(), Some(2018));
    assert_eq!(r.csl["PMCID"], "PMC9999902");
    assert_eq!(r.paperstack.folders, ["Research/Fins"]);
    assert!(r.paperstack.starred && hit.has_pdf);
    assert!(hit.path.starts_with("2018/Gómez-Ruiz/Juana María/"));
    let bib = bibtex::entry(r);
    assert!(
        bib.starts_with("@article{Gomez-Ruiz2018-vd,")
            && bib.contains("journal = {Developmental Fiction}"),
        "{bib}"
    );

    let ml = index.get("Vance2017-pp").unwrap().unwrap();
    // Braced whole by the other manager: unbraced as it comes in.
    assert_eq!(ml.record.csl["title"], "Notice is all you want");
    assert_eq!(ml.record.csl_type(), "paper-conference");
    assert_eq!(
        keys(&index, "folder:\"Research/ML/NLP\"", Sort::Added),
        ["Vance2017-pp"]
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
    let here = index.get("Whitlock2017-pp").unwrap().unwrap();
    assert_eq!(here.record.key, "Whitlock2017-ex");
    assert!(
        here.record
            .paperstack
            .folders
            .contains(&"grant_xyz".to_string())
    );

    // Again: nothing added twice.
    let again = crate::migrate::import(&lib, &mut index, &export, Some(&pdfs), false).unwrap();
    assert_eq!((again.added, again.merged), (0, 4));
}

#[test]
fn serves_the_library_over_mcp() {
    let (_dir, lib, mut index) = library();
    let key = "Kowalczyk2025-zs";
    let title = index.get(key).unwrap().unwrap().record.title().to_string();
    crate::add::attach(&lib, &mut index, key, &first_page(&title), "x.pdf", false).unwrap();
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
    assert!(names.contains(&"search_papers") && names.contains(&"read_paper") && names.len() == 13);

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
        "Brandt-Okafor2025-pb"
    );
    assert_eq!(
        found["structuredContent"]["papers"][0]["authors"][0],
        "Brandt-Okafor, C"
    );

    let bib = call(
        &mut server,
        "get_paper",
        serde_json::json!({ "key": "Whitlock2017-ex", "format": "bibtex" }),
    );
    assert!(
        bib["content"][0]["text"]
            .as_str()
            .unwrap()
            .starts_with("@article{Whitlock2017-ex,")
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
            .contains("@Corbett2021-mr")
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
        serde_json::json!({ "key": "Whitlock2017-ex" }),
    );
    assert_eq!(none["isError"], true);

    let updated = call(
        &mut server,
        "update_paper",
        serde_json::json!({
        "key": "Nakamura2016-al", "add_collections": ["grant_xyz"], "starred": true }),
    );
    assert_eq!(updated["structuredContent"]["starred"], true);
    assert!(
        updated["structuredContent"]["collections"]
            .as_array()
            .unwrap()
            .contains(&serde_json::json!("grant_xyz"))
    );
    let gone = call(
        &mut server,
        "delete_collection",
        serde_json::json!({ "name": "grant_xyz" }),
    );
    assert_eq!(gone["structuredContent"]["papers"], 1, "{gone}");
    let deleted = call(
        &mut server,
        "delete_papers",
        serde_json::json!({ "keys": ["Nakamura2016-al"] }),
    );
    assert_eq!(deleted["isError"], false, "{deleted}");
    let again = call(
        &mut server,
        "get_paper",
        serde_json::json!({ "key": "Nakamura2016-al" }),
    );
    assert_eq!(again["isError"], true);
    let unknown = call(&mut server, "no_such_tool", serde_json::json!({}));
    assert_eq!(unknown["isError"], true);
    // A notification is answered with nothing.
    assert!(
        server
            .answer(&serde_json::json!({ "jsonrpc": "2.0", "method": "notifications/initialized" }))
            .is_none()
    );
}

#[test]
fn finds_the_library_where_it_is_or_should_be() {
    let home = tempfile::tempdir().unwrap();
    let h = home.path();
    // No Dropbox: a folder of its own, and no Dropbox folder made.
    assert_eq!(crate::default_root_in(h), h.join("paperstack"));
    // Dropbox, as recent macOS keeps it.
    let cloud = h.join("Library/CloudStorage/Dropbox");
    std::fs::create_dir_all(&cloud).unwrap();
    assert_eq!(crate::default_root_in(h), cloud.join("paperstack"));
    // ~/Dropbox first, where it is.
    std::fs::create_dir(h.join("Dropbox")).unwrap();
    assert_eq!(crate::default_root_in(h), h.join("Dropbox/paperstack"));
    // A library made before Dropbox came is kept to.
    std::fs::create_dir(h.join("paperstack")).unwrap();
    std::fs::write(
        h.join("paperstack").join(crate::library::MARKER),
        "version = 1",
    )
    .unwrap();
    assert_eq!(crate::default_root_in(h), h.join("paperstack"));
}

/// The web with nothing in it.
fn offline() -> crate::net::Net {
    crate::net::Net::canned(&[])
}

fn record(index: &Index, key: &str) -> std::sync::Arc<crate::record::Record> {
    index.get(key).unwrap().unwrap().record
}

/// A paper's record as doi.org gives it, under `doi`.
fn csl(doi: &str) -> serde_json::Value {
    serde_json::json!({
        "type": "article-journal",
        "title": "A fictional map of CT1 cells",
        "author": [{ "family": "Doe", "given": "Jane" }],
        "issued": { "date-parts": [[2026, 1, 2]] },
        "DOI": doi,
    })
}

/// A paper's page with its Highwire tags, under `doi`.
fn tagged_page(doi: &str) -> String {
    format!(
        r#"<html><head>
        <meta name="citation_title" content="A fictional map of CT1 cells">
        <meta name="citation_author" content="Doe, Jane">
        <meta name="citation_author" content="Roe, Richard">
        <meta name="citation_publication_date" content="2026/01/02">
        <meta name="citation_doi" content="{doi}">
        </head></html>"#
    )
}

fn adder<'a>(
    lib: &'a Library,
    index: &'a mut Index,
    net: &'a crate::net::Net,
) -> crate::add::Adder<'a> {
    crate::add::Adder {
        lib,
        index,
        net,
        options: crate::add::Options::default(),
    }
}

#[test]
fn reads_the_page_given_where_doi_org_does_not_know_its_doi() {
    let (_dir, lib, mut index) = library();
    let doi = "10.64898/2026.01.01.999901";
    let url = format!("https://www.biorxiv.org/content/{doi}v1.full");
    let net = crate::net::Net::canned(&[(&format!("page {url}"), &tagged_page(doi))]);
    let added = adder(&lib, &mut index, &net).add(&url).unwrap();
    let record = record(&index, &added.key);
    assert_eq!(record.title(), "A fictional map of CT1 cells");
    assert_eq!(record.doi(), Some(doi));
}

#[test]
fn reads_a_dois_page_where_doi_org_gives_no_record() {
    let (_dir, lib, mut index) = library();
    let doi = "10.5555/fict.2026.02";
    let page = format!("page https://doi.org/{doi}");
    let net = crate::net::Net::canned(&[(&page, &tagged_page(doi))]);
    let added = adder(&lib, &mut index, &net).add(doi).unwrap();
    let record = record(&index, &added.key);
    assert_eq!(record.title(), "A fictional map of CT1 cells");
    assert_eq!(record.doi(), Some(doi));
}

#[test]
fn reads_a_pages_tags_where_its_doi_is_not_known() {
    let doi = "10.5555/fict.2026.03";
    let url = "https://journal.example/article/3";
    let net = crate::net::Net::canned(&[(&format!("page {url}"), &tagged_page(doi))]);
    let found = crate::resolve::by_page(&net, url).unwrap();
    assert_eq!(found.title(), "A fictional map of CT1 cells");
    assert_eq!(found.doi(), Some(doi));
}

#[test]
fn keeps_pubmeds_record_where_its_doi_is_not_known() {
    let csl = csl("10.5555/fict.2026.04");
    let net = crate::net::Net::canned(&[(
        "json https://api.ncbi.nlm.nih.gov/lit/ctxp/v1/pubmed/",
        &csl.to_string(),
    )]);
    let found = crate::resolve::by_pmid(&net, "99999904").unwrap();
    assert_eq!(found.title(), "A fictional map of CT1 cells");
    assert_eq!(found.doi(), Some("10.5555/fict.2026.04"));
    assert_eq!(found.csl["PMID"], "99999904");
}

#[test]
fn files_a_paper_no_one_knows_with_its_pdf_titled_by_it() {
    let (_dir, lib, mut index) = library();
    let doi = "10.5555/fict.2026.05";
    let pdf = String::from_utf8(first_page("A Fictional Map of CT1 Cells")).unwrap();
    let net = crate::net::Net::canned(&[
        (
            &format!("json https://api.openalex.org/works/doi:{doi}"),
            r#"{ "best_oa_location": { "pdf_url": "https://repository.example/5.pdf" } }"#,
        ),
        ("pdf https://repository.example/5.pdf", &pdf),
    ]);
    let mut adder = adder(&lib, &mut index, &net);
    adder.options.pdf = true;
    let added = adder.add(doi).unwrap();
    assert!(added.has_pdf);
    let record = record(&index, &added.key);
    assert_eq!(record.title(), "A Fictional Map of CT1 Cells");
    assert_eq!(record.doi(), Some(doi));
    assert_eq!(record.paperstack.labels, ["unresolved"]);
}

#[test]
fn files_a_paper_no_one_knows_and_resolves_it_when_known() {
    let (_dir, lib, mut index) = library();
    let doi = "10.5555/fict.2026.06";
    let offline = offline();
    let first = adder(&lib, &mut index, &offline).add(doi).unwrap();
    assert!(!first.has_pdf);
    let placeholder = record(&index, &first.key);
    assert_eq!(placeholder.title(), doi);
    assert_eq!(placeholder.paperstack.labels, ["unresolved"]);

    let csl = csl(doi);
    let known =
        crate::net::Net::canned(&[(&format!("json https://doi.org/{doi}"), &csl.to_string())]);
    let again = adder(&lib, &mut index, &known).add(doi).unwrap();
    assert_eq!(again.key, first.key);
    assert_eq!(again.outcome, crate::add::Outcome::Resolved);
    let resolved = record(&index, &again.key);
    assert_eq!(resolved.title(), "A fictional map of CT1 cells");
    assert!(resolved.paperstack.labels.is_empty());
}

#[test]
fn attaches_to_an_unresolved_paper_the_pdf_naming_its_doi() {
    let (_dir, lib, mut index) = library();
    let doi = "10.5555/fict.2026.07";
    let offline = offline();
    let key = adder(&lib, &mut index, &offline).add(doi).unwrap().key;
    let body = [
        "We study what the title says, and report what we found, in some detail,",
        "with methods, data and results set out over the pages that follow this one.",
    ];
    let pdf = |head: &[&str]| tiny_pdf(&[head, &body[..]].concat());
    let other = pdf(&["A Fictional Map of CT1 Cells"]);
    let err = crate::add::attach(&lib, &mut index, &key, &other, "x.pdf", false);
    assert!(err.is_err());
    let doi_line = format!("https://doi.org/{doi}");
    let its = pdf(&["A Fictional Map of CT1 Cells", &doi_line]);
    let added = crate::add::attach(&lib, &mut index, &key, &its, "x.pdf", false).unwrap();
    assert!(added.has_pdf);
}

#[test]
fn attaches_to_a_page_unresolved_the_pdf_named_by_its_path() {
    let (_dir, lib, mut index) = library();
    let offline = offline();
    let url = "https://journal.example/article/8";
    let key = adder(&lib, &mut index, &offline).add(url).unwrap().key;
    let pdf = first_page("A Fictional Map of CT1 Cells");
    assert!(crate::add::attach(&lib, &mut index, &key, &pdf, "x.pdf", false).is_err());
    let added = crate::add::attach(&lib, &mut index, &key, &pdf, "x.pdf", true).unwrap();
    assert!(added.has_pdf);
}

#[test]
fn leaves_a_paper_here_alone_where_nothing_knows_its_doi() {
    let (_dir, lib, mut index) = library();
    let doi = "10.5555/demo.2017.01";
    let before = index.by_doi(doi).unwrap().unwrap().record;
    let offline = offline();
    let added = adder(&lib, &mut index, &offline).add(doi).unwrap();
    assert_eq!(added.key, before.key);
    assert_eq!(added.outcome, crate::add::Outcome::Known);
    let after = index.by_doi(doi).unwrap().unwrap().record;
    assert_eq!(after.csl, before.csl);
    assert_eq!(after.paperstack.labels, before.paperstack.labels);
    assert!(!after.is_unresolved());
}

#[test]
fn resolves_a_page_filed_unresolved_when_its_page_is_had() {
    let (_dir, lib, mut index) = library();
    let url = "https://journal.example/article/9";
    let offline = offline();
    let first = adder(&lib, &mut index, &offline).add(url).unwrap();
    let net =
        crate::net::Net::canned(&[(&format!("page {url}"), &tagged_page("10.5555/fict.2026.09"))]);
    let again = adder(&lib, &mut index, &net).add(url).unwrap();
    assert_eq!(again.key, first.key);
    assert_eq!(again.outcome, crate::add::Outcome::Resolved);
    let record = record(&index, &again.key);
    assert_eq!(record.title(), "A fictional map of CT1 cells");
    assert!(!record.is_unresolved());
}

#[test]
fn reads_an_arxiv_doi_on_a_page_as_arxivs() {
    let url = "https://journal.example/article/10";
    let atom = "<feed><entry><title>A Fictional Map of CT1 Cells</title>\
                <author><name>Jane Doe</name></author>\
                <published>2026-01-02T00:00:00Z</published></entry></feed>";
    let net = crate::net::Net::canned(&[
        (
            &format!("page {url}"),
            &tagged_page("10.48550/arXiv.2601.00010"),
        ),
        ("text https://export.arxiv.org/api/query", atom),
    ]);
    let found = crate::resolve::by_page(&net, url).unwrap();
    assert_eq!(found.arxiv.as_deref(), Some("2601.00010"));
}

#[test]
fn tells_why_a_pdf_is_not_an_unresolved_papers() {
    let (_dir, lib, mut index) = library();
    let offline = offline();
    let key = adder(&lib, &mut index, &offline)
        .add("10.5555/fict.2026.11")
        .unwrap()
        .key;
    let pdf = first_page("A Fictional Map of CT1 Cells");
    let err = crate::add::attach(&lib, &mut index, &key, &pdf, "x.pdf", false).unwrap_err();
    assert!(err.to_string().contains("does not name"), "{err}");
}

#[test]
fn fits_a_long_name_to_the_sidebar() {
    use crate::ui::fit;
    assert_eq!(fit("cancer · tumor", 20), "cancer · tumor");
    assert_eq!(fit("genome wide · wide association", 20), "genome wide");
    assert_eq!(fit("a very long collection name", 10), "a very lo…");
}

#[test]
fn takes_a_small_letter_shifted_as_its_capital() {
    use crate::app::View;
    use ratatui::crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
    let (_dir, lib, index) = library();
    let mut app = App::new(lib, index).unwrap();
    let sort = app.sort;
    // As a terminal saying keys with their modifiers may send `S`.
    app.key(KeyEvent::new(KeyCode::Char('s'), KeyModifiers::SHIFT));
    assert_ne!(app.sort, sort, "sorted, not starred");
    assert_eq!(app.view, View::List);
}

#[test]
fn takes_keys_as_terminals_say_them_with_all_their_modifiers() {
    use crate::app::Popup;
    use ratatui::crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
    let (_dir, lib, index) = library();
    let mut app = App::new(lib, index).unwrap();
    let key = |code, m| KeyEvent::new(code, m);
    app.key(key(KeyCode::Char('j'), KeyModifiers::NONE));
    let at = app.list.selected();
    let starred = app.current().unwrap().record.paperstack.starred;
    // ⌘S is the system's, not a star; ctrl-g in the list, nothing: not to
    // the first paper, nor unmarking all, as esc would.
    app.key(key(KeyCode::Char('s'), KeyModifiers::SUPER));
    assert_eq!(app.current().unwrap().record.paperstack.starred, starred);
    app.key(key(KeyCode::Char(' '), KeyModifiers::NONE));
    app.key(key(KeyCode::Char('k'), KeyModifiers::NONE));
    app.key(key(KeyCode::Char('g'), KeyModifiers::CONTROL));
    assert_eq!(app.list.selected(), at, "not to the first paper");
    assert_eq!(app.marked.len(), 1);
    // In a dialog, or typing a search, it cancels, as esc does.
    app.key(key(KeyCode::Char('?'), KeyModifiers::NONE));
    app.key(key(KeyCode::Char('g'), KeyModifiers::CONTROL));
    assert!(matches!(app.popup, Popup::None));
    app.key(key(KeyCode::Char('/'), KeyModifiers::NONE));
    app.key(key(KeyCode::Char('x'), KeyModifiers::NONE));
    app.key(key(KeyCode::Char('g'), KeyModifiers::CONTROL));
    assert!(!app.searching && app.query.is_empty());
    // ctrl-m is Enter, as terminals send it otherwise.
    app.key(key(KeyCode::Char('/'), KeyModifiers::NONE));
    assert!(app.searching);
    app.key(key(KeyCode::Char('m'), KeyModifiers::CONTROL));
    assert!(!app.searching);
    // The keys, on a small screen, scroll; any other key closes them.
    app.key(key(KeyCode::Char('?'), KeyModifiers::NONE));
    let s = screen(&mut app, 80, 24);
    assert!(s.contains("j k scroll"), "{s}");
    app.key(key(KeyCode::Char('j'), KeyModifiers::NONE));
    assert!(matches!(app.popup, Popup::Help(1)));
    app.key(key(KeyCode::Char('x'), KeyModifiers::NONE));
    assert!(matches!(app.popup, Popup::None));
}

#[test]
fn reads_a_time_with_its_offset_taken_off() {
    use crate::add::{rfc3339, secs_of};
    let z = secs_of("2026-10-19T16:00:00Z").unwrap();
    assert_eq!(rfc3339(z), "2026-10-19T16:00:00Z");
    assert_eq!(secs_of("2026-10-19T09:00:00-07:00"), Some(z));
    assert_eq!(secs_of("2026-10-19T17:30:00.123+01:30"), Some(z));
    assert_eq!(
        secs_of("2000-02-29T00:00:00Z").map(rfc3339).as_deref(),
        Some("2000-02-29T00:00:00Z")
    );
    assert_eq!(secs_of("Mon, 05 Mar 2021 10:00:00"), None);
}

#[test]
fn keeps_a_record_filed_under_its_name_in_nfd() {
    use unicode_normalization::UnicodeNormalization;
    for with_pdf in [false, true] {
        let (_dir, lib, mut index) = library();
        let mut r = demo::records().remove(0);
        r.csl["author"] = serde_json::json!([{ "family": "Ålundqvé", "given": "A" }]);
        r.key = "Alundqve2026-nf".to_string();
        // Filed by another machine, its accents stored apart.
        let nfc = layout::record(&r).to_string_lossy().to_string();
        let nfd = PathBuf::from(nfc.nfd().collect::<String>());
        assert_ne!(nfd, layout::record(&r));
        std::fs::create_dir_all(lib.root.join(&nfd).parent().unwrap()).unwrap();
        crate::library::write(&lib.root.join(&nfd), &serde_json::to_vec(&r).unwrap()).unwrap();
        if with_pdf {
            std::fs::write(lib.root.join(&nfd).with_extension("pdf"), b"%PDF-1.4").unwrap();
        }
        let done = index.refresh(&lib).unwrap();
        assert!(done.errors.is_empty(), "{:?}", done.errors);
        let was = index.get(&r.key).unwrap().unwrap().path;

        r.paperstack.folders.push("CT1".to_string());
        // Where the file system tells the two forms of its name apart, as
        // Linux's do, it is moved, and its old folders go; where it does
        // not, as APFS, it is written where it was.
        let moved = !lib.root.join(layout::record(&r)).exists();
        let saved = lib.save(&r, Some(&was)).unwrap();
        assert_eq!(saved != was, moved);
        if moved {
            assert!(
                !lib.root.join(&was).parent().unwrap().exists(),
                "its folder gone"
            );
        }
        let done = index.refresh(&lib).unwrap();
        assert!(done.errors.is_empty(), "{:?}", done.errors);
        assert_eq!(done.removed, usize::from(moved), "with_pdf={with_pdf}");
        let records = lib.walk().unwrap().records;
        let ours = records
            .iter()
            .filter(|f| f.path.to_string_lossy().contains(&r.key));
        assert_eq!(ours.count(), 1, "one record of it");
        let hit = index.get(&r.key).unwrap().unwrap();
        assert_eq!(hit.path, saved);
        assert!(hit.record.paperstack.folders.iter().any(|f| f == "CT1"));
        assert_eq!(hit.has_pdf, with_pdf);
    }
}

/// The title and journal fields of a record with these, as BibTeX.
fn bib_fields(title: &str, venue: &str) -> (String, String) {
    let mut r = demo::records().remove(0);
    r.csl["title"] = serde_json::json!(title);
    r.csl["container-title"] = serde_json::json!(venue);
    let bib = bibtex::entry(&r);
    let field = |name: &str| {
        let start = bib.find(&format!("  {name} = {{")).unwrap() + name.len() + 6;
        let end = bib[start..].find("},\n").unwrap();
        bib[start..start + end].to_string()
    };
    (field("title"), field("journal"))
}

#[test]
fn keeps_a_titles_case_through_citeproc() {
    // Sentence case: every capital is the authors', kept from being lowered.
    let (title, _) = bib_fields(
        "Perturb-seq maps GENE1 in phase X samples of Ndiaye's disease",
        "Ann Fict Med",
    );
    assert_eq!(
        title,
        "Perturb-seq maps {GENE1} in phase {X} samples of {Ndiaye's} disease"
    );
    // Title case: only what has capitals past its first letter is kept.
    let (title, _) = bib_fields("A Single-Cell Atlas of eQTLs in the Fictional Kingdom", "X");
    assert_eq!(
        title,
        "A Single-Cell Atlas of {eQTLs} in the Fictional Kingdom"
    );
    // Braced whole, as by hand, to keep its case: its braces kept, and what
    // is within them escaped.
    let (title, _) = bib_fields("{Imputation via matrix completion & more}", "X");
    assert_eq!(title, "{Imputation via matrix completion \\& more}");
    // Braced in part: the braces are BibTeX's, kept.
    let (title, _) = bib_fields("Bounding bias with the {W}-value", "X");
    assert_eq!(title, "Bounding bias with the {W}-value");
    // Math, as imported, is LaTeX's; a price is not math.
    let (title, _) = bib_fields("GENE1-$\\beta$ costs $5 in mice", "X");
    assert_eq!(title, "{GENE1-}$\\beta$ costs \\$5 in mice");
}

#[test]
fn turns_html_in_titles_into_latex() {
    let (title, _) = bib_fields(
        "A circuit drives CT1 <sup>+</sup> cells and <i>GENE1</i> &amp; <b>x</b><span>y</span>",
        "X",
    );
    assert_eq!(
        title,
        "A circuit drives {CT1} \\textsuperscript{+} cells and \\textit{{GENE1}} \\& \\textbf{x}y"
    );
}

#[test]
fn writes_journals_in_title_case() {
    for (stored, written) in [
        ("Nature methods", "Nature Methods"),
        (
            "The annals of applied statistics",
            "The Annals of Applied Statistics",
        ),
        ("PLoS computational biology", "PLoS Computational Biology"),
        (
            "Journal of fictional medicine",
            "Journal of Fictional Medicine",
        ),
        ("eLife", "eLife"),
        ("npj fictional medicine", "npj fictional medicine"),
        ("Ann Fict Med", "Ann Fict Med"),
    ] {
        assert_eq!(bib_fields("A title", stored).1, written);
    }
}

/// A server on the demo library, offline, and a call of its tools.
fn mcp_server() -> (tempfile::TempDir, crate::mcp::Server) {
    let (dir, lib, index) = library();
    let server = crate::mcp::Server {
        lib,
        index,
        net: offline(),
    };
    (dir, server)
}

fn mcp_call(
    server: &mut crate::mcp::Server,
    name: &str,
    args: serde_json::Value,
) -> serde_json::Value {
    server
        .answer(
            &serde_json::json!({ "jsonrpc": "2.0", "id": 1, "method": "tools/call",
            "params": { "name": name, "arguments": args } }),
        )
        .unwrap()["result"]
        .clone()
}

#[test]
fn cites_what_it_has_and_says_what_it_lacks() {
    let (dir, mut server) = mcp_server();
    let mut r = record_of(&server.lib, &server.index, "Whitlock2017-ex");
    r.paperstack.aliases.push("Old2017-aa".into());
    let path = server.index.get("Whitlock2017-ex").unwrap().unwrap().path;
    server.lib.save(&r, Some(&path)).unwrap();

    let cited = mcp_call(
        &mut server,
        "cite",
        serde_json::json!({ "keys": ["Whitlock2017-ex", "Nope2020-xx", "Old2017-aa"] }),
    );
    assert_eq!(cited["isError"], false, "{cited}");
    let bib = cited["content"][0]["text"].as_str().unwrap();
    assert!(bib.contains("@article{Whitlock2017-ex,"), "{bib}");
    // Under the key the manuscript cites, though it was merged.
    assert!(bib.contains("@article{Old2017-aa,"), "{bib}");
    let s = &cited["structuredContent"];
    assert_eq!(s["missing"], serde_json::json!(["Nope2020-xx"]));
    assert_eq!(
        s["merged"],
        serde_json::json!([{ "key": "Old2017-aa", "into": "Whitlock2017-ex" }])
    );

    let out = dir.path().join("refs.bib");
    let wrote = mcp_call(
        &mut server,
        "cite",
        serde_json::json!({ "query": "collection:mediation", "output": out }),
    );
    assert_eq!(wrote["isError"], false, "{wrote}");
    assert!(
        std::fs::read_to_string(&out)
            .unwrap()
            .contains("@article{Corbett2021-mr,")
    );
    let refused = mcp_call(
        &mut server,
        "cite",
        serde_json::json!({ "query": "collection:mediation", "output": dir.path().join("notes.md") }),
    );
    assert_eq!(refused["isError"], true, "a bibliography only");
    let lacking = mcp_call(
        &mut server,
        "cite",
        serde_json::json!({ "keys": ["Whitlock2017-ex", "Nope2020-xx"], "output": out }),
    );
    assert_eq!(lacking["isError"], true, "{lacking}");
    assert!(
        std::fs::read_to_string(&out)
            .unwrap()
            .contains("@article{Corbett2021-mr,"),
        "left as it was"
    );
}

#[test]
fn checks_a_manuscripts_keys() {
    let (dir, mut server) = mcp_server();
    let mut r = record_of(&server.lib, &server.index, "Whitlock2017-ex");
    r.paperstack.aliases.push("Old2017-aa".into());
    let path = server.index.get("Whitlock2017-ex").unwrap().unwrap().path;
    server.lib.save(&r, Some(&path)).unwrap();
    let text = dir.path().join("draft.rmd");
    std::fs::write(
        &text,
        "As shown [@Whitlock2017-ex; @Old2017-aa, p. 3] and @Nope2020-xx, @corbett2021-mr.\n\
         See \\@ref(fig:a) and @fig-b; write to a@b.com; \\citep{Corbett2021-mr,Whitlock2017-ex}.\n",
    )
    .unwrap();
    let checked = mcp_call(
        &mut server,
        "check_keys",
        serde_json::json!({ "path": text }),
    );
    let s = &checked["structuredContent"];
    assert_eq!(
        s["found"],
        serde_json::json!(["Whitlock2017-ex", "Corbett2021-mr"]),
        "{checked}"
    );
    // Pandoc tells keys by their case: one cited in another is told too.
    assert_eq!(
        s["merged"],
        serde_json::json!([
            { "key": "Old2017-aa", "into": "Whitlock2017-ex" },
            { "key": "corbett2021-mr", "into": "Corbett2021-mr" }
        ])
    );
    assert_eq!(s["missing"], serde_json::json!(["Nope2020-xx"]));
}

#[test]
fn updates_many_papers_at_once() {
    let (_dir, mut server) = mcp_server();
    let n = server
        .index
        .search(
            &Query::parse("collection:mediation"),
            Sort::Added,
            false,
            None,
        )
        .unwrap()
        .len();
    let done = mcp_call(
        &mut server,
        "update_paper",
        serde_json::json!({ "query": "collection:mediation", "add_collections": ["CT1"],
            "remove_collections": ["mediation"] }),
    );
    assert_eq!(done["structuredContent"]["updated"], n, "{done}");
    assert_eq!(keys(&server.index, "collection:CT1", Sort::Added).len(), n);
    assert!(keys(&server.index, "collection:mediation", Sort::Added).is_empty());

    let some = mcp_call(
        &mut server,
        "update_paper",
        serde_json::json!({ "keys": ["Whitlock2017-ex", "Nope2020-xx"], "add_labels": ["L1"] }),
    );
    assert_eq!(some["structuredContent"]["updated"], 1, "{some}");
    assert_eq!(
        some["structuredContent"]["missing"],
        serde_json::json!(["Nope2020-xx"])
    );

    let aliased = mcp_call(
        &mut server,
        "update_paper",
        serde_json::json!({ "key": "Whitlock2017-ex", "add_aliases": ["Old2017-aa"] }),
    );
    assert_eq!(aliased["isError"], false, "{aliased}");
    assert_eq!(
        server.index.get("Old2017-aa").unwrap().unwrap().record.key,
        "Whitlock2017-ex"
    );
    let taken = mcp_call(
        &mut server,
        "update_paper",
        serde_json::json!({ "key": "Whitlock2017-ex", "add_aliases": ["Corbett2021-mr"] }),
    );
    assert_eq!(taken["isError"], true, "another paper's key");
}

#[test]
fn counts_authors_past_the_first_few_in_a_search() {
    let (_dir, mut server) = mcp_server();
    let mut r = demo::records().remove(0);
    let names: Vec<serde_json::Value> = (0..30)
        .map(|i| serde_json::json!({ "family": format!("Name{i}"), "given": "A" }))
        .collect();
    r.csl["author"] = serde_json::Value::Array(names);
    let path = server.index.get(&r.key).unwrap().unwrap().path;
    server.lib.save(&r, Some(&path)).unwrap();
    let found = mcp_call(
        &mut server,
        "search_papers",
        serde_json::json!({ "query": format!("key:{}", r.key) }),
    );
    let paper = &found["structuredContent"]["papers"][0];
    assert_eq!(paper["authors"].as_array().unwrap().len(), 10, "{found}");
    assert_eq!(paper["authors_total"], 30);
    let whole = mcp_call(
        &mut server,
        "get_paper",
        serde_json::json!({ "key": r.key }),
    );
    assert_eq!(
        whole["structuredContent"]["authors"]
            .as_array()
            .unwrap()
            .len(),
        30
    );
}

#[test]
fn fetches_open_copies_for_many_papers_over_mcp() {
    let (_dir, mut server) = mcp_server();
    let done = mcp_call(
        &mut server,
        "attach_pdf",
        serde_json::json!({ "keys": ["Whitlock2017-ex", "Nope2020-xx"] }),
    );
    assert_eq!(done["isError"], false, "{done}");
    let papers = done["structuredContent"]["papers"].as_array().unwrap();
    assert_eq!(papers.len(), 1);
    assert_eq!(papers[0]["key"], "Whitlock2017-ex");
    assert_eq!(papers[0]["reason"], "no-open-copy", "{done}");
    assert_eq!(
        done["structuredContent"]["missing"],
        serde_json::json!(["Nope2020-xx"])
    );
}

/// A first page with a preprint server's stamp on it.
fn preprint_page(title: &str) -> Vec<u8> {
    tiny_pdf(&[
        "bioRxiv preprint doi: https://doi.org/10.1101/2026.01.01.000001",
        title,
        "A. Author, B. Author",
        "We study what the title says, and report what we found, in some detail,",
        "with methods, data and results set out over the pages that follow this one.",
    ])
}

#[test]
fn prefers_the_published_copy_and_says_why_others_failed() {
    use crate::record::Version;
    let (_dir, lib, mut index) = library();
    let hit = index.get("Whitlock2017-ex").unwrap().unwrap();
    let (title, doi) = (
        hit.record.title().to_string(),
        hit.record.doi().unwrap().to_string(),
    );
    let (pre, blocked, page, published) = (
        "https://www.biorxiv.org/content/x.full.pdf",
        "https://journal.example/blocked.pdf",
        "https://journal.example/page.pdf",
        "https://repository.example/published.pdf",
    );
    let work = serde_json::json!({
        "best_oa_location": { "pdf_url": pre, "version": "submittedVersion" },
        "locations": [
            { "pdf_url": pre, "version": "submittedVersion" },
            { "pdf_url": blocked, "version": "publishedVersion" },
            { "pdf_url": page, "version": "publishedVersion" },
            { "pdf_url": published, "version": "publishedVersion" },
        ]
    })
    .to_string();
    let pre_pdf = String::from_utf8(preprint_page(&title)).unwrap();
    let pub_pdf = String::from_utf8(first_page(&title)).unwrap();
    let alex = format!("json https://api.openalex.org/works/doi:{doi}");
    let answers = |published_ok: bool| {
        let mut a = vec![
            (alex.clone(), work.clone()),
            (format!("pdf {pre}"), pre_pdf.clone()),
            (format!("pdf {blocked}"), "!status 403".to_string()),
            (format!("pdf {page}"), "<html>".to_string()),
        ];
        let at_published = if published_ok {
            pub_pdf.clone()
        } else {
            "!status 404".to_string()
        };
        a.push((format!("pdf {published}"), at_published));
        a
    };
    let canned = |a: &[(String, String)]| {
        let a: Vec<(&str, &str)> = a.iter().map(|(k, v)| (k.as_str(), v.as_str())).collect();
        crate::net::Net::canned(&a)
    };
    let quiet = |_: &str| true;

    // Published copies first: the preprint only once they fail.
    let net = canned(&answers(false));
    let got = crate::add::open_copy(&lib, &mut index, &net, &hit, false, &quiet).unwrap();
    assert!(got.how.contains(pre), "{}", got.how);
    let r = record_of(&lib, &index, "Whitlock2017-ex");
    assert_eq!(r.paperstack.pdf_version, Some(Version::Preprint));

    // Asked for the published copy: none is had, and each failure is told.
    let hit = index.get("Whitlock2017-ex").unwrap().unwrap();
    let err = crate::add::open_copy(&lib, &mut index, &net, &hit, true, &quiet).unwrap_err();
    let none = err.downcast_ref::<crate::add::NoOpenCopy>().expect("why");
    let reasons: Vec<&str> = none.tried.iter().map(|t| t.reason.as_str()).collect();
    assert_eq!(
        reasons,
        ["http-403", "html-page", "http-404", "preprint"],
        "{err:#}"
    );
    assert!(
        index.get("Whitlock2017-ex").unwrap().unwrap().has_pdf,
        "kept"
    );

    // Once there is one, it takes the preprint's place, which is trashed.
    let net = canned(&answers(true));
    let got = crate::add::open_copy(&lib, &mut index, &net, &hit, true, &quiet).unwrap();
    assert!(got.how.contains(published), "{}", got.how);
    let r = record_of(&lib, &index, "Whitlock2017-ex");
    assert_eq!(r.paperstack.pdf_version, Some(Version::Published));
    assert!(
        lib.root
            .join(crate::remove::TRASH)
            .join(&hit.path)
            .with_extension("pdf")
            .exists()
    );
}

#[test]
fn takes_query_words_that_leave_out() {
    use clap::Parser;
    let parse = |words: &[&str]| {
        crate::Args::try_parse_from(crate::args(words.iter().map(|w| w.into()))).unwrap()
    };
    let args = parse(&[
        "paperstack",
        "find",
        "collection:\"CT1\"",
        "-has:pdf",
        "--bib",
    ]);
    let Some(crate::Command::Find {
        query, not, format, ..
    }) = args.command
    else {
        panic!("not find");
    };
    assert_eq!(
        (query, not),
        (
            vec!["collection:\"CT1\"".to_string()],
            vec!["has:pdf".to_string()]
        )
    );
    assert!(format.bib);
    let args = parse(&[
        "paperstack",
        "-L",
        "find",
        "find",
        "-author:Doe",
        "x",
        "--limit",
        "3",
    ]);
    assert!(matches!(
        args.command,
        Some(crate::Command::Find { not, limit: Some(3), .. }) if not == ["author:Doe"]
    ));
}

#[test]
fn attaches_to_many_papers_without_the_browser() {
    use clap::Parser;
    let args = crate::Args::try_parse_from([
        "paperstack",
        "attach",
        "--query",
        "collection:CT1",
        "--no-browser",
        "--published",
    ])
    .unwrap();
    assert!(matches!(
        args.command,
        Some(crate::Command::Attach {
            no_browser: true,
            published: true,
            query: Some(_),
            ..
        })
    ));
    assert!(crate::Args::try_parse_from(["paperstack", "attach"]).is_err());
}

#[test]
fn keeps_proper_nouns_of_a_title_in_title_case_by_its_abstract() {
    let mut r = demo::records().remove(0);
    r.csl["title"] =
        serde_json::json!("Consistent Estimation in Ndiaye Randomization Using a Median");
    r.csl["abstract"] = serde_json::json!(
        "Applications of Ndiaye randomization grow. Consistent estimates need valid \
         instruments. We use a median, and the Ndiaye method."
    );
    let bib = bibtex::entry(&r);
    assert!(
        bib.contains("title = {Consistent Estimation in {Ndiaye} Randomization Using a Median}"),
        "{bib}"
    );
}

/// A server at 127.0.0.1 that gives these answers, one a request, in turn:
/// its address.
fn serve(answers: Vec<Vec<u8>>) -> String {
    use std::io::{Read, Write};
    let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
    let addr = listener.local_addr().unwrap();
    std::thread::spawn(move || {
        for answer in answers {
            let (mut stream, _) = listener.accept().unwrap();
            let mut buf = [0u8; 4096];
            let _ = stream.read(&mut buf);
            stream.write_all(&answer).unwrap();
        }
    });
    format!("http://{addr}/x.pdf")
}

fn http(status: &str, headers: &str, body: &[u8]) -> Vec<u8> {
    let mut out = format!(
        "HTTP/1.1 {status}\r\n{headers}Content-Length: {}\r\nConnection: close\r\n\r\n",
        body.len()
    )
    .into_bytes();
    out.extend_from_slice(body);
    out
}

#[test]
fn asks_again_a_server_that_asks_for_fewer_requests() {
    use crate::net::{Refused, Why};
    let net = crate::net::Net::new(None);
    let url = serve(vec![
        http("429 Too Many Requests", "Retry-After: 0\r\n", b""),
        http("200 OK", "Content-Type: application/pdf\r\n", b"%PDF-1.4 x"),
    ]);
    assert_eq!(net.pdf(&url).unwrap(), b"%PDF-1.4 x");
    let url = serve(vec![http(
        "429 Too Many Requests",
        "Retry-After: 3600\r\n",
        b"",
    )]);
    let asked = std::time::Instant::now();
    let e = net.pdf(&url).unwrap_err();
    assert!(matches!(
        e.downcast_ref::<Refused>().unwrap().why,
        Why::Status(429)
    ));
    assert!(
        asked.elapsed() < std::time::Duration::from_secs(5),
        "not waited for"
    );
    // Nor asked again meanwhile: it would not answer.
    let e = net.pdf(&url).unwrap_err();
    assert!(matches!(
        e.downcast_ref::<Refused>().unwrap().why,
        Why::Status(429)
    ));

    let url = serve(vec![http("403 Forbidden", "", b"no")]);
    let e = net.pdf(&url).unwrap_err();
    assert!(matches!(
        e.downcast_ref::<Refused>().unwrap().why,
        Why::Status(403)
    ));
    let url = serve(vec![http(
        "200 OK",
        "Content-Type: text/html\r\n",
        b"<html>",
    )]);
    let e = net.pdf(&url).unwrap_err();
    assert!(matches!(
        e.downcast_ref::<Refused>().unwrap().why,
        Why::Page
    ));
}

#[test]
fn tells_a_preprints_stamp_from_a_mention_of_one() {
    use crate::pdf::preprint_stamp;
    assert!(preprint_stamp(
        "bioRxiv preprint doi: https://doi.org/10.1101/2026.01.01.000001; this version posted"
    ));
    assert!(preprint_stamp("arXiv:2101.00001v2 [cs.LG] 3 Jan 2021"));
    assert!(!preprint_stamp(
        "A preprint is available at Research Square"
    ));
    assert!(!preprint_stamp("as in ref. 12, arXiv:1706.03762 [cs.CL]."));
    assert!(!preprint_stamp(
        "posted as a bioRxiv preprint before review"
    ));
}

#[test]
fn asks_no_published_copy_of_a_paper_filed_as_its_preprint() {
    let (_dir, lib, mut index) = library();
    let hit = index.get("SCM-team2024-lc").unwrap().unwrap();
    assert_eq!(hit.record.kind(), "Preprint");
    let e = crate::add::open_copy(&lib, &mut index, &offline(), &hit, true, &|_| true).unwrap_err();
    assert!(e.to_string().contains("published DOI"), "{e:#}");
}

#[test]
fn tells_a_pdfs_path_from_a_key() {
    let w = |a: &[&str]| a.iter().map(|s| s.to_string()).collect::<Vec<_>>();
    assert!(crate::pdf_given(&w(&["K2025-ab", "~/Downloads/papr.pdf"])).is_some());
    assert!(crate::pdf_given(&w(&["K2025-ab", "https://x.example/p"])).is_some());
    assert!(crate::pdf_given(&w(&["K2025-ab", "L2024-cd"])).is_none());
    assert!(crate::pdf_given(&w(&["K2025-ab"])).is_none());
}

#[test]
fn keeps_a_name_with_a_particle_whole() {
    let mut r = demo::records().remove(0);
    r.csl["author"] = serde_json::json!([
        { "family": "van der Ndiaye", "given": "A B" },
        { "family": "Ndiaye", "non-dropping-particle": "de", "given": "C" },
        { "family": "Okafor", "non-dropping-particle": "d'", "given": "E" },
        { "family": "Okafor", "given": "D" }
    ]);
    let bib = bibtex::entry(&r);
    assert!(
        bib.contains(
            "author = {{van der Ndiaye}, A B and {de Ndiaye}, C and {d'Okafor}, E and Okafor, D}"
        ),
        "{bib}"
    );
}

#[test]
fn follows_a_landing_page_to_its_pdf_and_says_what_it_passed_over() {
    let (_dir, lib, mut index) = library();
    let hit = index.get("Whitlock2017-ex").unwrap().unwrap();
    let (title, doi) = (
        hit.record.title().to_string(),
        hit.record.doi().unwrap().to_string(),
    );
    let (landing, file, pre) = (
        "https://repository.example/handle/1",
        "https://repository.example/bitstream/1.pdf",
        "https://www.biorxiv.org/content/x.full.pdf",
    );
    let work = serde_json::json!({
        "locations": [
            { "pdf_url": pre, "version": "acceptedVersion" },
            { "pdf_url": null, "landing_page_url": "https://pubmed.ncbi.nlm.nih.gov/1", "is_oa": true },
            { "pdf_url": null, "landing_page_url": "https://closed.example/1", "is_oa": false },
            { "pdf_url": null, "landing_page_url": landing, "is_oa": true },
        ]
    })
    .to_string();
    // Named from the page's own folder.
    let page =
        r#"<html><head><meta name="citation_pdf_url" content="../bitstream/1.pdf"></head></html>"#;
    let pdf = String::from_utf8(first_page(&title)).unwrap();
    let alex = format!("json https://api.openalex.org/works/doi:{doi}");
    let answers = [
        (alex.as_str(), work.as_str()),
        (&format!("pdf {landing}"), page),
        (&format!("pdf {file}"), pdf.as_str()),
    ];
    let net = crate::net::Net::canned(&answers);
    // A repository's open copy, by the PDF its page names, told by its own
    // address; no closed copy, nor PubMed's page, asked for.
    let got = crate::add::open_copy(&lib, &mut index, &net, &hit, true, &|_| true).unwrap();
    assert!(got.how.contains(file), "{}", got.how);

    // With none to be had, the preprint passed over is told.
    let hit = index.get("Corbett2021-mr").unwrap().unwrap();
    let doi = hit.record.doi().unwrap().to_string();
    let work = serde_json::json!({ "locations": [{ "pdf_url": pre }] }).to_string();
    let alex = format!("json https://api.openalex.org/works/doi:{doi}");
    let net = crate::net::Net::canned(&[(alex.as_str(), work.as_str())]);
    let e = crate::add::open_copy(&lib, &mut index, &net, &hit, true, &|_| true).unwrap_err();
    let none = e.downcast_ref::<crate::add::NoOpenCopy>().expect("why");
    assert_eq!(none.tried.len(), 1, "{e:#}");
    assert_eq!(
        (none.tried[0].url.as_str(), none.tried[0].reason.as_str()),
        (pre, "preprint")
    );
}

#[test]
fn makes_an_address_a_page_names_whole() {
    use crate::resolve::absolute;
    let base = "https://repo.example/items/7/view?x=1";
    for (href, whole) in [
        ("https://cdn.example/a.pdf", "https://cdn.example/a.pdf"),
        ("//cdn.example/a.pdf", "https://cdn.example/a.pdf"),
        ("/bitstream/a.pdf", "https://repo.example/bitstream/a.pdf"),
        ("files/a.pdf", "https://repo.example/items/7/files/a.pdf"),
        (
            "httpdocs/a.pdf",
            "https://repo.example/items/7/httpdocs/a.pdf",
        ),
        ("../../b/./a.pdf", "https://repo.example/b/a.pdf"),
    ] {
        assert_eq!(absolute(href, base).as_deref(), Some(whole), "{href}");
    }
}

#[test]
fn keeps_a_bibliography_up_to_date() {
    let (dir, mut server) = mcp_server();
    let out = dir.path().join("refs.bib");
    let kept = mcp_call(
        &mut server,
        "cite",
        serde_json::json!({ "query": "collection:CT1", "output": out, "keep": true }),
    );
    assert_eq!(
        kept["isError"], true,
        "a query finding nothing is not kept: {kept}"
    );
    let kept = mcp_call(
        &mut server,
        "cite",
        serde_json::json!({ "query": "collection:mediation", "output": out, "keep": true }),
    );
    assert_eq!(kept["isError"], false, "{kept}");
    assert!(
        !std::fs::read_to_string(&out)
            .unwrap()
            .contains("Nakamura2016-al")
    );
    // A paper put in the collection is in the file once the change is done.
    mcp_call(
        &mut server,
        "update_paper",
        serde_json::json!({ "key": "Nakamura2016-al", "add_collections": ["mediation"] }),
    );
    assert!(
        std::fs::read_to_string(&out)
            .unwrap()
            .contains("@article{Nakamura2016-al,")
    );
    // Kept by keys, one deleted: the file is left as it was, not cut short.
    let keys_out = dir.path().join("keys.bib");
    mcp_call(
        &mut server,
        "cite",
        serde_json::json!({ "keys": ["Whitlock2017-ex", "Corbett2021-mr"], "output": keys_out, "keep": true }),
    );
    mcp_call(
        &mut server,
        "delete_papers",
        serde_json::json!({ "keys": ["Corbett2021-mr"] }),
    );
    assert!(
        std::fs::read_to_string(&keys_out)
            .unwrap()
            .contains("Corbett2021-mr")
    );
    let listed = crate::kept::list(&server.index);
    assert_eq!(listed.len(), 2);
    assert!(crate::kept::forget(&server.index, &out).unwrap());
    assert_eq!(crate::kept::list(&server.index).len(), 1);
}

#[test]
fn restores_a_paper_and_a_pdf_from_the_trash() {
    let (_dir, lib, mut index) = library();
    let key = "Kowalczyk2025-zs";
    let title = index.get(key).unwrap().unwrap().record.title().to_string();
    crate::add::attach(&lib, &mut index, key, &first_page(&title), "x.pdf", false).unwrap();
    let path = index.get(key).unwrap().unwrap().path;
    crate::remove::paper(&lib, &mut index, key).unwrap();
    assert!(index.get(key).unwrap().is_none());
    let back = crate::remove::restore(&lib, &mut index, key).unwrap();
    assert_eq!(back, path);
    let hit = index.get(key).unwrap().unwrap();
    assert!(hit.has_pdf, "with its PDF");
    assert!(!lib.root.join(crate::remove::TRASH).join(&path).exists());
    assert!(
        crate::remove::restore(&lib, &mut index, key).is_err(),
        "here already"
    );

    crate::remove::pdf(&lib, &mut index, key).unwrap();
    assert!(!index.get(key).unwrap().unwrap().has_pdf);
    crate::remove::restore_pdf(&lib, &mut index, key).unwrap();
    let hit = index.get(key).unwrap().unwrap();
    assert!(hit.has_pdf);
    assert!(record_of(&lib, &index, key).paperstack.pdf_sha256.is_some());
}

#[test]
fn reads_a_paper_again_from_its_doi_keeping_what_is_ours() {
    let (_dir, lib, mut index) = library();
    let hit = index.get("Whitlock2017-ex").unwrap().unwrap();
    let doi = hit.record.doi().unwrap().to_string();
    let mut fresh = hit.record.csl.clone();
    fresh["page"] = serde_json::json!("R89-R98");
    fresh["volume"] = serde_json::json!("50");
    fresh.as_object_mut().unwrap().remove("abstract");
    let net =
        crate::net::Net::canned(&[(&format!("json https://doi.org/{doi}"), &fresh.to_string())]);
    let folders = hit.record.paperstack.folders.clone();

    let seen = crate::add::refresh(&lib, &mut index, &net, &hit, true).unwrap();
    let fields: Vec<&str> = seen.iter().map(|c| c.field.as_str()).collect();
    assert_eq!(
        fields,
        ["page", "volume"],
        "the abstract kept, so not changed"
    );
    assert_ne!(
        record_of(&lib, &index, "Whitlock2017-ex").csl["page"],
        "R89-R98",
        "a dry run"
    );

    crate::add::refresh(&lib, &mut index, &net, &hit, false).unwrap();
    let r = record_of(&lib, &index, "Whitlock2017-ex");
    assert_eq!(
        (r.csl["page"].as_str(), r.csl["volume"].as_str()),
        (Some("R89-R98"), Some("50"))
    );
    assert!(r.abstract_text().is_some());
    assert_eq!(r.paperstack.folders, folders);
}

#[test]
fn sets_a_records_fields_by_hand() {
    let (_dir, lib, mut index) = library();
    let key = "Whitlock2017-ex";
    crate::add::set(
        &lib,
        &mut index,
        key,
        &[
            (
                "title".to_string(),
                Some(serde_json::json!("{Memory Mosaics}")),
            ),
            ("note".to_string(), None),
            ("page".to_string(), Some(serde_json::json!("R89-R98"))),
        ],
        &[],
    )
    .unwrap();
    let r = record_of(&lib, &index, key);
    assert_eq!(r.csl["page"], "R89-R98");
    assert!(bibtex::entry(&r).contains("title = {{Memory Mosaics}}"));
    assert!(crate::add::set(&lib, &mut index, key, &[("id".to_string(), None)], &[]).is_err());
}

#[test]
fn tidies_titles_braced_whole() {
    let (_dir, lib, mut index) = library();
    let key = "Whitlock2017-ex";
    let mut r = record_of(&lib, &index, key);
    let title = r.title().to_string();
    r.csl["title"] = serde_json::json!(format!("{{{title}}}"));
    r.paperstack.source = Some("import".to_string());
    let path = index.get(key).unwrap().unwrap().path;
    lib.save(&r, Some(&path)).unwrap();
    index.refresh(&lib).unwrap();
    assert_eq!(crate::add::tidy(&lib, &mut index, true).unwrap(), [key]);
    assert_ne!(
        record_of(&lib, &index, key).csl["title"],
        title.as_str(),
        "a dry run"
    );
    assert_eq!(crate::add::tidy(&lib, &mut index, false).unwrap(), [key]);
    assert_eq!(record_of(&lib, &index, key).csl["title"], title.as_str());
    assert!(
        crate::add::tidy(&lib, &mut index, false)
            .unwrap()
            .is_empty()
    );
}

#[test]
fn says_where_to_get_a_paper_none_gives() {
    let (_dir, lib, mut index) = library();
    let hit = index.get("Whitlock2017-ex").unwrap().unwrap();
    let e =
        crate::add::open_copy(&lib, &mut index, &offline(), &hit, false, &|_| true).unwrap_err();
    let none = e.downcast_ref::<crate::add::NoOpenCopy>().unwrap();
    assert!(
        none.page.contains(hit.record.doi().unwrap()),
        "{}",
        none.page
    );
    assert!(crate::cli::failed_json(&e, "x")["page"].as_str().is_some());
}

#[test]
fn sets_fields_over_mcp_once_all_is_checked() {
    let (_dir, mut server) = mcp_server();
    let key = "Whitlock2017-ex";
    // Its title changed, its record moves: set and saved at once.
    let set = mcp_call(
        &mut server,
        "update_paper",
        serde_json::json!({ "key": key, "set": { "title": "A wholly new title", "page": "1-2" },
            "add_labels": ["L1"] }),
    );
    assert_eq!(set["isError"], false, "{set}");
    let r = record_of(&server.lib, &server.index, key);
    assert_eq!(
        (r.title(), r.csl["page"].as_str()),
        ("A wholly new title", Some("1-2"))
    );
    assert!(r.paperstack.labels.contains(&"L1".to_string()));
    // An alias another's: nothing changes.
    let refused = mcp_call(
        &mut server,
        "update_paper",
        serde_json::json!({ "key": key, "set": { "page": "9" }, "add_aliases": ["Corbett2021-mr"] }),
    );
    assert_eq!(refused["isError"], true);
    assert_eq!(
        record_of(&server.lib, &server.index, key).csl["page"],
        "1-2"
    );
}

#[test]
fn keeps_what_was_set_by_hand_through_refresh_and_tidy() {
    let (_dir, lib, mut index) = library();
    let key = "Whitlock2017-ex";
    let hit = index.get(key).unwrap().unwrap();
    let doi = hit.record.doi().unwrap().to_string();
    crate::add::set(
        &lib,
        &mut index,
        key,
        &[
            (
                "title".to_string(),
                Some(serde_json::json!("{Bounding Hidden Bias}")),
            ),
            ("page".to_string(), Some(serde_json::json!("R89-R98"))),
            ("note".to_string(), Some(serde_json::json!("ours"))),
        ],
        &[],
    )
    .unwrap();
    let mut fresh = hit.record.csl.clone();
    fresh["page"] = serde_json::json!("wrong");
    fresh["volume"] = serde_json::json!("50");
    let net =
        crate::net::Net::canned(&[(&format!("json https://doi.org/{doi}"), &fresh.to_string())]);
    let hit = index.get(key).unwrap().unwrap();
    let changed = crate::add::refresh(&lib, &mut index, &net, &hit, false).unwrap();
    assert_eq!(
        changed.iter().map(|c| c.field.as_str()).collect::<Vec<_>>(),
        ["volume"]
    );
    let r = record_of(&lib, &index, key);
    assert_eq!(r.csl["page"], "R89-R98", "as set by hand");
    assert_eq!(r.csl["note"], "ours", "the source gives none");
    assert_eq!(r.csl["title"], "{Bounding Hidden Bias}");
    assert!(
        crate::add::tidy(&lib, &mut index, false)
            .unwrap()
            .is_empty(),
        "braced by hand"
    );
}

#[test]
fn reads_an_arxiv_paper_again_as_itself() {
    let (_dir, lib, mut index) = library();
    let hit = index.get("SCM-team2024-lc").unwrap().unwrap();
    let id = hit.record.paperstack.arxiv.clone().unwrap();
    let atom = format!(
        "<feed><entry><title>{}</title><author><name>Jane Doe</name></author>\
         <published>2024-12-01T00:00:00Z</published><arxiv:doi>10.5555/published.1</arxiv:doi></entry></feed>",
        hit.record.title()
    );
    let journal = serde_json::json!({ "type": "article-journal", "title": "The published one",
        "DOI": "10.5555/published.1" })
    .to_string();
    let net = crate::net::Net::canned(&[
        ("text https://export.arxiv.org/api/query", atom.as_str()),
        ("json https://doi.org/10.5555/published.1", journal.as_str()),
    ]);
    crate::add::refresh(&lib, &mut index, &net, &hit, false).unwrap();
    let r = record_of(&lib, &index, "SCM-team2024-lc");
    assert_eq!(
        r.title(),
        hit.record.title(),
        "not the paper it was published as"
    );
    assert_eq!(r.paperstack.arxiv.as_deref(), Some(id.as_str()));
}

#[test]
fn writes_a_bibliography_only_as_it_is_asked() {
    use clap::Parser;
    let parse = |w: &[&str]| crate::Args::try_parse_from(crate::args(w.iter().map(|w| w.into())));
    assert!(
        parse(&[
            "paperstack",
            "find",
            "x",
            "--output",
            "a.bib",
            "--limit",
            "5"
        ])
        .is_err()
    );
    assert!(parse(&["paperstack", "find", "x", "--output", "a.bib", "--keys"]).is_err());
    assert!(parse(&["paperstack", "find", "x", "--output", "a.bib", "--bib"]).is_ok());

    let (dir, mut server) = mcp_server();
    let out = dir.path().join("one.bib");
    let one = mcp_call(
        &mut server,
        "cite",
        serde_json::json!({ "key": "Whitlock2017-ex", "output": out }),
    );
    assert_eq!(one["isError"], false, "{one}");
    assert_eq!(one["structuredContent"]["entries"], 1);
}

#[test]
fn restores_no_pdf_it_would_have_to_guess() {
    let (_dir, lib, mut index) = library();
    let key = "Kowalczyk2025-zs";
    let path = index.get(key).unwrap().unwrap().path;
    let trash = lib.root.join(crate::remove::TRASH);
    let elsewhere = trash
        .join("2020/Else/A")
        .join(path.file_name().unwrap())
        .with_extension("pdf");
    let other = trash
        .join("2021/Else/B")
        .join(path.file_name().unwrap())
        .with_extension("pdf");
    for p in [&elsewhere, &other] {
        std::fs::create_dir_all(p.parent().unwrap()).unwrap();
        std::fs::write(p, b"%PDF-1.4").unwrap();
    }
    let e = crate::remove::restore_pdf(&lib, &mut index, key).unwrap_err();
    assert!(e.to_string().contains("more than one"), "{e}");
    // The one trashed where the paper is filed is the one meant.
    let here = trash.join(&path).with_extension("pdf");
    std::fs::create_dir_all(here.parent().unwrap()).unwrap();
    std::fs::write(&here, b"%PDF-1.4").unwrap();
    crate::remove::restore_pdf(&lib, &mut index, key).unwrap();
    assert!(index.get(key).unwrap().unwrap().has_pdf);
}

#[test]
fn keeps_what_was_set_by_hand_when_a_preprint_is_published() {
    let (_dir, lib, mut index) = library();
    let key = "SCM-team2024-lc";
    let hit = index.get(key).unwrap().unwrap();
    crate::add::set(
        &lib,
        &mut index,
        key,
        &[("note".to_string(), Some(serde_json::json!("ours")))],
        &[],
    )
    .unwrap();
    let published = serde_json::json!({
        "type": "article-journal", "title": hit.record.title(), "DOI": "10.5555/published.2",
        "author": hit.record.csl["author"], "issued": hit.record.csl["issued"],
        "container-title": "Fict Rev"
    })
    .to_string();
    let net = crate::net::Net::canned(&[(
        "json https://doi.org/10.5555/published.2",
        published.as_str(),
    )]);
    let added = adder(&lib, &mut index, &net)
        .add("10.5555/published.2")
        .unwrap();
    assert_eq!(added.key, key);
    assert_eq!(added.outcome, crate::add::Outcome::Published);
    let r = record_of(&lib, &index, key);
    assert_eq!(
        (r.doi(), r.csl["note"].as_str()),
        (Some("10.5555/published.2"), Some("ours"))
    );
}

#[test]
fn writes_a_bibliography_into_a_folder_not_there_yet() {
    let (dir, mut server) = mcp_server();
    let out = dir.path().join("new/folder/refs.bib");
    let wrote = mcp_call(
        &mut server,
        "cite",
        serde_json::json!({ "query": "collection:mediation", "output": out }),
    );
    assert_eq!(wrote["isError"], false, "{wrote}");
    assert!(out.is_file());
}

#[test]
fn checks_a_text_citing_nothing() {
    let (dir, mut server) = mcp_server();
    let text = dir.path().join("mail.md");
    std::fs::write(&text, "write to a@b.com\n").unwrap();
    let checked = mcp_call(
        &mut server,
        "check_keys",
        serde_json::json!({ "path": text }),
    );
    assert_eq!(checked["isError"], false, "{checked}");
    assert_eq!(checked["structuredContent"]["found"], serde_json::json!([]));
}

#[test]
fn gives_a_field_back_to_its_source_when_asked() {
    let (_dir, mut server) = mcp_server();
    let key = "Whitlock2017-ex";
    let set = mcp_call(
        &mut server,
        "update_paper",
        serde_json::json!({ "key": key, "set": { "note": "test" } }),
    );
    assert_eq!(set["structuredContent"]["set"]["note"], "test", "{set}");
    mcp_call(
        &mut server,
        "update_paper",
        serde_json::json!({ "key": key, "set": { "note": null } }),
    );
    let r = record_of(&server.lib, &server.index, key);
    assert!(r.csl.get("note").is_none());
    assert_eq!(r.paperstack.hand, ["note"], "taken away, and kept away");
    mcp_call(
        &mut server,
        "update_paper",
        serde_json::json!({ "key": key, "reset": ["note"] }),
    );
    let r = record_of(&server.lib, &server.index, key);
    assert!(
        r.paperstack.hand.is_empty(),
        "the source's again: {:?}",
        r.paperstack.hand
    );
}

#[test]
fn says_what_a_refresh_would_change() {
    let (_dir, lib, mut index) = library();
    let hit = index.get("Whitlock2017-ex").unwrap().unwrap();
    let doi = hit.record.doi().unwrap().to_string();
    let mut fresh = hit.record.csl.clone();
    fresh["page"] = serde_json::json!("R89-R98");
    let net =
        crate::net::Net::canned(&[(&format!("json https://doi.org/{doi}"), &fresh.to_string())]);
    let seen = crate::add::refresh(&lib, &mut index, &net, &hit, true).unwrap();
    assert_eq!(seen.len(), 1);
    assert_eq!(seen[0].field, "page");
    assert_eq!(seen[0].new, Some(serde_json::json!("R89-R98")));
}

#[test]
fn tells_copies_turned_away_from_none_known() {
    let (_dir, lib, mut index) = library();
    let hit = index.get("Whitlock2017-ex").unwrap().unwrap();
    let doi = hit.record.doi().unwrap().to_string();
    let blocked = "https://journal.example/blocked.pdf";
    let work =
        serde_json::json!({ "locations": [{ "pdf_url": blocked, "version": "publishedVersion" }] })
            .to_string();
    let net = crate::net::Net::canned(&[
        (
            format!("json https://api.openalex.org/works/doi:{doi}").as_str(),
            work.as_str(),
        ),
        (format!("pdf {blocked}").as_str(), "!status 403"),
    ]);
    let e = crate::add::open_copy(&lib, &mut index, &net, &hit, false, &|_| true).unwrap_err();
    assert_eq!(crate::add::reason(&e).as_deref(), Some("blocked"));
    let e =
        crate::add::open_copy(&lib, &mut index, &offline(), &hit, false, &|_| true).unwrap_err();
    assert_eq!(crate::add::reason(&e).as_deref(), Some("no-open-copy"));
}

#[test]
fn writes_an_article_number_as_its_pages() {
    let mut r = demo::records().remove(0);
    if let serde_json::Value::Object(m) = &mut r.csl {
        m.remove("page");
    }
    r.csl["article-number"] = serde_json::json!("eadp1720");
    assert!(
        bibtex::entry(&r).contains("pages = {eadp1720}"),
        "{}",
        bibtex::entry(&r)
    );
}

#[test]
fn says_a_change_on_one_line() {
    use crate::add::Change;
    let c = Change {
        field: "author".to_string(),
        old: Some(serde_json::json!([{ "family": "A" }, { "family": "B" }])),
        new: Some(serde_json::json!([{ "family": "A" }, { "family": "C" }])),
    };
    assert_eq!(c.said(), r#"author[1]: {"family":"B"} → {"family":"C"}"#);
    let c = Change {
        field: "abstract".to_string(),
        old: None,
        new: Some(serde_json::json!("<jats:p>Two\nlines</jats:p>")),
    };
    assert_eq!(c.said(), "abstract: none → Two lines");
}

#[test]
fn tells_a_page_naming_no_pdf_from_one_turned_away() {
    use crate::net::Why;
    assert!(Why::Page.blocked() && Why::Status(202).blocked() && Why::Status(429).blocked());
    assert!(!Why::Unnamed.blocked() && !Why::Status(404).blocked());
}

// An unreadable file, as Unix makes one, stands for one kept only online.
#[cfg(unix)]
#[test]
fn labels_without_reading_records_unchanged_since() {
    use std::os::unix::fs::PermissionsExt;
    let (_dir, lib, mut index) = library();
    let all = keys(&index, "", Sort::Added);
    let (kept, changed) = (&all[0], &all[1]);
    // One kept only online, as Dropbox keeps them: not to be read.
    let online = lib.root.join(index.get(kept).unwrap().unwrap().path);
    std::fs::set_permissions(&online, std::fs::Permissions::from_mode(0o000)).unwrap();
    // One another program changed since: read again, its change kept.
    let file = lib.root.join(index.get(changed).unwrap().unwrap().path);
    let mut r = crate::library::read(&file).unwrap();
    r.paperstack.labels.push("Elsewhere".to_string());
    std::fs::write(&file, serde_json::to_vec_pretty(&r).unwrap()).unwrap();

    let n = crate::remove::label(&lib, &mut index, &all, "Later").unwrap();
    assert_eq!(n, all.len());
    std::fs::set_permissions(&online, std::fs::Permissions::from_mode(0o644)).unwrap();
    assert_eq!(keys(&index, "label:Later", Sort::Added).len(), all.len());
    let was = crate::library::read(&file).unwrap().paperstack.labels;
    assert!(was.contains(&"Elsewhere".to_string()), "{was:?}");
    assert!(was.contains(&"Later".to_string()), "{was:?}");
}

#[test]
fn moves_as_emacs_does_but_pastes_while_typing() {
    let (_dir, lib, index) = library();
    let mut app = App::new(lib, index).unwrap();
    let press = |app: &mut App, spec: &str| {
        for k in crate::app::keys(spec) {
            app.key(k);
        }
        app.list.selected()
    };
    let last = app.shown.len() - 1;
    assert_eq!(press(&mut app, "<c-n>"), Some(1));
    assert_eq!(press(&mut app, "<c-n>"), Some(2));
    assert_eq!(press(&mut app, "<c-p>"), Some(1));
    assert_eq!(press(&mut app, "<c-v>"), Some(6.min(last)));
    assert_eq!(press(&mut app, "<m-v>"), Some(1));
    assert_eq!(press(&mut app, "<m->>"), Some(last));
    assert_eq!(press(&mut app, "<m-<>"), Some(0));
    // A label being typed: ctrl-n is not an n.
    press(&mut app, "L");
    press(&mut app, "<c-n>");
    let Popup::Ask(ask) = &app.popup else {
        panic!("no label asked for")
    };
    assert_eq!(ask.line, "");
}

#[test]
fn cites_in_a_style() {
    let (_dir, mut server) = mcp_server();
    let cited = mcp_call(
        &mut server,
        "cite",
        serde_json::json!({ "keys": ["Whitlock2017-ex", "Whitlock2017-ex"], "format": "text" }),
    );
    assert_eq!(cited["isError"], false, "{cited}");
    let text = cited["content"][0]["text"].as_str().unwrap();
    assert!(
        text.starts_with("Whitlock, T. J., & Dunmore, P. (2017)."),
        "{text}"
    );
    assert!(text.contains("*Ann Fict Med*"), "{text}");
    assert_eq!(text.lines().count(), 1, "{text}");

    let plain = mcp_call(
        &mut server,
        "cite",
        serde_json::json!({ "key": "Whitlock2017-ex", "format": "text",
            "style": "chicago-author-date", "markup": "plain" }),
    );
    let text = plain["content"][0]["text"].as_str().unwrap();
    assert!(text.starts_with("Whitlock, T"), "{text}");
    assert!(!text.contains('*'), "{text}");

    let bad = mcp_call(
        &mut server,
        "cite",
        serde_json::json!({ "key": "Whitlock2017-ex", "format": "text", "style": "nope" }),
    );
    assert_eq!(bad["isError"], true, "{bad}");
}

#[test]
fn sets_papers_out_in_a_style_to_copy() {
    use ratatui::crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
    let (_dir, lib, index) = library();
    let mut app = App::new(lib, index).unwrap();
    app.list.select(Some(0));
    let ctrl_e = KeyEvent::new(KeyCode::Char('e'), KeyModifiers::CONTROL);
    app.key(ctrl_e);
    assert!(matches!(&app.popup, Popup::Cite(c) if c.style().is_none()));
    app.key(crate::app::keys("s")[0]);
    let s = screen(&mut app, 160, 44);
    assert!(s.contains("APA Style"), "{s}");
    let Popup::Cite(c) = &app.popup else {
        panic!("closed")
    };
    let first = app.shown[0].record.authors()[0].family.clone();
    assert!(c.text.starts_with(&format!("{first}, ")), "{}", c.text);
    // Asked for again, as it was last set out; round to BibTeX at last.
    app.key(crate::app::keys("<esc>")[0]);
    app.key(ctrl_e);
    assert!(matches!(&app.popup, Popup::Cite(c) if c.style() == Some("apa")));
    for _ in crate::style::CYCLED {
        app.key(crate::app::keys("s")[0]);
    }
    assert!(matches!(&app.popup, Popup::Cite(c) if c.style().is_none() && c.text.starts_with('@')));
}
