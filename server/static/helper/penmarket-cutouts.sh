#!/bin/bash
# ThePenMarket automatic cut-outs. Run by macOS (launchd) every 15 minutes, installed by install.sh.
# Asks the site which pens for sale have no cut-out yet, takes the background out of each one's first
# photo with Apple's Remove Background (penmarket-cutout) and sends it back. Log:
# ~/Library/Logs/ThePenMarket-cutouts.log
set -u
DIR="$HOME/Library/Application Support/ThePenMarket"
LOG="$HOME/Library/Logs/ThePenMarket-cutouts.log"
# shellcheck disable=SC1091
. "$DIR/helper.conf"
BIN="$DIR/penmarket-cutout"
TMP="$(mktemp -d)"; trap 'rm -rf "$TMP"' EXIT
log() { printf '%s %s\n' "$(date '+%Y-%m-%d %H:%M:%S')" "$*" >> "$LOG"; }
AUTH="Authorization: Bearer $KEY"

list="$(curl -fsS --max-time 30 -H "$AUTH" "$SITE/api/cutouts/needed" 2>&1)" || { log "could not ask $SITE: $list"; exit 0; }
[ -n "$list" ] || exit 0
while IFS=$'\t' read -r id path; do
  case "$id" in ''|*[!0-9]*) continue ;; esac
  case "$path" in /media/*) ;; *) continue ;; esac
  ext="${path##*.}"; in="$TMP/photo.$ext"; out="$TMP/cutout.png"
  if ! curl -fsS --max-time 60 -o "$in" "$SITE$path"; then log "pen $id: could not download the photo"; continue; fi
  "$BIN" "$in" "$out" >/dev/null 2>&1; rc=$?
  if [ $rc -eq 0 ]; then
    code="$(curl -sS --max-time 120 -o /dev/null -w '%{http_code}' -H "$AUTH" -F "photo=@$out;type=image/png" "$SITE/api/cutouts/$id")"
    log "pen $id: cut-out sent (site answered $code)"
  elif [ $rc -eq 3 ]; then
    curl -fsS --max-time 30 -o /dev/null -X POST -H "$AUTH" "$SITE/api/cutouts/$id/none" && log "pen $id: no clear subject in the photo, skipped"
  else
    log "pen $id: the cut-out failed (code $rc), will try again"
  fi
  rm -f "$in" "$out"
done <<< "$list"
