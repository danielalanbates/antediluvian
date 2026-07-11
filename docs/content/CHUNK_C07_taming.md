# CHUNK C07 — Creature Mastery taming sandbox

**Status: done** (requires C03 + C06)

## Goal
Any bestiary mob flagged `Tameable: Yes` can be tamed with the
weaken→subdue loop from the design doc and ridden or stabled: the sandbox
mount system.

## Read first
- `docs/quests/dynamic_mounting_system.md` (32 lines).
- `assets/data/mobs.json` (from C03) — `tameable` flag.
- C06's mount plumbing.

## Design
1. Craftable `taming_lasso` (leatherworking-ish: hides + wood via the
   existing Craft system).
2. `ClientMsg::Tame { target }`: valid when target is tameable, below 30%
   HP, player has a lasso (consumed). Success chance = 40% + 10% per act
   tier the player is above the mob; failure enrages (mob heals 50%, +damage
   briefly). Success despawns the mob and adds `mount:<species>` item.
3. Stable: at any inn, `Stable`/`Unstable` messages move mount items
   between inventory and a `stable: Vec<String>` sheet field (SheetExt —
   three-place rule). Active mount = whichever mount item is "equipped" in
   a new `mount` equipment slot; C06's Mount toggle uses it.
4. Species speed/utility tiers (doc): wolves/cats ×1.7, oxen/brutes ×1.35
   but +8 inventory cap, bears ×1.5 with no dismount-on-first-hit.
   Derive tier from species name keywords; keep the table in one place.
5. Client renders the active mount species' closest model (C03 keyword map).

## Verify
- Unit tests: tame gates (HP%, lasso, tameable flag), enrage on fail,
  stable round-trip persists.
- Live: tame a docile wildlife mob, ride it, screenshot; relog and confirm
  the stable survived.
