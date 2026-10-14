#!/usr/bin/env bash
# From sparky: ship the build context, a database dump and the secrets to the droplet, then deploy.
#   bash scripts/push-droplet.sh                                   # CrossGen's droplet, penmarket.crossgen-ai.com
#   DROPLET=penmarket-do SITE_HOST=new.thepenmarket.com bash scripts/push-droplet.sh   # Nathaniel's droplet
# DROPLET is an ssh alias for root on the box; SITE_HOST is the public name Caddy serves (its DNS must
# already point at the droplet for the TLS certificate). Both are passed on to deploy-droplet.sh.
# Secrets: DATABASE_URL and CSRF_SECRET are generated for the droplet. GUIDE_AI_* are copied from the
# crossgen-site-api container ON the droplet when that container exists (CrossGen's box); on his own
# droplet they are left empty and the assistant shows its offline notice until they are filled in on the
# box. Stripe and SMTP values are only ever added on the box (see RUNBOOK "Nathaniel's droplet").
set -euo pipefail
cd "$(dirname "$0")/.."
H="${DROPLET:-crossgen-droplet}"
SITE_HOST="${SITE_HOST:-penmarket.crossgen-ai.com}"
case "$SITE_HOST" in *[!A-Za-z0-9.-]*|"") echo "SITE_HOST must be a bare host name" >&2; exit 1 ;; esac
DIR=/srv/apps/thepenmarket
ssh -o BatchMode=yes "$H" "mkdir -p $DIR/build $DIR/uploads $DIR/media"
rsync -az --delete --exclude /target --exclude /uploads --exclude /media --exclude server.log server/ "$H:$DIR/build/"
rsync -az --delete --exclude /uploads/admin server/media/ "$H:$DIR/media/"   # photos Nathaniel adds on the droplet live under uploads/admin and are kept
LOCAL_DB="$(grep '^DATABASE_URL=' "$HOME/.config/thepenmarket.env" | cut -d= -f2-)"
# The dump holds password hashes, buyers and messages: private file, private copy, deleted after use.
DUMP="$(umask 077; mktemp "${TMPDIR:-/tmp}/thepenmarket-db.XXXXXX")"
trap 'rm -f "$DUMP"' EXIT
pg_dump --no-owner --no-privileges "$LOCAL_DB" | gzip > "$DUMP"
rsync -az --chmod=F600 "$DUMP" "$H:$DIR/db.sql.gz"
# .env on the box: keep an existing one, otherwise assemble it there
ssh -o BatchMode=yes "$H" bash -s <<'REMOTE'
set -euo pipefail
DIR=/srv/apps/thepenmarket
if [ ! -f "$DIR/.env" ]; then
  PW="$(openssl rand -hex 24)"; CS="$(openssl rand -hex 32)"
  umask 077
  {
    echo "DATABASE_URL=postgres://thepenmarket:$PW@thepenmarket-db:5432/thepenmarket"
    echo "CSRF_SECRET=$CS"
    if docker inspect crossgen-site-api >/dev/null 2>&1; then
      docker inspect crossgen-site-api --format '{{range .Config.Env}}{{println .}}{{end}}' | grep -E '^GUIDE_AI_(URL|KEY|MODEL)='
    else
      printf 'GUIDE_AI_URL=\nGUIDE_AI_KEY=\nGUIDE_AI_MODEL=\n'
    fi
  } > "$DIR/.env"
  chmod 600 "$DIR/.env"
  echo "env: created ($(grep -c . "$DIR/.env") lines)"
else
  echo "env: kept"
fi
REMOTE
ssh -o BatchMode=yes "$H" "SITE_HOST=$SITE_HOST bash -s" < scripts/deploy-droplet.sh
