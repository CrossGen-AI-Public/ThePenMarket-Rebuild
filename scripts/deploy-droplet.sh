#!/usr/bin/env bash
# Deploy the public copy to the crossgen-ai.com droplet. Run ON the droplet (root):
#   ssh crossgen-droplet 'bash -s' < scripts/deploy-droplet.sh
# Expects, already on the box (put there by scripts/push-droplet.sh from sparky):
#   /srv/apps/thepenmarket/build/   the server/ directory (Dockerfile, src, templates, static, data)
#   /srv/apps/thepenmarket/media/   the product photos (mounted read-only into the container)
#   /srv/apps/thepenmarket/db.sql.gz  a pg_dump of the sparky database
#   /srv/apps/thepenmarket/.env     DATABASE_URL, CSRF_SECRET, GUIDE_AI_URL, GUIDE_AI_KEY, GUIDE_AI_MODEL (never printed)
# Mirrors the codehawk pattern: no host port published, Caddy reaches the container by name on the `web`
# network, secrets live only in /srv/apps/thepenmarket/.env on the box. Postgres runs as its own container.
set -euo pipefail
NAME=thepenmarket
DIR=/srv/apps/$NAME
HOST=penmarket.crossgen-ai.com
CADDYFILE=/root/proxy/Caddyfile
DB=thepenmarket-db

[ -f "$DIR/.env" ] && grep -q '^DATABASE_URL=.\+' "$DIR/.env" && grep -q '^GUIDE_AI_KEY=.\+' "$DIR/.env" || { echo "missing $DIR/.env with DATABASE_URL and GUIDE_AI_*" >&2; exit 1; }
[ -d "$DIR/build" ] || { echo "missing $DIR/build (rsync server/ there first)" >&2; exit 1; }

# 1. Postgres container on the web network (data in a named volume)
if ! docker ps --format '{{.Names}}' | grep -qx "$DB"; then
  docker rm -f "$DB" >/dev/null 2>&1 || true
  PGPASS="$(grep '^DATABASE_URL=' "$DIR/.env" | sed -E 's#.*://[^:]+:([^@]+)@.*#\1#')"
  docker run -d --name "$DB" --restart unless-stopped --network web -v "${NAME}_pgdata:/var/lib/postgresql/data" \
    -e POSTGRES_USER=thepenmarket -e POSTGRES_PASSWORD="$PGPASS" -e POSTGRES_DB=thepenmarket postgres:16-alpine >/dev/null
  for i in $(seq 1 30); do sleep 1; docker exec "$DB" pg_isready -U thepenmarket >/dev/null 2>&1 && break; done
  echo "postgres: started"
fi
# 2. restore the sparky dump when the database is empty
if [ -f "$DIR/db.sql.gz" ] && [ "$(docker exec "$DB" psql -U thepenmarket -Atc "select count(*) from information_schema.tables where table_schema='public'" 2>/dev/null || echo 0)" = 0 ]; then
  gunzip -c "$DIR/db.sql.gz" | docker exec -i "$DB" psql -U thepenmarket -q thepenmarket
  echo "postgres: restored $(docker exec "$DB" psql -U thepenmarket -Atc 'select count(*) from product') products"
fi

# 3. build + swap, keeping the previous image for rollback
docker image tag "$NAME:latest" "$NAME:previous" 2>/dev/null || true
docker build -q -t "$NAME:latest" "$DIR/build" >/dev/null
docker rm -f "$NAME" >/dev/null 2>&1 || true
docker run -d --name "$NAME" --restart unless-stopped --network web --env-file "$DIR/.env" \
  -e PORT=8140 -e HOST=0.0.0.0 -e APP_DIR=/app -e SITE_ORIGIN="https://$HOST" -v "$DIR/uploads:/app/uploads" -v "$DIR/media:/app/media:ro" "$NAME:latest" >/dev/null

# 4. Caddy route (idempotent)
if ! grep -q "^$HOST {" "$CADDYFILE"; then
  printf '\n%s {\n    reverse_proxy %s:8140\n}\n' "$HOST" "$NAME" >> "$CADDYFILE"
  docker exec proxy-caddy-1 caddy reload --config /etc/caddy/Caddyfile >/dev/null 2>&1 || docker restart proxy-caddy-1 >/dev/null
  echo "caddy: added $HOST"
fi

# 5. health, in-container then through Caddy
for i in $(seq 1 30); do sleep 1; docker exec "$NAME" curl -sf http://127.0.0.1:8140/healthz 2>/dev/null | grep -q ok && break; [ "$i" = 30 ] && { echo "container unhealthy" >&2; docker logs --tail 30 "$NAME" >&2; exit 1; }; done
for i in $(seq 1 20); do sleep 2; curl -sf -m 5 "https://$HOST/api/guide/health" >/dev/null 2>&1 && break; [ "$i" = 20 ] && echo "warning: https://$HOST not answering yet (TLS may still be provisioning)"; done
echo "$NAME: $(curl -sf -m 5 "https://$HOST/api/guide/health" 2>/dev/null || echo 'container up, public URL pending')"
