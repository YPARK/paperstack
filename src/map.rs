//! The library as a map: papers near one another where their words are
//! alike or their authors shared, gathered into themes, each named by the
//! words that set it apart, and laid out in two dimensions as UMAP lays out
//! a graph.
//!
//! Each paper's title (twice), abstract and keywords are its words, and
//! pairs of words beside each other, as "single cell", are words too. Their
//! TF-IDF vectors give each paper its nearest neighbours by cosine, weighed
//! as UMAP's fuzzy graph weighs them; papers sharing an author are linked
//! besides, but not through papers of a consortium's many authors. Themes
//! are the graph's communities by Louvain's moves (Blondel et al., J Stat
//! Mech 2008); the layout is UMAP's SGD on the same graph (McInnes, Healy &
//! Melville, arXiv 1802.03426), as legume-numeric's `matrix::umap` runs it.

use anyhow::Context;
use rayon::prelude::*;
use serde::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU32, Ordering};

/// A paper as the map reads it.
pub struct Doc {
    pub key: String,
    /// Its title's words, then its abstract's and its keywords'.
    pub title: Vec<String>,
    pub body: Vec<String>,
    /// Its authors, as `Family G`.
    pub authors: Vec<String>,
}

#[derive(Clone, Serialize, Deserialize)]
pub struct Map {
    /// When it was built, RFC 3339.
    pub built: String,
    /// Louvain's resolution its themes were gathered at.
    #[serde(default = "resolution")]
    pub resolution: f64,
    /// By paper: its key, its place, and its theme, by its place in
    /// `themes`; `None` for one with neither words nor links to place it by.
    pub keys: Vec<String>,
    pub xy: Vec<[f32; 2]>,
    pub theme: Vec<Option<usize>>,
    /// Largest first.
    pub themes: Vec<Theme>,
    /// The words most papers have, with how many have them.
    pub common: Vec<(String, usize)>,
}

#[derive(Clone, Default, Serialize, Deserialize)]
pub struct Theme {
    /// The words that set it apart, most first.
    pub words: Vec<String>,
    pub size: usize,
    /// Its authors of most papers in it, with how many.
    pub authors: Vec<(String, usize)>,
    /// The middle of its papers on the map.
    pub centre: [f32; 2],
    /// The name it was given, if one was, in place of its words.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub called: Option<String>,
}

fn resolution() -> f64 {
    RESOLUTION
}

impl Theme {
    /// The name it was given, else its first words.
    pub fn name(&self) -> String {
        self.called.clone().unwrap_or_else(|| self.words_name())
    }

    /// Its first words, as its name.
    pub fn words_name(&self) -> String {
        self.words
            .iter()
            .take(2)
            .cloned()
            .collect::<Vec<_>>()
            .join(" · ")
    }
}

/// Neighbours each paper is linked to by its words.
const KNN: usize = 15;
/// A paper's words, by weight, its neighbours are looked for by.
const QUERY_TERMS: usize = 40;
/// Papers of more authors than this are not linked by them.
const MAX_AUTHORS: usize = 50;
/// Papers each is linked to by shared authors, at most.
const COAUTHOR_KNN: usize = 10;
/// How much a shared author counts beside alike words.
const COAUTHOR_WEIGHT: f32 = 0.5;
/// Themes of fewer papers are left unnamed on the map.
pub const MIN_THEME: usize = 5;
/// Louvain's resolution, unless another is asked for.
pub const RESOLUTION: f64 = 1.0;

/// `r`, if a resolution themes can be gathered at.
fn checked(r: f64) -> anyhow::Result<f64> {
    anyhow::ensure!(
        r.is_finite() && r > 0.0,
        "the resolution is a number above 0, not {r}"
    );
    Ok(r)
}
const NAMED_AT_LEAST: usize = 8;
const WORDS_NAMED: usize = 6;
const AUTHORS_NAMED: usize = 6;

/// The papers' words and links, from which the themes are gathered.
struct Graph {
    vocab: Vec<String>,
    vectors: Vec<Vec<(u32, f32)>>,
    df: Vec<usize>,
    common: Vec<(String, usize)>,
    /// `(i, j, weight)`, each pair once, in one order each run.
    edges: Vec<(usize, usize, f32)>,
}

impl Graph {
    /// The links among the papers of `members`, by their places in it, each
    /// to its nearest of them; the words weighed over all `docs`.
    fn among(docs: &[Doc], members: &[usize]) -> Graph {
        let (vocab, vectors, df, common) = tfidf(docs);
        let mut local = vec![None; docs.len()];
        for (l, &d) in members.iter().enumerate() {
            local[d] = Some(l);
        }
        let near = knn(&vectors, vocab.len(), members, &|k| local[k].is_some());
        let mut edges = fuzzy(members, &near);
        blend(&mut edges, &coauthors(docs, &|d| local[d].is_some()));
        let mut edges: Vec<(usize, usize, f32)> = edges
            .into_iter()
            .filter_map(|((i, j), w)| Some((local[i]?, local[j]?, w)))
            .collect();
        // In one order each run, not the hash map's.
        edges.sort_unstable_by_key(|&(i, j, _)| (i, j));
        Graph {
            vocab,
            vectors,
            df,
            common,
            edges,
        }
    }

    /// Each theme's words and authors, as `theme` gathers `docs`, by doc.
    fn name(&self, themes: &mut [Theme], docs: &[Doc], theme: &[Option<usize>]) {
        name_themes(themes, docs, theme, &self.vectors, &self.vocab, &self.df);
    }
}

/// The communities of `edges` among `n` nodes at `resolution`, numbered
/// largest first; nodes linked to nothing in none.
fn communities(
    n: usize,
    edges: &[(usize, usize, f32)],
    resolution: f64,
) -> (Vec<Option<usize>>, usize) {
    let mut linked = vec![false; n];
    for &(i, j, _) in edges {
        linked[i] = true;
        linked[j] = true;
    }
    let community = louvain(n, edges, resolution);
    let mut sizes: HashMap<usize, usize> = HashMap::new();
    for i in (0..n).filter(|&i| linked[i]) {
        *sizes.entry(community[i]).or_default() += 1;
    }
    let mut order: Vec<(usize, usize)> = sizes.into_iter().collect();
    order.sort_by(|a, b| b.1.cmp(&a.1).then(a.0.cmp(&b.0)));
    let renumber: HashMap<usize, usize> = order
        .iter()
        .enumerate()
        .map(|(t, &(c, _))| (c, t))
        .collect();
    let theme = (0..n)
        .map(|i| linked[i].then(|| renumber[&community[i]]))
        .collect();
    (theme, order.len())
}

impl Map {
    /// The map of `docs`, its themes gathered at `resolution`: the higher,
    /// the more themes, and the smaller.
    pub fn build(docs: &[Doc], resolution: f64) -> Map {
        let all: Vec<usize> = (0..docs.len()).collect();
        Map::gather(docs, &all, resolution, |edges, theme, count| {
            layout(docs.len(), edges, theme, count)
        })
    }

