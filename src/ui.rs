//! The screen, laid out as a reference manager's: a search line, the library's
//! shelves on a tinted sidebar, the papers on the page, each with its box
//! to mark and its star, the paper picked on the right, and the keys at
//! the foot. Colours are the theme's.

use crate::app::{App, Fetch, Focus, Popup, Shelf, Target};
use crate::browser;
use crate::record::Record;
use crate::source::Source;
use crate::theme::Theme;
use ratatui::Frame;
use ratatui::layout::{Constraint, Layout, Rect};
use ratatui::style::{Modifier, Style};
use ratatui::text::{Line, Span, Text};
use ratatui::widgets::{
    Block, BorderType, Borders, Clear, List, ListItem, ListState, Padding, Paragraph, Wrap,
};

/// The badge of a paper without its PDF, to get it in the browser.
const GET_PDF: &str = " get PDF ";
/// Authors listed before `… n more`.
const AUTHORS_SHOWN: usize = 5;
const SIDEBAR: u16 = 30;
const DETAILS: u16 = 54;
/// The narrowest screen the details are shown on.
const WIDE: u16 = 130;

pub fn draw(f: &mut Frame, app: &mut App) {
    let t = app.theme;
    app.targets.clear();
    f.render_widget(
        Block::new().style(Style::new().bg(t.page).fg(t.text)),
        f.area(),
    );
    let [top, gap, main, foot] = Layout::vertical([
        Constraint::Length(1),
        Constraint::Length(1),
        Constraint::Min(5),
        Constraint::Length(1),
    ])
    .areas(f.area());
    f.render_widget(Block::new().style(Style::new().bg(t.side)), gap);
    search(f, app, top);
    let wide = main.width >= WIDE;
    let [side, list, detail] = Layout::horizontal([
        Constraint::Length(SIDEBAR),
        Constraint::Min(40),
        Constraint::Length(if wide { DETAILS } else { 0 }),
    ])
    .areas(main);
    shelves(f, app, side);
    papers(f, app, list);
    if wide {
        details(f, app, detail);
    }
    keys(f, app, foot);
    let popup = match &app.popup {
        Popup::Add(line) => Some(add(f, &t, line)),
        Popup::Cite(text, scroll) => Some(cite(f, &t, text, *scroll)),
        Popup::Help => Some(help(f, &t)),
        Popup::Fetch(fetch) => Some(fetch_dialog(f, &t, fetch)),
        Popup::None => None,
    };
    if let Some(area) = popup {
        app.targets.push((area, Target::Popup));
    }
}

/// Where the items of a list drawn in `area`, from `offset`, each as high
/// as `heights` says, are on the screen, by their place in the list.
fn rows(area: Rect, offset: usize, heights: &[u16]) -> Vec<(usize, Rect)> {
    let mut y = area.y;
    let mut rows = Vec::new();
    for (i, &h) in heights.iter().enumerate().skip(offset) {
        if y >= area.bottom() {
            break;
        }
        let height = h.min(area.bottom() - y);
        rows.push((i, Rect { y, height, ..area }));
        y += height;
    }
    rows
}

