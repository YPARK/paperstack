//! The TUI: what is shown, and what keys do. The sidebar's shelves and the
//! search line are one query, searched in the index; a star is written to
//! the paper's record, and read back into the index.

use crate::bibtex;
use crate::index::{Hit, Index, Shelves, Sort};
use crate::library::Library;
use crate::query::Query;
use crate::source::Source;
use crate::ui;
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

/// What the sidebar lists, under headings: each a query narrowing the
/// papers.
#[derive(Clone, PartialEq)]
pub enum Shelf {
    Heading(&'static str),
    All,
    Starred,
    NoPdf,
    NoAbstract,
    Folder(String),
    Label(String),
}

impl Shelf {
    pub fn query(&self) -> Query {
        let quoted = |field: &str, name: &str| format!("{field}:\"{name}\"");
        Query::parse(&match self {
            Shelf::Heading(_) | Shelf::All => String::new(),
            Shelf::Starred => "is:starred".to_string(),
            Shelf::NoPdf => "-has:pdf".to_string(),
            Shelf::NoAbstract => "-has:abstract".to_string(),
            Shelf::Folder(f) => quoted("folder", f),
            Shelf::Label(l) => quoted("label", l),
        })
    }

    pub fn name(&self) -> &str {
        match self {
            Shelf::Heading(h) => h,
            Shelf::All => "All",
            Shelf::Starred => "Starred",
            Shelf::NoPdf => "No PDF",
            Shelf::NoAbstract => "No abstract",
            Shelf::Folder(n) | Shelf::Label(n) => n,
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
    pub lib: Library,
    pub index: Index,
    /// The shelves, and how many papers each holds.
    pub shelves: Vec<(Shelf, usize)>,
    pub shelf: usize,
    pub focus: Focus,
    pub query: String,
    pub searching: bool,
    pub sort: Sort,
    pub shown: Vec<Hit>,
    pub list: ListState,
    /// By key.
    pub marked: BTreeSet<String>,
    pub popup: Popup,
    pub status: String,
    quit: bool,
}

pub fn run((lib, index): (Library, Index)) -> anyhow::Result<()> {
    let mut app = App::new(lib, index)?;
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

impl App {
    pub fn new(lib: Library, index: Index) -> anyhow::Result<Self> {
        let mut app = App {
            lib,
            index,
            shelves: Vec::new(),
            shelf: 1,
            focus: Focus::List,
            query: String::new(),
            searching: false,
            sort: Sort::Relevance,
            shown: Vec::new(),
            list: ListState::default(),
            marked: BTreeSet::new(),
            popup: Popup::None,
            status: String::new(),
            quit: false,
        };
        app.restock()?;
        app.narrow();
        Ok(app)
    }

    /// Read the shelves, and their counts, from the index again.
    fn restock(&mut self) -> anyhow::Result<()> {
        let Shelves {
            all,
            starred,
            no_pdf,
            no_abstract,
            folders,
            labels,
        } = self.index.shelves()?;
        let picked = self.shelves.get(self.shelf).map(|(s, _)| s.clone());
        let mut shelves = vec![
            (Shelf::Heading("MY LIBRARY"), 0),
            (Shelf::All, all),
            (Shelf::Starred, starred),
            (Shelf::NoPdf, no_pdf),
            (Shelf::NoAbstract, no_abstract),
            (Shelf::Heading("FOLDERS"), 0),
        ];
        shelves.extend(folders.into_iter().map(|(f, n)| (Shelf::Folder(f), n)));
        shelves.push((Shelf::Heading("LABELS"), 0));
        shelves.extend(labels.into_iter().map(|(l, n)| (Shelf::Label(l), n)));
        self.shelves = shelves;
        self.shelf = picked
            .and_then(|p| self.shelves.iter().position(|(s, _)| *s == p))
            .unwrap_or(1);
        Ok(())
    }

    pub fn current(&self) -> Option<&Hit> {
        self.shown.get(self.list.selected()?)
    }

    pub fn pdf(&self, h: &Hit) -> PathBuf {
        self.lib.root.join(&h.path).with_extension("pdf")
    }

    /// Show the papers on the shelf that the query finds.
    pub fn narrow(&mut self) {
        let query = self.shelves[self.shelf]
            .0
            .query()
            .and(Query::parse(&self.query));
        let key = self.current().map(|h| h.record.key.clone());
        match self.index.search(&query, self.sort, false, None) {
            Ok(hits) => self.shown = hits,
            Err(e) => self.status = format!("{e:#}"),
        }
        let at = key
            .and_then(|k| self.shown.iter().position(|h| h.record.key == k))
            .or(self.list.selected())
            .unwrap_or(0);
        self.list
            .select((!self.shown.is_empty()).then(|| at.min(self.shown.len() - 1)));
    }

    /// The papers marked, in the order shown, then those marked on another
    /// shelf or before a search left them out; or, with none marked, the
    /// one picked.
    fn picked(&self) -> Vec<Hit> {
        if self.marked.is_empty() {
            return self.current().cloned().into_iter().collect();
        }
        let mut all: Vec<Hit> = self
            .shown
            .iter()
            .filter(|h| self.marked.contains(&h.record.key))
            .cloned()
            .collect();
        for key in &self.marked {
            if !all.iter().any(|h| &h.record.key == key)
                && let Ok(Some(h)) = self.index.get(key)
            {
                all.push(h);
            }
        }
        all
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
                        let (what, _) = Source::of(line).plan();
                        self.status = if what.is_empty() {
                            "nothing to add".to_string()
                        } else {
                            format!("not yet: adding {what} comes with `paperstack add`")
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
            KeyCode::Esc => self.marked.clear(),
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
            KeyCode::Char('S') => {
                let at = Sort::ALL.iter().position(|&s| s == self.sort).unwrap_or(0);
                self.sort = Sort::ALL[(at + 1) % Sort::ALL.len()];
                self.narrow();
            }
            KeyCode::Char('a') => {
                let clip = paste();
                let clip = clip.trim();
                let line = if !clip.contains('\n')
                    && !matches!(Source::of(clip), Source::Words(_) | Source::Nothing)
                {
                    clip.to_string()
                } else {
                    String::new()
                };
                self.popup = Popup::Add(line);
            }
            KeyCode::Char(' ') => {
                if let Some(key) = self.current().map(|h| h.record.key.clone()) {
                    if !self.marked.remove(&key) {
                        self.marked.insert(key);
                    }
                    self.step(1);
                }
            }
            KeyCode::Char('*') => {
                let keys: Vec<String> = self.shown.iter().map(|h| h.record.key.clone()).collect();
                if keys.iter().all(|k| self.marked.contains(k)) {
                    for k in &keys {
                        self.marked.remove(k);
                    }
                } else {
                    self.marked.extend(keys);
                }
            }
            KeyCode::Char('s') => self.star(),
            KeyCode::Char('c') => {
                let picked = self.picked();
                let records: Vec<_> = picked.iter().map(|h| &h.record).collect();
                if !records.is_empty() {
                    self.popup = Popup::Cite(bibtex::entries(&records), 0);
                }
            }
            KeyCode::Char('y') => {
                let keys: Vec<String> = self
                    .picked()
                    .iter()
                    .map(|h| format!("@{}", h.record.key))
                    .collect();
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
                    if !matches!(self.shelves[at].0, Shelf::Heading(_)) {
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

    /// Star the paper picked, or take its star away, in its record.
    pub fn star(&mut self) {
        let Some(hit) = self.current() else { return };
        let mut record = hit.record.clone();
        let was = hit.path.clone();
        record.paperstack.starred = !record.paperstack.starred;
        let done = self
            .lib
            .save(&record, Some(&was))
            .and_then(|_| self.index.refresh(&self.lib))
            .and_then(|_| self.restock());
        if let Err(e) = done {
            self.status = format!("cannot star: {e:#}");
        }
        self.narrow();
    }

    /// Present the paper's PDF with ohp, which takes the terminal until it
    /// quits.
    fn open(&mut self, terminal: &mut DefaultTerminal) {
        let Some(h) = self.current() else { return };
        let path = self.pdf(h);
        if !h.has_pdf || !path.exists() {
            self.status = format!("no PDF: {}", path.display());
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
