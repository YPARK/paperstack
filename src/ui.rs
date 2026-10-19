//! The screen: a search line, shelves on the left, papers in the middle,
//! the paper picked on the right, and the keys at the foot.

use crate::app::{App, Focus, Popup, Shelf};
use crate::record::Record;
use crate::source::Source;
use ratatui::Frame;
use ratatui::layout::{Constraint, Layout, Rect};
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span, Text};
use ratatui::widgets::{Block, BorderType, Clear, List, ListItem, ListState, Paragraph, Wrap};

/// Authors listed before `… n more`.
const AUTHORS_SHOWN: usize = 5;

fn dim() -> Style {
    Style::new().fg(Color::DarkGray)
}

fn picked() -> Style {
    Style::new().fg(Color::Yellow).add_modifier(Modifier::BOLD)
}

fn venue() -> Style {
    Style::new().fg(Color::Green)
}

fn frame(title: &str, focused: bool) -> Block<'_> {
    Block::bordered()
        .border_type(BorderType::Rounded)
        .border_style(if focused {
            Style::new().fg(Color::Yellow)
        } else {
            dim()
        })
        .title(Span::styled(
            format!(" {title} "),
            Style::new().add_modifier(Modifier::BOLD),
        ))
}

pub fn draw(f: &mut Frame, app: &mut App) {
    let [top, main, foot] = Layout::vertical([
        Constraint::Length(1),
        Constraint::Min(5),
        Constraint::Length(1),
    ])
    .areas(f.area());
    search(f, app, top);
    let wide = main.width >= 130;
    let [side, list, detail] = Layout::horizontal([
        Constraint::Length(30),
        Constraint::Min(40),
        Constraint::Length(if wide { 52 } else { 0 }),
    ])
    .areas(main);
    shelves(f, app, side);
    papers(f, app, list);
    if wide {
        details(f, app, detail);
    }
    keys(f, app, foot);
    match &app.popup {
        Popup::Add(line) => add(f, line),
        Popup::Cite(text, scroll) => cite(f, text, *scroll),
        Popup::Help => help(f),
        Popup::None => {}
    }
}

fn search(f: &mut Frame, app: &App, area: Rect) {
    let mut spans = vec![
        Span::styled(
            " paperstack ",
            Style::new().fg(Color::Black).bg(Color::Yellow).bold(),
        ),
        Span::styled(
            format!(" {} ", tilde(&app.lib.root.display().to_string())),
            dim(),
        ),
        Span::raw(" / "),
    ];
    if app.query.is_empty() && !app.searching {
        spans.push(Span::styled("search library", dim()));
    } else {
        spans.push(Span::raw(app.query.clone()));
    }
    if app.searching {
        spans.push(Span::styled("▏", Style::new().fg(Color::Yellow)));
    }
    let all = app.total();
    let right = format!(
        "{} of {all} references · {} ",
        app.shown.len(),
        app.sort.name()
    );
    let [l, r] = Layout::horizontal([
        Constraint::Min(1),
        Constraint::Length(right.chars().count() as u16),
    ])
    .areas(area);
    f.render_widget(Paragraph::new(Line::from(spans)), l);
    f.render_widget(Paragraph::new(Span::styled(right, dim())), r);
}

fn shelves(f: &mut Frame, app: &App, area: Rect) {
    let width = area.width.saturating_sub(4) as usize;
    let items: Vec<ListItem> = app
        .shelves
        .iter()
        .map(|(shelf, n)| {
            let icon = match shelf {
                Shelf::Heading(h) => {
                    return ListItem::new(Line::styled(*h, dim().add_modifier(Modifier::BOLD)));
                }
                Shelf::All => "▤",
                Shelf::Starred => "★",
                Shelf::NoPdf => "∅",
                Shelf::NoAbstract => "¶",
                Shelf::Folder(_) => "▸",
                Shelf::Label(_) => "#",
            };
            let count = n.to_string();
            let room = width.saturating_sub(count.len() + 3);
            let name: String = shelf.name().chars().take(room).collect();
            let pad = width.saturating_sub(name.chars().count() + count.len() + 2);
            let style = if *n == 0 { dim() } else { Style::new() };
            ListItem::new(Line::from(vec![
                Span::styled(format!("{icon} "), Style::new().fg(Color::Yellow)),
                Span::styled(name, style),
                Span::raw(" ".repeat(pad)),
                Span::styled(count, dim()),
            ]))
        })
        .collect();
    let focused = app.focus == Focus::Side;
    let mut state = ListState::default().with_selected(Some(app.shelf));
    let list = List::new(items)
        .block(frame("Library", focused))
        .highlight_style(if focused {
            picked().bg(Color::Indexed(236))
        } else {
            Style::new().bg(Color::Indexed(236))
        });
    f.render_stateful_widget(list, area, &mut state);
}

