#!/usr/bin/env bash
# The whole test gate for the Rust site in one command. Nothing ships with a red line here.
#   scripts/gate.sh [base-url=http://127.0.0.1:8140] [label=local]
# Rows: engine (Rust + JS fixtures), seo (every indexable route: one h1, title, meta description,
# canonical, valid JSON-LD with required fields), robots + sitemaps (every sitemap URL answers 200),
# links (every href/src on every route), console (WebGL on, 1440 and 500), overflow (320-1300 in the
# iframe runner), screenshots (every route at 1440 and 500), slop scan, redirects (old URLs 301).
set -uo pipefail
BASE="${1:-http://127.0.0.1:8140}"; LABEL="${2:-local}"
P="$(cd "$(dirname "$0")/.." && pwd)"; S="$HOME/.claude/skills/new-customer/scripts"; OUT="$P/research/gate"; mkdir -p "$OUT/shots"
export CHROME="${CHROME:-/usr/bin/chromium-browser}"
RES="$OUT/results.txt"; : > "$RES"
ROUTES="shop/ on-sale-pens/ product-category/vintage-pens/ product-category/pre-owned-pens/ product-category/pencils/ brand/parker/ era/04-1930-1939/ nib/fine/ pw-filling-mechanism/lever-filler/ pw-price-range/03-100-500range/ product/vintage-pens-montblanc-644/ product/pre-owned-pens-namiki-pilot-white-tiger/ product/unlimited-posts/ blog/ category/how-do-i-start-collecting-pens/ how-do-i-restore-a-parker-vacumatic/ trading-post/ trading-post/montblanc-fountain-pen/ post-your-product/ pen-repairs/ sell-my-pens/ contact/ guarantee/ about-us/ privacy/"
run() { local name="$1"; shift; echo; echo "===== $name"; if "$@" > "$OUT/$name.log" 2>&1; then echo "$name PASS" >> "$RES"; else echo "$name FAIL" >> "$RES"; fi; tail -n 14 "$OUT/$name.log"; }

engine() { (cd "$P/server" && cargo test --release --lib 2>&1 | tail -5 | grep -q "test result: ok") && node "$P/scripts/test-engine-js.mjs"; }
run engine engine

seo() { python3 "$P/scripts/seo_check.py" "$BASE" "" $ROUTES; }
run seo seo

sitemaps() {
  local fail=0
  curl -sf "$BASE/robots.txt" | grep -q "Allow: /llms.txt" || { echo "robots.txt missing llms allow"; fail=1; }
  for bot in GPTBot ClaudeBot PerplexityBot Google-Extended; do curl -sf "$BASE/robots.txt" | grep -q "User-agent: $bot" || { echo "robots.txt lacks $bot"; fail=1; }; done
  curl -sf "$BASE/llms.txt" | grep -q "ThePenMarket.com" || { echo "llms.txt missing"; fail=1; }
  local n=0 bad=0
  for sm in $(curl -sf "$BASE/sitemap.xml" | grep -oE '<loc>[^<]+' | sed 's/<loc>//'); do
    local local_sm="${sm/#http*:\/\/[^\/]*/$BASE}"
    for u in $(curl -sf "$local_sm" | grep -oE '<loc>[^<]+' | sed 's/<loc>//'); do
      local lu; lu="$(echo "$u" | sed -E "s#^https?://[^/]+#$BASE#")"
      n=$((n+1)); code=$(curl -s -o /dev/null -w '%{http_code}' "$lu"); [ "$code" = 200 ] || { echo "FAIL $code $u"; bad=$((bad+1)); }
    done
  done
  echo "sitemap urls: $n checked, $bad failed"; [ "$bad" = 0 ] && [ "$fail" = 0 ]
}
run sitemaps sitemaps

redirects() {
  local bad=0
  while IFS='|' read -r old new; do
    code=$(curl -s -o /dev/null -w '%{http_code}' "$BASE$old"); loc=$(curl -s -o /dev/null -w '%{redirect_url}' "$BASE$old")
    if [ "$code" != 301 ] || [[ "$loc" != *"$new"* ]]; then echo "FAIL $old -> $code $loc (want 301 $new)"; bad=$((bad+1)); fi
  done <<'LIST'
/refund_returns/|/guarantee/
/product-tag/parker/|/brand/parker/
/tag/parker-51/|/blog/?q=parker+51
/trading-posts/|/trading-post/
/post-sitemap.xml|/sitemap-posts.xml
/vintage-pens-blog/|/blog/
/shop|/shop/
/wp-content/uploads/2026/08/6967-Montblanc-644.jpg|/media/uploads/2026/08/6967-Montblanc-644.jpg
/cart/|/shop/
/author/nathaniel-cerf/|/about-us/
LIST
  code=$(curl -s -o /dev/null -w '%{http_code}' "$BASE/no-such-page/"); [ "$code" = 404 ] || { echo "FAIL 404 page returned $code"; bad=$((bad+1)); }
  curl -s "$BASE/no-such-page/" | grep -q "<h1" || { echo "FAIL 404 page has no h1"; bad=$((bad+1)); }
  echo "redirects: $bad failed"; [ "$bad" = 0 ]
}
run redirects redirects

run links python3 "$P/scripts/links_url.py" "$BASE" "$OUT/links-report.md" $ROUTES
run console python3 "$P/scripts/console_url.py" "$BASE" $ROUTES
run overflow python3 "$P/scripts/sweep_url.py" "$BASE" $ROUTES

shots() { local ok=0; for r in "" $ROUTES; do for sz in 1440x1600 500x1400; do
  n="$(echo "${r:-home}" | tr '/' '_' | sed 's/_$//')-${sz%x*}"; bash "$S/shot.sh" "$BASE/${r}" "$OUT/shots/$n.png" "$sz" >/dev/null 2>&1 || ok=1; done; done
  ls "$OUT/shots" | wc -l; return $ok; }
run screenshots shots

slop() { local bad=0; for r in "" shop/ product/vintage-pens-montblanc-644/ blog/ pen-repairs/; do curl -s "$BASE/$r" -o "$OUT/slop-$(echo "${r:-home}" | tr '/' '_').html"; done
  python3 "$S/slop_scan.py" "$OUT"/slop-*.html | tee "$OUT/slop-findings.txt" | grep -q "FAIL" && bad=1
  # deliberate exceptions recorded in research/gate/slop-exceptions.md are subtracted
  if [ "$bad" = 1 ] && [ -f "$OUT/slop-exceptions.md" ]; then
    left=$(grep "FAIL" "$OUT/slop-findings.txt" | grep -vFf <(grep -oE '^\- \[[0-9]+\] [^:]+' "$OUT/slop-exceptions.md" | sed -E 's/^- \[([0-9]+)\] .*/[\1]/') | wc -l); echo "unexcepted slop FAILs: $left"; [ "$left" = 0 ] && bad=0; fi
  [ "$bad" = 0 ]; }
run slop slop

if [ -n "${DEPLOYED_URL:-}" ]; then run deployed-links python3 "$P/scripts/links_url.py" "$DEPLOYED_URL" "$OUT/links-deployed.md" $ROUTES; fi

echo; echo "===== GATE ($LABEL, $BASE)"; fail=0
sort "$RES" | while read -r k v; do printf '%-16s %s\n' "$k" "$v"; done; grep -q FAIL "$RES" && fail=1
echo "logs and screenshots: $OUT"
exit $fail
