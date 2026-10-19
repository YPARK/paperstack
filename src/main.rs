//! paperstack: a library of papers in the terminal, kept in a folder (as in
//! Dropbox) with each PDF and its record under `year/lastname/firstname/`.
//!
//! This is a mock-up: the papers are those in `mock.rs`, nothing is fetched,
//! and nothing is written but the clipboard. PDFs are opened with ohp, from
//! `$PAPERSTACK_LIBRARY` (`~/Dropbox/paperstack` without it), at the path
//! `layout.rs` gives.

mod bib;
mod layout;
mod mock;
mod ui;

use mock::Paper;
use ratatui::DefaultTerminal;
use ratatui::crossterm::event::{self, Event, KeyCode, KeyEvent, KeyEventKind, KeyModifiers};
use ratatui::widgets::ListState;
use std::collections::BTreeSet;
use std::io::Write;
use std::path::PathBuf;
use std::process::{Command, Stdio};

#[derive(Clone, Copy, PartialEq)]
pub enum Focus {
    Side,
    List,
}

/// What the sidebar lists: what narrows the papers, under headings.
#[derive(Clone, Copy, PartialEq)]
pub enum Shelf {
    Heading(&'static str),
    All,
    Starred,
    NoPdf,
    Folder(&'static str),
    Label(&'static str),
    Project(&'static str),
}

impl Shelf {
    fn holds(self, p: &Paper) -> bool {
        match self {
            Shelf::Heading(_) | Shelf::All => true,
            Shelf::Starred => p.starred,
            Shelf::NoPdf => !p.pdf,
            Shelf::Folder(f) => p.folders.contains(&f),
            Shelf::Label(l) => p.labels.contains(&l),
            Shelf::Project(d) => p.cited_in.contains(&d),
        }
    }
}

pub enum Popup {
    None,
    /// What is typed, as a DOI, arXiv id, PMID, URL, PDF or BibTeX.
    Add(String),
    /// BibTeX shown, and how far it is scrolled.
    Cite(String, u16),
    Help,
}

pub struct App {
    pub root: PathBuf,
    pub papers: Vec<Paper>,
    pub shelves: Vec<Shelf>,
    pub shelf: usize,
    pub focus: Focus,
    pub query: String,
    pub searching: bool,
    /// The papers shown, by index into `papers`, newest added first.
    pub shown: Vec<usize>,
    pub list: ListState,
    pub marked: BTreeSet<usize>,
    pub popup: Popup,
    pub status: String,
    quit: bool,
}

impl App {
    fn new() -> Self {
        let root = std::env::var_os("PAPERSTACK_LIBRARY").map_or_else(
            || {
                PathBuf::from(std::env::var_os("HOME").unwrap_or_default())
                    .join("Dropbox/paperstack")
            },
            PathBuf::from,
        );
        let mut shelves = vec![
            Shelf::Heading("MY LIBRARY"),
            Shelf::All,
            Shelf::Starred,
            Shelf::NoPdf,
            Shelf::Heading("FOLDERS"),
        ];
        shelves.extend(mock::FOLDERS.iter().map(|&f| Shelf::Folder(f)));
        shelves.push(Shelf::Heading("LABELS"));
        shelves.extend(mock::LABELS.iter().map(|&l| Shelf::Label(l)));
        shelves.push(Shelf::Heading("CITED IN"));
        shelves.extend(mock::PROJECTS.iter().map(|&d| Shelf::Project(d)));
        let mut app = App {
            root,
            papers: mock::papers(),
            shelves,
            shelf: 1,
            focus: Focus::List,
            query: String::new(),
            searching: false,
            shown: Vec::new(),
            list: ListState::default(),
            marked: BTreeSet::new(),
            popup: Popup::None,
            status: "mock-up: nothing is fetched or written".to_string(),
            quit: false,
        };
        app.narrow();
        app
    }

    pub fn current(&self) -> Option<&Paper> {
        let at = self.list.selected()?;
        self.shown.get(at).map(|&i| &self.papers[i])
    }

    pub fn pdf(&self, p: &Paper) -> PathBuf {
        self.root.join(layout::pdf(p))
    }

    /// Show the papers on the shelf that every word of the query is in.
    fn narrow(&mut self) {
        let shelf = self.shelves[self.shelf];
        let words: Vec<String> = self
            .query
            .to_lowercase()
            .split_whitespace()
            .map(str::to_string)
            .collect();
        self.shown = (0..self.papers.len())
            .filter(|&i| {
                let p = &self.papers[i];
                shelf.holds(p) && {
                    let hay = haystack(p);
                    words.iter().all(|w| hay.contains(w.as_str()))
                }
            })
            .collect();
        let at = self.list.selected().unwrap_or(0);
        self.list
            .select((!self.shown.is_empty()).then(|| at.min(self.shown.len() - 1)));
    }

    fn key(&mut self, key: KeyEvent, terminal: &mut DefaultTerminal) {
        let ctrl = key.modifiers.contains(KeyModifiers::CONTROL);
        if ctrl && key.code == KeyCode::Char('c') {
            self.quit = true;
            return;
        }
        match &mut self.popup {
            Popup::Add(line) => {
                match key.code {
                    KeyCode::Esc => self.popup = Popup::None,
                    KeyCode::Enter => {
                        let (what, _) = bib::Source::of(line).plan();
                        self.status = if what.is_empty() {
                            "nothing to add".to_string()
                        } else {
                            format!("mock: would add {what}")
                        };
                        self.popup = Popup::None;
                    }
                    KeyCode::Backspace => {
                        line.pop();
                    }
                    KeyCode::Char('u') if ctrl => line.clear(),
                    KeyCode::Char('v') if ctrl => line.push_str(paste().trim()),
                    KeyCode::Char(c) => line.push(c),
                    _ => {}
                }
                return;
            }
            Popup::Cite(text, scroll) => {
                match key.code {
                    KeyCode::Char('j') | KeyCode::Down => *scroll = scroll.saturating_add(1),
                    KeyCode::Char('k') | KeyCode::Up => *scroll = scroll.saturating_sub(1),
                    KeyCode::Char('y') => {
                        let n = text.matches("\n@").count() + 1;
                        let text = text.clone();
                        self.status = copy(&text, &format!("{n} BibTeX entries"));
                        self.popup = Popup::None;
                    }
                    _ => self.popup = Popup::None,
                }
                return;
            }
            Popup::Help => {
                self.popup = Popup::None;
                return;
            }
            Popup::None => {}
        }
        if self.searching {
            match key.code {
                KeyCode::Esc => {
                    self.query.clear();
                    self.searching = false;
                }
                KeyCode::Enter | KeyCode::Down | KeyCode::Up => self.searching = false,
                KeyCode::Backspace => {
                    self.query.pop();
                }
                KeyCode::Char('u') if ctrl => self.query.clear(),
                KeyCode::Char(c) => self.query.push(c),
                _ => {}
            }
            self.narrow();
            return;
        }
        match key.code {
            KeyCode::Char('q') => self.quit = true,
            KeyCode::Char('?') => self.popup = Popup::Help,
            KeyCode::Char('/') => {
                self.searching = true;
                self.focus = Focus::List;
            }
            KeyCode::Esc if !self.query.is_empty() => {
                self.query.clear();
                self.narrow();
            }
            KeyCode::Tab | KeyCode::BackTab => {
                self.focus = match self.focus {
                    Focus::Side => Focus::List,
                    Focus::List => Focus::Side,
                }
            }
            KeyCode::Char('h') | KeyCode::Left => self.focus = Focus::Side,
            KeyCode::Char('l') | KeyCode::Right => self.focus = Focus::List,
            KeyCode::Char('j') | KeyCode::Down => self.step(1),
            KeyCode::Char('k') | KeyCode::Up => self.step(-1),
            KeyCode::PageDown => self.step(5),
            KeyCode::PageUp => self.step(-5),
            KeyCode::Char('a') => {
                let clip = paste();
                let clip = clip.trim();
                let line = if !clip.contains('\n')
                    && !matches!(bib::Source::of(clip), bib::Source::Words(_))
                {
                    clip.to_string()
                } else {
                    String::new()
                };
                self.popup = Popup::Add(line);
            }
            KeyCode::Char(' ') => {
                if let Some(at) = self.list.selected().and_then(|at| self.shown.get(at)) {
                    if !self.marked.remove(at) {
                        self.marked.insert(*at);
                    }
                    self.step(1);
                }
            }
            KeyCode::Char('s') => {
                if let Some(&i) = self.list.selected().and_then(|at| self.shown.get(at)) {
                    self.papers[i].starred = !self.papers[i].starred;
                    self.narrow();
                }
            }
            KeyCode::Char('c') => {
                let picked: Vec<usize> = if self.marked.is_empty() {
                    self.list
                        .selected()
                        .and_then(|at| self.shown.get(at))
                        .into_iter()
                        .copied()
                        .collect()
                } else {
                    self.marked.iter().copied().collect()
                };
                if !picked.is_empty() {
                    let text = picked
                        .iter()
                        .map(|&i| bib::bibtex(&self.papers[i]))
                        .collect::<Vec<_>>()
                        .join("\n");
                    self.popup = Popup::Cite(text, 0);
                }
            }
            KeyCode::Char('y') => {
                let keys: Vec<String> = if self.marked.is_empty() {
                    self.current()
                        .map(|p| format!("@{}", p.key))
                        .into_iter()
                        .collect()
                } else {
                    self.marked
                        .iter()
                        .map(|&i| format!("@{}", self.papers[i].key))
                        .collect()
                };
                if !keys.is_empty() {
                    let cite = format!("[{}]", keys.join("; "));
                    self.status = copy(&cite, &cite);
                }
            }
            KeyCode::Enter | KeyCode::Char('o') if self.focus == Focus::List => self.open(terminal),
            KeyCode::Enter => self.focus = Focus::List,
            _ => {}
        }
    }

    fn step(&mut self, by: isize) {
        match self.focus {
            Focus::Side => {
                let mut at = self.shelf;
                loop {
                    let next = at.saturating_add_signed(by.signum());
                    if next == at || next >= self.shelves.len() {
                        break;
                    }
                    at = next;
                    if !matches!(self.shelves[at], Shelf::Heading(_)) {
                        self.shelf = at;
                        break;
                    }
                }
                self.list.select(Some(0));
                self.narrow();
            }
            Focus::List if !self.shown.is_empty() => {
                let at = self.list.selected().unwrap_or(0);
                self.list
                    .select(Some(at.saturating_add_signed(by).min(self.shown.len() - 1)));
            }
            Focus::List => {}
        }
    }

    /// Present the paper's PDF with ohp, which takes the terminal until it
    /// quits.
    fn open(&mut self, terminal: &mut DefaultTerminal) {
        let Some(p) = self.current() else { return };
        let path = self.pdf(p);
        if !path.exists() {
            self.status = format!("no {}", path.display());
            return;
        }
        ratatui::restore();
        let ran = Command::new("ohp").arg(&path).status();
        *terminal = ratatui::init();
        self.status = match ran {
            Ok(s) if s.success() => String::new(),
            Ok(s) => format!("ohp: {s}"),
            Err(e) => format!("cannot run ohp: {e}"),
        };
    }
}

/// What a query is looked for in, lowercase.
fn haystack(p: &Paper) -> String {
    let mut s = format!("{} {} {} {}", p.title, p.key, p.venue, p.year);
    for (family, _) in p.authors {
        s.push(' ');
        s.push_str(family);
    }
    for id in [p.doi, p.arxiv].into_iter().flatten() {
        s.push(' ');
        s.push_str(id);
    }
    s.to_lowercase()
}

fn paste() -> String {
    Command::new("pbpaste")
        .output()
        .map(|o| String::from_utf8_lossy(&o.stdout).into_owned())
        .unwrap_or_default()
}

/// Put `text` on the clipboard, and say so of `what`.
fn copy(text: &str, what: &str) -> String {
    let child = Command::new("pbcopy").stdin(Stdio::piped()).spawn();
    match child.and_then(|mut c| {
        c.stdin.take().map(|mut i| i.write_all(text.as_bytes()));
        c.wait()
    }) {
        Ok(_) => format!("copied {what}"),
        Err(e) => format!("cannot copy: {e}"),
    }
}

fn main() -> anyhow::Result<()> {
    let mut app = App::new();
    let mut terminal = ratatui::init();
    let result = (|| -> anyhow::Result<()> {
        while !app.quit {
            terminal.draw(|f| ui::draw(f, &mut app))?;
            if let Event::Key(key) = event::read()?
                && key.kind == KeyEventKind::Press
            {
                app.key(key, &mut terminal);
            }
        }
        Ok(())
    })();
    ratatui::restore();
    result
}

#[cfg(test)]
mod tests;
