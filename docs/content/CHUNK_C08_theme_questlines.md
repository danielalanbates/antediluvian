# CHUNK C08 — Cross-zone theme questline: The Forbidden Arts

**Status: todo** (requires C02)

## Goal
Implement the first of the eight "overarching theme" pillars as a long
chained questline that deliberately sends the player across multiple acts —
the docs' answer to WoW's zone-spanning epic chains. The other seven pillars
repeat this recipe in later sessions (one pillar ≈ one session).

## Read first
- `docs/quests/themes/README.md` and `docs/quests/themes/01_the_forbidden_arts.md`.
- The C01 engine (`crates/server/src/quests.rs`).

## Design
1. Pick ~10 representative quests from the pillar's 50 (the docs are a
   design superset; a faithful subset chained end-to-end beats 50 stubs —
   note which were skipped at the top of the Rust module).
2. Each quest's `act` field routes it to a giver NPC in a *different* act
   than its predecessor where the doc says so; turn-in text tells the player
   where to travel next (`Travel` already exists).
3. Add pillar-specific targets (e.g. `azazel_cultist`, `forge_golem`) to
   spawn tables near relevant POIs; the finale is an elite boss
   (`azazel_herald`, alpha-tier stats) with a unique reward item.
4. Tag pillar quests `theme: Option<&'static str>` so the client quest
   tracker can prefix them ("[Forbidden Arts] …").
5. Keep per-quest XP/gold on the existing act-tier curve.

## Verify
- Unit test: chain integrity (each requires the previous; giver acts match
  the intended hop pattern).
- Wire E2E: scripted client runs the first 3 hops (accept in Eden → travel
  → progress → turn-in elsewhere).
- Doc: append a "pillar recipe" note here for the next seven when done.
