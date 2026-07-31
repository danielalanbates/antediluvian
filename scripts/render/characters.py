#!/usr/bin/env python3
"""Upgrade the shading/material fidelity of the character GLBs, in place-safe
copies, WITHOUT touching the armature or its animations.

Why this shape, and not a from-scratch procedural character generator:
the Bevy client indexes animations by NUMBER (`rig_for` in client-bevy:
36 = idle, 48 = run, 23 = death, plus a per-class attack index). Each
character GLB ships 76 actions on a 41-bone `Rig`. Regenerating bodies from
scratch would throw all of that away and every animation index in the client
would have to be re-derived by hand. So we keep the rig and the actions
exactly as they are and only change how the body *renders*.

The actual fidelity problem is NOT polycount — the Knight body is 1141 verts /
1052 faces, which is mid-poly. It is:
  1. flat (faceted) shading on organic surfaces, and
  2. one unlit-looking colour-atlas material with no roughness variation,
     so nothing in the lighting rig has anything to catch.

This script fixes both, and renders a before/after turnaround so the change
can be judged rather than assumed.

Usage (headless):
  /Applications/Blender.app/Contents/MacOS/Blender --background \
      --python scripts/render/characters.py -- <name> [<name> ...]
Names are files under assets/models/characters (e.g. Knight, Mage). With no
names it processes the four builder body types.

Outputs:
  assets/models/characters/<name>_pbr.glb   upgraded model
  docs/art/character_pass/<name>_{before,after}.png   turnaround renders
"""
import math
import os
import sys

import bpy
from mathutils import Vector

REPO = os.path.abspath(os.path.join(os.path.dirname(__file__), "..", ".."))
CHARS = os.path.join(REPO, "assets", "models", "characters")
OUT_RENDER = os.path.join(REPO, "docs", "art", "character_pass")
DEFAULT = ["Knight", "Barbarian", "Rogue", "Mage"]

# Meshes that are held equipment, not body — leave their hard edges alone,
# a sword blade should stay crisp.
EQUIPMENT_HINTS = ("sword", "shield", "axe", "bow", "staff", "spear", "helmet")


def reset():
    bpy.ops.wm.read_factory_settings(use_empty=True)


def is_body(obj):
    n = obj.name.lower()
    return obj.type == "MESH" and not any(h in n for h in EQUIPMENT_HINTS)


def smooth_and_thicken(obj):
    """Smooth-shade the organic surfaces and add ONE subdivision level.

    Level 1 only: it quadruples faces, and near-field characters are the only
    ones that ever use this mesh (distant players fall back to the crowd LOD
    impostor), but there is no reason to pay for level 2 on a stylised body.
    """
    for poly in obj.data.polygons:
        poly.use_smooth = True
    # Keep genuinely sharp creases sharp; Blender 4.1+ dropped the old
    # auto_smooth_angle flag in favour of a modifier.
    if "SmoothByAngle" not in [m.name for m in obj.modifiers]:
        try:
            mod = obj.modifiers.new(name="SmoothByAngle", type="SMOOTH_BY_ANGLE")
            mod.angle = math.radians(50)
        except TypeError:
            pass  # older/newer Blender: plain smooth shading is still applied
    sub = obj.modifiers.new(name="Subdiv", type="SUBSURF")
    sub.levels = 1
    sub.render_levels = 1


def upgrade_material(mat):
    """Give the flat colour atlas something for the lighting rig to work with.

    The base-colour texture is left connected — it carries the art direction.
    What is added is roughness/specular response so the three-point lighting
    and the shadow pass actually read on the surface.
    """
    if not mat or not mat.use_nodes:
        return
    bsdf = next((n for n in mat.node_tree.nodes if n.type == "BSDF_PRINCIPLED"), None)
    if not bsdf:
        return
    def setv(key, value):
        if key in bsdf.inputs:
            bsdf.inputs[key].default_value = value
    setv("Roughness", 0.62)     # cloth/skin, not plastic
    setv("Metallic", 0.0)
    setv("Specular IOR Level", 0.35)
    # A touch of sheen keeps cloth from going dead flat under the rim light.
    setv("Sheen Weight", 0.18)


