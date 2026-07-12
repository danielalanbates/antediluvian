# CHUNK C10 — Factions & reputation

**Status: DONE (2026-07-11)** — lineage choice @10 (switch wipes rep), rival-kill/boss/quest rep, WoW-style rungs, Quartermaster vendor + Talk listing, faction quest variants, HUD lineage tag, revered mantle tint; unit test + wire E2E (FACTION-E2E-OK) + on-screen HUD check.

## Goal
Players align with the Sethites or the Cainites (the docs' central mortal
conflict) and grind reputation that gates vendors and quest variants —
WoW-style rep, antediluvian flavor.

## Read first
- `docs/lore/entities/factions.md` (Sethite vs Cainite section).
- `docs/quests/mount_questline.md` Part 2 (the two-path pattern).

## Design
1. Sheet gains `faction: Option<Faction>` + `reputation: BTreeMap<String,i32>`
   (SheetExt three-place rule). A one-time choice quest at level 10
   ("The Two Lineages begin") from the Elder sets it; changing costs all rep.
2. Rep sources: faction-tagged quests (+250), killing the rival faction's
   mob tags (+5, e.g. `cainite_scavenger` for Sethites), act bosses (+100).
   Standard WoW-ish rungs: Neutral 0 / Friendly 3k / Honored 9k / Revered 21k.
3. Each act inn gains a faction Quartermaster NPC selling rep-gated gear
   (Friendly: consumables; Honored: a weapon sidegrade; Revered: a unique
   cosmetic tint the client renders as a tinted material on the rig).
4. Where C02 quests came in Cainite/Sethite variants, gate each variant on
   faction; rewards equal, lore differs.
5. Client: unit-frame shows faction tag; vendor interaction reuses the Talk
   flow (Talk to Quartermaster lists wares; `Buy {item}` message).

## Verify
- Unit tests: choice is one-shot, rep accrues from kills/quests, vendor
  refuses under-rep purchases, persistence round-trip.
- Wire E2E: pick faction, grind to Friendly on a boosted rate (test-only
  multiplier env), buy an item.
