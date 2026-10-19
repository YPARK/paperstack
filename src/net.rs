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

pub struct Net {
    agent: ureq::Agent,
    /// Given to the registries, which serve those who give one first.
    pub email: Option<String>,
    agent_name: String,
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
        }
    }

    /// Add to `query` the address to write to, as `name`, where one is given.
    pub fn sign<'a>(&'a self, query: &mut Vec<(&'a str, &'a str)>, name: &'a str) {
        if let Some(e) = &self.email {
            query.push((name, e));
        }
    }

    /// JSON from a registry, with `query` in its address.
    pub fn json(&self, url: &str, accept: &str, query: &[(&str, &str)]) -> anyhow::Result<Value> {
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
                .limit(20 << 20)
                .read_to_string()?,
        ))
    }

    /// A PDF; anything else, as a page asking to sign in, is an error.
    pub fn pdf(&self, url: &str) -> anyhow::Result<Vec<u8>> {
        let mut resp = self
            .agent
            .get(url)
            .header("User-Agent", BROWSER)
            .header("Accept", "application/pdf,*/*;q=0.5")
            .call()
            .with_context(|| format!("cannot download {url}"))?;
        let bytes = resp
            .body_mut()
            .with_config()
            .limit(PDF_LIMIT)
            .read_to_vec()?;
        if !bytes.starts_with(b"%PDF") {
            bail!("{url} gave no PDF");
        }
        Ok(bytes)
    }
}
