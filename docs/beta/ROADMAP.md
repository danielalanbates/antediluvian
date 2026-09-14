# Antediluvia — Beta Roadmap (goal revised 2026-09-13)

**Goal (Daniel):** world-class Rust .app, 1,000 players on screen, walk-through
players, all systems of a classic (pre-2010) MMORPG, beta-test ready, dev menu,
optional PvP + auto-flag near enemy capitals, full locations + quest data,
character builder with Apple login, thousands of terrain models, hundreds of mob
models, hundreds of creation choices. **Keep batesai.org/play always playable;
only push non-breaking updates** (use the gate in
`docs/verification/2026-09-13/README.md`).

Skeptical baseline (screenshot-verified 2026-09-12/13): the server has most
classic *systems* as commands, but the *player experience* is far from a
classic MMO. The gaps below are ordered by how much they block a beta tester
from having a good first hour.

## Status snapshot
| Area | Today | Classic bar |
|---|---|---|
| Classes / abilities | 4 classes x 2 abilities + talents | 8–9 classes, 15–25 abilities, spellbook, rotations |
| Quests | 77 authored, 3 givers per act | thousands; "!"/"?" markers, quest log, map objectives |
| UI windows | unit frame, 3-slot bar, chat, tracker, builder | bags, character sheet, spellbook, quest log, world map, minimap, vendor, loot, AH, guild roster, friends, keybinds |
| Social | guild, party, mail, trade, duel (chat commands) | raid groups, channels, friends/ignore, emotes, who |
| World | 5 acts, 200 POIs, caves, day/night, water | flight paths, rest XP, graveyards/spirit, weather, zone-in banners |
| Items | equip, bank, vendors, craft | bags, rarity colours, durability/repair, loot rolls, set items |
| PvP | opt-in flag, capital auto-flag, honor ticks, duels | battleground instances, honor ranks/rewards |
| Scale | 1000 bots, 150-entity snapshot cap | delta + binary snapshots for hosted 1000-in-zone |
| Art | low-poly CC0 + procedural props/creatures | authored hero art (asset production, not code) |
| Login | local UUID; SIWA helper without entitlement | real Sign in with Apple (needs Daniel's dev-portal step) |
| Web | playable single + live shard, gate-checked deploys | same, auto-updated each safe release |

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
- `batesai.org/membership/` redirects to itself (functions/membership.js).
- Fresh server character showed 64/100 HP despite godmode (unexplained).
- Headless SwiftShader renders characters translucent (artifact only).
- Native 0.5.7 app staged in `dist/`, not installed while Daniel's game runs.
