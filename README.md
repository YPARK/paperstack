# paperstack

A library of papers in the terminal: PDFs and their references, kept in a
folder, as in Dropbox, that an AI agent can read as easily as you can.
PDFs are presented with [ohp](https://github.com/YPARK/ohp).

This is a mock-up. The papers shown are made up in `src/mock.rs`; nothing is
fetched, and nothing is written but the clipboard.

## Run

Rust 1.92 or later, and `ohp` on the `PATH` to open PDFs:

```sh
cargo run
```

| Key | |
|---|---|
| `j` / `k`, arrows | move; on the left, change folder |
| `h` / `l`, `tab` | switch between folders and papers |
| `/` | search title, authors, key, venue and DOI; `esc` clears |
| `enter`, `o` | open the PDF in ohp; quit ohp to come back |
| `a` | add from a DOI, arXiv id, PMID, URL, PDF or BibTeX; the clipboard is filled in |
| `space` | mark a paper, for `c` and `y` to take several |
| `c` | BibTeX of the papers marked, or the one picked |
| `y` | copy `[@key; @key]` to the clipboard |
| `s` | star |
| `q` | quit |

## The library

One folder, `$PAPERSTACK_LIBRARY` or `~/Dropbox/paperstack`. Each paper is
filed by year and first author, its PDF and its record side by side:

```
2025/Ahlmann-Eltze/C/
  Ahlmann-Eltze2025-pb_deep-learning-based-gene-perturbation-effect.pdf
  Ahlmann-Eltze2025-pb_deep-learning-based-gene-perturbation-effect.json
```

The first author's given name names the third folder, or the initials where
only they are known. The file starts with the citation key, a reference manager's
`Surname2025-xy`, which never changes, so `find . -name 'KEY_*'` finds a
paper. The record is CSL JSON, as pandoc and ohp read it, with the paper's
folders, labels and star.

The index searched, SQLite, is kept on the machine, not in Dropbox, and is
built again from the records whenever they change: a database synced
between machines can be corrupted, while a record can at worst be a
conflicted copy of itself.

## License

MIT; see [LICENSE](LICENSE).
