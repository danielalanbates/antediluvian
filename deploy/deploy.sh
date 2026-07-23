#!/bin/bash
# Deploy the Antediluvia server to an Ubuntu ARM64 VM (Oracle Always Free).
# Builds release ON the VM (native aarch64 — no cross-compile toolchain needed).
#
# Usage: ./deploy.sh <ssh-key> <user@host> <domain>
#   ./deploy.sh ~/.ssh/oci.key ubuntu@129.x.x.x play.batesai.org
set -euo pipefail

KEY="${1:?ssh key path}"; HOST="${2:?user@host}"; DOMAIN="${3:?domain for TLS}"
HERE="$(cd "$(dirname "$0")/.." && pwd)"          # repo root
SSH="ssh -i $KEY -o StrictHostKeyChecking=accept-new $HOST"
RSYNC_E="ssh -i $KEY -o StrictHostKeyChecking=accept-new"

echo "==> [1/5] Installing build deps + Caddy on the VM"
$SSH 'sudo bash -s' <<'REMOTE'
set -e
export DEBIAN_FRONTEND=noninteractive
apt-get update -qq
apt-get install -y -qq build-essential pkg-config libssl-dev git rsync curl debian-keyring debian-archive-keyring apt-transport-https
# Rust (system-wide, if absent)
if ! command -v cargo >/dev/null; then
  curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh -s -- -y --no-modify-path
fi
# Caddy (auto-TLS reverse proxy)
if ! command -v caddy >/dev/null; then
  curl -1sLf 'https://dl.cloudsmith.io/public/caddy/stable/gpg.key' | gpg --dearmor -o /usr/share/keyrings/caddy-stable-archive-keyring.gpg
  curl -1sLf 'https://dl.cloudsmith.io/public/caddy/stable/debian.deb.txt' | tee /etc/apt/sources.list.d/caddy-stable.list >/dev/null
  apt-get update -qq && apt-get install -y -qq caddy
fi
id antediluvia >/dev/null 2>&1 || useradd --system --home /var/lib/antediluvia --create-home antediluvia
mkdir -p /var/lib/antediluvia && chown antediluvia:antediluvia /var/lib/antediluvia
REMOTE

echo "==> [2/5] Syncing source (protocol + sim + server crates)"
$SSH 'mkdir -p ~/antediluvia-src/crates'
rsync -az --delete -e "$RSYNC_E" \
  --exclude target --exclude '*.sqlite*' \
  "$HERE/Cargo.toml" "$HERE/Cargo.lock" "$HOST:~/antediluvia-src/"
for c in protocol sim server; do
  rsync -az --delete -e "$RSYNC_E" --exclude target "$HERE/crates/$c" "$HOST:~/antediluvia-src/crates/"
done

echo "==> [3/5] Building release on the VM (this can take a few minutes)"
$SSH 'source ~/.cargo/env 2>/dev/null || true; cd ~/antediluvia-src && cargo build --release -p antediluvia-server'

echo "==> [4/5] Installing binary + systemd service"
$SSH 'sudo bash -s' <<'REMOTE'
set -e
install -m755 ~/antediluvia-src/target/release/antediluvia-server /usr/local/bin/antediluvia-server
REMOTE
rsync -az -e "$RSYNC_E" "$HERE/deploy/antediluvia.service" "$HOST:/tmp/antediluvia.service"
$SSH 'sudo mv /tmp/antediluvia.service /etc/systemd/system/antediluvia.service && sudo systemctl daemon-reload && sudo systemctl enable --now antediluvia && sudo systemctl restart antediluvia'

echo "==> [5/5] Configuring Caddy TLS reverse proxy for $DOMAIN"
$SSH "sudo bash -s" <<REMOTE
set -e
cat > /etc/caddy/Caddyfile <<EOF
$DOMAIN {
    reverse_proxy 127.0.0.1:8787
}
EOF
systemctl restart caddy
REMOTE

echo "==> Done. Server live at wss://$DOMAIN  (health: journalctl -u antediluvia -f)"
