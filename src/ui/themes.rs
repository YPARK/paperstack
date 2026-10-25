//! The map of the library's themes as drawn: its papers, the names of
//! its themes, a theme from within with its links, and a theme's card.

use super::*;

/// The papers as points on the plane: the picked theme's in its colour,
/// over the rest in grey, or each theme in its own where all are coloured;
/// the papers new since the map, to be asked of, as crosses. Themes are
/// named where their names do not run into one another's, the picked
/// one's always. Within a theme, its own map: its finer themes, each in
/// its colour, the links among its papers beneath, the paper picked marked.
pub(super) fn map_canvas(f: &mut Frame, app: &mut App, area: Rect) {
    let t = app.theme;
    let Some(m) = &mut app.map else { return };
    let mut area = area;
    if let Some(inside) = m.inside {
        // The theme gone into, as a chip in its colour, and how to go on.
        let th = &m.map().themes[inside];
        let chip = Style::new().fg(t.on_accent).bg(colour_of(inside)).bold();
        banner(
            f,
            &mut area,
            vec![
                Span::styled(format!(" ◆ {} ", th.name()), chip),
                Span::styled(
                    format!(" {} papers, in finer themes", th.size),
                    Style::new().fg(t.text).bold(),
                ),
                Span::styled(
                    "  · click one · ⏎ its card · ← → the next theme · esc out",
                    Style::new().fg(t.hint),
                ),
            ],
            t.page,
        );
    }
    // The papers new since the map, on the whole map: a line to see where
    // each would go.
    if m.inside.is_none() && !m.kept.placed.is_empty() {
        let at = Rect { height: 1, ..area };
        banner(
            f,
            &mut area,
            vec![
                Span::styled(" + ", Style::new().fg(t.star).bold()),
                Span::styled(
                    format!("{} papers new since the map", m.kept.placed.len()),
                    Style::new().fg(t.star).bold(),
                ),
                Span::styled(
                    "  · N, or a click here, where each would go · A puts them there",
                    Style::new().fg(t.hint),
                ),
            ],
            t.page,
        );
        app.targets
            .push((at, Target::Key(crate::app::keys("N")[0])));
    }
    m.canvas = area;
    app.targets.push((area, Target::Map));
    let within = m.within();
    let map = m.shown();
    let picked = m.top().picked;
    // Within a theme, none picked till a finer theme is chosen.
    let shows_picked = within.is_none_or(|w| w.chosen);
    // Within a theme, every finer theme in its colour till one is chosen.
    let all = m.colours || within.is_some_and(|w| !w.chosen);
    // Its links; a finer theme chosen, its own alone.
    let chosen = within.filter(|w| w.chosen).map(|w| w.lens.picked);
    let links: Vec<(usize, usize)> = within
        .map(|w| w.links.as_slice())
        .unwrap_or_default()
        .iter()
        .copied()
        .filter(|&(i, j)| chosen.is_none_or(|c| map.theme[i] == Some(c) && map.theme[j] == Some(c)))
        .collect();
    let paper = within.and_then(|w| w.lens.paper).map(|i| map.xy[i]);
    let bounds = m.bounds();
    let [x0, x1, y0, y1] = bounds;
    let mut by_theme: Vec<Vec<(f64, f64)>> = vec![Vec::new(); map.themes.len()];
    for (p, th) in map.xy.iter().zip(&map.theme) {
        if let Some(th) = th {
            by_theme[*th].push((p[0] as f64, p[1] as f64));
        }
    }
    // The papers new since the map, on the whole map only.
    let placed: &[crate::map::Placing] = if within.is_some() {
        &[]
    } else {
        &m.kept.placed
    };
    let new: Vec<(f64, f64)> = placed
        .iter()
        .map(|p| (p.xy[0] as f64, p.xy[1] as f64))
        .collect();
    let labels = place_labels(map, picked, area, bounds);
    let named = map.named();
    // What the search finds, on the map and new since, each at its own
    // place, one to a cell of the screen, as the canvas puts it there.
    let found: Option<Vec<(f64, f64)>> = app.found.as_ref().map(|found| {
        let on_map = map
            .keys
            .iter()
            .zip(&map.xy)
            .zip(&map.theme)
            .filter(|((k, _), t)| t.is_some() && found.contains(*k))
            .map(|((_, p), _)| *p);
        let new = placed
            .iter()
            .filter(|p| found.contains(&p.key))
            .map(|p| p.xy);
        let mut cells = std::collections::HashSet::new();
        on_map
            .chain(new)
            .map(|p| (p[0] as f64, p[1] as f64))
            .filter(|&(x, y)| {
                let col = ((x - x0) / (x1 - x0) * f64::from(area.width)).round() as i64;
                let row = ((y1 - y) / (y1 - y0) * f64::from(area.height)).round() as i64;
                cells.insert((col, row))
            })
            .collect()
    });
    // A theme in its colour: the picked one, or every one named where all
    // are; none while a search is shown, over the rest in grey.
    let colour = |i: usize| {
        if found.is_none() && (i == picked || all && i < named) {
            colour_of(i)
        } else {
            t.dim
        }
    };
    let xy = |i: usize| (f64::from(map.xy[i][0]), f64::from(map.xy[i][1]));
    let canvas = Canvas::default()
        .background_color(t.page)
        .marker(Marker::Braille)
        .x_bounds([x0, x1])
        .y_bounds([y0, y1])
        .paint(|ctx| {
            for &(i, j) in &links {
                let ((ax, ay), (bx, by)) = (xy(i), xy(j));
                ctx.draw(&ratatui::widgets::canvas::Line {
                    x1: ax,
                    y1: ay,
                    x2: bx,
                    y2: by,
                    color: t.rule,
                });
            }
            ctx.layer();
            for (i, coords) in by_theme.iter().enumerate().filter(|(i, _)| *i != picked) {
                ctx.draw(&Points {
                    coords,
                    color: colour(i),
                });
            }
            ctx.layer();
            if let Some(coords) = by_theme.get(picked) {
                ctx.draw(&Points {
                    coords,
                    color: colour(picked),
                });
            }
            ctx.layer();
            // The papers found, a dot to be seen among the rest.
            for &(x, y) in found.iter().flatten() {
                ctx.print(x, y, Span::styled("●", Style::new().fg(t.star).bold()));
            }
            ctx.layer();
            for &(x, y) in &new {
                ctx.print(x, y, Span::styled("+", Style::new().fg(t.star).bold()));
            }
            if let Some(p) = paper {
                ctx.print(
                    f64::from(p[0]),
                    f64::from(p[1]),
                    Span::styled("◉", Style::new().fg(t.accent).bold()),
                );
            }
        });
    f.render_widget(canvas, area);
    // Each theme's name over the map, in the cells it is clicked in.
    for (i, col, row, len) in labels {
        let style = if i == picked && shows_picked {
            Style::new().fg(t.on_accent).bg(colour_of(i)).bold()
        } else if all {
            Style::new().fg(colour_of(i)).bold()
        } else {
            Style::new().fg(t.text).bold()
        };
        let at = Rect {
            x: area.x + col,
            y: area.y + row,
            width: len.min(area.width.saturating_sub(col)),
            height: 1,
        };
        let name = format!(" {} ", map.themes[i].name());
        f.render_widget(Paragraph::new(Span::styled(name, style)), at);
        app.targets.push((at, Target::Label(i)));
    }
}

