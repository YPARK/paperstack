//! Getting a PDF that only the browser can: one behind a paywall, or a
//! publisher's bot check. The paper's page is opened in the browser, as
//! you, signed in, through your library's proxy if `PAPERSTACK_PROXY`
//! names one, as `https://ezproxy.example.edu/login?url=`; the PDF you download there is noticed as it lands in the
//! downloads folder, `PAPERSTACK_DOWNLOADS` or `~/Downloads`.

use crate::record::Record;
use anyhow::{Context, bail};
use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::time::SystemTime;

/// The page to open for `r`: its own, its DOI's or address through the
/// proxy if there is one; else a search for its title.
pub fn page(r: &Record) -> String {
    match own(r) {
        Some((page, true)) => {
            let proxy = std::env::var("PAPERSTACK_PROXY").unwrap_or_default();
            format!("{proxy}{page}")
        }
        Some((page, false)) => page,
        None => {
            let words: String = r.title().split_whitespace().collect::<Vec<_>>().join("+");
            format!("https://scholar.google.com/scholar?q={words}")
        }
    }
}

/// The paper's own page on the web, to share: its DOI's, its address,
/// arXiv's or PubMed's; none, if nothing names one.
pub fn address(r: &Record) -> Option<String> {
    own(r).map(|(page, _)| page)
}

/// The paper's own page, and whether a paywall may stand before it, as
/// before a publisher's, though not arXiv's or PubMed's.
fn own(r: &Record) -> Option<(String, bool)> {
    if let Some(doi) = r.doi() {
        // A DOI may hold what an address reads otherwise, as SICI's `#`.
        let mut path = String::new();
        for c in doi.chars() {
            match c {
                '%' | '#' | '?' | '"' | '<' | '>' | ' ' => {
                    path.push_str(&format!("%{:02X}", c as u32))
                }
                c => path.push(c),
            }
        }
        return Some((format!("https://doi.org/{path}"), true));
    }
    if let Some(url) = r.text("URL") {
        return Some((url.to_string(), true));
    }
    if let Some(id) = &r.paperstack.arxiv {
        return Some((format!("https://arxiv.org/abs/{id}"), false));
    }
    let pmid = r.text("PMID")?;
    Some((format!("https://pubmed.ncbi.nlm.nih.gov/{pmid}/"), false))
}

/// Open `url` in the browser: the one `PAPERSTACK_BROWSER` names, as
/// `open -a "Google Chrome"`, where paperstack's extension is; none, if it
/// says `none`, the address only shown; else the one the system opens
/// addresses with.
pub fn open(url: &str) -> anyhow::Result<()> {
    // Nor any from the tests.
    if cfg!(test) {
        return Ok(());
    }
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
    /// The PDFs there before, as they were then: a file's time, on some
    /// systems, is of a coarser clock than the one now is told by, and a
    /// download just begun may seem older than the watching.
    before: HashMap<PathBuf, Option<SystemTime>>,
    sizes: HashMap<PathBuf, u64>,
    given: HashSet<PathBuf>,
}

impl Arrivals {
    pub fn new(dir: PathBuf) -> Self {
        let before = std::fs::read_dir(&dir)
            .into_iter()
            .flatten()
            .flatten()
            .map(|e| (e.path(), e.metadata().and_then(|m| m.modified()).ok()))
            .collect();
        Arrivals {
            dir,
            before,
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
            if self.before.get(&path) == Some(&meta.modified().ok()) {
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

/// A PDF the person picks in the system's file dialog, opened at the
/// downloads folder; none, where they cancel. The dialog is the system's,
/// so the terminal is given back before this is asked.
pub fn choose_pdf(prompt: &str) -> anyhow::Result<Option<PathBuf>> {
    let out = if cfg!(target_os = "macos") {
        choose_macos(prompt)?
    } else if cfg!(windows) {
        choose_windows(prompt)?
    } else {
        choose_unix(prompt)?
    };
    let path = out.trim();
    Ok((!path.is_empty()).then(|| PathBuf::from(path)))
}

/// `choose file`, cancelled as error −128, which is no file.
fn choose_macos(prompt: &str) -> anyhow::Result<String> {
    let prompt = prompt.replace('\\', "\\\\").replace('"', "\\\"");
    let script = format!(
        r#"try
POSIX path of (choose file of type {{"pdf", "com.adobe.pdf"}} with prompt "{prompt}" default location (path to downloads folder))
on error number -128
""
end try"#
    );
    let out = std::process::Command::new("osascript")
        .args(["-e", &script])
        .output()
        .context("cannot open the file dialog")?;
    if !out.status.success() {
        let err = String::from_utf8_lossy(&out.stderr);
        bail!("the file dialog could not be opened: {}", err.trim());
    }
    Ok(String::from_utf8_lossy(&out.stdout).into_owned())
}

fn choose_windows(prompt: &str) -> anyhow::Result<String> {
    let prompt = prompt.replace('\'', "''");
    let script = format!(
        "Add-Type -AssemblyName System.Windows.Forms; \
         $d = New-Object System.Windows.Forms.OpenFileDialog; \
         $d.Filter = 'PDF (*.pdf)|*.pdf'; $d.Title = '{prompt}'; \
         if ($d.ShowDialog() -eq 'OK') {{ $d.FileName }}"
    );
    dialog(
        std::process::Command::new("powershell").args(["-NoProfile", "-STA", "-Command", &script]),
        "powershell",
    )
}

/// zenity, or kdialog where that is what the desktop has.
fn choose_unix(prompt: &str) -> anyhow::Result<String> {
    if have("zenity") {
        return dialog(
            std::process::Command::new("zenity").args([
                "--file-selection",
                "--title",
                prompt,
                "--filename",
                &downloads().display().to_string(),
                "--file-filter",
                "PDF | *.pdf *.PDF",
            ]),
            "zenity",
        );
    }
    if have("kdialog") {
        let start = downloads().display().to_string();
        return dialog(
            std::process::Command::new("kdialog").args([
                "--getopenfilename",
                &start,
                "application/pdf",
                "--title",
                prompt,
            ]),
            "kdialog",
        );
    }
    bail!("no file dialog: install zenity, or give the PDF's path")
}

fn have(cmd: &str) -> bool {
    std::process::Command::new(cmd)
        .arg("--version")
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .status()
        .is_ok_and(|s| s.success())
}

/// What `command` printed; an empty string where the person cancelled,
/// which these dialogs signal by exiting with 1.
fn dialog(command: &mut std::process::Command, name: &str) -> anyhow::Result<String> {
    let out = command
        .output()
        .with_context(|| format!("cannot open the file dialog ({name})"))?;
    if out.status.success() || out.status.code() == Some(1) {
        return Ok(String::from_utf8_lossy(&out.stdout).into_owned());
    }
    let err = String::from_utf8_lossy(&out.stderr);
    bail!("the file dialog could not be opened: {}", err.trim())
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
