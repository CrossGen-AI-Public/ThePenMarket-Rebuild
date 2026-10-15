#!/usr/bin/env python3
"""Every address the live thepenmarket.com lists in its sitemaps today, and what the new site does
with it: the same page (200), a permanent redirect (301 -> where), or nothing (404).

  python3 scripts/url_audit.py <new-site-base> [out.csv] [--live https://thepenmarket.com]

Run it at cutover against the new site: any 404 row is an old address that needs a redirect (paste
it into /admin/redirects/) or a pen listed since the last import. Read-only on both sites.
"""
import csv
import re
import sys
import urllib.error
import urllib.parse
import urllib.request

args = [a for a in sys.argv[1:] if not a.startswith("--")]
LIVE = sys.argv[sys.argv.index("--live") + 1] if "--live" in sys.argv else "https://thepenmarket.com"
if "--live" in sys.argv:
    args.remove(LIVE)
NEW = args[0].rstrip("/")
OUT = args[1] if len(args) > 1 else "url-audit.csv"
UA = {"User-Agent": "ThePenMarket-rebuild-url-audit (CrossGen)"}


class NoRedirect(urllib.request.HTTPRedirectHandler):
    def redirect_request(self, *a, **k):
        return None


plain = urllib.request.build_opener()
noredir = urllib.request.build_opener(NoRedirect())


def fetch(url):
    with plain.open(urllib.request.Request(url, headers=UA), timeout=30) as r:
        return r.read().decode("utf-8", "replace")


def locs(xml):
    return [urllib.parse.unquote(m).replace("&amp;", "&") for m in re.findall(r"<loc>\s*([^<\s]+)\s*</loc>", xml)]


def sitemap_urls():
    seen, pages = set(), []
    queue = []
    for index in ("/sitemap_index.xml", "/wp-sitemap.xml", "/sitemap.xml"):
        try:
            queue = locs(fetch(LIVE + index))
            if queue:
                break
        except urllib.error.URLError:
            continue
    while queue:
        sm = queue.pop(0)
        if sm in seen:
            continue
        seen.add(sm)
        try:
            body = fetch(sm)
        except urllib.error.URLError as e:
            print(f"skip {sm}: {e}", file=sys.stderr)
            continue
        for u in locs(body):
            (queue if u.endswith(".xml") else pages).append(u)
    return sorted(set(pages))


def land(path):
    """(first status, final status, final path) after up to five hops on the new site."""
    first, at = None, path
    for _ in range(5):
        try:
            r = noredir.open(urllib.request.Request(NEW + urllib.parse.quote(at, safe="/?=&%:+,;@"), headers=UA), timeout=30)
            code, loc = r.status, None
        except urllib.error.HTTPError as e:
            code, loc = e.code, e.headers.get("Location")
        first = first or code
        if code in (301, 302, 307, 308) and loc:
            u = urllib.parse.urlparse(loc)
            at = u.path + (f"?{u.query}" if u.query else "")
            continue
        return first, code, at
    return first, 0, at


urls = sitemap_urls()
counts = {"same page": 0, "redirects": 0, "missing": 0}
with open(OUT, "w", newline="") as f:
    w = csv.writer(f)
    w.writerow(["old_address", "on_new_site", "goes_to", "final_status"])
    for u in urls:
        p = urllib.parse.urlparse(u)
        path = (p.path or "/") + (f"?{p.query}" if p.query else "")
        first, final, at = land(path)
        if final != 200:
            kind = "missing"
        elif first == 200 or at.rstrip("/") == path.rstrip("/"):
            kind = "same page"
        else:
            kind = "redirects"
        counts[kind] += 1
        w.writerow([u, kind, "" if kind == "same page" else at, final])
print(f"{len(urls)} addresses in {LIVE}'s sitemaps: {counts['same page']} same page, {counts['redirects']} redirect, {counts['missing']} missing -> {OUT}")
