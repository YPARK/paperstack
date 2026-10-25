//! The screen, laid out as a reference manager's: a search line, the library's
//! shelves on a tinted sidebar, the papers on the page, each with its box
//! to mark and its star, the paper picked on the right, and the keys at
//! the foot. Colours are the theme's.

use crate::app::{App, Busy, Choose, Doomed, Fetch, Focus, Popup, Shelf, Target, Then, View};
use crate::browser;
use crate::record::Record;
use crate::source::Source;
use crate::theme::Theme;
use ratatui::Frame;
use ratatui::layout::{Constraint, Layout, Rect};
use ratatui::style::{Color, Modifier, Style};
use ratatui::symbols::Marker;
use ratatui::text::{Line, Span, Text};
use ratatui::widgets::canvas::{Canvas, Points};
use ratatui::widgets::{
    Block, BorderType, Borders, Clear, List, ListItem, ListState, Padding, Paragraph, Wrap,
};

/// The badge of a paper without its PDF, to get it in the browser.
const GET_PDF: &str = " get PDF ";
/// The badge of a paper that has its PDF. The click target is this wide.
const HAS_PDF: &str = " PDF ";
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
    if app.view == View::Map && app.map.is_some() {
        let [canvas, panel] = Layout::horizontal([
            Constraint::Min(30),
            Constraint::Length(if wide { DETAILS } else { 0 }),
        ])
        .areas(Rect {
            width: list.width + detail.width,
            ..list
        });
        map_canvas(f, app, canvas);
        if wide {
            map_panel(f, app, panel);
        }
    } else {
        papers(f, app, list);
        if wide {
            details(f, app, detail);
        }
    }
    keys(f, app, foot);
    let popup = match &app.popup {
        Popup::Add(line) => Some(add(f, &t, line)),
        Popup::Cite(text, scroll) => Some(cite(f, &t, text, *scroll)),
        Popup::Help => Some(help(f, &t)),
        Popup::Fetch(fetch) => Some(fetch_dialog(f, &t, fetch)),
        Popup::Delete(doomed) => Some(delete(f, &t, doomed)),
        Popup::Choose(choose) => Some(chooser(f, &t, choose)),
        Popup::Busy(busy) => Some(busy_dialog(f, &t, busy)),
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
        Constraint::Length(52),
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
    let sort = format!("Sorted by {} ", app.sort.of(&app.wanted()).name());
    let count = format!("  {} of {} references ", app.shown.len(), app.total());
    let width = (sort.chars().count() + count.chars().count()) as u16;
    let start = right.x + right.width.saturating_sub(width);
    app.targets.push((
        Rect {
            x: start,
            width: (sort.chars().count() as u16).min(right.width),
            ..right
        },
        Target::Sort,
    ));
    f.render_widget(
        Paragraph::new(Line::from(vec![
            Span::styled(sort, Style::new().fg(t.accent).bold()),
            Span::styled(count, Style::new().fg(t.dim)),
        ]))
        .alignment(ratatui::layout::Alignment::Right),
        right,
    );
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
                Shelf::Folder(_) => ("▸", t.collection),
                Shelf::Label(_) => ("●", t.dim),
                Shelf::Theme(i, _) => ("◆", colour_of(*i)),
            };
            let room = (area.width as usize).saturating_sub(n.to_string().len() + 6);
            let name = fit(shelf.name(), room);
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

/// `A, B, C … n more`, a long list of authors shortened.
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