    /// The papers on the map gathered in themes again at `resolution`,
    /// each left where it is: finer or coarser themes, in the same layout.
    pub fn regroup(&self, docs: &[Doc], resolution: f64) -> Map {
        let places = self.places(docs);
        let members: Vec<usize> = (0..docs.len()).filter(|&d| places[d].is_some()).collect();
        let xy = members
            .iter()
            .filter_map(|&d| Some(self.xy[places[d]?]))
            .collect();
        Map::gather(docs, &members, resolution, |_, _, _| xy)
    }

    /// The papers of `members` gathered in themes at `resolution`, and put
    /// on the plane by `place`, from the links and the themes.
    fn gather(
        docs: &[Doc],
        members: &[usize],
        resolution: f64,
        place: impl FnOnce(&[(usize, usize, f32)], &[Option<usize>], usize) -> Vec<[f32; 2]>,
    ) -> Map {
        let mut graph = Graph::among(docs, members);
        let (theme, count) = communities(members.len(), &graph.edges, resolution);
        let xy = place(&graph.edges, &theme, count);
        let mut map = Map {
            built: crate::add::now(),
            resolution,
            keys: members.iter().map(|&d| docs[d].key.clone()).collect(),
            xy,
            theme,
            themes: vec![Theme::default(); count],
            common: std::mem::take(&mut graph.common),
        };
        map.recount();
        let mut by_doc = vec![None; docs.len()];
        for (&d, &t) in members.iter().zip(&map.theme) {
            by_doc[d] = t;
        }
        graph.name(&mut map.themes, docs, &by_doc);
        map
    }

    /// Theme `t` split in its own themes, gathered at `resolution` among
    /// its papers alone, each linked to its nearest within it; the papers
    /// left where they are on the map, and the other themes as they were.
    /// The theme of the largest part now, or none where it does not split.
    pub fn split(&mut self, docs: &[Doc], t: usize, resolution: f64) -> Option<usize> {
        let on_map = self.places(docs);
        // The theme's papers, by doc; and each doc's place among them.
        let members: Vec<usize> = (0..docs.len())
            .filter(|&d| on_map[d].is_some_and(|m| self.theme[m] == Some(t)))
            .collect();
        let graph = Graph::among(docs, &members);
        let (part, count) = communities(members.len(), &graph.edges, resolution);
        if count < 2 {
            return None;
        }
        // Part 0, the largest, in `t`, with the papers linked to none in
        // it; the others in themes of their own. Their words are found
        // anew, the other themes' kept.
        let first = self.themes.len();
        self.themes.extend(vec![Theme::default(); count - 1]);
        self.themes[t].words.clear();
        let mut largest = None;
        for (l, &d) in members.iter().enumerate() {
            let m = on_map[d].expect("a member is on the map");
            match part[l] {
                Some(0) => largest = Some(m),
                Some(p) => self.theme[m] = Some(first + p - 1),
                None => {}
            }
        }
        self.recount();
        let by_doc: Vec<Option<usize>> = on_map
            .iter()
            .map(|m| m.and_then(|m| self.theme[m]))
            .collect();
        // Only those whose words were cleared, `t` and the new, named anew.
        let mut named = self.themes.clone();
        graph.name(&mut named, docs, &by_doc);
        for (theme, named) in self.themes.iter_mut().zip(named) {
            if theme.words.is_empty() {
                theme.words = named.words;
                theme.authors = named.authors;
            }
        }
        self.theme[largest?]
    }

    /// The keys of theme `t`'s papers.
    pub fn member_keys(&self, t: usize) -> Vec<String> {
        self.members(t).map(|(_, k)| k.to_string()).collect()
    }

    /// The names given to the themes of `from` on the themes here that
    /// share most of their papers: at least half of the two together. A
    /// theme gathered too otherwise loses its name.
    fn carry_names(&mut self, from: &Map) {
        let here: HashMap<&str, usize> = self
            .keys
            .iter()
            .zip(&self.theme)
            .filter_map(|(k, t)| Some((k.as_str(), (*t)?)))
            .collect();
        let mut given: Vec<(usize, String)> = Vec::new();
        for (f, theme) in from.themes.iter().enumerate() {
            let Some(called) = &theme.called else {
                continue;
            };
            let mut shared: HashMap<usize, usize> = HashMap::new();
            for (_, k) in from.members(f) {
                if let Some(&t) = here.get(k) {
                    *shared.entry(t).or_default() += 1;
                }
            }
            let best = shared
                .into_iter()
                .max_by_key(|&(t, n)| (n, std::cmp::Reverse(t)));
            if let Some((t, n)) = best {
                let union = theme.size + self.themes[t].size - n;
                if 2 * n >= union {
                    given.push((t, called.clone()));
                }
            }
        }
        for t in &mut self.themes {
            t.called = None;
        }
        for (t, called) in given {
            self.themes[t].called.get_or_insert(called);
        }
    }

    /// Each of `docs`' place on the map; none for one not on it.
    fn places(&self, docs: &[Doc]) -> Vec<Option<usize>> {
        let at: HashMap<&str, usize> = self
            .keys
            .iter()
            .enumerate()
            .map(|(i, k)| (k.as_str(), i))
            .collect();
        docs.iter()
            .map(|d| at.get(d.key.as_str()).copied())
            .collect()
    }

    /// Each theme's size and centre, from its papers.
    fn recount(&mut self) {
        for t in &mut self.themes {
            t.size = 0;
            t.centre = [0.0; 2];
        }
        for (p, t) in self.xy.iter().zip(&self.theme) {
            if let Some(t) = *t {
                let th = &mut self.themes[t];
                th.size += 1;
                th.centre[0] += p[0];
                th.centre[1] += p[1];
            }
        }
        for t in self.themes.iter_mut().filter(|t| t.size > 0) {
            t.centre[0] /= t.size as f32;
            t.centre[1] /= t.size as f32;
        }
        // Largest first still, as `named` counts them.
        let mut order: Vec<usize> = (0..self.themes.len()).collect();
        order.sort_by_key(|&t| std::cmp::Reverse(self.themes[t].size));
        if order.iter().enumerate().any(|(i, &t)| i != t) {
            let mut renumber = vec![0; order.len()];
            for (i, &t) in order.iter().enumerate() {
                renumber[t] = i;
            }
            let mut themes: Vec<Option<Theme>> = self.themes.drain(..).map(Some).collect();
            self.themes = order.iter().map(|&t| themes[t].take().unwrap()).collect();
            for t in self.theme.iter_mut().flatten() {
                *t = renumber[*t];
            }
        }
    }

    /// The papers of theme `t`, by their place on the map, and their keys.
    pub fn members(&self, t: usize) -> impl Iterator<Item = (usize, &str)> {
        self.keys
            .iter()
            .enumerate()
            .filter(move |&(i, _)| self.theme[i] == Some(t))
            .map(|(i, k)| (i, k.as_str()))
    }

    /// How many themes are named, and listed: those of `MIN_THEME` papers
    /// or more, but never fewer than the largest few, as a small library's.
    pub fn named(&self) -> usize {
        let big = self
            .themes
            .iter()
            .take_while(|t| t.size >= MIN_THEME)
            .count();
        big.max(self.themes.len().min(NAMED_AT_LEAST))
    }

