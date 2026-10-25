//! The map of the library's themes in the TUI: what is kept of it while
//! shown, its keys and clicks, going into a theme, and its card.

use super::*;

/// How much finer each `+` gathers the themes, and how many steps each
/// way from 1 it goes.
pub(super) const STEP: f64 = 1.5;

pub(super) const STEPS: i32 = 5;

/// How many times twice as close the map can be shown.
pub(super) const ZOOMS: i32 = 3;

/// The map of themes, as kept on this machine, and what is picked on it.
pub struct MapView {
    pub kept: map::Kept,
    /// What is picked on the whole map, and how close it is shown.
    pub lens: Lens,
    /// The other way about from how the view shows them at first: as
    /// laid out, every theme in its colour, not the picked one alone; as a
    /// mosaic, the picked one alone.
    pub colours: bool,
    /// Where the map was last drawn.
    pub canvas: Rect,
    /// The theme gone into, shown as a map of its own.
    pub inside: Option<usize>,
    /// Each theme gone into, from within, kept while the map is.
    pub within: HashMap<usize, Inside>,
    /// The papers the map was last brought up to, placed or not.
    pub(super) seen: HashSet<String>,
    /// Shown as a mosaic, rather than as laid out; and the mosaic last
    /// drawn.
    pub mosaic: bool,
    pub(super) tiled: Option<Tiled>,
    /// Each theme coloured by how many of its papers came lately, rather
    /// than in its own colour; and how far back, as one of `LATELY`.
    pub recent: Option<Recent>,
    pub lately: usize,
}

/// The papers that came lately, of the library as it held so many.
pub struct Recent {
    pub keys: HashSet<String>,
    of: usize,
}

/// How far back papers count as recent at first: 6 months.
pub const LATELY_FIRST: usize = 2;

/// How far back papers count as recent: in days, and as said.
pub const LATELY: [(i64, &str); 7] = [
    (30, "month"),
    (91, "3 months"),
    (182, "6 months"),
    (365, "year"),
    (730, "2 years"),
    (1095, "3 years"),
    (1826, "5 years"),
];

/// The keys of the papers added in the last of `LATELY` at `lately`.
fn recent(index: &Index, lately: usize) -> HashSet<String> {
    index.added_since(crate::add::now_secs() - LATELY[lately].0 * 86_400)
}

/// The map shown closer: the middle of what it is closer to, and how wide
/// and high what is shown is.
type Closer = ([f64; 2], f64, f64);

/// The map shown, as a mosaic for a canvas of a size: what it was made
/// of, so a map changed since makes it anew.
pub(super) struct Tiled {
    mosaic: map::Mosaic,
    inside: Option<usize>,
    size: (u16, u16),
    from: Vec<[f32; 2]>,
}

impl Tiled {
    /// Whether it is the mosaic of `plain`, the map shown from `inside`.
    fn of(&self, plain: &map::Map, inside: Option<usize>) -> bool {
        self.inside == inside
            && self.from == plain.xy
            && self.mosaic.map.theme == plain.theme
            && self
                .mosaic
                .map
                .themes
                .iter()
                .map(|t| &t.called)
                .eq(plain.themes.iter().map(|t| &t.called))
    }
}

/// What is picked on a map, the whole or a theme's from within: a theme,
/// a paper, and how many times twice as close to the theme it is shown.
#[derive(Default)]
pub struct Lens {
    pub picked: usize,
    pub paper: Option<usize>,
    pub zoom: i32,
    /// Shown closer, how far from what it is closer to, across and up, in
    /// what is shown; with that, the theme picked when moved, so another
    /// picked is shown as it is.
    pub pan: Option<(usize, [f64; 2])>,
}

/// A theme from within: a map of its papers alone, in their own finer
/// themes; the links it shows; what is picked on it, and whether a finer
/// theme was chosen, or all are shown alike.
pub struct Inside {
    pub map: map::Map,
    pub links: Vec<(usize, usize)>,
    pub lens: Lens,
    pub chosen: bool,
}

impl MapView {
    pub fn map(&self) -> &map::Map {
        &self.kept.current
    }

    /// The theme gone into, if any.
    pub fn within(&self) -> Option<&Inside> {
        self.within.get(&self.inside?)
    }

    fn within_mut(&mut self) -> Option<&mut Inside> {
        self.within.get_mut(&self.inside?)
    }

    /// The map shown: the theme gone into's, else the whole; as a mosaic,
    /// where it is one.
    pub fn shown(&self) -> &map::Map {
        self.current().map_or(self.laid_out(), |t| &t.mosaic.map)
    }

    /// The blocks of the mosaic shown, if one is.
    pub fn blocks(&self) -> Option<&map::Blocks> {
        self.current().map(|t| &t.mosaic.blocks)
    }

    /// The mosaic last made, if one is shown and it is of the map shown.
    fn current(&self) -> Option<&Tiled> {
        self.tiled
            .as_ref()
            .filter(|t| self.mosaic && t.of(self.laid_out(), self.inside))
    }

    /// The map shown, as laid out.
    fn laid_out(&self) -> &map::Map {
        self.within().map_or(self.map(), |w| &w.map)
    }

    /// The mosaic, where one is shown, made for a canvas as large as
    /// `area` if the one last made is not: a paper to a half of a cell,
    /// as square as a terminal's cells halved are.
    pub fn tile(&mut self, area: Rect) {
        if !self.mosaic {
            self.tiled = None;
            return;
        }
        let size = (area.width, area.height);
        if self.current().is_some_and(|t| t.size == size) {
            return;
        }
        let plain = self.laid_out();
        let aspect = f32::from(size.0.max(1)) / (2.0 * f32::from(size.1.max(1)));
        self.tiled = Some(Tiled {
            mosaic: plain.mosaic(aspect),
            inside: self.inside,
            size,
            from: plain.xy.clone(),
        });
    }

    /// What is picked on the map shown.
    pub fn top(&self) -> &Lens {
        self.within().map_or(&self.lens, |w| &w.lens)
    }

    fn top_mut(&mut self) -> &mut Lens {
        match self.inside {
            Some(t) if self.within.contains_key(&t) => {
                &mut self.within.get_mut(&t).expect("gone into").lens
            }
            _ => &mut self.lens,
        }
    }

    /// The papers in view: within a theme, its finer theme chosen, else
    /// the theme; on the whole map, the theme picked. As the map holding
    /// them, and the theme on it.
    pub fn group(&self) -> (&map::Map, usize) {
        match self.within() {
            Some(w) if w.chosen => (&w.map, w.lens.picked),
            _ => (self.map(), self.lens.picked),
        }
    }

    /// The part of the plane shown, `[x0, x1, y0, y1]`: the papers', and a
    /// little more; closer to the theme picked, as zoomed.
    pub fn bounds(&self) -> [f64; 4] {
        let (whole, closer) = self.view();
        let Some((c, w, h)) = closer else {
            return whole;
        };
        let c = Self::moved(whole, c, w, h, self.panned());
        [
            c[0] - w / 2.0,
            c[0] + w / 2.0,
            c[1] - h / 2.0,
            c[1] + h / 2.0,
        ]
    }

    /// The part of the plane of the whole map shown, and a little more;
    /// and, shown closer, the middle of what it is closer to, and how wide
    /// and high what is shown is.
    fn view(&self) -> ([f64; 4], Option<Closer>) {
        let [x0, x1, y0, y1] = self.shown().bounds();
        let (px, py) = ((x1 - x0) * 0.04, (y1 - y0) * 0.04);
        let whole = [x0 - px, x1 + px, y0 - py, y1 + py];
        let [x0, x1, y0, y1] = whole;
        let lens = self.top();
        if lens.zoom == 0 {
            return (whole, None);
        }
        let k = f64::from(1u32 << lens.zoom);
        let c = self
            .closer_to()
            .and_then(|t| self.shown().themes.get(t))
            .map_or([(x0 + x1) / 2.0, (y0 + y1) / 2.0], |t| {
                [f64::from(t.centre[0]), f64::from(t.centre[1])]
            });
        (whole, Some((c, (x1 - x0) / k, (y1 - y0) / k)))
    }

