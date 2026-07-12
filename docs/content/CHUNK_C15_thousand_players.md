# CHUNK C15 — 1,000 players on screen

**Status: DONE (2026-07-11)** — measured, then shipped: snapshots 10 Hz; per-zone shared serialization (entities serialize once per tick, frames assembled per client — Out::Raw path in net.rs); per-conn SetAoi (bots run 150 u); tick telemetry (60 s avg/max + slow-tick warns); swarm harness `antediluvia-swarm N url aoi`; client capsule-impostor LOD beyond 350 u with nameplate cull; players-never-collide pinned by unit test; proto v13.
Measured on the 8 GB M1: 1,000 bots → avg tick 6.3 ms / max 34 ms, server 78 MB RSS, zero slow ticks headless; with a graphical client in the crowd (433 MB RSS, screen full of players, camera interactive) occasional 50–100 ms spikes under whole-machine load. Crowd screenshot verified. Honest gaps: LOD is chosen at spawn (re-evaluated only via AoI churn), wire is still JSON (shared-frame trick made binary unnecessary at this scale).

## Goal
Up to 1,000 concurrent players visible in one zone at playable frame rates,
walking **through** each other (players never collide with players — verify
and pin with a test).

## Design (measure first — profile before optimizing)
1. **Load harness**: a bot-swarm binary (reuse test-client) that connects N
   scripted players who wander/fight near the inn. Drive N up; record server
   tick time + client FPS + bandwidth at N = 100/250/500/1000.
2. **Wire**: full-zone JSON snapshots every tick will not survive 1000
   entities. Move to interest-managed deltas: per-client area-of-interest
   (~visual range), send only changed fields, binary encoding
   (e.g. bincode/postcard) behind the same ServerMsg surface; keep JSON for
   the login/notice path so the test scripts stay easy.
3. **Client**: instanced/shared meshes + materials for player rigs, LOD or
   impostor beyond mid-range, cap animation updates for distant rigs,
   nameplates culled by distance.
4. **Server**: spatial hash for AoI queries (likely already needed by combat);
   budget: 1000 players in one zone at 10 Hz snapshots.
5. Keep 8 GB M1 in mind for local testing — the swarm can run headless with
   tiny per-bot buffers; never swarm + client build simultaneously.

## Verify
- Swarm of 1000 headless bots + 1 graphical client: client stays interactive,
  server tick under budget; screenshot the crowd.
- Unit: player-player movement resolves with no collision displacement.
