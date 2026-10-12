#!/usr/bin/env bash
# Deploy the public copy to the crossgen-ai.com droplet. Run ON the droplet (root):
#   ssh crossgen-droplet 'bash -s' < scripts/deploy-droplet.sh
# Expects, already on the box (put there by scripts/push-droplet.sh from sparky):
#   /srv/apps/thepenmarket/build/   the server/ directory (Dockerfile, src, templates, static, data)
#   /srv/apps/thepenmarket/media/   the product photos (read-write: the admin adds photos under uploads/admin/)
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

# 1. Postgres on a private network only the app joins (data in a named volume). Nothing else on the
#    shared `web` network can reach the database.
INTERNAL=${NAME}-internal
docker network inspect "$INTERNAL" >/dev/null 2>&1 || docker network create --internal "$INTERNAL" >/dev/null
if ! docker ps --format '{{.Names}}' | grep -qx "$DB"; then
  docker rm -f "$DB" >/dev/null 2>&1 || true
  PGPASS="$(grep '^DATABASE_URL=' "$DIR/.env" | sed -E 's#.*://[^:]+:([^@]+)@.*#\1#')"
  docker run -d --name "$DB" --restart unless-stopped --network "$INTERNAL" -v "${NAME}_pgdata:/var/lib/postgresql/data" \
    -e POSTGRES_USER=thepenmarket -e POSTGRES_PASSWORD="$PGPASS" -e POSTGRES_DB=thepenmarket postgres:16-alpine >/dev/null
  for i in $(seq 1 30); do sleep 1; docker exec "$DB" pg_isready -U thepenmarket >/dev/null 2>&1 && break; done
  echo "postgres: started"
fi
# A database container from before this change: move it off the shared network.
docker network connect "$INTERNAL" "$DB" >/dev/null 2>&1 || true
docker network disconnect web "$DB" >/dev/null 2>&1 || true
# 2. restore the sparky dump when the database is empty
if [ -f "$DIR/db.sql.gz" ] && [ "$(docker exec "$DB" psql -U thepenmarket -Atc "select count(*) from information_schema.tables where table_schema='public'" 2>/dev/null || echo 0)" = 0 ]; then
  gunzip -c "$DIR/db.sql.gz" | docker exec -i "$DB" psql -U thepenmarket -q thepenmarket
  echo "postgres: restored $(docker exec "$DB" psql -U thepenmarket -Atc 'select count(*) from product') products"
fi
# The dump holds password hashes, buyers and messages: never leave it lying on the disk.
rm -f "$DIR/db.sql.gz"

# 3. build + swap, keeping the previous image for rollback
docker image tag "$NAME:latest" "$NAME:previous" 2>/dev/null || true
docker build -q -t "$NAME:latest" "$DIR/build" >/dev/null
docker rm -f "$NAME" >/dev/null 2>&1 || true
# The app runs as uid 10001 (see the Dockerfile): it owns the two folders it writes to, nothing else.
chown -R 10001:10001 "$DIR/uploads" "$DIR/media"
# Only Caddy, on the web network, may tell the app a visitor's address (X-Forwarded-For).
WEB_SUBNET="$(docker network inspect web -f '{{range .IPAM.Config}}{{.Subnet}},{{end}}' | sed 's/,$//')"
# Created, given both networks, then started, so it never boots without its database.
docker create --name "$NAME" --restart unless-stopped --network web --env-file "$DIR/.env" \
  --read-only --tmpfs /tmp --cap-drop ALL --security-opt no-new-privileges --memory 1g --pids-limit 256 \
  -e PORT=8140 -e HOST=0.0.0.0 -e APP_DIR=/app -e SITE_ORIGIN="https://$HOST" -e TRUSTED_PROXIES="$WEB_SUBNET" \
  -v "$DIR/uploads:/app/uploads" -v "$DIR/media:/app/media" "$NAME:latest" >/dev/null
docker network connect "$INTERNAL" "$NAME"
docker start "$NAME" >/dev/null

# 4. Caddy route (idempotent)
if ! grep -q "^$HOST {" "$CADDYFILE"; then
  printf '\n%s {\n    request_body {\n        max_size 13MB\n    }\n    header -Server\n    reverse_proxy %s:8140\n}\n' "$HOST" "$NAME" >> "$CADDYFILE"
  docker exec proxy-caddy-1 caddy reload --config /etc/caddy/Caddyfile >/dev/null 2>&1 || docker restart proxy-caddy-1 >/dev/null
  echo "caddy: added $HOST"
fi

# 5. health, in-container then through Caddy
for i in $(seq 1 30); do sleep 1; docker exec "$NAME" curl -sf http://127.0.0.1:8140/healthz 2>/dev/null | grep -q ok && break; [ "$i" = 30 ] && { echo "container unhealthy" >&2; docker logs --tail 30 "$NAME" >&2; exit 1; }; done
for i in $(seq 1 20); do sleep 2; curl -sf -m 5 "https://$HOST/api/guide/health" >/dev/null 2>&1 && break; [ "$i" = 20 ] && echo "warning: https://$HOST not answering yet (TLS may still be provisioning)"; done
echo "$NAME: $(curl -sf -m 5 "https://$HOST/api/guide/health" 2>/dev/null || echo 'container up, public URL pending')"