    /// What the map is shown closer to: the theme picked; within a theme
    /// with no finer one chosen, its middle.
    fn closer_to(&self) -> Option<usize> {
        let middle = self.within().is_some_and(|w| !w.chosen);
        (!middle).then_some(self.top().picked)
    }

    /// How far the map shown closer is moved, if for what it is closer to.
    fn panned(&self) -> [f64; 2] {
        match self.top().pan {
            Some((to, by)) if to == self.closer_to().unwrap_or(usize::MAX) => by,
            _ => [0.0, 0.0],
        }
    }

    /// The middle `c` of what is shown, `w` by `h`, moved `by` of it, as far
    /// as the edge of the `whole` map.
    fn moved([x0, x1, y0, y1]: [f64; 4], c: [f64; 2], w: f64, h: f64, by: [f64; 2]) -> [f64; 2] {
        let clamp = |v: f64, lo: f64, hi: f64| {
            if lo < hi {
                v.clamp(lo, hi)
            } else {
                (lo + hi) / 2.0
            }
        };
        [
            clamp(c[0] + by[0] * w, x0 + w / 2.0, x1 - w / 2.0),
            clamp(c[1] + by[1] * h, y0 + h / 2.0, y1 - h / 2.0),
        ]
    }

    /// A theme picked that the map names.
    pub(super) fn settle(&mut self) {
        if self.lens.picked >= self.map().named() {
            self.lens.picked = 0;
        }
    }

    /// Whether the papers are those the map was last brought up to.
    pub(super) fn has(&self, keys: impl Iterator<Item = impl AsRef<str>>) -> bool {
        let mut n = 0;
        for k in keys {
            if !self.seen.contains(k.as_ref()) {
                return false;
            }
            n += 1;
        }
        n == self.seen.len()
    }
}

/// A theme's card, or a finer theme's: its name, words and authors, its
/// papers newest first, and the one picked; and whether it is a finer
/// theme's, of the theme gone into.
pub struct Card {
    pub theme: usize,
    pub finer: bool,
    pub name: String,
    pub about: String,
    pub words: Vec<String>,
    pub authors: Vec<(String, usize)>,
    pub papers: Vec<Hit>,
    /// Beside each paper, what is said of it, if anything.
    pub notes: Vec<String>,
    pub at: usize,
}

/// What the map's work gave: the maps, a theme to pick, what to say.
pub struct Mapped {
    pub kept: map::Kept,
    pub picked: Option<usize>,
    pub status: String,
}

/// What to say of `n` papers new since the map; nothing of none.
pub(super) fn new_since(n: usize) -> String {
    if n == 0 {
        return String::new();
    }
    format!(
        "{n} papers new since the map: A puts them in the themes said, R gathers the themes again"
    )
}

impl App {
    /// What the query finds in the whole library, for the map: found again
    /// only where the query, or the papers, changed since.
    pub(super) fn find(&mut self) {
        let query = self.query.trim().to_string();
        if self.found_for.as_ref() == Some(&query) {
            return;
        }
        self.found = (!query.is_empty()).then(|| {
            match self
                .index
                .search(&Query::parse(&query), Sort::Relevance, false, None)
            {
                Ok(hits) => hits.into_iter().map(|h| h.record.key.clone()).collect(),
                Err(e) => {
                    self.status = one_line(&e);
                    HashSet::new()
                }
            }
        });
        self.found_for = Some(query);
    }

    /// The card of the papers in view: the theme picked, or within it the
    /// finer theme chosen; its papers newest first.
    pub(super) fn show_card(&mut self) {
        let Some(m) = &self.map else { return };
        let finer = m.within().is_some_and(|w| w.chosen);
        let (map, t) = m.group();
        let Some(th) = map.themes.get(t) else { return };
        let mut papers: Vec<Hit> = map
            .members(t)
            .filter_map(|(_, k)| self.index.get(k).ok().flatten())
            .collect();
        papers.sort_by_cached_key(|h| std::cmp::Reverse(h.record.year()));
        let about = if finer {
            format!(
                "finer theme {} of {} · {} papers",
                t + 1,
                map.named(),
                th.size
            )
        } else {
            format!(
                "theme {} of {} · {} papers · resolution {:.2}",
                t + 1,
                map.named(),
                th.size,
                map.resolution
            )
        };
        self.popup = Popup::Card(Box::new(Card {
            theme: t,
            finer,
            name: th.name(),
            about,
            words: th.words.clone(),
            authors: th.authors.clone(),
            papers,
            notes: Vec::new(),
            at: 0,
        }));
    }

    /// The card of the papers new since the map: where each would go, and
    /// how many of its nearest are there.
    pub(super) fn show_new(&mut self) {
        let Some(m) = &self.map else { return };
        if m.kept.placed.is_empty() {
            self.status = "no papers new since the map".to_string();
            return;
        }
        let map = m.map();
        let (papers, notes): (Vec<Hit>, Vec<String>) = m
            .kept
            .placed
            .iter()
            .filter_map(|p| {
                let h = self.index.get(&p.key).ok().flatten()?;
                let to = map.themes.get(p.theme).map(map::Theme::name)?;
                Some((h, format!("→ {to} {:.0}%", p.share * 100.0)))
            })
            .unzip();
        self.popup = Popup::Card(Box::new(Card {
            theme: m.lens.picked,
            finer: true,
            name: "new since the map".to_string(),
            about: format!(
                "{} papers, each by its nearest on the map · A puts each there, u takes them back",
                papers.len()
            ),
            words: Vec::new(),
            authors: Vec::new(),
            papers,
            notes,
            at: 0,
        }));
    }

    /// The paper picked on the card, to open in ohp; the card stays.
    pub(super) fn open_card(&self) -> Option<Action> {
        let Popup::Card(card) = &self.popup else {
            return None;
        };
        let h = card.papers.get(card.at)?.clone();
        Some(Action::Open(Some(Box::new(h))))
    }

    /// On the card, the next theme's, or the one before's, of its map.
    pub(super) fn next_card(&mut self, by: isize) {
        let Popup::Card(card) = &self.popup else {
            return;
        };
        let finer = card.finer;
        let Some(m) = &mut self.map else { return };
        match m.within_mut() {
            Some(w) if finer => {
                let named = w.map.named().max(1);
                w.lens.picked = (w.lens.picked as isize + by).rem_euclid(named as isize) as usize;
            }
            _ => {
                let named = m.map().named().max(1);
                let t = (m.lens.picked as isize + by).rem_euclid(named as isize) as usize;
                self.pick_theme(t);
            }
        }
        self.show_card();
    }

    /// The map, if shown, else the papers; the map read, or built, when
    /// first shown, and the papers new since placed, to be asked of.
    pub(super) fn toggle_view(&mut self) {
        if self.view == View::Map {
            self.view = View::List;
            return;
        }
        // The theme whose papers are listed, picked.
        let listed = match &self.shelves[self.shelf].0 {
            Shelf::Theme(t, _) => Some(*t),
            _ => None,
        };
        match &mut self.map {
            Some(m) if m.has(self.index.keys()) => {
                let listed = listed.filter(|&t| t != m.lens.picked);
                self.view = View::Map;
                let new = m.kept.placed.len();
                if new > 0 {
                    self.status = new_since(new);
                }
                self.find();
                if let Some(t) = listed {
                    self.pick_theme(t);
                }
            }
            Some(_) => self.bring_up_map(listed),
            None => {
                let path = map::default_path(self.index.path());
                let docs = self.index.docs();
                self.map_work("the library's themes", move |say| {
                    let kept = map::Kept::open(&path, &docs, say)?;
                    let status = new_since(kept.placed.len());
                    Ok(Mapped {
                        kept,
                        picked: None,
                        status,
                    })
                });
            }
        }
    }

    /// The map's `job` done on a thread of its own, what it does said as it
    /// goes; the map shown with what it gave once done.
    pub(super) fn map_work(
        &mut self,
        what: &str,
        job: impl FnOnce(map::Say) -> anyhow::Result<Mapped> + Send + 'static,
    ) {
        self.spawn("Map", what.to_string(), Then::Map, move |say| {
            let mapped = job(&|s: &str| {
                say(&format!("{s}…"));
            })?;
            Ok(Done::Mapped(Box::new(mapped)))
        });
    }

