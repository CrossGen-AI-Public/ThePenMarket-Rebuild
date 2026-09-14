#!/usr/bin/env bash
# From sparky: ship the build context, a database dump and the secrets to the droplet, then deploy.
#   bash scripts/push-droplet.sh
# Secrets: DATABASE_URL and CSRF_SECRET are generated for the droplet; the three GUIDE_AI_* lines are
# copied from the crossgen-site-api container's environment ON the droplet (never through this shell).
set -euo pipefail
cd "$(dirname "$0")/.."
H=crossgen-droplet
DIR=/srv/apps/thepenmarket
ssh -o BatchMode=yes "$H" "mkdir -p $DIR/build $DIR/uploads $DIR/media"
rsync -az --delete --exclude /target --exclude /uploads --exclude /media --exclude server.log server/ "$H:$DIR/build/"
rsync -az --delete --exclude /uploads/admin server/media/ "$H:$DIR/media/"   # photos Nathaniel adds on the droplet live under uploads/admin and are kept
LOCAL_DB="$(grep '^DATABASE_URL=' "$HOME/.config/thepenmarket.env" | cut -d= -f2-)"
pg_dump --no-owner --no-privileges "$LOCAL_DB" | gzip > /tmp/thepenmarket-db.sql.gz
rsync -az /tmp/thepenmarket-db.sql.gz "$H:$DIR/db.sql.gz"; rm -f /tmp/thepenmarket-db.sql.gz
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
    docker inspect crossgen-site-api --format '{{range .Config.Env}}{{println .}}{{end}}' | grep -E '^GUIDE_AI_(URL|KEY|MODEL)='
  } > "$DIR/.env"
  chmod 600 "$DIR/.env"
  echo "env: created ($(grep -c . "$DIR/.env") lines)"
else
  echo "env: kept"
fi
REMOTE
ssh -o BatchMode=yes "$H" 'bash -s' < scripts/deploy-droplet.sh
