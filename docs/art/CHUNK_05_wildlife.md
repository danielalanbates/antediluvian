# CHUNK 05 — Wildlife & enemy species variety

**Status: DONE 2026-07-10** (wildlife + per-act species; enemy models remain
the skeleton set from CHUNK_01 — full monster variety deferred).
Quaternius Animated Animals (CC0, self-contained .gltf w/ data-URI buffers,
via the pack's Google Drive; gdown) in `assets/models/wildlife/`: Deer,
Alpaca, Bull, ShibaInu, Fox (+License.txt). Clip index orderings —
herbivores (Alpaca/Bull/Deer): Attack_Headbutt=0 Death=2 Gallop=4 Idle=6;
predators (Fox/ShibaInu/Wolf): Attack=0 Death=1 Gallop=3 Idle=5.
Wildlife now uses the same rig/Mover pipeline as characters (sphere branch
deleted). Server wildlife tags: enoch raven→dog, flood fish→fox (cosmetic;
quests key on enemy tags). Verified on screen: Eden deer galloping, Flood
fox; 13 tests green, no panics.

## Goal
Animated animals instead of the beige sphere; per-act enemy identities so
Eden ≠ Flood visually.

## Assets
- Quaternius **Animals** / **Animated Animals** packs (CC0): deer, wolf, boar,
  bear, birds — rigged with Idle/Walk/Run/Attack/Death. Fetch the itch.io zip
  (`curl -L` the download URL) or any GitHub mirror; put GLBs in
  `assets/models/wildlife/`.
- More monster variety for enemies: Quaternius **Monsters** packs, or KayKit
  Halloween (ghosts, spiders). `assets/models/enemies/`.
- After download, ALWAYS extract each GLB's animation list (script in
  docs/art/README.md "Animation indices") and write the indices into the
  `rig_for()` tables — every pack has its own ordering. Verify names like
  Idle/Walk exist; Quaternius uses different clip names than KayKit
  (e.g. "Idle_2", "Walk"). Choose per-file.

## Steps
1. Extend `rig_for()` to cover `EntityKind::Wildlife`: hash tag → species
   model; return that pack's [idle, run, attack] indices. Delete the sphere
   branch and give wildlife the same root/yaw/SceneRoot + `Mover` treatment
   (they wander, so Idle↔Run just works).
2. Server (`world.rs populate_zone`): wildlife/enemy tags are currently
   generic per act. Give each act distinct species tags (eden: deer/boar;
   hermon: wolf/goat; nephilim: hyena/vulture; enoch: rat/dog; flood: crab/
   gull — adjust to the models you actually got) and map them in `rig_for()`.
   Tags are cosmetic to the server, so this is a rename, not a logic change —
   BUT quests reference kill tags (`quests` in world.rs): update quest target
   tags to match, and keep boss `_alpha` suffix convention.
3. Scale table per species (a wolf isn't a bear).

## Verify
`cargo test -p antediluvia-server` (quest tests must still pass — they pin
tags). Run: Eden shows deer-like wildlife wandering with walk anims; travel
to another act → different species. Kill quest still completes (test-client
bot or manual). Screenshots per act.
