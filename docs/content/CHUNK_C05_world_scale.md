# CHUNK C05 — World scale & layout

**Status: DONE (2026-07-10)**

## Goal
Grow each act's playable map so it can hold ~40 POIs, cave entrances, and
multiple quest hubs without crowding, and give it basic legibility: a dirt
road from the inn toward the far side, and clustered (not uniform) spawns.

## Read first
- `crates/server/src/world.rs` — map bounds, spawn scatter, `AOI_RADIUS`.
- `crates/client-bevy/src/terrain.rs` (or the terrain module in main.rs) —
  fBm terrain size must match server bounds.
- `docs/locations/README.md` — the long-term vision is one seamless
  supercontinent; this chunk only *scales the existing per-act maps* (a
  true seamless merge is a much later project — do not attempt it here).

## Design
1. Grow act bounds ~4× (e.g. 3000×3000 → 6000–8000 square). Keep
   `AOI_RADIUS` as is — that's the point of AoI.
2. Server: spread enemy/wildlife/resource spawns in clusters ("camps") of
   4–8 around deterministic cluster centers rather than uniform random;
   keep a safe ring (~400u) around the inn spawn-point mob-free.
3. Client terrain mesh + decor scatter must read the same bounds (share the
   constant through `protocol` or a snapshot field so they can't drift).
4. Road: flatten + recolor a strip of terrain vertices from the inn to the
   map center (client visual only; pick vertices within distance-to-segment).
5. Player run speed check: crossing half the map should take ~2–3 min on
   foot (WoW-ish); tune speed or bounds. Mounts (C06) will ease this.

## Verify
- Two clients + screenshots: inn hub with no spawns inside the safe ring,
  a mob camp cluster, the road visible.
- test-client walks to the far corner and back without desync or falling
  through terrain; 13+ tests green.