    /// The paper of a named theme nearest `at` on the map.
    pub fn nearest(&self, at: [f32; 2]) -> Option<usize> {
        let d = |p: &[f32; 2]| (p[0] - at[0]).powi(2) + (p[1] - at[1]).powi(2);
        let named = self.named();
        (0..self.xy.len())
            .filter(|&i| self.theme[i].is_some_and(|t| t < named))
            .min_by(|&a, &b| d(&self.xy[a]).total_cmp(&d(&self.xy[b])))
    }

    /// The bounds of the papers placed, `[x0, x1, y0, y1]`.
    pub fn bounds(&self) -> [f64; 4] {
        let mut b = [f64::MAX, f64::MIN, f64::MAX, f64::MIN];
        for (p, t) in self.xy.iter().zip(&self.theme) {
            if t.is_some() {
                b[0] = b[0].min(p[0] as f64);
                b[1] = b[1].max(p[0] as f64);
                b[2] = b[2].min(p[1] as f64);
                b[3] = b[3].max(p[1] as f64);
            }
        }
        if b[0] > b[1] {
            return [-1.0, 1.0, -1.0, 1.0];
        }
        b
    }
}

/// A paper new since the map was built, and the theme its nearest papers
/// on the map have most: what share of them, by weight, and where among
/// them it would be put.
pub struct Placing {
    pub key: String,
    pub theme: usize,
    pub share: f32,
    pub xy: [f32; 2],
}

impl Map {
    /// Where each paper of `docs` not on the map would go, by its nearest
    /// on it in words and its co-authors; none for a paper near none.
    pub fn place(&self, docs: &[Doc]) -> Vec<Placing> {
        let places = self.places(docs);
        // Those in a theme, to place by.
        let on_map: Vec<Option<usize>> = places
            .iter()
            .map(|m| m.filter(|&m| self.theme[m].is_some()))
            .collect();
        let new: Vec<usize> = (0..docs.len()).filter(|&i| places[i].is_none()).collect();
        if new.is_empty() || on_map.iter().all(Option::is_none) {
            return Vec::new();
        }
        let (vocab, vectors, _, _) = tfidf(docs);
        let near = knn(&vectors, vocab.len(), &new, &|k| on_map[k].is_some());
        // Each new paper's co-authors on the map.
        let mut shared: HashMap<usize, Vec<(usize, f32)>> = HashMap::new();
        for ((a, b), x) in coauthors(docs, &|_| true) {
            for (i, j) in [(a, b), (b, a)] {
                if on_map[i].is_none() && on_map[j].is_some() {
                    shared.entry(i).or_default().push((j, x * COAUTHOR_WEIGHT));
                }
            }
        }
        new.iter()
            .zip(near)
            .filter_map(|(&i, nn)| {
                let mut w = weights(&nn);
                w.extend(shared.remove(&i).unwrap_or_default());
                let mut votes: HashMap<usize, f32> = HashMap::new();
                let (mut xy, mut total) = ([0.0f32; 2], 0.0f32);
                for &(j, x) in &w {
                    let m = on_map[j]?;
                    *votes.entry(self.theme[m]?).or_default() += x;
                    xy[0] += x * self.xy[m][0];
                    xy[1] += x * self.xy[m][1];
                    total += x;
                }
                let (&theme, &best) = votes
                    .iter()
                    .max_by(|a, b| a.1.total_cmp(b.1).then(b.0.cmp(a.0)))?;
                Some(Placing {
                    key: docs[i].key.clone(),
                    theme,
                    share: best / total,
                    xy: [xy[0] / total, xy[1] / total],
                })
            })
            .collect()
    }

    /// Put the papers placed on the map, in their themes.
    pub fn accept(&mut self, placed: &[Placing]) {
        for p in placed {
            self.keys.push(p.key.clone());
            self.xy.push(p.xy);
            self.theme.push(Some(p.theme));
        }
        self.recount();
    }

    /// Take off the map the papers no longer in the library.
    pub fn keep(&mut self, keys: &HashSet<&str>) {
        let mut i = 0;
        while i < self.keys.len() {
            if keys.contains(self.keys[i].as_str()) {
                i += 1;
                continue;
            }
            self.keys.swap_remove(i);
            self.xy.swap_remove(i);
            self.theme.swap_remove(i);
        }
        self.recount();
    }
}

/// The maps kept: the one shown, and the one before it was built again;
/// and where the papers new since the one shown was built would go, to be
/// asked of before they are put there.
#[derive(Serialize, Deserialize)]
pub struct Kept {
    pub current: Map,
    pub previous: Option<Map>,
    #[serde(skip)]
    pub placed: Vec<Placing>,
    #[serde(skip)]
    path: PathBuf,
}

impl Kept {
    /// The maps kept at `path`, as they were; none where there are none,
    /// or they cannot be read.
    pub fn read(path: &Path) -> Option<Kept> {
        let mut kept: Kept = serde_json::from_slice(&std::fs::read(path).ok()?).ok()?;
        kept.path = path.to_path_buf();
        Some(kept)
    }

    /// A map of `docs` built at `resolution`, and kept at `path`, with none
    /// before it.
    pub fn build(path: &Path, docs: &[Doc], resolution: f64) -> anyhow::Result<Kept> {
        let kept = Kept {
            current: Map::build(docs, checked(resolution)?),
            previous: None,
            placed: Vec::new(),
            path: path.to_path_buf(),
        };
        kept.save()?;
        Ok(kept)
    }

    /// The maps kept at `path`, brought up to `docs`; one built and kept
    /// where none was.
    pub fn open(path: &Path, docs: &[Doc]) -> anyhow::Result<Kept> {
        match Kept::read(path) {
            Some(mut kept) => {
                kept.refresh(docs);
                Ok(kept)
            }
            None => Kept::build(path, docs, RESOLUTION),
        }
    }

    fn save(&self) -> anyhow::Result<()> {
        crate::library::write(&self.path, &serde_json::to_vec(self)?)
    }

    /// The papers no longer in `docs` taken off the map shown, and those
    /// new since it was built placed.
    pub fn refresh(&mut self, docs: &[Doc]) {
        self.current.keep(&keys(docs));
        self.placed = self.current.place(docs);
    }

    /// Put the papers placed in their themes, and keep the map; how many.
    pub fn assign(&mut self) -> anyhow::Result<usize> {
        if self.placed.is_empty() {
            return Ok(0);
        }
        let placed = std::mem::take(&mut self.placed);
        self.previous = Some(self.current.clone());
        self.current.accept(&placed);
        self.save()?;
        Ok(placed.len())
    }

    /// Theme `t` of the map shown named `name`, kept; its own words again
    /// where `name` is blank.
    pub fn name(&mut self, t: usize, name: &str) -> anyhow::Result<()> {
        let theme = self.current.themes.get_mut(t).context("no such theme")?;
        // Its own words, as they were offered, are no name of its own.
        let name = name.trim();
        theme.called = (!name.is_empty() && name != theme.words_name()).then(|| name.to_string());
        self.save()
    }

