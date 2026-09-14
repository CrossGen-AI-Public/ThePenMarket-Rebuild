#!/usr/bin/env python3
"""Dead-link gate for a served, path-routed site. Fetches every route's HTML, then checks every
href / src / action: internal paths must answer 200 (301 to a 200 is fine), external URLs must
answer (HEAD, then GET; bot-blocked hosts are judged by a headless-Chrome page load), in-page anchors
must exist in that route's DOM, mailto/tel must be well formed, placeholders fail outright.
Usage: links_url.py <base-url> <report.md> <route> [route...]   (route "" = home)
"""
import os, re, subprocess, sys, urllib.request, urllib.error

base = sys.argv[1].rstrip("/")
report = sys.argv[2]
routes = [""] + [r for r in sys.argv[3:] if r]
UA = "Mozilla/5.0 (Macintosh; Intel Mac OS X 10_15_7) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/128.0.0.0 Safari/537.36"
CHROME = os.environ.get("CHROME", "/usr/bin/chromium-browser")
cache = {}
rows = []
fails = 0

def status(url, method="HEAD"):
    req = urllib.request.Request(url, headers={"User-Agent": UA}, method=method)
    try:
        with urllib.request.urlopen(req, timeout=25) as r:
            return r.status
    except urllib.error.HTTPError as e:
        return e.code
    except Exception:
        return 0

def check_external(u):
    if u in cache:
        return cache[u]
    c = status(u, "HEAD")
    if c in (403, 405, 0) or c >= 500:
        c = status(u, "GET")
    if c in (400, 401, 403, 405, 406, 429, 0) or c >= 500:
        try:
            dom = subprocess.run([CHROME, "--headless=new", "--disable-gpu", "--virtual-time-budget=8000", f"--user-agent={UA}", "--dump-dom", u], capture_output=True, text=True, timeout=60).stdout
            text = re.sub(r"<[^>]+>", " ", re.sub(r"(?is)<script\b.*?</script>", "", dom))
            if len(text) > 400 and not re.search(r"access denied|403 forbidden|404 not found|page not found|isn.t available|site can.t be reached|ERR_NAME_NOT_RESOLVED|ERR_CONNECTION", text, re.I):
                c = "200-chrome"
        except Exception:
            pass
    cache[u] = c
    return c

def check_internal(path):
    u = f"{base}{path}"
    if u in cache:
        return cache[u]
    c = status(u, "GET")
    cache[u] = c
    return c

for r in routes:
    url = f"{base}/{r}"
    try:
        with urllib.request.urlopen(urllib.request.Request(url, headers={"User-Agent": UA}), timeout=30) as resp:
            html = resp.read().decode("utf-8", "replace")
    except Exception as e:
        rows.append((r, url, f"FAIL fetch {e}")); fails += 1
        continue
    ids = set(re.findall(r'\sid="([^"]+)"', html))
    clean = re.sub(r"(?is)<script\b.*?</script>|<template\b.*?</template>|<style\b.*?</style>", "", html)
    clean = re.sub(r'(?i)<link\b[^>]*rel="(preconnect|dns-prefetch|preload|modulepreload)"[^>]*>', "", clean)
    targets = sorted(set(re.findall(r'(?:href|src|action|data-href)="([^"]*)"', clean)))
    for t in targets:
        t = t.replace("&amp;", "&")
        res = "ok"
        if t.strip() == "":
            res = "FAIL placeholder (empty)"
        elif t in ("#", "#!") or t.startswith("javascript:") or "TODO" in t or "lorem" in t.lower() or "example.com" in t or "placeholder" in t.lower():
            res = "FAIL placeholder"
        elif t.startswith("mailto:"):
            res = "ok" if re.match(r"^mailto:[^@\s]+@[^@\s]+\.[a-z]{2,}(\?.*)?$", t) else "FAIL bad mailto"
        elif t.startswith("tel:"):
            res = "ok" if re.match(r"^tel:\+?[0-9().\s-]{7,}$", t) else "FAIL bad tel"
        elif t.startswith("#"):
            res = "ok" if t[1:] in ids else f"FAIL missing anchor {t}"
        elif t.startswith("http://") or t.startswith("https://"):
            if t.startswith(base + "/") or t == base:
                c = check_internal(t[len(base):] or "/")
            else:
                c = check_external(t)
            res = f"ok {c}" if str(c).startswith(("2", "3")) else f"FAIL {c}"
        elif t.startswith("data:") or t.startswith("blob:"):
            res = "ok"
        else:
            path = t if t.startswith("/") else "/" + t
            path = path.split("#")[0]
            c = check_internal(path)
            res = f"ok {c}" if str(c).startswith(("2", "3")) else f"FAIL {c}"
        if res.startswith("FAIL"):
            fails += 1
            print(f"FAIL  [/{r}] {t}  ({res})")
        rows.append((r, t, res))

with open(report, "w") as f:
    f.write(f"# Link report: {base}\n\n| route | target | result |\n|---|---|---|\n")
    for r, t, res in rows:
        f.write(f"| /{r} | `{t.replace('|', '/')}` | {res} |\n")
    f.write(f"\n{len(rows) - fails} ok, {fails} failed\n")
print(f"links: {len(rows) - fails} ok, {fails} failed -> {report}")
sys.exit(1 if fails else 0)
