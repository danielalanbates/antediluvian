# PROJECT — Antediluvia

_Completes the `ARCHITECT_TODO.md` mapping task, applied to the Rust rewrite._

## What this is
**Antediluvia** is an original **MMORPG** set in the antediluvian (pre-Flood)
age, told across five acts: **Eden → Hermon → Nephilim → Enoch → Flood**. Each
act is a persistent, continuously-simulated zone.

The game began as a Rust/Bevy single-player prototype, briefly detoured through a
TypeScript (React + three.js) client with a TypeScript server, and as of
2026-07-09 is being built **as a full Rust project**: a headless, authoritative
server plus a Rust client. The TypeScript detour is archived under
`archive_typescript_2026-07-09/` (nothing deleted).

## Engine / stack
- **Language:** Rust (Cargo workspace, edition 2021).
- **Server:** `tokio` async runtime, `tokio-tungstenite` WebSocket transport,
  `serde_json` wire format, `rusqlite` (bundled SQLite) persistence,
  `glam` math. No external RNG — a deterministic xorshift in the sim.
- **Client (graphical):** `bevy` 0.15 (2D), `crates/client-bevy`. A *thin*
  networked client — it holds no game logic; it connects over WebSocket, sends
  input intents, and renders the server's snapshots. A background thread runs a
  tokio runtime and bridges to Bevy via tokio mpsc channels.
- **Test client:** a headless `tokio` WebSocket bot (`crates/test-client`).

## Workspace layout
| Crate | Role | In default build? |
|-------|------|-------------------|
| `crates/protocol` | Shared wire types (`ClientMsg`/`ServerMsg`, `Act`, `EntityState`, `CharacterSheet`) | yes |
| `crates/server` | Authoritative headless server: WS acceptor, per-connection tasks, single 20 Hz game loop owning the whole `World`, SQLite persistence | yes |
| `crates/test-client` | Headless bot that logs in and hunts enemies (E2E proof) | yes |
| `crates/client-bevy` | Bevy 2D graphical client (revived original prototype) | **no** — heavy Bevy compile; build explicitly |

## How to run
```sh
# Toolchain (rustup via Homebrew): cargo/rustc live at
#   ~/.rustup/toolchains/stable-aarch64-apple-darwin/bin
export PATH="$HOME/.rustup/toolchains/stable-aarch64-apple-darwin/bin:$PATH"

# Server + two headless players (authoritative loop, verified E2E)
cargo run -p antediluvia-server                 # ws://127.0.0.1:8787
cargo run -p antediluvia-client -- Adam
cargo run -p antediluvia-client -- Eve

# Graphical Bevy client (heavy first build)
cargo run -p antediluvia-client-bevy
```
Env: `ANTEDILUVIA_BIND` (default `127.0.0.1:8787`), `ANTEDILUVIA_DB`
(default `antediluvia.sqlite`).

## Goal (set by Daniel 2026-07-09)
A Rust-based competitor to **World of Warcraft Classic** — full classic-style
graphics and systems: 3D world/characters, combat, class skills, PvP, talent
trees, guilds, professions, auction houses, inns. Build order: (1) server
systems ✅ (below), (2) Bevy 3D client migration, (3) content (quests, dungeons,
itemization across the five acts).

## Status (2026-07-09 late) — WoW systems layer live (proto v2)

All server-authoritative; covered by 9 unit tests + 22 wire-level E2E checks
(two live WebSocket clients):
- **Classes:** warrior / hunter / priest / mage (`SelectClass`, once per
  character; each applies a base-stat kit).
- **Abilities:** 2 per class (`Cast`) — mana costs, per-ability cooldowns, 1s
  global cooldown; single-target, AoE, and self-heal effects; damage scales
  with talents and crafted gear (stone_axe/oak_staff).
- **Talents:** 1 point per level-up; 3 branches per class (`<class>_power/_toughness/_spirit`),
  5 ranks each (`LearnTalent`).
- **PvP:** opt-in world-PvP flag (`TogglePvp`, both sides must be flagged) and
  duels (`Duel`/`DuelAccept`; loser ends at 1 HP, never dies).
- **Guilds:** create/invite/accept/leave, persisted roster, cross-zone `[G]` chat.
- **Professions:** woodcutting/mining skill-ups from harvesting; crafting with
  material + skill requirements (`Craft`); consumables (`UseItem` — bread heals).
- **Economy:** gold from kills (scales by act tier); **auction house** usable at
  any inn (`AuctionList`/`AuctionBrowse`/`AuctionBuy`) — listings persisted,
  offline sellers credited on their saved sheet, double-buy rejected.
- **Inns:** the `INN_RADIUS` around each zone entry banks rested XP (capped at
  2000), which doubles kill XP until the bank is spent.
- **Persistence:** new sheet fields ride a JSON `ext` column (v1 DBs
  auto-migrate); guilds + auctions get their own tables.

## Status (2026-07-09) — playable networked MMORPG

**Done & verified:**
- **Authoritative server:** name-based auth + auto-register, SQLite persistence,
  five simulated zones, enemy/wildlife AI, melee combat, XP/leveling,
  death/respawn, zone travel (`Travel`), zone chat. (E2E: two concurrent bots —
  movement, combat both directions, kill attribution, loot, save on disconnect.)
- **Networked Bevy client:** connects, logs in, renders server snapshots, sends
  input, camera-follows the player. (E2E: client connected + server logged the
  login + ran with no panic; character persisted.)
- **Area-of-interest snapshots:** each client is sent only entities within
  `AOI_RADIUS` (1400u) of its player — the MMO bandwidth control. (Verified:
  a bot's per-tick entity count varies 14–21 vs the full 30 as it moves.)
- **Resource harvesting:** a melee swing also fells a tree/rock in front,
  granting `wood`/`stone` and respawning the node. (Verified: unit test
  `attacking_a_resource_harvests_a_material`.)
- **Tests:** `cargo test -p antediluvia-server` (harvest + enemy-kill/xp/trophy).

**Deliberately deferred (honest gaps, not started):**
- **Password auth.** Login is still name-only. Not faking it with a weak hash —
  wants a real password-hashing crate (argon2/bcrypt). Deferred, not "done."
- **Binary / delta-compressed snapshots.** Snapshots are full JSON of the AoI set
  each tick. AoI already bounds bandwidth for the MVP; delta+binary is a later
  optimization.
- **NPC dialogue / quests**, act-transition gates beyond free `Travel`,
  client-side sprites/animation polish (client currently draws colored circles).
