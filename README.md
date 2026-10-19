# paperstack

A library of papers in the terminal: PDFs and their references, kept in a
folder, as in Dropbox, that an AI agent can read as easily as you can.
PDFs are presented with [ohp](https://github.com/YPARK/ohp).

Early days: papers cannot yet be added but by hand; `paperstack init --demo`
makes a library of nine to try it with.

## Install

Rust 1.92 or later, and `ohp` on the `PATH` to open PDFs:

```sh
cargo install --git https://github.com/YPARK/paperstack
```

SQLite is built in, which needs the system C compiler.

## Use

```sh
paperstack init --demo                 # a library at ~/Dropbox/paperstack, with papers to try
paperstack                             # browse it
paperstack find perturbation 'author:Huber'
paperstack find 'folder:"Single Cell Data" -label:benchmark' --sort year --bib > refs.bib
paperstack get Ahlmann-Eltze2025-pb --json
```

The library is `--library`, else `$PAPERSTACK_LIBRARY`, else
`~/Dropbox/paperstack`.

| Key | |
|---|---|
| `j` / `k`, arrows | move; on the left, change shelf |
| `h` / `l`, `tab` | switch between shelves and papers |
| `/` | search, as below; `esc` clears |
| `S` | sort by relevance, date added, year, first author or title |
| `enter`, `o` | open the PDF in ohp; quit ohp to come back |
| `space` | mark a paper; `*` marks all shown, or unmarks them; `esc` unmarks all |
| `c` | BibTeX of the papers marked, or the one picked |
| `y` | copy `[@key; @key]` to the clipboard |
| `s` | star, written to the paper's record |
| `q` | quit |

## Searching

The TUI's `/` and `paperstack find` read the same query:

```
perturbation "linear baseline" author:Huber year:2020..2026 folder:"Single Cell Data" -label:benchmark has:pdf
```

Words, and phrases in quotes, are looked for in the title, abstract,
authors, venue, keywords, key and DOI, title first; a word matches the words
it starts, and accents need not be typed. `title:`, `abstract:`, `author:`
and `venue:` look in one of those; `year:` takes a year or a range,
`2020..2026`, `2020..` or `..2020`; `folder:`, `label:`, `key:` and `doi:`
match whole; `has:pdf`, `has:abstract`, `has:doi` and `is:starred` say what a
paper has. A `-` before any of them leaves out what it matches.

`find` lists the papers found best first, or `--sort year|added|author|title`,
`--reverse`, `--limit N`, as a line each, or:

| | |
|---|---|
| `--bib` | BibTeX, as a reference manager writes it |
| `--csl` | CSL JSON, for pandoc's and ohp's `bibliography:` |
| `--keys` | `[@a; @b]`, to cite them |
| `--json` | key, title, authors, year, ids, folders, the files' paths: for agents |
| `--paths` | the PDFs |

## The library

Each paper is filed by year and first author, its PDF and its record side
by side:

```
2025/Ahlmann-Eltze/C/
  Ahlmann-Eltze2025-pb_deep-learning-based-gene-perturbation-effect.pdf
  Ahlmann-Eltze2025-pb_deep-learning-based-gene-perturbation-effect.json
```

The first author's given name names the third folder, or the initials where
only they are known. The file starts with the citation key, a reference manager's
`Surname2025-xy`, which never changes, so `find . -name 'KEY_*'` finds a
paper. The record is CSL JSON, as pandoc and ohp read it, under `csl`, with
the paper's folders, labels and star under `paperstack`. `AGENTS.md`, at the
root, tells an agent how to find papers and cite them.

The index searched, SQLite with full-text search, is kept on the machine, in
`~/Library/Caches/paperstack` (`~/.cache` elsewhere), not in Dropbox, and is
brought up to the records whenever paperstack starts, reading only those
changed: a database synced between machines can be corrupted, while a
record can at worst be a conflicted copy of itself, which paperstack names
rather than reads.

## License

MIT; see [LICENSE](LICENSE).