    /// `map` shown, with the names given to the themes of the one shown
    /// till now, which is kept as the previous; new papers placed again.
    fn show(&mut self, mut map: Map, docs: &[Doc]) -> anyhow::Result<()> {
        map.carry_names(&self.current);
        self.previous = Some(std::mem::replace(&mut self.current, map));
        self.placed = self.current.place(docs);
        self.save()
    }

    /// A map of `docs` built and shown at `resolution`, else the one shown
    /// till now's, which is kept as the previous.
    pub fn rebuild(&mut self, docs: &[Doc], resolution: Option<f64>) -> anyhow::Result<()> {
        let resolution = checked(resolution.unwrap_or(self.current.resolution))?;
        self.show(Map::build(docs, resolution), docs)
    }

    /// The map shown, brought up to `docs` first, gathered in themes again
    /// at `resolution`, its layout as it was; the map as it was kept as
    /// the previous.
    pub fn regroup(&mut self, docs: &[Doc], resolution: f64) -> anyhow::Result<()> {
        let resolution = checked(resolution)?;
        self.current.keep(&keys(docs));
        let map = self.current.regroup(docs, resolution);
        self.show(map, docs)
    }

    /// Theme `t` of the map shown, brought up to `docs` first, split in
    /// finer themes, the map as it was kept as the previous; the theme of
    /// its largest part now, or none where it does not split.
    pub fn split(
        &mut self,
        docs: &[Doc],
        t: usize,
        resolution: f64,
    ) -> anyhow::Result<Option<usize>> {
        let resolution = checked(resolution)?;
        self.current.keep(&keys(docs));
        anyhow::ensure!(t < self.current.themes.len(), "no such theme");
        let mut map = self.current.clone();
        let Some(largest) = map.split(docs, t, resolution) else {
            return Ok(None);
        };
        self.show(map, docs)?;
        Ok(Some(largest))
    }

    /// The previous map shown again, brought up to `docs`, the one shown
    /// till now its previous; whether there was one.
    pub fn undo(&mut self, docs: &[Doc]) -> anyhow::Result<bool> {
        let Some(previous) = self.previous.as_mut() else {
            return Ok(false);
        };
        std::mem::swap(&mut self.current, previous);
        self.refresh(docs);
        self.save()?;
        Ok(true)
    }
}

fn keys(docs: &[Doc]) -> HashSet<&str> {
    docs.iter().map(|d| d.key.as_str()).collect()
}

/// Where the maps of the library whose index is at `index` are kept: beside
/// it, on this machine.
pub fn default_path(index: &Path) -> PathBuf {
    let name = index
        .file_name()
        .map_or_else(Default::default, |n| n.to_string_lossy().into_owned());
    let name = name.strip_prefix("index").unwrap_or(&name);
    index.with_file_name(format!("map{name}"))
}

/// Words that say nothing of what a paper is about.
#[rustfmt::skip]
const STOP: &[&str] = &[
    "a", "about", "above", "across", "after", "again", "against", "all", "almost", "along",
    "also", "although", "among", "an", "and", "another", "any", "are", "as", "at", "be",
    "because", "been", "before", "being", "between", "both", "but", "by", "can", "could",
    "did", "do", "does", "done", "due", "during", "each", "either", "et", "etc", "even", "ever",
    "every", "few", "for", "from", "further", "had", "has", "have", "having", "here", "how",
    "however", "i", "if", "in", "into", "is", "it", "its", "itself", "just", "least", "less",
    "like", "made", "make", "many", "may", "more", "most", "much", "must", "near", "neither",
    "no", "nor", "not", "now", "of", "off", "often", "on", "once", "one", "only", "or", "other",
    "others", "otherwise", "our", "out", "over", "own", "per", "perhaps", "rather", "same",
    "several", "should", "show", "shown", "shows", "showed", "since", "so", "some", "such",
    "than", "that", "the", "their", "them", "then", "there", "therefore", "these", "they",
    "this", "those", "though", "through", "thus", "to", "together", "too", "two", "three",
    "under", "until", "up", "upon", "us", "use", "used", "uses", "using", "very", "via", "was",
    "we", "well", "were", "what", "when", "where", "whereas", "whether", "which", "while",
    "who", "whose", "why", "will", "with", "within", "without", "would", "yet", "you", "your",
    // A paper's abstract says these of any subject.
    "abstract", "addition", "additionally", "analysis", "analyses", "analyzed", "approach",
    "approaches", "available", "based", "background", "conclusion", "conclusions", "different",
    "evidence", "find", "findings", "first", "found", "furthermore", "here", "high", "higher",
    "important", "including", "increased", "known", "large", "low", "lower", "method",
    "methods", "new", "novel", "number", "observed", "obtained", "paper", "particular",
    "potential", "present", "previously", "propose", "proposed", "provide", "provides",
    "recent", "related", "report", "reported", "result", "results", "reveal", "revealed",
    "role", "significant", "significantly", "similar", "specific", "studies", "study",
    "suggest", "suggests", "understanding", "work", "able", "across", "identified", "identify",
    "investigate", "investigated", "key", "major", "multiple", "remains", "respectively",
    "jats", "sec", "title", "italic", "sup", "sub", "copyright", "rights", "reserved", "author",
    "authors", "published", "elsevier", "springer", "wiley", "inc", "ltd", "license",
];

/// Each paper's TF-IDF vector over the words of two papers or more and of
/// fewer than half, sorted by word and of unit length; the words, and how
/// many papers have each; and the single words most papers have, of all.
#[allow(clippy::type_complexity)]
fn tfidf(
    docs: &[Doc],
) -> (
    Vec<String>,
    Vec<Vec<(u32, f32)>>,
    Vec<usize>,
    Vec<(String, usize)>,
) {
    let stop: HashSet<&str> = STOP.iter().copied().collect();
    let keep = |w: &str| w.len() > 2 && !stop.contains(w) && !w.bytes().all(|b| b.is_ascii_digit());
    let terms = |words: &[String]| -> Vec<String> {
        let mut out: Vec<String> = Vec::new();
        for (k, w) in words.iter().enumerate() {
            if !keep(w) {
                continue;
            }
            out.push(w.clone());
            if let Some(next) = words.get(k + 1).filter(|v| keep(v)) {
                out.push(format!("{w} {next}"));
            }
        }
        out
    };
    let counted: Vec<HashMap<String, f32>> = docs
        .iter()
        .map(|d| {
            let mut tf: HashMap<String, f32> = HashMap::new();
            for t in terms(&d.title) {
                *tf.entry(t).or_default() += 2.0;
            }
            for t in terms(&d.body) {
                *tf.entry(t).or_default() += 1.0;
            }
            tf
        })
        .collect();
    let mut df: HashMap<&str, usize> = HashMap::new();
    for tf in &counted {
        for t in tf.keys() {
            *df.entry(t.as_str()).or_default() += 1;
        }
    }
    let n = docs.len();
    let mut vocab: Vec<&str> = df
        .iter()
        .filter(|&(_, &d)| d >= 2 && d * 2 <= n.max(4))
        .map(|(t, _)| *t)
        .collect();
    vocab.sort_unstable();
    let id: HashMap<&str, u32> = vocab
        .iter()
        .enumerate()
        .map(|(i, t)| (*t, i as u32))
        .collect();
    let vectors = counted
        .iter()
        .map(|tf| {
            let mut v: Vec<(u32, f32)> = tf
                .iter()
                .filter_map(|(t, &c)| {
                    let &j = id.get(t.as_str())?;
                    let idf = (n as f32 / df[t.as_str()] as f32).ln();
                    Some((j, (1.0 + c.ln()) * idf))
                })
                .collect();
            let norm = v.iter().map(|(_, x)| x * x).sum::<f32>().sqrt();
            v.iter_mut()
                .for_each(|(_, x)| *x /= norm.max(f32::MIN_POSITIVE));
            v.sort_unstable_by_key(|&(j, _)| j);
            v
        })
        .collect();
    let dfs = vocab.iter().map(|t| df[t]).collect();
    let mut common: Vec<(String, usize)> = df
        .iter()
        .filter(|(w, _)| !w.contains(' '))
        .map(|(w, &d)| (w.to_string(), d))
        .collect();
    common.sort_by(|a, b| b.1.cmp(&a.1).then_with(|| a.0.cmp(&b.0)));
    common.truncate(30);
    (
        vocab.into_iter().map(str::to_string).collect(),
        vectors,
        dfs,
        common,
    )
}

