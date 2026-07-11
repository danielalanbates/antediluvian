# CHUNK C03 — Bestiary integration (2,500 mobs)

**Status: DONE (2026-07-10)**

## Goal
The five acts spawn mobs drawn from the 2,500-entry bestiary instead of the
handful of hardcoded tags: level-appropriate species per act, with the
bestiary's drops, temperament, and names surfacing in-game.

## Read first
- `docs/mobs/Bestiary_Vol_01.md` — first ~5 entries only (the format:
  ID / Level Range / Habitat / Temperament / Tameable / Drops / Lore).
- `crates/server/src/world.rs` — enemy spawn tables and stat scaling.
- `scripts/` — put the generator here.

## Design
1. **Generator script** (`scripts/gen_mobs.py`): parse all 25 volumes into
   `assets/data/mobs.json`: `{id, name, level_min, level_max, habitat,
   temperament, tameable, drops[]}`. Habitat strings map to acts:
   Edenic Basin/Plains of Nod→Eden, Hermon Range→Hermon, Nephilim
   Wastes→Nephilim, City of Enoch→Enoch, Abyssal Basins→Flood.
   Commit both script and JSON.
2. **Server loads mobs.json at boot** (serde). Each act's spawn table picks
   randomly among that act's habitat entries whose level range fits the act
   tier (Eden 1–12, Hermon 13–24, Nephilim 25–36, Enoch 37–48, Flood 49–60 —
   clamp: many entries are high-level; if an act has <10 eligible species,
   widen the band). Docile species spawn as neutral wildlife (don't aggro);
   Bloodthirsty/Aggressive as enemies.
3. Mob level within its range drives HP/damage/XP via the existing scaling
   formulas; entity `tag` becomes the bestiary name (snake_cased) so quests
   can target species. Keep the legacy tags (serpent, watcher…) spawning too
   so existing quests still work.
4. Drops: on kill, roll 1 of the entry's Common Drops into loot (inventory
   item, snake_cased). These feed C11's economy.
5. **Client**: map species → existing model set by keyword (serpent/wolf/
   cat→fox model, bear/boar→bull, bird→raptor pose of fox, …default
   skeleton). Crude is fine; a later art chunk refines.

## Verify
- Generator: entry count == 2500, spot-check 3 entries against the markdown.
- Unit test: every act resolves ≥10 species; docile species have `aggro ==
  false`; a kill grants a bestiary drop.
- Live: run server + client in Eden, screenshot showing a bestiary-named
  nameplate; kill one and see the drop in inventory via test-client.
