//! Running on: `paperstack watch` files PDFs as they come into the inbox,
//! or another folder, as a browser's downloads; `paperstack serve` does so
//! too, and takes papers from the browser extension at `127.0.0.1`, their
//! PDFs fetched in the browser, past paywalls and bot checks.
//!
//! The server answers only this machine, and of browsers only paperstack's
//! extension, or those named: a page cannot add papers. Requests are read
//! by a few threads, so a slow one keeps no other waiting; what they ask is
//! done one at a time, as the index is one.

use crate::add::{Added, Adder, Options, Page, one_line};
use crate::index::Index;
use crate::library::{self, Library};
use crate::net::Net;
use anyhow::Context;
use base64::Engine;
use serde_json::{Value, json};
use std::collections::HashMap;
use std::io::{BufRead, BufReader, Read, Write};
use std::net::{TcpListener, TcpStream};
use std::path::{Path, PathBuf};
use std::sync::{Arc, mpsc};
use std::time::{Duration, Instant, SystemTime};

pub const PORT: u16 = 8765;
/// The extension in `extension/`: its id, as the `key` in its manifest
/// fixes it wherever it is loaded from.
pub const EXTENSION: &str = "kckhjncnkhapnmnjlpjefjamffaecmcf";
/// How long a request may take to come, all of it, and each part of it.
const DEADLINE: Duration = Duration::from_secs(60);
const PAUSE: Duration = Duration::from_secs(10);
/// The longest line of a request's head, and the most lines of it.
const LINE_LIMIT: u64 = 16 << 10;
const HEADERS: usize = 100;
/// The most requests read at once, each by a thread of its own; more wait
/// to be taken.
const READERS: usize = 8;
/// How often folders are looked in.
const SCAN: Duration = Duration::from_secs(2);
/// The largest request taken: a PDF, in base64.
const BODY_LIMIT: usize = 400 << 20;

/// A folder watched for PDFs.
pub struct Watched {
    pub dir: PathBuf,
    /// Its PDFs there already are filed too, as the inbox's are; another
    /// folder's, as old downloads, are not.
    pub existing: bool,
    /// A PDF filed is taken out of it, as out of the inbox.
    pub take: bool,
    /// A PDF without an id in it is filed by its text, searched for; not
    /// so of a folder as ~/Downloads, whose PDFs may be anyone's letters.
    pub search: bool,
}

pub struct Daemon<'a> {
    pub lib: &'a Library,
    pub index: &'a mut Index,
    pub net: &'a Net,
    pub folders: Vec<String>,
    /// The origins of the extensions that may ask, besides paperstack's.
    pub origins: Vec<String>,
}

/// A request read whole, and the connection to answer it on.
struct Request {
    method: String,
    path: String,
    body: Vec<u8>,
    stream: TcpStream,
}