fn search(f: &mut Frame, app: &mut App, area: Rect) {
    let t = app.theme;
    f.render_widget(Block::new().style(Style::new().bg(t.side)), area);
    let [brand, box_, right] = Layout::horizontal([
        Constraint::Length(SIDEBAR),
        Constraint::Min(20),
        Constraint::Length(42),
    ])
    .areas(area);
    let (name, button) = (" ▤ paperstack ", " + Add ");
    f.render_widget(
        Paragraph::new(Line::from(vec![
            Span::styled(name, Style::new().fg(t.title).bold()),
            Span::styled(button, Style::new().fg(t.on_accent).bg(t.add).bold()),
        ])),
        brand,
    );
    let at = name.chars().count() as u16;
    app.targets.push((
        Rect {
            x: brand.x + at,
            width: (button.len() as u16).min(brand.width.saturating_sub(at)),
            ..brand
        },
        Target::Add,
    ));
    let mut field = vec![Span::raw(" ⌕ ")];
    if app.query.is_empty() && !app.searching {
        field.push(Span::styled("Search library", Style::new().fg(t.dim)));
    } else {
        field.push(Span::styled(app.query.clone(), Style::new().fg(t.text)));
    }
    if app.searching {
        field.push(Span::styled("▏", Style::new().fg(t.accent)));
    }
    let width = box_.width.saturating_sub(2).min(72);
    let field_area = Rect { width, ..box_ };
    app.targets.push((field_area, Target::Search));
    f.render_widget(
        Paragraph::new(Line::from(field)).style(Style::new().bg(t.field).fg(t.dim)),
        field_area,
    );
    let hint = Rect {
        x: field_area.x + width.saturating_sub(4),
        width: 3,
        ..field_area
    };
    f.render_widget(
        Paragraph::new(Span::styled(" / ", Style::new().fg(t.dim).bg(t.page))),
        hint,
    );
    let count = format!(
        "↓ {}   {} of {} references ",
        sorted_by(app),
        app.shown.len(),
        app.total()
    );
    f.render_widget(
        Paragraph::new(Span::styled(count, Style::new().fg(t.dim)))
            .alignment(ratatui::layout::Alignment::Right),
        right,
    );
}

fn sorted_by(app: &App) -> String {
    let name = app.sort.name();
    let mut c = name.chars();
    c.next()
        .map(|first| first.to_uppercase().chain(c).collect())
        .unwrap_or_default()
}

fn shelves(f: &mut Frame, app: &mut App, area: Rect) {
    let t = app.theme;
    let focused = app.focus == Focus::Side;
    let items: Vec<ListItem> = app
        .shelves
        .iter()
        .map(|(shelf, n)| {
            let (icon, colour) = match shelf {
                Shelf::Heading(h) => {
                    return ListItem::new(vec![
                        Line::raw(""),
                        Line::styled(
                            format!(" {h}"),
                            Style::new().fg(t.dim).add_modifier(Modifier::BOLD),
                        ),
                    ]);
                }
                Shelf::All => ("▤", t.title),
                Shelf::Starred => ("★", t.dim),
                Shelf::NoPdf => ("∅", t.dim),
                Shelf::NoAbstract => ("¶", t.dim),
                Shelf::Folder(_) => ("■", t.folder),
                Shelf::Label(_) => ("●", t.dim),
            };
            let room = (area.width as usize).saturating_sub(n.to_string().len() + 6);
            let name: String = shelf.name().chars().take(room).collect();
            ListItem::new(Line::from(vec![
                Span::styled(format!(" {icon} "), Style::new().fg(colour)),
                Span::styled(name, Style::new().fg(t.text).bold()),
                Span::styled(format!(" {n}"), Style::new().fg(t.dim)),
            ]))
        })
        .collect();
    let mut state = ListState::default().with_selected(Some(app.shelf));
    let picked = if focused {
        Style::new().bg(t.accent).fg(t.on_accent)
    } else {
        Style::new().bg(t.row)
    };
    let list = List::new(items)
        .style(Style::new().bg(t.side))
        .highlight_style(picked);
    f.render_stateful_widget(list, area, &mut state);
    app.targets.push((area, Target::Side));
    let heights: Vec<u16> = app
        .shelves
        .iter()
        .map(|(s, _)| if matches!(s, Shelf::Heading(_)) { 2 } else { 1 })
        .collect();
    for (i, row) in rows(area, state.offset(), &heights) {
        if !matches!(app.shelves[i].0, Shelf::Heading(_)) {
            app.targets.push((row, Target::Shelf(i)));
        }
    }
}

/// `A, B, C … n more`, as a reference manager shortens a long list of authors.
fn authors(r: &Record, shown: usize) -> String {
    let all = r.authors();
    let names: Vec<String> = all
        .iter()
        .take(shown)
        .map(|n| format!("{} {}", n.family, n.given).trim().to_string())
        .collect();
    if all.len() > shown {
        format!("{} … {} more", names.join(", "), all.len() - shown)
    } else {
        names.join(", ")
    }
}

