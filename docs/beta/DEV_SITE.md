# Private dev site — batesai.org/dev (Daniel, 2026-09-14)

**Rule:** no local test servers or local play-testing. Pre-release builds are
tested only on the private dev site; /antediluvia gets a build only after it passes there.

| Piece | Where |
|---|---|
| Web client | `batesai.org/dev/play/` — deployed as `dist/dev/play` of the site build (copy of `web/dist`, `ANTEDILUVIA_SERVER` = `wss://<host>/dev/ws`) |
| Gate | site repo `functions/dev/_middleware.js`: 404 unless cookie from `/dev/?key=<DEV_KEY>`; noindex, private cache |
| Key | Pages secret `DEV_KEY`; copy at `~/Library/Application Support/BatesAI/keys/antediluvia-dev-key.txt` (never in git) |
| WS proxy | site repo `functions/dev/ws.js` → `159-54-191-177.sslip.io:8080` |
| Dev server | Oracle VM: `antediluvia-dev.service`, `:8788` (iptables NAT 8080→8788, OCI list opened 8080), DB `/var/lib/antediluvia-dev/`, binary `/usr/local/bin/antediluvia-server-dev`, source `~ubuntu/antediluvia-dev-src` |

Deploy a dev build: `scripts/deploy_dev.sh` (server rsync+build+restart, web build staged to /dev/play, site deploy).
Production shard (`antediluvia.service`, :8787) is untouched by dev deploys.

## Current state (2026-11-24)

- Dev shard: the Sep 14 binary (proto v14) and its DB. On 11-24 protocol 17 was deployed and then rolled back, because the quest bot failed there (see STATUS_2026-11-24 section 7). The p17 binary and DB are at `~/dev-p17-20261124/` on the VM; the old ones are at `~/dev-backup-20261124/`.
- /dev/play: **not deployed, and there is no gate.** Production (GitHub `website` main `3e8a959`) has no `functions/dev`, so `/dev/*` serves the homepage fallback.
- **Where the site source is:** production is now deployed from GitHub `danielalanbates/website` main (it was `3e8a959` on 11-24), not from `/Users/daniel/.git`. `scripts/deploy_dev.sh`'s web half still builds in `/Users/daniel/.git`, which no longer has `public/`. **Do not run the web half as it is.** It would publish a stale site. Instead, take a tarball of the current main (no clone), add `functions/dev/*` and `dist/dev/play`, diff against live, then deploy. Any non-dev deploy that leaves out `functions/dev/` silently turns the gate off (the 11-20 deploys did).
- Live gate: `scripts/web_gate.py --url https://batesai.org/dev/play/ --key-file "<key file>" --server-log <journalctl -fu antediluvia-dev output> --secs 200 ...`
