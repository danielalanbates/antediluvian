# CHUNK 02 — Combat events → remote animations

**Status: todo**

## Goal
Everyone sees everyone's swings, hits, and deaths. Today only the *local*
player animates attacks; remote players and enemies slide around in
Idle/Run and dead things pop out of existence.

## Design
Add a broadcast event message to the protocol (bump `PROTOCOL_VERSION`):
```rust
// ServerMsg
Event { act: Act, kind: EventKind, src: EntityId, dst: Option<EntityId> },
// protocol enum
pub enum EventKind { Attack, Cast, Hit, Die }
```
Server: in `world.rs`, the melee/ability/death code paths already exist
(grep `attack_queued`, `cast_queued`, `dead_timer`). Extend `SimEvent` with
`Combat { act, kind, src, dst }`, emit at those sites, and in `main.rs`'s
sim-event drain, send to every conn in that act (reuse the zone-chat fan-out
pattern; AoI filtering optional — events are tiny).

Client: on `ServerMsg::Event`, look up `EntityMap` → root → `Mover.rig` →
`RigAnim`, and:
- `Attack`/`Cast` → play `rig.attack` one-shot + set `attack_until`
  (reuse `trigger_attack_anim`'s body; factor a helper
  `play_one_shot(rig, &mut players, node, secs)`).
- `Hit` → optional: flash the health bar / small stagger (skip if time-tight).
- `Die` → play Death_A one-shot. Death clips must be added to `RigClips` and
  the graph (4th node; indices: adventurers 23, skeletons 24). Then delay
  the visual despawn: server keeps corpses ~2 s (it already has `dead_timer`
  for players; enemies currently respawn-in-place — check `enemy_death` path)
  OR client-side: when a `Die` event arrives, mark the Mirrored entry
  "dying" and skip its despawn for 1.5 s after it leaves the snapshot.
  Client-side is less invasive — prefer it.
- Local player: keep the instant local trigger (feels responsive); dedupe by
  ignoring `Attack` events whose src == my entity.

## Files
`crates/protocol/src/lib.rs`, `crates/server/src/world.rs`,
`crates/server/src/main.rs`, `crates/client-bevy/src/main.rs`.

## Verify
- Two clients (Adam + Eve). From Adam's window watch Eve press space
  (drive Eve with Quartz key events): Eve's model must swing in Adam's view.
- Kill a skeleton; it must play its death animation before disappearing.
- `cargo test -p antediluvia-server` still green; both clients panic-free.
- Screenshot mid-swing from the observer client as proof.
