# Antediluvia Content Pass — AI Work Manual

**Goal:** turn the design documentation under `docs/` (quests, bestiary, POIs,
caves, mounts, factions) into *playable, server-authoritative game content*,
the way `docs/art/` turned the client from primitives into a WoW-style
presentation. This directory is a **work queue for AI sessions**: each
`CHUNK_CNN_*.md` is sized to fit one focused session in a small context
window (read → implement → verify → commit).

## How to work a chunk (protocol — follow exactly)
1. Read `PROJECT.md`, this README, and **only** the chunk file plus the doc
   files it lists. Do NOT read all of `docs/` — the bestiary alone is 2,500
   entries. Chunks tell you exactly which docs to sample.
2. `export PATH="$HOME/.rustup/toolchains/stable-aarch64-apple-darwin/bin:$PATH"`.
   Server work: `cargo build -p antediluvia-server && cargo test -p antediluvia-server`.
   Client work: `cargo build -p antediluvia-client-bevy` (heavy first build).
   **8 GB M1 — never run two builds or a build + ML job concurrently.**
3. The server is authoritative; the client renders. New gameplay data lives in
   `crates/server` (or generated data files under `assets/data/`), never in
   the client.
4. **Verify honestly**: unit tests for rules, the headless `test-client` or a
   scripted ws client for wire-level checks, and for anything visible run the
   real client and screenshot it (see `docs/art/README.md` step 4 for the
   screencapture recipe). Never mark a chunk DONE from a compile alone.
5. Kill every server/client you spawned. Update the chunk's **Status** line,
   the index below, and `PROJECT.md`. Commit with a descriptive message.

## Gotchas (learned the hard way)
- **iCloud eviction**: `~/Documents` is iCloud-synced. If reads time out or
  builds fail with "Need authenticator (os error 81)", byte-force the tree:
  `find crates assets docs -type f -exec dd if={} of=/dev/null bs=1m \;`
  (`brctl download` alone does NOT materialize files).
- **Disk**: this Mac runs near-full. If ENOSPC, clear `~/Library/Caches/Homebrew`
  and `~/Library/Caches/pip` first; never delete `target/` unless desperate.
- **Persistence**: new `CharacterSheet` fields must be added in THREE places:
  `crates/protocol/src/lib.rs` (with `#[serde(default)]`), `SheetExt` in
  `crates/server/src/db.rs`, AND both the `load()` and `save()` bodies there.
  The 2026-07-10 fatigue commit forgot db.rs and broke the build.
- **Protocol versions**: bump the proto version constant when the wire format
  changes; the client rejects mismatches.
- **Big data**: don't hand-write 2,500 mobs into Rust source. Parse the
  markdown archives into a generated JSON under `assets/data/` with a small
  script in `scripts/`, and have the server load it at boot. Keep the
  generator committed and rerunnable.

## Where the design docs live
| Docs | Contents | Used by chunks |
|------|----------|----------------|
| `docs/quests/act*_*.md` | ~9 quests per act × 5 acts (givers, objectives, rewards) | C01, C02 |
| `docs/quests/themes/` | 8 cross-zone questline pillars, 400 quests | C08 |
| `docs/quests/mount_questline.md`, `dynamic_mounting_system.md` | Mounts + taming | C06, C07 |
| `docs/mobs/Bestiary_Vol_01..25.md` | 2,500 mobs (level, habitat, temperament, drops, tameable) | C03 |
| `docs/locations/*.md` | 5 regions, seamless-world philosophy, 116 located quests | C04, C05 |
| `docs/locations/points_of_interest/` | 2,000 POIs with coordinates | C04 |
| `docs/locations/caves/` | 1,000 caves/mines with resources | C09 |
| `docs/lore/entities/factions.md` | Watchers, Nephilim, Sethite/Cainite factions | C10 |

## Chunk index (do in order unless a chunk says otherwise)
| # | File | What | Status |
|---|------|------|--------|
| C01 | CHUNK_C01_quest_engine.md | Data-driven multi-quest engine (multiple concurrent quests, multiple givers per act) | DONE |
| C02 | CHUNK_C02_act_quests.md | All ~45 act quests from docs/quests/act*.md loaded + rewards/items | DONE |
| C03 | CHUNK_C03_bestiary.md | Bestiary → generated mobs.json → level-ranged spawn tables per act | DONE |
| C04 | CHUNK_C04_pois.md | POI placement + discovery XP + named subzones on the map | DONE |
| C05 | CHUNK_C05_world_scale.md | Bigger per-act maps matching POI coordinate space; road between inn and POIs | todo |
| C06 | CHUNK_C06_mounts.md | Level-40 Dire-Wolf mount questline + riding (speed buff + client model) | todo |
| C07 | CHUNK_C07_taming.md | Creature Mastery sandbox: weaken→subdue taming of tameable bestiary mobs, stable | todo |
| C08 | CHUNK_C08_theme_questlines.md | First cross-zone theme pillar (Forbidden Arts) as a chained questline | todo |
| C09 | CHUNK_C09_caves.md | Cave/mine sites: entrances, interior mobs + ore nodes from cave archives | todo |
| C10 | CHUNK_C10_factions.md | Sethite/Cainite alignment choice + reputation + faction vendors | todo |
| C11 | CHUNK_C11_economy.md | Bestiary drop tables in loot, vendor buy/sell, AH seeded by NPC listings | todo |
| C12 | CHUNK_C12_audio.md | CC0 sound: combat hits, casts, ambient loops per act, UI clicks (bevy_audio) | todo |

Art-pass chunks 09 (visible equipment) and 10 (performance) still live in
`docs/art/` and can be interleaved; C05 before C04 if map scale blocks POI
placement.
