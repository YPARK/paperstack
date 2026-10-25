//! The TUI: what is shown, and what keys do. The sidebar's shelves and the
//! search line are one query, searched in the index; a star is written to
//! the paper's record, and read back into the index. A paper deleted goes
//! to the library's trash, asked first; its PDF alone too, the paper kept.
//! A PDF located on disk is filed beside the paper picked.

use crate::add::{Added, Adder, Ambiguous, Options, attach, one_line, open_copy};
use crate::bibtex;
use crate::browser::{self, Arrivals};
use crate::index::{Hit, Index, Shelves, Sort};
use crate::library::Library;
use crate::map;
use crate::net::Net;
use crate::query::Query;
use crate::source::Source;
use crate::theme::{self, Theme};
use crate::ui;

mod themes;
pub use themes::{Card, Inside, MapView, Mapped};

/// What a click lands on, as a double click is known by.
#[derive(Clone, Copy, PartialEq, Debug)]
enum Clicked {
    /// A paper in the list, by its row.
    Paper(usize),
    /// A paper on a card, by its row.
    Card(usize),
    /// A paper on the map shown, by its place.
    Dot(usize),
    /// A theme's name on the map shown.
    Name(usize),
}
use ratatui::DefaultTerminal;
use ratatui::crossterm::event::{
    self, DisableMouseCapture, EnableMouseCapture, Event, KeyCode, KeyEvent, KeyEventKind,
    KeyModifiers, KeyboardEnhancementFlags, MouseButton, MouseEvent, MouseEventKind,
    PopKeyboardEnhancementFlags, PushKeyboardEnhancementFlags,
};
use ratatui::crossterm::execute;
use ratatui::layout::{Position, Rect};
use ratatui::style::Modifier;
use ratatui::widgets::ListState;
use std::collections::{BTreeSet, HashMap, HashSet};
use std::io::Write;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, mpsc};
use std::time::{Duration, Instant};

/// Papers so many that a label for them all is typed out, not offered.
pub const MANY_LABELLED: usize = 200;

/// Two clicks on a paper within this open it.
const DOUBLE_CLICK: Duration = Duration::from_millis(400);
/// How often the downloads folder is looked in, while a PDF is awaited.
const LOOK: Duration = Duration::from_millis(500);
/// How often work on its own thread is asked how it goes, and the
/// spinner turned.
const SPIN: Duration = Duration::from_millis(100);

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
    /// A theme of the map, by its place in the map's themes, and its name.
    Theme(usize, String),
}

impl Shelf {
    pub fn query(&self) -> Query {
        let quoted = |field: &str, name: &str| format!("{field}:\"{name}\"");
        Query::parse(&match self {
            Shelf::Heading(_) | Shelf::All | Shelf::Theme(..) => String::new(),
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
            Shelf::Folder(n) | Shelf::Label(n) | Shelf::Theme(_, n) => n,
        }
    }
}

/// What a key asks of the terminal, beyond the screen.
pub enum Action {
    /// Present a paper with ohp, which takes the terminal: this one, or
    /// the one picked.
    Open(Option<Box<Hit>>),
    /// Add what is named, on a thread of its own, its steps shown.
    Add(String),
    /// Ask for a PDF on disk, in the system's file dialog, which takes the
    /// terminal.
    Locate,
    /// File the paper chosen of those a PDF holds, as `Add` does.
    Choose,
}

/// What a click lands on, as the screen was last drawn.
#[derive(Clone, Copy, PartialEq, Debug)]
pub enum Target {
    Search,
    /// What the papers are sorted by: the next order.
    Sort,
    /// The sidebar, and a shelf on it, by its place in `shelves`.
    Side,
    Shelf(usize),
    /// The papers, and one of them, its box and its star, by its place in
    /// `shown`.
    List,
    Paper(usize),
    Mark(usize),
    Star(usize),
    /// The PDF's badge: get one, an open copy or in the browser, or drop the
    /// one the paper has.
    GetPdf(usize),
    Popup,
    /// The map of themes.
    Map,
    /// A key in the footer: as if pressed, in the list or on the map.
    Key(KeyEvent),
    /// A button top left: as if its key were pressed where one is.
    Button(KeyEvent),
    /// A theme's name on the map.
    Label(usize),
    /// The edge of the pane of the paper picked, to drag; and where the
    /// pane's right is.
    Edge(u16),
    /// A paper on a theme's card, by its place there.
    CardPaper(usize),
}

/// What the page shows: the papers, or the map of their themes.
#[derive(Clone, Copy, PartialEq, Debug)]
pub enum View {
    List,
    Map,
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
    /// The card it was asked for from, to show again.
    pub back: Option<Box<Card>>,
}

pub enum Popup {
    None,
    /// What is typed, as a DOI, arXiv id, PMID, URL, PDF or BibTeX.
    Add(String),
    /// BibTeX shown, and how far it is scrolled.
    Cite(String, u16),
    /// The keys, and how far they are scrolled.
    Help(u16),
    /// A PDF awaited from the browser.
    Fetch(Box<Fetch>),
    /// What `y` deletes, asked first.
    Delete(Doomed),
    /// Which of the papers a PDF holds to file it as.
    Choose(Box<Choose>),
    /// Work on a thread of its own, as a download, and what it says.
    Busy(Box<Busy>),
    /// A name typed: a theme's, or a label for papers.
    Ask(Box<Ask>),
    /// A theme of the map: its words, its authors, its papers to open.
    Card(Box<Card>),
}

/// A name being typed, and what it is for.
pub struct Ask {
    pub what: Asked,
    pub line: String,
}

pub enum Asked {
    /// Theme `t`'s name; blank, its own words again.
    Name(usize),
    /// A label for these papers.
    Label(Vec<String>),
}

/// Work on a thread of its own, as downloading a PDF, with what it has said
/// so far; the screen goes on being drawn meanwhile.
pub struct Busy {
    pub title: &'static str,
    /// What it is for: a paper's title, or what is being added.
    pub what: String,
    /// What it has said of its steps, the latest last.
    pub said: Vec<String>,
    pub since: Instant,
    pub events: mpsc::Receiver<Report>,
    /// Set to stop it after the step it is at, where it can stop.
    pub stop: Arc<AtomicBool>,
    /// Asked to stop, and to get the PDF in the browser after.
    pub browse: bool,
    pub then: Then,
}

/// What work on its own thread sends.
pub enum Report {
    Said(String),
    Done(anyhow::Result<Done>),
}

/// What work on its own thread gave.
pub enum Done {
    Added(Added),
    Mapped(Box<Mapped>),
    /// A theme from within: its own map, and the links it shows.
    Within(usize, Box<Inside>),
}

/// What is done with what the work gave.
pub enum Then {
    /// An open copy of this paper's PDF: filed; else got in the browser;
    /// the card it was asked for from shown again after.
    Fetch(Hit, Option<Box<Card>>),
    /// A paper added: shown; or, where its PDF holds several, asked which.
    Add,
    /// The map built, or its themes gathered again: shown.
    Map,
    /// A theme from within, made: gone into. Nothing is written.
    Within,
}

/// A PDF that holds more than one paper, as a page of letters, and the
/// one picked of them.
pub struct Choose {
    /// The PDF, as it was named.
    pub name: String,
    pub papers: Vec<crate::resolve::Found>,
    pub pdf: Vec<u8>,
    pub at: usize,
}

/// What is to be deleted, if `y` is pressed.
pub enum Doomed {
    /// Papers, by key, with their titles.
    Papers(Vec<(String, String)>),
    /// PDFs of papers that stay, by key, with their titles.
    Pdfs(Vec<(String, String)>),
    /// A collection or label, and how many papers have it.
    Shelf(crate::remove::Shelf, String, usize),
}

/// Papers from the map, a finer theme's, by its name, on the shelf of
/// the theme it is in, by its number.
pub struct Group {
    pub theme: usize,
    pub name: String,
    pub keys: HashSet<String>,
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
    /// Papers sent from the map to read, shown alone on their shelf.
    pub group: Option<Group>,
    /// Fewer papers than this in view on the map, ⏎ or a double click
    /// sends them to the list to read, rather than further in; 0, never,
    /// as tests on the demo library's few papers ask.
    pub to_read: usize,
    pub focus: Focus,
    pub query: String,
    /// What the query finds in the whole library, for the map to show:
    /// none without a query; and the query it was found for.
    pub found: Option<HashSet<String>>,
    found_for: Option<String>,

