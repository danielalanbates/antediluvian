# Private dev site — batesai.org/dev (Daniel, 2026-09-14)

**Rule:** no local test servers or local play-testing. Pre-release builds are
tested only on the private dev site; /play gets a build only after it passes there.

| Piece | Where |
|---|---|
| Web client | `batesai.org/dev/play/` — site repo `public/dev/play` (copy of `web/dist`, `ANTEDILUVIA_SERVER` = `wss://<host>/dev/ws`) |
| Gate | site repo `functions/dev/_middleware.js`: 404 unless cookie from `/dev/?key=<DEV_KEY>`; noindex, private cache |
| Key | Pages secret `DEV_KEY`; copy at `~/Library/Application Support/BatesAI/keys/antediluvia-dev-key.txt` (never in git) |
| WS proxy | site repo `functions/dev/ws.js` → `159-54-191-177.sslip.io:8080` |
| Dev server | Oracle VM: `antediluvia-dev.service`, `:8788` (iptables NAT 8080→8788, OCI list opened 8080), DB `/var/lib/antediluvia-dev/`, binary `/usr/local/bin/antediluvia-server-dev`, source `~ubuntu/antediluvia-dev-src` |

Deploy a dev build: `scripts/deploy_dev.sh` (server rsync+build+restart, web build staged to /dev/play, site deploy).
Production shard (`antediluvia.service`, :8787) is untouched by dev deploys.
