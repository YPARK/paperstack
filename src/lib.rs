//! paperstack: a library of papers in the terminal, kept in a folder, as in
//! Dropbox, with each PDF and its record under `year/lastname/firstname/`,
//! searched through an index kept on this machine. PDFs are presented with
//! ohp.

mod add;
mod agents;
mod app;
mod bibtex;
mod browser;
mod cli;
mod daemon;
mod demo;
mod index;
mod keymap;
mod layout;
mod library;
mod map;
mod mcp;
mod migrate;
mod net;
mod pdf;
mod query;
mod record;
mod remove;
mod resolve;
mod source;
mod theme;
mod ui;

use clap::{Parser, Subcommand};
use index::{Index, Sort};
use library::Library;
use query::Query;
use std::path::{Path, PathBuf};

#[derive(Parser)]
#[command(version, about)]
struct Args {
    /// The library: a folder, in Dropbox if Dropbox is here, else ~/paperstack
    #[arg(
        long,
        short = 'L',
        env = "PAPERSTACK_LIBRARY",
        default_value_os_t = default_root(),
        global = true
    )]
    library: PathBuf,
    /// Given to Crossref, OpenAlex and NCBI, who serve first those who give one;
    /// git's user.email without it
    #[arg(long, env = "PAPERSTACK_EMAIL", global = true)]
    email: Option<String>,
    /// The TUI's colours: a light page, or the terminal's own dark one
    #[arg(
        long,
        env = "PAPERSTACK_THEME",
        value_enum,
        default_value_t,
        global = true
    )]
    theme: theme::Name,
    #[command(subcommand)]
    command: Option<Command>,
}