/// The nearest to each paper of `queries`, of those `allowed`, by cosine
/// distance, nearest first, by its words of most weight; papers of no words
/// have none.
fn knn(
    vectors: &[Vec<(u32, f32)>],
    terms: usize,
    queries: &[usize],
    allowed: &(dyn Fn(usize) -> bool + Sync),
) -> Vec<Vec<(usize, f32)>> {
    let mut postings: Vec<Vec<(u32, f32)>> = vec![Vec::new(); terms];
    for (i, v) in vectors.iter().enumerate().filter(|&(i, _)| allowed(i)) {
        for &(j, x) in v {
            postings[j as usize].push((i as u32, x));
        }
    }
    let n = vectors.len();
    queries
        .par_iter()
        .map_init(
            || (vec![0.0f32; n], Vec::<usize>::new()),
            |(score, touched), &i| {
                let mut query = vectors[i].clone();
                query.sort_unstable_by(|a, b| b.1.total_cmp(&a.1));
                query.truncate(QUERY_TERMS);
                for &(j, x) in &query {
                    for &(k, y) in &postings[j as usize] {
                        let k = k as usize;
                        if score[k] == 0.0 {
                            touched.push(k);
                        }
                        score[k] += x * y;
                    }
                }
                let mut near: Vec<(usize, f32)> = touched
                    .drain(..)
                    .map(|k| (k, 1.0 - std::mem::take(&mut score[k])))
                    .filter(|&(k, _)| k != i && allowed(k))
                    .collect();
                if near.len() > KNN {
                    near.select_nth_unstable_by(KNN, |a, b| a.1.total_cmp(&b.1));
                    near.truncate(KNN);
                }
                near.sort_unstable_by(|a, b| a.1.total_cmp(&b.1));
                near
            },
        )
        .collect()
}

/// UMAP's fuzzy graph of the neighbours: each one's weight
/// `exp(-(d - ρ)/σ)`, ρ the nearest's distance and σ such that the weights
/// add to log2 k; then each pair's two weights as one, `a + b - ab`.
fn fuzzy(of: &[usize], near: &[Vec<(usize, f32)>]) -> HashMap<(usize, usize), f32> {
    let mut edges: HashMap<(usize, usize), f32> = HashMap::new();
    for (&i, nn) in of.iter().zip(near) {
        for (j, w) in weights(nn) {
            union(&mut edges, (i.min(j), i.max(j)), w);
        }
    }
    edges
}

/// `w` into the pair's weight, the two as one, `a + b - ab`.
fn union(edges: &mut HashMap<(usize, usize), f32>, pair: (usize, usize), w: f32) {
    edges
        .entry(pair)
        .and_modify(|v| *v = *v + w - *v * w)
        .or_insert(w);
}

/// Each neighbour's weight, `exp(-(d - ρ)/σ)`.
fn weights(nn: &[(usize, f32)]) -> Vec<(usize, f32)> {
    let Some(&(_, rho)) = nn.first() else {
        return Vec::new();
    };
    let dists: Vec<f32> = nn.iter().map(|&(_, d)| d).collect();
    let sigma = smooth_knn_sigma(&dists, rho, (nn.len() as f32).log2().max(1.0));
    nn.iter()
        .map(|&(j, d)| (j, (-((d - rho).max(0.0)) / sigma).exp()))
        .collect()
}

/// σ by bisection, as UMAP finds it, so the weights add to `target`.
fn smooth_knn_sigma(dists: &[f32], rho: f32, target: f32) -> f32 {
    let (mut lo, mut hi, mut mid) = (0.0f32, f32::INFINITY, 1.0f32);
    for _ in 0..64 {
        let sum: f32 = dists
            .iter()
            .map(|&d| (-((d - rho).max(0.0)) / mid).exp())
            .sum();
        if (sum - target).abs() < 1e-5 {
            break;
        }
        if sum > target {
            hi = mid;
            mid = (lo + hi) / 2.0;
        } else {
            lo = mid;
            mid = if hi.is_finite() {
                (lo + hi) / 2.0
            } else {
                mid * 2.0
            };
        }
    }
    mid.max(1e-3)
}

/// Pairs of papers sharing authors, weighed by how many they share against
/// the fewer authors of the two, each paper keeping its strongest few; not
/// through papers of more than `MAX_AUTHORS`.
fn coauthors(docs: &[Doc], among: &dyn Fn(usize) -> bool) -> HashMap<(usize, usize), f32> {
    let mut by_author: HashMap<&str, Vec<usize>> = HashMap::new();
    for (i, d) in docs.iter().enumerate() {
        if d.authors.len() > MAX_AUTHORS || !among(i) {
            continue;
        }
        let authors: HashSet<&str> = d.authors.iter().map(String::as_str).collect();
        for a in authors {
            by_author.entry(a).or_default().push(i);
        }
    }
    let mut shared: HashMap<(usize, usize), u32> = HashMap::new();
    for papers in by_author.values() {
        for (x, &i) in papers.iter().enumerate() {
            for &j in &papers[x + 1..] {
                if i != j {
                    *shared.entry((i.min(j), i.max(j))).or_default() += 1;
                }
            }
        }
    }
    let mut strongest: Vec<Vec<(usize, f32)>> = vec![Vec::new(); docs.len()];
    for (&(i, j), &s) in &shared {
        let fewer = docs[i].authors.len().min(docs[j].authors.len()).max(1) as f32;
        let w = (s as f32 / fewer.sqrt()).min(1.0);
        strongest[i].push((j, w));
        strongest[j].push((i, w));
    }
    let mut edges = HashMap::new();
    for (i, mut s) in strongest.into_iter().enumerate() {
        s.sort_unstable_by(|a, b| b.1.total_cmp(&a.1).then(a.0.cmp(&b.0)));
        for (j, w) in s.into_iter().take(COAUTHOR_KNN) {
            edges.insert((i.min(j), i.max(j)), w);
        }
    }
    edges
}

