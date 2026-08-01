# Antediluvia — Rust MMORPG Server

A full-fledged, **headless authoritative MMORPG backend** written in Rust. The
server owns the entire game world; clients send *intents* and receive
*snapshots*. Built as a Cargo workspace.

> Antediluvia's world is the antediluvian (pre-Flood) age, told across five acts:
> **Eden → Hermon → Nephilim → Enoch → Flood**. Each act is an independent,
> continuously-simulated zone.

## Workspace layout

| Crate | What it is |
|-------|------------|
| `crates/protocol` | Shared wire types (`ClientMsg` / `ServerMsg`, `Act`, `EntityState`, `CharacterSheet`). The message contract between server and any client. |
| `crates/server` | The authoritative server: TCP/WebSocket acceptor, one task per connection, and a single fixed-tick **game loop** that owns the `World` and simulates every zone. SQLite persistence. |
| `crates/test-client` | A thin headless bot client that logs in, hunts the nearest enemy, and prints world summaries — proves the loop end-to-end. |

## Architecture

```
 client ──ws──▶ connection task ──mpsc(GameCmd)──▶  ┌─────────────┐
 client ◀─ws── connection task ◀─mpsc(ServerMsg)──  │  game loop  │
                                                    │  owns World │  ── 20 Hz tick
                                                    │  (no locks) │  ── SQLite save
                                                    └─────────────┘
```

- **Authoritative**: clients never set position. They send `Move{dx,dy}` /
  `Attack`; the server integrates movement, runs enemy & wildlife AI, resolves
  combat, awards XP, handles death/respawn, and broadcasts a full zone snapshot
  each tick.
- **Single-threaded sim**: the whole `World` lives in one task, reached only via
  channels — no `Mutex` in the hot path, and the tick is deterministic given a
  seed (`World::new(seed)` uses an internal xorshift RNG).
- **Zones**: the five acts each run their own entity set; `Travel { act }` moves
  a character between them.
- **Persistence**: characters (position, level, XP, HP/MP, inventory) are stored
  in SQLite on login, every ~10s, and on disconnect.

## Public alpha shard

A hosted shard runs the world continuously, so you don't have to run a server to
play with other people:

```
ws://159.54.191.177:8787
```

Point the macOS client at it (the bundled `.app` otherwise spawns its own local
server):

```sh
echo 'ws://159.54.191.177:8787' > ~/Library/Application\ Support/Antediluvia/server_url
# or, per-launch:  ANTEDILUVIA_SERVER=ws://159.54.191.177:8787 open -a Antediluvia
```

It's a 1-core / 1 GB Oracle Always Free box — sized for a handful of alpha
players, not the 1,000-player load the server is benchmarked at. Characters
persist across restarts. Plain `ws://` for now: browser clients need `wss://`,
which is waiting on a DNS name and a TLS cert.

## Run (locally)

```sh
# terminal 1 — server
cargo run -p antediluvia-server            # listens ws://127.0.0.1:8787
#   ANTEDILUVIA_BIND=0.0.0.0:8787  ANTEDILUVIA_DB=world.sqlite  to override

# terminal 2 — a bot
cargo run -p antediluvia-client -- Adam    # logs in, hunts enemies
cargo run -p antediluvia-client -- Eve     # a second concurrent player
```

## Protocol (JSON over WebSocket, one message per frame)

Client → Server: `login`, `move`, `attack`, `travel`, `chat`, `ping`
Server → Client: `welcome`, `login_rejected`, `snapshot`, `stats`, `chat`, `notice`, `pong`

Binary framing / delta-compressed snapshots are the next optimization; the enum
contract in `crates/protocol` stays the same.

## Status

Playable networked MMORPG (MVP). Done & verified: authoritative server (auth +
persistence, 5 zones, AI, combat, XP, death/respawn, travel, chat),
**area-of-interest snapshots**, **resource harvesting**, and a **networked Bevy
client** (`crates/client-bevy`) that renders the server world. Server has unit
tests (`cargo test -p antediluvia-server`).

Deliberately deferred: real password auth (name-only today), binary/delta
snapshot compression (AoI bounds bandwidth for now), NPC quests, client sprite
polish (draws colored circles). See `PROJECT.md` for the full breakdown.
