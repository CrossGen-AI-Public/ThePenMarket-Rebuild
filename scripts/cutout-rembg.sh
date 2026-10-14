#!/usr/bin/env bash
# CUTOUT_CMD for a Linux server (sparky, the droplet): takes the backdrop out of one photo with rembg, an
# open-source background remover that runs on the box itself (no photo leaves it).
#   cutout-rembg.sh <photo> <out.png>
# One-time setup on the box, as the user that runs the site:
#   uv tool install "rembg[cpu,cli]"        # the first cutout then downloads its model (~180 MB) to ~/.u2net
#   echo "CUTOUT_CMD=$HOME/thepenmarket/scripts/cutout-rembg.sh" >> ~/.config/thepenmarket.env
# The site calls it in the background when he features a pen whose photo has no cutout yet
# (server/src/hero_photo.rs); the Mac makes the same files with scripts/make-cutouts.sh.
set -euo pipefail
[ $# -eq 2 ] || { echo "usage: cutout-rembg.sh <photo> <out.png>" >&2; exit 2; }
REMBG="${REMBG:-$HOME/.local/bin/rembg}"
[ -x "$REMBG" ] || REMBG="$(command -v rembg || true)"
[ -n "$REMBG" ] || { echo "rembg is not installed (uv tool install \"rembg[cpu,cli]\")" >&2; exit 1; }
exec "$REMBG" i -m "${REMBG_MODEL:-isnet-general-use}" "$1" "$2"
