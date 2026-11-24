# Beta server track — 2026-11-23

Branch `beta-server-track` (from `main` @ `e192420`), built in a separate
clone to be merged with the unpushed `beta-b01-b09`. Scope: sim + protocol +
server + test-client. **No Bevy client UI** — everything is exposed as
`ClientMsg` / `ServerMsg` for a later client job to bind.

## What was added

| Roadmap | Feature | Where | Tests |
|---|---|---|---|
| B05 | 950 generated quests: 4 per POI (kill / collect / explore / deliver), 5 per cave (guard kill, ore collect, Dweller kill, explore, deliver). 73 authored + 950 = **1,023 quests** | `crates/sim/src/questgen.rs` | 11 |
| B06 (sim) | Six rarity tiers; durability on weapon/chest/legs/head/wrist; wear on death and in combat; inn repair as a gold sink | `crates/sim/src/durability.rs` | 9 |
| B07 | Raids to 40, channels (general / trade / lfg / zone), friends + ignore (persisted), whispers, emotes, `/who` | `crates/sim/src/social.rs`, `crates/server/src/social_db.rs`, `crates/server/src/track.rs` | 8 sim + 11 server |
| B08 | Flight masters (5 inns + 20 landmark POIs), offline rested XP, graveyards + spirit release, zone-in banner | `crates/sim/src/travel.rs` | 8 |
| PvP | Confirmation tests only (behaviour already existed) | `crates/sim/src/pvp_tests.rs` | 3 |
| Tooling | Two-connection smoke client `antediluvia-track` | `crates/test-client/src/track.rs` | prints `TRACK-E2E-OK` |

### B05 quest rules
- Generated quests are **never offered by `talk()`**. Their giver key is
  `"board"`, which no NPC carries; players take them from the notice board
  at the POI / cave itself. NPC offer order is untouched.
- Every generated quest also `requires` its act's main finale
  (`the_flaming_sword`, `stargazers_fall`, … — computed, not hard-coded).
- Kill / collect targets are guaranteed to spawn: each POI outside the
  starting areas owns a 2-mob camp of one hostile bestiary species.
  Eden has no camps inside its softened starting country (within 1500 of
  the inn or 1200 of the Gate); those POIs target spawn-table mobs instead.
- Explore = walk into another anchor while the quest is active. Deliver =
  carry a parcel (handed over on accept, unsellable, taken back on abandon)
  to another anchor or the inn.
- Turn-in: at the board or the act inn; deliveries only at the destination.
- Anchors are keyed `poi:<id>` / `cave:<id>` because display names repeat
  (175 unique POI names of 200, 28 unique cave names of 30).
- **Levels.** `BoardQuest.level` is the act band (1–12 … 49–60) and scales
  rewards. `min_level` (the acceptance gate) is `1 + act*3 + tier` (1–15),
  because the XP curve (`max_xp *= 1.35`) makes the nominal bands
  unreachable in later acts. See open pathways.

### B06 rules
- Durability is stored per **item id** on the sheet (`durability`, absent =
  pristine), so swapping, banking or re-equipping never mends an item.
- Death: every worn durable piece loses 10% of max. Combat: ~1 swing in 12
  costs the weapon a point, ~1 blow taken in 8 costs one armour piece a point.
- Broken (0) gear stops adding its melee/spell bonus. The `hp` bonus is not
  removed (simplification).
- Repair: at the inn, `ceil(sum(missing * tier price) / 10)` gold, counted in
  `gold_sunk`. Only items still owned are billed and cleared.

### B07 rules
- A raid is one shared-credit group in the sim (group id carries bit 62).
  **Kill credit is the existing party rule at raid size**: members within 900
  units split kill XP evenly and each get quest credit; loot, gold and rep stay
  with the killer. There is no raid-specific XP penalty.
