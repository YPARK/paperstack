// A click on the button: the page's tags and PDF links read, the PDF fetched
// here, with this browser's sign-ins, and both sent to `paperstack serve`,
// which tells which paper it is and files it.

const SERVER = "http://127.0.0.1:8765";

chrome.action.onClicked.addListener(async (tab) => {
  badge(tab.id, "…", "#777");
  try {
    const { token } = await chrome.storage.local.get("token");
    if (!token) {
      chrome.runtime.openOptionsPage();
      throw new Error(NO_TOKEN);
    }
    const [{ result: page }] = await chrome.scripting.executeScript({
      target: { tabId: tab.id },
      func: readPage,
    });
    let pdf = null;
    let pdfUrl = null;
    for (const url of page.pdfUrls) {
      const how = credentials(url, page.url);
      if (!how) continue;
      pdf = (await fetchHere(url, how, page.url)) || (how === "include" && (await fetchInPage(tab.id, url)));
      if (pdf) {
        pdfUrl = url;
        break;
      }
    }
    // The page as a line of JSON, then the PDF as it is: no base64, a third
    // smaller and copied fewer times than in a JSON string.
    const head = JSON.stringify({ url: page.url, tags: page.tags, pdf_url: pdfUrl });
    const body = pdf ? new Blob([head, "\n", pdf]) : head;
    const kind = pdf ? "application/x-paperstack-page" : "application/json";
    // Chrome stops a worker whose fetch takes over 30 s, and telling a paper
    // by its PDF can take longer: asking Chrome something now and then
    // keeps it going.
    const alive = setInterval(() => chrome.runtime.getPlatformInfo(() => {}), 20000);
    let res, out;
    try {
      res = await fetch(`${SERVER}/add`, {
        method: "POST",
        headers: { "Content-Type": kind, "X-Paperstack-Token": token },
        body,
      });
      out = await res.json();
    } finally {
      clearInterval(alive);
    }
    if (res.status === 401) {
      chrome.runtime.openOptionsPage();
      throw new Error(NO_TOKEN);
    }
    if (!res.ok) throw new Error(out.error || res.statusText);
    badge(tab.id, out.has_pdf ? "✓" : "✓·", "#2a7d2a");
    const what = { added: "Added", known: "Already there", published: "Now as published", "pdf-attached": "Given its PDF" };
    notify(`${what[out.outcome] || out.outcome}: ${out.key}`, `${out.title}${out.has_pdf ? "" : "\n(no PDF)"}`);
  } catch (e) {
    badge(tab.id, "!", "#b22");
    const why = String((e && e.message) || e);
    notify(
      "Not added",
      /Failed to fetch|NetworkError/.test(why) ? "Is `paperstack serve` running?" : why,
    );
  }
});

const NO_TOKEN = "Give the extension paperstack's token: `paperstack serve --print-token` prints it.";

// How the PDF at `url` may be fetched for the page at `from`: with this
// browser's sign-ins on the page's own site only; not at all on this machine
// or its network, but for a page there; elsewhere as anyone would, without
// them. A page is not to fetch, as the person, what only they may see.
function credentials(url, from) {
  const u = new URL(url);
  const f = new URL(from);
  if (u.hostname === f.hostname) return "include";
  if (local(u.hostname)) return null;
  return site(u.hostname) === site(f.hostname) ? "include" : "omit";
}

// A host on this machine or its network, as far as its name tells.
function local(host) {
  const h = host.replace(/^\[|\]$/g, "").toLowerCase();
  if (!h.includes(".") || /\.(localhost|local|internal|lan|home|corp|intranet)$/.test(h)) return true;
  const v4 = h.match(/^(\d+)\.(\d+)\.\d+\.\d+$/);
  if (!v4) return false;
  const [a, b] = [+v4[1], +v4[2]];
  return (
    a === 0 || a === 10 || a === 127 ||
    (a === 169 && b === 254) || (a === 172 && b >= 16 && b <= 31) ||
    (a === 192 && b === 168) || (a === 100 && b >= 64 && b <= 127)
  );
}

