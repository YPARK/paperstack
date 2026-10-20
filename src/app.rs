//! The TUI: what is shown, and what keys do. The sidebar's shelves and the
//! search line are one query, searched in the index; a star is written to
//! the paper's record, and read back into the index.

use crate::add::{Adder, Options, attach, one_line};
use crate::bibtex;
use crate::browser::{self, Arrivals};
use crate::index::{Hit, Index, Shelves, Sort};
use crate::library::Library;
use crate::net::Net;
use crate::query::Query;
use crate::source::Source;
use crate::theme::{self, Theme};
use crate::ui;
use ratatui::DefaultTerminal;
use ratatui::crossterm::event::{
    self, DisableMouseCapture, EnableMouseCapture, Event, KeyCode, KeyEvent, KeyEventKind,
    KeyModifiers, MouseButton, MouseEvent, MouseEventKind,
};
use ratatui::crossterm::execute;
use ratatui::layout::{Position, Rect};
use ratatui::style::Modifier;
use ratatui::widgets::ListState;
use std::collections::BTreeSet;
use std::io::Write;
use std::path::PathBuf;
use std::process::{Command, Stdio};
use std::time::{Duration, Instant};

/// Two clicks on a paper within this open it.
const DOUBLE_CLICK: Duration = Duration::from_millis(400);
/// How often the downloads folder is looked in, while a PDF is awaited.
const LOOK: Duration = Duration::from_millis(500);

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

/// What a key asks of the terminal, beyond the screen.
pub enum Action {
    /// Present the paper picked with ohp, which takes the terminal.
    Open,
    /// Add what is named, the screen drawn first, saying so, as it takes a
    /// while.
    Add(String),
}

/// What a click lands on, as the screen was last drawn.
#[derive(Clone, Copy, PartialEq, Debug)]
pub enum Target {
    Search,
    Add,
    /// The sidebar, and a shelf on it, by its place in `shelves`.
    Side,
    Shelf(usize),
    /// The papers, and one of them, its box and its star, by its place in
    /// `shown`.
    List,
    Paper(usize),
    Mark(usize),
    Star(usize),
    /// The badge of a paper with no PDF, to get it in the browser.
    GetPdf(usize),
    Popup,
}

/// A PDF awaited from the browser, for the paper `key`.
pub struct Fetch {
    pub key: String,
    pub title: String,
    /// The page opened.
    pub page: String,
    /// A path or an address typed, or pasted, instead.
    pub line: String,
    pub arrivals: Arrivals,
    /// What came of the last PDF that came, if it was not this paper's.
    pub note: String,
}

pub enum Popup {
    None,
    /// What is typed, as a DOI, arXiv id, PMID, URL, PDF or BibTeX.
    Add(String),
    /// BibTeX shown, and how far it is scrolled.
    Cite(String, u16),
    Help,
    /// A PDF awaited from the browser.
    Fetch(Box<Fetch>),
}

pub struct App {
    pub lib: Library,
    pub index: Index,
    /// To add papers with; none, as in tests, and nothing is fetched.
    pub net: Option<Net>,
    pub theme: Theme,
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
    /// Where each thing clicked on was drawn; the last drawn over the rest.
    pub targets: Vec<(Rect, Target)>,
    /// The paper last clicked, and when, to know a double click.
    clicked: Option<(Instant, usize)>,
    quit: bool,
}

