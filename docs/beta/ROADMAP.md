# Antediluvia — Beta Roadmap (goal revised 2026-09-13)

**Goal (Daniel):** world-class Rust .app, 1,000 players on screen, walk-through
players, all systems of a classic (pre-2010) MMORPG, beta-test ready, dev menu,
optional PvP + auto-flag near enemy capitals, full locations + quest data,
character builder with Apple login, thousands of terrain models, hundreds of mob
models, hundreds of creation choices. **Keep batesai.org/antediluvia always playable;
only push non-breaking updates** (use the gate in
`docs/verification/2026-09-13/README.md`).

Skeptical baseline (screenshot-verified 2026-09-12/13): the server has most
classic *systems* as commands, but the *player experience* is far from a
classic MMO. The gaps below are ordered by how much they block a beta tester
from having a good first hour.

## Status snapshot
| Area | Today | Classic bar |
|---|---|---|
| Classes / abilities | 8 classes x 8 abilities on a 10-slot bar, stamina/mana, cast bar, cooldowns + talents | 8–9 classes, 15–25 abilities, spellbook, rotations |
| Quests | 73 authored + 950 generated board quests (chat commands only) | thousands; "!"/"?" markers, quest log, map objectives |
| UI windows | unit frame, 10-slot bar, cast bar, chat tabs, tracker, builder, escape menu + dev panel, bags, character, quest log, spellbook, map, minimap, vendor, loot record, board, flight, social (not draggable; mouse paths untested) | bags, character sheet, spellbook, quest log, world map, minimap, vendor, loot, AH, guild roster, friends, keybinds |
| Social | guild, party, mail, trade, duel, raids (40), channels, friends/ignore, emotes, who (chat commands) | raid groups, channels, friends/ignore, emotes, who |
| World | 5 acts, 200 POIs, caves, day/night, water, flight masters, offline rest XP, graveyards, zone banners | flight paths, rest XP, graveyards/spirit, weather, zone-in banners |
| Items | equip, bank, vendors, craft, rarity tiers, durability + repair (no windows) | bags, rarity colours, durability/repair, loot rolls, set items |
| PvP | opt-in flag, capital auto-flag, honor ticks, duels | battleground instances, honor ranks/rewards |
| Scale | 1000 bots, 150-entity snapshot cap | delta + binary snapshots for hosted 1000-in-zone |
| Art | low-poly CC0 + procedural props/creatures; test-counted variety: 1473 mob silhouettes over mobs.json, 6956 distinct terrain/prop meshes over 5 acts | authored hero art (asset production, not code) |
| Login | local UUID; SIWA helper without entitlement; builder with 207 look options, live preview, name rules | real Sign in with Apple (owner steps: `docs/beta/APPLE_SIGN_IN.md`) |
| Web | playable single + live shard, gate-checked deploys | same, auto-updated each safe release |

## Integration status (2026-11-23, branch `beta-b01-b09`, not pushed)
`beta-b01-b09` = class kits + `origin/main` e192420 + `beta-server-track`,
`PROTOCOL_VERSION` 16. Sim 127 / server 14 / client 40 tests pass; client and
test-client build. Details, shots and the unverified list:
`docs/verification/2026-11-23/README.md`; server-side rules:
`docs/beta/SERVER_TRACK_2026-11-23.md`.

| Chunk | State |
|---|---|
| B01 UI windows I | done: bags, character, quest log, spellbook (`win/`); shot-verified opened by script, mouse and hotkeys untested |
| B02 Quest markers | done: "!" over givers and boards seen in shots; "?" and grey states unit-tested only |
| B03 Ability kits | done: bar, keys 1-0, cooldown countdown, cast bar, resource bar. Shot-verified for Occultist only; 56 of 64 abilities have no icon |
| B04 Map + minimap | done: terrain picture, POIs, graveyards, flight masters, capitals, arrow, quest pins; Eden seen, other acts not |
| B05 Quest scale | server done; client board window: list and Accept seen working (networked and solo); Turn in / Abandon buttons unpressed |
| B06 Rarity, durability, repair | server done; client: rarity colours, wear, vendor window seen; buy / sell / repair buttons unpressed; loot window seen empty only |
| B07 Social | server done; client: social window (Who seen), chat tabs (General round trip seen); other tabs unseen with data |
| B08 Travel | server done; client: zone banner, flight window, rested stretch, spirit-release prompt (solo) seen; Fly and Release unpressed |
| B09 Classes 5-8 | done in sim and builder; they reuse the four old rigs |
| Variety | done: character looks (protocol, db, server clamp, rigs, LOD, builder), creature grafts, 12 landform families; counts and gaps in the verification README. Builder mouse/keyboard untested |
| Review 2026-11-23 | merge and client reviewed, 10 defects fixed with tests; see `docs/beta/HANDOFF_2026-11-23.md` (sim 132 / server 16 / client 130 tests) |
| B10-B12 | not started |

## Chunks (do in order; each ships through the web gate + app staging)
- **B01 UI windows I:** bags (B), character sheet/paperdoll (C), quest log (L),
  spellbook (P) — read-only views over data the client already receives
  (`Stats`, sheet, quests). Screenshot each.
- **B02 Quest readability:** "!" / "?" markers over givers (client has the sim
  quest table; server Stats already carry quest state), gold names done.
- **B03 Ability kits:** 8 abilities per class on a 10-slot bar (keys 1–0),
  cooldown sweep, resource costs; server-side defs in sim.
- **B04 World map + minimap:** top-down render of terrain height + POIs,
  player arrow, quest objective pins.
- **B05 Quest scale:** POI-anchored quest generator — one kill/collect/explore
  quest per POI (200) from local bestiary species + POI lore, level-banded,
  gated behind each act's main chain (unchained quests hijack `talk()` order —
  see C16 notes).
- **B06 Vendors + loot windows, item rarity colours, durability + repair.**
- **B07 Social:** raid groups (40), chat channels (General/Trade/LFG),
  friends/ignore, emotes, /who.
- **B08 Travel:** flight masters between act inns and POIs, rested XP at inns.
- **B09 Classes 5–8** (e.g. rogue, paladin-analogue, druid-analogue, warlock-
  analogue fitting the antediluvian lore).
- **B10 Battlegrounds:** instanced capture-the-relic 10v10 with queue.
- **B11 Scale:** delta snapshots + bincode on the wire; re-run 1000-bot swarm.
- **B12 Hosted shard redeploy** with current server (live shard is behind).

## Known defects (open)
- Client aborted at startup with a worker-thread stack overflow (nested glTF
  loads); worked around with a 16 MB thread stack (2026-11-23).
- Cooldown sweep is low-contrast. (Stamina bar now seen.)
- Window buttons, right-click, tooltips and hotkeys have never been driven by a real mouse or keyboard (see verification README, 2026-11-23 later section).
- No aura list on the wire: enemy debuffs cannot be shown.
- XP curve overflows near level 55 (see server track notes).
- Sim test `questgen::tests::cave_ore_collect_and_dweller_quests_complete` was flaky (any ore node from a hash map); it now takes the node nearest the cave and passed 12 of 12 runs. Why other nodes yielded a different count is not root-caused.
- `batesai.org/membership/` redirects to itself (functions/membership.js).
- Fresh server character showed 64/100 HP despite godmode (unexplained).
- Headless SwiftShader renders characters translucent (artifact only).
- Native 0.5.7 app staged in `dist/`, not installed while Daniel's game runs.
