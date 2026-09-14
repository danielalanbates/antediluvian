# 2026-09-13 — web deploy gate, crash fix, far grass

## Web deploy gate (use before every batesai.org/play push)
1. `scripts/build_web.sh` (≈30–40 min; 21 MB wasm).
2. Local test server on 8797 (never the live shard — test characters would
   land in production) + `python3 -m http.server 8099` in `web/dist`.
3. Headless Chromium (Playwright, SwiftShader): load
   `http://127.0.0.1:8099/?server=ws://127.0.0.1:8797&name=<new name>`,
   run ≥180 s LOGGED IN (the builder screen streams no mobs and hides
   entity races), fail on any wasm panic / non-winit page error.
4. Site: build from `/Users/daniel/.git` (the checkout live was deployed
   from), diff every page vs live with tags stripped — must be identical
   except /play — then `npx wrangler pages deploy dist --project-name=website
   --branch=main --commit-dirty=true`, then re-run the gate on live.

Headless SwiftShader renders characters semi-transparent at ~1 fps in BOTH
the old and new builds — an artifact, not a regression.

## Crash caught by the gate (fixed before deploy)
`bevy_hierarchy child_builder.rs:202 "entity ... does not exist"`: hair /
species-part / gear grafts queued `with_children` on rig bones in Update
while the snapshot system despawned the same mob that frame. Graft systems
now run in PreUpdate; component inserts use `try_insert`.

## Far grass
The swaying tuft field ends at 210u (~3 character heights), so grass
visibly stopped near the player. Added static merged grass meshes per 120u
chunk out to 1000u (1 tuft / 20u cell), cached per chunk. See `far_grass.jpg`.

## Deployed
Live /play build stamp `f5e78c12f942` (crash fix + PR #1–#3 work; far grass
is NOT in it yet). Previous live bundle archived at
`~/Downloads/Antediluvia-archived/web-play-5ade7a1fba8a`.

## Found, not fixed
- Live `https://batesai.org/membership/` 308-redirects to itself (infinite
  loop) — `functions/membership.js` shadows the slash path too. Pre-existing.
- Live shard (Oracle) still runs the older server: "Login rejected" message
  shows a missing-glyph box until the shard is redeployed.

## Update 20:40 — far grass deployed
Live /play stamp `846472336e88` (PR #4 incl. far grass), gate PASS locally
(200 s logged in) and on live. Site pages diffed identical to live before
deploy. Prior bundle archived at `web-play-f5e78c12f942`.
