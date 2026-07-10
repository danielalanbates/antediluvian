# CHUNK 10 — Performance pass

**Status: todo**

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