    /// The maps kept, changed by `job` on a thread of its own, from the
    /// papers as they are; it gives a theme to pick, and what to say.
    pub(super) fn redo_map(
        &mut self,
        what: &str,
        job: impl FnOnce(
            &mut map::Kept,
            &[map::Doc],
            map::Say,
        ) -> anyhow::Result<(Option<usize>, String)>
        + Send
        + 'static,
    ) {
        let Some(m) = &self.map else { return };
        let mut kept = m.kept.clone();
        let docs = self.index.docs();
        self.map_work(what, move |say| {
            // Done but not kept, as where it cannot be written, it is shown
            // all the same, with why.
            let (picked, status) =
                job(&mut kept, &docs, say).unwrap_or_else(|e| (None, one_line(&e)));
            Ok(Mapped {
                kept,
                picked,
                status,
            })
        });
    }

    /// The map's work on its own thread, if any, waited for and its map
    /// shown: as `paperstack screen` drives the keys, and before quitting,
    /// so a map being written is not cut off.
    pub fn wait_for_map(&mut self) {
        while let Popup::Busy(busy) = &self.popup
            && matches!(busy.then, Then::Map | Then::Within)
        {
            std::thread::sleep(SPIN / 10);
            self.tick();
        }
    }

    /// The map's work that writes the map, if any, waited for: before
    /// quitting, so it is not cut off.
    pub(super) fn wait_for_writes(&mut self) {
        while let Popup::Busy(busy) = &self.popup
            && matches!(busy.then, Then::Map)
        {
            std::thread::sleep(SPIN / 10);
            self.tick();
        }
    }

    /// The map the work gave, shown, and what it said.
    pub(super) fn mapped(&mut self, mapped: Mapped) {
        let Mapped {
            kept,
            picked,
            status,
        } = mapped;
        // A finer theme's papers listed, of a map no longer.
        self.group = None;
        match &mut self.map {
            Some(m) => {
                m.kept = kept;
                m.within.clear();
                m.inside = None;
                m.lens.pan = None;
            }
            None => {
                self.map = Some(MapView {
                    kept,
                    lens: Lens::default(),
                    colours: false,
                    canvas: Rect::default(),
                    inside: None,
                    within: HashMap::new(),
                    seen: HashSet::new(),
                    mosaic: true,
                    tiled: None,
                    recent: None,
                    lately: LATELY_FIRST,
                })
            }
        }
        if let (Some(m), Some(p)) = (&mut self.map, picked) {
            m.lens.picked = p;
        }
        self.view = View::Map;
        self.status = status;
        self.map_settled();
    }

    /// The map brought up to the papers: those gone taken off it, those
    /// new placed, to be asked of; `picked`, if any, picked.
    pub(super) fn bring_up_map(&mut self, picked: Option<usize>) {
        self.redo_map(
            "the map, brought up to the papers",
            move |kept, docs, say| {
                say("placing the papers new since the map");
                kept.refresh(docs);
                Ok((picked, new_since(kept.placed.len())))
            },
        );
    }

    /// The map as brought up to the papers as they are now, a theme it
    /// names picked, and the shelves again.
    pub(super) fn map_settled(&mut self) {
        let Some(m) = &mut self.map else { return };
        m.seen = self.index.keys().map(str::to_string).collect();
        m.settle();
        self.reshelve();
    }

    /// Take `key` on the map; whether it was the map's.
    pub(super) fn map_key(&mut self, key: KeyEvent) -> bool {
        if key
            .modifiers
            .intersects(KeyModifiers::CONTROL | KeyModifiers::ALT)
        {
            return false;
        }
        // Few enough papers, in the list to read.
        if key.code == KeyCode::Enter && self.few() {
            self.read_few(None);
            return true;
        }
        let Some(m) = &self.map else { return false };
        let (inside, chosen) = (m.inside.is_some(), m.within().is_some_and(|w| w.chosen));
        let shift = key.modifiers.contains(KeyModifiers::SHIFT);
        match key.code {
            // Shown closer, moved; shift and an arrow.
            KeyCode::Right if shift => self.pan(1, 0),
            KeyCode::Left if shift => self.pan(-1, 0),
            KeyCode::Up if shift => self.pan(0, 1),
            KeyCode::Down if shift => self.pan(0, -1),
            KeyCode::Right | KeyCode::Char('l' | 'j') | KeyCode::Down | KeyCode::Tab => {
                self.next_theme(1)
            }
            KeyCode::Left | KeyCode::Char('h' | 'k') | KeyCode::Up | KeyCode::BackTab => {
                self.next_theme(-1)
            }
            // Within, the card of what is in view; without, into the theme.
            KeyCode::Enter if inside => self.show_card(),
            KeyCode::Enter => {
                let t = m.lens.picked;
                self.go_in(t);
            }
            KeyCode::Char('>') => self.zoom(1),
            KeyCode::Char('<') => self.zoom(-1),
            // Within, the finer theme chosen let go, then out.
            KeyCode::Esc if chosen => {
                if let Some(w) = self.map.as_mut().and_then(MapView::within_mut) {
                    w.chosen = false;
                    w.lens.paper = None;
                }
                self.status.clear();
            }
            KeyCode::Esc if inside => self.go_out(),
            KeyCode::Esc | KeyCode::Char('m') => self.view = View::List,
            KeyCode::Char('A') => self.assign(),
            KeyCode::Char('N') => self.show_new(),
            // Theme 1 to 9, as numbered on the map and by `paperstack themes`.
            KeyCode::Char(c @ '1'..='9') => self.pick_name(c as usize - '1' as usize, false),
            KeyCode::Char('n') => {
                let t = m.lens.picked;
                self.ask_name(t);
            }
            KeyCode::Char('L') => {
                let (map, t) = m.group();
                let keys = map.member_keys(t);
                let name = map.themes.get(t).map(map::Theme::name);
                self.ask_label(keys, name.unwrap_or_default());
            }
            KeyCode::Char('R') => self.rebuild(false),
            KeyCode::Char('F') => self.rebuild(true),
            KeyCode::Char('s') => self.split_theme(),
            KeyCode::Char('+' | '=') => self.regroup(1),
            KeyCode::Char('-') => self.regroup(-1),
            KeyCode::Char('u') => self.undo_map(),
            KeyCode::Char('g') => {
                if let Some(m) = &mut self.map {
                    m.mosaic = !m.mosaic;
                    self.status = if m.mosaic {
                        "the mosaic: each paper a cell of its own, each theme a block by those it lies by; g as laid out"
                    } else {
                        "the map as laid out: papers near where alike; g the mosaic"
                    }
                    .to_string();
                }
            }
            // Coloured by recent papers, first back to each theme's own.
            KeyCode::Char('c') => {
                if let Some(m) = &mut self.map
                    && m.recent.take().is_none()
                {
                    m.colours = !m.colours;
                }
            }
            KeyCode::Char('t') => {
                let was = m.lately;
                if self.map.as_mut().and_then(|m| m.recent.take()).is_some() {
                    self.status = "coloured as before".to_string();
                } else {
                    self.lately(was);
                }
            }
            // Recent, shorter or longer.
            KeyCode::Char('[' | ']') => {
                let by = if matches!(key.code, KeyCode::Char('[')) {
                    -1
                } else {
                    1
                };
                // Off, on again as it was.
                let lately = match m.recent {
                    Some(_) => m.lately.saturating_add_signed(by).min(LATELY.len() - 1),
                    None => m.lately,
                };
                self.lately(lately);
            }
            _ => return false,
        }
        true
    }

    /// Each theme coloured by how many of its papers came in the last of
    /// `LATELY` at `lately`.
    fn lately(&mut self, lately: usize) {
        let Some(m) = &mut self.map else { return };
        m.lately = lately;
        let keys = recent(&self.index, lately);
        self.status = format!(
            "{} papers added in the last {} · [ ] shorter, longer",
            keys.len(),
            LATELY[lately].1
        );
        let of = self.total();
        if let Some(m) = &mut self.map {
            m.recent = Some(Recent { keys, of });
        }
    }