impl Daemon<'_> {
    fn adder(&mut self) -> Adder<'_> {
        Adder {
            lib: self.lib,
            index: self.index,
            net: self.net,
            options: Options {
                pdf: true,
                folders: self.folders.clone(),
                ..Options::default()
            },
        }
    }

    pub fn run(&mut self, port: Option<u16>, watched: &[Watched]) -> anyhow::Result<()> {
        // Kept here, so waiting on it waits, whether or not anything listens.
        // A request read waits for the one before to be done.
        let (tx, rx) = mpsc::sync_channel::<Request>(1);
        if let Some(p) = port {
            let listener = TcpListener::bind(("127.0.0.1", p)).with_context(|| {
                format!("cannot listen at 127.0.0.1:{p}: is paperstack serve running already?")
            })?;
            say(&format!(
                "listening at http://127.0.0.1:{p} for the browser extension"
            ));
            let origins: Arc<[String]> = std::iter::once(origin(EXTENSION))
                .chain(self.origins.iter().cloned())
                .collect();
            // A few readers, each taking the next request as it comes.
            for _ in 0..READERS {
                let (listener, tx, origins) = (listener.try_clone()?, tx.clone(), origins.clone());
                std::thread::spawn(move || {
                    loop {
                        match listener.accept() {
                            Ok((stream, _)) => match read(stream, p, &origins) {
                                Ok(Some(request)) => {
                                    let _ = tx.send(request);
                                }
                                Ok(None) => {}
                                Err(e) => say(&format!("a request failed: {e:#}")),
                            },
                            Err(e) => {
                                say(&format!("cannot take a request: {e}"));
                                std::thread::sleep(Duration::from_millis(100));
                            }
                        }
                    }
                });
            }
        }
        // Files done with, or given up on, as they were then; and files
        // seen once, by their size, to be taken when it has not changed.
        let mut done: HashMap<PathBuf, (u64, SystemTime)> = HashMap::new();
        let mut growing: HashMap<PathBuf, u64> = HashMap::new();
        for w in watched {
            say(&format!("watching {}", w.dir.display()));
            if !w.existing {
                for (path, size, mtime) in pdfs(&w.dir) {
                    done.insert(path, (size, mtime));
                }
            }
        }
        let mut last_scan = Instant::now() - SCAN;
        loop {
            if let Ok(request) = rx.recv_timeout(SCAN.saturating_sub(last_scan.elapsed()))
                && let Err(e) = self.answer(request)
            {
                say(&format!("a request failed: {e:#}"));
            }
            if last_scan.elapsed() >= SCAN {
                last_scan = Instant::now();
                for w in watched {
                    for (path, size, mtime) in pdfs(&w.dir) {
                        if done.get(&path) == Some(&(size, mtime)) {
                            continue;
                        }
                        if growing.insert(path.clone(), size) != Some(size) {
                            continue;
                        }
                        growing.remove(&path);
                        done.insert(path.clone(), (size, mtime));
                        self.file(&path, w.take, w.search);
                    }
                }
            }
        }
    }

    /// File the PDF at `path`, and take it out of its folder if `take`.
    fn file(&mut self, path: &Path, take: bool, search: bool) {
        let name = path.display().to_string();
        let done = std::fs::read(path)
            .with_context(|| format!("cannot read {name}"))
            .and_then(|bytes| {
                self.index.refresh(self.lib)?;
                self.adder().add_pdf_with(&bytes, &name, search)
            });
        match done {
            Ok(added) => {
                say(&tell(&added));
                if take {
                    let _ = std::fs::remove_file(path);
                }
            }
            Err(e) => say(&one_line(&e)),
        }
    }

    fn answer(&mut self, request: Request) -> anyhow::Result<()> {
        let Request {
            method,
            path,
            body,
            mut stream,
        } = request;
        match (method.as_str(), path.as_str()) {
            ("GET", "/ping") => reply(
                &mut stream,
                200,
                &json!({ "paperstack": env!("CARGO_PKG_VERSION"), "library": self.lib.root }),
            ),
            ("POST", "/add") => {
                let result = self.add(&body);
                match result {
                    Ok((added, title)) => {
                        say(&tell(&added));
                        reply(
                            &mut stream,
                            200,
                            &json!({
                                "key": added.key,
                                "title": title,
                                "outcome": added.outcome,
                                "how": added.how,
                                "has_pdf": added.has_pdf,
                                "record": self.lib.root.join(&added.path),
                            }),
                        )
                    }
                    Err(e) => {
                        let e = one_line(&e);
                        say(&e);
                        reply(&mut stream, 422, &json!({ "error": e }))
                    }
                }
            }
            ("OPTIONS", _) => reply(&mut stream, 204, &Value::Null),
            _ => reply(&mut stream, 404, &json!({ "error": "no such thing" })),
        }
    }

    /// Add what the extension sent, and its title as filed.
    fn add(&mut self, body: &[u8]) -> anyhow::Result<(Added, String)> {
        /// The page, and its PDF in base64.
        #[derive(serde::Deserialize)]
        struct Sent {
            #[serde(flatten)]
            page: Page,
            #[serde(default)]
            pdf: Option<String>,
        }
        let Sent { mut page, pdf } = serde_json::from_slice(body).context("not a page")?;
        if let Some(b64) = pdf {
            let pdf = base64::engine::general_purpose::STANDARD
                .decode(b64)
                .context("the PDF is not base64")?;
            page.pdf = pdf.starts_with(b"%PDF").then_some(pdf);
        }
        self.index.refresh(self.lib)?;
        let added = self.adder().add_from_browser(&page)?;
        let title = self
            .index
            .get(&added.key)?
            .map(|h| h.record.title().to_string())
            .unwrap_or_default();
        Ok((added, title))
    }
}

/// A reader that fails once its time is up, however slowly its bytes come.
struct Timed<R> {
    inner: R,
    until: Instant,
}

impl<R: Read> Read for Timed<R> {
    fn read(&mut self, buf: &mut [u8]) -> std::io::Result<usize> {
        if Instant::now() > self.until {
            return Err(std::io::Error::new(
                std::io::ErrorKind::TimedOut,
                "a request too slow in coming",
            ));
        }
        self.inner.read(buf)
    }
}

/// An extension's origin: an id is Chrome's; one given whole, as Firefox's
/// `moz-extension://`, is kept.
pub fn origin(id: &str) -> String {
    if id.contains("://") {
        id.to_string()
    } else {
        format!("chrome-extension://{id}")
    }
}