- The old party messages still work; the server reconciles `Conn::party` with
  the raid book 4×/s (PartyLeave drops you from the raid; a PartyInvite from a
  raid member seats the invitee, or refuses at 40).
- Ignore filters whispers, channels, raid chat, emotes, raid invites and zone
  `Chat`. Guild chat is **not** filtered.

### B08 rules
- A flight node is known once visited: POI nodes reuse the POI `discovered`
  list; an inn node is added the first time you ask its flight master.
  Both ends must be known. Fare: `2 + dist/400` in-act; `10 + 15/act` across.
- Offline rest: 5% of `max_xp` per hour logged out at an inn, a quarter of that
  elsewhere; cap `max(2000, 1.5 * max_xp)`. `last_logout` is reset at login.
- Spirit release works only during the existing 6 s death window; afterwards
  the old inn/home respawn fires as before.
- No new sheet fields for B08 (reuses `discovered`, `rested_xp`, `last_logout`).

## Message names

| ClientMsg | Reply / effect |
|---|---|
| `QuestBoard` | `ServerMsg::QuestBoard { place, quests: [BoardQuest] }` |
| `QuestAccept { id }`, `QuestTurnIn { id }`, `QuestAbandon { id }` | `Notice` + `Stats` |
| `RaidCreate`, `RaidInvite { player }`, `RaidAccept`, `RaidLeave`, `RaidKick { player }` | `RaidInfo { leader, members }` |
| `RaidChat { text }` | `ChannelMsg { channel: "raid", from, text }` |
| `ChannelJoin { channel }`, `ChannelLeave { channel }`, `ChannelList` | `Channels { joined, available }` |
| `ChannelSay { channel, text }` | `ChannelMsg { channel, from, text }` |
| `Whisper { to, text }` | `Whisper { from, to, text }` (echoed to sender) |
| `FriendAdd/FriendRemove/IgnoreAdd/IgnoreRemove { player }`, `FriendList` | `Friends { friends: [FriendEntry], ignored }` |
| `Emote { emote, target? }` | `Emote { from, emote, text }` to players within 1400 |
| `Who { filter? }` | `WhoList { players: [WhoEntry], total }` (cap 50) |
| `FlightList` | `FlightPaths { here, nodes: [FlightNode] }` |
| `FlightTake { node }` | `Notice`; cross-act also re-sends `Welcome` (new entity id) |
| `ReleaseSpirit` | `Notice` + `Stats` |
| `Repair` | `Notice` + `Stats` + `Gear` |
| `GearInfo` | `Gear { items: [ItemInfo], repair_cost }` |
| — (server push) | `ZoneBanner { act, name, subtitle, level_min, level_max, pvp }` on login, act change, and entering an inn / POI / cave / capital |

Payload structs (`Rarity`, `ItemInfo`, `BoardQuest`, `FriendEntry`, `WhoEntry`,
`FlightNode`) live in `crates/protocol/src/track.rs`. `PROTOCOL_VERSION` is
unchanged (already 15 on `main`).

## How to verify

```sh
export PATH="$HOME/.rustup/toolchains/stable-aarch64-apple-darwin/bin:$PATH" \
  RUSTUP_TOOLCHAIN=stable-aarch64-apple-darwin CARGO_BUILD_JOBS=2
cargo test --release -p antediluvia-sim -- --test-threads=1   # 92 tests
cargo test --release -p antediluvia-server                    # 11 tests
cargo build --release -p antediluvia-test-client
# live check (temp DB, never port 8787):
ANTEDILUVIA_BIND=127.0.0.1:8797 ANTEDILUVIA_DB=/tmp/t.sqlite target/release/antediluvia-server &
target/release/antediluvia-track ws://127.0.0.1:8797 A        # ends with TRACK-E2E-OK
```