    /// The papers that came lately read again, where the library has more
    /// or fewer since.
    pub(crate) fn stay_recent(&mut self) {
        let of = self.total();
        if let Some(m) = &mut self.map
            && let Some(r) = &mut m.recent
            && r.of != of
        {
            *r = Recent {
                keys: recent(&self.index, m.lately),
                of,
            };
        }
    }

    /// Pick the theme `by` on from the one picked, of those named; within a
    /// theme, into the next.
    pub(super) fn next_theme(&mut self, by: isize) {
        let Some(m) = &mut self.map else { return };
        let named = m.map().named().max(1);
        m.lens.picked = (m.lens.picked as isize + by).rem_euclid(named as isize) as usize;
        if m.inside.is_some() {
            let t = m.lens.picked;
            self.go_in(t);
        }
    }

    /// Into theme `t`: a map of its papers alone, in their own finer
    /// themes, with their links; made on a thread of its own the first time.
    pub(super) fn go_in(&mut self, t: usize) {
        let Some(m) = &mut self.map else { return };
        let Some(name) = m.map().themes.get(t).map(map::Theme::name) else {
            return;
        };
        m.lens.picked = t;
        if m.within.contains_key(&t) {
            m.inside = Some(t);
            self.status = format!(
                "within {name}: a click picks a finer theme, or a paper, twice opens it, or lists a few; ⏎ the card; esc out"
            );
            return;
        }
        let docs = self.index.docs();
        let members = m.map().theme_docs(&docs, t);
        let resolution = m.map().resolution;
        self.spawn(
            "Map",
            format!("{name}, from within"),
            Then::Within,
            move |say| {
                let (map, links) = map::Map::within(&docs, &members, resolution, &|s: &str| {
                    say(&format!("{s}…"));
                });
                let inside = Inside {
                    map,
                    links,
                    lens: Lens::default(),
                    chosen: false,
                };
                Ok(Done::Within(t, Box::new(inside)))
            },
        );
    }

    /// Out of the theme gone into, to the whole map.
    pub(super) fn go_out(&mut self) {
        if let Some(m) = &mut self.map {
            m.inside = None;
        }
        self.status.clear();
    }

    /// The papers in view on the map, few enough, in the list to read,
    /// the paper `pick` picked, if one is: a theme's on its shelf, a finer
    /// theme's there alone.
    pub(super) fn read_few(&mut self, pick: Option<&str>) {
        let Some(m) = &self.map else { return };
        let (map, t) = m.group();
        let name = map.themes.get(t).map(map::Theme::name).unwrap_or_default();
        let keys: HashSet<String> = map.member_keys(t).into_iter().collect();
        let n = keys.len();
        let finer = m.within().is_some_and(|w| w.chosen);
        self.show_papers(finer.then(|| (name.clone(), keys)), pick);
        self.status = format!("{n} papers of {name}, to read; ⏎ opens one");
    }

    /// Whether the papers in view on the map are few enough to read.
    pub(super) fn few(&self) -> bool {
        self.map.as_ref().is_some_and(|m| {
            let (map, t) = m.group();
            (1..self.to_read).contains(&map.members(t).count())
        })
    }

    /// The papers of the theme picked on the whole map in the list, on its
    /// shelf: all of them, or a finer theme's, by its name, alone; the
    /// search let go, so none is left out; `pick` picked, else the first.
    pub(super) fn show_papers(
        &mut self,
        finer: Option<(String, HashSet<String>)>,
        pick: Option<&str>,
    ) {
        let Some(m) = &self.map else { return };
        let theme = m.lens.picked;
        if let Some(i) = self
            .shelves
            .iter()
            .position(|(s, _)| matches!(s, Shelf::Theme(t, _) if *t == theme))
        {
            self.shelf = i;
        }
        self.group = finer.map(|(name, keys)| crate::app::Group { theme, name, keys });
        self.query.clear();
        self.searching = false;
        self.view = View::List;
        self.focus = Focus::List;
        self.narrow();
        let at = pick.and_then(|k| self.shown.iter().position(|h| h.record.key == k));
        self.list
            .select((!self.shown.is_empty()).then_some(at.unwrap_or(0)));
    }

    /// A click on the map: the paper nearest it picked, and its theme; the
    /// same again, into the theme, or within one, the paper opened.
    pub(super) fn map_click(&mut self, column: u16, row: u16) -> Option<Action> {
        self.searching = false;
        let m = self.map.as_mut()?;
        let (r, [x0, x1, y0, y1]) = (m.canvas, m.bounds());
        if r.width == 0 || r.height == 0 {
            return None;
        }
        let x = x0 + (f64::from(column - r.x) + 0.5) / f64::from(r.width) * (x1 - x0);
        let y = y1 - (f64::from(row - r.y) + 0.5) / f64::from(r.height) * (y1 - y0);
        let inside = m.inside.is_some();
        let shown = m.shown();
        // On the whole map, a paper of a theme named; within, any.
        let named = shown.named();
        let at = [x as f32, y as f32];
        // As a mosaic, a paper of the block clicked, if one is.
        let block = m.blocks().map(|b| b.at(at));
        let i = shown.nearest_where(at, |t| match block {
            Some(b) => b == Some(t) && (inside || t < named),
            None => inside || t < named,
        })?;
        let t = shown.theme[i]?;
        let key = shown.keys[i].clone();
        let lens = m.top_mut();
        lens.paper = Some(i);
        lens.picked = t;
        if let Some(w) = m.within_mut() {
            w.chosen = true;
        }
        let again = self.twice(Clicked::Dot(i));
        if !inside {
            if again && self.few() {
                self.read_few(Some(&key));
            } else if again {
                self.go_in(t);
            }
            return None;
        }
        let hit = self.index.get(&key).ok().flatten()?;
        // Twice, among few enough, in the list with them, picked, to read;
        // else opened.
        if again && self.few() {
            self.read_few(Some(&key));
            return None;
        }
        if again {
            return Some(Action::Open(Some(Box::new(hit))));
        }
        let year = hit
            .record
            .year()
            .map_or_else(|| "n.d.".to_string(), |y| y.to_string());
        let twice = if self.few() {
            "twice, with its theme in the list"
        } else {
            "twice opens it"
        };
        self.status = format!("{year} {} · {twice}", hit.record.title());
        None
    }

    /// Theme `t` of the map shown picked, by its name or its number; its
    /// name `clicked` twice, into it, or within one, its card.
    pub(super) fn pick_name(&mut self, t: usize, clicked: bool) {
        let Some(m) = &mut self.map else { return };
        let shown = m.shown();
        let Some(th) = shown.themes.get(t).filter(|_| t < shown.named()) else {
            return;
        };
        let said = format!("{} · {} papers", th.name(), th.size);
        let inside = m.inside.is_some();
        let lens = m.top_mut();
        lens.picked = t;
        lens.paper = None;
        if let Some(w) = m.within_mut() {
            w.chosen = true;
        }
        if inside {
            let then = if self.few() {
                "to read in the list"
            } else {
                "its card"
            };
            self.status = format!("{said}; ⏎ {then}");
        }
        if !clicked || !self.twice(Clicked::Name(t)) {
            return;
        }
        if self.few() {
            self.read_few(None);
            return;
        }
        if inside {
            self.show_card();
        } else {
            self.go_in(t);
        }
    }

    /// Theme `t` of the whole map picked; within a theme, into it.
    pub(super) fn pick_theme(&mut self, t: usize) {
        let Some(m) = &mut self.map else { return };
        if m.lens.picked == t {
            return;
        }
        m.lens.picked = t;
        if m.inside.is_some() {
            self.go_in(t);
        }
    }

    /// The wheel on the map: the next theme, or within one, the next finer
    /// theme chosen.
    pub(super) fn wheel_theme(&mut self, by: isize) {
        let Some(m) = &self.map else { return };
        match m.within() {
            Some(w) => {
                let named = w.map.named().max(1) as isize;
                // None chosen yet, the first, or the last.
                let from = match (w.chosen, by > 0) {
                    (true, _) => w.lens.picked as isize,
                    (false, true) => -1,
                    (false, false) => 0,
                };
                let t = (from + by).rem_euclid(named) as usize;
                self.pick_name(t, false);
            }
            None => self.next_theme(by),
        }
    }

