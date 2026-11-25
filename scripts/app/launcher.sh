#!/bin/bash
# Antediluvia launcher: starts a local server (if none is listening) with its
# DB in Application Support, then runs the Bevy client. If this launcher
# started the server, it shuts it down when the client exits.
set -u
RES="$(cd "$(dirname "$0")/../Resources" && pwd)"
SUPPORT="$HOME/Library/Application Support/Antediluvia"
mkdir -p "$SUPPORT"

export ANTEDILUVIA_ASSETS="$RES/assets"
export ANTEDILUVIA_DB="$SUPPORT/antediluvia.sqlite"

# Server selection. Default is the bundled single-player server on localhost.
# Point at the hosted shard for real multiplayer with either:
#   ANTEDILUVIA_SERVER=wss://play.batesai.org  (env override, wins)
#   a one-line file at "$SUPPORT/server_url"   (persists across launches)
SERVER_URL="${ANTEDILUVIA_SERVER:-}"
if [ -z "$SERVER_URL" ] && [ -f "$SUPPORT/server_url" ]; then
  SERVER_URL="$(tr -d ' \t\r\n' < "$SUPPORT/server_url")"
fi
# Beta bundles carry a default shard (the private dev shard). Write "local"
# to $SUPPORT/server_url to force the bundled single-player server instead.
if [ -z "$SERVER_URL" ] && [ -f "$RES/default_server_url" ]; then
  SERVER_URL="$(tr -d ' \t\r\n' < "$RES/default_server_url")"
fi
[ "$SERVER_URL" = "local" ] && SERVER_URL=""

SERVER_PID=""
if [ -n "$SERVER_URL" ]; then
  # Remote shard: never spawn a local server. Never kill servers this
  # launcher did not start (other local shards may be running on purpose).
  :
elif ! nc -z 127.0.0.1 8787 2>/dev/null; then
  # Start our own server; only the PID started here is ever killed.
  "$RES/antediluvia-server" >>"$SUPPORT/server.log" 2>&1 &
  SERVER_PID=$!
  for _ in $(seq 1 50); do nc -z 127.0.0.1 8787 2>/dev/null && break; sleep 0.1; done
fi

# Account identity: a per-machine id kept in "$SUPPORT/local_account_id",
# "local-<uuid>", created on first launch. Same file and format as the Sign in
# with Apple helper this replaces (dropped 2026-11-24, see
# archive/apple-sign-in/), so existing local players keep their characters.
# Continuity for older installs, in the helper's order:
#   - an apple_user_id cached by the helper still wins (never issued in
#     practice: the entitlement was never provisioned);
#   - pre-helper saves were keyed by $USER: an install with a DB but no id
#     file keeps $USER as its id.
ACCOUNT_ID=""
for f in "$SUPPORT/apple_user_id" "$SUPPORT/local_account_id"; do
  if [ -z "$ACCOUNT_ID" ] && [ -s "$f" ]; then
    ACCOUNT_ID="$(tr -d ' \t\r\n' < "$f")"
  fi
done
if [ -z "$ACCOUNT_ID" ]; then
  if [ -f "$SUPPORT/antediluvia.sqlite" ] && [ ! -e "$SUPPORT/local_account_id" ]; then
    ACCOUNT_ID="$USER"
  else
    UUID="$(uuidgen 2>/dev/null || od -An -N16 -tx1 /dev/urandom | tr -d ' \n')"
    ACCOUNT_ID="local-$(printf '%s' "$UUID" | tr 'A-Z' 'a-z')"
  fi
  printf '%s' "$ACCOUNT_ID" > "$SUPPORT/local_account_id"
fi

# Only the account id: a new account gets the character builder. (The old
# launcher also passed $USER, which named a new character after the macOS
# account and skipped the builder, on a public shard too.)
CLIENT_ARGS=("$ACCOUNT_ID")
[ -n "$SERVER_URL" ] && CLIENT_ARGS+=("$SERVER_URL")
"$RES/antediluvia-client-bevy" "${CLIENT_ARGS[@]}" >>"$SUPPORT/client.log" 2>&1
STATUS=$?

if [ -n "$SERVER_PID" ] && kill -0 "$SERVER_PID" 2>/dev/null; then
  kill "$SERVER_PID" 2>/dev/null
fi
exit $STATUS