fn papers(f: &mut Frame, app: &mut App, area: Rect) {
    let t = app.theme;
    let width = area.width.saturating_sub(4) as usize;
    let rule = Line::styled("─".repeat(area.width as usize), Style::new().fg(t.rule));
    let (items, titles): (Vec<ListItem>, Vec<u16>) = app
        .shown
        .iter()
        .map(|h| {
            let r = &h.record;
            let mark = if app.marked.contains(&r.key) {
                Span::styled(" ☑ ", Style::new().fg(t.accent).bold())
            } else {
                Span::styled(" ☐ ", Style::new().fg(t.dim))
            };
            let star = if r.paperstack.starred {
                Span::styled(" ★", Style::new().fg(t.star))
            } else {
                Span::styled(" ☆", Style::new().fg(t.dim))
            };
            let year = r
                .year()
                .map_or_else(|| "n.d.".to_string(), |y| y.to_string());
            let venue = clip(r.venue(), width / 2);
            let rest = format!(" · {year} · {}", r.kind());
            // The PDF's badge at the right, as a reference manager's; the key before
            // it, where there is room.
            let pdf = if h.has_pdf { " PDF " } else { GET_PDF };
            let room = (width + 2)
                .saturating_sub(3 + venue.chars().count() + rest.chars().count() + pdf.len());
            let key = if room > r.key.len() + 4 {
                format!("  {}", r.key)
            } else {
                String::new()
            };
            let pad = room.saturating_sub(key.len());
            let meta = vec![
                Span::raw("   "),
                Span::styled(venue, Style::new().fg(t.venue)),
                Span::styled(rest, Style::new().fg(t.dim)),
                Span::styled(key, Style::new().fg(t.dim)),
                Span::raw(" ".repeat(pad)),
                if h.has_pdf {
                    Span::styled(pdf, Style::new().fg(t.on_badge).bg(t.badge))
                } else {
                    Span::styled(pdf, Style::new().fg(t.accent).underlined())
                },
            ];
            let title = clip(r.title(), width.saturating_sub(4));
            let title_width = title.chars().count() as u16;
            let item = ListItem::new(Text::from(vec![
                Line::from(vec![
                    mark,
                    Span::styled(title, Style::new().fg(t.title).bold()),
                    star,
                ]),
                Line::from(vec![
                    Span::raw("   "),
                    Span::styled(
                        clip(&authors(r, AUTHORS_SHOWN), width),
                        Style::new().fg(t.text),
                    ),
                ]),
                Line::from(meta),
                rule.clone(),
            ]));
            (item, title_width)
        })
        .unzip();
    let focused = app.focus == Focus::List;
    let shelf = app.shelves[app.shelf].0.name().to_string();
    let block = Block::new()
        .title(Line::styled(
            format!(" {shelf}"),
            Style::new().fg(t.dim).add_modifier(Modifier::BOLD),
        ))
        .style(Style::new().bg(t.page));
    let inner = block.inner(area);
    let list = List::new(items)
        .block(block)
        .highlight_symbol(if focused { "▎" } else { " " })
        .highlight_style(Style::new().bg(t.row))
        .style(Style::new().bg(t.page));
    f.render_stateful_widget(list, area, &mut app.list);
    app.targets.push((area, Target::List));
    if app.total() == 0 {
        empty(f, &t, inner);
    }
    let heights = vec![4; titles.len()];
    for (i, row) in rows(inner, app.list.offset(), &heights) {
        app.targets.push((row, Target::Paper(i)));
        // Past the highlight's column: the box, then the title, then the star.
        let line = |x: u16, width: u16| {
            let x = (row.x + x).min(row.right());
            Rect {
                x,
                width: width.min(row.right() - x),
                height: 1,
                ..row
            }
        };
        app.targets.push((line(1, 3), Target::Mark(i)));
        app.targets.push((line(4 + titles[i], 2), Target::Star(i)));
        if !app.shown[i].has_pdf {
            let badge = GET_PDF.chars().count() as u16;
            let x = row.right().saturating_sub(badge + 1);
            app.targets.push((
                Rect {
                    x,
                    y: row.y + 2,
                    width: badge,
                    height: 1,
                },
                Target::GetPdf(i),
            ));
        }
    }
}

