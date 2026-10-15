#!/usr/bin/env bash
# Stripe TEST mode on a local build, without a key ever appearing on screen or in a file in the repo.
#   bash scripts/stripe-local.sh key            # after copying the test "Secret key" in the Stripe
#                                               # dashboard: saves it from the clipboard into the env file
#   bash scripts/stripe-local.sh listen [port]  # forwards Stripe test events to the local site (default
#                                               # 8140) and saves the matching webhook secret; leave running
# Needs the Stripe CLI (`brew install stripe/stripe-cli/stripe`) and `stripe login` once.
# Live keys (sk_live_/rk_live_) are refused: they belong only on the production server.
set -euo pipefail
ENV_FILE="${ENV_FILE:-$HOME/.config/thepenmarket.env}"
EVENTS="checkout.session.completed,checkout.session.expired,checkout.session.async_payment_succeeded,checkout.session.async_payment_failed,invoice.paid,customer.subscription.deleted"

put() { # put NAME VALUE: replace or append NAME=VALUE in the env file, mode 600, value never printed
  local name="$1" value="$2" tmp
  umask 077
  touch "$ENV_FILE"
  tmp="$(mktemp)"
  grep -v "^$name=" "$ENV_FILE" > "$tmp" || true
  printf '%s=%s\n' "$name" "$value" >> "$tmp"
  mv "$tmp" "$ENV_FILE"
  chmod 600 "$ENV_FILE"
}

case "${1:-}" in
  key)
    k="$(pbpaste | tr -d '[:space:]')"
    case "$k" in
      sk_test_*|rk_test_*) ;;
      sk_live_*|rk_live_*) echo "That is a LIVE key. Only test keys go on this machine; nothing was saved." >&2; exit 1 ;;
      *) echo "The clipboard does not hold a Stripe test secret key (sk_test_… or rk_test_…). Copy it from the dashboard and run this again." >&2; exit 1 ;;
    esac
    put STRIPE_SECRET_KEY "$k"
    echo "Saved STRIPE_SECRET_KEY (${k:0:8}…${k: -4}) to $ENV_FILE"
    ;;
  listen)
    port="${2:-8140}"
    command -v stripe >/dev/null || { echo "Install the Stripe CLI first: brew install stripe/stripe-cli/stripe" >&2; exit 1; }
    s="$(stripe listen --print-secret 2>/dev/null | tr -d '[:space:]')"
    case "$s" in whsec_*) ;; *) echo "No webhook secret from the Stripe CLI; run \`stripe login\` first." >&2; exit 1 ;; esac
    put STRIPE_WEBHOOK_SECRET "$s"
    echo "Saved STRIPE_WEBHOOK_SECRET (${s:0:6}…${s: -4}). Restart the site so it reads both values, then keep this running:"
    exec stripe listen --events "$EVENTS" --forward-to "localhost:$port/api/stripe/webhook"
    ;;
  *)
    sed -n '2,8p' "$0"; exit 1 ;;
esac