    pub searching: bool,
    pub sort: Sort,
    pub shown: Vec<Hit>,
    pub list: ListState,
    /// How wide the pane of the paper picked is, at the right; and, its
    /// edge being dragged, where its right is.
    pub pane: u16,
    pub dragging: Option<u16>,
    /// How wide the pane may be, as last drawn.
    pub pane_room: u16,
    /// By key.
    pub marked: BTreeSet<String>,
    pub popup: Popup,
    pub view: View,
    /// Read when first shown.
    pub map: Option<MapView>,
    pub status: String,
    /// Where each thing clicked on was drawn; the last drawn over the rest.
    pub targets: Vec<(Rect, Target)>,
    /// What was last clicked, and when, to know a double click.
    clicked: Option<(Instant, Clicked)>,
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
            // Awaiting a PDF, the downloads folder is looked in between keys;
            // work on its own thread is asked how it goes.
            let wait = match app.popup {
                Popup::Fetch(_) => Some(LOOK),
                Popup::Busy(_) => Some(SPIN),
                _ => None,
            };
            if let Some(wait) = wait
                && !event::poll(wait)?
            {
                stale = app.tick();
                continue;
            }
            let event = event::read()?;
            // A drag drawn only while it moves the pane's edge.
            stale = app.dragging.is_some()
                || !matches!(
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
                Some(Action::Add(line)) => app.add(&line),
                Some(Action::Open(h)) => app.open(&mut terminal, h.map(|h| *h)),
                Some(Action::Locate) => app.locate(&mut terminal),
                Some(Action::Choose) => app.choose(),
                None => {}
            }
        }
        Ok(())
    })();
    app.wait_for_writes();
    give_back();
    result
}

/// The terminal, as ratatui takes it, with the mouse's clicks and wheel.
fn take() -> DefaultTerminal {
    let terminal = ratatui::init();
    // Work on a thread of its own that panics is said to have stopped; the
    // terminal stays the TUI's, which goes on.
    static HOOK: std::sync::Once = std::sync::Once::new();
    HOOK.call_once(|| {
        let hook = std::panic::take_hook();
        std::panic::set_hook(Box::new(move |info| {
            if std::thread::current().name() == Some("main") {
                give_back_keys();
                hook(info);
            }
        }));
    });
    // Keys said with their modifiers, ⌘ or the Windows key among them, by
    // terminals that can, as kitty: ⌘G, or super+g, for the map. Those
    // that cannot pass the asking by.
    let _ = execute!(
        std::io::stdout(),
        EnableMouseCapture,
        PushKeyboardEnhancementFlags(KeyboardEnhancementFlags::DISAMBIGUATE_ESCAPE_CODES)
    );
    terminal
}

/// The mouse, and keys as terminals say them by default, given back.
fn give_back_keys() {
    let _ = execute!(std::io::stdout(), DisableMouseCapture);
    let _ = execute!(std::io::stdout(), PopKeyboardEnhancementFlags);
}

