use super::*;
use ratatui::Terminal;
use ratatui::backend::TestBackend;

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
fn draws_the_library() {
    let mut app = App::new();
    let s = screen(&mut app, 160, 44);
    println!("{s}");
    assert!(s.contains("Sensitivity analysis"));
    assert!(s.contains("VanderWeele2017-ex"));
}

#[test]
fn narrows_by_folder_and_query() {
    let mut app = App::new();
    app.query = "linear".to_string();
    app.narrow();
    assert_eq!(app.shown.len(), 1);
    app.query.clear();
    app.shelf = app
        .shelves
        .iter()
        .position(|&s| s == Shelf::Folder("mediation"))
        .unwrap();
    app.narrow();
    assert_eq!(app.shown.len(), 2);
}

#[test]
fn draws_add() {
    let mut app = App::new();
    app.popup = Popup::Add("https://arxiv.org/abs/2412.08821".to_string());
    let s = screen(&mut app, 160, 44);
    println!("{s}");
    assert!(s.contains("arXiv 2412.08821"));
}

#[test]
fn files_papers_by_year_and_first_author() {
    let papers = mock::papers();
    let p = papers
        .iter()
        .find(|p| p.key == "Ahlmann-Eltze2025-pb")
        .unwrap();
    assert_eq!(
        layout::pdf(p),
        PathBuf::from(
            "2025/Ahlmann-Eltze/C/Ahlmann-Eltze2025-pb_deep-learning-based-gene-perturbation-effect.pdf"
        )
    );
    let p = papers.iter().find(|p| p.key == "LCM-team2024-lc").unwrap();
    assert!(layout::record(p).starts_with("2024/LCM team/_/"));
    assert_eq!(layout::slug("A/B: c? d"), "a-b-c-d");
}

#[test]
fn reads_sources() {
    use bib::Source;
    assert!(
        matches!(Source::of("https://doi.org/10.7554/eLife.10047"), Source::Doi(d) if d == "10.7554/eLife.10047")
    );
    assert!(matches!(Source::of("2412.08821v2"), Source::Arxiv(_)));
    assert!(matches!(Source::of("PMID: 28693043"), Source::Pmid(_)));
    assert!(matches!(Source::of("~/Downloads/x.pdf"), Source::Pdf(_)));
    assert!(matches!(Source::of("@article{x,"), Source::Bibtex));
    assert!(matches!(Source::of("linear baselines"), Source::Words(_)));
}
