# Antediluvia Art Pass — AI Work Manual

**Goal (set by Daniel 2026-07-09):** bring Antediluvia to *WoW-Classic-grade*
presentation: modeled/animated characters and creatures, real terrain, spell
VFX, a proper UI, atmosphere. Built entirely from **free (CC0/MIT) assets and
crates**, integrated into the existing Bevy 0.15 client.

This directory is a **work queue for AI sessions**. Each `CHUNK_NN_*.md` is
sized to fit one focused AI session (read → implement → verify → commit).
Do them in order unless a chunk says otherwise; later chunks assume earlier
ones landed.

## How to work a chunk (protocol — follow exactly)
1. Read `PROJECT.md`, this README, and the chunk file. Nothing else is needed.
2. `export PATH="$HOME/.rustup/toolchains/stable-aarch64-apple-darwin/bin:$PATH"`
   and build with `cargo build -p antediluvia-client-bevy` (server:
   `-p antediluvia-server`). First Bevy build is heavy (~5 min); incremental
   builds ~30 s. **This is an 8 GB M1 — never run two builds or a build plus
   an ML job concurrently.**
3. Implement the chunk. Small, surgical diffs; the client stays a *thin*
   client (no game logic — server is authoritative).
4. **Verify with your eyes**, not just the compiler: run
   `./target/debug/antediluvia-server` (env `ANTEDILUVIA_DB=/tmp/x.sqlite`
   for a throwaway world), then `./target/debug/antediluvia-client-bevy Adam`,
   wait ~12 s, and screenshot the window:
   `screencapture -x -l <windowid> /tmp/shot.png` (get the id from
   `Quartz.CGWindowListCopyWindowInfo`, kCGWindowName == "Antediluvia").
   Drive input with `osascript`/Quartz key events (F4 = pick mage,
   key code 13 = W, 49 = space). Read the PNG and confirm the chunk's
   "Verify" checklist. Check `/tmp` logs for panics.
5. Kill the server/client you spawned. Update the chunk file's **Status**
   line and `PROJECT.md`'s status section. Commit with a descriptive message.

## Current state (after CHUNK_01, 2026-07-09)
- Players/NPCs render as KayKit **Adventurers** rigged GLBs
  (`assets/models/characters/`): warrior→Barbarian, hunter→Rogue,
  priest→Knight, mage→Mage, classless→Knight, NPC→Rogue_Hooded.
- Enemies render as KayKit **Skeletons** (`assets/models/enemies/`); bosses
  (`*_alpha` tag) are Skeleton_Warrior at 1.5× scale; species hash picks
  Minion/Rogue/Mage otherwise.
- Animation state machine in `crates/client-bevy/src/main.rs`: `RigClips`
  (clip handles, set at spawn) → `attach_rigs` (builds `AnimationGraph` when
  the scene's `AnimationPlayer` appears) → `animate_movement`
  (Idle↔Running_A crossfade from server-authoritative movement) +
  `trigger_attack_anim` (local one-shot on Space/1/2).
- The server now sends a player's **class as the entity `tag`** so remote
  clients pick the correct model (`world.rs: spawn_player`, `select_class`).

## Conventions & gotchas (learned the hard way — reread before coding)
- **Asset root** is the workspace `assets/` dir, wired via `AssetPlugin`
  `file_path` from `CARGO_MANIFEST_DIR/../../assets`. Paths in code are
  relative to it, e.g. `models/characters/Knight.glb`.
- **Animation indices, not names.** Clips load as
  `GltfAssetLabel::Animation(idx).from_asset(path)`. Verified indices —
  Adventurers (all 5 files identical): Idle=36, Running_A=48, 1H_slice=1,
  2H_chop=8, Spellcast_Shoot=62, Death_A=23.
  Skeletons (all 4 identical): Idle=40, Running_A=54, 1H_slice=2, 2H_chop=9,
  Spellcast_Shoot=77, Death_A=24. To re-derive for a new GLB, parse the glb
  JSON chunk (`struct.unpack('<I', data[12:16])` = JSON length, animations
  array order = index).
- **Rig orientation:** KayKit glTF rigs face **+Z**; the server's facing
  convention is **+X**. The SceneRoot child carries a baked
  `Quat::from_rotation_y(FRAC_PI_2)`; the parent "yaw node" gets
  `Quat::from_rotation_y(-e.rot)` from snapshots. Scale: `CHAR_SCALE = 30.0`
  (~1.8-unit rigs → ~55 world units tall), `ALPHA_SCALE = 45.0`.
- **Scene hierarchy per character:** root (translation only, `ServerEnt`,
  `Mover`) → yaw node (rotation) → SceneRoot (`RigClips`, scale+quarter-turn)
  → …glTF nodes… → the entity with `AnimationPlayer` (found by walking
  `Parent` links upward). Health-bar `Billboard` is a root child so it never
  inherits yaw.
- **Bevy 0.15 query rule:** `Added<AnimationPlayer>` + a second
  `Query<&mut AnimationPlayer>` in the same system = B0001 panic at runtime
  (not compile time). Merge into one query. **Runtime panics like this only
  surface when you actually run the client — always do the run+screenshot
  step.**
- Where to find more free assets (all CC0): github.com/KayKit-Game-Assets
  (dungeon/medieval/halloween packs, same rig), quaternius.com (animals,
  monsters, modular fantasy characters + Universal Animation Library),
  kenney.nl (props, UI), opengameart.org. Prefer GLB with embedded textures.
- Licenses: keep `docs/art/LICENSES.md` updated when adding asset packs.

## Chunk index
| # | File | What | Status |
|---|------|------|--------|
| 01 | CHUNK_01_characters.md | Rigged animated characters (players/NPC/enemies) | **DONE 2026-07-09** |
| 02 | CHUNK_02_combat_events.md | Server combat events → remote attack/hit/death anims | **DONE 2026-07-10** |
| 03 | CHUNK_03_terrain.md | Per-act heightmap terrain, ground texture, water | **DONE 2026-07-10** |
| 04 | CHUNK_04_environment.md | Real trees/rocks, inn building, per-act prop sets | **DONE 2026-07-10** |
| 05 | CHUNK_05_wildlife.md | Animated wildlife + per-act enemy species models | **DONE 2026-07-10** (enemy variety deferred) |
| 06 | CHUNK_06_vfx.md | Spell/hit particles (hand-rolled), cast beams | **DONE 2026-07-10** |
| 07 | CHUNK_07_ui.md | WoW-style UI: unit frames, action bar, quest tracker | **DONE 2026-07-10** |
| 08 | CHUNK_08_atmosphere.md | Sky, fog, day/night, per-act lighting mood | **DONE 2026-07-10** |
| 09 | CHUNK_09_equipment.md | Visible weapons/armor from equipment slots (bone sockets) | todo |
| 10 | CHUNK_10_performance.md | Preload, LOD/AoI tuning, shadow budget, 60 fps target | todo |