/// What an empty library says: how to fill it, from a reference manager first.
fn empty(f: &mut Frame, t: &Theme, area: Rect) {
    let dim = Style::new().fg(t.dim);
    let key = Style::new().fg(t.title).bold();
    let lines = vec![
        Line::raw(""),
        Line::styled("  No papers yet.", Style::new().fg(t.title).bold()),
        Line::raw(""),
        Line::styled("  From a reference manager:", Style::new().bold()),
        Line::styled(
            "   1. Settings › Data and files › Export data › JSON",
            Style::new().fg(t.text),
        ),
        Line::styled(
            "   2. PDFs: select all, ⋯ › Download files as ZIP; unzip it",
            Style::new().fg(t.text),
        ),
        Line::from(vec![
            Span::styled("   3. press ", Style::new().fg(t.text)),
            Span::styled("a", key),
            Span::styled(
                " and drop the .json in, or run paperstack import",
                Style::new().fg(t.text),
            ),
        ]),
        Line::raw(""),
        Line::from(vec![
            Span::styled("  Or press ", dim),
            Span::styled("a", key),
            Span::styled(" for a DOI, arXiv id, PMID, address, PDF or title.", dim),
        ]),
    ];
    f.render_widget(
        Paragraph::new(lines)
            .style(Style::new().bg(t.page))
            .wrap(Wrap { trim: false }),
        area,
    );
}

fn details(f: &mut Frame, app: &App, area: Rect) {
    let t = app.theme;
    let block = Block::new()
        .borders(Borders::LEFT)
        .border_style(Style::new().fg(t.rule))
        .padding(Padding::horizontal(1))
        .style(Style::new().bg(t.page));
    let Some(h) = app.current() else {
        f.render_widget(block, area);
        return;
    };
    let r = &h.record;
    let m = &r.paperstack;
    let dim = Style::new().fg(t.dim);
    let field = |name: &'static str, value: String| {
        Line::from(vec![
            Span::styled(format!("{name:<9}"), dim),
            Span::styled(value, Style::new().fg(t.text)),
        ])
    };
    let none = || "—".to_string();
    let file = h
        .path
        .file_name()
        .map_or_else(String::new, |n| n.to_string_lossy().into_owned());
    let folder = h.path.parent().map_or_else(String::new, |d| {
        // With `/` between folders on any system, as the library is shared.
        d.iter().map(|c| c.to_string_lossy() + "/").collect()
    });
    let mut lines = vec![
        Line::styled(r.title().to_string(), Style::new().fg(t.title).bold()),
        Line::raw(""),
        Line::styled(authors(r, usize::MAX), Style::new().fg(t.text)),
        Line::from(vec![
            Span::styled(r.venue().to_string(), Style::new().fg(t.venue)),
            Span::styled(
                format!(
                    " · {} · {}",
                    r.year().map_or_else(none, |y| y.to_string()),
                    r.kind()
                ),
                dim,
            ),
        ]),
        Line::raw(""),
        field("key", r.key.clone()),
        field("DOI", r.doi().map_or_else(none, str::to_string)),
        field("arXiv", m.arxiv.clone().unwrap_or_else(none)),
        field("folder", folder),
        Line::from(vec![
            Span::styled(format!("{:<9}", "PDF"), dim),
            if h.has_pdf {
                Span::styled(" PDF ", Style::new().fg(t.on_badge).bg(t.badge))
            } else {
                Span::styled("none · p gets it in your browser", dim)
            },
        ]),
        field("record", file),
        field("folders", join(&m.folders)),
        field("labels", join(&m.labels)),
        Line::raw(""),
        Line::styled("ABSTRACT", dim.add_modifier(Modifier::BOLD)),
        Line::styled(
            r.abstract_text().unwrap_or("—").to_string(),
            Style::new().fg(t.text),
        ),
    ];
    if !app.marked.is_empty() {
        lines.push(Line::raw(""));
        lines.push(Line::styled(
            format!(
                "{} marked: ctrl-e gives their BibTeX, y their keys",
                app.marked.len()
            ),
            Style::new().fg(t.accent).bold(),
        ));
    }
    f.render_widget(
        Paragraph::new(lines)
            .block(block)
            .wrap(Wrap { trim: false }),
        area,
    );
}

