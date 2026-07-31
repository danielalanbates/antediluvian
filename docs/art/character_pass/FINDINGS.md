# Character fidelity pass — experiment and result (2026-07-29)

**Question:** can the "low-poly art ceiling" be lifted by post-processing the
existing character GLBs, instead of commissioning/generating new art?

**Setup:** `scripts/render/characters.py` — headless Blender 5.2, imports a
character GLB, applies a treatment, renders a 3/4 turnaround under a key/fill/
rim rig matching the in-game three-point lighting, and re-exports GLB.

The pipeline had to preserve the rig: the Bevy client indexes animations by
NUMBER (`rig_for`: 36 idle, 48 run, 23 death, per-class attack). Verified on
export — **76 actions and 41 bones survive intact**, so the mechanism is sound
and reusable for any future treatment.

## Two treatments tested, both rejected

### 1. Geometry: smooth shading + 1 subdivision level
Body went 1141 -> 6545 verts. Result: **worse.** Stylised hard-surface armour
plates inflate into soft blobs; the chest and pauldrons lose the crisp plate
definition that carries the silhouette. Boots and belt improved slightly, which
is not worth the trade. Kept in the script behind `ANTEDILUVIA_GEO=1` for
organic-only future use; OFF by default.

### 2. Material: roughness 0.62 / specular 0.35 / sheen 0.18
Result: **no visible change.** The atlas material is flat colour by design;
there is no normal or roughness map for the lighting rig to catch, so changing
BSDF scalars moves nothing perceptible.

## Conclusion

The stylised look is baked into the **source geometry and the flat colour
atlas**. It is not a shading or material-parameter problem, and no cheap
post-process lifts it. Raising character fidelity genuinely requires new source
art: authored/sculpted meshes with real PBR texture sets (albedo + normal +
roughness), retargeted onto this same 41-bone `Rig` so the 76 actions carry
over.

The retarget path is the valuable part proven here — new bodies can be skinned
to the existing armature and exported with animations intact, so the animation
library is NOT a blocker for an art replacement.

Renders: `Knight_before.png`, `Knight_after.png` (material-only; deliberately
near-identical — that IS the finding).

## Follow-up: Quaternius Ultimate + procedural generators (2026-07-29)

**Quaternius Ultimate packs — rejected on inspection.** Ultimate Modular Men is
CC0, rigged, 11 characters / 4 swappable parts, but **untextured** and ships
**24 animations vs our current 76**. Every Quaternius character pack is
untextured stylised low-poly. That is the same tier as (arguably below) the
textured KayKit art already in the game, so it does not raise fidelity. The
packs ARE still worth having for VARIETY — "Ultimate Monsters", "Ultimate
Animated Animal Pack" and "Modular Character Outfits - Fantasy" would widen the
mob roster and the outfit axis of the character builder.

**MPFB2 (MakeHuman Plugin For Blender) — works, installed, verified.**
This is the real answer to "is there a better tool/language for 3D art": it is
Python-scriptable inside Blender, code GPLv3 but **bundled assets CC0**, and it
runs fully headless.

  blender --online-mode --command extension install mpfb

143 operators including `create_random_human`, `create_random_human_batch`,
`load_library_skin`, `load_clothes`, `load_library_pose`. Verified headlessly:
`create_random_human` produced a **19,158-vertex anatomically-correct rigged
human** (see `mpfb_human.png`) — 17x the Knight body's 1,141 verts, with real
anatomy and smooth surfaces.

GOTCHA: enable the addon AFTER `read_factory_settings(use_empty=True)`, not
before — the reset unregisters it and `bpy.ops.mpfb` silently disappears.

### The catch — this is an ART-DIRECTION decision, not a drop-in upgrade

MPFB2 makes **realistic-proportioned humans**. The game is **stylised chibi**
(oversized head, short body — Kenney/KayKit/Quaternius). Dropping a realistic
human into the current world would clash with every mob, prop, building and
hero landmark. Adopting MPFB2 means re-arting the whole game, not just the
player characters.

Remaining work if MPFB2 IS adopted: apply a skin material
(`load_library_skin`), add clothing, and retarget its rig onto our 41-bone
`Rig` so the 76 actions survive (the retarget path is already proven above).

## MPFB2 adoption cost — measured, not estimated (2026-07-30)

Daniel chose MPFB2. Generation works: `create_random_human` + `create_v2_skin`
gives a 19,158-vert rigged human with a proper skin material (see
`mpfb_skinned.png`). But the rigs are incompatible on every axis:

| | Knight (current) | MPFB2 |
|---|---|---|
| bones | 41 | 163 |
| head height fraction | 0.832 (chibi) | 0.912 (realistic) |
| naming | `hips`, `spine`, `upperarm.l` | `pelvis.L`, `upperleg01.L` |

So the body CANNOT simply be skinned onto the existing armature — different
bone count, different names, and chibi-vs-realistic proportions would deform it
badly. The earlier "retarget path is proven" result holds only for a mesh swap
onto the SAME armature.

### What adopting MPFB2 actually costs
1. New rig (163-bone MPFB/Rigify) replaces the 41-bone `Rig`.
2. **All 76 animations must be re-sourced or retargeted.** Free option:
   the Rokoko Blender addon has retargeting; Auto-Rig Pro is paid.
3. `rig_for` in client-bevy indexes animations by NUMBER (36 idle, 48 run,
   23 death, per-class attack) — every index must be re-derived.
4. Art direction: realistic humans clash with the stylised Kenney/KayKit mobs,
   props, buildings and 12 hero landmarks, so those need replacing too.

This is a multi-session project, not a drop-in. Generation is the easy 10%;
rig + animation + world-art coherence is the other 90%.
