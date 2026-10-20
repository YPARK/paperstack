//! Getting a PDF that only the browser can: one behind a paywall, or a
//! publisher's bot check. The paper's page is opened in the browser, as
//! you, signed in, through your library's proxy if `PAPERSTACK_PROXY`
//! names one, as `https://ezproxy.library.ubc.ca/login?url=`; the PDF you download there is noticed as it lands in the
//! downloads folder, `PAPERSTACK_DOWNLOADS` or `~/Downloads`.

use crate::record::Record;
use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::time::SystemTime;

/// The page to open for `r`: its DOI's, through the proxy if there is one;
/// else its address, or arXiv's; else a search for its title.
pub fn page(r: &Record) -> String {
    let proxy = std::env::var("PAPERSTACK_PROXY").unwrap_or_default();
    if let Some(doi) = r.doi() {
        return format!("{proxy}https://doi.org/{doi}");
    }
    if let Some(url) = r.csl.get("URL").and_then(|u| u.as_str()) {
        return format!("{proxy}{url}");
    }
    if let Some(id) = &r.paperstack.arxiv {
        return format!("https://arxiv.org/abs/{id}");
    }
    let words: String = r.title().split_whitespace().collect::<Vec<_>>().join("+");
    format!("https://scholar.google.com/scholar?q={words}")
}

/// Open `url` in the browser: the one `PAPERSTACK_BROWSER` names, as
/// `open -a "Google Chrome"`, where paperstack's extension is; none, if it
/// says `none`, the address only shown; else the one the system opens
/// addresses with.
pub fn open(url: &str) -> anyhow::Result<()> {
    let chosen = std::env::var("PAPERSTACK_BROWSER").unwrap_or_default();
    let mut command = match chosen.trim() {
        "none" => return Ok(()),
        "" if cfg!(target_os = "macos") => std::process::Command::new("open"),
        "" if cfg!(windows) => std::process::Command::new("explorer"),
        "" => std::process::Command::new("xdg-open"),
        given => {
            let mut c = std::process::Command::new("sh");
            c.args(["-c", &format!("{given} \"$1\""), "sh"]);
            c
        }
    };
    let ok = command
        .arg(url)
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .status()?
        .success();
    anyhow::ensure!(ok, "the browser could not be opened at {url}");
    Ok(())
}

/// Where the browser saves what it downloads.
pub fn downloads() -> PathBuf {
    std::env::var_os("PAPERSTACK_DOWNLOADS").map_or_else(
        || PathBuf::from(std::env::var_os("HOME").unwrap_or_default()).join("Downloads"),
        PathBuf::from,
    )
}

/// PDFs as they come into a folder from now on, each once, when whole: as
/// big as it was when last looked at, as a browser's finished download is.
pub struct Arrivals {
    pub dir: PathBuf,
    since: SystemTime,
    sizes: HashMap<PathBuf, u64>,
    given: HashSet<PathBuf>,
}

impl Arrivals {
    pub fn new(dir: PathBuf) -> Self {
        Arrivals {
            dir,
            since: SystemTime::now(),
            sizes: HashMap::new(),
            given: HashSet::new(),
        }
    }

    /// The PDFs that have come, and are whole, since last asked.
    pub fn next(&mut self) -> Vec<PathBuf> {
        let Ok(entries) = std::fs::read_dir(&self.dir) else {
            return Vec::new();
        };
        let mut whole = Vec::new();
        for e in entries.flatten() {
            let path = e.path();
            let name = e.file_name().to_string_lossy().to_lowercase();
            if name.starts_with('.') || !name.ends_with(".pdf") || self.given.contains(&path) {
                continue;
            }
            let Ok(meta) = e.metadata() else { continue };
            if meta.modified().is_ok_and(|m| m < self.since) {
                continue;
            }
            if self.sizes.insert(path.clone(), meta.len()) == Some(meta.len()) && meta.len() > 0 {
                self.given.insert(path.clone());
                whole.push(path);
            }
        }
        whole
    }
}

/// A path as typed, or dropped on a terminal, which quotes it or escapes
/// its spaces; `~/` as the home folder.
pub fn dropped(line: &str) -> PathBuf {
    let path = line.trim().trim_matches(['\'', '"']).replace("\\ ", " ");
    match path.strip_prefix("~/") {
        Some(rest) => PathBuf::from(std::env::var_os("HOME").unwrap_or_default()).join(rest),
        None => PathBuf::from(path),
    }
}

/// `path` with the home folder as `~`, as it is shown.
pub fn tilde(path: &Path) -> String {
    let home = std::env::var("HOME").unwrap_or_default();
    let shown = path.display().to_string();
    match shown.strip_prefix(&home) {
        Some(rest) if !home.is_empty() => format!("~{rest}"),
        _ => shown,
    }
}