fn keys(f: &mut Frame, app: &App, area: Rect) {
    let t = app.theme;
    let mut spans = Vec::new();
    for (k, what) in [
        ("/", "search"),
        ("S", "sort"),
        ("⏎", "open in ohp"),
        ("p", "get PDF"),
        ("␣", "mark"),
        ("*", "mark all"),
        ("^E", "BibTeX"),
        ("y", "yank [@key]"),
        ("s", "star"),
        ("a", "add"),
        ("?", "help"),
        ("q", "quit"),
    ] {
        spans.push(Span::styled(
            format!(" {k} "),
            Style::new().fg(t.title).bold(),
        ));
        spans.push(Span::styled(format!("{what} "), Style::new().fg(t.dim)));
    }
    if !app.status.is_empty() {
        spans.push(Span::styled(
            format!("  {}", app.status),
            Style::new().fg(t.text).bold(),
        ));
    }
    f.render_widget(
        Paragraph::new(Line::from(spans)).style(Style::new().bg(t.side)),
        area,
    );
}

/// A box over the screen, as a reference manager's dialogs are.
fn dialog<'a>(t: &Theme, title: &'a str) -> Block<'a> {
    Block::bordered()
        .border_type(BorderType::Rounded)
        .border_style(Style::new().fg(t.accent))
        .title(Span::styled(
            format!(" {title} "),
            Style::new().fg(t.title).bold(),
        ))
        .padding(Padding::horizontal(1))
        .style(Style::new().bg(t.page).fg(t.text))
}

fn add(f: &mut Frame, t: &Theme, line: &str) -> Rect {
    let area = centered(f.area(), 86, 16);
    f.render_widget(Clear, area);
    let (what, steps) = Source::of(line).plan();
    let dim = Style::new().fg(t.dim);
    let mut lines = vec![
        Line::styled(
            "DOI, arXiv id, PMID, address, a PDF's path, a title, or a reference manager's export",
            dim,
        ),
        Line::from(vec![
            Span::styled(" › ", Style::new().fg(t.accent).bg(t.field)),
            Span::styled(format!("{line}▏"), Style::new().fg(t.text).bg(t.field)),
        ]),
        Line::raw(""),
    ];
    if !what.is_empty() {
        lines.push(Line::from(vec![
            Span::styled("reads as ", dim),
            Span::styled(what, Style::new().fg(t.title).bold()),
        ]));
        for (n, step) in steps.iter().enumerate() {
            lines.push(Line::from(vec![
                Span::styled(format!("  {}. ", n + 1), dim),
                Span::raw(*step),
            ]));
        }
    }
    let block = dialog(t, "Add paper")
        .title_bottom(Line::styled(" ⏎ add · ctrl-v paste · esc cancel ", dim));
    f.render_widget(Paragraph::new(lines).block(block), area);
    area
}

