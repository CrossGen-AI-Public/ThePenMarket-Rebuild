#!/usr/bin/env python3
"""Recover blog images that are 404 on the live site (and absent from the Aug 24 backup)
from the Wayback Machine. Reads the list of missing originals produced by the import prep,
tries the original and the size-suffixed variants the posts reference, and saves whatever
answers as an image into server/media/uploads/<rel>. Never writes into backup/.

Usage: recover-blog-media.py <missing-list.txt> <out-dir> [log.tsv]
"""
import sys, os, re, json, time, urllib.request, urllib.error

lst, out = sys.argv[1], sys.argv[2]
log = sys.argv[3] if len(sys.argv) > 3 else os.path.join(out, "recovery.tsv")
os.makedirs(out, exist_ok=True)
UA = "Mozilla/5.0 (compatible; thepenmarket-rebuild-media-recovery/1.0)"
PREFIXES = ["https://thepenmarket.com/vintage-pens-blog/wp-content/uploads/", "https://thepenmarket.com/wp-content/uploads/"]
YEARS = ["2025", "2024", "2023", "2021", "2019", "2017", "2015"]

# sized variants referenced by posts, per original
variants = {}
for f in ("backup/database/posts.ndjson", "backup/database/pages.ndjson"):
    if not os.path.exists(f):
        continue
    for line in open(f):
        d = json.loads(line)
        for u in re.findall(r'<img[^>]+src="([^"]+)"', d["content"]["rendered"]):
            if "wp-content/uploads/" in u:
                rel = u.split("wp-content/uploads/")[1].split("?")[0]
                orig = re.sub(r"-\d+x\d+(\.[a-z]+)$", r"\1", rel)
                variants.setdefault(orig, set()).add(rel)

def fetch(url):
    req = urllib.request.Request(url, headers={"User-Agent": UA})
    try:
        with urllib.request.urlopen(req, timeout=40) as r:
            ct = r.headers.get("Content-Type", "")
            data = r.read()
            if ct.startswith("image/") and len(data) > 500:
                return data
    except Exception:
        return None
    return None

done = 0; failed = 0
with open(log, "a") as lg:
    for orig in [l.strip() for l in open(lst) if l.strip()]:
        targets = [orig] + sorted(v for v in variants.get(orig, set()) if v != orig)
        got_any = False
        for rel in targets:
            dest = os.path.join(out, rel)
            if os.path.exists(dest):
                got_any = True; continue
            data = None; src = ""
            for y in YEARS:
                for p in PREFIXES:
                    url = f"https://web.archive.org/web/{y}id_/{p}{rel}"
                    data = fetch(url)
                    if data:
                        src = url; break
                if data:
                    break
                time.sleep(0.3)
            if data:
                os.makedirs(os.path.dirname(dest), exist_ok=True)
                open(dest, "wb").write(data)
                lg.write(f"ok\t{rel}\t{len(data)}\t{src}\n"); lg.flush(); got_any = True
            else:
                lg.write(f"miss\t{rel}\t0\t\n"); lg.flush()
            time.sleep(0.4)
        done += 1 if got_any else 0
        failed += 0 if got_any else 1
print(f"recovered originals-or-variants for {done}, nothing for {failed}")