#[derive(Subcommand)]
enum Command {
    /// Make a library at --library, with AGENTS.md and an inbox
    Init {
        /// Add papers to try it with
        #[arg(long)]
        demo: bool,
    },
    /// Bring the index up to the records, and say what could not be read
    Index,
    /// The library's themes: papers gathered by their words and shared
    /// authors, each theme named by the words that set it apart; papers
    /// new since put in the themes of their nearest, once asked
    Themes {
        #[arg(long)]
        json: bool,
        /// Put the new papers in the themes said
        #[arg(long, group = "action")]
        assign: bool,
        /// Gather the themes again from all papers, keeping these as the
        /// previous; laid out from where the papers are, so the map keeps
        /// its shape
        #[arg(long, group = "action")]
        rebuild: bool,
        /// With `--rebuild`, the map laid out anew
        #[arg(long, requires = "rebuild")]
        fresh: bool,
        /// Show the previous themes again, keeping these as the previous
        #[arg(long, group = "action")]
        undo: bool,
        /// Split theme N, as numbered here, in finer themes of its own,
        /// the rest as they are, keeping these as the previous
        #[arg(long, group = "action", value_name = "N", value_parser = theme_number)]
        split: Option<usize>,
        /// Name theme N, as numbered here; a blank name gives it back its
        /// own words. The name follows its papers when the themes are
        /// gathered again
        #[arg(long, group = "action", num_args = 2, value_names = ["N", "NAME"])]
        name: Option<Vec<String>>,
        /// Put a label on every paper of theme N, in its record: NAME, else
        /// the theme's name. `paperstack remove --label` takes it off
        #[arg(long, group = "action", num_args = 1..=2, value_names = ["N", "NAME"])]
        label: Option<Vec<String>>,
        /// How fine the themes are: higher, more and smaller. Alone, the
        /// themes gathered again at R, the papers where they are; with
        /// `--rebuild`, the map's own unless given; with `--split`, 1
        #[arg(long, value_name = "R", conflicts_with_all = ["assign", "undo", "name", "label"])]
        resolution: Option<f64>,
    },
    /// Find papers: words, "phrases", author:, year:2020..2026, folder:, has:pdf, -...
    Find {
        query: Vec<String>,
        /// Words to leave out, as `-has:pdf` is read: by `args`
        #[arg(long = "not", hide = true)]
        not: Vec<String>,
        #[arg(long, value_enum, default_value_t)]
        sort: Sort,
        #[arg(long)]
        reverse: bool,
        #[arg(long)]
        limit: Option<usize>,
        #[command(flatten)]
        format: cli::Format,
    },
    /// One paper, by its key or a key merged into it
    Get {
        key: String,
        #[command(flatten)]
        format: cli::Format,
    },
    /// Add papers: DOIs, arXiv ids, PMIDs, PMCIDs, publishers' addresses,
    /// PDFs or titles; `-` reads them from stdin, one a line
    Add {
        #[arg(required = true)]
        what: Vec<String>,
        /// Put them in this collection too
        #[arg(long, visible_alias = "collection")]
        folder: Vec<String>,
        #[arg(long)]
        label: Vec<String>,
        /// Do not download an open copy of the PDF
        #[arg(long)]
        no_pdf: bool,
        /// What was added, as JSON
        #[arg(long)]
        json: bool,
    },
    /// Give papers their PDFs: open copies, downloaded, where there are
    /// some, published versions first; else, for one paper behind a
    /// paywall, its page opened in the browser, the PDF downloaded there
    /// awaited. Or one paper a PDF's path or address: `attach KEY FILE`
    Attach {
        /// The papers' keys; or a key and a PDF's path or address
        #[arg(required_unless_present = "query")]
        what: Vec<String>,
        /// The papers this finds, as collection:"name", that have no PDF
        /// (or, with --published, a preprint's)
        #[arg(long, short)]
        query: Option<String>,
        /// Open copies only: no page opened in the browser. So it is for
        /// more than one paper
        #[arg(long)]
        no_browser: bool,
        /// No preprint: a paper whose PDF is a preprint's is given the
        /// published version in its place, the preprint's to the trash
        #[arg(long)]
        published: bool,
        /// How long to wait for the download, in minutes
        #[arg(long, default_value_t = 15)]
        wait: u64,
        #[arg(long)]
        json: bool,
    },
    /// Move a library from another reference manager: its JSON export, its
    /// keys kept, with its folders, labels and stars; and its PDFs,
    /// downloaded as a ZIP and unzipped, or the Google Drive folder it syncs
    Import {
        export: PathBuf,
        /// Where the library's PDFs are; else looked for beside the export,
        /// in Downloads, and in Google Drive
        #[arg(long)]
        pdfs: Option<PathBuf>,
        /// Say what would be done, and do nothing
        #[arg(long)]
        dry_run: bool,
        #[arg(long)]
        json: bool,
    },
    /// Delete papers, by their keys, into the library's .trash/; or only
    /// their PDFs, with --pdf, the papers staying; or take a collection or
    /// label off its papers, which stay
    #[command(visible_alias = "rm")]
    Delete {
        #[arg(required_unless_present_any = ["collection", "label"])]
        keys: Vec<String>,
        /// Delete this collection, and those within it, as name/sub
        #[arg(long, visible_alias = "folder")]
        collection: Vec<String>,
        /// Delete this label
        #[arg(long)]
        label: Vec<String>,
        /// Delete only these papers' PDFs, into .trash/; the papers stay
        #[arg(long, conflicts_with_all = ["collection", "label"])]
        pdf: bool,
    },
    /// Serve the library to an AI agent over MCP, on stdin and stdout: its
    /// program, as Cursor or Claude Code, runs `paperstack mcp`
    Mcp,
    /// File the PDFs in the library's inbox/, and take out those filed
    Inbox {
        #[arg(long)]
        json: bool,
    },
    /// File PDFs as they come into inbox/, and into the folders given, as
    /// ~/Downloads; theirs from before are left alone
    Watch {
        dirs: Vec<PathBuf>,
        /// Take a PDF filed out of the folders given too, as out of inbox/
        #[arg(long)]
        take: bool,
        /// Put what is filed in this collection too
        #[arg(long, visible_alias = "collection")]
        folder: Vec<String>,
    },
    /// Watch as `watch` does, and take papers from the browser extension
    /// at 127.0.0.1, their PDFs fetched in the browser
    Serve {
        #[arg(long, default_value_t = daemon::PORT)]
        port: u16,
        /// Folders to watch besides inbox/, as ~/Downloads
        #[arg(long)]
        watch: Vec<PathBuf>,
        #[arg(long)]
        take: bool,
        #[arg(long, visible_alias = "collection")]
        folder: Vec<String>,
        /// Other extensions that may ask, besides paperstack's own: a Chrome
        /// extension's id, or an origin whole, as moz-extension://ID
        #[arg(long)]
        extension: Vec<String>,
        /// Print the token the extension is to send, for its options, and stop
        #[arg(long)]
        print_token: bool,
    },
    /// The TUI's screen after the keys given, as text: `/perturb <enter>`
    #[command(hide = true)]
    Screen {
        #[arg(long, default_value_t = 160)]
        width: u16,
        #[arg(long, default_value_t = 44)]
        height: u16,
        /// As an SVG, with its colours
        #[arg(long)]
        svg: bool,
        keys: Vec<String>,
    },
    /// What paperstack reads in a PDF to tell which paper it is
    #[command(hide = true)]
    Pdf {
        file: PathBuf,
        /// Say whether this title is the PDF's
        #[arg(long)]
        title: Option<String>,
    },
}

