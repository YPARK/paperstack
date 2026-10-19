//! paperstack: a library of papers in the terminal, kept in a folder, as in
//! Dropbox, with each PDF and its record under `year/lastname/firstname/`,
//! searched through an index kept on this machine. PDFs are presented with
//! ohp.

mod agents;
mod app;
mod bibtex;
mod cli;
mod demo;
mod index;
mod layout;
mod library;
mod query;
mod record;
mod source;
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
}

fn default_root() -> PathBuf {
    PathBuf::from(std::env::var_os("HOME").unwrap_or_default()).join("Dropbox/paperstack")
}

/// The library at `root`, and its index brought up to it; what could not
/// be read is said on stderr.
fn open(root: &Path) -> anyhow::Result<(Library, Index)> {
    let lib = Library::open(root)?;
    let mut index = Index::open(&index::default_path(&lib.root))?;
    let done = index.refresh(&lib)?;
    for e in &done.errors {
        eprintln!("paperstack: {e}");
    }
    for c in &done.conflicts {
        eprintln!("paperstack: a conflicted copy, not read: {}", c.display());
    }
    Ok((lib, index))
}

fn main() -> anyhow::Result<()> {
    let args = Args::parse();
    match args.command {
        None => app::run(open(&args.library)?),
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
            for e in &done.errors {
                eprintln!("paperstack: {e}");
            }
            for c in &done.conflicts {
                eprintln!("paperstack: a conflicted copy, not read: {}", c.display());
            }
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
