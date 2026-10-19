//! paperstack: a library of papers in the terminal, kept in a folder, as in
//! Dropbox, with each PDF and its record under `year/lastname/firstname/`,
//! searched through an index kept on this machine. PDFs are presented with
//! ohp.

mod add;
mod agents;
mod app;
mod bibtex;
mod cli;
mod daemon;
mod demo;
mod index;
mod layout;
mod library;
mod net;
mod pdf;
mod query;
mod record;
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
    /// The library: a folder, as in Dropbox
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
    /// The TUI's colours: Paperpile's light page, or the terminal's own dark one
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
    /// Find papers: words, "phrases", author:, year:2020..2026, folder:, has:pdf, -...
    Find {
        query: Vec<String>,
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
        /// Put them in this folder too
        #[arg(long)]
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
        /// Put what is filed in this folder of the library too
        #[arg(long)]
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
        #[arg(long)]
        folder: Vec<String>,
        /// Other extensions that may ask, besides paperstack's own: a Chrome
        /// extension's id, or an origin whole, as moz-extension://ID
        #[arg(long)]
        extension: Vec<String>,
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
    PathBuf::from(std::env::var_os("HOME").unwrap_or_default()).join("Dropbox/paperstack")
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

fn main() -> anyhow::Result<()> {
    let args = Args::parse();
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
            let done: Vec<(String, anyhow::Result<add::Added>)> = lines
                .iter()
                .filter(|l| !l.trim().is_empty())
                .map(|l| (l.clone(), adder.add(l)))
                .collect();
            cli::print_added(&lib, &done, json)
        }
        Some(Command::Watch { dirs, take, folder }) => {
            run_daemon(&args.library, &net(), None, &dirs, take, folder, Vec::new())
        }
        Some(Command::Serve {
            port,
            watch,
            take,
            folder,
            extension,
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
            let done: Vec<(String, anyhow::Result<add::Added>)> = adder
                .inbox()?
                .into_iter()
                .map(|(p, r)| (p.display().to_string(), r))
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
        Some(Command::Find {
            query,
            sort,
            reverse,
            limit,
            format,
        }) => {
            let (lib, index) = open(&args.library)?;
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
