#!/usr/bin/env bash
# Deploy on sparky: release build, migrate, (re)install the user service on :8140, health-check.
# Idempotent. Run from anywhere: bash scripts/deploy.sh. Set SKIP_IMPORT=1 to keep the database as is.
set -euo pipefail
cd "$(dirname "$0")/../server"
export PATH="$HOME/.cargo/bin:$HOME/.local/bin:$PATH"
PORT="${PORT:-8140}"
cargo build --release 2>&1 | tail -3
if [ "${SKIP_IMPORT:-}" != 1 ] && [ -d "$HOME/thepenmarket/backup" ]; then
  ./target/release/import | tail -1
fi
mkdir -p "$HOME/.config/systemd/user"
cp ops/thepenmarket.service "$HOME/.config/systemd/user/thepenmarket.service"
systemctl --user daemon-reload
systemctl --user enable thepenmarket.service >/dev/null 2>&1 || true
systemctl --user restart thepenmarket.service
ok=0; for i in $(seq 1 20); do sleep 1; if curl -sf "http://127.0.0.1:${PORT}/healthz" | grep -q ok; then ok=1; break; fi; done
[ "$ok" = 1 ] || { echo "thepenmarket: service did not become healthy" >&2; systemctl --user status thepenmarket.service --no-pager | tail -20 >&2; exit 1; }
echo "thepenmarket: healthy on :${PORT} ($(git -C .. rev-parse --short HEAD 2>/dev/null || echo no-git))"
curl -sf "http://127.0.0.1:${PORT}/api/guide/health"; echo
