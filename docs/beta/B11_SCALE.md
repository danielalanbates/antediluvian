# B11 - 1,000 players on one screen: how it works

Branch `beta-b10-b11`. `PROTOCOL_VERSION` 17. Measured numbers and shots:
`docs/verification/2026-11-24/README.md`.

## Wire

| | JSON wire (`Login.wire = 0`) | binary wire (`Login.wire = 1`) |
|---|---|---|
| Who uses it | `antediluvia-client`, `antediluvia-track`, `antediluvia-swarm ... json` | the game client (native and browser), `antediluvia-swarm ... bin` |
| Snapshot | text `ServerMsg::Snapshot`, every entity in full | WebSocket binary frame: enter / changed fields / leave |
| Cap per client | nearest 150 entities | nearest 1,200 players + nearest 400 others |
| Rate | 10 Hz everything | 10 Hz for the nearest 64 players + 64 others inside 450 units, 2.5 Hz for the rest |
| Bytes per walking player per update | about 170 | 7 (position) or 8 (position + facing) |

Everything that is not a snapshot (chat, stats, windows) is JSON text on both.
`ClientMsg::Login` has a new defaulted field `wire`; `ClientMsg::Resync` is new.
Frame layout: header comment of `crates/protocol/src/wire.rs`.

- Positions: u16, 1/8 world unit. Facing: u8, 1/256 turn. Ids and health: varints.
- Name, class tag, gear, lineage, appearance, look and mount travel once, when
  an entity enters view, and again only if they change.
- Keyframe (whole view as enters): login, zone change, `Resync`, and every 60 s.
  `Resync` is honoured at most every 2 s per client.
- The client decoder checks a per-connection frame sequence. A frame that does
  not decode or skips a number makes it send `Resync` once and wait.

## Server (`crates/server/src/snap.rs`)

Per snapshot tick (10 Hz):
1. Mark the AoI grid cells any binary client can see.
2. Read the entities filed in those cells, once, into a per-zone table. Each
   row keeps the snapshot number at which position, facing, health, max health
   and "meta" last changed. Entities in unwatched cells are not read at all.
3. Per client: leaves for rows that vanished; every 4th snapshot (staggered by
   connection) a grid scan for enters, leaves, caps and tiers; then for the
   near list and one quarter of the far list, compare the row's change numbers
   with the number the client was last sent. No per-client copy of any entity.

The JSON path also walks the grid now and serializes an entity only when a
JSON client sees it (it used to serialize the whole zone every snapshot).

Telemetry: `ANTEDILUVIA_TELEMETRY_SECS=20` shortens the window; the log line
`snap telemetry` gives the snapshot pass time and bytes per binary client.

## Client

- `net.rs` `Inbound`: one decoder for both transports. Native reads
  `Message::Binary`; the browser reads `ArrayBuffer` messages. Bevy still
  receives whole `ServerMsg::Snapshot` values, so game code is unchanged.
  Solo play (`local.rs`) has no wire and is unchanged (`Resync` is a no-op arm).
- `crowd.rs`: block figures (everyone past the 40-rig budget) have no health
  bar and no name; rigs have both; block figures ease slower because they
  are updated at 2.5 Hz. Snapshot entities are sorted nearest-first so the rig
  budget goes to the closest players.
- `ANTEDILUVIA_WIRE=json` (native) asks for the old wire, for comparison.
- `ANTEDILUVIA_AUTOCMD="cam 1150 0.95"` pulls the camera back for a crowd shot.

## Server changes that came out of measuring

- Combat events (`ServerMsg::Event`) went to every client in the act: N
  listeners for each of N fighters. They now go to clients that hold the
  source in their 10 Hz tier (binary) or within 450 units (JSON), plus the
  victim. Events only drive rig animations.
- Character saves ran for every connection in one tick every 10 s. They are
  now spread over the 10 s by connection id (5 per tick at 1,000).
- Periodic keyframes are staggered by connection id, so clients that logged
  in together are not all re-keyed in the same second.

## Limits and what to do next

- Snapshot pass at 1,000 in one spot: 8-19 ms of the 50 ms tick, about
  300,000 field updates per pass. Next lever: scan every 8th snapshot, or a
  position-only fast path that skips the per-update mask.
- 22.5 KB/s per client at 1,000, all moving. Next lever: far tier at 2 Hz,
  or signed 1-byte position steps for the near tier.
- The client rebuilds a whole `Snapshot` (clones every entity) for each
  frame. It did not show in the frame rate at 1,000; a delta-aware apply
  in `main.rs` would remove it.
- Rig budget is first come, nearest at first sight. A nearest-first swap
  when the budget is full is not written.
- `MAX_PLAYER_RIGS` stays 40: nothing was measured that would justify more.

## Measuring

```
ANTEDILUVIA_BIND=127.0.0.1:8797 ANTEDILUVIA_DB=target/tmp/b11.sqlite \
ANTEDILUVIA_DEV_ACCOUNTS=b11dev ANTEDILUVIA_TELEMETRY_SECS=20 target/release/antediluvia-server

# server alone
target/release/antediluvia-swarm 1000 ws://127.0.0.1:8797 1400 bin 500

# one client watching the crowd grow (start the client after the first line)
target/release/antediluvia-swarm 200 ws://127.0.0.1:8797 1400 bin 180 0 1000
target/release/antediluvia-swarm 300 ws://127.0.0.1:8797 1400 bin 180 200 1000
target/release/antediluvia-swarm 500 ws://127.0.0.1:8797 1400 bin 180 500 1000
ANTEDILUVIA_NO_PRELOAD=1 ANTEDILUVIA_SHOTS=<dir> ANTEDILUVIA_SHOT_TIMES=45,95,150,168,180 \
ANTEDILUVIA_AUTOCMD="god;time 0.4;<49 more 'time 0.4'>;cam 800 1.3" \
  target/release/antediluvia-client-bevy b11dev Jared ws://127.0.0.1:8797
```
Swarm arguments: count, url, AoI radius, `json|bin`, gather radius, first bot
number, total bots (the last two place a later batch outside the earlier
ones). Bots walk out to an even disc around the Eden gate and pace out and
back, with the full 1400 AoI in `bin` mode, so each one costs the server what
a real client would. The first 20 bots of a process decode every frame and
count frames that fail. The server logs `tick telemetry` and `snap telemetry`
(pass time, bytes per binary client) every window; the client logs `fps:` and
`crowd: N other players drawn (R rigs, B block figures)` every 10 s.

`ANTEDILUVIA_NO_PRELOAD=1` loads models on first use. On an 8 GB Mac that is
already swapping, the normal preload peaked at 904 MB and took 1.2 GB of swap
in 10 s. The camera's far plane is 1,000 units: `cam` values much past
`800 1.3` cull the far side of the crowd.
