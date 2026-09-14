#!/usr/bin/env bash
# Push the current tree to the PRIVATE dev site (see docs/beta/DEV_SITE.md).
# Usage: scripts/deploy_dev.sh [--server-only|--web-only]
set -euo pipefail
cd "$(dirname "$0")/.."
K=~/.ssh/antediluvia_shard_ssh; H=ubuntu@159.54.191.177; SITE=/Users/daniel/.git
if [ "${1:-}" != "--web-only" ]; then
  rsync -az --delete -e "ssh -i $K" --relative crates/protocol crates/sim crates/server Cargo.toml Cargo.lock assets/data "$H:antediluvia-dev-src/" --exclude target
  ssh -i "$K" -n "$H" 'set -e; cd antediluvia-dev-src
    sed -i -E "s#^(default-)?members = \[.*#\1members = [\"crates/protocol\",\"crates/sim\",\"crates/server\"]#" Cargo.toml
    sed -i "/^\[patch.crates-io\]/,/^bevy_pbr/d" Cargo.toml
    source ~/.cargo/env && cargo build --release -p antediluvia-server 2>&1 | tail -1
    sudo install -m755 target/release/antediluvia-server /usr/local/bin/antediluvia-server-dev
    sudo systemctl restart antediluvia-dev && systemctl is-active antediluvia-dev'
fi
if [ "${1:-}" != "--server-only" ]; then
  ./scripts/build_web.sh
  rm -rf "$SITE/public/dev/play" && mkdir -p "$SITE/public/dev" && cp -R web/dist "$SITE/public/dev/play"
  sed -i '' "s#window.ANTEDILUVIA_SERVER = '';#window.ANTEDILUVIA_SERVER = 'wss://' + location.host + '/dev/ws';#" "$SITE/public/dev/play/index.html"
  (cd "$SITE" && npm run build && npx wrangler pages deploy dist --project-name=website --branch=main --commit-dirty=true)
fi
