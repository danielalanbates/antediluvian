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

# Account identity: Sign in with Apple via the bundled helper (real Apple
# `user` id when the app is provisioned with the applesignin entitlement;
# otherwise a stable per-machine UUID). Never the raw $USER.
# Continuity: pre-helper saves were keyed by $USER — keep that identity for
# an existing install so nobody loses their character.
if [ -f "$SUPPORT/antediluvia.sqlite" ] && [ ! -f "$SUPPORT/local_account_id" ] && [ ! -f "$SUPPORT/apple_user_id" ]; then
  printf '%s' "$USER" > "$SUPPORT/local_account_id"
fi
APPLE_ID="$("$RES/apple-signin" 2>>"$SUPPORT/client.log" || true)"
[ -n "$APPLE_ID" ] || APPLE_ID="$USER"

CLIENT_ARGS=("$APPLE_ID" "$USER")
[ -n "$SERVER_URL" ] && CLIENT_ARGS+=("$SERVER_URL")
"$RES/antediluvia-client-bevy" "${CLIENT_ARGS[@]}" >>"$SUPPORT/client.log" 2>&1
STATUS=$?

if [ -n "$SERVER_PID" ] && kill -0 "$SERVER_PID" 2>/dev/null; then
  kill "$SERVER_PID" 2>/dev/null
fi
exit $STATUS
