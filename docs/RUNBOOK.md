# Runbook: ThePenMarket.com rebuild

## Where it runs
- sparky (Tailscale): http://100.117.164.79:8140/ as the user service `thepenmarket.service`, working dir `~/thepenmarket/server`, binary `target/release/thepenmarket`, log `~/thepenmarket/server/server.log`.
- Postgres 16 on sparky, port 5433, database and role `thepenmarket`.
- Secrets: `~/.config/thepenmarket.env` (chmod 600): `DATABASE_URL`, `CSRF_SECRET`, `GUIDE_AI_URL`, `GUIDE_AI_KEY`, `GUIDE_AI_MODEL`. Never commit or print it.

## Health
```
curl -s http://100.117.164.79:8140/healthz            # ok
curl -s http://100.117.164.79:8140/api/guide/health   # {"backend":"openai-compatible","model":...,"catalog":248}
systemctl --user status thepenmarket.service
tail -50 ~/thepenmarket/server/server.log
```

## Redeploy after a change
```
cd ~/thepenmarket && bash scripts/deploy.sh          # release build, re-import, restart, health-check
SKIP_IMPORT=1 bash scripts/deploy.sh                 # keep the database as is
```

## Re-import the catalog
```
cd ~/thepenmarket/server && ./target/release/import   # truncates and reloads from ~/thepenmarket/backup; log in research/import-log.md
systemctl --user restart thepenmarket.service         # or wait: the catalog snapshot refreshes every 10 minutes
```

## Switch or fix the model
Edit the three `GUIDE_AI_*` lines in `~/.config/thepenmarket.env` and `systemctl --user restart thepenmarket.service`. With no model configured the assistant says it is offline and shows the phone and e-mail; it never answers from canned text.

## The gate
```
bash scripts/gate.sh http://127.0.0.1:8140            # engine, seo, sitemaps, redirects, links, console, overflow, screenshots, slop
DEPLOYED_URL=http://100.117.164.79:8140 bash scripts/gate.sh http://127.0.0.1:8140 sparky
```
Output under `research/gate/` (results.txt, per-row logs, shots/).

## Rate limits and bodies
- `/api/guide`: 10 requests per minute per IP (`GUIDE_RATE_PER_MIN`), 4 tool rounds per turn, 24 turns of history.
- Forms: 20 per minute per IP, 10 MB photo (re-encoded to JPEG, metadata stripped, stored under `server/uploads/`, not served).
- Everything else: 64 KB request bodies.

## The public copy (droplet): https://penmarket.crossgen-ai.com/
Deployed 2026-09-14 by `scripts/push-droplet.sh` (from sparky) + `scripts/deploy-droplet.sh` (on the droplet, as root):
- containers `thepenmarket` (the app, image built from `server/Dockerfile`) and `thepenmarket-db` (postgres:16-alpine, volume `thepenmarket_pgdata`) on the `web` network; Caddy block `penmarket.crossgen-ai.com` in `/root/proxy/Caddyfile`.
- files: `/srv/apps/thepenmarket/build` (build context), `/srv/apps/thepenmarket/media` (photos, mounted read-only), `/srv/apps/thepenmarket/uploads` (form photos), `/srv/apps/thepenmarket/.env` (secrets), `db.sql.gz` (the dump restored on first run).
- redeploy: `bash scripts/push-droplet.sh` from sparky (rsyncs, dumps the sparky database, rebuilds the image, swaps the container, health-checks). The Rust build takes about 8 minutes on the droplet's two cores.
- rollback: `docker run` the `thepenmarket:previous` image with the same flags.