fn default_root() -> PathBuf {
    default_root_in(&PathBuf::from(std::env::var_os("HOME").unwrap_or_default()))
}

/// Where the library is, with `home` the home folder and nothing given:
/// in Dropbox, if Dropbox is there, as `~/Dropbox`, or, on recent macOS,
/// in `~/Library/CloudStorage`; else `~/paperstack`. A library already
/// made in any of them is kept to, as Dropbox installed after it would
/// otherwise move the default to an empty folder. No Dropbox folder is
/// made where there is none.
fn default_root_in(home: &Path) -> PathBuf {
    let mut places = Vec::new();
    if home.join("Dropbox").is_dir() {
        places.push(home.join("Dropbox/paperstack"));
    }
    if let Ok(clouds) = std::fs::read_dir(home.join("Library/CloudStorage")) {
        let mut dropboxes: Vec<PathBuf> = clouds
            .flatten()
            .filter(|c| c.file_name().to_string_lossy().starts_with("Dropbox"))
            .map(|c| c.path())
            .filter(|p| p.is_dir())
            .collect();
        dropboxes.sort();
        places.extend(dropboxes.into_iter().map(|d| d.join("paperstack")));
    }
    places.push(home.join("paperstack"));
    places
        .iter()
        .find(|p| p.join(library::MARKER).is_file())
        .unwrap_or(&places[0])
        .clone()
}

/// A theme's place in the map, from its number as `paperstack themes`
/// shows it, from 1.
fn theme_number(n: &str) -> anyhow::Result<usize> {
    let n: usize = anyhow::Context::context(n.parse(), "a theme's number")?;
    anyhow::Context::context(n.checked_sub(1), "themes are numbered from 1")
}

/// The library at `root`, and its index brought up to it; what could not
/// be read is said on stderr.
fn open(root: &Path) -> anyhow::Result<(Library, Index)> {
    let lib = Library::open(root)?;
    let mut index = Index::open(&index::default_path(&lib.root))?;
    report(&index.refresh(&lib)?);
    Ok((lib, index))
}

/// Say on stderr what a refresh could not read.
fn report(done: &index::Refresh) {
    for e in &done.errors {
        eprintln!("paperstack: {e}");
    }
    for c in &done.conflicts {
        eprintln!("paperstack: a conflicted copy, not read: {}", c.display());
    }
}

/// Watch `dirs` and the inbox, and serve the extension at `port` if one is
/// given, until stopped.
fn run_daemon(
    root: &Path,
    net: &net::Net,
    port: Option<u16>,
    dirs: &[PathBuf],
    take: bool,
    folders: Vec<String>,
    extensions: Vec<String>,
) -> anyhow::Result<()> {
    let (lib, mut index) = open(root)?;
    let watched = daemon::folders(&lib, dirs, take);
    daemon::Daemon {
        lib: &lib,
        index: &mut index,
        net,
        folders,
        origins: extensions.iter().map(|e| daemon::origin(e)).collect(),
    }
    .run(port, &watched)
}

