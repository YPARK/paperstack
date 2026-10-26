# paperstack

A library of papers in the terminal: PDFs and their references, kept in a
folder, as in Dropbox, that an AI agent can read as easily as you can.
PDFs are presented with [ohp](https://crates.io/crates/ohp).

Early days. `paperstack init --demo` makes a library of nine papers to try
it with.

![paperstack's TUI, in its light theme](docs/tui.png)

`--theme dark`, or `PAPERSTACK_THEME=dark`, draws it in the terminal's own
colours instead.

## Install

Rust 1.92 or later, and `ohp` on the `PATH` to open PDFs:

```sh
cargo install paperstack ohp
```

or `cargo install --path .` in a clone of this repository. Either installs
`paperstack` and `pst`, its short name: `pst find Halvorsen` is `paperstack find
Halvorsen`.

Pure Rust: nothing in C is compiled, as for ohp. HTTPS is rustls with
RustCrypto's algorithms, which are not yet audited.

## Use

```sh
paperstack init --demo                 # a library, with papers to try
paperstack                             # browse it
paperstack find perturbation 'author:Halvorsen'
paperstack find 'collection:"Single Cell Data" -label:benchmark' --sort year --bib > refs.bib
paperstack get Brandt-Okafor2025-pb --json
```

The library is `--library`, else `$PAPERSTACK_LIBRARY`, else one made
already in one of these, else the first of them there is:
`~/Dropbox/paperstack`, if `~/Dropbox` is there; Dropbox's folder in
`~/Library/CloudStorage`, as recent macOS has it; `~/paperstack`. No Dropbox
folder is made where there is none.

## Moving from another reference manager

1. Export the library as **JSON**: every reference, with its citation key,
   folders, labels and star.
2. For the PDFs: download them all as a **ZIP**, and unzip it. A Google
   Drive folder the library syncs its PDFs to will do as well.
3. Then:

```sh
paperstack import ~/Downloads/library-export.json --dry-run   # what it would do
paperstack import ~/Downloads/library-export.json --pdfs ~/Downloads/library-pdfs
```

Or press `a` in the browser, and drop the `.json` on it: the same, the PDFs
looked for beside it, in Downloads and in Google Drive. Nothing is fetched,
so 6,000 references take a second or so.

The library's citation keys are kept as they are, so what cites them still
does. Its folders, subfolders as `Research/ML/NLP`, become paperstack's collections;
its labels, labels; papers in its trash are left. A PDF is found by the
name the export gives it, in the unzipped folder, accents however the zip
stored them, or by its key. A paper already here, by its key, DOI or title,
is given the export's folders and labels, and its other key as an
alias, so either key finds it; importing again adds nothing twice.

## Adding papers

```sh
paperstack add 10.5555/demo.2026.01                 # a DOI
paperstack add arXiv:2412.99901 'PMID: 99999901'    # an arXiv id, a PMID, a PMCID
paperstack add https://onlinelibrary.wiley.com/doi/full/10.1002/fict.0001
paperstack add ~/Downloads/paper.pdf --collection mediation
paperstack add 'A paper title, in full'             # a title, if just one paper has it
paperstack inbox                                    # file what was saved to inbox/
echo 10.5555/demo.2016.03 | paperstack add -        # one a line, as an agent would
```

`a` in the browser does the same, into the folder or label picked on the
left, with what is on the clipboard filled in.

Publishers' pages mostly turn away programs that are not browsers, so
paperstack reads what it needs from the registries instead: the paper's
record from doi.org, arXiv or PubMed, and from OpenAlex an abstract the
record lacks and where open copies of the PDF are. Most publishers'
addresses carry the DOI, or an id that leads to it, as ScienceDirect's PII;
a page that does not is read for its `citation_doi`, if it can be. A
preprint since published is filed as what it was published as, its arXiv
id kept. A PDF behind a paywall is downloaded in the browser, as before,
and saved to `inbox/`.

A PDF is told by the DOI in its metadata or on its first page, or arXiv's
stamp down its margin, taken only where the title it leads to is on the
page, with one of its authors: first pages name other papers' DOIs too.
Without one, the first lines are searched for in Crossref, OpenAlex and
arXiv, and the same check made. A first page that holds more than one
paper, as a journal's page of letters, is not guessed at: in a terminal
paperstack asks which, and the TUI shows them to pick from; otherwise it
lists them and adds none, until one is added by its id. Given a title
alone, paperstack adds the paper only if one paper has just that title:
registries hold copies of famous papers reposted under other DOIs, so where
several do it lists them, and adds the one then given by its id.

Registries are asked as paperstack, with an address to write to, as they
ask: `--email`, `$PAPERSTACK_EMAIL`, or git's `user.email`.

| Key | |
|---|---|
| `j` / `k`, `↑` / `↓` | move; on the left, change shelf |
| `ctrl-n` / `ctrl-p`, `ctrl-v` / `alt-v`, `ctrl-f` / `ctrl-b` | move as Emacs does: down and up, by five, across; `ctrl-v` still pastes while typing |
| `g` / `G`, `home` / `end`, `ctrl-↑` / `ctrl-↓`, `alt-<` / `alt->` | the first paper, the last |
| `h` / `l`, `tab` | switch between shelves and papers |
| `/` | search, as below; `esc` clears |
| `S` | sort by relevance, date added, year, first author or title, as shown at the top right; click it too |
| `⏎`, `o` | open the PDF in ohp; quit ohp to come back |
| `space` | mark a paper; `*` marks all shown, or unmarks them; `esc` unmarks all |
| `ctrl-e` | BibTeX of the papers marked, or the one picked; `s` there sets them out as APA, Chicago (author-date, notes), MLA, IEEE or Nature has them instead, and `y` copies what is shown |
| `y` | copy `[@key; @key]` to the clipboard |
| `s` | star, written to the paper's record |
| `d`, `del` | delete the papers marked, or the one picked; on the left, the collection or label picked; asked first |
| `p` | get the PDF of a paper without one: an open copy, else in the browser |
| `f` | locate a PDF on disk for the paper picked, where it has none; `ctrl-l` in the get-PDF dialog does the same |
| `x` | delete the PDF of the papers marked, or the one picked; the papers stay; asked first |
| `a` | add a paper, as `paperstack add` does |
| `r` | read again papers added meanwhile, as by `paperstack serve` |
| `m`, `⌘G` | the map of themes, and back to the list, as the ◆ Map button top left does; `⌘G` (super+g, the Windows key on Linux) while typing a search too, in a terminal that passes it on, as kitty does |
| `n` | on a theme's shelf, name the theme |
| `L` | label the papers marked, else all shown |
| `esc`, `ctrl-g` | clear the search; `esc` again, papers sent from the map among all of their shelf; again, unmark all; either closes a dialog, as `ctrl-g` cancels in Emacs |
| `?`, `F1` | the keys, of the list or of the map; `F1` while typing a search too |
| `<`, `>` | the pane of the paper picked, on the right, wider or narrower; or drag its edge |
| `q` | quit |

The mouse works too: click a shelf or a paper to pick it, its box to mark
it, its star to star it, its PDF badge to get a PDF or to delete the one it
has, or a paper twice to open it; the wheel moves through the list under it.

## Deleting

```sh
paperstack delete Brandt-Okafor2025-pb Nakamura2016-al     # papers, by key; or pst rm
paperstack delete --pdf Brandt-Okafor2025-pb            # only its PDF; the paper stays
paperstack delete --collection "Single Cell Data"      # a collection; its papers stay
paperstack delete --label to-read
```

A paper deleted is not erased: its record and PDF are moved into `.trash/`,
at the library's root, filed there as they were, and are no longer found.
`paperstack restore KEY` brings it back, and `paperstack restore --pdf KEY`
a paper's PDF deleted or replaced, as a preprint's by the published one;
empty `.trash/` to be rid of them. Dropbox keeps what is deleted from it
besides.
A PDF deleted alone is moved there the same way, and the paper stays, so
another file can be located for it. A collection deleted is taken off the
papers in it, as are the collections within it, as `Research/ML/NLP` is
within `Research/ML`; the papers stay in the library. In the browser, `d`
asks first, and `x` before a PDF is deleted.

## Correcting records

```sh
paperstack refresh Brandt-Okafor2025-pb --dry-run   # what doi.org, or arXiv, says now
paperstack refresh --query 'collection:"grant_demo"'
paperstack set Brandt-Okafor2025-pb page=R89-R98 'title={Memory Mosaics}'
paperstack tidy --dry-run                           # titles a BibTeX import braced whole
```

`refresh` reads a paper again by its DOI or arXiv id, as itself, a preprint
not as what it was published as, keeping its key, collections, labels,
star and PDF, the fields the source does not give, and those set by hand.
`set` sets a record's fields by hand, as CSL names them; a field given no
value is taken away, and stays away; `set KEY --reset FIELD` gives a field
back to the source, for the next refresh. A title braced, `{Memory Mosaics}`, keeps its case in
BibTeX. `tidy` takes off the braces another reference manager put around
whole titles of papers imported before this release, but not of a title
set by hand, so citeproc may set them in the style's case; papers imported
now come in without them.

## From the browser

PDFs behind a paywall, or a publisher's bot check, are had only in the
browser, signed in as you.

A paper here without its PDF shows **get PDF**: click it, or press `p`, and
an open copy is looked for first, as `add` does: the open publisher's own,
then OpenAlex's, a preprint's among them, then arXiv's. It is filed if its
first page has the paper's title. Where there is none, its page opens in the
browser, by its DOI, saying why. A popup shows each step meanwhile; `b`
goes to the browser at once, `esc` stops. Download the PDF there, as you
would, and it is filed beside the paper as soon as it lands in
`~/Downloads`, if its first page has the paper's title; anything else
downloaded meanwhile is left alone. Or paste the PDF's path, or its address,
into the dialog, or press `ctrl-l` to locate a file on disk. From the list,
`f` opens the same file dialog for the paper picked. A scan, with too little
text to show its title, is taken from Downloads only with the paper's DOI in
it; given by its path, or located on disk, it is taken as you say. From the
shell, the same:

```sh
paperstack attach Brandt-Okafor2025-pb                 # an open copy, else open its page, await the download
paperstack attach Brandt-Okafor2025-pb ~/Downloads/paper.pdf
paperstack attach --query 'collection:"grant_demo"'    # open copies for each paper there without a PDF
paperstack attach KEY1 KEY2 --published --json         # no preprints; a preprint's PDF replaced
```

Open copies are tried published versions first, then accepted manuscripts,
preprints last; the version taken is kept in the record, as `pdf_version`.
A server that asks for fewer requests, as bioRxiv's, is asked again after a
while. For more than one paper, or with `--no-browser`, no page is opened in
the browser, unless `--open` asks: then the pages of the papers no open copy
was had of are opened, and each PDF downloaded there is filed with the
paper it is. Each paper's outcome is said, and with `--json`, why none was
had, with each copy tried and its reason (`http-429`, `http-403`,
`html-page`, `not-the-paper`, …), and the paper's page to open. Its
`reason` is `blocked` where a copy was turned away, as by a paywall or a bot
check, so the browser may get it there; else `no-open-copy`. `--published` takes no preprint, and gives
a paper whose PDF is a preprint's the published version in its place, the
preprint's moved to `.trash/`.

| | |
|---|---|
| `PAPERSTACK_PROXY` | your library's proxy, put before the page's address: `https://ezproxy.example.edu/login?url=` |
| `PAPERSTACK_BROWSER` | the browser to open, as `open -a "Google Chrome"`, where the extension is; `none` to only show the address |
| `PAPERSTACK_DOWNLOADS` | where the browser saves downloads, if not `~/Downloads` |

Two ways bring in PDFs of papers not yet here, or many at once:

```sh
paperstack watch ~/Downloads        # file PDFs as they come into inbox/ and ~/Downloads
paperstack serve --watch ~/Downloads   # the same, and the browser extension's papers
```

Only PDFs that come into a folder after it is watched are filed, but in the
inbox, and a PDF filed is taken out of the inbox; out of other folders only
with `--take`.

`serve` listens at `127.0.0.1:8765` for the extension in `extension/`,
which is loaded into Chrome from this checkout, not the Web Store:

1. `chrome://extensions`, turn on Developer mode, Load unpacked, pick the
   `extension` folder.
2. `paperstack serve --print-token`, and paste what it prints into the
   extension's options (its Details, Extension options). Its button opens
   them by itself while it has none.

Its button, on a paper's page, reads the page's DOI and `citation_*` tags,
fetches the PDF there, with your institution's access, and sends both to
`serve`, which files the paper if the PDF is the page's paper, or says it is
not, as a supplement would be. A scan, without the text or the DOI to tell
it by, is not kept: the paper is filed without it, to be given it by its
path. The badge says how it went: `✓`, `✓·` without a PDF, `!` and a
notification of why not.

Who may add papers:

- Only a program with the token. It is made the first time `serve` runs,
  and kept where only you can read it, in
  `~/Library/Application Support/paperstack/token` on macOS,
  `~/.config/paperstack/token` elsewhere: another account on this machine,
  which can reach its port, cannot read the token. Delete the file to have a
  new one made, and paste that into the extension.
- Only this machine: a page whose address is made to point here, as by DNS
  rebinding, is refused.
- Of browsers, only paperstack's extension, whose id the `key` in its
  manifest fixes as `kckhjncnkhapnmnjlpjefjamffaecmcf` wherever it is
  loaded from; others are let in with `--extension ID`, or with their origin
  whole, as `--extension moz-extension://ID`. No page can add papers.

The extension sends your sign-ins, its cookies, only to the page's own site,
as `nature.com` of `www.nature.com`; a PDF elsewhere is fetched without them,
and one on this machine or its network not at all, unless the page is there
too. A page cannot have the extension fetch what only you may see. Requests
are read by a few threads, so a slow one keeps no other waiting.

To have `serve` run from login on macOS, a LaunchAgent:

```sh
cat > ~/Library/LaunchAgents/paperstack.serve.plist <<PLIST
<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0"><dict>
  <key>Label</key><string>paperstack.serve</string>
  <key>ProgramArguments</key><array>
    <string>$HOME/.cargo/bin/paperstack</string><string>serve</string>
    <string>--watch</string><string>$HOME/Downloads</string>
  </array>
  <key>RunAtLoad</key><true/><key>KeepAlive</key><true/>
  <key>StandardOutPath</key><string>/tmp/paperstack.log</string>
  <key>StandardErrorPath</key><string>/tmp/paperstack.log</string>
</dict></plist>
PLIST
launchctl load ~/Library/LaunchAgents/paperstack.serve.plist
```

## For AI agents: MCP

`paperstack mcp` serves the library to an AI agent over the Model Context
Protocol (MCP), on stdin and stdout. The agent's program starts it when it
needs it: there is no server to keep running.

### Setting it up

1. **Install paperstack**, as above, and find where it went:

   ```sh
   which paperstack          # ~/.cargo/bin/paperstack after cargo install
   ```

   Programs opened from the Dock may not have your shell's `PATH`, so give
   them this whole path, as below.

2. **Check the library** it will serve:

   ```sh
   paperstack find '' --keys        # the keys of every paper in it
   ```

   This is the library at `--library`, `$PAPERSTACK_LIBRARY` or the default
   (see [Use](#use)), and the one the agent gets. For another, set
   `PAPERSTACK_LIBRARY` in the client's settings, as below.

3. **Tell your agent's program about it.**

   **Claude Code**, for all your projects:

   ```sh
   claude mcp add --scope user paperstack -- ~/.cargo/bin/paperstack mcp
   ```

   Without `--scope user` it is for the folder it was added in only. For
   another library, put `--env PAPERSTACK_LIBRARY=/path/to/library` before
   the `--`.

   **Cursor**: in `~/.cursor/mcp.json`, for all your projects, or a
   project's `.cursor/mcp.json`, add `paperstack` to `mcpServers`, keeping
   any servers there already:

   ```json
   {
     "mcpServers": {
       "paperstack": {
         "command": "/Users/you/.cargo/bin/paperstack",
         "args": ["mcp"]
       }
     }
   }
   ```

   **Claude Desktop**: the same JSON, in
   `~/Library/Application Support/Claude/claude_desktop_config.json`
   (Settings → Developer → Edit Config).

   **Others**: a stdio server, its command `paperstack` and its arguments
   `mcp`.

   For another library, in the JSON, add
   `"env": { "PAPERSTACK_LIBRARY": "/path/to/library" }` beside `"args"`.
   JSON takes no `~`: write out the whole path.

4. **Restart the program, and see that it is connected.** In Claude Code,
   `claude mcp list` or `/mcp` in a session shows `paperstack` connected. In
   Cursor, Cursor Settings → Tools & MCP (MCP in older versions) shows it
   with its ten tools; switch it on if it is off. In Claude Desktop, it is under the tools button in a chat.

5. **Ask for papers**, as: "find the papers in my library on single-cell
   foundation models, and give me their BibTeX".

### If it does not connect

- *Command not found*: give the whole path from `which paperstack`.
- *No `mcp` command*: the `paperstack` installed is older; install it again.
- *The wrong papers, or none*: the program sees another library. Ask
  paperstack itself what it serves, as the program would run it:

  ```sh
  echo '{"jsonrpc":"2.0","id":1,"method":"tools/call","params":{"name":"list_collections","arguments":{}}}' \
    | ~/.cargo/bin/paperstack mcp
  ```

  It answers with how many papers it holds. Set `PAPERSTACK_LIBRARY` in the
  program's settings if they are not yours.
- To see every tool and try it: `npx @modelcontextprotocol/inspector
  ~/.cargo/bin/paperstack mcp`.

To remove it: `claude mcp remove paperstack -s user`, or delete its entry
from the JSON.

### Its tools

| Tool | | |
|---|---|---|
| `search_papers` | find papers, by the query language below | reads |
| `get_paper` | a paper's metadata, abstract and BibTeX, by its key | reads |
| `cite` | BibTeX, CSL JSON, `[@a; @b]`, or references in a citation style (APA unless another is named), of keys given, or of a query; or written to a `.bib` | writes |
| `check_keys` | which keys a manuscript cites are here, merged into another's, or missing | reads |
| `read_paper` | the text of a paper's PDF, 20 pages at a time | reads |
| `list_collections` | collections and labels, and how many papers each holds | reads |
| `add_paper` | add by DOI, arXiv id, PMID, address, PDF or title | adds |
| `attach_pdf` | open copies for papers by keys or a query, or the PDF at a path, if it is that paper's | adds |
| `update_paper` | collections, labels, star and older keys, of a paper or many; one paper's fields | changes |
| `refresh_papers` | read papers again from doi.org or arXiv, keeping what is ours of them | changes |
| `restore_papers` | bring papers, or a paper's PDF, back from `.trash/` | adds |
| `delete_papers` | move papers to the library's `.trash/` | deletes |
| `delete_collection` | take a collection or label off its papers, which stay | deletes |

`search_papers`, `get_paper`, `check_keys`, `read_paper` and
`list_collections` are marked read-only, so a program may let the agent use
them without asking; it asks before the other eight, and the delete tools
are marked destructive. `cite` lists the keys it has no paper for, and cites the
rest; a key merged into another paper's is cited under the key given. The library's `AGENTS.md`,
which tells an agent how the library is laid out, is a resource too.

## Themes

`m`, `⌘G`, or the ◆ Map button top left, shows the library as a map:
papers near one another where their words are alike or their authors
shared, gathered into themes, each named by the words that set it apart;
`m` or `esc` again shows the list. `←` / `→`, `h` / `l`, a click on a
theme's name, or picking its shelf under THEMES, picks a theme; its papers are listed on its shelf, where search narrows them
further. `c` colours the picked theme alone, or every theme again;
`1` to `9` pick those themes; `?` or `F1` on the map lists its keys,
scrolling where they do not fit. `/` searches here, what it finds lit on the map as it is typed, and `a`
adds a paper, as in the list; `q` quits.

`⏎`, or a theme's name clicked twice, goes into it: a map of its papers
alone, laid out by their links among themselves and gathered in their own
finer themes, each paper's strongest links drawn beneath. Within, `←` /
`→` go into the next theme, a click picks a paper and says its title,
twice opens it in ohp, and `⏎` shows the theme's card: its words, its
authors and its papers, newest first, `⏎` opening one, `p` getting its
PDF, the card shown again after, `l` listing them.
`esc`, or `ctrl-g`, comes out to the whole map. Fewer than ten papers in
view, a theme's or a finer theme's, `⏎` or a double click lists them to
read instead, the paper clicked picked, a finer theme's alone on its
theme's shelf till `esc`. `>` and `<` show the map closer to the
theme picked, and further; `shift-←→↑↓`, or the wheel sideways or with
shift, move what is shown closer.

The map is shown first as a mosaic: each paper a cell of its own, none
hiding another, and each theme a block, boxed, beside the themes it lies
by, as recursive bisection partitions a graph: the themes split in two by
where they lie as laid out, the cells in two as many as each half has
papers, and again, till each theme has its block, then its papers alike.
Within a theme, its finer themes are blocks. `g` shows the map as laid
out, each paper a dot near those alike, and again the mosaic. A theme picked on the left is picked on the
map too.

`t` colours each theme by the papers added to it lately, from grey,
where none came, to orange, where most did: the topics you read now
stand out. Lately is the last 6 months at first; `[` and `]` make it
shorter and longer, from a month to 5 years. `t` again, or `c`, the colours as
before.

A search typed in the list shows on the map too: the papers it finds,
those new since among them, as dots over the rest in grey, with how many
above the map. Each key in the footer, clicked, is as if pressed.

```sh
paperstack themes                 # the words most papers have, and each theme's
paperstack themes --json          # with each theme's keys, for an agent; numbered from 1, as shown
paperstack themes --split 3       # theme 3 in finer themes of its own
paperstack themes --resolution 2  # every theme finer, the papers where they are
paperstack themes --rebuild       # gather the themes again from all papers
paperstack themes --rebuild --fresh  # and lay the map out anew
paperstack themes --undo          # the themes before, shown again
paperstack themes --name 3 "VAEs and flows"  # a name of your own for theme 3
paperstack themes --label 3 vae   # the label vae on each of theme 3's papers
```

Each paper's title, abstract and keywords, with pairs of words as "single
cell", are weighed by TF-IDF; its nearest by cosine, weighed as UMAP weighs
them, and papers sharing authors, but not through a consortium's hundreds,
make a graph. Its communities, by Louvain's moves, are the themes; UMAP lays
it out. Five thousand papers take a second or two.

A theme too broad, as one of all machine learning, splits in finer themes
of its own: `s`, or `--split N`, gathers its papers again, each by its
nearest within the theme, the rest of the map as it was. `+` and `-`
gather every theme again finer or coarser, by Louvain's resolution, 1 at
first, in steps of half again; `--resolution` gives one. Neither moves a
paper on the map; `R`, or `--rebuild`, gathers the themes again from all
papers, and lays them out from where they are, so the map keeps its
shape and its size: a paper new since starts among its nearest, as the
crosses show it, a theme all new off at the map's edge, and where most
papers are new, the map is laid out anew. `F`, or `--rebuild --fresh`,
lays it out anew whenever asked.

`n`, on the map or on a theme's shelf, names a theme as you would: the
name follows its papers when the themes are gathered again, as long as
they stay together, at least half of them. `L` puts a label on the
picked theme's papers, in their records, synced as any label; in the list,
on the papers marked, else on all shown; for 200 papers or more, the
label is typed out, not offered. A label put on too many is taken off
again by `d` on it under LABELS.

The map is kept on this machine, beside the index, with the one before
the last `s`, `+`, `-`, `R`, `F` or `A`, which `u`, or `--undo`, shows
again.
Papers added since go to the theme their nearest papers have most, but
only when asked: the map shows them as crosses, `N` lists where each
would go, and `A`, or `paperstack themes --assign`, puts them there.

## Searching

The TUI's `/` and `paperstack find` read the same query:

```
perturbation "linear baseline" author:Halvorsen year:2020..2026 collection:"Single Cell Data" -label:benchmark has:pdf
```

Words, and phrases in quotes, are looked for in the title, abstract,
authors, venue, keywords, key and DOI, title first; a word matches the words
it starts, and accents need not be typed. `title:`, `abstract:`, `author:`
and `venue:` look in one of those; `year:` takes a year or a range,
`2020..2026`, `2020..` or `..2020`; `collection:` (or `folder:`), `label:`, `key:` and `doi:`
match whole; `has:pdf`, `has:abstract`, `has:doi` and `is:starred` say what a
paper has. A `-` before any of them leaves out what it matches.

`find` lists the papers found best first, or `--sort year|added|author|title`,
`--reverse`, `--limit N`, as a line each, or, with `--output refs.bib` (or
`refs.json`), writes the bibliography there; with `--keep`, the file is kept
up to date as the library changes, whenever paperstack runs on this
machine, but never cut short, as where a key it cites is gone.
`paperstack kept` lists those kept, and `--forget FILE` stops one. Or:

| | |
|---|---|
| `--bib` | BibTeX, plain, as reference managers write it; the capitals of a title that are its authors' braced, so citeproc keeps them, and HTML in it as LaTeX |
| `--csl` | CSL JSON, for pandoc's and ohp's `bibliography:` |
| `--keys` | `[@a; @b]`, to cite them |
| `--json` | key, title, authors, year, ids, folders, the files' paths: for agents |
| `--paths` | the PDFs |
| `--style apa` | references in a citation style: `apa`, `chicago-author-date`, `chicago-notes`, `mla`, `ieee`, `nature`, any other of the CSL styles [hayagriva](https://github.com/typst/hayagriva) keeps, by name, or a `.csl` file; `--markup markdown` or `html` keeps their italics |

## The library

Each paper is filed by year and first author, its PDF and its record side
by side:

```
2025/Brandt-Okafor/C/
  Brandt-Okafor2025-pb_neural-gene-perturbation-effect-models-are.pdf
  Brandt-Okafor2025-pb_neural-gene-perturbation-effect-models-are.json
```

The first author's given name names the third folder, or the initials where
only they are known. The file starts with the citation key, of the form
`Surname2025-xy`, which never changes, so `find . -name 'KEY_*'` finds a
paper. The record is CSL JSON, as pandoc and ohp read it, under `csl`, with
the paper's folders, labels and star under `paperstack`. `AGENTS.md`, at the
root, tells an agent how to find papers and cite them.

The index searched is the records, read and kept in memory with the words
of their text, ranked by BM25: a search among 6,000 papers takes a few
milliseconds. Between runs it is cached on the machine, in
`~/Library/Caches/paperstack` (`~/.cache` elsewhere), not in Dropbox, and is
brought up to the records whenever paperstack starts, reading only those
changed, in about a tenth of a second. A database synced between machines
can be corrupted, while a record can at worst be a conflicted copy of
itself, which paperstack names rather than reads.

## License

MIT; see [LICENSE](LICENSE).
