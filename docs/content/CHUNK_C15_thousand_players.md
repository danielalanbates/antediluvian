# CHUNK C15 — 1,000 players on screen

**Status: todo** (Daniel directive 2026-07-11: alpha readiness)

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
