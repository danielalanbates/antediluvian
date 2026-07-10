# CHUNK 06 — Spell & combat VFX

**Status: todo** (do after CHUNK_02 — VFX keys off its combat events)

## Goal
Casts, hits, and level-ups read visually: firebolts fly, heals sparkle,
melee hits flash.

## Tools
- `bevy_hanabi` (Apache/MIT, GPU particles) — check crates.io for the version
  matching Bevy 0.15 (0.14.x line at time of writing; verify with
  `cargo add bevy_hanabi --dry-run`). If its Bevy-version lag blocks you,
  fall back to hand-rolled particles: spawn 10-30 tiny unlit quads with
  velocity + shrink + despawn timer in a `vfx.rs` system — at this art style
  that looks fine and adds zero deps. **Prefer the fallback if bevy_hanabi
  fights you for more than ~20 min of session time.**

## Steps
1. `vfx.rs`: `spawn_burst(commands, pos, color, n, speed, life)` +
   `update_vfx` system (move, fade via material alpha or scale, despawn).
   Unlit `StandardMaterial`, additive-ish colors, `AlphaMode::Blend`.
2. Hook `ServerMsg::Event` (CHUNK_02):
   - `Cast` by mage → orange burst at src; priest → soft yellow.
   - Projectile feel: lerp a glowing sphere from src to dst over ~0.25 s
     (spawn with `Projectile { from, to, t }` component + system), burst on
     arrival.
   - `Hit` → small red flash at dst; `Die` → gray puff.
   - Level-up (`Notice` contains "level" today — better: add
     `EventKind::LevelUp` while in there) → gold column burst on self.
3. Inn ring pulse: slow sin-alpha on the ring material so the rest area
   feels alive (one system, 5 lines).

## Verify
Two clients; cast firebolt from one, observe projectile + burst from the
other. Kill an enemy → death puff. Screenshot mid-effect. Frame rate stays
smooth with ~10 simultaneous bursts (spam space near mobs).
