# CHUNK 10 — Performance pass

**Status: DONE 2026-07-18**

## Delivered
- FPS overlay behind `ANTEDILUVIA_FPS=1` (FrameTimeDiagnosticsPlugin).
- Startup preload of every .glb/.gltf/.ogg/.wav via a filtered dir walk
  (`load_folder` error-spammed on glTF `.bin` buffers — 0 load errors now).
- Shadow budget: sun-only caster, cascades capped at 900 m (first bound 220 m).
- Decor/formation `VisibilityRange` fade at 2200–2600 m (inside the fog).
- **Msaa::Off on the camera — this was the real bottleneck.** 4x MSAA at
  2880x1800 retina held the M1 at ~25 fps in release; Off → 59 fps at the
  inn hub with NPCs + players on screen (screenshot-verified live).
- Memory: RSS stable/declining (248→213 MB over 45 s); travel despawns
  Terrain recursively and formation meshes are entity-owned so assets free.

## Goal
Steady 60 fps on this 8 GB M1 with a busy zone on screen, and no hitching
on travel or spawn waves.

## Design (measure FIRST — add an FPS overlay before changing anything)
1. Add `FrameTimeDiagnosticsPlugin` + a small HUD FPS counter (debug only,
   `ANTEDILUVIA_FPS=1` env).
2. Asset preload: load all character/enemy/wildlife GLBs + weapon meshes at
   startup (loading screen state) instead of first-spawn hitches.
3. Shadow budget: only the sun casts shadows; cap shadow distance;
   spot-check the atmosphere night path (point lights, if any, no shadows).
4. Terrain: verify the rebuilt-on-travel mesh is dropped (no leak across
   many travels — watch memory over 10 travel cycles).
5. Entity churn: despawn far-outside-AoI mirrors promptly; reuse materials
   (no per-spawn `materials.add` in hot paths — audit spawn sites).
6. If still short of 60: reduce decor scatter counts per act, then LOD the
   rigs (KayKit is already low-poly; scatter is the likelier culprit).

## Verify
FPS overlay screenshots: inn hub with 2 clients + combat + particles ≥55
fps sustained; 10× travel round-trip with stable memory (log RSS via
`ps -o rss=`); no first-combat hitch after preload.
