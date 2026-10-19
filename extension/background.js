// A click on the button: the page's tags and PDF links read, the PDF fetched
// here, with this browser's sign-ins, and both sent to `paperstack serve`,
// which tells which paper it is and files it.

const SERVER = "http://127.0.0.1:8765";

chrome.action.onClicked.addListener(async (tab) => {
  badge(tab.id, "…", "#777");
  try {
    const [{ result: page }] = await chrome.scripting.executeScript({
      target: { tabId: tab.id },
      func: readPage,
    });
    let pdf = null;
    let pdfUrl = null;
    for (const url of page.pdfUrls) {
      pdf = (await fetchPdf(url)) || (await fetchInPage(tab.id, url));
      if (pdf) {
        pdfUrl = url;
        break;
      }
    }
    // Chrome stops a worker whose fetch takes over 30 s, and telling a paper
    // by its PDF can take longer: asking Chrome something now and then
    // keeps it going.
    const alive = setInterval(() => chrome.runtime.getPlatformInfo(() => {}), 20000);
    let res, out;
    try {
      res = await fetch(`${SERVER}/add`, {
        method: "POST",
        headers: { "Content-Type": "application/json" },
        body: JSON.stringify({ url: page.url, tags: page.tags, pdf_url: pdfUrl, pdf }),
      });
      out = await res.json();
    } finally {
      clearInterval(alive);
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
      const a = new URL(u, location.href).href;
      if (!urls.includes(a)) urls.push(a);
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

// The PDF at `url`, fetched with the browser's cookies, as base64; none where
// what comes is not a PDF, as a page asking to sign in. Self-contained: it
// also runs in the page, where nothing outside it is in reach.
async function fetchPdf(url) {
  try {
    const r = await fetch(url, { credentials: "include" });
    if (!r.ok) return null;
    const b = new Uint8Array(await r.arrayBuffer());
    if (String.fromCharCode(...b.subarray(0, 4)) !== "%PDF") return null;
    let s = "";
    for (let i = 0; i < b.length; i += 0x8000) s += String.fromCharCode(...b.subarray(i, i + 0x8000));
    return btoa(s);
  } catch (_) {
    return null;
  }
}

// The same, fetched by the page itself, for publishers that look at more
// than cookies.
async function fetchInPage(tabId, url) {
  try {
    const [{ result }] = await chrome.scripting.executeScript({ target: { tabId }, args: [url], func: fetchPdf });
    return result;
  } catch (_) {
    return null;
  }
}

function badge(tabId, text, color) {
  chrome.action.setBadgeText({ tabId, text });
  chrome.action.setBadgeBackgroundColor({ tabId, color });
}

function notify(title, message) {
  chrome.notifications.create({ type: "basic", iconUrl: "icon.png", title, message });
}
