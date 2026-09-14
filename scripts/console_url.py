#!/usr/bin/env python3
"""Console gate for a served site: load every route at desktop and phone widths in Playwright
Chromium with WebGL on (SwiftShader) and fail on any console error, uncaught exception, failed
request, or THREE / WebGL / shader complaint. Usage: console_url.py <base-url> <route> [route...]
"""
import sys
from playwright.sync_api import sync_playwright

base = sys.argv[1].rstrip("/")
routes = [""] + [r for r in sys.argv[2:] if r]
widths = [1440, 500]
total = 0
with sync_playwright() as p:
    b = p.chromium.launch(headless=True, args=["--use-angle=swiftshader", "--enable-unsafe-swiftshader", "--ignore-gpu-blocklist"])
    for w in widths:
        ctx = b.new_context(viewport={"width": w, "height": 1000})
        for r in routes:
            pg = ctx.new_page()
            bad = []
            pg.on("console", lambda m, bad=bad: bad.append(f"console.{m.type}: {m.text[:160]}") if m.type == "error" or any(k in m.text for k in ("THREE.", "WebGL", "shader", "GLSL")) else None)
            pg.on("pageerror", lambda e, bad=bad: bad.append(f"pageerror: {str(e)[:160]}"))
            pg.on("requestfailed", lambda req, bad=bad: bad.append(f"requestfailed: {req.url[:120]} {req.failure}"))
            pg.on("response", lambda res, bad=bad: bad.append(f"http {res.status}: {res.url[:120]}") if res.status >= 400 else None)
            try:
                pg.goto(f"{base}/{r}", wait_until="load", timeout=60000)
                pg.wait_for_timeout(2500)
            except Exception as e:
                bad.append(f"load: {str(e)[:120]}")
            pg.close()
            total += len(bad)
            print(f"width {w:<5} route /{r:<48} {'clean' if not bad else str(len(bad)) + ' errors'}")
            for x in bad[:8]:
                print("   " + x)
        ctx.close()
    b.close()
print(f"console errors: {total}")
sys.exit(1 if total else 0)
