#!/usr/bin/env python3
"""Admin gate: sign in as the test account, pass the new-device code (read from the mail log),
open a product page with editing on, change the title inline and change it back, toggle preview,
and fail on any console error or failed request along the way. Screenshots go next to the other
gate output. Usage: ADMIN_TEST_EMAIL=… ADMIN_TEST_PASSWORD=… admin_check.py <base-url> <mail-log> <out-dir>
"""
import os, re, sys, time
from playwright.sync_api import sync_playwright

base, mail_log, out = sys.argv[1].rstrip("/"), sys.argv[2], sys.argv[3]
email, password = os.environ["ADMIN_TEST_EMAIL"], os.environ["ADMIN_TEST_PASSWORD"]
os.makedirs(out, exist_ok=True)
bad = []

def latest_code():
    text = open(mail_log, encoding="utf-8", errors="replace").read()
    codes = re.findall(r"code is: (\d{6})", text)
    return codes[-1] if codes else None

with sync_playwright() as p:
    b = p.chromium.launch(headless=True, args=["--use-angle=swiftshader", "--enable-unsafe-swiftshader", "--ignore-gpu-blocklist"])
    ctx = b.new_context(viewport={"width": 1440, "height": 1000})
    pg = ctx.new_page()
    pg.on("console", lambda m: bad.append(f"console.{m.type}: {m.text[:160]}") if m.type == "error" else None)
    pg.on("pageerror", lambda e: bad.append(f"pageerror: {str(e)[:160]}"))
    pg.on("requestfailed", lambda r: bad.append(f"requestfailed: {r.url[:120]}"))
    pg.on("response", lambda r: bad.append(f"http {r.status}: {r.url[:120]}") if r.status >= 500 else None)

    pg.goto(f"{base}/admin/login/", wait_until="load")
    pg.screenshot(path=f"{out}/admin-login-1440.png", full_page=True)
    pg.fill("#email", email); pg.fill("#password", password); pg.click("button[type=submit]")
    pg.wait_for_load_state("load")
    if "/admin/verify/" in pg.url:
        pg.screenshot(path=f"{out}/admin-verify-1440.png", full_page=True)
        code = None
        for _ in range(20):
            code = latest_code()
            if code: break
            time.sleep(0.5)
        assert code, "no sign-in code in the mail log"
        pg.fill("input[name=code]", code); pg.click("button[type=submit]"); pg.wait_for_load_state("load")
    assert "/admin/" not in pg.url or pg.url.endswith("/shop/"), f"unexpected page after sign-in: {pg.url}"

    # a product page with editing on
    pg.goto(f"{base}/shop/", wait_until="load")
    href = pg.get_attribute("a[href^='/product/']", "href")
    pg.goto(f"{base}{href}", wait_until="load")
    assert pg.locator("#admBar").count() == 1, "editing bar missing"
    assert pg.evaluate("document.documentElement.classList.contains('editing')"), "editing class missing"
    pg.screenshot(path=f"{out}/admin-product-1440.png", full_page=True)
    title = pg.locator("[data-edit=title]"); before = title.inner_text().strip()
    title.click(); pg.wait_for_selector("[data-edit=title] input")
    pg.fill("[data-edit=title] input", before + " (gate)"); pg.click("[data-edit=title] .save")
    pg.wait_for_function("document.querySelector('[data-edit=title]').innerText.includes('(gate)')")
    pg.screenshot(path=f"{out}/admin-product-edited-1440.png", full_page=True)
    title.click(); pg.wait_for_selector("[data-edit=title] input"); pg.fill("[data-edit=title] input", before); pg.click("[data-edit=title] .save")
    pg.wait_for_function(f"document.querySelector('[data-edit=title]').innerText.trim() === {before!r}")
    # preview as a customer, then back
    pg.click("#admBar button:has-text('Preview as customer')"); pg.wait_for_load_state("load")
    assert pg.locator("#admBar").count() == 0 and pg.locator(".adm-pill").count() == 1, "preview mode did not hide the editing bar"
    pg.screenshot(path=f"{out}/admin-preview-1440.png", full_page=False)
    pg.click(".adm-pill button"); pg.wait_for_load_state("load")
    assert pg.locator("#admBar").count() == 1, "leaving preview did not restore the editing bar"
    # history shows the two edits
    pg.goto(f"{base}/admin/history/", wait_until="load")
    assert pg.locator("td:has-text('(gate)')").count() >= 1, "history missing the edit"
    pg.screenshot(path=f"{out}/admin-history-1440.png", full_page=True)
    # phone width
    ctx2 = b.new_context(viewport={"width": 390, "height": 844}, storage_state=ctx.storage_state())
    m = ctx2.new_page(); m.goto(f"{base}{href}", wait_until="load"); m.screenshot(path=f"{out}/admin-product-390.png", full_page=True)
    over = m.evaluate("document.documentElement.scrollWidth - document.documentElement.clientWidth")
    if over > 0: bad.append(f"overflow {over}px at 390 on the product page with editing on")
    b.close()

for line in bad: print(line)
print("admin check:", "PASS" if not bad else f"FAIL ({len(bad)})")
sys.exit(1 if bad else 0)