/// The co-author links into the word links, each pair's weights as one.
fn blend(edges: &mut HashMap<(usize, usize), f32>, coauthors: &HashMap<(usize, usize), f32>) {
    for (&pair, &w) in coauthors {
        union(edges, pair, COAUTHOR_WEIGHT * w);
    }
}

/// The graph's communities: each node moved to the neighbours' community
/// that most raises modularity at resolution `gamma`, until none moves; the
/// communities then made nodes, and again, until nothing is gained.
fn louvain(n: usize, edges: &[(usize, usize, f32)], gamma: f64) -> Vec<usize> {
    let mut adj: Vec<Vec<(usize, f64)>> = vec![Vec::new(); n];
    for &(i, j, w) in edges {
        adj[i].push((j, w as f64));
        adj[j].push((i, w as f64));
    }
    let mut of: Vec<usize> = (0..n).collect();
    let mut rng = Rng(0x9e37_79b9_7f4a_7c15);
    loop {
        let m = adj.len();
        let k: Vec<f64> = adj.iter().map(|a| a.iter().map(|e| e.1).sum()).collect();
        let m2: f64 = k.iter().sum();
        if m2 == 0.0 {
            break;
        }
        let mut comm: Vec<usize> = (0..m).collect();
        let mut tot = k.clone();
        let mut moved_any = false;
        let mut order: Vec<usize> = (0..m).collect();
        let mut to: HashMap<usize, f64> = HashMap::new();
        loop {
            rng.shuffle(&mut order);
            let mut moved = false;
            for &i in &order {
                let c = comm[i];
                to.clear();
                for &(j, w) in &adj[i] {
                    if j != i {
                        *to.entry(comm[j]).or_default() += w;
                    }
                }
                tot[c] -= k[i];
                let gain = |c: usize, w: f64| w - gamma * tot[c] * k[i] / m2;
                let mut best = (c, gain(c, to.get(&c).copied().unwrap_or(0.0)));
                for (&d, &w) in &to {
                    let g = gain(d, w);
                    if g > best.1 + 1e-12 || (g == best.1 && d < best.0) {
                        best = (d, g);
                    }
                }
                tot[best.0] += k[i];
                if best.0 != c {
                    comm[i] = best.0;
                    moved = true;
                    moved_any = true;
                }
            }
            if !moved {
                break;
            }
        }
        if !moved_any {
            break;
        }
        let mut renumber: HashMap<usize, usize> = HashMap::new();
        for c in &mut comm {
            let next = renumber.len();
            *c = *renumber.entry(*c).or_insert(next);
        }
        let mut merged: Vec<HashMap<usize, f64>> = vec![HashMap::new(); renumber.len()];
        for (i, a) in adj.iter().enumerate() {
            for &(j, w) in a {
                *merged[comm[i]].entry(comm[j]).or_default() += w;
            }
        }
        // A self-loop is listed once, holding both directions' weight.
        adj = merged
            .into_iter()
            .map(|e| {
                let mut v: Vec<(usize, f64)> = e.into_iter().collect();
                v.sort_unstable_by_key(|e| e.0);
                v
            })
            .collect();
        for c in &mut of {
            *c = comm[*c];
        }
    }
    of
}

/// The papers placed by UMAP's SGD on the graph, each theme starting
/// gathered at a place of its own.
fn layout(
    n: usize,
    edges: &[(usize, usize, f32)],
    theme: &[Option<usize>],
    themes: usize,
) -> Vec<[f32; 2]> {
    let mut rng = Rng(42);
    let centres: Vec<[f32; 2]> = (0..themes)
        .map(|_| [rng.range(-10.0, 10.0), rng.range(-10.0, 10.0)])
        .collect();
    let mut init = Vec::with_capacity(n * 2);
    for t in theme {
        let c = t.map_or([0.0, 0.0], |t| centres[t]);
        init.push(c[0] + rng.range(-1.0, 1.0));
        init.push(c[1] + rng.range(-1.0, 1.0));
    }
    let y = Umap {
        n_epochs: if n > 10_000 { 200 } else { 500 },
        ..Umap::default()
    }
    .fit(edges, n, &init);
    y.as_chunks::<2>().0.to_vec()
}

/// Each theme's words: those of its papers most and of all papers least,
/// as c-TF-IDF weighs them, a word left out where a pair holding it is
/// named; and its authors of most papers.
fn name_themes(
    themes: &mut [Theme],
    docs: &[Doc],
    theme: &[Option<usize>],
    vectors: &[Vec<(u32, f32)>],
    vocab: &[String],
    df: &[usize],
) {
    let n = docs.len() as f32;
    let mut inside: Vec<HashMap<u32, usize>> = vec![HashMap::new(); themes.len()];
    let mut authors: Vec<HashMap<&str, usize>> = vec![HashMap::new(); themes.len()];
    for (i, t) in theme.iter().enumerate() {
        let Some(t) = *t else { continue };
        for &(j, _) in &vectors[i] {
            *inside[t].entry(j).or_default() += 1;
        }
        for a in &docs[i].authors {
            *authors[t].entry(a.as_str()).or_default() += 1;
        }
    }
    for (t, theme) in themes.iter_mut().enumerate() {
        let size = theme.size as f32;
        let mut scored: Vec<(u32, f32)> = inside[t]
            .iter()
            .filter(|&(_, &c)| c >= 2 || theme.size < 4)
            .map(|(&j, &c)| {
                let pair = if vocab[j as usize].contains(' ') {
                    1.2
                } else {
                    1.0
                };
                (
                    j,
                    pair * (c as f32 / size) * (1.0 + n / df[j as usize] as f32).ln(),
                )
            })
            .collect();
        scored.sort_unstable_by(|a, b| b.1.total_cmp(&a.1).then(a.0.cmp(&b.0)));
        let mut words: Vec<String> = Vec::new();
        for (j, _) in scored {
            let w = &vocab[j as usize];
            let parts: Vec<&str> = w.split(' ').collect();
            // Not a word a pair named has, nor a pair of words named.
            let said = words.iter().any(|v| {
                v.split(' ').any(|p| p == w) || parts.iter().all(|p| v.split(' ').any(|q| q == *p))
            });
            if !said {
                words.retain(|v| !parts.contains(&v.as_str()));
                words.push(w.clone());
            }
            if words.len() == WORDS_NAMED {
                break;
            }
        }
        theme.words = words;
        let mut a: Vec<(String, usize)> = authors[t]
            .iter()
            .filter(|&(_, &c)| c >= 2)
            .map(|(a, &c)| (a.to_string(), c))
            .collect();
        a.sort_by(|x, y| y.1.cmp(&x.1).then_with(|| x.0.cmp(&y.0)));
        a.truncate(AUTHORS_NAMED);
        theme.authors = a;
    }
}

/// xorshift64*: enough for shuffling and sampling, and the same each run.
struct Rng(u64);

impl Rng {
    fn next(&mut self) -> u64 {
        self.0 ^= self.0 >> 12;
        self.0 ^= self.0 << 25;
        self.0 ^= self.0 >> 27;
        self.0.wrapping_mul(0x2545_f491_4f6c_dd1d)
    }

