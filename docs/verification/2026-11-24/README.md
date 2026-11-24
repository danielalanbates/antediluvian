# 2026-11-24 - B11: 1,000 players on one screen

Branch `beta-b10-b11`, `PROTOCOL_VERSION` 17. Not pushed, not deployed, `.app`
not rebuilt. Design: `docs/beta/B11_SCALE.md`. This Mac: 8 GB, about 7 GB of
swap already in use by other apps during every run.

## Result

**1,000 players are drawn on one screen at 46-56 fps (windowless 1600x900),
with the server at 6-15 ms average per tick and 22.5 KB/s of snapshots per
client.** The client log counts them: `crowd: 1000 other players drawn (40
rigs, 960 block figures), 1025 entities in all`. Shots below.

It is not clean in three places: the server had rare ticks over 50 ms in one
of the three 1,000-bot runs; the frame rate is the windowless figure, never
seen in a real window; the browser transport was written but not compiled.

## Shots (windowless run 3, one client watching the crowd grow)

| File | What it shows |
|---|---|
| `b11_crowd_200_near.jpg` | 200 bots at the Eden gate, default camera: 40 rigs with names and health bars, block figures behind |
| `b11_crowd_500_near.jpg` | 500 bots, same camera |
| `b11_crowd_1000_near.jpg` | 1,000 bots, same camera: block figures fill the foreground, rigs and names in the middle |
| `b11_crowd_1000_overview.jpg` | 1,000 bots from above (`cam 800 1.3`): the whole crowd on one screen |
| `b11_crowd_1000_overview_2.jpg` | same, 12 s later |

Seen in the shots and not fixed:
- Names of players standing on the same spot pile into an unreadable clump.
- Block figures stand inside each other and inside rigs (players do not collide, by design); from above they read as boxes.
- The 40 rigs are the nearest players when the crowd first appears. A block figure that later walks closer than a rig is promoted only when a rig slot frees (a rig moving past 437 units).
- Health bars over near rigs are very wide (old behaviour).

## Numbers

Bots use the binary wire with the full 1400 AoI, walk out to a disc around the
gate and pace out and back, attacking now and then. "Snapshot B/s" is the
server's own count of binary frame bytes per client; "bot rx" is everything a
bot received (snapshots plus JSON stats and events).

### Server alone (no game client; bots spread over a 500-unit disc; one 20 s window per figure)

| Bots | Tick avg | Tick max per window | Snapshot pass avg | Snapshot B/s per client | Bot rx B/s | Entities a bot holds | Server CPU / RSS |
|---|---|---|---|---|---|---|---|
| 200 | 2.2 ms | 6.1 - 9.6 ms | 1.9 ms | 7,365 - 7,684 | 7,814 - 8,218 | 225 | 11% / 22 MB |
| 500 | 7.1 - 9.1 ms | 19.5 - 23.0 ms | 8.2 - 9.8 ms | 12,958 - 13,155 | 13,547 - 14,151 | 525 | 26% / 48 MB |
| 1000 | 13.1 - 15.3 ms | 31.9, 119.0, 32.2, 32.4, 51.9 ms | 16.3 - 19.4 ms | 21,339 - 23,735 | 22,063 - 22,507 | 1,024 | 74% / 86 MB |

1,000 bots, 115 s: three ticks went over the 50 ms budget (51.9, 91.3 and
119.0 ms; the 119 ms one was a 107 ms snapshot pass). Why is not established.
No keyframes fell in that window. The bots (33% of a core) and the server
share this machine, which was swapping. An earlier 72 s run had one 75 ms tick
while 1,000 bots logged in within 4 s, and none after.

### With the game client (run 3: bots on a 180-unit disc, added in steps of 200, 300, 500)

| Players near the client | Client fps (10 s samples) | Client drew | Server tick avg | Tick max per window | Snapshot B/s per client |
|---|---|---|---|---|---|
| 200 | 47.5 - 52.3 | 200 (40 rigs, 160 block) | 0.8 ms | 2.0 - 2.3 ms | 7,329 - 7,356 |
| 500 | 46.5 - 55.0 | 500 (40 rigs, 460 block) | 2.4 - 2.5 ms | 7.2 - 22.8 ms | 12,915 - 13,631 |
| 1000 | 46.2 - 52.3 (default camera), 50.7 - 56.1 (from above) | 1,000 (40 rigs, 960 block) | 6.1 - 6.4 ms | 12.0 - 32.7 ms | 22,470 - 22,801 |

Run 2 (same, 150-unit disc, before the count line existed): fps 47.5 - 54.1,
tick avg 6.3 - 6.9 ms at 1,001 connections, max 14.4 - 45.4 ms. Neither run
with the client had a tick over 50 ms.

