# CHUNK 03 — Terrain

**Status: DONE 2026-07-10.** `crates/client-bevy/src/terrain.rs`: seeded fBm
(`noise` 0.9, seed = act index + 7) → `terrain_height`, flattened inside
~240u of the origin; 128×128 vertex-colored mesh (height-banded per-act
palette, colors converted sRGB→linear — raw values wash out), smooth normals.
Character/NPC/resource roots get `translation.y = terrain_height(...)` at
spawn and every snapshot; terrain rebuilds on act change (Welcome/Stats).
Verified on screen: Eden grassland + Hermon foothills (distinct palettes),
feet on slopes, flat inn disc, no panics. Known minor artifact: far entities
can float a couple units where the 33u grid interpolates steep noise.

## Goal
Replace the flat green plane with rolling, textured terrain per act, WoW-
Classic style: grassland (Eden), foothills (Hermon), badlands (Nephilim),
city outskirts (Enoch), stormy coast (Flood).

## Constraints
- **Server stays 2D**: gameplay positions are (x, y) on a plane; terrain is
  *visual* height only. Characters must sit on the surface: client offsets a
  character root's `translation.y` by `height(x, z)`.
- Deterministic: client and any future server-side use must agree. Use a
  seeded noise function (e.g. `noise` crate, Perlin/fBm, seed = act index) —
  NOT random.

## Steps
1. Add `noise = "0.9"` to client-bevy. Write
   `fn terrain_height(act: Act, x: f32, z: f32) -> f32` in a new
   `terrain.rs`: fBm noise, amplitude ~40u, wavelength ~600u, flattened to 0
   inside the inn radius (lerp to 0 within 300u of the entry) so gameplay
   landmarks stay readable.
2. Build a terrain `Mesh` at startup: grid ~128×128 over the 4200×4200 world,
   heights from `terrain_height`, recompute normals
   (`Mesh::compute_smooth_normals`). Replace the `Plane3d` spawn.
3. Vertex-color the mesh by height/slope (grass → dirt → rock bands, per-act
   palette) — cheaper and better-looking at this art style than a tiled
   texture. Insert `ATTRIBUTE_COLOR` and use a `StandardMaterial` with
   `base_color: WHITE`.
4. Offset every character/resource/NPC root y by `terrain_height` — one line
   in the snapshot-apply loop and in `spawn_visual` (`pos.y`), plus health
   bars already ride the root. The camera target also needs the offset.
5. On `Travel` (act change in `Stats`/`Welcome`), rebuild the terrain mesh
   for the new act (despawn old mesh entity, spawn new).

## Verify
Run + screenshot: hills visible on the horizon, characters standing ON the
slope (feet not floating/buried) near and far from the inn, flat disc at the
inn. Travel to hermon (`Travel` via a test client or walk) → different
palette. No fps collapse (frame time in logs with `LogDiagnosticsPlugin` if
unsure).
