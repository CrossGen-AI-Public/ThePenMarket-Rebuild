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

## The admin (Nathaniel's sign-in, click-to-edit, preview)
Spec: `docs/spec/0001-admin-login-click-to-edit-preview.md`. Sign-in page: `/admin/login/` (not linked anywhere; `robots.txt` disallows `/admin/`).
```
cd ~/thepenmarket/server
ADMIN_PASSWORD='…' ./target/release/admin create nathaniel@example.com --name Nathaniel   # first account; password checked against the policy and HIBP
./target/release/admin set-password nathaniel@example.com     # reads the new password from stdin (or ADMIN_PASSWORD)
./target/release/admin unlock nathaniel@example.com           # clears a lockout (10 wrong passwords in 15 min locks for an hour)
./target/release/admin forget-devices nathaniel@example.com   # every device asks for the e-mail code again
```
- E-mail: with `SMTP_URL` (`smtps://user:pass@host:465`) and `MAIL_FROM` in the env file, codes and reset links are sent. Without them every message is appended to `server/uploads/admin-mail.log` (on the droplet `/srv/apps/thepenmarket/uploads/admin-mail.log`); read the six-digit code from there. `ADMIN_ALERT_EMAIL` gets a copy of lockout notices.
- On the droplet, run the CLI inside the container: `docker exec -it -e ADMIN_PASSWORD='…' thepenmarket /app/admin create …`.
- Data: `admin_account`, `admin_session`, `admin_device`, `admin_code`; every change is an `event_log` row (`admin.*`) shown at `/admin/history/` and downloadable as CSV. Photos he removes are archived (`product_image.archived_at`), never deleted. Hidden pens are `product.status = 'draft'` and 404 for everyone but him.
- **Re-importing is now guarded**: `import` refuses once any admin edit exists, because it truncates `product`. `FORCE_IMPORT=1` overrides. Deploy with `SKIP_IMPORT=1` from here on.
- The droplet keeps its own database after the first restore, so once Nathaniel edits on https://penmarket.crossgen-ai.com/ the droplet is the source of truth; `push-droplet.sh` no longer touches its data or the photos he added (`media/uploads/admin/`).
- Gate row: `ADMIN_TEST_EMAIL=… ADMIN_TEST_PASSWORD=… bash scripts/gate.sh …` adds an `admin` row (sign in, code, inline edit and revert, preview toggle, history, 390 px overflow).

## The public copy (droplet): https://penmarket.crossgen-ai.com/
Deployed 2026-09-14 by `scripts/push-droplet.sh` (from sparky) + `scripts/deploy-droplet.sh` (on the droplet, as root):
- containers `thepenmarket` (the app, image built from `server/Dockerfile`) and `thepenmarket-db` (postgres:16-alpine, volume `thepenmarket_pgdata`) on the `web` network; Caddy block `penmarket.crossgen-ai.com` in `/root/proxy/Caddyfile`.
- files: `/srv/apps/thepenmarket/build` (build context), `/srv/apps/thepenmarket/media` (photos, mounted read-only), `/srv/apps/thepenmarket/uploads` (form photos), `/srv/apps/thepenmarket/.env` (secrets), `db.sql.gz` (the dump restored on first run).
- redeploy: `bash scripts/push-droplet.sh` from sparky (rsyncs, dumps the sparky database, rebuilds the image, swaps the container, health-checks). The Rust build takes about 8 minutes on the droplet's two cores.
- rollback: `docker run` the `thepenmarket:previous` image with the same flags.