/// A paper's PDF, awaited from the browser.
fn fetch_dialog(f: &mut Frame, t: &Theme, fetch: &Fetch) -> Rect {
    let area = centered(f.area(), 88, 15);
    f.render_widget(Clear, area);
    let dim = Style::new().fg(t.dim);
    let mut lines = vec![
        Line::styled(fetch.title.clone(), Style::new().fg(t.title).bold()),
        Line::raw(""),
        Line::from(vec![
            Span::styled("opened  ", dim),
            Span::styled(fetch.page.clone(), Style::new().fg(t.accent)),
        ]),
        Line::raw(""),
        Line::raw("Download its PDF there, signed in, as you would: it is filed as soon"),
        Line::from(vec![
            Span::raw("as it lands in "),
            Span::styled(browser::tilde(&fetch.arrivals.dir), Style::new().bold()),
            Span::raw(", if it is this paper's."),
        ]),
        Line::styled(
            "Or paste its path or its address; or press paperstack's button in Chrome.",
            dim,
        ),
        Line::raw(""),
        Line::from(vec![
            Span::styled(" › ", Style::new().fg(t.accent).bg(t.field)),
            Span::styled(
                format!("{}▏", fetch.line),
                Style::new().fg(t.text).bg(t.field),
            ),
        ]),
    ];
    if !fetch.note.is_empty() {
        lines.push(Line::raw(""));
        lines.push(Line::styled(fetch.note.clone(), Style::new().fg(t.star)));
    }
    let block = dialog(t, "Get the PDF").title_bottom(Line::styled(
        " ⏎ take what is pasted · ctrl-v paste · ctrl-o open the page again · esc stop ",
        dim,
    ));
    f.render_widget(
        Paragraph::new(lines)
            .block(block)
            .wrap(Wrap { trim: false }),
        area,
    );
    area
}

fn cite(f: &mut Frame, t: &Theme, text: &str, scroll: u16) -> Rect {
    let area = centered(f.area(), 92, 26);
    f.render_widget(Clear, area);
    let block = dialog(t, "BibTeX").title_bottom(Line::styled(
        " y copy · j/k scroll · any key closes ",
        Style::new().fg(t.dim),
    ));
    f.render_widget(
        Paragraph::new(text.to_string())
            .wrap(Wrap { trim: false })
            .block(block)
            .scroll((scroll, 0)),
        area,
    );
    area
}

fn help(f: &mut Frame, t: &Theme) -> Rect {
    let rows = [
        ("j k ↑ ↓", "move; on the left, change shelf"),
        ("h l tab", "switch between shelves and papers"),
        (
            "/",
            "search; esc clears. Words and \"phrases\" in title, abstract, authors",
        ),
        (
            "",
            "author:  year:2020..2026  folder:\"name\"  label:  has:pdf  is:starred  -...",
        ),
        (
            "S",
            "sort: relevance, date added, year, first author, title",
        ),
        ("⏎ o", "open the PDF in ohp; quit ohp to come back"),
        (
            "p",
            "get the PDF in your browser, signed in: download it there, as you would",
        ),
        (
            "space",
            "mark a paper; * marks all shown, again unmarks; esc unmarks all",
        ),
        ("ctrl-e", "BibTeX of the papers marked, or the one picked"),
        (
            "click",
            "pick; on a box, mark; on a star, star; twice, open",
        ),
        ("y", "copy [@key; @key] to the clipboard"),
        ("s", "star, written to the paper's record"),
        ("a", "add from a DOI, arXiv id, PMID, address, PDF or title"),
        (
            "r",
            "read again what paperstack serve, or another machine, added",
        ),
        ("q", "quit"),
    ];
    let area = centered(f.area(), 94, rows.len() as u16 + 2);
    f.render_widget(Clear, area);
    let lines: Vec<Line> = rows
        .iter()
        .map(|(k, what)| {
            Line::from(vec![
                Span::styled(format!("{k:<9}"), Style::new().fg(t.title).bold()),
                Span::raw(*what),
            ])
        })
        .collect();
    f.render_widget(Paragraph::new(lines).block(dialog(t, "Keys")), area);
    area
}

fn centered(area: Rect, width: u16, height: u16) -> Rect {
    let width = width.min(area.width);
    let height = height.min(area.height);
    Rect {
        x: area.x + (area.width - width) / 2,
        y: area.y + (area.height - height) / 2,
        width,
        height,
    }
}

fn clip(s: &str, width: usize) -> String {
    if s.chars().count() <= width {
        s.to_string()
    } else {
        let mut out: String = s.chars().take(width.saturating_sub(1)).collect();
        out.push('…');
        out
    }
}

fn join(items: &[String]) -> String {
    if items.is_empty() {
        "—".to_string()
    } else {
        items.join(", ")
    }
}
