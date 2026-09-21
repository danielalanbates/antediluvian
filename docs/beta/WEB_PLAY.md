# Web play — smallest safe path to game-live-on-batesai.org

Branch: `feature/web-play` (from `origin/main`). Status: recon + build
verification + edge-config drafts done; **no live changes made**.

## The architecture (already working — keep it)

- **Client:** Bevy WASM, `scripts/build_web.sh` → `web/dist/`, copied into the
  site repo as `public/play/` → served at `https://batesai.org/play/`.
- **TLS:** browsers open `wss://batesai.org/ws` (same-origin; `web/index.html`
  derives it from the page URL). Cloudflare terminates TLS and a Pages
  Function proxies the upgrade to the shard over plain HTTP — **the shard
  needs no certificate and no DNS of its own**.
- **Shard:** Oracle Always Free VM `159.54.191.177`, `antediluvia.service`,
  `ws://…:8787`; iptables NAT `:80 → :8787` because Cloudflare's outbound
  subrequests can't use port 8787.
- **Fragile hop:** the Function currently addresses the shard through
  `159-54-191-177.sslip.io` (Cloudflare refuses subrequests to bare IPs).
  Draft replacement in `web/cloudflare/ws.js` uses `shard.batesai.org`.

## What "playable" means (the gate)

Every `/play/` update passes the web gate before the site deploys — see
`docs/verification/web-play-staging-checklist.md`. The gate caught a real
crash on 2026-09-13; it is the thing standing between a broken build and the
live site. Daniel's standing rule: pre-release builds are tested **only** on
the private dev site (`batesai.org/dev/play`, docs/beta/DEV_SITE.md); `/play`
gets a build only after it passes there.

## Step 1 — replace sslip.io with a real DNS record (the one change that matters)

Nothing else on the critical path is fragile; this is the whole of Step 1.

1. **Daniel** adds in Cloudflare DNS: `A  shard  159.54.191.177`, proxy OFF
   (grey cloud — orange would hairpin the Function's subrequest through Pages).
2. Verify `dig shard.batesai.org` → `159.54.191.177`.
3. Copy `web/cloudflare/ws.js` (this branch) over the site repo's
   `functions/ws.js`.
4. `npx wrangler pages deploy dist --project-name=website --branch=main`
   from the site checkout (his credentials).
5. Re-run the gate against live `/play/`.

Rollback: Pages deployment history → previous deployment (seconds). The old
`SHARD_ORIGIN` is kept as a comment in the draft.

## Step 2 — routine /play updates (no approvals beyond the deploy itself)

1. Work on a feature branch; green test suite before merge.
2. `scripts/build_web.sh` (≈30–40 min; wasm must stay under 25 MB — the
   script fails the build otherwise).
3. Web gate on the checklist (fresh test server on `:8797` — **never the live
   shard**, test characters would land in production — + `python3 -m http.server`
   on `web/dist`; headless Chromium ≥180 s logged in; fail on wasm panic).
4. Copy `web/dist` → site checkout `public/play/`, build site, diff every
   page vs live (must be identical except `/play/`), deploy, re-run gate live.
5. Archive the previous bundle (`web-play-<stamp>` convention).

## Step 3 — shard updates (his VM, his go-ahead, every time)

- Server-only change: rsync source to VM, `cargo build --release` on the VM,
  DB backup
  (`sqlite3 /var/lib/antediluvia/antediluvia.sqlite ".backup /var/lib/antediluvia/pre-<date>.sqlite"`),
  `sudo systemctl restart antediluvia`.
- Health check: `systemctl is-active`, journal shows `listening on ws://`,
  then a real handshake through `wss://batesai.org/ws`.
- **Protocol version:** bump `PROTOCOL_VERSION` only on wire-incompatible
  changes, and never restart the shard with a protocol the live `/play`
  client can't speak (deploy the client first or keep backward compat).
- Rollback: keep the previous release binary as `antediluvia-server.bak`
  before install; `systemctl restart` reverts in seconds.

## What needs Daniel's explicit approval

1. **DNS** — the `shard.batesai.org` A record (only he has the Cloudflare account).
2. **Anything on the Oracle VM** — SSH + go-ahead per change (abuse-heuristic
   and lockout risks are real; NAT/port rules must be documented before touching).
3. **Site repo deploys** — run from his checkout with his wrangler credentials,
   only after the web gate passes.
4. **The dev site** (docs/beta/DEV_SITE.md) — same approvals, plus the dev
   systemd unit and cookie-gate middleware; draft proxy in
   `web/cloudflare/dev-ws.js`.
5. **Apple login** — deferred, not attempted (needs his Apple Developer account).

## Risks

- **1-core VM:** TLS terminates at Cloudflare's edge — the box pays ~nothing
  for it. Game loop measured 6.3 ms/tick @1000 players; the box is sized for a
  handful of alpha players. Beta wants a bigger shape (A1 Flex is free to
  4 OCPU/24 GB).
- **Oracle host firewall:** Ubuntu images ship a default REJECT in INPUT —
  opening the VCN security list alone is not enough. Needed: 22 (deploy),
  80 (WS proxy target). 443 is not needed (no cert on the VM).
- **sslip.io on the critical path** until Step 1 lands.
- **Pages deploy blast radius:** every site deploy re-ships `/play` — the
  page-diff gate keeps marketing deploys from breaking the game.
- **DNS/registrar unverifiable from this sandbox** (stubbed resolver) —
  confirm Cloudflare DNS ownership in the dashboard before Step 1.