/// Whether to ask on the terminal which paper a PDF is, where it holds
/// more than one: not where what is written is read by a program, nor
/// where stdin is not a person's, as with `add -`.
fn asks(json: bool) -> bool {
    use std::io::IsTerminal;
    !json && std::io::stdin().is_terminal()
}

/// The address given to registries: `--email`, else git's.
fn email(given: Option<String>) -> Option<String> {
    given.or_else(|| {
        let out = std::process::Command::new("git")
            .args(["config", "user.email"])
            .output()
            .ok()?;
        let e = String::from_utf8(out.stdout).ok()?.trim().to_string();
        e.contains('@').then_some(e)
    })
}

/// The program, as `paperstack` or `pst`.
/// Open the page of `hit` in the browser and wait `wait` minutes for its
/// PDF to be downloaded there: the first that is that paper's is filed.
fn await_download(
    lib: &Library,
    index: &mut Index,
    hit: &index::Hit,
    wait: u64,
) -> anyhow::Result<anyhow::Result<add::Added>> {
    let key = &hit.record.key;
    let page = browser::page(&hit.record);
    browser::open(&page)?;
    let mut arrivals = browser::Arrivals::new(browser::downloads());
    eprintln!(
        "opened {page}\ndownload its PDF there; waiting for it in {} …",
        browser::tilde(&arrivals.dir)
    );
    let until = std::time::Instant::now() + std::time::Duration::from_secs(wait * 60);
    while std::time::Instant::now() < until {
        std::thread::sleep(std::time::Duration::from_millis(500));
        for path in arrivals.next() {
            let from = browser::tilde(&path);
            match add::attach(lib, index, key, &std::fs::read(&path)?, &from, false) {
                Ok(added) => return Ok(Ok(added)),
                Err(e) => eprintln!("paperstack: {}", add::one_line(&e)),
            }
        }
    }
    Ok(Err(anyhow::anyhow!(
        "no PDF of {key} came in {wait} minutes"
    )))
}

/// The key and the PDF's path or address of `attach KEY FILE`: a second
/// word that is a path, by a slash or `.pdf` in it, or an address, as no
/// key is.
fn pdf_given(what: &[String]) -> Option<(&str, &str)> {
    match what {
        [key, given]
            if given.contains('/')
                || given.to_lowercase().ends_with(".pdf")
                || Path::new(given).exists() =>
        {
            Some((key, given))
        }
        _ => None,
    }
}

/// The command line, a word of `find` that leaves out, as `-has:pdf`,
/// given as `--not=has:pdf`: else it would be read as options, `-h` first.
/// What is an option, and which take a value, is as clap is told.
fn args<I: IntoIterator<Item = std::ffi::OsString>>(given: I) -> Vec<std::ffi::OsString> {
    use clap::CommandFactory;
    let command = Args::command();
    let find = command.find_subcommand("find").expect("a find command");
    let options: Vec<&clap::Arg> = command
        .get_arguments()
        .chain(find.get_arguments())
        .collect();
    let takes_value = |s: &str| {
        options.iter().any(|a| {
            a.get_action().takes_values()
                && (a.get_long().is_some_and(|l| s == format!("--{l}"))
                    || a.get_short().is_some_and(|c| s == format!("-{c}")))
        })
    };
    // `-h`, or a short option, its value joined to it or not.
    let is_short = |s: &str| {
        let c = s.chars().nth(1);
        (s == "-h")
            || options.iter().any(|a| {
                a.get_short().is_some()
                    && a.get_short() == c
                    && (s.len() == 2 || a.get_action().takes_values())
            })
    };
    let mut out = Vec::new();
    let mut finding = false;
    let mut value_next = false;
    for arg in given {
        let s = arg.to_string_lossy();
        let leaves_out = finding
            && !value_next
            && !s.starts_with("--")
            && s.starts_with('-')
            && s.chars().nth(1).is_some_and(char::is_alphabetic)
            && !is_short(&s);
        if leaves_out {
            out.push(format!("--not={}", &s[1..]).into());
            continue;
        }
        if !finding && !value_next && s == "find" && !out.is_empty() {
            finding = true;
        }
        value_next = !value_next && takes_value(&s);
        out.push(arg);
    }
    out
}

