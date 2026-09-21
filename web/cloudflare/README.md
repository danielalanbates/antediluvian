# Cloudflare edge config for Antediluvia — drafts, not applied

The game is served from Daniel's marketing site (Cloudflare Pages project
`website`, custom domain `batesai.org`). The edge pieces the game needs all
live in the **site repo**, not here — this directory versions them next to the
client so a future deploy can diff "what the game needs" against "what the
site has". **Nothing in this directory is live.** Copying any of it into the
site repo and redeploying is a live-site change and needs Daniel's explicit
go-ahead.

## Files

| File | Target in the site repo | Status |
|---|---|---|
| `ws.js` | `functions/ws.js` | Draft of the proxy with the `shard.batesai.org` origin (replaces sslip.io). Needs the DNS record first. |
| `dev-ws.js` | `functions/dev/ws.js` | Draft of the dev-shard proxy for the private dev site (docs/beta/DEV_SITE.md). Needs the dev VM pieces first. |

## Already live (reference — do not duplicate here)

- `functions/ws.js` (sslip.io origin, working) — the thing `ws.js` here proposes to replace.
- `public/_headers` cache rules for `/play/` (pkg immutable 1y, assets 1d, loader no-cache).
- `/play/` itself — currently served from the site repo's `public/play/` (copy of `web/dist`).

## Applying the production proxy change (needs Daniel)

1. **DNS (Daniel, Cloudflare dashboard):** `A  shard  159.54.191.177`, proxy OFF (grey cloud).
2. Verify: `dig shard.batesai.org` answers `159.54.191.177`.
3. Copy `ws.js` → site repo `functions/ws.js`.
4. `npx wrangler pages deploy dist --project-name=website --branch=main` from the site checkout.
5. Verify: `wss://batesai.org/ws` handshake reaches the shard (browser client loads and logs in at `/play/`).
6. Rollback: Pages dashboard → previous deployment; or restore the old `SHARD_ORIGIN` line (kept as a comment in `ws.js`) and redeploy.

## Applying the dev proxy (needs Daniel; bigger)

Requires, in order: the dev systemd unit + `:8788` + `8080→8788` NAT + OCI
8080 open on the Oracle VM (SSH + his go-ahead), the `shard.batesai.org` DNS
record, `functions/dev/_middleware.js` cookie gate (see docs/beta/DEV_SITE.md),
then this file at `functions/dev/ws.js`, then a site deploy. The dev site is
`batesai.org/dev/play` with `ANTEDILUVIA_SERVER=wss://<host>/dev/ws`.
