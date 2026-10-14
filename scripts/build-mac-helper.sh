#!/usr/bin/env bash
# Build the cut-out program the Mac helper uses (server/static/helper/penmarket-cutout) from
# scripts/cutout.swift: one universal binary (Apple Silicon + Intel, macOS 14+), ad-hoc signed so it
# runs without Xcode on his Mac. Rebuild after changing cutout.swift, then commit the binary.
set -euo pipefail
cd "$(dirname "$0")/.."
T="$(mktemp -d)"; trap 'rm -rf "$T"' EXIT
swiftc -O -target arm64-apple-macos14 scripts/cutout.swift -o "$T/arm64"
swiftc -O -target x86_64-apple-macos14 scripts/cutout.swift -o "$T/x86_64"
lipo -create "$T/arm64" "$T/x86_64" -output "$T/penmarket-cutout"
codesign --force --sign - "$T/penmarket-cutout"
install -m 755 "$T/penmarket-cutout" server/static/helper/penmarket-cutout
lipo -info server/static/helper/penmarket-cutout