def look_at(obj, target):
    d = target - obj.location
    obj.rotation_euler = d.to_track_quat("-Z", "Y").to_euler()


def render_turnaround(path, subject_height=1.8):
    """Three-quarter view render, EEVEE, neutral studio light."""
    scene = bpy.context.scene
    # The EEVEE enum name moved around across releases (BLENDER_EEVEE ->
    # BLENDER_EEVEE_NEXT -> back again), so pick whatever this build exposes.
    engines = scene.render.bl_rna.properties["engine"].enum_items.keys()
    scene.render.engine = next(
        (e for e in ("BLENDER_EEVEE_NEXT", "BLENDER_EEVEE") if e in engines),
        "CYCLES",
    )
    scene.render.resolution_x = 720
    scene.render.resolution_y = 900
    scene.render.film_transparent = False

    cam_data = bpy.data.cameras.new("Cam")
    cam = bpy.data.objects.new("Cam", cam_data)
    scene.collection.objects.link(cam)
    scene.camera = cam
    focus = Vector((0.0, 0.0, subject_height * 0.55))
    cam.location = Vector((subject_height * 1.5, -subject_height * 2.0, subject_height * 1.1))
    look_at(cam, focus)

    # Key / fill / rim — mirrors the in-game three-point rig so the render is
    # representative of what the player will actually see.
    for name, loc, energy, size in [
        ("Key", (3, -4, 5), 900, 4),
        ("Fill", (-4, -2, 1.5), 250, 6),
        ("Rim", (-2, 4, 3), 500, 3),
    ]:
        ld = bpy.data.lights.new(name, type="AREA")
        ld.energy = energy
        ld.size = size
        lo = bpy.data.objects.new(name, ld)
        lo.location = Vector(loc)
        scene.collection.objects.link(lo)
        look_at(lo, focus)

    world = bpy.data.worlds.new("W")
    world.use_nodes = True
    world.node_tree.nodes["Background"].inputs[0].default_value = (0.05, 0.06, 0.08, 1)
    scene.world = world

    os.makedirs(os.path.dirname(path), exist_ok=True)
    scene.render.filepath = path
    bpy.ops.render.render(write_still=True)


def process(name):
    src = os.path.join(CHARS, f"{name}.glb")
    if not os.path.exists(src):
        print(f"SKIP {name}: {src} not found")
        return False

    # ---- before ----
    reset()
    bpy.ops.import_scene.gltf(filepath=src)
    render_turnaround(os.path.join(OUT_RENDER, f"{name}_before.png"))

    # ---- after ----
    reset()
    bpy.ops.import_scene.gltf(filepath=src)
    actions_before = len(bpy.data.actions)
    bodies = [o for o in bpy.data.objects if is_body(o)]
    # Geometry smoothing is OPT-IN (ANTEDILUVIA_GEO=1). Measured on Knight:
    # blanket subdivision inflates stylised hard-surface armour into mush and
    # reads as a downgrade. Polycount was never the problem (body is 1141
    # verts); the flat material response is. Default to material-only.
    do_geo = os.environ.get("ANTEDILUVIA_GEO") == "1"
    for obj in bodies:
        if do_geo:
            smooth_and_thicken(obj)
        for slot in obj.material_slots:
            upgrade_material(slot.material)
    render_turnaround(os.path.join(OUT_RENDER, f"{name}_after.png"))

    dst = os.path.join(CHARS, f"{name}_pbr.glb")
    bpy.ops.export_scene.gltf(
        filepath=dst,
        export_format="GLB",
        export_animations=True,
        export_apply=True,      # bake the subdivision into the exported mesh
        use_selection=False,
    )
    # The whole point is that animations survive; say so out loud.
    print(
        f"OK {name}: bodies={len(bodies)} actions_in={actions_before} -> {dst}"
    )
    return True


def main():
    argv = sys.argv
    names = argv[argv.index("--") + 1:] if "--" in argv else []
    names = names or DEFAULT
    done = [n for n in names if process(n)]
    print(f"DONE {len(done)}/{len(names)}: {', '.join(done)}")


if __name__ == "__main__":
    main()
