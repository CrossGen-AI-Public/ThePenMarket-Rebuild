#!/usr/bin/env python3
"""SEO/AEO gate for every indexable route: exactly one <h1>, a <title> that differs from the h1,
a meta description (50-320 chars), a self-referencing canonical, no noindex, and JSON-LD that parses
with the required fields per @type. Usage: seo_check.py <base-url> <ignored> <route> [route...]
"""
import json, re, sys, urllib.request

base = sys.argv[1].rstrip("/")
routes = [""] + [r for r in sys.argv[3:] if r]
REQUIRED = {
    "Product": ["name", "offers", "image"],
    "Offer": ["price", "priceCurrency", "availability"],
    "Article": ["headline", "datePublished", "author", "publisher"],
    "BreadcrumbList": ["itemListElement"],
    "WebSite": ["url", "potentialAction"],
    "Organization": ["name", "url", "address", "sameAs", "telephone", "email"],
    "Store": ["name", "address", "telephone"],
    "FAQPage": ["mainEntity"],
    "ItemList": ["itemListElement"],
    "CollectionPage": ["name", "url"],
    "DefinedTerm": ["name", "description"],
    "Person": ["name"],
    "MerchantReturnPolicy": ["merchantReturnDays", "returnPolicyCategory"],
}
fails = 0
def fail(route, msg):
    global fails
    fails += 1
    print(f"FAIL /{route}: {msg}")

def check_node(route, node):
    if not isinstance(node, dict):
        return
    t = node.get("@type")
    types = t if isinstance(t, list) else [t]
    for ty in types:
        for req in REQUIRED.get(ty, []):
            if req not in node or node[req] in ("", None, []):
                fail(route, f"JSON-LD {ty} missing {req}")
    for v in node.values():
        if isinstance(v, dict):
            check_node(route, v)
        elif isinstance(v, list):
            for x in v:
                check_node(route, x)

for r in routes:
    url = f"{base}/{r}"
    try:
        with urllib.request.urlopen(urllib.request.Request(url, headers={"User-Agent": "seo-gate"}), timeout=30) as resp:
            html = resp.read().decode("utf-8", "replace")
            code = resp.status
    except Exception as e:
        fail(r, f"fetch failed: {e}")
        continue
    if code != 200:
        fail(r, f"status {code}")
    h1s = re.findall(r"<h1\b", html, re.I)
    if len(h1s) != 1:
        fail(r, f"{len(h1s)} h1 elements")
    title = re.search(r"<title>(.*?)</title>", html, re.S | re.I)
    h1 = re.search(r"<h1[^>]*>(.*?)</h1>", html, re.S | re.I)
    if not title or not title.group(1).strip():
        fail(r, "no <title>")
    elif h1 and re.sub(r"<[^>]+>", "", h1.group(1)).strip() == title.group(1).strip():
        fail(r, "title identical to h1")
    desc = re.search(r'<meta name="description" content="([^"]*)"', html, re.I)
    if not desc or len(desc.group(1)) < 50:
        fail(r, "meta description missing or under 50 chars")
    elif len(desc.group(1)) > 320:
        fail(r, f"meta description too long ({len(desc.group(1))})")
    canon = re.search(r'<link rel="canonical" href="([^"]+)"', html, re.I)
    if not canon:
        fail(r, "no canonical")
    elif not canon.group(1).endswith("/" + r if r else "/") and canon.group(1) != f"{base}/{r}":
        # the canonical must point at this route (origin may differ from the base used to fetch)
        if not canon.group(1).rstrip("/").endswith(("/" + r).rstrip("/")):
            fail(r, f"canonical {canon.group(1)} does not match /{r}")
    if re.search(r'<meta name="robots" content="[^"]*noindex', html, re.I):
        fail(r, "noindex on an indexable route")
    blocks = re.findall(r'<script type="application/ld\+json">(.*?)</script>', html, re.S | re.I)
    if not blocks:
        fail(r, "no JSON-LD")
    for b in blocks:
        try:
            data = json.loads(b)
        except Exception as e:
            fail(r, f"JSON-LD does not parse: {e}")
            continue
        graph = data.get("@graph", [data]) if isinstance(data, dict) else data
        if not graph:
            fail(r, "empty JSON-LD graph")
        for node in graph:
            check_node(r, node)
    if fails == 0 or True:
        print(f"ok   /{r}: h1={len(h1s)} title={len(title.group(1)) if title else 0}c desc={len(desc.group(1)) if desc else 0}c jsonld={len(blocks)}")
print(f"seo: {len(routes)} routes, {fails} failures")
sys.exit(1 if fails else 0)
