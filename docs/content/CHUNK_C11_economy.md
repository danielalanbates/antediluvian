# CHUNK C11 — Loot economy & auction house life

**Status: DONE (2026-07-11)** — prices.json from drop rarity+act tier (gen_prices.py), Innkeeper buy-at-80%/staples-at-120% + Talk listing, AH boot+daily seeding (King List NPCs, idempotent top-up to 5), 5% AH cut, gold minted/sunk hourly log; 4 unit tests + wire E2E (ECON-E2E-OK, exact balances) + restart no-flood check.

## Goal
Make the economy breathe: bestiary drops have vendor value, inns buy/sell,
and the auction house starts seeded so a lone player still sees a market.

## Read first
- `crates/server/src/world.rs` — auction handlers, gold flow.
- `assets/data/mobs.json` drops (C03).

## Design
1. **Price table**: derive a vendor price per item from rarity of its
   sources (count of mobs dropping it across the bestiary — rarer = pricier)
   plus act tier. Generate `assets/data/prices.json` in the C03 script or a
   sibling; hand-tune the handful of crafted/quest items on top.
2. **Vendor**: innkeeper NPC accepts `Sell {item}` (80% of table price) and
   sells staples (bread, lasso materials, low-tier gear at 120%).
3. **AH seeding**: at boot, if an act's AH has <5 listings, list 5 random
   act-appropriate items at 90–140% price under NPC seller names (Sumerian
   King List names: Alulim, Alalngar, Enmeduranki…). Their proceeds vanish
   (gold sink). Re-seed daily (server uptime timer).
4. **Gold sinks**: repair-free game, so sinks are vendor staples, C06/C07
   taming materials, and a 5% AH cut (add if missing).
5. Log per-act gold-in/gold-out counters hourly (server log) so future
   tuning has data.

## Verify
- Unit tests: sell price math, seeding idempotence (never floods), AH cut.
- Wire E2E: sell a bestiary drop, buy a seeded listing, balance changes
  match the table.
