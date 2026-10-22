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
paperstack add 10.1038/s41586-020-2649-2            # a DOI
paperstack add arXiv:2412.08821 'PMID: 28693043'    # an arXiv id, a PMID, a PMCID
paperstack add https://onlinelibrary.wiley.com/doi/full/10.1002/sim.7400
paperstack add ~/Downloads/paper.pdf --collection mediation
paperstack add 'Array programming with NumPy'       # a title, if just one paper has it
paperstack inbox                                    # file what was saved to inbox/
echo 10.7554/eLife.10047 | paperstack add -         # one a line, as an agent would
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
| `j` / `k`, arrows | move; on the left, change shelf |
| `h` / `l`, `tab` | switch between shelves and papers |
| `/` | search, as below; `esc` clears |
| `S` | sort by relevance, date added, year, first author or title |
| `enter`, `o` | open the PDF in ohp; quit ohp to come back |
| `space` | mark a paper; `*` marks all shown, or unmarks them; `esc` unmarks all |
| `ctrl-e` | BibTeX of the papers marked, or the one picked |
| `y` | copy `[@key; @key]` to the clipboard |
| `s` | star, written to the paper's record |
| `d` | delete the papers marked, or the one picked; on the left, the collection or label picked; asked first |
| `p` | get the PDF of a paper without one, in the browser |
| `f` | locate a PDF on disk for the paper picked, where it has none |
| `x` | delete the PDF of the papers marked, or the one picked; the papers stay; asked first |
| `a` | add a paper, as `paperstack add` does |
| `r` | read again papers added meanwhile, as by `paperstack serve` |
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
Move them back to where they were to have the paper again, or empty
`.trash/` to be rid of them; Dropbox keeps what is deleted from it besides.
A PDF deleted alone is moved there the same way, and the paper stays, so
another file can be located for it. A collection deleted is taken off the
papers in it, as are the collections within it, as `Research/ML/NLP` is
within `Research/ML`; the papers stay in the library. In the browser, `d`
asks first, and `x` before a PDF is deleted.

## From the browser

PDFs behind a paywall, or a publisher's bot check, are had only in the
browser, signed in as you.

A paper here without its PDF shows **get PDF**: click it, or press `p`, and
its page opens in the browser, by its DOI. Download the PDF there, as you
would, and it is filed beside the paper as soon as it lands in
`~/Downloads`, if its first page has the paper's title; anything else
downloaded meanwhile is left alone. Or paste the PDF's path, or its address,
into the dialog, or press `ctrl-l` to locate a file on disk. From the list,
`f` opens the same file dialog for the paper picked. A scan, with too little
text to show its title, is taken from Downloads only with the paper's DOI in
it; given by its path, or located on disk, it is taken as you say. From the
shell, the same:

```sh
paperstack attach Brandt-Okafor2025-pb                 # open its page, await the download
paperstack attach Brandt-Okafor2025-pb ~/Downloads/paper.pdf
```

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
| `cite` | BibTeX, CSL JSON or `[@a; @b]` of keys given, or of a query | reads |
| `read_paper` | the text of a paper's PDF, 20 pages at a time | reads |
| `list_collections` | collections and labels, and how many papers each holds | reads |
| `add_paper` | add by DOI, arXiv id, PMID, address, PDF or title | adds |
| `attach_pdf` | give a paper the PDF at a path, if it is that paper's | adds |
| `update_paper` | its collections, labels and star | changes |
| `delete_papers` | move papers to the library's `.trash/` | deletes |
| `delete_collection` | take a collection or label off its papers, which stay | deletes |

The first five are marked read-only, so a program may let the agent use them
without asking; it asks before the other five, and the last two are marked
destructive. The library's `AGENTS.md`,
which tells an agent how the library is laid out, is a resource too.

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
`--reverse`, `--limit N`, as a line each, or:

| | |
|---|---|
| `--bib` | BibTeX, plain, as reference managers write it |
| `--csl` | CSL JSON, for pandoc's and ohp's `bibliography:` |
| `--keys` | `[@a; @b]`, to cite them |
| `--json` | key, title, authors, year, ids, folders, the files' paths: for agents |
| `--paths` | the PDFs |

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