The server was about twice as fast in these two runs as in the server-alone
runs above at the same bot count. The differences were the disc radius (150 -
180 against 500) and three bot processes against one. Which of them matters,
or whether it was the machine, was not found out. Quote the slower figures.

- Client memory (RSS, sampled every 5 s): 113 - 175 MB in run 3, 131 - 162 MB
  with 1,000 players. Both runs set `ANTEDILUVIA_NO_PRELOAD=1`. RSS on this
  machine understates the real footprint (pages get compressed or swapped).
  With the normal preload the client peaked at 904 MB in its first 5 s (run 1).
- Frame rate is capped near 60 by the windowless run loop (16 ms). The empty
  gate measured 44 - 52 fps on 2026-11-23, so the crowd of 1,000 costs nothing
  that these samples can show; the scene is the limit.
- Bandwidth before B11, for comparison: about 10 KB/s per bot at a 150 AoI,
  about 250 KB/s computed for a client at the 150-entity cap.
- Bots that decode (20 per bot process): 0 frames failed in every run.
- Bot frames per second fell from about 40 to about 15 when combat events
  stopped going to the whole zone.
- Unit test, no network: one client watching 1,000 walking players gets
  21,955 B/s; first sight of the crowd is a 40,775-byte keyframe.

## Tests (release)

| Suite | Result |
|---|---|
| `antediluvia-protocol` | 12 passed (6 new: quantisation, keyframe round trip, masked deltas, desync and resync, text cut, bytes per entity) |
| `antediluvia-sim -- --test-threads=1` | 132 passed in 2 of 3 runs; in the other, `questgen::tests::cave_ore_collect_and_dweller_quests_complete` failed (131 / 1). The sim crate is untouched by B11: the test is still flaky, whatever the 2026-11-23 handoff says |
| `antediluvia-server` | 25 passed (9 new in `snap`: keyframe, enter / leave / teleport, live world in step for 10 clients, resync rate limit, periodic keyframe and zone change, unwatched cells unread, caps and tiers, event routing, byte budget at 1,000) |
| `antediluvia-client-bevy` | 138 passed (8 new: decoder to whole snapshots, one resync per fault, login asks for binary, crowd budget x4, camera command) |
| `cargo build --release -p antediluvia-server -p antediluvia-client-bevy -p antediluvia-test-client` | builds |

## JSON wire still works (test tools, own server on 8797)

- `antediluvia-track ws://127.0.0.1:8797 B`: `TRACK-E2E-OK`, exit 0.
- `antediluvia-swarm 100 ws://127.0.0.1:8797 150 json 300`: 100 connected, 34.8 - 35.6 KB/s per bot for at most 100 entities each (the binary wire carries 1,025 entities in 22.5 KB/s).
- `antediluvia-client JsonProbe ws://127.0.0.1:8797`: welcome, banner and notices printed.
- That 20 s window had one 41 ms tick with 100 connections and a 2.2 ms average: slow ticks on this machine are not only a crowd effect.

## Windowless runs: 3 of 4 used

| Run | What | Outcome |
|---|---|---|
| 1 | 200 bots + client with preload | Stopped by my own guard 10 s in: free swap fell from 1,576 MB to 366 MB while the client loaded. Nothing measured. |
| 2 | client first, crowd grown 200 -> 500 -> 1,000, no preload | exit 0, 5 shots, swap free stayed 1,319 - 1,486 MB |
| 3 | same with the on-screen count logged | exit 0, 5 shots (the ones kept here) |

Own server on `127.0.0.1:8797` each time, database under `target/tmp`
(removed), server and bots stopped after each run, port free, 8787 untouched.

## Not verified

- **Browser (wasm) transport.** `net.rs` reads `ArrayBuffer` frames and sends `Resync`; it was never compiled (the wasm target is off limits here). Expect to fix it at the first `scripts/build_web.sh`.
- **A real window, Retina size, battery.** All frame rates are windowless 1600x900.
- **A hosted link.** Everything ran over loopback. 22.5 KB/s per client is 22.5 MB/s out of the server at 1,000 clients in one spot.
- **Solo play (`local.rs`).** Compiles (one new match arm); not run.
- **`ANTEDILUVIA_WIRE=json` on the game client.** Not run. The JSON wire itself was checked with the tools (below).
- **Resync over a real socket.** No frame ever failed, so the client's `Resync` and the server's keyframe answer are covered by unit tests only.
- **Periodic keyframes at the client.** They fired on the server (counted in telemetry) and bots decoded them; the client showed no hitch in fps samples, nothing finer was measured.
- **Mounted players, gear changes, deaths in the crowd.** Unit-tested on the wire; not looked at in a shot.
- **More than 1,200 players** in one AoI (the cap), and two crowds in two zones.