// A host's site, as `nature.com` of `www.nature.com`, or `ox.ac.uk`.
function site(host) {
  const p = host.split(".");
  const second = p.length >= 3 && p[p.length - 1].length === 2 && /^(ac|co|com|edu|gov|net|org)$/.test(p[p.length - 2]);
  return p.slice(second ? -3 : -2).join(".");
}

// Runs in the page: its meta tags, which name the paper, and where its PDF
// may be, best first.
function readPage() {
  const tags = [...document.querySelectorAll("meta[name], meta[property]")]
    .map((m) => [
      (m.getAttribute("name") || m.getAttribute("property")).toLowerCase(),
      m.getAttribute("content") || "",
    ])
    .filter(([, v]) => v.trim());
  const urls = [];
  const add = (u) => {
    try {
      const a = new URL(u, location.href);
      if (/^https?:$/.test(a.protocol) && !urls.includes(a.href)) urls.push(a.href);
    } catch (_) {}
  };
  if (document.contentType === "application/pdf") add(location.href);
  const meta = (tags.find(([k]) => k === "citation_pdf_url") || [])[1];
  // Wiley's and other Atypon sites' /doi/pdf/ is a reader; /doi/pdfdirect/
  // the file.
  if (meta && /\/doi\/e?pdf\//.test(meta)) add(meta.replace(/\/doi\/e?pdf\//, "/doi/pdfdirect/"));
  if (meta) add(meta);
  for (const a of document.querySelectorAll("a[href]")) {
    if (urls.length >= 6) break;
    const href = a.getAttribute("href");
    const text = (a.textContent || "").trim().toLowerCase();
    if (
      /\.pdf([?#]|$)/i.test(href) ||
      /^(download )?pdf$|download pdf|view pdf|full text pdf|article pdf/.test(text) ||
      a.matches(".c-pdf-download__link, .pdf-download, [data-track-action='download pdf']")
    ) {
      add(href);
    }
  }
  return { url: location.href, tags, pdfUrls: urls };
}

// The PDF at `url`, fetched with the browser's cookies or without, as its
// bytes; none where what comes is not a PDF, as a page asking to sign in,
// or comes from this machine or its network, sent there on the way.
async function fetchHere(url, how, from) {
  try {
    const r = await fetch(url, { credentials: how });
    if (!r.ok) return null;
    const at = new URL(r.url).hostname;
    if (at !== new URL(from).hostname && local(at)) return null;
    const b = new Uint8Array(await r.arrayBuffer());
    return isPdf(b) ? b : null;
  } catch (_) {
    return null;
  }
}

// The same, fetched by the page itself, for publishers that look at more
// than cookies. What a page gives back must be text: a data: address, made
// and read by the browser's own code, not a string built a letter at a time.
async function fetchInPage(tabId, url) {
  try {
    const [{ result }] = await chrome.scripting.executeScript({
      target: { tabId },
      args: [url],
      func: async (u) => {
        try {
          const r = await fetch(u, { credentials: "include" });
          if (!r.ok) return null;
          const blob = await r.blob();
          const head = new Uint8Array(await blob.slice(0, 4).arrayBuffer());
          if (String.fromCharCode(...head) !== "%PDF") return null;
          return await new Promise((done) => {
            const reader = new FileReader();
            reader.onload = () => done(reader.result);
            reader.onerror = () => done(null);
            reader.readAsDataURL(blob);
          });
        } catch (_) {
          return null;
        }
      },
    });
    if (!result) return null;
    const b = new Uint8Array(await (await fetch(result)).arrayBuffer());
    return isPdf(b) ? b : null;
  } catch (_) {
    return null;
  }
}

function isPdf(b) {
  return b.length > 4 && String.fromCharCode(...b.subarray(0, 4)) === "%PDF";
}

function badge(tabId, text, color) {
  chrome.action.setBadgeText({ tabId, text });
  chrome.action.setBadgeBackgroundColor({ tabId, color });
}

function notify(title, message) {
  chrome.notifications.create({ type: "basic", iconUrl: "icon.png", title, message });
}
