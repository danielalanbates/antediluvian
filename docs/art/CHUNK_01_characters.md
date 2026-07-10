# CHUNK 01 — Rigged, animated characters

**Status: DONE 2026-07-09** (commit "Art pass 1: rigged KayKit characters…").
Kept for reference — this documents what exists so later chunks can build on it.

## What was delivered
- `assets/models/characters/`: KayKit Adventurers 1.0 GLBs (CC0) —
  Knight, Mage, Rogue, Rogue_Hooded, Barbarian.
- `assets/models/enemies/`: KayKit Skeletons 1.0 GLBs (CC0) —
  Skeleton_Minion/Rogue/Mage/Warrior.
- Class→model map + per-class attack clip in `rig_for()`
  (`crates/client-bevy/src/main.rs`): melee classes swing, casters
  Spellcast_Shoot.
- Animation pipeline: `RigClips` → `attach_rigs` → `AnimationGraph` +
  `AnimationTransitions`; `animate_movement` crossfades Idle/Run (150 ms) from
  actual root movement; `trigger_attack_anim` plays a local one-shot
  (`attack_until` guard, 0.9 s).
- Server: player entities carry class in `tag` (spawn + select_class), so
  every client renders every player correctly.
- Class change respawns the local rig (Stats handler despawn → next snapshot
  respawns with new model).

## Verified
Screenshots confirmed: Knight idle + hooded Elder NPC at the inn; F4 →
"You are now a mage" + Mage model; W-movement; attack pose change on Space;
no panics. Anim indices verified by parsing GLB JSON chunks.

## Known gaps (feed later chunks)
- Remote players/enemies never show attack/hit/death anims (server doesn't
  event them) → CHUNK_02.
- Wildlife is still a sphere → CHUNK_05.
- Dead enemies just vanish (despawn on AoI drop) — death anim needs
  CHUNK_02's events.
- Equipment (bronze_sword etc.) not visible on the model → CHUNK_09.
