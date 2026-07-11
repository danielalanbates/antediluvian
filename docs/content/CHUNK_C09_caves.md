# CHUNK C09 — Caves & mines

**Status: done** (requires C03; C04 helpful)

## Goal
Acts contain enterable cave/mine sites from the cave archives: a visible
entrance, a darker interior area with richer ore nodes and tougher mobs —
the game's first dungeon-lite loop.

## Read first
- `docs/locations/caves/Cave_Archive_01.md` — first 3 entries (name, region,
  coords, Primary Resources, lore).
- `crates/server/src/world.rs` — resource node spawning.

## Design — keep it cheap (no real interiors yet)
1. Generator (`scripts/gen_caves.py` → `assets/data/caves.json`), same
   region→act mapping and coordinate rescale as C04. Pick ~6 caves per act.
2. A cave is a **surface pocket**, not an instance: server marks a circular
   cave area; inside it spawn the cave's Primary Resources as ore nodes
   (2× yield, feeds mining skill) plus 3–5 mobs one tier above the act norm
   and a mini-boss (`<cave>_dweller`, alpha stats) that drops crafting rares.
3. Client: rock-arch entrance from the KayKit props at the cave center,
   darkened local lighting inside the radius (spot the existing per-act mood
   system: multiply ambient when camera target is in a cave area), fog
   tightened. Name banner on entry reuses C04's discovery mechanism if
   present (caves count as POIs).
4. New ores (Luminous Quartz, Orichalcum…) enter the professions table with
   one craftable upgrade each (e.g. orichalcum_blade > bronze_sword).

## Verify
- Unit tests: cave nodes yield the archive's resources; mini-boss drop.
- Live: enter a cave, screenshot the darkened interior + entrance arch;
  mine a node and craft the new recipe via test-client.

## Done notes (2026-07-11)
- 30 caves (6/act) in assets/data/caves.json via scripts/gen_caves.py.
- Pocket contents: 4 ore_<resource> nodes (2x yield, mining), 3 next-act
  mobs, <resource>_dweller mini-boss (4x hp, guaranteed resource drop,
  respawns at its cave). Ores respawn inside the pocket.
- Discovery reuses the C04 flow (+75 xp * tier). Recipes: orichalcum_blade
  (mining 10), luminous_charm (mining 5).
- Client: 3-rock entrance arch per cave; ambient x0.25 + fog x4 inside.
- Verified: 31 unit tests; wire E2E (discovery notice, double sulfur yield,
  mining +1, dweller present); screenshots show the darkened interior and
  the dweller nameplate. CAVEAT: the entrance-arch rocks were not visually
  isolated in the dark screenshot — they use the same spawn_prop path as
  the C04 cairns; eyeball them next session in daylight.
