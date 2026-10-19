//! `AGENTS.md`, written at the library's root: how an agent finds papers
//! here, and makes a bibliography of them.

pub const TEXT: &str = r#"# This folder is a paperstack library

Papers are filed as `year/lastname/firstname/<key>_<slug>.pdf`, each with
its record beside it, the same name ending `.json`: the paper as CSL JSON
under `csl`, its folders, labels and star under `paperstack`. The year is
the year issued, the names the first author's. The key, a reference manager's form
`Surname2025-xy`, starts the file's name and never changes.

Do not write or rename files here by hand; use `paperstack`, which keeps the
records and the PDFs together. Reading them is fine.

## Finding papers

```sh
paperstack find 'perturbation "linear baseline"' --json      # best match first
paperstack find 'author:Huber year:2020..2026 has:pdf' --json
paperstack find 'folder:"Single Cell Data" -label:benchmark' --sort year --json
paperstack get Ahlmann-Eltze2025-pb --json                   # one paper, by key
find . -name 'Ahlmann-Eltze2025-pb_*'                        # its files
```

Words and "phrases" are searched in the title, abstract, authors, venue,
keywords, key and identifiers; a word matches the words it starts. Fields:
`title:` `abstract:` `author:` `venue:` `year:2020..2026` `folder:` `label:`
`key:` `doi:` `has:pdf` `has:abstract` `has:doi` `is:starred`; `-` before
any of them leaves out what it matches. `--sort relevance|year|added|author|title`,
`--reverse`, `--limit N`.

## Making a bibliography

```sh
paperstack find 'QUERY' --bib > refs.bib      # BibTeX, a reference manager's form
paperstack find 'QUERY' --csl > refs.json     # CSL JSON, for pandoc and ohp
paperstack find 'QUERY' --keys                # [@a; @b], to cite them
paperstack get KEY --bib                      # one entry
```

Cite only keys that `paperstack` gives; never make one up.
"#;
