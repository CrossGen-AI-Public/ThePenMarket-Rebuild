#!/usr/bin/env python3
"""Overflow sweep for a served site: load every route at real phone and desktop widths (Playwright
Chromium, so widths below Chrome's 500px window floor are real viewports) and fail on any route whose
document is wider than the viewport. Usage: sweep_url.py <base-url> <route> [route...]
"""
import sys
from playwright.sync_api import sync_playwright

base = sys.argv[1].rstrip("/")
routes = [""] + [r for r in sys.argv[2:] if r]
widths = [320, 360, 390, 430, 600, 768, 900, 1024, 1100, 1200, 1300]
JS = """() => {
  const d = document, w = window, vw = d.documentElement.clientWidth;
  const sw = Math.max(d.documentElement.scrollWidth, d.body.scrollWidth);
  const clipped = (e) => { for (let a = e.parentElement; a && a !== d.body && a !== d.documentElement; a = a.parentElement) { const o = w.getComputedStyle(a); if (/hidden|clip/.test(o.overflowX) || /hidden|clip/.test(o.overflow)) return true; } return false; };
  const bad = [];
  d.querySelectorAll('body *').forEach(e => { const r = e.getBoundingClientRect(); const cs = w.getComputedStyle(e); if (cs.display === 'none' || cs.position === 'fixed') return; const offLeft = cs.position === 'absolute' && r.right <= 0; if (offLeft) return; if ((r.right > vw + 1 || r.left < -1) && r.width > 0 && !clipped(e)) bad.push(e.tagName.toLowerCase() + (e.id ? '#' + e.id : '') + '.' + [...e.classList].slice(0, 3).join('.') + ' R' + Math.round(r.right)); });
  return { vw, sw, bad: bad.slice(0, 6) };
}"""
total = 0
with sync_playwright() as p:
    b = p.chromium.launch(headless=True, args=["--use-angle=swiftshader", "--enable-unsafe-swiftshader", "--ignore-gpu-blocklist"])
    for wdt in widths:
        ctx = b.new_context(viewport={"width": wdt, "height": 900}, device_scale_factor=1)
        pg = ctx.new_page()
        n_bad = 0
        for r in routes:
            try:
                pg.goto(f"{base}/{r}", wait_until="load", timeout=60000)
                pg.wait_for_timeout(600)
                res = pg.evaluate(JS)
                if res["sw"] > res["vw"] or res["bad"]:
                    n_bad += 1
                    print(f"   OVERFLOW width {wdt} /{r} vw={res['vw']} scrollW={res['sw']} :: {' | '.join(res['bad'])}")
            except Exception as e:
                n_bad += 1
                print(f"   ERROR width {wdt} /{r}: {str(e)[:120]}")
        ctx.close()
        total += n_bad
        print(f"width {wdt}: {len(routes)} routes, {n_bad} overflow")
    b.close()
print(f"total overflow: {total}")
sys.exit(1 if total else 0)
