#!/bin/bash
# Sets up ThePenMarket automatic cut-outs on this Mac, for this user. Made to be pasted from the admin:
#   curl -fsSL '<site>/static/helper/install.sh' | bash -s -- '<site>' '<key>'
# Puts the helper in ~/Library/Application Support/ThePenMarket (key in helper.conf, readable only by
# you) and asks macOS to run it every 15 minutes. Undo: curl -fsSL '<site>/static/helper/uninstall.sh' | bash
set -euo pipefail
SITE="${1:-}"; KEY="${2:-}"
[ -n "$SITE" ] && [ -n "$KEY" ] || { echo "Paste the whole line from the admin (it ends with your key)."; exit 1; }
SITE="${SITE%/}"
case "$SITE" in https://*|http://127.0.0.1:*|http://localhost:*) ;; *) echo "The site address must start with https://"; exit 1 ;; esac
case "$KEY" in *[!0-9a-f]*|"") echo "That key does not look right. Copy the line from the admin again."; exit 1 ;; esac
[ "$(uname)" = Darwin ] || { echo "This is for a Mac."; exit 1; }
ver="$(sw_vers -productVersion)"
[ "${ver%%.*}" -ge 14 ] || { echo "This needs macOS 14 Sonoma or newer; this Mac has $ver."; exit 1; }

code="$(curl -s -o /dev/null -w '%{http_code}' -H "Authorization: Bearer $KEY" "$SITE/api/cutouts/needed")"
[ "$code" = 200 ] || { echo "The site did not accept this key (answer $code). In the admin, press \"Set up a Mac\" again and paste the new line."; exit 1; }

DIR="$HOME/Library/Application Support/ThePenMarket"
mkdir -p "$DIR" "$HOME/Library/Logs" "$HOME/Library/LaunchAgents"
chmod 700 "$DIR"
curl -fsSL "$SITE/static/helper/penmarket-cutout" -o "$DIR/penmarket-cutout.new"
curl -fsSL "$SITE/static/helper/penmarket-cutouts.sh" -o "$DIR/penmarket-cutouts.sh.new"
chmod 755 "$DIR/penmarket-cutout.new" "$DIR/penmarket-cutouts.sh.new"
mv -f "$DIR/penmarket-cutout.new" "$DIR/penmarket-cutout"
mv -f "$DIR/penmarket-cutouts.sh.new" "$DIR/penmarket-cutouts.sh"
( umask 077; printf 'SITE=%q\nKEY=%q\n' "$SITE" "$KEY" > "$DIR/helper.conf" )

PLIST="$HOME/Library/LaunchAgents/com.thepenmarket.cutouts.plist"
cat > "$PLIST" <<PL
<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0">
<dict>
  <key>Label</key><string>com.thepenmarket.cutouts</string>
  <key>ProgramArguments</key><array><string>/bin/bash</string><string>$DIR/penmarket-cutouts.sh</string></array>
  <key>StartInterval</key><integer>900</integer>
  <key>RunAtLoad</key><true/>
  <key>ProcessType</key><string>Background</string>
  <key>StandardErrorPath</key><string>$HOME/Library/Logs/ThePenMarket-cutouts.log</string>
</dict>
</plist>
PL
launchctl bootout "gui/$(id -u)" "$PLIST" 2>/dev/null || true
launchctl bootstrap "gui/$(id -u)" "$PLIST"
echo "Done. Your Mac will make a cut-out for each new pen every 15 minutes while it is on."
echo "(The first run is happening now. What it did is in ~/Library/Logs/ThePenMarket-cutouts.log)"