pub fn main() -> anyhow::Result<()> {
    let args = Args::parse_from(args(std::env::args_os()));
    let net = || net::Net::new(email(args.email.clone()));
    match args.command {
        None => app::run(open(&args.library)?, net(), theme::Theme::of(args.theme)),
        Some(Command::Add {
            what,
            folder,
            label,
            no_pdf,
            json,
        }) => {
            let (lib, mut index) = open(&args.library)?;
            let lines: Vec<String> = if what == ["-"] {
                std::io::stdin().lines().map_while(Result::ok).collect()
            } else {
                what
            };
            let net = net();
            let mut adder = add::Adder {
                lib: &lib,
                index: &mut index,
                net: &net,
                options: add::Options {
                    pdf: !no_pdf,
                    folders: folder,
                    labels: label,
                },
            };
            let ask = asks(json);
            let done: Vec<(String, anyhow::Result<add::Added>)> = lines
                .iter()
                .filter(|l| !l.trim().is_empty())
                .map(|l| {
                    let done = adder.add(l);
                    (l.clone(), cli::settle(&mut adder, done, ask))
                })
                .collect();
            cli::print_added(&lib, &done, json)
        }
        Some(Command::Watch { dirs, take, folder }) => {
            run_daemon(&args.library, &net(), None, &dirs, take, folder, Vec::new())
        }
        Some(Command::Serve {
            print_token: true, ..
        }) => {
            println!("{}", daemon::token()?);
            Ok(())
        }
        Some(Command::Serve {
            port,
            watch,
            take,
            folder,
            extension,
            ..
        }) => run_daemon(
            &args.library,
            &net(),
            Some(port),
            &watch,
            take,
            folder,
            extension,
        ),
        Some(Command::Screen {
            width,
            height,
            svg,
            keys,
        }) => {
            let (lib, index) = open(&args.library)?;
            let mut app = app::App::new(lib, index)?;
            app.theme = theme::Theme::of(args.theme);
            for key in keys.iter().flat_map(|k| app::keys(k)) {
                let _ = app.key(key);
                app.wait_for_map();
            }
            if svg {
                print!("{}", app::screen_svg(&mut app, width, height));
            } else {
                println!("{}", app::screen(&mut app, width, height));
            }
            Ok(())
        }
        Some(Command::Pdf { file, title }) => {
            let reading = pdf::read(&std::fs::read(&file)?)?;
            if let Some(t) = title {
                println!("has title: {}", pdf::has_title(&t, &reading.text()));
            }
            println!("embedded: {:?}", reading.embedded);
            println!("ids: {:?}", pdf::ids(&reading));
            println!("article ids: {:?}", pdf::article_info_dois(&reading));
            let first = pdf::first_page(&reading);
            for t in pdf::title_lines(first) {
                println!("heading: {t}");
            }
            for q in pdf::queries(&reading) {
                println!("query: {q}");
            }
            println!("---- page 1");
            for line in reading
                .pages
                .first()
                .into_iter()
                .flat_map(|p| p.lines())
                .take(40)
            {
                println!("| {line}");
            }
            Ok(())
        }
        Some(Command::Attach {
            what,
            query,
            no_browser,
            published,
            wait,
            json,
        }) => {
            let (lib, mut index) = open(&args.library)?;
            // A key and a PDF's path or address: that PDF, for that paper.
            if let Some((key, given)) = pdf_given(&what).filter(|_| query.is_none()) {
                let bytes = if given.starts_with("http://") || given.starts_with("https://") {
                    net().pdf_via_page(given, &|_| true)?.0
                } else {
                    std::fs::read(given)?
                };
                let done = add::attach(&lib, &mut index, key, &bytes, given, true);
                return cli::print_added(&lib, &[(key.to_string(), done)], json);
            }
            let chosen = index.choose(what.iter().map(String::as_str), query.as_deref())?;
            let browser = chosen.papers.len() == 1 && !no_browser && wait > 0;
            let net = net();
            let say = |s: &str| {
                eprintln!("paperstack: {s}");
                true
            };
            let mut done = add::open_copies(&lib, &mut index, &net, chosen.papers, published, &say);
            // One paper behind a paywall: its page in the browser.
            if let [(key, got)] = &mut done[..]
                && browser
                && let Err(e) = got
                && e.is::<add::NoOpenCopy>()
            {
                eprintln!("paperstack: {}", add::one_line(e));
                let hit = index
                    .get(key)?
                    .ok_or_else(|| anyhow::anyhow!("no paper with the key {key}"))?;
                *got = await_download(&lib, &mut index, &hit, wait)?;
            }
            done.extend(
                chosen
                    .missing
                    .into_iter()
                    .map(|k| (k.clone(), Err(anyhow::anyhow!("no paper with the key {k}")))),
            );
            if done.is_empty() {
                anyhow::bail!("no paper there wants a PDF");
            }
            cli::print_added(&lib, &done, json)
        }
        Some(Command::Delete {
            keys,
            collection,
            label,
            pdf,
        }) => {
            let (lib, mut index) = open(&args.library)?;
            let mut failed = 0;
            for key in &keys {
                let done = if pdf {
                    remove::pdf(&lib, &mut index, key)
                } else {
                    remove::paper(&lib, &mut index, key)
                };
                match done {
                    Ok(to) => {
                        let what = if pdf { "PDF deleted" } else { "deleted" };
                        println!("{key:<24} {what}: {}", to.display());
                    }
                    Err(e) => {
                        failed += 1;
                        eprintln!("paperstack: {key}: {e:#}");
                    }
                }
            }
            let shelves = collection
                .iter()
                .map(|n| (remove::Shelf::Collection, n))
                .chain(label.iter().map(|n| (remove::Shelf::Label, n)));
            for (shelf, name) in shelves {
                match remove::shelf(&lib, &mut index, shelf, name) {
                    Ok(n) => println!("{} {name} deleted: taken off {n} papers", shelf.name()),
                    Err(e) => {
                        failed += 1;
                        eprintln!("paperstack: {e:#}");
                    }
                }
            }
            if failed > 0 {
                anyhow::bail!("{failed} not deleted");
            }
            Ok(())
        }
        Some(Command::Mcp) => {
            let (lib, index) = open(&args.library)?;
            mcp::Server {
                lib,
                index,
                net: net(),
            }
            .run()
        }
        Some(Command::Import {
            export,
            pdfs,
            dry_run,
            json,
        }) => {
            if pdfs
                .as_ref()
                .is_some_and(|p| p.extension().is_some_and(|e| e.eq_ignore_ascii_case("zip")))
            {
                anyhow::bail!("unzip the ZIP first, and give the folder it makes");
            }
            let (lib, mut index) = open(&args.library)?;
            let pdfs = pdfs.or_else(|| migrate::find_pdf_dir(&export));
            let done = migrate::import(&lib, &mut index, &export, pdfs.as_deref(), dry_run)?;
            for e in done.errors.iter().take(20) {
                eprintln!("paperstack: {e}");
            }
            if json {
                println!("{}", serde_json::to_string_pretty(&done)?);
            } else {
                let would = if dry_run { "would be: " } else { "" };
                println!("{would}{}", done.summary());
            }
            Ok(())
        }
        Some(Command::Inbox { json }) => {
            let (lib, mut index) = open(&args.library)?;
            let net = net();
            let mut adder = add::Adder {
                lib: &lib,
                index: &mut index,
                net: &net,
                options: add::Options {
                    pdf: false,
                    ..add::Options::default()
                },
            };
            let ask = asks(json);
            let done: Vec<(String, anyhow::Result<add::Added>)> = adder
                .inbox()?
                .into_iter()
                .map(|(p, r)| {
                    // One chosen of those a PDF holds is filed, and out of the inbox.
                    let r = cli::settle(&mut adder, r, ask);
                    if r.is_ok() {
                        let _ = std::fs::remove_file(&p);
                    }
                    (p.display().to_string(), r)
                })
                .collect();
            cli::print_added(&lib, &done, json)
        }
        Some(Command::Init { demo }) => {
            let lib = Library::init(&args.library)?;
            if demo {
                for r in demo::records() {
                    lib.save(&r, None)?;
                }
            }
            let (_, index) = open(&lib.root)?;
            let n = index.shelves()?.all;
            println!("a library at {}, {n} papers", lib.root.display());
            Ok(())
        }
        Some(Command::Index) => {
            let lib = Library::open(&args.library)?;
            let mut index = Index::open(&index::default_path(&lib.root))?;
            let done = index.refresh(&lib)?;
            report(&done);
            println!(
                "{} read, {} gone, {} papers",
                done.read,
                done.removed,
                index.shelves()?.all
            );
            Ok(())
        }
        Some(Command::Themes {
            json,
            assign,
            rebuild,
            fresh,
            undo,
            split,
            resolution,
            name,
            label,
        }) => {
            let (lib, mut index) = open(&args.library)?;
            let path = map::default_path(index.path());
            let docs = index.docs();
            // Each step of the work, on a terminal's stderr, a step getting
            // on in place of its line before; the themes alone on stdout.
            let last = std::cell::RefCell::new(String::new());
            let terminal = std::io::IsTerminal::is_terminal(&std::io::stderr());
            let say = &|s: &str| {
                use ratatui::crossterm::{cursor, execute, terminal as term};
                if !terminal {
                    return;
                }
                let mut last = last.borrow_mut();
                if map::same_step(&last, s) {
                    let _ = execute!(
                        std::io::stderr(),
                        cursor::MoveUp(1),
                        term::Clear(term::ClearType::CurrentLine)
                    );
                }
                eprintln!("{s}…");
                *last = s.to_string();
            };
            let kept = match map::Kept::read(&path) {
                Some(mut kept) => {
                    if rebuild {
                        kept.rebuild(&docs, resolution, fresh, say)?;
                    } else if let Some(t) = split {
                        let r = resolution.unwrap_or(map::RESOLUTION);
                        anyhow::ensure!(
                            kept.split(&docs, t, r, say)?.is_some(),
                            "theme {} does not split at that resolution; try a higher --resolution",
                            t + 1
                        );
                    } else if undo {
                        anyhow::ensure!(kept.undo(&docs)?, "no previous themes kept");
                    } else if let Some(r) = resolution {
                        kept.regroup(&docs, r, say)?;
                    } else {
                        kept.refresh(&docs);
                        if assign {
                            kept.assign()?;
                        } else if let Some(name) = &name {
                            kept.name(theme_number(&name[0])?, &name[1])?;
                        } else if let Some(label) = &label {
                            let t = theme_number(&label[0])?;
                            let theme = kept.current.themes.get(t);
                            let theme = anyhow::Context::with_context(theme, || {
                                format!("no theme {}", t + 1)
                            })?;
                            let label = label.get(1).cloned().unwrap_or_else(|| theme.name());
                            let keys = kept.current.member_keys(t);
                            let n = remove::label(&lib, &mut index, &keys, &label)?;
                            eprintln!("{n} papers labelled {label}");
                        }
                    }
                    kept
                }
                None => {
                    anyhow::ensure!(
                        !undo && split.is_none() && name.is_none() && label.is_none(),
                        "no themes kept yet: `paperstack themes` gathers them"
                    );
                    map::Kept::build(&path, &docs, resolution.unwrap_or(map::RESOLUTION), say)?
                }
            };
            cli::themes(&kept.current, &kept.placed, json)
        }
        Some(Command::Find {
            mut query,
            not,
            sort,
            reverse,
            limit,
            format,
        }) => {
            let (lib, index) = open(&args.library)?;
            query.extend(not.iter().map(|w| format!("-{w}")));
            let hits = index.search(&Query::parse(&query.join(" ")), sort, reverse, limit)?;
            cli::print(&lib, &hits, &format)
        }
        Some(Command::Get { key, format }) => {
            let (lib, index) = open(&args.library)?;
            let Some(hit) = index.get(&key)? else {
                anyhow::bail!("no paper with the key {key}");
            };
            cli::print(&lib, &[hit], &format)
        }
    }
}

#[cfg(test)]
mod tests;