/// `A, B, C … n more`, as Paperpile shortens a long list of authors.
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
    let width = area.width.saturating_sub(5) as usize;
    let items: Vec<ListItem> = app
        .shown
        .iter()
        .map(|h| {
            let r = &h.record;
            let mark = if app.marked.contains(&r.key) {
                Span::styled("● ", Style::new().fg(Color::Cyan))
            } else {
                Span::raw("  ")
            };
            let star = if r.paperstack.starred {
                Span::styled(" ★", Style::new().fg(Color::Yellow))
            } else {
                Span::raw("")
            };
            let pdf = if h.has_pdf {
                Span::styled(" PDF ", Style::new().fg(Color::Black).bg(Color::Gray))
            } else {
                Span::styled(" no PDF ", dim())
            };
            let year = r
                .year()
                .map_or_else(|| "n.d.".to_string(), |y| y.to_string());
            ListItem::new(Text::from(vec![
                Line::from(vec![
                    mark,
                    Span::styled(
                        clip(r.title(), width.saturating_sub(4)),
                        Style::new().fg(Color::LightBlue).bold(),
                    ),
                    star,
                ]),
                Line::from(vec![
                    Span::raw("  "),
                    Span::raw(clip(&authors(r, AUTHORS_SHOWN), width)),
                ]),
                Line::from(vec![
                    Span::raw("  "),
                    Span::styled(r.venue().to_string(), venue()),
                    Span::styled(format!(" · {year} · {}  ", r.kind()), dim()),
                    pdf,
                    Span::styled(format!("  {}", r.key), dim()),
                ]),
                Line::raw(""),
            ]))
        })
        .collect();
    let focused = app.focus == Focus::List;
    let title = app.shelves[app.shelf].0.name().to_string();
    let list = List::new(items)
        .block(frame(&title, focused))
        .highlight_symbol("▌")
        .highlight_style(Style::new().bg(Color::Indexed(236)));
    f.render_stateful_widget(list, area, &mut app.list);
}

