#!/bin/bash
# Removes ThePenMarket automatic cut-outs from this Mac:  curl -fsSL '<site>/static/helper/uninstall.sh' | bash
PLIST="$HOME/Library/LaunchAgents/com.thepenmarket.cutouts.plist"
launchctl bootout "gui/$(id -u)" "$PLIST" 2>/dev/null || true
rm -f "$PLIST"
rm -rf "$HOME/Library/Application Support/ThePenMarket"
echo "Removed. (To stop the key working too, press \"Turn off\" in the admin under Homepage and photos.)"
