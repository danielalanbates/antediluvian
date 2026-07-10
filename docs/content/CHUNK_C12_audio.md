# CHUNK C12 — Audio: combat, ambient, UI

**Status: todo** (independent — good filler session)

## Goal
The game stops being silent: melee hits, spell casts, level-up chime, UI
clicks, and a looping ambient bed per act. All CC0.

## Read first
- `crates/client-bevy/src/main.rs` — `apply_combat_events` (the VFX hook
  sites are exactly where sounds attach) and the act/travel handling.
- `docs/art/LICENSES.md` — add sources.

## Design
1. Sources: Kenney audio packs (kenney.nl — impact/interface/rpg packs,
   CC0) and freesound CC0 loops for ambience (wind, forest birds, city
   murmur, storm). Keep total assets < 15 MB; OGG format.
2. `assets/audio/{sfx,ambient}/…`; a small `AudioAssets` resource loaded at
   startup (mirror `VfxAssets`).
3. Hook `bevy_audio` one-shots on `EventKind::{Attack,Cast,Hit,Die,LevelUp}`
   with slight random pitch (0.9–1.1) to avoid machine-gun repetition;
   distance-attenuate by burst position vs camera (simple volume falloff —
   full spatial audio not required).
4. Ambient: one looping track per act, crossfaded on travel (two
   `AudioPlayer` entities, fade over ~2 s). Flood act layers rain.
   Night (from the day/night cycle) drops ambient volume ~30%.
5. UI: click on action-bar keypress; discovery/level-up flourish.
6. Respect an `ANTEDILUVIA_MUTE=1` env for CI/screenshot runs.

## Verify
Compiler can't hear. Run the client, attack/cast/level with speakers on and
confirm by ear; check no audio spam at 20 Hz snapshot rate (events only);
LICENSES.md updated; kill spawned processes.
