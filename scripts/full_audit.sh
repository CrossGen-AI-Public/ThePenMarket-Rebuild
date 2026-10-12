#!/usr/bin/env bash
# The whole site, not a sample: every URL in the site's own sitemaps through the gate's checkers, every
# row of the redirect table, every admin screen signed in, and the security headers.
#   scripts/full_audit.sh [base-url=http://127.0.0.1:8140] [out-dir=research/audit/<date>]
# Needs Python Playwright (CHROME for the links checker's bot-blocked fallback). Takes about an hour
# on ~700 URLs: console and overflow run in 4 shards in parallel. ADMIN_TEST_EMAIL / ADMIN_TEST_PASSWORD
# enable the signed-in admin sweep. Read-only except for that test sign-in.
set -uo pipefail
BASE="${1:-http://127.0.0.1:8140}"; BASE="${BASE%/}"
P="$(cd "$(dirname "$0")/.." && pwd)"
OUT="${2:-$P/research/audit/$(date +%F)}"; mkdir -p "$OUT"
PY="${PYTHON:-python3}"
RES="$OUT/results.txt"; : > "$RES"
# ONLY=admin,redirects,headers runs just those steps.
want() { [ -z "${ONLY:-}" ] || [[ ",$ONLY," == *",$1,"* ]]; }
run() { local name="$1"; shift; want "$name" || return 0; echo "===== $name"; if "$@" > "$OUT/$name.log" 2>&1; then echo "$name PASS" >> "$RES"; else echo "$name FAIL" >> "$RES"; fi; tail -n 6 "$OUT/$name.log"; }

# Every URL the site lists, as routes relative to the base ("" is the home page).
: > "$OUT/routes.txt"
for sm in $(curl -sf "$BASE/sitemap.xml" | grep -oE '<loc>[^<]+' | sed 's/<loc>//'); do
  curl -sf "$BASE$(echo "$sm" | sed -E 's#^https?://[^/]+##')" | grep -oE '<loc>[^<]+' | sed -E 's#<loc>https?://[^/]+/?##' >> "$OUT/routes.txt"
done
sort -u "$OUT/routes.txt" -o "$OUT/routes.txt"
N=$(wc -l < "$OUT/routes.txt" | tr -d ' ')
echo "$N URLs from the sitemaps"
ROUTES=$(tr '\n' ' ' < "$OUT/routes.txt")

# 1. Every URL answers 200 (no redirect, no error) — what the sitemap promises Google.
status200() { local bad=0; while read -r r; do c=$(curl -s -o /dev/null -w '%{http_code}' "$BASE/$r"); [ "$c" = 200 ] || { echo "FAIL $c /$r"; bad=$((bad+1)); }; done < "$OUT/routes.txt"; echo "$N checked, $bad not 200"; [ "$bad" = 0 ]; }
run status status200

# 2. SEO on every indexable URL: one h1, title, description, canonical, JSON-LD.
run seo $PY "$P/scripts/seo_check.py" "$BASE" "" $ROUTES

# 3. Every link and image on every page.
# Internal links must all work. Dead links to other websites inside his old posts are his content: they
# are listed (links-external.txt) for him to decide on, and do not fail the audit.
links_all() {
  $PY "$P/scripts/links_url.py" "$BASE" "$OUT/links-report.md" $ROUTES > "$OUT/links-raw.log" 2>&1
  grep -E '^FAIL' "$OUT/links-raw.log" | grep -E '\] https?://' > "$OUT/links-external.txt" || true
  grep -E '^FAIL' "$OUT/links-raw.log" | grep -vE '\] https?://' > "$OUT/links-internal.txt" || true
  tail -n 1 "$OUT/links-raw.log"
  echo "internal broken: $(wc -l < "$OUT/links-internal.txt" | tr -d ' '); external dead (his posts, listed): $(wc -l < "$OUT/links-external.txt" | tr -d ' ')"
  cat "$OUT/links-internal.txt"
  [ ! -s "$OUT/links-internal.txt" ]
}
run links links_all

# 4 + 5. Console errors and horizontal overflow, in 4 parallel shards.
rm -f "$OUT"/shard.*; awk -v o="$OUT" '{print > (o "/shard." (NR % 4))}' "$OUT/routes.txt"
shard() { local kind="$1" f="$2"; if [ "$kind" = console ]; then WIDTHS=1440,390 WAIT_MS=1200 $PY "$P/scripts/console_url.py" "$BASE" $(tr '\n' ' ' < "$f"); else WIDTHS=320,390,768,1300 WAIT_MS=400 $PY "$P/scripts/sweep_url.py" "$BASE" $(tr '\n' ' ' < "$f"); fi; }
sharded() { local kind="$1" pids=() rc=0; for f in "$OUT"/shard.[0-3]; do shard "$kind" "$f" > "$f.$kind.log" 2>&1 & pids+=($!); done; for p in "${pids[@]}"; do wait "$p" || rc=1; done; cat "$OUT"/shard.[0-3]."$kind".log | grep -vE '^width .* clean$'; return $rc; }
run console sharded console
run overflow sharded overflow

