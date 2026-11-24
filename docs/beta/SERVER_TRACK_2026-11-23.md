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
cargo test --release -p antediluvia-sim -- --test-threads=1   # 96 tests after review (92 before)
cargo test --release -p antediluvia-server                    # 14 tests after review (11 before)
cargo build --release -p antediluvia-test-client
# live check (temp DB, never port 8787):
ANTEDILUVIA_BIND=127.0.0.1:8797 ANTEDILUVIA_DB=/tmp/t.sqlite target/release/antediluvia-server &
target/release/antediluvia-track ws://127.0.0.1:8797 A        # ends with TRACK-E2E-OK
```

| Check | Result on 2026-11-23 |
|---|---|
| Sim suite | 96 passed after review; 92 before (52 on `main`) |
| Server suite | 14 passed after review; 11 before (0 on `main`) |
| `antediluvia-track` against a live server on 8797 | `TRACK-E2E-OK`; after review also `TRACK-E2E-DEV-OK` (board quest accept + paid flight) |
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
- **Chat.** The new messages are rate limited (see Review findings); the
  older `Chat`, `GuildChat`, `PartyInvite`, `Duel`, `TradeGive` and `MailSend`
  in `main.rs` are not, and none of those except zone `Chat` honour the ignore
  list. Friend names are stored as typed, not in canonical case.
- **Durability.** Two copies of one item id share wear; `hp` bonus survives
  breakage; no "your weapon broke" notice.
- **Flight** is an instant move; a timed flight path is a client/server follow-up.
- **Quest-chain bot.** `ANTEDILUVIA_QUEST=1` only hunts mobs inside its snapshot radius and idles when none are near the inn; it needs to roam (or the seed needs serpents near the inn) before it can serve as an early-flow gate again.
- **Pre-existing flake** `class_select_and_cast_damages_target_and_costs_mana`
  (about 2 runs in 100, same on `main`) was left alone.

## Review findings (adversarial review, 2026-11-23)

Reviewed `git diff main...beta-server-track` (20 files). Fixes are in one
commit on top of `7a36e81`. "Verified" means a test that fails without the
fix, unless stated otherwise.

| # | Severity | Finding | Fix | Verified by |
|---|---|---|---|---|
| 1 | High (economy) | Cave "ore collect" board quests used the **real, sellable ore** as the quest item. The sim hands a collect item to every group member in range per node broken while they hold fewer than 4, so a 40-raid minted up to 40 extra ore per node, and a solo player who never turned in got +1 per node for ever | Quest item is now a quest-only sample (`gq_<act>_c<id>_ore`), unsellable and unwearable | `cave_ore_collect_and_dweller_quests_complete` (exact real-ore yield, sample refused by the innkeeper, consumed on turn-in); `every_objective_is_completable_in_its_act` now asserts every Collect item has no vendor price and no item def |
| 2 | High (DoS) | No rate limit on any new message. One client could fan a channel line out to every listener, scan all connections (`Who`, `Emote`) or force an SQLite write per `FriendAdd`/`FriendRemove`, as fast as it could send frames, all on the game-loop thread | `social::FloodGate`: 48 points per connection per 100 ticks (5 s); chat, whisper, emote, who and friend/ignore writes cost 4, raid invite 2, the rest 1. One warning per window, then silent drops | sim `flood_gate_caps_a_window_and_refills`; server `chat_flood_is_throttled_per_connection_and_refills` (200 lines sent, 12 delivered, 1 warning, no DB row, other connection unaffected, refills) |
| 3 | Medium (DoS) | Name, id, channel and filter fields were used at any length: echoed back in notices, lowercased, passed to SQLite. `who_matches` re-lowercased the whole filter once per online character (a 2 MB filter x 1,000 players per request) | Every identifier field is clipped to 48 chars before handling (`clip_msg`); `who_list` clips the filter to 32 chars | server `oversized_and_hostile_fields_are_clipped_not_echoed` (2 MB fields, replies under 400 bytes, nothing written); sim `hostile_strings_are_clipped_and_cleaned` |
| 4 | Medium (spoofing) | Emote targets were free text (24 chars, control characters allowed), so a line break or a bracketed fake system tag could be put in text shown to nearby players | `social::clean_name`: letters, digits, space, apostrophe, hyphen only | same two tests as #3 |
| 5 | Medium (gold loss) | `flight_take` took the fare and then could still fail (`no owner` on a cross-act flight), losing gold with no flight | The check runs before the fare is taken | `every_refused_flight_is_free` |
| 6 | Medium (latent) | Flight-master discovery rides the name-only `discovered` list, but node names were only checked unique within one act. 23 POI names repeat in the data; a namesake in another act would have marked an unvisited flight master as known. No current node collides (checked against the data), so this was luck, not design | Node picking now requires a name unique across all POIs and caves in every act | `flight_master_names_are_unique_across_every_act` |
| 7 | Low (panic) | `RaidBook::invite` / `kick` indexed `self.raids[&group]`; a mismatch between the two internal maps would panic the game loop | `get()` with an error return | `hostile_strings_are_clipped_and_cleaned` (last block) |
| 8 | Low (per-tick cost) | `durability::is_broken` runs for every worn item of every player each sim tick (through `gear_bonus`) and scanned the item table two or three times even for pristine gear | Map probe first; the table is only touched for an item recorded at 0 | Covered by the existing durability tests (behaviour unchanged); not benchmarked |
| 9 | Low (per-tick cost) | The banner pass rebuilt a region (230-anchor scan plus three `String`s and a faction clone) for every player four times a second, moving or not | Last position cached per connection; unmoved players are skipped, faction borrowed not cloned | `standing_still_costs_no_banner_work_and_moving_still_banners`; not benchmarked |
| 10 | Low (test) | `friends_and_ignore_persist...` asserted `!online && a \|\| b`, which passed for an online friend | Split into two asserts | Reads correctly; still passes |

Checked and found sound (no change): old databases open (`durability` is
`serde(default)` in the ext JSON, `social_list` is `CREATE TABLE IF NOT
EXISTS`, both tested); `PROTOCOL_VERSION` untouched and every new wire field
optional or in a new variant; generated quests cannot reach `talk()` (giver
`board`, and `quests_for` only reads the authored table); board quests pay
once (re-accept and second turn-in refused); delivery parcels cannot be
duplicated into value (unsellable, abandon removes them); repair and flight
only remove gold; messages from connections that are not logged in are
ignored; the sim additions use no fs, time or threads.

Found and **not** fixed (need an owner decision or touch shared code):

- **Sheet size.** `quests_done` is a `Vec<String>` and the whole sheet is sent
  as `Stats` after every per-player sim event. A character who clears several
  hundred board quests carries roughly 25 bytes per quest on every one of
  those sends (about 24 KB with all 950 done). Needs a compact completion
  store, which is a sheet change on both branches.
- **Group collect/Dweller loot scales with group size** (existing rule, now
  reachable at 40): every member in range with the quest gets the collect
  item, and every member present gets the Dweller's rare. Authored collect
  quests whose item is a real material have the same shape as finding #1.
- **Legacy party has no size cap** (existing), so the 40 cap only binds
  groups created through `RaidCreate`.
- **Durability follows the owner, not the item.** A worn piece traded to
  another character is pristine for them (one free cycle per item id per
  pair of players).
- **Release spirit** skips the `Died` event, so a character whose home inn is
  in another act stays in the act where they fell (intended, but untested
  over the wire).
- **`flight_take`, `release_spirit`** move the entity without refiling the
  AoI grid; the grid catches up on the next sim tick (50 ms), as it does for
  the existing inn respawn.
- The flood gate's clock is the sim tick, sampled every 5 ticks; zone `Chat`
  in `main.rs` is outside it.

Integration check run for this review (real server on 127.0.0.1:8797, temp
DB, two WebSocket clients, `ANTEDILUVIA_TRACK_DEV=1`): channel chat received
by the second client, whisper, friend add (row confirmed in SQLite), who,
emote, ignore, raid of two, then with dev commands: flight refused before
discovery, landmark discovered, board listed, **generated quest accepted**
(`gq_eden_p381_kill` present in `Stats`), **paid flight** inn to landmark
(9 gold, charged once), board shows the quest `active` on landing. Output
ended `TRACK-E2E-OK` / `TRACK-E2E-DEV-OK`; server stopped, port free, no
panic in the log. The Bevy client was still not built.
