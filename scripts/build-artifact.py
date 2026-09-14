#!/usr/bin/env python3
"""Build dist/index.html: the rendered homepage plus the assistant in claude.ai `sample` mode, with
the catalog snapshot inlined so it works on claude.ai without the server. Every root-relative link
becomes absolute to the deployed site; product photos point at the client's own public originals.

Usage: build-artifact.py [server-base=http://127.0.0.1:8140] [public-base=http://100.117.164.79:8140]
"""
import base64, json, mimetypes, os, re, sys, urllib.request

server = (sys.argv[1] if len(sys.argv) > 1 else "http://127.0.0.1:8140").rstrip("/")
public = (sys.argv[2] if len(sys.argv) > 2 else "http://100.117.164.79:8140").rstrip("/")
root = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
srv = os.path.join(root, "server")

def fetch(path):
    with urllib.request.urlopen(f"{server}{path}", timeout=60) as r:
        return r.read().decode("utf-8")

def data_uri(path):
    p = os.path.join(srv, "static", path.lstrip("/").replace("static/", "", 1))
    mime = mimetypes.guess_type(p)[0] or "application/octet-stream"
    return f"data:{mime};base64,{base64.b64encode(open(p, 'rb').read()).decode()}"

html = fetch("/")
catalog = json.loads(fetch("/api/guide/catalog.json"))

def live_original(media_path):
    """/media/uploads/2026/08/6967-Montblanc-644-480.jpg -> https://thepenmarket.com/wp-content/uploads/2026/08/6967-Montblanc-644.jpg"""
    rel = media_path.split("/media/uploads/", 1)[1] if "/media/uploads/" in media_path else None
    if not rel:
        return media_path
    rel = re.sub(r"-(480|960)(\.[a-z]+)$", r"\1", rel)
    return f"https://thepenmarket.com/wp-content/uploads/{rel}"

for p in catalog["pens"]:
    if p.get("image"):
        p["image"] = live_original(p["image"])

# 1. images: media -> the client's public originals
html = re.sub(r'(src|href)="(/media/uploads/[^"]+)"', lambda m: f'{m.group(1)}="{live_original(m.group(2))}"', html)
# 2. links: root-relative -> the deployed site; in-page anchors stay
html = re.sub(r'href="/(?!/)([^"#]*)(#[^"]*)?"', lambda m: f'href="{public}/{m.group(1)}{m.group(2) or ""}"', html)
html = re.sub(r'action="/([^"]*)"', lambda m: f'action="{public}/{m.group(1)}"', html)
html = html.replace(f'href="{public}/#ask"', 'href="#ask"')
# 3. head: fonts from Google (the self-hosted files are not reachable from claude.ai), CSS inline
fonts_link = '<link rel="preconnect" href="https://fonts.googleapis.com"><link rel="preconnect" href="https://fonts.gstatic.com" crossorigin><link rel="stylesheet" href="https://fonts.googleapis.com/css2?family=Cormorant+Garamond:ital,wght@0,500;0,600;1,500&family=Archivo:wght@400;500;600;700&family=IBM+Plex+Mono:wght@400;500;600&display=swap">'
html = html.replace('<link rel="stylesheet" href="/static/fonts/fonts.css">', fonts_link)
css = open(os.path.join(srv, "static", "css", "site.css"), encoding="utf-8").read()
css = css.replace("url(/static/img/hero-photo.jpg)", f"url({data_uri('img/hero-photo.jpg')})")
html = html.replace('<link rel="stylesheet" href="/static/css/site.css">', f"<style>\n{css}\n</style>")
html = re.sub(r'<link rel="preload" as="image" href="[^"]+">', "", html)
html = html.replace('href="/static/img/favicon.png"', f'href="{data_uri("img/favicon.png")}"')
html = html.replace('href="/static/img/apple-touch-icon.png"', f'href="{data_uri("img/apple-touch-icon.png")}"')
html = re.sub(r'content="http[^"]*/static/img/og-default.jpg"', f'content="{public}/static/img/og-default.jpg"', html)
# 4. scripts: inline ours, three.js from cdnjs, catalog + mode flags before the guide
def inline(name):
    js = open(os.path.join(srv, "static", "js", name), encoding="utf-8").read().replace("</script>", "<\\/script>")
    return f"<script>\n{js}\n</script>"
html = html.replace('<script src="/static/js/site.js" defer></script>', inline("site.js"))
html = html.replace('<script src="/static/vendor/three.min.js" defer></script>', '<script src="https://cdnjs.cloudflare.com/ajax/libs/three.js/0.160.0/three.min.js"></script>')
html = html.replace('<script src="/static/js/hero.js" defer></script>', inline("hero.js"))
snapshot = json.dumps(catalog).replace("</", "<\\/")
html = html.replace('<script src="/static/js/engine.js" defer></script>', f"<script>window.PEN_CATALOG = {snapshot}; window.PEN_SITE_BASE = {json.dumps(public)}; window.PEN_NO_SERVER = true;</script>\n" + inline("engine.js"))
html = html.replace('<script src="/static/js/guide.js" defer></script>', inline("guide.js"))
# 5. the canonical stays the deployed site; mark the artifact
html = html.replace("<head>", "<head>\n<!-- ThePenMarket.com rebuild: shareable preview built from the Rust site by scripts/build-artifact.py. The catalog snapshot is inlined; the assistant runs in claude.ai sample mode. -->", 1)
assert "/static/" not in re.sub(r"https?://[^\"' )]+", "", html), "unresolved /static/ reference"
os.makedirs(os.path.join(root, "dist"), exist_ok=True)
out = os.path.join(root, "dist", "index.html")
open(out, "w", encoding="utf-8").write(html)
print(f"wrote {out}: {len(html):,} bytes, {len(catalog['pens'])} pens inlined")