| Check | Result on 2026-11-23 |
|---|---|
| Sim suite | 92 passed (52 on `main`) |
| Server suite | 11 passed (0 on `main`) |
| `antediluvia-track` against a live server on 8797 | `TRACK-E2E-OK` |
| Sim suite repeated 100× | 3 runs failed, all in two **pre-existing** flaky tests (`main`: 8 of 100) |
| `ANTEDILUVIA_QUEST=1 antediluvia-client` (Eden chain bot) | **stalls** after accepting the first quest: no serpent inside its view at the inn. Identical stall on `main` built from `e192420` with the same world seed, so not a regression, but the chain was not proven end to end over the wire |
| `antediluvia-client-bevy` | **not built** (forbidden on this machine); only two catch-all match arms were added |

## Merge plan with `beta-b01-b09`

| File | What this branch did | Expected conflict |
|---|---|---|
| `protocol/src/lib.rs` | Variants appended after `Ping` / `Pong`; `durability` appended to `CharacterSheet`; `mod track` at file end | Adjacent-line: keep both sides (their `CastStart` / `AbilityUsed` / `stamina`, then ours) |
| `sim/src/world.rs` | 7 one-line hooks tagged `beta-server-track`, one field in `new_character_with`, an appended block at file end, edits inside `party_shares_xp_and_quest_credit` | Hooks sit in `populate_zone`, the damage loop, `gear_bonus`, the quest-progress match and the end of `step_zone`; re-apply by tag if the surrounding lines moved |
| `sim/src/quests.rs` | Two `Objective` variants at the end, one arm in `count()`, `quest()` falls through to generated quests | Low |
| `server/src/db.rs` | `durability` in `SheetExt` + load + save; `raw()` appended | Adjacent to their `stamina` lines: keep both |
| `server/src/main.rs` | `mod` lines, `track::tick`, `track::offline_rest`, ignore filter in `Chat`, catch-all arm after `Ping` | Low; the catch-all must stay the **last** arm |
| `client-bevy/src/{main,local}.rs` | One `_ =>` arm each | If their branch already added a wildcard arm, drop ours |
| New files | `questgen.rs`, `durability.rs`, `social.rs`, `travel.rs`, `pvp_tests.rs`, `protocol/src/track.rs`, `social_db.rs`, `server/src/track.rs`, `test-client/src/track.rs` | None |

- New match sites to check after merging: any exhaustive `match` on
  `Objective`, `ClientMsg` or `ServerMsg` their branch added.
- If their branch bumped `PROTOCOL_VERSION` differently, take theirs.
- After merging, rerun the three commands above, then build the Bevy client
  on a machine with room for it.

## Open pathways
- **Client UI** for all of the above (boards, raid frames, channel tabs,
  friends panel, flight map, banner, rarity colours, durability + repair).
  The solo (wasm) path answers "Not available in solo play yet".
- **XP curve vs act bands.** `max_xp *= 1.35` overflows `u32` near level 55
  and makes the 37–60 bands unreachable. Fix the curve, then set generated
  `min_level` to the real bands.
- **Camps disperse.** A killed camp mob respawns at a random point (existing
  bestiary rule), so the species stays in the act but leaves its POI.
- **More hostiles.** Camps add about 70–80 enemies per act; the 1,000-bot
  swarm was not re-run.
- **Raid rules.** No raid XP penalty, no raid-only quests, no loot rolls.
- **Chat.** No rate limiting; guild chat ignores the ignore list; friend
  names are stored as typed, not in canonical case.
- **Durability.** Two copies of one item id share wear; `hp` bonus survives
  breakage; no "your weapon broke" notice.
- **Flight** is an instant move; a timed flight path is a client/server follow-up.
- **Quest-chain bot.** `ANTEDILUVIA_QUEST=1` only hunts mobs inside its snapshot radius and idles when none are near the inn; it needs to roam (or the seed needs serpents near the inn) before it can serve as an early-flow gate again.
- **Pre-existing flake** `class_select_and_cast_damages_target_and_costs_mana`
  (about 2 runs in 100, same on `main`) was left alone.
