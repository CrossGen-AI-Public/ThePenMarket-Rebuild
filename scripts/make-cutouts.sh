#!/usr/bin/env bash
# Homepage photo with the backdrop taken out: for the lead photo of every pen still for sale, write
# <photo>-cutout.png (just the pen and cap, transparent) next to it in server/media. The homepage uses
# it in photo mode when the file exists and falls back to the normal photo when it does not.
#   bash scripts/make-cutouts.sh            # pens without a cutout yet
#   FORCE=1 bash scripts/make-cutouts.sh    # redo all of them
#   SYNC=sparky bash scripts/make-cutouts.sh  # then copy the cutouts (only) to sparky's media folder
# macOS 14+ only: it uses Apple's Vision subject mask (scripts/cutout.swift), so run it on the Mac.
# A pen added on the live site later has no cutout until this is run again and synced.
set -euo pipefail
cd "$(dirname "$0")/.."
[ "$(uname)" = Darwin ] || { echo "make-cutouts.sh runs on macOS (Vision)" >&2; exit 1; }
DB="${DATABASE_URL:-$(grep '^DATABASE_URL=' "$HOME/.config/thepenmarket.env" | cut -d= -f2-)}"
MEDIA="server/media"
BIN="$(mktemp -d)/cutout"
swiftc -O scripts/cutout.swift -o "$BIN" 2>&1 | grep -v '^$' || true
[ -x "$BIN" ] || { echo "could not build scripts/cutout.swift" >&2; exit 1; }

made=0 kept=0 failed=0
# lead photo (lowest position, not archived) of every live, unsold, not deleted pen
while IFS='|' read -r path has960; do
  [ -n "$path" ] || continue
  case "$path" in *..*) continue ;; esac
  stem="${path%.*}"; ext="${path##*.}"
  src="$MEDIA/$path"; [ "$has960" = t ] && src="$MEDIA/$stem-960.$ext"
  out="$MEDIA/$stem-cutout.png"
  if [ -f "$out" ] && [ -z "${FORCE:-}" ]; then kept=$((kept+1)); continue; fi
  [ -f "$src" ] || { echo "missing photo: $src" >&2; failed=$((failed+1)); continue; }
  if "$BIN" "$src" "$out" >/dev/null; then made=$((made+1)); else echo "no cutout for $path" >&2; rm -f "$out"; failed=$((failed+1)); fi
done < <(psql "$DB" -Atc "
  SELECT DISTINCT ON (i.product_id) i.path, i.has_960
  FROM product_image i JOIN product p ON p.id = i.product_id
  WHERE i.archived_at IS NULL AND p.status = 'live' AND p.sold_at IS NULL AND p.deleted_at IS NULL
  ORDER BY i.product_id, i.position")
echo "cutouts: $made made, $kept already there, $failed failed"

if [ -n "${SYNC:-}" ]; then
  # Only the -cutout.png files; never deletes or overwrites anything else on the server.
  rsync -am --include='*/' --include='*-cutout.png' --exclude='*' "$MEDIA/" "$SYNC:thepenmarket/server/media/"
  echo "synced cutouts to $SYNC"
fi