/// The papers, as many as the screen shows: only those are drawn, as a
/// library may hold tens of thousands.
fn papers(f: &mut Frame, app: &mut App, area: Rect) {
    let t = app.theme;
    let width = area.width.saturating_sub(4) as usize;
    let rule = Line::styled("─".repeat(area.width as usize), Style::new().fg(t.rule));
    // Each paper takes four lines; the first shown is kept, unless the
    // paper picked would be off the screen.
    let fits = (area.height.saturating_sub(1) / 4).max(1) as usize;
    let picked = app.list.selected().unwrap_or(0);
    let mut top = app.list.offset().min(app.shown.len().saturating_sub(1));
    if picked < top {
        top = picked;
    } else if picked >= top + fits {
        top = picked + 1 - fits;
    }
    let end = (top + fits + 1).min(app.shown.len());
    let (items, titles): (Vec<ListItem>, Vec<u16>) = app.shown[top..end]
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
            // The PDF's badge at the right; the key before
            // it, where there is room.
            let pdf = if h.has_pdf { HAS_PDF } else { GET_PDF };
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
    let mut state = ListState::default().with_selected(app.list.selected().map(|s| s - top));
    f.render_stateful_widget(list, area, &mut state);
    *app.list.offset_mut() = top;
    app.targets.push((area, Target::List));
    if app.total() == 0 {
        empty(f, &t, inner);
    }
    let heights = vec![4; titles.len()];
    for (at, row) in rows(inner, 0, &heights) {
        let i = top + at;
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
        app.targets.push((line(4 + titles[at], 2), Target::Star(i)));
        let badge = if app.shown[i].has_pdf {
            HAS_PDF
        } else {
            GET_PDF
        };
        let badge = badge.chars().count() as u16;
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

/// What an empty library says: how to fill it, from another library first.
fn empty(f: &mut Frame, t: &Theme, area: Rect) {
    let dim = Style::new().fg(t.dim);
    let key = Style::new().fg(t.title).bold();
    let lines = vec![
        Line::raw(""),
        Line::styled("  No papers yet.", Style::new().fg(t.title).bold()),
        Line::raw(""),
        Line::styled("  From another reference manager:", Style::new().bold()),
        Line::styled(
            "   1. export its library as JSON, with folders and labels",
            Style::new().fg(t.text),
        ),
        Line::styled(
            "   2. download its PDFs as a ZIP, and unzip it",
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
        field("on disk", folder),
        Line::from(vec![
            Span::styled(format!("{:<9}", "PDF"), dim),
            if h.has_pdf {
                Span::styled(HAS_PDF, Style::new().fg(t.on_badge).bg(t.badge))
            } else {
                Span::styled("none", dim)
            },
            Span::styled(
                if h.has_pdf {
                    "  ⏎ opens it · x deletes it"
                } else {
                    "  p gets it · f locates a file"
                },
                dim,
            ),
        ]),
        field("record", file),
        field("in", join(&m.folders)),
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

/// Themes' colours, told apart on a light page and a dark one alike.
const PALETTE: [Color; 12] = [
    Color::Rgb(31, 119, 180),
    Color::Rgb(214, 96, 39),
    Color::Rgb(44, 160, 44),
    Color::Rgb(200, 45, 60),
    Color::Rgb(148, 103, 189),
    Color::Rgb(140, 86, 75),
    Color::Rgb(219, 97, 176),
    Color::Rgb(110, 110, 110),
    Color::Rgb(170, 160, 20),
    Color::Rgb(23, 170, 190),
    Color::Rgb(90, 70, 200),
    Color::Rgb(60, 130, 90),
];

fn colour_of(theme: usize) -> Color {
    PALETTE[theme % PALETTE.len()]
}

/// `name` in `room` columns: a theme's first words alone where all do not
/// fit, else cut short with an ellipsis.
pub(crate) fn fit(name: &str, room: usize) -> String {
    if name.chars().count() <= room {
        return name.to_string();
    }
    match name.split_once(" · ") {
        Some((first, _)) if first.chars().count() <= room => first.to_string(),
        _ => {
            let mut cut: String = name.chars().take(room.saturating_sub(1)).collect();
            cut.push('…');
            cut
        }
    }
}

/// The papers as points on the plane: the picked theme's in its colour,
/// over the rest in grey, or each theme in its own where all are coloured;
/// the papers new since the map, to be asked of, as crosses. Themes are
/// named where their names do not run into one another's, the picked
/// one's always.
fn map_canvas(f: &mut Frame, app: &mut App, area: Rect) {
    let t = app.theme;
    let Some(m) = &mut app.map else { return };
    m.canvas = area;
    app.targets.push((area, Target::Map));
    let map = m.map();
    let (picked, all) = (m.picked, m.colours);
    let bounds = m.bounds();
    let [x0, x1, y0, y1] = bounds;
    let mut by_theme: Vec<Vec<(f64, f64)>> = vec![Vec::new(); map.themes.len()];
    for (p, th) in map.xy.iter().zip(&map.theme) {
        if let Some(th) = th {
            by_theme[*th].push((p[0] as f64, p[1] as f64));
        }
    }
    let new: Vec<(f64, f64)> = m
        .kept
        .placed
        .iter()
        .map(|p| (p.xy[0] as f64, p.xy[1] as f64))
        .collect();
    let labels = place_labels(map, picked, area, bounds);
    let named = map.named();
    let canvas = Canvas::default()
        .background_color(t.page)
        .marker(Marker::Braille)
        .x_bounds([x0, x1])
        .y_bounds([y0, y1])
        .paint(|ctx| {
            for (i, coords) in by_theme.iter().enumerate().filter(|(i, _)| *i != picked) {
                let color = if all && i < named {
                    colour_of(i)
                } else {
                    t.dim
                };
                ctx.draw(&Points { coords, color });
            }
            ctx.layer();
            if let Some(coords) = by_theme.get(picked) {
                ctx.draw(&Points {
                    coords,
                    color: colour_of(picked),
                });
            }
            ctx.layer();
            for &(x, y) in &new {
                ctx.print(x, y, Span::styled("+", Style::new().fg(t.star).bold()));
            }
            for &(i, x, y) in &labels {
                let style = if i == picked {
                    Style::new().fg(t.on_accent).bg(colour_of(i)).bold()
                } else if all {
                    Style::new().fg(colour_of(i)).bold()
                } else {
                    Style::new().fg(t.text).bold()
                };
                ctx.print(
                    x,
                    y,
                    Span::styled(format!(" {} ", map.themes[i].name()), style),
                );
            }
        });
    f.render_widget(canvas, area);
}

/// Where each theme's name goes, as `(theme, x, y)` on the plane: at the
/// middle of its papers, or a row or two off it, where it runs into no
/// name placed before it; the picked theme's first, then the largest's.
fn place_labels(
    map: &crate::map::Map,
    picked: usize,
    area: Rect,
    [x0, x1, y0, y1]: [f64; 4],
) -> Vec<(usize, f64, f64)> {
    let (w, h) = (f64::from(area.width.max(1)), f64::from(area.height.max(1)));
    let mut order: Vec<usize> = (0..map.named()).collect();
    order.sort_by_key(|&i| (i != picked, i));
    // Taken cells, by row, as spans of columns.
    let mut taken: Vec<(i64, i64, i64)> = Vec::new();
    let mut out = Vec::new();
    for i in order {
        let th = &map.themes[i];
        let len = th.name().chars().count() as i64 + 2;
        let col = ((th.centre[0] as f64 - x0) / (x1 - x0) * w) as i64 - len / 2;
        let col = col.clamp(0, (w as i64 - len).max(0));
        let row = ((y1 - th.centre[1] as f64) / (y1 - y0) * h) as i64;
        let free = |r: i64| {
            (0..h as i64).contains(&r)
                && !taken
                    .iter()
                    .any(|&(tr, a, b)| tr == r && col < b + 1 && a < col + len + 1)
        };
        let Some(r) = [0, -1, 1, -2, 2]
            .iter()
            .map(|d| row + d)
            .find(|&r| free(r) || i == picked)
        else {
            continue;
        };
        taken.push((r, col, col + len));
        let x = x0 + (col as f64 + 0.5) / w * (x1 - x0);
        let y = y1 - (r as f64 + 0.5) / h * (y1 - y0);
        out.push((i, x, y));
    }
    out
}

/// The theme picked: its words, its authors of most papers, its newest
/// papers; and where the papers new since the map would go.
fn map_panel(f: &mut Frame, app: &App, area: Rect) {
    let t = app.theme;
    let Some(m) = &app.map else { return };
    let map = m.map();
    let Some(th) = map.themes.get(m.picked) else {
        return;
    };
    let named = map.named();
    let mut lines = vec![
        Line::from(vec![
            Span::styled("◆ ", Style::new().fg(colour_of(m.picked))),
            Span::styled(th.name(), Style::new().fg(t.title).bold()),
        ]),
        Line::styled(
            format!("theme {} of {named} · {} papers", m.picked + 1, th.size),
            Style::new().fg(t.dim),
        ),
        Line::raw(""),
        Line::styled("WORDS", Style::new().fg(t.dim).bold()),
        Line::styled(th.words.join(" · "), Style::new().fg(t.venue)),
        Line::raw(""),
        Line::styled("AUTHORS", Style::new().fg(t.dim).bold()),
    ];
    for (a, n) in &th.authors {
        lines.push(Line::from(vec![
            Span::styled(a.clone(), Style::new().fg(t.text)),
            Span::styled(format!(" {n}"), Style::new().fg(t.dim)),
        ]));
    }
    lines.push(Line::raw(""));
    lines.push(Line::styled("NEWEST", Style::new().fg(t.dim).bold()));
    let mut papers: Vec<_> = map
        .members(m.picked)
        .filter_map(|(_, k)| app.index.get(k).ok().flatten())
        .collect();
    papers.sort_by_key(|h| std::cmp::Reverse(h.record.year()));
    for h in papers.iter().take(8) {
        let year = h
            .record
            .year()
            .map_or_else(|| "n.d.".to_string(), |y| y.to_string());
        lines.push(Line::from(vec![
            Span::styled(format!("{year} "), Style::new().fg(t.dim)),
            Span::styled(h.record.title().to_string(), Style::new().fg(t.text)),
        ]));
    }
    if !m.kept.placed.is_empty() {
        lines.push(Line::raw(""));
        lines.push(Line::styled(
            format!("NEW SINCE THE MAP · {}", m.kept.placed.len()),
            Style::new().fg(t.star).bold(),
        ));
        for p in m.kept.placed.iter().take(6) {
            lines.push(Line::from(vec![
                Span::styled(format!("{} → ", p.key), Style::new().fg(t.text)),
                Span::styled(
                    map.themes[p.theme].name(),
                    Style::new().fg(colour_of(p.theme)),
                ),
                Span::styled(format!(" {:.0}%", p.share * 100.0), Style::new().fg(t.dim)),
            ]));
        }
        lines.push(Line::styled(
            "a assigns them · R gathers again",
            Style::new().fg(t.hint),
        ));
    }
    lines.push(Line::raw(""));
    lines.push(Line::styled(
        format!("built {}", map.built),
        Style::new().fg(t.dim),
    ));
    f.render_widget(
        Paragraph::new(lines)
            .wrap(Wrap { trim: true })
            .block(
                Block::new()
                    .borders(Borders::LEFT)
                    .border_style(Style::new().fg(t.rule))
                    .padding(Padding::new(2, 1, 1, 0)),
            )
            .style(Style::new().bg(t.page)),
        area,
    );
}

fn keys(f: &mut Frame, app: &App, area: Rect) {
    let t = app.theme;
    let mut spans = Vec::new();
    let map_keys = [
        ("← →", "theme"),
        ("⏎", "its papers"),
        ("a", "assign new"),
        ("c", "colours"),
        ("R", "rebuild"),
        ("u", "previous"),
        ("^G m", "list"),
        ("?", "help"),
        ("q", "quit"),
    ];
    let list_keys = [
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
        ("^G", "map"),
        ("?", "help"),
        ("q", "quit"),
    ];
    let shown: &[(&str, &str)] = if app.view == View::Map {
        &map_keys
    } else {
        &list_keys
    };
    for &(k, what) in shown {
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

/// A box over the screen.
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
    let hint = Style::new().fg(t.hint);
    let mut lines = vec![
        Line::styled(
            "DOI, arXiv id, PMID, address, a PDF's path, a title, or a library's export",
            hint,
        ),
        Line::from(vec![
            Span::styled(" › ", Style::new().fg(t.accent).bg(t.field)),
            Span::styled(format!("{line}▏"), Style::new().fg(t.text).bg(t.field)),
        ]),
        Line::raw(""),
    ];
    if !what.is_empty() {
        lines.push(Line::from(vec![
            Span::styled("reads as ", hint),
            Span::styled(what, Style::new().fg(t.title).bold()),
        ]));
        for (n, step) in steps.iter().enumerate() {
            lines.push(Line::from(vec![
                Span::styled(format!("  {}. ", n + 1), hint),
                Span::raw(*step),
            ]));
        }
    }
    let block = dialog(t, "Add paper")
        .title_bottom(Line::styled(" ⏎ add · ctrl-v paste · esc cancel ", hint));
    f.render_widget(Paragraph::new(lines).block(block), area);
    area
}

/// The papers a PDF holds, to pick the one it is filed as.
fn chooser(f: &mut Frame, t: &Theme, choose: &Choose) -> Rect {
    let hint = Style::new().fg(t.hint);
    let mut lines = vec![
        Line::raw(""),
        Line::styled(clip(&choose.name, 84), hint),
        Line::raw("Its first page holds more than one paper. Which is it to be filed as?"),
        Line::raw(""),
    ];
    for (n, p) in choose.papers.iter().enumerate() {
        let picked = n == choose.at;
        let style = if picked {
            Style::new().fg(t.title).bold()
        } else {
            Style::new().fg(t.text)
        };
        let mark = if picked { "›" } else { " " };
        lines.push(Line::from(vec![
            Span::styled(format!("{mark} {}. ", n + 1), Style::new().fg(t.accent)),
            Span::styled(clip(&crate::add::listed(p), 80), style),
        ]));
    }
    let area = centered(f.area(), 92, lines.len() as u16 + 3);
    f.render_widget(Clear, area);
    let block = dialog(t, "Which paper?")
        .title_bottom(Line::styled(" j/k or 1–9 pick · ⏎ add · esc cancel ", hint));
    f.render_widget(Paragraph::new(lines).block(block), area);
    area
}

/// A paper's PDF, awaited from the browser.
fn fetch_dialog(f: &mut Frame, t: &Theme, fetch: &Fetch) -> Rect {
    let area = centered(f.area(), 88, 15);
    f.render_widget(Clear, area);
    let hint = Style::new().fg(t.hint);
    let mut lines = vec![
        Line::styled(fetch.title.clone(), Style::new().fg(t.title).bold()),
        Line::raw(""),
        Line::from(vec![
            Span::styled("opened  ", hint),
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
            "Or paste a path or address, press ctrl-l to locate a file, or the button in Chrome.",
            hint,
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
        " ⏎ take it · ctrl-v paste · ctrl-l a file on disk · ctrl-o the page · esc stop ",
        hint,
    ));
    f.render_widget(
        Paragraph::new(lines)
            .block(block)
            .wrap(Wrap { trim: false }),
        area,
    );
    area
}

/// Steps said of work on its own thread shown, the latest of them.
const STEPS: usize = 6;

/// Work on its own thread, as a download: what it is for, how long it has
/// taken, and what it has said, the latest last.
fn busy_dialog(f: &mut Frame, t: &Theme, busy: &Busy) -> Rect {
    const SPINNER: [char; 10] = ['⠋', '⠙', '⠹', '⠸', '⠼', '⠴', '⠦', '⠧', '⠇', '⠏'];
    let area = centered(f.area(), 88, STEPS as u16 + 7);
    f.render_widget(Clear, area);
    let hint = Style::new().fg(t.hint);
    let since = busy.since.elapsed();
    let spin = SPINNER[(since.as_millis() / 100) as usize % SPINNER.len()];
    let stopping = busy.stop.load(std::sync::atomic::Ordering::Relaxed);
    let mut lines = vec![
        Line::styled(clip(&busy.what, 84), Style::new().fg(t.title).bold()),
        Line::raw(""),
        Line::from(vec![
            Span::styled(format!("{spin} "), Style::new().fg(t.accent).bold()),
            Span::styled(
                if stopping {
                    "stopping after this step".to_string()
                } else {
                    "working".to_string()
                },
                Style::new().fg(t.accent),
            ),
            Span::styled(format!("  {} s", since.as_secs()), hint),
        ]),
    ];
    let from = busy.said.len().saturating_sub(STEPS);
    let last = busy.said.len().saturating_sub(1);
    for (i, s) in busy.said.iter().enumerate().skip(from) {
        let style = if i == last {
            Style::new().fg(t.text)
        } else {
            hint
        };
        lines.push(Line::styled(clip(s, 84), style));
    }
    let keys = match busy.then {
        Then::Fetch(_) => " b the browser now · esc stop ",
        Then::Add => " the screen is yours again when it is done ",
    };
    let block = dialog(t, busy.title).title_bottom(Line::styled(keys, hint));
    f.render_widget(Paragraph::new(lines).block(block), area);
    area
}

/// What is to be deleted, asked first.
fn delete(f: &mut Frame, t: &Theme, doomed: &Doomed) -> Rect {
    let hint = Style::new().fg(t.hint);
    let pdfs = matches!(doomed, Doomed::Pdfs(_));
    let (title, mut lines) = match doomed {
        Doomed::Papers(papers) | Doomed::Pdfs(papers) => {
            let mut lines: Vec<Line> = papers
                .iter()
                .take(10)
                .map(|(key, title)| {
                    Line::from(vec![
                        Span::styled(format!("{key:<24} "), Style::new().fg(t.title).bold()),
                        Span::raw(clip(title, 62)),
                    ])
                })
                .collect();
            if papers.len() > 10 {
                lines.push(Line::styled(
                    format!("and {} more", papers.len() - 10),
                    hint,
                ));
            }
            lines.push(Line::raw(""));
            let (title, note) = if pdfs {
                if papers.len() == 1 {
                    (
                        "Delete this PDF?".to_string(),
                        "The paper stays. Its PDF is moved to the library's .trash/ folder.",
                    )
                } else {
                    (
                        format!("Delete {} PDFs?", papers.len()),
                        "The papers stay. Their PDFs are moved to the library's .trash/ folder.",
                    )
                }
            } else if papers.len() == 1 {
                (
                    "Delete this paper?".to_string(),
                    "Its record and PDF are moved to the library's .trash/ folder.",
                )
            } else {
                (
                    format!("Delete {} papers?", papers.len()),
                    "Their records and PDFs are moved to the library's .trash/ folder.",
                )
            };
            lines.push(Line::styled(note, hint));
            (title, lines)
        }
        Doomed::Shelf(kind, name, count) => {
            let what = match kind {
                crate::remove::Shelf::Collection => ", as are the collections within it",
                crate::remove::Shelf::Label => "",
            };
            let lines = vec![
                Line::styled(name.clone(), Style::new().fg(t.title).bold()),
                Line::raw(""),
                Line::raw(format!(
                    "Its {count} papers stay in the library: it is taken off them{what}."
                )),
            ];
            (format!("Delete this {}?", kind.name()), lines)
        }
    };
    lines.insert(0, Line::raw(""));
    let area = centered(f.area(), 92, lines.len() as u16 + 3);
    f.render_widget(Clear, area);
    let block =
        dialog(t, &title).title_bottom(Line::styled(" y delete · any other key keeps it ", hint));
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
        Style::new().fg(t.hint),
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
            "sort: relevance, date added, year, first author, title; or click it",
        ),
        (
            "g G",
            "the first paper, the last; home end, ctrl-↑ ctrl-↓ too",
        ),
        ("⏎ o", "open the PDF in ohp; quit ohp to come back"),
        (
            "p",
            "get the PDF: an open copy, else in your browser, signed in, to download",
        ),
        (
            "f",
            "locate a PDF on disk for the paper picked, where it has none",
        ),
        (
            "ctrl-l",
            "the same, from the get-PDF dialog, instead of a download",
        ),
        (
            "x",
            "delete the PDF picked, or those marked, to .trash/; papers stay",
        ),
        (
            "^G m",
            "the map of themes, and back; ← → a theme, ⏎ its papers, a R u",
        ),
        (
            "space",
            "mark a paper; * marks all shown, again unmarks; esc unmarks all",
        ),
        ("ctrl-e", "BibTeX of the papers marked, or the one picked"),
        (
            "click",
            "pick; on a box, mark; on a star, star; on PDF, get or delete; twice, open",
        ),
        ("y", "copy [@key; @key] to the clipboard"),
        ("s", "star, written to the paper's record"),
        (
            "d",
            "delete the papers marked, or the one picked, to .trash/; asked first",
        ),
        ("", "on the left, a collection or label: its papers stay"),
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