    fn below(&mut self, n: usize) -> usize {
        (((self.next() >> 32) * n as u64) >> 32) as usize
    }

    fn range(&mut self, lo: f32, hi: f32) -> f32 {
        lo + (hi - lo) * ((self.next() >> 40) as f32 / (1u64 << 24) as f32)
    }

    fn shuffle<T>(&mut self, v: &mut [T]) {
        for i in (1..v.len()).rev() {
            v.swap(i, self.below(i + 1));
        }
    }
}

// UMAP's SGD, as legume-numeric's `matrix::umap` runs it, copied here:
// the low-dimensional kernel `1 / (1 + a·d^(2b))`, `(a, b)` fit for spread 1
// and min_dist 0.1. Each edge is sampled from both ends, as uwot and
// umap-learn sample it, in proportion to its weight; each end in turn
// pulled towards the other and pushed from `negative_sample_rate` random
// points. Edges are taken in parallel, the places shared without locks, as
// HOGWILD! (Recht et al., NeurIPS 2011) and umap-learn's numba take them.

const A: f32 = 1.929;
const B: f32 = 0.7915;

struct Umap {
    n_epochs: usize,
    negative_sample_rate: usize,
    learning_rate: f32,
    seed: u64,
    a: f32,
    b: f32,
}

impl Default for Umap {
    fn default() -> Self {
        Self {
            n_epochs: 500,
            negative_sample_rate: 5,
            learning_rate: 1.0,
            seed: 42,
            a: A,
            b: B,
        }
    }
}

/// The places, shared by the threads HOGWILD! as f32 bits: each thread's
/// updates may race and be lost, which the method tolerates, each a
/// gradient clamped small; relaxed atomics make that defined.
struct Hogwild(Vec<AtomicU32>);

impl Hogwild {
    #[inline]
    fn get(&self, i: usize) -> [f32; 2] {
        let at = |k: usize| f32::from_bits(self.0[k].load(Ordering::Relaxed));
        [at(i * 2), at(i * 2 + 1)]
    }

    #[inline]
    fn add(&self, i: usize, d: [f32; 2]) {
        for (k, d) in [(i * 2, d[0]), (i * 2 + 1, d[1])] {
            let v = f32::from_bits(self.0[k].load(Ordering::Relaxed)) + d;
            self.0[k].store(v.to_bits(), Ordering::Relaxed);
        }
    }
}

impl Umap {
    /// `edges` as `(i, j, weight)`, weight in (0, 1], each pair once; `init`
    /// the `n × 2` places to start from, row by row. The result is centred.
    fn fit(&self, edges: &[(usize, usize, f32)], n: usize, init: &[f32]) -> Vec<f32> {
        assert_eq!(init.len(), n * 2, "init size mismatch");

        let max_weight = edges.iter().map(|e| e.2).fold(1e-4f32, f32::max);
        let epochs_per_sample: Vec<f32> = edges
            .iter()
            .map(|&(_, _, w)| {
                if w > 0.0 {
                    max_weight / w
                } else {
                    f32::INFINITY
                }
            })
            .collect();
        let mut next_epoch = epochs_per_sample.clone();
        let coords = &Hogwild(init.iter().map(|x| AtomicU32::new(x.to_bits())).collect());
        let (a, b, n_neg, seed) = (self.a, self.b, self.negative_sample_rate, self.seed);
        for epoch in 0..self.n_epochs {
            let epoch_f = epoch as f32;
            let alpha = self.learning_rate * (1.0 - epoch_f / self.n_epochs as f32);
            edges
                .par_iter()
                .zip(next_epoch.par_iter_mut())
                .zip(epochs_per_sample.par_iter())
                .for_each_init(
                    || {
                        let tid = rayon::current_thread_index().unwrap_or(0) as u64;
                        Rng((seed ^ ((epoch as u64) << 32) ^ (tid << 8)) | 1)
                    },
                    |rng, ((&(i, j, _), next), &every)| {
                        if *next > epoch_f {
                            return;
                        }
                        for (head, tail) in [(i, j), (j, i)] {
                            attract(coords, head, tail, alpha, a, b);
                            for _ in 0..n_neg {
                                let k = rng.below(n);
                                if k != head {
                                    repel(coords, head, k, alpha, a, b);
                                }
                            }
                        }
                        *next += every;
                    },
                );
        }
        let mut y: Vec<f32> = coords
            .0
            .iter()
            .map(|x| f32::from_bits(x.load(Ordering::Relaxed)))
            .collect();
        for c in 0..2 {
            let mean = (0..n).map(|i| y[i * 2 + c]).sum::<f32>() / n.max(1) as f32;
            (0..n).for_each(|i| y[i * 2 + c] -= mean);
        }
        y
    }
}

#[inline]
fn attract(y: &Hogwild, i: usize, j: usize, alpha: f32, a: f32, b: f32) {
    let (p, q) = (y.get(i), y.get(j));
    let diff = [p[0] - q[0], p[1] - q[1]];
    let d2 = diff[0] * diff[0] + diff[1] * diff[1];
    if d2 <= 0.0 {
        return;
    }
    let d2b = d2.powf(b);
    let coeff = -2.0 * a * b * (d2b / d2) / (a * d2b + 1.0);
    let g = diff.map(|d| (d * coeff).clamp(-4.0, 4.0) * alpha);
    y.add(i, g);
    y.add(j, g.map(|x| -x));
}

