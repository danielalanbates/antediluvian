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
- **Client (graphical):** `bevy` 0.15 (2D). This is the revived original
  prototype (`crates/client-bevy`), currently single-player; being wired to the
  server next.
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

## Status (2026-07-09)
**Server MVP — verified end-to-end:** auth (name-based auto-register), character
persistence, five simulated zones, enemy/wildlife AI, melee combat, XP/leveling,
death/respawn, zone travel, zone chat. Two concurrent bots confirmed movement,
combat both directions, kill attribution, loot, and SQLite save on disconnect.

**Client:** Bevy prototype revived into the workspace; still single-player, not
yet networked to the server.

## Next
- Net the Bevy client to the server (replace its local world with server snapshots).
- Real auth (passwords), binary/delta-compressed snapshots + area-of-interest culling.
- Resource harvesting, NPC dialogue/quests, act-transition gates.