    /// The map shown closer moved a quarter of what is shown, `dx` across
    /// and `dy` up, no further than its edge; the whole map, shown closer
    /// first.
    pub(super) fn pan(&mut self, dx: i32, dy: i32) {
        if self.map.as_ref().is_some_and(|m| m.top().zoom == 0) {
            self.zoom(1);
        }
        let Some(m) = &mut self.map else { return };
        let (whole, Some((c, w, h))) = m.view() else {
            return;
        };
        // Kept as far as it is moved, the edge reached, so back is at once.
        let by = m.panned();
        let by = [by[0] + 0.25 * f64::from(dx), by[1] + 0.25 * f64::from(dy)];
        let at = MapView::moved(whole, c, w, h, by);
        let to = m.closer_to().unwrap_or(usize::MAX);
        m.top_mut().pan = Some((to, [(at[0] - c[0]) / w, (at[1] - c[1]) / h]));
    }

    /// The map `by` steps closer to the theme picked, or further, from
    /// all of it to an eighth of it.
    pub(super) fn zoom(&mut self, by: i32) {
        let Some(m) = &mut self.map else { return };
        let lens = m.top_mut();
        let zoom = (lens.zoom + by).clamp(0, ZOOMS);
        // Closer or further, about what it is closer to again.
        if zoom != lens.zoom {
            lens.pan = None;
        }
        lens.zoom = zoom;
        // What it is closer to: the theme picked, or within one with none
        // chosen, its middle.
        let to = match m.within() {
            Some(w) if !w.chosen => "its middle".to_string(),
            _ => m
                .group()
                .0
                .themes
                .get(m.top().picked)
                .map_or_else(|| "its middle".to_string(), map::Theme::name),
        };
        self.status = if zoom == 0 {
            "the whole map".to_string()
        } else {
            format!("{}× closer to {to}; < further", 1 << zoom)
        };
    }

    /// Put the papers new since the map in the themes said, and keep it.
    pub(super) fn assign(&mut self) {
        let Some(m) = &mut self.map else { return };
        if m.kept.placed.is_empty() {
            self.status = "no papers new since the map".to_string();
            return;
        }
        self.status = match m.kept.assign() {
            Ok(n) => format!("{n} papers put in their themes; u takes them back"),
            Err(e) => one_line(&e),
        };
        // Its themes may be numbered otherwise now: each seen from within anew.
        m.within.clear();
        m.inside = None;
        self.reshelve();
    }

    /// Gather the themes again from all papers, at the map's resolution,
    /// the map till now kept as the previous: laid out from where the
    /// papers are, so the map keeps its shape, or, `fresh`, anew.
    pub(super) fn rebuild(&mut self, fresh: bool) {
        let what = if fresh {
            "all papers' themes, gathered and laid out anew"
        } else {
            "all papers' themes, gathered again"
        };
        self.redo_map(what, move |kept, docs, say| {
            let started = Instant::now();
            kept.rebuild(docs, None, fresh, say)?;
            let status = format!(
                "{} themes gathered at resolution {:.2} in {:.1}s; u shows the previous again",
                kept.current.named(),
                kept.current.resolution,
                started.elapsed().as_secs_f32()
            );
            Ok((None, status))
        });
    }

    /// Gather the themes again a step finer, `by` 1, or coarser, -1, in
    /// steps of half again, the papers where they are.
    pub(super) fn regroup(&mut self, by: i32) {
        let Some(m) = &self.map else { return };
        let now = m.map().resolution;
        // The next step on, from a resolution on one or between two.
        let at = now.ln() / STEP.ln();
        let step = if by > 0 {
            (at + 1e-6).floor() as i32 + 1
        } else {
            (at - 1e-6).ceil() as i32 - 1
        };
        let r = STEP.powi(step.clamp(-STEPS, STEPS));
        if (r - now) * f64::from(by) <= 1e-9 {
            let most = if by > 0 { "finest" } else { "coarsest" };
            self.status = format!("the themes are at their {most}, resolution {now:.2}");
            return;
        }
        self.redo_map("the themes, gathered again", move |kept, docs, say| {
            kept.regroup(docs, r, say)?;
            let status = format!(
                "{} themes at resolution {r:.2}; u shows the previous again",
                kept.current.named()
            );
            Ok((None, status))
        });
    }

    /// The theme picked split in finer themes of its own; the largest of
    /// them picked.
    pub(super) fn split_theme(&mut self) {
        let Some(m) = &self.map else { return };
        let t = m.lens.picked;
        let Some(name) = m.map().themes.get(t).map(map::Theme::name) else {
            return;
        };
        self.redo_map(&format!("{name}, split"), move |kept, docs, say| {
            let picked = kept.split(docs, t, map::RESOLUTION, say)?;
            let status = match picked {
                Some(_) => format!("{name} split in finer themes; u shows it whole again"),
                None => format!("{name} does not split further"),
            };
            Ok((picked, status))
        });
    }

    /// Ask for theme `t`'s name, its own or its words to start from.
    pub(super) fn ask_name(&mut self, t: usize) {
        let Some(name) = self
            .map
            .as_ref()
            .and_then(|m| m.map().themes.get(t))
            .map(map::Theme::name)
        else {
            return;
        };
        self.popup = Popup::Ask(Box::new(Ask {
            what: Asked::Name(t),
            line: name,
        }));
    }

    /// The previous map shown again, the one till now kept as its previous.
    pub(super) fn undo_map(&mut self) {
        self.redo_map("the map before", |kept, docs, say| {
            say("placing the papers new since the map");
            let status = if kept.undo(docs)? {
                format!(
                    "the map of {} shown again; u for the other",
                    kept.current.built
                )
            } else {
                "no previous map kept".to_string()
            };
            Ok((None, status))
        });
    }
}

#[cfg(test)]
mod tests {
    use crate::app::{App, Popup};
    use crate::tests::{click, library, screen};

