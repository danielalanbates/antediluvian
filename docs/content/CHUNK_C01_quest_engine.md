# CHUNK C01 — Data-driven quest engine

**Status: todo**

## Goal
Replace the one-hardcoded-quest-per-act system with an engine that supports
the full quest database: multiple quests per act, multiple questgiver NPCs,
several concurrent quests per player, kill AND collect objectives, and quest
chains (quest B requires quest A done).

## Read first
- `crates/server/src/world.rs` — `Quest` struct (~line 143), `QUESTS` array,
  `quest_for_act`, the `Talk` handler (~line 1054), kill-credit site (~line 832).
- `docs/quests/act1_eden.md` only (as the shape reference — 6 quests: kill,
  collect-from-kill ("loot 6 ingots"), and gather objectives).

## Design
1. `QuestDef` replaces `Quest`: add `giver: &'static str` (NPC tag, e.g.
   `elder`, `wanderer`), `objective: Objective` enum
   (`Kill{target,count}` | `Collect{item,count,source}`), `requires:
   Option<&'static str>` (prerequisite quest id), `side: bool`.
2. Keep quests as `&'static [QuestDef]` in a new `crates/server/src/quests.rs`
   — data-driven-in-source is fine at this scale (~45 quests); C02 fills it.
   For this chunk migrate only the existing 5 quests + add 2 of the Eden
   quests from the doc (one collect, one chained) to prove the engine.
3. `sheet.quests` (BTreeMap<String,u32>) already holds progress; keep it.
   Multiple concurrent quests = multiple map entries — the cap is 10.
4. `Talk` targets the *nearest* NPC within range; each NPC offers its own
   list: not-yet-started (requirements met) → accept first; in-progress
   complete → turn in; else progress report.
5. Spawn one extra questgiver NPC per act (`wanderer` tag) at a fixed offset
   from the inn. Client already models NPCs; the tag decides the model later.
6. Collect objectives: killing the source mob rolls the quest item into
   inventory (100% while quest active); turn-in removes them.

## Verify
- Unit tests: accept→kill→turn-in for a kill quest; collect quest consumes
  items on turn-in; chained quest refuses until prerequisite done; two quests
  progress concurrently. All existing 13 tests still green.
- Wire E2E: test-client accepts and completes a quest against a live server.

## Out of scope
Full quest content (C02), theme questlines (C08), quest-log UI polish
(client already lists `sheet.quests` in the tracker).
