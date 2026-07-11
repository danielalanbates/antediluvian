# CHUNK C14 — Developer menu (alpha tooling)

**Status: todo** (Daniel directive 2026-07-11: alpha readiness)

## Goal
Alpha testers with dev rights get an in-game developer menu; everyone else
never sees it. Server-authoritative: the client only *requests* dev actions.

## Design
1. `dev_accounts` list in a server-side config file (apple_ids). Sheet gains
   a derived (not persisted) `is_dev` flag sent in `Welcome`.
2. `ClientMsg::Dev(DevCmd)` enum, all server-gated on is_dev:
   `Teleport { x, y }`, `TravelAny`, `GiveItem { item, n }`, `SetLevel(u32)`,
   `Heal`, `SpawnMob { tag }`, `KillTarget`, `Godmode`, `TimeOfDay(f32)`.
3. Client: backquote (`) toggles a dev panel listing the commands with
   simple fields; hidden entirely when not is_dev.
4. Log every dev command server-side (who, what, when) — alpha audit trail.

## Verify
- Unit: non-dev account sending DevCmd → rejected + logged; each command
  does what it says.
- Live: toggle panel on a dev account, teleport across the map, spawn a mob,
  confirm a non-dev client never renders the panel.
