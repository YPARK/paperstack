//! `AGENTS.md`, written at the library's root: how an agent finds papers
//! here, and makes a bibliography of them.

pub const TEXT: &str = r#"# This folder is a paperstack library

Papers are filed as `year/lastname/firstname/<key>_<slug>.pdf`, each with
its record beside it, the same name ending `.json`: the paper as CSL JSON
under `csl`, its folders, labels and star under `paperstack`. The year is
the year issued, the names the first author's. The key, of the form
`Surname2025-xy`, starts the file's name and never changes.

Do not write or rename files here by hand; use `paperstack`, which keeps the
records and the PDFs together. Reading them is fine.

## Finding papers

```sh
paperstack find 'perturbation "linear baseline"' --json      # best match first
paperstack find 'author:Huber year:2020..2026 has:pdf' --json
paperstack find 'collection:"Single Cell Data" -label:benchmark' --sort year --json
paperstack get Ahlmann-Eltze2025-pb --json                   # one paper, by key
find . -name 'Ahlmann-Eltze2025-pb_*'                        # its files
```

Words and "phrases" are searched in the title, abstract, authors, venue,
keywords, key and identifiers; a word matches the words it starts. Fields:
`title:` `abstract:` `author:` `venue:` `year:2020..2026` `folder:` `label:`
`key:` `doi:` `collection:` `has:pdf` `has:abstract` `has:doi` `is:starred`; `-` before
any of them leaves out what it matches. `--sort relevance|year|added|author|title`,
`--reverse`, `--limit N`.

## Adding papers

```sh
paperstack add 10.1038/s41586-020-2649-2 --json              # by DOI
paperstack add arXiv:2412.08821 'PMID: 28693043' PMC5629050  # by arXiv id, PMID, PMCID
paperstack add https://www.sciencedirect.com/science/article/pii/S0092867423013442
paperstack add ~/Downloads/paper.pdf --folder grant_OCRA_AI  # a PDF, read to tell which paper
paperstack add 'Array programming with NumPy'                # a title: added only if one paper has it
paperstack inbox                                             # file the PDFs saved to inbox/
paperstack import library-export.json --dry-run            # another library, its keys kept
paperstack attach KEY ~/Downloads/paper.pdf                  # give a paper the PDF it lacks
```

A paper already here is not added again. Given only a title that several
papers have, `add` lists them with their ids and adds none: add the one meant
by its id. Never add a paper you have not checked is the one meant.

## Deleting

```sh
paperstack delete KEY                         # a paper, moved to .trash/
paperstack delete --collection NAME           # a collection; its papers stay
paperstack delete --label NAME
```

Delete only what the person asked to have deleted.

## Making a bibliography

```sh
paperstack find 'QUERY' --bib > refs.bib      # BibTeX, plain
paperstack find 'QUERY' --csl > refs.json     # CSL JSON, for pandoc and ohp
paperstack find 'QUERY' --keys                # [@a; @b], to cite them
paperstack get KEY --bib                      # one entry
```

Cite only keys that `paperstack` gives; never make one up.

An agent whose program speaks MCP, as Cursor or Claude Code, may instead be
given `paperstack mcp`: the same, as tools, and `read_paper` for a paper's
text.
"#;