pub fn run((lib, index): (Library, Index), net: Net, theme: Theme) -> anyhow::Result<()> {
    let mut app = App::new(lib, index)?;
    app.net = Some(net);
    app.theme = theme;
    let mut terminal = take();
    let result = (|| -> anyhow::Result<()> {
        // Drawn again only for what may change it: not the mouse moving.
        let mut stale = true;
        while !app.quit {
            if stale {
                terminal.draw(|f| ui::draw(f, &mut app))?;
            }
            // Awaiting a PDF, the downloads folder is looked in between keys.
            if matches!(app.popup, Popup::Fetch(_)) && !event::poll(LOOK)? {
                stale = app.tick();
                continue;
            }
            let event = event::read()?;
            stale = !matches!(
                event,
                Event::Mouse(MouseEvent {
                    kind: MouseEventKind::Moved | MouseEventKind::Drag(_),
                    ..
                }) | Event::FocusGained
                    | Event::FocusLost
            );
            let action = match event {
                Event::Key(key) if key.kind == KeyEventKind::Press => app.key(key),
                Event::Mouse(m) => app.mouse(m),
                _ => None,
            };
            match action {
                Some(Action::Add(line)) => {
                    terminal.draw(|f| ui::draw(f, &mut app))?;
                    app.add(&line);
                }
                Some(Action::Open) => app.open(&mut terminal),
                None => {}
            }
        }
        Ok(())
    })();
    give_back();
    result
}

/// The terminal, as ratatui takes it, with the mouse's clicks and wheel.
fn take() -> DefaultTerminal {
    let terminal = ratatui::init();
    let _ = execute!(std::io::stdout(), EnableMouseCapture);
    terminal
}

fn give_back() {
    let _ = execute!(std::io::stdout(), DisableMouseCapture);
    ratatui::restore();
}

