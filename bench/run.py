"""How well `paperstack add file.pdf` tells which paper each PDF in
bench/data is: added to a library of its own, the title it is filed under
compared with the paper's.

    cargo build --release && python3 bench/run.py [path/to/paperstack]
"""

import json
import re
import subprocess
import sys
import tempfile
import time
import unicodedata
from collections import defaultdict
from pathlib import Path

DATA = Path(__file__).parent / "data"
BIN = sys.argv[1] if len(sys.argv) > 1 else str(
    Path(__file__).parent.parent / "target/release/paperstack")


def words(s):
    s = unicodedata.normalize("NFKD", s).encode("ascii", "ignore").decode().lower()
    return re.findall(r"[a-z0-9]+", s)


def main():
    truth = [line.rstrip("\n").split("\t") for line in open(DATA / "truth.tsv")]
    lib = Path(tempfile.mkdtemp()) / "lib"
    subprocess.run([BIN, "-L", lib, "init"], check=True, capture_output=True)
    tally = defaultdict(lambda: [0, 0])
    hows = defaultdict(int)
    for name, src, id_, title in truth:
        t0 = time.time()
        r = subprocess.run([BIN, "-L", lib, "add", DATA / name, "--json"],
                           capture_output=True, text=True)
        secs = time.time() - t0
        out = json.loads(r.stdout or "[{}]")[0]
        if "key" in out:
            got = subprocess.run([BIN, "-L", lib, "get", out["key"], "--json"],
                                 capture_output=True, text=True)
            filed = json.loads(got.stdout)[0]["title"]
            ok = "".join(words(filed)) == "".join(words(title))
            how = re.sub(r"(DOI|arXiv id|PMID) \S+", r"\1", out["how"])
            verdict = "right" if ok else f"WRONG: {filed[:50]}"
        else:
            ok, how = False, "-"
            verdict = "not added: " + out.get("error", r.stderr).splitlines()[0][:60]
        tally[src][0] += ok
        tally[src][1] += 1
        hows[how] += 1
        print(f"{name:12} {secs:5.1f}s  {how[:28]:28}  {verdict}")
    print()
    for src, (ok, n) in tally.items():
        print(f"{src:8} {ok}/{n}")
    ok = sum(v[0] for v in tally.values())
    n = sum(v[1] for v in tally.values())
    print(f"{'all':8} {ok}/{n}")
    print("by:", dict(hows))


if __name__ == "__main__":
    main()
