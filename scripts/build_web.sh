#!/usr/bin/env bash
# Build the browser client into web/dist/ — a self-contained directory ready to
# publish (Cloudflare Pages, or any static host).
#
#   scripts/build_web.sh
#
# Output layout:
#   web/dist/index.html            loader + splash
#   web/dist/pkg/antediluvia.js    wasm-bindgen glue
#   web/dist/pkg/*_bg.wasm         the engine
#   web/dist/assets/               models, textures, audio, data
#   web/dist/build.txt             build stamp + source commit (for deploy notes)
#
# Cache busting: index.html loads pkg/ as pkg/<file>?v=<stamp>, the stamp being
# a hash of the engine, so a host may cache pkg/ forever and a browser can
# never pair a new page with an old engine. index.html itself must be served
# revalidated (Cloudflare Pages' default, max-age=0). The page derives its
# server from its own host (wss://<host>/ws), so one bundle serves any host;
# a client older than the server is told to refresh (crates/client-bevy/src/beta.rs).
#
# Cloudflare Pages rejects any single file over 25 MB, which an unoptimized
# wasm build exceeds — the wasm-release profile plus wasm-opt is what keeps it
# under. The check at the end is a hard failure, not a warning, so a too-large
# build never reaches a deploy.
set -euo pipefail

cd "$(dirname "$0")/.."
ROOT="$PWD"
OUT="$ROOT/web/dist"
CRATE=antediluvia-client-bevy
# The [[bin]] name keeps its dashes, so this is not the usual underscored path.
WASM="$ROOT/target/wasm32-unknown-unknown/wasm-release/${CRATE}.wasm"
MAX_BYTES=$((25 * 1024 * 1024))

have_dir() { [ -d "$1" ]; }
# Toolchain lookup, in order: an explicit RUST_TOOLCHAIN_BIN, the rustup
# toolchain matching this machine (macOS arm64 keeps Daniel's original path),
# then whatever cargo/rustc is already on PATH. The old script hardcoded only
# the macOS path, which silently did nothing on Linux builders.
ARCH="$(uname -m)"
case "$ARCH" in
  arm64|aarch64) MAC_TOOLCHAIN="$HOME/.rustup/toolchains/stable-aarch64-apple-darwin/bin" ;;
  x86_64)       MAC_TOOLCHAIN="$HOME/.rustup/toolchains/stable-x86_64-unknown-linux-gnu/bin" ;;
  *)            MAC_TOOLCHAIN="" ;;
esac
# Prepend in reverse priority order so the highest-priority dir ends up first
# on PATH (a plain prepend loop would leave the lowest-priority dir first).
for d in "$HOME/.cargo/bin" "$MAC_TOOLCHAIN" "${RUST_TOOLCHAIN_BIN:-}"; do
  if [ -n "$d" ] && have_dir "$d"; then
    case ":$PATH:" in *":$d:"*) ;; *) PATH="$d:$PATH" ;; esac
  fi
done
export PATH
unset -f have_dir

if ! rustc --print target-list 2>/dev/null | grep -q '^wasm32-unknown-unknown$'; then
  echo "!! wasm32-unknown-unknown std not installed for $(rustc -vV | grep '^host' || echo '?')" >&2
  echo "!! install it (needs a working rustup): rustup target add wasm32-unknown-unknown" >&2
fi
command -v cargo >/dev/null || { echo "FAIL: cargo not on PATH" >&2; exit 1; }

echo "==> compiling wasm (size-optimized; this is slow)"
cargo build -p "$CRATE" --target wasm32-unknown-unknown --profile wasm-release

echo "==> generating js bindings"
rm -rf "$OUT"
mkdir -p "$OUT/pkg"
wasm-bindgen --target web --no-typescript \
  --out-dir "$OUT/pkg" --out-name antediluvia "$WASM"

if command -v wasm-opt >/dev/null 2>&1; then
  echo "==> wasm-opt"
  # `-all` permits the wasm features rustc already emits (bulk memory, sign
  # extension, reference types...). Without it wasm-opt refuses to even
  # validate the input. It permits, it does not introduce: nothing here makes
  # the output need a newer browser than the compiler already required.
  wasm-opt -all -Os --strip-debug --strip-producers \
    "$OUT/pkg/antediluvia_bg.wasm" -o "$OUT/pkg/antediluvia_bg.wasm.opt"
  mv "$OUT/pkg/antediluvia_bg.wasm.opt" "$OUT/pkg/antediluvia_bg.wasm"
else
  echo "!! wasm-opt not found (brew install binaryen) — shipping unoptimized"
fi

echo "==> copying loader and assets"
# Stamp the loader with a hash of the engine it was built against, so the host
# can cache pkg/ indefinitely and a rebuild still busts it.
STAMP=$(shasum -a 256 "$OUT/pkg/antediluvia_bg.wasm" | cut -c1-12)
sed "s/__BUILD_STAMP__/$STAMP/g" "$ROOT/web/index.html" > "$OUT/index.html"
grep -q "__BUILD_STAMP__" "$OUT/index.html" && { echo "FAIL: stamp not applied" >&2; exit 1; }
echo "    build stamp: $STAMP"
COMMIT=$(git -C "$ROOT" rev-parse --short=12 HEAD 2>/dev/null || echo unknown)
DIRTY=$(git -C "$ROOT" status --porcelain --untracked-files=no 2>/dev/null | grep -q . && echo "+dirty" || true)
printf 'stamp %s\ncommit %s%s\nbuilt %s\n' "$STAMP" "$COMMIT" "$DIRTY" "$(date -u +%Y-%m-%dT%H:%M:%SZ)" > "$OUT/build.txt"
# Same exclusions as make_app.sh: assets/ holds 3.2 GB of gathered CC0 source
# models the game does not reference yet. Shipping the lot would be a 3.3 GB
# download. Keep this list in step with make_app.sh.
mkdir -p "$OUT/assets"
rsync -a --exclude 'models/polyhaven' --exclude 'models/quaternius' --exclude 'models/kaykit' --exclude 'hdri' --exclude 'art' --exclude 'textures/terrain_src' \
  "$ROOT/assets/" "$OUT/assets/"

size=$(wc -c < "$OUT/pkg/antediluvia_bg.wasm" | tr -d ' ')
echo
echo "wasm:   $(du -h "$OUT/pkg/antediluvia_bg.wasm" | cut -f1)"
echo "assets: $(du -sh "$OUT/assets" | cut -f1)"
echo "total:  $(du -sh "$OUT" | cut -f1)"

if [ "$size" -gt "$MAX_BYTES" ]; then
  echo
  echo "FAIL: wasm is ${size} bytes, over the ${MAX_BYTES}-byte host limit." >&2
  exit 1
fi
# The limit is per file, so check every file, not only the engine.
big=$(find "$OUT" -type f -size +$((MAX_BYTES / 1024))k)
if [ -n "$big" ]; then
  echo
  echo "FAIL: over the ${MAX_BYTES}-byte host limit:" >&2
  echo "$big" >&2
  exit 1
fi
echo "OK: every file ($(find "$OUT" -type f | wc -l | tr -d ' ')) is under the 25 MB per-file host limit."
