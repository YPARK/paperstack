//! The web, as paperstack asks it: registries' APIs as paperstack, with an
//! address to write to, as Crossref and OpenAlex ask; publishers' pages as
//! a browser asks for them.

use anyhow::{Context, bail};
use serde_json::Value;
use std::sync::Mutex;
use std::time::{Duration, Instant};
use ureq::ResponseExt;

/// The time asked between requests to arXiv, and more after it turned one
/// away.
const ARXIV_GAP: Duration = Duration::from_secs(3);
const ARXIV_BACKOFF: Duration = Duration::from_secs(10);

/// Wait until arXiv may be asked again; longer on another `attempt`.
fn wait_for_arxiv(attempt: usize) {
    static LAST: Mutex<Option<Instant>> = Mutex::new(None);
    let mut last = LAST.lock().unwrap_or_else(|e| e.into_inner());
    let gap = if attempt == 0 {
        ARXIV_GAP
    } else {
        ARXIV_BACKOFF
    };
    if let Some(at) = *last {
        let since = at.elapsed();
        if since < gap {
            std::thread::sleep(gap - since);
        }
    }
    *last = Some(Instant::now());
}

const BROWSER: &str = "Mozilla/5.0 (Macintosh; Intel Mac OS X 10_15_7) AppleWebKit/537.36 \
                       (KHTML, like Gecko) Chrome/141.0 Safari/537.36";
/// The largest PDF downloaded.
const PDF_LIMIT: u64 = 200 << 20;
/// The largest page read.
const PAGE_LIMIT: u64 = 20 << 20;
/// How long a PDF's server is waited for, before the first of its bytes: a
/// copy that does not come is soon given up, for the next.
const PDF_WAIT: Duration = Duration::from_secs(10);
/// How often a PDF's server that asks for fewer requests is asked again,
/// and how long it is waited for first, doubled each time, at most.
/// A server that asks for a longer wait is not waited for.
const PDF_RETRIES: u32 = 2;
const PDF_BACKOFF: Duration = Duration::from_secs(5);
const PDF_BACKOFF_MOST: Duration = Duration::from_secs(20);

#[derive(Clone)]
pub struct Net {
    agent: ureq::Agent,
    /// Given to the registries, which serve those who give one first.
    pub email: Option<String>,
    agent_name: String,
    /// The only answers there are, by `json`, `text`, `page` or `pdf` and
    /// the address: the web as a test sees it.
    #[cfg(test)]
    canned: Option<std::sync::Arc<std::collections::HashMap<String, String>>>,
}

impl Net {
    pub fn new(email: Option<String>) -> Self {
        // Once a process; again, it is already the provider.
        let _ = rustls_rustcrypto::provider().install_default();
        let agent = ureq::Agent::config_builder()
            .timeout_global(Some(Duration::from_secs(30)))
            .build()
            .into();
        let agent_name = match &email {
            Some(e) => format!(
                "paperstack/{} (https://github.com/YPARK/paperstack; mailto:{e})",
                env!("CARGO_PKG_VERSION")
            ),
            None => format!(
                "paperstack/{} (https://github.com/YPARK/paperstack)",
                env!("CARGO_PKG_VERSION")
            ),
        };
        Net {
            agent,
            email,
            agent_name,
            #[cfg(test)]
            canned: None,
        }
    }

    /// A web of these answers only, each under its kind and address, as
    /// `page https://doi.org/10.5555/x`; anything else is not to be had.
    #[cfg(test)]
    pub fn canned(answers: &[(&str, &str)]) -> Self {
        Net {
            canned: Some(std::sync::Arc::new(
                answers
                    .iter()
                    .map(|(k, v)| (k.to_string(), v.to_string()))
                    .collect(),
            )),
            ..Net::new(None)
        }
    }

    /// The canned answer to `kind` of `url`, where the web is canned.
    #[cfg(test)]
    fn answer(&self, kind: &str, url: &str) -> Option<anyhow::Result<String>> {
        let canned = self.canned.as_ref()?;
        Some(
            canned
                .get(&format!("{kind} {url}"))
                .cloned()
                .with_context(|| format!("cannot read {url}")),
        )
    }

