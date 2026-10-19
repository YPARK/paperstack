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
      pdf = (await fetchHere(url)) || (await fetchInPage(tab.id, url));
      if (pdf) {
        pdfUrl = url;
        break;
      }
    }
    const res = await fetch(`${SERVER}/add`, {
      method: "POST",
      headers: { "Content-Type": "application/json" },
      body: JSON.stringify({
        url: page.url,
        doi: page.doi,
        tags: page.tags,
        pdf_url: pdfUrl,
        pdf,
      }),
    });
    const out = await res.json();
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

// Runs in the page: its meta tags, its DOI, and where its PDF may be, best
// first.
function readPage() {
  const tags = [...document.querySelectorAll("meta[name], meta[property]")]
    .map((m) => [
      (m.getAttribute("name") || m.getAttribute("property")).toLowerCase(),
      m.getAttribute("content") || "",
    ])
    .filter(([, v]) => v.trim());
  const get = (n) => (tags.find(([k]) => k === n) || [])[1];
  const doi = get("citation_doi") || get("prism.doi") || get("dc.identifier.doi") || null;
  const urls = [];
  const add = (u) => {
    try {
      const a = new URL(u, location.href).href;
      if (!urls.includes(a)) urls.push(a);
    } catch (_) {}
  };
  if (document.contentType === "application/pdf") add(location.href);
  const meta = get("citation_pdf_url");
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
  return { url: location.href, doi, tags, pdfUrls: urls };
}

// The PDF at `url` fetched by the extension, with the browser's cookies, as
// base64; none where what comes is not a PDF, as a page asking to sign in.
async function fetchHere(url) {
  try {
    const r = await fetch(url, { credentials: "include", redirect: "follow" });
    if (!r.ok) return null;
    return pdfBase64(new Uint8Array(await r.arrayBuffer()));
  } catch (_) {
    return null;
  }
}

// The same, fetched by the page itself, for publishers that look at more
// than cookies.
async function fetchInPage(tabId, url) {
  try {
    const [{ result }] = await chrome.scripting.executeScript({
      target: { tabId },
      args: [url],
      func: async (u) => {
        try {
          const r = await fetch(u, { credentials: "include" });
          const b = new Uint8Array(await r.arrayBuffer());
          if (String.fromCharCode(...b.subarray(0, 4)) !== "%PDF") return null;
          let s = "";
          for (let i = 0; i < b.length; i += 0x8000) s += String.fromCharCode(...b.subarray(i, i + 0x8000));
          return btoa(s);
        } catch (_) {
          return null;
        }
      },
    });
    return result;
  } catch (_) {
    return null;
  }
}

function pdfBase64(bytes) {
  if (String.fromCharCode(...bytes.subarray(0, 4)) !== "%PDF") return null;
  let s = "";
  for (let i = 0; i < bytes.length; i += 0x8000) s += String.fromCharCode(...bytes.subarray(i, i + 0x8000));
  return btoa(s);
}

function badge(tabId, text, color) {
  chrome.action.setBadgeText({ tabId, text });
  chrome.action.setBadgeBackgroundColor({ tabId, color });
}

function notify(title, message) {
  chrome.notifications.create({ type: "basic", iconUrl: "icon.png", title, message });
}