/// Where each theme's name goes, as `(theme, column, row, width)` on the
/// screen: at the
/// middle of its papers, or a row or two off it, where it runs into no
/// name placed before it; the picked theme's first, then the largest's.
pub(super) fn place_labels(
    map: &crate::map::Map,
    picked: usize,
    area: Rect,
    [x0, x1, y0, y1]: [f64; 4],
) -> Vec<(usize, u16, u16, u16)> {
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
        out.push((i, col as u16, r as u16, len as u16));
    }
    out
}

/// A theme's card, or a finer theme's: its name in its colour, what it is,
/// its words and authors, what a search finds of it, and its papers,
/// newest first, the one picked to open in ohp, each clicked where drawn.
pub(super) fn card(f: &mut Frame, app: &mut App) {
    let t = app.theme;
    let Popup::Card(card) = &app.popup else {
        return;
    };
    let area = centered(f.area(), 100, f.area().height.saturating_sub(4));
    f.render_widget(Clear, area);
    // The card's own area beneath its papers, so a click on one finds it.
    app.targets.push((area, Target::Popup));
    let width = usize::from(area.width.saturating_sub(4));
    let hint = Style::new().fg(t.hint);
    let label = Style::new().fg(t.dim).bold();
    let found = |k: &str| app.found.as_ref().is_some_and(|f| f.contains(k));
    let authors: Vec<String> = card
        .authors
        .iter()
        .map(|(a, n)| format!("{a} {n}"))
        .collect();
    let mut head = vec![Line::styled(card.about.clone(), hint), Line::raw("")];
    if !card.words.is_empty() {
        head.push(Line::from(vec![
            Span::styled("words    ", label),
            Span::styled(
                clip(&card.words.join(" · "), width.saturating_sub(9)),
                Style::new().fg(t.venue),
            ),
        ]));
        head.push(Line::from(vec![
            Span::styled("authors  ", label),
            Span::styled(
                clip(&authors.join(", "), width.saturating_sub(9)),
                Style::new().fg(t.text),
            ),
        ]));
    }
    if app.found.is_some() {
        let n = card.papers.iter().filter(|h| found(&h.record.key)).count();
        head.push(Line::from(vec![
            Span::styled("found    ", label),
            Span::styled(
                format!("● {n} of them by “{}”", app.query.trim()),
                Style::new().fg(t.star).bold(),
            ),
        ]));
    }
    head.push(Line::styled("─".repeat(width), Style::new().fg(t.rule)));
    let rows = usize::from(area.height.saturating_sub(2)).saturating_sub(head.len());
    // The papers, as far down as the one picked.
    let from = card.at.saturating_sub(rows.saturating_sub(1));
    let top = area.y + 1 + head.len() as u16;
    let mut lines = head;
    for (i, h) in card.papers.iter().enumerate().skip(from).take(rows) {
        let year = h
            .record
            .year()
            .map_or_else(|| "n.d.".to_string(), |y| y.to_string());
        let mark = if found(&h.record.key) { "● " } else { "  " };
        let pdf = if h.has_pdf { HAS_PDF } else { "" };
        let note = card.notes.get(i).map_or(String::new(), |n| format!(" {n}"));
        let title = clip(
            h.record.title(),
            width.saturating_sub(year.len() + 4 + pdf.len() + note.chars().count()),
        );
        let row = if i == card.at {
            Style::new().bg(t.accent).fg(t.on_accent)
        } else {
            Style::new().fg(t.text)
        };
        lines.push(
            Line::from(vec![
                Span::styled(mark, Style::new().fg(t.star)),
                Span::styled(format!("{year} "), Style::new().fg(t.dim)),
                Span::raw(format!("{title} ")),
                Span::styled(pdf, Style::new().fg(t.on_badge).bg(t.badge).bold()),
                Span::styled(note, Style::new().fg(t.star)),
            ])
            .style(row),
        );
        app.targets.push((
            Rect {
                x: area.x + 1,
                y: top + (i - from) as u16,
                width: area.width.saturating_sub(2),
                height: 1,
            },
            Target::CardPaper(i),
        ));
    }
    let title = format!("◆ {}", fit(&card.name, width.saturating_sub(4)));
    let block = dialog(&t, &title)
        .border_style(Style::new().fg(colour_of(card.theme)))
        .title_bottom(Line::styled(
            " ⏎ or a double click opens in ohp · ← → the next theme · l in the list · esc close ",
            hint,
        ));
    f.render_widget(Paragraph::new(lines).block(block), area);
}

/// A line of `spans` at the top of `area`, on `bg`, the rest of `area`
/// below it.
pub(super) fn banner(f: &mut Frame, area: &mut Rect, spans: Vec<Span>, bg: Color) {
    f.render_widget(
        Paragraph::new(Line::from(spans)).style(Style::new().bg(bg)),
        Rect { height: 1, ..*area },
    );
    area.y += 1;
    area.height = area.height.saturating_sub(1);
}