/// Why a request with these headers is refused, if it is.
pub fn refusal(
    headers: &HashMap<String, String>,
    port: u16,
    origins: &[String],
    length: usize,
) -> Option<(u16, &'static str)> {
    // A page renamed to this machine, as by DNS rebinding, names its own
    // host; a page's request names the page as its origin, an extension's
    // the extension, a program on this machine's nothing.
    let host = headers.get("host").map(String::as_str).unwrap_or("");
    if host != format!("127.0.0.1:{port}") && host != format!("localhost:{port}") {
        Some((403, "not this machine's"))
    } else if let Some(origin) = headers.get("origin")
        && !origins.contains(origin)
    {
        Some((403, "only paperstack's extension may ask"))
    } else if length > BODY_LIMIT {
        Some((413, "too large"))
    } else {
        None
    }
}

/// The request on `stream`, read whole, if it may be answered; one that may
/// not is answered here. It is given a while to come, all of it.
fn read(mut stream: TcpStream, port: u16, origins: &[String]) -> anyhow::Result<Option<Request>> {
    stream.set_read_timeout(Some(PAUSE))?;
    stream.set_write_timeout(Some(PAUSE))?;
    let mut reader = BufReader::new(Timed {
        inner: stream.try_clone()?,
        until: Instant::now() + DEADLINE,
    });
    let mut line = String::new();
    (&mut reader).take(LINE_LIMIT).read_line(&mut line)?;
    let mut parts = line.split_whitespace();
    let method = parts.next().unwrap_or("").to_string();
    let path = parts.next().unwrap_or("").to_string();
    let mut headers: HashMap<String, String> = HashMap::new();
    for _ in 0..HEADERS {
        let mut h = String::new();
        if (&mut reader).take(LINE_LIMIT).read_line(&mut h)? == 0 || h.trim().is_empty() {
            break;
        }
        if let Some((k, v)) = h.split_once(':') {
            headers.insert(k.trim().to_lowercase(), v.trim().to_string());
        }
    }
    let length: usize = headers
        .get("content-length")
        .and_then(|l| l.parse().ok())
        .unwrap_or(0);
    if let Some((code, why)) = refusal(&headers, port, origins, length) {
        // What was sent is read first, while there is time: a connection
        // closed with some unread is cut, not answered.
        let _ = std::io::copy(&mut (&mut reader).take(length as u64), &mut std::io::sink());
        reply(&mut stream, code, &json!({ "error": why }))?;
        return Ok(None);
    }
    let mut body = Vec::with_capacity(length.min(1 << 20));
    let mut chunk = vec![0; 64 << 10];
    while body.len() < length {
        let want = (length - body.len()).min(chunk.len());
        let n = reader.read(&mut chunk[..want])?;
        if n == 0 {
            anyhow::bail!("a request cut short");
        }
        body.extend_from_slice(&chunk[..n]);
    }
    Ok(Some(Request {
        method,
        path,
        body,
        stream,
    }))
}

/// The PDFs in `dir`, not hidden, nor still downloading.
fn pdfs(dir: &Path) -> Vec<(PathBuf, u64, SystemTime)> {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return Vec::new();
    };
    entries
        .flatten()
        .filter_map(|e| {
            let name = e.file_name().to_string_lossy().to_lowercase();
            if name.starts_with('.') || !name.ends_with(".pdf") {
                return None;
            }
            let meta = e.metadata().ok()?;
            meta.is_file().then(|| {
                (
                    e.path(),
                    meta.len(),
                    meta.modified().unwrap_or(SystemTime::UNIX_EPOCH),
                )
            })
        })
        .collect()
}

fn tell(a: &Added) -> String {
    format!("{} {}: {}", a.key, a.summary(), a.path.display())
}

fn say(what: &str) {
    println!("{} {what}", crate::add::now());
}

fn reply(stream: &mut TcpStream, code: u16, body: &Value) -> anyhow::Result<()> {
    let reason = match code {
        200 => "OK",
        204 => "No Content",
        403 => "Forbidden",
        404 => "Not Found",
        413 => "Payload Too Large",
        _ => "Unprocessable Content",
    };
    let body = if body.is_null() {
        String::new()
    } else {
        body.to_string()
    };
    write!(
        stream,
        "HTTP/1.1 {code} {reason}\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
        body.len()
    )?;
    Ok(stream.flush()?)
}

/// The inbox, and the folders given: those given only for what comes into
/// them now, and kept unless `take`.
pub fn folders(lib: &Library, given: &[PathBuf], take: bool) -> Vec<Watched> {
    let mut all = vec![Watched {
        dir: lib.root.join(library::INBOX),
        existing: true,
        take: true,
        search: true,
    }];
    all.extend(given.iter().map(|d| Watched {
        dir: d.clone(),
        existing: false,
        take,
        search: false,
    }));
    all
}
