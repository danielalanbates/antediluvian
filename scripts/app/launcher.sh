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

SERVER_PID=""
if ! nc -z 127.0.0.1 8787 2>/dev/null; then
  # One server only: kill any stray non-listening leftovers first.
  pkill -f antediluvia-server 2>/dev/null
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

"$RES/antediluvia-client-bevy" "$APPLE_ID" "$USER" >>"$SUPPORT/client.log" 2>&1
STATUS=$?

if [ -n "$SERVER_PID" ]; then
  kill "$SERVER_PID" 2>/dev/null
fi
exit $STATUS
