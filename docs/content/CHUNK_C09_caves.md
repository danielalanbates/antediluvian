# CHUNK C09 — Caves & mines

**Status: todo** (requires C03; C04 helpful)

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