    /// Add to `query` the address to write to, as `name`, where one is given.
    pub fn sign<'a>(&'a self, query: &mut Vec<(&'a str, &'a str)>, name: &'a str) {
        if let Some(e) = &self.email {
            query.push((name, e));
        }
    }

    /// JSON from a registry, with `query` in its address.
    pub fn json(&self, url: &str, accept: &str, query: &[(&str, &str)]) -> anyhow::Result<Value> {
        #[cfg(test)]
        if let Some(text) = self.answer("json", url) {
            return Ok(serde_json::from_str(&text?)?);
        }
        let mut req = self
            .agent
            .get(url)
            .header("User-Agent", &self.agent_name)
            .header("Accept", accept);
        for (k, v) in query {
            req = req.query(*k, *v);
        }
        let mut resp = req.call().with_context(|| format!("cannot read {url}"))?;
        let text = resp
            .body_mut()
            .with_config()
            .limit(50 << 20)
            .read_to_string()?;
        serde_json::from_str(&text).with_context(|| format!("{url} gave no JSON"))
    }

    /// Text from a registry, as arXiv's Atom. arXiv asks for no more than
    /// a request every three seconds, and turns away those who ask faster;
    /// turned away, it is asked once more, later.
    pub fn text(&self, url: &str, query: &[(&str, &str)]) -> anyhow::Result<String> {
        #[cfg(test)]
        if let Some(text) = self.answer("text", url) {
            return text;
        }
        let arxiv = url.contains("arxiv.org");
        for attempt in 0..2 {
            if arxiv {
                wait_for_arxiv(attempt);
            }
            let mut req = self.agent.get(url).header("User-Agent", &self.agent_name);
            for (k, v) in query {
                req = req.query(*k, *v);
            }
            match req.call() {
                Ok(mut resp) => {
                    return Ok(resp
                        .body_mut()
                        .with_config()
                        .limit(50 << 20)
                        .read_to_string()?);
                }
                Err(ureq::Error::StatusCode(429)) if arxiv && attempt == 0 => continue,
                Err(e) => return Err(e).with_context(|| format!("cannot read {url}")),
            }
        }
        bail!("{url} turned paperstack away: too many requests")
    }

    /// A publisher's page, and where it was at last, after redirects.
    pub fn page(&self, url: &str) -> anyhow::Result<(String, String)> {
        #[cfg(test)]
        if let Some(html) = self.answer("page", url) {
            return Ok((url.to_string(), html?));
        }
        let mut resp = self
            .agent
            .get(url)
            .header("User-Agent", BROWSER)
            .header("Accept", "text/html,application/xhtml+xml")
            .header("Accept-Language", "en")
            .call()
            .with_context(|| format!("{url} would not give paperstack its page"))?;
        let at = resp.get_uri().to_string();
        Ok((
            at,
            resp.body_mut()
                .with_config()
                .limit(PAGE_LIMIT)
                .read_to_string()?,
        ))
    }

    /// A PDF; anything else, as a page asking to sign in, is a [`Refused`].
    pub fn pdf(&self, url: &str) -> anyhow::Result<Vec<u8>> {
        self.pdf_at(url).map(|(bytes, _)| bytes)
    }

    /// A PDF, as [`Net::pdf`], and where it came from at last.
    fn pdf_at(&self, url: &str) -> anyhow::Result<(Vec<u8>, String)> {
        match self.pdf_or_page(url, false)? {
            Got::Pdf(bytes, at) => Ok((bytes, at)),
            Got::Page(..) => Err(refused(url, Why::Page)),
        }
    }

    /// A PDF, as [`Net::pdf`], or, given a page that names its PDF, as a
    /// repository's does by `citation_pdf_url`, that PDF, if `wanted`; and
    /// where it came from at last, after redirects.
    pub fn pdf_via_page(
        &self,
        url: &str,
        wanted: &dyn Fn(&str) -> bool,
    ) -> anyhow::Result<(Vec<u8>, String)> {
        match self.pdf_or_page(url, true)? {
            Got::Pdf(bytes, at) => Ok((bytes, at)),
            Got::Page(html, at) => {
                let named = crate::resolve::page_pdf(&crate::resolve::metas(&html), &at)
                    .filter(|next| *next != url && *next != at)
                    .ok_or_else(|| refused(url, Why::Page))?;
                if !wanted(&named) {
                    anyhow::bail!("{named}, which {url} names, was had already");
                }
                self.pdf_at(&named)
            }
        }
    }

    /// A PDF, or the page given in its place, read only if `read_page`;
    /// each with where it came from at last. A server that asks for fewer
    /// requests, as bioRxiv's, is asked again after the time it says, or a
    /// while; one that asks for longer is let be for that long.
    fn pdf_or_page(&self, url: &str, read_page: bool) -> anyhow::Result<Got> {
        #[cfg(test)]
        if let Some(answer) = self.answer("pdf", url) {
            let answer = answer?;
            let at = url.to_string();
            if let Some(code) = answer.strip_prefix("!status ") {
                return Err(refused(url, Why::Status(code.parse()?)));
            }
            if answer.starts_with('<') {
                return Ok(Got::Page(answer, at));
            }
            return Ok(Got::Pdf(answer.into_bytes(), at));
        }
        let host = crate::source::host(url).map(|(h, _)| h).unwrap_or_default();
        if resting(&host) {
            return Err(refused(url, Why::Status(429)));
        }
        let mut attempt = 0;
        let mut resp = loop {
            let resp = self
                .agent
                .get(url)
                .config()
                .timeout_connect(Some(PDF_WAIT))
                .timeout_recv_response(Some(PDF_WAIT))
                .http_status_as_error(false)
                .build()
                .header("User-Agent", BROWSER)
                .header("Accept", "application/pdf,*/*;q=0.5")
                .call()
                .map_err(|e| refused(url, Why::Network(e.to_string())))?;
            let status = resp.status().as_u16();
            if status == 429 && attempt < PDF_RETRIES {
                let said = resp
                    .headers()
                    .get("retry-after")
                    .and_then(|v| v.to_str().ok()?.trim().parse::<u64>().ok())
                    .map(Duration::from_secs);
                let wait = said.unwrap_or(PDF_BACKOFF * 2u32.pow(attempt));
                if wait <= PDF_BACKOFF_MOST {
                    std::thread::sleep(wait);
                    attempt += 1;
                    continue;
                }
                rest(&host, wait);
            }
            if status >= 400 {
                return Err(refused(url, Why::Status(status)));
            }
            break resp;
        };
        let at = resp.get_uri().to_string();
        let page = resp.body().mime_type() == Some("text/html");
        if page && !read_page {
            return Ok(Got::Page(String::new(), at));
        }
        let bytes = resp
            .body_mut()
            .with_config()
            .limit(if page { PAGE_LIMIT } else { PDF_LIMIT })
            .read_to_vec()
            .map_err(|e| refused(url, Why::Network(e.to_string())))?;
        if bytes.starts_with(b"%PDF") {
            Ok(Got::Pdf(bytes, at))
        } else if page || bytes.trim_ascii_start().starts_with(b"<") {
            // As its bytes: a page need not be in UTF-8.
            let html = if read_page {
                String::from_utf8(bytes)
                    .unwrap_or_else(|e| String::from_utf8_lossy(e.as_bytes()).into_owned())
            } else {
                String::new()
            };
            Ok(Got::Page(html, at))
        } else {
            Err(refused(url, Why::NotPdf))
        }
    }
}