fn details(f: &mut Frame, app: &App, area: Rect) {
    let block = frame("Details", false);
    let Some(h) = app.current() else {
        f.render_widget(block, area);
        return;
    };
    let r = &h.record;
    let m = &r.paperstack;
    let field = |name: &'static str, value: String| {
        Line::from(vec![
            Span::styled(format!("{name:<9}"), dim()),
            Span::raw(value),
        ])
    };
    let none = || "—".to_string();
    let file = h
        .path
        .file_name()
        .map_or_else(String::new, |n| n.to_string_lossy().into_owned());
    let folder = h
        .path
        .parent()
        .map_or_else(String::new, |d| format!("{}/", d.display()));
    let mut lines = vec![
        Line::styled(r.title().to_string(), Style::new().bold()),
        Line::raw(""),
        Line::raw(authors(r, usize::MAX)),
        Line::from(vec![
            Span::styled(r.venue().to_string(), venue()),
            Span::styled(
                format!(" · {}", r.year().map_or_else(none, |y| y.to_string())),
                dim(),
            ),
        ]),
        Line::raw(""),
        field("key", r.key.clone()),
        field("DOI", r.doi().map_or_else(none, str::to_string)),
        field("arXiv", m.arxiv.clone().unwrap_or_else(none)),
        field("folder", folder),
        Line::from(vec![
            Span::styled(format!("{:<9}", "PDF"), dim()),
            if h.has_pdf {
                Span::styled("✓", Style::new().fg(Color::Green))
            } else {
                Span::styled("✗", Style::new().fg(Color::Red))
            },
        ]),
        field("record", file),
        field("folders", join(&m.folders)),
        field("labels", join(&m.labels)),
        Line::raw(""),
        Line::styled("Abstract", dim()),
        Line::raw(r.abstract_text().unwrap_or("—").to_string()),
    ];
    if !app.marked.is_empty() {
        lines.push(Line::raw(""));
        lines.push(Line::styled(
            format!(
                "{} marked: c gives their BibTeX, y their keys",
                app.marked.len()
            ),
            Style::new().fg(Color::Cyan),
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
    let mut spans = Vec::new();
    for (k, what) in [
        ("/", "search"),
        ("S", "sort"),
        ("⏎", "open in ohp"),
        ("␣", "mark"),
        ("*", "mark all"),
        ("c", "BibTeX"),
        ("y", "yank [@key]"),
        ("s", "star"),
        ("a", "add"),
        ("?", "help"),
        ("q", "quit"),
    ] {
        spans.push(Span::styled(
            format!(" {k} "),
            Style::new().fg(Color::Yellow).bold(),
        ));
        spans.push(Span::styled(format!("{what} "), dim()));
    }
    if !app.status.is_empty() {
        spans.push(Span::styled(
            format!("  {}", app.status),
            Style::new().add_modifier(Modifier::BOLD),
        ));
    }
    f.render_widget(Paragraph::new(Line::from(spans)), area);
}

fn add(f: &mut Frame, line: &str) {
    let area = centered(f.area(), 84, 16);
    f.render_widget(Clear, area);
    let (what, steps) = Source::of(line).plan();
    let mut lines = vec![
        Line::styled(
            "DOI, arXiv id, PMID, URL, a PDF's path, BibTeX or words",
            dim(),
        ),
        Line::from(vec![
            Span::styled("› ", Style::new().fg(Color::Yellow)),
            Span::raw(line.to_string()),
            Span::styled("▏", Style::new().fg(Color::Yellow)),
        ]),
        Line::raw(""),
    ];
    if !what.is_empty() {
        lines.push(Line::from(vec![
            Span::styled("reads as ", dim()),
            Span::styled(what, Style::new().fg(Color::Cyan).bold()),
        ]));
        for (n, step) in steps.iter().enumerate() {
            lines.push(Line::from(vec![
                Span::styled(format!("  {}. ", n + 1), dim()),
                Span::raw(*step),
            ]));
        }
    }
    let block = frame("Add paper", true)
        .title_bottom(Line::styled(" ⏎ add · ctrl-v paste · esc cancel ", dim()));
    f.render_widget(Paragraph::new(lines).block(block), area);
}

fn cite(f: &mut Frame, text: &str, scroll: u16) {
    let area = centered(f.area(), 90, 24);
    f.render_widget(Clear, area);
    let block = frame("BibTeX", true).title_bottom(Line::styled(
        " y copy · j/k scroll · any key closes ",
        dim(),
    ));
    f.render_widget(
        Paragraph::new(text.to_string())
            .wrap(Wrap { trim: false })
            .block(block)
            .scroll((scroll, 0)),
        area,
    );
}

fn help(f: &mut Frame) {
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
            "space",
            "mark a paper; * marks all shown, again unmarks; esc unmarks all",
        ),
        ("c", "BibTeX of the papers marked, or the one picked"),
        ("y", "copy [@key; @key] to the clipboard"),
        ("s", "star, written to the paper's record"),
        ("a", "add from a DOI, arXiv id, PMID, address, PDF or title"),
        (
            "r",
            "read again what paperstack serve, or another machine, added",
        ),
        ("q", "quit"),
    ];
    let area = centered(f.area(), 92, rows.len() as u16 + 2);
    f.render_widget(Clear, area);
    let lines: Vec<Line> = rows
        .iter()
        .map(|(k, what)| {
            Line::from(vec![
                Span::styled(format!(" {k:<9}"), Style::new().fg(Color::Yellow).bold()),
                Span::raw(*what),
            ])
        })
        .collect();
    f.render_widget(Paragraph::new(lines).block(frame("Keys", true)), area);
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

fn tilde(path: &str) -> String {
    match std::env::var("HOME") {
        Ok(home) if path.starts_with(&home) => format!("~{}", &path[home.len()..]),
        _ => path.to_string(),
    }
}
