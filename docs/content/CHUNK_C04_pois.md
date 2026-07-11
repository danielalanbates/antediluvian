# CHUNK C04 — Points of interest & discovery

**Status: DONE (2026-07-10)**

## Goal
Each act contains named micro-locations from the POI archives; walking into
one for the first time announces it ("Discovered: The Smoldering Watcher-Tech
Armory") and grants discovery XP, WoW-style.

## Read first
- `docs/locations/points_of_interest/POI_Archive_01.md` — first 3 entries
  (format: name, Region, Coordinates [x,y], Visuals, Acoustics, Lore).
- `crates/server/src/world.rs` — zone dimensions and entity placement.

## Design
1. Generator (`scripts/gen_pois.py` → `assets/data/pois.json`): parse all 20
   archives (2,000 POIs): `{id, name, region→act, x, y}`. Rescale doc
   coordinates into each act's playable bounds deterministically (doc coords
   span roughly ±8000; divide/clamp to map size — note the transform).
   Cap at ~40 POIs per act (deterministic pick by id hash) so zones aren't
   wallpapered; log how many were dropped.
2. Server: load at boot; track `discovered: Vec<String>` on the sheet
   (persist via SheetExt — remember the THREE-place rule in the README).
   On movement tick, if within POI radius (~120u) and not yet discovered:
   mark, grant XP (50 × act tier), send a Notice ("Discovered: …").
3. Client: show the notice big and centered for ~3 s (gold text, fade), and
   drop a small marker mesh (stone cairn from existing props) at POI sites
   so they're findable.
4. POIs become anchor points later chunks reuse (C02 questgivers, C09 cave
   entrances) — expose a `poi_near(act, seed)` helper.

## Verify
- Unit test: discovery grants XP once, persists across save/load.
- Live: walk a client into a POI, screenshot the discovery banner; check
  the sheet's discovered list via a second login.
