# CHUNK C16 — WoW-Classic parity audit + world-PvP zones

**Status: DONE (2026-07-11)** — see audit + zone system below. Verified: 38 unit tests incl. own/enemy-capital flag + linger-drop; wire E2E (force-flag banner + cross-faction melee damage at the Cainite capital, PVPZONE-E2E-OK).

## Parity audit (alpha depth, 2026-07-11)
HAVE: quests (per-act + 8 theme pillars + faction variants), classes/abilities,
talents, professions/crafting, itemization+equipment, bosses, mounts+taming,
stable, rested XP, POIs/caves/discovery, factions+reputation+vendors, economy
(prices, vendor buy/sell, seeded AH w/ cut), guilds+guild chat, duels, world
PvP (manual flag + capital zones), hearthstone (NEW this chunk: every new
character starts with one; 30-min cooldown teleport to the inn), day/night,
audio, char builder + Apple-ID account keying, dev menu, 1000-player scale.
GAPS (post-alpha, deliberate): player-to-player trade window, bank slots,
mail, group/party + shared kill credit, dungeons/raids, battlegrounds,
durability/repair (design choice: repair-free), talent respec cost.

## Goal
Close the gap to "all the systems of WoW Classic" at alpha depth, and make
world PvP zone-driven: PvP stays opt-in via menu, but designated zones —
especially near an enemy faction's capital city — flag you automatically.

## Design
1. **Parity audit first**: one pass listing WoW-Classic systems vs ours
   (have: quests, classes, talents, professions/crafting, AH, guilds, duels,
   mounts, rested XP, PvP flag, POIs). Likely gaps: player trade, bank slots,
   mail, group/party + shared quest credit, vendors/repair, hearthstone.
   Write the audit into this file, then implement the alpha-worthy gaps.
2. **Zones**: each act gets faction capitals (C10) with a surrounding
   `pvp_zone` radius. Entering enemy-capital territory force-flags you
   (banner warning at the border, like WoW contested→enemy zones); leaving
   drops the forced flag after the usual PvP cooldown. Manual toggle keeps
   working everywhere else.
3. Zone PvP rules live in data (`assets/data/` alongside POIs), not code.

## Verify
- Unit: unflagged player entering an enemy-capital zone becomes attackable;
  same-faction capital does not flag; flag drops on leaving after cooldown.
- Live: two clients, opposite factions, fight at a capital border.
