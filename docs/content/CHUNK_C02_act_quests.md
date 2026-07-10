# CHUNK C02 — Full act quest database

**Status: todo** (requires C01)

## Goal
Load all ~45 quests from `docs/quests/act1..act5.md` into the C01 engine,
with their rewards, items, and questgivers.

## Read first
- `crates/server/src/quests.rs` (from C01).
- All five `docs/quests/act*_*.md` files (~43 lines each — small).

## Design
1. Transcribe every Main Storyline + Side quest into `QuestDef` entries.
   Main quests chain in order (`requires` previous); side quests are free.
2. New questgiver NPC tags as the docs name them (Elder Adamah → existing
   `elder`; Sethite Wanderer, Evie, Abel's Echo, Healer Mahalalel, Noah…) —
   map each to one of ~3 spawned NPC entities per act (elder at inn,
   wanderer at a mid-zone POI, specialist near the act boss area). Multiple
   doc-givers may share one NPC; note the mapping in a comment.
3. New reward items (Spark-Woven Cloak, Bronze Bracers, Bitter Bread…):
   add to the item/equipment tables with sensible slots/stats consistent
   with existing stone_axe/bronze_sword scaling; consumables heal like bread.
4. Collect targets that aren't mob drops (Briar-Fruit, Cherubim Sparks):
   make them drop from harvesting specific resource nodes in that act, or
   from a themed mob if no node fits. Note choices in comments.
5. Quest target mob tags that don't exist yet (cainite, boar, elemental…):
   add them to the act spawn tables as reskins of existing stat blocks
   (proper bestiary models come in C03).

## Verify
- Unit test: iterate all QuestDefs — every `requires` id exists, every act
  has ≥1 starter quest, every reward item resolves in the item table, every
  kill/collect target has a spawner or drop source.
- Play test: on a throwaway DB, walk one act's main chain end-to-end via the
  test-client (accept → objective → turn-in ×3).
- Update the quest count in `PROJECT.md`.