fn give_back() {
    give_back_keys();
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
            group: None,
            to_read: 10,
            focus: Focus::List,
            query: String::new(),
            found: None,
            found_for: None,

            searching: false,
            sort: Sort::Relevance,
            shown: Vec::new(),
            list: ListState::default(),
            pane: crate::ui::DETAILS,
            dragging: None,
            pane_room: u16::MAX,
            marked: BTreeSet::new(),
            popup: Popup::None,
            view: View::List,
            map: None,
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
        // The papers may have changed: what a query finds, found again.
        self.found_for = None;
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
        if let Some(m) = &self.map {
            shelves.push((Shelf::Heading("THEMES"), 0));
            shelves.extend(
                m.map()
                    .themes
                    .iter()
                    .take(m.map().named())
                    .enumerate()
                    .map(|(i, t)| (Shelf::Theme(i, t.name()), t.size)),
            );
        }
        self.shelves = shelves;
        // A theme's shelf found again by its number, as its name may change.
        let same = |s: &Shelf, p: &Shelf| match (s, p) {
            (Shelf::Theme(a, _), Shelf::Theme(b, _)) => a == b,
            _ => s == p,
        };
        self.shelf = picked
            .and_then(|p| self.shelves.iter().position(|(s, _)| same(s, &p)))
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

    /// Whether a key asked to quit.
    #[cfg(test)]
    pub(crate) fn quitting(&self) -> bool {
        self.quit
    }

    pub fn current(&self) -> Option<&Hit> {
        self.shown.get(self.list.selected()?)
    }

    pub fn pdf(&self, h: &Hit) -> PathBuf {
        self.lib.root.join(&h.path).with_extension("pdf")
    }

    /// What is shown: the shelf's query and the one typed.
    pub fn wanted(&self) -> Query {
        self.shelves[self.shelf]
            .0
            .query()
            .and(Query::parse(&self.query))
    }

    /// Show the papers on the shelf that the query finds.
    pub fn narrow(&mut self) {
        let query = self.wanted();
        let key = self.current().map(|h| h.record.key.clone());
        match self.index.search(&query, self.sort, false, None) {
            Ok(hits) => self.shown = hits,
            Err(e) => self.status = format!("{e:#}"),
        }
        if let (Shelf::Theme(t, _), Some(m)) = (&self.shelves[self.shelf].0, &mut self.map) {
            let of: HashSet<&str> = m.map().members(*t).map(|(_, k)| k).collect();
            self.shown.retain(|h| of.contains(h.record.key.as_str()));
        }
        // Papers from the map, alone, while their shelf is shown.
        let shelf = &self.shelves[self.shelf].0;
        self.group
            .take_if(|g| !matches!(shelf, Shelf::Theme(t, _) if *t == g.theme));
        if let Some(g) = &self.group {
            self.shown.retain(|h| g.keys.contains(&h.record.key));
        }
        // Its shelf picked, the theme is picked on the map too.
        if let Shelf::Theme(t, _) = self.shelves[self.shelf].0 {
            self.pick_theme(t);
        }
        if self.view == View::Map {
            self.find();
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
        let mut key = plain(key)?;
        let ctrl = key.modifiers.contains(KeyModifiers::CONTROL);
        if ctrl && key.code == KeyCode::Char('c') {
            self.quit = true;
            return None;
        }
        // As Emacs cancels: a dialog, a search being typed, the map; not the
        // list's esc, which would unmark all.
        if ctrl
            && key.code == KeyCode::Char('g')
            && (!matches!(self.popup, Popup::None) || self.searching || self.view == View::Map)
        {
            key = KeyEvent::new(KeyCode::Esc, KeyModifiers::NONE);
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
                    _ => edit_line(line, key),
                }
                return None;
            }
            Popup::Ask(ask) => {
                match key.code {
                    KeyCode::Esc => self.popup = Popup::None,
                    KeyCode::Enter => {
                        if let Popup::Ask(ask) = std::mem::replace(&mut self.popup, Popup::None) {
                            self.answer(*ask);
                        }
                    }
                    _ => edit_line(&mut ask.line, key),
                }
                return None;
            }
            Popup::Card(card) => {
                let last = card.papers.len().saturating_sub(1);
                match key.code {
                    KeyCode::Char('j') | KeyCode::Down => card.at = (card.at + 1).min(last),
                    KeyCode::Char('k') | KeyCode::Up => card.at = card.at.saturating_sub(1),
                    KeyCode::PageDown => card.at = (card.at + 10).min(last),
                    KeyCode::PageUp => card.at = card.at.saturating_sub(10),
                    KeyCode::Char('g') | KeyCode::Home => card.at = 0,
                    KeyCode::Char('G') | KeyCode::End => card.at = last,
                    KeyCode::Enter | KeyCode::Char('o') => return self.open_card(),
                    KeyCode::Char('p') => self.card_pdf(),
                    // The next theme's card, or the one before.
                    KeyCode::Right => self.next_card(1),
                    KeyCode::Left | KeyCode::Char('h') => self.next_card(-1),
                    // The theme's papers in the list; a finer theme's there
                    // alone.
                    KeyCode::Char('l') => {
                        let finer = card.finer.then(|| {
                            let keys = card.papers.iter().map(|h| h.record.key.clone());
                            (card.name.clone(), keys.collect())
                        });
                        self.popup = Popup::None;
                        self.show_papers(finer, None);
                    }
                    KeyCode::Esc | KeyCode::Char('q') => self.popup = Popup::None,
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
            Popup::Help(scroll) => {
                match key.code {
                    KeyCode::Char('j') | KeyCode::Down => *scroll = scroll.saturating_add(1),
                    KeyCode::Char('k') | KeyCode::Up => *scroll = scroll.saturating_sub(1),
                    KeyCode::PageDown | KeyCode::Char(' ') => *scroll = scroll.saturating_add(10),
                    KeyCode::PageUp => *scroll = scroll.saturating_sub(10),
                    _ => self.popup = Popup::None,
                }
                return None;
            }
            Popup::Fetch(fetch) => {
                match key.code {
                    KeyCode::Esc => {
                        self.status = "no PDF got".to_string();
                        self.close_popup();
                    }
                    KeyCode::Enter => {
                        let line = fetch.line.trim().to_string();
                        if !line.is_empty() {
                            self.attach_from(&line);
                        }
                    }
                    KeyCode::Char('o') if ctrl => {
                        if let Err(e) = browser::open(&fetch.page) {
                            fetch.note = one_line(&e);
                        }
                    }
                    KeyCode::Char('l') if ctrl => return Some(Action::Locate),
                    _ => edit_line(&mut fetch.line, key),
                }
                return None;
            }
            Popup::Busy(busy) => {
                if let Then::Fetch(..) = busy.then {
                    match key.code {
                        KeyCode::Char('b') => {
                            busy.browse = true;
                            busy.stop.store(true, Ordering::Relaxed);
                        }
                        KeyCode::Esc => busy.stop.store(true, Ordering::Relaxed),
                        _ => {}
                    }
                }
                return None;
            }
            Popup::Delete(_) => {
                if key.code == KeyCode::Char('y') {
                    self.delete();
                } else {
                    self.popup = Popup::None;
                    self.status = "nothing deleted".to_string();
                }
                return None;
            }
            Popup::Choose(choose) => {
                match key.code {
                    KeyCode::Esc => {
                        self.popup = Popup::None;
                        self.status = "nothing added".to_string();
                    }
                    KeyCode::Char('j') | KeyCode::Down => {
                        choose.at = (choose.at + 1).min(choose.papers.len() - 1);
                    }
                    KeyCode::Char('k') | KeyCode::Up => choose.at = choose.at.saturating_sub(1),
                    KeyCode::Char(c @ '1'..='9') => {
                        let n = c as usize - '1' as usize;
                        if n < choose.papers.len() {
                            choose.at = n;
                        }
                    }
                    KeyCode::Enter => {
                        self.status = format!("adding {}…", choose.papers[choose.at].title());
                        return Some(Action::Choose);
                    }
                    _ => {}
                }
                return None;
            }
            Popup::None => {}
        }
        // ⌘G, or super+g, where the terminal says so: the map, and back,
        // while searching too.
        if key.modifiers.contains(KeyModifiers::SUPER) && key.code == KeyCode::Char('g') {
            self.searching = false;
            self.toggle_view();
            return None;
        }
        // Help, from the map and while typing a search too.
        if key.code == KeyCode::F(1) {
            self.popup = Popup::Help(0);
            return None;
        }
        // On the map, the list's keys would act on papers not shown.
        if self.view == View::Map
            && !self.searching
            && (self.map_key(key) || !matches!(key.code, KeyCode::Char('q' | '?' | 'a' | '/')))
        {
            return None;
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
                _ => edit_line(&mut self.query, key),
            }
            self.narrow();
            return None;
        }
        // A ctrl-letter not bound above is nothing, not its letter.
        if ctrl && matches!(key.code, KeyCode::Char(_)) {
            return None;
        }
        match key.code {
            KeyCode::Char('q') => self.quit = true,
            KeyCode::Char('?') => self.popup = Popup::Help(0),
            KeyCode::Char('/') => {
                // On the map, it stays, what is found lit as it is typed.
                self.searching = true;
                self.focus = Focus::List;
            }
            KeyCode::Esc if !self.query.is_empty() => {
                self.query.clear();
                self.narrow();
            }
            // Papers from the map, back among all of their shelf.
            KeyCode::Esc if self.group.is_some() => {
                self.group = None;
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
            KeyCode::Up if ctrl => self.step(isize::MIN),
            KeyCode::Down if ctrl => self.step(isize::MAX),
            KeyCode::Char('j') | KeyCode::Down => self.step(1),
            KeyCode::Char('k') | KeyCode::Up => self.step(-1),
            KeyCode::PageDown => self.step(5),
            KeyCode::PageUp => self.step(-5),
            KeyCode::Home | KeyCode::Char('g') => self.step(isize::MIN),
            KeyCode::End | KeyCode::Char('G') => self.step(isize::MAX),
            KeyCode::Char('S') => self.resort(),
            KeyCode::Char('n') => match &self.shelves[self.shelf].0 {
                _ if self.group.is_some() => {
                    self.status = "n names a finer theme on the map, within its theme".to_string();
                }
                Shelf::Theme(t, _) => self.ask_name(*t),
                _ => self.status = "n names a theme: pick one under THEMES first".to_string(),
            },
            KeyCode::Char('L') => {
                let keys: Vec<String> = if self.marked.is_empty() {
                    self.shown.iter().map(|h| h.record.key.clone()).collect()
                } else {
                    self.marked.iter().cloned().collect()
                };
                let name = match (&self.group, &self.shelves[self.shelf].0) {
                    (Some(g), _) => g.name.clone(),
                    (None, Shelf::Theme(_, name)) => name.clone(),
                    _ => String::new(),
                };
                self.ask_label(keys, name);
            }
            KeyCode::Char('m') => self.toggle_view(),
            // The pane's edge further left, the pane wider; or right.
            KeyCode::Char('<') => self.widen(4),
            KeyCode::Char('>') => self.widen(-4),
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
            KeyCode::Char('d') | KeyCode::Delete => self.ask_delete(),
            KeyCode::Char('x') => self.ask_drop_pdf(),
            KeyCode::Char('p') => self.get_pdf(),
            KeyCode::Char('f') => return Some(Action::Locate),
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
                return Some(Action::Open(None));
            }
            KeyCode::Enter => self.focus = Focus::List,
            _ => {}
        }
        None
    }

    /// The pane of the paper picked `by` columns wider, or narrower, as
    /// wide as it may be.
    fn widen(&mut self, by: i16) {
        self.set_pane(self.pane.saturating_add_signed(by));
    }

    fn set_pane(&mut self, width: u16) {
        let room = self.pane_room.max(crate::ui::NARROWEST);
        self.pane = width.clamp(crate::ui::NARROWEST, room);
    }

    /// Take a click or a turn of the wheel at where it lands.
    pub fn mouse(&mut self, m: MouseEvent) -> Option<Action> {
        let at = self
            .targets
            .iter()
            .rev()
            .find(|(r, _)| r.contains(Position::new(m.column, m.row)))
            .map(|&(_, t)| t);
        // The pane's edge, caught, follows the pointer till let go; at
        // anything but a drag, let go, as where the release went unheard.
        if let Some(right) = self.dragging {
            if m.kind == MouseEventKind::Drag(MouseButton::Left) {
                self.set_pane(right.saturating_sub(m.column));
                return None;
            }
            self.dragging = None;
            if matches!(m.kind, MouseEventKind::Up(_)) {
                return None;
            }
        }
        // Over the map, a name on it too, the wheel sideways, or with
        // shift, moves it, once shown closer: a trackpad's stray sideways
        // turn leaves the whole map as it is.
        let on_map = at == Some(Target::Map) && matches!(self.popup, Popup::None);
        let over_map = matches!(at, Some(Target::Map | Target::Label(_)))
            && matches!(self.popup, Popup::None)
            && self.map.as_ref().is_some_and(|m| m.top().zoom > 0);
        let shift = m.modifiers.contains(KeyModifiers::SHIFT);
        let pan = match m.kind {
            MouseEventKind::ScrollRight => Some((1, 0)),
            MouseEventKind::ScrollLeft => Some((-1, 0)),
            MouseEventKind::ScrollUp if shift => Some((0, 1)),
            MouseEventKind::ScrollDown if shift => Some((0, -1)),
            _ => None,
        };
        if over_map && let Some((dx, dy)) = pan {
            self.pan(dx, dy);
            return None;
        }
        let by = match m.kind {
            MouseEventKind::Down(MouseButton::Left) if on_map => {
                return self.map_click(m.column, m.row);
            }
            MouseEventKind::Down(MouseButton::Left) => return self.click(at),
            MouseEventKind::ScrollDown => 1,
            MouseEventKind::ScrollUp => -1,
            _ => return None,
        };
        match (&mut self.popup, at) {
            (Popup::Cite(_, scroll), _) => *scroll = scroll.saturating_add_signed(by as i16),
            (Popup::None, Some(Target::Map | Target::Label(_))) => self.wheel_theme(by),
            (Popup::Card(card), _) => {
                let last = card.papers.len().saturating_sub(1) as isize;
                card.at = (card.at as isize + by).clamp(0, last.max(0)) as usize;
            }
            (Popup::None, Some(Target::Side | Target::Shelf(_))) => {
                self.focus = Focus::Side;
                self.step(by);
            }
            (
                Popup::None,
                Some(
                    Target::List
                    | Target::Paper(_)
                    | Target::Mark(_)
                    | Target::Star(_)
                    | Target::GetPdf(_),
                ),
            ) => {
                self.focus = Focus::List;
                self.step(by);
            }
            _ => {}
        }
        None
    }

    /// Whether `what` was clicked just before, a double click; if not, it
    /// is the one clicked last.
    fn twice(&mut self, what: Clicked) -> bool {
        let again = self
            .clicked
            .is_some_and(|(when, was)| was == what && when.elapsed() < DOUBLE_CLICK);
        self.clicked = (!again).then(|| (Instant::now(), what));
        again
    }

    fn click(&mut self, at: Option<Target>) -> Option<Action> {
        if let (Some(Target::CardPaper(i)), Popup::Card(card)) = (at, &mut self.popup) {
            // Once picks it; twice opens it.
            card.at = i.min(card.papers.len().saturating_sub(1));
            return if self.twice(Clicked::Card(i)) {
                self.open_card()
            } else {
                None
            };
        }
        if !matches!(self.popup, Popup::None) {
            if at != Some(Target::Popup) && !matches!(self.popup, Popup::Busy(_)) {
                self.close_popup();
            }
            return None;
        }
        self.searching = false;
        match at? {
            Target::Search => {
                self.searching = true;
                self.focus = Focus::List;
            }
            Target::Key(key) => {
                // As the list's keys would be pressed in the list.
                self.focus = Focus::List;
                return self.key(key);
            }
            Target::Label(t) => self.pick_name(t, true),
            Target::Edge(right) => self.dragging = Some(right),
            Target::CardPaper(_) => {}
            // A button, pressed where it is.
            Target::Button(key) => return self.key(key),
            Target::Sort => self.resort(),
            Target::Side => self.focus = Focus::Side,
            Target::Shelf(i) => {
                self.focus = Focus::Side;
                if i != self.shelf {
                    self.shelf = i;
                    self.list.select(Some(0));
                    self.narrow();
                }
            }
            Target::List | Target::Popup | Target::Map => self.focus = Focus::List,
            Target::Paper(i) => {
                self.focus = Focus::List;
                self.list.select(Some(i));
                if self.twice(Clicked::Paper(i)) {
                    return Some(Action::Open(None));
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
                if self.shown.get(i).is_some_and(|h| h.has_pdf) {
                    let h = &self.shown[i];
                    self.popup = Popup::Delete(Doomed::Pdfs(vec![(
                        h.record.key.clone(),
                        h.record.title().to_string(),
                    )]));
                } else {
                    self.get_pdf();
                }
            }
        }
        None
    }

    /// The shelves again, the map's themes among them, keeping to the one
    /// picked; on a theme's, the theme picked on the map.
    fn reshelve(&mut self) {
        if let (Some(m), Some((Shelf::Theme(t, _), _))) =
            (&self.map, self.shelves.get_mut(self.shelf))
        {
            *t = m.lens.picked;
        }
        if let Err(e) = self.restock() {
            self.status = one_line(&e);
        }
        self.narrow();
    }

    /// Ask for a label to put on the papers of `keys`, `name` to start from.
    fn ask_label(&mut self, keys: Vec<String>, name: String) {
        if keys.is_empty() {
            self.status = "no papers to label".to_string();
            return;
        }
        // So many are labelled only by a name typed out, not one offered.
        let line = if keys.len() < MANY_LABELLED {
            name
        } else {
            String::new()
        };
        self.popup = Popup::Ask(Box::new(Ask {
            what: Asked::Label(keys),
            line,
        }));
    }

    /// Do what the name typed was for.
    fn answer(&mut self, ask: Ask) {
        let line = ask.line.trim();
        match ask.what {
            Asked::Name(t) => {
                let Some(m) = &mut self.map else { return };
                self.status = match m.kept.name(t, line) {
                    Ok(()) => format!("theme {} is {}", t + 1, m.map().themes[t].name()),
                    Err(e) => one_line(&e),
                };
            }
            Asked::Label(keys) => {
                self.status = match crate::remove::label(&self.lib, &mut self.index, &keys, line) {
                    Ok(n) => format!(
                        "{n} papers labelled {line}; d on it under LABELS takes it off again"
                    ),
                    Err(e) => one_line(&e),
                };
            }
        }
        self.reshelve();
    }

    /// Sort by the next of [`Sort::ALL`] that orders the papers otherwise,
    /// as relevance does not without words to rank by; saying by what.
    fn resort(&mut self) {
        let wanted = self.wanted();
        let was = self.sort.of(&wanted);
        let at = Sort::ALL.iter().position(|&s| s == self.sort).unwrap_or(0);
        self.sort = (1..Sort::ALL.len())
            .map(|i| Sort::ALL[(at + i) % Sort::ALL.len()])
            .find(|s| s.of(&wanted) != was)
            .unwrap_or(self.sort);
        self.narrow();
        self.status = format!("sorted by {}", self.sort.of(&wanted).name());
    }

    /// Move `by` papers, or shelves, as far as there are: `isize::MIN` to
    /// the first, `isize::MAX` to the last.
    fn step(&mut self, by: isize) {
        match self.focus {
            Focus::Side => {
                let (mut at, mut left) = (self.shelf, by.unsigned_abs());
                while left > 0 {
                    let next = at.saturating_add_signed(by.signum());
                    if next == at || next >= self.shelves.len() {
                        break;
                    }
                    at = next;
                    if !matches!(self.shelves[at].0, Shelf::Heading(_)) {
                        self.shelf = at;
                        left -= 1;
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
        let (what, steps) = Source::of(line).plan();
        let options = self.options();
        let line = line.to_string();
        self.work("Adding", what, Then::Add, move |lib, index, net, say| {
            say(&format!("asking {}", steps.join(", then ")));
            Adder {
                lib,
                index,
                net,
                options,
            }
            .add(&line)
        });
    }

    /// File the PDF asked about as the paper picked of those it holds,
    /// and show it.
    pub fn choose(&mut self) {
        let Popup::Choose(choose) = std::mem::replace(&mut self.popup, Popup::None) else {
            return;
        };
        let Choose {
            papers, pdf, at, ..
        } = *choose;
        let Some(found) = papers.into_iter().nth(at) else {
            return;
        };
        let options = self.options();
        let what = found.title().to_string();
        self.work("Adding", what, Then::Add, move |lib, index, net, say| {
            say("asking the registries of it");
            Adder {
                lib,
                index,
                net,
                options,
            }
            .file_chosen(found, &pdf)
        });
    }

    /// What is added is added into the folder or label picked on the left,
    /// with an open copy of its PDF.
    fn options(&self) -> Options {
        let mut options = Options {
            pdf: true,
            ..Options::default()
        };
        match &self.shelves[self.shelf].0 {
            Shelf::Folder(f) => options.folders.push(f.clone()),
            Shelf::Label(l) => options.labels.push(l.clone()),
            _ => {}
        }
        options
    }

    /// Do `job` on a thread of its own, with a library and index of its own,
    /// brought up to what is on disk; what it says of its steps, by `say`,
    /// is shown in a popup until it is done, and `then` is done with what it
    /// gave. `say` answers whether to go on.
    fn work(
        &mut self,
        title: &'static str,
        what: String,
        then: Then,
        job: impl FnOnce(&Library, &mut Index, &Net, &dyn Fn(&str) -> bool) -> anyhow::Result<Added>
        + Send
        + 'static,
    ) {
        let Some(net) = self.net.clone() else {
            self.status = "cannot fetch here: nothing is fetched".to_string();
            return;
        };
        let root = self.lib.root.clone();
        self.spawn(title, what, then, move |say| {
            Library::open(&root)
                .and_then(|lib| {
                    let mut index = Index::open(&crate::index::default_path(&lib.root))?;
                    index.refresh(&lib)?;
                    job(&lib, &mut index, &net, say)
                })
                .map(Done::Added)
        });
    }

    /// Do `job` on a thread of its own; what it says of its steps, by
    /// `say`, is shown in a popup until it is done, and `then` is done with
    /// what it gave. `say` answers whether to go on.
    fn spawn(
        &mut self,
        title: &'static str,
        what: String,
        then: Then,
        job: impl FnOnce(&dyn Fn(&str) -> bool) -> anyhow::Result<Done> + Send + 'static,
    ) {
        let (tx, events) = mpsc::channel();
        let stop = Arc::new(AtomicBool::new(false));
        let stopped = stop.clone();
        std::thread::spawn(move || {
            let done = job(&|s: &str| {
                let _ = tx.send(Report::Said(s.to_string()));
                !stopped.load(Ordering::Relaxed)
            });
            let _ = tx.send(Report::Done(done));
        });
        self.status.clear();
        self.popup = Popup::Busy(Box::new(Busy {
            title,
            what,
            said: Vec::new(),
            since: Instant::now(),
            events,
            stop,
            browse: false,
            then,
        }));
    }

    /// Take what the work on its own thread has said; when it is done, do
    /// with what it gave as it was to be done.
    fn heed(&mut self) {
        let Popup::Busy(busy) = &mut self.popup else {
            return;
        };
        let done = loop {
            match busy.events.try_recv() {
                Ok(Report::Said(s)) => {
                    // A step said again, as it gets on, in place of before.
                    if busy
                        .said
                        .last()
                        .is_some_and(|last| map::same_step(last, &s))
                    {
                        busy.said.pop();
                    }
                    busy.said.push(s);
                }
                Ok(Report::Done(done)) => break done,
                Err(mpsc::TryRecvError::Empty) => return,
                Err(mpsc::TryRecvError::Disconnected) => {
                    break Err(anyhow::anyhow!("the work stopped, saying nothing"));
                }
            }
        };
        let Popup::Busy(busy) = std::mem::replace(&mut self.popup, Popup::None) else {
            return;
        };
        // What the work wrote, read into this index; the map's writes none.
        if !matches!(busy.then, Then::Map | Then::Within) {
            if let Err(e) = self.index.refresh(&self.lib) {
                self.status = one_line(&e);
            }
            self.found_for = None;
        }
        match (busy.then, done) {
            (Then::Map, Ok(Done::Mapped(mapped))) => self.mapped(*mapped),
            (Then::Within, Ok(Done::Within(t, inside))) => {
                if let Some(m) = &mut self.map {
                    m.within.insert(t, *inside);
                }
                self.go_in(t);
            }
            (Then::Map | Then::Within, Err(e)) => self.status = one_line(&e),
            (Then::Fetch(_, back), Ok(Done::Added(added))) => {
                self.attached(&added);
                self.back_to(back);
            }
            (Then::Add, Ok(Done::Added(added))) => self.added(Ok(added)),
            // Work gives what it was started for.
            (_, Ok(_)) => {}
            (Then::Fetch(h, back), Err(e)) if !busy.stop.load(Ordering::Relaxed) => {
                self.browse(&h, format!("no open copy: {}", one_line(&e)), back);
            }
            (Then::Fetch(h, back), Err(_)) if busy.browse => self.browse(&h, String::new(), back),
            (Then::Fetch(h, back), Err(_)) => {
                self.status = format!("{}: stopped; no PDF got", h.record.key);
                self.back_to(back);
            }
            (Then::Add, Err(e)) => match e.downcast::<Ambiguous>() {
                Ok(a) => {
                    self.popup = Popup::Choose(Box::new(Choose {
                        name: a.name,
                        papers: a.papers,
                        pdf: a.pdf,
                        at: 0,
                    }));
                }
                Err(e) => self.added(Err(e)),
            },
        }
    }

    /// Say what came of adding, and show the paper added.
    fn added(&mut self, done: anyhow::Result<crate::add::Added>) {
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
                // Shown on the map, as a cross, to be put in a theme.
                if self.view == View::Map {
                    self.bring_up_map(None);
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

    /// Ask whether to delete what `d` was pressed on: on the left, the
    /// collection or label picked; else the papers marked, or the one picked.
    pub fn ask_delete(&mut self) {
        use crate::remove::Shelf as Kind;
        let doomed = match (&self.focus, &self.shelves[self.shelf]) {
            (Focus::Side, (Shelf::Folder(n), count)) => {
                Doomed::Shelf(Kind::Collection, n.clone(), *count)
            }
            (Focus::Side, (Shelf::Label(n), count)) => {
                Doomed::Shelf(Kind::Label, n.clone(), *count)
            }
            (Focus::Side, _) => {
                self.status = "only a collection or a label is deleted from here".to_string();
                return;
            }
            (Focus::List, _) => {
                let picked = self.picked();
                if picked.is_empty() {
                    return;
                }
                Doomed::Papers(
                    picked
                        .iter()
                        .map(|h| (h.record.key.clone(), h.record.title().to_string()))
                        .collect(),
                )
            }
        };
        self.popup = Popup::Delete(doomed);
    }

    /// Ask whether to delete the PDFs of the papers marked, or of the one
    /// picked. The papers stay.
    pub fn ask_drop_pdf(&mut self) {
        if self.focus != Focus::List {
            self.status = "a paper's PDF is deleted from the list".to_string();
            return;
        }
        let papers: Vec<(String, String)> = self
            .picked()
            .iter()
            .filter(|h| h.has_pdf)
            .map(|h| (h.record.key.clone(), h.record.title().to_string()))
            .collect();
        if papers.is_empty() {
            self.status = "no PDF to delete".to_string();
            return;
        }
        self.popup = Popup::Delete(Doomed::Pdfs(papers));
    }

    /// Delete what was asked about, and say what came of it.
    fn delete(&mut self) {
        let Popup::Delete(doomed) = std::mem::replace(&mut self.popup, Popup::None) else {
            return;
        };
        // What another program changed meanwhile is read first.
        if let Err(e) = self.index.refresh(&self.lib) {
            self.status = one_line(&e);
            return;
        }
        self.status = match doomed {
            Doomed::Papers(papers) => {
                let mut deleted = 0;
                let mut why = None;
                for (key, _) in &papers {
                    match crate::remove::paper(&self.lib, &mut self.index, key) {
                        Ok(_) => {
                            deleted += 1;
                            self.marked.remove(key);
                        }
                        Err(e) => why = Some(one_line(&e)),
                    }
                }
                match why {
                    None if deleted == 1 => format!("{} deleted, to .trash/", papers[0].0),
                    None => format!("{deleted} papers deleted, to .trash/"),
                    Some(why) => format!("{deleted} of {} deleted: {why}", papers.len()),
                }
            }
            Doomed::Pdfs(papers) => {
                let mut deleted = 0;
                let mut why = None;
                for (key, _) in &papers {
                    match crate::remove::pdf(&self.lib, &mut self.index, key) {
                        Ok(_) => deleted += 1,
                        Err(e) => why = Some(one_line(&e)),
                    }
                }
                match why {
                    None if deleted == 1 => format!("{}'s PDF deleted, to .trash/", papers[0].0),
                    None => format!("{deleted} PDFs deleted, to .trash/"),
                    Some(why) => format!("{deleted} of {} deleted: {why}", papers.len()),
                }
            }
            Doomed::Shelf(kind, name, _) => {
                match crate::remove::shelf(&self.lib, &mut self.index, kind, &name) {
                    Ok(n) => format!("{} {name} deleted; its {n} papers kept", kind.name()),
                    Err(e) => one_line(&e),
                }
            }
        };
        if let Err(e) = self.restock() {
            self.status = one_line(&e);
        }
        self.narrow();
    }

    /// Get the PDF of the paper picked: an open copy, downloaded on a
    /// thread of its own, its steps shown, if one is to be had; else, or
    /// asked, in the browser.
    pub fn get_pdf(&mut self) {
        if let Some(h) = self.current().cloned() {
            self.fetch_pdf(h, None);
        }
    }

    /// Get the PDF of the paper picked on the card, the card shown again
    /// after.
    fn card_pdf(&mut self) {
        let Popup::Card(card) = std::mem::replace(&mut self.popup, Popup::None) else {
            return;
        };
        match card.papers.get(card.at).cloned() {
            Some(h) => self.fetch_pdf(h, Some(card)),
            None => self.popup = Popup::Card(card),
        }
    }

    /// The card a PDF was asked for from, again, that paper read anew;
    /// unless what came after opened something else.
    fn back_to(&mut self, back: Option<Box<Card>>) {
        let Some(mut card) = back else {
            return;
        };
        if !matches!(self.popup, Popup::None) {
            return;
        }
        if let Some(h) = card.papers.get_mut(card.at)
            && let Ok(Some(now)) = self.index.get(&h.record.key)
        {
            *h = now;
        }
        self.popup = Popup::Card(card);
    }

    /// The popup closed, whichever it is; where it awaited a PDF asked for
    /// from a card, the card again.
    fn close_popup(&mut self) {
        if let Popup::Fetch(fetch) = std::mem::replace(&mut self.popup, Popup::None) {
            self.back_to(fetch.back);
        }
    }

    /// Get `h`'s PDF, `back` shown again after.
    fn fetch_pdf(&mut self, h: Hit, back: Option<Box<Card>>) {
        // As it is now: another program may have filed its PDF since.
        let h = match self.index.get(&h.record.key) {
            Ok(Some(now)) => now,
            _ => h,
        };
        if h.has_pdf {
            self.status = format!("{} has its PDF: ⏎ opens it", h.record.key);
            self.back_to(back);
            return;
        }
        if self.net.is_none() {
            self.browse(&h, String::new(), back);
            return;
        }
        let what = h.record.title().to_string();
        let hit = h.clone();
        self.work(
            "Looking for an open copy",
            what,
            Then::Fetch(h, back),
            move |lib, index, net, say| open_copy(lib, index, net, &hit, false, say),
        );
    }

    /// Get the PDF of `h` in the browser: its page opened there, the PDF
    /// downloaded there awaited; `note` said in the dialog; `back` shown
    /// again after.
    fn browse(&mut self, h: &Hit, note: String, back: Option<Box<Card>>) {
        let page = browser::page(&h.record);
        let note = match browser::open(&page) {
            Ok(()) => note,
            Err(e) => one_line(&e),
        };
        self.popup = Popup::Fetch(Box::new(Fetch {
            key: h.record.key.clone(),
            title: h.record.title().to_string(),
            page,
            line: String::new(),
            arrivals: Arrivals::new(browser::downloads()),
            note,
            back,
        }));
    }

    /// Look for the PDF awaited among what has been downloaded since; say
    /// whether anything came of it, to be shown.
    pub fn tick(&mut self) -> bool {
        if let Popup::Busy(_) = self.popup {
            self.heed();
            return true;
        }
        let Popup::Fetch(fetch) = &mut self.popup else {
            return false;
        };
        let mut came = false;
        for path in fetch.arrivals.next() {
            let from = browser::tilde(&path);
            let done = std::fs::read(&path)
                .map_err(anyhow::Error::from)
                .and_then(|bytes| {
                    attach(&self.lib, &mut self.index, &fetch.key, &bytes, &from, false)
                });
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
                Some(net) => net.pdf_via_page(line, &|_| true).map(|(bytes, _)| bytes),
                None => Err(anyhow::anyhow!("cannot download here")),
            }
        } else {
            let path = browser::dropped(line);
            std::fs::read(&path).map_err(|e| anyhow::anyhow!("cannot read {}: {e}", path.display()))
        };
        let done =
            bytes.and_then(|b| attach(&self.lib, &mut self.index, &fetch.key, &b, line, true));
        match done {
            Ok(added) => self.attached(&added),
            Err(e) => fetch.note = one_line(&e),
        }
    }

    fn attached(&mut self, added: &crate::add::Added) {
        let back = match std::mem::replace(&mut self.popup, Popup::None) {
            Popup::Fetch(fetch) => fetch.back,
            _ => None,
        };
        self.status = format!("{}: {}", added.key, added.summary());
        if let Err(e) = self.restock() {
            self.status = one_line(&e);
        }
        self.narrow();
        self.back_to(back);
    }

    /// The paper to give a PDF from disk: the one whose PDF is awaited, or
    /// the one picked, where it has none yet.
    fn paper_to_file(&mut self) -> Option<String> {
        if let Popup::Fetch(fetch) = &self.popup {
            return Some(fetch.key.clone());
        }
        let h = self.current()?;
        if h.has_pdf {
            self.status = format!("{} has its PDF: x deletes it", h.record.key);
            return None;
        }
        Some(h.record.key.clone())
    }

    /// Give that paper the PDF at `path`, as one the person named.
    pub fn give_pdf(&mut self, path: &Path) {
        let Some(key) = self.paper_to_file() else {
            return;
        };
        let line = browser::tilde(path);
        let done = std::fs::read(path)
            .map_err(|e| anyhow::anyhow!("cannot read {}: {e}", path.display()))
            .and_then(|b| attach(&self.lib, &mut self.index, &key, &b, &line, true));
        match done {
            Ok(added) => self.attached(&added),
            Err(e) => self.note(one_line(&e)),
        }
    }

    /// Ask for a PDF on disk and file it beside the paper picked, or the
    /// one whose PDF is awaited.
    fn locate(&mut self, terminal: &mut DefaultTerminal) {
        let Some(key) = self.paper_to_file() else {
            return;
        };
        give_back();
        let chosen = browser::choose_pdf(&format!("PDF for {key}"));
        *terminal = take();
        let note = match chosen {
            Ok(Some(path)) => return self.give_pdf(&path),
            Ok(None) => "no file chosen".to_string(),
            Err(e) => one_line(&e),
        };
        self.note(note);
    }

    /// Where a PDF is awaited, say `note` in that dialog; else on the
    /// status line.
    fn note(&mut self, note: String) {
        if let Popup::Fetch(fetch) = &mut self.popup {
            fetch.note = note;
        } else {
            self.status = note;
        }
    }

    /// Present the paper's PDF with ohp, which takes the terminal until it
    /// quits.
    fn open(&mut self, terminal: &mut DefaultTerminal, paper: Option<Hit>) {
        let Some(h) = paper.as_ref().or(self.current()) else {
            return;
        };
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
/// `<space>`, `<up>`, `<down>`, `<left>`, `<right>`, `<backspace>`, `<home>`,
/// `<end>`, `<pageup>`, `<pagedown>`, `<del>`, `<f1>`; any of those or a
/// character with control, as `<c-e>` or `<c-up>`, with ⌘, as `<d-g>`, or
/// with shift, as `<s-left>`;
/// anything else, each character typed.
pub fn keys(spec: &str) -> Vec<KeyEvent> {
    let Some(name) = spec.strip_prefix('<').and_then(|s| s.strip_suffix('>')) else {
        return spec
            .chars()
            .map(|c| KeyEvent::new(KeyCode::Char(c), KeyModifiers::NONE))
            .collect();
    };
    // With control, or ⌘ (super), as vim writes them.
    let (mods, name) = match name.split_at_checked(2) {
        Some(("c-", rest)) => (KeyModifiers::CONTROL, rest),
        Some(("d-", rest)) => (KeyModifiers::SUPER, rest),
        Some(("s-", rest)) => (KeyModifiers::SHIFT, rest),
        _ => (KeyModifiers::NONE, name),
    };
    let code = match name {
        "enter" => KeyCode::Enter,
        "esc" => KeyCode::Esc,
        "tab" => KeyCode::Tab,
        "space" => KeyCode::Char(' '),
        "up" => KeyCode::Up,
        "down" => KeyCode::Down,
        "left" => KeyCode::Left,
        "right" => KeyCode::Right,
        "backspace" => KeyCode::Backspace,
        "home" => KeyCode::Home,
        "end" => KeyCode::End,
        "pageup" => KeyCode::PageUp,
        "pagedown" => KeyCode::PageDown,
        "del" => KeyCode::Delete,
        "f1" => KeyCode::F(1),
        _ => match name.chars().collect::<Vec<_>>()[..] {
            [c] if mods != KeyModifiers::NONE => KeyCode::Char(c),
            // Not a key's name: each character typed, as `<` alone is.
            _ => {
                return spec
                    .chars()
                    .map(|c| KeyEvent::new(KeyCode::Char(c), KeyModifiers::NONE))
                    .collect();
            }
        },
    };
    vec![KeyEvent::new(code, mods)]
}

/// `key` as the handlers read it, from a terminal that says keys with
/// all their modifiers: a small letter shifted as its capital; ctrl-m,
/// ctrl-[ and ctrl-i as Enter, Esc and Tab, as other terminals send them;
/// and ⌘, or super, with any key but G, which is the map's, as nothing,
/// those being the system's.
fn plain(mut key: KeyEvent) -> Option<KeyEvent> {
    if key.modifiers.contains(KeyModifiers::SUPER) && key.code != KeyCode::Char('g') {
        return None;
    }
    if let KeyCode::Char(c) = key.code {
        // A letter only: a symbol's shifted one is the keyboard's own.
        if key.modifiers == KeyModifiers::SHIFT && c.is_ascii_lowercase() {
            key.code = KeyCode::Char(c.to_ascii_uppercase());
        }
        if key.modifiers == KeyModifiers::CONTROL {
            let alias = match c {
                'm' => Some(KeyCode::Enter),
                '[' => Some(KeyCode::Esc),
                'i' => Some(KeyCode::Tab),
                _ => None,
            };
            if let Some(code) = alias {
                key = KeyEvent::new(code, KeyModifiers::NONE);
            }
        }
    }
    Some(key)
}

/// `key` typed into `line`: a character, backspace, ctrl-u to clear it or
/// ctrl-v to paste.
fn edit_line(line: &mut String, key: KeyEvent) {
    let ctrl = key.modifiers.contains(KeyModifiers::CONTROL);
    match key.code {
        KeyCode::Backspace => {
            line.pop();
        }
        KeyCode::Char('u') if ctrl => line.clear(),
        KeyCode::Char('v') if ctrl => line.push_str(paste().trim()),
        KeyCode::Char(c) if !ctrl => line.push(c),
        _ => {}
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
        if let Some(mut stdin) = c.stdin.take() {
            stdin.write_all(text.as_bytes())?;
        }
        c.wait()
    }) {
        Ok(_) => format!("copied {what}"),
        Err(e) => format!("cannot copy: {e}"),
    }
}