#[inline]
fn repel(y: &Hogwild, i: usize, k: usize, alpha: f32, a: f32, b: f32) {
    let (p, q) = (y.get(i), y.get(k));
    let diff = [p[0] - q[0], p[1] - q[1]];
    let d2 = diff[0] * diff[0] + diff[1] * diff[1];
    let coeff = if d2 > 0.0 {
        2.0 * b / ((0.001 + d2) * (a * d2.powf(b) + 1.0))
    } else {
        4.0
    };
    y.add(i, diff.map(|d| (d * coeff).clamp(-4.0, 4.0) * alpha));
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A paper of `words`, by `authors`.
    fn doc(key: &str, words: &str, authors: &[&str]) -> Doc {
        Doc {
            key: key.to_string(),
            title: crate::pdf::words(words),
            body: crate::pdf::words(words),
            authors: authors.iter().map(|a| a.to_string()).collect(),
        }
    }

    /// Two subjects, each of `n` papers by its own authors, and their words
    /// each paper's own besides.
    fn two_subjects(n: usize) -> Vec<Doc> {
        let mut docs = Vec::new();
        for i in 0..n {
            docs.push(doc(
                &format!("cell{i}"),
                &format!(
                    "single cell rna sequencing atlas transcriptome cluster{} marker{i}",
                    i % 3
                ),
                &["Theis F", "Regev A"],
            ));
            docs.push(doc(
                &format!("gwas{i}"),
                &format!(
                    "genome wide association heritability variants loci trait{} snp{i}",
                    i % 3
                ),
                &["Visscher P", "Price A"],
            ));
        }
        docs
    }

    #[test]
    fn gathers_papers_by_their_words_and_names_the_themes() {
        let docs = two_subjects(30);
        let map = Map::build(&docs, RESOLUTION);
        let theme_of = |key: &str| map.theme[map.keys.iter().position(|k| k == key).unwrap()];
        assert!(map.themes.len() >= 2);
        // Each subject's papers in themes of their own.
        let cells: HashSet<_> = (0..30).map(|i| theme_of(&format!("cell{i}"))).collect();
        let gwas: HashSet<_> = (0..30).map(|i| theme_of(&format!("gwas{i}"))).collect();
        assert!(cells.is_disjoint(&gwas), "{cells:?} {gwas:?}");
        let words = |t: Option<usize>| map.themes[t.unwrap()].words.join(" ");
        assert!(
            words(theme_of("cell0")).contains("cell"),
            "{}",
            words(theme_of("cell0"))
        );
        assert!(
            words(theme_of("gwas0")).contains("genome"),
            "{}",
            words(theme_of("gwas0"))
        );
        assert_eq!(
            map.themes[theme_of("cell0").unwrap()].authors[0].1,
            map.themes[theme_of("cell0").unwrap()].size
        );
        assert!(map.xy.iter().all(|p| p[0].is_finite() && p[1].is_finite()));
    }

    #[test]
    fn gathers_the_themes_again_in_the_same_layout() {
        let mut docs = two_subjects(20);
        let map = Map::build(&docs, RESOLUTION);
        // A paper new since stays off it, to be placed.
        docs.push(doc("new", "single cell atlas marker3 cluster1", &[]));
        let coarse = map.regroup(&docs, 0.2);
        assert_eq!(coarse.resolution, 0.2);
        assert_eq!(coarse.keys, map.keys);
        assert_eq!(coarse.xy, map.xy);
        assert!(coarse.themes.len() <= map.themes.len());
        assert_eq!(coarse.themes.iter().map(|t| t.size).sum::<usize>(), 40);
    }

    #[test]
    fn a_name_given_follows_its_papers() {
        let docs = two_subjects(20);
        let dir = tempfile::tempdir().unwrap();
        let mut kept = Kept::open(&dir.path().join("map.json"), &docs).unwrap();
        let cell0 =
            |map: &Map| map.theme[map.keys.iter().position(|k| k == "cell0").unwrap()].unwrap();
        let t = cell0(&kept.current);
        kept.name(t, "  scRNA atlases ").unwrap();
        assert_eq!(kept.current.themes[t].name(), "scRNA atlases");
        // Gathered again at the same resolution, the same papers: named still.
        kept.regroup(&docs, RESOLUTION).unwrap();
        assert_eq!(
            kept.current.themes[cell0(&kept.current)].name(),
            "scRNA atlases"
        );
        let kept = Kept::read(&dir.path().join("map.json")).unwrap();
        assert_eq!(
            kept.current.themes[cell0(&kept.current)].name(),
            "scRNA atlases"
        );
        // A blank name, its own words again; so are its words, as offered.
        let mut kept = kept;
        let t = cell0(&kept.current);
        let words = kept.current.themes[t].words_name();
        kept.name(t, &words).unwrap();
        assert!(kept.current.themes[t].called.is_none());
        kept.name(t, " ").unwrap();
        assert!(kept.current.themes[t].called.is_none());
        assert!(kept.name(99, "x").is_err());
    }

    #[test]
    fn splits_a_theme_in_finer_ones() {
        let docs = two_subjects(20);
        let mut map = Map::build(&docs, RESOLUTION);
        // Both subjects as one theme, as at a coarse resolution.
        let xy = map.xy.clone();
        for t in map.theme.iter_mut().flatten() {
            *t = 0;
        }
        map.recount();
        assert_eq!(map.themes[0].size, 40);
        let largest = map.split(&docs, 0, RESOLUTION).expect("it splits");
        assert!(map.themes.len() >= 2);
        assert_eq!(map.themes[largest].size, map.themes[0].size);
        let theme_of = |key: &str| map.theme[map.keys.iter().position(|k| k == key).unwrap()];
        let cells: HashSet<_> = (0..20).map(|i| theme_of(&format!("cell{i}"))).collect();
        let gwas: HashSet<_> = (0..20).map(|i| theme_of(&format!("gwas{i}"))).collect();
        assert!(cells.is_disjoint(&gwas), "{cells:?} {gwas:?}");
        assert_eq!(map.themes.iter().map(|t| t.size).sum::<usize>(), 40);
        assert!(
            map.themes
                .iter()
                .filter(|t| t.size > 0)
                .all(|t| !t.words.is_empty())
        );
        // The papers stay where they were.
        assert_eq!(map.xy, xy);
    }

    #[test]
    fn places_a_new_paper_with_its_nearest() {
        let mut docs = two_subjects(20);
        let mut map = Map::build(&docs, RESOLUTION);
        // Words of half the papers say nothing; these say which half.
        docs.push(doc(
            "new",
            "a single cell atlas marker3 cluster1",
            &["Someone N"],
        ));
        let placed = map.place(&docs);
        assert_eq!(placed.len(), 1);
        // Its theme, as the map numbers them now: largest first.
        let cell0 = |map: &Map| map.theme[map.keys.iter().position(|k| k == "cell0").unwrap()];
        let size = |map: &Map| map.themes[cell0(map).unwrap()].size;
        assert_eq!(Some(placed[0].theme), cell0(&map));
        assert!(placed[0].share > 0.5);
        let was = size(&map);
        map.accept(&placed);
        assert_eq!(size(&map), was + 1);
        assert_eq!(map.theme.last().copied().flatten(), cell0(&map));
        assert!(map.themes.windows(2).all(|w| w[0].size >= w[1].size));
        assert!(map.place(&docs).is_empty());
        // A paper gone is taken off.
        map.keep(
            &docs
                .iter()
                .map(|d| d.key.as_str())
                .filter(|k| *k != "new")
                .collect(),
        );
        assert_eq!(size(&map), was);
        assert!(!map.keys.contains(&"new".to_string()));
    }

    #[test]
    fn keeps_the_map_before_and_shows_it_again() {
        let dir = tempfile::tempdir().unwrap();
        let path = default_path(&dir.path().join("index-ab.json"));
        assert!(path.ends_with("map-ab.json"));
        let docs = two_subjects(10);
        let kept = Kept::open(&path, &docs).unwrap();
        assert!(kept.placed.is_empty() && kept.previous.is_none());
        let mut kept = Kept::read(&path).unwrap();
        let more = two_subjects(12);
        kept.rebuild(&more, None).unwrap();
        let mut kept = Kept::read(&path).unwrap();
        assert_eq!(kept.current.keys.len(), more.len());
        assert_eq!(kept.previous.as_ref().unwrap().keys.len(), docs.len());
        // Nothing to assign keeps the map before as it was.
        assert_eq!(kept.assign().unwrap(), 0);
        assert_eq!(kept.previous.as_ref().unwrap().keys.len(), docs.len());
        assert!(kept.undo(&docs).unwrap());
        let kept = Kept::read(&path).unwrap();
        assert_eq!(kept.current.keys.len(), docs.len());
        assert_eq!(kept.previous.as_ref().unwrap().keys.len(), more.len());
    }
}
