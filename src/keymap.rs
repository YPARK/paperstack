//! Every key of the list and of the map, once: the footer shows those it
//! says, the help dialog lists them all, and the README must name them.

use crate::app::View;

/// A key, or keys acting alike, in one view.
pub struct Key {
    pub view: View,
    /// As the help dialog lists it; blank on a line going on from the one
    /// before.
    pub keys: &'static str,
    pub what: &'static str,
    /// Each as `paperstack screen` is given it, and as pressed: the tests
    /// press each, to see it bound.
    #[cfg_attr(not(test), allow(dead_code))]
    pub pressed: &'static [&'static str],
    /// In the footer: as drawn, what it says, and the key a click presses.
    pub foot: &'static [(&'static str, &'static str, &'static str)],
}

const fn key(
    view: View,
    keys: &'static str,
    what: &'static str,
    pressed: &'static [&'static str],
    foot: &'static [(&'static str, &'static str, &'static str)],
) -> Key {
    Key {
        view,
        keys,
        what,
        pressed,
        foot,
    }
}

/// A line of help going on from the one before.
const fn more(view: View, what: &'static str) -> Key {
    key(view, "", what, &[], &[])
}

use View::{List, Map};

pub const KEYS: &[Key] = &[
    key(
        List,
        "j k ↑ ↓",
        "move; pgup pgdn by five; on the left, change shelf",
        &["j", "k", "<up>", "<down>", "<pagedown>", "<pageup>"],
        &[],
    ),
    key(
        List,
        "ctrl-n ctrl-p",
        "move, as Emacs does; ctrl-v alt-v by five, ctrl-f ctrl-b across",
        &["<c-n>", "<c-p>", "<c-v>", "<m-v>", "<c-f>", "<c-b>"],
        &[],
    ),
    key(
        List,
        "h l tab",
        "switch between shelves and papers",
        &["h", "l", "<tab>"],
        &[],
    ),
    key(
        List,
        "g G",
        "the first paper, the last; home end, ctrl-↑ ctrl-↓, alt-< alt-> too",
        &[
            "g", "G", "<home>", "<end>", "<c-up>", "<c-down>", "<m-<>", "<m->>",
        ],
        &[],
    ),
    key(
        List,
        "/",
        "search; esc clears. Words and \"phrases\" in title, abstract, authors",
        &["/"],
        &[("/", "search", "/")],
    ),
    more(
        List,
        "author:  year:2020..2026  folder:\"name\"  label:  has:pdf  is:starred  -...",
    ),
    key(
        List,
        "S",
        "sort: relevance, date added, year, first author, title; or click it",
        &["S"],
        &[("S", "sort", "S")],
    ),
    key(
        List,
        "⏎ o",
        "open the PDF in ohp; quit ohp to come back",
        &["<enter>", "o"],
        &[("⏎", "open in ohp", "<enter>")],
    ),
    key(
        List,
        "p",
        "get the PDF: an open copy, else in your browser, signed in, to download",
        &["p"],
        &[("p", "get PDF", "p")],
    ),
    key(
        List,
        "f",
        "locate a PDF on disk for the paper picked, where it has none",
        &["f"],
        &[],
    ),
    key(
        List,
        "ctrl-l",
        "the same, from the get-PDF dialog, instead of a download",
        &[],
        &[],
    ),
    key(
        List,
        "x",
        "delete the PDF picked, or those marked, to .trash/; papers stay",
        &["x"],
        &[],
    ),
    key(
        List,
        "space *",
        "mark a paper; * marks all shown, again unmarks; esc unmarks all",
        &["<space>", "*"],
        &[("␣", "mark", "<space>"), ("*", "mark all", "*")],
    ),
    key(
        List,
        "ctrl-e",
        "BibTeX of the papers marked, or the one picked; s there, APA, Chicago...",
        &["<c-e>"],
        &[("^E", "BibTeX", "<c-e>")],
    ),
    key(
        List,
        "C",
        "references of the same, in APA or the style last shown; s there, the next",
        &["C"],
        &[],
    ),
    key(
        List,
        "w",
        "web pages of the same: DOI, address, arXiv, PubMed; ⏎ opens one, y copies them",
        &["w"],
        &[],
    ),
    key(
        List,
        "y",
        "copy [@key; @key] to the clipboard",
        &["y"],
        &[("y", "yank [@key]", "y")],
    ),
    key(
        List,
        "s",
        "star, written to the paper's record",
        &["s"],
        &[("s", "star", "s")],
    ),
    key(List, "n", "on a theme's shelf, name the theme", &["n"], &[]),
    key(
        List,
        "L",
        "label the papers marked, or else all shown",
        &["L"],
        &[],
    ),
    key(
        List,
        "d del",
        "delete the papers marked, or the one picked, to .trash/; asked first",
        &["d", "<del>"],
        &[],
    ),
    more(List, "on the left, a collection or label: its papers stay"),
    key(
        List,
        "a",
        "add from a DOI, arXiv id, PMID, address, PDF or title",
        &["a"],
        &[("a", "add", "a")],
    ),
    key(
        List,
        "r",
        "read again what paperstack serve, or another machine, added",
        &["r"],
        &[],
    ),
    key(
        List,
        "m ⌘G",
        "the library's themes as a map, and back; or click ◆ Map; ? there for its keys",
        &["m", "<d-g>"],
        &[("m", "map", "m")],
    ),
    key(
        List,
        "< >",
        "the pane on the right wider, narrower; or drag its edge",
        &["<", ">"],
        &[],
    ),
    key(
        List,
        "click",
        "pick; on a box, mark; on a star, star; on PDF, get or delete; twice, open",
        &[],
        &[],
    ),
    key(
        List,
        "esc ctrl-g",
        "clear the search; again, papers from the map among all their shelf's; again, unmark all",
        // ctrl-g only in a dialog or a search, which these tests do not open.
        &["<esc>"],
        &[],
    ),
    key(
        List,
        "? F1",
        "these keys; F1 while typing a search too",
        &["?", "<f1>"],
        &[("?", "help", "?")],
    ),
    key(
        List,
        "ctrl-x ctrl-c",
        "quit, as Emacs does, from a dialog or a search too",
        &["<c-c>"],
        &[],
    ),
    key(
        Map,
        "← → h l",
        "pick a theme; ↑ ↓ j k, tab and the wheel too; or click its name",
        &[
            "<left>", "<right>", "h", "l", "<up>", "<down>", "j", "k", "<tab>",
        ],
        &[("→", "theme", "<right>")],
    ),
    key(
        Map,
        "1-9",
        "theme 1 to 9, as `paperstack themes` numbers them",
        &["1", "2"],
        &[],
    ),
    key(
        Map,
        "⏎",
        "into it, its papers alone, and their links; fewer than ten, to read in the list",
        &["<enter>"],
        &[("⏎", "into", "<enter>")],
    ),
    more(
        Map,
        "within, ← → the next theme, a click picks a paper, twice opens it, or lists a few",
    ),
    more(
        Map,
        "⏎ its card: its papers, newest first, ⏎ opens one, p gets its PDF, l lists them",
    ),
    key(
        Map,
        "esc ctrl-g",
        "let go of a finer theme picked; again, out of the theme; again, the list",
        &["<esc>", "<c-g>"],
        &[],
    ),
    key(
        Map,
        "n",
        "name it; blank, its own words again; a name follows its papers",
        &["n"],
        &[],
    ),
    key(
        Map,
        "L",
        "label its papers, in their records; d on the label takes it off",
        &["L"],
        &[],
    ),
    key(
        Map,
        "s",
        "split it in finer themes of its own; the rest stay as they are",
        &["s"],
        &[("s", "split", "s")],
    ),
    key(
        Map,
        "+ -",
        "every theme finer or coarser, the papers where they are",
        &["+", "-"],
        &[("+", "finer", "+"), ("-", "coarser", "-")],
    ),
    key(
        Map,
        "< >",
        "further from the theme picked, and closer",
        &["<", ">"],
        &[(">", "closer", ">")],
    ),
    key(
        Map,
        "shift-←→↑↓",
        "move what is shown closer; the wheel sideways, or with shift, too",
        &["<s-right>", "<s-left>", "<s-up>", "<s-down>"],
        &[],
    ),
    key(
        Map,
        "g",
        "the map laid out, papers near where alike; again, the mosaic, as at first",
        &["g"],
        &[("g", "view", "g")],
    ),
    key(
        Map,
        "R",
        "gather the themes again from all papers, the map keeping its shape",
        &["R"],
        &[],
    ),
    key(Map, "F", "the same, the map laid out anew", &["F"], &[]),
    key(
        Map,
        "N A",
        "where each paper new since, a cross, would go; A puts them there",
        &["N", "A"],
        &[],
    ),
    key(
        Map,
        "u",
        "the map before the last s, + -, R, F or A; again, back",
        &["u"],
        &[("u", "undo", "u")],
    ),
    key(
        Map,
        "c",
        "every theme in its colour, or the picked one alone, as shown at first; after t, as before",
        &["c"],
        &[],
    ),
    key(
        Map,
        "t [ ]",
        "each theme from grey to orange by its papers added lately; [ ] lately shorter, longer",
        &["t", "[", "]"],
        &[],
    ),
    key(
        Map,
        "/",
        "search, here, what it finds lit as typed; ⏎ done, esc clears it",
        &["/"],
        &[],
    ),
    key(Map, "a", "add a paper", &["a"], &[]),
    key(
        Map,
        "? F1",
        "these keys",
        &["?", "<f1>"],
        &[("?", "more", "?")],
    ),
    key(
        Map,
        "m ⌘G",
        "the list; esc too, from the whole map; or click ▤ List, top left",
        &["m", "<d-g>"],
        &[("m", "list", "m")],
    ),
    key(Map, "ctrl-x ctrl-c", "quit, as Emacs does", &["<c-c>"], &[]),
];

