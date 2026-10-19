"""Download open-access PDFs whose papers are known, to see how well
`paperstack add file.pdf` tells which paper a PDF is.

    python3 bench/fetch.py            # into bench/data, with truth.tsv

Each source tests something else: arXiv's PDFs carry their id down the
margin; bioRxiv's, PLOS's and eLife's a DOI on the first page; PMLR's
(ICML's proceedings) neither, so only their titles tell them.
"""

import json
import re
import sys
import time
import urllib.parse
import urllib.request
from pathlib import Path

DATA = Path(__file__).parent / "data"
UA = ("Mozilla/5.0 (Macintosh; Intel Mac OS X 10_15_7) AppleWebKit/537.36 "
      "(KHTML, like Gecko) Chrome/141.0 Safari/537.36")
EACH = int(sys.argv[1]) if len(sys.argv) > 1 else 6


def get(url, accept="*/*"):
    req = urllib.request.Request(url, headers={"User-Agent": UA, "Accept": accept})
    with urllib.request.urlopen(req, timeout=60) as r:
        return r.read()


def arxiv():
    cats = ["q-bio.GN", "stat.ME", "cs.LG", "astro-ph.GA", "math.ST", "physics.bio-ph"]
    out = []
    for cat in cats[:EACH]:
        q = urllib.parse.urlencode({"search_query": f"cat:{cat}", "max_results": 1,
                                    "start": 40, "sortBy": "submittedDate"})
        atom = get(f"https://export.arxiv.org/api/query?{q}").decode()
        for e in atom.split("<entry>")[1:]:
            id_ = re.search(r"<id>[^<]*/abs/([^<v]+)", e).group(1)
            title = " ".join(re.search(r"<title>(.*?)</title>", e, re.S).group(1).split())
            out.append(("arxiv", title, f"arXiv:{id_}", f"https://arxiv.org/pdf/{id_}"))
        time.sleep(3)
    return out


def biorxiv():
    d = json.loads(get("https://api.biorxiv.org/details/biorxiv/2025-06-02/2025-06-02/0"))
    out = []
    for p in d["collection"][::5][:EACH]:
        out.append(("biorxiv", p["title"], p["doi"],
                    f"https://www.biorxiv.org/content/{p['doi']}v{p['version']}.full.pdf"))
    return out


def crossref(prefix, n):
    q = urllib.parse.urlencode({"filter": f"prefix:{prefix},type:journal-article,"
                                "from-pub-date:2024-03-01,until-pub-date:2024-03-31",
                                "select": "DOI,title", "sample": n})
    d = json.loads(get(f"https://api.crossref.org/works?{q}"))
    return [(w["DOI"], w["title"][0]) for w in d["message"]["items"] if w.get("title")]


def plos():
    journals = {"pone": "plosone", "pcbi": "ploscompbiol", "pgen": "plosgenetics",
                "pbio": "plosbiology"}
    out = []
    for doi, title in crossref("10.1371", EACH):
        j = journals.get(doi.split(".")[2], "plosone")
        out.append(("plos", title, doi,
                    f"https://journals.plos.org/{j}/article/file?id={doi}&type=printable"))
    return out


def elife():
    out = []
    for doi, title in crossref("10.7554", EACH):
        n = re.sub(r"\D", "", doi.lower().split("elife.")[-1].split(".")[0]).zfill(5)
        out.append(("elife", title, doi,
                    f"https://cdn.elifesciences.org/articles/{n}/elife-{n}-v1.pdf"))
    return out


def pmlr():
    html = get("https://proceedings.mlr.press/v235/").decode()
    papers = re.findall(r'<p class="title">(.*?)</p>.*?href="(https://raw[^"]+\.pdf)"', html, re.S)
    step = max(1, len(papers) // EACH)
    return [("pmlr", " ".join(t.split()), "-", pdf) for t, pdf in papers[::step][:EACH]]


def main():
    DATA.mkdir(exist_ok=True)
    truth = DATA / "truth.tsv"
    # A PDF kept from before keeps the paper it was downloaded as.
    known = {}
    if truth.exists():
        for line in open(truth):
            row = line.rstrip("\n").split("\t")
            known[row[0]] = tuple(row)
    rows = []
    for source in (arxiv, biorxiv, plos, elife, pmlr):
        try:
            papers = source()
        except Exception as e:
            print(f"{source.__name__}: {e}", file=sys.stderr)
            continue
        for i, (src, title, id_, url) in enumerate(papers):
            name = f"{src}-{i}.pdf"
            path = DATA / name
            fresh = not path.exists()
            try:
                if fresh:
                    pdf = get(url, "application/pdf")
                    if not pdf.startswith(b"%PDF"):
                        raise ValueError("not a PDF")
                    path.write_bytes(pdf)
                    time.sleep(2)
                row = known.get(name) if name in known and not fresh else (name, src, id_, title)
                rows.append(row)
                print(f"{name}  {row[3][:70]}")
            except Exception as e:
                print(f"{name}: {url}: {e}", file=sys.stderr)
    with open(truth, "w") as f:
        for r in rows:
            f.write("\t".join(r) + "\n")
    print(f"{len(rows)} PDFs in {DATA}")


if __name__ == "__main__":
    main()