impl App {
    pub fn new(lib: Library, index: Index) -> anyhow::Result<Self> {
        let mut app = App {
            lib,
            index,
            net: None,
            theme: Theme::of(theme::Name::default()),
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
            targets: Vec::new(),
            clicked: None,
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
            (Shelf::Heading("COLLECTIONS"), 0),
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

    /// How many papers the library holds, as the shelf All counts them.
    pub fn total(&self) -> usize {
        self.shelves
            .iter()
            .find(|(s, _)| *s == Shelf::All)
            .map_or(0, |(_, n)| *n)
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
        let shown: std::collections::HashSet<String> =
            all.iter().map(|h| h.record.key.clone()).collect();
        for key in self.marked.iter().filter(|k| !shown.contains(*k)) {
            if let Ok(Some(h)) = self.index.get(key) {
                all.push(h);
            }
        }
        all
    }

    /// Take `key`; what it asks of the terminal, as opening a PDF in ohp,
    /// is left to the caller.
    pub fn key(&mut self, key: KeyEvent) -> Option<Action> {
        let ctrl = key.modifiers.contains(KeyModifiers::CONTROL);
        if ctrl && key.code == KeyCode::Char('c') {
            self.quit = true;
            return None;
        }
        match &mut self.popup {
            Popup::Add(line) => {
                match key.code {
                    KeyCode::Esc => self.popup = Popup::None,
                    KeyCode::Enter => {
                        let line = line.clone();
                        self.popup = Popup::None;
                        self.status = format!("adding {}…", Source::of(&line).plan().0);
                        return Some(Action::Add(line));
                    }
                    KeyCode::Backspace => {
                        line.pop();
                    }
                    KeyCode::Char('u') if ctrl => line.clear(),
                    KeyCode::Char('v') if ctrl => line.push_str(paste().trim()),
                    KeyCode::Char(c) => line.push(c),
                    _ => {}
                }
                return None;
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
                return None;
            }
            Popup::Help => {
                self.popup = Popup::None;
                return None;
            }
            Popup::Fetch(fetch) => {
                match key.code {
                    KeyCode::Esc => {
                        self.popup = Popup::None;
                        self.status = "no PDF got".to_string();
                    }
                    KeyCode::Enter => {
                        let line = fetch.line.trim().to_string();
                        if !line.is_empty() {
                            self.attach_from(&line);
                        }
                    }
                    KeyCode::Backspace => {
                        fetch.line.pop();
                    }
                    KeyCode::Char('o') if ctrl => {
                        if let Err(e) = browser::open(&fetch.page) {
                            fetch.note = one_line(&e);
                        }
                    }
                    KeyCode::Char('u') if ctrl => fetch.line.clear(),
                    KeyCode::Char('v') if ctrl => fetch.line.push_str(paste().trim()),
                    KeyCode::Char(c) => fetch.line.push(c),
                    _ => {}
                }
                return None;
            }
            Popup::None => {}
        }
        if ctrl && key.code == KeyCode::Char('e') {
            self.searching = false;
            let picked = self.picked();
            let records: Vec<_> = picked.iter().map(|h| h.record.as_ref()).collect();
            if !records.is_empty() {
                self.popup = Popup::Cite(bibtex::entries(&records), 0);
            }
            return None;
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
            return None;
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
            KeyCode::Char('p') => self.get_pdf(),
            KeyCode::Char('r') => {
                let before = self.total();
                match self.index.refresh(&self.lib).and_then(|_| self.restock()) {
                    Ok(()) => {
                        let now = self.total();
                        self.status = format!("{} papers, {} new", now, now.saturating_sub(before));
                    }
                    Err(e) => self.status = format!("{e:#}"),
                }
                self.narrow();
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
            KeyCode::Enter | KeyCode::Char('o') if self.focus == Focus::List => {
                return Some(Action::Open);
            }
            KeyCode::Enter => self.focus = Focus::List,
            _ => {}
        }
        None
    }

    /// Take a click or a turn of the wheel at where it lands.
    pub fn mouse(&mut self, m: MouseEvent) -> Option<Action> {
        let at = self
            .targets
            .iter()
            .rev()
            .find(|(r, _)| r.contains(Position::new(m.column, m.row)))
            .map(|&(_, t)| t);
        let by = match m.kind {
            MouseEventKind::Down(MouseButton::Left) => return self.click(at),
            MouseEventKind::ScrollDown => 1,
            MouseEventKind::ScrollUp => -1,
            _ => return None,
        };
        match (&mut self.popup, at) {
            (Popup::Cite(_, scroll), _) => *scroll = scroll.saturating_add_signed(by as i16),
            (Popup::None, Some(Target::Side | Target::Shelf(_))) => {
                self.focus = Focus::Side;
                self.step(by);
            }
            (
                Popup::None,
                Some(Target::List | Target::Paper(_) | Target::Mark(_) | Target::Star(_)),
            ) => {
                self.focus = Focus::List;
                self.step(by);
            }
            _ => {}
        }
        None
    }

    fn click(&mut self, at: Option<Target>) -> Option<Action> {
        if !matches!(self.popup, Popup::None) {
            if at != Some(Target::Popup) {
                self.popup = Popup::None;
            }
            return None;
        }
        self.searching = false;
        match at? {
            Target::Search => {
                self.searching = true;
                self.focus = Focus::List;
            }
            Target::Add => return self.key(KeyEvent::new(KeyCode::Char('a'), KeyModifiers::NONE)),
            Target::Side => self.focus = Focus::Side,
            Target::Shelf(i) => {
                self.focus = Focus::Side;
                if i != self.shelf {
                    self.shelf = i;
                    self.list.select(Some(0));
                    self.narrow();
                }
            }
            Target::List | Target::Popup => self.focus = Focus::List,
            Target::Paper(i) => {
                self.focus = Focus::List;
                self.list.select(Some(i));
                let again = self
                    .clicked
                    .is_some_and(|(when, j)| j == i && when.elapsed() < DOUBLE_CLICK);
                self.clicked = (!again).then(|| (Instant::now(), i));
                if again {
                    return Some(Action::Open);
                }
            }
            Target::Mark(i) => {
                self.focus = Focus::List;
                if let Some(key) = self.shown.get(i).map(|h| h.record.key.clone())
                    && !self.marked.remove(&key)
                {
                    self.marked.insert(key);
                }
            }
            Target::Star(i) => {
                self.focus = Focus::List;
                self.list.select(Some(i));
                self.star();
            }
            Target::GetPdf(i) => {
                self.focus = Focus::List;
                self.list.select(Some(i));
                self.get_pdf();
            }
        }
        None
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

    /// Add what `line` names, into the folder or label picked on the left,
    /// and show it.
    fn add(&mut self, line: &str) {
        // A whole library, from its export: nothing is fetched for it.
        if let Source::Export(given) = Source::of(line) {
            let export = browser::dropped(&given);
            let pdfs = crate::migrate::find_pdf_dir(&export);
            let done = self.index.refresh(&self.lib).and_then(|_| {
                crate::migrate::import(&self.lib, &mut self.index, &export, pdfs.as_deref(), false)
            });
            match done {
                Ok(imported) => {
                    self.status = format!("imported: {}", imported.summary());
                    if let Err(e) = self.restock() {
                        self.status = one_line(&e);
                    }
                    self.narrow();
                }
                Err(e) => self.status = one_line(&e),
            }
            return;
        }
        let Some(net) = &self.net else {
            self.status = "cannot add here: nothing is fetched".to_string();
            return;
        };
        // What another program added meanwhile is a paper already here.
        if let Err(e) = self.index.refresh(&self.lib) {
            self.status = one_line(&e);
            return;
        }
        let mut options = Options {
            pdf: true,
            ..Options::default()
        };
        match &self.shelves[self.shelf].0 {
            Shelf::Folder(f) => options.folders.push(f.clone()),
            Shelf::Label(l) => options.labels.push(l.clone()),
            _ => {}
        }
        let done = Adder {
            lib: &self.lib,
            index: &mut self.index,
            net,
            options,
        }
        .add(line);
        match done {
            Ok(added) => {
                self.status = format!("{}: {}", added.key, added.summary());
                if let Err(e) = self.restock() {
                    self.status = one_line(&e);
                }
                self.query.clear();
                self.narrow();
                if let Some(at) = self.shown.iter().position(|h| h.record.key == added.key) {
                    self.list.select(Some(at));
                }
            }
            Err(e) => self.status = one_line(&e),
        }
    }

    /// Star the paper picked, or take its star away, in its record.
    /// The record is read again first, from its file: another program, as
    /// `paperstack serve`, or another machine, may have changed it since it
    /// was shown.
    pub fn star(&mut self) {
        let Some(key) = self.current().map(|h| h.record.key.clone()) else {
            return;
        };
        let done = (|| -> anyhow::Result<()> {
            // Its file, as it is now; the whole library looked at again
            // only if it has moved.
            let mut hit = self.index.get(&key)?;
            let mut fresh = hit
                .as_ref()
                .and_then(|h| crate::library::read(&self.lib.root.join(&h.path)).ok());
            if fresh.is_none() {
                self.index.refresh(&self.lib)?;
                hit = self.index.get(&key)?;
                fresh = hit
                    .as_ref()
                    .map(|h| crate::record::Record::clone(&h.record));
            }
            let (Some(hit), Some(mut record)) = (hit, fresh) else {
                anyhow::bail!("{key} is no longer in the library");
            };
            record.paperstack.starred = !record.paperstack.starred;
            let path = self.lib.save(&record, Some(&hit.path))?;
            self.index
                .upsert(&self.lib, &path, record, Some(&hit.path))?;
            self.restock()
        })();
        if let Err(e) = done {
            self.status = format!("cannot star: {e:#}");
        }
        self.narrow();
    }

    /// Get the PDF of the paper picked in the browser: its page opened
    /// there, the PDF downloaded there awaited.
    pub fn get_pdf(&mut self) {
        let Some(h) = self.current() else { return };
        if h.has_pdf {
            self.status = format!("{} has its PDF: ⏎ opens it", h.record.key);
            return;
        }
        let page = browser::page(&h.record);
        let note = match browser::open(&page) {
            Ok(()) => String::new(),
            Err(e) => one_line(&e),
        };
        self.popup = Popup::Fetch(Box::new(Fetch {
            key: h.record.key.clone(),
            title: h.record.title().to_string(),
            page,
            line: String::new(),
            arrivals: Arrivals::new(browser::downloads()),
            note,
        }));
    }

    /// Look for the PDF awaited among what has been downloaded since; say
    /// whether anything came of it, to be shown.
    pub fn tick(&mut self) -> bool {
        let Popup::Fetch(fetch) = &mut self.popup else {
            return false;
        };
        let mut came = false;
        for path in fetch.arrivals.next() {
            let from = browser::tilde(&path);
            let done = std::fs::read(&path)
                .map_err(anyhow::Error::from)
                .and_then(|bytes| attach(&self.lib, &mut self.index, &fetch.key, &bytes, &from));
            came = true;
            match done {
                Ok(added) => {
                    self.attached(&added);
                    return true;
                }
                Err(e) => fetch.note = one_line(&e),
            }
        }
        came
    }

    /// Give the paper awaited the PDF at `line`, a path or an address.
    fn attach_from(&mut self, line: &str) {
        let Popup::Fetch(fetch) = &mut self.popup else {
            return;
        };
        let bytes = if line.starts_with("http://") || line.starts_with("https://") {
            match &self.net {
                Some(net) => net.pdf(line),
                None => Err(anyhow::anyhow!("cannot download here")),
            }
        } else {
            let path = browser::dropped(line);
            std::fs::read(&path).map_err(|e| anyhow::anyhow!("cannot read {}: {e}", path.display()))
        };
        let done = bytes.and_then(|b| attach(&self.lib, &mut self.index, &fetch.key, &b, line));
        match done {
            Ok(added) => self.attached(&added),
            Err(e) => fetch.note = one_line(&e),
        }
    }

    fn attached(&mut self, added: &crate::add::Added) {
        self.popup = Popup::None;
        self.status = format!("{}: {}", added.key, added.summary());
        if let Err(e) = self.restock() {
            self.status = one_line(&e);
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
        give_back();
        let ran = Command::new("ohp").arg(&path).status();
        *terminal = take();
        self.status = match ran {
            Ok(s) if s.success() => String::new(),
            Ok(s) => format!("ohp: {s}"),
            Err(e) => format!("cannot run ohp: {e}"),
        };
    }
}

/// The screen as `app` would draw it on a terminal `width` by `height`, as
/// text, for tests and `paperstack screen`.
pub fn screen(app: &mut App, width: u16, height: u16) -> String {
    let mut terminal = ratatui::Terminal::new(ratatui::backend::TestBackend::new(width, height))
        .expect("a screen in memory");
    terminal
        .draw(|f| ui::draw(f, app))
        .expect("drawn in memory");
    let buffer = terminal.backend().buffer();
    (0..height)
        .map(|y| {
            (0..width)
                .map(|x| buffer[(x, y)].symbol())
                .collect::<String>()
                .trim_end()
                .to_string()
        })
        .collect::<Vec<_>>()
        .join("\n")
}

/// The screen as an SVG, with its colours, as a terminal would show it:
/// for a look at a theme, or a picture for the README.
pub fn screen_svg(app: &mut App, width: u16, height: u16) -> String {
    use ratatui::style::Color;
    use std::fmt::Write;
    const CW: f64 = 8.4;
    const CH: f64 = 18.0;
    let mut terminal = ratatui::Terminal::new(ratatui::backend::TestBackend::new(width, height))
        .expect("a screen in memory");
    terminal
        .draw(|f| ui::draw(f, app))
        .expect("drawn in memory");
    let buffer = terminal.backend().buffer();
    let dark = matches!(app.theme.page, Color::Reset);
    let hex = |c: Color, bg: bool| -> String {
        let (r, g, b) = match c {
            Color::Rgb(r, g, b) => (r, g, b),
            Color::Reset if bg => {
                if dark {
                    (30, 30, 30)
                } else {
                    (255, 255, 255)
                }
            }
            Color::Reset => {
                if dark {
                    (220, 220, 220)
                } else {
                    (51, 51, 51)
                }
            }
            Color::Black => (0, 0, 0),
            Color::Red => (205, 49, 49),
            Color::Green => (13, 188, 121),
            Color::Yellow => (229, 192, 37),
            Color::Blue => (36, 114, 200),
            Color::Magenta => (188, 63, 188),
            Color::Cyan => (17, 168, 205),
            Color::Gray => (204, 204, 204),
            Color::DarkGray => (118, 118, 118),
            Color::LightBlue => (59, 142, 234),
            Color::White => (255, 255, 255),
            Color::Indexed(n) if n >= 232 => {
                let v = 8 + (n - 232) * 10;
                (v, v, v)
            }
            _ => (128, 128, 128),
        };
        format!("#{r:02x}{g:02x}{b:02x}")
    };
    let mut svg = format!(
        "<svg xmlns='http://www.w3.org/2000/svg' width='{}' height='{}' font-family='Menlo, Monaco, monospace' font-size='14'>\n",
        f64::from(width) * CW,
        f64::from(height) * CH
    );
    for y in 0..height {
        for x in 0..width {
            let cell = &buffer[(x, y)];
            let (px, py) = (f64::from(x) * CW, f64::from(y) * CH);
            let _ = writeln!(
                svg,
                "<rect x='{px:.1}' y='{py:.1}' width='{:.1}' height='{CH}' fill='{}'/>",
                CW + 0.6,
                hex(cell.bg, true)
            );
            let symbol = cell.symbol();
            if !symbol.trim().is_empty() {
                let bold = if cell.modifier.contains(Modifier::BOLD) {
                    " font-weight='bold'"
                } else {
                    ""
                };
                let text = symbol
                    .replace('&', "&amp;")
                    .replace('<', "&lt;")
                    .replace('>', "&gt;");
                let _ = writeln!(
                    svg,
                    "<text x='{px:.1}' y='{:.1}' fill='{}'{bold}>{text}</text>",
                    py + 13.5,
                    hex(cell.fg, false)
                );
            }
        }
    }
    svg.push_str("</svg>\n");
    svg
}

/// A key as `paperstack screen` is given it: `<enter>`, `<esc>`, `<tab>`,
/// `<space>`, `<up>`, `<down>`, `<left>`, `<right>`, `<backspace>`, and
/// with control, as `<c-u>` or `<c-e>`; anything else, each character typed.
pub fn keys(spec: &str) -> Vec<KeyEvent> {
    let named = match spec {
        "<enter>" => Some(KeyCode::Enter),
        "<esc>" => Some(KeyCode::Esc),
        "<tab>" => Some(KeyCode::Tab),
        "<space>" => Some(KeyCode::Char(' ')),
        "<up>" => Some(KeyCode::Up),
        "<down>" => Some(KeyCode::Down),
        "<left>" => Some(KeyCode::Left),
        "<right>" => Some(KeyCode::Right),
        "<backspace>" => Some(KeyCode::Backspace),
        _ => None,
    };
    if let Some(code) = named {
        return vec![KeyEvent::new(code, KeyModifiers::NONE)];
    }
    let ctrl: Option<Vec<char>> = spec
        .strip_prefix("<c-")
        .and_then(|s| s.strip_suffix('>'))
        .map(|s| s.chars().collect());
    if let Some(&[c]) = ctrl.as_deref() {
        return vec![KeyEvent::new(KeyCode::Char(c), KeyModifiers::CONTROL)];
    }
    spec.chars()
        .map(|c| KeyEvent::new(KeyCode::Char(c), KeyModifiers::NONE))
        .collect()
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
        if let Some(mut stdin) = c.stdin.take() {
            stdin.write_all(text.as_bytes())?;
        }
        c.wait()
    }) {
        Ok(_) => format!("copied {what}"),
        Err(e) => format!("cannot copy: {e}"),
    }
}
