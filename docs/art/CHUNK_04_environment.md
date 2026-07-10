# CHUNK 04 — Environment sets

**Status: todo**

## Goal
Real vegetation, rocks, and an inn *building* instead of cone-trees, sphere-
rocks, and a yellow disc. Per-act flavor.

## Assets (all CC0, GLB, no rigs needed — static props)
- KayKit **Medieval Builder / Village** packs — houses, fences, wells, carts:
  github.com/KayKit-Game-Assets (pick the repo with gltf/ dirs; clone depth 1,
  copy only needed GLBs into `assets/models/props/`).
- Quaternius **Ultimate Nature** / **Stylized Nature** packs — trees, rocks,
  bushes, stumps (quaternius.com → itch download is a zip; the
  github mirror gltf-universal-* only has anims — for nature use
  https://github.com/quaternius has some; otherwise fetch the itch zip via
  curl). If download friction is high, KayKit "Forest" / "Nature" packs also
  exist on the same GitHub org and are easier: prefer them.
- Record every pack + license in `docs/art/LICENSES.md`.

## Steps
1. Resources: server tags are `"tree"`/`"rock"` (`world.rs populate_zone`).
   In `spawn_visual`, replace the procedural tree/rock with `SceneRoot` props;
   hash the entity id to pick 1-of-3 tree variants and randomize yaw+scale
   (0.9–1.3×) for natural variety. Scale to match old silhouettes (~50u tall
   trees) so harvest click-targets feel the same.
2. Inn: at the zone entry, spawn a tavern/house prop + a couple of fences and
   a well; keep (shrink) the gold ring as the rested-XP boundary indicator.
3. Scatter *non-gameplay* decor client-side: bushes/grass-tufts/pebbles from
   the same seeded noise as CHUNK_03 (positions deterministic per act, ~200
   instances, skip within 260u of the inn). These have no server entity — pure
   visuals, so the thin-client rule is intact.
4. Per-act palettes: pick prop subsets per act (lush for Eden, pines/rocks for
   Hermon, dead trees for Nephilim, buildings for Enoch, driftwood for Flood).
   A simple `match act` table is fine.

## Verify
Screenshots in ≥2 acts: varied trees where old cones were, inn building at
entry, decor scatter, no z-fighting, and harvesting a tree still works
(attack near a tree → wood in inventory notice, node visual respawns).