/// What a PDF's address gave, and where it came from at last.
enum Got {
    Pdf(Vec<u8>, String),
    Page(String, String),
}

fn refused(url: &str, why: Why) -> anyhow::Error {
    Refused {
        url: url.to_string(),
        why,
    }
    .into()
}

/// Hosts that asked for a longer wait than is waited, and until when.
static RESTING: Mutex<Option<std::collections::HashMap<String, Instant>>> = Mutex::new(None);

/// Ask `host` nothing more for `wait`.
fn rest(host: &str, wait: Duration) {
    let mut resting = RESTING.lock().unwrap_or_else(|e| e.into_inner());
    resting
        .get_or_insert_with(Default::default)
        .insert(host.to_string(), Instant::now() + wait);
}

/// Whether `host` asked to be asked nothing for now.
fn resting(host: &str) -> bool {
    let resting = RESTING.lock().unwrap_or_else(|e| e.into_inner());
    resting
        .as_ref()
        .and_then(|r| r.get(host))
        .is_some_and(|until| Instant::now() < *until)
}

/// A PDF not had from where it was asked for, and why.
#[derive(Debug)]
pub struct Refused {
    pub url: String,
    pub why: Why,
}

#[derive(Debug)]
pub enum Why {
    /// Its HTTP status, as 403 for a paywall or 429 for too many requests.
    Status(u16),
    /// A page, as one asking to sign in, or a bot check.
    Page,
    NotPdf,
    Network(String),
}

impl Why {
    /// What an agent reads of it: `http-403`, `html-page`, `not-a-pdf`, `network`.
    pub fn reason(&self) -> String {
        match self {
            Why::Status(n) => format!("http-{n}"),
            Why::Page => "html-page".to_string(),
            Why::NotPdf => "not-a-pdf".to_string(),
            Why::Network(_) => "network".to_string(),
        }
    }
}

impl std::fmt::Display for Refused {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let url = &self.url;
        match &self.why {
            Why::Status(429) => write!(f, "{url} turned paperstack away: too many requests"),
            Why::Status(n) => write!(f, "{url} answered {n}"),
            Why::Page => write!(f, "{url} gave a page, not a PDF"),
            Why::NotPdf => write!(f, "{url} gave no PDF"),
            Why::Network(e) => write!(f, "cannot download {url}: {e}"),
        }
    }
}

impl std::error::Error for Refused {}