    #[test]
    fn toggles_the_map_and_shows_a_themes_papers() {
        use crate::app::View;
        use ratatui::crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
        let (dir, lib, index) = library();
        let mut app = App::new(lib, index).unwrap();
        // The demo library's themes are each few: into them, not to the list.
        app.to_read = 0;
        app.key(KeyEvent::new(KeyCode::Char('m'), KeyModifiers::NONE));
        // Built on a thread of its own, the popup saying how it goes.
        assert!(matches!(app.popup, Popup::Busy(_)));
        app.wait_for_map();
        assert_eq!(app.view, crate::app::View::Map);
        // Kept beside the index, not in the user's cache.
        assert!(dir.path().join("map.json").is_file());
        let m = app.map.as_ref().unwrap();
        let picked = m.lens.picked;
        let size = m.map().themes[picked].size;
        let text = screen(&mut app, 160, 44);
        assert!(text.contains("THEMES"), "{text}");
        // Into the theme, its card, and its papers in the list.
        app.key(KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE));
        app.wait_for_map();
        assert_eq!(app.map.as_ref().unwrap().inside, Some(picked));
        app.key(KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE));
        app.key(KeyEvent::new(KeyCode::Char('l'), KeyModifiers::NONE));
        assert_eq!(app.view, View::List);
        assert_eq!(app.shown.len(), size);
        let map = app.map.as_ref().unwrap().map();
        let of: Vec<&str> = map.members(picked).map(|(_, k)| k).collect();
        assert!(
            app.shown
                .iter()
                .all(|h| of.contains(&h.record.key.as_str()))
        );
        // Typed into the search, ⌘G, or super+g, still toggles.
        app.key(KeyEvent::new(KeyCode::Char('/'), KeyModifiers::NONE));
        assert_eq!(app.view, View::List);
        app.key(KeyEvent::new(KeyCode::Char('g'), KeyModifiers::SUPER));
        assert_eq!(app.view, crate::app::View::Map);
        app.key(KeyEvent::new(KeyCode::Char('m'), KeyModifiers::NONE));
        assert_eq!(app.view, View::List);
        // The button top left, clicked, does too: ◆ Map, then ▤ List, after
        // the ▤ of paperstack's name.
        click(&mut app, "◆", 0, true);
        assert_eq!(app.view, crate::app::View::Map);
        click(&mut app, "▤", 1, true);
        assert_eq!(app.view, View::List);
    }

    #[test]
    fn gathers_the_themes_finer_and_back() {
        use ratatui::crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
        let (_dir, lib, index) = library();
        let mut app = App::new(lib, index).unwrap();
        let key = |c| KeyEvent::new(KeyCode::Char(c), KeyModifiers::NONE);
        app.key(KeyEvent::new(KeyCode::Char('m'), KeyModifiers::NONE));
        app.wait_for_map();
        let resolution = |app: &App| app.map.as_ref().unwrap().map().resolution;
        assert_eq!(resolution(&app), 1.0);
        app.key(key('+'));
        app.wait_for_map();
        assert_eq!(resolution(&app), 1.5);
        // Not starred, as `s` would a paper in the list: the theme is split,
        // and its largest part picked, or it is said that it does not split.
        let themes = |app: &App| app.map.as_ref().unwrap().map().themes.len();
        let before = themes(&app);
        app.key(key('s'));
        app.wait_for_map();
        let m = app.map.as_ref().unwrap();
        if app.status.contains("does not split") {
            assert_eq!(themes(&app), before);
        } else {
            assert!(themes(&app) > before, "{}", app.status);
            assert!(m.lens.picked < m.map().named());
        }
        assert_eq!(resolution(&app), 1.5);
        app.key(key('-'));
        app.wait_for_map();
        assert_eq!(resolution(&app), 1.0);
    }

    #[test]
    fn names_a_theme_and_labels_its_papers() {
        use crate::app::{Popup, Shelf, View};
        use ratatui::crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
        let (_dir, lib, index) = library();
        let mut app = App::new(lib, index).unwrap();
        let press = |app: &mut App, code| {
            app.key(KeyEvent::new(code, KeyModifiers::NONE));
        };
        let typed = |app: &mut App, s: &str| {
            app.key(KeyEvent::new(KeyCode::Char('u'), KeyModifiers::CONTROL));
            for c in s.chars() {
                app.key(KeyEvent::new(KeyCode::Char(c), KeyModifiers::NONE));
            }
        };
        app.key(KeyEvent::new(KeyCode::Char('m'), KeyModifiers::NONE));
        app.wait_for_map();
        let picked = app.map.as_ref().unwrap().lens.picked;
        // n names the picked theme; its shelf says so.
        press(&mut app, KeyCode::Char('n'));
        assert!(matches!(app.popup, Popup::Ask(_)));
        typed(&mut app, "perturbation");
        press(&mut app, KeyCode::Enter);
        assert_eq!(
            app.map.as_ref().unwrap().map().themes[picked].name(),
            "perturbation"
        );
        assert!(
            app.shelves.iter().any(
                |(s, _)| matches!(s, Shelf::Theme(t, n) if *t == picked && n == "perturbation")
            )
        );
        // L labels its papers, in their records.
        let size = app.map.as_ref().unwrap().map().themes[picked].size;
        press(&mut app, KeyCode::Char('L'));
        typed(&mut app, "theme-perturbation");
        press(&mut app, KeyCode::Enter);
        assert!(
            app.status.starts_with(&format!("{size} papers labelled")),
            "{}",
            app.status
        );
        assert!(
            app.shelves.iter().any(
                |(s, n)| matches!(s, Shelf::Label(l) if l == "theme-perturbation") && *n == size
            )
        );
        // a adds a paper, as in the list; / searches, on the map.
        press(&mut app, KeyCode::Char('a'));
        assert!(matches!(app.popup, Popup::Add(_)));
        press(&mut app, KeyCode::Esc);
        press(&mut app, KeyCode::Char('/'));
        assert_eq!(app.view, View::Map);
        assert!(app.searching);
    }

    #[test]
    fn picks_a_theme_by_its_number_and_asks_a_label_for_many_typed_out() {
        use crate::app::{Ask, Asked, Popup};
        use ratatui::crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
        let (_dir, lib, index) = library();
        let mut app = App::new(lib, index).unwrap();
        app.key(KeyEvent::new(KeyCode::Char('m'), KeyModifiers::NONE));
        app.wait_for_map();
        app.key(KeyEvent::new(KeyCode::Char('2'), KeyModifiers::NONE));
        assert_eq!(app.map.as_ref().unwrap().lens.picked, 1);
        app.key(KeyEvent::new(KeyCode::Char('9'), KeyModifiers::NONE));
        let m = app.map.as_ref().unwrap();
        assert!(
            m.lens.picked < m.map().named(),
            "no theme 9: picked as it was"
        );
        app.popup = Popup::Ask(Box::new(Ask {
            what: Asked::Label(vec!["x".to_string(); crate::app::MANY_LABELLED]),
            line: String::new(),
        }));
        let s = screen(&mut app, 160, 44);
        assert!(s.contains("so many papers"), "{s}");
    }

    #[test]
    fn shows_what_a_search_finds_on_the_map_and_takes_clicks_on_its_keys() {
        use crate::app::{Popup, Target};
        use ratatui::crossterm::event::{
            KeyCode, KeyEvent, KeyModifiers, MouseButton, MouseEvent, MouseEventKind,
        };
        let (_dir, lib, index) = library();
        let mut app = App::new(lib, index).unwrap();
        // The demo library's themes are each few: into them, not to the list.
        app.to_read = 0;
        let press = |app: &mut App, c| {
            app.key(KeyEvent::new(KeyCode::Char(c), KeyModifiers::NONE));
        };
        // Searched in the list, then shown on the map, and said beside it.
        press(&mut app, '/');
        for c in "perturbation".chars() {
            press(&mut app, c);
        }
        app.key(KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE));
        // Found for the map only once it is shown, not as the search is typed.
        assert!(app.found.is_none());
        press(&mut app, 'm');
        app.wait_for_map();
        let found = app.found.clone().expect("what the search finds");
        assert!(!found.is_empty());
        let s = screen(&mut app, 160, 44);
        assert!(
            s.contains(&format!(
                "{} found for “perturbation” in the library",
                found.len()
            )),
            "{s}"
        );
        // With no room beside the map, said above it.
        let s = screen(&mut app, 110, 40);
        assert!(
            s.contains("found for") && s.contains("/ then esc clears it"),
            "{s}"
        );
        // Searched on the map, it stays, what is found lit as it is typed;
        // cleared there, nothing is.
        press(&mut app, '/');
        assert_eq!(app.view, crate::app::View::Map);
        app.key(KeyEvent::new(KeyCode::Backspace, KeyModifiers::NONE));
        assert!(app.found.is_some(), "lit as it is typed");
        app.key(KeyEvent::new(KeyCode::Esc, KeyModifiers::NONE));
        assert_eq!(app.view, crate::app::View::Map);
        assert!(app.found.is_none());
        // A theme's name, clicked, picks it; twice, its card.
        let click_on = |app: &mut App, what: &dyn Fn(&Target) -> bool| {
            screen(app, 160, 44);
            let (at, _) = *app
                .targets
                .iter()
                .rev()
                .find(|(_, t)| what(t))
                .expect("drawn");
            app.mouse(MouseEvent {
                kind: MouseEventKind::Down(MouseButton::Left),
                column: at.x + 1,
                row: at.y,
                modifiers: KeyModifiers::NONE,
            })
        };
        click_on(&mut app, &|t| matches!(t, Target::Label(1)));
        assert_eq!(app.map.as_ref().unwrap().lens.picked, 1);
        click_on(&mut app, &|t| matches!(t, Target::Label(1)));
        app.wait_for_map();
        assert_eq!(app.map.as_ref().unwrap().inside, Some(1), "into it");
        let s = screen(&mut app, 160, 44);
        assert!(s.contains("esc out"), "{s}");
        app.key(KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE));
        let Popup::Card(card) = &app.popup else {
            panic!("its card");
        };
        assert_eq!(card.theme, 1);
        assert!(!card.finer);
        let second = card.papers[1].record.key.clone();
        // On it, a paper clicked is picked; twice, it is to be opened.
        click_on(&mut app, &|t| matches!(t, Target::CardPaper(1)));
        let opened = click_on(&mut app, &|t| matches!(t, Target::CardPaper(1)));
        let Some(crate::app::Action::Open(Some(h))) = opened else {
            panic!("a paper to open");
        };
        assert_eq!(h.record.key, second);
        app.key(KeyEvent::new(KeyCode::Esc, KeyModifiers::NONE));
        assert!(matches!(app.popup, Popup::None));
        // Out of the theme, to the whole map.
        app.key(KeyEvent::new(KeyCode::Esc, KeyModifiers::NONE));
        assert_eq!(app.map.as_ref().unwrap().inside, None);
        // > closer; a symbol shifted is the keyboard's own, not taken for one.
        app.key(KeyEvent::new(KeyCode::Char('.'), KeyModifiers::SHIFT));
        assert_eq!(app.map.as_ref().unwrap().lens.zoom, 0);
        press(&mut app, '>');
        assert_eq!(app.map.as_ref().unwrap().lens.zoom, 1);
        press(&mut app, '<');
        assert_eq!(app.map.as_ref().unwrap().lens.zoom, 0);
        // A key in the footer, clicked, as if pressed: + finer, - coarser.
        let click = |app: &mut App, c| {
            screen(app, 160, 44);
            let (at, _) = *app
                .targets
                .iter()
                .find(|(_, t)| matches!(t, Target::Key(k) if k.code == KeyCode::Char(c)))
                .expect("the key in the footer");
            app.mouse(MouseEvent {
                kind: MouseEventKind::Down(MouseButton::Left),
                column: at.x + 1,
                row: at.y,
                modifiers: KeyModifiers::NONE,
            });
            app.wait_for_map();
        };
        let resolution = |app: &App| app.map.as_ref().unwrap().map().resolution;
        click(&mut app, '+');
        assert_eq!(resolution(&app), 1.5);
        click(&mut app, '-');
        assert_eq!(resolution(&app), 1.0);
    }

    #[test]
    fn picks_on_the_map_the_theme_picked_on_the_left() {
        use crate::app::Shelf;
        use ratatui::crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
        let (_dir, lib, index) = library();
        let mut app = App::new(lib, index).unwrap();
        app.key(KeyEvent::new(KeyCode::Char('m'), KeyModifiers::NONE));
        app.wait_for_map();
        let at = app
            .shelves
            .iter()
            .position(|(s, _)| matches!(s, Shelf::Theme(1, _)))
            .expect("theme 2 on the left");
        app.shelf = at;
        app.narrow();
        assert_eq!(app.map.as_ref().unwrap().lens.picked, 1);
    }

    #[test]
    fn picks_a_finer_theme_within_one_by_its_name() {
        use crate::app::{Popup, Target};
        use ratatui::crossterm::event::{
            KeyCode, KeyEvent, KeyModifiers, MouseButton, MouseEvent, MouseEventKind,
        };
        let (_dir, lib, index) = library();
        let mut app = App::new(lib, index).unwrap();
        // The demo library's themes are each few: into them, not to the list.
        app.to_read = 0;
        app.key(KeyEvent::new(KeyCode::Char('m'), KeyModifiers::NONE));
        app.wait_for_map();
        // Into the largest theme, of finer themes of its own.
        app.key(KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE));
        app.wait_for_map();
        let inner = |app: &App| app.map.as_ref().unwrap().within().unwrap().map.themes.len();
        assert!(inner(&app) >= 1);
        let name = |app: &mut App| {
            screen(app, 160, 44);
            let (at, _) = *app
                .targets
                .iter()
                .rev()
                .find(|(_, t)| matches!(t, Target::Label(0)))
                .expect("a finer theme named");
            app.mouse(MouseEvent {
                kind: MouseEventKind::Down(MouseButton::Left),
                column: at.x + 1,
                row: at.y,
                modifiers: KeyModifiers::NONE,
            })
        };
        // Its name clicked, the finer theme is chosen, said; twice, its card.
        name(&mut app);
        let w = app.map.as_ref().unwrap().within().unwrap();
        assert!(w.chosen && w.lens.picked == 0);
        assert!(app.status.contains("papers"), "{}", app.status);
        name(&mut app);
        let Popup::Card(card) = &app.popup else {
            panic!("the finer theme's card");
        };
        assert!(card.finer);
        // Esc closes it; again, lets the finer theme go; again, out.
        app.key(KeyEvent::new(KeyCode::Esc, KeyModifiers::NONE));
        app.key(KeyEvent::new(KeyCode::Esc, KeyModifiers::NONE));
        assert!(!app.map.as_ref().unwrap().within().unwrap().chosen);
        app.key(KeyEvent::new(KeyCode::Esc, KeyModifiers::NONE));
        assert_eq!(app.map.as_ref().unwrap().inside, None);
    }

    #[test]
    fn takes_keys_on_a_map_of_no_themes() {
        use ratatui::crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
        let (_dir, lib, index) = library();
        let mut app = App::new(lib, index).unwrap();
        app.key(KeyEvent::new(KeyCode::Char('m'), KeyModifiers::NONE));
        app.wait_for_map();
        // As a library of one paper, or of none alike, is gathered.
        let m = app.map.as_mut().unwrap();
        m.kept.current.themes.clear();
        m.kept.current.theme.iter_mut().for_each(|t| *t = None);
        for c in [
            KeyCode::Enter,
            KeyCode::Char('>'),
            KeyCode::Char('1'),
            KeyCode::Right,
        ] {
            app.key(KeyEvent::new(c, KeyModifiers::NONE));
        }
        app.wait_for_map();
        assert_eq!(app.map.as_ref().unwrap().inside, None);
    }

    #[test]
    fn keeps_within_the_theme_the_wheel_and_the_shelf_say() {
        use crate::app::Shelf;
        use ratatui::crossterm::event::{
            KeyCode, KeyEvent, KeyModifiers, MouseEvent, MouseEventKind,
        };
        let (_dir, lib, index) = library();
        let mut app = App::new(lib, index).unwrap();
        // The demo library's themes are each few: into them, not to the list.
        app.to_read = 0;
        app.key(KeyEvent::new(KeyCode::Char('m'), KeyModifiers::NONE));
        app.wait_for_map();
        app.key(KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE));
        app.wait_for_map();
        assert_eq!(app.map.as_ref().unwrap().inside, Some(0));
        // The wheel over the map chooses a finer theme, staying within.
        screen(&mut app, 160, 44);
        let m = app.map.as_ref().unwrap().canvas;
        app.mouse(MouseEvent {
            kind: MouseEventKind::ScrollDown,
            column: m.x + 1,
            row: m.y + 1,
            modifiers: KeyModifiers::NONE,
        });
        let m = app.map.as_ref().unwrap();
        assert_eq!(m.inside, Some(0));
        assert!(m.within().unwrap().chosen);
        // Another theme's shelf picked, into that theme.
        let at = app
            .shelves
            .iter()
            .position(|(s, _)| matches!(s, Shelf::Theme(1, _)))
            .expect("theme 2 on the left");
        app.shelf = at;
        app.narrow();
        app.wait_for_map();
        assert_eq!(app.map.as_ref().unwrap().inside, Some(1));
    }

    #[test]
    fn sends_a_few_papers_from_the_map_to_the_list_to_read() {
        use crate::app::{Shelf, View};
        use ratatui::crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
        let (_dir, lib, index) = library();
        let mut app = App::new(lib, index).unwrap();
        let press = |app: &mut App, code| {
            app.key(KeyEvent::new(code, KeyModifiers::NONE));
            app.wait_for_map();
        };
        press(&mut app, KeyCode::Char('m'));
        // A theme of fewer than ten, ⏎: its papers in the list, its shelf.
        let (t, size) = {
            let m = app.map.as_ref().unwrap();
            (m.lens.picked, m.map().members(m.lens.picked).count())
        };
        press(&mut app, KeyCode::Enter);
        assert_eq!(app.view, View::List);
        assert!(matches!(app.shelves[app.shelf].0, Shelf::Theme(s, _) if s == t));
        assert_eq!(app.shown.len(), size);
        // Within a theme, a finer one: its papers alone, on that shelf;
        // esc, all of the shelf again.
        press(&mut app, KeyCode::Char('m'));
        app.to_read = 0;
        press(&mut app, KeyCode::Enter);
        assert!(app.map.as_ref().unwrap().inside.is_some());
        press(&mut app, KeyCode::Char('1'));
        let finer = {
            let (map, t) = app.map.as_ref().unwrap().group();
            map.member_keys(t)
        };
        // Its card, l: the same, from the card.
        press(&mut app, KeyCode::Enter);
        assert!(matches!(&app.popup, crate::app::Popup::Card(c) if c.finer));
        // p, its PDF asked for: the card again once given up.
        press(&mut app, KeyCode::Char('p'));
        assert!(matches!(&app.popup, crate::app::Popup::Fetch(_)));
        press(&mut app, KeyCode::Esc);
        assert!(matches!(&app.popup, crate::app::Popup::Card(c) if c.finer));
        // Again, a PDF of it named: the card again, the paper with it.
        let title = match &app.popup {
            crate::app::Popup::Card(c) => c.papers[c.at].record.title().to_string(),
            _ => unreachable!(),
        };
        let pdf = _dir.path().join("got.pdf");
        std::fs::write(&pdf, crate::tests::first_page(&title)).unwrap();
        press(&mut app, KeyCode::Char('p'));
        for c in pdf.display().to_string().chars() {
            press(&mut app, KeyCode::Char(c));
        }
        press(&mut app, KeyCode::Enter);
        assert!(
            matches!(&app.popup, crate::app::Popup::Card(c) if c.papers[c.at].has_pdf),
            "{}",
            app.status
        );
        press(&mut app, KeyCode::Char('l'));
        assert_eq!(app.view, View::List);
        assert_eq!(app.shown.len(), finer.len());
        // The group let go, so ⏎ on the map is what sets it again.
        press(&mut app, KeyCode::Esc);
        assert!(app.group.is_none());
        press(&mut app, KeyCode::Char('m'));
        app.to_read = 10;
        press(&mut app, KeyCode::Enter);
        assert_eq!(app.view, View::List);
        let mut shown: Vec<String> = app.shown.iter().map(|h| h.record.key.clone()).collect();
        shown.sort();
        let mut finer = finer;
        finer.sort();
        assert_eq!(shown, finer);
        assert!(screen(&mut app, 160, 44).contains("esc all of it"));
        press(&mut app, KeyCode::Esc);
        assert!(app.group.is_none());
        assert_eq!(app.shown.len(), size);
        // Sent again, then the whole theme asked for, from its card: all
        // of it, the finer theme's let go.
        press(&mut app, KeyCode::Char('m'));
        press(&mut app, KeyCode::Enter);
        assert!(app.group.is_some());
        press(&mut app, KeyCode::Char('m'));
        press(&mut app, KeyCode::Esc);
        app.to_read = 0;
        press(&mut app, KeyCode::Enter);
        press(&mut app, KeyCode::Char('l'));
        assert!(app.group.is_none());
        assert_eq!(app.shown.len(), size);
        // A search let go when papers are sent, so all are listed.
        app.to_read = 10;
        press(&mut app, KeyCode::Char('m'));
        press(&mut app, KeyCode::Esc);
        press(&mut app, KeyCode::Char('/'));
        for c in "zzz".chars() {
            press(&mut app, KeyCode::Char(c));
        }
        press(&mut app, KeyCode::Enter);
        press(&mut app, KeyCode::Enter);
        assert_eq!(app.view, View::List);
        assert!(app.query.is_empty());
        assert_eq!(app.shown.len(), size);
    }

    #[test]
    fn colours_the_map_by_papers_added_lately_over_a_range_asked() {
        use ratatui::crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
        let (_dir, lib, index) = library();
        let mut app = App::new(lib, index).unwrap();
        let press = |app: &mut App, code| {
            app.key(KeyEvent::new(code, KeyModifiers::NONE));
            app.wait_for_map();
        };
        press(&mut app, KeyCode::Char('m'));
        let lately = |app: &App| {
            let m = app.map.as_ref().unwrap();
            m.recent.is_some().then_some(m.lately)
        };
        let before = crate::app::screen_svg(&mut app, 160, 44);
        press(&mut app, KeyCode::Char('t'));
        assert_eq!(lately(&app), Some(super::LATELY_FIRST));
        assert!(app.status.contains("6 months"), "{}", app.status);
        press(&mut app, KeyCode::Char(']'));
        assert_eq!(lately(&app), Some(3));
        for _ in 0..9 {
            press(&mut app, KeyCode::Char(']'));
        }
        assert_eq!(lately(&app), Some(super::LATELY.len() - 1));
        // Off and on again, as far back as it was.
        press(&mut app, KeyCode::Char('t'));
        press(&mut app, KeyCode::Char('t'));
        assert_eq!(lately(&app), Some(super::LATELY.len() - 1));
        for _ in 0..9 {
            press(&mut app, KeyCode::Char('['));
        }
        assert_eq!(lately(&app), Some(0));
        assert_ne!(crate::app::screen_svg(&mut app, 160, 44), before);
        press(&mut app, KeyCode::Char('t'));
        assert_eq!(lately(&app), None);
        // Its colours as before, what was said aside.
        app.status.clear();
        assert_eq!(crate::app::screen_svg(&mut app, 160, 44), before);
    }

    #[test]
    fn widens_the_pane_of_the_paper_picked_by_keys_and_by_its_edge_dragged() {
        use ratatui::crossterm::event::{
            KeyCode, KeyEvent, KeyModifiers, MouseButton, MouseEvent, MouseEventKind,
        };
        let (_dir, lib, index) = library();
        let mut app = App::new(lib, index).unwrap();
        screen(&mut app, 160, 44);
        let was = app.pane;
        app.key(KeyEvent::new(KeyCode::Char('<'), KeyModifiers::NONE));
        assert_eq!(app.pane, was + 4);
        app.key(KeyEvent::new(KeyCode::Char('>'), KeyModifiers::NONE));
        assert_eq!(app.pane, was);
        // Its edge caught, dragged ten columns left, let go.
        screen(&mut app, 160, 44);
        let (at, _) = *app
            .targets
            .iter()
            .find(|(_, t)| matches!(t, crate::app::Target::Edge(_)))
            .expect("the pane's edge");
        let mouse = |kind, column| MouseEvent {
            kind,
            column,
            row: at.y + 2,
            modifiers: KeyModifiers::NONE,
        };
        let edge = at.x + 1;
        app.mouse(mouse(MouseEventKind::Down(MouseButton::Left), edge));
        assert!(app.dragging.is_some());
        app.mouse(mouse(MouseEventKind::Drag(MouseButton::Left), edge - 10));
        app.mouse(mouse(MouseEventKind::Up(MouseButton::Left), edge - 10));
        assert!(app.dragging.is_none());
        assert_eq!(app.pane, was + 10);
        // No narrower than it may be, nor wider.
        for _ in 0..100 {
            app.key(KeyEvent::new(KeyCode::Char('>'), KeyModifiers::NONE));
        }
        assert_eq!(app.pane, crate::ui::NARROWEST);
        for _ in 0..100 {
            app.key(KeyEvent::new(KeyCode::Char('<'), KeyModifiers::NONE));
        }
        let widest = 160 - crate::ui::SIDEBAR - crate::ui::LIST_NARROWEST;
        assert_eq!(app.pane, widest);
        // A narrower screen for a while, the width asked kept.
        screen(&mut app, 140, 44);
        screen(&mut app, 160, 44);
        assert_eq!(app.pane, widest);
        // A drag whose release went unheard let go at the next click.
        app.mouse(mouse(MouseEventKind::Down(MouseButton::Left), edge));
        app.mouse(mouse(MouseEventKind::Down(MouseButton::Left), 5));
        assert!(app.dragging.is_none());
    }
}