/// The keys of `view`, as the help dialog lists them.
pub fn of(view: View) -> impl Iterator<Item = &'static Key> {
    KEYS.iter().filter(move |k| k.view == view)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::app::{App, keys, screen_svg};
    use crate::tests::library;
    use std::collections::HashSet;

    /// What `spec`, pressed in `view` after `first`, changes: the screen,
    /// in its colours too, or what the caller is asked to do.
    fn acts(view: View, first: &str, spec: &str) -> bool {
        let (_dir, lib, index) = library();
        let mut app = App::new(lib, index).unwrap();
        if view == Map {
            app.key(keys("m")[0]);
            app.wait_for_map();
        }
        for k in keys(first) {
            app.key(k);
            app.wait_for_map();
        }
        let before = screen_svg(&mut app, 160, 44);
        let asked = app.key(keys(spec)[0]).is_some();
        app.wait_for_map();
        asked || app.quitting() || screen_svg(&mut app, 160, 44) != before
    }

    /// Each key of `view` pressed, to see it does something, and listed
    /// once.
    fn bound_and_listed_once(view: View) {
        // Each acting outside: a browser, the clipboard.
        let outside = ["p", "y", "a"];
        // Some act only from where another key has been: up from the second
        // paper, back from the shelves, unmarked once marked, quit after
        // ctrl-x.
        let first = ["", "j", "h", "<space>", "<right>", "<c-x>"];
        let mut seen = HashSet::new();
        for k in of(view) {
            for &spec in k.pressed {
                assert!(seen.insert(spec), "{spec} twice in {view:?}");
                if outside.contains(&spec) {
                    continue;
                }
                assert!(
                    first.iter().any(|f| acts(view, f, spec)),
                    "{spec}, in the table, does nothing in {view:?}"
                );
            }
            for &(_, _, spec) in k.foot {
                assert!(k.pressed.contains(&spec), "{spec} in the footer only");
            }
        }
    }

    #[test]
    fn every_list_key_in_the_table_is_bound_and_listed_once() {
        bound_and_listed_once(List);
    }

    #[test]
    fn every_map_key_in_the_table_is_bound_and_listed_once() {
        bound_and_listed_once(Map);
    }

    #[test]
    fn the_readme_names_every_key_the_help_does() {
        let readme = include_str!("../README.md");
        let between = |from: &str, to: &str| {
            let start = readme.find(from).unwrap();
            &readme[start..start + readme[start..].find(to).unwrap()]
        };
        let list = between("| Key | |", "## Deleting");
        let map = between("## Themes", "## Searching");
        for k in KEYS {
            let readme = if k.view == List { list } else { map };
            for word in k.keys.split_whitespace() {
                let word = match word {
                    "click" => continue,
                    "1-9" => "1",
                    w => w,
                };
                assert!(
                    readme.contains(&format!("`{word}`")),
                    "the README does not name `{word}` among the {:?} keys",
                    k.view
                );
            }
        }
    }
}