# 6. Every redirect in the table: 301, and the destination opens.
redirects_all() {
  local bad=0 n=0
  while IFS='|' read -r old new; do
    n=$((n+1))
    # Plain command substitution: macOS's bash 3.2 leaks a descriptor per "read < <(...)" in a loop.
    out=$(curl -s -o /dev/null -w '%{http_code} %{redirect_url}' "$BASE$old"); code=${out%% *}; loc=${out#* }
    [ "$code" = 301 ] || { echo "FAIL $old -> $code (want 301 $new)"; bad=$((bad+1)); continue; }
    fin=$(curl -s -o /dev/null -L --max-redirs 5 -w '%{http_code}' "$BASE$old")
    [ "$fin" = 200 ] || { echo "FAIL $old -> $loc ends $fin"; bad=$((bad+1)); }
  done < <(curl -sf "$BASE/admin/redirects.csv" -b "$OUT/.cookies" 2>/dev/null | tail -n +2 | awk -F, '{print $1"|"$2}' | grep -v '^"')
  echo "redirects: $n checked, $bad failed"; [ "$n" -gt 0 ] && [ "$bad" = 0 ]
}

# 7. Admin: sign in, open every admin screen, sign out (needs the test account).
admin_sweep() {
  [ -n "${ADMIN_TEST_EMAIL:-}" ] || { echo "skipped: ADMIN_TEST_EMAIL not set"; return 0; }
  rm -f "$OUT/.cookies"
  local tok; tok=$(curl -s -c "$OUT/.cookies" "$BASE/admin/login/" | grep -oE 'name="csrf" value="[^"]+"' | head -1 | sed -E 's/.*value="([^"]+)"/\1/')
  curl -s -o /dev/null -b "$OUT/.cookies" -c "$OUT/.cookies" --data-urlencode "csrf=$tok" --data-urlencode "email=$ADMIN_TEST_EMAIL" --data-urlencode "password=$ADMIN_TEST_PASSWORD" "$BASE/admin/login/"
  local bad=0
  for p in admin/ admin/settings/ admin/brands/ admin/orders/ admin/inbox/ admin/posts/ admin/posts/new/ admin/pages/ admin/pages/new/ admin/faqs/ admin/redirects/ admin/search/ admin/glossary/ admin/insights/ admin/experiments/ admin/sourcing/ admin/devices/ admin/password/ admin/history/ admin/pens/ admin/redirects.csv admin/history.csv; do
    c=$(curl -s -o /dev/null -b "$OUT/.cookies" -w '%{http_code}' "$BASE/$p"); [ "$c" = 200 ] || { echo "FAIL $c /$p"; bad=$((bad+1)); }
    cc=$(curl -s -o /dev/null -D - -b "$OUT/.cookies" "$BASE/$p" | grep -i '^cache-control:' | tr -d '\r'); case "$cc" in *no-store*) ;; *) echo "FAIL /$p cache-control: ${cc:-none}"; bad=$((bad+1));; esac
  done
  echo "admin screens: $bad problems"; [ "$bad" = 0 ]
}
run admin admin_sweep
if [ -f "$OUT/.cookies" ]; then run redirects redirects_all; fi
if [ -f "$OUT/.cookies" ]; then
  tok=$(curl -s -b "$OUT/.cookies" "$BASE/admin/" | grep -oE 'name="csrf" value="[^"]+"' | head -1 | sed -E 's/.*value="([^"]+)"/\1/')
  curl -s -o /dev/null -b "$OUT/.cookies" --data-urlencode "csrf=$tok" "$BASE/admin/logout/"; rm -f "$OUT/.cookies"
fi

# 8. Security headers on a public page, an admin page and a static file.
headers() {
  local bad=0
  for p in / /shop/ /admin/login/ /static/css/site.css; do
    h=$(curl -s -o /dev/null -D - "$BASE$p" | tr -d '\r')
    for want in "x-content-type-options: nosniff" "referrer-policy:" "content-security-policy:" "permissions-policy:" "cross-origin-opener-policy:"; do
      echo "$h" | grep -qi "^$want" || { echo "FAIL $p lacks $want"; bad=$((bad+1)); }
    done
    case "$BASE" in https://*) echo "$h" | grep -qi '^strict-transport-security:' || { echo "FAIL $p lacks HSTS"; bad=$((bad+1)); };; esac
  done
  curl -s -o /dev/null -D - "$BASE/admin/login/" | tr -d '\r' | grep -qi '^cache-control:.*no-store' || { echo "FAIL /admin/login/ is cacheable"; bad=$((bad+1)); }
  [ "$(curl -s -o /dev/null -w '%{http_code}' "$BASE/.well-known/security.txt")" = 200 ] || { echo "FAIL no /.well-known/security.txt"; bad=$((bad+1)); }
  echo "headers: $bad problems"; [ "$bad" = 0 ]
}
run headers headers

echo; echo "===== FULL AUDIT ($BASE, $N URLs)"
sort "$RES" | while read -r k v; do printf '%-12s %s\n' "$k" "$v"; done
echo "logs: $OUT"
grep -q FAIL "$RES" && exit 1 || exit 0
