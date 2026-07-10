# CHUNK C06 — Mounts: Dire-Wolf questline + riding

**Status: todo** (requires C01; C05 recommended)

## Goal
Level-40 players complete the "Taming the Antediluvian Wild" questline and
earn a rideable Dire-Wolf: +60% move speed, visible wolf model under the
character, dismount on entering combat.

## Read first
- `docs/quests/mount_questline.md` (42 lines — the 3-part chain, Cainite
  chain vs Sethite bridle variants).
- `crates/server/src/world.rs` — movement speed application.

## Design
1. Server: `Mount` ability — new `ClientMsg::Mount` toggles `mounted` when
   the sheet has item `dire_wolf_whistle`; refuse in combat; speed ×1.6
   while mounted; any damage taken or dealt dismounts. `mounted` rides the
   snapshot `EntityState` (bump proto).
2. Questline via the C01 engine: Part 1 kill 15 dire-wolves (spawn
   `dire_wolf` in the level-40 act), Part 2 collect quest (two doc variants
   collapse to one collect quest whose reward differs by talent branch or a
   simple choice — keep it simple: reward both flavors' lore text, one item),
   Part 3 turn-in grants `dire_wolf_whistle`.
3. Client: when `mounted`, parent the character rig to a wolf model
   (Quaternius animal set already shipped — reuse fox/shiba scaled up, or
   pull a CC0 wolf; update LICENSES.md), play its run clip, raise rider Y.
4. Persist `mounted=false` on login (never save mid-mount weirdness).

## Verify
- Unit tests: mount refused without whistle / in combat; speed multiplier
  applies server-side; damage dismounts.
- Live: complete the chain with a dev-leveled character (add a test-only
  `ANTEDILUVIA_DEV_LEVEL` env or SQL update), screenshot mounted riding.
